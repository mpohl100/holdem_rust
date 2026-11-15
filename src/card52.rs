//! Card, rank, and suit definitions translated from `Card52.h` and `Card52.cpp`.

use std::fmt;
use std::str::FromStr;

/// A playing-card rank in C++ `Rank52` declaration order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
#[repr(u8)]
pub enum Rank52 {
    /// Two.
    Deuce,
    /// Three.
    Three,
    /// Four.
    Four,
    /// Five.
    Five,
    /// Six.
    Six,
    /// Seven.
    Seven,
    /// Eight.
    Eight,
    /// Nine.
    Nine,
    /// Ten.
    Ten,
    /// Jack.
    Jack,
    /// Queen.
    Queen,
    /// King.
    King,
    /// Ace.
    Ace,
}

impl Rank52 {
    /// Returns the rank's zero-based C++ numeric value.
    #[must_use]
    pub const fn value(self) -> u8 {
        self as u8
    }

    /// Returns the one-character rank spelling used by `Card52::toString`.
    #[must_use]
    pub const fn symbol(self) -> char {
        match self {
            Self::Deuce => '2',
            Self::Three => '3',
            Self::Four => '4',
            Self::Five => '5',
            Self::Six => '6',
            Self::Seven => '7',
            Self::Eight => '8',
            Self::Nine => '9',
            Self::Ten => 'T',
            Self::Jack => 'J',
            Self::Queen => 'Q',
            Self::King => 'K',
            Self::Ace => 'A',
        }
    }
}

impl fmt::Display for Rank52 {
    /// Formats the rank with its source single-character symbol.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Deuce => "2",
            Self::Three => "3",
            Self::Four => "4",
            Self::Five => "5",
            Self::Six => "6",
            Self::Seven => "7",
            Self::Eight => "8",
            Self::Nine => "9",
            Self::Ten => "T",
            Self::Jack => "J",
            Self::Queen => "Q",
            Self::King => "K",
            Self::Ace => "A",
        })
    }
}

/// A playing-card suit in C++ `Suit` declaration order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
#[repr(u8)]
pub enum Suit {
    /// Hearts.
    Hearts,
    /// Diamonds.
    Diamonds,
    /// Spades.
    Spades,
    /// Clubs.
    Clubs,
}

impl Suit {
    /// Returns the suit's zero-based C++ numeric value.
    #[must_use]
    pub const fn value(self) -> u8 {
        self as u8
    }

    /// Returns the one-character suit spelling used by `Card52::toString`.
    #[must_use]
    pub const fn symbol(self) -> char {
        match self {
            Self::Hearts => 'h',
            Self::Diamonds => 'd',
            Self::Spades => 's',
            Self::Clubs => 'c',
        }
    }
}

impl fmt::Display for Suit {
    /// Formats the suit with its source single-character symbol.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Hearts => "h",
            Self::Diamonds => "d",
            Self::Spades => "s",
            Self::Clubs => "c",
        })
    }
}

/// Error returned when a card string or numeric index is invalid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardParseError {
    /// The string has fewer than two bytes.
    InvalidLength(usize),
    /// The card string contains an unrecognized rank character.
    InvalidRank(char),
    /// The card string contains an unrecognized suit character.
    InvalidSuit(char),
    /// The numeric card index is outside the C++ valid range 0 through 51.
    IndexOutOfRange(i32),
}

impl fmt::Display for CardParseError {
    /// Formats a card parsing or numeric-index error.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength(length) => {
                write!(
                    formatter,
                    "card text must contain two characters, got {length}"
                )
            }
            Self::InvalidRank(rank) => write!(formatter, "wrong input rank: {rank}"),
            Self::InvalidSuit(suit) => write!(formatter, "wrong input suit: {suit}"),
            Self::IndexOutOfRange(index) => write!(formatter, "Card52 out of range: {index}"),
        }
    }
}

impl std::error::Error for CardParseError {}

