/**
 * Tests for the replay script's public API.
 *
 * These cover fixture loading, validation and resume arithmetic — everything
 * that does not need a live database. `replay()` itself is exercised against
 * Postgres by `npm run replay`.
 * Regression coverage for the replay script's fixture-path resolution and
 * resume-index logic.
 */

import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { DEFAULT_FIXTURE, findResumeIndex, loadFixture } from "./replay.js";
import type { Cursor, RawEvent } from "./types.js";

const ORIGIN: Cursor = { ledger_sequence: 0, tx_index: 0, event_index: 0 };

function event(ledger: number, tx = 0, index = 0): RawEvent {
  return {
    ledger_sequence: ledger,
    tx_index: tx,
    event_index: index,
    contract_id: "CESCROW",
    topics: ["Escrow", "Created"],
    payload: { schema_version: 1 },
  };
}

/** Write `content` to a throwaway fixture file and return its path. */
function fixtureFile(content: unknown): string {
  const dir = mkdtempSync(join(tmpdir(), "replay-test-"));
  const path = join(dir, "events.json");
  writeFileSync(path, JSON.stringify(content));
  return path;
}

// ---------------------------------------------------------------------------
// DEFAULT_FIXTURE
// ---------------------------------------------------------------------------

test("DEFAULT_FIXTURE points at the bundled fixture and it loads", () => {
  // Guards the path arithmetic: a wrong number of `..` segments resolves
  // outside indexer/ and the CLI's no-argument form breaks.
  assert.match(DEFAULT_FIXTURE, /indexer[/\\]fixtures[/\\]events\.json$/);
  assert.ok(loadFixture(DEFAULT_FIXTURE).length > 0);
});

// ---------------------------------------------------------------------------
// loadFixture
// ---------------------------------------------------------------------------

test("loadFixture returns events in fixture order", () => {
  const events = loadFixture(fixtureFile([event(100), event(101), event(102)]));
  assert.deepEqual(
    events.map((e) => e.ledger_sequence),
    [100, 101, 102],
  );
});

test("loadFixture accepts an empty fixture", () => {
  assert.deepEqual(loadFixture(fixtureFile([])), []);
});

test("loadFixture rejects a non-array fixture", () => {
  assert.throws(() => loadFixture(fixtureFile({ events: [] })), /must be a JSON array/);
});

test("loadFixture rejects an entry missing a positional field", () => {
  const broken = { ...event(100) } as Record<string, unknown>;
  delete broken["tx_index"];
  assert.throws(() => loadFixture(fixtureFile([broken])), /non-integer "tx_index"/);
});

test("loadFixture rejects an entry with no topics", () => {
  assert.throws(
    () => loadFixture(fixtureFile([{ ...event(100), topics: [] }])),
    /no "topics" array/,
  );
});

test("loadFixture rejects an entry with no payload", () => {
  const broken = { ...event(100) } as Record<string, unknown>;
  delete broken["payload"];
  assert.throws(() => loadFixture(fixtureFile([broken])), /no "payload" object/);
});

test("loadFixture rejects an out-of-order fixture", () => {
  // Out-of-order events would be skipped for good on resume: the cursor only
  // moves forward.
  assert.throws(
    () => loadFixture(fixtureFile([event(101), event(100)])),
    /is not sorted/,
  );
});

test("loadFixture rejects duplicate cursors", () => {
  assert.throws(() => loadFixture(fixtureFile([event(100), event(100)])), /is not sorted/);
});

test("loadFixture orders within a ledger by tx then event index", () => {
  const ok = [event(100, 0, 0), event(100, 0, 1), event(100, 1, 0), event(101, 0, 0)];
  assert.equal(loadFixture(fixtureFile(ok)).length, 4);

  assert.throws(
    () => loadFixture(fixtureFile([event(100, 1, 0), event(100, 0, 5)])),
    /is not sorted/,
  );
});

// ---------------------------------------------------------------------------
// findResumeIndex
// ---------------------------------------------------------------------------

test("findResumeIndex starts at 0 for a fresh database", () => {
  assert.equal(findResumeIndex([event(100), event(101)], ORIGIN), 0);
});

test("findResumeIndex skips events at or before the cursor", () => {
  const events = [event(100), event(101), event(102)];
  const cursor: Cursor = { ledger_sequence: 101, tx_index: 0, event_index: 0 };
  assert.equal(findResumeIndex(events, cursor), 2);
});

test("findResumeIndex resumes mid-ledger", () => {
  const events = [event(100, 0, 0), event(100, 0, 1), event(100, 1, 0)];
  const cursor: Cursor = { ledger_sequence: 100, tx_index: 0, event_index: 0 };
  assert.equal(findResumeIndex(events, cursor), 1);
});

test("findResumeIndex returns events.length when everything is committed", () => {
  const events = [event(100), event(101)];
  const cursor: Cursor = { ledger_sequence: 101, tx_index: 0, event_index: 0 };
  assert.equal(findResumeIndex(events, cursor), events.length);
});

test("findResumeIndex handles an empty fixture", () => {
  assert.equal(findResumeIndex([], ORIGIN), 0);
});

test("findResumeIndex is idempotent across repeated resumes", () => {
  // The resume-safety guarantee: replaying after a full run has nothing to do.
  const events = [event(100), event(101), event(102)];
  const afterAll: Cursor = { ledger_sequence: 102, tx_index: 0, event_index: 0 };
  assert.equal(findResumeIndex(events, afterAll), events.length);
  assert.equal(findResumeIndex(events, afterAll), events.length);
});
