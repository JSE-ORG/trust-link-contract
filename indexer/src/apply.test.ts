/**
 * Coverage guard for processEvent.
 *
 * Every topic key the contract emits (see KNOWN_TOPIC_KEYS in types.ts) must
 * reach a handler that writes to the expected table.  A topic that silently
 * falls through to the `default` branch fails here, so new contract events
 * cannot be added without a matching indexer handler.
 */

import test from "node:test";
import assert from "node:assert/strict";

import { processEvent } from "./apply.js";
import { KNOWN_TOPIC_KEYS, type RawEvent } from "./types.js";

// ---------------------------------------------------------------------------
// Fake pg client — records every statement instead of talking to Postgres
// ---------------------------------------------------------------------------

interface RecordedQuery {
  sql: string;
  params: unknown[];
}

function fakeClient(): { client: never; queries: RecordedQuery[] } {
  const queries: RecordedQuery[] = [];
  const client = {
    query(sql: string, params: unknown[] = []) {
      queries.push({ sql, params });
      return Promise.resolve({ rows: [], rowCount: 0 });
    },
  };
  return { client: client as never, queries };
}

/** Capture console.warn for the duration of `fn`. */
async function captureWarnings(fn: () => Promise<void>): Promise<string[]> {
  const warnings: string[] = [];
  const original = console.warn;
  console.warn = (...args: unknown[]) => {
    warnings.push(args.map(String).join(" "));
  };
  try {
    await fn();
  } finally {
    console.warn = original;
  }
  return warnings;
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ADDR_A = "GA".padEnd(56, "A");
const ADDR_B = "GB".padEnd(56, "B");

function event(topics: string[], payload: Record<string, unknown>): RawEvent {
  return {
    ledger_sequence: 100,
    tx_index: 0,
    event_index: 0,
    contract_id: "CTRUSTLINK",
    topics,
    payload: { schema_version: 1, ...payload },
  };
}

const stateFields = { timestamp: 1_700_000_000, prev_state: "Created", new_state: "Funded" };

/**
 * One fixture per known topic key.
 *
 * `table` is a fragment that must appear in at least one emitted statement —
 * proof the event was materialized rather than dropped.
 */
const CASES: ReadonlyArray<{ key: string; event: RawEvent; table: string }> = [
  {
    key: "Escrow:Created",
    table: "INSERT INTO escrows",
    event: event(["Escrow", "Created", ADDR_A], {
      escrow_id: 1,
      seller: ADDR_A,
      resolver: ADDR_B,
      token: ADDR_B,
      amount: "1000",
      fee_bps: 100,
      resolver_fee_bps: 50,
      shipping_window: 86_400,
      ...stateFields,
    }),
  },
  {
    key: "Escrow:Funded",
    table: "UPDATE escrows",
    event: event(["Escrow", "Funded", ADDR_B], { escrow_id: 1, buyer: ADDR_B, amount: "1000", ...stateFields }),
  },
  {
    key: "Escrow:Shipped",
    table: "UPDATE escrows",
    event: event(["Escrow", "Shipped", ADDR_A], { escrow_id: 1, seller: ADDR_A, tracking_id: "TRK1", ...stateFields }),
  },
  {
    key: "Escrow:Delivered",
    table: "UPDATE escrows",
    event: event(["Escrow", "Delivered"], { escrow_id: 1, delivered_at: 1_700_000_100 }),
  },
  {
    key: "Escrow:Completed",
    table: "UPDATE escrows",
    event: event(["Escrow", "Completed", ADDR_A], { escrow_id: 1, recipient: ADDR_A, amount: "1000", fee_bps: 100, ...stateFields }),
  },
  {
    key: "Escrow:Canceled",
    table: "UPDATE escrows",
    event: event(["Escrow", "Canceled", ADDR_B], { escrow_id: 1, seller: ADDR_A, cancelled_by: ADDR_B, ...stateFields }),
  },
  {
    key: "Escrow:Released",
    table: "UPDATE escrows",
    event: event(["Escrow", "Released", ADDR_A], { escrow_id: 1, seller: ADDR_A, amount: "1000", fee_bps: 100, ...stateFields }),
  },
  {
    key: "Basket:Created",
    table: "INSERT INTO basket_escrows",
    event: event(["Basket", "Created", ADDR_A], { escrow_id: 2, seller: ADDR_A, token_count: 3, timestamp: 1_700_000_000 }),
  },
  {
    key: "Refund:Requested",
    table: "UPDATE escrows",
    event: event(["Refund", "Requested", ADDR_B], { escrow_id: 1, buyer: ADDR_B, ...stateFields }),
  },
  {
    key: "Refund:Approved",
    table: "UPDATE escrows",
    event: event(["Refund", "Approved", ADDR_A], { escrow_id: 1, seller: ADDR_A, ...stateFields }),
  },
  {
    key: "Dispute:Raised",
    table: "INSERT INTO disputes",
    event: event(["Dispute", "Raised", ADDR_B], {
      escrow_id: 1,
      buyer: ADDR_B,
      reason: "not_delivered",
      description: "never arrived",
      evidence_hash: "ab".repeat(32),
      ...stateFields,
    }),
  },
  {
    key: "Dispute:Resolved",
    table: "UPDATE disputes",
    event: event(["Dispute", "Resolved", ADDR_B], {
      escrow_id: 1,
      resolver: ADDR_B,
      resolution: "Release",
      recipient: ADDR_A,
      amount: "1000",
      arbitration_fee: "10",
      resolver_fee: "5",
      ...stateFields,
    }),
  },
  {
    key: "Dispute:Pending",
    table: "UPDATE disputes",
    event: event(["Dispute", "Pending", ADDR_B], {
      escrow_id: 1,
      resolver: ADDR_B,
      resolution: "Release",
      amount: "1000",
      appeal_deadline: 1_700_100_000,
      pending_at: 1_700_000_000,
    }),
  },
  {
    key: "Dispute:Appealed",
    table: "UPDATE escrows",
    event: event(["Dispute", "Appealed", ADDR_B], { escrow_id: 1, appellant: ADDR_B, timestamp: 1_700_000_000 }),
  },
  {
    key: "Resolver:Rotated",
    table: "UPDATE escrows",
    event: event(["Resolver", "Rotated"], { escrow_id: 1, old_resolver: ADDR_A, new_resolver: ADDR_B, rotated_at: 1_700_000_000 }),
  },
  {
    key: "Resolver:Approved",
    table: "INSERT INTO approved_resolvers",
    event: event(["Resolver", "Approved", ADDR_B], { resolver: ADDR_B, caller: ADDR_A, timestamp: 1_700_000_000 }),
  },
  {
    key: "Resolver:Removed",
    table: "INSERT INTO approved_resolvers",
    event: event(["Resolver", "Removed", ADDR_B], { resolver: ADDR_B, caller: ADDR_A, timestamp: 1_700_000_000 }),
  },
  {
    key: "ResStrct:Updated",
    table: "INSERT INTO contract_config",
    event: event(["ResStrct", "Updated"], { old_strict: false, new_strict: true, caller: ADDR_A, timestamp: 1_700_000_000 }),
  },
  {
    key: "resolver_vote_recorded",
    table: "INSERT INTO resolver_votes",
    event: event(["resolver_vote_recorded"], {
      escrow_id: 1,
      resolver: ADDR_B,
      resolution: "Release",
      vote_count: 1,
      threshold: 2,
      voted_at: 1_700_000_000,
    }),
  },
  {
    key: "Message:Posted",
    table: "INSERT INTO messages",
    event: event(["Message", "Posted", ADDR_B], { escrow_id: 1, sender: ADDR_B, timestamp: 1_700_000_000 }),
  },
  {
    key: "Contract:Init",
    table: "INSERT INTO contract_config",
    event: event(["Contract", "Init"], { admin: ADDR_A, fee_collector: ADDR_B, arbitration_fee_bps: 100, timestamp: 1_700_000_000 }),
  },
  {
    key: "Contract:Paused",
    table: "INSERT INTO contract_config",
    event: event(["Contract", "Paused", ADDR_A], { admin: ADDR_A, timestamp: 1_700_000_000 }),
  },
  {
    key: "Contract:Unpaused",
    table: "INSERT INTO contract_config",
    event: event(["Contract", "Unpaused", ADDR_A], { admin: ADDR_A, timestamp: 1_700_000_000 }),
  },
  {
    key: "Action:Paused",
    table: "INSERT INTO contract_config",
    event: event(["Action", "Paused", "fund_escrow"], { action: "fund_escrow", caller: ADDR_A, timestamp: 1_700_000_000 }),
  },
  {
    key: "Action:Unpaused",
    table: "INSERT INTO contract_config",
    event: event(["Action", "Unpaused", "fund_escrow"], { action: "fund_escrow", caller: ADDR_A, timestamp: 1_700_000_000 }),
  },
  {
    key: "Admin:Rotated",
    table: "INSERT INTO contract_config",
    event: event(["Admin", "Rotated"], { old_admin: ADDR_A, new_admin: ADDR_B, timestamp: 1_700_000_000 }),
  },
  {
    key: "Fee:Updated",
    table: "INSERT INTO contract_config",
    event: event(["Fee", "Updated"], { old_fee_bps: 100, new_fee_bps: 200, timestamp: 1_700_000_000 }),
  },
  {
    key: "ProtoFee:Updated",
    table: "INSERT INTO contract_config",
    event: event(["ProtoFee", "Updated"], { old_fee_bps: 10, new_fee_bps: 20, timestamp: 1_700_000_000 }),
  },
  {
    key: "ArbFee:Updated",
    table: "INSERT INTO contract_config",
    event: event(["ArbFee", "Updated"], { old_fee_bps: 30, new_fee_bps: 40, timestamp: 1_700_000_000 }),
  },
  {
    key: "PlatFee:Updated",
    table: "INSERT INTO contract_config",
    event: event(["PlatFee", "Updated"], { old_fee_bps: 5, new_fee_bps: 15, timestamp: 1_700_000_000 }),
  },
  {
    key: "Treasury:Updated",
    table: "INSERT INTO contract_config",
    event: event(["Treasury", "Updated"], { old_treasury: ADDR_A, new_treasury: ADDR_B, timestamp: 1_700_000_000 }),
  },
  {
    key: "TtlExt:Updated",
    table: "INSERT INTO contract_config",
    event: event(["TtlExt", "Updated"], { old_ledgers: 100, new_ledgers: 200, caller: ADDR_A, timestamp: 1_700_000_000 }),
  },
  {
    key: "AmtLimit:Updated",
    table: "INSERT INTO contract_config",
    event: event(["AmtLimit", "Updated"], {
      old_min_amount: "1",
      new_min_amount: "10",
      old_max_amount: "1000",
      new_max_amount: "2000",
      caller: ADDR_A,
      timestamp: 1_700_000_000,
    }),
  },
  {
    key: "Token:Allowlist",
    table: "INSERT INTO token_allowlist",
    event: event(["Token", "Allowlist", ADDR_B], { token: ADDR_B, added: true, timestamp: 1_700_000_000 }),
  },
  {
    key: "Allowlist:Toggled",
    table: "INSERT INTO contract_config",
    event: event(["Allowlist", "Toggled"], { enabled: true, timestamp: 1_700_000_000 }),
  },
  {
    key: "contract_upgraded",
    table: "INSERT INTO contract_config",
    event: event(["contract_upgraded"], { admin: ADDR_A, new_wasm_hash: "cd".repeat(32), timestamp: 1_700_000_000 }),
  },
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test("every known topic key has a fixture", () => {
  const covered = new Set(CASES.map((c) => c.key));
  const missing = [...KNOWN_TOPIC_KEYS].filter((k) => !covered.has(k));
  assert.deepEqual(missing, [], `topic keys without a test fixture: ${missing.join(", ")}`);
});

for (const testCase of CASES) {
  test(`processEvent materializes ${testCase.key}`, async () => {
    const { client, queries } = fakeClient();
    const warnings = await captureWarnings(async () => {
      await processEvent(client, testCase.event);
    });

    assert.deepEqual(warnings, [], `${testCase.key} should not warn`);
    assert.ok(queries.length > 0, `${testCase.key} produced no SQL`);
    assert.ok(
      queries.some((q) => q.sql.includes(testCase.table)),
      `${testCase.key} did not touch "${testCase.table}"; got:\n${queries.map((q) => q.sql).join("\n---\n")}`,
    );
  });
}

test("unknown topics warn instead of being silently dropped", async () => {
  const { client, queries } = fakeClient();
  const warnings = await captureWarnings(async () => {
    await processEvent(client, event(["Totally", "Unknown"], { timestamp: 1 }));
  });

  assert.equal(queries.length, 0);
  assert.equal(warnings.length, 1);
  assert.match(warnings[0]!, /unhandled event topic "Totally:Unknown"/);
  assert.match(warnings[0]!, /unknown topic/);
});

test("schema_version newer than the indexer is rejected", async () => {
  const { client } = fakeClient();
  const future = event(["Escrow", "Delivered"], { escrow_id: 1, delivered_at: 1 });
  future.payload["schema_version"] = 99;

  await assert.rejects(() => processEvent(client, future), /Unsupported schema_version 99/);
});
