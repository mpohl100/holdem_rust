//! Betting, side-pot, and showdown management translated from `Dealer.h/.cpp`.

use std::collections::{BTreeMap, BTreeSet};

use crate::action::{BettingAction, LoggedAction, PotAction, ShowdownAction};
use crate::board::{Board, Street};
use crate::hand::{Hand, HandError, compare_hands};
use crate::hand_history::HandHistory;
use crate::player::{Decision, Player, PlayerId};
use crate::pot::Pot;
use crate::stack::Stack;

/// Legal actions available to a player.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Options {
    /// Options in the same order as the C++ implementation.
    ///
    /// Each tuple maps the source nested pair
    /// `(Decision, (minimum_stack, maximum_stack))`.
    pub options: Vec<(Decision, (Stack, Stack))>,
}

/// Dealer state for the players registered in one hand.
///
/// The C++ `reference_wrapper<Player>` list and `Player*`-keyed maps become
/// stable `PlayerId` indices. `players_` preserves table order; bets and pots
/// are keyed by identity, not by player address.
#[derive(Clone, Debug)]
pub struct Dealer {
    /// Stable player IDs in hand order; maps to C++ `players_`.
    players_: Vec<PlayerId>,
    /// Big blind size.
    big_blind_: Stack,
    /// Current betting street; maps to C++ `street_`.
    street: Street,
    /// Chips committed during the current betting round.
    bets_: BTreeMap<PlayerId, Stack>,
    /// Previously closed side pots.
    closed_pots_: Vec<Pot>,
    /// Current unclosed pot.
    current_pot_: Pot,
}

impl Dealer {
    /// Creates a dealer for the specified stable player IDs.
    ///
    /// The IDs are supplied in the order formerly represented by C++
    /// `std::vector<std::reference_wrapper<Player>>`.
    /// This maps to `Dealer(std::vector<std::reference_wrapper<Player>>, Stack)`.
    #[must_use]
    pub fn new(players: Vec<PlayerId>, big_blind: Stack) -> Self {
        Self {
            players_: players,
            big_blind_: big_blind,
            street: Street::Preflop,
            bets_: BTreeMap::new(),
            closed_pots_: Vec::new(),
            current_pot_: Pot::default(),
        }
    }

    /// Computes the legal decision options for `player`.
    ///
    /// This maps C++ `Dealer::getOptions`.
    ///
    /// # Errors
    ///
    /// Returns an error if `player` or another registered player has no
    /// matching entry in `players`, or if the dealer has too few players to
    /// identify the big blind.
    pub fn get_options(
        &self,
        player: PlayerId,
        players: &[Player],
    ) -> Result<Options, DealerError> {
        let player_state = find_player(players, player)?;
        if !player_state.has_hole_cards()
            || player_state.is_allin(get_allin_amount(player, &self.bets_))
            || !self.action_required(player, players)?
        {
            return Ok(Options::default());
        }

        let (current_bet, previous_bet) = get_current_bets(&self.bets_);
        let mut max_amount = get_max_amount(&self.players_, players)?;
        if max_amount > player_state.get_stack() {
            max_amount = player_state.get_stack();
        }
        if max_amount == Stack::default() {
            return Ok(Options::default());
        }

        let big_blind_player = self.big_blind_player()?;
        let options = if current_bet == Stack::default()
            || (self.street == Street::Preflop
                && current_bet == self.big_blind_
                && player == big_blind_player)
        {
            vec![
                (Decision::Check, (Stack::default(), Stack::default())),
                (
                    Decision::Raise,
                    (take_smaller(self.big_blind_, max_amount), max_amount),
                ),
            ]
        } else if current_bet < max_amount {
            vec![
                (Decision::Fold, (Stack::default(), Stack::default())),
                (Decision::Call, (current_bet, current_bet)),
                (
                    Decision::Raise,
                    (
                        take_smaller(
                            current_bet + take_bigger(current_bet - previous_bet, self.big_blind_),
                            max_amount,
                        ),
                        max_amount,
                    ),
                ),
            ]
        } else {
            vec![
                (Decision::Fold, (Stack::default(), Stack::default())),
                (Decision::Call, (max_amount, max_amount)),
            ]
        };
        Ok(Options { options })
    }

    /// Books a betting action into the current betting-round contributions.
    ///
    /// This maps C++ `Dealer::acceptBet`.
    pub fn accept_bet(&mut self, player: PlayerId, betting_action: &BettingAction) {
        let current = self.bets_.entry(player).or_default();
        let mut to_be_booked = betting_action.next_bet - *current;
        if to_be_booked < Stack::default() {
            to_be_booked = Stack::default();
        }
        *current += to_be_booked;
        self.street = betting_action.street;
    }