/// Signals that the source declared an operation without defining it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceDefinitionUnavailable;

impl fmt::Display for SourceDefinitionUnavailable {
    /// Describes the missing C++ definition without guessing its behavior.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the corresponding C++ declaration has no definition")
    }
}

impl std::error::Error for SourceDefinitionUnavailable {}

/// A standard-deck playing card.
///
/// C++ mapping: `game52::Card52`; private `rank_` and `suit_` remain private.
/// The C++ default constructor leaves both members uninitialized, so Rust
/// intentionally provides no `Default` implementation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct Card52 {
    /// C++ private member `rank_`.
    rank_: Rank52,
    /// C++ private member `suit_`.
    suit_: Suit,
}

impl Card52 {
    /// Constructs a card from its rank and suit.
    ///
    /// C++ mapping: `Card52(Rank52, Suit)`.
    #[must_use]
    pub const fn new(rank: Rank52, suit: Suit) -> Self {
        Self {
            rank_: rank,
            suit_: suit,
        }
    }

    /// Constructs the card at a C++ numeric deck index from 0 through 51.
    ///
    /// C++ mapping: `Card52(int)`, where rank is `index % 13` and suit is
    /// `index / 13`. Invalid indices return an error rather than throwing.
    ///
    /// # Errors
    ///
    /// Returns [`CardParseError::IndexOutOfRange`] unless `index` is in
    /// `0..52`.
    pub fn from_index(index: i32) -> Result<Self, CardParseError> {
        if !(0..52).contains(&index) {
            return Err(CardParseError::IndexOutOfRange(index));
        }
        let rank_index =
            u8::try_from(index % 13).map_err(|_| CardParseError::IndexOutOfRange(index))?;
        let suit_index =
            u8::try_from(index / 13).map_err(|_| CardParseError::IndexOutOfRange(index))?;
        let rank = rank_from_value(rank_index);
        let suit = suit_from_value(suit_index);
        match (rank, suit) {
            (Some(rank), Some(suit)) => Ok(Self::new(rank, suit)),
            _ => Err(CardParseError::IndexOutOfRange(index)),
        }
    }

    /// Returns this card's rank.
    ///
    /// C++ mapping: `Card52::rank()`.
    #[must_use]
    pub const fn rank(self) -> Rank52 {
        self.rank_
    }

    /// Returns this card's suit.
    ///
    /// C++ mapping: `Card52::suit()`.
    #[must_use]
    pub const fn suit(self) -> Suit {
        self.suit_
    }

    /// Returns the integer card index requested by C++ `Card52::nb()`.
    ///
    /// The C++ declaration has no definition in `Card52.cpp` and has no
    /// observed call sites. Its apparent inverse of the integer constructor
    /// is not treated as specified behavior; this method reports that absence.
    ///
    /// # Errors
    ///
    /// Always returns [`SourceDefinitionUnavailable`] because no source
    /// definition specifies the requested index.
    pub fn nb(self) -> Result<i32, SourceDefinitionUnavailable> {
        Err(SourceDefinitionUnavailable)
    }

    /// Returns all 52 cards in the C++ integer-constructor order.
    ///
    /// C++ mapping: `Card52::getAll()`.
    ///
    /// # Panics
    ///
    /// This panics only if an index in the fixed `0..52` standard-deck range
    /// is unexpectedly rejected by [`Card52::from_index`].
    #[must_use]
    pub fn get_all() -> Vec<Self> {
        (0..52)
            .map(|index| Self::from_index(index).expect("index is in the standard deck"))
            .collect()
    }
}

impl fmt::Display for Card52 {
    /// Formats the card as its two-character rank and suit spelling.
    ///
    /// This `Display` implementation maps C++ `Card52::toString()`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.rank_, self.suit_)
    }
}

impl FromStr for Card52 {
    type Err = CardParseError;

