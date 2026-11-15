//! Made-hand and draw scoring, plus board-completion evaluation.
//!
//! This module maps `cpp_code/src/RatedHand.h` and `RatedHand.cpp`.
//! `RatedHand::getBoards()` and both score helpers remain private; the public
//! `getScore()` and `getSimulatedScore()` methods map to `get_score` and
//! `get_simulated_score`. Rust methods return [`HandError`] for input states
//! that made the C++ implementation throw or rely on invalid iterator/output
//! ranges.
//!
//! The C++ `Deck52` module is not present in the Rust checkout. This
//! translation constructs the standard deck from the declared rank/suit
//! variants. Since the C++ code samples 1,000 random preflop boards, while no
//! random/deck API is available here, preflop uses 1,000 independent
//! partial-shuffle samples with replacement. This preserves the source's
//! sampling model, but its random seed/sequence cannot be source-identical.

use crate::board::Board;
use crate::card52::{Card52, Rank52, Suit};
use crate::combinatorics;
use crate::hand::{AllDraws, Hand, HandError, HandRank52, try_get_best_hand};
use crate::hole_cards::HoleCards;
use std::time::{SystemTime, UNIX_EPOCH};

/// A hole-card holding evaluated against a board.
///
/// C++ mapping: `game52::RatedHand`; its three private data members
/// `holeCards_`, `board_`, and `nbPreflopBoards_` map to private Rust fields
/// `hole_cards`, `board`, and `preflop_board_samples`.
#[derive(Clone, Debug)]
pub struct RatedHand {
    /// `RatedHand::holeCards_`: the player's private cards.
    hole_cards: HoleCards,
    /// `RatedHand::board_`: the current community board.
    board: Board,
    /// `RatedHand::nbPreflopBoards_`: number of preflop completions to sample.
    preflop_board_samples: usize,
}

impl Default for RatedHand {
    /// Creates the C++ default rating state, including its 1,000-board sample size.
    fn default() -> Self {
        Self {
            hole_cards: HoleCards::default(),
            board: Board::default(),
            preflop_board_samples: 1000,
        }
    }
}

impl RatedHand {
    /// Creates a rating request for the supplied hole cards and board.
    ///
    /// C++ mapping: `RatedHand(HoleCards const&, Board const&)`.
    #[must_use]
    pub fn new(hole_cards: &HoleCards, board: &Board) -> Self {
        Self {
            hole_cards: hole_cards.clone(),
            board: board.clone(),
            preflop_board_samples: 1000,
        }
    }

    /// Returns the sum of the made-hand and draw scores.
    ///
    /// C++ mapping: `RatedHand::getScore()`.
    ///
    /// # Errors
    ///
    /// Returns an error if made-hand or draw evaluation fails for the current
    /// hole cards and board.
    pub fn get_score(&self) -> Result<f32, HandError> {
        Ok(self.get_made_hand_score()? + self.get_drawing_hand_score()?)
    }

    /// Returns the average simulated made-hand score over completed boards.
    ///
    /// C++ mapping: `RatedHand::getSimulatedScore()`. The C++ code attempted
    /// `set_intersection` into an empty vector, which is invalid; this safe
    /// translation computes the intersection by membership and sorts ranks
    /// descending before applying the same one-card/two-card kicker formula.
    ///
    /// # Errors
    ///
    /// Returns an error if the board is incomplete/invalid, no completion
    /// boards can be generated, or made-hand evaluation fails.
    ///
    /// # Panics
    ///
    /// Panics if the generated board count exceeds `u32::MAX`, which is
    /// required for conversion to the floating-point denominator.
    pub fn get_simulated_score(&self) -> Result<f64, HandError> {
        let boards = self.get_boards()?;
        if boards.is_empty() {
            return Err(HandError::NoCompletionBoards);
        }
        let mut sum = 0.0;
        for board in &boards {
            let hand = Hand::new(self.hole_cards.clone(), board.clone());
            let made_hand = try_get_best_hand(&hand)?;
            let mut score = f64::from(made_hand.get_rank() as i32);
            let hole_cards = self.hole_cards.get_cards();
            let mut intersection: Vec<Rank52> = made_hand
                .get_cards()
                .iter()
                .filter(|card| hole_cards.contains(card))
                .map(|card| card.rank())
                .collect();
            intersection.sort_unstable_by(|left, right| right.cmp(left));
            match intersection.as_slice() {
                [rank] => score += f64::from(rank_value(*rank)) / 13.0 / 13.0,
                [high, low] => {
                    score += f64::from(rank_value(*high)) / 13.0
                        + f64::from(rank_value(*low)) / 13.0 / 13.0;
                }
                _ => {}
            }
            sum += score;
        }
        let board_count = u32::try_from(boards.len()).expect("the number of boards fits in u32");
        Ok(sum / f64::from(board_count))
    }

