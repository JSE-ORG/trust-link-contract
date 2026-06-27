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

export interface Payee {
  address: AddressLike;
  bps: number;
}

export interface EscrowData {
  payees: Payee[];
  buyer: AddressLike | null;
  resolver: AddressLike;
  token: AddressLike;
  amount: bigint;
  fee_bps: number;
  resolver_fee_bps: number;
  shipping_window: bigint;
  funded_at: bigint;
  dispute_deadline: bigint;
  shipped_at: bigint;
  delivered_at: bigint | null;
  tracking_id: string | null;
  state: EscrowState;
  notes: string | null;
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

/**
 * A single call descriptor for the {@link EscrowBatch} / `multicall` entry-point.
 * `function` is the on-chain method name; `args` are its Soroban-native
 * arguments in the order expected by the contract.
 */
export interface ContractCall {
  /** The exact name of the contract function to invoke. */
  function: string;
  /** Ordered arguments for the function (Soroban-native types). */
  args: readonly unknown[];
}
