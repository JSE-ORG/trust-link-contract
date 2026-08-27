use soroban_sdk::contracterror;

/// Stable contract error codes for the escrow lifecycle.
///
/// The numeric values are part of the public ABI and must not be renumbered.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// Returned when an amount is zero, negative, or otherwise invalid for the requested operation.
    InvalidAmount = 1,
    /// Returned when the contract does not hold enough tokens for the requested transfer or withdrawal.
    InsufficientBalance = 2,
    /// Returned when the requested escrow id does not exist in storage.
    EscrowNotFound = 3,
    /// Returned when the escrow is in a state that does not permit the requested action.
    InvalidState = 4,
    /// Returned when the caller is not authorized to perform the requested privileged action.
    NotAuthorized = 5,
    /// Returned when `initialize` is called after the contract has already been initialized.
    AlreadyInitialized = 6,
    /// Returned when a fee basis-point value exceeds the configured hard cap.
    FeeExceedsMax = 7,
    /// Returned when an action requires an assigned buyer but the escrow has none yet.
    EscrowHasNoBuyer = 8,
    /// Returned when auto-release is attempted before the configured shipping window has elapsed.
    ShippingWindowNotElapsed = 9,
    /// Reserved. The `evidence_hash` parameter is typed `BytesN<32>`, so a
    /// wrong-length digest is rejected by the host before `raise_dispute` runs
    /// and this code is never returned today. Kept for future use if content
    /// validation is added.
    InvalidEvidenceHash = 10,
    /// Returned when a dispute record is missing for the requested escrow.
    DisputeNotFound = 11,
    /// Returned when internal checked arithmetic fails while computing a payout or fee.
    ArithmeticError = 12,
    /// Returned when delivery is attempted before the dispute window opens.
    DeliveryBeforeDisputeWindow = 13,
    /// Returned when a contract action is blocked because the contract is paused.
    ContractPaused = 14,
    /// Returned when checked arithmetic overflows in helper payout calculations.
    ArithmeticOverflow = 15,
    /// Returned when a state transition is not part of the approved lifecycle matrix.
    InvalidStateTransition = 16,
    /// Returned when a supplied string or payload exceeds the supported length.
    InputTooLong = 17,
    /// Returned when an address argument is invalid for its role.
    InvalidAddress = 18,
    /// Returned when an update is a no-op because the new value equals the current one.
    SameAddress = 19,
    /// Returned when an escrow amount exceeds the maximum allowed limit.
    AmountExceedsMaximum = 20,
    /// Returned when a tracking ID is empty or otherwise invalid for shipment.
    InvalidTrackingId = 21,
    /// Returned when auto-release is attempted before delivery has been recorded.
    DeliveryNotRecorded = 22,
    /// Returned when two roles that must be distinct are assigned the same address.
    ConflictingRoles = 23,
    /// Returned when a buyer attempts to confirm delivery while the dispute window is still open.
    DisputeWindowStillOpen = 24,
    /// Returned when a resolver is not in the approved registry and strict mode is enabled.
    UnauthorizedResolver = 25,
    /// Returned when emergency_drain is called but the contract is not paused.
    ContractNotPaused = 26,
    /// Returned when a token is not in the allowlist and the allowlist is enabled.
    TokenNotAllowed = 27,
    /// Returned when an escrow's pending expiration window has passed.
    EscrowExpired = 28,
    /// Returned when an escrow amount is below the configured minimum.
    AmountBelowMinimum = 29,
    /// Returned when an action requires the escrow to be in PendingFinalization state.
    NotPendingFinalization = 30,
    /// Returned when finalization is attempted while the appeal window is still active.
    AppealWindowActive = 31,
    /// Returned when the platform fee exceeds its allowed maximum.
    PlatformFeeExceedsMax = 32,
    /// Returned when shipping_window is zero or exceeds the maximum allowed value.
    InvalidShippingWindow = 33,
    /// Returned when `record_delivery` is called on an escrow that already has delivery recorded.
    DeliveryAlreadyRecorded = 34,
    /// Returned when a read accessor is called before the contract has been initialized.
    NotInitialized = 35,
    /// Returned when an internal collection index is out of bounds, indicating a
    /// storage or argument invariant was violated.
    IndexOutOfBounds = 36,
    /// Returned when a supplied expiration timestamp is not strictly in the future.
    InvalidExpiration = 37,
    /// Returned when `record_delivery` is called before a delivery proposal is initiated.
    DeliveryNotProposed = 38,
    /// Returned when `record_delivery` is called before the required timelock delay has elapsed.
    TimelockNotElapsed = 39,
    /// Returned when `reclaim_expired` is called after `expires_at` but before
    /// `expires_at + grace_period` has elapsed.
    GracePeriodNotElapsed = 40,
    /// Returned when the maximum number of dispute appeals has been reached.
    MaxAppealsReached = 41,
    /// Returned when `create_basket_escrow` is called with mismatched or empty `tokens`/`amounts`.
    BasketTokenMismatch = 42,
    /// Returned when a `multicall` call argument is missing or fails to decode into the expected type.
    InvalidMulticallArg = 43,
    /// Returned when a `Payee` list's basis points do not sum to exactly `BASIS_POINTS` (10_000).
    PayeeBpsMismatch = 44,
    /// Returned when the maximum number of messages for an escrow has been reached.
    TooManyMessages = 45,
}
