export class EscrowClient {
    constructor(transport) {
        this.transport = transport;
    }
    initialize(admin, feeCollector, arbitrationFeeBps) {
        return this.transport.invoke("initialize", [admin, feeCollector, arbitrationFeeBps]);
    }
    pause_contract(caller) {
        return this.transport.invoke("pause_contract", [caller]);
    }
    unpause_contract(caller) {
        return this.transport.invoke("unpause_contract", [caller]);
    }
    withdraw_fees(caller, token, to, amount) {
        return this.transport.invoke("withdraw_fees", [caller, token, to, amount]);
    }
    create_escrow(payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow) {
        return this.transport.invoke("create_escrow", [payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow]);
    }
    fund_escrow(escrowId, buyer) {
        return this.transport.invoke("fund_escrow", [escrowId, buyer]);
    }
    mark_shipped(caller, escrowId, trackingId) {
        return this.transport.invoke("mark_shipped", [caller, escrowId, trackingId]);
    }
    confirm_delivery(caller, escrowId) {
        return this.transport.invoke("confirm_delivery", [caller, escrowId]);
    }
    raise_dispute(caller, escrowId, reason, description, evidenceHash) {
        return this.transport.invoke("raise_dispute", [caller, escrowId, reason, description, evidenceHash]);
    }
    resolve_dispute(caller, escrowId, resolution) {
        return this.transport.invoke("resolve_dispute", [caller, escrowId, resolution]);
    }
    auto_release(escrowId) {
        return this.transport.invoke("auto_release", [escrowId]);
    }
    cancel_escrow(caller, escrowId) {
        return this.transport.invoke("cancel_escrow", [caller, escrowId]);
    }
    get_escrow(escrowId) {
        return this.transport.invoke("get_escrow", [escrowId]);
    }
    get_dispute(escrowId) {
        return this.transport.invoke("get_dispute", [escrowId]);
    }
    get_fee_config() {
        return this.transport.invoke("get_fee_config", []);
    }
    set_arbitration_fee(caller, feeBps) {
        return this.transport.invoke("set_arbitration_fee", [caller, feeBps]);
    }
    get_arbitration_fee() {
        return this.transport.invoke("get_arbitration_fee", []);
    }
    rotate_resolver(caller, escrowId, newResolver) {
        return this.transport.invoke("rotate_resolver", [caller, escrowId, newResolver]);
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
    multicall(calls) {
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
    batch() {
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
    /** @internal Use {@link EscrowClient.batch} instead. */
    constructor(client) {
        this.client = client;
        /** Accumulated call descriptors, built up by the fluent API. */
        this._calls = [];
    }
    // ---- helpers --------------------------------------------------------------
    push(fn, args) {
        this._calls.push({ function: fn, args });
        return this;
    }
    /**
     * Returns a snapshot of the pending calls (useful for debugging / testing).
     */
    pendingCalls() {
        return this._calls;
    }
    /**
     * Dispatches all accumulated calls in a single `multicall` transaction.
     * The returned array contains the decoded return value for each call, in
     * the same order the calls were added.
     */
    execute() {
        return this.client.multicall([...this._calls]);
    }
    // ---- call builders -------------------------------------------------------
    /** Batch `initialize(admin, feeCollector, arbitrationFeeBps)`. */
    initialize(admin, feeCollector, arbitrationFeeBps) {
        return this.push("initialize", [admin, feeCollector, arbitrationFeeBps]);
    }
    /** Batch `pause_contract(caller)`. */
    pause_contract(caller) {
        return this.push("pause_contract", [caller]);
    }
    /** Batch `unpause_contract(caller)`. */
    unpause_contract(caller) {
        return this.push("unpause_contract", [caller]);
    }
    /** Batch `withdraw_fees(caller, token, to, amount)`. */
    withdraw_fees(caller, token, to, amount) {
        return this.push("withdraw_fees", [caller, token, to, amount]);
    }
    /** Batch `create_escrow(payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow)`. */
    create_escrow(payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow) {
        return this.push("create_escrow", [payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow]);
    }
    /** Batch `fund_escrow(escrowId, buyer)`. */
    fund_escrow(escrowId, buyer) {
        return this.push("fund_escrow", [escrowId, buyer]);
    }
    /** Batch `mark_shipped(caller, escrowId, trackingId)`. */
    mark_shipped(caller, escrowId, trackingId) {
        return this.push("mark_shipped", [caller, escrowId, trackingId]);
    }
    /** Batch `confirm_delivery(caller, escrowId)`. */
    confirm_delivery(caller, escrowId) {
        return this.push("confirm_delivery", [caller, escrowId]);
    }
    /** Batch `raise_dispute(caller, escrowId, reason, description, evidenceHash)`. */
    raise_dispute(caller, escrowId, reason, description, evidenceHash) {
        return this.push("raise_dispute", [caller, escrowId, reason, description, evidenceHash]);
    }
    /** Batch `resolve_dispute(caller, escrowId, resolution)`. */
    resolve_dispute(caller, escrowId, resolution) {
        return this.push("resolve_dispute", [caller, escrowId, resolution]);
    }
    /** Batch `auto_release(escrowId)`. */
    auto_release(escrowId) {
        return this.push("auto_release", [escrowId]);
    }
    /** Batch `get_escrow(escrowId)` (read-only – safe to include in any batch). */
    get_escrow(escrowId) {
        return this.push("get_escrow", [escrowId]);
    }
    /** Batch `get_dispute(escrowId)` (read-only). */
    get_dispute(escrowId) {
        return this.push("get_dispute", [escrowId]);
    }
    /** Batch `get_fee_config()` (read-only). */
    get_fee_config() {
        return this.push("get_fee_config", []);
    }
    /** Batch `set_arbitration_fee(caller, feeBps)`. */
    set_arbitration_fee(caller, feeBps) {
        return this.push("set_arbitration_fee", [caller, feeBps]);
    }
    /** Batch `get_arbitration_fee()` (read-only). */
    get_arbitration_fee() {
        return this.push("get_arbitration_fee", []);
    }
    /** Batch `cancel_escrow(caller, escrowId)`. */
    cancel_escrow(caller, escrowId) {
        return this.push("cancel_escrow", [caller, escrowId]);
    }
    /** Batch `rotate_resolver(caller, escrowId, newResolver)`. */
    rotate_resolver(caller, escrowId, newResolver) {
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
export function createBatch(transport) {
    return new EscrowClient(transport).batch();
}
