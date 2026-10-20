# Oracle Trust Model — Why the Backend Signer Is Needed

## Overview

TrustLink is a non-custodial P2P escrow on Stellar/Soroban. Most state transitions
are governed entirely by on-chain signatures from the involved parties (seller, buyer).
Two flows, however, require a trusted off-chain actor:

1. **Dispute resolution** — a designated resolver (Single address, Multi M-of-N committee, or Fallback pair) adjudicates whether funds
   go to the seller or back to the buyer.
2. **Auto-release triggering** — any caller (including a backend service) may invoke
   `auto_release` once the shipping window has elapsed.

This document describes the three resolver models supported by the contract and explains
what trust each requires, how voting and deadline gating work, and how to manage resolver
keys safely.

---

## Why a Resolver Is Necessary

Soroban contracts have no access to real-world shipping data, courier APIs, or legal
dispute processes. When buyer and seller disagree on delivery, the contract itself
cannot adjudicate. A neutral third party — the `resolver` — is therefore embedded in
each escrow at creation time.

The resolver configuration is selected when an escrow is created:

- **Single** stores one resolver address. That resolver authorizes the outcome directly.
  Can be rotated if needed via `rotate_resolver`.
- **Multi** stores an M-of-N committee. Each committee member may call `vote` with
  `Release` or `Refund`; once either outcome reaches the configured threshold, the escrow
  enters pending finalization and the recorded resolution can be finalized.
- **Fallback** stores a primary and backup resolver. The primary may act immediately;
  the backup becomes authorized only when the configured absolute `dispute_deadline` is
  reached. This deadline gates resolver authority, separately from the buyer's dispute
  window.

For the Single and Fallback primary paths, the authorized resolver's `require_auth()` call
is the way to move an escrow out of `Disputed` state. Multi uses the same address
authorization for each vote, followed by threshold-based resolution.

```
Disputed ──[resolver signs resolve_dispute / committee votes]──> Completed | Refunded
```

---

## Trust Assumptions

