//! Community-board representation translated from `Board.h` and `Board.cpp`.

use crate::card52::Card52;
use crate::hand::{Hand, try_get_best_hand};
use crate::hole_cards::HoleCards;
use std::cmp::Ordering;

/// The stage of a poker hand, in C++ `Street` declaration order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Street {
    /// No community cards have been dealt.
    Preflop,
    /// Three community cards have been dealt.
    Flop,
    /// Four community cards have been dealt.
    Turn,
    /// Five community cards have been dealt.
    River,
}

/// Returns the source phrase for a community-card street.
///
/// C++ mapping: free function `toString(Street)`.
#[must_use]
pub fn street_to_string(street: Street) -> &'static str {
    match street {
        Street::Preflop => "*** HOLE CARDS ***",
        Street::Flop => "*** FLOP ***",
        Street::Turn => "*** TURN ***",
        Street::River => "*** RIVER ***",
    }
}

/// Community cards currently on the table.
///
/// C++ mapping: `game52::Board`; private member `cards_` remains private.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Board {
    /// C++ private member `cards_`, in deal order.
    cards_: Vec<Card52>,
}

impl Board {
    /// Returns the street inferred from the number of community cards.
    ///
    /// C++ mapping: `Board::street()`. Unsupported card counts fall back to
    /// `Preflop`, as the source implementation does.
    #[must_use]
    pub fn street(&self) -> Street {
        match self.cards_.len() {
            3 => Street::Flop,
            4 => Street::Turn,
            5 => Street::River,
            _ => Street::Preflop,
        }
    }

    /// Replaces the board with the supplied flop cards.
    ///
    /// C++ mapping: `Board::dealFlop()`.
    pub fn deal_flop(&mut self, flop: Vec<Card52>) {
        self.cards_ = flop;
    }

    /// Adds the turn card to the board.
    ///
    /// C++ mapping: `Board::dealTurn()`.
    pub fn deal_turn(&mut self, turn: Card52) {
        self.cards_.push(turn);
    }

    /// Adds the river card to the board.
    ///
    /// C++ mapping: `Board::dealRiver()`.
    pub fn deal_river(&mut self, river: Card52) {
        self.cards_.push(river);
    }

    /// Borrows the community cards in deal order.
    ///
    /// C++ mapping: `Board::getCards()`.
    #[must_use]
    pub fn get_cards(&self) -> &[Card52] {
        &self.cards_
    }

    /// Returns all ordered distinct holdings ranked by their best made hand.
    ///
    /// C++ mapping: `Board::getBestHoldings()`. Equal-ranked holdings are
    /// returned in card-enumeration order; the C++ sort did not define their
    /// relative order.
    #[must_use]
    pub fn get_best_holdings(&self) -> Vec<HoleCards> {
        let mut ranked = Vec::with_capacity(52 * 51);
        let cards = Card52::get_all();
        for &first in &cards {
            for &second in &cards {
                if first == second {
                    continue;
                }
                let holding = HoleCards::new(first, second);
                let hand = Hand::new(holding.clone(), self.clone());
                if let Ok(rank) = try_get_best_hand(&hand) {
                    ranked.push((holding, rank));
                }
            }
        }
        ranked.sort_by(|left, right| match right.1.cmp(&left.1) {
            Ordering::Equal => Ordering::Equal,
            order => order,
        });
        ranked.into_iter().map(|(holding, _)| holding).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Board, Street, street_to_string};
    use crate::card52::Card52;

    /// Checks board street transitions and the invalid-count source fallback.
    #[test]
    fn street_transitions_and_fallback() {
        let mut board = Board::default();
        assert_eq!(board.street(), Street::Preflop);
        board.deal_flop(Card52::get_all()[..3].to_vec());
        assert_eq!(board.street(), Street::Flop);
        board.deal_turn(Card52::get_all()[3]);
        assert_eq!(board.street(), Street::Turn);
        board.deal_river(Card52::get_all()[4]);
        assert_eq!(board.street(), Street::River);
        assert_eq!(street_to_string(Street::River), "*** RIVER ***");
    }
}
