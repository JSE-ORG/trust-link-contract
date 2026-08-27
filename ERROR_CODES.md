# TrustLink Error Codes Reference

This document provides a comprehensive reference of all `ContractError` variants returned by the TrustLink escrow contract. These numeric codes are part of the stable ABI and are used to diagnose failed transactions.

## Error Codes Table

| Code | Name | Meaning | Resolution |
|:---:|---|---|---|
| **1** | `InvalidAmount` | The supplied amount is zero, negative, or otherwise invalid for the operation. | Ensure the amount is positive and satisfies `MIN_ESCROW_AMOUNT`. |
| **2** | `InsufficientBalance` | The contract vault does not hold enough tokens to complete the transfer or fee withdrawal. | Verify the contract holds sufficient tokens for the requested payout. |
| **3** | `EscrowNotFound` | The requested Escrow ID does not exist in persistent storage. | Re-verify the Escrow ID; it may have been mistyped or never created. |
| **4** | `InvalidState` | The escrow is in a lifecycle state that does not permit the requested action. | Consult the state machine diagram to ensure the action is valid for the current state. |
| **5** | `NotAuthorized` | The caller's address does not have the required permissions for this function. | Ensure you are calling from the correct account (Seller, Buyer, Resolver, or Admin). |
| **6** | `AlreadyInitialized` | The contract has already been initialized with an admin and fee collector. | No action required; initialization can only be performed once per contract deployment. |
| **7** | `FeeExceedsMax` | A fee basis-point value exceeds the configured protocol hard cap. | Supply a fee value within the permitted limits (e.g., <= 3% for escrow fees). |
| **8** | `EscrowHasNoBuyer` | The operation requires an assigned buyer, but the escrow is still in `Pending` state. | The buyer must call `fund_escrow` before this operation can proceed. |
| **9** | `ShippingWindowNotElapsed` | `auto_release` was triggered before the mandatory shipping window elapsed. | Wait until `funded_at + shipping_window` has passed before re-triggering. |
| **10** | `InvalidEvidenceHash` | Reserved. The `evidence_hash` parameter is typed `BytesN<32>`, so a wrong-length digest is rejected by the host before `raise_dispute` runs and this code is never returned today. | Provide a 32-byte SHA-256 digest of the evidence — `hashEvidence()` in `@trustlink/contract-bindings` produces one. |
| **11** | `DisputeNotFound` | No dispute record exists for the requested escrow ID. | Ensure that `raise_dispute` was successfully called before attempting resolution. |
| **12** | `ArithmeticError` | An internal checked arithmetic operation failed (division by zero or general failure). | Review input amounts or contract state for edge-case values. |
| **13** | `DeliveryBeforeDisputeWindow` | `auto_release` was triggered on a `Funded` (never-shipped) escrow before its `dispute_deadline`. (`confirm_delivery` returns `DisputeWindowStillOpen` (24) for the same timing condition.) | Wait for the `dispute_deadline` to pass before re-triggering. |
| **14** | `ContractPaused` | The contract is currently paused by the admin for maintenance or emergency. | Wait for the admin to call `unpause_contract` before attempting the operation again. |
| **15** | `ArithmeticOverflow` | A calculation resulted in a value larger than the supported integer type (i128). | Check if the escrow amount exceeds safety limits; use smaller amounts if possible. |
| **16** | `InvalidStateTransition` | The requested state change is not allowed by the escrow lifecycle rules. | Ensure the action follows the allowed sequence (e.g., Pending -> Funded -> Shipped). |
| **17** | `InputTooLong` | A supplied string (like tracking ID or description) exceeds the maximum length. | Shorten the input string to be within the character limits (e.g., Tracking ID <= 64 chars). |
| **18** | `InvalidAddress` | An address argument is invalid (e.g., it is the zero address or a duplicate role). | Use a valid Stellar address and ensure roles (Admin/FeeCollector) are distinct. |
| **19** | `SameAddress` | A rotation or update was attempted with the same address as the current one. | Supply a different address to perform the update. |
| **20** | `AmountExceedsMaximum` | The escrow amount exceeds the safety-capped `MAX_ESCROW_AMOUNT`. | Reduce the escrow amount to be within the protocol's arithmetic safety bounds. |
| **21** | `InvalidTrackingId` | The tracking ID supplied is empty or improperly formatted. | Provide a non-empty, valid string for the tracking ID. |
| **22** | `DeliveryNotRecorded` | `auto_release` was attempted but the admin has not yet recorded the delivery. | Ensure that delivery is recorded via `record_delivery` if required by the flow. |
| **23** | `ConflictingRoles` | Multiple roles (Seller, Buyer, Resolver) were assigned the same address. | Ensure that the Seller, Buyer, and Resolver are all distinct accounts. |
| **24** | `DisputeWindowStillOpen` | A buyer attempted to confirm delivery while the dispute window is still open. | Wait for the dispute window to close before confirming delivery. |
| **25** | `UnauthorizedResolver` | A resolver is not in the approved registry and strict mode is enabled. | Use an authorized resolver from the approved registry. |
| **26** | `ContractNotPaused` | `emergency_drain` was called but the contract is not paused. | Ensure the contract is paused before calling emergency drain operations. |
| **27** | `TokenNotAllowed` | A token is not in the allowlist and the allowlist is enabled. | Use a token from the approved allowlist or contact the admin to add the token. |
| **28** | `EscrowExpired` | An escrow's pending expiration window has passed. | The escrow can no longer be funded; create a new escrow. |
| **29** | `AmountBelowMinimum` | An escrow amount is below the configured minimum. | Increase the escrow amount to meet the minimum requirement. |
| **30** | `NotPendingFinalization` | An action requires the escrow to be in `PendingFinalization` state. | Ensure the escrow has reached the PendingFinalization state before this action. |
| **31** | `AppealWindowActive` | Finalization is attempted while the appeal window is still active. | Wait for the appeal window to expire before finalizing. |
| **32** | `PlatformFeeExceedsMax` | The platform fee exceeds its allowed maximum. | Configure a platform fee within the allowed limits. |
| **33** | `InvalidShippingWindow` | `shipping_window` is zero or exceeds the maximum allowed value. | Provide a valid shipping window duration within the allowed range. |
| **34** | `DeliveryAlreadyRecorded` | `record_delivery` was called on an escrow that already has delivery recorded. | No action required; delivery has already been recorded for this escrow. |
| **35** | `NotInitialized` | A read accessor was called before the contract has been initialized. | Initialize the contract first by calling `initialize`. |
| **36** | `IndexOutOfBounds` | An internal collection index is out of bounds, indicating a storage or argument invariant violation. | This is an internal error; contact the contract maintainer. |
| **37** | `InvalidExpiration` | A supplied expiration timestamp is not strictly in the future. | Provide an expiration timestamp that is in the future. |
| **38** | `DeliveryNotProposed` | `record_delivery` was called before a delivery proposal was initiated. | Admin must first call `propose_record_delivery` and wait for the timelock. |
| **39** | `TimelockNotElapsed` | `record_delivery` was called before the required 24-hour timelock elapsed. | Wait until 24 hours have passed since `propose_record_delivery` was called. |
| **40** | `GracePeriodNotElapsed` | The grace period has not elapsed yet. | Wait for the grace period to elapse before performing this action. |
| **41** | `MaxAppealsReached` | The maximum number of appeals has been reached. | The dispute can no longer be appealed. |
| **42** | `BasketTokenMismatch` | `create_basket_escrow` was called with mismatched or empty `tokens`/`amounts`. | Ensure `tokens` and `amounts` vectors are non-empty and of equal length. |
| **43** | `InvalidMulticallArg` | A `multicall` argument is missing or fails to decode into the expected type. | Provide valid arguments that match the types expected by the target function. |
| **44** | `PayeeBpsMismatch` | A `Payee` list's basis points do not sum to exactly 10,000 (100%). | Ensure the sum of all `bps` values across payees equals exactly 10,000. |
| **45** | `TooManyMessages` | The maximum number of messages for an escrow has been reached. | No more messages can be attached to this escrow. |
| **46** | `InvalidTtlExtension` | The requested TTL extension is below `MIN_TTL_EXTENSION` (1,000 ledgers). | Supply a TTL extension value of at least `MIN_TTL_EXTENSION`. |
