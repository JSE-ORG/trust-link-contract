import {
  type AddressLike,
  type Bytes32,
  type ContractConfig,
  type ContractStats,
  type ContractCall,
  type ContractSymbol,
  type DisputeData,
  type EscrowData,
  type EscrowInput,
  type FeeConfig,
  type Message,
  type Payee,
  type PublicContractConfig,
  type ResolutionType,
} from "./types.js";

/**
 * Transport abstraction the client delegates every entry-point call to.
 *
 * Implementations decide how `method` + `args` reach the deployed contract
 * (Soroban RPC, a mock, a simulation harness, etc.) and whether the result is
 * returned synchronously or as a `Promise`.
 */
export interface ContractTransport {
  invoke<TReturn>(method: string, args: readonly unknown[]): TReturn | Promise<TReturn>;
}

/** A return value that may resolve synchronously or asynchronously. */
type Call<T> = T | Promise<T>;

/**
 * Fully typed client for the TrustLink escrow contract (#369).
 *
 * Every public contract entry point has a corresponding method with typed
 * parameters and return value, so editors provide intellisense for all params
 * and results. Methods that return `Result<(), ContractError>` on-chain map to
 * `void`; the transport surfaces contract errors as rejected calls.
 */
export class EscrowClient {
  constructor(private readonly transport: ContractTransport) {}

  // ── Lifecycle & administration ──────────────────────────────────────────

  get_version(): Call<number> {
    return this.transport.invoke("get_version", []);
  }

  initialize(
    admin: AddressLike,
    feeCollector: AddressLike,
    arbitrationFeeBps: number,
  ): Call<void> {
    return this.transport.invoke("initialize", [admin, feeCollector, arbitrationFeeBps]);
  }

  set_admin(newAdmin: AddressLike): Call<void> {
    return this.transport.invoke("set_admin", [newAdmin]);
  }

  upgrade(caller: AddressLike, newWasmHash: Bytes32): Call<void> {
    return this.transport.invoke("upgrade", [caller, newWasmHash]);
  }

  // ── Pause controls ──────────────────────────────────────────────────────

  pause_contract(caller: AddressLike): Call<void> {
    return this.transport.invoke("pause_contract", [caller]);
  }

  unpause_contract(caller: AddressLike): Call<void> {
    return this.transport.invoke("unpause_contract", [caller]);
  }

  is_paused(): Call<boolean> {
    return this.transport.invoke("is_paused", []);
  }

  pause_action(caller: AddressLike, action: ContractSymbol): Call<void> {
    return this.transport.invoke("pause_action", [caller, action]);
  }

  unpause_action(caller: AddressLike, action: ContractSymbol): Call<void> {
    return this.transport.invoke("unpause_action", [caller, action]);
  }

  is_action_paused(action: ContractSymbol): Call<boolean> {
    return this.transport.invoke("is_action_paused", [action]);
  }

  // ── Fees ────────────────────────────────────────────────────────────────

  set_fee(caller: AddressLike, feeBps: number): Call<void> {
    return this.transport.invoke("set_fee", [caller, feeBps]);
  }

  set_protocol_fee(caller: AddressLike, feeBps: number): Call<void> {
    return this.transport.invoke("set_protocol_fee", [caller, feeBps]);
  }

  set_arbitration_fee(caller: AddressLike, feeBps: number): Call<void> {
    return this.transport.invoke("set_arbitration_fee", [caller, feeBps]);
  }

  get_arbitration_fee(): Call<number> {
    return this.transport.invoke("get_arbitration_fee", []);
  }

  get_total_arbitration_fees(token: AddressLike): Call<bigint> {
    return this.transport.invoke("get_total_arbitration_fees", [token]);
  }

  set_ttl_extension(caller: AddressLike, ledgers: number): Call<void> {
    return this.transport.invoke("set_ttl_extension", [caller, ledgers]);
  }

  set_fee_collector(newCollector: AddressLike): Call<void> {
    return this.transport.invoke("set_fee_collector", [newCollector]);
  }

  withdraw_fees(
    caller: AddressLike,
    token: AddressLike,
    to: AddressLike,
    amount: bigint,
  ): Call<void> {
    return this.transport.invoke("withdraw_fees", [caller, token, to, amount]);
  }

