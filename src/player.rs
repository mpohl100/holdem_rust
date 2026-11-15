//! Player state and decisions translated from `Player.h` and `Player.cpp`.

use std::sync::Arc;

use crate::board::Board;
use crate::dealer::Dealer;
use crate::decision_engine::{DecisionEngine, DecisionEngineError};
use crate::hand_history::HandHistory;
use crate::hole_cards::HoleCards;
use crate::stack::Stack;

/// Stable identifier for a player in a table's player collection.
///
/// The C++ modules use `Player*` as identity in pots and bets. This index
/// replaces pointer identity and remains unchanged when seat order rotates.
pub type PlayerId = usize;

/// Table position, mapped one-to-one from C++ `Position`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    /// Under the gun.
    UTG,
    /// One seat after under the gun.
    UTGp1,
    /// Two seats after under the gun.
    UTGp2,
    /// Lojack.
    Lojack,
    /// Hijack.
    Hijack,
    /// Cutoff.
    Cutoff,
    /// Dealer button.
    Button,
    /// Small blind.
    SmallBlind,
    /// Big blind.
    BigBlind,
}

/// Decision made by a player, mapped one-to-one from C++ `Decision`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Decision {
    /// Fold the hand.
    Fold,
    /// Check without adding chips.
    Check,
    /// Call the current bet.
    Call,
    /// Raise the current bet.
    Raise,
}

/// Formats a table position using the original hand-history labels.
///
/// This maps C++ free function `toString(Position)` to `position_to_string`.
#[must_use]
pub fn position_to_string(position: Position) -> &'static str {
    match position {
        Position::Button => "Button",
        Position::SmallBlind => "Small Blind",
        Position::BigBlind => "Big Blind",
        Position::UTG => "UTG",
        Position::UTGp1 => "UTG+1",
        Position::UTGp2 => "UTG+2",
        Position::Lojack => "Lojack",
        Position::Hijack => "Hijack",
        Position::Cutoff => "Cutoff",
    }
}

/// Player state, mapped one-to-one from C++ `Player`.
#[derive(Clone, Debug)]
pub struct Player {
    /// Stable index replacing the pointer identity used by C++ pots/dealers.
    /// Stable Rust identity used in place of C++ player-pointer keys.
    id: PlayerId,
    /// Remaining chips; maps to C++ mutable field `stack_`.
    stack_: Stack,
    /// Shared decision engine; maps to C++ `engine_`.
    engine_: Arc<DecisionEngine>,
    /// Hole cards currently held; maps to C++ `holeCards_`.
    hole_cards_: HoleCards,
    /// Seat index and table size; maps to C++ `pos_`.
    pos_: (i32, usize),
    /// Display number; maps to C++ `nb_`.
    nb_: i32,
}

impl Default for Player {
    /// Creates the safe default state corresponding to C++ `Player()`.
    ///
    /// The C++ default leaves `nb_` uninitialized; Rust uses zero so the value
    /// remains valid even when a default `Player` is logged accidentally.
    fn default() -> Self {
        Self {
            id: 0,
            stack_: Stack::default(),
            engine_: Arc::new(DecisionEngine),
            hole_cards_: HoleCards::default(),
            pos_: (0, 2),
            nb_: 0,
        }
    }
}

impl Player {
    /// Creates a player with a stack, shared decision engine, and display number.
    ///
    /// This maps to C++ `Player(Stack, DecisionEngine const&, int)`.
    /// The stable ID defaults to zero because C++ has no ID field; table
    /// owners should assign a unique value with `set_player_id`.
    #[must_use]
    pub fn new(stack: Stack, engine: &DecisionEngine, number: i32) -> Self {
        Self {
            id: 0,
            stack_: stack,
            engine_: Arc::new(engine.clone()),
            hole_cards_: HoleCards::default(),
            pos_: (0, 2),
            nb_: number,
        }
    }

    /// Returns this player's stable table identifier.
    ///
    /// This is the safe replacement for the C++ pointer identity and has no
    /// separate C++ accessor.
    #[must_use]
    pub fn id(&self) -> PlayerId {
        self.id
    }

    /// Sets this player's stable table identifier.
    ///
    /// This assigns the index used instead of C++ pointer identity.
    pub fn set_player_id(&mut self, player_id: PlayerId) {
        self.id = player_id;
    }

    /// Withdraws up to `amount` chips, returning the amount actually withdrawn.
    ///
    /// This maps C++ `getAmount`; `Stack::get_amount` is assumed to perform the
    /// source operation of mutating the stack and returning the withdrawn value.
    pub fn get_amount(&mut self, amount: Stack) -> Stack {
        self.stack_.get_amount(amount)
    }

    /// Adds chips to the player's stack.
    ///
    /// This maps C++ `putAmount`.
    pub fn put_amount(&mut self, amount: Stack) {
        self.stack_ += amount;
    }

    /// Replaces this player's hole cards.
    ///
    /// This maps C++ `dealHoleCards`.
    pub fn deal_hole_cards(&mut self, hole_cards: &HoleCards) {
        self.hole_cards_ = hole_cards.clone();
    }

    /// Returns whether this player currently holds hole cards.
    ///
    /// This maps C++ `hasHoleCards`.
    #[must_use]
    pub fn has_hole_cards(&self) -> bool {
        !self.hole_cards_.empty()
    }

    /// Returns a clone of this player's hole cards.
    ///
    /// This maps C++ `getHoleCards`.
    #[must_use]
    pub fn get_hole_cards(&self) -> HoleCards {
        self.hole_cards_.clone()
    }