    /// Parses a spelling using the first two bytes, as C++ `Card52(string)` does.
    ///
    /// Extra bytes are ignored by the C++ constructor and remain ignored here.
    /// A string shorter than two bytes is rejected rather than indexing outside
    /// the source string's bounds.
    ///
    /// # Errors
    ///
    /// Returns an error when the input is shorter than two bytes, its rank
    /// symbol is not recognized, or its suit symbol is not recognized.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let bytes = input.as_bytes();
        if bytes.len() < 2 {
            return Err(CardParseError::InvalidLength(input.len()));
        }
        let rank = get_rank(char::from(bytes[0]))?;
        let suit = get_suit(char::from(bytes[1]))?;
        Ok(Self::new(rank, suit))
    }
}

/// Maps a C++ rank character to its `Rank52` variant.
///
/// # Errors
///
/// Returns [`CardParseError::InvalidRank`] for an unrecognized character.
pub fn get_rank(character: char) -> Result<Rank52, CardParseError> {
    match character {
        '2' => Ok(Rank52::Deuce),
        '3' => Ok(Rank52::Three),
        '4' => Ok(Rank52::Four),
        '5' => Ok(Rank52::Five),
        '6' => Ok(Rank52::Six),
        '7' => Ok(Rank52::Seven),
        '8' => Ok(Rank52::Eight),
        '9' => Ok(Rank52::Nine),
        'T' => Ok(Rank52::Ten),
        'J' => Ok(Rank52::Jack),
        'Q' => Ok(Rank52::Queen),
        'K' => Ok(Rank52::King),
        'A' => Ok(Rank52::Ace),
        other => Err(CardParseError::InvalidRank(other)),
    }
}

/// Maps a C++ suit character to its `Suit` variant.
///
/// # Errors
///
/// Returns [`CardParseError::InvalidSuit`] for an unrecognized character.
pub fn get_suit(character: char) -> Result<Suit, CardParseError> {
    match character {
        'h' => Ok(Suit::Hearts),
        'd' => Ok(Suit::Diamonds),
        's' => Ok(Suit::Spades),
        'c' => Ok(Suit::Clubs),
        other => Err(CardParseError::InvalidSuit(other)),
    }
}

/// Formats a rank as the C++ free `to_string(Rank52)` function does.
#[must_use]
pub fn rank_to_string(rank: Rank52) -> String {
    rank.to_string()
}

/// Formats a suit as the C++ free `to_string(Suit)` function does.
#[must_use]
pub fn suit_to_string(suit: Suit) -> String {
    suit.to_string()
}

/// Converts a zero-based source rank value to its enum variant.
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

/// Converts a zero-based source suit value to its enum variant.
fn suit_from_value(value: u8) -> Option<Suit> {
    match value {
        0 => Some(Suit::Hearts),
        1 => Some(Suit::Diamonds),
        2 => Some(Suit::Spades),
        3 => Some(Suit::Clubs),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Card52, CardParseError, Rank52, Suit};
    use std::str::FromStr;

    /// Checks the enum and string constructors represented by `CardTests.cpp`.
    #[test]
    fn cards_from_enums_and_strings() {
        let card = Card52::new(Rank52::Deuce, Suit::Hearts);
        assert_eq!(card.rank(), Rank52::Deuce);
        assert_eq!(card.suit(), Suit::Hearts);
        assert_eq!(Card52::from_str("2s").unwrap().to_string(), "2s");
    }

    /// Checks numeric-deck ordering and safe invalid-input errors.
    #[test]
    fn all_cards_and_invalid_inputs() {
        let cards = Card52::get_all();
        assert_eq!(cards.len(), 52);
        assert_eq!(cards[0].to_string(), "2h");
        assert_eq!(cards[51].to_string(), "Ac");
        assert_eq!(
            Card52::from_str("2").unwrap_err(),
            CardParseError::InvalidLength(1)
        );
        assert_eq!(Card52::from_str("2h trailing").unwrap().to_string(), "2h");
        assert!(Card52::from_index(52).is_err());
    }
}
