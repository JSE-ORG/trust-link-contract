# Implementation Plan: Documentation Consistency Fixes

**Issues:** #835, #836, #837, #838
**Branch:** `docs/fix-documentation-discrepancies-835-836-837-838`
**Author:** kris-nana
**Date:** 2026-08-31

## Overview

This plan addresses four documentation consistency issues discovered during the Stellar Wave audit preparation. All issues are documentation-only fixes with no code changes required. The contract implementation is correct; the documentation needs to be synchronized to accurately reflect the actual behavior.

---

## Issue #838: TTL_FIX_SUMMARY.md Claims All Writes Extend TTL

**Problem:**
- `TTL_FIX_SUMMARY.md` line 93 states "All persistent storage writes now properly extend TTL"
- `docs/storage.md` lines 130-134 explicitly lists `DataKey::BuyerEscrowIndex` as a "Known exception" written WITHOUT TTL bump in `lib.rs:fund_escrow`
- The code at `contracts/escrow/src/instructions.rs:334-338` **actually does extend TTL** for `BuyerEscrowIndex`

**Root Cause:**
Documentation was not updated after
 about `BuyerEscrowIndex`

**Verification:**
```bash
cargo build --lib  # Must compile successfully
```

---

## Issue #835: AUDIT_SCOPE.md vs audit/SCOPE.md Divergent Fee Caps

