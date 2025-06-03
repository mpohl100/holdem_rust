
use poker::card::Card;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoleCards {
    cards: Vec<Card>,
}

impl HoleCards {
    pub fn new(first: Card, second: Card) -> Self {
        Self { cards: vec![first, second] }
    }

    pub fn to_string(&self) -> String {
        format!("{} {}", self.cards[0].to_string(), self.cards[1].to_string())
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn get_cards(&self) -> Vec<Card> {
        self.cards.clone()
    }

    pub fn get_all() -> Vec<Self> {
        let all_cards = Card::get_all();
        let mut ret = Vec::new();

        for card1 in &all_cards {
            for card2 in &all_cards {
                if card1 != card2 {
                    ret.push(HoleCards::new(card1.clone(), card2.clone()));
                }
            }
        }

        ret
    }
}