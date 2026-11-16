//! Hand-log actions translated from `Action.h` and `Action.cpp`.

use std::fmt::{self, Write as _};

use crate::board::{Board, Street};
use crate::hand::Hand;
use crate::hole_cards::HoleCards;
use crate::player::{Decision, Player};
use crate::pot::Pot;
use crate::stack::Stack;

/// Common behavior of a logged action.
///
/// The C++ abstract `HandAction` base class becomes a trait because Rust does
/// not support class inheritance. Concrete actions retain their own public
/// `player` field, corresponding to `HandAction::player`.
pub trait HandAction {
    /// Returns the player snapshot stored with this action.
    fn player(&self) -> &Player;

    /// Formats this action as a hand-history line.
    ///
    /// This explicitly maps C++ `toString()` to Rust `to_string()`.
    fn to_string(&self) -> String;
}

/// A player's action while betting.
#[derive(Clone, Debug)]
pub struct BettingAction {
    /// Player snapshot; maps to the inherited C++ `HandAction::player`.
    pub player: Player,
    /// Bet size before this action; maps to C++ `previousBet`.
    pub previous_bet: Stack,
    /// Total bet size after this action; maps to C++ `nextBet`.
    pub next_bet: Stack,
    /// Current amount to call; maps to C++ `toCall`.
    pub to_call: Stack,
    /// Chosen poker decision.
    pub decision: Decision,
    /// Betting round in which the action occurred.
    pub street: Street,
}

impl BettingAction {
    /// Creates a default check action for `player` on `street`.
    #[must_use]
    pub fn new(player: &Player, street: Street) -> Self {
        Self {
            player: player.clone(),
            previous_bet: Stack::default(),
            next_bet: Stack::default(),
            to_call: Stack::default(),
            decision: Decision::Check,
            street,
        }
    }

    /// Creates a betting action with all values supplied.
    ///
    /// This maps to C++ `BettingAction::create`.
    #[must_use]
    pub fn create(
        player: &Player,
        street: Street,
        next_bet: Stack,
        previous_bet: Stack,
        to_call: Stack,
        decision: Decision,
    ) -> Self {
        Self {
            player: player.clone(),
            previous_bet,
            next_bet,
            to_call,
            decision,
            street,
        }
    }
}

impl Default for BettingAction {
    /// Creates the safe default counterpart of C++ `BettingAction()`.
    ///
    /// The C++ default constructor leaves `street` uninitialized; Rust uses
    /// `Preflop` as a safe placeholder.
    fn default() -> Self {
        Self {
            player: Player::default(),
            previous_bet: Stack::default(),
            next_bet: Stack::default(),
            to_call: Stack::default(),
            decision: Decision::Check,
            street: Street::Preflop,
        }
    }
}

impl HandAction for BettingAction {
    /// Returns this action's player snapshot.
    fn player(&self) -> &Player {
        &self.player
    }

    /// Formats this betting decision.
    fn to_string(&self) -> String {
        let name = self.player.get_name();
        match self.decision {
            Decision::Fold => format!("{name}:  folds."),
            Decision::Check => format!("{name}:  checks."),
            Decision::Call => format!("{name}:  calls {}.", self.to_call - self.previous_bet),
            Decision::Raise => format!(
                "{name}:  raises {} to {}.",
                self.next_bet - self.to_call,
                self.next_bet
            ),
        }
    }
}

/// Records hole cards dealt to a player.
#[derive(Clone, Debug)]
pub struct DealingAction {
    /// Player snapshot; maps to the inherited C++ `HandAction::player`.
    pub player: Player,
    /// Cards dealt to the player; maps to C++ `holeCards`.
    pub hole_cards: HoleCards,
}

impl DealingAction {
    /// Creates the source-declared hole-card-only constructor's equivalent.
    ///
    /// C++ `DealingAction(HoleCards const&)` leaves the inherited player at its
    /// default value; the NLHE controller assigns the player afterward.
    #[must_use]
    pub fn from_hole_cards(hole_cards: &HoleCards) -> Self {
        Self {
            player: Player::default(),
            hole_cards: hole_cards.clone(),
        }
    }

