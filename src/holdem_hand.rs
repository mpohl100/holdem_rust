//! Texas Hold'em hand classification and comparison.
//!
//! This module maps `cpp_code/src/HoldemHand.h` and `HoldemHand.cpp`.
//! `HoldemHand52` retains the source class name, public methods use
//! `snake_case`, and the source nested `HoldemHand52::HandRank52` enum maps to
//! [`HoldemHandRank52`]. The misspelling in `findOccurences` is retained in
//! [`HoldemHand52::find_occurences`] to keep the declaration mapping exact.

use crate::board::Board;
use crate::card52::{Card52, CardParseError, Rank52, Suit};
use crate::hole_cards::HoleCards;
use std::cmp::Ordering;
use std::str::FromStr;

/// C++ nested enum `HoldemHand52::HandRank52`, in declaration order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(i32)]
pub enum HoldemHandRank52 {
    /// No pair or stronger made hand.
    HighCard = 0,
    /// One pair.
    Pair = 1,
    /// Two distinct pairs.
    TwoPair = 2,
    /// Three cards of one rank.
    Trips = 3,
    /// Five consecutive ranks.
    Straight = 4,
    /// Five cards of one suit.
    Flush = 5,
    /// Three of a rank and a pair.
    FullHouse = 6,
    /// Four cards of one rank.
    Quads = 7,
    /// A straight and a flush.
    StraightFlush = 8,
}

/// Error returned while parsing a `HoldemHand52` from text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HoldemHandError {
    /// A card in the source space-separated string could not be parsed.
    InvalidCard(CardParseError),
}

impl std::fmt::Display for HoldemHandError {
    /// Formats the card parsing error.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCard(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for HoldemHandError {}

/// A classified Texas Hold'em holding, possibly including board cards.
///
/// C++ mapping: `game52::HoldemHand52`. The source private members
/// `cards_`, `handRank_`, `rankOccurences_`, and `suitOccurences_` map to
/// these private Rust fields. The source's default constructor leaves
/// `handRank_` indeterminate; Rust uses `HighCard` as a safe placeholder.
#[derive(Clone, Debug)]
pub struct HoldemHand52 {
    /// Sorted hole and board cards used to classify and compare the holding.
    cards_: Vec<Card52>,
    /// Classified made-hand rank.
    hand_rank_: HoldemHandRank52,
    /// Rank counts, including the extra ace-at-low index used for wheels.
    rank_occurences_: Vec<u8>,
    /// Counts for suits in C++ `Suit` declaration order.
    suit_occurences_: Vec<u8>,
}

impl Default for HoldemHand52 {
    /// Creates the safe counterpart of the source default constructor.
    fn default() -> Self {
        Self {
            cards_: Vec::new(),
            hand_rank_: HoldemHandRank52::HighCard,
            rank_occurences_: Vec::new(),
            suit_occurences_: Vec::new(),
        }
    }
}

impl HoldemHand52 {
    /// Classifies a player's hole cards together with the board.
    ///
    /// C++ mapping: `HoldemHand52(HoleCards const&, Board const&)`.
    #[must_use]
    pub fn from_hole_cards(hole_cards: &HoleCards, board: &Board) -> Self {
        let mut cards = hole_cards.get_cards().to_vec();
        cards.extend_from_slice(board.get_cards());
        Self::from_sorted_input(cards)
    }

    /// Classifies the supplied card range.
    ///
    /// C++ mapping: the iterator-range constructor
    /// `HoldemHand52(begin, end)`.
    #[must_use]
    pub fn from_cards(cards: &[Card52]) -> Self {
        Self::from_sorted_input(cards.to_vec())
    }

    fn from_sorted_input(mut cards: Vec<Card52>) -> Self {
        cards.sort_unstable();
        let mut hand = Self {
            cards_: cards,
            hand_rank_: HoldemHandRank52::HighCard,
            rank_occurences_: vec![0; usize::from(Rank52::Ace.value()) + 2],
            suit_occurences_: vec![0; 4],
        };
        hand.classify_hand();
        hand
    }

