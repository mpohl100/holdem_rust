// board.rs
use std::fmt;

use poker::card::Card;
use poker::holecards::HoleCards;
use poker::hand::{Hand, get_best_hand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
}

impl fmt::Display for Street {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Street::Preflop => "*** HOLE CARDS ***",
            Street::Flop => "*** FLOP ***",
            Street::Turn => "*** TURN ***",
            Street::River => "*** RIVER ***",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Board {
    cards: Vec<Card>,
}

impl Board {
    pub fn new() -> Self {
        Self { cards: Vec::new() }
    }

    pub fn street(&self) -> Street {
        match self.cards.len() {
            0 => Street::Preflop,
            3 => Street::Flop,
            4 => Street::Turn,
            5 => Street::River,
            _ => Street::Preflop,
        }
    }

    pub fn deal_flop(&mut self, flop: Vec<Card>) {
        self.cards = flop;
    }

    pub fn deal_turn(&mut self, turn: Card) {
        self.cards.push(turn);
    }

    pub fn deal_river(&mut self, river: Card) {
        self.cards.push(river);
    }

    pub fn get_cards(&self) -> &Vec<Card> {
        &self.cards
    }

    pub fn get_best_holdings(&self) -> Vec<HoleCards> {
        let mut cards: Vec<Card> = (0..52).map(|i| Card::from_nb(i).unwrap()).collect();
        let mut holdings = Vec::new();

        for i in 0..52 {
            for j in 0..52 {
                if i != j {
                    let h = HoleCards::new(cards[i], cards[j]);
                    let hand = Hand::new(h.clone(), self);
                    holdings.push((h, get_best_hand(&hand)));
                }
            }
        }

        holdings.sort_by(|a, b| b.1.cmp(&a.1)); // descending
        holdings.into_iter().map(|(h, _)| h).collect()
    }
}