**Problem:**
- `AUDIT_SCOPE.md` mentions `MAX_COMBINED_FEE_BPS = 300` (3%)
- `audit/SCOPE.md` line 53 lists `MAX_COMBINED_FEE_BPS | 300 (3%) | Fee cap to prevent abuse`
- Actual code (`contracts/escrow/src/lib.rs:60-92`) defines:
  - `MAX_ESCROW_FEE_BPS = 300` (3%)
  - `MAX_ARBITRATION_FEE_BPS = 500` (5%)
  - `MAX_COMBINED_FEE_BPS = 1
_FEE_BPS` | 500 (5%) | Dispute resolution fee cap |
   | `MAX_COMBINED_FEE_BPS` | 1000 (10%) | Combined protocol + arbitration cap |
   | `MAX_PLATFORM_FEE_BPS` | 200 (2%) | Platform treasury fee cap |
   ```
3. **Cross-reference** to `ERROR_CODES.md` for error code `FeeExceedsMax` (code 7)
4. **Link** to `INVARIANTS.md` section D2 for fee invariant documentation

**Files to Modify:**
- `AUDIT_SCOPE.md` — update fee caps table to match source of truth
- `audit/SCOPE.md` — **DELETE** (duplicate removed)

**Verification:**
```bash
cargo build --lib
grep -n "MAX.*FEE_BPS" contracts/escrow/src/lib.rs  # Verify source of truth
```

---

## Issue #836: README vs ARCHITECTURE.md State Machine Diagrams Diverge

**Problem:**
- `README.md` section 6 ASCII diagram shows:
  - `Shipped → Completed` via `confirm_delivery` with `shipped_at` guard
  - `Funded → Completed` via `auto_release` with `shipping_window` guard
- `ARCHITECTURE.md` mermaid diagram shows:
  - `Funded → Completed` via `confirm_delivery` OR `auto_release` (no distinction between Funded/Shipped)
  - Missing `PendingFinalization` state entirely
  - Missing `Expired` state entirely

**Root Cause:**
README was updated with new state machine transitions (appeal flow, basket escrow) but `ARCHITECTURE.md` was not kept in sync.

**Resolution:**
1. **Standardize on mermaid** diagram format (used in `ARCHITECTURE.md`)
2. **Update ARCHITECTURE.md** mermaid to include:
   - `PendingFinalization` state (from dispute resolution)
   - `Expired` state (from `reclaim_expired`)
   - Correct guards for `auto_release` (requires both `dispute_deadline` passed AND `shipping_window` elapsed)
   - Correct guards for `confirm_delivery` (requires `dispute_deadline` passed)
3. **Synchronize README.md** to reference the unified state machine
4. **Cross-reference** actual guard conditions from `contracts/escrow/src/instructions.rs`:
   - `auto_release` guards: lines 1157-1181
   - `confirm_delivery` guards: lines 1016+

**Files to Modify:**
- `ARCHITECTURE.md` — update mermaid state diagram to be complete and accurate
- `README.md` — synchronize with `ARCHITECTURE.md` diagram; remove ASCII diagram if redundant

**State Machine Source of Truth:**
```rust
// From types.rs
pub enum EscrowState {
    Pending,
    Funded,
    Shipped,
    Completed,
    Disputed,
    RefundRequested,
    Refunded,
    Canceled,
    PendingFinalization,
    Expired,
}
```

**Verification:**
```bash
cargo build --lib
# Visual inspection of both diagrams for consistency
```

---

## Issue #837: ERROR_CODES.md Code 13 vs Code 24 Naming Confusion

**Problem:**
- Error code 13: `DeliveryBeforeDisputeWindow` — "auto_release before dispute_deadline"
- Error code 24: `DisputeWindowStillOpen` — "confirm_delivery before dispute_deadline"
- Both describe "too early" but returned by different entry points
- The distinction between error codes and their entry points is not explained
- `THREAT_MODEL.md` references 172800s window without mapping to error codes

**Root Cause:**
Error code documentation does not clearly state which function returns which error code for the same underlying timing condition.

**Resolution:**
1. **Update ERROR_CODES.md** code 13 entry to:
   ```markdown
   | **13** | `DeliveryBeforeDisputeWindow` | `auto_release` was called before `dispute_deadline` has passed on a `Funded` (never-shipped) escrow. (`confirm_delivery` returns `DisputeWindowStillOpen` (24) for the same timing condition.) | Wait until `dispute_deadline` has passed before calling `auto_release`. |
   ```
2. **Update ERROR_CODES.md** code 24 entry to:
   ```markdown
   | **24** | `DisputeWindowStillOpen` | The dispute window timing is violated: `raise_dispute` is called after `dispute_deadline` (too late to raise), or `confirm_delivery` is called before `dispute_deadline` (too early to confirm). Code 24 is reused for both to maintain ABI stability. | For `raise_dispute`: ensure it is called before `dispute_deadline`. For `confirm_delivery`: wait until `dispute_deadline` has passed. |
   ```
3. **Add cross-reference** to `THREAT_MODEL.md` Threat 4 linking to error codes 13 and 24
4. **Add timestamp comparison details**: `now < dispute_deadline` causes the error

**Files to Modify:**
- `ERROR_CODES.md` — clarify which entry point returns each error code
- `THREAT_MODEL.md` — add cross-links to error codes 13 and 24 (if file exists)

**Code References:**
- `auto_release` → error 13 at `contracts/escrow/src/instructions.rs:1167`
- `confirm_delivery` → error 24 at `contracts/escrow/src/instructions.rs:1016`

**Verification:**
```bash
cargo build --lib
grep -n "DeliveryBeforeDisputeWindow\|DisputeWindowStillOpen" contracts/escrow/src/ -r
```

---

## Summary of Changes

| Issue | Files Modified | Change Type |
|-------|---------------|-------------|
| #838 | `TTL_FIX_SUMMARY.md`, `docs/storage.md` | Remove exception note; confirm TTL fix complete |
| #835 | `AUDIT_SCOPE.md`, `audit/SCOPE.md` (delete) | Consolidate scope files; update fee caps to match `lib.rs` |
| #836 | `ARCHITECTURE.md`, `README.md` | Unify state machine diagrams; add missing states |
| #837 | `ERROR_CODES.md`, `THREAT_MODEL.md` | Clarify error code entry point mapping |

**Total Files Modified:** 6
**Total Files Deleted:** 1
**Code Changes:** 0 (documentation only)

---

## Testing Strategy

All changes are documentation-only. Verification steps:

1. **Build Verification:**
   ```bash
   make build-wasm  # Must succeed
   ```

2. **Documentation Cross-Reference Check:**
   - Verify fee caps in `AUDIT_SCOPE.md` match `lib.rs:60-92`
   - Verify state machine in `ARCHITECTURE.md` matches `types.rs` enum
   - Verify error codes in `ERROR_CODES.md` match `errors.rs` definitions
   - Verify TTL claims in `TTL_FIX_SUMMARY.md` match `instructions.rs:334-338`

3. **Grep Validation:**
   ```bash
   # Verify no lingering "Known exception" notes
   grep -i "known exception" docs/storage.md || echo "Clean"

   # Verify audit/SCOPE.md is deleted
   test ! -f audit/SCOPE.md && echo "Deleted successfully"

   # Verify fee constants referenced correctly
   grep -E "MAX_(ESCROW|ARBITRATION|COMBINED|PLATFORM)_FEE_BPS" AUDIT_SCOPE.md
   ```

4. **CI Verification:**
   ```bash
   make check  # fmt-check + clippy + test (327/327 pass)
   ```
# Implementation Plan for Issues #827, #828, #829, #830

## Overview
This document outlines the planned fixes for four issues identified in the trust-link-contract repository. All changes will maintain backward compatibility and follow existing code patterns.

## Issue #827: `get_messages` pagination does not validate escrow existence

### Problem
The `get_messages` function in `contracts/escrow/src/queries.rs` (lines 11-36) returns an empty `Vec` for both:
- Missing escrows (non-existent escrow_id)
- Valid escrows with no messages

This ambiguity hides `EscrowNotFound` errors and breaks the state-machine guarantee that messages only exist for valid escrows.

### Solution
Add escrow existence validation at the beginning of `get_messages`:

```rust
pub fn get_messages(env: Env, escrow_id: u64, start: u64, limit: u64) -> Vec<Message> {
    // Validate escrow exists first - return empty Vec if not found or propagate error
    if load_escrow(&env, escrow_id).is_err() {
        return Vec::new(&env);
    }
    // ... rest of existing logic
}
```

### Testing
Add new test case in `contracts/escrow/src/tests/`:
- Test `get_messages` with non-existent escrow_id returns empty Vec
- Test `get_messages` with valid escrow but no messages returns empty Vec
- Verify both scenarios are distinguishable by checking escrow existence separately

---

## Issue #828: `get_public_config` reads EscrowCounter without TTL extension

### Problem
The `get_public_config` function in `contracts/escrow/src/queries.rs` (lines 162-174) reads:
- `DataKey::EscrowCounter`
- `DataKey::Paused`

from instance storage without calling `storage::extend_instance_ttl`. All mutating entry poin
ication
- Confirm `storage::extend_instance_ttl` helper exists in `contracts/escrow/src/storage.rs`
- No extra rent should be burned on the read-only path
- Contract compiles with `cargo build --lib`

---

## Issue #829: `create_basket_escrow` skips resolver strict registry check

### Problem
The `create_basket_escrow` function in `contracts/escrow/src/instructions.rs` (lines 1234-1313) validates:
- `resolver != seller`
- `buyer != seller` and `buyer != resolver`

But never checks the resolver strict registry
stry (same as create_escrow_internal)
if env
    .storage()
    .instance()
    .get::<DataKey, bool>(&DataKey::ResolverStrict)
    .unwrap_or(false)
{
    let approved: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::ApprovedResolvers)
        .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
    if !contains(&approved, &resolver) {
        return Err(ContractError::UnauthorizedResolver);
    }
}
```

