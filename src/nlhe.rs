//! No-limit hold'em controller translated from `Nlhe.h` and `Nlhe.cpp`.

use crate::action::{BettingAction, BoardAction, DealingAction, LoggedAction, SeatingAction};
use crate::board::{Board, Street};
use crate::dealer::{Dealer, DealerError};
use crate::decision_engine::{DecisionEngine, DecisionEngineError};
use crate::deck52::{Deck52, DeckError};
use crate::hand_history::HandHistory;
use crate::player::{Decision, Player, PlayerId};
use crate::stack::Stack;

/// No-limit hold'em table controller, corresponding to C++ `Nlhe52`.
#[derive(Clone, Debug)]
pub struct Nlhe52 {
    /// Deck used to deal cards; maps to C++ `deck_`.
    deck_: Deck52,
    /// All seated players; maps to C++ `players_`.
    players_: Vec<Player>,
    /// Stable player IDs in current hand order; replaces C++ `playersInHand_`.
    players_in_hand_: Vec<PlayerId>,
    /// Small blind amount; maps to C++ `smallBlind`.
    small_blind: i32,
    /// Big blind amount; maps to C++ `bigBlind`.
    big_blind: i32,
}

impl Nlhe52 {
    /// Creates a table with `number_of_players` players, each holding 100 big blinds.
    ///
    /// This maps to C++ `Nlhe52(int)`.
    ///
    /// # Panics
    ///
    /// Panics only if a positive player number cannot be represented as a
    /// `usize` player ID.
    pub fn new(number_of_players: i32) -> Self {
        let small_blind = 10;
        let big_blind = small_blind * 2;
        let mut players = Vec::new();
        for number in 1..=number_of_players {
            let mut player = Player::new(Stack(100 * big_blind), &DecisionEngine, number);
            player.set_player_id(usize::try_from(number - 1).expect("player number is positive"));
            players.push(player);
        }
        let players_in_hand = players.iter().map(Player::id).collect();
        Self {
            deck_: Deck52::default(),
            players_: players,
            players_in_hand_: players_in_hand,
            small_blind,
            big_blind,
        }
    }

    /// Plays one hand, prints the hand history and stack totals, and checks chip conservation.
    ///
    /// This maps to C++ `playHand`.
    ///
    /// # Errors
    ///
    /// Returns an error if dealing, betting, player lookup, pot accounting, or
    /// hand evaluation fails.
    ///
    /// # Panics
    ///
    /// Panics if the player index cannot fit in `i32` or if the final stack
    /// total differs from the total before the hand.
    pub fn play_hand(&mut self) -> Result<(), NlheError> {
        for &player_id in &self.players_in_hand_ {
            let player = find_player_mut(&mut self.players_, player_id)?;
            if player.get_stack() == Stack::default() {
                player.put_amount(Stack(self.big_blind * 100));
            }
        }
        let all_players = total_stack(&self.players_in_hand_, &self.players_)?;
        self.deck_.shuffle();
        if !self.players_in_hand_.is_empty() {
            self.players_in_hand_.rotate_left(1);
        }

        for (index, &player_id) in self.players_in_hand_.iter().enumerate() {
            find_player_mut(&mut self.players_, player_id)?.set_player_index(
                i32::try_from(index).expect("player index fits in i32"),
                self.players_in_hand_.len(),
            );
        }

        let mut hand_history = HandHistory::default();
        for player in &self.players_ {
            hand_history.log_action(LoggedAction::Seating(SeatingAction {
                player: player.clone(),
            }));
        }

        let mut dealer = Dealer::new(self.players_in_hand_.clone(), Stack(self.big_blind));
        let mut board = Board::default();
        let small_blind = self.small_blind_player()?;
        let small_blind_action = {
            let player = find_player(&self.players_, small_blind)?;
            let mut action = BettingAction::new(player, Street::Preflop);
            action.next_bet = minimum(Stack(self.small_blind), player.get_stack());
            action.decision = Decision::Raise;
            action
        };
        hand_history.log_action(LoggedAction::Betting(small_blind_action.clone()));
        dealer.accept_bet(small_blind, &small_blind_action);

        let big_blind = self.big_blind_player()?;
        let big_blind_action = {
            let player = find_player(&self.players_, big_blind)?;
            let mut action = BettingAction::new(player, Street::Preflop);
            action.next_bet = minimum(Stack(self.big_blind), player.get_stack());
            action.decision = Decision::Raise;
            action
        };
        hand_history.log_action(LoggedAction::Betting(big_blind_action.clone()));
        dealer.accept_bet(big_blind, &big_blind_action);

        hand_history.log_action(LoggedAction::Board(BoardAction {
            player: Player::default(),
            board: board.clone(),
        }));

        for player_id in offset_ids(&self.players_in_hand_, 1, self.players_in_hand_.len())? {
            let hole_cards = self.deck_.get_hole_cards()?;
            let player = find_player_mut(&mut self.players_, player_id)?;
            player.deal_hole_cards(&hole_cards);
            hand_history.log_action(LoggedAction::Dealing(DealingAction::new(
                player,
                &hole_cards,
            )));
        }

        let first_to_act = if self.players_.len() > 3 { 3 } else { 0 };
        if self.finish_betting_round(
            first_to_act,
            &mut dealer,
            &board,
            &mut hand_history,
            all_players,
        )? {
            return Ok(());
        }

        board.deal_flop(self.deck_.get_flop()?);
        hand_history.log_action(LoggedAction::Board(BoardAction {
            player: Player::default(),
            board: board.clone(),
        }));
        if self.finish_betting_round(1, &mut dealer, &board, &mut hand_history, all_players)? {
            return Ok(());
        }

        board.deal_turn(self.deck_.get_card()?);
        hand_history.log_action(LoggedAction::Board(BoardAction {
            player: Player::default(),
            board: board.clone(),
        }));
        if self.finish_betting_round(1, &mut dealer, &board, &mut hand_history, all_players)? {
            return Ok(());
        }

        board.deal_river(self.deck_.get_card()?);
        hand_history.log_action(LoggedAction::Board(BoardAction {
            player: Player::default(),
            board: board.clone(),
        }));
        if self.finish_betting_round(1, &mut dealer, &board, &mut hand_history, all_players)? {
            return Ok(());
        }

        self.print_hand_result(&hand_history, all_players)?;
        Ok(())
    }

