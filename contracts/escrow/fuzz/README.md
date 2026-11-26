# Fuzz Testing for TrustLink Escrow Contract

This directory contains fuzz harness targets for stress-testing the public entry points of the TrustLink escrow contract using cargo-fuzz.

## Overview

Fuzz testing is a security testing technique that automatically generates random inputs to find bugs, crashes, and edge cases that might be missed by traditional unit tests. These harnesses target the main entry points of the contract to ensure robustness against malformed inputs.

## Fuzz Targets

### Core lifecycle

- **create_escrow.rs**: Fuzzes the `create_escrow` function with random amounts, fees, and shipping windows
- **fund_escrow.rs**: Fuzzes the `fund_escrow` function with random buyer addresses
- **mark_shipped.rs**: Fuzzes the `mark_shipped` function with random tracking IDs
- **confirm_delivery.rs**: Fuzzes the `confirm_delivery` function with random ledger timestamps
- **cancel_escrow.rs**: Fuzzes the `cancel_escrow` function
- **auto_release.rs**: Fuzzes the `auto_release` function with random time values
- **co_signed_release.rs**: Fuzzes `co_signed_release` with co-signatures arriving in any order, from any role, including on a disputed escrow
- **refund_flow.rs**: Interleaves `request_refund`, `approve_refund` and `mutual_cancel`, checking no ordering lets one escrow settle twice
- **expiration.rs**: Fuzzes `create_escrow_with_expiration` plus `reclaim_expired` and `auto_cancel_pending`, probing the `expires_at + grace_period` arithmetic for overflow

### Disputes

- **raise_dispute.rs**: Fuzzes the `raise_dispute` function with random dispute metadata
- **resolve_dispute.rs**: Fuzzes the `resolve_dispute` function with random resolution types
- **dispute_appeal.rs**: Drives `resolve_dispute` → `appeal_dispute` → `finalize_dispute` across arbitrary timestamps, probing both sides of the appeal deadline
- **vote.rs**: Fuzzes multi-resolver voting via `create_escrow_multi`, with resolver-set sizes and thresholds drawn from the input (including threshold 0 and thresholds above the set size)

### Batching and multi-asset

- **basket_escrow.rs**: Fuzzes `create_basket_escrow` with independently drawn `tokens` and `amounts` lengths, so mismatched vectors are exercised, then funds the basket
- **batch_create_escrow.rs**: Fuzzes `batch_create_escrow` over batch sizes and per-entry fields, asserting a successful batch returns exactly one readable escrow id per input

### Governance and messaging

- **admin_timelock.rs**: Fuzzes the queue → wait → execute cycle (and `cancel_timelock_op`) around the 24h admin delay, covering early execution, cancellation races and re-queuing
- **post_message.rs**: Fuzzes `post_message` content lengths and `get_messages` pagination, including `start`/`limit` pairs large enough to overflow a naive `start + limit`

## Running Fuzz Tests Locally

### Prerequisites

- Rust nightly toolchain
- cargo-fuzz installed: `cargo install cargo-fuzz --locked`

All `cargo fuzz` commands run from `contracts/escrow` (the crate that owns the
`fuzz/` directory), not from `fuzz/` itself.

### Build Fuzz Targets

```bash
cd contracts/escrow
cargo fuzz build --release
# or, from the repository root:
make fuzz-build
```

### Run Individual Fuzz Targets

```bash
# Run create_escrow fuzzer
cargo fuzz run create_escrow

# Run with specific duration
cargo fuzz run --release create_escrow -- -max_total_time=60
```

### Run All Fuzz Targets

```bash
# From the repository root; FUZZ_TIME defaults to 60 seconds per target.
FUZZ_TIME=30 make fuzz
```

## CI Integration

`.github/workflows/fuzz.yml` keeps the targets from rotting:

1. Installs the nightly toolchain (via `RUSTUP_TOOLCHAIN`, which overrides the
   stable channel pinned in `rust-toolchain.toml`) and `cargo-fuzz`.
2. Runs `cargo fuzz build --release`, so any ABI change that breaks a harness
   fails the build immediately.
3. Smoke-runs every target for 60 seconds on each push and pull request.
4. Runs the same targets for 15 minutes each on a nightly schedule, and for a
   caller-supplied duration via `workflow_dispatch`.

Crash inputs are uploaded as a `fuzz-artifacts` artifact when a run fails.

Targets run sequentially, so the smoke run costs roughly
`60s x <number of targets>`. Keep that in mind when adding targets — the job's
timeout is 180 minutes.

## Fuzz Target Design

`fuzz_targets/common.rs` holds the shared harness. Each target follows the same
pattern:

1. **Setup** — `Harness::new()` builds an `Env` with mocked auth, registers a
   Stellar Asset Contract as the payment token, mints balances, registers the
   escrow contract and initializes it.
2. **State setup** — helpers such as `create_funded_escrow()` drive the escrow
   to the state the target needs.
3. **Input extraction** — `Reader` is a zero-padding cursor over the fuzz bytes,
   so a short input never panics inside the harness itself.
4. **Function call** — invoked through the generated `EscrowClient` using the
   `try_*` methods, so declared `ContractError`s come back as values instead of
   unwinding. Anything that still panics is a genuine finding.
5. **Idempotency probe** — most targets optionally repeat the call to check that
   a second invocation cannot double-pay or re-advance the state.

Two deliberate constraints keep the signal clean:

- Dispute reasons are drawn from a fixed list, because `Symbol::new` panics on
  characters outside the Soroban symbol alphabet.
- Fuzzed ledger timestamps are masked to 63 bits, which still covers every
  deadline the contract computes without probing host clock limits.

## Adding New Fuzz Targets

To add a new fuzz target:

1. Create a new file in `fuzz_targets/` directory, starting with `mod common;`
2. Add the target to `Cargo.toml` as a new `[[bin]]` entry
3. Follow the pattern of existing targets
4. Update this README — the CI workflow picks up new targets automatically via
   `cargo fuzz list`

`Harness` provides the setup most targets need:

| Helper | Use |
|---|---|
| `create_valid_escrow()` / `create_funded_escrow()` | Reach `Created` / `Funded` |
| `create_disputed_escrow(&mut r)` | Reach `Disputed` in one call |
| `dispute_reason(&mut r)` | A valid reason symbol from `DISPUTE_REASONS` |
| `actor(byte)` | One of the five known roles, authorized or not |
| `stranger()` / `addresses(n)` | Fresh addresses with no role |
| `extra_token()` | A second funded SAC, for basket escrows |

`Reader` adds `len(max)` for collection sizes and `target_id(real_id)` for the
common "real escrow or an arbitrary fuzzed id" choice, on top of the scalar
readers — so targets cover the empty case, the documented cap, and the
not-found path without hand-rolled maths.

Without a real toolchain, `cargo check --manifest-path contracts/escrow/fuzz/Cargo.toml
--all-targets` type-checks every harness on stable — a fast way to catch ABI
drift before pushing.

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
