//! Dependency-free timing coverage for loop-containing translated functions.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use holdem_rust::action::{BoardAction, HandAction};
use holdem_rust::board::{Board, Street};
use holdem_rust::card52::{Card52, Rank52, Suit};
use holdem_rust::cli::parse_arguments;
use holdem_rust::combinatorics::{self, detail::DiscontinuousRange};
use holdem_rust::dealer::{Dealer, Options, divide_pot};
use holdem_rust::decision_engine::{DecisionEngine, get_to_call};
use holdem_rust::deck52::Deck52;
use holdem_rust::hand::{
    AllDraws, DrawingHand52, Hand, MadeHand52, all_cards_for_rank, all_cards_for_suit,
    try_get_best_hand,
};
use holdem_rust::hand_history::HandHistory;
use holdem_rust::hand_range::{
    HandRangeKey, calculate_preflop_strengths, get_hand_strength, get_preflop_strengths,
    print_preflop_strengths,
};
use holdem_rust::holdem_hand::HoldemHand52;
use holdem_rust::hole_cards::HoleCards;
use holdem_rust::nlhe::Nlhe52;
use holdem_rust::par::{CalcStep, Calculation, Scheduler, SubCalculation, Task};
use holdem_rust::player::{Decision, Player};
use holdem_rust::rated_hand::RatedHand;
use holdem_rust::stack::Stack;

/// Measures one or more invocations of a translated loop-containing operation.
fn measure<T>(name: &str, iterations: usize, mut operation: impl FnMut() -> T) {
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(operation());
    }
    println!(
        "{name}: {:?} ({iterations} iteration(s))",
        started.elapsed()
    );
}

/// Adds one to a calculation argument for the parallel-calculation benchmarks.
fn increment((value,): (i32,)) -> i32 {
    value + 1
}

/// Continues each benchmark combinatoric traversal without modifying its selection.
fn keep_going(_selected: &mut [i32]) -> bool {
    false
}

/// Creates the same royal-flush board and hole cards for deterministic evaluations.
fn royal_flush_setup() -> (HoleCards, Board) {
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
    (hole_cards, board)
}

/// Benchmarks card, deck, hole-card, and board loop workloads.
fn bench_value_types() {
    measure("Card52::getAll", 10, Card52::get_all);
    measure("HoleCards::getAll", 1, HoleCards::get_all);
    let hole_cards = HoleCards::new(Card52::get_all()[0], Card52::get_all()[1]);
    measure("HoleCards::toString", 100, || format!("{hole_cards}"));
    measure("Deck52::Deck52", 10, Deck52::new);
    let deck = Deck52::new();
    measure("Deck52::toString", 10, || deck.to_string());
    measure("Deck52::getHoleCards", 10, || {
        Deck52::new().get_hole_cards().unwrap()
    });
    measure("Deck52::getFlop", 10, || Deck52::new().get_flop().unwrap());
    measure("Deck52::getCard", 10, || Deck52::new().get_card().unwrap());
    measure("Deck52::getRange", 10, || {
        Deck52::new().get_range(5).unwrap()
    });
    measure("Deck52::getRemainingCards", 10, || {
        Deck52::new().get_remaining_cards().to_vec()
    });
    measure("Deck52::moveToFront", 10, || {
        let mut deck = Deck52::new();
        let cards = deck.get_range(2).unwrap();
        deck.move_to_front(&cards).unwrap();
        deck
    });
    measure("Deck52::shuffle", 10, || {
        let mut deck = Deck52::new();
        deck.shuffle();
        deck
    });
    measure("Deck52::shuffleRemainder", 10, || {
        let mut deck = Deck52::new();
        deck.get_hole_cards().unwrap();
        deck.shuffle_remainder();
        deck
    });
    let mut flop = Board::default();
    flop.deal_flop(Card52::get_all()[..3].to_vec());
    measure("Board::getBestHoldings", 1, || flop.get_best_holdings());
}