    /// Moves current-round bets into the current pot and closes any all-in side pots.
    ///
    /// This maps C++ `Dealer::rakeIn`.
    ///
    /// # Errors
    ///
    /// Returns an error if a bet references a player ID absent from `players`.
    pub fn rake_in(&mut self, players: &mut [Player]) -> Result<(), DealerError> {
        let mut all_in_amounts = BTreeSet::new();
        for (&player_id, &amount) in &self.bets_ {
            let player = find_player(players, player_id)?;
            if player.is_allin(amount) && allin_is_called(amount, &self.bets_) {
                all_in_amounts.insert(amount);
            }
        }

        let mut previous_all_in = Stack::default();
        for all_in_amount in all_in_amounts {
            for (&player_id, amount) in &mut self.bets_ {
                let part = amount.get_amount(all_in_amount - previous_all_in);
                let contributed = find_player_mut(players, player_id)?.get_amount(part);
                self.current_pot_.put_amount(player_id, contributed);
            }
            self.closed_pots_.push(self.current_pot_.clone());
            self.current_pot_ = Pot::default();
            previous_all_in = all_in_amount;
        }

        for (&player_id, amount) in &self.bets_ {
            let contributed = find_player_mut(players, player_id)?.get_amount(*amount);
            self.current_pot_.put_amount(player_id, contributed);
        }
        self.bets_.clear();
        Ok(())
    }

