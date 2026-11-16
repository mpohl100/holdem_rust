//! Hand-range containers and preflop-strength entry points.
//!
//! This module maps `cpp_code/src/HandRange.h` and `HandRange.cpp`. The C++
//! strategy string constructor and `toString()` are declared but have no
//! implementation in the source files; their Rust mappings therefore return
//! an explicit [`HandStrategyError`] instead of inventing parsing or display
//! behavior.
//!
//! The C++ range calculations require `Deck52` and `HoleCards::getAll()`,
//! neither of which is part of the stated Rust interfaces. Their behavior is
//! reproduced locally by enumerating rank/suit variants and sampling with a
//! partial Fisher–Yates shuffle; the exact C++ random sequence is therefore
//! unavailable. The `Decision` field uses `crate::player::Decision`, the
//! source mapping of the C++ decision enum.

use crate::card52::{Card52, Rank52, Suit};
use crate::hand::{DrawingHand52, HandError, MadeHand52};
use crate::hole_cards::HoleCards;
use crate::player::Decision;
use crate::rated_hand::RatedHand;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Number of random five-card samples used by the source preflop strength helper.
///
/// C++ mapping: local constant `N` in `getHandStrength()`.
const PRE_FLOP_STRENGTH_SAMPLES: u16 = 1000;

/// The C++ `HandRangeCont<T>::Key`, containing a rank pair and suited flag.
///
/// The derived field order matches C++ defaulted three-way comparison:
/// `suited`, then `rank1`, then `rank2`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct HandRangeKey {
    /// `HandRangeCont<T>::Key::suited`: whether both cards share a suit.
    pub suited: bool,
    /// `HandRangeCont<T>::Key::rank1`: the first source rank.
    pub rank1: Rank52,
    /// `HandRangeCont<T>::Key::rank2`: the second source rank.
    pub rank2: Rank52,
}

impl HandRangeKey {
    /// Constructs a key from two cards, sorting their ranks highest first.
    ///
    /// C++ mapping: `Key(Card52 one, Card52 two)`.
    #[must_use]
    pub fn from_cards(one: Card52, two: Card52) -> Self {
        let mut rank1 = one.rank();
        let mut rank2 = two.rank();
        if rank1 < rank2 {
            std::mem::swap(&mut rank1, &mut rank2);
        }
        Self {
            suited: one.suit() == two.suit(),
            rank1,
            rank2,
        }
    }

    /// Constructs a key from ranks without reordering them.
    ///
    /// C++ mapping: `Key(Rank52 one, Rank52 two, bool suited)`.
    #[must_use]
    pub fn new(rank1: Rank52, rank2: Rank52, suited: bool) -> Self {
        Self {
            suited,
            rank1,
            rank2,
        }
    }
}

impl Default for HandRangeKey {
    /// Returns the safe initialized counterpart of C++'s default key.
    ///
    /// C++ left `rank1` and `rank2` uninitialized; Rust uses `Deuce` for both.
    fn default() -> Self {
        Self {
            suited: false,
            rank1: Rank52::Deuce,
            rank2: Rank52::Deuce,
        }
    }
}

/// A C++-mapped range container keyed by rank/suit class.
///
/// C++ mapping: `HandRangeCont<T>`.
#[derive(Clone, Debug)]
pub struct HandRangeCont<T> {
    /// `HandRangeCont<T>::cont`: the ordered key-to-value map.
    pub cont: BTreeMap<HandRangeKey, T>,
}

impl<T> Default for HandRangeCont<T> {
    /// Creates an empty ordered range map.
    fn default() -> Self {
        Self {
            cont: BTreeMap::new(),
        }
    }
}

/// Error from an unavailable C++ range calculation dependency.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HandRangeError {
    /// A translated hand evaluation failed.
    HandEvaluation(HandError),
    /// System time could not provide a random-sampling seed.
    ClockUnavailable,
}

impl std::fmt::Display for HandRangeError {
    /// Formats the unavailable-dependency error.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HandEvaluation(error) => write!(formatter, "{error}"),
            Self::ClockUnavailable => formatter.write_str("system clock is before the Unix epoch"),
        }
    }
}

