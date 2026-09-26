# contract-upgraded-event-topic Bugfix Design

## Overview

`emit_contract_upgraded` in `contracts/escrow/src/events.rs` emits a single-symbol topic
`(Symbol::new(env, "contract_upgraded"),)`.  Every other event in the file uses the
two-symbol `(symbol_short!("Namespace"), symbol_short!("Action"), â€¦)` pattern.  This
inconsistency forces indexers to special-case the upgrade event and makes the first-topic
filter `symbol_short!("Contract")` miss the event entirely.

The fix is minimal and targeted:

1. Change the `publish` call in `emit_contract_upgraded` from
   `(Symbol::new(env, "contract_upgraded"),)` to
   `(symbol_short!("Contract"), symbol_short!("Upgraded"))` and update its doc comment.
2. Add a snapshot / topic-assertion test that asserts the new two-symbol topic pattern.
3. Add a `contract_upgraded` entry to `docs/events.md` under "Contract Initialization & Config".

No other logic changes are required; the payload struct (`ContractUpgradedEvent`) and the
calling code in `lib.rs` are unchanged.

---

## Glossary

- **Bug_Condition (C)**: `emit_contract_upgraded` is called and the emitted event's first
  topic is the single long symbol `"contract_upgraded"` rather than `symbol_short!("Contract")`.
- **Property (P)**: After the fix, calling `upgrade` SHALL emit an event whose topic tuple is
  `(symbol_short!("Contract"), symbol_short!("Upgraded"))` â€” a two-element tuple matching the
  namespace/action pattern used by all other events.
- **Preservation**: All other event emitters and all existing tests that do not reference the
  upgrade-event topic MUST continue to behave identically after the change.
- **`emit_contract_upgraded`**: The function in `contracts/escrow/src/events.rs` that publishes
  a `ContractUpgradedEvent` when the admin upgrades the contract WASM.
- **`symbol_short!`**: Soroban SDK macro that creates a `Symbol` from a short (â‰¤ 9 character)
  static string literal, compile-time checked and cheaper than `Symbol::new`.
- **`Symbol::new`**: Runtime `Symbol` constructor that accepts arbitrary-length strings; used
  by the buggy code to encode `"contract_upgraded"` (18 chars) as a single topic symbol.

---

## Bug Details

### Bug Condition

The bug manifests whenever `upgrade` is called on the deployed contract.  The
`emit_contract_upgraded` helper publishes a topic tuple containing a single
`Symbol::new(env, "contract_upgraded")` instead of the two `symbol_short!` tokens that
all other events use.

**Formal Specification:**

```
FUNCTION isBugCondition(event)
  INPUT:  event â€” a Soroban contract event emitted by the escrow contract
  OUTPUT: boolean

  RETURN event.emitter == "emit_contract_upgraded"
         AND event.topics.len() == 1
         AND event.topics[0] == Symbol::new(env, "contract_upgraded")
END FUNCTION
```

### Examples

- **Upgrade called by admin**: `upgrade(admin, [1u8; 32])` emits a single-topic event
  `("contract_upgraded",)`.  An indexer filtering `first_topic == "Contract"` gets zero
  results.  **Expected**: `("Contract", "Upgraded")`.

- **Indexer namespace scan**: A consumer fetching all `"Contract"` namespace events
  (paused, unpaused, initialized) never sees the upgrade event.
  **Expected**: upgrade appears alongside the other `"Contract"` events.

- **Edge case â€” no other event changed**: Calling `fund_escrow` after the fix still emits
  `("Escrow", "Funded", buyer)` unchanged.

---

## Expected Behavior

### Preservation Requirements

**Unchanged Behaviors:**

- All other event emitters (`emit_fee_updated`, `emit_admin_rotated`, `emit_contract_paused`,
  `emit_contract_unpaused`, `emit_escrow_created`, etc.) MUST continue to publish the exact
  same topic tuples they publish today.
- `upgrade` MUST still call `env.deployer().update_current_contract_wasm(new_wasm_hash)`.
- The `ContractUpgradedEvent` data payload (`admin`, `new_wasm_hash`, `timestamp`) MUST remain
  unchanged.
- `test_upgrade` and `test_upgrade_unauthorized` in `test_admin.rs` MUST continue to pass
  after the change.
- All other existing tests MUST continue to pass without modification.

**Scope:**

All inputs that do NOT involve the `upgrade` entry-point should be completely unaffected.
This includes:
- Any escrow lifecycle operation (`create_escrow`, `fund_escrow`, `mark_shipped`, etc.)
- Any fee-management operation (`set_protocol_fee`, `set_arbitration_fee`, etc.)
- Admin rotation, pausing, unpausing, and token allowlist management

