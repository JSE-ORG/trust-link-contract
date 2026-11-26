# TrustLink Contract — Audit Scope

## Contract Overview

**Project**: TrustLink Escrow Contract
**Network**: Stellar Soroban
**Language**: Rust
**Soroban SDK Version**: 26.0.1
**Contract Type**: Escrow / Payment Settlement

## Scope

### In Scope
- `contracts/escrow/src/lib.rs` — main contract implementation
- `contracts/escrow/src/types.rs` — data structures and storage keys
- `contracts/escrow/src/errors.rs` — error handling
- `contracts/escrow/src/events.rs` — event emission
- `contracts/escrow/src/storage.rs` — storage helpers
- `contracts/escrow/src/helpers/` — fee calculations and payout logic
- State machine transitions and lifecycle
- Authorization and authentication patterns
- Token transfers and fund management
- Fee calculation and accumulation
- TTL/persistent storage management

### Out of Scope
- TypeScript bindings (`bindings/` directory)
- Off-chain indexers or Mercury configurations
- UI/UX implementation
- Deployment scripts and CI/CD
- Test suite implementation details
- Documentation and comments

## Key Functions Under Review

| Function | Risk Level | Notes |
|----------|-----------|-------|
| `create_escrow` | High | State creation, seller auth |
| `fund_escrow` | Critical | Token transfer, buyer auth, role validation |
| `confirm_delivery` | High | Payout release to seller |
| `raise_dispute` | High | State change, evidence validation |
| `resolve_dispute` | Critical | Conditional payout, fee handling |
| `auto_release` | High | Permissionless, time-dependent release |
| `record_delivery` | Medium | Admin-only delivery timestamp |
| `pause_contract` / `unpause_contract` | High | Admin controls |
| Fee calculation helpers | Critical | Arithmetic correctness |
| Storage read/write | Medium | TTL management |

## Entry Points

1. **create_escrow(seller, payees, resolver, token, amount, fee_bps, shipping_window)** → u64
2. **fund_escrow(escrow_id, buyer)** → Result
3. **confirm_delivery(escrow_id)** → Result
4. **raise_dispute(escrow_id, category, reason, evidence_hash)** → Result
5. **resolve_dispute(escrow_id, resolution)** → Result
6. **auto_release(escrow_id)** → Result
7. **record_delivery(escrow_id)** → Result (admin-only)
8. **initialize(admin, fee_collector, arbitration_fee_bps)** → Result
9. **set_protocol_fee(admin, fee_bps)** → Result
10. **pause_contract(admin)** → Result
11. **get_escrow(escrow_id)** → EscrowData
12. **multicall(calls)** → Vec (batching)

## Critical Data Flows

### Escrow Creation
```
create_escrow()
├─ Generate escrow_id from counter
├─ Store EscrowData in persistent storage
├─ Emit create_escrow event
└─ Return escrow_id
```

### Funding & Token Transfer
```
fund_escrow(escrow_id, buyer)
├─ Validate buyer auth (require_auth)
├─ Load escrow from storage
├─ Validate state is Pending
├─ Validate role separation (buyer ≠ seller ≠ resolver)
├─ Transfer buyer's tokens to contract
├─ Update escrow state to Funded
├─ Save escrow to storage
└─ Emit fund_escrow event
```

### Dispute Resolution
```
resolve_dispute(escrow_id, resolution)
├─ Validate resolver auth
├─ Load escrow + dispute data
├─ Calculate arbitration fee
├─ Deduct arbitration fee from amount
├─ Calculate protocol fee on reduced amount
├─ Transfer net amount to recipient
├─ Transfer fees to fee_collector
├─ Update escrow state
└─ Emit dispute_resolved event
```

### Auto-Release (Permissionless)
```
auto_release(escrow_id)
├─ Load escrow from storage
├─ Validate state is Shipped/Funded
├─ Validate shipping_window elapsed
├─ Transfer amount to seller (no auth required)
├─ Update state to Completed
└─ Emit auto_released event
```

## Storage Schema

| Key | Type | Persistence | Notes |
|-----|------|-------------|-------|
| `Escrow(id)` | EscrowData | Persistent | Full escrow record |
| `EscrowCounter` | u64 | Instance | Monotonic ID generator |
| `Admin` | Address | Instance | Contract admin |
| `FeeCollector` | Address | Instance | Fee recipient |
| `Paused` | bool | Instance | Global pause flag |
| `FeeConfig` | FeeConfig | Instance | Protocol + arbitration fees |
| `DisputeData(id)` | DisputeData | Persistent | Dispute records |
| `BuyerEscrowIndex(buyer)` | Vec<u64> | Persistent | Buyer's escrow IDs |
| `SellerEscrowIndex(seller)` | Vec<u64> | Persistent | Seller's escrow IDs |

## Token Interactions

