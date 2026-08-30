# TrustLink Contract — Security Guidance

## For Auditors

### Pre-Audit Checklist

- [ ] Review AUDIT_SCOPE.md for function list and data flows
- [ ] Review THREAT_MODEL.md for known threat analysis
- [ ] Review INVARIANTS.md for correctness assumptions
- [ ] Review ARCHITECTURE.md for state machine and design
- [ ] Read all entry point functions in lib.rs
- [ ] Trace token transfer logic in helpers/
- [ ] Examine fee calculation in helpers/payout.rs
- [ ] Review storage schema in types.rs
- [ ] Check authorization pattern in all functions

### Critical Functions to Audit

1. **fund_escrow** — Token transfer + role validation + state change
2. **resolve_dispute** — Conditional payout + fee deduction + role check
3. **auto_release** — Permissionless + time-based + state guard
4. **initialize** — Admin setup; must be one-time only
5. Fee calculation helpers — Arithmetic correctness

### Red Flags

- ❌ State read after require_auth (potential confused deputy)
- ❌ Token transfer before state update (potential reentrancy)
- ❌ Unchecked arithmetic in fee calculations
- ❌ State transitions not validated against approved matrix
- ❌ Auth check missing from privileged function
- ❌ Storage key collision (two functions using same DataKey)
- ❌ TTL not extended before persistent write
- ❌ Role validation only at creation, not enforced elsewhere

---

## For Deployers / Operators

### Pre-Deployment

1. **Key Security**
   - Generate admin key via cold storage / HSM
   - Use multi-sig for admin role if possible
   - Never share or hardcode admin key
   - Rotate admin key annually

2. **Fee Configuration**
   - Protocol fee: recommend 100-300 BPS (1-3%) for sustainability
   - Arbitration fee: recommend 0-50 BPS; inform users at creation
   - Document fee rationale; communicate to stakeholders
   - Set fees BEFORE opening to users

3. **Contract Initialization**
   - Call `initialize()` once after deployment
   - Set correct admin address
   - Set fee_collector address (can be multi-sig)
   - Verify initialization via get_fee_config query
   - NEVER call initialize twice

4. **Token Whitelisting** (recommended)
   - Audit all tokens before use
   - Maintain allowlist of supported tokens
   - Reject malicious tokens with false transfer callbacks
   - Document token audit results

### Post-Deployment

5. **Monitoring**
   - Monitor all event logs for anomalies
   - Track fee accumulation weekly
   - Alert on pause events (rare)
   - Verify resolver performance (response time, fair decisions)

6. **Fee Withdrawal**
   - Withdraw accumulated fees monthly to avoid loss
   - Verify fee_collector address before withdrawal
   - Document all withdrawals for auditing

7. **Admin Operations**
   - Change protocol fee only with advance notice
   - Pause contract only in emergency (revert to unpause ASAP)
   - Never update admin key without migration plan
   - Log all admin actions for audit trail

8. **Dispute Mediation**
   - Ensure resolver is neutral third party
   - Document resolver selection criteria
   - Require resolver SLA (response time: <24h)
   - Escalate unresolved disputes to governance

---

## For Smart Contract Developers (Using Bindings)

### Integration Security

1. **Fund Verification**
   - Always verify `EscrowFunded` event before confirmation
   - Check buyer, seller, amount in event
   - Use fund_escrow result, not event, as source of truth

2. **Dispute Handling**
   - Evidence hash must be SHA-256 (32 bytes)
   - Collect evidence off-chain before raising dispute
   - Verify evidence_hash matches off-chain evidence
   - Never modify evidence post-dispute

3. **Token Safety**
   - Verify token address before fund_escrow
   - Use only SEP-41 compliant tokens
   - Test token with small amounts first
   - Handle token transfer failures gracefully

4. **Role Management**
   - Buyer and seller must be different accounts
   - Resolver should be neutral (not buyer or seller)
   - Use hardware wallet for high-value escrows
   - Implement rate limiting on escrow creation

5. **Front-End Security**
   - Validate all inputs client-side
   - Sanitize evidence_hash before submission
   - Show fee impact clearly to user
   - Display dispute window duration prominently
   - Implement transaction confirmation UI

### Error Handling

```typescript
try {
  const escrowId = await client.fund_escrow(amount, buyer);
  // Wait for on-chain confirmation
  await client.get_escrow(escrowId);
} catch (e) {
  if (e.code === 'ConflictingRoles') {
    // Handle buyer == seller error
  } else if (e.code === 'InvalidAmount') {
    // Handle zero or negative amount
  } else if (e.code === 'ContractPaused') {
    // Retry later
  }
}
```

---

## Known Limitations

### By Design
1. **No Escrow Cancellation Post-Fund** — Once funded, only 3 paths: complete, refund (via dispute), or auto-release. Buyer cannot cancel alone.
2. **Fixed Dispute Window** — 172800 seconds (2 days); not adjustable per-escrow.
3. **No Partial Releases** — Full amount or nothing; no milestone-based split payments.
4. **Single Resolver** — No multi-sig or voting for dispute resolution.
5. **No Token Whitelisting** — Any SEP-41 token accepted; contract-level filtering not enforced.

### Performance
6. **O(n) Lookups** — `get_escrows_by_buyer` must scan index; large indices may be slow.
7. **Persistent Storage** — All escrow history retained; storage grows unbounded.
8. **Single-Threaded** — Soroban transactions are sequential; no parallel operations.

### Operational
9. **Fee Cannot Be Retroactively Changed** — Fees are frozen at creation; new fee applied only to new escrows.
10. **No Emergency Pause Mitigation** — Pause stops all operations; unpause requires admin action.

