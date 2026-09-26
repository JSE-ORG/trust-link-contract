# Frontend Integration Guide

Guide for integrating the TrustLink escrow contract into a frontend application
using the official `@trustlink/contract-bindings` package. For the full API
surface (all client methods, React hooks, batching, evidence hashing, error
codes), see [bindings/README.md](../bindings/README.md) — this guide covers
the create-escrow flow end to end and links out to it for everything else.

## Table of Contents
- [Installation](#installation)
- [Creating a transport](#creating-a-transport)
- [Creating an escrow](#creating-an-escrow)
- [Funding and progressing an escrow](#funding-and-progressing-an-escrow)
- [Listening for events](#listening-for-events)
- [Error handling](#error-handling)
- [Testing on testnet](#testing-on-testnet)

## Installation

```bash
npm install @trustlink/contract-bindings
# peer deps for the Freighter transport used below
npm install @stellar/stellar-sdk @stellar/freighter-api
```

See [bindings/README.md](../bindings/README.md#installation) for the other
transport options (`@soroban-react/core`) and which peer deps each needs.

## Creating a transport

The `EscrowClient` talks to the contract through a `ContractTransport`. The
bundled Freighter factory covers the common browser-extension case:

```ts
import { createFreighterTransport, EscrowClient } from "@trustlink/contract-bindings";

const transport = await createFreighterTransport({
  contractId: "C...YOUR_CONTRACT_ADDRESS",
  networkPassphrase: "Test SDF Network ; September 2015",
  rpcUrl: "https://soroban-testnet.stellar.org",
});

const client = new EscrowClient(transport);
```

## Creating an escrow

The real contract entry point is a **9-argument** `create_escrow`, not the
`(recipient, amount, deadline)` shape older drafts of this guide used:

```rust
pub fn create_escrow(
    env: Env,
    seller_or_payees: Val,          // Address, or Vec<Payee> for a split payout
    buyer: Option<Address>,         // omit to let anyone fund it
    resolver: Address,
    token: Address,
    amount: i128,
    fee_bps: u32,
    resolver_fee_bps: u32,
    shipping_window: u64,
    notes: Option<String>,
) -> Result<u64, ContractError>;
```

`seller_or_payees` is polymorphic: pass a single `Address` for a normal escrow,
or a `Vec<Payee>` (`{ address, bps }` pairs summing to 10,000 bps) to split the
payout across multiple sellers. The bindings client accepts either as its
first argument:

```ts
// Single seller
const escrowId = await client.create_escrow(
  sellerAddress,     // Address — or an array of { address, bps } payees
  buyerAddress,      // Option<Address> — pass null to leave it open
  resolverAddress,
  tokenAddress,
  1_000_0000000n,    // amount, i128 as bigint (7 decimals for most SEP-41 tokens)
  250,               // fee_bps — protocol fee, in basis points
  50,                // resolver_fee_bps
  86_400n,           // shipping_window, in seconds
);

// Split payout across two sellers
const splitEscrowId = await client.create_escrow(
  [
    { address: sellerA, bps: 7_000 },
    { address: sellerB, bps: 3_000 },
  ],
  buyerAddress,
  resolverAddress,
  tokenAddress,
  1_000_0000000n,
  250,
  50,
  86_400n,
);
```

For a resolver committee or a primary/backup resolver instead of a single
resolver, use `create_escrow_multi` or `create_escrow_with_fallback` — see the
`EscrowClient` method table in
[bindings/README.md](../bindings/README.md#api-reference).

## Funding and progressing an escrow

```ts
await client.fund_escrow(escrowId, buyerAddress);
await client.mark_shipped(sellerAddress, escrowId, "TRK-001");
await client.record_delivery(sellerAddress, escrowId);
await client.confirm_delivery(buyerAddress, escrowId);

// Read it back
const escrow = await client.get_escrow(escrowId);
console.log(escrow.state); // e.g. "Completed"
```

The full lifecycle (dispute raising/resolution, refunds, cancellation,
auto-release, message threads) is documented method-by-method in
[bindings/README.md](../bindings/README.md#api-reference).

## Listening for events

Event topics are **not** `escrow_created` / `escrow_released` string topics —
they are structured `symbol_short!` topic tuples, e.g. escrow creation is
`(Symbol("Escrow"), Symbol("Created"), seller)`. The full topic and payload
reference for every event lives in [events.md](events.md); don't duplicate it
here, as it drifts. A minimal listener:

```ts
import { xdr, scValToNative } from "@stellar/stellar-sdk";

async function listenForEscrowEvents(startLedger: number) {
  const events = await server.getEvents({
    startLedger,
    filters: [{ type: "contract", contractIds: [CONTRACT_ID] }],
  });

  for (const event of events.events) {
    const topics = event.topic.map((t) => scValToNative(xdr.ScVal.fromXDR(t, "base64")));
    const payload = scValToNative(xdr.ScVal.fromXDR(event.value, "base64"));

    if (topics[0] === "Escrow" && topics[1] === "Created") {
      console.log("Escrow created:", payload); // { schema_version, escrow_id, seller, ... }
    }
  }
}
```

Check `payload.schema_version` before relying on field shape — see the
[Schema Versioning](events.md#schema-versioning) section of `events.md`.

## Error handling

Contract errors surface as a typed `ContractInvokeError` with an `ErrorCode`,
not a string you have to pattern-match:

```ts
import { ContractInvokeError, ErrorCode } from "@trustlink/contract-bindings/errors";

try {
  await client.fund_escrow(escrowId, buyerAddress);
} catch (err) {
  if (err instanceof ContractInvokeError) {
    if (err.code === ErrorCode.EscrowNotFound) {
      alert("That escrow does not exist.");
    } else {
      console.error(err.code, err.message);
    }
  }
}
```

To avoid spending fees on a call that would fail, simulate first with
`simulateAndCatch` — see
[bindings/README.md](../bindings/README.md#simulating-calls-before-submitting).

## Testing on testnet

```bash
# Install the Stellar CLI
cargo install --locked stellar-cli

# Create and fund test accounts
stellar keys generate alice --network testnet
stellar keys generate bob --network testnet
stellar keys fund alice --network testnet
stellar keys fund bob --network testnet
```

```bash
# Build and deploy the contract
cd contracts/escrow
cargo build --target wasm32v1-none --release
stellar contract deploy \
  --wasm ../../target/wasm32v1-none/release/trustlink_escrow.wasm \
  --source alice \
  --network testnet
```

Regenerate the TypeScript bindings after any contract ABI change — see
[bindings/README.md](../bindings/README.md#regenerating-bindings) — and run
the exercised flow above (`create_escrow` → `fund_escrow` → … → `get_escrow`)
against the deployed contract id.

## Support

For issues or questions, please open an issue on the GitHub repository.
