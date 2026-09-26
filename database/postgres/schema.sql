-- TrustLink Postgres schema for off-chain escrow indexing.
-- This schema is append-friendly and keeps contract numeric values in raw units.

CREATE TYPE escrow_state AS ENUM (
    'Pending',
    'Funded',
    'Shipped',
    'Completed',
    'Disputed',
    'RefundRequested',
    'Refunded',
    'Canceled',
    'Cancelled',
    'PendingFinalization',
    'Expired'
);

CREATE TYPE dispute_status AS ENUM (
    'Active',
    'Resolved'
);

CREATE TYPE resolution_type AS ENUM (
    'Release',
    'Refund'
);

CREATE TABLE IF NOT EXISTS contract_config (
    contract_id TEXT PRIMARY KEY,
    admin TEXT NOT NULL,
    fee_collector TEXT NOT NULL,
    fee_bps INTEGER NOT NULL CHECK (fee_bps BETWEEN 0 AND 10000),
    arbitration_fee_bps INTEGER NOT NULL CHECK (arbitration_fee_bps BETWEEN 0 AND 10000),
    escrow_count BIGINT NOT NULL DEFAULT 0 CHECK (escrow_count >= 0),
    paused BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS escrows (
    contract_id TEXT NOT NULL,
    escrow_id BIGINT NOT NULL CHECK (escrow_id >= 0),
    seller TEXT NOT NULL,
    buyer TEXT,
    resolver TEXT NOT NULL,
    token TEXT NOT NULL,
    amount NUMERIC(39, 0) NOT NULL CHECK (amount >= 0),
    fee_bps INTEGER NOT NULL CHECK (fee_bps BETWEEN 0 AND 10000),
    resolver_fee_bps INTEGER NOT NULL DEFAULT 0 CHECK (resolver_fee_bps BETWEEN 0 AND 10000),
    shipping_window BIGINT NOT NULL CHECK (shipping_window >= 0),
    funded_at BIGINT NOT NULL DEFAULT 0 CHECK (funded_at >= 0),
    dispute_deadline BIGINT NOT NULL DEFAULT 0 CHECK (dispute_deadline >= 0),
    state escrow_state NOT NULL,
    shipped_at BIGINT NOT NULL DEFAULT 0 CHECK (shipped_at >= 0),
    delivered_at BIGINT CHECK (delivered_at IS NULL OR delivered_at >= 0),
    tracking_id TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (contract_id, escrow_id)
);

CREATE TABLE IF NOT EXISTS escrow_payees (
    contract_id TEXT NOT NULL,
    escrow_id BIGINT NOT NULL,
    payee_index INTEGER NOT NULL CHECK (payee_index >= 0),
    address TEXT NOT NULL,
    bps INTEGER NOT NULL CHECK (bps BETWEEN 0 AND 10000),
    PRIMARY KEY (contract_id, escrow_id, payee_index),
    FOREIGN KEY (contract_id, escrow_id) REFERENCES escrows (contract_id, escrow_id)
);

CREATE TABLE IF NOT EXISTS disputes (
    contract_id TEXT NOT NULL,
    escrow_id BIGINT NOT NULL,
    reason TEXT NOT NULL,
    description TEXT NOT NULL,
    evidence_hash BYTEA NOT NULL CHECK (octet_length(evidence_hash) = 32),
    status dispute_status NOT NULL,
    disputed_at BIGINT NOT NULL CHECK (disputed_at >= 0),
    tracking_id TEXT,
    resolution resolution_type,
    resolved_at BIGINT CHECK (resolved_at IS NULL OR resolved_at >= 0),
    PRIMARY KEY (contract_id, escrow_id),
    FOREIGN KEY (contract_id, escrow_id) REFERENCES escrows (contract_id, escrow_id)
);

CREATE TABLE IF NOT EXISTS escrow_events (
    id BIGSERIAL PRIMARY KEY,
    contract_id TEXT NOT NULL,
    escrow_id BIGINT CHECK (escrow_id IS NULL OR escrow_id >= 0),
    topic TEXT NOT NULL,
    ledger_sequence BIGINT NOT NULL CHECK (ledger_sequence >= 0),
    ledger_timestamp BIGINT NOT NULL CHECK (ledger_timestamp >= 0),
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (contract_id, topic, ledger_sequence, id)
);

CREATE INDEX IF NOT EXISTS idx_escrows_seller ON escrows (seller);
CREATE INDEX IF NOT EXISTS idx_escrows_buyer ON escrows (buyer) WHERE buyer IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_escrows_resolver ON escrows (resolver);
CREATE INDEX IF NOT EXISTS idx_escrows_token ON escrows (token);
CREATE INDEX IF NOT EXISTS idx_escrows_state ON escrows (state);
CREATE INDEX IF NOT EXISTS idx_disputes_status ON disputes (status);
CREATE INDEX IF NOT EXISTS idx_escrow_events_contract_escrow ON escrow_events (contract_id, escrow_id);
CREATE INDEX IF NOT EXISTS idx_escrow_events_topic ON escrow_events (topic);
CREATE INDEX IF NOT EXISTS idx_escrow_events_ledger ON escrow_events (ledger_sequence, ledger_timestamp);
