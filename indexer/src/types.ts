/**
 * Shared types for the TrustLink escrow indexer.
 *
 * RawEvent is the normalized wire format consumed by the ingester — produced
 * either by the Soroban RPC adapter (live mode) or loaded from a fixture file
 * (replay mode).  Payload shapes mirror the #[contracttype] structs in
 * contracts/escrow/src/events.rs.
 */

// ---------------------------------------------------------------------------
// Position / cursor
// ---------------------------------------------------------------------------

export interface Cursor {
  ledger_sequence: number;
  tx_index: number;
  event_index: number;
}

/** Returns true when `a` is strictly after `b` in event-stream order. */
export function cursorAfter(a: Cursor, b: Cursor): boolean {
  if (a.ledger_sequence !== b.ledger_sequence) return a.ledger_sequence > b.ledger_sequence;
  if (a.tx_index !== b.tx_index) return a.tx_index > b.tx_index;
  return a.event_index > b.event_index;
}

// ---------------------------------------------------------------------------
// Raw event (normalized; topics already decoded to strings)
// ---------------------------------------------------------------------------

export interface RawEvent {
  ledger_sequence: number;
  tx_index: number;
  event_index: number;
  contract_id: string;
  /** Decoded topic symbols, e.g. ["Escrow", "Created", "<seller-address>"]. */
  topics: string[];
  /** Decoded XDR payload.  Field names match the Rust struct fields. */
  payload: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Event topic key
// Derived from the first two topics joined by ":".
// Single-topic events (e.g. "resolver_vote_recorded") use the topic directly.
// ---------------------------------------------------------------------------

/**
 * Every topic key emitted by contracts/escrow/src/events.rs.
 *
 * Keep this union, `KNOWN_TOPIC_KEYS` and the `processEvent` switch in
 * apply.ts in lockstep — an event missing from the switch is dropped from the
 * materialized tables.
 */
export type EventTopicKey =
  // Escrow lifecycle
  | "Escrow:Created"
  | "Escrow:Funded"
  | "Escrow:Shipped"
  | "Escrow:Delivered"
  | "Escrow:Completed"
  | "Escrow:Canceled"
  | "Escrow:Released"
  | "Basket:Created"
  | "Refund:Requested"
  | "Refund:Approved"
  // Disputes & resolvers
  | "Dispute:Raised"
  | "Dispute:Resolved"
  | "Dispute:Pending"
  | "Dispute:Appealed"
  | "Resolver:Rotated"
  | "Resolver:Approved"
  | "Resolver:Removed"
  | "ResStrct:Updated"
  | "resolver_vote_recorded"
  // Messaging
  | "Message:Posted"
  // Contract-level governance / configuration
  | "Contract:Init"
  | "Contract:Paused"
  | "Contract:Unpaused"
  | "Action:Paused"
  | "Action:Unpaused"
  | "Admin:Rotated"
  | "Fee:Updated"
  | "ProtoFee:Updated"
  | "ArbFee:Updated"
  | "PlatFee:Updated"
  | "Treasury:Updated"
  | "TtlExt:Updated"
  | "AmtLimit:Updated"
  | "Token:Allowlist"
  | "Allowlist:Toggled"
  | "contract_upgraded";

/** Runtime mirror of `EventTopicKey` — used to distinguish unknown events. */
export const KNOWN_TOPIC_KEYS: ReadonlySet<string> = new Set<EventTopicKey>([
  "Escrow:Created",
  "Escrow:Funded",
  "Escrow:Shipped",
  "Escrow:Delivered",
  "Escrow:Completed",
  "Escrow:Canceled",
  "Escrow:Released",
  "Basket:Created",
  "Refund:Requested",
  "Refund:Approved",
  "Dispute:Raised",
  "Dispute:Resolved",
  "Dispute:Pending",
  "Dispute:Appealed",
  "Resolver:Rotated",
  "Resolver:Approved",
  "Resolver:Removed",
  "ResStrct:Updated",
  "resolver_vote_recorded",
  "Message:Posted",
  "Contract:Init",
  "Contract:Paused",
  "Contract:Unpaused",
  "Action:Paused",
  "Action:Unpaused",
  "Admin:Rotated",
  "Fee:Updated",
  "ProtoFee:Updated",
  "ArbFee:Updated",
  "PlatFee:Updated",
  "Treasury:Updated",
  "TtlExt:Updated",
  "AmtLimit:Updated",
  "Token:Allowlist",
  "Allowlist:Toggled",
  "contract_upgraded",
]);

export function topicKey(topics: string[]): string {
  if (topics.length === 0) throw new Error("event has no topics");
  if (topics.length === 1) return topics[0]!;
  return `${topics[0]}:${topics[1]}`;
}

// ---------------------------------------------------------------------------
// Typed payload interfaces  (schema_version = 1)
// ---------------------------------------------------------------------------

export interface EscrowCreatedPayload {
  schema_version: number;
  escrow_id: string | number;
  seller: string;
  resolver: string;
  token: string;
  amount: string;
  fee_bps: number;
  resolver_fee_bps: number;
  shipping_window: string | number;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface EscrowFundedPayload {
  schema_version: number;
  escrow_id: string | number;
  buyer: string;
  amount: string;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface EscrowShippedPayload {
  schema_version: number;
  escrow_id: string | number;
  seller: string;
  tracking_id: string;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface DeliveryRecordedPayload {
  schema_version: number;
  escrow_id: string | number;
  delivered_at: string | number;
}

export interface EscrowCompletedPayload {
  schema_version: number;
  escrow_id: string | number;
  recipient: string;
  amount: string;
  fee_bps: number;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface EscrowCancelledPayload {
  schema_version: number;
  escrow_id: string | number;
  seller: string;
  /** Address that actually initiated the cancellation (buyer or a payee/seller). */
  cancelled_by: string;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface AutoReleasedPayload {
  schema_version: number;
  escrow_id: string | number;
  seller: string;
  amount: string;
  fee_bps: number;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface DisputeRaisedPayload {
  schema_version: number;
  escrow_id: string | number;
  buyer: string;
  reason: string;
  description: string;
  evidence_hash: string;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface DisputeResolvedPayload {
  schema_version: number;
  escrow_id: string | number;
  resolver: string;
  resolution: string;
  recipient: string;
  amount: string;
  arbitration_fee: string;
  resolver_fee: string;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface DisputePendingPayload {
  schema_version: number;
  escrow_id: string | number;
  resolver: string;
  resolution: string;
  amount: string;
  appeal_deadline: string | number;
  pending_at: string | number;
}

export interface DisputeAppealedPayload {
  schema_version: number;
  escrow_id: string | number;
  appellant: string;
  timestamp: string | number;
}

export interface ResolverRotatedPayload {
  schema_version: number;
  escrow_id: string | number;
  old_resolver: string;
  new_resolver: string;
  rotated_at: string | number;
}

export interface ResolverVoteRecordedPayload {
  schema_version: number;
  escrow_id: string | number;
  resolver: string;
  resolution: string;
  vote_count: number;
  threshold: number;
  voted_at: string | number;
}

export interface BasketEscrowCreatedPayload {
  schema_version: number;
  escrow_id: string | number;
  seller: string;
  token_count: number;
  timestamp: string | number;
}

export interface MessagePostedPayload {
  schema_version: number;
  escrow_id: string | number;
  sender: string;
  timestamp: string | number;
}

export interface RefundPayload {
  schema_version: number;
  escrow_id: string | number;
  timestamp: string | number;
  prev_state: string;
  new_state: string;
}

export interface ContractInitializedPayload {
  schema_version: number;
  admin: string;
  fee_collector: string;
  arbitration_fee_bps: number;
  timestamp: string | number;
}

export interface PauseTogglePayload {
  schema_version: number;
  admin: string;
  timestamp: string | number;
}

export interface ActionPauseTogglePayload {
  schema_version: number;
  action: string;
  caller: string;
  timestamp: string | number;
}

export interface AdminRotatedPayload {
  schema_version: number;
  old_admin: string;
  new_admin: string;
  timestamp: string | number;
}

/** Shared shape of Fee:Updated, ProtoFee:Updated, ArbFee:Updated, PlatFee:Updated. */
export interface FeeUpdatedPayload {
  schema_version: number;
  old_fee_bps: number;
  new_fee_bps: number;
  timestamp: string | number;
}

export interface TreasuryUpdatedPayload {
  schema_version: number;
  old_treasury: string;
  new_treasury: string;
  timestamp: string | number;
}

export interface TtlExtensionUpdatedPayload {
  schema_version: number;
  old_ledgers: number;
  new_ledgers: number;
  caller: string;
  timestamp: string | number;
}

export interface AmountLimitsUpdatedPayload {
  schema_version: number;
  old_min_amount: string;
  new_min_amount: string;
  old_max_amount: string;
  new_max_amount: string;
  caller: string;
  timestamp: string | number;
}

export interface TokenAllowlistUpdatedPayload {
  schema_version: number;
  token: string;
  added: boolean;
  timestamp: string | number;
}

export interface AllowlistToggledPayload {
  schema_version: number;
  enabled: boolean;
  timestamp: string | number;
}

export interface ResolverRegistryPayload {
  schema_version: number;
  resolver: string;
  caller: string;
  timestamp: string | number;
}

export interface ResolverStrictUpdatedPayload {
  schema_version: number;
  old_strict: boolean;
  new_strict: boolean;
  caller: string;
  timestamp: string | number;
}

export interface ContractUpgradedPayload {
  schema_version: number;
  admin: string;
  new_wasm_hash: string;
  timestamp: string | number;
}

/** Convenience: coerce a payload numeric field to string (handles bigint/number/string). */
export function str(v: unknown): string {
  if (v === null || v === undefined) throw new Error(`expected numeric value, got ${v}`);
  return String(v);
}

export function num(v: unknown): number {
  const n = Number(v);
  if (!Number.isFinite(n)) throw new Error(`expected number, got ${v}`);
  return n;
}
