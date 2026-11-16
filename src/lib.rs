//! Safe Rust translation of the `cpp_code/` poker implementation.
//!
//! The C++ project license and copyright notice are retained in the repository
//! `LICENSE` file. The separately licensed Howard Hinnant combinatorics source
//! carries its Boost Software License attribution in [`combinatorics`].
#![deny(missing_docs)]

pub mod action;
pub mod board;
pub mod card52;
pub mod cli;
pub mod combinatorics;
pub mod dealer;
pub mod decision_engine;
pub mod deck52;
pub mod hand;
pub mod hand_history;
pub mod hand_range;
pub mod holdem_hand;
pub mod hole_cards;
pub mod nlhe;
pub mod par;
pub mod player;
pub mod pot;
pub mod rated_hand;
pub mod stack;

/// Prints the greeting implemented by `cpp_code/src/Lib.cpp`.
pub fn say_hello(name: String) {
    println!("Hello, {name}!");
}
