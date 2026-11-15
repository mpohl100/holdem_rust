//! Deck management translated from `Deck52.h` and `Deck52.cpp`.

use crate::card52::Card52;
use crate::hole_cards::HoleCards;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash, Hasher};

/// Error returned when a safe deck operation cannot be performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeckError {
    /// The deck does not contain enough remaining cards for the requested deal.
    Exhausted,
    /// A requested move-to-front card is not present in the deck.
    CardNotFound,
    /// A requested prefix extends beyond the 52-card deck.
    RangeOutOfBounds(usize),
}

impl std::fmt::Display for DeckError {
    /// Formats a deck-operation error.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exhausted => formatter.write_str("no cards remain in the deck"),
            Self::CardNotFound => formatter.write_str("card is not present in the deck"),
            Self::RangeOutOfBounds(length) => {
                write!(formatter, "deck prefix length {length} exceeds 52")
            }
        }
    }
}

impl std::error::Error for DeckError {}

/// A standard 52-card deck and the index of its next card.
///
/// C++ mapping: `game52::Deck52`; private `cards_` and `position_` map to
/// private Rust fields. `position_` is represented as `usize` to maintain the
/// vector indexing invariant.
#[derive(Clone, Debug)]
pub struct Deck52 {
    /// C++ private member `cards_`, containing all 52 cards.
    cards_: Vec<Card52>,
    /// C++ private member `position_`, the next position to deal.
    position_: usize,
}

impl Default for Deck52 {
    /// Constructs the deck in C++ numeric-index order.
    fn default() -> Self {
        Self {
            cards_: Card52::get_all(),
            position_: 0,
        }
    }
}

impl Deck52 {
    /// Constructs a deck in C++ numeric-index order.
    ///
    /// C++ mapping: `Deck52()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Randomizes all cards and resets the next-card index.
    ///
    /// C++ mapping: `Deck52::shuffle()`. The standard library's per-instance
    /// randomized hash seed supplies a dependency-free process-random ordering.
    pub fn shuffle(&mut self) {
        let random = RandomState::new();
        shuffle_by_random_key(&mut self.cards_, &random);
        self.position_ = 0;
    }

    /// Returns every card in current deck order, followed by a space.
    ///
    /// C++ mapping: `Deck52::toString()`.
    fn formatted_cards(&self) -> String {
        let mut result = String::new();
        for card in &self.cards_ {
            result.push_str(&card.to_string());
            result.push(' ');
        }
        result
    }

    /// Deals the next two cards as a hole-card holding.
    ///
    /// C++ mapping: `Deck52::getHoleCards()`. Exhaustion is reported as an
    /// error rather than indexing outside the C++ vector.
    ///
    /// # Errors
    ///
    /// Returns [`DeckError::Exhausted`] unless two cards remain to deal.
    pub fn get_hole_cards(&mut self) -> Result<HoleCards, DeckError> {
        let first = self.deal_card()?;
        let second = self.deal_card()?;
        Ok(HoleCards::new(first, second))
    }

    /// Burns one card and deals the next three cards as the flop.
    ///
    /// C++ mapping: `Deck52::getFlop()`.
    ///
    /// # Errors
    ///
    /// Returns [`DeckError::Exhausted`] unless the burn card and all three
    /// flop cards are available.
    pub fn get_flop(&mut self) -> Result<Vec<Card52>, DeckError> {
        self.deal_card()?;
        (0..3).map(|_| self.deal_card()).collect()
    }

    /// Burns one card and deals the next card.
    ///
    /// C++ mapping: `Deck52::getCard()`.
    ///
    /// # Errors
    ///
    /// Returns [`DeckError::Exhausted`] unless both the burn and dealt card
    /// are available.
    pub fn get_card(&mut self) -> Result<Card52, DeckError> {
        self.deal_card()?;
        self.deal_card()
    }

    /// Returns the cards from the next-card position through the deck's end.
    ///
    /// C++ mapping: `Deck52::getRemainingCards()`.
    #[must_use]
    pub fn get_remaining_cards(&self) -> &[Card52] {
        &self.cards_[self.position_..]
    }

