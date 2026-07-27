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
| **10** | `InvalidEvidenceHash` | The supplied dispute evidence hash failed validation (e.g., incorrect length). | Provide a valid 32-byte SHA-256 digest of the evidence. |
| **11** | `DisputeNotFound` | No dispute record exists for the requested escrow ID. | Ensure that `raise_dispute` was successfully called before attempting resolution. |
| **12** | `ArithmeticError` | An internal checked arithmetic operation failed (division by zero or general failure). | Review input amounts or contract state for edge-case values. |
| **13** | `DeliveryBeforeDisputeWindow` | Attempted to confirm delivery while the dispute window was still open. | Wait for the `dispute_deadline` to pass before confirming delivery. |
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
| **38** | `DeliveryNotProposed` | `record_delivery` was called before a delivery proposal was initiated. | Admin must first call `propose_record_delivery` and wait for the timelock. |
| **39** | `TimelockNotElapsed` | `record_delivery` was called before the required 24-hour timelock elapsed. | Wait until 24 hours have passed since `propose_record_delivery` was called. |
