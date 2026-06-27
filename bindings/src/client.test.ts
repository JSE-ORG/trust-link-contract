/**
 * Integration tests for EscrowClient (#371).
 *
 * Uses a mock ContractTransport so the tests run without a live Soroban node.
 * Each test verifies that EscrowClient correctly serialises the method name and
 * argument list and returns the value produced by the transport.
 *
 * Run with: `npm run test` (compiles tests to `dist-test/` then runs Node's test runner).
 */

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { EscrowClient, createBatch, type ContractTransport } from "./client.js";
import {
  EscrowState,
  DisputeStatus,
  ResolutionType,
  type EscrowData,
  type DisputeData,
  type FeeConfig,
} from "./types.js";

// ---------------------------------------------------------------------------
// Mock transport
// ---------------------------------------------------------------------------

function makeMockTransport(returnValue: unknown = undefined): {
  transport: ContractTransport;
  calls: { method: string; args: readonly unknown[] }[];
} {
  const calls: { method: string; args: readonly unknown[] }[] = [];
  const transport: ContractTransport = {
    invoke(method, args) {
      calls.push({ method, args });
      return returnValue as never;
    },
  };
  return { transport, calls };
}

// ---------------------------------------------------------------------------
// Fixture data
// ---------------------------------------------------------------------------

const ADMIN = "GADMIN000000000000000000000000000000000000000000000000000000";
const FEE_COLLECTOR = "GFEES000000000000000000000000000000000000000000000000000000";
const SELLER = "GSELLER00000000000000000000000000000000000000000000000000000";
const BUYER = "GBUYER000000000000000000000000000000000000000000000000000000";
const RESOLVER = "GRESOLVE000000000000000000000000000000000000000000000000000";
const TOKEN = "GTOKEN000000000000000000000000000000000000000000000000000000";

const ESCROW_ID = 1n;
const AMOUNT = 1_000_000n;
const FEE_BPS = 50;
const SHIPPING_WINDOW = 172_800n;

const MOCK_ESCROW: EscrowData = {
  payees: [{ address: SELLER, bps: 10000 }],
  buyer: BUYER,
  resolver: RESOLVER,
  token: TOKEN,
  amount: AMOUNT,
  fee_bps: FEE_BPS,
  resolver_fee_bps: 0,
  shipping_window: SHIPPING_WINDOW,
  funded_at: 1_700_000_000n,
  dispute_deadline: 1_700_172_800n,
  state: EscrowState.Funded,
  shipped_at: 0n,
  tracking_id: null,
  delivered_at: 0n,
  notes: null,
};

const MOCK_DISPUTE: DisputeData = {
  escrow_id: ESCROW_ID,
  reason: "damaged",
  description: "Item arrived damaged",
  evidence_hash: new Uint8Array(32),
  status: DisputeStatus.Active,
  disputed_at: 1_700_100_000n,
};

const MOCK_FEE_CONFIG: FeeConfig = {
  collector: FEE_COLLECTOR,
  max_fee_bps: 500,
};

// ---------------------------------------------------------------------------
// Single method invocation tests
// ---------------------------------------------------------------------------

describe("initialize", () => {
  it("invokes 'initialize' with admin and feeCollector args", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.initialize(ADMIN, FEE_COLLECTOR, 200);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].method, "initialize");
    assert.deepEqual(calls[0].args, [ADMIN, FEE_COLLECTOR, 200]);
  });
});

describe("pause_contract", () => {
  it("invokes 'pause_contract' with caller", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.pause_contract(ADMIN);
    assert.equal(calls[0].method, "pause_contract");
    assert.deepEqual(calls[0].args, [ADMIN]);
  });
});

describe("unpause_contract", () => {
  it("invokes 'unpause_contract' with caller", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.unpause_contract(ADMIN);
    assert.equal(calls[0].method, "unpause_contract");
    assert.deepEqual(calls[0].args, [ADMIN]);
  });
});