    /// Returns the five cards constituting the classified hand.
    ///
    /// C++ mapping: `HoldemHand52::getCards()`.
    #[must_use]
    pub fn get_cards(&self) -> Vec<Card52> {
        let find_cards_with_ranks = |ranks: &[Rank52]| {
            let mut result = Vec::with_capacity(ranks.len());
            for rank in ranks {
                if let Some(card) = self.cards_.iter().find(|card| card.rank() == *rank) {
                    result.push(*card);
                }
            }
            result
        };

        match self.hand_rank_ {
            HoldemHandRank52::HighCard => find_cards_with_ranks(&self.get_high_cards()),
            HoldemHandRank52::Pair
            | HoldemHandRank52::TwoPair
            | HoldemHandRank52::Trips
            | HoldemHandRank52::FullHouse
            | HoldemHandRank52::Quads => {
                let mut cards = self.get_cards_mattering();
                cards.extend(find_cards_with_ranks(&self.get_high_cards()));
                cards
            }
            HoldemHandRank52::Straight
            | HoldemHandRank52::Flush
            | HoldemHandRank52::StraightFlush => self.get_cards_mattering(),
        }
    }

    /// Returns the classified category.
    ///
    /// C++ mapping: `HoldemHand52::getClassifiedPokerHand()`.
    #[must_use]
    pub const fn get_classified_poker_hand(&self) -> HoldemHandRank52 {
        self.hand_rank_
    }

    /// Finds up to the two highest ranks occurring exactly `count` times.
    ///
    /// C++ mapping: `HoldemHand52::findOccurences()`. C++ leaves array entries
    /// indeterminate when fewer than two ranks match; Rust duplicates the only
    /// match when exactly one rank matches and reports no matches by panicking.
    ///
    /// # Panics
    ///
    /// Panics if no rank has the requested multiplicity.
    #[must_use]
    pub fn find_occurences(&self, count: i32) -> [Rank52; 2] {
        let mut result = Vec::with_capacity(2);
        for index in (0..self.rank_occurences_.len()).rev() {
            if i32::from(self.rank_occurences_[index]) == count
                && let Some(rank) = rank_from_occurrence_index_option(index)
            {
                result.push(rank);
                if result.len() == 2 {
                    break;
                }
            }
        }
        if result.len() == 1 {
            // In the source the extra low-ace slot can duplicate an ace rank;
            // its second array element is not read for single-pair/trips/etc.
            result.push(result[0]);
        }
        result
            .try_into()
            .unwrap_or_else(|_| panic!("HoldemHand52::findOccurences requires two matching ranks"))
    }

    /// Returns the ranks used to determine the made hand.
    ///
    /// C++ mapping: `HoldemHand52::getRanksMattering()`.
    #[must_use]
    pub fn get_ranks_mattering(&self) -> Vec<Rank52> {
        match self.hand_rank_ {
            HoldemHandRank52::HighCard | HoldemHandRank52::Flush => self.get_high_cards(),
            HoldemHandRank52::Pair => vec![self.find_occurences(2)[0]],
            HoldemHandRank52::TwoPair => {
                let first_rank = self.find_occurences(2)[0];
                vec![
                    first_rank,
                    find_occurences_at_least(&reset_rank(&self.rank_occurences_, first_rank), 2)[0],
                ]
            }
            HoldemHandRank52::Trips => vec![self.find_occurences(3)[0]],
            HoldemHandRank52::Straight | HoldemHandRank52::StraightFlush => vec![
                self.get_straight_rank(&self.rank_occurences_)
                    .expect("a classified straight has a straight rank"),
            ],
            HoldemHandRank52::FullHouse => {
                let trips_rank = self.find_occurences(3)[0];
                vec![
                    trips_rank,
                    find_occurences_at_least(&reset_rank(&self.rank_occurences_, trips_rank), 2)[0],
                ]
            }
            HoldemHandRank52::Quads => vec![self.find_occurences(4)[0]],
        }
    }

