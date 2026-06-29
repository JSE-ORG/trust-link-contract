# Fuzz Testing for TrustLink Escrow Contract

This directory contains fuzz harness targets for stress-testing the public entry points of the TrustLink escrow contract using cargo-fuzz.

## Overview

Fuzz testing is a security testing technique that automatically generates random inputs to find bugs, crashes, and edge cases that might be missed by traditional unit tests. These harnesses target the main entry points of the contract to ensure robustness against malformed inputs.

## Fuzz Targets

The following fuzz targets have been created for each public entry point:

- **create_escrow.rs**: Fuzzes the `create_escrow` function with random amounts, fees, and shipping windows
- **fund_escrow.rs**: Fuzzes the `fund_escrow` function with random buyer addresses
- **mark_shipped.rs**: Fuzzes the `mark_shipped` function with random tracking IDs
- **confirm_delivery.rs**: Fuzzes the `confirm_delivery` function with random ledger timestamps
- **raise_dispute.rs**: Fuzzes the `raise_dispute` function with random dispute metadata
- **resolve_dispute.rs**: Fuzzes the `resolve_dispute` function with random resolution types
- **cancel_escrow.rs**: Fuzzes the `cancel_escrow` function
- **auto_release.rs**: Fuzzes the `auto_release` function with random time values

## Running Fuzz Tests Locally

### Prerequisites

- Rust nightly toolchain
- cargo-fuzz installed: `cargo install cargo-fuzz --locked`

### Build Fuzz Targets

```bash
cd contracts/escrow/fuzz
cargo build --release --bins
```

### Run Individual Fuzz Targets

```bash
# Run create_escrow fuzzer
cargo fuzz run create_escrow

# Run fund_escrow fuzzer
cargo fuzz run fund_escrow

# Run with specific duration
cargo fuzz run create_escrow -- -max_total_time=60
```

### Run All Fuzz Targets

```bash
for target in create_escrow fund_escrow mark_shipped confirm_delivery raise_dispute resolve_dispute cancel_escrow auto_release; do
    echo "Fuzzing $target..."
    cargo fuzz run $target -- -max_total_time=30
done
```

## CI Integration

Fuzz tests run on nightly CI builds (Linux only) on pushes to main. The CI job:

1. Uses the nightly Rust toolchain
2. Builds all fuzz targets in release mode
3. Runs each target with a short timeout to ensure compilation and basic functionality

The fuzz job is configured in `.github/workflows/ci.yml`.

## Fuzz Target Design

Each fuzz target follows this pattern:

1. **Setup**: Initialize the Soroban environment with mocked authentication
2. **Contract Initialization**: Initialize the escrow contract with admin and fee collector
3. **State Setup**: Create the necessary escrow state (e.g., create and fund before testing mark_shipped)
4. **Fuzz Input Extraction**: Extract parameters from the fuzz input byte array
5. **Function Call**: Invoke the target function with fuzzed inputs
6. **Error Handling**: The harness gracefully handles expected errors (e.g., validation failures)

## Adding New Fuzz Targets

To add a new fuzz target:

1. Create a new file in `fuzz_targets/` directory
2. Add the target to `Cargo.toml` as a new `[[bin]]` entry
3. Follow the pattern of existing targets
4. Update this README and the CI workflow if needed

## Limitations

- Fuzz targets currently use Soroban's test environment, which may not perfectly match production behavior
- Some complex state transitions may require additional setup in the harness
- Windows builds are not supported due to cargo-fuzz limitations (CI runs on Linux)

## Security Considerations

Fuzz testing helps identify:
- Integer overflows/underflows
- Panic conditions
- Invalid state transitions
- Boundary condition failures
- Malformed input handling

However, fuzz testing is complementary to, not a replacement for, formal verification, manual audits, and comprehensive unit tests.