describe("withdraw_fees", () => {
  it("invokes 'withdraw_fees' with caller, token, to, and amount", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.withdraw_fees(ADMIN, TOKEN, FEE_COLLECTOR, AMOUNT);
    assert.equal(calls[0].method, "withdraw_fees");
    assert.deepEqual(calls[0].args, [ADMIN, TOKEN, FEE_COLLECTOR, AMOUNT]);
  });
});

describe("create_escrow", () => {
  it("invokes 'create_escrow' and returns the escrow id", () => {
    const { transport, calls } = makeMockTransport(ESCROW_ID);
    const client = new EscrowClient(transport);
    const payees = [{ address: SELLER, bps: 10000 }];
    const id = client.create_escrow(payees, BUYER, RESOLVER, TOKEN, AMOUNT, FEE_BPS, 0, SHIPPING_WINDOW);
    assert.equal(calls[0].method, "create_escrow");
    assert.deepEqual(calls[0].args, [payees, BUYER, RESOLVER, TOKEN, AMOUNT, FEE_BPS, 0, SHIPPING_WINDOW]);
    assert.equal(id, ESCROW_ID);
  });
});

describe("fund_escrow", () => {
  it("invokes 'fund_escrow' with escrowId and buyer", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.fund_escrow(ESCROW_ID, BUYER);
    assert.equal(calls[0].method, "fund_escrow");
    assert.deepEqual(calls[0].args, [ESCROW_ID, BUYER]);
  });
});

describe("mark_shipped", () => {
  it("invokes 'mark_shipped' with caller, escrowId, and trackingId", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.mark_shipped(SELLER, ESCROW_ID, "TRACK001");
    assert.equal(calls[0].method, "mark_shipped");
    assert.deepEqual(calls[0].args, [SELLER, ESCROW_ID, "TRACK001"]);
  });
});

describe("confirm_delivery", () => {
  it("invokes 'confirm_delivery' with caller and escrowId", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.confirm_delivery(BUYER, ESCROW_ID);
    assert.equal(calls[0].method, "confirm_delivery");
    assert.deepEqual(calls[0].args, [BUYER, ESCROW_ID]);
  });
});

describe("raise_dispute", () => {
  it("invokes 'raise_dispute' with all required args", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    const evidenceHash = new Uint8Array(32);
    client.raise_dispute(BUYER, ESCROW_ID, "damaged", "Item arrived damaged", evidenceHash);
    assert.equal(calls[0].method, "raise_dispute");
    assert.deepEqual(calls[0].args, [BUYER, ESCROW_ID, "damaged", "Item arrived damaged", evidenceHash]);
  });
});

describe("resolve_dispute", () => {
  it("invokes 'resolve_dispute' with Release resolution", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.resolve_dispute(RESOLVER, ESCROW_ID, ResolutionType.Release);
    assert.equal(calls[0].method, "resolve_dispute");
    assert.deepEqual(calls[0].args, [RESOLVER, ESCROW_ID, ResolutionType.Release]);
  });

  it("invokes 'resolve_dispute' with Refund resolution", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.resolve_dispute(RESOLVER, ESCROW_ID, ResolutionType.Refund);
    assert.deepEqual(calls[0].args, [RESOLVER, ESCROW_ID, ResolutionType.Refund]);
  });
});

describe("auto_release", () => {
  it("invokes 'auto_release' with escrowId", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.auto_release(ESCROW_ID);
    assert.equal(calls[0].method, "auto_release");
    assert.deepEqual(calls[0].args, [ESCROW_ID]);
  });
});

describe("get_escrow", () => {
  it("invokes 'get_escrow' and returns EscrowData", () => {
    const { transport, calls } = makeMockTransport(MOCK_ESCROW);
    const client = new EscrowClient(transport);
    const data = client.get_escrow(ESCROW_ID);
    assert.equal(calls[0].method, "get_escrow");
    assert.deepEqual(calls[0].args, [ESCROW_ID]);
    assert.deepEqual(data, MOCK_ESCROW);
  });
});

