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
 */

import { getPool, withTx, closePool } from "./db.js";
import { readCursor, writeCursor } from "./cursor.js";
import { processEvent } from "./apply.js";
import { topicKey, cursorAfter, type RawEvent, type Cursor } from "./types.js";

// ---------------------------------------------------------------------------
// EventSource abstraction
// ---------------------------------------------------------------------------

/**
 * Anything that can deliver a batch of RawEvents after the given cursor.
 * Swap this for the Soroban RPC adapter (stellar-sdk GetEvents) in production.
 */
export interface EventSource {
  fetchEvents(afterCursor: Cursor, contractId: string): Promise<RawEvent[]>;
}

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

const POLL_INTERVAL_MS = parseInt(process.env["POLL_INTERVAL_MS"] ?? "6000", 10);
const CONTRACT_ID = process.env["CONTRACT_ID"] ?? "";

/**
 * Minimal stub for the Soroban RPC event source.
 *
 * Replace the body of fetchEvents with a real stellar-sdk GetEvents call that:
 *   1. Opens a Soroban RPC connection to SOROBAN_RPC_URL
 *   2. Calls getEvents({ startLedger, filters: [{ contractIds: [contractId] }] })
 *   3. Decodes XDR topics and values using stellar-sdk
 *   4. Maps the results to RawEvent[]
 */
class SorobanRpcSource implements EventSource {
  async fetchEvents(afterCursor: Cursor, contractId: string): Promise<RawEvent[]> {
    const rpcUrl = process.env["SOROBAN_RPC_URL"];
    if (!rpcUrl) throw new Error("SOROBAN_RPC_URL is required for live ingestion");

    // TODO: integrate stellar-sdk SorobanRpc.Server.getEvents()
    // For now return empty so the loop idles gracefully.
    void afterCursor;
    void contractId;
    console.warn("[ingest] SorobanRpcSource is a stub — wire up stellar-sdk here");
    return [];
  }
}

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
  runLive(new SorobanRpcSource()).catch((err) => {
    console.error("[ingest] fatal:", err);
    process.exit(1);
  });
}

export { cursorAfter };
