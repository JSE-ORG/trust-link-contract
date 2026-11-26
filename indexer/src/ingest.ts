/**
 * Live ingestion loop — polls an EventSource for new contract events, applies
 * them to the materialized tables, and advances the cursor.
 *
 * Usage:
 *   DATABASE_URL=postgres://... \
 *   CONTRACT_ID=C...            \
 *   SOROBAN_RPC_URL=https://...  \
 *   npx tsx src/ingest.ts
 *
 * The loop resumes from the persisted cursor on every restart, so no event is
 * processed twice and no event is skipped.
 *
 * Environment variables:
 *   - `DATABASE_URL`      (required, read by `db.ts`) — Postgres connection string.
 *   - `CONTRACT_ID`       (required) — the escrow contract to poll events for.
 *   - `SOROBAN_RPC_URL`   (required) — RPC endpoint passed to `SorobanRpcSource`.
 *   - `POLL_INTERVAL_MS`  (optional, default `6000`) — delay between poll cycles.
 *   - `EVENTS_PAGE_SIZE`  (optional, default `1000`) — `getEvents` page size per poll,
 *     read by `SorobanRpcSource` in `soroban-event-source.ts`.
 *
 * Architecture: this module owns two concerns that `replay.ts` (fixture-driven
 * replay) also depends on:
 *   1. `EventSource` / `SorobanRpcSource` (`soroban-event-source.ts`) — fetch
 *      events from the chain. Only used in live mode; `replay.ts` reads
 *      events straight from a JSON file instead.
 *   2. `ingestBatch` (below) — apply a batch of events to Postgres. Shared
 *      verbatim by both live ingestion and replay, so materialization logic
 *      only lives in one place and behaves identically in both modes.
 *
 * Minimal example wiring a custom source into the shared batch-apply logic
 * (this is essentially what `runLive` below does):
 * ```ts
 * import { getPool } from "./db.js";
 * import { readCursor } from "./cursor.js";
 * import { ingestBatch, SorobanRpcSource } from "./ingest.js";
 *
 * const pool = getPool();
 * const source = new SorobanRpcSource(process.env.SOROBAN_RPC_URL!);
 * const cursor = await readCursor(pool);
 * const events = await source.fetchEvents(cursor, process.env.CONTRACT_ID!);
 * const applied = await ingestBatch(pool, events);
 * console.log(`applied ${applied} events`);
 * ```
 */

import { getPool, withTx, closePool } from "./db.js";
import { readCursor, writeCursor } from "./cursor.js";
import { processEvent } from "./apply.js";
import { topicKey, cursorAfter, type RawEvent } from "./types.js";
import {
  type EventSource,
  SorobanRpcSource,
  toJsonSafe,
  decodeTopics,
} from "./soroban-event-source.js";

// ---------------------------------------------------------------------------
// Main ingestion logic (shared by live and replay modes)
// ---------------------------------------------------------------------------

/**
 * Process one batch of events atomically.
 *
 * Each event is inserted into the raw `events` log and applied to the
 * materialized tables inside a single transaction.  The cursor advances only
 * after the transaction commits, so a crash mid-batch leaves the cursor at
 * the last committed event and the next run resumes correctly.
 *
 * `events` must already be in ascending `(ledger_sequence, tx_index,
 * event_index)` order — both `SorobanRpcSource.fetchEvents` and the replay
 * fixture loader guarantee this. Individual events are idempotent (the raw
 * insert is `ON CONFLICT DO NOTHING` and `writeCursor` is an upsert), so
 * re-passing an already-applied event is harmless — it just re-derives the
 * same materialized state and re-writes the same cursor value.
 *
 * @param pool - Postgres pool from `getPool()`.
 * @param events - events to apply, oldest first.
 * @returns the number of events processed (equal to `events.length` on success).
 *
 * @example
 * ```ts
 * const pool = getPool();
 * const applied = await ingestBatch(pool, [
 *   { ledger_sequence: 101, tx_index: 0, event_index: 0, contract_id: "C...",
 *     topics: ["Escrow", "Created"], payload: { schema_version: 1, escrow_id: 1, ... } },
 * ]);
 * // applied === 1; the cursor now points at (101, 0, 0).
 * ```
 */