    /// Generates all completion boards for flop/turn/river, and the configured
    /// number of sampled boards preflop.
    ///
    /// C++ mapping: private `RatedHand::getBoards()`.
    fn get_boards(&self) -> Result<Vec<Board>, HandError> {
        let board_cards = self.board.get_cards();
        if board_cards.len() == 5 {
            return Ok(vec![self.board.clone()]);
        }
        if !matches!(board_cards.len(), 0 | 3 | 4) {
            return Err(HandError::InvalidBoardCardCount(board_cards.len()));
        }

        let mut remaining = all_cards()
            .into_iter()
            .filter(|card| {
                !self.hole_cards.get_cards().contains(card) && !board_cards.contains(card)
            })
            .collect::<Vec<_>>();
        let cards_needed = 5 - board_cards.len();
        let mut completed_boards = Vec::new();
        if board_cards.is_empty() {
            let sample_size = self.preflop_board_samples;
            let mut state = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| {
                    u64::try_from(duration.as_nanos() & u128::from(u64::MAX))
                        .expect("masking to 64 bits guarantees the seed fits")
                })
                .map_err(|_| HandError::ClockUnavailable)?;
            // Do not allow the xorshift state to become the absorbing zero.
            if state == 0 {
                state = 0x9e37_79b9_7f4a_7c15;
            }
            for _ in 0..sample_size {
                let mut shuffled = remaining.clone();
                for index in 0..cards_needed {
                    let tail_length = shuffled.len() - index;
                    let tail_length_u64 =
                        u64::try_from(tail_length).expect("deck size fits in u64");
                    let pointer_width_mask = u64::MAX >> u64::BITS.saturating_sub(usize::BITS);
                    let source_index = next_random(&mut state) & pointer_width_mask;
                    let offset =
                        usize::try_from(source_index % tail_length_u64).unwrap_or_default();
                    let selected_index = index + offset;
                    shuffled.swap(index, selected_index);
                }
                completed_boards.push(complete_board(&self.board, &shuffled[..cards_needed])?);
            }
        } else {
            let mut callback_error = None;
            let _callback =
                combinatorics::for_each_combination(&mut remaining, cards_needed, |selection| {
                    match complete_board(&self.board, selection) {
                        Ok(board) => completed_boards.push(board),
                        Err(error) => callback_error = Some(error),
                    }
                    false
                });
            if let Some(error) = callback_error {
                return Err(error);
            }
        }
        Ok(completed_boards)
    }

    /// Computes the score for the best made hand on the current board.
    ///
    /// C++ mapping: private `RatedHand::getMadeHandScore()`.
    fn get_made_hand_score(&self) -> Result<f32, HandError> {
        let hand = Hand::new(self.hole_cards.clone(), self.board.clone());
        let made_hand = try_get_best_hand(&hand)?;
        let relevant_card = made_hand.get_relevant_card()?;
        let kicker = made_hand.get_kicker()?;
        let hand_rank = f32::from(
            i16::try_from(made_hand.hand_rank_ as i32).expect("hand-rank ordinals fit in i16"),
        );
        let relevant_rank =
            f32::from(i16::try_from(rank_value(relevant_card)).expect("rank values fit in i16"));
        let kicker_rank =
            f32::from(i16::try_from(rank_value(kicker)).expect("rank values fit in i16"));
        Ok(13.0 * 13.0 * hand_rank + 13.0 * relevant_rank + kicker_rank)
    }

    /// Computes the source top-pair draw score, truncated to an integer value.
    ///
    /// C++ mapping: private `RatedHand::getDrawingHandScore()`.
    fn get_drawing_hand_score(&self) -> Result<f32, HandError> {
        let hand = Hand::new(self.hole_cards.clone(), self.board.clone());
        let all_draws = AllDraws::new(&hand)?;
        let top_pair = rank_value(Rank52::Ten) * 13
            + rank_value(Rank52::Jack)
            + 13 * 13 * HandRank52::Pair as i32;
        let outs = all_draws.get_outs();
        let truncated_score = top_pair * outs / 9;
        Ok(f32::from(
            i16::try_from(truncated_score).expect("bounded poker score fits in i16"),
        ))
    }
}

