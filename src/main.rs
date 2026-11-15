//! C++ `cpp_code/apps/main.cpp` application entry point.

use std::process::ExitCode;

use holdem_rust::cli::{CommandLine, help_text, parse_arguments};
use holdem_rust::hand_range::print_preflop_strengths;

/// Parses the source CLI and prints hand-range preflop strengths.
fn main() -> ExitCode {
    match parse_arguments(std::env::args().skip(1)) {
        Ok(CommandLine::Help) => {
            print!("{}", help_text("main"));
            ExitCode::SUCCESS
        }
        Ok(CommandLine::Run { number_of_hands: _ }) => match print_preflop_strengths() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Failed to print preflop strengths: {error}");
                ExitCode::from(1)
            }
        },
        Err(error) => {
            eprintln!("Error in command line: {error}");
            ExitCode::from(1)
        }
    }
}