    /// Creates a dealing action for `player` and `hole_cards`.
    ///
    /// This convenience constructor combines C++ default construction,
    /// player assignment, and the hole-card-only constructor.
    #[must_use]
    pub fn new(player: &Player, hole_cards: &HoleCards) -> Self {
        Self {
            player: player.clone(),
            hole_cards: hole_cards.clone(),
        }
    }
}

impl HandAction for DealingAction {
    /// Returns this action's player snapshot.
    fn player(&self) -> &Player {
        &self.player
    }

    /// Formats the dealt cards.
    fn to_string(&self) -> String {
        format!("dealt to {} [{}]", self.player.get_name(), self.hole_cards)
    }
}

/// Records a board state reached during the hand.
#[derive(Clone, Debug, Default)]
pub struct BoardAction {
    /// Player snapshot; maps to the inherited C++ `HandAction::player`.
    pub player: Player,
    /// Board state being recorded; maps to C++ `BoardAction::board`.
    pub board: Board,
}

impl HandAction for BoardAction {
    /// Returns the default inherited player snapshot.
    fn player(&self) -> &Player {
        &self.player
    }

    /// Formats the board street and cards.
    fn to_string(&self) -> String {
        let mut line = match self.board.street() {
            Street::Preflop => "*** HOLE CARDS ***".to_owned(),
            Street::Flop => "*** FLOP ***".to_owned(),
            Street::Turn => "*** TURN ***".to_owned(),
            Street::River => "*** RIVER ***".to_owned(),
        };
        let cards = self.board.get_cards();
        let first_count = if self.board.street() == Street::Flop {
            cards.len()
        } else {
            cards.len().saturating_sub(1)
        };
        let mut first_cards = cards[..first_count]
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        if matches!(
            self.board.street(),
            Street::Flop | Street::Turn | Street::River
        ) && !first_cards.is_empty()
        {
            first_cards = format!("[{first_cards}]");
        }
        if matches!(self.board.street(), Street::Turn | Street::River)
            && let Some(last) = cards.last()
        {
            let last = last.to_string();
            if !first_cards.is_empty() {
                first_cards.push(' ');
            }
            write!(first_cards, "[{last}]").expect("writing to a String cannot fail");
        }
        if !first_cards.is_empty() {
            line.push('\n');
            line.push_str(&first_cards);
        }
        line
    }
}

/// Records a player's seat and starting stack.
#[derive(Clone, Debug, Default)]
pub struct SeatingAction {
    /// Player snapshot; maps to the inherited C++ `HandAction::player`.
    pub player: Player,
}

impl HandAction for SeatingAction {
    /// Returns the seated player's snapshot.
    fn player(&self) -> &Player {
        &self.player
    }

    /// Formats the player's seat information.
    fn to_string(&self) -> String {
        let position = match self.player.get_position() {
            Ok(position) => crate::player::position_to_string(position).to_owned(),
            Err(error) => panic!("{error}"),
        };
        format!(
            "Seat {}: {}({}) ({} in chips)",
            self.player.get_number(),
            self.player.get_name(),
            position,
            self.player.get_stack()
        )
    }
}

/// Records whether a player showed or mucked their hand.
#[derive(Clone, Debug, Default)]
pub struct ShowdownAction {
    /// Player snapshot; maps to the inherited C++ `HandAction::player`.
    pub player: Player,
    /// The evaluated hand, or `None` when the player mucks; maps to the
    /// optional-valued C++ `std::optional<Hand> hand`.
    pub hand: Option<Hand>,
}

impl ShowdownAction {
    /// Creates a showdown action for `player` and an optional shown hand.
    #[must_use]
    pub fn new(player: &Player, hand: Option<Hand>) -> Self {
        Self {
            player: player.clone(),
            hand,
        }
    }
}

