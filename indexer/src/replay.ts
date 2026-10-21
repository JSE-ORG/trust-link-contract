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

import { fileURLToPath } from "node:url";

import { cursorOf, formatCursor, loadFixture, planReplay } from "./replay-fixture.js";
export { findResumeIndex, formatCursor, loadFixture } from "./replay-fixture.js";

import { getPool, closePool } from "./db.js";
import { readCursor } from "./cursor.js";
import { ingestBatch } from "./ingest.js";

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
    const plan = planReplay(events, cursor);

    if (plan.pending.length === 0) {
      log("[replay] all events already ingested — nothing to do");
      return 0;
    }

    if (plan.startIndex > 0) {
      log(
        `[replay] resuming after ${formatCursor(cursor)} — ` +
          `skipping ${plan.startIndex} already-committed event(s)`,
      );
    }

    const applied = await ingestBatch(pool, plan.pending);

    const last = cursorOf(plan.pending[plan.pending.length - 1]!);
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
  return DEFAULT_FIXTURE;
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

export { defaultFixturePath };
