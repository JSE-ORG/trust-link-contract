/**
 * Event application layer — translates each decoded on-chain event into SQL
 * mutations against the materialized tables (escrows, disputes).
 *
 * Every handler is called inside an open transaction so all mutations for one
 * event are atomic.  Handlers are idempotent: re-applying the same event (e.g.
 * during replay after a restart) must produce identical state.
 *
 * The EVENT_SCHEMA_VERSION guard at the top of processEvent rejects payloads
 * whose schema_version exceeds what this code understands, preventing silent
 * misinterpretation of unknown fields.
 */

import type pg from "pg";
import type { RawEvent } from "./types.js";
import {
  topicKey,
  str,
  num,
  type EscrowCreatedPayload,
  type EscrowFundedPayload,
  type EscrowShippedPayload,
  type DeliveryRecordedPayload,
  type EscrowCompletedPayload,
  type EscrowCancelledPayload,
  type AutoReleasedPayload,
  type DisputeRaisedPayload,
  type DisputeResolvedPayload,
  type DisputePendingPayload,
  type DisputeAppealedPayload,
  type ResolverRotatedPayload,
} from "./types.js";

const SUPPORTED_SCHEMA_VERSION = 1;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

export async function processEvent(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const payload = event.payload;
  const version = num(payload["schema_version"]);

  if (version > SUPPORTED_SCHEMA_VERSION) {
    throw new Error(
      `Unsupported schema_version ${version} (indexer supports up to ${SUPPORTED_SCHEMA_VERSION}). ` +
        `Upgrade the indexer before continuing.`,
    );
  }
  if (version < SUPPORTED_SCHEMA_VERSION) {
    console.warn(
      `[apply] schema_version ${version} < ${SUPPORTED_SCHEMA_VERSION} for event ` +
        `${event.ledger_sequence}/${event.tx_index}/${event.event_index} — processing with best-effort.`,
    );
  }

  const key = topicKey(event.topics);

  switch (key) {
    case "Escrow:Created":
      return applyEscrowCreated(client, event);
    case "Escrow:Funded":
      return applyEscrowFunded(client, event);
    case "Escrow:Shipped":
      return applyEscrowShipped(client, event);
    case "Escrow:Delivered":
      return applyDeliveryRecorded(client, event);
    case "Escrow:Completed":
      return applyEscrowCompleted(client, event);
    case "Escrow:Canceled":
      return applyEscrowCancelled(client, event);
    case "Escrow:Released":
      return applyAutoReleased(client, event);
    case "Dispute:Raised":
      return applyDisputeRaised(client, event);
    case "Dispute:Resolved":
      return applyDisputeResolved(client, event);
    case "Dispute:Pending":
      return applyDisputePending(client, event);
    case "Dispute:Appealed":
      return applyDisputeAppealed(client, event);
    case "Resolver:Rotated":
      return applyResolverRotated(client, event);
    case "Refund:Requested":
      return applyRefundRequested(client, event);
    case "Refund:Approved":
      return applyRefundApproved(client, event);
    default:
      // Non-escrow events (fee updates, admin rotation, etc.) are recorded in
      // the events table by the caller but require no materialized state change.
      break;
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function p<T>(event: RawEvent): T {
  return event.payload as T;
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async function applyEscrowCreated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<EscrowCreatedPayload>(event);
  await client.query(
    `INSERT INTO escrows
       (escrow_id, seller, resolver, token, amount, fee_bps, resolver_fee_bps,
        shipping_window, state, created_at, updated_ledger)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
     ON CONFLICT (escrow_id) DO NOTHING`,
    [
      str(d.escrow_id),
      d.seller,
      d.resolver,
      d.token,
      str(d.amount),
      num(d.fee_bps),
      num(d.resolver_fee_bps),
      str(d.shipping_window),
      d.new_state,
      str(d.timestamp),
      event.ledger_sequence,
    ],
  );
}

async function applyEscrowFunded(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<EscrowFundedPayload>(event);
  await client.query(
    `UPDATE escrows
        SET buyer = $2, funded_at = $3, state = $4, updated_ledger = $5
      WHERE escrow_id = $1`,
    [str(d.escrow_id), d.buyer, str(d.timestamp), d.new_state, event.ledger_sequence],
  );
}

async function applyEscrowShipped(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<EscrowShippedPayload>(event);
  await client.query(
    `UPDATE escrows
        SET shipped_at = $2, tracking_id = $3, state = $4, updated_ledger = $5
      WHERE escrow_id = $1`,
    [str(d.escrow_id), str(d.timestamp), d.tracking_id, d.new_state, event.ledger_sequence],
  );
}

async function applyDeliveryRecorded(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<DeliveryRecordedPayload>(event);
  await client.query(
    `UPDATE escrows
        SET delivered_at = $2, updated_ledger = $3
      WHERE escrow_id = $1`,
    [str(d.escrow_id), str(d.delivered_at), event.ledger_sequence],
  );
}

async function applyEscrowCompleted(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<EscrowCompletedPayload>(event);
  await client.query(
    `UPDATE escrows
        SET state = $2, completed_at = $3, updated_ledger = $4
      WHERE escrow_id = $1`,
    [str(d.escrow_id), d.new_state, str(d.timestamp), event.ledger_sequence],
  );
}

async function applyEscrowCancelled(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<EscrowCancelledPayload>(event);
  await client.query(
    `UPDATE escrows
        SET state = $2, cancelled_at = $3, updated_ledger = $4
      WHERE escrow_id = $1`,
    [str(d.escrow_id), d.new_state, str(d.timestamp), event.ledger_sequence],
  );
}

async function applyAutoReleased(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<AutoReleasedPayload>(event);
  await client.query(
    `UPDATE escrows
        SET state = $2, completed_at = $3, updated_ledger = $4
      WHERE escrow_id = $1`,
    [str(d.escrow_id), d.new_state, str(d.timestamp), event.ledger_sequence],
  );
}

async function applyDisputeRaised(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<DisputeRaisedPayload>(event);
  const escrowId = str(d.escrow_id);

  await client.query(
    `UPDATE escrows SET state = $2, updated_ledger = $3 WHERE escrow_id = $1`,
    [escrowId, d.new_state, event.ledger_sequence],
  );

  await client.query(
    `INSERT INTO disputes
       (escrow_id, buyer, reason, description, evidence_hash, status, disputed_at)
     VALUES ($1,$2,$3,$4,$5,'Active',$6)
     ON CONFLICT (escrow_id) DO UPDATE
       SET buyer         = EXCLUDED.buyer,
           reason        = EXCLUDED.reason,
           description   = EXCLUDED.description,
           evidence_hash = EXCLUDED.evidence_hash,
           status        = 'Active',
           disputed_at   = EXCLUDED.disputed_at,
           resolution    = NULL,
           resolver      = NULL,
           resolved_at   = NULL`,
    [escrowId, d.buyer, d.reason, d.description, d.evidence_hash, str(d.timestamp)],
  );
}

async function applyDisputeResolved(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<DisputeResolvedPayload>(event);
  const escrowId = str(d.escrow_id);

  await client.query(
    `UPDATE escrows SET state = $2, completed_at = $3, updated_ledger = $4 WHERE escrow_id = $1`,
    [escrowId, d.new_state, str(d.timestamp), event.ledger_sequence],
  );

  await client.query(
    `UPDATE disputes
        SET status = 'Resolved', resolution = $2, resolver = $3, resolved_at = $4
      WHERE escrow_id = $1`,
    [escrowId, d.resolution, d.resolver, str(d.timestamp)],
  );
}

async function applyDisputePending(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<DisputePendingPayload>(event);
  const escrowId = str(d.escrow_id);

  await client.query(
    `UPDATE escrows SET state = 'PendingFinalization', updated_ledger = $2 WHERE escrow_id = $1`,
    [escrowId, event.ledger_sequence],
  );

  await client.query(
    `UPDATE disputes SET appeal_deadline = $2, resolver = $3 WHERE escrow_id = $1`,
    [escrowId, str(d.appeal_deadline), d.resolver],
  );
}

async function applyDisputeAppealed(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<DisputeAppealedPayload>(event);
  await client.query(
    `UPDATE escrows SET state = 'Disputed', updated_ledger = $2 WHERE escrow_id = $1`,
    [str(d.escrow_id), event.ledger_sequence],
  );
}

async function applyResolverRotated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<ResolverRotatedPayload>(event);
  await client.query(
    `UPDATE escrows SET resolver = $2, updated_ledger = $3 WHERE escrow_id = $1`,
    [str(d.escrow_id), d.new_resolver, event.ledger_sequence],
  );
}

async function applyRefundRequested(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const escrowId = str(event.payload["escrow_id"]);
  const newState = String(event.payload["new_state"]);
  await client.query(
    `UPDATE escrows SET state = $2, updated_ledger = $3 WHERE escrow_id = $1`,
    [escrowId, newState, event.ledger_sequence],
  );
}

async function applyRefundApproved(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const escrowId = str(event.payload["escrow_id"]);
  const newState = String(event.payload["new_state"]);
  const timestamp = str(event.payload["timestamp"]);
  await client.query(
    `UPDATE escrows SET state = $2, completed_at = $3, updated_ledger = $4 WHERE escrow_id = $1`,
    [escrowId, newState, timestamp, event.ledger_sequence],
  );
}
