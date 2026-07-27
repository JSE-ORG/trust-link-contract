# Contract Upgrades and Storage Migration

`Escrow::upgrade` swaps the contract WASM in place. It does **not** touch
storage: the contract id, its instance entries and every persistent
`DataKey::Escrow(id)` survive the upgrade byte-for-byte, and the new code reads
them back exactly as the old code wrote them.

That is the whole risk. Nothing is lost during an upgrade; what breaks is a new
build reading old bytes under a schema it no longer matches. This document
describes how to avoid that, and `scripts/migrate.sh` automates the procedure.

## The two-step release

An upgrade is always two transactions, in this order:

1. `upgrade(caller, new_wasm_hash)` — install the new code.
2. `migrate(caller)` — bring storage up to the new schema.

Both are admin-only. Step 2 is a no-op for releases that did not change the
storage layout: it returns `AlreadyInitialized` when storage is already current,
which makes the whole sequence safe to retry.

```bash
make build-wasm

./scripts/migrate.sh \
  --contract CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX \
  --admin mainnet-deployer \
  --network public \
  --sample 1,2,3
```

The script uploads the WASM, invokes `upgrade`, invokes `migrate`, and then
re-reads the escrows passed to `--sample`, diffing them against a snapshot taken
before the upgrade. A non-empty diff fails the run. Use `--dry-run` to rehearse
without submitting anything.

## Schema versioning

Two independent numbers are tracked:

| Value | Where | Meaning |
|---|---|---|
| `CONTRACT_VERSION` | compiled into the WASM, read via `get_version` | semantic version of the *code* |
| `STORAGE_VERSION` | `DataKey::StorageVersion` in instance storage, read via `get_storage_version` | schema version of the *data* |

`get_storage_version` returns `0` for contracts deployed before versioning
existed. `initialize` stamps the current version, so fresh deployments never
need a migration.

Deciding whether a release needs a migration:

```
get_storage_version() < STORAGE_VERSION   →  migrate() must run
get_storage_version() == STORAGE_VERSION  →  migrate() is a no-op
```

## Backward-compatible storage layout strategy

The cheapest migration is the one you do not have to write. These rules keep
most releases in the no-op case.

### 1. `DataKey` variants are addressed by name, not by index

`DataKey` is a `#[contracttype]` enum, so each variant is encoded by its symbol
name. **Adding a variant anywhere in the enum is safe**; renaming or removing
one orphans every entry stored under the old name. Treat variant names as
public ABI.

### 2. Extend structs only at the end, and only with `Option`

Soroban encodes `#[contracttype]` structs as maps keyed by field name, so an
absent field is a decode error rather than a default. A field appended as
`Option<T>` still fails to decode against an old entry, which is why a struct
change always requires either a migration step or a new key.

Prefer, in order:

1. **A new `DataKey` variant** holding the new data, read with `.unwrap_or(...)`
   when absent. No migration needed, no existing entry rewritten. This is how
   `TtlExtensionLedgers`, `MinAmount` and `ResolverStrict` were added.
2. **A lazy upgrade-on-read** helper that tries the new layout and falls back to
   the old one, rewriting the entry on the next mutation. Suits high-cardinality
   data like `DataKey::Escrow(id)`, where rewriting every entry in one
   transaction is not affordable.
3. **An eager rewrite in `migrate`** — only for instance-storage singletons,
   which are few and fixed in number.

### 3. Never renumber `ContractError`

The numeric discriminants are the public ABI, as noted in `errors.rs`, and
clients (including `bindings/src/errors.ts`, checked by
`scripts/check-error-codes.mjs`) map them by value.

### 4. Never reuse an escrow id or reset `EscrowCounter`

Indexes (`BuyerEscrowIndex`, `VendorEscrowIndex`) hold raw ids. Reusing one
silently rebinds historical references.

### 5. Persistent entries have TTLs

An escrow whose TTL expired before the upgrade is not recoverable by migrating.
Archived entries must be restored before `migrate` runs; `migrate` extends the
instance TTL but cannot resurrect expired persistent data.

## Adding a migration step

When a release does change the layout:

1. Bump `STORAGE_VERSION` in `contracts/escrow/src/lib.rs`.
2. Append a step to `Escrow::migrate`, guarded on the version being migrated
   *from*. Never edit an existing step — a contract may be several versions
   behind and has to walk through each one:

   ```rust
   if from < 2 {
       // v1 -> v2: <what changed and why the old bytes are still readable>
   }
   ```

3. Add a test to `contracts/escrow/src/test_upgrade_migration.rs` that writes
   the *old* representation, runs `migrate`, and asserts the data reads back
   correctly under the new schema.
4. If the step rewrites per-escrow entries, make it resumable: process a bounded
   batch per call and record progress, so the migration cannot exceed the
   transaction resource budget.

## What the tests cover

`contracts/escrow/src/test_upgrade_migration.rs` verifies that:

- a fresh deployment is already at `STORAGE_VERSION` and `migrate` refuses to
  run;
- `migrate` on an unversioned deployment leaves `EscrowData` byte-identical;
- a migrated escrow continues through its lifecycle normally;
- `migrate` is idempotent — a retried call is rejected, not applied twice;
- a non-admin caller is rejected and storage is left unchanged.

The "old deployment" is reproduced by removing `DataKey::StorageVersion` from
instance storage, which is exactly what a pre-versioning build's storage looks
like to the new code. Swapping a real WASM artifact needs two compiled binaries
and belongs to the deployment rehearsal (`scripts/migrate.sh --dry-run` against
a devnet from `scripts/start-testnet.sh`), not to the unit test suite.

## Rollback

There is no automatic downgrade. To roll back, upgrade to the previous WASM
hash — but only if the migration did not rewrite storage, since the old build
cannot read a newer schema. Where a release contains a destructive migration,
rehearse the rollback on the local devnet first:

```bash
./scripts/start-testnet.sh          # devnet with seeded escrows
./scripts/migrate.sh --contract <id> --admin tl_local_admin --network local --sample 1,2,3
```