---

## Hypothesized Root Cause

The upgrade functionality was added after the two-symbol topic convention was already
established for other events.  The author used `Symbol::new(env, "contract_upgraded")` â€” a
pattern that avoids the 9-character limit of `symbol_short!` â€” rather than decomposing the
event name into a namespace/action pair of short symbols.  The result is a topic tuple of
length 1 instead of length 2, which is inconsistent with every other event in the file.

Specifically:

1. **Wrong macro / constructor used**: `Symbol::new(env, "contract_upgraded")` encodes the
   entire compound name as one symbol.  The correct approach is two separate
   `symbol_short!("Contract")` and `symbol_short!("Upgraded")` tokens.

2. **No existing test for topic shape**: `test_upgrade` in `test_admin.rs` verifies the call
   succeeds and the WASM hash check is skipped in test mode, but it does not assert the emitted
   event's topic tuple, so the inconsistency went undetected.

3. **Missing documentation**: The event was never added to `docs/events.md`, removing the
   cross-check that would have revealed the mismatch against the documented pattern.

---

## Correctness Properties

Property 1: Bug Condition â€” Upgrade Event Uses Two-Symbol Topic

_For any_ call to `upgrade(admin, new_wasm_hash)` where `admin` is the authorized admin and
`new_wasm_hash` is any valid `BytesN<32>`, the fixed `emit_contract_upgraded` SHALL emit a
contract event whose topic tuple is exactly
`(symbol_short!("Contract"), symbol_short!("Upgraded"))` â€” a two-element tuple where the first
element equals `symbol_short!("Contract")` and the second equals `symbol_short!("Upgraded")`.

**Validates: Requirements 2.1, 2.2**

Property 2: Preservation â€” All Other Events Unchanged

_For any_ contract entry-point call that does NOT invoke `upgrade` (i.e., `isBugCondition`
returns false for every event emitted by that call), the fixed code SHALL emit the same topic
tuples as the original code, preserving all existing event shapes for fee updates, admin
rotation, escrow lifecycle events, dispute events, and every other emitter.

**Validates: Requirements 3.1, 3.2, 3.3, 3.4**

---

## Fix Implementation

### Changes Required

**File 1:** `contracts/escrow/src/events.rs`

**Function:** `emit_contract_upgraded`

**Specific Changes:**

1. **Update doc comment** â€” change the leading `///` comment from:
   ```rust
   /// Topic: `("contract_upgraded",)`, data: `ContractUpgradedEvent`.
   ```
   to:
   ```rust
   /// Topic: `(symbol_short!("Contract"), symbol_short!("Upgraded"),)`, data: `ContractUpgradedEvent`.
   ```

2. **Replace topic tuple** â€” change the `env.events().publish(â€¦)` call from:
   ```rust
   env.events().publish(
       (Symbol::new(env, "contract_upgraded"),),
       ContractUpgradedEvent { â€¦ },
   );
   ```
   to:
   ```rust
   env.events().publish(
       (symbol_short!("Contract"), symbol_short!("Upgraded"),),
       ContractUpgradedEvent { â€¦ },
   );
   ```
   `Symbol` is no longer needed in this function; if it is unused elsewhere in the file the
   import can be dropped, but that is a cosmetic cleanup, not a correctness requirement.

---

**File 2:** `contracts/escrow/src/test_admin.rs` (or a new dedicated file)

**Change:** Add a test that asserts the emitted event's topic tuple after calling `upgrade`.

**Specific Changes:**

1. Add `test_upgrade_event_topic` â€” call `client.upgrade(&admin, &new_wasm_hash)`, then
   iterate `env.events().all()` filtered to the contract and assert that exactly one event has
   a two-element topic tuple `[symbol_short!("Contract"), symbol_short!("Upgraded")]`.

---

**File 3:** `docs/events.md`

**Change:** Add `contract_upgraded` entry under "Contract Initialization & Config".

**Specific Changes:**

1. Append the following bullet after the `contract_unpaused` entry (or at a logical position
   in the "Contract Initialization & Config" section):
   ```markdown
   - **contract_upgraded**:
     - Topics: `["Contract", "Upgraded", admin]`
     - Payload: `ContractUpgradedEvent` `{ admin, new_wasm_hash, timestamp }`
   ```

---

## Testing Strategy

### Validation Approach