    /// Returns the small blind player's stable ID.
    ///
    /// This maps to C++ `smallBlindPlayer`.
    fn small_blind_player(&self) -> Result<PlayerId, NlheError> {
        let position = usize::from(self.players_.len() != 2);
        self.players_in_hand_
            .get(position)
            .copied()
            .ok_or(NlheError::NotEnoughPlayers)
    }

    /// Returns the big blind player's stable ID.
    ///
    /// This maps to C++ `bigBlindPlayer`.
    fn big_blind_player(&self) -> Result<PlayerId, NlheError> {
        let position = if self.players_.len() == 2 { 1 } else { 2 };
        self.players_in_hand_
            .get(position)
            .copied()
            .ok_or(NlheError::NotEnoughPlayers)
    }

    /// Returns whether all seated players have completed a betting round.
    ///
    /// This maps to C++ `ready`.
    #[must_use]
    pub fn ready(&self, current_bet: Stack, board: &Board, hand_history: &HandHistory) -> bool {
        self.players_
            .iter()
            .all(|player| player.ready(current_bet, board, hand_history))
    }

    /// Runs one betting round and reports completion plus the last raiser's seat index.
    ///
    /// This maps to C++ `playRound`; the returned integer remains the source
    /// seat-index value even though the hand controller currently discards it.
    fn play_round(
        &mut self,
        starting_position: usize,
        dealer: &mut Dealer,
        board: &Board,
        hand_history: &mut HandHistory,
    ) -> Result<(bool, i32), NlheError> {
        let player_count = self.players_in_hand_.len();
        if player_count == 0 {
            return Err(NlheError::NotEnoughPlayers);
        }
        let mut first_to_ask = starting_position;
        let mut count = player_count;
        loop {
            let mut new_raiser = false;
            let mut visited = 0usize;
            for player_id in offset_ids(&self.players_in_hand_, first_to_ask, count)? {
                visited += 1;
                let player_snapshot = self.players_.clone();
                if dealer
                    .get_options(player_id, &player_snapshot)?
                    .options
                    .is_empty()
                {
                    continue;
                }
                let player = find_player_mut(&mut self.players_, player_id)?;
                let decision = player.decide(dealer, board, hand_history, &player_snapshot)?;
                if decision == Decision::Raise {
                    new_raiser = true;
                    first_to_ask += visited;
                    if first_to_ask >= player_count {
                        first_to_ask -= player_count;
                    }
                    break;
                }
            }
            count = player_count - 1;
            if !new_raiser {
                break;
            }
        }

        dealer.rake_in(&mut self.players_)?;
        let first_to_ask = i32::try_from(first_to_ask).expect("player index fits in i32");
        let player_count = i32::try_from(player_count).expect("player count fits in i32");
        let last_raiser = first_to_ask - 1;
        let last_raiser = if last_raiser < 0 {
            last_raiser + player_count
        } else {
            last_raiser
        };
        Ok((
            dealer.award_pots(board, hand_history, &mut self.players_)?,
            last_raiser,
        ))
    }