/// Constructs the standard 52-card deck in C++ card-number order.
///
/// `Card52::nb()` is intentionally not used: that method is not part of the
/// stated Rust dependency interface.
fn all_cards() -> Vec<Card52> {
    all_suits()
        .into_iter()
        .flat_map(|suit| {
            all_ranks()
                .into_iter()
                .map(move |rank| Card52::new(rank, suit))
        })
        .collect()
}

/// Completes a board using cards from one selected combination.
fn complete_board(board: &Board, selected: &[Card52]) -> Result<Board, HandError> {
    let mut completed = board.clone();
    match board.get_cards().len() {
        0 if selected.len() == 5 => {
            completed.deal_flop(selected[..3].to_vec());
            completed.deal_turn(selected[3]);
            completed.deal_river(selected[4]);
        }
        3 if selected.len() == 2 => {
            completed.deal_turn(selected[0]);
            completed.deal_river(selected[1]);
        }
        4 if selected.len() == 1 => completed.deal_river(selected[0]),
        count => return Err(HandError::InvalidBoardCardCount(count)),
    }
    Ok(completed)
}

/// Advances the local xorshift64* generator used for reservoir sampling.
fn next_random(state: &mut u64) -> u64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    (*state).wrapping_mul(0x2545_f491_4f6c_dd1d)
}

/// Converts a zero-based rank enum discriminant to the C++ integer value.
fn rank_value(rank: Rank52) -> i32 {
    rank as i32
}

/// Returns rank variants in their C++ declaration order.
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

/// Returns suit variants in their C++ declaration order.
fn all_suits() -> [Suit; 4] {
    [Suit::Hearts, Suit::Diamonds, Suit::Spades, Suit::Clubs]
}

#[cfg(test)]
mod tests {
    use super::RatedHand;
    use crate::board::Board;
    use crate::card52::{Card52, Rank52, Suit};
    use crate::hole_cards::HoleCards;

    /// Verifies the source score path on a complete board.
    #[test]
    fn simulated_score_is_finite_on_river() {
        let hole_cards = HoleCards::new(
            Card52::new(Rank52::Ace, Suit::Hearts),
            Card52::new(Rank52::King, Suit::Hearts),
        );
        let mut board = Board::default();
        board.deal_flop(vec![
            Card52::new(Rank52::Queen, Suit::Hearts),
            Card52::new(Rank52::Jack, Suit::Hearts),
            Card52::new(Rank52::Ten, Suit::Hearts),
        ]);
        board.deal_turn(Card52::new(Rank52::Deuce, Suit::Clubs));
        board.deal_river(Card52::new(Rank52::Three, Suit::Diamonds));
        let rating = RatedHand::new(&hole_cards, &board);
        let Ok(score) = rating.get_simulated_score() else {
            panic!("river board must be scoreable");
        };
        assert!(score.is_finite());
    }

    /// Verifies invalid intermediate board sizes produce explicit errors.
    #[test]
    fn incomplete_invalid_board_is_rejected() {
        let mut board = Board::default();
        board.deal_flop(vec![Card52::new(Rank52::Ace, Suit::Hearts)]);
        let rating = RatedHand::new(&HoleCards::default(), &board);
        assert!(matches!(
            rating.get_boards(),
            Err(crate::hand::HandError::InvalidBoardCardCount(1))
        ));
    }
}