    /// Chooses and accepts a betting action, logs it, and clears folded cards.
    ///
    /// This maps C++ `decide`; the player slice replaces the dealer's C++
    /// references and enables safe ID-based access to all table state.
    /// `players` contains the current table state. It is passed separately
    /// because Rust cannot safely alias `self` mutably inside that slice.
    ///
    /// # Errors
    ///
    /// Returns an error if the decision engine cannot obtain legal options
    /// for this player.
    pub fn decide(
        &mut self,
        dealer: &mut Dealer,
        board: &Board,
        hand_history: &mut HandHistory,
        players: &[Player],
    ) -> Result<Decision, DecisionEngineError> {
        let engine = Arc::clone(&self.engine_);
        let action = engine.decide(dealer, board, self, players)?;
        hand_history.log_action(crate::action::LoggedAction::Betting(action.clone()));
        dealer.accept_bet(self.id, &action);
        if action.decision == Decision::Fold {
            self.hole_cards_ = HoleCards::default();
        }
        Ok(action.decision)
    }

    /// Returns whether this player has matched `current_bet` on `board`.
    ///
    /// This maps C++ `ready`.
    #[must_use]
    pub fn ready(&self, current_bet: Stack, board: &Board, hand_history: &HandHistory) -> bool {
        if !self.has_hole_cards() {
            return true;
        }
        hand_history
            .get_last_bet(board.street(), Some(self.id))
            .is_some_and(|last_bet| last_bet.next_bet == current_bet)
    }

    /// Sets this player's seat index and total number of seats.
    ///
    /// This maps C++ `setPlayerIndex`.
    pub fn set_player_index(&mut self, index: i32, size: usize) {
        self.pos_ = (index, size);
    }

    /// Returns this player's table position.
    ///
    /// Unsupported table sizes return an error; out-of-range indices for
    /// supported table sizes retain the C++ fallback `Button`.
    ///
    /// This maps C++ `getPosition`.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerPositionError`] when the configured table size is
    /// outside `2..=9`.
    ///
    /// # Errors
    ///
    /// Returns [`PlayerPositionError`] when the configured table size is
    /// outside `2..=9`.
    pub fn get_position(&self) -> Result<Position, PlayerPositionError> {
        let (index, size) = self.pos_;
        let position_index = usize::try_from(index).ok();
        let position = match size {
            2 => [Position::Button, Position::BigBlind]
                .get(position_index.unwrap_or(usize::MAX))
                .copied(),
            3 => [Position::Button, Position::SmallBlind, Position::BigBlind]
                .get(position_index.unwrap_or(usize::MAX))
                .copied(),
            4 => [
                Position::Button,
                Position::SmallBlind,
                Position::BigBlind,
                Position::Cutoff,
            ]
            .get(position_index.unwrap_or(usize::MAX))
            .copied(),
            5 => [
                Position::Button,
                Position::SmallBlind,
                Position::BigBlind,
                Position::Hijack,
                Position::Cutoff,
            ]
            .get(position_index.unwrap_or(usize::MAX))
            .copied(),
            6 => [
                Position::Button,
                Position::SmallBlind,
                Position::BigBlind,
                Position::Lojack,
                Position::Hijack,
                Position::Cutoff,
            ]
            .get(position_index.unwrap_or(usize::MAX))
            .copied(),
            7 => [
                Position::Button,
                Position::SmallBlind,
                Position::BigBlind,
                Position::UTGp2,
                Position::Lojack,
                Position::Hijack,
                Position::Cutoff,
            ]
            .get(position_index.unwrap_or(usize::MAX))
            .copied(),
            8 => [
                Position::Button,
                Position::SmallBlind,
                Position::BigBlind,
                Position::UTGp1,
                Position::UTGp2,
                Position::Lojack,
                Position::Hijack,
                Position::Cutoff,
            ]
            .get(position_index.unwrap_or(usize::MAX))
            .copied(),
            9 => [
                Position::Button,
                Position::SmallBlind,
                Position::BigBlind,
                Position::UTG,
                Position::UTGp1,
                Position::UTGp2,
                Position::Lojack,
                Position::Hijack,
                Position::Cutoff,
            ]
            .get(position_index.unwrap_or(usize::MAX))
            .copied(),
            _ => None,
        };
        if !(2..=9).contains(&size) {
            return Err(PlayerPositionError { index, size });
        }
        Ok(position.unwrap_or(Position::Button))
    }

    /// Returns the player's current stack.
    ///
    /// This maps C++ `getStack`.
    #[must_use]
    pub fn get_stack(&self) -> Stack {
        self.stack_
    }

    /// Returns the player's display number.
    ///
    /// This maps C++ `getNumber`.
    #[must_use]
    pub fn get_number(&self) -> i32 {
        self.nb_
    }

    /// Returns the hand-history player name.
    ///
    /// This maps C++ `getName`.
    #[must_use]
    pub fn get_name(&self) -> String {
        format!("Player {}", self.nb_)
    }

    /// Returns whether the player's remaining stack is no greater than `bet`.
    ///
    /// This maps C++ `isAllin`.
    #[must_use]
    pub fn is_allin(&self, bet: Stack) -> bool {
        self.stack_ <= bet
    }
}

/// Unsupported table size for a position lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerPositionError {
    /// Requested seat index.
    pub index: i32,
    /// Table size used for the lookup.
    pub size: usize,
}

impl std::fmt::Display for PlayerPositionError {
    /// Formats the same invalid-position diagnostic used by C++.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "wrong position indeces: index {} size {}",
            self.index, self.size
        )
    }
}

impl std::error::Error for PlayerPositionError {}