/// Benchmarks Howard Hinnant combinatorial count helpers.
fn bench_combinatorial_counts() {
    measure("gcd", 100, || combinatorics::gcd(12_345, 3_210));
    measure("count_each_combination", 100, || {
        combinatorics::count_each_combination(5, 7).unwrap()
    });
    measure("count_each_permutation", 100, || {
        combinatorics::count_each_permutation(5, 7).unwrap()
    });
    measure("count_each_circular_permutation", 100, || {
        combinatorics::count_each_circular_permutation(5, 7).unwrap()
    });
    measure("count_each_reversible_permutation", 100, || {
        combinatorics::count_each_reversible_permutation(5, 7).unwrap()
    });
    measure("count_each_reversible_circular_permutation", 100, || {
        combinatorics::count_each_reversible_circular_permutation(5, 7).unwrap()
    });
    measure("count range variants", 100, || {
        (
            combinatorics::count_each_combination_for_range(0, 5, 12).unwrap(),
            combinatorics::count_each_permutation_for_range(0, 5, 12).unwrap(),
            combinatorics::count_each_circular_permutation_for_range(0, 5, 12).unwrap(),
            combinatorics::count_each_reversible_permutation_for_range(0, 5, 12).unwrap(),
            combinatorics::count_each_reversible_circular_permutation_for_range(0, 5, 12).unwrap(),
        )
    });
}

/// Benchmarks the five public combinatorial traversals.
fn bench_combinatorial_traversals() {
    measure("for_each_combination", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5, 6];
        let mut callback = combinatorics::for_each_combination(&mut values, 3, |_| false);
        callback(&mut values);
        values
    });
    measure("for_each_permutation", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5];
        let mut callback = combinatorics::for_each_permutation(&mut values, 4, |_| false);
        callback(&mut values);
        values
    });
    measure("for_each_circular_permutation", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5];
        let mut callback = combinatorics::for_each_circular_permutation(&mut values, 4, |_| false);
        callback(&mut values);
        values
    });
    measure("for_each_reversible_permutation", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5];
        let mut callback =
            combinatorics::for_each_reversible_permutation(&mut values, 4, |_| false);
        callback(&mut values);
        values
    });
    measure("for_each_reversible_circular_permutation", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5];
        let mut callback =
            combinatorics::for_each_reversible_circular_permutation(&mut values, 4, |_| false);
        callback(&mut values);
        values
    });
}

/// Benchmarks Howard Hinnant's low-level discontinuous range helpers.
fn bench_combinatorial_detail() {
    measure("detail::rotate_discontinuous", 100, || {
        let mut values = [0, 1, 2, 3, 4, 5];
        combinatorics::detail::rotate_discontinuous(&mut values, 0, 1, 1, 2, 5, 3);
        values
    });
    measure("detail::rotate_discontinuous3", 100, || {
        let mut values = [0, 1, 2, 3, 4, 5, 6];
        combinatorics::detail::rotate_discontinuous3(
            &mut values,
            [
                DiscontinuousRange::new(0, 1, 1),
                DiscontinuousRange::new(2, 4, 2),
                DiscontinuousRange::new(5, 7, 2),
            ],
        );
        values
    });
    measure("detail::combine_discontinuous", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5];
        combinatorics::detail::combine_discontinuous(
            &mut values,
            DiscontinuousRange::new(0, 1, 1),
            DiscontinuousRange::new(1, 6, 5),
            0..1,
            &mut keep_going,
            0,
        );
        values
    });
    measure("detail::combine_discontinuous3", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5, 6];
        combinatorics::detail::combine_discontinuous3(
            &mut values,
            [
                DiscontinuousRange::new(0, 1, 1),
                DiscontinuousRange::new(1, 3, 2),
                DiscontinuousRange::new(3, 7, 4),
            ],
            0..1,
            &mut keep_going,
        );
        values
    });
    measure("detail::permute", 20, || {
        let mut values = [0, 1, 2, 3, 4];
        combinatorics::detail::permute(&mut values, 0, 5, 5, 0..5, &mut keep_going);
        values
    });
    measure("detail::combine_discontinuous3_", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5, 6];
        combinatorics::detail::combine_discontinuous3_(
            &mut values,
            [
                DiscontinuousRange::new(0, 1, 1),
                DiscontinuousRange::new(1, 3, 2),
                DiscontinuousRange::new(3, 7, 4),
            ],
            0..1,
            &mut keep_going,
            0,
        );
        values
    });
    measure("detail::permute_", 20, || {
        let mut values = [0, 1, 2, 3, 4];
        combinatorics::detail::permute_(&mut values, 0, 5, 5, 0..5, &mut keep_going);
        values
    });
    measure("detail::reversible_permutation", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5];
        combinatorics::detail::reversible_permutation(&mut values, 5, &mut keep_going);
        values
    });
}

