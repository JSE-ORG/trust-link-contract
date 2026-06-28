/**
 * Numeric error codes that the TrustLink escrow contract may return.
 * Values are stable ABI — do NOT renumber.
 */
export const enum ErrorCode {
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
  DisputeWindowClosed = 24,
}

/** Human-readable message for every contract error code. */
export const ERROR_MESSAGES: Readonly<Record<ErrorCode, string>> = {
  [ErrorCode.InvalidAmount]: "Amount must be greater than zero.",
  [ErrorCode.InsufficientBalance]: "Contract does not hold enough tokens for the transfer.",
  [ErrorCode.EscrowNotFound]: "Escrow ID does not exist.",
  [ErrorCode.InvalidState]: "The escrow is not in a valid state for this action.",
  [ErrorCode.NotAuthorized]: "Caller is not authorised to perform this action.",
  [ErrorCode.AlreadyInitialized]: "Contract has already been initialised.",
  [ErrorCode.FeeExceedsMax]: "Fee basis points exceed the configured maximum.",
  [ErrorCode.EscrowHasNoBuyer]: "This action requires an assigned buyer.",
  [ErrorCode.ShippingWindowNotElapsed]: "The shipping window has not elapsed yet.",
  [ErrorCode.InvalidEvidenceHash]: "Evidence hash failed validation.",
  [ErrorCode.DisputeNotFound]: "No dispute record found for this escrow.",
  [ErrorCode.ArithmeticError]: "Arithmetic check failed during payout calculation.",
  [ErrorCode.DeliveryBeforeDisputeWindow]: "Delivery cannot be confirmed before the dispute window opens.",
  [ErrorCode.ContractPaused]: "The contract is currently paused.",
  [ErrorCode.ArithmeticOverflow]: "Arithmetic overflow in payout helper.",
  [ErrorCode.InvalidStateTransition]: "Requested state transition is not part of the approved lifecycle.",
  [ErrorCode.InputTooLong]: "A supplied string or payload exceeds the maximum allowed length.",
  [ErrorCode.InvalidAddress]: "An address argument is invalid for its role.",
  [ErrorCode.SameAddress]: "New value is identical to the current value — no-op update rejected.",
  [ErrorCode.AmountExceedsMaximum]: "Escrow amount exceeds the contract maximum.",
  [ErrorCode.InvalidTrackingId]: "Tracking ID is empty or invalid.",
  [ErrorCode.DeliveryNotRecorded]: "Auto-release attempted before delivery has been recorded.",
  [ErrorCode.ConflictingRoles]: "Two roles that must be distinct have been assigned the same address.",
  [ErrorCode.DisputeWindowClosed]: "The dispute window has closed — disputes are no longer accepted.",
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
  readonly code: ErrorCode;

  constructor(code: ErrorCode, message?: string) {
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
export function parseContractError(raw: unknown): ContractInvokeError | null {
  if (raw instanceof ContractInvokeError) return raw;

  // Soroban SDK surfaces errors as objects with a `code` field or as strings
  // like "Error(Contract, #3)".
  if (raw && typeof raw === "object") {
    const obj = raw as Record<string, unknown>;

    // Stellar SDK: { code: number } shape
    if (typeof obj["code"] === "number") {
      const code = obj["code"] as ErrorCode;
      if (code in ERROR_MESSAGES) return new ContractInvokeError(code);
    }

    // Some adapters wrap the message in `message` string
    if (typeof obj["message"] === "string") {
      const match = (obj["message"] as string).match(/Error\(Contract,\s*#(\d+)\)/);
      if (match) {
        const code = Number(match[1]) as ErrorCode;
        if (code in ERROR_MESSAGES) return new ContractInvokeError(code);
      }
    }
  }

  if (typeof raw === "string") {
    const match = raw.match(/Error\(Contract,\s*#(\d+)\)/);
    if (match) {
      const code = Number(match[1]) as ErrorCode;
      if (code in ERROR_MESSAGES) return new ContractInvokeError(code);
    }
  }

  return null;
}
