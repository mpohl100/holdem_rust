//! Betting decision selection translated from `DecisionEngine.h/.cpp`.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::action::BettingAction;
use crate::board::Board;
use crate::dealer::{Dealer, DealerError};
use crate::player::{Decision, Player};
use crate::stack::Stack;

/// Stateless policy for selecting one of the dealer's legal betting options.
#[derive(Clone, Debug, Default)]
pub struct DecisionEngine;

impl DecisionEngine {
    /// Selects a legal decision and random bet size for `hero`.
    ///
    /// C++ uses `rand() % option_count` and an independent `rand() % 100`.
    /// Rust's standard library has no random-number API; this implementation
    /// uses a deterministic, process-wide xorshift generator with a fixed seed.
    /// It is dependency-free but is not sequence-compatible with C `rand()`.
    ///
    /// This maps C++ `DecisionEngine::decide`.
    ///
    /// # Errors
    ///
    /// Returns an error when the dealer rejects the player ID or no action is
    /// available.
    ///
    /// # Panics
    ///
    /// Panics only if an allocated action list cannot fit in `u64`/`usize`;
    /// the percentage conversion is bounded by the fixed `0..100` draw.
    pub fn decide(
        &self,
        dealer: &Dealer,
        board: &Board,
        hero: &Player,
        players: &[Player],
    ) -> Result<BettingAction, DecisionEngineError> {
        let options = dealer
            .get_options(hero.id(), players)
            .map_err(DecisionEngineError::Dealer)?;
        if options.options.is_empty() {
            return Err(DecisionEngineError::NoActions);
        }

        let option_count = u64::try_from(options.options.len())
            .expect("an allocated option vector length fits in u64");
        let choice = usize::try_from(random_below(option_count))
            .expect("a random index below the option count fits in usize");
        let (decision, (minimum, maximum)) = options.options[choice];
        let percentage = i32::try_from(random_below(100)).expect("percentage is below 100");
        let next_bet = minimum + (maximum - minimum) * percentage / 100;
        let previous_bet = dealer.get_current_bet(hero.id());
        let to_call = get_to_call(&options);
        let mut action = BettingAction::new(hero, board.street());
        action.next_bet = next_bet;
        action.previous_bet = previous_bet;
        action.to_call = to_call;
        action.decision = decision;
        Ok(action)
    }
}

/// Gets the amount to call from an option list.
///
/// This maps C++ free function `getToCall` to `get_to_call`.
pub fn get_to_call(options: &crate::dealer::Options) -> Stack {
    options
        .options
        .iter()
        .find(|(decision, _)| *decision == Decision::Call)
        .map_or_else(Stack::default, |(_, range)| range.0)
}

/// Process-wide state for the dependency-free decision-selection generator.
static RANDOM_STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// Returns a pseudo-random integer less than `upper_bound`.
///
/// The loop retries only when another thread updates the generator state
/// between this function's load and compare-exchange operations.
fn random_below(upper_bound: u64) -> u64 {
    let mut previous = RANDOM_STATE.load(Ordering::Relaxed);
    loop {
        let mut next = previous;
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        match RANDOM_STATE.compare_exchange_weak(
            previous,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return next % upper_bound,
            Err(actual) => previous = actual,
        }
    }
}

/// Failure when selecting a betting action.
#[derive(Debug)]
pub enum DecisionEngineError {
    /// The player had no legal decision options.
    NoActions,
    /// Dealer failed while computing legal options.
    Dealer(DealerError),
}

impl std::fmt::Display for DecisionEngineError {
    /// Formats the failure reason.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoActions => formatter.write_str(
                "player is asked to decide, although there are no more actions necessary from him",
            ),
            Self::Dealer(error) => std::fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for DecisionEngineError {}

/// Exercises the C++-mapped `getToCall` behavior.
#[cfg(test)]
mod tests {
    use super::*;

    /// Selects the call amount from the first call option.
    #[test]
    fn call_amount_is_taken_from_call_option() {
        let options = crate::dealer::Options {
            options: vec![
                (Decision::Fold, (Stack::default(), Stack::default())),
                (Decision::Call, (Stack(20), Stack(20))),
            ],
        };
        assert_eq!(get_to_call(&options), Stack(20));
    }

    /// Uses zero when the options contain no call action.
    #[test]
    fn call_amount_is_zero_when_no_call_exists() {
        let options = crate::dealer::Options {
            options: vec![(Decision::Check, (Stack::default(), Stack::default()))],
        };
        assert_eq!(get_to_call(&options), Stack::default());
    }
}
