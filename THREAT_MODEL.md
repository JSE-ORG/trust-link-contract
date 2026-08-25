# TrustLink Contract — Threat Model

## Executive Summary

TrustLink is a three-party escrow contract with seller, buyer, and resolver roles. The primary threat surface involves:
1. Unauthorized state transitions
2. Role confusion attacks
3. Arithmetic errors in fee calculations
4. Token-level exploits (blacklists, callbacks)
5. Storage/TTL integrity issues
6. Permissionless operation guard failures

## Threat Analysis

### THREAT 1: Unauthorized Fund Release

**Scenario**: Attacker transfers funds to resolver/seller without buyer authorization.

**Attack Vector**:
- Modify `confirm_delivery` to skip buyer signature check
- Call `resolve_dispute` with incorrect resolver
- Trigger `auto_release` before shipping window elapses

**Mitigation**:
- ✅ All state-modifying functions require `require_auth()` for specific roles
- ✅ Buyer auth checked before state reads in `confirm_delivery`
- ✅ Resolver validated against stored `escrow.resolver` before payout
- ✅ `auto_release` validates shipping window elapsed and state is Funded/Shipped
- ✅ Role separation enforced: buyer ≠ seller ≠ resolver

**Residual Risk**: Low. Auth checks are preconditions.

---

### THREAT 2: Role Confusion / Multi-Role Attack

**Scenario**: Same address acts as buyer AND seller to manipulate fund flow.

**Attack Vector**:
- Create escrow with attacker as seller
- Fund escrow as same attacker (buyer role)
- Skip dispute, directly confirm delivery
- Collect funds without real counterparty

**Mitigation**:
- ✅ Role validation: `buyer ≠ seller` enforced in `fund_escrow`
- ✅ `buyer ≠ resolver` enforced
- ✅ `seller ≠ resolver` enforced (via validate_resolvers)
- ✅ Payee list separate from resolver role

**Residual Risk**: Low. Role checks at fund time.

---

### THREAT 3: Fee Arithmetic Overflow

**Scenario**: Fee calculation overflows, causing underflow payout or fee loss.

**Attack Vector**:
- Create escrow with `amount = i128::MAX`
- Set `fee_bps = 10000` (100%)
- Trigger fee calculation
- `amount * fee_bps / 10000` overflows

**Mitigation**:
- ✅ Fee calculation uses `checked_mul` / `checked_div` in helpers
- ✅ Overflow returns `ArithmeticError`
- ✅ Fee cap: max 1000 BPS (10%) enforced on initialization
- ✅ Arbitration fee capped per-escrow at creation
- ✅ Amounts validated > 0 at creation

**Residual Risk**: Very Low. Checked arithmetic throughout.

---

### THREAT 4: Dispute Window Bypass

**Scenario**: Attacker raises dispute after window closes, blocking auto-release.

**Attack Vector**:
- Fund escrow
- Wait past shipping window
- Raise dispute to change state from Funded
- Block auto-release permanently

**Mitigation**:
- ✅ Dispute window: 172800s (2 days) from funding
- ✅ Shipped state from `mark_shipped` call
- ✅ `auto_release` checks `funded_at + shipping_window <= now`
- ✅ Dispute raises new dispute window, separate from shipping window
- ✅ Can auto-release even with expired dispute window if no active dispute

**Residual Risk**: Low. Separate windows; shipping window is hardcoded.

---

### THREAT 5: Token Callback Reentrancy

**Scenario**: Malicious token contract calls back into escrow during transfer.

**Attack Vector**:
- Create escrow with attacker-controlled token
- Trigger `fund_escrow`
- Token's `transfer` hook calls escrow methods in callback
- Re-enter `confirm_delivery` or other state-modifying function

**Mitigation**:
- ✅ State updated BEFORE token transfer (`escrow.state = Funded`)
- ✅ Fund transfer is last step; no further state changes after
- ✅ Escrow data loaded once; no re-loads during callback
- ✅ Soroban environment prevents cross-contract re-entrancy in single tx

**Residual Risk**: Low. Stellar/Soroban provides strong reentrancy protection.

---

### THREAT 6: Dispute Fee Deduction Order

**Scenario**: Attacker manipulates fee deduction order to cause loss.

