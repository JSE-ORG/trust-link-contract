# TrustLink Escrow Indexer

Three ways to consume on-chain TrustLink events, from lightest to heaviest:

| Option | Good for | Setup |
|---|---|---|
| **Stellar Expert** | Ad-hoc queries, dashboards | Zero — public REST API |
| **Mercury** | Production backends, realtime feeds | Subscribe via CLI or dashboard |
| **Self-hosted** (this repo) | Full control, custom materialized state | `docker compose up` + run replay |

---

## 1. Stellar Expert (REST API)

No account required. Query the public Stellar Expert API directly.

```sh
# All Escrow:Created events for a contract
curl -s "https://api.stellar.expert/explorer/testnet/contract-events
  ?contract=$CONTRACT_ID
  &topic0=0000000f00000006457363726f77
  &topic1=0000000f000000074372656174656400
  &order=asc&limit=100"
```

Decode the hex topic and value XDR with the stellar-sdk:

```ts
import { xdr, scValToNative } from '@stellar/stellar-sdk'

const payload = scValToNative(xdr.ScVal.fromXDR(record.value, 'hex'))
// → { schema_version: 1, escrow_id: 1n, seller: "G...", ... }
```

**Query by participant:** `stellar-expert/queries/escrows_by_participant.http`
covers seller, buyer, and resolver roles via topic[2] filters.

---

## 2. Mercury (hosted Soroban indexer)

Mercury captures events in realtime and exposes them via GraphQL.

### Subscribe

```sh
# Install the Mercury CLI
npm install -g mercury-cli

# Register all TrustLink event subscriptions
mercury-cli subscribe \
  --email $MERCURY_EMAIL \
  --password $MERCURY_PASSWORD \
  --manifest indexer/mercury/manifest.toml
```

Or use the [Mercury dashboard](https://mercurydata.app) to add subscriptions
manually using the `topic_1_xdr` / `topic_2_xdr` values from `manifest.toml`.

### Query by participant (GraphQL)

```graphql
query AllEscrowsForParticipant($contractId: String!, $address: String!) {
  asSeller_created: eventsByContractId(
    contractId: $contractId
    topic1: "AAAADwAAAAZFc2Nyb3c="   # Symbol("Escrow")
    topic2: "AAAADwAAAAdDcmVhdGVk"   # Symbol("Created")
    topic3: $address                  # XDR ScVal of participant address
    first: 100
  ) {
    nodes { ledger txHash topic2 topic3 data }
    pageInfo { hasNextPage endCursor }
  }

  asBuyer_funded: eventsByContractId(
    contractId: $contractId
    topic1: "AAAADwAAAAZFc2Nyb3c="
    topic2: "AAAADwAAAAZGdW5kZWQ="   # Symbol("Funded")
    topic3: $address
    first: 100
  ) {
    nodes { ledger txHash topic2 topic3 data }
    pageInfo { hasNextPage endCursor }
  }

  asResolver_resolved: eventsByContractId(
    contractId: $contractId
    topic1: "AAAADwAAAAdEaXNwdXRl"   # Symbol("Dispute")
    topic2: "AAAADwAAAAhSZXNvbHZlZA=" # Symbol("Resolved")
    topic3: $address
    first: 100
  ) {
    nodes { ledger txHash topic2 topic3 data }
    pageInfo { hasNextPage endCursor }
  }
}
```

Full query file: `mercury/queries/escrows_by_participant.graphql`

Generate the `$address` XDR variable:
```sh
stellar xdr encode --type ScVal --input json \
  '{"type":"address","value":{"type":"account","account_id":"G..."}}'
```

---

## 3. Self-hosted PostgreSQL indexer

Materializes all events into `escrows` and `disputes` tables.

```sh
# 1. Start Postgres
docker compose up -d postgres

# 2. Create schema
psql $DATABASE_URL -f indexer/schema.sql

# 3. Replay fixture (deterministic, resume-safe)
DATABASE_URL=postgres://stellar:stellar@localhost/stellar \
  npx tsx indexer/src/replay.ts indexer/fixtures/events.json

# 4. Query by participant (plain SQL)
psql $DATABASE_URL -c \
  "SELECT escrow_id, seller, buyer, resolver, state, amount
     FROM escrows
    WHERE seller = 'G...' OR buyer = 'G...' OR resolver = 'G...'"
```

Live ingestion (after wiring `SorobanRpcSource` in `indexer/src/ingest.ts`):
```sh
CONTRACT_ID=C...           \
SOROBAN_RPC_URL=https://...  \
DATABASE_URL=postgres://...  \
  npx tsx indexer/src/ingest.ts
```

---

## Schema versioning

Every event payload includes `schema_version: 1`.  If this field ever exceeds
what your consumer expects, it means the event struct changed.  See
`docs/events.md` for the version changelog and upgrade policy.

```ts
if (payload.schema_version > SUPPORTED_SCHEMA_VERSION) {
  throw new Error(`Update your indexer — schema_version ${payload.schema_version} is unsupported`)
}
```

---

## File map

```
indexer/
├── schema.sql                                     # PostgreSQL DDL
├── fixtures/events.json                           # 17-event test fixture
├── src/
│   ├── types.ts                                   # shared interfaces + cursor
│   ├── db.ts                                      # pg Pool + withTx
│   ├── cursor.ts                                  # read/write indexer_cursor
│   ├── apply.ts                                   # event → SQL state machine
│   ├── ingest.ts                                  # polling loop + ingestBatch
│   └── replay.ts                                  # deterministic fixture replay
├── mercury/
│   ├── manifest.toml                              # subscription manifest
│   └── queries/
│       ├── escrows_by_participant.graphql          # GraphQL queries
│       └── schema_version_check.graphql
└── stellar-expert/
    ├── config.yaml                                # API endpoint + topic hex map
    └── queries/
        └── escrows_by_participant.http            # REST API examples
```
