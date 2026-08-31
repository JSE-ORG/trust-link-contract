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
