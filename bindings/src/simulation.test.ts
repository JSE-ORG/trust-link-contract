/**
 * Behavioural tests for the transaction simulation helpers.
 *
 * Run with: `npm run test` (compiles to `dist/` then runs Node's test runner).
 */

import test from "node:test";
import assert from "node:assert/strict";

import { ContractInvokeError, ErrorCode } from "./errors.js";
import {
  assertSimulationSucceeds,
  createEscrowSimulator,
  isSimulationError,
  simulateAndCatch,
} from "./simulation.js";
import type { ContractTransport } from "./client.js";

test("simulateAndCatch returns the value on success", async () => {
  const result = await simulateAndCatch(() => Promise.resolve(42n));
  assert.equal(result.ok, true);
  if (result.ok) assert.equal(result.value, 42n);
});

test("simulateAndCatch returns the expected error for a thrown ContractInvokeError", async () => {
  const result = await simulateAndCatch(() => {
    throw new ContractInvokeError(ErrorCode.EscrowNotFound);
  });

  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.ok(result.error instanceof ContractInvokeError);
    assert.equal(result.code, ErrorCode.EscrowNotFound);
    assert.match(result.error.message, /does not exist/i);
  }
});

test("simulateAndCatch parses a raw Soroban contract error string", async () => {
  const result = await simulateAndCatch(() => {
    throw new Error("Simulation failed: Error(Contract, #5)");
  });

  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.equal(result.code, ErrorCode.NotAuthorized);
    assert.ok(result.error instanceof ContractInvokeError);
  }
});

test("simulateAndCatch parses a { code } shaped contract error", async () => {
  const result = await simulateAndCatch(() => Promise.reject({ code: ErrorCode.ContractPaused }));

  assert.equal(result.ok, false);
  if (!result.ok) assert.equal(result.code, ErrorCode.ContractPaused);
});

test("simulateAndCatch wraps a non-contract failure with code null", async () => {
  const result = await simulateAndCatch(() => {
    throw new Error("network down");
  });

  assert.equal(result.ok, false);
  if (!result.ok) {
    assert.equal(result.code, null);
    assert.equal(result.error.message, "network down");
  }
});

test("isSimulationError narrows the failure variant", async () => {
  const result = await simulateAndCatch(() => {
    throw new ContractInvokeError(ErrorCode.InvalidAmount);
  });
  assert.equal(isSimulationError(result), true);
});

test("assertSimulationSucceeds returns the value or throws the expected error", async () => {
  assert.equal(await assertSimulationSucceeds(() => Promise.resolve("ok")), "ok");

  await assert.rejects(
    () =>
      assertSimulationSucceeds(() => {
        throw new ContractInvokeError(ErrorCode.DisputeWindowClosed);
      }),
    (err: unknown) =>
      err instanceof ContractInvokeError && err.code === ErrorCode.DisputeWindowClosed,
  );
});

test("createEscrowSimulator simulates calls through a transport", async () => {
  const transport: ContractTransport = {
    invoke(method) {
      if (method === "get_arbitration_fee") return 250 as never;
      throw new ContractInvokeError(ErrorCode.NotAuthorized);
    },
  };
  const sim = createEscrowSimulator(transport);

  const ok = await sim.simulate<number>("get_arbitration_fee", []);
  assert.equal(ok.ok, true);
  if (ok.ok) assert.equal(ok.value, 250);

  const failed = await sim.simulate("withdraw_fees", []);
  assert.equal(failed.ok, false);
  if (!failed.ok) assert.equal(failed.code, ErrorCode.NotAuthorized);
});