    /// Awards the pot when only one player remains or performs a river showdown.
    ///
    /// Returns `true` when the hand is complete.
    ///
    /// This maps C++ `Dealer::awardPots`.
    ///
    /// # Errors
    ///
    /// Returns an error if a pot participant is missing or hand evaluation
    /// fails for the supplied board and player cards.
    pub fn award_pots(
        &mut self,
        board: &Board,
        hand_history: &mut HandHistory,
        players: &mut [Player],
    ) -> Result<bool, DealerError> {
        if self.closed_pots_.is_empty() {
            let mut remaining = Vec::new();
            for &player_id in self.current_pot_.get_bets().keys() {
                if find_player(players, player_id)?.has_hole_cards() {
                    remaining.push(player_id);
                }
            }
            if remaining.len() == 1 {
                let player_id = remaining[0];
                let amount = self.current_pot_.get();
                let player = find_player_mut(players, player_id)?;
                player.put_amount(amount);
                hand_history.log_action(LoggedAction::Pot(PotAction::new(
                    player,
                    amount,
                    &self.current_pot_,
                )));
                return Ok(true);
            }
        }

        if board.street() == Street::River {
            showdown(&mut self.current_pot_, board, hand_history, players)?;
            for side_pot in self.closed_pots_.iter_mut().rev() {
                showdown(side_pot, board, hand_history, players)?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Returns a mutable reference to the current pot.
    ///
    /// This maps C++ `Dealer::getCurrentPot`.
    pub fn get_current_pot(&mut self) -> &mut Pot {
        &mut self.current_pot_
    }

    /// Returns a player's current-round bet, or zero when no bet is recorded.
    ///
    /// This maps C++ `Dealer::getCurrentBet`.
    #[must_use]
    pub fn get_current_bet(&self, player: PlayerId) -> Stack {
        self.bets_.get(&player).copied().unwrap_or_default()
    }

    /// Returns the stable ID of the big blind player.
    ///
    /// This maps C++ `Dealer::bigBlindPlayer`.
    ///
    /// # Errors
    ///
    /// Returns [`DealerError::NotEnoughPlayers`] when the registered list
    /// cannot identify a big blind.
    pub fn big_blind_player(&self) -> Result<PlayerId, DealerError> {
        if self.players_.len() == 2 {
            self.players_.get(1).copied()
        } else {
            self.players_.get(2).copied()
        }
        .ok_or(DealerError::NotEnoughPlayers)
    }

    /// Returns the closed side pots.
    #[must_use]
    pub fn closed_pots(&self) -> &[Pot] {
        &self.closed_pots_
    }

    /// Determines whether the player still needs to act.
    ///
    /// This maps to C++ `Dealer::actionRequired`.
    fn action_required(&self, player: PlayerId, players: &[Player]) -> Result<bool, DealerError> {
        let Some(&player_bet) = self.bets_.get(&player) else {
            return Ok(true);
        };
        let (highest_bet, _) = get_current_bets(&self.bets_);
        if self.street == Street::Preflop
            && player == self.big_blind_player()?
            && highest_bet == self.big_blind_
        {
            let mut players_with_cards = 0;
            for &player_id in &self.players_ {
                if find_player(players, player_id)?.has_hole_cards() {
                    players_with_cards += 1;
                }
            }
            return Ok(players_with_cards > 1);
        }
        Ok(highest_bet != player_bet)
    }
}

/// Returns the distinct bet totals sorted from highest to lowest.
///
/// This maps C++ helper `getMadeBets`.
fn get_made_bets(bets: &BTreeMap<PlayerId, Stack>) -> Vec<Stack> {
    let mut made_bets = bets.values().copied().collect::<Vec<_>>();
    made_bets.sort_by(|left, right| right.cmp(left));
    made_bets.dedup();
    made_bets
}

/// Returns the highest and second-highest distinct bet totals.
///
/// This maps C++ helper `getCurrentBets`.
fn get_current_bets(bets: &BTreeMap<PlayerId, Stack>) -> (Stack, Stack) {
    let made_bets = get_made_bets(bets);
    (
        made_bets.first().copied().unwrap_or_default(),
        made_bets.get(1).copied().unwrap_or_default(),
    )
}

/// Returns a player's current-round bet, or zero when no bet has been recorded.
///
/// This maps C++ helper `getAllinAmount`.
fn get_allin_amount(player: PlayerId, bets: &BTreeMap<PlayerId, Stack>) -> Stack {
    bets.get(&player).copied().unwrap_or_default()
}

/// Returns IDs of players who still hold hole cards.
///
/// This maps C++ helper `getLeftInHand`.
fn get_left_in_hand(
    player_ids: &[PlayerId],
    players: &[Player],
) -> Result<Vec<PlayerId>, DealerError> {
    let mut left_in_hand = Vec::new();
    for &player_id in player_ids {
        if find_player(players, player_id)?.has_hole_cards() {
            left_in_hand.push(player_id);
        }
    }
    Ok(left_in_hand)
}

/// Returns the maximum total contribution allowed by the second-largest stack.
///
/// This maps C++ helper `getMaxAmount`.
fn get_max_amount(player_ids: &[PlayerId], players: &[Player]) -> Result<Stack, DealerError> {
    let left_in_hand = get_left_in_hand(player_ids, players)?;
    let mut stacks = Vec::with_capacity(left_in_hand.len());
    for player_id in left_in_hand {
        let player = find_player(players, player_id)?;
        stacks.push(player.get_stack());
    }
    stacks.sort_by(|left, right| right.cmp(left));
    stacks.get(1).copied().ok_or(DealerError::NotEnoughPlayers)
}

/// Returns the amount of `left` or `right`, whichever is smaller.
///
/// This maps C++ helper `takeSmaller`.
fn take_smaller(left: Stack, right: Stack) -> Stack {
    if left < right { left } else { right }
}

/// Returns the amount of `left` or `right`, whichever is larger.
///
/// This maps C++ helper `takeBigger`.
fn take_bigger(left: Stack, right: Stack) -> Stack {
    if left > right { left } else { right }
}

/// Determines whether a second bet covers an all-in amount.
///
/// This maps C++ helper `allinIsCalled`.
fn allin_is_called(all_in: Stack, bets: &BTreeMap<PlayerId, Stack>) -> bool {
    let mut sorted = bets.values().copied().collect::<Vec<_>>();
    sorted.sort();
    sorted.reverse();
    sorted.get(1).is_some_and(|second| *second >= all_in)
}

/// Divides chips evenly, assigning remainder chips in player order.
///
/// This preserves C++ `dividePot`; the integer field assumption is identical
/// to `Pot::get_amount`.
///
/// # Errors
///
/// Returns an error when `winners` is zero or cannot fit in a signed 32-bit
/// stack divisor.
pub fn divide_pot(pot: Stack, winners: usize) -> Result<Vec<Stack>, DealerError> {
    if pot == Stack::default() {
        return Ok(vec![Stack::default()]);
    }
    if winners == 0 {
        return Err(DealerError::CannotDividePot {
            amount: pot.0,
            winners,
        });
    }
    let winner_count = i32::try_from(winners).map_err(|_| DealerError::TooManyWinners(winners))?;
    let share = pot / winner_count;
    let remainder = usize::try_from(pot.0 % winner_count).unwrap_or(usize::MAX);
    let mut amounts = vec![share; winners];
    for amount in amounts.iter_mut().take(remainder) {
        *amount += Stack(1);
    }
    Ok(amounts)
}

/// Compares eligible players and awards a pot to the best hand or tied hands.
///
/// This maps C++ helper `showdown`.
fn showdown(
    pot: &mut Pot,
    board: &Board,
    hand_history: &mut HandHistory,
    players: &mut [Player],
) -> Result<(), DealerError> {
    let mut eligible = Vec::new();
    for &player_id in pot.get_bets().keys() {
        if find_player(players, player_id)?.has_hole_cards() {
            eligible.push(player_id);
        }
    }
    let mut winner: Option<(Hand, Vec<PlayerId>)> = None;
    for player_id in eligible {
        let player = find_player(players, player_id)?;
        let hole_cards = player.get_hole_cards();
        let hand = Hand::new(hole_cards, board.clone());
        match winner.as_mut() {
            None => {
                hand_history.log_action(LoggedAction::Showdown(ShowdownAction::new(
                    player,
                    Some(hand.clone()),
                )));
                winner = Some((hand, vec![player_id]));
            }
            Some((winning_hand, winning_players)) => match compare_hands(&hand, winning_hand) {
                1 => {
                    hand_history.log_action(LoggedAction::Showdown(ShowdownAction::new(
                        player,
                        Some(hand.clone()),
                    )));
                    *winning_hand = hand;
                    *winning_players = vec![player_id];
                }
                0 => {
                    hand_history.log_action(LoggedAction::Showdown(ShowdownAction::new(
                        player,
                        Some(hand),
                    )));
                    winning_players.push(player_id);
                }
                -1 => hand_history
                    .log_action(LoggedAction::Showdown(ShowdownAction::new(player, None))),
                _ => return Err(DealerError::InvalidHandComparison),
            },
        }
    }

    let (_, winner_ids) = winner.ok_or(DealerError::NoEligibleShowdownPlayer)?;
    let amounts = divide_pot(Stack(pot.get_amount()), winner_ids.len())?;
    for (player_id, amount) in winner_ids.into_iter().zip(amounts) {
        let player = find_player_mut(players, player_id)?;
        player.put_amount(amount);
        hand_history.log_action(LoggedAction::Pot(PotAction::new(player, amount, pot)));
    }
    Ok(())
}

/// Finds a player by stable ID.
fn find_player(players: &[Player], player_id: PlayerId) -> Result<&Player, DealerError> {
    players
        .iter()
        .find(|player| player.id() == player_id)
        .ok_or(DealerError::UnknownPlayer(player_id))
}

/// Finds a mutable player by stable ID.
fn find_player_mut(
    players: &mut [Player],
    player_id: PlayerId,
) -> Result<&mut Player, DealerError> {
    players
        .iter_mut()
        .find(|player| player.id() == player_id)
        .ok_or(DealerError::UnknownPlayer(player_id))
}

/// Errors raised while computing or awarding bets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DealerError {
    /// The requested ID is not in the supplied player state.
    UnknownPlayer(PlayerId),
    /// The operation requires a second active player or a registered blind.
    NotEnoughPlayers,
    /// A non-empty pot has no eligible winner.
    NoEligibleShowdownPlayer,
    /// The hand comparer returned a value other than -1, 0, or 1.
    InvalidHandComparison,
    /// Hand evaluation could not compare the supplied player and board cards.
    HandEvaluation(HandError),
    /// A non-empty pot cannot be split among zero winners.
    CannotDividePot {
        /// Pot size in chips.
        amount: i32,
        /// Requested number of winners.
        winners: usize,
    },
    /// Winner count cannot be represented by the assumed integer Stack API.
    TooManyWinners(usize),
}

impl std::fmt::Display for DealerError {
    /// Formats the dealer failure.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPlayer(player) => write!(formatter, "unknown player ID {player}"),
            Self::NotEnoughPlayers => formatter.write_str(
                "max amount could not be deduced as not enough players are left in the hand",
            ),
            Self::NoEligibleShowdownPlayer => {
                formatter.write_str("showdown pot has no eligible player")
            }
            Self::InvalidHandComparison => {
                formatter.write_str("hand comparison returned an invalid ordering")
            }
            Self::HandEvaluation(error) => std::fmt::Display::fmt(error, formatter),
            Self::CannotDividePot { amount, winners } => write!(
                formatter,
                "trying to divide pot {amount} to {winners} players."
            ),
            Self::TooManyWinners(winners) => {
                write!(
                    formatter,
                    "winner count {winners} exceeds Stack's integer range"
                )
            }
        }
    }
}

