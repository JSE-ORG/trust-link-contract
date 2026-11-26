# Storage Layout

This document is the authoritative reference for **every** storage entry the
TrustLink escrow contract reads or writes: its key, value type, storage tier
(instance vs. persistent), and TTL strategy. It lets developers, auditors, and
indexer authors determine the exact on-chain storage layout without reading the
source.

Source of truth:
- Key enum: [`contracts/escrow/src/types.rs`](../contracts/escrow/src/types.rs) (`DataKey`).
  There is a single key enum today — the `StorageKey` enum this doc used to
  describe has been removed from `storage.rs`.
- Access + TTL logic: [`contracts/escrow/src/storage.rs`](../contracts/escrow/src/storage.rs)
  and [`contracts/escrow/src/internal.rs`](../contracts/escrow/src/internal.rs).

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
- **Bump pattern** — every bump uses
  `extend_ttl(threshold = ext / TTL_THRESHOLD_DIVISOR, extend_to = ext)`: if
  fewer than `ext / TTL_THRESHOLD_DIVISOR` ledgers remain, top the TTL back up
  to `ext`.
- **Core per-escrow records** (`Escrow`, `Dispute`) are bumped on **every
  read and write**: `internal::save_escrow`/`load_escrow`
  ([internal.rs:552-591](../contracts/escrow/src/internal.rs#L552-L591)) and
  `internal::save_dispute`/`load_dispute`
  ([internal.rs:670-691](../contracts/escrow/src/internal.rs#L670-L691)).
- **Buyer and vendor escrow indexes are paged**, not single `Vec` entries
  (`BuyerEscrow(Address, u32)` / `BuyerEscrowCount(Address)` and the vendor
  equivalents). Every page touched by an append, a full read, or a full
  rewrite — plus the count key alongside it — gets its TTL bumped via the
  shared `extend_ttl_for_key` helper
  ([storage.rs:173-179](../contracts/escrow/src/storage.rs#L173-L179)), called
  from `append_buyer_escrow_index` / `append_vendor_escrow_index`
  ([storage.rs:494-510](../contracts/escrow/src/storage.rs#L494-L510) and
  [storage.rs:342-358](../contracts/escrow/src/storage.rs#L342-L358)),
  `read_buyer_escrow_index` / `read_vendor_escrow_index`
  ([storage.rs:518-536](../contracts/escrow/src/storage.rs#L518-L536) and
  [storage.rs:366-384](../contracts/escrow/src/storage.rs#L366-L384)), and
  `write_buyer_escrow_index` / `write_vendor_escrow_index` via the shared
  `write_pages` helper
  ([storage.rs:600-621](../contracts/escrow/src/storage.rs#L600-L621)).
  `fund_escrow` and `fund_basket_escrow` both call `append_buyer_escrow_index`
  directly ([instructions.rs:367](../contracts/escrow/src/instructions.rs#L367)
  and [instructions.rs:1605](../contracts/escrow/src/instructions.rs#L1605))
  with no separate inline `extend_ttl` call at either site — none is needed,
  since the helper already extends both the page and the count key on every
  call. **There is no remaining case where a `BuyerEscrowIndex`-family write
  skips TTL extension**, including the `fund_escrow` path.
- The pre-paging flat `DataKey::BuyerEscrowIndex(Address)` /
  `DataKey::VendorEscrowIndex(Address)` vectors are legacy: they are never
  written to anymore. They're read (and TTL-bumped) by `read_legacy_buyer_index`
  / `read_legacy_vendor_index`
  ([storage.rs:553-564](../contracts/escrow/src/storage.rs#L553-L564) and
  [storage.rs:459-470](../contracts/escrow/src/storage.rs#L459-L470)), and
  one-time migrated into the paged layout (then `remove()`d) by
  `migrate_legacy_buyer_index` / `migrate_legacy_vendor_index`
  ([storage.rs:568-583](../contracts/escrow/src/storage.rs#L568-L583) and
  [storage.rs:474-489](../contracts/escrow/src/storage.rs#L474-L489)).
- **Sharded lifecycle counters** (`ShardedCounter`) are bumped on every
  increment by the same `ext / TTL_THRESHOLD_DIVISOR` → `ext` rule; see
  [Lifecycle counter TTL and overflow](#lifecycle-counter-ttl-and-overflow).
- **Instance storage** is extended explicitly only at specific call sites, not
  on every mutation: `migrate` ([admin.rs:391](../contracts/escrow/src/admin.rs#L391)),
  the read-only `get_public_config` query, to keep singleton config alive under
  frequent read-only traffic
  ([queries.rs:238](../contracts/escrow/src/queries.rs#L238)), and the
  timelock-proposal helpers
  ([storage.rs:623-636](../contracts/escrow/src/storage.rs#L623-L636)).
  `create_escrow` and the admin setters (`set_fee`, `set_protocol_fee`, etc.)
  write the consolidated `GlobalConfig` entry via `update_global_config`/
  `write_global_config` ([storage.rs:61-70](../contracts/escrow/src/storage.rs#L61-L70))
  without an accompanying `extend_instance_ttl` call.

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
| `StorageVersion` | `u32` | Schema version of the stored data. Set by `initialize` and advanced by `migrate`; reads as `0` on deployments predating versioning. See [UPGRADES.md](UPGRADES.md). |
| `TotalArbitrationFees(Address)` | `i128` | Per-token running total of arbitration fees collected. Keyed by token address. |
| `TotalCreated` | `u64` | Lifetime count of escrows created (stats). |
| `TotalCompleted` | `u64` | Lifetime count of escrows completed (stats). |
| `TotalDisputed` | `u64` | Lifetime count of escrows disputed (stats). |
| `TotalRefunded` | `u64` | Lifetime count of escrows refunded (stats). |

All instance entries share the contract instance TTL (bumped during
`create_escrow`). Counters default to `0` when unset.

#### Lifecycle counter TTL and overflow

Lifecycle counters (created/completed/disputed/refunded) are incremented by
`increment_sharded_counter` into one of `COUNTER_SHARDS` (16) **persistent**
`ShardedCounter(kind, bucket)` entries, chosen by `escrow_id % 16`. Each
increment uses `checked_add(1)`, writes the value, then extends that entry's
TTL with `extend_ttl(ext / TTL_THRESHOLD_DIVISOR, ext)`, where `ext` is the
configured TTL extension (`TtlExtensionLedgers`, default `120_960` ledgers).
If a shard is already `u64::MAX` the increment returns
`ContractError::ArithmeticError` (12) and the stored value is left unchanged.
Reads (`get_stats`) sum the shards plus any legacy singleton value without
extending TTL.

### Persistent storage (per-record)

| Key | Value type | TTL bumped on | Description |
|---|---|---|---|
| `Escrow(u64)` | `EscrowData` | read **and** write | The full escrow record, keyed by escrow ID. See [`types.rs`](../contracts/escrow/src/types.rs) for `EscrowData`. TTL logic: [internal.rs:552-591](../contracts/escrow/src/internal.rs#L552-L591). |
| `Dispute(u64)` | `DisputeData` | read **and** write | The dispute record for an escrow, keyed by escrow ID. TTL logic: [internal.rs:670-691](../contracts/escrow/src/internal.rs#L670-L691). |
| `BuyerEscrow(Address, u32)` | `Vec<u64>` (one page) | read **and** write | A bounded page of a buyer's funded-escrow index, keyed by `(buyer, page_index)`. TTL logic: [storage.rs:494-551](../contracts/escrow/src/storage.rs#L494-L551). |
| `BuyerEscrowCount(Address)` | `u32` | read **and** write | Total number of escrow IDs indexed for a buyer; drives which page an append lands on. Bumped alongside every page touch, same functions as above. |
| `VendorEscrow(Address, u32)` | `Vec<u64>` (one page) | read **and** write | A bounded page of a vendor's created-escrow index, keyed by `(vendor, page_index)`. TTL logic: [storage.rs:342-457](../contracts/escrow/src/storage.rs#L342-L457). |
| `VendorEscrowCount(Address)` | `u32` | read **and** write | Total number of escrow IDs indexed for a vendor. Bumped alongside every page touch, same functions as above. |
| `BuyerEscrowIndex(Address)` *(legacy)* | `Vec<u64>` | read only | Pre-paging flat buyer index. Never written anymore; read once per address by `read_legacy_buyer_index` and migrated into the paged layout (then removed) by `migrate_legacy_buyer_index`. [storage.rs:553-583](../contracts/escrow/src/storage.rs#L553-L583). |
| `VendorEscrowIndex(Address)` *(legacy)* | `Vec<u64>` | read only | Pre-paging flat vendor index. Same migrate-then-remove pattern via `read_legacy_vendor_index` / `migrate_legacy_vendor_index`. [storage.rs:459-489](../contracts/escrow/src/storage.rs#L459-L489). |

### Declared but unused (legacy)

These variants exist in the `DataKey` enum but are **never read or written** by
the current contract. They are retained for enum/ABI stability and should be
treated as reserved:

| Key | Status |
|---|---|
| `DefaultFeeBps` | Legacy — superseded by `FeeConfig`. Referenced only in comments. |
| `ArbitrationFee` | Legacy — superseded by `FeeConfig.arbitration_fee_bps`. Not accessed. |

---

## `StorageKey` — removed

This doc previously described a second, parallel key enum (`StorageKey` in
`storage.rs`) that carried `VendorEscrowIndex` separately from `DataKey`. That
enum has been removed; `storage.rs` now has no `StorageKey` type at all. There
is a single key enum, `DataKey` ([types.rs](../contracts/escrow/src/types.rs)),
and both the buyer and vendor escrow indexes live under it as the paged
`BuyerEscrow`/`BuyerEscrowCount` and `VendorEscrow`/`VendorEscrowCount` keys
described in the table above.

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
| StorageVersion | `DataKey` | Instance | `u32` | — |
| TotalArbitrationFees | `DataKey` | Instance | `i128` | token `Address` |
| TotalCreated / Completed / Disputed / Refunded | `DataKey` | Instance | `u64` | — |
| Escrow | `DataKey` | Persistent | `EscrowData` | escrow ID `u64` |
| Dispute | `DataKey` | Persistent | `DisputeData` | escrow ID `u64` |
| BuyerEscrow / BuyerEscrowCount | `DataKey` | Persistent | `Vec<u64>` page / `u32` | buyer `Address` (+ page index) |
| VendorEscrow / VendorEscrowCount | `DataKey` | Persistent | `Vec<u64>` page / `u32` | vendor `Address` (+ page index) |
| BuyerEscrowIndex *(legacy, read-only)* | `DataKey` | Persistent | `Vec<u64>` | buyer `Address` |
| VendorEscrowIndex *(legacy, read-only)* | `DataKey` | Persistent | `Vec<u64>` | vendor `Address` |

---

## Implementation notes

These are factual observations about the current layout that affect how the
storage behaves. They are documented here for completeness; addressing them is
out of scope for this reference.

1. **Buyer and vendor indexes share one paged layout.** Both are stored under
   `DataKey` (`BuyerEscrow`/`BuyerEscrowCount` and
   `VendorEscrow`/`VendorEscrowCount`); the separate `StorageKey` enum that
   used to carry `VendorEscrowIndex` on its own has been removed from
   `storage.rs`.

2. **Fee bookkeeping is per token.** `TotalArbitrationFees` is keyed by the
   token `Address`, so multi-token deployments track arbitration fee totals
   independently per asset. The protocol fee itself is not tracked in
   storage — it is forwarded to the fee collector directly at payout time.
