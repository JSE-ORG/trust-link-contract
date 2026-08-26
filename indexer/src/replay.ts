/**
 * Deterministic fixture replay for the TrustLink escrow indexer.
 *
 * Reads a JSON array of {@link RawEvent}s — a recorded or hand-written event
 * log — and drives it through the same {@link ingestBatch} path the live
 * ingester uses, so materialized state produced by a replay is identical to
 * state produced from the chain.
 *
 * ## Guarantees
 *
 * 1. **Deterministic.** Replaying the same fixture against a clean database
 *    always yields the same final state. Every handler in `apply.ts` is
 *    idempotent, and events are applied strictly in cursor order.
 * 2. **Resume-safe.** Each event is written, applied and cursor-advanced in one
 *    transaction. An interrupted run leaves the cursor at the last *committed*
 *    event, and the next run skips everything up to it — so replaying twice is
 *    the same as replaying once.
 *
 * ## Fixture format
 *
 * A JSON array of {@link RawEvent}, sorted ascending by
 * `(ledger_sequence, tx_index, event_index)` — the order Soroban emits them in.
 * {@link loadFixture} enforces both the shape and the ordering, because an
 * out-of-order fixture would silently defeat the resume logic: the cursor only
 * moves forward, so an event that sorts before it is skipped for good.
 *
 * ```json
 * [
 *   {
 *     "ledger_sequence": 100,
 *     "tx_index": 0,
 *     "event_index": 0,
 *     "contract_id": "CESCROW...",
 *     "topics": ["Contract", "Init"],
 *     "payload": { "schema_version": 1, "admin": "GADMIN...", "timestamp": 1700000000 }
 *   }
 * ]
 * ```
 *
 * ## Command line
 *
 * ```bash
 * # Replay the bundled fixture (indexer/fixtures/events.json).
 * DATABASE_URL=postgres://user:pass@localhost:5432/trustlink npx tsx src/replay.ts
 *
 * # Replay a specific fixture.
 * DATABASE_URL=postgres://... npx tsx src/replay.ts ./fixtures/regression-812.json
 * ```
 *
 * Requires the schema to exist — `psql "$DATABASE_URL" -f schema.sql` — and
 * exits non-zero on failure so it can gate CI.
 *
 * ## Programmatic use
 *
 * Importing this module does **not** run the CLI, so a test or script can drive
 * a replay directly:
 *
 * ```ts
 * import { replay, loadFixture } from "./replay.js";
 *
 * // Replay a file, returning how many events were applied this run.
 * const applied = await replay("./fixtures/events.json");
 *
 * // Or inspect a fixture without touching the database.
 * const events = loadFixture("./fixtures/events.json");
 * console.log(events.length, "events");
 * ```
 *
 * By default {@link replay} closes the shared connection pool when it finishes.
 * Pass `{ closePoolWhenDone: false }` to replay several fixtures in one process.
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { getPool, closePool } from "./db.js";
import { readCursor } from "./cursor.js";
import { ingestBatch } from "./ingest.js";
import { cursorAfter, type RawEvent, type Cursor } from "./types.js";

/** Bundled fixture used when the CLI is invoked without a path. */
export const DEFAULT_FIXTURE = fileURLToPath(
  new URL("../fixtures/events.json", import.meta.url),
);

/** Options accepted by {@link replay}. */
export interface ReplayOptions {
  /**
   * Close the shared connection pool when the replay finishes.
   *
   * Defaults to `true`, which is what the CLI wants. Set it to `false` when
   * replaying several fixtures in one process, then call `closePool()` yourself.
   */
  closePoolWhenDone?: boolean;
  /** Sink for progress messages. Defaults to `console.log`. */
  log?: (message: string) => void;
}

// ---------------------------------------------------------------------------
// Fixture loading
// ---------------------------------------------------------------------------

/** Cursor position of one event, for ordering comparisons. */
function cursorOf(event: RawEvent): Cursor {
  return {
    ledger_sequence: event.ledger_sequence,
    tx_index: event.tx_index,
    event_index: event.event_index,
  };
}

/** Render a cursor as `ledger/tx/event`, the form used throughout the logs. */
function formatCursor(cursor: Cursor): string {
  return `${cursor.ledger_sequence}/${cursor.tx_index}/${cursor.event_index}`;
}

/**
 * Validate that `value` has the positional fields every event needs.
 *
 * Payload contents are deliberately not checked here — `processEvent` owns
 * that, including the `schema_version` guard.
 */
