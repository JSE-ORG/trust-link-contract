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

export type EventTopicKey =
  | "Escrow:Created"
  | "Escrow:Funded"
  | "Escrow:Shipped"
  | "Escrow:Delivered"
  | "Escrow:Completed"
  | "Escrow:Canceled"
  | "Escrow:Released"
  | "Dispute:Raised"
  | "Dispute:Resolved"
  | "Dispute:Pending"
  | "Dispute:Appealed"
  | "Resolver:Rotated"
  | "resolver_vote_recorded"
  | "Contract:Init"
  | "Contract:Paused"
  | "Contract:Unpaused"
  | "Admin:Rotated"
  | "Fee:Updated"
  | "ProtoFee:Updated"
  | "ArbFee:Updated"
  | "PlatFee:Updated"
  | "Fee:Withdrawn"
  | "Token:Allowlist"
  | "Allowlist:Toggled"
  | "Treasury:Updated"
  | "Message:Posted"
  | "Refund:Requested"
  | "Refund:Approved"
  | "contract_upgraded"
  | "Basket:Created";

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
