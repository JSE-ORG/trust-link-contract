/**
 * Integration coverage for `SorobanRpcSource.fetchEvents` (#674).
 *
 * Builds real `xdr.ScVal` topics/values with `nativeToScVal` (the same
 * encoding Soroban RPC returns from a live `getEvents` call) and feeds them
 * through a fake `rpc.Server` double, so the test exercises the actual XDR
 * decoding path rather than a hand-rolled mock of `RawEvent`.
 */

import test from "node:test";
import assert from "node:assert/strict";

import { nativeToScVal, xdr, Contract, StrKey, type rpc } from "@stellar/stellar-sdk";

import { SorobanRpcSource, toJsonSafe, cursorAfter } from "./ingest.js";
import type { Cursor } from "./types.js";

const CONTRACT_ID = new Contract(StrKey.encodeContract(Buffer.alloc(32, 7))).toString();

function symbol(s: string): xdr.ScVal {
  return nativeToScVal(s, { type: "symbol" });
}

function struct(fields: Record<string, xdr.ScVal>): xdr.ScVal {
  const entries = Object.entries(fields).map(
    ([key, val]) => new xdr.ScMapEntry({ key: symbol(key), val }),
  );
  return xdr.ScVal.scvMap(entries);
}

/** A minimal RawEventResponse-shaped object, pre-parsed the way `getEvents()` returns it. */
interface FakeEvent {
  ledger: number;
  transactionIndex: number;
  contractId: Contract;
  topic: xdr.ScVal[];
  value: xdr.ScVal;
}

function fakeEvent(ledger: number, transactionIndex: number, escrowId: number): FakeEvent {
  return {
    ledger,
    transactionIndex,
    contractId: new Contract(CONTRACT_ID),
    topic: [symbol("Escrow"), symbol("Created")],
    value: struct({
      schema_version: nativeToScVal(1, { type: "u32" }),
      escrow_id: nativeToScVal(BigInt(escrowId), { type: "u64" }),
      amount: nativeToScVal(1_000_000_000n, { type: "i128" }),
      evidence_hash: nativeToScVal(Buffer.from("aabbcc", "hex"), { type: "bytes" }),
    }),
  };
}

/** Fake `rpc.Server` returning a fixed batch and recording the request it received. */
function fakeServer(events: FakeEvent[]): {
  server: Pick<rpc.Server, "getEvents">;
  lastRequest: () => unknown;
} {
  let lastRequest: unknown;
  const server: Pick<rpc.Server, "getEvents"> = {
    getEvents: async (request) => {
      lastRequest = request;
      return {
        latestLedger: 1000,
        oldestLedger: 1,
        latestLedgerCloseTime: "0",
        oldestLedgerCloseTime: "0",
        cursor: "fake-cursor",
        events: events as unknown as rpc.Api.EventResponse[],
      };
    },
  };
  return { server, lastRequest: () => lastRequest };
}

const ORIGIN: Cursor = { ledger_sequence: 0, tx_index: 0, event_index: 0 };

test("fetchEvents decodes topics and payload from real XDR ScVals", async () => {
  const { server } = fakeServer([fakeEvent(101, 0, 1)]);
  const source = new SorobanRpcSource(server);

  const events = await source.fetchEvents(ORIGIN, CONTRACT_ID);

  assert.equal(events.length, 1);
  const [event] = events;
  assert.deepEqual(event!.topics, ["Escrow", "Created"]);
  assert.equal(event!.contract_id, CONTRACT_ID);
  assert.equal(event!.ledger_sequence, 101);
  assert.equal(event!.tx_index, 0);
  assert.equal(event!.event_index, 0);
  assert.deepEqual(event!.payload, {
    schema_version: 1,
    escrow_id: "1",
    amount: "1000000000",
    evidence_hash: "aabbcc",
  });
});

test("fetchEvents derives a stable, incrementing event_index per (ledger, tx)", async () => {
  const { server } = fakeServer([
    fakeEvent(101, 0, 1),
    fakeEvent(101, 0, 2),
    fakeEvent(101, 1, 3),
    fakeEvent(102, 0, 4),
  ]);
  const source = new SorobanRpcSource(server);

  const events = await source.fetchEvents(ORIGIN, CONTRACT_ID);

  assert.deepEqual(
    events.map((e) => [e.ledger_sequence, e.tx_index, e.event_index]),
    [
      [101, 0, 0],
      [101, 0, 1],
      [101, 1, 0],
      [102, 0, 0],
    ],
  );
});

test("fetchEvents passes the cursor's ledger as startLedger for resumability", async () => {
  const { server, lastRequest } = fakeServer([]);
  const source = new SorobanRpcSource(server);

  const cursor: Cursor = { ledger_sequence: 500, tx_index: 2, event_index: 1 };
  await source.fetchEvents(cursor, CONTRACT_ID);

  const request = lastRequest() as { startLedger: number };
  assert.equal(request.startLedger, 500);
});

test("fetchEvents excludes events at or before the cursor (mid-ledger resume)", async () => {
  const { server } = fakeServer([
    fakeEvent(101, 0, 1), // event_index 0 — at the cursor, must be dropped
    fakeEvent(101, 0, 2), // event_index 1 — after the cursor, must be kept
    fakeEvent(101, 1, 3), // different tx — after the cursor, must be kept
  ]);
  const source = new SorobanRpcSource(server);

  // Simulates resuming after the first event of ledger 101 tx 0 was already
  // applied — fetchEvents re-requests the whole ledger but must not
  // reprocess what's already committed.
  const cursor: Cursor = { ledger_sequence: 101, tx_index: 0, event_index: 0 };
  const events = await source.fetchEvents(cursor, CONTRACT_ID);

  assert.deepEqual(
    events.map((e) => [e.ledger_sequence, e.tx_index, e.event_index]),
    [
      [101, 0, 1],
      [101, 1, 0],
    ],
  );
  for (const event of events) {
    assert.ok(
      cursorAfter(
        { ledger_sequence: event.ledger_sequence, tx_index: event.tx_index, event_index: event.event_index },
        cursor,
      ),
    );
  }
});

test("toJsonSafe converts bigints to strings and byte buffers to hex", () => {
  assert.equal(toJsonSafe(123n), "123");
  assert.equal(toJsonSafe(Buffer.from("aabbcc", "hex")), "aabbcc");
  assert.deepEqual(toJsonSafe({ a: 1n, b: [2n, "x"] }), { a: "1", b: ["2", "x"] });
});