    /// Returns the cards used by the made hand, excluding kickers.
    ///
    /// C++ mapping: `HoldemHand52::getCardsMattering()`.
    #[must_use]
    pub fn get_cards_mattering(&self) -> Vec<Card52> {
        let find_cards_by_rank = |ranks: &[Rank52]| {
            let mut result = Vec::new();
            for rank in ranks {
                for card in &self.cards_ {
                    if card.rank() == *rank {
                        result.push(*card);
                    }
                }
            }
            result
        };

        match self.hand_rank_ {
            HoldemHandRank52::HighCard
            | HoldemHandRank52::Pair
            | HoldemHandRank52::TwoPair
            | HoldemHandRank52::Trips
            | HoldemHandRank52::FullHouse
            | HoldemHandRank52::Quads => find_cards_by_rank(&self.get_ranks_mattering()),
            HoldemHandRank52::Straight => {
                let highest_rank = self
                    .get_straight_rank(&self.rank_occurences_)
                    .expect("a classified straight has a straight rank");
                get_straight_ranks(highest_rank)
                    .into_iter()
                    .filter_map(|rank| self.cards_.iter().find(|card| card.rank() == rank))
                    .copied()
                    .collect()
            }
            HoldemHandRank52::Flush => {
                let flush_suit = self
                    .get_flush_suit(&self.suit_occurences_)
                    .expect("a classified flush has a flush suit");
                self.get_high_cards()
                    .into_iter()
                    .map(|rank| Card52::new(rank, flush_suit))
                    .collect()
            }
            HoldemHandRank52::StraightFlush => {
                let highest_rank = *self
                    .get_high_cards()
                    .first()
                    .expect("a classified straight flush has a high card");
                let flush_suit = self
                    .get_flush_suit(&self.suit_occurences_)
                    .expect("a classified straight flush has a flush suit");
                get_straight_ranks(highest_rank)
                    .into_iter()
                    .map(|rank| Card52::new(rank, flush_suit))
                    .collect()
            }
        }
    }

    /// Returns the category's high-card ranks and kickers.
    ///
    /// C++ mapping: `HoldemHand52::getHighCards()`.
    #[must_use]
    pub fn get_high_cards(&self) -> Vec<Rank52> {
        match self.hand_rank_ {
            HoldemHandRank52::HighCard => self
                .cards_
                .iter()
                .rev()
                .take(5)
                .map(|card| card.rank())
                .collect(),
            HoldemHandRank52::Pair => {
                get_first_n_high_cards(&self.rank_occurences_, 3, &self.cards_)
            }
            HoldemHandRank52::TwoPair => {
                let mut occurrences = self.rank_occurences_.clone();
                for rank in self.get_ranks_mattering() {
                    occurrences = reset_rank(&occurrences, rank);
                }
                get_first_n_high_cards(&occurrences, 1, &self.cards_)
            }
            HoldemHandRank52::Trips => {
                get_first_n_high_cards(&self.rank_occurences_, 2, &self.cards_)
            }
            HoldemHandRank52::Straight => vec![
                self.get_straight_rank(&self.rank_occurences_)
                    .expect("a classified straight has a straight rank"),
            ],
            HoldemHandRank52::Flush => {
                let flush_suit = self
                    .get_flush_suit(&self.suit_occurences_)
                    .expect("a classified flush has a flush suit");
                let mut ranks = self
                    .cards_
                    .iter()
                    .filter(|card| card.suit() == flush_suit)
                    .take(5)
                    .map(|card| card.rank())
                    .collect::<Vec<_>>();
                ranks.sort_unstable_by(|left, right| right.cmp(left));
                ranks
            }
            HoldemHandRank52::FullHouse => Vec::new(),
            HoldemHandRank52::Quads => {
                let mut occurrences = self.rank_occurences_.clone();
                occurrences = reset_rank(&occurrences, self.get_ranks_mattering()[0]);
                get_first_n_high_cards(&occurrences, 1, &self.cards_)
            }
            HoldemHandRank52::StraightFlush => {
                let flush_suit = self
                    .get_flush_suit(&self.suit_occurences_)
                    .expect("a classified straight flush has a flush suit");
                for highest_rank in get_straight_ranks_from_occurrences(&self.rank_occurences_) {
                    let is_flush = get_straight_ranks(highest_rank)
                        .iter()
                        .all(|rank| self.cards_.contains(&Card52::new(*rank, flush_suit)));
                    if is_flush {
                        return vec![highest_rank];
                    }
                }
                Vec::new()
            }
        }
    }

