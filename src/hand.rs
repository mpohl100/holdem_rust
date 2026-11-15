//! Poker hand construction, made-hand comparison, and draw evaluation.
//!
//! This module maps `cpp_code/src/Hand.h` and `Hand.cpp`. C++ class and enum
//! names retain their spelling; member functions use idiomatic `snake_case`.
//! C++ iterator constructors map to `new` methods taking slices, and C++
//! exceptions for invalid card counts map to [`HandError`].
//!
//! Dependency assumptions (the corresponding Rust modules are not present in
//! this checkout): `Card52` provides `rank()`, `suit()`, an infallible
//! `new(Rank52, Suit)` constructor, `FromStr`, and `Clone + Eq + Ord`;
//! `Rank52` and `Suit` are ordered enums. `HoleCards` and `Board` provide
//! `get_cards()`; `Board` provides `deal_flop`, `deal_turn`, and `deal_river`.
//! The rank and suit variant names follow the C++ declarations.

use crate::board::Board;
use crate::card52::{Card52, Rank52, Suit};
use crate::combinatorics;
use crate::hole_cards::HoleCards;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

/// Error returned when a hand operation cannot be performed on the supplied cards.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HandError {
    /// A made hand did not contain exactly five cards.
    MadeHandCardCount(usize),
    /// Five supplied cards produced a rank distribution not classifiable by C++.
    InvalidRankMultiplicity,
    /// A drawing hand did not contain exactly four cards.
    DrawingHandCardCount(usize),
    /// The input string contained a card that `Card52` could not parse.
    InvalidCard(String),
    /// A requested rank could not be determined from the hand's occurrence map.
    MissingRank,
    /// A hand contained fewer than the five cards required for made-hand evaluation.
    NotEnoughCards(usize),
    /// A hand contained fewer than the four cards required for draw evaluation.
    NotEnoughDrawCards(usize),
    /// The board has a card count unsupported by the C++ board model.
    InvalidBoardCardCount(usize),
    /// No board completions were available for the supplied hole cards.
    NoCompletionBoards,
    /// The number of completion cards does not fit the source C++ `int` type.
    TooManyOuts(usize),
    /// System time could not provide the seed required for preflop sampling.
    ClockUnavailable,
}

impl std::fmt::Display for HandError {
    /// Formats this hand-evaluation error.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MadeHandCardCount(count) => {
                write!(formatter, "classified hand != size 5 (got {count})")
            }
            Self::InvalidRankMultiplicity => {
                formatter.write_str("five cards have an unclassifiable rank multiplicity")
            }
            Self::DrawingHandCardCount(count) => {
                write!(formatter, "drawing hand != size 4 (got {count})")
            }
            Self::InvalidCard(card) => write!(formatter, "invalid card: {card}"),
            Self::MissingRank => formatter.write_str("hand does not contain a required rank"),
            Self::NotEnoughCards(count) => {
                write!(
                    formatter,
                    "hand has {count} cards; at least five are required"
                )
            }
            Self::NotEnoughDrawCards(count) => {
                write!(
                    formatter,
                    "hand has {count} cards; at least four are required"
                )
            }
            Self::InvalidBoardCardCount(count) => {
                write!(formatter, "board has unsupported card count {count}")
            }
            Self::NoCompletionBoards => formatter.write_str("no completed boards were generated"),
            Self::TooManyOuts(count) => write!(formatter, "{count} outs do not fit in an i32"),
            Self::ClockUnavailable => formatter.write_str("system clock is before the Unix epoch"),
        }
    }
}

impl std::error::Error for HandError {}

/// A player's hole cards together with the board cards.
///
/// C++ mapping: `game52::Hand`; its private `cards_`, `holeCards_`, and
/// `board_` members map to Rust fields `cards_`, `hole_cards_`, and `board`.
#[derive(Clone, Debug, Default)]
pub struct Hand {
    /// `Hand::cards_`: hole cards followed by board cards.
    cards_: Vec<Card52>,
    /// `Hand::holeCards_`: the player's private cards.
    hole_cards_: HoleCards,
    /// `Hand::board_`: the community board retained with the hand.
    _board: Board,
}

impl Hand {
    /// Creates a hand from owned hole cards and board cards in source order.
    ///
    /// C++ mapping: `Hand(HoleCards const&, Board const&)`.
    #[must_use]
    pub fn new(hole_cards: HoleCards, board: Board) -> Self {
        let mut cards = hole_cards.get_cards().to_vec();
        cards.extend(board.get_cards().iter().copied());
        Self {
            cards_: cards,
            hole_cards_: hole_cards,
            _board: board,
        }
    }

    /// Returns a copy of the complete hole-card-plus-board card sequence.
    ///
    /// C++ mapping: `Hand::getCards()`.
    #[must_use]
    pub fn get_cards(&self) -> Vec<Card52> {
        self.cards_.clone()
    }

    /// Borrows the player's hole cards.
    ///
    /// C++ mapping: `Hand::getHoleCards()`.
    #[must_use]
    pub fn get_hole_cards(&self) -> &HoleCards {
        &self.hole_cards_
    }

    /// Returns the source phrase describing the best five-card made hand.
    ///
    /// C++ mapping: `Hand::toString()`. Unlike the C++ function, formatting
    /// panics with the descriptive source error if an invalid hand cannot be
    /// evaluated. Use [`Hand::try_to_string`] for explicit error propagation.
    fn formatted_hand(&self) -> String {
        match self.try_to_string() {
            Ok(description) => description,
            Err(error) => panic!("Hand::toString failed: {error}"),
        }
    }

