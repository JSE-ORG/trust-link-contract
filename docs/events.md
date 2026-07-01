# Escrow Contract Events

This document details the events emitted by the Soroban TrustLink Escrow contract. Events use structured `symbol_short!` topics to enable efficient filtering by indexers, alongside detailed XDR data payloads.

## Schema Versioning

Every event payload carries a `schema_version: u32` field whose value is the
`EVENT_SCHEMA_VERSION` constant defined in `contracts/escrow/src/events.rs`.

**Current version: `1`**

Indexers and consumers **must** read `schema_version` before decoding the rest
of the payload.  A version mismatch means the XDR field layout has changed and
the consumer needs to be updated before it can safely decode the event.

### Changelog Policy

| Version | Change summary |
|---------|---------------|
| 1 | Initial versioned schema — `schema_version` field added to all event structs. |

**Rules for contributors:**

1. Any addition, removal, or rename of a field in an event struct **requires**
   incrementing `EVENT_SCHEMA_VERSION` in `events.rs`.
2. The increment must be accompanied by a new row in the table above describing
   the change.
3. The `CHANGELOG.md` `[Unreleased]` section must reference the new version
   number so that downstream consumers can plan their migrations.
4. Do **not** reuse a version number — versions are strictly increasing integers
   with no gaps.

## Topic Structure

Most events contain a standard structure:

```rust
(symbol_short!("Topic1"), symbol_short!("Topic2"), [indexed_participant_address])
```

By placing relevant addresses in the topics, indexers can filter directly for events involving a specific `Address` (e.g. all escrows created by a specific seller).

## Event Reference

> **Note:** All payload structs include `schema_version: u32` as their first
> field.  Only the remaining fields are listed below for brevity.

### Contract Initialization & Config
- **contract_initialized**: 
  - Topics: `["Contract", "Init"]`
  - Payload: `ContractInitialized` `{ schema_version, admin, fee_collector, arbitration_fee_bps, timestamp }`
- **admin_rotated**: 
  - Topics: `["Admin", "Rotated"]`
  - Payload: `AdminRotated` `{ schema_version, old_admin, new_admin, timestamp }`
- **contract_paused**: 
  - Topics: `["Contract", "Paused", admin]`
  - Payload: `ContractPausedEvent` `{ schema_version, admin, timestamp }`
- **contract_unpaused**: 
  - Topics: `["Contract", "Unpaused", admin]`
  - Payload: `ContractUnpausedEvent` `{ schema_version, admin, timestamp }`
- **allowlist_toggled**:
  - Topics: `["Allowlist", "Toggled"]`
  - Payload: `AllowlistToggled` `{ schema_version, enabled, timestamp }`
- **token_allowlist_updated**:
  - Topics: `["Token", "Allowlist", token]`
  - Payload: `TokenAllowlistUpdated` `{ schema_version, token, added, timestamp }`
- **treasury_updated**:
  - Topics: `["Treasury", "Updated"]`
  - Payload: `TreasuryUpdated` `{ schema_version, old_treasury, new_treasury, timestamp }`

### Fees
- **fee_updated**: 
  - Topics: `["Fee", "Updated"]`
  - Payload: `FeeUpdated` `{ schema_version, old_fee_bps, new_fee_bps, timestamp }`
- **protocol_fee_updated**: 
  - Topics: `["ProtoFee", "Updated"]`
  - Payload: `ProtocolFeeUpdated` `{ schema_version, old_fee_bps, new_fee_bps, timestamp }`
- **arbitration_fee_updated**: 
  - Topics: `["ArbFee", "Updated"]`
  - Payload: `ArbitrationFeeUpdated` `{ schema_version, old_fee_bps, new_fee_bps, timestamp }`
- **platform_fee_updated**:
  - Topics: `["PlatFee", "Updated"]`
  - Payload: `PlatformFeeUpdated` `{ schema_version, old_fee_bps, new_fee_bps, timestamp }`