    /// Runs a betting round and emits the same report used by each C++ early-return path.
    fn finish_betting_round(
        &mut self,
        starting_position: usize,
        dealer: &mut Dealer,
        board: &Board,
        hand_history: &mut HandHistory,
        initial_total: Stack,
    ) -> Result<bool, NlheError> {
        let (finished, _last_raiser) =
            self.play_round(starting_position, dealer, board, hand_history)?;
        if finished {
            self.print_hand_result(hand_history, initial_total)?;
        }
        Ok(finished)
    }

    /// Prints hand history and stack totals and verifies conservation.
    fn print_hand_result(
        &self,
        hand_history: &HandHistory,
        initial_total: Stack,
    ) -> Result<(), NlheError> {
        println!("{hand_history}");
        println!("{initial_total}");
        let final_total = total_stack(&self.players_in_hand_, &self.players_)?;
        println!("{final_total}");
        assert_eq!(initial_total, final_total);
        Ok(())
    }
}

/// Returns a requested circular slice of player IDs.
///
/// This maps C++ `offset`; an empty input produces an empty result only when
/// the requested count is zero.
fn offset_ids(ids: &[PlayerId], offset: usize, count: usize) -> Result<Vec<PlayerId>, NlheError> {
    if offset > count {
        return Err(NlheError::InvalidOffset(offset));
    }
    if ids.is_empty() {
        return if count == 0 {
            Ok(Vec::new())
        } else {
            Err(NlheError::NotEnoughPlayers)
        };
    }
    Ok((offset..count + offset)
        .map(|index| ids[index % ids.len()])
        .collect())
}

/// Finds a player by stable ID.
fn find_player(players: &[Player], player_id: PlayerId) -> Result<&Player, NlheError> {
    players
        .iter()
        .find(|player| player.id() == player_id)
        .ok_or(NlheError::Dealer(DealerError::UnknownPlayer(player_id)))
}

/// Finds a mutable player by stable ID.
fn find_player_mut(players: &mut [Player], player_id: PlayerId) -> Result<&mut Player, NlheError> {
    players
        .iter_mut()
        .find(|player| player.id() == player_id)
        .ok_or(NlheError::Dealer(DealerError::UnknownPlayer(player_id)))
}

/// Totals the stacks for the players participating in this hand.
fn total_stack(ids: &[PlayerId], players: &[Player]) -> Result<Stack, NlheError> {
    let mut total = Stack::default();
    for &player_id in ids {
        total += find_player(players, player_id)?.get_stack();
    }
    Ok(total)
}

/// Returns the smaller of two stack amounts.
fn minimum(left: Stack, right: Stack) -> Stack {
    if left < right { left } else { right }
}

/// Controller errors.
#[derive(Debug)]
pub enum NlheError {
    /// A betting or payout operation failed.
    Dealer(DealerError),
    /// A player had no legal action to choose.
    Decision(DecisionEngineError),
    /// The deck ran out of cards during a deal.
    Deck(DeckError),
    /// The table has too few players for blind posting or action order.
    NotEnoughPlayers,
    /// C++ `offset` rejected this requested starting position.
    InvalidOffset(usize),
}

impl From<DealerError> for NlheError {
    /// Wraps a dealer failure from a hand-controller operation.
    fn from(error: DealerError) -> Self {
        Self::Dealer(error)
    }
}

impl From<DecisionEngineError> for NlheError {
    /// Wraps a decision failure from a hand-controller operation.
    fn from(error: DecisionEngineError) -> Self {
        Self::Decision(error)
    }
}

impl From<DeckError> for NlheError {
    /// Wraps a safe deck failure from a hand-controller operation.
    fn from(error: DeckError) -> Self {
        Self::Deck(error)
    }
}

impl std::fmt::Display for NlheError {
    /// Formats the controller failure.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dealer(error) => std::fmt::Display::fmt(error, formatter),
            Self::Decision(error) => std::fmt::Display::fmt(error, formatter),
            Self::Deck(error) => std::fmt::Display::fmt(error, formatter),
            Self::NotEnoughPlayers => formatter.write_str("not enough players for this hand"),
            Self::InvalidOffset(offset) => {
                write!(formatter, "wrong offset in offset range creation: {offset}")
            }
        }
    }
}

impl std::error::Error for NlheError {}