    /// Sums the ranks returned by `getHighCards()`.
    ///
    /// C++ mapping: `HoldemHand52::sum()`.
    #[must_use]
    pub fn sum(&self) -> i32 {
        self.get_high_cards()
            .into_iter()
            .map(|rank| i32::from(rank.value()))
            .sum()
    }

    /// Returns the classified category.
    ///
    /// C++ mapping: `HoldemHand52::getRank()`.
    #[must_use]
    pub const fn get_rank(&self) -> HoldemHandRank52 {
        self.hand_rank_
    }

    /// Parses space-separated card spellings into a classified holding.
    ///
    /// C++ mapping: `HoldemHand52::fromString()`.
    ///
    /// # Errors
    ///
    /// Returns an error when any token cannot be parsed as a card.
    pub fn from_string(input: &str) -> Result<Self, HoldemHandError> {
        let cards = input
            .split(' ')
            .map(Card52::from_str)
            .collect::<Result<Vec<_>, _>>()
            .map_err(HoldemHandError::InvalidCard)?;
        Ok(Self::from_cards(&cards))
    }

    fn classify_hand(&mut self) {
        let mut different_ranks = 0_usize;
        for card in &self.cards_ {
            let rank_index = usize::from(card.rank().value()) + 1;
            self.rank_occurences_[rank_index] += 1;
            if self.rank_occurences_[rank_index] == 1 {
                different_ranks += 1;
            }
            if card.rank() == Rank52::Ace {
                self.rank_occurences_[0] += 1;
            }
            self.suit_occurences_[usize::from(card.suit().value())] += 1;
        }

        let flush_suit = self.get_flush_suit(&self.suit_occurences_);
        let straight_rank = self.get_straight_rank(&self.rank_occurences_);
        if different_ranks == self.cards_.len() && flush_suit.is_none() && straight_rank.is_none() {
            self.hand_rank_ = HoldemHandRank52::HighCard;
            return;
        }
        if different_ranks == self.cards_.len().saturating_sub(1)
            && flush_suit.is_none()
            && straight_rank.is_none()
        {
            self.hand_rank_ = HoldemHandRank52::Pair;
            return;
        }
        if different_ranks == self.cards_.len().saturating_sub(2)
            && flush_suit.is_none()
            && straight_rank.is_none()
        {
            self.hand_rank_ = if self.rank_occurences_.contains(&3) {
                HoldemHandRank52::Trips
            } else {
                HoldemHandRank52::TwoPair
            };
            return;
        }
        if different_ranks <= self.cards_.len().saturating_sub(3)
            && flush_suit.is_none()
            && straight_rank.is_none()
        {
            self.hand_rank_ = if self.rank_occurences_.contains(&4) {
                HoldemHandRank52::Quads
            } else if self.rank_occurences_.contains(&3) {
                HoldemHandRank52::FullHouse
            } else {
                HoldemHandRank52::TwoPair
            };
            return;
        }
        match (flush_suit, straight_rank) {
            (Some(_), None) => self.hand_rank_ = HoldemHandRank52::Flush,
            (None, Some(_)) => self.hand_rank_ = HoldemHandRank52::Straight,
            (Some(suit), Some(_)) => {
                let straight_ranks = get_straight_ranks_from_occurrences(&self.rank_occurences_);
                for highest_rank in straight_ranks {
                    if get_straight_ranks(highest_rank)
                        .iter()
                        .all(|rank| self.cards_.contains(&Card52::new(*rank, suit)))
                    {
                        self.hand_rank_ = HoldemHandRank52::StraightFlush;
                        return;
                    }
                }
                self.hand_rank_ = HoldemHandRank52::Flush;
            }
            (None, None) => {}
        }
    }

    fn get_flush_suit(&self, suit_occurrences: &[u8]) -> Option<Suit> {
        suit_occurrences
            .iter()
            .position(|count| *count >= 5)
            .and_then(|index| suit_from_value(index as u8))
    }