**Attack Vector**:
- Create escrow with amount = 1000, arbitration_fee = 500, protocol_fee = 1000
- Raise dispute
- If arbitration fee deducted first: payout = 1000 - 500 - (500 * 1000 / 10000) = 449
- If protocol fee deducted first: different result

**Mitigation**:
- ✅ Fee order is deterministic and documented:
  1. Arbitration fee deducted from escrow amount
  2. Protocol fee calculated on reduced amount
  3. Net payout transferred
- ✅ All fees accumulated in contract, withdrawn by admin
- ✅ Event emissions show all fee components

**Residual Risk**: Very Low. Fee order is fixed and auditable.

---

### THREAT 7: Storage TTL Expiration

**Scenario**: Escrow data deleted from storage due to TTL expiration during transaction.

**Attack Vector**:
- Create escrow with minimum TTL
- Perform operations that don't extend TTL
- Storage expires mid-lifecycle
- Subsequent operations fail or read stale data

**Mitigation**:
- ✅ TTL extended on every state change (fund, dispute, resolve, etc.)
- ✅ Persistent storage has explicit TTL windows (typically months)
- ✅ `extend_ttl` called before storage writes
- ✅ Missing escrow returns `EscrowNotFound` error (fail-safe)

**Residual Risk**: Very Low. TTL management is automatic and conservative.

---

### THREAT 8: Timestamp Manipulation

**Scenario**: Validator manipulates ledger timestamp to bypass time-based guards.

**Attack Vector**:
- Auto-release requires `env.ledger().timestamp() > funded_at + shipping_window`
- Attacker controls validator; sets timestamp backwards
- Triggers auto-release prematurely

**Mitigation**:
- ✅ Ledger timestamps are canonical and cannot be manipulated by contract
- ✅ Stellar consensus ensures monotonic timestamp progression
- ✅ Validator collusion would require 66%+ of network
- ✅ This is a network-level concern, not contract-level

**Residual Risk**: Network-level. Out of contract scope.

---

### THREAT 9: Pause Function Abuse

**Scenario**: Admin pauses contract, freezing all escrows permanently.

**Attack Vector**:
- Admin (compromised key) calls `pause_contract`
- All fund transfers, disputes, and resolution blocked
- Escrows stuck in Funded state indefinitely

**Mitigation**:
- ✅ Pause affects only state-modifying operations
- ✅ Read-only queries (get_escrow) remain available
- ✅ Pause is toggleable; unpause by same admin
- ✅ Separate from data integrity; no data lost
- ✅ Admin role assumed trusted; key management is user responsibility

**Residual Risk**: Medium (by design). Admin is trusted role. Key management critical.

---

### THREAT 10: Invalid Evidence Hash

**Scenario**: Attacker provides a malformed evidence hash, leaving the dispute record inconsistent or the commitment meaningless.

**Attack Vector**:
- Call `raise_dispute` with a 31-byte evidence_hash
- Validation fails part-way, after state has already changed
- Contract left in an inconsistent state

**Mitigation**:
- ✅ `evidence_hash` is typed `BytesN<32>`, so the host rejects any other length while decoding the arguments — `raise_dispute` never begins executing, and no state can be written
- ✅ Length is therefore enforced by the ABI rather than by a runtime check; `ContractError::InvalidEvidenceHash` exists but is not returned today

**Residual Risk**: Low. The length is guaranteed, but the *content* is not: the
contract cannot tell a real SHA-256 digest from arbitrary bytes, and the
all-zero digest is accepted as the conventional "no evidence attached" marker.
A dispute's evidence hash is a commitment the buyer makes, not a claim the
contract verifies — off-chain consumers must re-hash the evidence and compare
(`verifyEvidence()` in `@trustlink/contract-bindings`) before treating it as
proof.

---

### THREAT 11: Arbitration Fee > Escrow Amount

**Scenario**: Admin sets arbitration_fee such that fee >= amount, draining escrow.

**Attack Vector**:
- Create escrow with amount = 100, arbitration_fee_bps = 10000 (100%)
- Raise dispute
- Arbitration fee = 100, payout = 0
- Funds stuck in contract