    /// Moves each requested card to the front and advances the deal position.
    ///
    /// C++ mapping: `Deck52::moveToFront()`. Cards are processed in the given
    /// order, using the same sequential swaps as the C++ implementation.
    ///
    /// # Errors
    ///
    /// Returns [`DeckError::CardNotFound`] if any requested card is absent
    /// from the deck.
    pub fn move_to_front(&mut self, cards: &[Card52]) -> Result<(), DeckError> {
        for (index, card) in cards.iter().enumerate() {
            let found = self
                .cards_
                .iter()
                .position(|candidate| candidate == card)
                .ok_or(DeckError::CardNotFound)?;
            self.cards_.swap(index, found);
        }
        self.position_ = self.position_.saturating_add(cards.len());
        Ok(())
    }

    /// Randomizes only the cards not yet dealt.
    ///
    /// C++ mapping: `Deck52::shuffleRemainder()`.
    pub fn shuffle_remainder(&mut self) {
        let random = RandomState::new();
        shuffle_by_random_key(&mut self.cards_[self.position_..], &random);
    }

    /// Returns a clone of the first `length` cards, irrespective of position.
    ///
    /// C++ mapping: `Deck52::getRange(size_t)`. The C++ return type is a pair
    /// of iterators; an owned prefix avoids leaking vector iterator lifetimes.
    ///
    /// # Errors
    ///
    /// Returns [`DeckError::RangeOutOfBounds`] if `length` exceeds the deck.
    pub fn get_range(&self, length: usize) -> Result<Vec<Card52>, DeckError> {
        self.cards_
            .get(..length)
            .map(<[Card52]>::to_vec)
            .ok_or(DeckError::RangeOutOfBounds(length))
    }

    /// Deals one card without the burn behavior of the public street helpers.
    ///
    /// C++ mapping: private `Deck52::dealCard()`.
    ///
    /// # Errors
    ///
    /// Returns [`DeckError::Exhausted`] when no undealt card remains.
    fn deal_card(&mut self) -> Result<Card52, DeckError> {
        let card = self
            .cards_
            .get(self.position_)
            .copied()
            .ok_or(DeckError::Exhausted)?;
        self.position_ += 1;
        Ok(card)
    }
}

/// Sorts the slice by independently hashed positions to randomize its order.
fn shuffle_by_random_key(cards: &mut [Card52], random: &RandomState) {
    let mut keys = cards
        .iter()
        .enumerate()
        .map(|(index, card)| {
            let mut hasher = random.build_hasher();
            index.hash(&mut hasher);
            card.hash(&mut hasher);
            hasher.finish()
        })
        .collect::<Vec<_>>();
    let mut indexed = cards
        .iter()
        .copied()
        .zip(keys.drain(..))
        .collect::<Vec<_>>();
    indexed.sort_by_key(|(_, key)| *key);
    for (card, (selected, _)) in cards.iter_mut().zip(indexed) {
        *card = selected;
    }
}

impl std::fmt::Display for Deck52 {
    /// Formats the deck as its cards, each followed by a space.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.formatted_cards())
    }
}

#[cfg(test)]
mod tests {
    use super::{Deck52, DeckError};

    /// Checks the deck's initial order and burn-card positions.
    #[test]
    fn deck_order_and_burn_dealing() {
        let mut deck = Deck52::new();
        let hole_cards = deck.get_hole_cards().unwrap();
        assert_eq!(hole_cards.to_string(), "2h 3h");
        let flop = deck.get_flop().unwrap();
        assert_eq!(
            flop.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ["5h", "6h", "7h"]
        );
        assert_eq!(deck.get_card().unwrap().to_string(), "9h");
        assert_eq!(deck.get_remaining_cards().len(), 44);
    }

    /// Checks range bounds and deck exhaustion without invalid indexing.
    #[test]
    fn deck_errors_are_explicit() {
        let mut deck = Deck52::new();
        assert_eq!(deck.get_range(53), Err(DeckError::RangeOutOfBounds(53)));
        for _ in 0..26 {
            deck.get_hole_cards().unwrap();
        }
        assert_eq!(deck.get_card(), Err(DeckError::Exhausted));
    }

    /// Checks move-to-front and remainder-shuffle position behavior.
    #[test]
    fn deck_front_and_remainder() {
        let mut deck = Deck52::new();
        let cards = deck.get_range(2).unwrap();
        deck.move_to_front(&cards).unwrap();
        assert_eq!(deck.get_remaining_cards().len(), 50);
        assert_eq!(deck.get_remaining_cards()[0].to_string(), "4h");
        deck.shuffle_remainder();
        assert_eq!(deck.get_remaining_cards().len(), 50);
    }
}
