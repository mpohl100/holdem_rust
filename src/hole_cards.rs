//! Two-card holdings translated from `HoleCards.h` and `HoleCards.cpp`.

use crate::card52::Card52;
use std::fmt;

/// A player's private hole cards.
///
/// C++ mapping: `game52::HoleCards`; its private `cards_` member maps to the
/// private Rust `cards_` vector. The default value represents the source's
/// empty default-constructed vector.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct HoleCards {
    /// C++ private member `cards_`, with two cards for a populated holding.
    cards_: Vec<Card52>,
}

impl HoleCards {
    /// Creates a holding with the two supplied cards in source order.
    ///
    /// C++ mapping: `HoleCards(Card52, Card52)`.
    #[must_use]
    pub fn new(first: Card52, second: Card52) -> Self {
        Self {
            cards_: vec![first, second],
        }
    }

    /// Returns the original source spelling, with a space between the cards.
    ///
    /// C++ mapping: `HoleCards::toString()`.
    fn formatted_cards(&self) -> String {
        self.cards_
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Returns whether this holding contains no cards.
    ///
    /// C++ mapping: `HoleCards::empty()`.
    #[must_use]
    pub fn empty(&self) -> bool {
        self.cards_.is_empty()
    }

    /// Borrows the cards in their original order.
    ///
    /// C++ mapping: `HoleCards::getCards()`.
    #[must_use]
    pub fn get_cards(&self) -> &[Card52] {
        &self.cards_
    }

    /// Generates all ordered pairs of distinct standard-deck cards.
    ///
    /// C++ mapping: `HoleCards::getAll()`. As in the source, both orders of
    /// every pair are included, producing 52 × 51 holdings.
    #[must_use]
    pub fn get_all() -> Vec<Self> {
        let cards = Card52::get_all();
        let mut holdings = Vec::with_capacity(cards.len() * (cards.len() - 1));
        for &first in &cards {
            for &second in &cards {
                if first != second {
                    holdings.push(Self::new(first, second));
                }
            }
        }
        holdings
    }
}

impl fmt::Display for HoleCards {
    /// Formats the holding with one space between the cards.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.formatted_cards())
    }
}

#[cfg(test)]
mod tests {
    use super::HoleCards;

    /// Checks the complete ordered-pair count and each holding's size.
    #[test]
    fn all_ordered_holdings_are_distinct() {
        let holdings = HoleCards::get_all();
        assert_eq!(holdings.len(), 52 * 51);
        assert!(
            holdings
                .iter()
                .all(|holding| holding.get_cards().len() == 2)
        );
        assert!(
            holdings
                .iter()
                .all(|holding| holding.get_cards()[0] != holding.get_cards()[1])
        );
    }

    /// Checks the empty default state and source string format.
    #[test]
    fn holding_format_and_empty_state() {
        assert!(HoleCards::default().empty());
        let cards = Card52::get_all();
        assert_eq!(HoleCards::new(cards[0], cards[1]).to_string(), "2h 3h");
    }

    use crate::card52::Card52;
}