    /// Returns the hand description with explicit invalid-hand error handling.
    ///
    /// This is the fallible Rust counterpart of C++ `Hand::toString()`.
    ///
    /// # Errors
    ///
    /// Returns a [`HandError`] if the hand contains fewer than five cards or
    /// its cards cannot be classified as a valid made hand.
    pub fn try_to_string(&self) -> Result<String, HandError> {
        let made_hand = try_get_best_hand(self)?;
        Ok(match made_hand.hand_rank_ {
            HandRank52::HighCard => "High Card.",
            HandRank52::Pair => "a pair.",
            HandRank52::TwoPair => "two pair.",
            HandRank52::Trips => "three of a kind.",
            HandRank52::Straight => "a straight.",
            HandRank52::Flush => "a flush.",
            HandRank52::FullHouse => "a full house.",
            HandRank52::Quads => "four of a kind.",
            HandRank52::StraightFlush => "a straight flush.",
        }
        .to_owned())
    }
}

impl std::fmt::Display for Hand {
    /// Formats the best five-card made-hand description.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.formatted_hand())
    }
}

/// The C++ `MadeHand52::HandRank52` enumeration, in source declaration order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(i32)]
pub enum HandRank52 {
    /// `HighCard`: no pair or stronger made hand.
    HighCard = 0,
    /// `Pair`: one pair.
    Pair = 1,
    /// `TwoPair`: two distinct pairs.
    TwoPair = 2,
    /// `Trips`: three cards of one rank.
    Trips = 3,
    /// `Straight`: five consecutive ranks, including a wheel.
    Straight = 4,
    /// `Flush`: five cards of one suit.
    Flush = 5,
    /// `FullHouse`: three of a rank and a pair.
    FullHouse = 6,
    /// `Quads`: four cards of one rank.
    Quads = 7,
    /// `StraightFlush`: a straight and a flush.
    StraightFlush = 8,
}

/// A classified five-card poker hand.
///
/// C++ mapping: `game52::MadeHand52`. The three public C++ data members remain
/// public; `rankOccurences_` retains the source misspelling.
#[derive(Clone, Debug)]
pub struct MadeHand52 {
    /// `MadeHand52::cards_`: the five cards in the supplied order.
    pub cards_: Vec<Card52>,
    /// `MadeHand52::handRank_`: the classified made-hand category.
    pub hand_rank_: HandRank52,
    /// `MadeHand52::rankOccurences_`: count of each rank in the hand.
    pub rank_occurences_: BTreeMap<Rank52, i32>,
}

impl Default for MadeHand52 {
    /// Creates a safely initialized empty made hand.
    ///
    /// C++ default construction left `handRank_` indeterminate; Rust uses
    /// `HighCard` as a safe placeholder while retaining the empty cards/map.
    fn default() -> Self {
        Self {
            cards_: Vec::new(),
            hand_rank_: HandRank52::HighCard,
            rank_occurences_: BTreeMap::new(),
        }
    }
}

impl MadeHand52 {
    /// Classifies exactly five cards.
    ///
    /// C++ mapping: the iterator-range constructor
    /// `MadeHand52(begin, end)`. Invalid sizes are reported as
    /// [`HandError::MadeHandCardCount`].
    ///
    /// # Errors
    ///
    /// Returns an error if there are not exactly five cards or their rank
    /// multiplicities do not form a supported made-hand category.
    pub fn new(cards: &[Card52]) -> Result<Self, HandError> {
        if cards.len() != 5 {
            return Err(HandError::MadeHandCardCount(cards.len()));
        }

        let mut rank_occurences = BTreeMap::new();
        for card in cards {
            *rank_occurences.entry(card.rank()).or_insert(0) += 1;
        }

        if rank_occurences.len() == 1 {
            return Err(HandError::InvalidRankMultiplicity);
        }
        let hand_rank = if rank_occurences.len() == 5 {
            let is_flush = cards.iter().all(|card| card.suit() == cards[0].suit());
            let mut ranks: Vec<i32> = cards.iter().map(|card| rank_value(card.rank())).collect();
            ranks.sort_unstable();
            let is_straight = ranks[4] - ranks[0] == 4;
            let is_wheel = ranks == [0, 1, 2, 3, rank_value(crate::card52::Rank52::Ace)];
            match (is_straight || is_wheel, is_flush) {
                (true, true) => HandRank52::StraightFlush,
                (true, false) => HandRank52::Straight,
                (false, true) => HandRank52::Flush,
                (false, false) => HandRank52::HighCard,
            }
        } else {
            match rank_occurences.len() {
                4 => HandRank52::Pair,
                3 if rank_occurences.values().any(|count| *count == 3) => HandRank52::Trips,
                3 => HandRank52::TwoPair,
                2 if rank_occurences.values().any(|count| *count == 4) => HandRank52::Quads,
                2 => HandRank52::FullHouse,
                _ => return Err(HandError::InvalidRankMultiplicity),
            }
        };

        Ok(Self {
            cards_: cards.to_vec(),
            hand_rank_: hand_rank,
            rank_occurences_: rank_occurences,
        })
    }

    /// Returns ranks occurring exactly `count` times, sorted highest first.
    ///
    /// C++ mapping: the misspelled declaration `findOccurences(int)`.
    /// Missing entries are initialized to `Rank52::Deuce`; C++ left those
    /// array entries indeterminate when fewer than two ranks matched.
    #[must_use]
    pub fn find_occurences(&self, count: i32) -> [Rank52; 2] {
        let mut result = [Rank52::Deuce; 2];
        let mut index = 0;
        for (rank, occurrences) in &self.rank_occurences_ {
            if *occurrences == count && index < result.len() {
                result[index] = *rank;
                index += 1;
            }
        }
        if index == 2 && result[0] < result[1] {
            result.swap(0, 1);
        }
        result
    }