- **fees_withdrawn**: 
  - Topics: `["Fee", "Withdrawn", to]`
  - Payload: `FeesWithdrawn` `{ schema_version, token, to, amount, timestamp }`

### Escrow Lifecycle
- **escrow_created**: 
  - Topics: `["Escrow", "Created", seller]`
  - Payload: `EscrowCreated` `{ schema_version, escrow_id, seller, resolver, token, amount, fee_bps, resolver_fee_bps, shipping_window, timestamp }`
- **basket_escrow_created**:
  - Topics: `["Basket", "Created", seller]`
  - Payload: `BasketEscrowCreated` `{ schema_version, escrow_id, seller, token_count, timestamp }`
- **escrow_funded**: 
  - Topics: `["Escrow", "Funded", buyer]`
  - Payload: `EscrowFunded` `{ schema_version, escrow_id, buyer, amount, timestamp }`
- **escrow_shipped**: 
  - Topics: `["Escrow", "Shipped", seller]`
  - Payload: `EscrowShipped` `{ schema_version, escrow_id, seller, tracking_id, timestamp }`
- **delivery_recorded**: 
  - Topics: `["Escrow", "Delivered"]`
  - Payload: `DeliveryRecorded` `{ schema_version, escrow_id, delivered_at }`
- **escrow_completed**: 
  - Topics: `["Escrow", "Completed", recipient]`
  - Payload: `EscrowCompleted` `{ schema_version, escrow_id, recipient, amount, fee_bps, timestamp }`
- **auto_released**: 
  - Topics: `["Escrow", "Released", seller]`
  - Payload: `AutoReleased` `{ schema_version, escrow_id, seller, amount, fee_bps, timestamp }`
- **escrow_cancelled**: 
  - Topics: `["Escrow", "Canceled", seller]`
  - Payload: `EscrowCancelled` `{ schema_version, escrow_id, seller, timestamp }`

### Dispute & Resolution
- **dispute_raised**: 
  - Topics: `["Dispute", "Raised", buyer]`
  - Payload: `DisputeRaised` `{ schema_version, escrow_id, buyer, reason, description, evidence_hash, timestamp }`
- **dispute_resolved**: 
  - Topics: `["Dispute", "Resolved", resolver]`
  - Payload: `DisputeResolved` `{ schema_version, escrow_id, resolver, resolution, recipient, amount, arbitration_fee, resolver_fee, timestamp }`
- **dispute_pending_finalization**:
  - Topics: `["Dispute", "Pending", resolver]`
  - Payload: `DisputePendingFinalization` `{ schema_version, escrow_id, resolver, resolution, amount, appeal_deadline, pending_at }`
- **dispute_appealed**:
  - Topics: `["Dispute", "Appealed", appellant]`
  - Payload: `DisputeAppealed` `{ schema_version, escrow_id, appellant, timestamp }`
- **resolver_rotated**: 
  - Topics: `["Resolver", "Rotated"]`
  - Payload: `ResolverRotated` `{ schema_version, escrow_id, old_resolver, new_resolver, rotated_at }`
- **resolver_vote_recorded**:
  - Topics: `["resolver_vote_recorded"]`
  - Payload: `ResolverVoteRecorded` `{ schema_version, escrow_id, resolver, resolution, vote_count, threshold, voted_at }`

### Messaging & Refunds
- **message_posted**:
  - Topics: `["Message", "Posted", sender]`
  - Payload: `MessagePosted` `{ schema_version, escrow_id, sender, timestamp }`
- **refund_requested**:
  - Topics: `["Refund", "Requested", buyer]`
  - Payload: `RefundRequestedEvent` `{ schema_version, escrow_id, buyer, timestamp }`
- **refund_approved**:
  - Topics: `["Refund", "Approved", seller]`
  - Payload: `RefundApprovedEvent` `{ schema_version, escrow_id, seller, timestamp }`

### Contract Upgrades
- **contract_upgraded**:
  - Topics: `["contract_upgraded"]`
  - Payload: `ContractUpgradedEvent` `{ schema_version, admin, new_wasm_hash, timestamp }`
