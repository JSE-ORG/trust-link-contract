-- TrustLink Escrow Indexer — PostgreSQL Schema
-- Run once against a fresh database:  psql $DATABASE_URL -f schema.sql

-- ---------------------------------------------------------------------------
-- Raw event log — append-only; the authoritative source for replay
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS events (
  id              BIGSERIAL    PRIMARY KEY,
  ledger_sequence BIGINT       NOT NULL,
  tx_index        INT          NOT NULL,
  event_index     INT          NOT NULL,
  contract_id     TEXT         NOT NULL,
  topic_key       TEXT         NOT NULL,  -- e.g. "Escrow:Created"
  schema_version  INT          NOT NULL,
  payload         JSONB        NOT NULL,
  ingested_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),

  UNIQUE (ledger_sequence, tx_index, event_index)
);

CREATE INDEX IF NOT EXISTS events_contract_topic  ON events (contract_id, topic_key);
CREATE INDEX IF NOT EXISTS events_ledger_seq      ON events (ledger_sequence);

-- ---------------------------------------------------------------------------
-- Materialized escrow state
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS escrows (
  escrow_id        BIGINT      PRIMARY KEY,
  seller           TEXT        NOT NULL,
  buyer            TEXT,
  resolver         TEXT        NOT NULL,
  token            TEXT        NOT NULL,
  amount           NUMERIC(39) NOT NULL,
  fee_bps          INT         NOT NULL,
  resolver_fee_bps INT         NOT NULL DEFAULT 0,
  shipping_window  BIGINT      NOT NULL,
  state            TEXT        NOT NULL,  -- mirrors EscrowState enum
  funded_at        BIGINT,
  shipped_at       BIGINT,
  tracking_id      TEXT,
  delivered_at     BIGINT,
  completed_at     BIGINT,
  cancelled_at     BIGINT,
  created_at       BIGINT      NOT NULL,
  updated_ledger   BIGINT      NOT NULL
);

CREATE INDEX IF NOT EXISTS escrows_seller   ON escrows (seller);
CREATE INDEX IF NOT EXISTS escrows_buyer    ON escrows (buyer);
CREATE INDEX IF NOT EXISTS escrows_resolver ON escrows (resolver);
CREATE INDEX IF NOT EXISTS escrows_state    ON escrows (state);

-- ---------------------------------------------------------------------------
-- Materialized dispute state
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS disputes (
  escrow_id      BIGINT  PRIMARY KEY REFERENCES escrows (escrow_id),
  buyer          TEXT    NOT NULL,
  reason         TEXT    NOT NULL,
  description    TEXT    NOT NULL,
  evidence_hash  TEXT    NOT NULL,
  status         TEXT    NOT NULL DEFAULT 'Active',   -- Active | Resolved
  resolution     TEXT,                               -- Release | Refund
  resolver       TEXT,
  appeal_deadline BIGINT,
  disputed_at    BIGINT  NOT NULL,
  resolved_at    BIGINT
);

-- ---------------------------------------------------------------------------
-- Materialized basket-escrow metadata (Basket:Created)
--
-- BasketEscrowCreated carries only the escrow id, seller and token count, so
-- basket escrows get their own table rather than a partial `escrows` row.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS basket_escrows (
  escrow_id      BIGINT PRIMARY KEY,
  seller         TEXT   NOT NULL,
  token_count    INT    NOT NULL,
  created_at     BIGINT NOT NULL,
  updated_ledger BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS basket_escrows_seller ON basket_escrows (seller);

-- ---------------------------------------------------------------------------
-- Per-escrow resolver votes (resolver_vote_recorded)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS resolver_votes (
  escrow_id      BIGINT NOT NULL,
  resolver       TEXT   NOT NULL,
  resolution     TEXT   NOT NULL,
  vote_count     INT    NOT NULL,
  threshold      INT    NOT NULL,
  voted_at       BIGINT NOT NULL,
  updated_ledger BIGINT NOT NULL,

  PRIMARY KEY (escrow_id, resolver)
);

-- ---------------------------------------------------------------------------
-- Escrow message log (Message:Posted)
--
-- Message content stays off-chain — the event only carries sender + timestamp.
-- Keyed by event position so replay is idempotent.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS messages (
  ledger_sequence BIGINT NOT NULL,
  tx_index        INT    NOT NULL,
  event_index     INT    NOT NULL,
  escrow_id       BIGINT NOT NULL,
  sender          TEXT   NOT NULL,
  posted_at       BIGINT NOT NULL,

  PRIMARY KEY (ledger_sequence, tx_index, event_index)
);

CREATE INDEX IF NOT EXISTS messages_escrow ON messages (escrow_id);

-- ---------------------------------------------------------------------------
-- Contract-level governance state, materialized as a key/value store.
--
-- Keys written by the indexer:
--   admin, fee_collector, paused, action_paused:<action>, default_fee_bps,
--   protocol_fee_bps, arbitration_fee_bps, platform_fee_bps, treasury,
--   ttl_extension_ledgers, min_escrow_amount, max_escrow_amount,
--   token_allowlist_enabled, resolver_strict, wasm_hash
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS contract_config (
  key            TEXT   PRIMARY KEY,
  value          TEXT,
  updated_at     BIGINT,
  updated_ledger BIGINT NOT NULL
);

-- ---------------------------------------------------------------------------
-- Token allowlist membership (Token:Allowlist)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS token_allowlist (
  token          TEXT    PRIMARY KEY,
  allowed        BOOLEAN NOT NULL,
  updated_at     BIGINT,
  updated_ledger BIGINT  NOT NULL
);

-- ---------------------------------------------------------------------------
-- Approved-resolver registry (Resolver:Approved / Resolver:Removed)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS approved_resolvers (
  resolver       TEXT    PRIMARY KEY,
  approved       BOOLEAN NOT NULL,
  updated_at     BIGINT,
  updated_ledger BIGINT  NOT NULL
);

-- ---------------------------------------------------------------------------
-- Single-row cursor — tracks the last successfully processed position.
-- The ingester reads this on startup to resume without reprocessing.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS indexer_cursor (
  id              INT   PRIMARY KEY DEFAULT 1 CHECK (id = 1),
  ledger_sequence BIGINT NOT NULL DEFAULT 0,
  tx_index        INT    NOT NULL DEFAULT 0,
  event_index     INT    NOT NULL DEFAULT 0,
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO indexer_cursor (id, ledger_sequence, tx_index, event_index)
VALUES (1, 0, 0, 0)
ON CONFLICT (id) DO NOTHING;
