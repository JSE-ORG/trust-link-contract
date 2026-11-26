# TrustLink Contract ? Invariants

## Core Invariants

These invariants must hold at the end of every transaction.

### I1: Balance Conservation

**Statement**: Total tokens held by contract = sum of all escrow amounts.

**Proof**:
- Contract only receives tokens via `fund_escrow` (adds to balance)
- Contract only sends via terminal states (removes from balance)
- Every escrow has exactly one terminal state (Completed, Refunded, Canceled)
- Fees are deducted atomically with payout
- Multi-payee rounding dust is assigned to the first payee, bounded by `N - 1` stroops for `N` payees (policy: payees `1..N` are floored, `payees[0]` receives the remainder, so the first payee is systematically favoured by at most `N - 1` stroops; total payout is still exactly conserved)
- No tokens created or destroyed

**Implementation**:
```
contract_balance = sum(escrow.amount for all non-terminal escrows)
                 + accumulated_fees
```

**Verification**: Checksum tokens via query at end of test runs. The full
per-path argument, including accrued protocol fees and basket tokens, is in
[Payout Conservation Proofs](#payout-conservation-proofs) below.

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
Pending ? Funded | Canceled
Funded ? Completed | Disputed | RefundRequested | Shipped | Canceled
Disputed ? Completed | Refunded | PendingFinalization
RefundRequested ? Funded | Refunded
Shipped ? Completed
PendingFinalization ? Completed | Refunded
Completed, Refunded, Canceled ? (terminal, no transitions)
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

**Statement**: In dispute resolution,
`payout + protocol_fee + platform_fee + arbitration_fee + resolver_fee == original_amount`.

**Proof**:
- At resolution the provisional arbitration and resolver fees are deducted from `escrow.amount`, giving `A' = A - arbitration_fee - resolver_fee`
- At finalization the fee basis is reconstructed as `B = A' + arbitration_fee + resolver_fee = A`
- Protocol and platform fees are each computed on the same gross basis `B` (no compounding on the reduced remainder)
- All components sum to the original principal; any split dust is credited to the first payee / buyer as documented in the conservation section

**Implementation**:
```
arbitration_fee = A * arbitration_fee_bps / 10000
resolver_fee = A * resolver_fee_bps / 10000
A' = A - arbitration_fee - resolver_fee
B = A' + arbitration_fee + resolver_fee
protocol_fee = B * fee_bps / 10000
platform_fee = B * platform_fee_bps / 10000
payout = A' - protocol_fee - platform_fee
verify: payout + protocol_fee + platform_fee + arbitration_fee + resolver_fee == A
```

See [Payout Conservation Proofs](#payout-conservation-proofs) for the complete
proof, the appeal-cycle argument, and the basket-token per-token identities.

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
**Consequence**: Completed, Refunded, Canceled escrows cannot be modified.

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

## Payout Conservation Proofs

This section gives the formal conservation argument for every settlement path.
It is written against `contracts/escrow/src/helpers/payout.rs`,
`contracts/escrow/src/internal.rs` (`settle_escrow_to_payees`,
`distribute_to_payees`, `payout_basket_tokens`, `payout_basket_tokens_split`),
`contracts/escrow/src/disputes.rs` (`finalize_dispute`,
`claim_dispute_timeout`, `cancel_pending_finalization`) and the release/refund
entry points in `contracts/escrow/src/instructions.rs`.

### Notation

| Symbol | Meaning |
|---|---|
| `A` | Principal funded into the escrow (`escrow.amount` at funding) |
| `A'` | Principal after provisional arbitration/resolver fees are deducted at resolution: `A' = A - a - r` |
| `B` | Fee basis used by `finalize_dispute` and `calculate_dispute_allocations`: `B = A' + a + r = A` |
| `f` | Protocol fee rate in basis points (`escrow.fee_bps` snapshot or live global rate) |
| `q` | Platform fee rate in basis points (`platform_fee_bps`) |
| `a` | Arbitration fee: `floor(A * arbitration_fee_bps / 10_000)` |
| `r` | Resolver fee: `floor(A * resolver_fee_bps / 10_000)` |
| `P` | Protocol fee actually charged on a path |
| `F` | Platform fee actually charged on a path |
| `bps` / `BASIS_POINTS` | Basis points, `10_000` |
| `M_j` | Amount of the `j`-th secondary basket token entry |
| `N` | Number of payees |
| `k` | Number of compensated resolvers |
| `C_t` | Contract custody of token `t` |

All arithmetic is checked; every division is floor division on non-negative
values, which is identical to truncation toward zero. `payout()` skips
non-positive amounts, so a fee that rounds to zero is simply not transferred
(`transfer_with_protocol_fee` / `transfer_basket_with_fees` return that zero
fee in their tuple and the identity below still holds).

### Global conservation law

**Statement (C-1).** For every token `t`, at the end of every transaction:

```text
C_t = Σ(escrow.amount for non-terminal escrows whose primary token is t)
    + Σ(M_j for non-primary basket entries of those escrows in t)
    + AccruedFees_t
    + UnallocatedResidual_t
```

where `AccruedFees_t` is `DataKey::AccumulatedFees(t)` and
`UnallocatedResidual_t` is non-zero only for the admin-cancel edge case
documented below.

**Proof.** Funding transfers exactly `A` (plus each `M_j`) into the contract and
records the matching amount. Every terminal path below is proved to transfer
exactly the recorded amount minus the fees it books, and each booked protocol
fee is credited to `AccruedFees_t` in the same invocation. `claim_accumulated_fees`
(admin) debits `AccruedFees_t` and transfers exactly that amount to the fee
collector, so the sum is invariant under claims. Refunds and releases are
mutually exclusive because every path transitions the escrow to a terminal
state before any external transfer (CEI ordering), so no principal can be
settled twice. ∎

### Settlement path matrix

| Entry point | Fee basis | Protocol `P` | Platform `F` | Other fees | Recipient rule | Conservation identity |
|---|---|---|---|---|---|---|
| `confirm_delivery` | `A` | `floor(A*f/10_000)`, `f` = snapshotted `escrow.fee_bps` | `0` on the primary; secondary basket tokens pay `floor(M_j*q/10_000)` | none | `net = A - P` split across all payees | `Σ p_i + P = A`; for each secondary entry `net_j + P_j + F_j = M_j` |
| `auto_release` | `A` | `floor(A*f/10_000)`, `f` = live global `protocol_fee_bps` | same as `confirm_delivery` | none | same as `confirm_delivery` | `Σ p_i + P = A`; secondary entries as above |
| `co_signed_release` | `A` | `floor(A*f/10_000)`, live global rate | `0` on the primary; secondary basket tokens pay protocol + platform | none | entire `net` to the primary payee | `net + P = A`; secondary entries as above |
| `finalize_dispute` (Release / Refund) | `B = A` | `floor(B*f/10_000)` | `floor(B*q/10_000)` when a treasury exists | `a` to fee collector, `r` split among `k` winning resolvers | `payout = A' - P - F` to the payee (Release) or buyer (Refund) | `payout + P + F + a + r = A` |
| `finalize_dispute` (`Split(s)`) | `B = A` | same | same | same | `payee_net = floor(payout*s/10_000)`, `buyer_net = payout - payee_net` | `payee_net + buyer_net + P + F + a + r = A` |
| `claim_dispute_timeout` | `A` | `floor(A*f/10_000)` | `floor(A*q/10_000)` only when a treasury exists | none (`a = r = 0`) | `payout = A - P - F` to buyer; secondary basket tokens gross | `payout + P + F = A` |
| `cancel_escrow` (Funded), `mutual_cancel`, `emergency_drain`, `reclaim_expired` | `A` | `0` | `0` | none | full `A` to buyer; all secondary basket entries gross | `refund + Σ M_j = A + Σ M_j` |
| `cancel_pending_finalization` | `A'` | `0` | `0` | none | `A'` to buyer; secondary basket entries gross | `refund + (a + r) + Σ M_j = A + Σ M_j`; see known exception |

### Lemma 1 — payee distribution sums exactly to its input

`distribute_to_payees(amount)` pays payee `i >= 1` the floored share
`p_i = floor(amount * b_i / 10_000)` and defines the primary payee as
`p_0 = amount - Σ_{i=1}^{N-1} p_i`.

By construction `Σ_{i=0}^{N-1} p_i = amount`. Because
`Σ_{i=1}^{N-1} p_i <= amount * (10_000 - b_0) / 10_000 <= amount`, `p_0 >= 0`,
so `remaining` never underflows. Writing `amount * b_i = 10_000 * p_i + d_i`
with `0 <= d_i < 10_000` gives
`p_0 - amount*b_0/10_000 = Σ_{i=1}^{N-1} d_i/10_000 < N - 1`, so the primary
payee is favoured by fewer than `N - 1` stroops and no stroop is stranded.
The creation-time invariant `Σ b_i = 10_000` is enforced by `validate_payees`.

### Lemma 2 — split net sums exactly to its input

`split_net(net, s)` computes `payee_net = floor(net*s/10_000)` using the
split-div form `(net/10_000)*s + (net%10_000)*s/10_000` (overflow-free and
exactly equal to the floor of the product) and `buyer_net = net - payee_net`.
Therefore `payee_net + buyer_net = net` and both are non-negative.

By the same argument as Lemma 1, `payee_net <= net` and the rounding remainder
`< 1` stroop is credited to the buyer.

### Lemma 3 — resolver fee distribution sums exactly to `r`

The resolver fee is split across the `k >= 1` compensated resolvers:
`share = floor(r/k)` and `dust = r - share*k` with `0 <= dust < k`. Each winner
receives `share`, and `resolved_by` receives `dust` when positive. Hence
`k*share + dust = r`. When the compensable set is empty the list falls back to
`resolved_by`, so `k >= 1` always.

### Theorem 1 — `confirm_delivery` / `auto_release` conservation

Let `P = floor(A*f/10_000)` and `net = A - P`. Then:

1. `calculate_protocol_fee` returns `(P, net)` with `P + net = A` (floor
   rounding keeps the remainder in `net`).
2. `settle_escrow_to_payees` transitions to `Completed` and persists before any
   transfer, then credits `AccruedFees_t += P` when `P > 0`, and calls
   `distribute_to_payees(net)`.
3. By Lemma 1, the payee transfers sum to exactly `net`.
4. `payout_basket_tokens(..., Some(f))` iterates the stored entries and skips
   every entry whose token equals `escrow.token` or whose amount is `<= 0`
   (`is_transferable_basket_entry`). For each remaining `j`,
   `transfer_basket_with_fees` returns
   `net_j = M_j - P_j - F_j` with `P_j = floor(M_j*f/10_000)` and
   `F_j = floor(M_j*q/10_000)` (or `F_j = 0` without a treasury); the primary
   token is therefore never paid twice and each secondary token satisfies
   `net_j + P_j + F_j = M_j`.
5. Adding the primary identity and each per-token identity yields
   `Σ p_i + P + Σ_j (net_j + P_j + F_j) = A + Σ_j M_j`.

`auto_release` is identical except that `f` is read live; the identity is
independent of the rate. ∎

### Theorem 2 — `finalize_dispute` conservation

At resolution time `execute_resolution_transition` records
`a = floor(A*arbitration_fee_bps/10_000)` and
`r = floor(A*resolver_fee_bps/10_000)`, rejects `a + r > A` with
`FeeExceedsMax`, stores both in `DisputeData`, and sets
`A' = A - a - r`. No token moves at this step (fees are provisional and an
appeal may clear them).

At finalization:

1. `B = A' + a + r = A` exactly, so the fee basis is the original funded
   principal, not the reduced remainder.
2. `P = floor(B*f/10_000)` and `F = floor(B*q/10_000)` when a treasury exists, and `F = 0` otherwise.
   Both fees are computed independently on the same gross basis; there is no
   compounding on a shrinking base.
3. `T = P + F`; if `T > A'` the call reverts with `InsufficientBalance`, so
   `payout = A' - T >= 0`.
4. The state transition and fee accounting are persisted before any transfer
   (CEI). The transfers are:
   - `a` to the fee collector and `r` split per Lemma 3,
   - `F` to the treasury when configured (`payout_platform_fee` skips zero and
     emits `PlatformFeeCollected`),
   - `P` credited to `AccruedFees_t`,
   - `payout` to the recipient, or `payee_net + buyer_net = payout` under
     `Split` per Lemma 2.
5. Therefore `payout + P + F + a + r = (A' - P - F) + P + F + a + r = A' + a + r = A`.

Secondary basket tokens are settled per entry:
`net_j + P_j + F_j = M_j` on Release/Refund, and under `Split`
`payee_j + buyer_j = net_j` by Lemma 2. The primary token is excluded by
`is_transferable_basket_entry`, so the identities compose without
double-counting. ∎

### Theorem 3 — appeal cycles preserve conservation

`appeal_dispute` is allowed only in `PendingFinalization` and restores the
principal exactly:
`A = A' + arbitration_fee + resolver_fee`, then clears the provisional
resolution, `resolved_by`, `resolved_at`, votes and the recorded fee amounts.
The arbitration and resolver fees were never transferred, so custody is
unchanged; the next resolution round records fresh fees against the restored
`A`. The appeal fee, when configured, is paid by the appellant directly to the
fee collector and is not deducted from escrow custody, so it cannot reduce the
principal. By induction over appeal rounds, every round satisfies Theorem 2
with the same `A`, and finalization at round `n` settles exactly once. ∎

### Theorem 4 — refund paths

`cancel_escrow` on a `Funded` escrow, `mutual_cancel`, `emergency_drain` and
`reclaim_expired` transfer exactly `escrow.amount` to the buyer and call
`payout_basket_tokens(..., None)`, which pays every secondary entry gross.
No fee is booked, so `refund + Σ M_j = A + Σ M_j` and `AccruedFees_t` is
untouched. `emergency_drain` additionally requires the global pause and rejects
`PendingFinalization`, so it cannot race a resolver ruling.

`claim_dispute_timeout` settles without arbitration or resolver fees (`a = r = 0`):
`P = floor(A*f/10_000)`, `F = floor(A*q/10_000)` when a treasury exists, and
`payout = A - P - F`; secondary basket entries are refunded gross. Thus
`payout + P + F = A`. ∎

### Basket-escrow invariants

| ID | Statement | Enforcement |
|---|---|---|
| B1 | `BasketTokens(escrow_id)` contains the primary token exactly once and every secondary token at most once | `build_basket_entries` + duplicate scan in `create_basket_escrow*`; `fund_basket_escrow` repeats the same scan |
| B2 | Primary custody is tracked only by `escrow.amount`; basket entries are additional | `is_transferable_basket_entry` skips `entry.token == escrow.token` on both the funding and payout legs |
| B3 | A release charges protocol (and platform) fees on each secondary entry; the primary follows the primary-token rule | `payout_basket_tokens(Some(f))`, `transfer_basket_with_fees`, `payout_basket_tokens_split` |
| B4 | A refund returns every secondary entry gross and the primary `escrow.amount` in full | `payout_basket_tokens(None)` on `cancel_escrow`, `mutual_cancel`, `emergency_drain`, `reclaim_expired`, `claim_dispute_timeout`, `cancel_pending_finalization` |
| B5 | Zero-amount entries never produce a zero-value transfer | `is_transferable_basket_entry` requires `entry.amount > 0`; `payout` also skips non-positive amounts |
| B6 | Basket size is bounded and the stored XDR shape is stable | `BoundedBasket::try_new` (capacity check) and `MAX_BASKET_SIZE` |

### Dust and rounding table

| Location | Rounding | Magnitude | Beneficiary | Conservation |
|---|---|---|---|---|
| Protocol fee | floor(`amount*f/10_000`) | remainder `< 1` stroop | stays in `net` (recipient) | `P + net = amount` |
| Platform fee | floor(`amount*q/10_000`) | remainder `< 1` stroop | stays in `net` (recipient) | `F + net = amount` component |
| Multi-payee split | payees `1..N` floored, payee 0 takes the remainder | total `< N - 1` stroops | primary payee | `Σ p_i = net` exactly |
| Dispute `Split(bps)` | payee floored, buyer takes the remainder | `< 1` stroop | buyer | `payee_net + buyer_net = payout` |
| Resolver fee split | `k - 1` winners floored, dust to `resolved_by` | `< k` stroops | `resolved_by` | `Σ shares + dust = r` |
| Basket secondary entry | protocol and platform each floored independently | `< 2` stroops per entry | recipient | `net_j + P_j + F_j = M_j` |

### Accrued protocol fees

Release and finalization paths credit `P` to `DataKey::AccumulatedFees(token)`
instead of transferring it immediately. The tokens remain in contract custody,
so C-1 still holds with `AccruedFees_t` as the counterparty. The admin-only
`claim_accumulated_fees` reads the ledger, transfers exactly that amount to the
configured fee collector and resets it to zero; this is a pure custody transfer
and cannot create or destroy value. `claim_accumulated_fees` reverts with
`InsufficientBalance` when the ledger is zero, so repeated calls are safe.

### Known accounting exception

`cancel_pending_finalization` (and its `admin_cancel_finalization` alias)
refunds `escrow.amount`, which at that point is `A' = A - a - r` because the
resolution transition provisionally deducted the arbitration and resolver fees.
Those fees were never transferred and are not added to `AccruedFees_t` by this
path, so `a + r` remains in the contract as an unallocated residual
(`UnallocatedResidual_t` in C-1). This is not a loss of funds and cannot be
withdrawn by an attacker, but it is not attributed to any beneficiary. Documented
here so an auditor can reconcile the ledger exactly; the normal finalization and
appeal paths do not have this residual.

### Worked examples

| Scenario | Principal | Fees | Transfers | Check |
|---|---|---|---|---|
| `confirm_delivery`, `f = 200`, `N = 1` | `1_000` | `P = 20` | seller `980`, accrued `20` | `980 + 20 = 1_000` |
| `confirm_delivery`, `f = 200`, payees `6_000/4_000` | `1_001` | `P = 20` | `net = 981`; payee 0 `589` (floor 588 + 1 dust), payee 1 `392` | `589 + 392 + 20 = 1_001` |
| `finalize_dispute` Release, `arb = 500`, `f = 200`, `q = 100` | `1_000` | `a = 50`, `P = 20`, `F = 10`; `A' = 950` | collector `50`, treasury `10`, accrued `20`, seller `920` | `920 + 20 + 10 + 50 = 1_000` |
| `finalize_dispute` `Split(5_000)`, `f = 100` | `10_001` | `P = 100`, `A' = 10_001` | payout `9_901`; payee `4_950`, buyer `4_951`; accrued `100` | `4_950 + 4_951 + 100 = 10_001` |
| Refund after appeal (`a = r = 0`) | `1_000` | `P = 20` | buyer `980`, accrued `20` | `980 + 20 = 1_000` |
| Basket release, secondary `M = 5_000`, `f = 100`, `q = 200` | entry `5_000` | `P_j = 50`, `F_j = 100` | seller `4_850`, accrued `50`, treasury `100` | `4_850 + 50 + 100 = 5_000` |

### Audit verification matrix

| Invariant | Primary evidence |
|---|---|
| C-1 global custody | `test_fee_property_invariants.rs`, `test_fee_calculation_accuracy.rs`, `test_edge_cases.rs`, `test_overflow.rs` |
| Theorem 1 (release) | `test.rs` (`test_confirm_delivery`, `test_auto_release`), `test_fee_snapshot.rs`, `test_sep41.rs` |
| Theorem 2 (dispute) | `test_arbitration_fee.rs`, `test_dispute.rs`, `test_fee_calculation_accuracy.rs` |
| Theorem 3 (appeal) | `test_appeal.rs`, `test_fallback_resolver.rs`, `test_finalize_dispute_appeal_boundary.rs` |
| Theorem 4 (refunds) | `test_cancel_restrictions.rs`, `test_mutual_cancel.rs`, `test_emergency_drain.rs`, `test_expiration.rs`, `test_dispute_timeout.rs` |
| Basket B1-B6 | `test_basket_escrow.rs`, `test_bounded_basket.rs`, `test_multi_asset.rs` |
| Accrued fee claim | `admin.rs` `accumulated_fee_tests`, `test_fee_snapshot.rs` |

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