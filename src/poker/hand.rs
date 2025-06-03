
use poker::card::Card;
use poker::holecards::HoleCards;
use poker::board::Board;
use poker::made_hand::{get_best_hand, HandRank};

#[derive(Clone)]
pub struct Hand {
    cards: Vec<Card>,
    hole_cards: HoleCards,
    board: Board,
}

impl Hand {
    pub fn new(hole_cards: HoleCards, board: Board) -> Self {
        let mut cards = hole_cards.get_cards();
        let board_cards = board.get_cards();
        cards.extend(board_cards);
        
        Hand {
            cards,
            hole_cards,
            board,
        }
    }

    pub fn get_cards(&self) -> &Vec<Card> {
        &self.cards
    }

    pub fn get_hole_cards(&self) -> &HoleCards {
        &self.hole_cards
    }

    pub fn to_string(&self) -> String {
        let classified = get_best_hand(self);
        match classified.hand_rank {
            HandRank::HighCard => "High Card.".to_string(),
            HandRank::Pair => "a pair.".to_string(),
            HandRank::TwoPair => "two pair.".to_string(),
            HandRank::Trips => "three of a kind.".to_string(),
            HandRank::Straight => "a straight.".to_string(),
            HandRank::Flush => "a flush.".to_string(),
            HandRank::FullHouse => "a full house.".to_string(),
            HandRank::Quads => "four of a kind.".to_string(),
            HandRank::StraightFlush => "a straight flush.".to_string(),
            _ => "".to_string(),
        }
    }
}
