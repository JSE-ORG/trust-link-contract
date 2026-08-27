import {
  type AddressLike,
  type ContractCall,
  type ContractSymbol,
  type Payee,
  type ResolutionType,
  type TokenEntry,
} from "./types.js";
import { type ContractTransport, EscrowClient } from "./client.js";

/**
 * A fluent builder for batching multiple escrow contract calls into a single
 * Stellar transaction via the `multicall` entry-point.
 *
 * Use {@link EscrowClient.batch} to obtain an instance.  Chain any number of
 * call methods then call {@link execute} to dispatch.
 */
export class EscrowBatch {
  /** Accumulated call descriptors, built up by the fluent API. */
  private readonly _calls: ContractCall[] = [];

  /** @internal Use {@link EscrowClient.batch} instead. */
  constructor(private readonly client: EscrowClient) {}

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

  initialize(admin: AddressLike, feeCollector: AddressLike, arbitrationFeeBps: number): this {
    return this.push("initialize", [admin, feeCollector, arbitrationFeeBps]);
  }

  pause_contract(caller: AddressLike): this {
    return this.push("pause_contract", [caller]);
  }

  unpause_contract(caller: AddressLike): this {
    return this.push("unpause_contract", [caller]);
  }

  withdraw_fees(caller: AddressLike, token: AddressLike, to: AddressLike, amount: bigint): this {
    return this.push("withdraw_fees", [caller, token, to, amount]);
  }

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

  fund_escrow(escrowId: bigint, buyer: AddressLike): this {
    return this.push("fund_escrow", [escrowId, buyer]);
  }

  mark_shipped(caller: AddressLike, escrowId: bigint, trackingId: string): this {
    return this.push("mark_shipped", [caller, escrowId, trackingId]);
  }

  confirm_delivery(caller: AddressLike, escrowId: bigint): this {
    return this.push("confirm_delivery", [caller, escrowId]);
  }

  raise_dispute(
    caller: AddressLike,
    escrowId: bigint,
    reason: ContractSymbol,
    description: string,
    evidenceHash: Uint8Array,
  ): this {
    return this.push("raise_dispute", [caller, escrowId, reason, description, evidenceHash]);
  }

  resolve_dispute(caller: AddressLike, escrowId: bigint, resolution: ResolutionType): this {
    return this.push("resolve_dispute", [caller, escrowId, resolution]);
  }

  auto_release(escrowId: bigint): this {
    return this.push("auto_release", [escrowId]);
  }

  get_escrow(escrowId: bigint): this {
    return this.push("get_escrow", [escrowId]);
  }

  get_dispute(escrowId: bigint): this {
    return this.push("get_dispute", [escrowId]);
  }

  get_fee_config(): this {
    return this.push("get_fee_config", []);
  }

  set_arbitration_fee(caller: AddressLike, feeBps: number): this {
    return this.push("set_arbitration_fee", [caller, feeBps]);
  }

  get_arbitration_fee(): this {
    return this.push("get_arbitration_fee", []);
  }

  cancel_escrow(caller: AddressLike, escrowId: bigint): this {
    return this.push("cancel_escrow", [caller, escrowId]);
  }

  rotate_resolver(caller: AddressLike, escrowId: bigint, newResolver: AddressLike): this {
    return this.push("rotate_resolver", [caller, escrowId, newResolver]);
  }

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

  fund_basket_escrow(escrowId: bigint, buyer: AddressLike): this {
    return this.push("fund_basket_escrow", [escrowId, buyer]);
  }

  get_basket_tokens(escrowId: bigint): this {
    return this.push("get_basket_tokens", [escrowId]);
  }
}

/**
 * Convenience wrapper – creates an {@link EscrowBatch} directly from a
 * transport, without first constructing an {@link EscrowClient}.
 */
export function createBatch(transport: ContractTransport): EscrowBatch {
  return new EscrowClient(transport).batch();
}