export async function ingestBatch(
  pool: ReturnType<typeof getPool>,
  events: RawEvent[],
): Promise<number> {
  let applied = 0;

  for (const event of events) {
    await withTx(pool, async (client) => {
      // Insert raw event (UNIQUE constraint makes this idempotent).
      await client.query(
        `INSERT INTO events
           (ledger_sequence, tx_index, event_index, contract_id, topic_key, schema_version, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7)
         ON CONFLICT (ledger_sequence, tx_index, event_index) DO NOTHING`,
        [
          event.ledger_sequence,
          event.tx_index,
          event.event_index,
          event.contract_id,
          topicKey(event.topics),
          Number(event.payload["schema_version"] ?? 0),
          JSON.stringify(event.payload),
        ],
      );

      // Apply state transition to materialized tables.
      await processEvent(client, event);

      // Advance the cursor — committed atomically with the above mutations.
      await writeCursor(client, {
        ledger_sequence: event.ledger_sequence,
        tx_index: event.tx_index,
        event_index: event.event_index,
      });
    });

    applied++;
  }

  return applied;
}

// ---------------------------------------------------------------------------
// Live polling loop
// ---------------------------------------------------------------------------

/** Delay between `runLive` poll cycles, in ms. Env: `POLL_INTERVAL_MS` (default 6000). */
const POLL_INTERVAL_MS = parseInt(process.env["POLL_INTERVAL_MS"] ?? "6000", 10);
/** Contract to poll events for in live mode. Env: `CONTRACT_ID` (required, see `runLive`). */
const CONTRACT_ID = process.env["CONTRACT_ID"] ?? "";

/**
 * Runs the live polling loop until the process receives `SIGINT`.
 *
 * Each cycle: read the persisted cursor, fetch anything newer from `source`,
 * apply it via `ingestBatch`, then sleep `POLL_INTERVAL_MS` and repeat. A
 * fetch/apply error is logged and swallowed rather than crashing the loop —
 * the next cycle simply re-reads the same cursor and retries, so a transient
 * RPC or DB outage self-heals without operator intervention.
 *
 * @example
 * ```ts
 * await runLive(new SorobanRpcSource(process.env.SOROBAN_RPC_URL!));
 * ```
 */
async function runLive(source: EventSource): Promise<void> {
  if (!CONTRACT_ID) throw new Error("CONTRACT_ID environment variable is required");

  const pool = getPool();
  console.log(`[ingest] starting live ingestion for contract ${CONTRACT_ID}`);

  process.on("SIGINT", async () => {
    console.log("[ingest] shutting down…");
    await closePool();
    process.exit(0);
  });

  // eslint-disable-next-line no-constant-condition
  while (true) {
    try {
      const cursor = await readCursor(pool);
      const events = await source.fetchEvents(cursor, CONTRACT_ID);

      if (events.length > 0) {
        const applied = await ingestBatch(pool, events);
        const last = events[events.length - 1]!;
        console.log(
          `[ingest] applied ${applied} events up to ledger ${last.ledger_sequence}`,
        );
      }
    } catch (err) {
      console.error("[ingest] error:", (err as Error).message);
    }

    await new Promise((r) => setTimeout(r, POLL_INTERVAL_MS));
  }
}

// Run when invoked directly.
const isMain =
  process.argv[1] !== undefined &&
  new URL(import.meta.url).pathname === process.argv[1];

if (isMain) {
  const rpcUrl = process.env["SOROBAN_RPC_URL"];
  if (!rpcUrl) throw new Error("SOROBAN_RPC_URL is required for live ingestion");

  runLive(new SorobanRpcSource(rpcUrl)).catch((err) => {
    console.error("[ingest] fatal:", err);
    process.exit(1);
  });
}

// Re-exported for backward compatibility — these used to be defined directly
// in this file; they now live in `soroban-event-source.ts`.
export { cursorAfter, SorobanRpcSource, toJsonSafe, decodeTopics };
export type { EventSource };
