/**
 * Numeric error codes that the TrustLink escrow contract may return.
 * Values are stable ABI — do NOT renumber.
 */
export declare const enum ErrorCode {
    InvalidAmount = 1,
    InsufficientBalance = 2,
    EscrowNotFound = 3,
    InvalidState = 4,
    NotAuthorized = 5,
    AlreadyInitialized = 6,
    FeeExceedsMax = 7,
    EscrowHasNoBuyer = 8,
    ShippingWindowNotElapsed = 9,
    InvalidEvidenceHash = 10,
    DisputeNotFound = 11,
    ArithmeticError = 12,
    DeliveryBeforeDisputeWindow = 13,
    ContractPaused = 14,
    ArithmeticOverflow = 15,
    InvalidStateTransition = 16,
    InputTooLong = 17,
    InvalidAddress = 18,
    SameAddress = 19,
    AmountExceedsMaximum = 20,
    InvalidTrackingId = 21,
    DeliveryNotRecorded = 22,
    ConflictingRoles = 23,
    DisputeWindowClosed = 24
}
/** Human-readable message for every contract error code. */
export declare const ERROR_MESSAGES: Readonly<Record<ErrorCode, string>>;
/**
 * Typed error thrown by `EscrowClient` and the React hooks when the contract
 * returns a known error code.
 *
 * @example
 * ```ts
 * try {
 *   await client.fund_escrow(id, buyer);
 * } catch (err) {
 *   if (err instanceof ContractInvokeError) {
 *     console.error(err.code, err.message);
 *   }
 * }
 * ```
 */
export declare class ContractInvokeError extends Error {
    readonly code: ErrorCode;
    constructor(code: ErrorCode, message?: string);
}
/**
 * Attempt to parse a raw contract invocation error (from Soroban SDK or
 * Horizon) into a `ContractInvokeError`.
 *
 * Returns `null` when the raw error is not a recognised contract error code.
 */
export declare function parseContractError(raw: unknown): ContractInvokeError | null;
//# sourceMappingURL=errors.d.ts.map