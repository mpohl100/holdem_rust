//! Hand-history storage and formatting translated from `HandHistory.h/.cpp`.

use crate::action::{BettingAction, LoggedAction};
use crate::board::Street;
use crate::player::PlayerId;

/// Ordered log of actions in one hand.
#[derive(Debug, Default)]
pub struct HandHistory {
    /// Actions in insertion order; maps to C++ `actions_`.
    actions_: Vec<LoggedAction>,
}

impl HandHistory {
    /// Appends one owned action to the history.
    ///
    /// The C++ `std::unique_ptr<HandAction>` ownership is represented by an
    /// owned `LoggedAction` enum.
    /// This maps to C++ `HandHistory::logAction`.
    pub fn log_action(&mut self, action: LoggedAction) {
        self.actions_.push(action);
    }

    /// Returns the most recently logged bet for `street` and optional player.
    ///
    /// `player` is a stable ID replacing the C++ `Player const*`; C++ matched
    /// its `getNumber()` value, and this migration uses the corresponding
    /// stable ID assigned when the player was created.
    /// This maps to C++ `HandHistory::getLastBet`.
    #[must_use]
    pub fn get_last_bet(&self, street: Street, player: Option<PlayerId>) -> Option<BettingAction> {
        self.actions_.iter().rev().find_map(|action| match action {
            LoggedAction::Betting(bet)
                if bet.street == street
                    && player.is_none_or(|player_id| bet.player.id() == player_id) =>
            {
                Some(bet.clone())
            }
            _ => None,
        })
    }

    /// Formats every action followed by a newline.
    ///
    /// This maps to C++ `HandHistory::toString`.
    fn formatted_history(&self) -> String {
        let mut output = String::new();
        for action in &self.actions_ {
            output.push_str(&action.to_string());
            output.push('\n');
        }
        output
    }
}

impl std::fmt::Display for HandHistory {
    /// Formats every logged action followed by a newline.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.formatted_history())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::BettingAction;
    use crate::decision_engine::DecisionEngine;
    use crate::player::{Decision, Player};
    use crate::stack::Stack;

    /// Confirms reverse search finds the newest bet matching both street and ID.
    #[test]
    /// Finds the newest bet matching the requested street and player.
    fn last_bet_filters_by_street_and_stable_player_id() {
        let mut first_player = Player::new(Stack(100), &DecisionEngine, 1);
        first_player.set_player_id(10);
        let mut second_player = Player::new(Stack(100), &DecisionEngine, 2);
        second_player.set_player_id(20);
        let mut history = HandHistory::default();
        let mut first_bet = BettingAction::new(&first_player, Street::Flop);
        first_bet.next_bet = Stack(30);
        first_bet.decision = Decision::Raise;
        history.log_action(LoggedAction::Betting(first_bet));
        let mut second_bet = BettingAction::new(&second_player, Street::Flop);
        second_bet.next_bet = Stack(50);
        second_bet.decision = Decision::Call;
        history.log_action(LoggedAction::Betting(second_bet));

        assert_eq!(
            history
                .get_last_bet(Street::Flop, Some(10))
                .map(|bet| bet.next_bet),
            Some(Stack(30))
        );
        assert!(history.get_last_bet(Street::Turn, Some(10)).is_none());
    }

    /// Confirms action formatting always appends one newline per log entry.
    #[test]
    /// Retains the source's trailing newline for each action.
    fn history_output_ends_each_action_with_a_newline() {
        let player = Player::new(Stack::default(), &DecisionEngine, 1);
        let mut history = HandHistory::default();
        history.log_action(LoggedAction::Betting(BettingAction::new(
            &player,
            Street::Preflop,
        )));

        assert_eq!(history.to_string(), "Player 1:  checks.\n");
    }
}