| Assumption | Where it appears | Risk if violated |
|---|---|---|
| Resolver will not collude with one party | `resolve_dispute` / `vote` requires resolver auth | Funds stolen or wrongly withheld |
| Resolver key is not compromised | Resolver signs every dispute settlement | Attacker can drain any disputed escrow |
| Resolver is liveness-available | Disputes resolved by configured resolver set (or time-delayed backup / timeout) | If all resolvers go offline and no fallback is configured, funds could be delayed until `claim_dispute_timeout` |
| Multi committee reaches a threshold | Votes are honest and sufficiently available; see the [`tally_votes` threshold rationale](contracts/escrow/src/internal.rs#L87-L126) | Split or incomplete votes can stall normal resolution until a documented escape hatch is used; see the [`tally_votes` deadlock rationale](contracts/escrow/src/internal.rs#L87-L126) |
| Fallback deadline is configured correctly | The backup is available when the deadline passes | A bad deadline can delay recovery or authorize the backup too early |
| Auto-release caller has no privileged access | `auto_release` is permissionless | None — caller cannot redirect funds |

### Multi-Resolver Liveness and Deadlock

Multi-resolver voting is an M-of-N quorum, not a majority rule. A split vote can leave
neither `Release` nor `Refund` at the threshold. In particular, unanimous configurations
can stall normal threshold resolution after all resolvers have voted different ways, because
no additional vote can change the tally. The implementation documents this limitation
and the implemented escape hatches in the [`tally_votes` comments in
`contracts/escrow/src/internal.rs`](contracts/escrow/src/internal.rs#L87-L126) and
[`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs):

1. **`resolve_deadlocked_dispute`** — once a dispute has been split (threshold not met)
   for `DISPUTE_DEADLOCK_WINDOW`, anyone may trigger simple-majority fallback
   (`tally_votes_majority`).
2. **`claim_dispute_timeout`** — if the dispute stays unresolved past the admin-configured
   `DisputeTimeout`, either party can claim a forced `Refund`.

Operators should avoid unanimous thresholds unless they have an operational process for
resolving a deadlock.

---

## Cryptographic Requirements

- The resolver must use an Ed25519 keypair (Stellar native) or a custom Soroban account
  contract implementing `__check_auth` (for multisig or hardware-key setups).
- In Single mode, `resolve_dispute` calls `escrow.resolver.require_auth()`. In Multi mode,
  each committee member authorizes their `vote`. In Fallback mode, the primary authorizes
  before `dispute_deadline`, and either primary or backup may authorize once the deadline
  is reached. Auth is anchored to the specific address(es) configured for the escrow.
- Evidence is committed as a 32-byte hash (SHA-256 of the off-chain evidence package,
  e.g. an IPFS CID). The contract does not verify the evidence content — the resolver is
  trusted to interpret it correctly.

---

## Security Risks

### 1. Resolver Key Compromise
If an attacker obtains the resolver's private key they can:
- Call `resolve_dispute` (or cast decisive votes) on any escrow in `Disputed` state.
- Choose any outcome (release to seller or refund buyer), effectively stealing funds.

**Mitigation**: Use a hardware security module (HSM) or multisig account contract as the
resolver. Soroban's `require_auth` accepts any address including custom account
contracts, so a 2-of-3 multisig resolver is fully supported. Multi-resolver committees
also distribute trust across multiple independent keys.

### 2. Resolver Censorship / Liveness Failure
A resolver that stops responding (key lost, service down, legal injunction) could leave
disputed escrows unresolved without recovery mechanisms.

**Mitigation**:
- **Dispute Timeout**: `claim_dispute_timeout` allows either buyer or seller to force a
  `Refund` once the configured dispute timeout window has elapsed without resolution.
- **Fallback Resolver**: Escrows created via `create_escrow_with_fallback` configure a backup
  resolver that becomes authorized after `dispute_deadline`.
- **Resolver Rotation**: For Single-resolver escrows, seller or admin can call
  `rotate_resolver` to replace an unresponsive resolver.
- **Deadlock Resolution**: For Multi-resolver escrows, `resolve_deadlocked_dispute` provides
  a simple-majority fallback after `DISPUTE_DEADLOCK_WINDOW`.
- Operate resolver infrastructure with high availability and monitoring.
- Store resolver private keys in geographically distributed secret-management systems.

### 3. Resolver Architecture Selection
Rather than relying on a single address with no recourse, contracts should choose the
appropriate resolver model at creation:
- **Single**: Simple, low-gas disputes; can be updated via `rotate_resolver`.
- **Fallback**: Built-in redundancy via primary/backup time-delayed handover.
- **Multi**: M-of-N committee eliminating single points of trust.

**Mitigation**: Use multisig account contracts or multi/fallback configurations so that
compromise or downtime of a single key cannot freeze or steal funds.

### 4. Evidence Integrity
The 32-byte `evidence_hash` is committed on-chain but its content is off-chain. The
resolver is trusted to verify that the hash corresponds to legitimate evidence before
signing a resolution.

**Mitigation**: Resolver infrastructure should re-derive the hash from the original
evidence package before signing, and refuse to sign if they cannot verify the source.

---

## Backup Strategies

### Resolver Key Backup
1. Generate the resolver keypair in a hardware wallet (Ledger, YubiHSM).
2. Store the mnemonic seed phrase in at minimum two physically separated, encrypted
   offline vaults.
3. For service-operated resolvers, use a KMS (AWS KMS, GCP Cloud KMS, HashiCorp Vault)
   with key material that never leaves the secure boundary.
4. Maintain a documented runbook for key recovery that has been tested at least annually.

### Multisig and Resolver Rotation
Because contract entry points accept any Soroban address as resolver, multisig account
contracts can be passed directly.
- For Single-resolver escrows, `rotate_resolver` can be called by seller or admin to update
  the resolver address on active escrows.
- For new escrows, deploy the updated multisig account contract and reference it at creation.
- For Multi and Fallback escrows, committee thresholds and backup time-gates provide
  in-protocol redundancy.

---

## Fee Collector Key Rotation

Unlike per-escrow resolver configurations (which are configured per-escrow and can be
rotated via `rotate_resolver` or handled via Multi/Fallback modes), the protocol-wide
fee collector can be rotated by calling `update_fee_collector(new_collector)`. This requires
authorization from the **current** fee collector, enabling a safe handoff:

```text
1. Generate or prepare the new collector address (can be a multisig contract).
2. Have the current fee collector sign a transaction calling update_fee_collector.
3. Verify get_fee_config() returns the new address.
4. Revoke or archive the old key.
```

This rotation path means the fee collector should itself be a multisig account for any
production deployment, so that key rotation is governed by a quorum rather than a
single signer.

---

## Auto-Release and the Backend Signer

`auto_release` is **permissionless**: any account — including an automated backend
service — can call it once `ledger.timestamp() >= funded_at + shipping_window`. The
caller cannot redirect funds; they always go to the seller minus the protocol fee.

The backend signer for auto-release therefore carries **no privileged trust** — its
compromise cannot steal funds, only trigger an early payout that the shipping window
was designed to allow anyway. Nevertheless, the service should be monitored to ensure
timely release and to prevent spamming the network with premature calls.

---

## Summary

| Component | Trust level | Rotation path |
|---|---|---|
| `resolver` | High — dispute finality (Single, Multi M-of-N committee, or Fallback primary/backup) | Single: `rotate_resolver`; Multi/Fallback: committee threshold / backup handover; or deploy new multisig |
| `fee_collector` | Medium — controls protocol revenue | `update_fee_collector` (current collector must sign) |
| Auto-release caller | None — permissionless, outcome is deterministic | N/A |

