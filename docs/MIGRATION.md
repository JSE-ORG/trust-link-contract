# Migration Guide

This document previously described a generic, illustrative v1→v2/v2→v3
migration flow (a `migrate_escrow_v1_to_v2` script, event topics versioned as
`["escrow", "v2", "created"]`, a `--upgrade` flag on `stellar contract
deploy`) that does not match how this contract is actually upgraded and never
did — it was a template that was never updated after the real flow landed.

**[docs/UPGRADES.md](UPGRADES.md) is the canonical reference for upgrading
this contract and migrating its storage.** Read it before planning a release
that changes the contract or its storage layout.

## The real flow, in short

An upgrade is always two separate admin-only calls, in this order:

1. `upgrade(caller, new_wasm_hash)` — installs the new WASM. Storage is
   untouched.
2. `migrate(caller)` — brings storage up to the new schema. It is a no-op
   (returns `AlreadyInitialized`) when storage is already current, so the
   sequence is safe to retry.

`scripts/migrate.sh` automates both steps plus a before/after diff of sampled
escrows. Schema state is tracked by two independent version numbers,
`CONTRACT_VERSION` (the code) and `STORAGE_VERSION` / `DataKey::StorageVersion`
(the data) — see [UPGRADES.md § Schema versioning](UPGRADES.md#schema-versioning)
for how they interact and when a release needs a `migrate` step at all.

For everything else — the backward-compatible storage layout strategy, how to
add a migration step, what the migration test suite covers, and rollback —
see [UPGRADES.md](UPGRADES.md) directly rather than a second copy here.
