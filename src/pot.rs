//! Pot accounting translated from `Pot.h` and `Pot.cpp`.

use std::collections::BTreeMap;

use crate::player::PlayerId;
use crate::stack::Stack;

/// A pot and the contributions associated with stable player IDs.
///
/// The C++ map keyed by `const Player*` is translated to a map keyed by
/// `PlayerId`; this avoids unsafe or address-based identity.
#[derive(Clone, Debug, Default)]
pub struct Pot {
    /// Contributions by player; maps to C++ `bets_`.
    bets_: BTreeMap<PlayerId, Stack>,
    /// Total chips in the pot; maps to C++ `amount_`.
    amount_: Stack,
}

impl Pot {
    /// Adds a non-zero contribution from `player`.
    ///
    /// This maps C++ `Pot::putAmount`.
    pub fn put_amount(&mut self, player: PlayerId, amount: Stack) {
        if amount == Stack::default() {
            return;
        }
        self.amount_ += amount;
        let contribution = self.bets_.entry(player).or_default();
        *contribution += amount;
    }

    /// Returns the total pot size in chips.
    ///
    /// This maps C++ `Pot::getAmount`.
    /// This preserves the C++ `getAmount()` integer return type and therefore
    /// assumes `Stack` is a tuple newtype; C++ `Stack::amount_` maps to `.0`.
    #[must_use]
    pub fn get_amount(&self) -> i32 {
        self.amount_.0
    }

    /// Takes and clears the pot's total amount.
    ///
    /// The contribution map is deliberately retained, matching C++ `Pot::get`.
    pub fn get(&mut self) -> Stack {
        let amount = self.amount_;
        self.amount_ = Stack::default();
        amount
    }

    /// Formats this pot for hand-history output.
    ///
    /// This maps C++ `Pot::toString`.
    fn formatted_pot() -> String {
        "main pot".to_owned()
    }

    /// Returns all player contributions.
    ///
    /// This maps C++ `Pot::getBets`.
    #[must_use]
    pub fn get_bets(&self) -> &BTreeMap<PlayerId, Stack> {
        &self.bets_
    }
}

impl std::fmt::Display for Pot {
    /// Formats the pot using the source's fixed text.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&Self::formatted_pot())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ignores zero-valued contributions while summing positive bets.
    #[test]
    fn ignores_zero_and_tracks_contributions() {
        let mut pot = Pot::default();
        pot.put_amount(4, Stack::default());
        pot.put_amount(4, Stack(12));
        pot.put_amount(9, Stack(7));

        assert_eq!(pot.get_amount(), 19);
        assert_eq!(pot.get_bets().get(&4), Some(&Stack(12)));
        assert_eq!(pot.get_bets().get(&9), Some(&Stack(7)));
    }

    /// Retains bet ownership data after taking the total pot amount.
    #[test]
    fn taking_amount_clears_total_but_keeps_bets() {
        let mut pot = Pot::default();
        pot.put_amount(2, Stack(31));

        assert_eq!(pot.get(), Stack(31));
        assert_eq!(pot.get(), Stack::default());
        assert_eq!(pot.get_bets().get(&2), Some(&Stack(31)));
    }
}
