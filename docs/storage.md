# Storage Layout

This document is the authoritative reference for **every** storage entry the
TrustLink escrow contract reads or writes: its key, value type, storage tier
(instance vs. persistent), and TTL strategy. It lets developers, auditors, and
indexer authors determine the exact on-chain storage layout without reading the
source.

Source of truth:
- Key enums: [`contracts/escrow/src/types.rs`](../contracts/escrow/src/types.rs) (`DataKey`)
  and [`contracts/escrow/src/storage.rs`](../contracts/escrow/src/storage.rs) (`StorageKey`).
- Access + TTL logic: [`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs)
  and [`contracts/escrow/src/storage.rs`](../contracts/escrow/src/storage.rs).

---

## Soroban storage tiers in one minute

Soroban exposes three storage tiers. This contract uses **two** of them:

| Tier | Used here | Lifetime | Purpose in this contract |
|---|---|---|---|
| **Instance** | ✅ | Shares the contract instance's TTL; all entries live/expire together | Global singletons: admin, fees, counters, pause flag, stats |
| **Persistent** | ✅ | Independent per-entry TTL; can be archived and restored | Per-escrow records, per-dispute records, and per-user indexes |
| **Temporary** | ❌ (not used) | Cheap, expires permanently | — |

There is **no `temporary()` storage** anywhere in the contract.

---

## TTL strategy

Persistent and instance entries are kept alive by bumping their TTL.

- **Extension amount** — controlled by `DataKey::TtlExtensionLedgers` (instance,
  `u32`). If unset it falls back to the constant `DEFAULT_TTL_EXTENSION =
  120_960` ledgers (≈ 7 days at ~5 s/ledger). The admin can change it via
  `set_ttl_extension`.
- **Bump pattern** — every bump uses `extend_ttl(threshold = ext / 2, extend_to
  = ext)`: "if fewer than `ext/2` ledgers remain, top the TTL back up to `ext`."
- **Persistent entries** (`Escrow`, `Dispute`, `VendorEscrowIndex`) are bumped on
  **every read and write** of that entry.
- **Instance storage** is bumped once per `create_escrow` call (alongside the
  escrow-counter increment), which keeps all instance singletons alive together.

> ⚠️ **Known exception:** `DataKey::BuyerEscrowIndex` is written to persistent
> storage **without** a TTL bump (see [Implementation notes](#implementation-notes)).

---

## `DataKey` — primary key enum

Defined in [`types.rs`](../contracts/escrow/src/types.rs). This is the enum used
throughout the contract.

### Instance storage (global singletons)

| Key | Value type | Description |
|---|---|---|
| `Admin` | `Address` | Contract administrator (set once in `initialize`; rotatable via admin rotation). |
| `FeeCollector` | `Address` | Address that receives swept protocol/arbitration fees. |
| `FeeConfig` | `FeeConfig { protocol_fee_bps: u32, arbitration_fee_bps: u32 }` | Active fee rates in basis points. |
| `Paused` | `bool` | Global pause flag; defaults to `false` when unset. |
| `EscrowCounter` | `u64` | Monotonic counter producing the next escrow ID. |
| `TtlExtensionLedgers` | `u32` | Configurable TTL extension (ledgers). Falls back to `120_960` when unset. |
| `TotalArbitrationFees(Address)` | `i128` | Per-token running total of arbitration fees collected. Keyed by token address. |
| `AccumulatedFees(Address)` | `i128` | Per-token fees sitting in the vault that are withdrawable via `withdraw_fees`. Keyed by token address. |
| `TotalCreated` | `u64` | Lifetime count of escrows created (stats). |
| `TotalCompleted` | `u64` | Lifetime count of escrows completed (stats). |
| `TotalDisputed` | `u64` | Lifetime count of escrows disputed (stats). |
| `TotalRefunded` | `u64` | Lifetime count of escrows refunded (stats). |

All instance entries share the contract instance TTL (bumped during
`create_escrow`). Counters default to `0` when unset.

### Persistent storage (per-record)

| Key | Value type | TTL bumped on | Description |
|---|---|---|---|
| `Escrow(u64)` | `EscrowData` | read **and** write | The full escrow record, keyed by escrow ID. See [`types.rs`](../contracts/escrow/src/types.rs) for `EscrowData`. |
| `Dispute(u64)` | `DisputeData` | read **and** write | The dispute record for an escrow, keyed by escrow ID. |
| `BuyerEscrowIndex(Address)` | `Vec<u64>` | ⚠️ **not bumped** | List of escrow IDs a buyer has funded. Keyed by buyer address. |

### Declared but unused (legacy)

These variants exist in the `DataKey` enum but are **never read or written** by
the current contract. They are retained for enum/ABI stability and should be
treated as reserved:

| Key | Status |
|---|---|
| `DefaultFeeBps` | Legacy — superseded by `FeeConfig`. Referenced only in comments. |
| `ArbitrationFee` | Legacy — superseded by `FeeConfig.arbitration_fee_bps`. Not accessed. |

---

## `StorageKey` — secondary key enum

Defined in [`storage.rs`](../contracts/escrow/src/storage.rs). The contract uses
this enum for **one** live key today:

| Key | Tier | Value type | TTL bumped on | Description |
|---|---|---|---|---|
| `VendorEscrowIndex(Address)` | Persistent | `Vec<u64>` | read **and** write | List of escrow IDs created by a seller/vendor. Keyed by vendor address. |

The remaining `StorageKey` variants (`AdminAddress`, `FeeConfig`,
`EscrowCounter`, `EscrowData`, `BuyerEscrowIndex`) and their helper functions in
`storage.rs` are **not wired into `lib.rs`** — the equivalent live data is stored
under `DataKey` instead. Only `VendorEscrowIndex` (and its
`read_vendor_escrow_index` / `write_vendor_escrow_index` helpers) is in active
use.

---

## Quick lookup: every live entry

| Entry | Enum | Tier | Value | Key params |
|---|---|---|---|---|
| Admin | `DataKey` | Instance | `Address` | — |
| FeeCollector | `DataKey` | Instance | `Address` | — |
| FeeConfig | `DataKey` | Instance | `FeeConfig` | — |
| Paused | `DataKey` | Instance | `bool` | — |
| EscrowCounter | `DataKey` | Instance | `u64` | — |
| TtlExtensionLedgers | `DataKey` | Instance | `u32` | — |
| TotalArbitrationFees | `DataKey` | Instance | `i128` | token `Address` |
| AccumulatedFees | `DataKey` | Instance | `i128` | token `Address` |
| TotalCreated / Completed / Disputed / Refunded | `DataKey` | Instance | `u64` | — |
| Escrow | `DataKey` | Persistent | `EscrowData` | escrow ID `u64` |
| Dispute | `DataKey` | Persistent | `DisputeData` | escrow ID `u64` |
| BuyerEscrowIndex | `DataKey` | Persistent | `Vec<u64>` | buyer `Address` |
| VendorEscrowIndex | `StorageKey` | Persistent | `Vec<u64>` | vendor `Address` |

---

## Implementation notes

These are factual observations about the current layout that affect how the
storage behaves. They are documented here for completeness; addressing them is
out of scope for this reference.

1. **Two key enums coexist.** Buyer indexes use `DataKey::BuyerEscrowIndex`,
   while vendor indexes use `StorageKey::VendorEscrowIndex` (a different enum in
   `storage.rs`). Most `StorageKey` variants are unused.

2. **`BuyerEscrowIndex` is written without a TTL bump.** Unlike every other
   persistent entry, the buyer index is `set` on persistent storage without a
   following `extend_ttl` (lib.rs `fund_escrow`). A buyer index can therefore
   reach its archival TTL earlier than the escrow records it points to.

3. **Fee bookkeeping is per token.** `AccumulatedFees` and
   `TotalArbitrationFees` are keyed by the token `Address`, so multi-token
   deployments track withdrawable balances independently per asset.
