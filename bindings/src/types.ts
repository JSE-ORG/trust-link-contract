/**
 * Shared enums and data interfaces for the TrustLink escrow contract.
 *
 * Every interface here mirrors a `#[contracttype]` struct or `#[contracterror]`
 * enum in `contracts/escrow/src`. Field names match the on-chain layout
 * (snake_case) so a decoded Soroban value maps straight onto these types.
 *
 * These types are maintained by hand. When in doubt about the exact contract
 * surface, {@link contractAbi} (`./abi.ts`) is the authoritative manifest, and
 * `npm run generate` re-derives raw bindings from the compiled Wasm.
 *
 * Value-type conventions:
 * - `AddressLike` — a Stellar/Soroban address as a `G…`/`C…` string.
 * - `bigint` — any `u64` / `i128` on-chain value (ids, amounts, timestamps).
 * - `number` — `u32` values that comfortably fit a JS number (fees in bps,
 *   counts, ledger counts).
 * - `Bytes32` — a fixed 32-byte value (`BytesN<32>`), e.g. an evidence hash.
 *
 * @module types
 */

/** A Stellar/Soroban address in string form (`G…` account or `C…` contract). */
export type AddressLike = string;
/** A Soroban `Symbol` represented as a plain string on the TypeScript side. */
export type ContractSymbol = string;
/** A fixed-width 32-byte value (`BytesN<32>` on chain). */
export type Bytes32 = Uint8Array;
/** A value that is either present or `null` — the TS shape of Rust's `Option<T>`. */
export type Result<T> = T | null;

/**
 * One call descriptor for {@link EscrowClient.multicall} / {@link EscrowBatch}.
 * `function` is the contract method name; `args` are its arguments in the same
 * order the individual client method takes them.
 */
export interface ContractCall {
  function: string;
  args: readonly unknown[];
}

/**
 * Lifecycle state of an escrow.
 *
 * Typical progression:
 * `Pending` → `Funded` → `Shipped` → `Completed`.
 * Branches: any funded state can move to `Disputed` (then back out to
 * `Completed` or `Refunded` on resolution); `Pending`/`Funded` can reach
 * `Canceled`; a buyer refund path ends in `Refunded`.
 */
export enum EscrowState {
  /** Created but not yet funded. Holds no tokens. */
  Pending = "Pending",
  /** Buyer has deposited the amount; the dispute window is open. */
  Funded = "Funded",
  /** Seller has marked the item shipped. */
  Shipped = "Shipped",
  /** Funds released to the payees — terminal. */
  Completed = "Completed",
  /** A dispute is active; normal release/cancel is frozen pending resolution. */
  Disputed = "Disputed",
  /** Funds returned to the buyer — terminal. */
  Refunded = "Refunded",
  /** Cancelled before completion; buyer refunded if it had been funded — terminal. */
  Canceled = "Canceled",
}

/** Whether a dispute record is still open or has been settled. */
export enum DisputeStatus {
  /** Awaiting a resolver decision. */
  Active = "Active",
  /** A resolver has decided; the escrow has moved on. */
  Resolved = "Resolved",
}

/** How a resolver settles a dispute — see {@link EscrowClient.resolve_dispute}. */
export enum ResolutionType {
  /** Pay the escrow out to the payees (seller wins). */
  Release = "Release",
  /** Return the escrow to the buyer (buyer wins). */
  Refund = "Refund",
}

/**
 * Numeric contract error codes.
 *
 * @deprecated This is an incomplete, legacy copy. Use {@link ErrorCode} from
 * `@trustlink/contract-bindings/errors`, which is the full set and is kept in
 * sync with `contracts/escrow/src/errors.rs` by CI. Values below have been
 * corrected to match that file.
 */
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
  ContractPaused = 14,
  InvalidTrackingId = 21,
  EscrowExpired = 28,
}

/**
 * Fee configuration returned by {@link EscrowClient.get_fee_config}.
 * `max_fee_bps` is the hard cap enforced on per-escrow fees (in basis points).
 */
export interface FeeConfig {
  collector: AddressLike;
  max_fee_bps: number;
}

/**
 * A release recipient. `bps` is this payee's share in basis points; the `bps`
 * of every payee on an escrow must sum to exactly `10_000` (100%).
 */
export interface Payee {
  address: AddressLike;
  bps: number;
}

/** One token/amount pair in a basket (multi-token) escrow. See `get_basket_tokens`. */
export interface TokenEntry {
  token: AddressLike;
  amount: bigint;
}

