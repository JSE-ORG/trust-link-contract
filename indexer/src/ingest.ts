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
 *   - `EVENTS_PAGE_SIZE`  (optional, default `1000`) — `getEvents` page size per poll.
 *
 * Architecture: this module owns two concerns that `replay.ts` (fixture-driven
 * replay) also depends on:
 *   1. `EventSource` / `SorobanRpcSource` — fetch events from the chain. Only
 *      used in live mode; `replay.ts` reads events straight from a JSON file
 *      instead.
 *   2. `ingestBatch` — apply a batch of events to Postgres. Shared verbatim by
 *      both live ingestion and replay, so materialization logic only lives in
 *      one place and behaves identically in both modes.
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

import { rpc, scValToNative } from "@stellar/stellar-sdk";

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
 *
 * `fetchEvents` must return events in ascending `(ledger_sequence, tx_index,
 * event_index)` order and must exclude anything at or before `afterCursor` —
 * `SorobanRpcSource` below does this with `cursorAfter`. Implementations are
 * also free to return an empty array on quiet polls; `runLive` treats that as
 * a no-op and just waits for the next cycle.
 *
 * @example Fake source for tests (see ingest.test.ts for a full XDR-backed version)
 * ```ts
 * const fixedSource: EventSource = {
 *   async fetchEvents(afterCursor, contractId) {
 *     return myEvents.filter((e) => cursorAfter(e, afterCursor));
 *   },
 * };
 * const applied = await ingestBatch(pool, await fixedSource.fetchEvents(cursor, contractId));
 * ```
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
/** Max events requested per `getEvents` call. Env: `EVENTS_PAGE_SIZE` (default 1000). */
const EVENTS_PAGE_SIZE = parseInt(process.env["EVENTS_PAGE_SIZE"] ?? "1000", 10);
/** Contract to poll events for in live mode. Env: `CONTRACT_ID` (required, see `runLive`). */
const CONTRACT_ID = process.env["CONTRACT_ID"] ?? "";

/**
 * Recursively converts a `scValToNative()` result into a JSON-safe value:
 * bigints (u64/i64/u128/i128/...) become decimal strings and raw byte
 * buffers (e.g. `BytesN<32>` evidence hashes) become hex strings, matching
 * the shapes of the `*Payload` interfaces in types.ts and the fixture data
 * in fixtures/events.json.
 *
 * @example
 * ```ts
 * toJsonSafe(1_000_000_000_000n);          // -> "1000000000000"
 * toJsonSafe(Buffer.from("aabbcc", "hex")); // -> "aabbcc"
 * toJsonSafe({ amount: 5n, tags: [1n, "x"] });
 * // -> { amount: "5", tags: ["1", "x"] }
 * ```
 */
function toJsonSafe(value: unknown): unknown {
  if (typeof value === "bigint") return value.toString();
  if (value instanceof Uint8Array) return Buffer.from(value).toString("hex");
  if (Array.isArray(value)) return value.map(toJsonSafe);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([k, v]) => [k, toJsonSafe(v)]),
    );
  }
  return value;
}

/**
 * Decodes a single event's topic list into the plain strings RawEvent expects.
 *
 * @example
 * ```ts
 * // topics = [Symbol("Escrow"), Symbol("Created"), Address("GSELLER...")]
 * decodeTopics(rawEvent.topic); // -> ["Escrow", "Created", "GSELLER..."]
 * ```
 */
function decodeTopics(topic: ReturnType<typeof scValToNative>[]): string[] {
  return topic.map((t) => String(toJsonSafe(scValToNative(t))));
}

/**
 * Soroban RPC event source — polls `getEvents` for the configured contract
 * and maps the results to `RawEvent[]`.
 *
 * `getEvents` only accepts a ledger-granular `startLedger`, finer than our
 * persisted (ledger, tx, event) cursor, so we always re-request from
 * `afterCursor.ledger_sequence` (not `+ 1`) and rely on `cursorAfter` to
 * drop anything at or before the cursor — this covers resuming mid-ledger
 * without skipping or reprocessing events (the latter is harmless anyway,
 * since `ingestBatch` upserts are idempotent).
 *
 * The RPC response has no explicit per-ledger event index, so `event_index`
 * is derived by counting events per `(ledger, transactionIndex)` in the
 * order the server returns them — stable and deterministic since Soroban
 * RPC always returns events in ledger/tx/operation order.
 *
 * @example Live usage
 * ```ts
 * const source = new SorobanRpcSource("https://soroban-testnet.stellar.org");
 * const events = await source.fetchEvents(cursor, contractId);
 * ```
 *
 * @example Test double (see ingest.test.ts for the full XDR-backed version)
 * ```ts
 * const source = new SorobanRpcSource({
 *   getEvents: async () => ({ events: [...], latestLedger: 1000, ... }),
 * });
 * ```
 */
export class SorobanRpcSource implements EventSource {
  private readonly server: Pick<rpc.Server, "getEvents">;

  /** Accepts either an RPC URL or a pre-built server (e.g. a test double). */
  constructor(rpcUrlOrServer: string | Pick<rpc.Server, "getEvents">) {
    this.server =
      typeof rpcUrlOrServer === "string" ? new rpc.Server(rpcUrlOrServer) : rpcUrlOrServer;
  }

  /**
   * Fetches events for `contractId` starting at `afterCursor.ledger_sequence`,
   * filters out anything at or before `afterCursor`, and derives a stable
   * `event_index` per `(ledger, tx)`. See the class doc above for why the
   * request always starts at the cursor's ledger rather than ledger + 1.
   */
  async fetchEvents(afterCursor: Cursor, contractId: string): Promise<RawEvent[]> {
    const startLedger = Math.max(afterCursor.ledger_sequence, 1);

    const response = await this.server.getEvents({
      startLedger,
      filters: [{ type: "contract", contractIds: [contractId] }],
      limit: EVENTS_PAGE_SIZE,
    });

    const perTxEventCount = new Map<string, number>();
    const events: RawEvent[] = [];

    for (const event of response.events) {
      const key = `${event.ledger}:${event.transactionIndex}`;
      const eventIndex = perTxEventCount.get(key) ?? 0;
      perTxEventCount.set(key, eventIndex + 1);

      const candidate: Cursor = {
        ledger_sequence: event.ledger,
        tx_index: event.transactionIndex,
        event_index: eventIndex,
      };
      if (!cursorAfter(candidate, afterCursor)) continue;

      events.push({
        ledger_sequence: event.ledger,
        tx_index: event.transactionIndex,
        event_index: eventIndex,
        contract_id: event.contractId?.toString() ?? contractId,
        topics: decodeTopics(event.topic),
        payload: toJsonSafe(scValToNative(event.value)) as Record<string, unknown>,
      });
    }

    return events;
  }
}

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

export { cursorAfter, toJsonSafe, decodeTopics };