impl std::error::Error for DealerError {}

impl From<HandError> for DealerError {
    /// Wraps a hand-evaluation failure from the showdown path.
    fn from(error: HandError) -> Self {
        Self::HandEvaluation(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card52::{Card52, Rank52, Suit};
    use crate::decision_engine::DecisionEngine;
    use crate::hole_cards::HoleCards;

    /// Creates a test player with an explicitly assigned stable ID.
    fn player(id: usize, number: i32, chips: i32) -> Player {
        let mut player = Player::new(Stack(chips), &DecisionEngine, number);
        player.set_player_id(id);
        player
    }

    /// Marks all test players as active using distinct valid cards.
    fn deal_test_hole_cards(players: &mut [Player]) {
        let cards = [
            HoleCards::new(
                Card52::new(Rank52::Ace, Suit::Hearts),
                Card52::new(Rank52::King, Suit::Hearts),
            ),
            HoleCards::new(
                Card52::new(Rank52::Queen, Suit::Hearts),
                Card52::new(Rank52::Jack, Suit::Hearts),
            ),
            HoleCards::new(
                Card52::new(Rank52::Ten, Suit::Hearts),
                Card52::new(Rank52::Nine, Suit::Hearts),
            ),
        ];
        for (player, hole_cards) in players.iter_mut().zip(cards.iter()) {
            player.deal_hole_cards(hole_cards);
        }
    }

    /// Compares the exact legal-option sequence produced for one player.
    fn assert_options(
        dealer: &Dealer,
        players: &[Player],
        player_id: PlayerId,
        expected: &[(Decision, (Stack, Stack))],
    ) {
        assert_eq!(
            dealer.get_options(player_id, players).unwrap().options,
            expected
        );
    }

    /// Matches the source `DealerTests.cpp` opening legal-option ranges.
    #[test]
    fn dealer_tests_cpp_initial_options_parity() {
        let mut players = vec![player(0, 1, 2500), player(1, 2, 2000), player(2, 3, 1500)];
        deal_test_hole_cards(&mut players);
        let mut dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        let small_blind = BettingAction::create(
            &players[1],
            Street::Preflop,
            Stack(10),
            Stack::default(),
            Stack::default(),
            Decision::Raise,
        );
        let big_blind = BettingAction::create(
            &players[2],
            Street::Preflop,
            Stack(20),
            Stack::default(),
            Stack::default(),
            Decision::Raise,
        );
        dealer.accept_bet(1, &small_blind);
        dealer.accept_bet(2, &big_blind);

        assert_eq!(
            dealer.get_options(0, &players).unwrap().options,
            vec![
                (Decision::Fold, (Stack::default(), Stack::default())),
                (Decision::Call, (Stack(20), Stack(20))),
                (Decision::Raise, (Stack(40), Stack(2000))),
            ]
        );
    }

    /// Ports every legal-option assertion from the C++ normal all-in section.
    #[test]
    fn dealer_tests_cpp_normal_all_in_options_parity() {
        let (mut dealer, mut players) = normal_all_in_preflop_state();
        normal_all_in_flop(&mut dealer, &mut players);
        normal_all_in_turn(&mut dealer, &mut players);
    }

    /// Creates the six preflop betting actions for the normal all-in fixture.
    ///
    /// The player slice must use the fixture's three-player order: small blind,
    /// big blind, then the remaining player.
    ///
    /// # Panics
    ///
    /// Panics if `players` contains fewer than three entries.
    fn normal_all_in_preflop_actions(players: &[Player]) -> [BettingAction; 6] {
        [
            BettingAction::create(
                &players[1],
                Street::Preflop,
                Stack(10),
                Stack(0),
                Stack(0),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[2],
                Street::Preflop,
                Stack(20),
                Stack(0),
                Stack(0),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[0],
                Street::Preflop,
                Stack(60),
                Stack(0),
                Stack(20),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[1],
                Street::Preflop,
                Stack(0),
                Stack(10),
                Stack(60),
                Decision::Fold,
            ),
            BettingAction::create(
                &players[2],
                Street::Preflop,
                Stack(200),
                Stack(20),
                Stack(60),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[0],
                Street::Preflop,
                Stack(200),
                Stack(60),
                Stack(200),
                Decision::Call,
            ),
        ]
    }

    /// Builds the normal all-in fixture through the preflop round and rakes
    /// that round's contributions before returning the dealer and players.
    ///
    /// Assertions verify the expected legal options at each preflop decision.
    ///
    /// # Panics
    ///
    /// Panics if an expected option differs or raking returns an error.
    fn normal_all_in_preflop_state() -> (Dealer, Vec<Player>) {
        let mut players = vec![player(0, 1, 2500), player(1, 2, 1500), player(2, 3, 2000)];
        deal_test_hole_cards(&mut players);
        let mut dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        let actions = normal_all_in_preflop_actions(&players);
        dealer.accept_bet(1, &actions[0]);
        dealer.accept_bet(2, &actions[1]);
        assert_options(
            &dealer,
            &players,
            0,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(20), Stack(20))),
                (Decision::Raise, (Stack(40), Stack(2000))),
            ],
        );
        dealer.accept_bet(0, &actions[2]);
        assert_options(
            &dealer,
            &players,
            1,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(60), Stack(60))),
                (Decision::Raise, (Stack(100), Stack(1500))),
            ],
        );
        dealer.accept_bet(1, &actions[3]);
        players[1].deal_hole_cards(&HoleCards::default());
        assert_options(
            &dealer,
            &players,
            2,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(60), Stack(60))),
                (Decision::Raise, (Stack(100), Stack(2000))),
            ],
        );
        dealer.accept_bet(2, &actions[4]);
        assert_options(
            &dealer,
            &players,
            0,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(200), Stack(200))),
                (Decision::Raise, (Stack(340), Stack(2000))),
            ],
        );
        dealer.accept_bet(0, &actions[5]);
        for id in 0..3 {
            assert_options(&dealer, &players, id, &[]);
        }
        dealer.rake_in(&mut players).unwrap();
        (dealer, players)
    }

    /// Applies and checks the normal all-in fixture's flop bets, then rakes
    /// the completed round.
    ///
    /// Both arguments are mutated as the betting actions are accepted.
    ///
    /// # Panics
    ///
    /// Panics if an expected legal-option list differs or raking returns an
    /// error.
    fn normal_all_in_flop(dealer: &mut Dealer, players: &mut [Player]) {
        let flop_actions = [
            BettingAction::create(
                &players[2],
                Street::Flop,
                Stack(300),
                Stack(0),
                Stack(0),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[0],
                Street::Flop,
                Stack(300),
                Stack(0),
                Stack(300),
                Decision::Call,
            ),
        ];
        assert_options(
            dealer,
            players,
            2,
            &[
                (Decision::Check, (Stack(0), Stack(0))),
                (Decision::Raise, (Stack(20), Stack(1800))),
            ],
        );
        dealer.accept_bet(2, &flop_actions[0]);
        assert_options(
            dealer,
            players,
            0,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(300), Stack(300))),
                (Decision::Raise, (Stack(600), Stack(1800))),
            ],
        );
        dealer.accept_bet(0, &flop_actions[1]);
        for id in [0, 2] {
            assert_options(dealer, players, id, &[]);
        }

        dealer.rake_in(players).unwrap();
    }

    /// Applies and checks the normal all-in fixture's subsequent betting
    /// round, then rakes the completed round.
    ///
    /// Both arguments are mutated as the betting actions are accepted.
    ///
    /// # Panics
    ///
    /// Panics if an expected legal-option list differs or raking returns an
    /// error.
    fn normal_all_in_turn(dealer: &mut Dealer, players: &mut [Player]) {
        let turn_actions = [
            BettingAction::create(
                &players[2],
                Street::Flop,
                Stack(1500),
                Stack(0),
                Stack(0),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[0],
                Street::Flop,
                Stack(1500),
                Stack(0),
                Stack(1500),
                Decision::Call,
            ),
        ];
        assert_options(
            dealer,
            players,
            2,
            &[
                (Decision::Check, (Stack(0), Stack(0))),
                (Decision::Raise, (Stack(20), Stack(1500))),
            ],
        );
        dealer.accept_bet(2, &turn_actions[0]);
        assert_options(
            dealer,
            players,
            0,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(1500), Stack(1500))),
            ],
        );
        dealer.accept_bet(0, &turn_actions[1]);
        for id in [0, 2] {
            assert_options(dealer, players, id, &[]);
        }
        dealer.rake_in(players).unwrap();
        for id in [0, 2] {
            assert_options(dealer, players, id, &[]);
        }
    }

    /// Ports every legal-option assertion from the C++ forced all-in section.
    #[test]
    fn dealer_tests_cpp_forced_all_in_options_parity() {
        let (mut dealer, mut players) = forced_all_in_preflop_state();
        forced_all_in_flop(&mut dealer, &mut players);
        forced_all_in_turn(&mut dealer, &mut players);
    }

    /// Builds the short-stack forced all-in fixture through preflop and rakes
    /// the completed round before returning the dealer and players.
    ///
    /// Assertions verify the expected preflop legal options and that no player
    /// may act again after the all-in calls.
    ///
    /// # Panics
    ///
    /// Panics if an expected option differs or raking returns an error.
    fn forced_all_in_preflop_state() -> (Dealer, Vec<Player>) {
        let mut players = vec![player(0, 1, 2500), player(1, 2, 2000), player(2, 3, 15)];
        deal_test_hole_cards(&mut players);
        let mut dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        let preflop_actions = [
            BettingAction::create(
                &players[1],
                Street::Preflop,
                Stack(10),
                Stack(0),
                Stack(0),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[2],
                Street::Preflop,
                Stack(20),
                Stack(0),
                Stack(0),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[0],
                Street::Preflop,
                Stack(60),
                Stack(0),
                Stack(20),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[1],
                Street::Preflop,
                Stack(60),
                Stack(10),
                Stack(60),
                Decision::Call,
            ),
        ];
        dealer.accept_bet(1, &preflop_actions[0]);
        dealer.accept_bet(2, &preflop_actions[1]);
        assert_options(
            &dealer,
            &players,
            0,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(20), Stack(20))),
                (Decision::Raise, (Stack(40), Stack(2000))),
            ],
        );
        dealer.accept_bet(0, &preflop_actions[2]);
        assert_options(
            &dealer,
            &players,
            1,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(60), Stack(60))),
                (Decision::Raise, (Stack(100), Stack(2000))),
            ],
        );
        dealer.accept_bet(1, &preflop_actions[3]);
        for id in 0..3 {
            assert_options(&dealer, &players, id, &[]);
        }
        dealer.rake_in(&mut players).unwrap();
        (dealer, players)
    }

    /// Applies and checks the forced all-in fixture's flop bets, then rakes the
    /// completed round.
    ///
    /// Both arguments are mutated as the betting actions are accepted.
    ///
    /// # Panics
    ///
    /// Panics if an expected legal-option list differs or raking returns an
    /// error.
    fn forced_all_in_flop(dealer: &mut Dealer, players: &mut [Player]) {
        let flop_actions = [
            BettingAction::create(
                &players[1],
                Street::Flop,
                Stack(300),
                Stack(0),
                Stack(0),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[0],
                Street::Flop,
                Stack(300),
                Stack(0),
                Stack(300),
                Decision::Call,
            ),
        ];
        assert_options(
            dealer,
            players,
            1,
            &[
                (Decision::Check, (Stack(0), Stack(0))),
                (Decision::Raise, (Stack(20), Stack(1940))),
            ],
        );
        dealer.accept_bet(1, &flop_actions[0]);
        assert_options(dealer, players, 2, &[]);
        assert_options(
            dealer,
            players,
            0,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(300), Stack(300))),
                (Decision::Raise, (Stack(600), Stack(1940))),
            ],
        );
        dealer.accept_bet(0, &flop_actions[1]);
        for id in [0, 1] {
            assert_options(dealer, players, id, &[]);
        }

        dealer.rake_in(players).unwrap();
    }

    /// Creates the four ordered turn actions used to test the forced all-in
    /// sequence for the fixture's players.
    ///
    /// The fixture supplies three players in order; this helper reads the
    /// first two entries to create the check and betting actions.
    ///
    /// # Panics
    ///
    /// Panics if `players` contains fewer than two entries.
    fn forced_all_in_turn_actions(players: &[Player]) -> [BettingAction; 4] {
        [
            BettingAction::create(
                &players[1],
                Street::Turn,
                Stack(0),
                Stack(0),
                Stack(0),
                Decision::Check,
            ),
            BettingAction::create(
                &players[0],
                Street::Turn,
                Stack(1500),
                Stack(0),
                Stack(0),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[1],
                Street::Turn,
                Stack(1640),
                Stack(0),
                Stack(1500),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[0],
                Street::Turn,
                Stack(140),
                Stack(1500),
                Stack(1640),
                Decision::Raise,
            ),
        ]
    }

    /// Applies the forced all-in turn actions, checks the legal options after
    /// each action, and rakes the completed round.
    ///
    /// Both arguments are mutated as the betting actions are accepted.
    ///
    /// # Panics
    ///
    /// Panics if an expected legal-option list differs or raking returns an
    /// error.
    fn forced_all_in_turn(dealer: &mut Dealer, players: &mut [Player]) {
        let turn_actions = forced_all_in_turn_actions(players);
        assert_options(
            dealer,
            players,
            1,
            &[
                (Decision::Check, (Stack(0), Stack(0))),
                (Decision::Raise, (Stack(20), Stack(1640))),
            ],
        );
        dealer.accept_bet(1, &turn_actions[0]);
        assert_options(dealer, players, 2, &[]);
        assert_options(
            dealer,
            players,
            0,
            &[
                (Decision::Check, (Stack(0), Stack(0))),
                (Decision::Raise, (Stack(20), Stack(1640))),
            ],
        );
        dealer.accept_bet(0, &turn_actions[1]);
        assert_options(
            dealer,
            players,
            1,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(1500), Stack(1500))),
                (Decision::Raise, (Stack(1640), Stack(1640))),
            ],
        );
        dealer.accept_bet(1, &turn_actions[2]);
        assert_options(dealer, players, 2, &[]);
        assert_options(
            dealer,
            players,
            0,
            &[
                (Decision::Fold, (Stack(0), Stack(0))),
                (Decision::Call, (Stack(1640), Stack(1640))),
            ],
        );
        dealer.accept_bet(0, &turn_actions[3]);
        dealer.rake_in(players).unwrap();
        for id in 0..3 {
            assert_options(dealer, players, id, &[]);
        }
    }

    /// Preserves the big blind's option to check after matching preflop bets.
    #[test]
    fn dealer_tests_cpp_big_blind_action_parity() {
        let mut players = vec![player(0, 1, 2500), player(1, 2, 2000), player(2, 3, 1500)];
        deal_test_hole_cards(&mut players);
        let mut dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        for (id, amount) in [(1, 10), (2, 20), (0, 20), (1, 20)] {
            let action = BettingAction::create(
                &players[id],
                Street::Preflop,
                Stack(amount),
                Stack::default(),
                Stack::default(),
                Decision::Call,
            );
            dealer.accept_bet(id, &action);
        }

        assert_eq!(
            dealer.get_options(2, &players).unwrap().options,
            vec![
                (Decision::Check, (Stack::default(), Stack::default())),
                (Decision::Raise, (Stack(20), Stack(1500))),
            ]
        );
    }

    /// Confirms a lone big blind has no action after every other player folds.
    #[test]
    fn dealer_tests_cpp_folded_to_big_blind_parity() {
        let mut players = vec![player(0, 1, 2500), player(1, 2, 2000), player(2, 3, 1500)];
        deal_test_hole_cards(&mut players);
        let mut dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        for (id, amount) in [(1, 10), (2, 20), (0, 0), (1, 0)] {
            let action = BettingAction::create(
                &players[id],
                Street::Preflop,
                Stack(amount),
                Stack::default(),
                Stack(20),
                Decision::Fold,
            );
            dealer.accept_bet(id, &action);
            if id < 2 {
                players[id].deal_hole_cards(&HoleCards::default());
            }
        }

        assert_eq!(dealer.get_options(2, &players).unwrap().options, Vec::new());
    }

    #[test]
    /// Preserves the `DealerTests.cpp` preflop all-in side-pot chip totals.
    fn dealer_tests_cpp_preflop_all_in_side_pots_parity() {
        let mut players = vec![player(0, 1, 1000), player(1, 2, 1500), player(2, 3, 2000)];
        let mut dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        let actions = [
            BettingAction::create(
                &players[0],
                Street::Preflop,
                Stack(1000),
                Stack::default(),
                Stack(20),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[1],
                Street::Preflop,
                Stack(1000),
                Stack::default(),
                Stack(1000),
                Decision::Call,
            ),
            BettingAction::create(
                &players[2],
                Street::Preflop,
                Stack(1500),
                Stack::default(),
                Stack(1000),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[1],
                Street::Preflop,
                Stack(1500),
                Stack(1000),
                Stack(1500),
                Decision::Call,
            ),
        ];
        dealer.accept_bet(0, &actions[0]);
        dealer.accept_bet(1, &actions[1]);
        dealer.accept_bet(2, &actions[2]);
        dealer.accept_bet(1, &actions[3]);
        dealer.rake_in(&mut players).unwrap();

        assert_eq!(dealer.closed_pots().len(), 2);
        assert_eq!(dealer.closed_pots()[0].get_amount(), 3000);
        assert_eq!(dealer.closed_pots()[1].get_amount(), 1000);
        assert_eq!(dealer.get_current_pot().get_amount(), 0);
        assert_eq!(players[0].get_stack(), Stack::default());
        assert_eq!(players[1].get_stack(), Stack::default());
        assert_eq!(players[2].get_stack(), Stack(500));
    }

    /// Preserves the `DealerTests.cpp` excess-all-in refund in the current pot.
    #[test]
    fn dealer_tests_cpp_preflop_all_in_refund_parity() {
        let mut players = vec![player(0, 1, 3000), player(1, 2, 1500), player(2, 3, 2000)];
        let mut dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        let bets = [
            BettingAction::create(
                &players[0],
                Street::Preflop,
                Stack(2000),
                Stack::default(),
                Stack(20),
                Decision::Raise,
            ),
            BettingAction::create(
                &players[1],
                Street::Preflop,
                Stack(1500),
                Stack::default(),
                Stack(2000),
                Decision::Call,
            ),
            BettingAction::create(
                &players[2],
                Street::Preflop,
                Stack::default(),
                Stack::default(),
                Stack(2000),
                Decision::Fold,
            ),
        ];
        for (id, action) in [0, 1, 2].into_iter().zip(&bets) {
            dealer.accept_bet(id, action);
        }
        dealer.rake_in(&mut players).unwrap();

        assert_eq!(dealer.closed_pots().len(), 1);
        assert_eq!(dealer.closed_pots()[0].get_amount(), 3000);
        assert_eq!(dealer.get_current_pot().get_amount(), 500);
        assert_eq!(players[0].get_stack(), Stack(1000));
        assert_eq!(players[1].get_stack(), Stack::default());
        assert_eq!(players[2].get_stack(), Stack(2000));
    }

    #[test]
    /// Assigns odd-pot remainder chips without losing chips.
    fn divides_split_pots_without_losing_remainder_chips() {
        assert_eq!(
            divide_pot(Stack(11), 3).unwrap(),
            vec![Stack(4), Stack(4), Stack(3)]
        );
        assert_eq!(
            divide_pot(Stack::default(), 0).unwrap(),
            vec![Stack::default()]
        );
        assert!(matches!(
            divide_pot(Stack(11), 0),
            Err(DealerError::CannotDividePot {
                amount: 11,
                winners: 0
            })
        ));
    }
}
