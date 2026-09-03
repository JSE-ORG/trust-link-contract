# TrustLink Contract ? Invariants

## Core Invariants

These invariants must hold at the end of every transaction.

### I1: Balance Conservation

**Statement**: Total tokens held by contract = sum of all escrow amounts.

**Proof**:
- Contract only receives tokens via `fund_escrow` (adds to balance)
- Contract only sends via terminal states (removes from balance)
- Every escrow has exactly one terminal state (Completed, Refunded, Cancelled)
- Fees are deducted atomically with payout
- Multi-payee rounding dust is assigned to the first payee, bounded by `N - 1` stroops for `N` payees
- No tokens created or destroyed

**Implementation**:
```
contract_balance = sum(escrow.amount for all non-terminal escrows)
                 + accumulated_fees
```

**Verification**: Checksum tokens via query at end of test runs.

---

### I2: Escrow ID Uniqueness

**Statement**: Every `escrow_id` appears at most once in storage.

**Proof**:
- `escrow_id` generated via monotonic counter (incremented atomically)
- Counter never resets or decreases
- Each ID stored exactly once under `DataKey::Escrow(id)`
- No ID reuse or collision possible

**Implementation**:
```
escrow_id = read_and_increment(DataKey::EscrowCounter)
save_escrow(escrow_id, escrow_data)
```

**Verification**: Scan all persisted escrows; all IDs unique.

---

### I3: State Machine Validity

**Statement**: Every escrow state transition is in the approved matrix.

**Approved Transitions**:
```
Pending ? Funded | Cancelled
Funded ? Completed | Disputed | RefundRequested | Shipped | Cancelled
Disputed ? Completed | Refunded | PendingFinalization
RefundRequested ? Funded | Refunded
Shipped ? Completed
PendingFinalization ? Completed | Refunded
Completed, Refunded, Cancelled ? (terminal, no transitions)
```

**Implementation**: All state-modifying functions guard with:
```rust
if escrow.state != expected_state {
    return Err(InvalidState);
}
```

**Verification**: Coverage of all state transitions in test suite.

---

### I4: Role Separation

**Statement**: At the time of funding, `buyer ? seller` and `buyer ? resolver` and `seller ? resolver`.

**Proof**:
- Check performed in `fund_escrow` before state change
- Once Funded, roles are immutable
- Terminal states do not allow role changes

**Implementation**:
```rust
if buyer == seller || buyer == resolver || seller == resolver {
    return Err(ConflictingRoles);
}
```

**Verification**: All test escrows verify role separation.

---

### I5: Authorization Ordering

**Statement**: All `require_auth()` calls must precede all storage reads/writes.

**Proof**:
- Auth checks are stateless predicates; safe to reorder
- State reads after auth establish authority before acting
- Prevents confused deputy attacks

**Implementation**:
```
1. require_auth(caller)
2. Load state
3. Validate preconditions
4. Modify state
5. Write state
```

**Verification**: Code review of all entry points.

---

### I6: Fee Monotonicity

**Statement**: For any given amount and fee_bps, the calculated fee is consistent.

**Proof**:
- Fee calculation is pure function of (amount, fee_bps)
- No randomness or state dependency
- Deterministic arithmetic (checked_mul, checked_div)

**Implementation**:
```
fee = amount.checked_mul(fee_bps)
        ?.checked_div(10000)
        .ok_or(ArithmeticError)?
```

**Verification**: Property-based fuzz testing with bounded inputs.

---

### I7: Amount Conservation in Dispute Resolution

**Statement**: In dispute resolution, `payout + fees = original_amount`.

**Proof**:
- Arbitration fee deducted first
- Protocol fee calculated on adjusted amount
- All components sum to original

**Implementation**:
```
arbitration_fee = amount * arbitration_fee_bps / 10000
adjusted_amount = amount - arbitration_fee
protocol_fee = adjusted_amount * protocol_fee_bps / 10000
payout = adjusted_amount - protocol_fee
verify: payout + arbitration_fee + protocol_fee == amount
multi_payee_dust <= payees.len() - 1
```

