# Contributing to TrustLink — Soroban Escrow Contract

Thank you for your interest in contributing to TrustLink! This repository is the trustless core of the TrustLink protocol — a Soroban smart contract written in Rust that powers secure escrow for social commerce on Stellar.

We welcome contributions of all kinds: bug fixes, new features, tests, documentation improvements, and security reviews. Every merged contribution moves us closer to making fraud-free social commerce a reality for millions of people.

---

## ⚡ Quick Start (≈ 10 minutes)

If you already have Rust installed, you can go from zero to a passing test run with the commands below. The detailed explanation of each step follows in the sections later.

```bash
# 1. Fork this repo on GitHub, then clone YOUR fork
git clone https://github.com/YOUR_USERNAME/trust-link-contract.git
cd trust-link-contract

# 2. Add the original repo as "upstream" so you can pull updates later
git remote add upstream https://github.com/JSE-ORG/trust-link-contract.git

# 3. Build the contract (the wasm target is installed automatically
#    from rust-toolchain.toml the first time you build)
cargo build --workspace --release

# 4. Run the full test suite — everything should pass on a clean checkout
cargo test --workspace
```

If `cargo test --workspace` ends with `test result: ok`, your environment is ready. Total time for a first-time contributor with Rust already installed is well under 30 minutes (the first build downloads and compiles dependencies, which takes a few minutes).

---