---

## Recommended Security Practices

### For Users
- Use hardware wallets for buyer/seller roles
- Verify counterparty identity before funding
- Collect evidence before initiating disputes
- Don't fund escrows with entire amount; use smaller test amounts first
- Monitor event logs to confirm successful state transitions

### For Platforms / Integrators
- Rate-limit new user accounts
- Require email / KYC for high-value escrows
- Implement dispute escalation workflow
- Maintain off-chain evidence repository
- Implement refund insurance for users
- Audit all token pairs before use

### For Governance
- Audit fee structure quarterly
- Review resolver decisions monthly
- Maintain admin key in cold storage
- Rotate admin/fee-collector addresses annually
- Maintain incident response playbook
- Document all emergency pauses

---

## Incident Response

### If Paused

1. Assess root cause (config error, bug, attack)
2. Notify all stakeholders
3. Develop fix or mitigation
4. Test on testnet
5. Deploy fix to mainnet
6. Call unpause_contract

### If Funds Lost

1. Calculate total loss
2. Contact affected users
3. Prepare insurance payout (if applicable)
4. Post-mortem analysis
5. Implement prevention measures
6. Deploy patched contract (requires new deployment)

### If Unauthorized State Transition Detected

1. Pause contract immediately
2. Investigate logs and audit trail
3. Calculate affected escrows
4. If smart contract bug:
   - Develop fix
   - Test exhaustively
   - Deploy new contract
   - Migrate data (complex; requires governance)
5. If account compromise:
   - Rotate admin key
   - Check audit trail for malicious actions
   - Undo state if possible

---

## Compliance & Governance

### Legal Considerations
- Contract assumes no custodial relationship (trustless)
- Disputes resolved by chosen neutral (not platform)
- Platform exempt from liability for escrow terms
- Recommend legal review before mainnet deployment

### Regulatory
- Monitor AML/KYC requirements in deployment jurisdiction
- Implement transaction limits if required
- Document KYC procedures for auditors
- Maintain transaction logs for regulatory review

### Transparency
- Publish fee structure on website
- Document resolver selection and compensation
- Maintain public event log / block explorer link
- Publish security policy and incident response plan
- Provide transparency reports on disputes

---

## Testing Requirements

Before mainnet deployment:

### Unit Tests
- [ ] All entry points tested
- [ ] All error conditions triggered
- [ ] All state transitions validated
- [ ] All fee calculations verified
- [ ] All authorization checks confirmed

### Integration Tests
- [ ] End-to-end escrow flow (create → fund → complete)
- [ ] Dispute flow (create → fund → dispute → resolve)
- [ ] Auto-release flow (fund → wait → release)
- [ ] Multiple concurrent escrows
- [ ] Large amounts (near i128 limits)
- [ ] Zero amounts (should fail)
- [ ] Multiple token types

### Fuzz Tests
- [ ] Random escrow creation parameters
- [ ] Random fee configurations
- [ ] Random dispute timing
- [ ] Random token amounts
- [ ] Invalid input combinations

### Load Tests
- [ ] Rapid escrow creation
- [ ] Many concurrent escrows
- [ ] Large lookups (1000+ escrows per buyer)
- [ ] Storage size scaling

---

## Audit Sign-Off

**Contract**: TrustLink Escrow
**Soroban SDK**: 26.0.1
**Rust Edition**: 2021
**Build Date**: [DEPLOYMENT_DATE]
**WASM SHA256**: [SHA256_HASH]

### Required Approvals
- [ ] Code review by auditor
- [ ] Testing verification
- [ ] Threat model review
- [ ] Invariant confirmation
- [ ] Legal review
- [ ] Deployment checklist

### Sign-Off Date: ___________

---

## Emergency Contacts

- **Critical Bug**: security@trustlink.dev
- **Resolver Issues**: disputes@trustlink.dev
- **Operational**: ops@trustlink.dev

---

## Responsible Disclosure Policy

We take the security of TrustLink seriously. If you believe you have found a security vulnerability, please report it to us as described below.

### Reporting a Vulnerability

Please do not report security vulnerabilities through public GitHub issues.

Instead, please report them by emailing **security@trustlink.dev**.

You can encrypt your communication using our PGP public key:

```text
-----BEGIN PGP PUBLIC KEY BLOCK-----
Version: OpenPGP.js v5.9.0
Comment: https://openpgpjs.org

xsBNBGMz... [PLACEHOLDER PGP KEY] ...
-----END PGP PUBLIC KEY BLOCK-----
```
*(Note: A real PGP key will be rotated in prior to mainnet launch)*

### Response SLA

- **Acknowledgment**: Within 24 hours of receiving your report.
- **Initial Assessment**: Within 48 hours of acknowledgment.
- **Resolution**: Depending on severity, typically within 7-14 days.

### Bug Bounty

At this time, we do not have an automated bug bounty program. However, we evaluate verified, critical reports on a case-by-case basis for potential rewards.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-30 | Initial audit package |

---

## Additional Resources

- **AUDIT_SCOPE.md** — Detailed scoping and entry points
- **THREAT_MODEL.md** — Threat analysis with mitigations
- **INVARIANTS.md** — Correctness assumptions
- **ARCHITECTURE.md** — Design and state machine
- **Source Code**: contracts/escrow/src/
- **Tests**: contracts/escrow/src/test*.rs (60+ files)
- **Fuzz Targets**: contracts/escrow/fuzz/

---

**This document should be reviewed by all auditors before beginning engagement.**
