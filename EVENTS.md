# TrustLink Escrow Contract Event Registry

This document is the authoritative specification of every event emitted by the TrustLink escrow contract. It is derived directly from `contracts/escrow/src/events.rs`; any discrepancy should be resolved by treating the contract source as canonical and updating this file.

All events are published through Soroban's `env.events().publish(topics, payload)`. Indexers should filter by the emitting contract address first, then by the topic tuple described below. The payload is a `#[contracttype]` XDR struct decoded from the event's data field.

## Schema versioning

- `pub const EVENT_SCHEMA_VERSION: u32 = 5` is stamped into the `schema_version` field of every payload.
- Every payload begins with `schema_version: u32`. Consumers must decode this field before any other field.
- Increment `EVENT_SCHEMA_VERSION` whenever a field is added, removed, or renamed in **any** event struct; the version is contract-wide, not per-event.
- Consumers should reject or skip events whose `schema_version` is greater than the version they understand, rather than attempting a best-effort decode.
- Versions 1–4 are historical snapshots. `basket_tokens` was added in version 3 to `EscrowFunded` and `RefundRequestedEvent`; version 4 was the previous shape. Version 5 adds `token` to `EscrowExpired` and introduces `MulticallSubcallFailed`; version 5 is the current shape.

## Decoding and indexing best practices

1. **Filter by contract.** Match the event's contract/contract ID before inspecting topics; token contracts emit their own transfer events with unrelated topics.
2. **Match topic tuples in order.** Topics are the identity of an event. The first two are usually short symbols; later topics may be an `Address`, `u64`, `u32`, or `Symbol`.
3. **Dynamic symbols.** `symbol_short!` values are at most 9 characters. Longer names are created with `Symbol::new` (`resolver_vote_recorded`, `contract_upgraded`, `storage_migrated`, `PendingExpiry`) and must be compared as full symbol strings, not truncated.
4. **Actors in topics vs payload.** Several events carry an actor both in a topic and in the payload (for example `seller`, `resolver`, `buyer`). This is intentional: topics enable cheap filtering; payload fields are the decoded XDR record. `FeeCollectorUpdated` is the exception: `new_collector` appears **only** in the topic, while the payload contains `old_collector`.
5. **Numeric widths.** `u32`/`u64`/`i128` must be decoded with their exact widths. `i128` amounts are token base units, not human-readable decimals.
6. **Enums.** `crate::EscrowState` values are `Pending`, `Funded`, `Shipped`, `Completed`, `Disputed`, `RefundRequested`, `Refunded`, `Canceled`, `PendingFinalization`, `Expired`. `ResolutionType` values are `Release` and `Refund`.
7. **Timestamps.** `timestamp`, `*_at`, and `*_deadline` fields are Unix timestamps in seconds taken from `env.ledger().timestamp()`.
8. **Optional basket data.** `Option<Vec<(Address, i128)>>` is `None` for single-token escrows; when present, each pair is the token address and its amount.
9. **Unknown events.** Treat an unrecognised topic tuple as opaque; do not attempt to decode it with a neighbouring struct.

Rust decoders can use `soroban_sdk::TryFromVal` against the event data after matching topics:

```rust,ignore
let body = match &event.body {
    ContractEventBody::V0(body) => body,
};
let topics: Vec<Val> = body.topics.clone();
if topics.get(0) == symbol_short!("Treasury").into_val(env)
    && topics.get(1) == symbol_short!("Fee").into_val(env)
{
    let fee: PlatformFeeCollected = PlatformFeeCollected::try_from_val(env, &body.data)?;
    // fee.schema_version == EVENT_SCHEMA_VERSION for the current contract build
}
```

## Topic registry and XDR payload layouts

The tables below list all 62 event topics. The `Payload fields` column is the exact XDR struct layout in declaration order. Rust tuple trailing commas are omitted from the topic column for readability; topic values and their order are unchanged.

### Lifecycle events