  get_fee_config(): Call<FeeConfig> {
    return this.transport.invoke("get_fee_config", []);
  initialize(admin: AddressLike, feeCollector: AddressLike, arbitrationFeeBps: number): void | Promise<void> {
    return this.transport.invoke("initialize", [admin, feeCollector, arbitrationFeeBps]);
  }

  pause_contract(caller: AddressLike): void | Promise<void> {
    return this.transport.invoke("pause_contract", [caller]);
  }

  unpause_contract(caller: AddressLike): void | Promise<void> {
    return this.transport.invoke("unpause_contract", [caller]);
  }

  withdraw_fees(caller: AddressLike, token: AddressLike, to: AddressLike, amount: bigint): void | Promise<void> {
    return this.transport.invoke("withdraw_fees", [caller, token, to, amount]);
  }

  get_accumulated_fees(token: AddressLike): Call<bigint> {
    return this.transport.invoke("get_accumulated_fees", [token]);
  }

  // ── Escrow lifecycle ────────────────────────────────────────────────────

  create_escrow(
    payees: readonly Payee[],
    buyer: AddressLike | null,
    resolver: AddressLike,
    token: AddressLike,
    amount: bigint,
    feeBps: number,
    resolverFeeBps: number,
    shippingWindow: bigint,
  ): Call<bigint> {
    return this.transport.invoke("create_escrow", [
      payees,
      buyer,
      resolver,
      token,
      amount,
      feeBps,
      resolverFeeBps,
      shippingWindow,
    ]);
  }

  batch_create_escrow(seller: AddressLike, escrows: readonly EscrowInput[]): Call<bigint[]> {
    return this.transport.invoke("batch_create_escrow", [seller, escrows]);
  ): bigint | Promise<bigint> {
    return this.transport.invoke("create_escrow", [payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow]);
  }

  fund_escrow(escrowId: bigint, buyer: AddressLike): Call<void> {
    return this.transport.invoke("fund_escrow", [escrowId, buyer]);
  }

  mark_shipped(caller: AddressLike, escrowId: bigint, trackingId: string): Call<void> {
    return this.transport.invoke("mark_shipped", [caller, escrowId, trackingId]);
  }

  record_delivery(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("record_delivery", [caller, escrowId]);
  }

  confirm_delivery(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("confirm_delivery", [caller, escrowId]);
  }

  auto_release(escrowId: bigint): Call<void> {
    return this.transport.invoke("auto_release", [escrowId]);
  }

  cancel_escrow(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("cancel_escrow", [caller, escrowId]);
  }

  mutual_cancel(escrowId: bigint): Call<void> {
    return this.transport.invoke("mutual_cancel", [escrowId]);
  }

  request_refund(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("request_refund", [caller, escrowId]);
  }

  approve_refund(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("approve_refund", [caller, escrowId]);
  }

  // ── Disputes ────────────────────────────────────────────────────────────

  raise_dispute(
    caller: AddressLike,
    escrowId: bigint,
    reason: ContractSymbol,
    description: string,
    evidenceHash: Bytes32,
  ): Call<void> {
    return this.transport.invoke("raise_dispute", [
      caller,
      escrowId,
      reason,
      description,
      evidenceHash,
    ]);
  }

  resolve_dispute(
    caller: AddressLike,
    escrowId: bigint,
    resolution: ResolutionType,
  ): Call<void> {
    return this.transport.invoke("resolve_dispute", [caller, escrowId, resolution]);
  }

  rotate_resolver(
    caller: AddressLike,
    escrowId: bigint,
    newResolver: AddressLike,
  ): Call<void> {
    return this.transport.invoke("rotate_resolver", [caller, escrowId, newResolver]);
  }

  // ── Messaging ───────────────────────────────────────────────────────────

  post_message(escrowId: bigint, sender: AddressLike, content: string): Call<void> {
    return this.transport.invoke("post_message", [escrowId, sender, content]);
  }

  get_messages(escrowId: bigint, start: bigint, limit: bigint): Call<Message[]> {
    return this.transport.invoke("get_messages", [escrowId, start, limit]);
  }

  // ── Queries ─────────────────────────────────────────────────────────────

  get_escrow(escrowId: bigint): Call<EscrowData> {
  cancel_escrow(caller: AddressLike, escrowId: bigint): void | Promise<void> {
    return this.transport.invoke("cancel_escrow", [caller, escrowId]);
  }

  get_escrow(escrowId: bigint): EscrowData | Promise<EscrowData> {
    return this.transport.invoke("get_escrow", [escrowId]);
  }

  get_dispute(escrowId: bigint): Call<DisputeData | null> {
    return this.transport.invoke("get_dispute", [escrowId]);
  }

  get_escrows_by_buyer(buyer: AddressLike): Call<bigint[]> {
    return this.transport.invoke("get_escrows_by_buyer", [buyer]);
  }

  get_escrows_by_vendor(vendor: AddressLike): Call<bigint[]> {
    return this.transport.invoke("get_escrows_by_vendor", [vendor]);
  }

  get_stats(): Call<ContractStats> {
    return this.transport.invoke("get_stats", []);
  }

  get_public_config(): Call<PublicContractConfig> {
    return this.transport.invoke("get_public_config", []);
  }

  get_contract_config(): Call<ContractConfig> {
    return this.transport.invoke("get_contract_config", []);
  }

  // ── Limits ──────────────────────────────────────────────────────────────

  set_amount_limits(caller: AddressLike, minAmount: bigint, maxAmount: bigint): Call<void> {
    return this.transport.invoke("set_amount_limits", [caller, minAmount, maxAmount]);
  }

  /**
   * Executes multiple contract calls in a **single transaction**, reducing
   * the total transaction count to 1.
   *
   * Each {@link ContractCall} specifies a function name and its arguments.
   * Results are returned in the same order as the calls.
   *
   * @example
   * ```ts
   * const results = await client.multicall([
   *   { function: "fund_escrow",   args: [escrowId, buyerAddress] },
   *   { function: "mark_shipped",  args: [sellerAddress, escrowId, "TRK-001"] },
   * ]);
   * ```
   */
  multicall(calls: ContractCall[]): unknown[] | Promise<unknown[]> {
    return this.transport.invoke("multicall", [calls]);
  }

  /**
   * Creates a fluent {@link EscrowBatch} builder that accumulates calls and
   * dispatches them in one shot via `multicall`.
   *
   * @example
   * ```ts
   * const results = await client
   *   .batch()
   *   .fund_escrow(escrowId, buyer)
   *   .mark_shipped(seller, escrowId, "TRK-001")
   *   .execute();
   * ```
   */
  batch(): EscrowBatch {
    return new EscrowBatch(this);
  }
}

// ---------------------------------------------------------------------------
// EscrowBatch — fluent builder that collects calls and dispatches via multicall
// ---------------------------------------------------------------------------

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
}

// ---------------------------------------------------------------------------
// Factory helper
// ---------------------------------------------------------------------------

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