function assertRawEvent(value: unknown, index: number): asserts value is RawEvent {
  if (value === null || typeof value !== "object") {
    throw new Error(`Fixture entry ${index} is not an object`);
  }
  const event = value as Record<string, unknown>;

  for (const field of ["ledger_sequence", "tx_index", "event_index"] as const) {
    if (!Number.isInteger(event[field])) {
      throw new Error(`Fixture entry ${index} has a non-integer "${field}"`);
    }
  }
  if (typeof event["contract_id"] !== "string") {
    throw new Error(`Fixture entry ${index} has no "contract_id" string`);
  }
  if (!Array.isArray(event["topics"]) || event["topics"].length === 0) {
    throw new Error(`Fixture entry ${index} has no "topics" array`);
  }
  if (event["payload"] === null || typeof event["payload"] !== "object") {
    throw new Error(`Fixture entry ${index} has no "payload" object`);
  }
}

/**
 * Read and validate a fixture file.
 *
 * @param filePath Path to a JSON array of {@link RawEvent}; relative paths
 *   resolve against the current working directory.
 * @returns The events, in fixture order.
 * @throws If the file is not a JSON array, an entry is missing a positional
 *   field, or the entries are not sorted strictly ascending by cursor.
 */
export function loadFixture(filePath: string): RawEvent[] {
  const abs = resolve(filePath);
  const raw = JSON.parse(readFileSync(abs, "utf-8")) as unknown;

  if (!Array.isArray(raw)) {
    throw new Error(`Fixture at ${abs} must be a JSON array`);
  }

  raw.forEach(assertRawEvent);
  const events = raw as RawEvent[];

  // Strictly ascending: equal cursors would mean duplicate events, which the
  // `events` UNIQUE constraint would silently drop on the second occurrence.
  for (let i = 1; i < events.length; i++) {
    const prev = cursorOf(events[i - 1]!);
    const curr = cursorOf(events[i]!);
    if (!cursorAfter(curr, prev)) {
      throw new Error(
        `Fixture at ${abs} is not sorted: entry ${i} (${formatCursor(curr)}) ` +
          `does not come after entry ${i - 1} (${formatCursor(prev)}). ` +
          `Events must ascend by (ledger_sequence, tx_index, event_index).`,
      );
    }
  }

  return events;
}

/**
 * Index of the first event strictly after `cursor` — where a resumed run picks
 * up.
 *
 * Returns `events.length` when every event is already committed, which the
 * caller treats as "nothing to do".
 *
 * @example
 * ```ts
 * findResumeIndex(events, { ledger_sequence: 0, tx_index: 0, event_index: 0 }); // 0
 * ```
 */
export function findResumeIndex(events: RawEvent[], cursor: Cursor): number {
  for (let i = 0; i < events.length; i++) {
    if (cursorAfter(cursorOf(events[i]!), cursor)) return i;
  }
  return events.length;
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

/**
 * Replay a fixture into the database, resuming from the persisted cursor.
 *
 * Already-committed events are skipped, so calling this twice on the same
 * fixture applies each event exactly once.
 *
 * @param fixturePath Path to the fixture; defaults to {@link DEFAULT_FIXTURE}.
 * @param options See {@link ReplayOptions}.
 * @returns The number of events applied *by this run* — `0` when the fixture
 *   was already fully ingested.
 * @throws If the fixture is invalid, or if applying an event fails. A failure
 *   leaves the cursor at the last successfully committed event.
 */
export async function replay(
  fixturePath: string = DEFAULT_FIXTURE,
  options: ReplayOptions = {},
): Promise<number> {
  const { closePoolWhenDone = true, log = console.log } = options;

  const events = loadFixture(fixturePath);
  log(`[replay] loaded ${events.length} events from ${fixturePath}`);

  const pool = getPool();

  try {
    const cursor = await readCursor(pool);
    const startIdx = findResumeIndex(events, cursor);

    if (startIdx === events.length) {
      log("[replay] all events already ingested — nothing to do");
      return 0;
    }

    if (startIdx > 0) {
      log(
        `[replay] resuming after ${formatCursor(cursor)} — ` +
          `skipping ${startIdx} already-committed event(s)`,
      );
    }

    const pending = events.slice(startIdx);
    const applied = await ingestBatch(pool, pending);

    const last = cursorOf(pending[pending.length - 1]!);
    log(`[replay] done — applied ${applied} events, final cursor ${formatCursor(last)}`);
    return applied;
  } finally {
    if (closePoolWhenDone) await closePool();
  }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

/** Path to the bundled fixture, relative to this module (`indexer/fixtures/events.json`). */
function defaultFixturePath(): string {
  return new URL("../fixtures/events.json", import.meta.url).pathname;
}

// Run when invoked directly (mirrors the `isMain` guard in ingest.ts) — this
// keeps the module import-safe so `findResumeIndex`/`defaultFixturePath` can
// be exercised from tests without kicking off a real replay.
const isMain =
  process.argv[1] !== undefined && new URL(import.meta.url).pathname === process.argv[1];

if (isMain) {
  const fixturePath = process.argv[2] ?? defaultFixturePath();
  replay(fixturePath).catch((err) => {
    console.error("[replay] fatal:", err);
    process.exit(1);
  });
}

export { findResumeIndex, defaultFixturePath };