/// Benchmarks bound callbacks and reversible-permutation adapters.
fn bench_combinatorial_adapters() {
    measure("CallCombineDiscontinuous::call", 20, || {
        let mut values = [0, 1, 2, 3, 4];
        let mut callback = keep_going as fn(&mut [i32]) -> bool;
        let mut bound = combinatorics::detail::CallCombineDiscontinuous::new(
            &mut callback,
            0..1,
            1,
            1..5,
            4,
            0..5,
        );
        bound.call(&mut values);
        values
    });
    measure("CallPermute::call", 20, || {
        let mut values = [0, 1, 2, 3];
        let mut bound = combinatorics::detail::CallPermute::new(
            keep_going as fn(&mut [i32]) -> bool,
            0,
            4,
            4,
            0..4,
        );
        bound.call(&mut values);
        values
    });
    measure("CircularPermutation::call", 20, || {
        let mut values = [0, 1, 2, 3];
        let mut bound = combinatorics::detail::CircularPermutation::new(
            keep_going as fn(&mut [i32]) -> bool,
            4,
        );
        bound.call(&mut values);
        values
    });
    measure("ReversiblePermutation::call", 20, || {
        let mut values = [0, 1, 2, 3, 4];
        let mut bound = combinatorics::detail::ReversiblePermutation::new(
            keep_going as fn(&mut [i32]) -> bool,
            5,
        );
        bound.call(&mut values);
        values
    });
    measure("ReverseCircularPermutation::call", 20, || {
        let mut values = [0, 1, 2, 3, 4];
        let mut bound = combinatorics::detail::ReverseCircularPermutation::new(
            keep_going as fn(&mut [i32]) -> bool,
            5,
        );
        bound.call(&mut values);
        values
    });
    measure("Rev1::call", 20, || {
        let mut values = [0, 1, 2];
        let mut bound =
            combinatorics::detail::Rev1::new(keep_going as fn(&mut [i32]) -> bool, 0, 3, 3);
        bound.call(&mut values);
        values
    });
    measure("Rev2::call", 20, || {
        let mut values = [0, 1, 2, 3];
        let mut bound = combinatorics::detail::Rev2::new(
            keep_going as fn(&mut [i32]) -> bool,
            0,
            2,
            1,
            2,
            4,
            1,
        );
        bound.call(&mut values);
        values
    });
    measure("Rev3::call", 20, || {
        let mut values = [0, 1, 2, 3, 4, 5];
        let mut bound = combinatorics::detail::Rev3::new(
            keep_going as fn(&mut [i32]) -> bool,
            [
                DiscontinuousRange::new(0, 2, 1),
                DiscontinuousRange::new(2, 4, 1),
                DiscontinuousRange::new(4, 6, 1),
            ],
        );
        bound.call(&mut values);
        values
    });
    measure("BoundRange::call", 100, || {
        let mut values = [0, 1, 2, 3];
        let mut bound =
            combinatorics::detail::BoundRange::new(keep_going as fn(&mut [i32]) -> bool, 0, 2);
        bound.call(&mut values);
        values
    });
}

