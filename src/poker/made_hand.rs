// This is a partial Rust translation of the C++ MadeHand class and logic
// You will need the following types to be defined elsewhere:
// Card, Rank, Suit, Hand, for_each_combination, etc.

use std::collections::HashMap;
use std::cmp::Ordering;

use poker::card::Card;
use poker::card::Rank;

#[derive(Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug)]
pub enum HandRank {
    HighCard,
    Pair,
    TwoPair,
    Trips,
    Straight,
    Flush,
    FullHouse,
    Quads,
    StraightFlush,
}

#[derive(Clone)]
pub struct MadeHand {
    pub cards: Vec<Card>,
    pub hand_rank: HandRank,
    pub rank_occurences: HashMap<Rank, usize>,
}

impl MadeHand {
    pub fn new(cards: Vec<Card>) -> Self {
        if cards.len() != 5 {
            panic!("classified hand != size 5");
        }

        let mut rank_occurences = HashMap::new();
        for card in &cards {
            *rank_occurences.entry(card.rank()).or_insert(0) += 1;
        }

        let mut hand_rank = HandRank::HighCard;
        if rank_occurences.len() == 5 {
            let mut suit_counts = HashMap::new();
            for card in &cards {
                *suit_counts.entry(card.suit()).or_insert(0) += 1;
            }
            let is_flush = suit_counts.len() == 1;

            let mut ranks: Vec<_> = cards.iter().map(|c| c.rank()).collect();
            ranks.sort();
            ranks.dedup();

            let is_wheel = ranks == vec![Rank::Ace, Rank::Two, Rank::Three, Rank::Four, Rank::Five];
            let is_straight = ranks.last().unwrap() as u8 - ranks[0] as u8 == 4 && ranks.len() == 5;

            hand_rank = match (is_flush, is_straight || is_wheel) {
                (true, true) => HandRank::StraightFlush,
                (true, false) => HandRank::Flush,
                (false, true) => HandRank::Straight,
                _ => HandRank::HighCard,
            };
        } else {
            let counts: Vec<_> = rank_occurences.values().cloned().collect();
            let max_count = *counts.iter().max().unwrap();
            hand_rank = match (rank_occurences.len(), max_count) {
                (4, _) => HandRank::Pair,
                (3, 3) => HandRank::Trips,
                (3, _) => HandRank::TwoPair,
                (2, 4) => HandRank::Quads,
                (2, _) => HandRank::FullHouse,
                _ => HandRank::HighCard,
            }
        }

        Self {
            cards,
            hand_rank,
            rank_occurences,
        }
    }

    pub fn find_occurences(&self, nb: usize) -> [Rank; 2] {
        let mut found = [Rank::Deuce; 2];
        let mut index = 0;
        for (&rank, &count) in &self.rank_occurences {
            if count == nb {
                found[index] = rank;
                index += 1;
                if index == 2 {
                    break;
                }
            }
        }
        if found[0] < found[1] {
            found.swap(0, 1);
        }
        found
    }

    pub fn get_high_cards(&self) -> Vec<Rank> {
        let mut highs: Vec<_> = self.rank_occurences
            .iter()
            .filter_map(|(&rank, &cnt)| if cnt == 1 { Some(rank) } else { None })
            .collect();
        highs.sort_by(|a, b| b.cmp(a));
        highs
    }

    pub fn sum(&self) -> usize {
        self.cards.iter().map(|c| c.rank() as usize).sum()
    }

    pub fn get_relevant_card(&self) -> Rank {
        match self.hand_rank {
            HandRank::HighCard => Rank::Deuce,
            HandRank::Pair | HandRank::TwoPair => self.find_occurences(2)[0],
            HandRank::Trips | HandRank::FullHouse => self.find_occurences(3)[0],
            HandRank::Quads => self.find_occurences(4)[0],
            HandRank::Straight | HandRank::Flush | HandRank::StraightFlush => self.get_high_cards()[0],
        }
    }

    pub fn get_kicker(&self) -> Rank {
        self.get_high_cards()[0]
    }

    pub fn get_rank(&self) -> HandRank {
        self.hand_rank.clone()
    }

    pub fn get_cards(&self) -> &Vec<Card> {
        &self.cards
    }

    pub fn from_string(s: &str) -> Self {
        let cards: Vec<_> = s
            .split_whitespace()
            .map(|token| Card::from_str(token).unwrap())
            .collect();
        MadeHand::new(cards)
    }
}

// Comparison helpers
fn compare_high_cards(a: &[Rank], b: &[Rank]) -> Ordering {
    a.iter().cmp(b.iter())
}

impl PartialOrd for MadeHand {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MadeHand {
    fn cmp(&self, other: &Self) -> Ordering {
        use HandRank::*;
        if self.hand_rank != other.hand_rank {
            return self.hand_rank.cmp(&other.hand_rank);
        }

        match self.hand_rank {
            HighCard | Flush => compare_high_cards(&self.get_high_cards(), &other.get_high_cards()),
            Pair | TwoPair | Trips | FullHouse | Quads => {
                let a = self.find_occurences(match self.hand_rank {
                    Pair | TwoPair => 2,
                    Trips | FullHouse => 3,
                    Quads => 4,
                    _ => 0,
                });
                let b = other.find_occurences(match self.hand_rank {
                    Pair | TwoPair => 2,
                    Trips | FullHouse => 3,
                    Quads => 4,
                    _ => 0,
                });
                a.cmp(&b).then(compare_high_cards(&self.get_high_cards(), &other.get_high_cards()))
            }
            Straight | StraightFlush => {
                let sum_a = self.sum() - if self.get_high_cards()[0] == Rank::Ace { 13 } else { 0 };
                let sum_b = other.sum() - if other.get_high_cards()[0] == Rank::Ace { 13 } else { 0 };
                sum_a.cmp(&sum_b)
            }
        }
    }
}

pub fn get_best_hand(hand: &Hand) -> MadeHand {
    let cards = hand.get_cards();
    let mut best = MadeHand::new(cards[..5].to_vec());
    for_each_combination(&cards, 5, |combo| {
        let candidate = MadeHand::new(combo.to_vec());
        if best < candidate {
            best = candidate;
        }
    });
    best
}