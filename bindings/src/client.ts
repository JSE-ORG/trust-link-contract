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
  type TokenEntry,
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

  /**
   * Returns the semantic version of the deployed contract (`CONTRACT_VERSION`).
   * Read-only; safe to call before {@link initialize}.
   */
  get_version(): Call<number> {
    return this.transport.invoke("get_version", []);
  }

  /**
   * One-time bootstrap. Sets the `admin`, the `feeCollector` that
   * {@link withdraw_fees} pays out to, and the default arbitration fee (in
   * basis points, capped at 5%). `admin` and `feeCollector` must be distinct
   * non-zero addresses.
   *
   * @param admin - Address granted every privileged (admin-only) entry point.
   * @param feeCollector - Address that accumulated protocol/arbitration fees
   *   are withdrawn to. Must differ from `admin`.
   * @param arbitrationFeeBps - Default arbitration fee in basis points
   *   (`100` = 1%); rejected above 500.
   * @throws `AlreadyInitialized` if called twice, `InvalidAddress` for a zero
   *   or duplicate address, `FeeExceedsMax` for an out-of-range fee.
   *
   * @example
   * ```ts
   * await client.initialize(adminAddress, feeCollectorAddress, 250); // 2.5%
   * ```
   */
  initialize(
    admin: AddressLike,
    feeCollector: AddressLike,
    arbitrationFeeBps: number,
  ): Call<void> {
    return this.transport.invoke("initialize", [admin, feeCollector, arbitrationFeeBps]);
  }

  /**
   * Transfers the admin role to `newAdmin`. Admin-only. Takes effect
   * immediately — there is no acceptance step, so double-check the address.
   */
  set_admin(newAdmin: AddressLike): Call<void> {
    return this.transport.invoke("set_admin", [newAdmin]);
  }

  /**
   * Replaces the contract's Wasm with the code identified by `newWasmHash`
   * (a 32-byte hash already uploaded to the network). Admin-only; `caller`
   * must be the current admin and authorize the call.
   *
   * @param caller - The admin address, which must sign the transaction.
   * @param newWasmHash - 32-byte hash of the uploaded replacement Wasm.
   */
  upgrade(caller: AddressLike, newWasmHash: Bytes32): Call<void> {
    return this.transport.invoke("upgrade", [caller, newWasmHash]);
  }

  // ── Pause controls ──────────────────────────────────────────────────────

  /**
   * Globally pauses the contract: every state-changing entry point starts
   * rejecting with `ContractPaused` until {@link unpause_contract}. Read
   * accessors keep working. Admin-only (`caller` must be the admin).
   */
  pause_contract(caller: AddressLike): Call<void> {
    return this.transport.invoke("pause_contract", [caller]);
  }

  /** Lifts a global pause set by {@link pause_contract}. Admin-only. */
  unpause_contract(caller: AddressLike): Call<void> {
    return this.transport.invoke("unpause_contract", [caller]);
  }

  /** True while the contract is globally paused. Read-only. */
  is_paused(): Call<boolean> {
    return this.transport.invoke("is_paused", []);
  }

  /**
   * Pauses a single action by name (e.g. `"RESOLVE"`, `"FUND"`) without
   * pausing the whole contract. Admin-only.
   *
   * @param action - The action symbol to pause; matched against the symbol
   *   the contract checks for that entry point.
   */
  pause_action(caller: AddressLike, action: ContractSymbol): Call<void> {
    return this.transport.invoke("pause_action", [caller, action]);
  }

  /** Re-enables a single action paused by {@link pause_action}. Admin-only. */
  unpause_action(caller: AddressLike, action: ContractSymbol): Call<void> {
    return this.transport.invoke("unpause_action", [caller, action]);
  }

  /** True while the named `action` is individually paused. Read-only. */
  is_action_paused(action: ContractSymbol): Call<boolean> {
    return this.transport.invoke("is_action_paused", [action]);
  }

  // ── Fees ────────────────────────────────────────────────────────────────
  //
  // All fees are expressed in basis points (bps): 100 bps = 1%, 10_000 bps =
  // 100%. Each setter is admin-only and `caller` must be the admin. Caps are
  // enforced on-chain (escrow fee ≤ 3%, arbitration ≤ 5%, platform ≤ 2%,
  // protocol + arbitration combined ≤ 10%) and a violation reverts with
  // `FeeExceedsMax` / `PlatformFeeExceedsMax`.

  /** Sets the default per-escrow fee (bps) applied when a caller omits one. Admin-only. */
  set_fee(caller: AddressLike, feeBps: number): Call<void> {
    return this.transport.invoke("set_fee", [caller, feeBps]);
  }

  /** Sets the protocol fee (bps) retained by the contract on release. Admin-only. */
  set_protocol_fee(caller: AddressLike, feeBps: number): Call<void> {
    return this.transport.invoke("set_protocol_fee", [caller, feeBps]);
  }

  /** Sets the arbitration fee (bps) deducted from an escrow on dispute resolution. Admin-only. */
  set_arbitration_fee(caller: AddressLike, feeBps: number): Call<void> {
    return this.transport.invoke("set_arbitration_fee", [caller, feeBps]);
  }

  /** Current arbitration fee in basis points. Read-only. */
  get_arbitration_fee(): Call<number> {
    return this.transport.invoke("get_arbitration_fee", []);
  }

  /** Lifetime arbitration fees collected in `token`, in that token's smallest unit. Read-only. */
  get_total_arbitration_fees(token: AddressLike): Call<bigint> {
    return this.transport.invoke("get_total_arbitration_fees", [token]);
  }

  /**
   * Sets how far the contract extends storage TTL on each write, in seconds
   * (stored as a ledger-extension value). Admin-only. Guards against entries
   * expiring between interactions on a low-traffic deployment.
   */
  set_ttl_extension(caller: AddressLike, ledgers: number): Call<void> {
    return this.transport.invoke("set_ttl_extension", [caller, ledgers]);
  }

  /** Changes the address {@link withdraw_fees} pays out to. Admin-only. */
  set_fee_collector(newCollector: AddressLike): Call<void> {
    return this.transport.invoke("set_fee_collector", [newCollector]);
  }

  /**
   * Moves `amount` of accumulated `token` fees to `to`. Admin-only; `caller`
   * must be the admin and `to` is typically the configured fee collector.
   *
   * @param amount - Quantity in the token's smallest unit; must not exceed the
   *   accumulated balance or the call reverts with `InsufficientBalance`.
   */
  withdraw_fees(
    caller: AddressLike,
    token: AddressLike,
    to: AddressLike,
    amount: bigint,
  ): Call<void> {
    return this.transport.invoke("withdraw_fees", [caller, token, to, amount]);
  }

  /** Returns the {@link FeeConfig} (collector + fee ceilings). Read-only. */
  get_fee_config(): Call<FeeConfig> {
    return this.transport.invoke("get_fee_config", []);
  }

  /** Withdrawable fee balance currently held in `token`. Read-only. */
  get_accumulated_fees(token: AddressLike): Call<bigint> {
    return this.transport.invoke("get_accumulated_fees", [token]);
  }

  // ── Escrow lifecycle ────────────────────────────────────────────────────
  //
  // Happy path: create_escrow → fund_escrow → mark_shipped → record_delivery →
  // confirm_delivery (or auto_release once the windows elapse). The escrow's
  // {@link EscrowState} advances Pending → Funded → Shipped → Completed; see
  // {@link EscrowData.state}.

  /**
   * Creates a single-token escrow and returns its numeric id.
   *
   * The escrow starts in `Pending` and holds no funds until
   * {@link fund_escrow}. Payee basis points must sum to exactly 10_000.
   *
   * @param payees - One or more recipients of the released amount, each with a
   *   `bps` share summing to 10_000. Pass `[{ address: seller, bps: 10_000 }]`
   *   for the common single-seller case.
   * @param buyer - The address allowed to fund/dispute, or `null` to let any
   *   address fund it (an "open" escrow).
   * @param resolver - Address authorized to resolve a dispute on this escrow.
   * @param token - SEP-41 token contract the escrow is denominated in.
   * @param amount - Escrow amount in the token's smallest unit.
   * @param feeBps - Platform fee for this escrow in basis points (≤ 300).
   * @param resolverFeeBps - Resolver's fee in basis points, paid only if a
   *   dispute is resolved.
   * @param shippingWindow - Seconds the seller has to ship before the buyer /
   *   `auto_release` can act; must be in `[1, 63_072_000]`.
   * @throws `PayeeBpsMismatch`, `InvalidShippingWindow`, `FeeExceedsMax`,
   *   `InvalidAmount`, `ConflictingRoles`, `TokenNotAllowed`.
   *
   * @example
   * ```ts
   * const id = await client.create_escrow(
   *   [{ address: seller, bps: 10_000 }],
   *   buyer, resolver, usdc,
   *   1_000_000n, 100, 50, 172_800n, // 1 USDC, 1% fee, 0.5% resolver fee, 48h
   * );
   * ```
   */
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

  /**
   * Creates several escrows for one `seller` in a single transaction and
   * returns their ids in input order. Each {@link EscrowInput} carries its own
   * buyer/resolver/token/amount.
   */
  batch_create_escrow(seller: AddressLike, escrows: readonly EscrowInput[]): Call<bigint[]> {
    return this.transport.invoke("batch_create_escrow", [seller, escrows]);
  }

  /**
   * Transfers `amount` from `buyer` into the escrow, moving it `Pending →
   * Funded` and opening the 48-hour dispute window. `buyer` must authorize the
   * transfer and, for an escrow created with a fixed buyer, match that address.
   *
   * @throws `EscrowNotFound`, `InvalidState` (not `Pending`), `EscrowExpired`,
   *   `EscrowHasNoBuyer`.
   */
  fund_escrow(escrowId: bigint, buyer: AddressLike): Call<void> {
    return this.transport.invoke("fund_escrow", [escrowId, buyer]);
  }

  /**
   * Seller records shipment with a carrier tracking id, moving the escrow
   * `Funded → Shipped`. `caller` must be a payee/seller on the escrow.
   *
   * @param trackingId - Non-empty carrier reference, ≤ 64 characters
   *   (`InvalidTrackingId` / `InputTooLong` otherwise).
   */
  mark_shipped(caller: AddressLike, escrowId: bigint, trackingId: string): Call<void> {
    return this.transport.invoke("mark_shipped", [caller, escrowId, trackingId]);
  }

  /**
   * Records that delivery occurred, starting the delivery-release window that
   * {@link auto_release} depends on. `caller` is typically the seller or
   * resolver.
   */
  record_delivery(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("record_delivery", [caller, escrowId]);
  }

  /**
   * Buyer accepts delivery and releases the held funds to the payees (minus
   * fees), moving the escrow to `Completed`. Only callable once the dispute
   * window has closed; reverts with `DisputeWindowStillOpen` otherwise.
   *
   * @param caller - The buyer address, which must authorize the call.
   */
  confirm_delivery(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("confirm_delivery", [caller, escrowId]);
  }

  /**
   * Permissionlessly releases funds to the payees once delivery was recorded
   * and the delivery-release window has elapsed with no dispute. Anyone may
   * call it (e.g. a keeper bot).
   *
   * @throws `DeliveryNotRecorded`, `ShippingWindowNotElapsed`, `InvalidState`.
   */
  auto_release(escrowId: bigint): Call<void> {
    return this.transport.invoke("auto_release", [escrowId]);
  }

  /**
   * Cancels a `Pending` (unfunded) escrow, or a `Funded` one under the
   * contract's cancel rules, refunding the buyer if funded. `caller` must be a
   * party to the escrow.
   *
   * @throws `NotAuthorized`, `InvalidState`.
   */
  cancel_escrow(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("cancel_escrow", [caller, escrowId]);
  }

  /**
   * Cancels a funded escrow when buyer and seller both agree. Requires
   * authorization from both parties within the same transaction; refunds the
   * buyer in full.
   */
  mutual_cancel(escrowId: bigint): Call<void> {
    return this.transport.invoke("mutual_cancel", [escrowId]);
  }

  /**
   * Buyer asks the seller to approve a refund, moving the escrow toward
   * `Refunded`. Pairs with {@link approve_refund}.
   */
  request_refund(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("request_refund", [caller, escrowId]);
  }

  /**
   * Seller approves a pending {@link request_refund}, returning the funds to
   * the buyer and moving the escrow to `Refunded`.
   */
  approve_refund(caller: AddressLike, escrowId: bigint): Call<void> {
    return this.transport.invoke("approve_refund", [caller, escrowId]);
  }

  // ── Basket (multi-token) escrows ────────────────────────────────────────

  /**
   * Creates an escrow that pays out multiple tokens to a single seller
   * instead of the single-token flow used by `create_escrow`. Must be funded
   * with {@link fund_basket_escrow}, not `fund_escrow`.
   *
   * @example
   * ```ts
   * const escrowId = await client.create_basket_escrow(
   *   seller, buyer, resolver,
   *   [usdcAddress, xlmAddress], [1_000_000n, 500_000n],
   *   feeBps, shippingWindow,
   * );
   * ```
   */
  create_basket_escrow(
    seller: AddressLike,
    buyer: AddressLike | null,
    resolver: AddressLike,
    tokens: readonly AddressLike[],
    amounts: readonly bigint[],
    feeBps: number,
    shippingWindow: bigint,
  ): Call<bigint> {
    return this.transport.invoke("create_basket_escrow", [
      seller,
      buyer,
      resolver,
      tokens,
      amounts,
      feeBps,
      shippingWindow,
    ]);
  }

  /** Funds a basket escrow by transferring every configured token from `buyer`. */
  fund_basket_escrow(escrowId: bigint, buyer: AddressLike): Call<void> {
    return this.transport.invoke("fund_basket_escrow", [escrowId, buyer]);
  }

  /** Returns the token/amount entries stored for a basket escrow (empty for a non-basket escrow). */
  get_basket_tokens(escrowId: bigint): Call<TokenEntry[]> {
    return this.transport.invoke("get_basket_tokens", [escrowId]);
  }

  // ── Disputes ────────────────────────────────────────────────────────────

  /**
   * Buyer opens a dispute on a `Funded`/`Shipped` escrow before the dispute
   * deadline, moving it to `Disputed` and freezing normal release/cancel.
   *
   * @param caller - The buyer address, which must authorize the call.
   * @param reason - Short machine-readable tag (e.g. `"not_received"`,
   *   `"damaged"`); stored as a Soroban symbol.
   * @param description - Free-text explanation, ≤ 256 characters.
   * @param evidenceHash - 32-byte SHA-256 commitment to off-chain evidence.
   *   Use {@link hashEvidence} to build it, or `EMPTY_EVIDENCE_HASH` for none.
   * @throws `InvalidState`, `DisputeWindowStillOpen` / deadline errors,
   *   `InvalidEvidenceHash`, `InputTooLong`.
   *
   * @example
   * ```ts
   * import { hashEvidence } from "@trustlink/contract-bindings/evidence";
   * const evidence = await hashEvidence(await photo.arrayBuffer());
   * await client.raise_dispute(buyer, id, "damaged", "Arrived cracked", evidence);
   * ```
   */
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

  /**
   * Resolver settles an active dispute. `resolution` of
   * {@link ResolutionType.Release} pays the payees; {@link ResolutionType.Refund}
   * returns funds to the buyer. Arbitration and resolver fees are deducted
   * either way.
   *
   * @param caller - The escrow's resolver (or, for a multi-resolver escrow, one
   *   of them — this records a vote). Must authorize the call.
   * @throws `NotAuthorized` (wrong resolver / strict mode), `InvalidState`
   *   (no active dispute), `DisputeNotFound`.
   */
  resolve_dispute(
    caller: AddressLike,
    escrowId: bigint,
    resolution: ResolutionType,
  ): Call<void> {
    return this.transport.invoke("resolve_dispute", [caller, escrowId, resolution]);
  }

  /**
   * Replaces the resolver assigned to `escrowId` with `newResolver`. `caller`
   * must be authorized to rotate (admin or the current resolver, per contract
   * rules). Emits `resolver_rotated`.
   */
  rotate_resolver(
    caller: AddressLike,
    escrowId: bigint,
    newResolver: AddressLike,
  ): Call<void> {
    return this.transport.invoke("rotate_resolver", [caller, escrowId, newResolver]);
  }

  // ── Messaging ───────────────────────────────────────────────────────────

  /**
   * Appends a message to the escrow's on-chain thread. `sender` must be a
   * party to the escrow and authorize the call.
   *
   * @param content - Message body, ≤ 500 characters. An escrow tops out at
   *   100 stored messages (`TooManyMessages` after that).
   */
  post_message(escrowId: bigint, sender: AddressLike, content: string): Call<void> {
    return this.transport.invoke("post_message", [escrowId, sender, content]);
  }

  /**
   * Reads a page of the escrow's message thread. Read-only.
   *
   * @param start - Zero-based index of the first message to return.
   * @param limit - Page size requested; the contract caps each page at 50.
   */
  get_messages(escrowId: bigint, start: bigint, limit: bigint): Call<Message[]> {
    return this.transport.invoke("get_messages", [escrowId, start, limit]);
  }

  // ── Queries ─────────────────────────────────────────────────────────────

  /**
   * Full {@link EscrowData} record for `escrowId`. Read-only.
   * @throws `EscrowNotFound` for an unknown id.
   */
  get_escrow(escrowId: bigint): Call<EscrowData> {
    return this.transport.invoke("get_escrow", [escrowId]);
  }

  /** {@link DisputeData} for `escrowId`, or `null` if no dispute was ever raised. Read-only. */
  get_dispute(escrowId: bigint): Call<DisputeData | null> {
    return this.transport.invoke("get_dispute", [escrowId]);
  }

  /** Ids of every escrow where `buyer` is the buyer. Read-only. */
  get_escrows_by_buyer(buyer: AddressLike): Call<bigint[]> {
    return this.transport.invoke("get_escrows_by_buyer", [buyer]);
  }

  /** Ids of every escrow where `vendor` (seller/payee) is a recipient. Read-only. */
  get_escrows_by_vendor(vendor: AddressLike): Call<bigint[]> {
    return this.transport.invoke("get_escrows_by_vendor", [vendor]);
  }

  /** Aggregate lifecycle counters ({@link ContractStats}). Read-only. */
  get_stats(): Call<ContractStats> {
    return this.transport.invoke("get_stats", []);
  }

  /** Non-sensitive config anyone may read ({@link PublicContractConfig}). Read-only. */
  get_public_config(): Call<PublicContractConfig> {
    return this.transport.invoke("get_public_config", []);
  }

  /** Full config including admin/fee-collector ({@link ContractConfig}). Read-only. */
  get_contract_config(): Call<ContractConfig> {
    return this.transport.invoke("get_contract_config", []);
  }

  // ── Limits ──────────────────────────────────────────────────────────────

  /**
   * Sets the inclusive `[minAmount, maxAmount]` range accepted by
   * {@link create_escrow}. Admin-only. Amounts outside the range later revert
   * with `AmountBelowMinimum` / `AmountExceedsMaximum`.
   */
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

// ---------------------------------------------------------------------------
// Factory helper
// ---------------------------------------------------------------------------

/**
 * Convenience wrapper – creates an {@link EscrowBatch} directly from a
 * transport, without first constructing an {@link EscrowClient}.
 *
 * @example
 * ```ts
 * import { createBatch } from "@trustlink/contract-bindings";
 *
 * const results = await createBatch(myTransport)
 *   .fund_escrow(escrowId, buyer)
 *   .execute();
 * ```
 */
export function createBatch(transport: ContractTransport): EscrowBatch {
  return new EscrowClient(transport).batch();
}