impl std::error::Error for HandRangeError {}

impl From<HandError> for HandRangeError {
    /// Wraps a hand-evaluation failure for range calculations.
    fn from(error: HandError) -> Self {
        Self::HandEvaluation(error)
    }
}

/// Error for the C++-declared, but unimplemented, `HandStrategy` string API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandStrategyError;

impl std::fmt::Display for HandStrategyError {
    /// Formats the unavailable strategy-string behavior.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("HandStrategy string parsing/display has no C++ implementation")
    }
}

impl std::error::Error for HandStrategyError {}

/// A strategy's range of holdings and per-holding decisions.
///
/// C++ mapping: `game52::HandStrategy`; both range fields remain private.
#[derive(Clone, Debug)]
pub struct HandStrategy {
    /// `HandStrategy::hands_`: the holdings represented by the strategy.
    _hands: HandRangeCont<Vec<HoleCards>>,
    /// `HandStrategy::decisions_`: a decision mapping for each holding.
    _decisions: HandRangeCont<BTreeMap<HoleCards, Decision>>,
}

impl Default for HandStrategy {
    /// Creates the C++ default strategy with empty ranges.
    fn default() -> Self {
        Self {
            _hands: HandRangeCont::default(),
            _decisions: HandRangeCont::default(),
        }
    }
}

impl HandStrategy {
    /// C++ mapping: `HandStrategy(std::string const&)`.
    ///
    /// The C++ declaration has no definition in `HandRange.cpp`, so its
    /// parsing semantics cannot be translated. The method is retained as an
    /// explicit unavailable operation.
    ///
    /// # Errors
    ///
    /// Always returns [`HandStrategyError`] because the C++ constructor has no
    /// definition in the available sources.
    pub fn from_string(_input: &str) -> Result<Self, HandStrategyError> {
        Err(HandStrategyError)
    }

    /// C++ mapping: `HandStrategy::toString()`.
    ///
    /// The C++ declaration has no definition in the available source files;
    /// this method does not invent a string format.
    ///
    /// # Errors
    ///
    /// Always returns [`HandStrategyError`] because the C++ method has no
    /// definition in the available sources.
    pub fn to_string(&self) -> Result<String, HandStrategyError> {
        Err(HandStrategyError)
    }
}

/// Builds the representative hole-card combination for a range key.
///
/// C++ mapping: translation-unit helper `getRepresentativeHolding()`.
/// `Card52::new` and `HoleCards::new` are assumed to be the idiomatic
/// constructors matching the C++ constructors.
#[must_use]
pub fn get_representative_holding(key: HandRangeKey) -> HoleCards {
    if key.suited {
        HoleCards::new(
            Card52::new(key.rank1, Suit::Hearts),
            Card52::new(key.rank2, Suit::Hearts),
        )
    } else {
        HoleCards::new(
            Card52::new(key.rank1, Suit::Hearts),
            Card52::new(key.rank2, Suit::Diamonds),
        )
    }
}

/// Converts a made-hand category to the source strength value.
///
/// C++ mapping: `deduceHandStrength(MadeHand52 const&)`.
#[must_use]
pub fn deduce_made_hand_strength(made_hand: &MadeHand52) -> f32 {
    f32::from(match made_hand.hand_rank_ {
        crate::hand::HandRank52::HighCard => 0_u8,
        crate::hand::HandRank52::Pair => 1,
        crate::hand::HandRank52::TwoPair => 2,
        crate::hand::HandRank52::Trips => 3,
        crate::hand::HandRank52::Straight => 4,
        crate::hand::HandRank52::Flush => 5,
        crate::hand::HandRank52::FullHouse => 6,
        crate::hand::HandRank52::Quads => 7,
        crate::hand::HandRank52::StraightFlush => 8,
    })
}