/// Benchmarks made-hand, draw, and hole-card valuation routines.
fn bench_hand_evaluation() {
    let cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::King, Suit::Hearts),
        Card52::new(Rank52::Queen, Suit::Hearts),
        Card52::new(Rank52::Jack, Suit::Hearts),
        Card52::new(Rank52::Ten, Suit::Hearts),
    ];
    let made_hand = MadeHand52::new(&cards).unwrap();
    measure("MadeHand52::MadeHand52", 100, || {
        MadeHand52::new(&cards).unwrap()
    });
    measure("MadeHand52::findOccurences", 100, || {
        made_hand.find_occurences(1)
    });
    measure("MadeHand52::getHighCards", 100, || {
        made_hand.get_high_cards()
    });
    measure("MadeHand52::sum", 100, || made_hand.sum());
    measure("MadeHand52::getCards", 100, || made_hand.get_cards());
    measure("MadeHand52::fromString", 100, || {
        MadeHand52::from_string("Ah Kh Qh Jh Th").unwrap()
    });
    let (hole_cards, board) = royal_flush_setup();
    let hand = Hand::new(hole_cards.clone(), board.clone());
    measure("Hand::getCards", 100, || hand.get_cards());
    measure("getBestHand", 100, || try_get_best_hand(&hand).unwrap());
    measure("compareHands", 100, || {
        holdem_rust::hand::compare_hands(&hand, &hand)
    });
    let draw_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::Deuce, Suit::Hearts),
        Card52::new(Rank52::Three, Suit::Hearts),
        Card52::new(Rank52::Four, Suit::Hearts),
    ];
    let draw = DrawingHand52::new(&draw_cards).unwrap();
    measure("DrawingHand52::DrawingHand52", 100, || {
        DrawingHand52::new(&draw_cards).unwrap()
    });
    measure("DrawingHand52::getHighCards", 100, || draw.get_high_cards());
    measure("DrawingHand52::getOuts", 100, || {
        DrawingHand52::new(&draw_cards).unwrap().get_outs()
    });
    measure("DrawingHand52::fromString", 100, || {
        DrawingHand52::from_string("Ah 2h 3h 4h").unwrap()
    });
    measure("getAllCards(Suit)", 100, || {
        all_cards_for_suit(Suit::Hearts)
    });
    measure("getAllCards(Rank52)", 100, || {
        all_cards_for_rank(Rank52::Ace)
    });
    measure("AllDraws::AllDraws", 100, || AllDraws::new(&hand).unwrap());
}

