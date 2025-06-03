
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Deuce,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}

impl Rank {
    pub fn to_string(&self) -> &'static str {
        match self {
            Rank::Deuce => "2",
            Rank::Three => "3",
            Rank::Four => "4",
            Rank::Five => "5",
            Rank::Six => "6",
            Rank::Seven => "7",
            Rank::Eight => "8",
            Rank::Nine => "9",
            Rank::Ten => "T",
            Rank::Jack => "J",
            Rank::Queen => "Q",
            Rank::King => "K",
            Rank::Ace => "A",
        }
    }

    pub fn from_char(c: char) -> Result<Self, String> {
        match c {
            '2' => Ok(Rank::Deuce),
            '3' => Ok(Rank::Three),
            '4' => Ok(Rank::Four),
            '5' => Ok(Rank::Five),
            '6' => Ok(Rank::Six),
            '7' => Ok(Rank::Seven),
            '8' => Ok(Rank::Eight),
            '9' => Ok(Rank::Nine),
            'T' => Ok(Rank::Ten),
            'J' => Ok(Rank::Jack),
            'Q' => Ok(Rank::Queen),
            'K' => Ok(Rank::King),
            'A' => Ok(Rank::Ace),
            _ => Err(format!("Invalid rank input: {}", c)),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Suit {
    Hearts,
    Diamonds,
    Spades,
    Clubs,
}

impl Suit {
    pub fn to_string(&self) -> &'static str {
        match self {
            Suit::Hearts => "h",
            Suit::Diamonds => "d",
            Suit::Spades => "s",
            Suit::Clubs => "c",
        }
    }

    pub fn from_char(c: char) -> Result<Self, String> {
        match c {
            'h' => Ok(Suit::Hearts),
            'd' => Ok(Suit::Diamonds),
            's' => Ok(Suit::Spades),
            'c' => Ok(Suit::Clubs),
            _ => Err(format!("Invalid suit input: {}", c)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    rank: Rank,
    suit: Suit,
}

impl Card {
    pub fn new(rank: Rank, suit: Suit) -> Self {
        Self { rank, suit }
    }

    pub fn from_str(card: &str) -> Result<Self, String> {
        if card.len() != 2 {
            return Err(format!("Invalid card format: {}", card));
        }

        let rank = Rank::from_char(card.chars().next().unwrap())?;
        let suit = Suit::from_char(card.chars().nth(1).unwrap())?;

        Ok(Self::new(rank, suit))
    }

    pub fn from_nb(nb: usize) -> Result<Self, String> {
        if nb > 51 {
            return Err(format!("Card out of range: {}", nb));
        }

        let rank = Rank::from_char("23456789TJQKA".chars().nth(nb % 13).unwrap())?;
        let suit = Suit::from_char("hdsc".chars().nth(nb / 13).unwrap())?;

        Ok(Self::new(rank, suit))
    }

    pub fn rank(&self) -> Rank {
        self.rank
    }

    pub fn suit(&self) -> Suit {
        self.suit
    }

    pub fn to_string(&self) -> String {
        format!("{}{}", self.rank.to_string(), self.suit.to_string())
    }

    pub fn get_all() -> Vec<Self> {
        (0..52).map(|i| Self::from_nb(i).unwrap()).collect()
    }
}