impl HandAction for ShowdownAction {
    /// Returns this action's player snapshot.
    fn player(&self) -> &Player {
        &self.player
    }

    /// Formats the shown or mucked hand.
    fn to_string(&self) -> String {
        match &self.hand {
            None => format!("{} mucks.", self.player.get_name()),
            Some(hand) => format!(
                "{} shows [{}], {}",
                self.player.get_name(),
                hand.get_hole_cards(),
                hand
            ),
        }
    }
}

/// Records a pot award.
#[derive(Clone, Debug, Default)]
pub struct PotAction {
    /// Player snapshot; maps to the inherited C++ `HandAction::player`.
    pub player: Player,
    /// Chips awarded.
    pub amount: Stack,
    /// Pot from which chips were awarded.
    pub pot: Pot,
}

impl PotAction {
    /// Creates a pot award record.
    #[must_use]
    pub fn new(player: &Player, amount: Stack, pot: &Pot) -> Self {
        Self {
            player: player.clone(),
            amount,
            pot: pot.clone(),
        }
    }
}

impl HandAction for PotAction {
    /// Returns this award action's player snapshot.
    fn player(&self) -> &Player {
        &self.player
    }

    /// Formats the pot award.
    fn to_string(&self) -> String {
        format!(
            "{} wins {} from {}",
            self.player.get_name(),
            self.amount,
            self.pot
        )
    }
}

/// Final hand-summary action.
#[derive(Clone, Debug, Default)]
pub struct SummaryAction {
    /// Player snapshot; maps to the inherited C++ `HandAction::player`.
    pub player: Player,
}

impl HandAction for SummaryAction {
    /// Returns the default inherited player snapshot.
    fn player(&self) -> &Player {
        &self.player
    }

    /// Returns the empty summary text.
    fn to_string(&self) -> String {
        String::new()
    }
}

/// Owned action variants stored in a hand history.
///
/// This enum replaces the C++ `std::unique_ptr<HandAction>` polymorphic
/// collection while retaining every concrete action type.
#[derive(Clone, Debug)]
pub enum LoggedAction {
    /// A betting action.
    Betting(BettingAction),
    /// A dealing action.
    Dealing(DealingAction),
    /// A board action.
    Board(BoardAction),
    /// A seating action.
    Seating(SeatingAction),
    /// A showdown action.
    Showdown(ShowdownAction),
    /// A pot award action.
    Pot(PotAction),
    /// A summary action.
    Summary(SummaryAction),
}

impl LoggedAction {
    /// Formats the action using its concrete action implementation.
    fn formatted_action(&self) -> String {
        match self {
            Self::Betting(action) => action.to_string(),
            Self::Dealing(action) => action.to_string(),
            Self::Board(action) => action.to_string(),
            Self::Seating(action) => action.to_string(),
            Self::Showdown(action) => action.to_string(),
            Self::Pot(action) => action.to_string(),
            Self::Summary(action) => action.to_string(),
        }
    }
}

impl fmt::Display for LoggedAction {
    /// Formats the action using its concrete action implementation.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.formatted_action())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the two spaces retained from C++ `BettingAction::toString`.
    #[test]
    fn betting_action_keeps_source_hand_history_spacing() {
        let player = Player::new(Stack::default(), &crate::decision_engine::DecisionEngine, 3);
        let action = BettingAction::new(&player, Street::Flop);
        assert_eq!(action.to_string(), "Player 3:  checks.");
    }

    /// Verifies call amounts use the source's `toCall - previousBet` formula.
    #[test]
    fn call_string_uses_increment_from_previous_bet() {
        let player = Player::new(Stack::default(), &crate::decision_engine::DecisionEngine, 3);
        let mut action = BettingAction::new(&player, Street::Turn);
        action.decision = Decision::Call;
        action.previous_bet = Stack(7);
        action.to_call = Stack(20);
        assert_eq!(action.to_string(), "Player 3:  calls 13.");
    }
}
