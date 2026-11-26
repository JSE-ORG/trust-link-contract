/**
 * Soroban RPC event source — fetches contract events over `getEvents` and
 * decodes them into the plain-JSON `RawEvent` shape the ingest pipeline
 * expects.
 *
 * Split out of `ingest.ts` (which owns applying events, not fetching them)
 * so the two concerns — chain I/O + XDR decoding vs. batch apply + the live
 * polling loop — can be read, tested, and changed independently. `ingest.ts`
 * re-exports everything here for backward compatibility.
 */

import { rpc, scValToNative } from "@stellar/stellar-sdk";

import { cursorAfter, type RawEvent, type Cursor } from "./types.js";

/** Max events requested per `getEvents` call. Env: `EVENTS_PAGE_SIZE` (default 1000). */
const EVENTS_PAGE_SIZE = parseInt(process.env["EVENTS_PAGE_SIZE"] ?? "1000", 10);

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
export function toJsonSafe(value: unknown): unknown {
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
export function decodeTopics(topic: ReturnType<typeof scValToNative>[]): string[] {
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