describe("get_dispute", () => {
  it("invokes 'get_dispute' and returns DisputeData when present", () => {
    const { transport, calls } = makeMockTransport(MOCK_DISPUTE);
    const client = new EscrowClient(transport);
    const dispute = client.get_dispute(ESCROW_ID);
    assert.equal(calls[0].method, "get_dispute");
    assert.deepEqual(calls[0].args, [ESCROW_ID]);
    assert.deepEqual(dispute, MOCK_DISPUTE);
  });

  it("returns null when no dispute exists", () => {
    const { transport } = makeMockTransport(null);
    const client = new EscrowClient(transport);
    const dispute = client.get_dispute(ESCROW_ID);
    assert.equal(dispute, null);
  });
});

describe("get_fee_config", () => {
  it("invokes 'get_fee_config' and returns FeeConfig", () => {
    const { transport, calls } = makeMockTransport(MOCK_FEE_CONFIG);
    const client = new EscrowClient(transport);
    const config = client.get_fee_config();
    assert.equal(calls[0].method, "get_fee_config");
    assert.deepEqual(calls[0].args, []);
    assert.deepEqual(config, MOCK_FEE_CONFIG);
  });
});

describe("set_arbitration_fee", () => {
  it("invokes 'set_arbitration_fee' with caller and feeBps", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.set_arbitration_fee(ADMIN, 200);
    assert.equal(calls[0].method, "set_arbitration_fee");
    assert.deepEqual(calls[0].args, [ADMIN, 200]);
  });
});

describe("get_arbitration_fee", () => {
  it("invokes 'get_arbitration_fee' and returns the fee value", () => {
    const { transport, calls } = makeMockTransport(200);
    const client = new EscrowClient(transport);
    const fee = client.get_arbitration_fee();
    assert.equal(calls[0].method, "get_arbitration_fee");
    assert.deepEqual(calls[0].args, []);
    assert.equal(fee, 200);
  });
});

describe("rotate_resolver", () => {
  it("invokes 'rotate_resolver' with caller, escrowId, and newResolver", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    const newResolver = "GNEWRESOLV0000000000000000000000000000000000000000000000000";
    client.rotate_resolver(SELLER, ESCROW_ID, newResolver);
    assert.equal(calls[0].method, "rotate_resolver");
    assert.deepEqual(calls[0].args, [SELLER, ESCROW_ID, newResolver]);
  });
});

describe("cancel_escrow", () => {
  it("invokes 'cancel_escrow' with caller and escrowId", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.cancel_escrow(SELLER, ESCROW_ID);
    assert.equal(calls[0].method, "cancel_escrow");
    assert.deepEqual(calls[0].args, [SELLER, ESCROW_ID]);
  });
});

// ---------------------------------------------------------------------------
// Transport: async support
// ---------------------------------------------------------------------------

describe("async transport", () => {
  it("get_escrow resolves the Promise returned by transport.invoke", async () => {
    const asyncTransport: ContractTransport = {
      invoke(_method, _args) {
        return Promise.resolve(MOCK_ESCROW) as never;
      },
    };
    const client = new EscrowClient(asyncTransport);
    const data = await client.get_escrow(ESCROW_ID);
    assert.deepEqual(data, MOCK_ESCROW);
  });

  it("get_dispute resolves null when no dispute exists (async)", async () => {
    const asyncTransport: ContractTransport = {
      invoke() {
        return Promise.resolve(null) as never;
      },
    };
    const client = new EscrowClient(asyncTransport);
    const result = await client.get_dispute(ESCROW_ID);
    assert.equal(result, null);
  });
});

// ---------------------------------------------------------------------------
// Multicall and Batching Tests
// ---------------------------------------------------------------------------

