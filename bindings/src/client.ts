import {
  type AddressLike,
  type Bytes32,
  type ContractConfig,
  type ContractStats,
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
}