| Event / data struct | Topic tuple | Payload fields (`name: type`) | Description |
|---|---|---|---|
| `EscrowCreated` | `(symbol_short!("Escrow"), symbol_short!("Created"), seller.clone(), resolver.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`seller: Address`<br>`resolver: Address`<br>`token: Address`<br>`amount: i128`<br>`fee_bps: u32`<br>`resolver_fee_bps: u32`<br>`shipping_window: u64`<br>`expires_at: Option<u64>`<br>`timestamp: u64`<br>`new_state: EscrowState` | Escrow created with snapshotted fee rates and expiry. |
| `EscrowFunded` | `(symbol_short!("Escrow"), symbol_short!("Funded"), escrow_id, buyer.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`buyer: Address`<br>`amount: i128`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState`<br>`basket_tokens: Option<Vec<(Address, i128)>>` | Buyer funded the escrow; basket tokens included when applicable. |
| `EscrowShipped` | `(symbol_short!("Escrow"), symbol_short!("Shipped"), seller.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`seller: Address`<br>`tracking_id: String`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState` | Seller marked goods shipped with tracking data. |
| `DeliveryRecorded` | `(symbol_short!("Escrow"), symbol_short!("Delivered"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`delivered_at: u64` | Delivery was recorded. |
| `EscrowCompleted` | `(symbol_short!("Escrow"), symbol_short!("Completed"), recipient.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`recipient: Address`<br>`amount: i128`<br>`fee_bps: u32`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState` | Normal completion and payout to the recipient. |
| `AutoReleased` | `(symbol_short!("Escrow"), symbol_short!("Released"), seller.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`seller: Address`<br>`amount: i128`<br>`fee_bps: u32`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState` | Funds auto-released to the seller after the shipping window. |
| `EscrowCanceled` | `(symbol_short!("Escrow"), symbol_short!("Canceled"), cancelled_by.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`seller: Address`<br>`cancelled_by: Address`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState` | Explicit cancellation. `cancelled_by` may be the buyer, not the seller. |
| `EscrowExpired` | `(symbol_short!("Escrow"), symbol_short!("Expired"), buyer.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`buyer: Address`<br>`token: Address`<br>`amount: i128`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState` | `reclaim_expired` returned principal (and basket tokens) to the buyer. |
| `EscrowAutoCanceled` | `(symbol_short!("Escrow"), symbol_short!("AutoCncl"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`timestamp: u64` | Permissionless `auto_cancel_pending` sweep; no actor topic. |
| `BasketEscrowCreated` | `(symbol_short!("Basket"), symbol_short!("Created"), seller.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`seller: Address`<br>`token_count: u32`<br>`timestamp: u64` | Multi-token basket escrow created. |
| `MessagePosted` | `(symbol_short!("Message"), symbol_short!("Posted"), sender.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`sender: Address`<br>`timestamp: u64` | Escrow-scoped message posted. |
| `RefundRequestedEvent` | `(symbol_short!("Refund"), symbol_short!("Requested"), buyer.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`buyer: Address`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState`<br>`basket_tokens: Option<Vec<(Address, i128)>>` | Buyer requested a refund; basket tokens included when applicable. |
| `RefundApprovedEvent` | `(symbol_short!("Refund"), symbol_short!("Approved"), seller.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`seller: Address`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState` | Refund approved. |
| `DeliveryProposed` | `(symbol_short!("Delivery"), symbol_short!("Proposed"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`proposed_at: u64`<br>`unlock_at: u64` | Delivery proposal queued with a timelocked unlock time. |
| `DeliveryProposalCancelled` | `(symbol_short!("Delivery"), symbol_short!("Cancelled"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`timestamp: u64` | Delivery proposal cancelled. |
| `PendingExpiryClear` | `(Symbol::new(env, "PendingExpiry"), symbol_short!("Cleared"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`timestamp: u64` | Stale pending-expiry marker cleared. |

### Dispute events

| Event / data struct | Topic tuple | Payload fields (`name: type`) | Description |
|---|---|---|---|
| `DisputeRaised` | `(symbol_short!("Dispute"), symbol_short!("Raised"), buyer.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`buyer: Address`<br>`reason: Symbol`<br>`description: Bytes`<br>`evidence_hash: BytesN<32>`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState` | Buyer raised a dispute with reason and evidence hash. |
| `DisputeEvidenceSubmitted` | `(symbol_short!("Dispute"), symbol_short!("Evidence"), submitter.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`submitter: Address`<br>`evidence_hash: BytesN<32>`<br>`description: Bytes`<br>`entry_count: u32`<br>`timestamp: u64` | Additional dispute evidence submitted. |
| `ResolverVoteRecorded` | `(Symbol::new(env, "resolver_vote_recorded"),)` | `schema_version: u32`<br>`escrow_id: u64`<br>`resolver: Address`<br>`resolution: ResolutionType`<br>`vote_count: u32`<br>`threshold: u32`<br>`voted_at: u64` | A multi-resolver vote was recorded. |
| `DisputePendingFinalization` | `(symbol_short!("Dispute"), symbol_short!("Pending"), resolver.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`resolver: Address`<br>`resolution: ResolutionType`<br>`amount: i128`<br>`appeal_deadline: u64`<br>`pending_at: u64` | Resolution reached; awaiting the appeal window before payout. |
| `DisputeAppealed` | `(symbol_short!("Dispute"), symbol_short!("Appealed"), appellant.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`appellant: Address`<br>`timestamp: u64`<br>`appeal_fee_charged: i128`<br>`appeal_round: u32` | Resolution appealed; votes and resolution state reset. |
| `DisputeResolved` | `(symbol_short!("Dispute"), symbol_short!("Resolved"), resolver.clone())` | `schema_version: u32`<br>`escrow_id: u64`<br>`resolver: Address`<br>`resolution: ResolutionType`<br>`recipient: Address`<br>`amount: i128`<br>`arbitration_fee: i128`<br>`resolver_fee: i128`<br>`platform_fee: i128`<br>`timestamp: u64`<br>`prev_state: EscrowState`<br>`new_state: EscrowState` | Final dispute settlement with fee breakdown. |

### Fee and treasury events

| Event / data struct | Topic tuple | Payload fields (`name: type`) | Description |
|---|---|---|---|
| `FeeUpdated` | `(symbol_short!("Fee"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_fee_bps: u32`<br>`new_fee_bps: u32`<br>`timestamp: u64` | Escrow fee rate changed. |
| `ProtocolFeeUpdated` | `(symbol_short!("ProtoFee"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_fee_bps: u32`<br>`new_fee_bps: u32`<br>`timestamp: u64` | Protocol fee rate changed. |
| `ArbitrationFeeUpdated` | `(symbol_short!("ArbFee"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_fee_bps: u32`<br>`new_fee_bps: u32`<br>`timestamp: u64` | Arbitration fee rate changed. |
| `PlatformFeeUpdated` | `(symbol_short!("PlatFee"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_fee_bps: u32`<br>`new_fee_bps: u32`<br>`timestamp: u64` | Platform fee rate changed. |
| `AppealFeeUpdated` | `(symbol_short!("AppealFee"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_fee_bps: u32`<br>`new_fee_bps: u32`<br>`timestamp: u64` | Appeal fee rate changed. |
| `FeeCollectorPending` | `(symbol_short!("FeeColl"), symbol_short!("Pending"))` | `schema_version: u32`<br>`old_collector: Address`<br>`pending_collector: Address`<br>`timestamp: u64` | Fee-collector rotation proposed. |
| `FeeCollectorAccepted` | `(symbol_short!("FeeColl"), symbol_short!("Accepted"))` | `schema_version: u32`<br>`old_collector: Address`<br>`new_collector: Address`<br>`timestamp: u64` | Fee-collector rotation accepted. |
| `FeeCollectorUpdated` | `(symbol_short!("FeeColl"), symbol_short!("Updated"), new_collector.clone())` | `schema_version: u32`<br>`old_collector: Address`<br>`timestamp: u64` | Legacy fee-collector update notice; the new collector is only in the topic. |
| `TreasuryPending` | `(symbol_short!("Treasury"), symbol_short!("Pending"))` | `schema_version: u32`<br>`old_treasury: Address`<br>`pending_treasury: Address`<br>`timestamp: u64` | Treasury rotation proposed. |
| `TreasuryAccepted` | `(symbol_short!("Treasury"), symbol_short!("Accepted"))` | `schema_version: u32`<br>`old_treasury: Address`<br>`new_treasury: Address`<br>`timestamp: u64` | Treasury rotation accepted. |
| `TreasuryUpdated` | `(symbol_short!("Treasury"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_treasury: Address`<br>`new_treasury: Address`<br>`timestamp: u64` | Legacy direct treasury change notice. |
| `PlatformFeeCollected` | `(symbol_short!("Treasury"), symbol_short!("Fee"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`token: Address`<br>`treasury: Address`<br>`fee_amount: i128`<br>`timestamp: u64` | Emitted for every non-zero platform fee transferred to the treasury, including basket-token fees. |

### Resolver and access-control events

| Event / data struct | Topic tuple | Payload fields (`name: type`) | Description |
|---|---|---|---|
| `ResolverRotated` | `(symbol_short!("Resolver"), symbol_short!("Rotated"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`old_resolver: Address`<br>`new_resolver: Address`<br>`rotated_at: u64` | Escrow resolver replaced. |
| `ResolverSanctioned` | `(symbol_short!("Resolver"), symbol_short!("Sanction"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`old_resolver: Address`<br>`replacement_resolver: Address`<br>`sanctioned_by: Address`<br>`sanctioned_at: u64` | Resolver sanctioned and replaced. |
| `ResolverApproved` | `(symbol_short!("Resolver"), symbol_short!("Approved"), resolver.clone())` | `schema_version: u32`<br>`resolver: Address`<br>`caller: Address`<br>`timestamp: u64` | Resolver added to the approved set. |
| `ResolverRemoved` | `(symbol_short!("Resolver"), symbol_short!("Removed"), resolver.clone())` | `schema_version: u32`<br>`resolver: Address`<br>`caller: Address`<br>`timestamp: u64` | Resolver removed from the approved set. |
| `ResolverStrictUpdated` | `(symbol_short!("ResStrct"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_strict: bool`<br>`new_strict: bool`<br>`caller: Address`<br>`timestamp: u64` | Strict resolver mode toggled. |
| `TokenAllowlistUpdated` | `(symbol_short!("Token"), symbol_short!("Allowlist"), token.clone())` | `schema_version: u32`<br>`added: bool`<br>`timestamp: u64` | Token added to or removed from the allowlist. |
| `AllowlistToggled` | `(symbol_short!("Allowlist"), symbol_short!("Toggled"))` | `schema_version: u32`<br>`enabled: bool`<br>`timestamp: u64` | Token allowlist enforcement enabled or disabled. |

### System and configuration events

| Event / data struct | Topic tuple | Payload fields (`name: type`) | Description |
|---|---|---|---|
| `ContractInitialized` | `(symbol_short!("Contract"), symbol_short!("Init"))` | `schema_version: u32`<br>`admin: Address`<br>`fee_collector: Address`<br>`arbitration_fee_bps: u32`<br>`timestamp: u64` | Contract initialized. |
| `AdminRotated` | `(symbol_short!("Admin"), symbol_short!("Rotated"))` | `schema_version: u32`<br>`old_admin: Address`<br>`new_admin: Address`<br>`timestamp: u64` | Admin role rotated. |
| `ContractPausedEvent` | `(symbol_short!("Contract"), symbol_short!("Paused"), admin.clone())` | `schema_version: u32`<br>`admin: Address`<br>`timestamp: u64` | Entire contract paused. |
| `ContractUnpausedEvent` | `(symbol_short!("Contract"), symbol_short!("Unpaused"), admin.clone())` | `schema_version: u32`<br>`admin: Address`<br>`timestamp: u64` | Entire contract unpaused. |
| `ActionPausedEvent` | `(symbol_short!("Action"), symbol_short!("Paused"), action.clone())` | `schema_version: u32`<br>`action: Symbol`<br>`caller: Address`<br>`timestamp: u64` | Individual action paused. |
| `ActionUnpausedEvent` | `(symbol_short!("Action"), symbol_short!("Unpaused"), action.clone())` | `schema_version: u32`<br>`action: Symbol`<br>`caller: Address`<br>`timestamp: u64` | Individual action unpaused. |
| `ContractUpgradedEvent` | `(Symbol::new(env, "contract_upgraded"),)` | `schema_version: u32`<br>`admin: Address`<br>`new_wasm_hash: BytesN<32>`<br>`timestamp: u64` | WASM upgrade executed. |
| `StorageMigratedEvent` | `(Symbol::new(env, "storage_migrated"),)` | `schema_version: u32`<br>`admin: Address`<br>`from_version: u32`<br>`to_version: u32`<br>`timestamp: u64` | Storage migration completed. |
| `TtlExtensionUpdated` | `(symbol_short!("TtlExt"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_ledgers: u32`<br>`new_ledgers: u32`<br>`caller: Address`<br>`timestamp: u64` | TTL extension policy changed. |
| `AmountLimitsUpdated` | `(symbol_short!("AmtLimit"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_min_amount: i128`<br>`new_min_amount: i128`<br>`old_max_amount: i128`<br>`new_max_amount: i128`<br>`caller: Address`<br>`timestamp: u64` | Min/max escrow amount limits changed. |
| `MaxAppealsUpdated` | `(symbol_short!("MaxAppeal"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_max: u32`<br>`new_max: u32`<br>`timestamp: u64` | Maximum appeal rounds changed. |
| `MaxBasketSizeUpdated` | `(symbol_short!("MaxBasket"), symbol_short!("Updated"))` | `schema_version: u32`<br>`old_max: u32`<br>`new_max: u32`<br>`timestamp: u64` | Maximum basket token count changed. |

### Timelock, emergency, and recovery events

| Event / data struct | Topic tuple | Payload fields (`name: type`) | Description |
|---|---|---|---|
| `TimelockQueued` | `(symbol_short!("Timelock"), symbol_short!("Queued"))` | `schema_version: u32`<br>`operation: u32`<br>`proposer: Address`<br>`queued_at: u64`<br>`ready_at: u64` | Administrative operation queued. |
| `TimelockExecuted` | `(symbol_short!("Timelock"), symbol_short!("Executed"))` | `schema_version: u32`<br>`operation: u32`<br>`proposer: Address`<br>`executor: Address`<br>`params: Vec<Val>`<br>`executed_at: u64` | Queued operation executed. |
| `TimelockCancelled` | `(symbol_short!("Timelock"), symbol_short!("Cancelled"))` | `schema_version: u32`<br>`operation: u32`<br>`proposer: Address`<br>`canceller: Address`<br>`cancelled_at: u64` | Queued operation cancelled. |
| `TimelockDelayUpdated` | `(symbol_short!("Admin"), symbol_short!("DelayUpd"))` | `schema_version: u32`<br>`old_delay: u64`<br>`new_delay: u64`<br>`caller: Address`<br>`timestamp: u64` | Timelock delay changed. |
| `EmergencyDrain` | `(symbol_short!("Emergency"), symbol_short!("Drain"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`token: Address`<br>`amount: i128`<br>`timestamp: u64` | Emergency drain of a single token from an escrow. |
| `EmergencyDrainBlocked` | `(symbol_short!("Emergency"), symbol_short!("Blocked"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`timestamp: u64` | Diagnostic: a drain attempt was rejected. |
| `RecoveryWithdraw` | `(symbol_short!("Recovery"), symbol_short!("Withdraw"))` | `schema_version: u32`<br>`escrow_id: u64`<br>`buyer: Address`<br>`amount: i128`<br>`timestamp: u64` | Recovery-mode withdrawal to the buyer. |
| `RecoveryModeUpdated` | `(symbol_short!("Recovery"), symbol_short!("ModeUpd"))` | `schema_version: u32`<br>`enabled: bool`<br>`caller: Address`<br>`timestamp: u64` | Recovery mode enabled or disabled. |

> Note: the source doc comments on the three `Timelock*` emitters mention an `operation as u32` topic. The actual `publish` calls use only the two-symbol tuples shown above; `operation` is a payload field, not a topic. This table documents the emitted reality.

### Diagnostic events

| Event / data struct | Topic tuple | Payload fields (`name: type`) | Description |
|---|---|---|---|
| `MulticallSubcallFailed` | `(symbol_short!("Multicall"), symbol_short!("Failed"))` | `schema_version: u32`<br>`index: u32`<br>`function: Symbol`<br>`error_code: u32` | A `multicall` sub-call failed at the given zero-based batch index. Published before the batch reverts, so the record is diagnostic only and never committed as a successful contract event. |

## Example XDR payloads

The following are illustrative JSON renderings of the topic XDR and payload XDR. Addresses are shortened; `i128` values are shown as decimal strings to avoid precision loss.

### `EscrowCreated`

```json
{
  "topics": [
    { "symbol": "Escrow" },
    { "symbol": "Created" },
    { "address": "G...SELLER" },
    { "address": "G...RESOLVER" }
  ],
  "value": {
    "EscrowCreated": {
      "schema_version": 5,
      "escrow_id": 42,
      "seller": "G...SELLER",
      "resolver": "G...RESOLVER",
      "token": "C...USDC",
      "amount": "10000000",
      "fee_bps": 25,
      "resolver_fee_bps": 10,
      "shipping_window": 604800,
      "expires_at": 1799999999,
      "timestamp": 1780000000,
      "new_state": "Pending"
    }
  }
}
```

### `EscrowFunded` with basket tokens

```json
{
  "topics": [
    { "symbol": "Escrow" },
    { "symbol": "Funded" },
    { "u64": 42 },
    { "address": "G...BUYER" }
  ],
  "value": {
    "EscrowFunded": {
      "schema_version": 5,
      "escrow_id": 42,
      "buyer": "G...BUYER",
      "amount": "10000000",
      "timestamp": 1780000100,
      "prev_state": "Pending",
      "new_state": "Funded",
      "basket_tokens": [
        ["C...TOKEN_A", "5000000"],
        ["C...TOKEN_B", "2000000"]
      ]
    }
  }
}
```

### `DisputeRaised`

```json
{
  "topics": [
    { "symbol": "Dispute" },
    { "symbol": "Raised" },
    { "address": "G...BUYER" }
  ],
  "value": {
    "DisputeRaised": {
      "schema_version": 5,
      "escrow_id": 42,
      "buyer": "G...BUYER",
      "reason": "OTHER",
      "description": "0x64657363",
      "evidence_hash": "0x0101...0101",
      "timestamp": 1780000200,
      "prev_state": "Shipped",
      "new_state": "Disputed"
    }
  }
}
```

### `DisputeResolved`

```json
{
  "topics": [
    { "symbol": "Dispute" },
    { "symbol": "Resolved" },
    { "address": "G...RESOLVER" }
  ],
  "value": {
    "DisputeResolved": {
      "schema_version": 5,
      "escrow_id": 42,
      "resolver": "G...RESOLVER",
      "resolution": "Release",
      "recipient": "G...SELLER",
      "amount": "10000000",
      "arbitration_fee": "100000",
      "resolver_fee": "50000",
      "platform_fee": "25000",
      "timestamp": 1780000300,
      "prev_state": "PendingFinalization",
      "new_state": "Completed"
    }
  }
}
```

### `PlatformFeeCollected`

```json
{
  "topics": [
    { "symbol": "Treasury" },
    { "symbol": "Fee" }
  ],
  "value": {
    "PlatformFeeCollected": {
      "schema_version": 5,
      "escrow_id": 42,
      "token": "C...USDC",
      "treasury": "G...TREASURY",
      "fee_amount": "25000",
      "timestamp": 1780000300
    }
  }
}
```

## Completeness checklist

- [x] All `#[contracttype]` event structs in `contracts/escrow/src/events.rs` are represented.
- [x] All 62 `emit_*` topic tuples are documented (Rust trailing commas omitted for readability).
- [x] Every payload lists its fields in XDR declaration order.
- [x] Schema version rules and decoding guidance are documented.
- [x] Example payloads cover lifecycle, dispute, basket, and fee accounting events.
