/**
 * `EscrowBatch` — a fluent builder that collects contract calls and dispatches
 * them in a single `multicall` transaction, plus the {@link createBatch}
 * convenience factory.
 *
 * Split out of `client.ts` so the batching component can be maintained on its
 * own. `EscrowClient.batch()` and `createBatch()` remain the intended entry
 * points, and both symbols are still re-exported from `./client.js` for
 * backwards compatibility.
 */
import {
  type AddressLike,
  type ContractCall,
  type ContractSymbol,
  type Payee,
  type ResolutionType,
} from "./types.js";
import { type ContractTransport, EscrowClient } from "./client.js";

/**
 * A fluent builder for batching multiple escrow contract calls into a single
 * Stellar transaction via the `multicall` entry-point.
 *
 * Use {@link EscrowClient.batch} to obtain an instance.  Chain any number of
 * call methods then call {@link execute} to dispatch.
 *
 * **Why this matters**: Stellar transactions containing
 * `InvokeHostFunction` operations are limited to one operation per
 * transaction.  Rather than submitting N separate transactions, `EscrowBatch`
 * packs N logical calls into a single `multicall` invocation, so only one
 * transaction is broadcast, paying one base fee and requiring one ledger close.
 */
export class EscrowBatch {
  /** Accumulated call descriptors, built up by the fluent API. */
  private readonly _calls: ContractCall[] = [];

  /** @internal Use {@link EscrowClient.batch} instead. */
  constructor(private readonly client: EscrowClient) {}

  // ---- helpers --------------------------------------------------------------

  private push(fn: string, args: readonly unknown[]): this {
    this._calls.push({ function: fn, args });
    return this;
  }

  /**
   * Returns a snapshot of the pending calls (useful for debugging / testing).
   */
  pendingCalls(): readonly ContractCall[] {
    return this._calls;
  }

  /**
   * Dispatches all accumulated calls in a single `multicall` transaction.
   * The returned array contains the decoded return value for each call, in
   * the same order the calls were added.
   */
  execute(): Promise<unknown[]> | unknown[] {
    return this.client.multicall([...this._calls]);
  }

  // ---- call builders -------------------------------------------------------

  /** Batch `initialize(admin, feeCollector, arbitrationFeeBps)`. */
  initialize(admin: AddressLike, feeCollector: AddressLike, arbitrationFeeBps: number): this {
    return this.push("initialize", [admin, feeCollector, arbitrationFeeBps]);
  }

  /** Batch `pause_contract(caller)`. */
  pause_contract(caller: AddressLike): this {
    return this.push("pause_contract", [caller]);
  }

  /** Batch `unpause_contract(caller)`. */
  unpause_contract(caller: AddressLike): this {
    return this.push("unpause_contract", [caller]);
  }

  /** Batch `withdraw_fees(caller, token, to, amount)`. */
  withdraw_fees(caller: AddressLike, token: AddressLike, to: AddressLike, amount: bigint): this {
    return this.push("withdraw_fees", [caller, token, to, amount]);
  }

  /** Batch `create_escrow(payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow)`. */
  create_escrow(
    payees: Payee[],
    buyer: AddressLike | null,
    resolver: AddressLike,
    token: AddressLike,
    amount: bigint,
    feeBps: number,
    resolverFeeBps: number,
    shippingWindow: bigint,
  ): this {
    return this.push("create_escrow", [payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow]);
  }

  /** Batch `fund_escrow(escrowId, buyer)`. */
  fund_escrow(escrowId: bigint, buyer: AddressLike): this {
    return this.push("fund_escrow", [escrowId, buyer]);
  }

  /** Batch `mark_shipped(caller, escrowId, trackingId)`. */
  mark_shipped(caller: AddressLike, escrowId: bigint, trackingId: string): this {
    return this.push("mark_shipped", [caller, escrowId, trackingId]);
  }

  /** Batch `confirm_delivery(caller, escrowId)`. */
  confirm_delivery(caller: AddressLike, escrowId: bigint): this {
    return this.push("confirm_delivery", [caller, escrowId]);
  }

  /** Batch `raise_dispute(caller, escrowId, reason, description, evidenceHash)`. */
  raise_dispute(
    caller: AddressLike,
    escrowId: bigint,
    reason: ContractSymbol,
    description: string,
    evidenceHash: Uint8Array,
  ): this {
    return this.push("raise_dispute", [caller, escrowId, reason, description, evidenceHash]);
  }

  /** Batch `resolve_dispute(caller, escrowId, resolution)`. */
  resolve_dispute(caller: AddressLike, escrowId: bigint, resolution: ResolutionType): this {
    return this.push("resolve_dispute", [caller, escrowId, resolution]);
  }

  /** Batch `auto_release(escrowId)`. */
  auto_release(escrowId: bigint): this {
    return this.push("auto_release", [escrowId]);
  }

  /** Batch `get_escrow(escrowId)` (read-only – safe to include in any batch). */
  get_escrow(escrowId: bigint): this {
    return this.push("get_escrow", [escrowId]);
  }

  /** Batch `get_dispute(escrowId)` (read-only). */
  get_dispute(escrowId: bigint): this {
    return this.push("get_dispute", [escrowId]);
  }

  /** Batch `get_fee_config()` (read-only). */
  get_fee_config(): this {
    return this.push("get_fee_config", []);
  }

  /** Batch `set_arbitration_fee(caller, feeBps)`. */
  set_arbitration_fee(caller: AddressLike, feeBps: number): this {
    return this.push("set_arbitration_fee", [caller, feeBps]);
  }

  /** Batch `get_arbitration_fee()` (read-only). */
  get_arbitration_fee(): this {
    return this.push("get_arbitration_fee", []);
  }

  /** Batch `cancel_escrow(caller, escrowId)`. */
  cancel_escrow(caller: AddressLike, escrowId: bigint): this {
    return this.push("cancel_escrow", [caller, escrowId]);
  }

  /** Batch `rotate_resolver(caller, escrowId, newResolver)`. */
  rotate_resolver(caller: AddressLike, escrowId: bigint, newResolver: AddressLike): this {
    return this.push("rotate_resolver", [caller, escrowId, newResolver]);
  }

  /** Batch `create_basket_escrow(seller, buyer, resolver, tokens, amounts, feeBps, shippingWindow)`. */
  create_basket_escrow(
    seller: AddressLike,
    buyer: AddressLike | null,
    resolver: AddressLike,
    tokens: readonly AddressLike[],
    amounts: readonly bigint[],
    feeBps: number,
    shippingWindow: bigint,
  ): this {
    return this.push("create_basket_escrow", [
      seller,
      buyer,
      resolver,
      tokens,
      amounts,
      feeBps,
      shippingWindow,
    ]);
  }

  /** Batch `fund_basket_escrow(escrowId, buyer)`. */
  fund_basket_escrow(escrowId: bigint, buyer: AddressLike): this {
    return this.push("fund_basket_escrow", [escrowId, buyer]);
  }

  /** Batch `get_basket_tokens(escrowId)` (read-only). */
  get_basket_tokens(escrowId: bigint): this {
    return this.push("get_basket_tokens", [escrowId]);
  }
}

/**
 * Convenience wrapper – creates an {@link EscrowBatch} directly from a
 * transport, without first constructing an {@link EscrowClient}.
 *
 * @example
 * ```ts
 * import { createBatch } from "trustlink-escrow-bindings";
 *
 * const results = await createBatch(myTransport)
 *   .fund_escrow(escrowId, buyer)
 *   .execute();
 * ```
 */
export function createBatch(transport: ContractTransport): EscrowBatch {
  return new EscrowClient(transport).batch();
}
