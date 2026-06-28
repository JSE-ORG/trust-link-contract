/**
 * Transaction simulation helpers for the TrustLink escrow contract.
 *
 * Soroban lets you *simulate* a contract call before signing and submitting it,
 * so a frontend can surface the exact error a transaction would produce without
 * spending fees or asking the user to sign. These helpers wrap that flow around
 * the `ContractTransport` / `EscrowClient` abstractions and normalise any
 * failure into a typed {@link ContractInvokeError} via `parseContractError`.
 *
 * @example
 * ```ts
 * import { EscrowClient } from "trustlink-escrow-bindings";
 * import { simulateAndCatch, createEscrowSimulator } from "trustlink-escrow-bindings";
 *
 * const client = new EscrowClient(transport);
 *
 * // Simulate any call and inspect the outcome before submitting.
 * const result = await simulateAndCatch(() => client.fund_escrow(id, buyer));
 * if (!result.ok) {
 *   console.error(result.error.code, result.error.message); // expected contract error
 *   return;
 * }
 * // result.value is the (typed) return value — safe to submit for real.
 * ```
 *
 * @module simulation
 */

import type { ContractTransport } from "./client.js";
import { ContractInvokeError, ErrorCode, parseContractError } from "./errors.js";

/**
 * Discriminated result of a simulated contract call.
 *
 * `ok: true`  → the call would succeed; `value` holds the decoded return value.
 * `ok: false` → the call would revert; `error` describes why.
 */
export type SimulationResult<T> =
  | { readonly ok: true; readonly value: T }
  | {
      readonly ok: false;
      /**
       * The parsed contract error when the failure maps to a known contract
       * error code, otherwise a generic `Error` wrapping the raw failure.
       */
      readonly error: ContractInvokeError | Error;
      /** The contract error code, or `null` for non-contract failures. */
      readonly code: ErrorCode | null;
      /** The original, unmodified error thrown by the transport. */
      readonly raw: unknown;
    };

/**
 * Run a contract call and capture its outcome instead of throwing.
 *
 * This never rejects: a thrown contract error is parsed into a
 * {@link ContractInvokeError} and returned as `{ ok: false, ... }`. Any other
 * thrown value is wrapped in a plain `Error` so callers always get a structured
 * result they can branch on.
 *
 * @param call - A thunk that performs the (read-only or mutating) contract call.
 *               Typically a method on {@link EscrowClient}.
 * @returns A {@link SimulationResult} describing success or the expected error.
 */
export async function simulateAndCatch<T>(call: () => T | Promise<T>): Promise<SimulationResult<T>> {
  try {
    const value = await call();
    return { ok: true, value };
  } catch (raw) {
    const parsed = parseContractError(raw);
    if (parsed) {
      return { ok: false, error: parsed, code: parsed.code, raw };
    }
    const error = raw instanceof Error ? raw : new Error(typeof raw === "string" ? raw : "Simulation failed.");
    return { ok: false, error, code: null, raw };
  }
}

/**
 * Simulate a contract call and throw if it would fail.
 *
 * Use this as a pre-submit guard when you want the failing path to surface as an
 * exception (e.g. inside an existing try/catch) rather than a result object. On
 * success it returns the decoded value so a single call both validates and reads.
 *
 * @throws {ContractInvokeError} when the call maps to a known contract error.
 * @throws {Error} for any other failure.
 */
export async function assertSimulationSucceeds<T>(call: () => T | Promise<T>): Promise<T> {
  const result = await simulateAndCatch(call);
  if (!result.ok) throw result.error;
  return result.value;
}

/**
 * Type guard narrowing a {@link SimulationResult} to its failure variant.
 */
export function isSimulationError<T>(
  result: SimulationResult<T>,
): result is Extract<SimulationResult<T>, { ok: false }> {
  return !result.ok;
}

/**
 * A transport whose every call is simulated and returns a {@link SimulationResult}
 * instead of throwing. Mirrors {@link ContractTransport} so it can be dropped in
 * wherever a dry-run is wanted.
 */
export interface SimulationTransport {
  simulate<TReturn>(method: string, args: readonly unknown[]): Promise<SimulationResult<TReturn>>;
}

/**
 * Wrap a {@link ContractTransport} so each invocation is simulated and its
 * outcome captured. Useful for batching pre-flight checks over arbitrary methods.
 *
 * @example
 * ```ts
 * const sim = createEscrowSimulator(transport);
 * const { ok, error } = await sim.simulate("raise_dispute", [id, reason, desc, hash]);
 * ```
 */
export function createEscrowSimulator(transport: ContractTransport): SimulationTransport {
  return {
    simulate<TReturn>(method: string, args: readonly unknown[]): Promise<SimulationResult<TReturn>> {
      return simulateAndCatch<TReturn>(() => transport.invoke<TReturn>(method, args));
    },
  };
}