/// Benchmarks HoldemHand52 loop sites via their public constructors and methods.
fn bench_holdem_hand() {
    let high_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::King, Suit::Clubs),
        Card52::new(Rank52::Queen, Suit::Diamonds),
        Card52::new(Rank52::Jack, Suit::Spades),
        Card52::new(Rank52::Nine, Suit::Hearts),
        Card52::new(Rank52::Seven, Suit::Clubs),
        Card52::new(Rank52::Deuce, Suit::Diamonds),
    ];
    let pair_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::Ace, Suit::Diamonds),
        Card52::new(Rank52::King, Suit::Clubs),
        Card52::new(Rank52::Queen, Suit::Diamonds),
        Card52::new(Rank52::Jack, Suit::Spades),
        Card52::new(Rank52::Seven, Suit::Clubs),
        Card52::new(Rank52::Deuce, Suit::Diamonds),
    ];
    let two_pair_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::Ace, Suit::Diamonds),
        Card52::new(Rank52::King, Suit::Clubs),
        Card52::new(Rank52::King, Suit::Diamonds),
        Card52::new(Rank52::Queen, Suit::Spades),
        Card52::new(Rank52::Seven, Suit::Clubs),
        Card52::new(Rank52::Deuce, Suit::Diamonds),
    ];
    let straight_cards = [
        Card52::new(Rank52::Nine, Suit::Hearts),
        Card52::new(Rank52::Eight, Suit::Diamonds),
        Card52::new(Rank52::Seven, Suit::Clubs),
        Card52::new(Rank52::Six, Suit::Spades),
        Card52::new(Rank52::Five, Suit::Hearts),
        Card52::new(Rank52::King, Suit::Clubs),
        Card52::new(Rank52::Deuce, Suit::Diamonds),
    ];
    let wheel_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::Five, Suit::Diamonds),
        Card52::new(Rank52::Four, Suit::Clubs),
        Card52::new(Rank52::Three, Suit::Spades),
        Card52::new(Rank52::Deuce, Suit::Hearts),
        Card52::new(Rank52::King, Suit::Clubs),
        Card52::new(Rank52::Nine, Suit::Diamonds),
    ];
    let flush_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::Jack, Suit::Hearts),
        Card52::new(Rank52::Nine, Suit::Hearts),
        Card52::new(Rank52::Seven, Suit::Hearts),
        Card52::new(Rank52::Five, Suit::Hearts),
        Card52::new(Rank52::King, Suit::Clubs),
        Card52::new(Rank52::Deuce, Suit::Diamonds),
    ];
    let full_house_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::Ace, Suit::Diamonds),
        Card52::new(Rank52::Ace, Suit::Clubs),
        Card52::new(Rank52::King, Suit::Hearts),
        Card52::new(Rank52::King, Suit::Diamonds),
        Card52::new(Rank52::Queen, Suit::Clubs),
        Card52::new(Rank52::Deuce, Suit::Spades),
    ];
    let quads_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::Ace, Suit::Diamonds),
        Card52::new(Rank52::Ace, Suit::Clubs),
        Card52::new(Rank52::Ace, Suit::Spades),
        Card52::new(Rank52::King, Suit::Hearts),
        Card52::new(Rank52::Queen, Suit::Clubs),
        Card52::new(Rank52::Deuce, Suit::Spades),
    ];
    let straight_flush_cards = [
        Card52::new(Rank52::Ace, Suit::Hearts),
        Card52::new(Rank52::King, Suit::Hearts),
        Card52::new(Rank52::Queen, Suit::Hearts),
        Card52::new(Rank52::Jack, Suit::Hearts),
        Card52::new(Rank52::Ten, Suit::Hearts),
        Card52::new(Rank52::Nine, Suit::Hearts),
        Card52::new(Rank52::Deuce, Suit::Clubs),
    ];

    // Each constructor case maps to from_sorted_input/classify_hand and drives
    // a distinct category branch, including straight and flush classification.
    measure("HoldemHand52::from_cards/classify(HighCard)", 100, || {
        HoldemHand52::from_cards(&high_cards)
    });
    measure("HoldemHand52::from_cards/classify(Pair)", 100, || {
        HoldemHand52::from_cards(&pair_cards)
    });
    measure("HoldemHand52::from_cards/classify(TwoPair)", 100, || {
        HoldemHand52::from_cards(&two_pair_cards)
    });
    measure("HoldemHand52::from_cards/classify(Straight)", 100, || {
        HoldemHand52::from_cards(&straight_cards)
    });
    measure(
        "HoldemHand52::from_cards/classify(WheelStraight)",
        100,
        || HoldemHand52::from_cards(&wheel_cards),
    );
    measure("HoldemHand52::from_cards/classify(Flush)", 100, || {
        HoldemHand52::from_cards(&flush_cards)
    });
    measure("HoldemHand52::from_cards/classify(FullHouse)", 100, || {
        HoldemHand52::from_cards(&full_house_cards)
    });
    measure("HoldemHand52::from_cards/classify(Quads)", 100, || {
        HoldemHand52::from_cards(&quads_cards)
    });
    measure(
        "HoldemHand52::from_cards/classify(StraightFlush)",
        100,
        || HoldemHand52::from_cards(&straight_flush_cards),
    );

    let two_pair = HoldemHand52::from_cards(&two_pair_cards);
    let pair = HoldemHand52::from_cards(&pair_cards);
    measure("HoldemHand52::findOccurences/TwoPair", 100, || {
        two_pair.find_occurences(2)
    });
    measure("HoldemHand52::getRanksMattering/TwoPair", 100, || {
        two_pair.get_ranks_mattering()
    });
    measure("HoldemHand52::getCardsMattering/TwoPair", 100, || {
        two_pair.get_cards_mattering()
    });
    measure("HoldemHand52::getCards/TwoPair", 100, || {
        two_pair.get_cards()
    });
    measure("HoldemHand52::getHighCards/Pair", 100, || {
        pair.get_high_cards()
    });
    let straight_flush = HoldemHand52::from_cards(&straight_flush_cards);
    measure("HoldemHand52::getHighCards/StraightFlush", 100, || {
        straight_flush.get_high_cards()
    });
}

