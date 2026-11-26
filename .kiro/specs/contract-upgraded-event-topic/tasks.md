# Implementation Plan

## Overview

Fix `emit_contract_upgraded` in `contracts/escrow/src/events.rs` to emit a two-symbol topic
`(symbol_short!("Contract"), symbol_short!("Upgraded"))` instead of the single-symbol
`(Symbol::new(env, "contract_upgraded"),)`, add tests asserting the corrected topic and payload,
and document the event in `docs/events.md`.

## Task Dependency Graph

```json
{
  "waves": [
    { "wave": 1, "tasks": ["1", "2"] },
    { "wave": 2, "tasks": ["3"] },
    { "wave": 3, "tasks": ["4"] },
    { "wave": 4, "tasks": ["5"] }
  ]
}
```

## Tasks

- [ ] 1. Write bug condition exploration test
  - **Property 1: Bug Condition** - Upgrade Event Uses Single-Symbol Topic
  - **CRITICAL**: This test MUST FAIL on unfixed code â€” failure confirms the bug exists
  - **DO NOT attempt to fix the test or the code when it fails**
  - **NOTE**: This test encodes the expected behavior â€” it will validate the fix when it passes after implementation
  - **GOAL**: Surface counterexamples that demonstrate the single-symbol topic bug
  - **Scoped PBT Approach**: For this deterministic bug, scope the property to the concrete failing case: call `upgrade(admin, [1u8; 32])` and assert the emitted event topic
  - Add `test_upgrade_event_topic` in `contracts/escrow/src/test_admin.rs`
  - Call `client.upgrade(&admin, &new_wasm_hash)` then inspect `env.events().all()`
  - Assert `event.topics.len() == 2` â€” on unfixed code this returns `1`, proving the bug
  - Assert `event.topics[0] == symbol_short!("Contract")` â€” on unfixed code the value is `"contract_upgraded"`, not `"Contract"`
  - Assert `event.topics[1] == symbol_short!("Upgraded")` â€” on unfixed code there is no second topic
  - Run test on **UNFIXED** code
  - **EXPECTED OUTCOME**: Test FAILS (this is correct â€” it proves the bug exists)
  - Document counterexample found: `upgrade(admin, [1u8; 32])` emits `("contract_upgraded",)` â€” a single-topic event where `topics[0] == "contract_upgraded"` and `topics.len() == 1`
  - Mark task complete when test is written, run, and failure is documented
  - _Requirements: 1.1_

- [ ] 2. Write preservation property tests (BEFORE implementing fix)
  - **Property 2: Preservation** - All Other Events Emit Unchanged Topic Tuples
  - **IMPORTANT**: Follow observation-first methodology â€” observe behavior on UNFIXED code first
  - Add `test_upgrade_event_payload` in `contracts/escrow/src/test_admin.rs` and property-based preservation tests
  - **Observe on unfixed code**: `emit_fee_updated`, `emit_escrow_created`, `emit_contract_paused`, `emit_admin_rotated`, etc. all emit their existing two-symbol topic tuples unchanged
  - Write property-based test that calls non-upgrade entry-points (e.g., `set_protocol_fee`, `create_escrow`) with varied inputs and asserts their topic tuples are unaffected:
    - For all `(old_fee_bps, new_fee_bps)` pairs: `emit_fee_updated` topics are `("Fee", "Updated")`
    - For all valid escrow parameters: `emit_escrow_created` first topic is `symbol_short!("Escrow")`
    - For any admin address: `emit_contract_paused` topics are `("Contract", "Paused", admin)`
  - Also write `test_upgrade_event_payload`: call `client.upgrade(&admin, &new_wasm_hash)`, find the `ContractUpgradedEvent` data, and assert `event.data.admin == admin`, `event.data.new_wasm_hash == new_wasm_hash`, `event.data.timestamp > 0` â€” these payload assertions should pass on BOTH unfixed and fixed code
  - Run tests on **UNFIXED** code
  - **EXPECTED OUTCOME**: Preservation tests PASS (confirms baseline behavior to preserve); payload test also passes because the data fields are not affected by the bug
  - Mark task complete when tests are written, run, and passing on unfixed code
  - _Requirements: 3.1, 3.2, 3.3, 3.4_