Two-phase: first run exploratory tests on the **unfixed** code to confirm the bug manifests
as hypothesized (single-symbol topic), then apply the fix and verify both the corrected topic
shape and the preservation of all other events.

---

### Exploratory Bug Condition Checking

**Goal:** Surface a counterexample proving the single-symbol topic exists on unfixed code.
Confirm the root cause (wrong constructor, not a logic error elsewhere).

**Test Plan:** Write a test that calls `upgrade` and directly inspects the emitted event's
topic tuple.  Run it on the **unfixed** code and observe the failure.

**Test Cases:**

1. **Topic length check** (will fail on unfixed code): Assert
   `event.topics.len() == 2` â€” on unfixed code this returns `1`, proving the bug.
2. **First-topic symbol check** (will fail on unfixed code): Assert
   `event.topics[0] == symbol_short!("Contract")` â€” on unfixed code the symbol is
   `"contract_upgraded"`, not `"Contract"`.
3. **Second-topic symbol check** (will fail on unfixed code): Assert
   `event.topics[1] == symbol_short!("Upgraded")` â€” on unfixed code there is no second topic.

**Expected Counterexample:**

```
thread 'test_upgrade_event_topic' panicked at 'assertion failed: topics.len() == 2
  left: 1, right: 2'
```

Possible causes confirmed: single `Symbol::new` rather than two `symbol_short!` tokens.

---

### Fix Checking

**Goal:** Verify that after the fix, every call to `upgrade` emits the correct two-symbol topic.

**Pseudocode:**

```
FOR ALL new_wasm_hash : BytesN<32> WHERE isBugCondition(emitted_event) WAS true DO
  call upgrade(admin, new_wasm_hash) on FIXED code
  event := find ContractUpgradedEvent in env.events()
  ASSERT event.topics[0] == symbol_short!("Contract")
  ASSERT event.topics[1] == symbol_short!("Upgraded")
  ASSERT event.data.admin    == admin
  ASSERT event.data.new_wasm_hash == new_wasm_hash
  ASSERT event.data.timestamp > 0
END FOR
```

---

### Preservation Checking

**Goal:** Verify that the fix does not alter the topic tuples of any other event.

**Pseudocode:**

```
FOR ALL entry_point_call WHERE NOT isBugCondition(events emitted) DO
  ASSERT topics_fixed(call) == topics_original(call)
END FOR
```

**Testing Approach:** Property-based testing is recommended for preservation because it
generates many random combinations of `(escrow_id, seller, buyer, â€¦)` addresses and amounts,
catching any accidental scope leak in the topic-tuple change.  The PBT library can generate
random `Address` values and verify that calling `emit_fee_updated`, `emit_escrow_created`, or
any other emitter still produces the same topic tuple it produces today.

**Test Cases:**

1. **Fee event preservation**: Generate random `(old_fee_bps, new_fee_bps)` pairs; assert
   `emit_fee_updated` still emits `("Fee", "Updated")`.
2. **Escrow lifecycle preservation**: Generate random escrow parameters; assert
   `emit_escrow_created` still emits `("Escrow", "Created", seller)`.
3. **Admin event preservation**: Assert `emit_admin_rotated` still emits
   `("Admin", "Rotated")` with random address pairs.
4. **Pause / unpause preservation**: Assert `emit_contract_paused` and
   `emit_contract_unpaused` still emit their three-element topic tuples including `admin`.

---

### Unit Tests

- `test_upgrade_event_topic`: Assert the emitted topic is
  `(symbol_short!("Contract"), symbol_short!("Upgraded"))` after the fix.
- `test_upgrade_event_payload`: Assert `ContractUpgradedEvent` fields (`admin`,
  `new_wasm_hash`, `timestamp`) are correct.
- `test_upgrade` and `test_upgrade_unauthorized` in `test_admin.rs`: Both must continue to
  pass unchanged.

### Property-Based Tests

- Generate random `BytesN<32>` WASM hashes and assert Property 1 holds for each.
- Generate random event-emitter calls (fee updates, escrow operations) and assert their topic
  tuples are unaffected by the change (Property 2).
- Across many randomly-generated `Address` values, assert `emit_contract_upgraded` never
  produces a single-symbol topic.

### Integration Tests

- Call `upgrade` in a full contract setup (initialized, funded escrow present) and verify the
  event log contains exactly one `("Contract", "Upgraded")` event with the correct payload.
- Verify that after an upgrade the contract still processes a subsequent `fund_escrow` and
  emits `("Escrow", "Funded", buyer)` correctly (no cross-contamination from the topic change).
