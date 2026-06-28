---
name: Release Checklist
about: Checklist to follow before publishing a new contract release
title: "Release vX.Y.Z"
labels: release
assignees: ''
---

## Pre-Release Checklist

### Code
- [ ] All issues targeted for this release are closed or explicitly deferred
- [ ] `CHANGELOG.md` updated with release notes
- [ ] Version bumped in `Cargo.toml` and `bindings/package.json`
- [ ] No uncommitted changes (`git status` is clean)

### Build
- [ ] `cargo build --target wasm32-unknown-unknown --release` succeeds
- [ ] WASM binary optimized (`wasm-opt -Oz`)
- [ ] `bindings/` TypeScript build passes (`npm run typecheck`)

### Tests
- [ ] All Rust unit/integration tests pass (`cargo test`)
- [ ] TypeScript binding tests pass
- [ ] No new `clippy` warnings (`cargo clippy -- -D warnings`)

### Security
- [ ] Fee caps verified against constants in `lib.rs` (`MAX_COMBINED_FEE_BPS` etc.)
- [ ] State transition matrix reviewed (`transition_state` in `lib.rs`)
- [ ] No new `unsafe` blocks introduced
- [ ] Auth ordering reviewed (all `require_auth()` calls precede state reads)

### Contract Deployment
- [ ] Contract deployed to testnet and smoke-tested
- [ ] `initialize` called with correct admin, fee_collector, and arbitration_fee_bps
- [ ] Event emission verified for at least one full escrow lifecycle on testnet
- [ ] Contract ID recorded in deployment log

### Post-Release
- [ ] GitHub release created with WASM artifact attached
- [ ] `bindings/` package published (if public)
- [ ] Deployment announcement sent to relevant channels
- [ ] Next milestone opened

## Notes

<!-- Any additional context for this release -->