/// Converts a drawing-hand category to the source strength value.
///
/// C++ mapping: `deduceHandStrength(DrawingHand52 const&)`.
#[must_use]
pub fn deduce_drawing_hand_strength(drawing_hand: &DrawingHand52) -> f32 {
    f32::from(match drawing_hand.hand_rank_ {
        crate::hand::DrawRank52::None => 0_u8,
        crate::hand::DrawRank52::WheelGutshot => 1,
        crate::hand::DrawRank52::Gutshot => 2,
        crate::hand::DrawRank52::Openend => 3,
        crate::hand::DrawRank52::Flush => 4,
        crate::hand::DrawRank52::FlushWheelGutshot => 5,
        crate::hand::DrawRank52::FlushGutshot => 6,
        crate::hand::DrawRank52::FlushOpenend => 7,
    })
}

/// Estimates the strength of a preflop rank/suit class from five-card samples.
///
/// C++ mapping: translation-unit helper `getHandStrength()`. Deck ordering and
/// partial shuffling are implemented locally because the Rust `Deck52` module
/// is not among the available dependencies.
///
/// # Errors
///
/// Returns an error if the system clock cannot provide a seed or a sampled
/// hand/draw cannot be evaluated.
pub fn get_hand_strength(key: HandRangeKey) -> Result<f32, HandRangeError> {
    let hole_cards = get_representative_holding(key);
    let remaining = range_all_cards()
        .into_iter()
        .filter(|card| !hole_cards.get_cards().contains(card))
        .collect::<Vec<_>>();
    let mut random_state = random_seed()?;
    let mut strength = 0.0;
    for _ in 0..PRE_FLOP_STRENGTH_SAMPLES {
        // Deck52::getRange(5) starts at cards_.begin(), even after
        // moveToFront() advances position_, so source samples contain both
        // representative hole cards plus three shuffled remaining cards.
        let mut sample = hole_cards.get_cards().to_vec();
        sample.extend(sample_without_replacement(&remaining, 3, &mut random_state));
        let made_hand = MadeHand52::new(&sample)?;
        // The source passes the same five-card range to DrawingHand52, whose
        // constructor rejects every size other than four. Preserve this
        // deterministic source error rather than silently dropping a card.
        let drawing_hand = DrawingHand52::new(&sample)?;
        strength += deduce_made_hand_strength(&made_hand);
        strength += deduce_drawing_hand_strength(&drawing_hand);
    }
    Ok(strength / f32::from(PRE_FLOP_STRENGTH_SAMPLES))
}

/// Calculates all preflop rank/suit strength entries.
///
/// C++ mapping: translation-unit helper `getPreflopStrengths()`.
///
/// # Errors
///
/// Returns an error if any sampled hand cannot be evaluated or the system
/// clock is unavailable.
pub fn get_preflop_strengths() -> Result<HandRangeCont<f32>, HandRangeError> {
    let mut strengths = HandRangeCont::default();
    let ranks = range_all_ranks();
    for (first_index, rank1) in ranks.iter().enumerate() {
        for rank2 in ranks.iter().skip(first_index) {
            for suited in [true, false] {
                if suited && rank1 == rank2 {
                    continue;
                }
                let key = HandRangeKey::new(*rank2, *rank1, suited);
                strengths.cont.insert(key, get_hand_strength(key)?);
            }
        }
    }
    Ok(strengths)
}

/// Prints preflop strengths in descending order.
///
/// C++ mapping: `printPreflopStrengths()`.
///
/// # Errors
///
/// Returns an error if preflop sampling or hand evaluation fails.
pub fn print_preflop_strengths() -> Result<(), HandRangeError> {
    let strengths = get_preflop_strengths()?;
    let mut flattened = strengths.cont.into_iter().collect::<Vec<_>>();
    flattened.sort_by(|left, right| right.1.total_cmp(&left.1));
    for (key, strength) in flattened {
        println!("{} {strength}", get_representative_holding(key));
    }
    Ok(())
}

