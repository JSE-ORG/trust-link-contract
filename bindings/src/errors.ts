/**
 * Numeric error codes that the TrustLink escrow contract may return.
 *
 * This enum is the TypeScript mirror of `ContractError` in
 * contracts/escrow/src/errors.rs.  Names and values must match that file
 * exactly — `scripts/check-error-codes.mjs` enforces this in CI.
 *
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
  DisputeWindowStillOpen = 24,
  UnauthorizedResolver = 25,
  ContractNotPaused = 26,
  TokenNotAllowed = 27,
  EscrowExpired = 28,
  AmountBelowMinimum = 29,
  NotPendingFinalization = 30,
  AppealWindowActive = 31,
  PlatformFeeExceedsMax = 32,
  InvalidShippingWindow = 33,
  DeliveryAlreadyRecorded = 34,
  NotInitialized = 35,
  IndexOutOfBounds = 36,
  InvalidExpiration = 37,
  DeliveryNotProposed = 38,
  TimelockNotElapsed = 39,
  GracePeriodNotElapsed = 40,
  MaxAppealsReached = 41,
  BasketTokenMismatch = 42,
  InvalidMulticallArg = 43,
  PayeeBpsMismatch = 44,
  TooManyMessages = 45,
  InvalidTtlExtension = 46,
  InvalidResolverThreshold = 47,
}

/** Human-readable message for every contract error code. */
export const ERROR_MESSAGES: Readonly<Record<ErrorCode, string>> = {
  [ErrorCode.InvalidAmount]: "Amount must be greater than zero.",
  [ErrorCode.InsufficientBalance]:
    "Contract does not hold enough tokens for the transfer.",
  [ErrorCode.EscrowNotFound]: "Escrow ID does not exist.",
  [ErrorCode.InvalidState]:
    "The escrow is not in a valid state for this action.",
  [ErrorCode.NotAuthorized]: "Caller is not authorised to perform this action.",
  [ErrorCode.AlreadyInitialized]: "Contract has already been initialised.",
  [ErrorCode.FeeExceedsMax]: "Fee basis points exceed the configured maximum.",
  [ErrorCode.EscrowHasNoBuyer]: "This action requires an assigned buyer.",
  [ErrorCode.ShippingWindowNotElapsed]:
    "The shipping window has not elapsed yet.",
  [ErrorCode.InvalidEvidenceHash]: "Evidence hash failed validation.",
  [ErrorCode.DisputeNotFound]: "No dispute record found for this escrow.",
  [ErrorCode.ArithmeticError]:
    "Arithmetic check failed during payout calculation.",
  [ErrorCode.DeliveryBeforeDisputeWindow]:
    "Delivery cannot be confirmed before the dispute window opens.",
  [ErrorCode.ContractPaused]: "The contract is currently paused.",
  [ErrorCode.ArithmeticOverflow]: "Arithmetic overflow in payout helper.",
  [ErrorCode.InvalidStateTransition]:
    "Requested state transition is not part of the approved lifecycle.",
  [ErrorCode.InputTooLong]:
    "A supplied string or payload exceeds the maximum allowed length.",
  [ErrorCode.InvalidAddress]: "An address argument is invalid for its role.",
  [ErrorCode.SameAddress]:
    "New value is identical to the current value — no-op update rejected.",
  [ErrorCode.AmountExceedsMaximum]:
    "Escrow amount exceeds the contract maximum.",
  [ErrorCode.InvalidTrackingId]: "Tracking ID is empty or invalid.",
  [ErrorCode.DeliveryNotRecorded]:
    "Auto-release attempted before delivery has been recorded.",
  [ErrorCode.ConflictingRoles]:
    "Two roles that must be distinct have been assigned the same address.",
  [ErrorCode.DisputeWindowStillOpen]:
    "Delivery cannot be confirmed while the dispute window is still open.",
  [ErrorCode.UnauthorizedResolver]:
    "The resolver is not in the approved registry and strict mode is enabled.",
  [ErrorCode.ContractNotPaused]:
    "emergency_drain requires the contract to be paused first.",
  [ErrorCode.TokenNotAllowed]:
    "The token is not on the allowlist and the allowlist is enabled.",
  [ErrorCode.EscrowExpired]:
    "The escrow's pending expiration window has passed.",
  [ErrorCode.AmountBelowMinimum]:
    "Escrow amount is below the configured minimum.",
  [ErrorCode.NotPendingFinalization]:
    "This action requires the escrow to be in the PendingFinalization state.",
  [ErrorCode.AppealWindowActive]:
    "Finalization is blocked while the appeal window is still active.",
  [ErrorCode.PlatformFeeExceedsMax]:
    "The platform fee exceeds its allowed maximum.",
  [ErrorCode.InvalidShippingWindow]:
    "shipping_window is zero or exceeds the maximum allowed value.",
  [ErrorCode.DeliveryAlreadyRecorded]:
    "Delivery has already been recorded for this escrow.",
  [ErrorCode.NotInitialized]:
    "The contract has not been initialised yet.",
  [ErrorCode.IndexOutOfBounds]:
    "An internal collection index was out of bounds.",
  [ErrorCode.InvalidExpiration]:
    "The supplied expiration timestamp is not strictly in the future.",
  [ErrorCode.DeliveryNotProposed]:
    "Delivery has not been proposed yet.",
  [ErrorCode.TimelockNotElapsed]:
    "The 24-hour timelock delay has not elapsed yet.",
  [ErrorCode.GracePeriodNotElapsed]:
    "The grace period has not elapsed yet.",
  [ErrorCode.MaxAppealsReached]:
    "The maximum number of appeals has been reached.",
  [ErrorCode.BasketTokenMismatch]: "Basket token mismatch.",
  [ErrorCode.InvalidMulticallArg]: "Invalid multicall argument.",
  [ErrorCode.PayeeBpsMismatch]: "Payee bps mismatch.",
  [ErrorCode.TooManyMessages]: "Too many messages.",
  [ErrorCode.InvalidTtlExtension]:
    "The requested TTL extension is below the minimum allowed limit.",
  [ErrorCode.InvalidResolverThreshold]:
    "Multi-resolver threshold is invalid.",
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
      const match = (obj["message"] as string).match(
        /Error\(Contract,\s*#(\d+)\)/
      );
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
