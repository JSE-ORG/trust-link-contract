/**
 * Integration tests for EscrowClient (#371).
 *
 * Uses a mock ContractTransport so the tests run without a live Soroban node.
 * Each test verifies that EscrowClient correctly serialises the method name and
 * argument list and returns the value produced by the transport.
 *
 * To run against Soroban testnet, replace MockTransport with a real
 * SorobanTransport (see README for connection details).
 */

import { describe, it, expect, vi } from "vitest";
import { EscrowClient, type ContractTransport } from "../src/client.js";
import {
  EscrowState,
  DisputeStatus,
  ResolutionType,
  type EscrowData,
  type DisputeData,
  type FeeConfig,
} from "../src/types.js";

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
  seller: SELLER,
  buyer: BUYER,
  resolver: RESOLVER,
  token: TOKEN,
  amount: AMOUNT,
  fee_bps: FEE_BPS,
  shipping_window: SHIPPING_WINDOW,
  funded_at: 1_700_000_000n,
  dispute_deadline: 1_700_172_800n,
  state: EscrowState.Funded,
  shipped_at: 0n,
  tracking_id: null,
  delivered_at: 0n,
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
// initialize
// ---------------------------------------------------------------------------

describe("initialize", () => {
  it("invokes 'initialize' with admin and feeCollector args", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.initialize(ADMIN, FEE_COLLECTOR);
    expect(calls).toHaveLength(1);
    expect(calls[0].method).toBe("initialize");
    expect(calls[0].args).toEqual([ADMIN, FEE_COLLECTOR]);
  });
});

// ---------------------------------------------------------------------------
// pause / unpause
// ---------------------------------------------------------------------------

describe("pause_contract", () => {
  it("invokes 'pause_contract' with no args", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.pause_contract();
    expect(calls[0].method).toBe("pause_contract");
    expect(calls[0].args).toEqual([]);
  });
});

describe("unpause_contract", () => {
  it("invokes 'unpause_contract' with no args", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.unpause_contract();
    expect(calls[0].method).toBe("unpause_contract");
    expect(calls[0].args).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// withdraw_fees
// ---------------------------------------------------------------------------

describe("withdraw_fees", () => {
  it("invokes 'withdraw_fees' with token, to, and amount", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.withdraw_fees(TOKEN, FEE_COLLECTOR, AMOUNT);
    expect(calls[0].method).toBe("withdraw_fees");
    expect(calls[0].args).toEqual([TOKEN, FEE_COLLECTOR, AMOUNT]);
  });
});

// ---------------------------------------------------------------------------
// create_escrow
// ---------------------------------------------------------------------------

describe("create_escrow", () => {
  it("invokes 'create_escrow' and returns the escrow id", () => {
    const { transport, calls } = makeMockTransport(ESCROW_ID);
    const client = new EscrowClient(transport);
    const id = client.create_escrow(SELLER, RESOLVER, TOKEN, AMOUNT, FEE_BPS, SHIPPING_WINDOW);
    expect(calls[0].method).toBe("create_escrow");
    expect(calls[0].args).toEqual([SELLER, RESOLVER, TOKEN, AMOUNT, FEE_BPS, SHIPPING_WINDOW]);
    expect(id).toBe(ESCROW_ID);
  });
});

// ---------------------------------------------------------------------------
// fund_escrow
// ---------------------------------------------------------------------------

describe("fund_escrow", () => {
  it("invokes 'fund_escrow' with escrowId and buyer", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.fund_escrow(ESCROW_ID, BUYER);
    expect(calls[0].method).toBe("fund_escrow");
    expect(calls[0].args).toEqual([ESCROW_ID, BUYER]);
  });
});

// ---------------------------------------------------------------------------
// mark_shipped
// ---------------------------------------------------------------------------

describe("mark_shipped", () => {
  it("invokes 'mark_shipped' with caller, escrowId, and trackingId", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.mark_shipped(SELLER, ESCROW_ID, "TRACK001");
    expect(calls[0].method).toBe("mark_shipped");
    expect(calls[0].args).toEqual([SELLER, ESCROW_ID, "TRACK001"]);
  });
});

// ---------------------------------------------------------------------------
// confirm_delivery
// ---------------------------------------------------------------------------

describe("confirm_delivery", () => {
  it("invokes 'confirm_delivery' with caller and escrowId", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.confirm_delivery(BUYER, ESCROW_ID);
    expect(calls[0].method).toBe("confirm_delivery");
    expect(calls[0].args).toEqual([BUYER, ESCROW_ID]);
  });
});

// ---------------------------------------------------------------------------
// raise_dispute
// ---------------------------------------------------------------------------

describe("raise_dispute", () => {
  it("invokes 'raise_dispute' with all required args", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    const evidenceHash = new Uint8Array(32);
    client.raise_dispute(ESCROW_ID, "damaged", "Item arrived damaged", evidenceHash);
    expect(calls[0].method).toBe("raise_dispute");
    expect(calls[0].args).toEqual([ESCROW_ID, "damaged", "Item arrived damaged", evidenceHash]);
  });
});

