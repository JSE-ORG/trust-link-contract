export type AddressLike = string;
export type ContractSymbol = string;
export type Bytes32 = Uint8Array;
export type Result<T> = T | null;

export enum EscrowState {
  Pending = "Pending",
  Funded = "Funded",
  Shipped = "Shipped",
  Completed = "Completed",
  Disputed = "Disputed",
  Refunded = "Refunded",
  Canceled = "Canceled",
}

export enum DisputeStatus {
  Active = "Active",
  Resolved = "Resolved",
}

export enum ResolutionType {
  Release = "Release",
  Refund = "Refund",
}

export enum ContractError {
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
  ContractPaused = 12,
  InvalidTrackingId = 13,
  EscrowExpired = 25,
}

export interface FeeConfig {
  collector: AddressLike;
  max_fee_bps: number;
}

export interface FeesWithdrawn {
  token: AddressLike;
  to: AddressLike;
  amount: bigint;
  timestamp: bigint;
}

export interface ContractPausedEvent {
  admin: AddressLike;
  timestamp: bigint;
}

export interface ContractUnpausedEvent {
  admin: AddressLike;
  timestamp: bigint;
}

export interface EscrowData {
  seller: AddressLike;
  buyer: AddressLike | null;
  resolver: AddressLike;
  token: AddressLike;
  amount: bigint;
  fee_bps: number;
  shipping_window: bigint;
  funded_at: bigint;
  dispute_deadline: bigint;
  state: EscrowState;
  shipped_at: bigint;
  tracking_id: string | null;
  delivered_at: bigint;
}

export interface DisputeData {
  escrow_id: bigint;
  reason: ContractSymbol;
  description: string;
  evidence_hash: Bytes32;
  status: DisputeStatus;
  disputed_at: bigint;
}

export interface ResolverRotated {
  escrow_id: bigint;
  old_resolver: AddressLike;
  new_resolver: AddressLike;
  rotated_at: bigint;
}

// ---------------------------------------------------------------------------
// Event type definitions (#370)
// Each interface mirrors its corresponding #[contracttype] struct in events.rs.
// ---------------------------------------------------------------------------

/** Emitted by `set_fee` / legacy fee update path. Topic: "fee_updated" */
export interface FeeUpdated {
  old_fee_bps: number;
  new_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `set_protocol_fee`. Topic: "protocol_fee_updated" */
export interface ProtocolFeeUpdated {
  old_fee_bps: number;
  new_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `set_arbitration_fee`. Topic: "arbitration_fee_updated" */
export interface ArbitrationFeeUpdated {
  old_fee_bps: number;
  new_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `set_admin`. Topic: "admin_rotated" */
export interface AdminRotated {
  old_admin: AddressLike;
  new_admin: AddressLike;
  timestamp: bigint;
}

/** Emitted by `initialize`. Topic: "contract_initialized" */
export interface ContractInitialized {
  admin: AddressLike;
  fee_collector: AddressLike;
  arbitration_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `create_escrow`. Topic: "escrow_created" */
export interface EscrowCreated {
  escrow_id: bigint;
  seller: AddressLike;
  resolver: AddressLike;
  token: AddressLike;
  amount: bigint;
  fee_bps: number;
  shipping_window: bigint;
  timestamp: bigint;
}

/** Emitted by `fund_escrow`. Topic: "escrow_funded" */
export interface EscrowFunded {
  escrow_id: bigint;
  buyer: AddressLike;
  amount: bigint;
  funded_at: bigint;
}

/** Emitted by `mark_shipped`. Topic: "escrow_shipped" */
export interface EscrowShipped {
  escrow_id: bigint;
  seller: AddressLike;
  tracking_id: string;
  shipped_at: bigint;
}

/** Emitted by `record_delivery`. Topic: "delivery_recorded" */
export interface DeliveryRecorded {
  escrow_id: bigint;
  delivered_at: bigint;
}

/** Emitted by `confirm_delivery` and `resolve_dispute` (release). Topic: "escrow_completed" */
export interface EscrowCompleted {
  escrow_id: bigint;
  recipient: AddressLike;
  amount: bigint;
  fee_bps: number;
  completed_at: bigint;
}

/** Emitted by `cancel_escrow` and `auto_cancel_pending`. Topic: "escrow_cancelled" */
export interface EscrowCancelled {
  escrow_id: bigint;
  seller: AddressLike;
  cancelled_at: bigint;
}

/** Emitted by `raise_dispute`. Topic: "dispute_raised" */
export interface DisputeRaised {
  escrow_id: bigint;
  buyer: AddressLike;
  reason: ContractSymbol;
  description: string;
  evidence_hash: Bytes32;
  disputed_at: bigint;
}

/** Emitted by `resolve_dispute`. Topic: "dispute_resolved" */
export interface DisputeResolved {
  escrow_id: bigint;
  resolver: AddressLike;
  resolution: ResolutionType;
  recipient: AddressLike;
  amount: bigint;
  arbitration_fee: bigint;
  resolved_at: bigint;
}

/** Emitted by `auto_release`. Topic: "auto_released" */
export interface AutoReleased {
  escrow_id: bigint;
  seller: AddressLike;
  amount: bigint;
  fee_bps: number;
  released_at: bigint;
}

/** Union of all event data payloads keyed by their topic string. */
export type ContractEventPayload =
  | { topic: "fee_updated"; data: FeeUpdated }
  | { topic: "protocol_fee_updated"; data: ProtocolFeeUpdated }
  | { topic: "arbitration_fee_updated"; data: ArbitrationFeeUpdated }
  | { topic: "admin_rotated"; data: AdminRotated }
  | { topic: "contract_initialized"; data: ContractInitialized }
  | { topic: "contract_paused"; data: ContractPausedEvent }
  | { topic: "contract_unpaused"; data: ContractUnpausedEvent }
  | { topic: "escrow_created"; data: EscrowCreated }
  | { topic: "escrow_funded"; data: EscrowFunded }
  | { topic: "escrow_shipped"; data: EscrowShipped }
  | { topic: "delivery_recorded"; data: DeliveryRecorded }
  | { topic: "escrow_completed"; data: EscrowCompleted }
  | { topic: "escrow_cancelled"; data: EscrowCancelled }
  | { topic: "dispute_raised"; data: DisputeRaised }
  | { topic: "dispute_resolved"; data: DisputeResolved }
  | { topic: "auto_released"; data: AutoReleased }
  | { topic: "fees_withdrawn"; data: FeesWithdrawn }
  | { topic: "resolver_rotated"; data: ResolverRotated };