/** The full escrow record, as returned by {@link EscrowClient.get_escrow}. */
export interface EscrowData {
  /** Release recipients with their basis-point shares (sum to 10_000). */
  payees: Payee[];
  /** Fixed buyer, or `null` for an open escrow anyone may fund. */
  buyer: AddressLike | null;
  /** Address authorized to resolve a dispute on this escrow. */
  resolver: AddressLike;
  /** SEP-41 token contract the escrow is denominated in. */
  token: AddressLike;
  /** Escrow amount in the token's smallest unit. */
  amount: bigint;
  /** Platform fee for this escrow, in basis points. */
  fee_bps: number;
  /** Resolver's fee in basis points, charged only on dispute resolution. */
  resolver_fee_bps: number;
  /** Seconds the seller has to ship, measured from funding. */
  shipping_window: bigint;
  /** Ledger timestamp of funding, or `0` while still `Pending`. */
  funded_at: bigint;
  /** Ledger timestamp after which the buyer can no longer raise a dispute. */
  dispute_deadline: bigint;
  /** Ledger timestamp of {@link EscrowClient.mark_shipped}, or `0`. */
  shipped_at: bigint;
  /** Ledger timestamp delivery was recorded, or `null` if not yet recorded. */
  delivered_at: bigint | null;
  /** Carrier tracking id supplied at `mark_shipped`, or `null`. */
  tracking_id: string | null;
  /** Current lifecycle state. */
  state: EscrowState;
  /** Free-text note attached at creation, or `null`. */
  notes: string | null;
}

/** A dispute record, as returned by {@link EscrowClient.get_dispute}. */
export interface DisputeData {
  escrow_id: bigint;
  /** Machine-readable reason tag passed to `raise_dispute` (e.g. `"damaged"`). */
  reason: ContractSymbol;
  /** Free-text explanation (≤ 256 chars). */
  description: string;
  /** 32-byte SHA-256 commitment to off-chain evidence (may be all-zero). */
  evidence_hash: Bytes32;
  status: DisputeStatus;
  /** Ledger timestamp the dispute was raised. */
  disputed_at: bigint;
}

/** A message attached to an escrow thread. */
export interface Message {
  sender: AddressLike;
  timestamp: bigint;
  content: string;
}

/**
 * One entry in a {@link EscrowClient.batch_create_escrow} call. The seller is
 * supplied once for the whole batch, so it is not repeated here.
 */
export interface EscrowInput {
  buyer: AddressLike | null;
  resolver: AddressLike;
  token: AddressLike;
  amount: bigint;
  fee_bps: number;
  shipping_window: bigint;
  notes: string | null;
}

/** Aggregate lifecycle counters returned by `get_stats`. */
export interface ContractStats {
  total_created: bigint;
  total_completed: bigint;
  total_disputed: bigint;
  total_refunded: bigint;
}

/** Public, read-only contract configuration from `get_public_config`. */
export interface PublicContractConfig {
  fee_bps: number;
  arbitration_fee_bps: number;
  paused: boolean;
  escrow_count: bigint;
}

/** Admin-visible contract configuration from `get_contract_config`. */
export interface ContractConfig {
  admin: AddressLike;
  fee_bps: number;
  arbitration_fee_bps: number;
  fee_collector: AddressLike;
  escrow_count: bigint;
}

// ---------------------------------------------------------------------------
// Event type definitions (#370, #594)
// Each interface mirrors its corresponding #[contracttype] struct in events.rs.
// ---------------------------------------------------------------------------