// ---------------------------------------------------------------------------
// resolve_dispute
// ---------------------------------------------------------------------------

describe("resolve_dispute", () => {
  it("invokes 'resolve_dispute' with Release resolution", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.resolve_dispute(ESCROW_ID, ResolutionType.Release);
    expect(calls[0].method).toBe("resolve_dispute");
    expect(calls[0].args).toEqual([ESCROW_ID, ResolutionType.Release]);
  });

  it("invokes 'resolve_dispute' with Refund resolution", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.resolve_dispute(ESCROW_ID, ResolutionType.Refund);
    expect(calls[0].args).toEqual([ESCROW_ID, ResolutionType.Refund]);
  });
});

// ---------------------------------------------------------------------------
// auto_release
// ---------------------------------------------------------------------------

describe("auto_release", () => {
  it("invokes 'auto_release' with escrowId", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.auto_release(ESCROW_ID);
    expect(calls[0].method).toBe("auto_release");
    expect(calls[0].args).toEqual([ESCROW_ID]);
  });
});

// ---------------------------------------------------------------------------
// get_escrow
// ---------------------------------------------------------------------------

describe("get_escrow", () => {
  it("invokes 'get_escrow' and returns EscrowData", () => {
    const { transport, calls } = makeMockTransport(MOCK_ESCROW);
    const client = new EscrowClient(transport);
    const data = client.get_escrow(ESCROW_ID);
    expect(calls[0].method).toBe("get_escrow");
    expect(calls[0].args).toEqual([ESCROW_ID]);
    expect(data).toEqual(MOCK_ESCROW);
  });
});

// ---------------------------------------------------------------------------
// get_dispute
// ---------------------------------------------------------------------------

describe("get_dispute", () => {
  it("invokes 'get_dispute' and returns DisputeData when present", () => {
    const { transport, calls } = makeMockTransport(MOCK_DISPUTE);
    const client = new EscrowClient(transport);
    const dispute = client.get_dispute(ESCROW_ID);
    expect(calls[0].method).toBe("get_dispute");
    expect(calls[0].args).toEqual([ESCROW_ID]);
    expect(dispute).toEqual(MOCK_DISPUTE);
  });

  it("returns null when no dispute exists", () => {
    const { transport } = makeMockTransport(null);
    const client = new EscrowClient(transport);
    const dispute = client.get_dispute(ESCROW_ID);
    expect(dispute).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// get_fee_config
// ---------------------------------------------------------------------------

describe("get_fee_config", () => {
  it("invokes 'get_fee_config' and returns FeeConfig", () => {
    const { transport, calls } = makeMockTransport(MOCK_FEE_CONFIG);
    const client = new EscrowClient(transport);
    const config = client.get_fee_config();
    expect(calls[0].method).toBe("get_fee_config");
    expect(calls[0].args).toEqual([]);
    expect(config).toEqual(MOCK_FEE_CONFIG);
  });
});

// ---------------------------------------------------------------------------
// set_arbitration_fee
// ---------------------------------------------------------------------------

describe("set_arbitration_fee", () => {
  it("invokes 'set_arbitration_fee' with caller and feeBps", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    client.set_arbitration_fee(ADMIN, 200);
    expect(calls[0].method).toBe("set_arbitration_fee");
    expect(calls[0].args).toEqual([ADMIN, 200]);
  });
});

// ---------------------------------------------------------------------------
// get_arbitration_fee
// ---------------------------------------------------------------------------

describe("get_arbitration_fee", () => {
  it("invokes 'get_arbitration_fee' and returns the fee value", () => {
    const { transport, calls } = makeMockTransport(200);
    const client = new EscrowClient(transport);
    const fee = client.get_arbitration_fee();
    expect(calls[0].method).toBe("get_arbitration_fee");
    expect(calls[0].args).toEqual([]);
    expect(fee).toBe(200);
  });
});

// ---------------------------------------------------------------------------
// rotate_resolver
// ---------------------------------------------------------------------------

describe("rotate_resolver", () => {
  it("invokes 'rotate_resolver' with caller, escrowId, and newResolver", () => {
    const { transport, calls } = makeMockTransport();
    const client = new EscrowClient(transport);
    const newResolver = "GNEWRESOLV0000000000000000000000000000000000000000000000000";
    client.rotate_resolver(SELLER, ESCROW_ID, newResolver);
    expect(calls[0].method).toBe("rotate_resolver");
    expect(calls[0].args).toEqual([SELLER, ESCROW_ID, newResolver]);
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
    expect(data).toEqual(MOCK_ESCROW);
  });

  it("get_dispute resolves null when no dispute exists (async)", async () => {
    const asyncTransport: ContractTransport = {
      invoke() {
        return Promise.resolve(null) as never;
      },
    };
    const client = new EscrowClient(asyncTransport);
    const result = await client.get_dispute(ESCROW_ID);
    expect(result).toBeNull();
  });
});