/// Benchmarks action-history and dealer betting loops.
fn bench_game_operations() {
    let mut players = vec![
        Player::new(Stack(1_000), &DecisionEngine, 1),
        Player::new(Stack(1_000), &DecisionEngine, 2),
        Player::new(Stack(1_000), &DecisionEngine, 3),
    ];
    for (index, player) in players.iter_mut().enumerate() {
        player.set_player_id(index);
        player.deal_hole_cards(&HoleCards::new(
            Card52::get_all()[index * 2],
            Card52::get_all()[index * 2 + 1],
        ));
    }
    let player = players[0].clone();
    let mut board = Board::default();
    board.deal_flop(Card52::get_all()[..3].to_vec());
    measure("BoardAction::toString", 100, || {
        BoardAction {
            player: player.clone(),
            board: board.clone(),
        }
        .to_string()
    });
    let mut history = HandHistory::default();
    history.log_action(holdem_rust::action::LoggedAction::Betting(
        holdem_rust::action::BettingAction::new(&player, Street::Flop),
    ));
    measure("HandHistory::getLastBet", 100, || {
        history.get_last_bet(Street::Flop, Some(player.id()))
    });
    measure("HandHistory::toString", 100, || history.to_string());
    let mut dealer = Dealer::new(vec![0, 1, 2], Stack(20));
    let small_blind = holdem_rust::action::BettingAction::create(
        &players[1],
        Street::Preflop,
        Stack(10),
        Stack(0),
        Stack(0),
        Decision::Raise,
    );
    let big_blind = holdem_rust::action::BettingAction::create(
        &players[2],
        Street::Preflop,
        Stack(20),
        Stack(0),
        Stack(0),
        Decision::Raise,
    );
    dealer.accept_bet(1, &small_blind);
    dealer.accept_bet(2, &big_blind);
    measure("Dealer::getOptions", 100, || {
        dealer.get_options(0, &players).unwrap()
    });
    measure("Dealer::rakeIn", 20, || {
        let mut round_players = players.clone();
        let mut round_dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        round_dealer.accept_bet(1, &small_blind);
        round_dealer.accept_bet(2, &big_blind);
        round_dealer.rake_in(&mut round_players).unwrap();
        round_dealer
    });
    measure("Dealer::rakeIn/allinIsCalled", 20, || {
        let mut short_players = vec![
            Player::new(Stack(20), &DecisionEngine, 1),
            Player::new(Stack(100), &DecisionEngine, 2),
            Player::new(Stack(100), &DecisionEngine, 3),
        ];
        for (index, player) in short_players.iter_mut().enumerate() {
            player.set_player_id(index);
            player.deal_hole_cards(&HoleCards::new(
                Card52::get_all()[index * 2],
                Card52::get_all()[index * 2 + 1],
            ));
        }
        let mut all_in_dealer = Dealer::new(vec![0, 1, 2], Stack(20));
        for (id, amount) in [(0, 20), (1, 20), (2, 20)] {
            let action = holdem_rust::action::BettingAction::create(
                &short_players[id],
                Street::Preflop,
                Stack(amount),
                Stack(0),
                Stack(0),
                Decision::Raise,
            );
            all_in_dealer.accept_bet(id, &action);
        }
        all_in_dealer.rake_in(&mut short_players).unwrap();
        all_in_dealer
    });
    measure("Dealer::dividePot", 100, || {
        divide_pot(Stack(100), 3).unwrap()
    });
}