**Verification**: Test all fee combinations; verify arithmetic.

---

### I8: Buyer Assigned at Fund Time

**Statement**: Once `escrow.state = Funded`, `escrow.buyer` is `Some(address)`.

**Proof**:
- Buyer assigned in `fund_escrow` before state transition
- No path to Funded without buyer assignment
- Buyer immutable after Funded

**Implementation**:
```rust
escrow.buyer = Some(buyer);
escrow.state = EscrowState::Funded;
```

**Verification**: Test that get_escrow always has buyer for Funded+ escrows.

---

### I9: Storage TTL Consistency

**Statement**: No escrow data is lost mid-transaction due to TTL expiration.

**Proof**:
- TTL extended before every state-modifying write
- Extension timestamp >> transaction duration
- Persistent storage TTL is measured in weeks/months

**Implementation**:
```
extend_ttl(&env);
env.storage().persistent().set(key, value);
```

**Verification**: Simulate TTL boundary; verify data persists.

---

### I10: Dispute Window Non-Overlap

**Statement**: For any funded escrow, dispute window and shipping window run independently.

**Proof**:
- Dispute window: funded_at to funded_at + 172800 (2 days)
- Shipping window: funded_at to funded_at + shipping_window (param-driven)
- Both timed from funding; can complete in any order
- Dispute closes auto-release; buyer can resolve early

**Invariant**:
```
dispute_deadline = funded_at + 172800
shipping_release_time = funded_at + shipping_window
dispute_window_closes BEFORE auto_release becomes available (typically)
```

**Verification**: Test scenarios with various shipping window values.

---

### I11: Resolver Immutability Post-Creation

**Statement**: Once created, `escrow.resolver` cannot change.

**Proof**:
- Resolver assigned at creation; no update functions exist
- Resolver used for validation in fund, dispute resolution
- Changing resolver would require state write; no such function

**Implementation**: Resolver is read-only after creation.

**Verification**: Attempt resolver update; verify it fails.

---

### I12: Counter Monotonicity

**Statement**: `EscrowCounter` value never decreases and never repeats.

**Proof**:
- Counter initialized at 1
- Only incremented via `read_and_increment`
- No decrement operations exist

**Implementation**:
```
let id = counter;
counter = counter + 1;
store(counter);
return id;
```

**Verification**: Check counter progression in tests.

---

### I13: Pause State Isolation

**Statement**: Paused state does not affect read-only queries or past state.

**Proof**:
- Pause check only in functions with side effects
- get_escrow, get_fee_config, etc. skip pause check
- Pause doesn't modify data; only gates operations

**Implementation**:
```
fn get_escrow(...) -> EscrowData {
    // no pause check; read-only
    load_escrow(&env, escrow_id)
}

fn confirm_delivery(...) -> Result<(), Error> {
    ensure_not_paused(&env)?;
    ...
}
```

**Verification**: Pause contract; verify reads work, writes fail.

---

### I14: Token Transfer Atomicity

**Statement**: Token transfers are atomic; escrow state updated atomically with transfer.

**Proof**:
- Soroban transactions are all-or-nothing
- State updated within same transaction as transfer
- No partial states visible to external observers

**Implementation**: Single transaction encompasses:
1. Auth check
2. State read
3. Token transfer
4. State write
5. Event emit

**Verification**: Test transaction boundaries; verify atomicity.

---

### I15: Evidence Hash Immutability

**Statement**: Once a dispute is recorded with an evidence hash, the hash cannot change.

**Proof**:
- Dispute data stored once at raise_dispute
- No update function for dispute evidence
- Evidence hash is read-only thereafter

**Implementation**: Evidence stored in `DisputeData`, read-only structure.

**Verification**: Attempt to update evidence; verify immutable.

---

### I16: State History Bounded Storage (Issue #812)

**Statement**: An escrow's state history is capped at MAX_STATE_HISTORY_ENTRIES=50; oldest entries are evicted when limit is reached.

