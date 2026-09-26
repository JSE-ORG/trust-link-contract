# Bugfix Requirements Document

## Introduction

`emit_contract_upgraded` in `contracts/escrow/src/events.rs` publishes a single-symbol topic
`(Symbol::new(env, "contract_upgraded"),)` while every other event in the file uses the
two-symbol `(symbol_short!("Namespace"), symbol_short!("Action"), â€¦)` pattern (e.g.
`("Contract", "Paused")`, `("Dispute", "Resolved")`).  This inconsistency forces indexers and
event consumers to special-case the one upgrade event, increasing the risk of missed events and
maintenance burden.  The fix aligns `emit_contract_upgraded` with the established topic
convention and documents the event in `docs/events.md`.

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN `upgrade` is called on the contract THEN the system emits an event whose topic tuple
    contains a single `Symbol::new(env, "contract_upgraded")` string, deviating from the
    two-symbol topic pattern used by all other events.

1.2 WHEN an indexer filters events by the first topic symbol THEN the system produces no match
    for `symbol_short!("Contract")` on the upgrade event, because its first (and only) topic
    symbol is the full string `"contract_upgraded"` rather than `"Contract"`.

1.3 WHEN `docs/events.md` is consulted THEN the system provides no entry for the
    `contract_upgraded` event, leaving indexer developers without authoritative documentation
    for this event.

### Expected Behavior (Correct)

2.1 WHEN `upgrade` is called on the contract THEN the system SHALL emit an event whose topic
    tuple is `(symbol_short!("Contract"), symbol_short!("Upgraded"))`, matching the two-symbol
    namespace/action pattern used by all other events.

2.2 WHEN an indexer filters events by first topic `symbol_short!("Contract")` and second topic
    `symbol_short!("Upgraded")` THEN the system SHALL surface the upgrade event without
    requiring special-case logic.

2.3 WHEN `docs/events.md` is consulted THEN the system SHALL contain a `contract_upgraded`
    entry under "Contract Initialization & Config" with topics `["Contract", "Upgraded", admin]`
    and payload `ContractUpgradedEvent { admin, new_wasm_hash, timestamp }`, formatted
    consistently with all other entries in that document.

### Unchanged Behavior (Regression Prevention)

3.1 WHEN any event other than `contract_upgraded` is emitted THEN the system SHALL CONTINUE TO
    use its existing topic tuple unchanged (e.g. `("Contract", "Paused", admin)`,
    `("Dispute", "Raised", buyer)`).

3.2 WHEN `upgrade` is called THEN the system SHALL CONTINUE TO invoke
    `env.deployer().update_current_contract_wasm(new_wasm_hash)` and include `admin`,
    `new_wasm_hash`, and `timestamp` in the `ContractUpgradedEvent` data payload.

3.3 WHEN existing snapshot or integration tests that do not reference the upgrade event topic
    are run THEN the system SHALL CONTINUE TO pass without modification.

3.4 WHEN `test_upgrade` and `test_upgrade_unauthorized` tests in `test_admin.rs` are run
    THEN the system SHALL CONTINUE TO pass, verifying that the upgrade call succeeds for an
    authorized admin and fails with `NotAuthorized` for an unauthorized caller.