    fn get_straight_rank(&self, rank_occurrences: &[u8]) -> Option<Rank52> {
        let mut neighbours_count = 0;
        let mut current_start_rank = None;
        for index in (0..rank_occurrences.len()).rev() {
            if neighbours_count == 4 {
                return current_start_rank;
            }
            if rank_occurrences[index] > 0 {
                if current_start_rank.is_none() {
                    current_start_rank = rank_from_occurrence_index_option(index);
                    neighbours_count = 1;
                } else {
                    neighbours_count += 1;
                }
            } else {
                neighbours_count = 0;
                current_start_rank = None;
            }
        }
        None
    }
}

impl PartialEq for HoldemHand52 {
    /// Compares hands by category and all source tie-break rules.
    fn eq(&self, other: &Self) -> bool {
        compare_hands(self, other) == Ordering::Equal
    }
}

impl Eq for HoldemHand52 {}

impl PartialOrd for HoldemHand52 {
    /// Compares hands by category and all source tie-break rules.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HoldemHand52 {
    /// Compares hands by category and all source tie-break rules.
    fn cmp(&self, other: &Self) -> Ordering {
        compare_hands(self, other)
    }
}

fn compare_hands(left: &HoldemHand52, right: &HoldemHand52) -> Ordering {
    let category_order = left.hand_rank_.cmp(&right.hand_rank_);
    if category_order != Ordering::Equal {
        return category_order;
    }
    match left.hand_rank_ {
        HoldemHandRank52::HighCard => {
            compare_high_cards(&left.get_high_cards(), &right.get_high_cards())
        }
        HoldemHandRank52::Pair => {
            let left_pair = left.find_occurences(2)[0];
            let right_pair = right.find_occurences(2)[0];
            left_pair
                .cmp(&right_pair)
                .then_with(|| compare_high_cards(&left.get_high_cards(), &right.get_high_cards()))
        }
        HoldemHandRank52::TwoPair => {
            let left_pairs = left.find_occurences(2);
            let right_pairs = right.find_occurences(2);
            left_pairs[0]
                .cmp(&right_pairs[0])
                .then_with(|| left_pairs[1].cmp(&right_pairs[1]))
                .then_with(|| compare_high_cards(&left.get_high_cards(), &right.get_high_cards()))
        }
        HoldemHandRank52::Trips => {
            let left_trips = left.find_occurences(3)[0];
            let right_trips = right.find_occurences(3)[0];
            left_trips
                .cmp(&right_trips)
                .then_with(|| compare_high_cards(&left.get_high_cards(), &right.get_high_cards()))
        }
        HoldemHandRank52::Straight | HoldemHandRank52::StraightFlush => {
            let mut left_sum = left.sum();
            let mut right_sum = right.sum();
            let wheel_sum = i32::from(Rank52::Ace.value())
                + i32::from(Rank52::Deuce.value())
                + i32::from(Rank52::Three.value())
                + i32::from(Rank52::Four.value())
                + i32::from(Rank52::Five.value());
            if left_sum == wheel_sum {
                left_sum -= i32::from(Rank52::Ace.value());
            }
            if right_sum == wheel_sum {
                right_sum -= i32::from(Rank52::Ace.value());
            }
            left_sum.cmp(&right_sum)
        }
        HoldemHandRank52::Flush => {
            compare_high_cards(&left.get_high_cards(), &right.get_high_cards())
        }
        HoldemHandRank52::FullHouse => left.find_occurences(3)[0]
            .cmp(&right.find_occurences(3)[0])
            .then_with(|| left.find_occurences(2)[0].cmp(&right.find_occurences(2)[0]))
            .then_with(|| compare_high_cards(&left.get_high_cards(), &right.get_high_cards())),
        HoldemHandRank52::Quads => left.find_occurences(4)[0]
            .cmp(&right.find_occurences(4)[0])
            .then_with(|| compare_high_cards(&left.get_high_cards(), &right.get_high_cards())),
    }
}

fn compare_high_cards(left: &[Rank52], right: &[Rank52]) -> Ordering {
    left.cmp(right)
}

fn find_occurences_at_least(occurrences: &[u8], count: u8) -> Vec<Rank52> {
    (1..occurrences.len())
        .rev()
        .filter(|index| occurrences[*index] >= count)
        .map(rank_from_occurrence_index)
        .collect()
}

fn reset_rank(occurrences: &[u8], rank: Rank52) -> Vec<u8> {
    let mut result = occurrences.to_vec();
    result[usize::from(rank.value()) + 1] = 0;
    result
}

fn get_current_straight_rank(rank: Rank52) -> Rank52 {
    if rank == Rank52::Ace {
        Rank52::Five
    } else {
        rank_from_value(rank.value() + 4)
            .expect("straight-rank arithmetic must remain within the rank enum")
    }
}

fn get_straight_ranks(highest_rank: Rank52) -> [Rank52; 5] {
    if highest_rank == Rank52::Five {
        [
            Rank52::Five,
            Rank52::Four,
            Rank52::Three,
            Rank52::Deuce,
            Rank52::Ace,
        ]
    } else {
        [
            highest_rank,
            rank_from_value(highest_rank.value() - 1).expect("a straight rank has a predecessor"),
            rank_from_value(highest_rank.value() - 2)
                .expect("a straight rank has two predecessors"),
            rank_from_value(highest_rank.value() - 3)
                .expect("a straight rank has three predecessors"),
            rank_from_value(highest_rank.value() - 4)
                .expect("a straight rank has four predecessors"),
        ]
    }
}

fn get_straight_ranks_from_occurrences(occurrences: &[u8]) -> Vec<Rank52> {
    let mut result = Vec::new();
    let mut neighbours_count = 0;
    let mut current_start_rank = None;
    for index in (0..occurrences.len()).rev() {
        if neighbours_count >= 4 {
            let source_rank_value =
                i32::try_from(index).expect("rank array length fits in i32") - 1;
            if let Some(rank) = get_current_straight_rank_from_value(source_rank_value) {
                result.push(rank);
            }
        }
        if occurrences[index] > 0 {
            if current_start_rank.is_none() {
                current_start_rank = rank_from_occurrence_index_option(index);
                neighbours_count = 1;
            } else {
                neighbours_count += 1;
            }
        } else {
            neighbours_count = 0;
            current_start_rank = None;
        }
    }
    result
}

fn get_current_straight_rank_from_value(rank_value: i32) -> Option<Rank52> {
    if rank_value == -1 {
        // The source's extra ace-at-low slot is cast to Rank52(-1), then
        // advanced by four to represent the five-high wheel.
        return Some(Rank52::Five);
    }
    u8::try_from(rank_value)
        .ok()
        .and_then(rank_from_value)
        .map(get_current_straight_rank)
}

fn get_first_n_high_cards(occurrences: &[u8], count: usize, _cards: &[Card52]) -> Vec<Rank52> {
    let mut high_cards = Vec::with_capacity(count);
    for index in (0..occurrences.len()).rev() {
        if count == 1 {
            if occurrences[index] >= 1
                && let Some(rank) = rank_from_occurrence_index_option(index)
            {
                high_cards.push(rank);
                break;
            }
        } else {
            if occurrences[index] == 1
                && let Some(rank) = rank_from_occurrence_index_option(index)
            {
                high_cards.push(rank);
            }
            if high_cards.len() == count {
                break;
            }
        }
    }
    high_cards
}

fn rank_from_occurrence_index(index: usize) -> Rank52 {
    rank_from_occurrence_index_option(index)
        .expect("rank occurrence index represents a standard rank")
}

fn rank_from_occurrence_index_option(index: usize) -> Option<Rank52> {
    index
        .checked_sub(1)
        .and_then(|rank| u8::try_from(rank).ok())
        .and_then(rank_from_value)
}

fn rank_from_value(value: u8) -> Option<Rank52> {
    Some(match value {
        0 => Rank52::Deuce,
        1 => Rank52::Three,
        2 => Rank52::Four,
        3 => Rank52::Five,
        4 => Rank52::Six,
        5 => Rank52::Seven,
        6 => Rank52::Eight,
        7 => Rank52::Nine,
        8 => Rank52::Ten,
        9 => Rank52::Jack,
        10 => Rank52::Queen,
        11 => Rank52::King,
        12 => Rank52::Ace,
        _ => return None,
    })
}

fn suit_from_value(value: u8) -> Option<Suit> {
    Some(match value {
        0 => Suit::Hearts,
        1 => Suit::Diamonds,
        2 => Suit::Spades,
        3 => Suit::Clubs,
        _ => return None,
    })
}
