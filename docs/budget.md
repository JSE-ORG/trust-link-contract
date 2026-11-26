# Gas Budget & Regression Policy

## Overview

This document defines baseline gas costs for critical contract entry points and establishes a regression detection threshold to prevent unintended performance degradation.

## Baseline Gas Costs

### Core Escrow Operations

| Entry Point | CPU Instructions | Memory Bytes | Type |
|---|---|---|---|
| `initialize` | 65,000 | 8,192 | Admin |
| `create_escrow` | 145,000 | 12,288 | Hot Path |
| `create_basket_escrow` | 180,000 | 15,360 | Hot Path |
| `fund_escrow` | 95,000 | 8,192 | Hot Path |
| `confirm_delivery` | 120,000 | 10,240 | Hot Path |
| `co_signed_release` | 110,000 | 9,216 | Hot Path |
| `auto_release` | 105,000 | 9,216 | Hot Path |
| `ensure_action_not_paused` | 8,000 | 512 | Gating Check |
| `cancel_escrow` | 100,000 | 8,192 | Standard |
| `mutual_cancel` | 105,000 | 9,216 | Standard |
| `mark_shipped` | 75,000 | 6,144 | Standard |
| `record_delivery` | 85,000 | 7,168 | Standard |
| `raise_dispute` | 125,000 | 11,264 | Standard |
| `resolve_dispute` | 150,000 | 13,312 | Standard |
| `appeal_dispute` | 140,000 | 12,288 | Standard |
| `finalize_dispute` | 160,000 | 14,336 | Standard |
| `fund_basket_escrow` | 120,000 | 10,240 | Multi-Token |
| `rotate_resolver` | 90,000 | 7,168 | Admin |
| `request_refund` | 80,000 | 6,144 | Standard |
| `approve_refund` | 85,000 | 7,168 | Standard |
| `batch_create_escrow` (per item) | 160,000 | 12,800 | Batch |
| `multicall` (fixed overhead) | 50,000 | 4,096 | Dispatch |

## Regression Threshold

**Failure Condition:** Any entry point's actual CPU cost exceeds its baseline by **> 5%**.

- **Warning:** Cost increase between 3-5% triggers a review comment.
- **Hard Fail:** Cost increase > 5% blocks CI.

### Calculation

```
regression = (actual_cpu - baseline_cpu) / baseline_cpu * 100
if regression > 5.0:
    FAIL the build with error message
```

## Measurement Method

Gas costs are measured using the Soroban test harness `env.budget()` API:

```rust
let before = env.budget().get_cpu_insns_count();
// execute contract function
let after = env.budget().get_cpu_insns_count();
let cpu_used = after - before;
```

Reference implementation: `contracts/escrow/src/test_gas_profile.rs`

## CI Integration

### Manual Testing

```bash
cd contracts/escrow
cargo test --lib test_gas_profile -- --nocapture
```

This runs gas profiling tests and prints results in the format:
```
gas_profile | operation_name             | cpu=XXXXX | mem=XXXXX
```

### CI Automation

A GitHub Actions workflow (`.github/workflows/gas-check.yml`) should:

1. Build the contract in release mode
2. Run `test_gas_profile` with `--nocapture`
3. Parse output and compare against `docs/budget.md` baselines
4. Fail if any entry point exceeds 5% regression threshold
5. Post regression report as a PR comment

**To be added:** `.github/workflows/gas-check.yml` with regression detection logic.

## Updating Baselines

When a refactor intentionally improves performance:

1. Run `cargo test --lib test_gas_profile -- --nocapture` to capture new costs
2. Update this document with new baselines and justification
3. Commit the change with a message explaining the optimization
4. A 0% regression on intentional optimizations is preferred; if small cost increase is unavoidable, document why in the commit

## Hot Paths (Priority Monitoring)

The following entry points are user-facing and should be optimized first:

- `create_escrow` — called for every escrow creation
- `create_basket_escrow` — called for multi-token escrows
- `fund_escrow` — called for every payment
- `confirm_delivery` — called for every release
- `co_signed_release` — called for co-signed payout
- `auto_release` — called when auto-release triggers
- `ensure_action_not_paused` — guard function, runs on most operations

## Notes

- All costs measured on Soroban testnet using `Env::budget()`.
- Memory costs are secondary; CPU is the primary constraint.
- Gas costs may vary slightly per platform (Windows/Linux/macOS) due to timing variations; ±2% variance is acceptable.
- Baselines were established on `rustup stable` and may change with compiler upgrades; re-baseline if toolchain updates significantly.