/** Emitted by `set_fee` / legacy fee update path. Topic: "fee_updated" */
export interface FeeUpdated {
  schema_version: number;
  old_fee_bps: number;
  new_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `set_protocol_fee`. Topic: "protocol_fee_updated" */
export interface ProtocolFeeUpdated {
  schema_version: number;
  old_fee_bps: number;
  new_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `set_arbitration_fee`. Topic: "arbitration_fee_updated" */
export interface ArbitrationFeeUpdated {
  schema_version: number;
  old_fee_bps: number;
  new_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `set_admin`. Topic: "admin_rotated" */
export interface AdminRotated {
  schema_version: number;
  old_admin: AddressLike;
  new_admin: AddressLike;
  timestamp: bigint;
}

/** Emitted by `initialize`. Topic: "contract_initialized" */
export interface ContractInitialized {
  schema_version: number;
  admin: AddressLike;
  fee_collector: AddressLike;
  arbitration_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `pause_contract`. Topic: "contract_paused" */
export interface ContractPausedEvent {
  schema_version: number;
  admin: AddressLike;
  timestamp: bigint;
}

/** Emitted by `unpause_contract`. Topic: "contract_unpaused" */
export interface ContractUnpausedEvent {
  schema_version: number;
  admin: AddressLike;
  timestamp: bigint;
}

/** Emitted by `create_escrow`. Topic: "escrow_created" */
export interface EscrowCreated {
  schema_version: number;
  escrow_id: bigint;
  seller: AddressLike;
  resolver: AddressLike;
  token: AddressLike;
  amount: bigint;
  fee_bps: number;
  resolver_fee_bps: number;
  shipping_window: bigint;
  timestamp: bigint;
  new_state: EscrowState;
}

/** Emitted by `fund_escrow`. Topic: "escrow_funded" */
export interface EscrowFunded {
  schema_version: number;
  escrow_id: bigint;
  buyer: AddressLike;
  amount: bigint;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `mark_shipped`. Topic: "escrow_shipped" */
export interface EscrowShipped {
  schema_version: number;
  escrow_id: bigint;
  seller: AddressLike;
  tracking_id: string;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `record_delivery`. Topic: "delivery_recorded" */
export interface DeliveryRecorded {
  schema_version: number;
  escrow_id: bigint;
  delivered_at: bigint;
}

/** Emitted by `confirm_delivery` and `resolve_dispute` (release). Topic: "escrow_completed" */
export interface EscrowCompleted {
  schema_version: number;
  escrow_id: bigint;
  recipient: AddressLike;
  amount: bigint;
  fee_bps: number;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `cancel_escrow` and `auto_cancel_pending`. Topic: "escrow_cancelled" */
export interface EscrowCancelled {
  schema_version: number;
  escrow_id: bigint;
  seller: AddressLike;
  /** Address that actually initiated the cancellation (buyer or a payee/seller). */
  cancelled_by: AddressLike;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `raise_dispute`. Topic: "dispute_raised" */
export interface DisputeRaised {
  schema_version: number;
  escrow_id: bigint;
  buyer: AddressLike;
  reason: ContractSymbol;
  description: string;
  evidence_hash: Bytes32;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `resolve_dispute`. Topic: "dispute_resolved" */
export interface DisputeResolved {
  schema_version: number;
  escrow_id: bigint;
  resolver: AddressLike;
  resolution: ResolutionType;
  recipient: AddressLike;
  amount: bigint;
  arbitration_fee: bigint;
  resolver_fee: bigint;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `auto_release`. Topic: "auto_released" */
export interface AutoReleased {
  schema_version: number;
  escrow_id: bigint;
  seller: AddressLike;
  amount: bigint;
  fee_bps: number;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `withdraw_fees`. Topic: "fees_withdrawn" */
export interface FeesWithdrawn {
  token: AddressLike;
  to: AddressLike;
  amount: bigint;
  timestamp: bigint;
}

/** Emitted by `rotate_resolver`. Topic: "resolver_rotated" */
export interface ResolverRotated {
  schema_version: number;
  escrow_id: bigint;
  old_resolver: AddressLike;
  new_resolver: AddressLike;
  rotated_at: bigint;
}

/** Emitted by `toggle_allowlist`. Topic: "allowlist_toggled" */
export interface AllowlistToggled {
  schema_version: number;
  enabled: boolean;
  timestamp: bigint;
}

/** Emitted by `add_approved_token` / `remove_approved_token`. Topic: "token_allowlist_updated" */
export interface TokenAllowlistUpdated {
  schema_version: number;
  token: AddressLike;
  added: boolean;
  timestamp: bigint;
}

/** Emitted by `create_basket_escrow`. Topic: "basket_escrow_created" */
export interface BasketEscrowCreated {
  schema_version: number;
  escrow_id: bigint;
  seller: AddressLike;
  token_count: number;
  timestamp: bigint;
}

/** Emitted by `post_message`. Topic: "message_posted" */
export interface MessagePosted {
  schema_version: number;
  escrow_id: bigint;
  sender: AddressLike;
  timestamp: bigint;
}

/** Emitted by `request_refund`. Topic: "refund_requested" */
export interface RefundRequested {
  schema_version: number;
  escrow_id: bigint;
  buyer: AddressLike;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `approve_refund`. Topic: "refund_approved" */
export interface RefundApproved {
  schema_version: number;
  escrow_id: bigint;
  seller: AddressLike;
  timestamp: bigint;
  prev_state: EscrowState;
  new_state: EscrowState;
}

/** Emitted by `upgrade`. Topic: "contract_upgraded" */
export interface ContractUpgraded {
  schema_version: number;
  admin: AddressLike;
  new_wasm_hash: Bytes32;
  timestamp: bigint;
}

/** Emitted by `set_platform_fee`. Topic: "platform_fee_updated" */
export interface PlatformFeeUpdated {
  schema_version: number;
  old_fee_bps: number;
  new_fee_bps: number;
  timestamp: bigint;
}

/** Emitted by `set_treasury`. Topic: "treasury_updated" */
export interface TreasuryUpdated {
  schema_version: number;
  old_treasury: AddressLike;
  new_treasury: AddressLike;
  timestamp: bigint;
}

/** Emitted by `resolve_dispute` before finalization window. Topic: "dispute_pending_finalization" */
export interface DisputePendingFinalization {
  schema_version: number;
  escrow_id: bigint;
  resolver: AddressLike;
  resolution: ResolutionType;
  amount: bigint;
  appeal_deadline: bigint;
  pending_at: bigint;
}

/** Emitted by `appeal_dispute`. Topic: "dispute_appealed" */
export interface DisputeAppealed {
  schema_version: number;
  escrow_id: bigint;
  appellant: AddressLike;
  timestamp: bigint;
}

/** Emitted when a multi-resolver casts a vote. Topic: "resolver_vote_recorded" */
export interface ResolverVoteRecorded {
  schema_version: number;
  escrow_id: bigint;
  resolver: AddressLike;
  resolution: ResolutionType;
  vote_count: number;
  threshold: number;
  voted_at: bigint;
}

/** Emitted by `migrate_storage`. Topic: "storage_migrated" */
export interface StorageMigrated {
  schema_version: number;
  admin: AddressLike;
  from_version: number;
  to_version: number;
  timestamp: bigint;
}

/** Emitted by `set_ttl_extension`. Topic: "ttl_extension_updated" */
export interface TtlExtensionUpdated {
  schema_version: number;
  old_ledgers: number;
  new_ledgers: number;
  caller: AddressLike;
  timestamp: bigint;
}

/** Emitted by `set_amount_limits`. Topic: "amount_limits_updated" */
export interface AmountLimitsUpdated {
  schema_version: number;
  old_min_amount: bigint;
  new_min_amount: bigint;
  old_max_amount: bigint;
  new_max_amount: bigint;
  caller: AddressLike;
  timestamp: bigint;
}

/** Emitted by `pause_action`. Topic: "action_paused" */
export interface ActionPaused {
  schema_version: number;
  action: ContractSymbol;
  caller: AddressLike;
  timestamp: bigint;
}

/** Emitted by `unpause_action`. Topic: "action_unpaused" */
export interface ActionUnpaused {
  schema_version: number;
  action: ContractSymbol;
  caller: AddressLike;
  timestamp: bigint;
}

/** Emitted by `add_approved_resolver`. Topic: "resolver_approved" */
export interface ResolverApproved {
  schema_version: number;
  resolver: AddressLike;
  caller: AddressLike;
  timestamp: bigint;
}

/** Emitted by `remove_approved_resolver`. Topic: "resolver_removed" */
export interface ResolverRemoved {
  schema_version: number;
  resolver: AddressLike;
  caller: AddressLike;
  timestamp: bigint;
}

/** Emitted by `set_resolver_strict`. Topic: "resolver_strict_updated" */
export interface ResolverStrictUpdated {
  schema_version: number;
  old_strict: boolean;
  new_strict: boolean;
  caller: AddressLike;
  timestamp: bigint;
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
  | { topic: "resolver_rotated"; data: ResolverRotated }
  | { topic: "allowlist_toggled"; data: AllowlistToggled }
  | { topic: "token_allowlist_updated"; data: TokenAllowlistUpdated }
  | { topic: "basket_escrow_created"; data: BasketEscrowCreated }
  | { topic: "message_posted"; data: MessagePosted }
  | { topic: "refund_requested"; data: RefundRequested }
  | { topic: "refund_approved"; data: RefundApproved }
  | { topic: "contract_upgraded"; data: ContractUpgraded }
  | { topic: "platform_fee_updated"; data: PlatformFeeUpdated }
  | { topic: "treasury_updated"; data: TreasuryUpdated }
  | { topic: "dispute_pending_finalization"; data: DisputePendingFinalization }
  | { topic: "dispute_appealed"; data: DisputeAppealed }
  | { topic: "resolver_vote_recorded"; data: ResolverVoteRecorded }
  | { topic: "storage_migrated"; data: StorageMigrated }
  | { topic: "ttl_extension_updated"; data: TtlExtensionUpdated }
  | { topic: "amount_limits_updated"; data: AmountLimitsUpdated }
  | { topic: "action_paused"; data: ActionPaused }
  | { topic: "action_unpaused"; data: ActionUnpaused }
  | { topic: "resolver_approved"; data: ResolverApproved }
  | { topic: "resolver_removed"; data: ResolverRemoved }
  | { topic: "resolver_strict_updated"; data: ResolverStrictUpdated };
