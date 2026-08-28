# TrustLink Escrow Contract — Audit Scope

**Contract Name:** TrustLink Escrow
**Network:** Stellar Soroban (testnet & mainnet)
**Language:** Rust (soroban-sdk v26)
**Audit Focus:** Smart contract security, economic model, state machine correctness

---

## Executive Summary

TrustLink is a Soroban smart contract that implements **trustless peer-to-peer escrow** on Stellar. The contract enables buyer-seller transactions with optional dispute resolution, featuring:

- **Three-party model**: Seller (recipient), Buyer (funder), Resolver (mediator)
- **Atomic state machine**: 8 states with validated transitions
- **SEP-41 token integration**: Works with any Stellar token (USDC, native assets, etc.)
- **Configurable fees**: Protocol fee + arbitration fee with admin controls
- **Permissionless reads**: Full escrow record accessibility
- **Authorization-based access control**: Each role must sign to act

---

## Contract Scope

### Deployable Artifact

```
contracts/escrow/target/wasm32v1-none/release/trustlink_escrow.wasm
```

**Size**: < 1 MB (optimized with wasm-opt)
**Soroban SDK Version**: 26.0.1
**Rust Toolchain**: 1.94.0 (stable)

### Included in This Audit

✅ **In Scope:**
1. **Core contract logic** (`contracts/escrow/src/lib.rs`)
   - All 40+ contract entry points
   - State machine transitions
   - Authorization checks
   - Fee calculations and transfers

2. **Data types and storage** (`contracts/escrow/src/types.rs`)
   - `EscrowData` structure and invariants
   - `DataKey` definitions
   - State machine enums

3. **Error handling** (`contracts/escrow/src/errors.rs`)
   - 40+ error codes
   - Contract-specific error conditions

4. **Event emission** (`contracts/escrow/src/events.rs`)
   - Event schema validation
   - Event ordering guarantees

5. **Helper functions** (`contracts/escrow/src/helpers/`)
   - Fee calculation logic
   - Payout routing
   - Authorization utilities

6. **Economic model**
   - Fee structure (protocol fee + arbitration fee)
   - Payment distribution logic
   - Fee collector mechanics

❌ **Out of Scope:**
- TypeScript bindings (`bindings/`)
- Indexer infrastructure (`indexer/`)
- Off-chain evidence storage
- Frontend implementations
- Deployment infrastructure
- CI/CD pipeline

---

## Entry Points

### State-Changing Operations (Require Auth)

| Function | Caller | State Guard | Cost | Purpose |
|----------|--------|-------------|------|---------|
| `initialize` | Deployer | First call only | Low | Set admin, fee collector, arbitration fee |
| `create_escrow` | Seller | None | Medium | Create pending escrow record |
| `fund_escrow` | Buyer | Pending → Funded | High | Lock tokens into contract |
| `confirm_delivery` | Buyer | Funded → Completed | High | Release to seller |
| `raise_dispute` | Buyer | Funded → Disputed | Medium | Escalate to resolver |
| `resolve_dispute` | Resolver | Disputed → Completed/Refunded | High | Settle dispute |
| `auto_release` | Anyone* | Funded → Completed (after window) | High | Auto-settle after shipping window |
| `mark_shipped` | Seller | Funded | Low | Record tracking ID |
| `record_delivery` | Admin | Funded | Low | Timestamp delivery |
| `cancel_escrow` | Seller | Pending/Funded | Medium | Abort unfunded escrow |
| `pause_contract` | Admin | None | Low | Block all state changes |
| `unpause_contract` | Admin | None | Low | Resume operations |
| `set_admin` | Admin | None | Low | Rotate admin address |
| `set_fee_collector` | Admin | None | Low | Update fee recipient |
| `set_protocol_fee` | Admin | None | Low | Update fee basis points |
| `set_arbitration_fee` | Admin | None | Low | Update dispute fee |

*`auto_release` is permissionless but only succeeds after `shipping_window` has elapsed.

### Read-Only Operations (No Auth)

| Function | Purpose | Access Pattern |
|----------|---------|-----------------|
| `get_escrow` | Retrieve full escrow record | By ID |
| `get_escrows_by_ids` | Batch read by multiple IDs | Up to 100 at once |
| `get_escrows_by_buyer` | Query buyer's escrows | Paginated by buyer |
| `get_escrows_by_seller` | Query seller's escrows | Paginated by seller |
| `get_total_escrow_count` | Count of all escrows | Global counter |
| `get_fee_config` | View current fee settings | Public |
| `get_total_arbitration_fees` | Sum of arbitration fees by token | By token |
| `is_contract_paused` | Check pause status | Public |

---

## Threat Model

See `THREATS.md` for detailed threat analysis.

### High-Risk Areas

1. **Authorization Bypass**
   - Missing or incorrect `require_auth()` calls
   - Wrong address matched for role
   - Caller identity spoofing

2. **Arithmetic Overflow/Underflow**
   - Fee calculations: `amount * bps / 10000`
   - Timestamp arithmetic: `now + window`
   - Counter increments

3. **State Machine Bypass**
   - Invalid state transitions
   - Premature payout before state update
   - Missing state guards