    /// Returns singleton ranks in descending order.
    ///
    /// C++ mapping: `MadeHand52::getHighCards()`.
    #[must_use]
    pub fn get_high_cards(&self) -> Vec<Rank52> {
        let mut ranks: Vec<_> = self
            .rank_occurences_
            .iter()
            .filter_map(|(rank, count)| (*count == 1).then_some(*rank))
            .collect();
        ranks.sort_unstable_by(|left, right| right.cmp(left));
        ranks
    }

    /// Sums the numeric rank values of the five cards.
    ///
    /// C++ mapping: `MadeHand52::sum()`.
    #[must_use]
    pub fn sum(&self) -> i32 {
        self.cards_.iter().map(|card| rank_value(card.rank())).sum()
    }

    /// Returns the principal rank used to distinguish this hand category.
    ///
    /// C++ mapping: `MadeHand52::getRelevantCard()`.
    ///
    /// # Errors
    ///
    /// Returns [`HandError::MissingRank`] if the category has no relevant
    /// rank in the stored cards.
    pub fn get_relevant_card(&self) -> Result<Rank52, HandError> {
        let rank = match self.hand_rank_ {
            HandRank52::HighCard => Rank52::Deuce,
            HandRank52::Pair | HandRank52::TwoPair => self.find_occurences(2)[0],
            HandRank52::Trips | HandRank52::FullHouse => self.find_occurences(3)[0],
            HandRank52::Quads => self.find_occurences(4)[0],
            HandRank52::Straight | HandRank52::Flush | HandRank52::StraightFlush => *self
                .get_high_cards()
                .first()
                .ok_or(HandError::MissingRank)?,
        };
        Ok(rank)
    }

    /// Returns the highest singleton rank, or an error if no singleton exists.
    ///
    /// C++ mapping: `MadeHand52::getKicker()`.
    ///
    /// # Errors
    ///
    /// Returns [`HandError::MissingRank`] when the hand has no singleton rank.
    pub fn get_kicker(&self) -> Result<Rank52, HandError> {
        self.get_high_cards()
            .first()
            .copied()
            .ok_or(HandError::MissingRank)
    }

    /// Returns this hand's category.
    ///
    /// C++ mapping: `MadeHand52::getRank()`.
    #[must_use]
    pub fn get_rank(&self) -> HandRank52 {
        self.hand_rank_
    }

    /// Returns a copy of the five cards.
    ///
    /// C++ mapping: `MadeHand52::getCards()`.
    #[must_use]
    pub fn get_cards(&self) -> Vec<Card52> {
        self.cards_.clone()
    }

