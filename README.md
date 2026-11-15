# holdem_rust
A NL Holdem Game

## C++ to Rust migration

This first-pass migration brought the C++ poker types and game flow, hand
evaluation and ranges, parallel calculation utilities, and both application
entry points into this single Rust 2024 crate. It also added Rust tests for the
existing C++ test scenarios and benchmarks under `benches/`. The work used the
repository's specialized Copilot agent harness for planning, implementation,
testing, documentation, benchmarking, linting, and source mapping review.

### Validation

The following checks passed locally:

- `cargo fmt --all -- --check`
- `cargo build --release --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings -D clippy::pedantic`
- `cargo test --release --all-features` (53 tests passed)
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`
- `cargo bench --all-features`

These are local results; a remote GitHub Actions run was not performed. The
native C++ test suite could not be run because its build tools and dependencies
were unavailable in the environment.

### Known source limitations

- The C++ `getHandStrength` passes five cards to `DrawingHand52`, whose
  constructor requires four. The Rust translation reports this source error
  rather than guessing which card should be omitted.
- `Card52::nb` and the `HandStrategy` string constructor/`toString` are declared
  but have no C++ definitions; the corresponding Rust operations report that
  their behavior is unavailable.
- Randomized results are not sequence-identical to C++. The C++ simulated-hand
  intersection also writes into invalid output storage; Rust uses a safe
  intersection implementation instead, so exact parity for that undefined
  behavior is not possible.

### AI usage

At the time of writing, local session telemetry recorded **98.26 AI usage
units** (98,262,963,000 nano-AIU) across 587 requests for this migration
session. This is the telemetry's AIU measure, not an authoritative Copilot
dashboard premium-request or billing-credit total; those may be counted
differently.
