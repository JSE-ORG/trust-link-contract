# Escrow Contract Events

This document details the events emitted by the Soroban TrustLink Escrow contract. Events use structured `symbol_short!` topics to enable efficient filtering by indexers, alongside detailed XDR data payloads.

## Topic Structure

Most events contain a standard structure:

```rust
(symbol_short!("Topic1"), symbol_short!("Topic2"), [indexed_participant_address])
```

By placing relevant addresses in the topics, indexers can filter directly for events involving a specific `Address` (e.g. all escrows created by a specific seller).

## Event Reference

### Contract Initialization & Config
- **contract_initialized**: 
  - Topics: `["Contract", "Init"]`
  - Payload: `ContractInitialized` `{ admin, fee_collector, arbitration_fee_bps, timestamp }`
- **admin_rotated**: 
  - Topics: `["Admin", "Rotated"]`
  - Payload: `AdminRotated` `{ old_admin, new_admin, timestamp }`
- **contract_paused**: 
  - Topics: `["Contract", "Paused", admin]`
  - Payload: `ContractPausedEvent` `{ admin, timestamp }`
- **contract_unpaused**: 
  - Topics: `["Contract", "Unpaused", admin]`
  - Payload: `ContractUnpausedEvent` `{ admin, timestamp }`
- **allowlist_toggled**:
  - Topics: `["Allowlist", "Toggled"]`
  - Payload: `AllowlistToggled` `{ enabled, timestamp }`
- **token_allowlist_updated**:
  - Topics: `["Token", "Allowlist", token]`
  - Payload: `TokenAllowlistUpdated` `{ token, added, timestamp }`
- **treasury_updated**:
  - Topics: `["Treasury", "Updated"]`
  - Payload: `TreasuryUpdated` `{ old_treasury, new_treasury, timestamp }`

### Fees
- **fee_updated**: 
  - Topics: `["Fee", "Updated"]`
  - Payload: `FeeUpdated` `{ old_fee_bps, new_fee_bps, timestamp }`
- **protocol_fee_updated**: 
  - Topics: `["ProtoFee", "Updated"]`
  - Payload: `ProtocolFeeUpdated` `{ old_fee_bps, new_fee_bps, timestamp }`
- **arbitration_fee_updated**: 
  - Topics: `["ArbFee", "Updated"]`
  - Payload: `ArbitrationFeeUpdated` `{ old_fee_bps, new_fee_bps, timestamp }`
- **platform_fee_updated**:
  - Topics: `["PlatFee", "Updated"]`
  - Payload: `PlatformFeeUpdated` `{ old_fee_bps, new_fee_bps, timestamp }`
- **fees_withdrawn**: 
  - Topics: `["Fee", "Withdrawn", to]`
  - Payload: `FeesWithdrawn` `{ token, to, amount, timestamp }`

### Escrow Lifecycle
- **escrow_created**: 
  - Topics: `["Escrow", "Created", seller]`
  - Payload: `EscrowCreated` `{ escrow_id, seller, resolver, token, amount, fee_bps, resolver_fee_bps, shipping_window, timestamp }`
- **basket_escrow_created**:
  - Topics: `["Basket", "Created", seller]`
  - Payload: `BasketEscrowCreated` `{ escrow_id, seller, token_count, timestamp }`
- **escrow_funded**: 
  - Topics: `["Escrow", "Funded", buyer]`
  - Payload: `EscrowFunded` `{ escrow_id, buyer, amount, funded_at }`
- **escrow_shipped**: 
  - Topics: `["Escrow", "Shipped", seller]`
  - Payload: `EscrowShipped` `{ escrow_id, seller, tracking_id, shipped_at }`
- **delivery_recorded**: 
  - Topics: `["Escrow", "Delivered"]`
  - Payload: `DeliveryRecorded` `{ escrow_id, delivered_at }`
- **escrow_completed**: 
  - Topics: `["Escrow", "Completed", recipient]`
  - Payload: `EscrowCompleted` `{ escrow_id, recipient, amount, fee_bps, completed_at }`
- **auto_released**: 
  - Topics: `["Escrow", "Released", seller]`
  - Payload: `AutoReleased` `{ escrow_id, seller, amount, fee_bps, released_at }`
- **escrow_cancelled**: 
  - Topics: `["Escrow", "Canceled", seller]`
  - Payload: `EscrowCancelled` `{ escrow_id, seller, cancelled_at }`

### Dispute & Resolution
- **dispute_raised**: 
  - Topics: `["Dispute", "Raised", buyer]`
  - Payload: `DisputeRaised` `{ escrow_id, buyer, reason, description, evidence_hash, disputed_at }`
- **dispute_resolved**: 
  - Topics: `["Dispute", "Resolved", resolver]`
  - Payload: `DisputeResolved` `{ escrow_id, resolver, resolution, recipient, amount, arbitration_fee, resolver_fee, resolved_at }`
- **dispute_pending_finalization**:
  - Topics: `["Dispute", "Pending", resolver]`
  - Payload: `DisputePendingFinalization` `{ escrow_id, resolver, resolution, amount, appeal_deadline, pending_at }`
- **dispute_appealed**:
  - Topics: `["Dispute", "Appealed", appellant]`
  - Payload: `DisputeAppealed` `{ escrow_id, appellant, timestamp }`
- **resolver_rotated**: 
  - Topics: `["Resolver", "Rotated"]`
  - Payload: `ResolverRotated` `{ escrow_id, old_resolver, new_resolver, rotated_at }`

### Messaging & Refunds
- **message_posted**:
  - Topics: `["Message", "Posted", sender]`
  - Payload: `MessagePosted` `{ escrow_id, sender, timestamp }`
- **refund_requested**:
  - Topics: `["Refund", "Requested", buyer]`
  - Payload: `RefundRequestedEvent` `{ escrow_id, buyer, timestamp }`
- **refund_approved**:
  - Topics: `["Refund", "Approved", seller]`
  - Payload: `RefundApprovedEvent` `{ escrow_id, seller, timestamp }`