## 📋 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Stellar Wave Program](#stellar-wave-program)
- [Before You Start](#before-you-start)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Making Changes](#making-changes)
- [Building & Testing](#building--testing)
- [Commit Convention](#commit-convention)
- [Pull Request Process](#pull-request-process)
- [Writing Tests](#writing-tests)
- [Security Vulnerabilities](#security-vulnerabilities)
- [Getting Help](#getting-help)

---

## Code of Conduct

This project follows a simple rule: **be respectful, be constructive, be helpful**. We're building open infrastructure for underserved commerce communities — everyone who contributes deserves a welcoming environment regardless of experience level.

Harassment, gatekeeping, or dismissive behaviour will not be tolerated. Report issues to the maintainers via the contact below.

---

## 🌊 Stellar Wave Program

This repository participates in the **[Stellar Wave Program](https://www.drips.network/wave/stellar)** — a funded, sprint-based contribution initiative by the Stellar Development Foundation. During active Wave cycles, contributors can earn real rewards for resolving labelled issues.

### How Wave Contributions Work

1. Browse issues tagged [`Stellar Wave`](../../issues?q=label%3A%22Stellar+Wave%22)
2. Sign in at [drips.network/wave](https://www.drips.network/wave) with your GitHub account
3. Apply to the issue you want to work on
4. Wait to be assigned by a maintainer (we review applications promptly)
5. Submit a Pull Request before the Wave cycle ends
6. Earn Points that translate to XLM rewards

### Issue Point Values

| Complexity Label | Points | Typical Scope |
|---|---|---|
| `complexity: trivial` | 100 pts | Typo, comment fix, minor error code addition |
| `complexity: medium` | 150 pts | New test case, view function, bug fix |
| `complexity: high` | 200 pts | New contract function, refactor, security improvement |

> ⚡ **Speed matters.** Maintainers assign contributors quickly during active Waves — apply early and have your dev environment ready before you apply.

---

## Before You Start

### Find Something to Work On

- **New to Soroban?** → Start with [`good first issue`](../../issues?q=label%3A%22good+first+issue%22) labels. These are deliberately scoped and well-documented.
- **Experienced with Rust/Soroban?** → Look at [`complexity: high`](../../issues?q=label%3A%22complexity%3A+high%22) issues or check the roadmap section of the README.
- **Have an idea?** → Open a [GitHub Discussion](../../discussions) first before building. This prevents duplicate effort and ensures your PR gets merged.

### Check Before You Build

- Is there already an open PR for this issue? Check the issue's linked PRs.
- Is there a comment from a maintainer saying the approach has changed? Read the full thread.
- For anything beyond a trivial fix — comment on the issue to briefly describe your intended approach. A maintainer will confirm before you spend time building.

---

## Development Setup

### Prerequisites

| Tool | Version | Install | Required? |
|---|---|---|---|
| Rust (via rustup) | `1.75+` (stable) | [rustup.rs](https://rustup.rs) | ✅ Required |
| `wasm32v1-none` target | latest | Auto-installed from `rust-toolchain.toml` | ✅ Required |
| Stellar CLI | `21+` | [Stellar Docs](https://developers.stellar.org/docs/tools/stellar-cli) | Optional — only for deploying/invoking on a network |
| `binaryen` (`wasm-opt`) | latest | `apt install binaryen` / `brew install binaryen` | Optional — only for optimized release builds via `build.sh` |

> **Note on the wasm target:** This project targets `wasm32v1-none` (the modern Soroban target), pinned in [`rust-toolchain.toml`](rust-toolchain.toml). When you use `rustup`, the correct toolchain and target are installed automatically the first time you run a `cargo` command in this repo — you do **not** need to add the target manually.

### First-Time Setup

```bash
# 1. Fork this repo on GitHub, then clone your fork
git clone https://github.com/YOUR_USERNAME/trust-link-contract.git
cd trust-link-contract

# 2. Add the upstream remote
git remote add upstream https://github.com/JSE-ORG/trust-link-contract.git

# 3. Make sure your stable toolchain is up to date
rustup update stable

# 4. Verify everything builds and tests pass on a clean checkout
cargo build --workspace --release
cargo test --workspace
```

### Staying Up to Date

```bash
git fetch upstream
git rebase upstream/main
```

Always rebase onto `main` before opening a PR.

---

## Project Structure

This is a Cargo **workspace**. The escrow contract lives under `contracts/escrow/`.

```
trust-link-contract/
├── Cargo.toml                  # Workspace manifest (members = contracts/*)
├── rust-toolchain.toml         # Pins stable + wasm32v1-none + clippy/rustfmt
├── build.sh                    # Optional optimized wasm build helper (wasm-opt)
├── bindings/                   # TypeScript client bindings
├── docs/                       # Architecture & protocol documentation
└── contracts/
    └── escrow/
        ├── Cargo.toml          # The trustlink-escrow package
        └── src/
            ├── lib.rs          # Contract entry point, public interface, and the
            │                   #   Escrow state-machine logic (transition_state, impl Escrow)
            ├── types.rs        # Shared structs & enums (EscrowData, EscrowState, …)
            ├── errors.rs       # All ContractError codes
            ├── events.rs       # On-chain event definitions and emitters
            ├── storage.rs      # Persistent storage helpers and storage-key constants
            ├── helpers/        # Internal helpers (e.g. payout calculation)
            ├── test.rs         # Core test module
            └── test_*.rs       # Focused test modules, declared as `mod test_*` in lib.rs
        └── tests/              # Integration tests (happy_path, edge_cases, auth_audit)
```

**Where to make changes:**

- **New contract feature** → add the logic and public function in `lib.rs`, plus an event in `events.rs` if it represents a meaningful state change.
- **New error code** → `errors.rs` only — never use raw `panic!()` in contract code.
- **New data fields** → `types.rs` — be mindful of storage cost on Stellar.
- **Storage changes** → `storage.rs` — keys must be defined as constants, never raw strings inline.
- **New tests** → add a focused module under `contracts/escrow/src/` and register it with `mod your_test;` in `lib.rs`, or add a scenario to the integration tests in `contracts/escrow/tests/`.

---

## Making Changes

### Branching

Always work on a feature branch off `main`:

```bash
git checkout main
git pull upstream main
git checkout -b feat/your-feature-name
```

Branch naming conventions:

| Type | Pattern | Example |
|---|---|---|
| New feature | `feat/short-description` | `feat/multi-asset-support` |
| Bug fix | `fix/short-description` | `fix/auto-release-double-sign` |
| Test addition | `test/short-description` | `test/dispute-edge-cases` |
| Documentation | `docs/short-description` | `docs/improve-storage-comments` |
| Refactor | `refactor/short-description` | `refactor/storage-key-naming` |

### Coding Standards

**General Rust**
- Run `cargo fmt --all` before every commit — CI rejects unformatted code.
- Run `cargo clippy --workspace -- -D warnings` — fix all warnings, never suppress them without a comment.
- Prefer explicit error returns over `unwrap()` — use `ContractError` variants from `errors.rs`.
- Comment non-obvious logic. If you had to think about it for more than 30 seconds, leave a comment.

**Soroban-specific**
- Every state-mutating function must call `require_auth()` on the appropriate address before any logic.
- Use `env.storage().instance()` for contract-level data and `env.storage().persistent()` for per-escrow data — understand the cost difference.
- Emit an event in `events.rs` for every meaningful state transition — the backend oracle depends on these.
- Never use `env.storage().temporary()` for data that must survive ledger expiry.
- Storage keys must be defined as constants in `storage.rs` — no raw strings inline.

**Error Handling**
```rust
// ✅ Correct
if escrow.state != EscrowState::Funded {
    return Err(ContractError::InvalidState);
}

// ❌ Wrong — panics are not catchable and give no useful error code
if escrow.state != EscrowState::Funded {
    panic!("wrong state");
}
```

---

## Building & Testing

All commands are run from the repository root.

```bash
# Format
cargo fmt --all

# Lint — zero warnings allowed
cargo clippy --workspace -- -D warnings

# Standard (native) build
cargo build --workspace --release

# WASM build (the deployable artifact)
cargo build --workspace --release --target wasm32v1-none

# Run the full test suite — all must pass
cargo test --workspace

# Run a single test or a module
cargo test test_full_escrow_flow
cargo test --workspace -- --nocapture   # show println! output

# Optional: optimized wasm via wasm-opt (requires binaryen)
./build.sh
```

These four checks — `fmt`, `clippy`, `build`, and `test` — are exactly what the CI pipeline runs, so running them locally before pushing means no CI surprises.

---

## Commit Convention

This repo uses [Conventional Commits](https://www.conventionalcommits.org/). PRs with non-conventional commit messages will be asked to squash/rebase.

```
<type>(<scope>): <short imperative description>

[optional body]

[optional footer: closes #123]
```

**Types:**

| Type | When to use |
|---|---|
| `feat` | New contract function or capability |
| `fix` | Bug fix |
| `test` | Adding or improving tests |
| `docs` | Comments, README, or documentation changes |
| `refactor` | Code restructuring with no behaviour change |
| `chore` | Dependency updates, CI config, tooling |
| `security` | Security hardening or vulnerability fix |

**Examples:**

```bash
git commit -m "feat(escrow): add multi-asset support for SEP-41 tokens"
git commit -m "fix(auto-release): prevent double-signing when delivery timestamp races"
git commit -m "test(dispute): add edge case for expired dispute window"
git commit -m "docs(storage): clarify TTL behaviour for instance storage keys"
```

---

## Pull Request Process

### Before Opening a PR

Run the same checks CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo build --workspace --release --target wasm32v1-none
cargo test --workspace

# Sanity-check the WASM binary size hasn't ballooned unexpectedly
ls -lh target/wasm32v1-none/release/trustlink_escrow.wasm
```

### PR Checklist

When you open a PR, the description must include:

- [ ] **What** — A clear description of what changed and why
- [ ] **How** — Brief explanation of your approach (especially for non-obvious choices)
- [ ] **Tests** — What test cases did you add or modify?
- [ ] **Breaking changes** — Does this change the contract ABI? (requires extra review)
- [ ] **Issue reference** — `Closes #123` or `Relates to #123`

### PR Template

```markdown
## Summary
<!-- What does this PR do? -->

## Motivation
<!-- Why is this change needed? Link to the issue. -->

## Changes
<!-- List the key changes -->
-

## Test Coverage
<!-- What tests were added/modified? -->
-

## Notes for Reviewers
<!-- Anything the reviewer should pay special attention to? -->

Closes #
```

### Review Process

- A maintainer will review your PR within **48 hours** during active Wave cycles, and within **5 business days** otherwise.
- At least **1 approving review** is required to merge.
- For changes touching the release logic, dispute resolution, or fee calculation — **2 approving reviews** are required.
- The CI pipeline must be green (build + tests + clippy + fmt) before merge.
- Maintainers may request changes — please respond within 5 days or the PR may be closed.

### What Maintainers Look For

- Does the code actually solve the stated problem?
- Are the auth checks correct and in the right order?
- Is the new code covered by tests?
- Does the storage model make sense (cost, TTL)?
- Are events emitted for state changes the backend oracle needs to observe?
- Is the WASM binary size increase justified?

---

## Writing Tests

Tests use the `soroban_sdk::testutils` environment. There are two places tests live:

- **Unit / focused tests** — files named `test_*.rs` in `contracts/escrow/src/`, each registered with `mod test_*;` in `lib.rs`. Most tests live here, grouped by feature (fees, disputes, auth, TTL, …).
- **Integration tests** — files in `contracts/escrow/tests/` (`happy_path.rs`, `edge_cases.rs`, `auth_audit.rs`) that exercise full end-to-end flows.

### Test Structure

```rust
#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    #[test]
    fn test_full_escrow_flow() {
        let env = Env::default();
        // ... set up contract, vendor, buyer, token ...

        // 1. Create escrow
        let escrow_id = client.create_escrow(&vendor, &buyer, &token, &amount, &window);

        // 2. Buyer funds
        client.fund_escrow(&escrow_id);

        // 3. Vendor ships
        client.mark_shipped(&escrow_id, &String::from_str(&env, "TRK123"));

        // 4. Buyer confirms
        client.confirm_delivery(&escrow_id);

        // Assert vendor received funds minus fee
        assert_eq!(token_client.balance(&vendor), expected_payout);
    }
}
```

### Test Coverage Requirements

New features must include tests for:
1. The happy path (expected usage)
2. At least one unauthorized access attempt (wrong caller)
3. Invalid state transitions (calling a function out of sequence)
4. Edge case inputs (zero amounts, expired windows, etc.)

---

## Security Vulnerabilities

**Do not open a public GitHub issue for security vulnerabilities.**

If you discover a security issue in the contract — especially anything related to fund drainage, unauthorized release, or state manipulation — please report it privately to the maintainers:

📧 **security@trustlink.xyz**

Include:
- A description of the vulnerability
- Steps to reproduce or a proof-of-concept test
- Your assessment of the severity and impact

We will acknowledge within 48 hours and aim to patch within 7 days for critical issues.

---

## Getting Help

Stuck on the codebase? Have a question before diving in?

- 💬 **GitHub Discussions** → [Ask a question](../../discussions/categories/q-a) — preferred for anything technical
- 🐛 **GitHub Issues** → For confirmed bugs only — include reproduction steps
- 🌊 **Stellar Wave Discord** → Join the Stellar developer community at [discord.gg/stellardev](https://discord.gg/stellardev) for real-time help

If you're new to Soroban, these resources will get you up to speed quickly:
- [Soroban Documentation](https://developers.stellar.org/docs/build/smart-contracts/overview)
- [Stellar Developers Discord](https://discord.gg/stellardev)

---

> We appreciate every contribution — from a one-line comment fix to a full feature implementation. TrustLink is open infrastructure for real people. Thank you for helping build it.
