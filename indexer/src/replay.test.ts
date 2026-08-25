/**
 * Regression coverage for the replay script's fixture-path resolution and
 * resume-index logic.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";

import { findResumeIndex, defaultFixturePath } from "./replay.js";
import type { Cursor, RawEvent } from "./types.js";

function event(ledger_sequence: number, tx_index: number, event_index: number): RawEvent {
  return {
    ledger_sequence,
    tx_index,
    event_index,
    contract_id: "C...",
    topics: ["Escrow", "Created"],
    payload: {},
  };
}

test("defaultFixturePath resolves to the bundled indexer/fixtures/events.json", () => {
  // Regression test: this previously resolved two directories up from
  // `src/`, landing outside the `indexer/` package entirely and causing
  // `ENOENT` whenever replay.ts was run without an explicit fixture path.
  const path = defaultFixturePath();
  assert.ok(path.endsWith("indexer/fixtures/events.json"), path);
  assert.ok(existsSync(path), `expected fixture to exist at ${path}`);
});

test("findResumeIndex skips events at or before the persisted cursor", () => {
  const events = [event(101, 0, 0), event(101, 0, 1), event(102, 0, 0)];
  const cursor: Cursor = { ledger_sequence: 101, tx_index: 0, event_index: 0 };

  assert.equal(findResumeIndex(events, cursor), 1);
});

test("findResumeIndex returns 0 for a fresh (origin) cursor", () => {
  const events = [event(101, 0, 0), event(102, 0, 0)];
  const cursor: Cursor = { ledger_sequence: 0, tx_index: 0, event_index: 0 };

  assert.equal(findResumeIndex(events, cursor), 0);
});

test("findResumeIndex returns events.length when every event is already committed", () => {
  const events = [event(101, 0, 0), event(102, 0, 0)];
  const cursor: Cursor = { ledger_sequence: 102, tx_index: 0, event_index: 0 };

  assert.equal(findResumeIndex(events, cursor), events.length);
});

test("findResumeIndex returns 0 for an empty fixture", () => {
  const cursor: Cursor = { ledger_sequence: 0, tx_index: 0, event_index: 0 };
  assert.equal(findResumeIndex([], cursor), 0);
});