4. **Token Transfer Vulnerabilities**
   - Reentrancy during transfers
   - Missing balance checks post-transfer
   - Token contract failure not propagated

5. **Fee Misrouting**
   - Fees lost due to calculation errors
   - Fees sent to wrong address
   - Double-charging fees

---

## Invariants to Validate

See `INVARIANTS.md` for complete list.

### Critical Invariants

**I1: No tokens stranded in contract**
- All tokens locked in an escrow must route to seller, buyer, or fee collector
- Contract balance = sum of all active escrow amounts + accumulated fees

**I2: Authorization always precedes state reads**
- `require_auth()` must be the first line (or immediately after pause check)
- No exception to this rule

**I3: State machine is acyclic and deterministic**
- Each state transition requires specific trigger
- Transitions form a DAG with no cycles
- Terminal states (Completed, Refunded) permit no further transitions

**I4: Fees never exceed amount**
- `fee_bps <= 10000` (hard capped at 100%)
- `fee = amount * bps / 10000` rounds down
- `net_payout = amount - fee >= 0` always

**I5: Role separation is enforced**
- Buyer ≠ Seller, Buyer ≠ Resolver, Seller ≠ Resolver
- Cannot exploit role confusion for unauthorized state changes

---

## Test Coverage Expected

- **Unit tests**: 60+ files, 500+ test cases
- **Integration tests**: Happy path, edge cases, error conditions
- **Property-based tests** (fuzzing): Fee calculations, state transitions
- **Coverage**: > 95% line coverage, all branches tested

---

## Configuration & Constants

### Hard Limits

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_COMBINED_FEE_BPS` | 300 (3%) | Fee cap to prevent abuse |
| `DISPUTE_WINDOW` | 172,800 (48 hours) | Time to raise dispute post-funding |
| `MIN_ESCROW_AMOUNT` | 1 | Dust guard; all amounts ≥ 1 |
| `MAX_DESCRIPTION_LEN` | 512 | String length limit |
| `MAX_TRACKING_ID_LEN` | 128 | Tracking ID field limit |

### Configurable Parameters

| Parameter | Default | Admin Settable | Validation |
|-----------|---------|----------------|-----------|
| `protocol_fee_bps` | 0 | Yes | ≤ MAX_COMBINED_FEE_BPS |
| `arbitration_fee_bps` | 0 | Yes | ≤ MAX_COMBINED_FEE_BPS |
| `admin` | Deployer | Yes | Not sender |
| `fee_collector` | Deployer | Yes | Not null |

---

## Security Assumptions

### Trust Model

- **Admin is trusted**: Can pause, update fees, withdraw accumulated fees
- **Resolver is trusted**: Decides dispute outcomes (unappealable)
- **Token contracts are correct**: Contract assumes SEP-41 implementations are honest
- **Timestamp is accurate**: Soroban ledger timestamp is monotonic and trusted

### Adversarial Assumptions

- **Buyer may withhold tokens**: Partial fund attempt caught by token contract
- **Seller may raise false disputes**: Resolver decides based on evidence
- **Resolver may collude with one party**: Contract cannot prevent, only audit via events
- **Tokens may be deflationary**: Fee calculations don't account for token-specific nuances

---

## Key Files for Auditor

| File | Lines | Purpose |
|------|-------|---------|
| `contracts/escrow/src/lib.rs` | 3000+ | Main contract logic |
| `contracts/escrow/src/types.rs` | 200+ | Data structures |
| `contracts/escrow/src/errors.rs` | 100+ | Error definitions |
| `contracts/escrow/src/helpers/` | 500+ | Utility functions |
| `contracts/escrow/src/test*.rs` | 5000+ | Comprehensive test suite |
| `ARCHITECTURE.md` | — | Design documentation |
| `INVARIANTS.md` | — | Contract invariants |
| `THREATS.md` | — | Threat model |
| `SECURITY.md` | — | Security guidelines |

---

## Deployment Timeline

| Phase | Duration | Activity |
|-------|----------|----------|
| **Code Freeze** | Day 0 | No new features; freeze main branch |
| **Testnet Deployment** | Days 1-3 | Deploy to testnet; run smoke tests |
| **Audit Period** | Days 4-21 | Internal/external audit |
| **Remediation** | Days 22-28 | Fix audit findings |
| **Mainnet Deployment** | Day 29+ | Deploy to mainnet (irreversible) |

---

## Auditor Deliverables

1. **Finding Report**
   - Critical, High, Medium, Low, Informational severities
   - Root cause analysis for each finding
   - Reproduction steps for exploitable findings

2. **Remediation Plan**
   - Fix priority and timeline
   - Code changes for each finding
   - Regression test plan

3. **Sign-Off**
   - Clearance to mainnet deployment
   - Known limitations or residual risks
   - Recommendations for ongoing monitoring

---

## Contact & Escalation

- **Technical Lead**: Available for architecture questions
- **Security Contact**: For sensitive findings
- **Emergency Contact**: For critical issues discovered post-deployment

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-06-30 | Initial mainnet release |
| — | — | — |