**Proof**:
- State history appended in `append_state_history` every time state changes
- Before pushing a new entry, check `history.len() > MAX_STATE_HISTORY_ENTRIES`
- If true, pop_front (evict oldest) until len == MAX_STATE_HISTORY_ENTRIES
- Ensures on-chain history never exceeds 50 entries per escrow

**Implementation**:
```rust
while history.len() > MAX_STATE_HISTORY_ENTRIES {
    history.pop_front();
}
history.push_back((state, timestamp));
```

**Verification**: Create an escrow and cycle through 100 state transitions; verify get_state_history returns at most 50 entries, all the most recent ones.

---

## Derived Invariants

### D1: Terminal States Prevent Further Operations

**From**: I3 (State Machine Validity)
**Consequence**: Completed, Refunded, Cancelled escrows cannot be modified.

---

### D2: Fees Cannot Exceed Principal

**From**: I7 (Amount Conservation in Disputes)
**Consequence**: With fee caps (max 10% each), payout remains conserved; any split dust is deterministically included in the first payee payout.

---

### D3: No Unauthorized Role Changes

**From**: I4 (Role Separation) + I5 (Authorization Ordering)
**Consequence**: Only authorized parties can initiate state changes per role.

---

### D4: Escrow Completeness

**From**: I8 (Buyer Assigned at Fund) + I11 (Resolver Immutability)
**Consequence**: All funded escrows have complete role tuple: (seller, buyer, resolver).

---

## Invariant Violations

If any invariant is violated:

1. **Balance Conservation (I1)**: Audit tokens; identify missing escrows
2. **ID Uniqueness (I2)**: Scan counter; find duplicate or skipped IDs
3. **State Machine (I3)**: Review transaction logs; find invalid transition
4. **Role Separation (I4)**: Check fund_escrow authorization; verify ConflictingRoles error
5. **Authorization (I5)**: Code review; identify auth check after state read
6. **Fee Monotonicity (I6)**: Test fee calculation; find inconsistency
7. **Amount Conservation (I7)**: Verify fee arithmetic; check payout + fees = amount
8. **Buyer Assignment (I8)**: Query non-Funded escrows with buyer set
9. **Storage TTL (I9)**: Simulate TTL expiration; verify data loss
10. **Dispute Windows (I10)**: Test competing timeouts; verify interaction
11. **Resolver Immutability (I11)**: Attempt resolver update
12. **Counter Monotonicity (I12)**: Scan counter; find decrement or repeat
13. **Pause Isolation (I13)**: Pause and verify read/write behavior
14. **Transfer Atomicity (I14)**: Review transaction logs; find partial updates
15. **Evidence Immutability (I15)**: Attempt evidence update

## Testing Strategy

### Unit Tests
- Verify each invariant locally
- Test boundary conditions (zero amounts, max fees, etc.)
- Test error paths that preserve invariants

### Property-Based Tests
- Generate random escrow scenarios
- Verify invariants hold after each operation
- Use QuickCheck-like framework for exhaustive testing

### Integration Tests
- End-to-end escrow flows
- Multiple concurrent escrows
- Verify invariants across lifecycle

### Fuzz Tests
- Random fee values, amounts, timestamps
- Random operation ordering
- Verify invariants never violated

---

## Verification Checklist

- [ ] Balance conservation verified at contract deployment
- [ ] ID uniqueness checked across all test runs
- [ ] State machine transitions tested exhaustively
- [ ] Role separation verified for all scenarios
- [ ] Authorization ordering reviewed in code
- [ ] Fee calculation fuzzed with boundary inputs
- [ ] Amount conservation verified in dispute resolution
- [ ] Buyer assignment tested for all paths to Funded
- [ ] TTL management tested with boundary timestamps
- [ ] Dispute windows tested with various shipping windows
- [ ] Resolver immutability verified
- [ ] Counter monotonicity checked
- [ ] Pause functionality tested
- [ ] Token transfer atomicity verified
- [ ] Evidence hash immutability tested