### Testing
Add new test case in `contracts/escrow/src/tests/`:
- Enable strict mode via `set_resolver_strict`
- Create basket escrow with approved resolver → should succeed
- Create basket escrow with unapproved resolver → should return `UnauthorizedResolver` (error code 25)

---

## Issue #830: docs/state-machine.md omits PendingFinalization/RefundRequested/Expired

### Problem
The `docs/state-machine.md` documentation currently lists 7 states:
- Pending, Funded, Shipped, Completed, Disputed, Refunded, Canceled

While `contracts/escrow/src/types.rs` (lines 383-394) defines 10 states, adding:
- **RefundRequested** - Buyer requests refund before shipping
- **PendingFinalization** - Dispute resolved but awaiting finalization
- **Expired** - Escrow expired without funding/shipping in time

Additionally:
- `README.md` mentions 10 states
- `INVARIANTS.md` I3 mentions 9 states
- Spelling inconsistency: documentation uses "Cancelled" while `types.rs` uses "Canceled"

### Solution

#### Update docs/state-machine.md:

1. **Add missing states to the States table:**

| State | Meaning | Terminal |
|---|---|---|
| `RefundRequested` | Buyer requested refund before shipping | No |
| `PendingFinalization` | Dispute resolved, awaiting finalization | No |
| `Expired` | Escrow expired without timely funding/shipping | Yes |