- [ ] 3. Fix emit_contract_upgraded topic in events.rs

  - [ ] 3.1 Implement the fix in `contracts/escrow/src/events.rs`
    - Update doc comment on `emit_contract_upgraded` from `/// Topic: \`("contract_upgraded",)\`` to `/// Topic: \`(symbol_short!("Contract"), symbol_short!("Upgraded"),)\``
    - Replace topic tuple in the `env.events().publish(â€¦)` call: change `(Symbol::new(env, "contract_upgraded"),)` to `(symbol_short!("Contract"), symbol_short!("Upgraded"),)`
    - Remove the `Symbol` import from `emit_contract_upgraded` if it is no longer used elsewhere in the file (cosmetic cleanup; not required for correctness)
    - The `ContractUpgradedEvent` struct, its fields (`admin`, `new_wasm_hash`, `timestamp`), and the calling code in `lib.rs` remain unchanged
    - _Bug_Condition: `emit_contract_upgraded` is called and `event.topics[0] == Symbol::new(env, "contract_upgraded")` (single topic)_
    - _Expected_Behavior: after fix, `event.topics == (symbol_short!("Contract"), symbol_short!("Upgraded"))` â€” two-element tuple matching namespace/action pattern_
    - _Preservation: all other event emitters (`emit_fee_updated`, `emit_admin_rotated`, `emit_contract_paused`, etc.) and their callers are untouched_
    - _Requirements: 2.1, 2.2, 3.1, 3.2_

  - [ ] 3.2 Verify bug condition exploration test now passes
    - **Property 1: Expected Behavior** - Upgrade Event Uses Two-Symbol Topic
    - **IMPORTANT**: Re-run the SAME `test_upgrade_event_topic` test from task 1 â€” do NOT write a new test
    - The test from task 1 encodes the expected behavior: `topics.len() == 2`, `topics[0] == symbol_short!("Contract")`, `topics[1] == symbol_short!("Upgraded")`
    - When this test passes it confirms the two-symbol topic fix is correct
    - Run: `cargo test test_upgrade_event_topic` from `contracts/escrow/`
    - **EXPECTED OUTCOME**: Test PASSES (confirms bug is fixed)
    - _Requirements: 2.1, 2.2_

  - [ ] 3.3 Verify preservation tests still pass
    - **Property 2: Preservation** - All Other Events Unchanged After Fix
    - **IMPORTANT**: Re-run the SAME tests from task 2 â€” do NOT write new tests
    - Run `cargo test` from `contracts/escrow/` to execute the full test suite including `test_upgrade`, `test_upgrade_unauthorized`, `test_upgrade_event_payload`, and all preservation property tests
    - **EXPECTED OUTCOME**: All tests PASS (confirms no regressions â€” other event topics unaffected)
    - Confirm that `test_upgrade` and `test_upgrade_unauthorized` in `test_admin.rs` still pass without modification

- [ ] 4. Document contract_upgraded in docs/events.md
  - Add a `contract_upgraded` entry under the "Contract Initialization & Config" section in `docs/events.md`
  - Insert after the `contract_unpaused` entry (or at a logical position consistent with existing ordering in that section)
  - Use the same bullet format as all other entries:
    ```markdown
    - **contract_upgraded**:
      - Topics: `["Contract", "Upgraded"]`
      - Payload: `ContractUpgradedEvent` `{ admin, new_wasm_hash, timestamp }`
    ```
  - Note: topics is a two-element list with no indexed participant address, matching the `(symbol_short!("Contract"), symbol_short!("Upgraded"))` tuple
  - _Requirements: 2.3_

- [ ] 5. Checkpoint â€” Ensure all tests pass
  - Run the full escrow test suite: `cargo test` from `contracts/escrow/`
  - Ensure all tests pass, ask the user if questions arise

## Notes

- The fix is purely in the topic tuple of `emit_contract_upgraded`. No logic, storage, or payload changes.
- `symbol_short!` enforces a 9-character limit at compile time â€” both `"Contract"` (8) and `"Upgraded"` (8) are within the limit.
- Task 1 (exploration test) is expected to FAIL on unfixed code â€” that is the correct outcome confirming the bug.
- Task 2 (preservation tests) is expected to PASS on unfixed code â€” these tests establish the behavioral baseline before the fix.
- The `Symbol` import at the top of `events.rs` is still used by `emit_resolver_vote_recorded`, so it should not be removed from the file-level `use` statement.