- **SEP-41 compliant** token at `EscrowData.token`
- **Transfer operations**:
  - Buyer → Contract (fund_escrow)
  - Contract → Seller (confirm_delivery, auto_release, resolve_dispute)
  - Contract → Buyer (resolve_dispute refund)
  - Contract → Fee Collector (fee withdrawals)
- **No approve()** calls; direct transfer only
- **Contract holds funds** until terminal state reached

## Authorization Model

| Operation | Required Auth | Validation |
|-----------|---------------|-----------|
| create_escrow | seller | require_auth() |
| fund_escrow | buyer | require_auth() + stored buyer check |
| confirm_delivery | buyer | require_auth() + stored buyer check |
| raise_dispute | buyer | require_auth() + stored buyer check |
| resolve_dispute | resolver | require_auth() + stored resolver check |
| auto_release | none | Time check only |
| record_delivery | admin | require_auth() + Admin storage check |
| initialize | contract | One-time, no auth (called via deploy) |
| pause_contract | admin | require_auth() + Admin storage check |

## State Machine

```
Pending ──fund──> Funded ──confirm──> Completed (terminal)
                    ├──dispute──> Disputed ──resolve──> Completed/Refunded (terminal)
                    └──auto_release──> Completed (terminal)

RefundRequested (optional flow):
Funded ──request_refund──> RefundRequested ──approve/deny──> Completed/Funded
```

## Fee Model

### Protocol Fee (Variable)
- Applied on fund amount after arbitration fee (if any)
- Default: 0 BPS (0%)
- Maximum: 1000 BPS (10%)
- Collected to `fee_collector`

### Arbitration Fee (Per-Escrow)
- Fixed at escrow creation time via `fee_bps` parameter
- Deducted from escrow amount in dispute resolution
- Default: 0 BPS (0%)
- Maximum: 1000 BPS (10%)
- Collected to `fee_collector`

### Fee Arithmetic
```
if dispute:
  arbitration_fee = amount * arbitration_fee_bps / 10000
  adjusted_amount = amount - arbitration_fee
  protocol_fee = adjusted_amount * protocol_fee_bps / 10000
  net_payout = adjusted_amount - protocol_fee
  fees_to_collector = arbitration_fee + protocol_fee
```

## Known Constraints

1. **Escrow IDs** are u64; counter never resets (infinite but practical limit)
2. **Shipping window** is u64 seconds; no upper bound validation
3. **Evidence hash** must be exactly 32 bytes (SHA-256)
4. **Token addresses** are not whitelisted; any SEP-41 token accepted
5. **Amounts** are i128; no decimal handling (stroops/smallest unit only)
6. **Dispute window** is 172800 seconds (2 days) hardcoded
7. **TTL management** uses env.ledger() for timestamp
8. **Paused state** is global; affects all operations except read-only views

## Attack Surface

### Potential Attack Vectors
1. **Role confusion** — buyer/seller/resolver overlap
2. **Reentrancy** — token transfer callbacks
3. **Front-running** — state-dependent transactions
4. **Arithmetic overflow** — fee calculations, timestamp additions
5. **Unauthorized state transitions** — invalid state paths
6. **Token blacklist** — malicious tokens accepting/rejecting transfers
7. **Timestamp manipulation** — dispute window, shipping window
8. **Storage collision** — persistent vs instance storage conflicts
9. **TTL expiration** — storage data loss mid-operation
10. **Fee manipulation** — excessive fee configuration

## Audit Focus Areas

1. **Authorization checks** — all require_auth() placements
2. **State transition validation** — correct state guards
3. **Arithmetic correctness** — overflow/underflow in fee math
4. **Token safety** — transfer callback handling
5. **Storage persistence** — TTL extension correctness
6. **Event accuracy** — correct event emissions
7. **Dispute logic** — fee deduction order and correctness
8. **Permissionless operations** — time-based guards on auto_release
9. **Admin functions** — pause/fee updates
10. **Edge cases** — duplicate operations, zero amounts, boundary conditions

## Artifacts Provided

- `ARCHITECTURE.md` — detailed design documentation
- `THREAT_MODEL.md` — threat analysis and mitigations
- `INVARIANTS.md` — contract invariants and assumptions
- `SECURITY.md` — security guidelines and recommendations
- Source code with inline security comments
- Comprehensive test suite (60+ test files)
- Fuzzing targets for core logic

## Testing

- **Unit tests**: 60+ test files covering all entry points
- **Integration tests**: End-to-end escrow flows
- **Fuzzing**: Core fee calculation and state transition logic
- **Property tests**: Invariant verification

## Delivery Schedule

Phase 1: Code review
Phase 2: Testing & fuzzing
Phase 3: Report & remediation

---

**Prepared for external audit by**: [TrustLink Team]
**Date**: June 2026
**SDK Version**: Soroban 26
**Rust Edition**: 2021