**Mitigation**:
- ✅ Fee cap enforced: max 1000 BPS (10%) on initialization
- ✅ Arbitration fee capped at 1000 BPS per-escrow
- ✅ Even with max fees: payout = 100 - 10 - ~0.1 = ~89.9 (positive)
- ✅ Fee configuration immutable per-escrow once created

**Residual Risk**: Very Low. Fee caps are hard-coded.

---

### THREAT 12: Multiple Concurrent Disputes

**Scenario**: Attacker raises multiple disputes on same escrow.

**Attack Vector**:
- Fund escrow
- Raise dispute (state = Disputed)
- Raise another dispute
- Process disputes in order, causing double-payout

**Mitigation**:
- ✅ State guard: only Funded state allows `raise_dispute`
- ✅ After first dispute, state is Disputed; second dispute fails
- ✅ Dispute data stored once; no concurrent disputes

**Residual Risk**: Very Low. State machine prevents it.

---

### THREAT 13: Delivery Before Dispute Window Closes

**Scenario**: Seller marks delivery to finalize escrow, bypassing dispute period.

**Attack Vector**:
- Fund escrow (timestamp = T0)
- Seller calls `mark_shipped` with tracking ID
- Buyer calls `confirm_delivery` (timestamp = T0 + 1s, within dispute window)
- Escrow completes before dispute window closes (T0 + 172800s)

**Mitigation**:
- ✅ Confirm delivery possible at any time after funding (permissionless)
- ✅ Dispute window runs concurrently; buyer can still dispute before deadline
- ✅ This is by design: buyer chooses to confirm early, accepting seller
- ✅ Events clearly separate shipping/delivery from dispute window

**Residual Risk**: Low (by design). Buyer must trust seller if confirming early.

---

## Threat Matrix

| Threat | Impact | Likelihood | Mitigation | Risk |
|--------|--------|-----------|-----------|------|
| Unauthorized release | Critical | Very Low | Auth checks | Very Low |
| Role confusion | High | Very Low | Role validation | Very Low |
| Fee overflow | Critical | Very Low | Checked math | Very Low |
| Dispute window bypass | High | Low | State guards | Very Low |
| Reentrancy | High | Very Low | Soroban protection | Very Low |
| Fee deduction order | Medium | Very Low | Deterministic order | Very Low |
| TTL expiration | High | Very Low | TTL management | Very Low |
| Timestamp manipulation | High | Very Low | Network-level | N/A |
| Pause abuse | High | Low | Admin trust | Medium |
| Invalid evidence | Medium | Very Low | Input validation | Very Low |
| Fee > amount | Medium | Very Low | Fee caps | Very Low |
| Multiple disputes | High | Very Low | State machine | Very Low |
| Early confirmation | Medium | Low | By design | Low |

## Assumptions & Trust Model

### Trusted Roles
1. **Admin**: Can pause, set fees, record delivery. Key must be secured.
2. **Fee Collector**: Receives protocol fees. No special permissions.
3. **Resolver**: Mediates disputes; chosen by seller at creation. Trusted by both parties.

### Untrusted
- Buyer: Can fund, confirm, dispute; confined by auth
- Seller: Can create, claim; confined by role checks
- Token contract: Assumed SEP-41 compliant

### Assumptions
1. Stellar validator set is honest (66%+ of network required for consensus)
2. Admin key is secure (compromised key is out of scope)
3. Tokens comply with SEP-41 (no malicious callbacks assumed)
4. Escrow amounts are in smallest token unit (stroops for native, lowest decimal for other tokens)

## Recommendations

### High Priority
1. ✅ Code review of fee calculation helpers (CRITICAL)
2. ✅ Verify all `require_auth()` placements (CRITICAL)
3. ✅ Audit state machine transitions (HIGH)
4. ✅ Review role validation logic (HIGH)

### Medium Priority
5. Fuzz fee calculations with boundary values
6. Test TTL expiration scenarios
7. Verify token transfer failure handling
8. Document admin key rotation procedure

### Low Priority
9. Add internal audit logging for disputed escrows
10. Consider timelock for admin functions
11. Evaluate multi-sig for admin role

## Out-of-Scope

- Validator consensus and network-level attacks
- Admin key management and operational security
- Token contract vulnerabilities (SEP-41 compliance)
- Deployment and initialization procedures
- Off-chain indexing and Oracle accuracy