/// Benchmarks hand rating, preflop calculations, and showdown traversal.
fn bench_rating_and_ranges() {
    let mut players = [
        Player::new(Stack(1_000), &DecisionEngine, 1),
        Player::new(Stack(1_000), &DecisionEngine, 2),
    ];
    for (index, player) in players.iter_mut().enumerate() {
        player.set_player_id(index);
        player.deal_hole_cards(&HoleCards::new(
            Card52::get_all()[index * 2],
            Card52::get_all()[index * 2 + 1],
        ));
    }
    let (hole_cards, river) = royal_flush_setup();
    let rating = RatedHand::new(&hole_cards, &river);
    measure("RatedHand::getScore", 100, || rating.get_score().unwrap());
    measure("RatedHand::getSimulatedScore(River)", 20, || {
        rating.get_simulated_score().unwrap()
    });
    let mut flop = Board::default();
    flop.deal_flop(Card52::get_all()[..3].to_vec());
    let flop_rating = RatedHand::new(&hole_cards, &flop);
    measure("RatedHand::getBoards/getSimulatedScore(Flop)", 1, || {
        flop_rating.get_simulated_score()
    });
    let preflop_one_sample =
        RatedHand::with_preflop_board_samples(&hole_cards, &Board::default(), 1);
    measure(
        "RatedHand::getBoards/getSimulatedScore(Preflop,1 sample)",
        1,
        || preflop_one_sample.get_simulated_score(),
    );
    let preflop_source_default = RatedHand::new(&hole_cards, &Board::default());
    measure(
        "RatedHand::getBoards/getSimulatedScore(Preflop,1000 samples)",
        1,
        || preflop_source_default.get_simulated_score(),
    );
    let key = HandRangeKey::new(Rank52::Ace, Rank52::King, true);
    measure("getHandStrength", 1, || get_hand_strength(key));
    measure("getPreflopStrengths", 1, get_preflop_strengths);
    measure("printPreflopStrengths", 1, print_preflop_strengths);
    measure("calculatePreflopStrengths", 1, || {
        calculate_preflop_strengths(1000).unwrap();
    });
    measure("DecisionEngine::getToCall", 100, || {
        get_to_call(&Options::default())
    });
    let decision_dealer = Dealer::new(vec![0, 1], Stack(20));
    measure("DecisionEngine::decide/random_below", 100, || {
        DecisionEngine
            .decide(&decision_dealer, &Board::default(), &players[0], &players)
            .unwrap()
    });
    let mut showdown_dealer = Dealer::new(vec![0, 1], Stack(20));
    showdown_dealer.get_current_pot().put_amount(0, Stack(10));
    let mut showdown_history = HandHistory::default();
    measure("Dealer::awardPots/showdown", 10, || {
        let mut current_dealer = showdown_dealer.clone();
        current_dealer.get_current_pot().put_amount(1, Stack(10));
        let mut current_players = vec![players[0].clone(), players[1].clone()];
        current_dealer
            .award_pots(&river, &mut showdown_history, &mut current_players)
            .unwrap()
    });
}

/// Benchmarks command parsing, NLHE hand play, and parallel utilities.
fn bench_application_and_parallel() {
    measure("CLI::parse", 100, || {
        parse_arguments(["-n", "100"]).unwrap()
    });
    measure("Nlhe52::Nlhe52", 10, || Nlhe52::new(3));
    measure("Nlhe52::ready", 10, || {
        Nlhe52::new(3).ready(Stack(0), &Board::default(), &HandHistory::default())
    });
    measure("Nlhe52::playHand", 1, || {
        Nlhe52::new(2).play_hand().unwrap();
    });
    measure("Scheduler::new/run/cancel/appendTasks", 1, || {
        let scheduler = Scheduler::new(1);
        let tasks: Vec<Task> = (0..4).map(|_| Arc::new(|| {}) as Task).collect();
        scheduler.append_tasks(tasks).unwrap();
        scheduler.cancel();
        scheduler.run(0);
        scheduler
    });
    measure("SubCalculation::calc", 100, || {
        SubCalculation::new(|(left, right)| left + right, (2, 3)).calc()
    });
    measure("CalcStep::calc", 20, || {
        let step = CalcStep::new(vec![
            SubCalculation::new(increment as fn((i32,)) -> i32, (1,)),
            SubCalculation::new(increment as fn((i32,)) -> i32, (2,)),
        ]);
        step.calc();
        (step.result(0), step.result(1))
    });
    measure("Calculation::calculate", 10, || {
        let mut calculation = Calculation::new(2);
        calculation.add_calculation(CalcStep::new(vec![
            SubCalculation::new(increment as fn((i32,)) -> i32, (1,)),
            SubCalculation::new(increment as fn((i32,)) -> i32, (2,)),
        ]));
        calculation.calculate();
        calculation
    });
}

/// Runs the dependency-free benchmark cases for translated source loops.
fn main() {
    bench_value_types();
    bench_combinatorial_counts();
    bench_combinatorial_traversals();
    bench_combinatorial_detail();
    bench_combinatorial_adapters();
    bench_hand_evaluation();
    bench_holdem_hand();
    bench_game_operations();
    bench_rating_and_ranges();
    bench_application_and_parallel();
}
