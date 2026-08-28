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
  KNOWN_TOPIC_KEYS,
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
  type ResolverVoteRecordedPayload,
  type BasketEscrowCreatedPayload,
  type MessagePostedPayload,
  type ContractInitializedPayload,
  type PauseTogglePayload,
  type ActionPauseTogglePayload,
  type AdminRotatedPayload,
  type FeeUpdatedPayload,
  type TreasuryUpdatedPayload,
  type TtlExtensionUpdatedPayload,
  type AmountLimitsUpdatedPayload,
  type TokenAllowlistUpdatedPayload,
  type AllowlistToggledPayload,
  type ResolverRegistryPayload,
  type ResolverStrictUpdatedPayload,
  type ContractUpgradedPayload,
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
    case "Basket:Created":
      return applyBasketCreated(client, event);
    case "resolver_vote_recorded":
      return applyResolverVoteRecorded(client, event);
    case "Message:Posted":
      return applyMessagePosted(client, event);

    // --- contract-level governance / configuration -------------------------
    case "Contract:Init":
      return applyContractInit(client, event);
    case "Contract:Paused":
      return applyContractPauseToggle(client, event, true);
    case "Contract:Unpaused":
      return applyContractPauseToggle(client, event, false);
    case "Action:Paused":
      return applyActionPauseToggle(client, event, true);
    case "Action:Unpaused":
      return applyActionPauseToggle(client, event, false);
    case "Admin:Rotated":
      return applyAdminRotated(client, event);
    case "Fee:Updated":
      return applyFeeUpdated(client, event, "default_fee_bps");
    case "ProtoFee:Updated":
      return applyFeeUpdated(client, event, "protocol_fee_bps");
    case "ArbFee:Updated":
      return applyFeeUpdated(client, event, "arbitration_fee_bps");
    case "PlatFee:Updated":
      return applyFeeUpdated(client, event, "platform_fee_bps");
    case "Treasury:Updated":
      return applyTreasuryUpdated(client, event);
    case "TtlExt:Updated":
      return applyTtlExtensionUpdated(client, event);
    case "AmtLimit:Updated":
      return applyAmountLimitsUpdated(client, event);
    case "Allowlist:Toggled":
      return applyAllowlistToggled(client, event);
    case "Token:Allowlist":
      return applyTokenAllowlistUpdated(client, event);
    case "Resolver:Approved":
      return applyResolverRegistryChange(client, event, true);
    case "Resolver:Removed":
      return applyResolverRegistryChange(client, event, false);
    case "ResStrct:Updated":
      return applyResolverStrictUpdated(client, event);
    case "contract_upgraded":
      return applyContractUpgraded(client, event);

    default:
      // Anything reaching here is emitted by a contract version this indexer
      // does not know about.  The raw event is still persisted by the caller,
      // but no materialized state is derived from it — surface it loudly so
      // the gap is visible instead of silently dropped.
      console.warn(
        `[apply] unhandled event topic "${key}" ` +
          `(topics=${JSON.stringify(event.topics)}) at ` +
          `${event.ledger_sequence}/${event.tx_index}/${event.event_index}` +
          (KNOWN_TOPIC_KEYS.has(key)
            ? " — topic is known but has no handler; this is an indexer bug."
            : " — unknown topic; upgrade the indexer."),
      );
      break;
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function p<T>(event: RawEvent): T {
  return event.payload as T;
}

/**
 * Upsert one row into the `contract_config` key/value table.
 *
 * Idempotent by construction: replaying the same event writes the same value.
 * The `updated_ledger` guard keeps out-of-order retries from rolling state
 * backwards.
 */
async function setConfig(
  client: pg.PoolClient,
  key: string,
  value: string | null,
  updatedAt: string | null,
  ledgerSequence: number,
): Promise<void> {
  await client.query(
    `INSERT INTO contract_config (key, value, updated_at, updated_ledger)
     VALUES ($1,$2,$3,$4)
     ON CONFLICT (key) DO UPDATE
       SET value          = EXCLUDED.value,
           updated_at     = EXCLUDED.updated_at,
           updated_ledger = EXCLUDED.updated_ledger
     WHERE contract_config.updated_ledger <= EXCLUDED.updated_ledger`,
    [key, value, updatedAt, ledgerSequence],
  );
}

/** Payload timestamp as a string, or null when the event carries none. */
function ts(payload: Record<string, unknown>, field = "timestamp"): string | null {
  const v = payload[field];
  return v === null || v === undefined ? null : String(v);
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

async function applyBasketCreated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<BasketEscrowCreatedPayload>(event);
  await client.query(
    `INSERT INTO basket_escrows (escrow_id, seller, token_count, created_at, updated_ledger)
     VALUES ($1,$2,$3,$4,$5)
     ON CONFLICT (escrow_id) DO UPDATE
       SET seller         = EXCLUDED.seller,
           token_count    = EXCLUDED.token_count,
           created_at     = EXCLUDED.created_at,
           updated_ledger = EXCLUDED.updated_ledger`,
    [str(d.escrow_id), d.seller, num(d.token_count), str(d.timestamp), event.ledger_sequence],
  );
}

async function applyResolverVoteRecorded(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<ResolverVoteRecordedPayload>(event);
  await client.query(
    `INSERT INTO resolver_votes
       (escrow_id, resolver, resolution, vote_count, threshold, voted_at, updated_ledger)
     VALUES ($1,$2,$3,$4,$5,$6,$7)
     ON CONFLICT (escrow_id, resolver) DO UPDATE
       SET resolution     = EXCLUDED.resolution,
           vote_count     = EXCLUDED.vote_count,
           threshold      = EXCLUDED.threshold,
           voted_at       = EXCLUDED.voted_at,
           updated_ledger = EXCLUDED.updated_ledger`,
    [
      str(d.escrow_id),
      d.resolver,
      d.resolution,
      num(d.vote_count),
      num(d.threshold),
      str(d.voted_at),
      event.ledger_sequence,
    ],
  );
}

