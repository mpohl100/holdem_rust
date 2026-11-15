//! Shared command-line parsing for the two C++ application entry points.

use std::fmt;

/// Parsed application arguments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandLine {
    /// Print the command-line usage text and exit successfully.
    Help,
    /// Run the app with its parsed, intentionally unused hand count.
    Run {
        /// C++ local variable `nbHands`, defaulting to 100.
        number_of_hands: i32,
    },
}

/// A malformed command line, corresponding to Clara's parse failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandLineError {
    /// Clara-style diagnostic text shown after `Error in command line:`.
    pub message: String,
}

impl fmt::Display for CommandLineError {
    /// Formats the argument parsing diagnostic.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandLineError {}

/// Parses the options accepted by each C++ Clara-based app.
///
/// C++ mapping: `Opt(nbHands, "numberHands")["-n"]["--nb_hands"]` plus
/// `Help(help)`. `numberHands` remains unused by both application bodies.
///
/// # Errors
///
/// Returns an error for an unrecognized option, a missing `-n`/`--nb_hands`
/// value, or a value that does not parse as a signed 32-bit integer.
pub fn parse_arguments<I, S>(arguments: I) -> Result<CommandLine, CommandLineError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = arguments.into_iter().map(Into::into);
    let mut number_of_hands = 100;
    while let Some(argument) = args.next() {
        if argument == "-h" || argument == "--help" {
            return Ok(CommandLine::Help);
        }
        if argument == "-n" || argument == "--nb_hands" {
            let value = args.next().ok_or_else(|| CommandLineError {
                message: format!("Expected an integer after {argument}"),
            })?;
            number_of_hands = parse_count(&value)?;
            continue;
        }
        if let Some((option, value)) = argument.split_once('=')
            && (option == "-n" || option == "--nb_hands")
        {
            number_of_hands = parse_count(value)?;
            continue;
        }
        return Err(CommandLineError {
            message: format!("Unrecognized command line argument: {argument}"),
        });
    }
    Ok(CommandLine::Run { number_of_hands })
}

/// Parses the signed C++ `int` value used for the number-of-hands option.
fn parse_count(value: &str) -> Result<i32, CommandLineError> {
    value.parse::<i32>().map_err(|_| CommandLineError {
        message: format!("Expected an integer, but received '{value}'"),
    })
}

/// Formats the help text for one of the preserved application names.
#[must_use]
pub fn help_text(program_name: &str) -> String {
    format!(
        "Usage: {program_name} [OPTIONS]\n\nOptions:\n  -n, --nb_hands <numberHands>  number of hands to play\n  -h, --help                    display usage information\n"
    )
}

#[cfg(test)]
mod tests {
    use super::{CommandLine, parse_arguments};

    /// Checks defaults and both C++ spellings of the hand-count option.
    #[test]
    fn parses_source_options_and_default() {
        assert_eq!(
            parse_arguments(Vec::<String>::new()).unwrap(),
            CommandLine::Run {
                number_of_hands: 100
            }
        );
        assert_eq!(
            parse_arguments(["-n", "12"]).unwrap(),
            CommandLine::Run {
                number_of_hands: 12
            }
        );
        assert_eq!(
            parse_arguments(["--nb_hands=7"]).unwrap(),
            CommandLine::Run { number_of_hands: 7 }
        );
    }

    /// Checks Clara-like help and malformed-option paths.
    #[test]
    fn handles_help_and_invalid_arguments() {
        assert_eq!(parse_arguments(["--help"]).unwrap(), CommandLine::Help);
        assert!(parse_arguments(["--unknown"]).is_err());
        assert!(parse_arguments(["-n"]).is_err());
        assert!(parse_arguments(["-n", "many"]).is_err());
    }
}
