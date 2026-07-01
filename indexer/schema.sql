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
