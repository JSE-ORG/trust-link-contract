/**
 * Fixture replay — processes a JSON event log deterministically.
 *
 * Usage:
 *   DATABASE_URL=postgres://... npx tsx src/replay.ts [path/to/events.json]
 *
 * Acceptance criteria:
 *   1. Deterministic: running the same fixture twice from a clean database
 *      always produces the same final state.
 *   2. Resume after restart: if interrupted, the next run skips already-
 *      committed events using the persisted cursor and continues from where
 *      it left off.
 *
 * The events in the fixture file must be sorted by
 * (ledger_sequence, tx_index, event_index) ascending — the same order in
 * which Soroban emits them.
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { getPool, closePool } from "./db.js";
import { readCursor } from "./cursor.js";
import { ingestBatch } from "./ingest.js";
import { cursorAfter, type RawEvent, type Cursor } from "./types.js";

// ---------------------------------------------------------------------------
// Fixture loader
// ---------------------------------------------------------------------------

function loadFixture(filePath: string): RawEvent[] {
  const abs = resolve(filePath);
  const raw = JSON.parse(readFileSync(abs, "utf-8")) as unknown;
  if (!Array.isArray(raw)) throw new Error(`Fixture at ${abs} must be a JSON array`);
  return raw as RawEvent[];
}

/** Return the index of the first event that comes strictly after `cursor`. */
function findResumeIndex(events: RawEvent[], cursor: Cursor): number {
  for (let i = 0; i < events.length; i++) {
    const e = events[i]!;
    const evCursor: Cursor = {
      ledger_sequence: e.ledger_sequence,
      tx_index: e.tx_index,
      event_index: e.event_index,
    };
    if (cursorAfter(evCursor, cursor)) return i;
  }
  // All events already processed.
  return events.length;
}

// ---------------------------------------------------------------------------
// Replay entry point
// ---------------------------------------------------------------------------

async function replay(fixturePath: string): Promise<void> {
  const events = loadFixture(fixturePath);
  console.log(`[replay] loaded ${events.length} events from ${fixturePath}`);

  const pool = getPool();

  try {
    const cursor = await readCursor(pool);
    const startIdx = findResumeIndex(events, cursor);

    if (startIdx === events.length) {
      console.log("[replay] all events already ingested — nothing to do");
      return;
    }

    if (startIdx > 0) {
      console.log(
        `[replay] resuming from index ${startIdx} ` +
          `(skipping ${startIdx} already-committed events)`,
      );
    }

    const pending = events.slice(startIdx);
    const applied = await ingestBatch(pool, pending);

    const last = pending[pending.length - 1]!;
    console.log(
      `[replay] done — applied ${applied} events, ` +
        `final ledger ${last.ledger_sequence}/${last.tx_index}/${last.event_index}`,
    );
  } finally {
    await closePool();
  }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

const fixturePath =
  process.argv[2] ?? new URL("../../fixtures/events.json", import.meta.url).pathname;

replay(fixturePath).catch((err) => {
  console.error("[replay] fatal:", err);
  process.exit(1);
});