    /// Parses five whitespace-separated card descriptions and classifies them.
    ///
    /// C++ mapping: `MadeHand52::fromString()`. Parsing is delegated to the
    /// dependency's standard `FromStr` implementation for `Card52`.
    ///
    /// # Errors
    ///
    /// Returns an error if any token is not a valid card or the parsed cards
    /// do not form a valid five-card made hand.
    pub fn from_string(input: &str) -> Result<Self, HandError> {
        let cards = input
            .split(' ')
            .map(|token| {
                Card52::from_str(token).map_err(|_| HandError::InvalidCard(token.to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(&cards)
    }
}

impl PartialEq for MadeHand52 {
    /// Compares poker value, matching C++ `operator==` rather than card identity.
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for MadeHand52 {}

impl PartialOrd for MadeHand52 {
    /// Compares poker value.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MadeHand52 {
    /// Compares made hands using the source category and tiebreak rules.
    fn cmp(&self, other: &Self) -> Ordering {
        let category = self.hand_rank_.cmp(&other.hand_rank_);
        if category != Ordering::Equal {
            return category;
        }
        match self.hand_rank_ {
            HandRank52::HighCard | HandRank52::Flush => {
                compare_high_cards(&self.get_high_cards(), &other.get_high_cards())
            }
            HandRank52::Pair => compare_repeated_then_high(self, other, 2),
            HandRank52::TwoPair => {
                let left = self.find_occurences(2);
                let right = other.find_occurences(2);
                left[0]
                    .cmp(&right[0])
                    .then_with(|| left[1].cmp(&right[1]))
                    .then_with(|| {
                        compare_high_cards(&self.get_high_cards(), &other.get_high_cards())
                    })
            }
            HandRank52::Trips => compare_repeated_then_high(self, other, 3),
            HandRank52::Straight | HandRank52::StraightFlush => {
                straight_value(self).cmp(&straight_value(other))
            }
            HandRank52::FullHouse => self.find_occurences(3)[0]
                .cmp(&other.find_occurences(3)[0])
                .then_with(|| self.find_occurences(2)[0].cmp(&other.find_occurences(2)[0]))
                .then_with(|| compare_high_cards(&self.get_high_cards(), &other.get_high_cards())),
            HandRank52::Quads => compare_repeated_then_high(self, other, 4),
        }
    }
}

/// Compares two hands using C++ `operator<`, then compares their singleton ranks.
fn compare_repeated_then_high(left: &MadeHand52, right: &MadeHand52, count: i32) -> Ordering {
    left.find_occurences(count)[0]
        .cmp(&right.find_occurences(count)[0])
        .then_with(|| compare_high_cards(&left.get_high_cards(), &right.get_high_cards()))
}

/// Converts a zero-based `Rank52` discriminant to the C++ integer rank value.
fn rank_value(rank: Rank52) -> i32 {
    rank as i32
}

/// Compares two descending high-card lists lexicographically.
fn compare_high_cards(left: &[Rank52], right: &[Rank52]) -> Ordering {
    left.cmp(right)
}

/// Returns the straight value, treating ace-low wheels as five-high.
fn straight_value(hand: &MadeHand52) -> i32 {
    let sum = hand.sum();
    let wheel_sum = rank_value(Rank52::Ace) + 1 + 2 + 3;
    if sum == wheel_sum {
        sum - rank_value(Rank52::Ace)
    } else {
        sum
    }
}

/// Returns the strongest five-card hand available from a hand's cards.
///
/// C++ mapping: `getBestHand(Hand const&)`. The C++ source evaluates every
/// five-card combination; this uses the crate's translated combination
/// traversal and reports short input instead of performing invalid iterator
/// arithmetic by panicking with a descriptive error. Use
/// [`try_get_best_hand`] to propagate errors.
///
/// # Panics
///
/// Panics if fewer than five cards are available or the selected cards cannot
/// be classified as a made hand.
#[must_use]
pub fn get_best_hand(hand: &Hand) -> MadeHand52 {
    match try_get_best_hand(hand) {
        Ok(best) => best,
        Err(error) => panic!("getBestHand failed: {error}"),
    }
}

/// Fallible implementation of [`get_best_hand`].
///
/// The C++ operation throws when fewer than five cards are available; this
/// Rust helper returns that failure as [`HandError`].
///
/// # Errors
///
/// Returns an error when fewer than five cards are available or any selected
/// five-card subset cannot be classified.
pub fn try_get_best_hand(hand: &Hand) -> Result<MadeHand52, HandError> {
    let mut cards = hand.get_cards();
    if cards.len() < 5 {
        return Err(HandError::NotEnoughCards(cards.len()));
    }
    let mut best: Option<MadeHand52> = None;
    let mut callback_error = None;
    let _callback = combinatorics::for_each_combination(&mut cards, 5, |selected| {
        match MadeHand52::new(selected) {
            Ok(candidate) => {
                if best
                    .as_ref()
                    .is_none_or(|current| candidate.cmp(current) == Ordering::Greater)
                {
                    best = Some(candidate);
                }
            }
            Err(error) => callback_error = Some(error),
        }
        false
    });
    if let Some(error) = callback_error {
        return Err(error);
    }
    best.ok_or(HandError::NotEnoughCards(cards.len()))
}

/// Compares the strongest five-card hands and returns -1, 0, or 1.
///
/// C++ mapping: `compareHands(Hand const&, Hand const&)`. Invalid hands panic
/// with a descriptive source error, matching uncaught C++ exceptions; use
/// [`try_compare_hands`] for explicit propagation.
///
/// # Panics
///
/// Panics if either hand has fewer than five cards or contains an invalid
/// made-hand selection.
#[must_use]
pub fn compare_hands(left: &Hand, right: &Hand) -> i32 {
    match try_compare_hands(left, right) {
        Ok(result) => result,
        Err(error) => panic!("compareHands failed: {error}"),
    }
}

/// Fallible implementation of [`compare_hands`].
///
/// # Errors
///
/// Returns an error if either hand cannot provide a valid five-card hand.
pub fn try_compare_hands(left: &Hand, right: &Hand) -> Result<i32, HandError> {
    let left_hand = try_get_best_hand(left)?;
    let right_hand = try_get_best_hand(right)?;
    Ok(match left_hand.cmp(&right_hand) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    })
}

/// The C++ `DrawingHand52::DrawRank52` enumeration, in source order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(i32)]
pub enum DrawRank52 {
    /// No flush or straight draw.
    None = 0,
    /// Four wheel ranks with a wheel gutshot completion.
    WheelGutshot = 1,
    /// Four ranks with one interior straight completion.
    Gutshot = 2,
    /// Four consecutive ranks with two open-ended completions.
    Openend = 3,
    /// Four cards of one suit.
    Flush = 4,
    /// Flush draw together with a wheel gutshot.
    FlushWheelGutshot = 5,
    /// Flush draw together with a gutshot.
    FlushGutshot = 6,
    /// Flush draw together with an open-ended straight draw.
    FlushOpenend = 7,
}

/// A four-card subset classified for straight and flush draws.
///
/// C++ mapping: `game52::DrawingHand52`, including its public C++ fields.
#[derive(Clone, Debug)]
pub struct DrawingHand52 {
    /// `DrawingHand52::cards_`: the four cards in source order.
    pub cards_: Vec<Card52>,
    /// `DrawingHand52::handRank_`: the classified draw category.
    pub hand_rank_: DrawRank52,
    /// `DrawingHand52::rankOccurences_`: occurrence count per rank.
    pub rank_occurences_: BTreeMap<Rank52, i32>,
}

impl Default for DrawingHand52 {
    /// Creates a safely initialized empty drawing hand.
    ///
    /// C++ default construction left `handRank_` indeterminate; Rust uses
    /// `DrawRank52::None` as a safe placeholder.
    fn default() -> Self {
        Self {
            cards_: Vec::new(),
            hand_rank_: DrawRank52::None,
            rank_occurences_: BTreeMap::new(),
        }
    }
}

impl DrawingHand52 {
    /// Classifies exactly four cards for flush and straight draws.
    ///
    /// C++ mapping: the iterator-range constructor `DrawingHand52(begin, end)`.
    ///
    /// # Errors
    ///
    /// Returns an error unless exactly four cards are supplied or if their
    /// rank multiplicities cannot form a supported draw.
    pub fn new(cards: &[Card52]) -> Result<Self, HandError> {
        if cards.len() != 4 {
            return Err(HandError::DrawingHandCardCount(cards.len()));
        }
        let mut rank_occurences = BTreeMap::new();
        for card in cards {
            *rank_occurences.entry(card.rank()).or_insert(0) += 1;
        }

        let mut hand_rank = DrawRank52::None;
        if rank_occurences.len() == 4 {
            let is_flush = cards.iter().all(|card| card.suit() == cards[0].suit());
            let mut ranks: Vec<i32> = cards.iter().map(|card| rank_value(card.rank())).collect();
            ranks.sort_unstable();
            let minimum = ranks[0];
            let maximum = ranks[3];
            let is_openend = maximum - minimum == 3 && maximum <= rank_value(Rank52::King);
            let is_gutshot = maximum - minimum == 4
                || (maximum == rank_value(Rank52::Ace) && minimum == rank_value(Rank52::Jack));
            let mut wheel_ranks = ranks.clone();
            if wheel_ranks.contains(&rank_value(Rank52::Ace)) {
                for rank in &mut wheel_ranks {
                    if *rank == rank_value(Rank52::Ace) {
                        *rank -= 13;
                    }
                }
            }
            wheel_ranks.sort_unstable();
            let is_wheel_gutshot = wheel_ranks[0] == rank_value(Rank52::Ace) - 13
                && wheel_ranks[3] <= rank_value(Rank52::Five);

            hand_rank = match (is_flush, is_wheel_gutshot, is_gutshot, is_openend) {
                (true, true, _, _) => DrawRank52::FlushWheelGutshot,
                (true, _, true, _) => DrawRank52::FlushGutshot,
                (true, _, _, true) => DrawRank52::FlushOpenend,
                (true, _, _, _) => DrawRank52::Flush,
                (false, true, _, _) => DrawRank52::WheelGutshot,
                (false, _, true, _) => DrawRank52::Gutshot,
                (false, _, _, true) => DrawRank52::Openend,
                (false, _, _, _) => DrawRank52::None,
            };
        }

        Ok(Self {
            cards_: cards.to_vec(),
            hand_rank_: hand_rank,
            rank_occurences_: rank_occurences,
        })
    }

    /// Returns the ranks in the same order as the input cards.
    ///
    /// C++ mapping: `DrawingHand52::getHighCards()`.
    #[must_use]
    pub fn get_high_cards(&self) -> Vec<Rank52> {
        self.cards_.iter().map(|card| card.rank()).collect()
    }

    /// Returns the possible completion cards for this draw.
    ///
    /// C++ mapping: `DrawingHand52::getOuts()`. The C++ implementation
    /// dereferenced a missing-gap iterator for wheel and ace-high gutshots;
    /// this safe translation identifies their missing rank explicitly.
    pub fn get_outs(&mut self) -> Vec<Card52> {
        let mut outs = Vec::new();
        if self.is_flush_draw() || self.is_straight_draw() {
            self.cards_.sort();
        }
        if self.is_flush_draw() {
            let suit = self.cards_[0].suit();
            let used: BTreeSet<_> = self.cards_.iter().copied().collect();
            outs.extend(
                all_cards_for_suit(suit)
                    .into_iter()
                    .filter(|card| !used.contains(card)),
            );
        }
        if self.is_straight_draw() {
            let mut ranks: Vec<_> = self
                .cards_
                .iter()
                .map(|card| rank_value(card.rank()))
                .collect();
            ranks.sort_unstable();
            let completion_ranks = if self.is_openend() {
                let lower = if ranks[0] == 0 {
                    rank_value(Rank52::Ace)
                } else {
                    ranks[0] - 1
                };
                let upper = ranks[3] + 1;
                vec![lower, upper]
            } else if ranks == [0, 1, 2, rank_value(Rank52::Ace)] {
                vec![rank_value(Rank52::Five)]
            } else if ranks
                == [
                    rank_value(Rank52::Jack),
                    rank_value(Rank52::Queen),
                    rank_value(Rank52::King),
                    rank_value(Rank52::Ace),
                ]
            {
                vec![rank_value(Rank52::Ten)]
            } else {
                let missing = ranks
                    .windows(2)
                    .find(|pair| pair[1] - pair[0] == 2)
                    .map(|pair| pair[0] + 1);
                missing.into_iter().collect()
            };
            for rank in completion_ranks {
                if let Some(rank) = rank_from_value(rank) {
                    outs.extend(all_cards_for_rank(rank));
                }
            }
        }
        outs
    }

    /// Parses four whitespace-separated card descriptions and classifies them.
    ///
    /// C++ mapping: `DrawingHand52::fromString()`.
    ///
    /// # Errors
    ///
    /// Returns an error if a token is not a valid card or the parsed cards do
    /// not form a valid four-card draw.
    pub fn from_string(input: &str) -> Result<Self, HandError> {
        let cards = input
            .split(' ')
            .map(|token| {
                Card52::from_str(token).map_err(|_| HandError::InvalidCard(token.to_owned()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(&cards)
    }

    /// Returns whether this draw contains a flush draw.
    ///
    /// C++ mapping: `DrawingHand52::isFlushDraw()`.
    #[must_use]
    pub fn is_flush_draw(&self) -> bool {
        matches!(
            self.hand_rank_,
            DrawRank52::Flush
                | DrawRank52::FlushWheelGutshot
                | DrawRank52::FlushGutshot
                | DrawRank52::FlushOpenend
        )
    }

    /// Returns whether this draw contains a straight draw.
    ///
    /// C++ mapping: `DrawingHand52::isStraightDraw()`.
    #[must_use]
    pub fn is_straight_draw(&self) -> bool {
        !matches!(self.hand_rank_, DrawRank52::None | DrawRank52::Flush)
    }

    /// Returns whether this draw is open-ended.
    ///
    /// C++ mapping: the source spelling `DrawingHand52::isOpenend()`.
    #[must_use]
    pub fn is_openend(&self) -> bool {
        matches!(
            self.hand_rank_,
            DrawRank52::Openend | DrawRank52::FlushOpenend
        )
    }
}

impl PartialEq for DrawingHand52 {
    /// Compares draw category and source-ordered ranks, matching C++ equality.
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for DrawingHand52 {}

impl PartialOrd for DrawingHand52 {
    /// Compares two drawing hands.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DrawingHand52 {
    /// Compares draw category first, then the source-ordered ranks.
    fn cmp(&self, other: &Self) -> Ordering {
        self.hand_rank_
            .cmp(&other.hand_rank_)
            .then_with(|| self.get_high_cards().cmp(&other.get_high_cards()))
    }
}

/// Constructs every card of a suit in ascending rank order.
///
/// C++ mapping: the translation-unit helper `getAllCards(Suit)`.
#[must_use]
pub fn all_cards_for_suit(suit: Suit) -> Vec<Card52> {
    all_ranks()
        .into_iter()
        .map(|rank| Card52::new(rank, suit))
        .collect()
}

/// Constructs every suit of a rank in the C++ suit declaration order.
///
/// C++ mapping: the translation-unit helper `getAllCards(Rank52)`.
#[must_use]
pub fn all_cards_for_rank(rank: Rank52) -> Vec<Card52> {
    all_suits()
        .into_iter()
        .map(|suit| Card52::new(rank, suit))
        .collect()
}

/// Returns the rank variants in their C++ declaration order.
fn all_ranks() -> [Rank52; 13] {
    [
        Rank52::Deuce,
        Rank52::Three,
        Rank52::Four,
        Rank52::Five,
        Rank52::Six,
        Rank52::Seven,
        Rank52::Eight,
        Rank52::Nine,
        Rank52::Ten,
        Rank52::Jack,
        Rank52::Queen,
        Rank52::King,
        Rank52::Ace,
    ]
}

/// Returns the suit variants in their C++ declaration order.
fn all_suits() -> [Suit; 4] {
    [Suit::Hearts, Suit::Diamonds, Suit::Spades, Suit::Clubs]
}

/// Converts a C++ zero-based rank value back to its corresponding rank variant.
fn rank_from_value(value: i32) -> Option<Rank52> {
    all_ranks()
        .into_iter()
        .find(|rank| rank_value(*rank) == value)
}

/// Describes all valid draws available from a hand.
///
/// C++ mapping: `game52::AllDraws`; its private `cards_` and `outs_` fields
/// remain private.
#[derive(Clone, Debug, Default)]
pub struct AllDraws {
    /// `AllDraws::cards_`: cards used when determining whether outs are live.
    cards_: Vec<Card52>,
    /// `AllDraws::outs_`: number of distinct live completion cards.
    outs_: i32,
}

impl AllDraws {
    /// Computes the number of distinct live outs in every four-card subset.
    ///
    /// C++ mapping: `AllDraws(Hand const&)`. As in C++, a complete seven-card
    /// hand has no draw outs; all outs are also rejected if they overlap any
    /// card already in the hand.
    ///
    /// # Errors
    ///
    /// Returns an error if the hand has fewer than four cards or a four-card
    /// subset cannot be classified as a draw.
    pub fn new(hand: &Hand) -> Result<Self, HandError> {
        let cards = hand.get_cards();
        if cards.len() == 7 {
            return Ok(Self {
                cards_: cards,
                outs_: 0,
            });
        }
        if cards.len() < 4 {
            return Err(HandError::NotEnoughDrawCards(cards.len()));
        }

        let mut traversal = cards.clone();
        let mut draws = Vec::new();
        let mut callback_error = None;
        let _callback = combinatorics::for_each_combination(&mut traversal, 4, |selected| {
            match DrawingHand52::new(selected) {
                Ok(draw) if draw.hand_rank_ > DrawRank52::None => draws.push(draw),
                Ok(_) => {}
                Err(error) => callback_error = Some(error),
            }
            false
        });
        if let Some(error) = callback_error {
            return Err(error);
        }
        let mut outs = BTreeSet::new();
        for mut draw in draws {
            outs.extend(draw.get_outs());
        }
        let mut result = Self {
            cards_: cards,
            outs_: 0,
        };
        if outs.iter().any(|out| result.cards_.contains(out)) {
            return Ok(result);
        }
        result.outs_ = i32::try_from(outs.len()).map_err(|_| HandError::TooManyOuts(outs.len()))?;
        Ok(result)
    }

    /// Returns the number of distinct completion cards.
    ///
    /// C++ mapping: `AllDraws::getOuts()`.
    #[must_use]
    pub fn get_outs(&self) -> i32 {
        self.outs_
    }
}

#[cfg(test)]
mod tests {
    use super::{DrawRank52, DrawingHand52, HandError, HandRank52, MadeHand52};

    /// Ports every category fixture in `ClassifiedHandTests.cpp`.
    #[test]
    fn cpp_made_hand_rank_fixtures() {
        let cases = [
            ("Kh Td 2c 3s 5d", HandRank52::HighCard),
            ("Ah Td 2c 3s 5d", HandRank52::HighCard),
            ("Kh 2h 2c Qs Jd", HandRank52::Pair),
            ("Ah 2h 2c 3s 5d", HandRank52::Pair),
            ("Ah 2h 2c 4s 5d", HandRank52::Pair),
            ("Ah Td Tc 3s 5d", HandRank52::Pair),
            ("Ah Ad 2c 3s 5d", HandRank52::Pair),
            ("Qh Qd 2c 2s 5d", HandRank52::TwoPair),
            ("Qh Qd Jc Js 6d", HandRank52::TwoPair),
            ("Ah Ad 2c 2s 5d", HandRank52::TwoPair),
            ("Ah 2d 2c 2s 5d", HandRank52::Trips),
            ("Ah Ad Ac 2s 5d", HandRank52::Trips),
            ("Ah Ad Ac 3s 5d", HandRank52::Trips),
            ("Ah 2d 3c 4s 5d", HandRank52::Straight),
            ("2d 3c 4s 5d 6c", HandRank52::Straight),
            ("Ah 2h Kh 4h 5h", HandRank52::Flush),
            ("Ah 2h Kh 6h 5h", HandRank52::Flush),
            ("5h 2d 2c 2s 5d", HandRank52::FullHouse),
            ("6h 2d 2c 2s 6d", HandRank52::FullHouse),
            ("5h 3d 3c 3s 5d", HandRank52::FullHouse),
            ("5h 2d 2c 2s 2h", HandRank52::Quads),
            ("5h 3d 3c 3s 3h", HandRank52::Quads),
            ("Ah 2h 3h 4h 5h", HandRank52::StraightFlush),
            ("2h 3h 4h 5h 6h", HandRank52::StraightFlush),
            ("Th Jh Qh Kh Ah", HandRank52::StraightFlush),
        ];
        for (cards, category) in cases {
            assert_eq!(
                MadeHand52::from_string(cards).unwrap().get_rank(),
                category,
                "{cards}"
            );
        }
    }

    /// Ports the source's made-hand total-order and suit-insensitive equality fixtures.
    #[test]
    fn cpp_made_hand_order_and_equality_fixtures() {
        let ordered = [
            "Kh Td 2c 3s 5d",
            "Ah Td 2c 3s 5d",
            "Kh 2h 2c Qs Jd",
            "Ah 2h 2c 3s 5d",
            "Ah 2h 2c Ks 5d",
            "Ah 2h 2c Ks Qd",
            "Ah Td Tc 3s 5d",
            "Ah Ad 2c 3s 5d",
            "Qh Qd 2c 2s 5d",
            "Qh Qd Jc Js 6d",
            "Ah Ad 2c 2s 5d",
            "Ah 2d 2c 2s 5d",
            "Ah Ad Ac 2s 5d",
            "Ah Ad Ac 3s 5d",
            "Ah 2d 3c 4s 5d",
            "2d 3c 4s 5d 6c",
            "Ah 2h Kh 4h 5h",
            "Ah 2h Kh 6h 5h",
            "5h 2d 2c 2s 5d",
            "6h 2d 2c 2s 6d",
            "5h 3d 3c 3s 5d",
            "5h 2d 2c 2s 2h",
            "5h 3d 3c 3s 3h",
            "Ah 2h 3h 4h 5h",
            "2h 3h 4h 5h 6h",
            "Th Jh Qh Kh Ah",
        ];
        let hands = ordered
            .into_iter()
            .map(|cards| MadeHand52::from_string(cards).unwrap())
            .collect::<Vec<_>>();
        assert!(hands.windows(2).all(|pair| pair[0] <= pair[1]));

        let equal_pairs = [
            ("Ah Kc 2h 7s Td", "Ac Kc 2h 7d Ts"),
            ("Ah 2c 2h 7s Td", "Ac 2s 2h 7d Ts"),
            ("Ah 2c 2h 7s 7d", "Ac 2c 2h 7d 7c"),
            ("Ah Kc 2h 2s 2d", "Ah Kh 2h 2d 2s"),
            ("Ah 2c 3h 4s 5d", "Ac 2c 3h 4d 5s"),
            ("Ah Kh 3h 4h 5h", "As Ks 3s 4s 5s"),
            ("2h 2c 2d 4s 4d", "2s 2c 2h 4c 4s"),
            ("2h 2c 2d 2s 4d", "2s 2c 2h 2d 4s"),
            ("Ah 2h 3h 4h 5h", "As 2s 3s 4s 5s"),
            ("2h 3h 4h 5h 6h", "2s 3s 4s 5s 6s"),
        ];
        for (left, right) in equal_pairs {
            assert_eq!(
                MadeHand52::from_string(left).unwrap(),
                MadeHand52::from_string(right).unwrap()
            );
        }
    }

    /// Ports the source's complete four-card draw classification fixtures.
    #[test]
    fn cpp_draw_rank_fixtures() {
        let cases = [
            ("Kh Td 2c 3s", DrawRank52::None),
            ("Ah 2d 3c 4s", DrawRank52::WheelGutshot),
            ("2d 3c 4s 6d", DrawRank52::Gutshot),
            ("Jd Qc Ks Ad", DrawRank52::Gutshot),
            ("2d 3c 4s 5d", DrawRank52::Openend),
            ("Ah 2h Kh 4h", DrawRank52::Flush),
            ("Ah 2h Kh 6h", DrawRank52::Flush),
            ("Ah 2h 3h 4h", DrawRank52::FlushWheelGutshot),
            ("2h 3h 4h 6h", DrawRank52::FlushGutshot),
            ("Jh Qh Kh Ah", DrawRank52::FlushGutshot),
            ("2h 3h 4h 5h", DrawRank52::FlushOpenend),
            ("Th Jh Qh Kh", DrawRank52::FlushOpenend),
        ];
        for (cards, category) in cases {
            assert_eq!(
                DrawingHand52::from_string(cards).unwrap().hand_rank_,
                category,
                "{cards}"
            );
        }
    }

    /// Ports the source's drawing-hand ordering and equality fixtures.
    #[test]
    fn cpp_draw_order_and_equality_fixtures() {
        let ordered = [
            "Kh Td 2c 3s",
            "Ah 2d 3c 4s",
            "2h 3d 4c 6s",
            "Jd Qc Ks Ad",
            "2d 3c 4s 5d",
            "Ah 2h Kh 4h",
            "Ah 2h Kh 6h",
            "Ah 2h 3h 4h",
            "Jh Qh Kh Ah",
            "2h 3h 4h 5h",
            "Th Jh Qh Kh",
        ];
        let draws = ordered
            .into_iter()
            .map(|cards| DrawingHand52::from_string(cards).unwrap())
            .collect::<Vec<_>>();
        assert!(draws.windows(2).all(|pair| pair[0] <= pair[1]));

        let equal_pairs = [
            ("Ah Kc 2h 7s", "Ac Kc 2h 7d"),
            ("Ah 2c 3h 4s", "Ac 2c 3h 4d"),
            ("2c 3h 4s 6h", "2c 3h 4d 6c"),
            ("Ah Kh 3h 4h", "As Ks 3s 4s"),
            ("Ah 2h 3h 4h", "As 2s 3s 4s"),
            ("2h 3h 4h 5h", "2s 3s 4s 5s"),
        ];
        for (left, right) in equal_pairs {
            assert_eq!(
                DrawingHand52::from_string(left).unwrap(),
                DrawingHand52::from_string(right).unwrap()
            );
        }
    }

    /// Verifies the source test's complete made-hand category set.
    #[test]
    fn translated_made_hand_categories() {
        let cases = [
            ("Kh Td 2c 3s 5d", HandRank52::HighCard),
            ("Kh 2h 2c Qs Jd", HandRank52::Pair),
            ("Qh Qd 2c 2s 5d", HandRank52::TwoPair),
            ("Ah 2d 2c 2s 5d", HandRank52::Trips),
            ("Ah 2d 3c 4s 5d", HandRank52::Straight),
            ("Ah 2h Kh 4h 5h", HandRank52::Flush),
            ("5h 2d 2c 2s 5d", HandRank52::FullHouse),
            ("5h 2d 2c 2s 2h", HandRank52::Quads),
            ("Ah 2h 3h 4h 5h", HandRank52::StraightFlush),
        ];
        for (cards, category) in cases {
            let hand = MadeHand52::from_string(cards).expect("valid five-card test hand");
            assert_eq!(hand.get_rank(), category);
        }
    }

    /// Verifies ordering and wheel-straight handling from the C++ tests.
    #[test]
    fn translated_made_hand_ordering_and_wheel() {
        let wheel = MadeHand52::from_string("Ah 2d 3c 4s 5d").expect("valid wheel");
        let six_high = MadeHand52::from_string("2d 3c 4s 5d 6c").expect("valid straight");
        assert!(wheel < six_high);
        assert_eq!(
            MadeHand52::from_string("Ah Kc 2h 7s Td").expect("valid first hand"),
            MadeHand52::from_string("Ac Kc 2h 7d Ts").expect("valid equal hand")
        );
    }

    /// Verifies translated draw categories and safe invalid-size handling.
    #[test]
    fn translated_draw_categories_and_errors() {
        let cases = [
            ("Ah 2d 3c 4s", DrawRank52::WheelGutshot),
            ("2d 3c 4s 6d", DrawRank52::Gutshot),
            ("2d 3c 4s 5d", DrawRank52::Openend),
            ("Ah 2h Kh 4h", DrawRank52::Flush),
            ("Ah 2h 3h 4h", DrawRank52::FlushWheelGutshot),
        ];
        for (cards, category) in cases {
            let draw = DrawingHand52::from_string(cards).expect("valid four-card draw");
            assert_eq!(draw.hand_rank_, category);
        }
        assert_eq!(
            MadeHand52::new(&[]).unwrap_err(),
            HandError::MadeHandCardCount(0)
        );
    }

    /// Verifies draw ordering, suit-insensitive equality, and wheel outs.
    #[test]
    fn translated_draw_order_equality_and_outs() {
        let draws = [
            DrawingHand52::from_string("Kh Td 2c 3s").expect("valid high-card draw"),
            DrawingHand52::from_string("Ah 2d 3c 4s").expect("valid wheel draw"),
            DrawingHand52::from_string("2d 3c 4s 5d").expect("valid open-ended draw"),
        ];
        assert!(draws.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(
            DrawingHand52::from_string("Ah Kc 2h 7s").expect("first equal draw"),
            DrawingHand52::from_string("Ac Kc 2h 7d").expect("second equal draw")
        );
        let mut wheel = DrawingHand52::from_string("Ah 2d 3c 4s").expect("wheel draw");
        assert_eq!(wheel.get_outs().len(), 4);
    }
}
