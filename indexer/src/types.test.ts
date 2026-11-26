import test from "node:test";
import assert from "node:assert/strict";

import {
  deriveDisputeNetAmount,
  deriveFee,
  deriveNetAmount,
  type DisputeResolvedPayload,
} from "./types.js";

test("deriveFee floors sub-stroop protocol fees to zero", () => {
  assert.equal(deriveFee("1", 300), 0n);
  assert.equal(deriveNetAmount("1", 300), 1n);
});

test("deriveNetAmount subtracts the floored fee from the gross amount", () => {
  assert.equal(deriveFee("1000", 300), 30n);
  assert.equal(deriveNetAmount("1000", 300), 970n);
  assert.equal(deriveNetAmount(1_000, 200), 980n);
});

test("deriveNetAmount handles amounts beyond Number.MAX_SAFE_INTEGER", () => {
  assert.equal(
    deriveNetAmount("100000000000000000001", 300),
    97_000_000_000_000_000_001n,
  );
});

test("deriveDisputeNetAmount subtracts platform and protocol fees", () => {
  const payload: Pick<
    DisputeResolvedPayload,
    "amount" | "arbitration_fee" | "resolver_fee" | "platform_fee"
  > = {
    amount: "950",
    arbitration_fee: "50",
    resolver_fee: "0",
    platform_fee: "0",
  };

  // fee basis = 950 + 50 + 0 = 1000; protocol fee at 200 bps = 20.
  assert.equal(deriveDisputeNetAmount(payload, 200), 930n);
});

test("deriveFee rejects out-of-range fee rates and negative amounts", () => {
  assert.throws(() => deriveFee("1000", -1), RangeError);
  assert.throws(() => deriveFee("1000", 10_001), RangeError);
  assert.throws(() => deriveFee("1000", 1.5), RangeError);
  assert.throws(() => deriveFee("-1", 100), RangeError);
});