/// Calculates and prints simulated preflop scores for all hole-card holdings.
///
/// C++ mapping: `calculatePreflopStrengths(int nbHands)`. `nb_hands` sets the
/// number of randomly sampled preflop boards per holding. Hole-card
/// enumeration is reproduced in card-number order without `Card52::nb()`.
///
/// # Errors
///
/// Returns an error if a simulated rating cannot be evaluated or the system
/// clock is unavailable.
pub fn calculate_preflop_strengths(nb_hands: i32) -> Result<(), HandRangeError> {
    let cards = range_all_cards();
    let board = crate::board::Board::default();
    let mut results = Vec::with_capacity(cards.len() * (cards.len() - 1));
    for first in &cards {
        for second in &cards {
            if first == second {
                continue;
            }
            let holding = HoleCards::new(*first, *second);
            let rating = RatedHand::with_preflop_board_samples(&holding, &board, nb_hands as usize);
            results.push((rating.get_simulated_score()?, holding));
        }
    }
    results.sort_by(|left, right| left.0.total_cmp(&right.0));
    for (score, holding) in results.into_iter().rev() {
        println!("Holding: {holding}: score {score}");
    }
    Ok(())
}

/// Constructs the standard deck in the C++ `Card52::getAll()` order.
fn range_all_cards() -> Vec<Card52> {
    [Suit::Hearts, Suit::Diamonds, Suit::Spades, Suit::Clubs]
        .into_iter()
        .flat_map(|suit| {
            range_all_ranks()
                .into_iter()
                .map(move |rank| Card52::new(rank, suit))
        })
        .collect()
}

/// Returns rank values in C++ enum order.
fn range_all_ranks() -> [Rank52; 13] {
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

/// Seeds a local generator for independent source-style shuffle samples.
fn random_seed() -> Result<u64, HandRangeError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| HandRangeError::ClockUnavailable)?
        .as_nanos();
    let seed = u64::try_from(timestamp & u128::from(u64::MAX)).unwrap_or_default();
    Ok(if seed == 0 {
        0x9e37_79b9_7f4a_7c15
    } else {
        seed
    })
}

/// Samples a fixed-size prefix using a partial Fisher–Yates shuffle.
fn sample_without_replacement(cards: &[Card52], count: usize, state: &mut u64) -> Vec<Card52> {
    let mut shuffled = cards.to_vec();
    for index in 0..count {
        let remaining = shuffled.len() - index;
        let remaining_u64 = u64::try_from(remaining).unwrap_or(u64::MAX);
        let pointer_width_mask = u64::MAX >> u64::BITS.saturating_sub(usize::BITS);
        let source_index = next_random(state) & pointer_width_mask;
        let offset = usize::try_from(source_index % remaining_u64).unwrap_or_default();
        let selected = index + offset;
        shuffled.swap(index, selected);
    }
    shuffled.truncate(count);
    shuffled
}

/// Advances the xorshift64* generator used for local deck shuffles.
fn next_random(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    (*state).wrapping_mul(0x2545_f491_4f6c_dd1d)
}

#[cfg(test)]
mod tests {
    use super::{HandRangeError, HandRangeKey, HandStrategy, HandStrategyError, get_hand_strength};
    use crate::card52::{Card52, Rank52, Suit};
    use crate::hand::HandError;

    /// Checks rank normalization for keys constructed from physical cards.
    #[test]
    fn card_keys_sort_ranks_and_preserve_suitedness() {
        let key = HandRangeKey::from_cards(
            Card52::new(Rank52::Deuce, Suit::Hearts),
            Card52::new(Rank52::Ace, Suit::Hearts),
        );
        assert!(key.suited);
        assert_eq!(key.rank1, Rank52::Ace);
        assert_eq!(key.rank2, Rank52::Deuce);
    }

    /// Confirms no invented `HandStrategy` text behavior is exposed.
    #[test]
    fn strategy_string_operations_are_explicitly_unavailable() {
        assert_eq!(
            HandStrategy::from_string("AKs").unwrap_err(),
            HandStrategyError
        );
        assert_eq!(
            HandStrategy::default().to_string().unwrap_err(),
            HandStrategyError
        );
    }

    /// Preserves the C++ bug passing five cards to a four-card draw constructor.
    #[test]
    fn get_hand_strength_reports_source_draw_range_size_error() {
        let key = HandRangeKey::new(Rank52::Ace, Rank52::King, true);
        assert_eq!(
            get_hand_strength(key).unwrap_err(),
            HandRangeError::HandEvaluation(HandError::DrawingHandCardCount(5))
        );
    }
}
