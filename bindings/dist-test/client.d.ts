import { type AddressLike, type ContractCall, type ContractSymbol, type DisputeData, type EscrowData, type FeeConfig, type Payee, type ResolutionType } from "./types.js";
export interface ContractTransport {
    invoke<TReturn>(method: string, args: readonly unknown[]): TReturn | Promise<TReturn>;
}
export declare class EscrowClient {
    private readonly transport;
    constructor(transport: ContractTransport);
    initialize(admin: AddressLike, feeCollector: AddressLike, arbitrationFeeBps: number): void | Promise<void>;
    pause_contract(caller: AddressLike): void | Promise<void>;
    unpause_contract(caller: AddressLike): void | Promise<void>;
    withdraw_fees(caller: AddressLike, token: AddressLike, to: AddressLike, amount: bigint): void | Promise<void>;
    create_escrow(payees: Payee[], buyer: AddressLike | null, resolver: AddressLike, token: AddressLike, amount: bigint, feeBps: number, resolverFeeBps: number, shippingWindow: bigint): bigint | Promise<bigint>;
    fund_escrow(escrowId: bigint, buyer: AddressLike): void | Promise<void>;
    mark_shipped(caller: AddressLike, escrowId: bigint, trackingId: string): void | Promise<void>;
    confirm_delivery(caller: AddressLike, escrowId: bigint): void | Promise<void>;
    raise_dispute(caller: AddressLike, escrowId: bigint, reason: ContractSymbol, description: string, evidenceHash: Uint8Array): void | Promise<void>;
    resolve_dispute(caller: AddressLike, escrowId: bigint, resolution: ResolutionType): void | Promise<void>;
    auto_release(escrowId: bigint): void | Promise<void>;
    cancel_escrow(caller: AddressLike, escrowId: bigint): void | Promise<void>;
    get_escrow(escrowId: bigint): EscrowData | Promise<EscrowData>;
    get_dispute(escrowId: bigint): DisputeData | null | Promise<DisputeData | null>;
    get_fee_config(): FeeConfig | Promise<FeeConfig>;
    set_arbitration_fee(caller: AddressLike, feeBps: number): void | Promise<void>;
    get_arbitration_fee(): number | Promise<number>;
    rotate_resolver(caller: AddressLike, escrowId: bigint, newResolver: AddressLike): void | Promise<void>;
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
    multicall(calls: ContractCall[]): unknown[] | Promise<unknown[]>;
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
    batch(): EscrowBatch;
}
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
export declare class EscrowBatch {
    private readonly client;
    /** Accumulated call descriptors, built up by the fluent API. */
    private readonly _calls;
    /** @internal Use {@link EscrowClient.batch} instead. */
    constructor(client: EscrowClient);
    private push;
    /**
     * Returns a snapshot of the pending calls (useful for debugging / testing).
     */
    pendingCalls(): readonly ContractCall[];
    /**
     * Dispatches all accumulated calls in a single `multicall` transaction.
     * The returned array contains the decoded return value for each call, in
     * the same order the calls were added.
     */
    execute(): Promise<unknown[]> | unknown[];
    /** Batch `initialize(admin, feeCollector, arbitrationFeeBps)`. */
    initialize(admin: AddressLike, feeCollector: AddressLike, arbitrationFeeBps: number): this;
    /** Batch `pause_contract(caller)`. */
    pause_contract(caller: AddressLike): this;
    /** Batch `unpause_contract(caller)`. */
    unpause_contract(caller: AddressLike): this;
    /** Batch `withdraw_fees(caller, token, to, amount)`. */
    withdraw_fees(caller: AddressLike, token: AddressLike, to: AddressLike, amount: bigint): this;
    /** Batch `create_escrow(payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow)`. */
    create_escrow(payees: Payee[], buyer: AddressLike | null, resolver: AddressLike, token: AddressLike, amount: bigint, feeBps: number, resolverFeeBps: number, shippingWindow: bigint): this;
    /** Batch `fund_escrow(escrowId, buyer)`. */
    fund_escrow(escrowId: bigint, buyer: AddressLike): this;
    /** Batch `mark_shipped(caller, escrowId, trackingId)`. */
    mark_shipped(caller: AddressLike, escrowId: bigint, trackingId: string): this;
    /** Batch `confirm_delivery(caller, escrowId)`. */
    confirm_delivery(caller: AddressLike, escrowId: bigint): this;
    /** Batch `raise_dispute(caller, escrowId, reason, description, evidenceHash)`. */
    raise_dispute(caller: AddressLike, escrowId: bigint, reason: ContractSymbol, description: string, evidenceHash: Uint8Array): this;
    /** Batch `resolve_dispute(caller, escrowId, resolution)`. */
    resolve_dispute(caller: AddressLike, escrowId: bigint, resolution: ResolutionType): this;
    /** Batch `auto_release(escrowId)`. */
    auto_release(escrowId: bigint): this;
    /** Batch `get_escrow(escrowId)` (read-only – safe to include in any batch). */
    get_escrow(escrowId: bigint): this;
    /** Batch `get_dispute(escrowId)` (read-only). */
    get_dispute(escrowId: bigint): this;
    /** Batch `get_fee_config()` (read-only). */
    get_fee_config(): this;
    /** Batch `set_arbitration_fee(caller, feeBps)`. */
    set_arbitration_fee(caller: AddressLike, feeBps: number): this;
    /** Batch `get_arbitration_fee()` (read-only). */
    get_arbitration_fee(): this;
    /** Batch `cancel_escrow(caller, escrowId)`. */
    cancel_escrow(caller: AddressLike, escrowId: bigint): this;
    /** Batch `rotate_resolver(caller, escrowId, newResolver)`. */
    rotate_resolver(caller: AddressLike, escrowId: bigint, newResolver: AddressLike): this;
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
export declare function createBatch(transport: ContractTransport): EscrowBatch;
//# sourceMappingURL=client.d.ts.map