describe("multicall", () => {
  it("invokes 'multicall' with calls array and returns results", () => {
    const mockResults = [null, null];
    const { transport, calls } = makeMockTransport(mockResults);
    const client = new EscrowClient(transport);
    const results = client.multicall([
      { function: "fund_escrow", args: [ESCROW_ID, BUYER] },
      { function: "mark_shipped", args: [SELLER, ESCROW_ID, "TRACK001"] },
    ]);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].method, "multicall");
    assert.deepEqual(calls[0].args, [[
      { function: "fund_escrow", args: [ESCROW_ID, BUYER] },
      { function: "mark_shipped", args: [SELLER, ESCROW_ID, "TRACK001"] },
    ]]);
    assert.deepEqual(results, mockResults);
  });
});

describe("EscrowBatch / batch / createBatch", () => {
  it("builds calls and invokes multicall on client.execute()", async () => {
    const mockResults = [null, null, MOCK_ESCROW];
    const { transport, calls } = makeMockTransport(mockResults);
    const client = new EscrowClient(transport);
    const results = await client
      .batch()
      .fund_escrow(ESCROW_ID, BUYER)
      .mark_shipped(SELLER, ESCROW_ID, "TRACK001")
      .get_escrow(ESCROW_ID)
      .execute();

    assert.equal(calls.length, 1);
    assert.equal(calls[0].method, "multicall");
    assert.deepEqual(calls[0].args, [[
      { function: "fund_escrow", args: [ESCROW_ID, BUYER] },
      { function: "mark_shipped", args: [SELLER, ESCROW_ID, "TRACK001"] },
      { function: "get_escrow", args: [ESCROW_ID] },
    ]]);
    assert.deepEqual(results, mockResults);
  });

  it("createBatch creates and executes a batch using the transport directly", async () => {
    const mockResults = [true];
    const { transport, calls } = makeMockTransport(mockResults);
    const results = await createBatch(transport)
      .auto_release(ESCROW_ID)
      .execute();

    assert.equal(calls.length, 1);
    assert.equal(calls[0].method, "multicall");
    assert.deepEqual(calls[0].args, [[
      { function: "auto_release", args: [ESCROW_ID] }
    ]]);
    assert.deepEqual(results, mockResults);
  });

  it("pendingCalls returns a snapshot of calls", () => {
    const { transport } = makeMockTransport();
    const batch = new EscrowClient(transport).batch()
      .fund_escrow(ESCROW_ID, BUYER)
      .mark_shipped(SELLER, ESCROW_ID, "TRACK001");

    assert.deepEqual(batch.pendingCalls(), [
      { function: "fund_escrow", args: [ESCROW_ID, BUYER] },
      { function: "mark_shipped", args: [SELLER, ESCROW_ID, "TRACK001"] },
    ]);
  });

  it("handles alternative fluent builders correctly", () => {
    const { transport } = makeMockTransport();
    const evidenceHash = new Uint8Array(32);
    const payees = [{ address: SELLER, bps: 10000 }];
    
    const batch = new EscrowClient(transport).batch()
      .initialize(ADMIN, FEE_COLLECTOR, 200)
      .pause_contract(ADMIN)
      .unpause_contract(ADMIN)
      .withdraw_fees(ADMIN, TOKEN, FEE_COLLECTOR, AMOUNT)
      .create_escrow(payees, BUYER, RESOLVER, TOKEN, AMOUNT, FEE_BPS, 0, SHIPPING_WINDOW)
      .confirm_delivery(BUYER, ESCROW_ID)
      .raise_dispute(BUYER, ESCROW_ID, "damaged", "Item arrived damaged", evidenceHash)
      .resolve_dispute(RESOLVER, ESCROW_ID, ResolutionType.Release)
      .get_dispute(ESCROW_ID)
      .get_fee_config()
      .set_arbitration_fee(ADMIN, 200)
      .get_arbitration_fee()
      .cancel_escrow(SELLER, ESCROW_ID)
      .rotate_resolver(SELLER, ESCROW_ID, RESOLVER);

    assert.equal(batch.pendingCalls().length, 14);
  });
});