async function applyMessagePosted(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<MessagePostedPayload>(event);
  await client.query(
    `INSERT INTO messages
       (ledger_sequence, tx_index, event_index, escrow_id, sender, posted_at)
     VALUES ($1,$2,$3,$4,$5,$6)
     ON CONFLICT (ledger_sequence, tx_index, event_index) DO NOTHING`,
    [
      event.ledger_sequence,
      event.tx_index,
      event.event_index,
      str(d.escrow_id),
      d.sender,
      str(d.timestamp),
    ],
  );
}

// ---------------------------------------------------------------------------
// Governance / configuration handlers
// ---------------------------------------------------------------------------

async function applyContractInit(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<ContractInitializedPayload>(event);
  const at = ts(event.payload);
  await setConfig(client, "admin", d.admin, at, event.ledger_sequence);
  await setConfig(client, "fee_collector", d.fee_collector, at, event.ledger_sequence);
  await setConfig(
    client,
    "arbitration_fee_bps",
    String(num(d.arbitration_fee_bps)),
    at,
    event.ledger_sequence,
  );
  await setConfig(client, "paused", "false", at, event.ledger_sequence);
}

async function applyContractPauseToggle(
  client: pg.PoolClient,
  event: RawEvent,
  paused: boolean,
): Promise<void> {
  const d = p<PauseTogglePayload>(event);
  const at = ts(event.payload);
  await setConfig(client, "paused", String(paused), at, event.ledger_sequence);
  await setConfig(client, "paused_by", d.admin, at, event.ledger_sequence);
}

async function applyActionPauseToggle(
  client: pg.PoolClient,
  event: RawEvent,
  paused: boolean,
): Promise<void> {
  const d = p<ActionPauseTogglePayload>(event);
  await setConfig(
    client,
    `action_paused:${d.action}`,
    String(paused),
    ts(event.payload),
    event.ledger_sequence,
  );
}

async function applyAdminRotated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<AdminRotatedPayload>(event);
  await setConfig(client, "admin", d.new_admin, ts(event.payload), event.ledger_sequence);
}

async function applyFeeUpdated(
  client: pg.PoolClient,
  event: RawEvent,
  key: string,
): Promise<void> {
  const d = p<FeeUpdatedPayload>(event);
  await setConfig(
    client,
    key,
    String(num(d.new_fee_bps)),
    ts(event.payload),
    event.ledger_sequence,
  );
}

async function applyTreasuryUpdated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<TreasuryUpdatedPayload>(event);
  await setConfig(client, "treasury", d.new_treasury, ts(event.payload), event.ledger_sequence);
}

async function applyTtlExtensionUpdated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<TtlExtensionUpdatedPayload>(event);
  await setConfig(
    client,
    "ttl_extension_ledgers",
    String(num(d.new_ledgers)),
    ts(event.payload),
    event.ledger_sequence,
  );
}

async function applyAmountLimitsUpdated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<AmountLimitsUpdatedPayload>(event);
  const at = ts(event.payload);
  await setConfig(client, "min_escrow_amount", str(d.new_min_amount), at, event.ledger_sequence);
  await setConfig(client, "max_escrow_amount", str(d.new_max_amount), at, event.ledger_sequence);
}

async function applyAllowlistToggled(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<AllowlistToggledPayload>(event);
  await setConfig(
    client,
    "token_allowlist_enabled",
    String(Boolean(d.enabled)),
    ts(event.payload),
    event.ledger_sequence,
  );
}

async function applyTokenAllowlistUpdated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<TokenAllowlistUpdatedPayload>(event);
  await client.query(
    `INSERT INTO token_allowlist (token, allowed, updated_at, updated_ledger)
     VALUES ($1,$2,$3,$4)
     ON CONFLICT (token) DO UPDATE
       SET allowed        = EXCLUDED.allowed,
           updated_at     = EXCLUDED.updated_at,
           updated_ledger = EXCLUDED.updated_ledger
     WHERE token_allowlist.updated_ledger <= EXCLUDED.updated_ledger`,
    [d.token, Boolean(d.added), ts(event.payload), event.ledger_sequence],
  );
}

async function applyResolverRegistryChange(
  client: pg.PoolClient,
  event: RawEvent,
  approved: boolean,
): Promise<void> {
  const d = p<ResolverRegistryPayload>(event);
  await client.query(
    `INSERT INTO approved_resolvers (resolver, approved, updated_at, updated_ledger)
     VALUES ($1,$2,$3,$4)
     ON CONFLICT (resolver) DO UPDATE
       SET approved       = EXCLUDED.approved,
           updated_at     = EXCLUDED.updated_at,
           updated_ledger = EXCLUDED.updated_ledger
     WHERE approved_resolvers.updated_ledger <= EXCLUDED.updated_ledger`,
    [d.resolver, approved, ts(event.payload), event.ledger_sequence],
  );
}

async function applyResolverStrictUpdated(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<ResolverStrictUpdatedPayload>(event);
  await setConfig(
    client,
    "resolver_strict",
    String(Boolean(d.new_strict)),
    ts(event.payload),
    event.ledger_sequence,
  );
}

async function applyContractUpgraded(client: pg.PoolClient, event: RawEvent): Promise<void> {
  const d = p<ContractUpgradedPayload>(event);
  const at = ts(event.payload);
  await setConfig(client, "wasm_hash", d.new_wasm_hash, at, event.ledger_sequence);
  await setConfig(client, "upgraded_by", d.admin, at, event.ledger_sequence);
}