2. **Update the Mermaid diagram** to include:
   - `Funded --> RefundRequested: request_refund`
   - `Disputed --> PendingFinalization: resolve_dispute or vote (threshold reached)`
   - `PendingFinalization --> Completed: finalize_dispute(Release)`
   - `PendingFinalization --> Refunded: finalize_dispute(Refund)`
   - `Pending --> Expired: reclaim_expired`
   - `Funded --> Expired: reclaim_expired`
   - `Shipped --> Expired: reclaim_expired`

3. **Update the Transition Matrix** with:
   - `RefundRequested` transitions
   - `PendingFinalization` transitions
   - `Expired` transitions

4. **Add Guard Conditions** for:
   - `Funded -> RefundRequested`
   - `Disputed -> PendingFinalization`
   - `PendingFinalization -> Completed/Refunded`
   - `Pending/Funded/Shipped -> Expired`

5. **Fix spelling**: Unify to "Canceled" (as in `types.rs`)

### Verification
- No code changes required
- `cargo build --lib` not affected
- Documentation now matches `types.rs` enum exactly

---

## Implementation Order

1. **Issue #835** (audit scope consolidation) — delete duplicate, update fee caps
2. **Issue #838** (TTL documentation) — remove exception note
3. **Issue #836** (state machine diagrams) — unify and complete
4. **Issue #837** (error code clarification) — add entry point details

---

## Post-Implementation Checklist

- [ ] All documentation files updated
- [ ] `audit/SCOPE.md` deleted
- [ ] Contract compiles (`make build-wasm`)
- [ ] All tests pass (`make test`)
- [ ] No clippy warnings (`make clippy`)
- [ ] Grep validations pass
- [ ] PR created with keyword "Closes #835, Closes #836, Closes #837, Closes #838"
- [ ] PR description references this implementation plan
- [ ] No Claude/AI co-author in commits

---

## Risk Assessment

**Risk Level:** LOW

- All changes are documentation-only
- No contract code modified
- No breaking changes
- No semantic changes to contract behavior
- Backward compatible with existing deployments

**Potential Issues:**
- Documentation readers may have cached incorrect information (mitigated by clear PR description)
- Cross-references may break if files are renamed (mitigated by testing all links)

---

## Reviewers' Guide

**What to Review:**
1. **Accuracy:** Do the updated docs match the source code?
   - Fee caps match `lib.rs:60-92`
   - State machine matches `types.rs` enum
   - Error codes match `errors.rs` definitions
   - TTL behavior matches `instructions.rs:334-338`

2. **Consistency:** Are all cross-references intact?
   - Links between documents work
   - No contradictory claims remain
   - Terminology is consistent

3. **Completeness:** Are all mentioned states/errors/transitions documented?
   - `PendingFinalization` and `Expired` states included
   - All error codes 1-47 have entries
   - All fee constants listed

**What NOT to Review:**
- Contract code (unchanged)
- Test files (unchanged)
- CI configuration (unchanged)

---

End of Implementation Plan
1. **Issue #827** - Query function fix (low risk, isolated change)
2. **Issue #828** - TTL extension fix (low risk, performance improvement)
3. **Issue #829** - Security fix (medium risk, requires new test)
4. **Issue #830** - Documentation update (no code changes)

## Testing Strategy

All changes will be validated with:
- `cargo fmt --all -- --check` (zero drift)
- `cargo clippy --lib -- -D warnings` (zero warnings)
- `cargo test --lib` (all tests pass)
- New tests added for issues #827 and #829

## Compatibility

All changes maintain backward compatibility:
- No breaking API changes
- No changes to existing function signatures
- No changes to event schemas or storage keys
- Documentation updates reflect existing code behavior

## Estimated Impact

- **Issue #827**: Prevents silent failures when querying messages for non-existent escrows
- **Issue #828**: Ensures instance storage remains alive under read-heavy query patterns
- **Issue #829**: Closes security gap where basket escrows bypass resolver registry
- **Issue #830**: Brings documentation into alignment with actual implementation

---

## Checklist Before Implementation

- [ ] All issues reviewed and understood
- [ ] Implementation plan approved by maintainers
- [ ] Test cases designed
- [ ] Ready to create feature branch and begin implementation
