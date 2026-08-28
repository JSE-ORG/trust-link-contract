/** Human-readable message for every contract error code. */
export const ERROR_MESSAGES = {
    [1 /* ErrorCode.InvalidAmount */]: "Amount must be greater than zero.",
    [2 /* ErrorCode.InsufficientBalance */]: "Contract does not hold enough tokens for the transfer.",
    [3 /* ErrorCode.EscrowNotFound */]: "Escrow ID does not exist.",
    [4 /* ErrorCode.InvalidState */]: "The escrow is not in a valid state for this action.",
    [5 /* ErrorCode.NotAuthorized */]: "Caller is not authorised to perform this action.",
    [6 /* ErrorCode.AlreadyInitialized */]: "Contract has already been initialised.",
    [7 /* ErrorCode.FeeExceedsMax */]: "Fee basis points exceed the configured maximum.",
    [8 /* ErrorCode.EscrowHasNoBuyer */]: "This action requires an assigned buyer.",
    [9 /* ErrorCode.ShippingWindowNotElapsed */]: "The shipping window has not elapsed yet.",
    [10 /* ErrorCode.InvalidEvidenceHash */]: "Evidence hash failed validation.",
    [11 /* ErrorCode.DisputeNotFound */]: "No dispute record found for this escrow.",
    [12 /* ErrorCode.ArithmeticError */]: "Arithmetic check failed during payout calculation.",
    [13 /* ErrorCode.DeliveryBeforeDisputeWindow */]: "Delivery cannot be confirmed before the dispute window opens.",
    [14 /* ErrorCode.ContractPaused */]: "The contract is currently paused.",
    [15 /* ErrorCode.ArithmeticOverflow */]: "Arithmetic overflow in payout helper.",
    [16 /* ErrorCode.InvalidStateTransition */]: "Requested state transition is not part of the approved lifecycle.",
    [17 /* ErrorCode.InputTooLong */]: "A supplied string or payload exceeds the maximum allowed length.",
    [18 /* ErrorCode.InvalidAddress */]: "An address argument is invalid for its role.",
    [19 /* ErrorCode.SameAddress */]: "New value is identical to the current value — no-op update rejected.",
    [20 /* ErrorCode.AmountExceedsMaximum */]: "Escrow amount exceeds the contract maximum.",
    [21 /* ErrorCode.InvalidTrackingId */]: "Tracking ID is empty or invalid.",
    [22 /* ErrorCode.DeliveryNotRecorded */]: "Auto-release attempted before delivery has been recorded.",
    [23 /* ErrorCode.ConflictingRoles */]: "Two roles that must be distinct have been assigned the same address.",
    [24 /* ErrorCode.DisputeWindowClosed */]: "The dispute window has closed — disputes are no longer accepted.",
};
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
export class ContractInvokeError extends Error {
    constructor(code, message) {
        super(message ?? ERROR_MESSAGES[code] ?? `Contract error ${code}`);
        this.name = "ContractInvokeError";
        this.code = code;
    }
}
/**
 * Attempt to parse a raw contract invocation error (from Soroban SDK or
 * Horizon) into a `ContractInvokeError`.
 *
 * Returns `null` when the raw error is not a recognised contract error code.
 */
export function parseContractError(raw) {
    if (raw instanceof ContractInvokeError)
        return raw;
    // Soroban SDK surfaces errors as objects with a `code` field or as strings
    // like "Error(Contract, #3)".
    if (raw && typeof raw === "object") {
        const obj = raw;
        // Stellar SDK: { code: number } shape
        if (typeof obj["code"] === "number") {
            const code = obj["code"];
            if (code in ERROR_MESSAGES)
                return new ContractInvokeError(code);
        }
        // Some adapters wrap the message in `message` string
        if (typeof obj["message"] === "string") {
            const match = obj["message"].match(/Error\(Contract,\s*#(\d+)\)/);
            if (match) {
                const code = Number(match[1]);
                if (code in ERROR_MESSAGES)
                    return new ContractInvokeError(code);
            }
        }
    }
    if (typeof raw === "string") {
        const match = raw.match(/Error\(Contract,\s*#(\d+)\)/);
        if (match) {
            const code = Number(match[1]);
            if (code in ERROR_MESSAGES)
                return new ContractInvokeError(code);
        }
    }
    return null;
}
