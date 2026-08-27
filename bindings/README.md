# @trustlink/contract-bindings

TypeScript bindings, React hooks, and a Soroban React adapter for the **TrustLink** escrow smart contract on Stellar.

The package is published as **`@trustlink/contract-bindings`**; older docs and
examples may still call it `trustlink-escrow-bindings`.

---

## Installation

```bash
npm install @trustlink/contract-bindings
# peer deps for React hooks
npm install react
# peer deps for the Soroban React adapter
npm install @stellar/stellar-sdk @stellar/freighter-api @soroban-react/core @soroban-react/chains
```

All four peer dependencies are **optional** — install only what you use. The
core `EscrowClient` and the type/error/evidence modules have no runtime
dependencies at all.

### Subpath exports

| Import | Contents |
|---|---|
| `@trustlink/contract-bindings` | everything (re-exports every module below) |
| `@trustlink/contract-bindings/client` | `EscrowClient`, `EscrowBatch`, `ContractTransport` |
| `@trustlink/contract-bindings/types` | enums + data interfaces |
| `@trustlink/contract-bindings/errors` | `ErrorCode`, `ContractInvokeError`, `parseContractError` |
| `@trustlink/contract-bindings/evidence` | dispute evidence hashing |
| `@trustlink/contract-bindings/simulation` | pre-submit simulation helpers |
| `@trustlink/contract-bindings/hooks` | React hooks |
| `@trustlink/contract-bindings/soroban-react` | wallet transports |
| `@trustlink/contract-bindings/abi` | machine-readable contract manifest |

---

## Quick start (< 15 min)

### 1 — Create a transport

The `EscrowClient` needs a `ContractTransport` that knows how to send calls
to the contract. Use one of the bundled factories or roll your own.

**Freighter (browser extension, no framework)**

```ts
import { createFreighterTransport } from "@trustlink/contract-bindings";

const transport = await createFreighterTransport({
  contractId: "C...YOUR_CONTRACT_ADDRESS",
  networkPassphrase: "Test SDF Network ; September 2015",
  rpcUrl: "https://soroban-testnet.stellar.org",
});
```

**@soroban-react/core**

```tsx
import { useSoroban } from "@soroban-react/core";
import { createSorobanTransport } from "@trustlink/contract-bindings";

const soroban = useSoroban();
const transport = createSorobanTransport({
  contractId: "C...YOUR_CONTRACT_ADDRESS",
  context: soroban,
});
```

### 2 — Use the client directly

```ts
import { EscrowClient } from "@trustlink/contract-bindings";

const client = new EscrowClient(transport);

// Read escrow
const escrow = await client.get_escrow(42n);
console.log(escrow.state); // "Funded"

// Fund an escrow
await client.fund_escrow(42n, "G...BUYER_ADDRESS");
```

### 3 — Use React hooks

```tsx
import { useEscrow, useFundEscrow, useDispute, useRaiseDispute } from "@trustlink/contract-bindings";

function EscrowCard({ escrowId }: { escrowId: bigint }) {
  const { data, loading, error, refetch } = useEscrow(transport, escrowId);
  const { fund, loading: funding, error: fundError } = useFundEscrow(transport);

  if (loading) return <p>Loading…</p>;
  if (error) return <p>Error: {error.message}</p>;

  return (
    <div>
      <p>State: {data?.state}</p>
      <button onClick={() => fund(escrowId, "G...BUYER")} disabled={funding}>
        Fund Escrow
      </button>
      {fundError && <p>{fundError.message}</p>}
    </div>
  );
}
```

---

## API reference

### `EscrowClient`

Every method maps 1:1 to a contract entry point. Each returns either the decoded
value or a `Promise` of it, depending on the transport. Full parameter docs,
authorization requirements and the errors each call can throw are in the JSDoc
on `EscrowClient` (hover in your editor).

| Method | Description |
|---|---|
| `create_escrow(payees, buyer, resolver, token, amount, feeBps, resolverFeeBps, shippingWindow)` | Creates a single-token escrow, returns its `bigint` id |
| `create_basket_escrow(seller, buyer, resolver, tokens, amounts, feeBps, shippingWindow)` | Creates a multi-token escrow |
| `batch_create_escrow(seller, escrows)` | Creates many escrows for one seller in one tx |
| `fund_escrow(escrowId, buyer)` | Buyer deposits the amount, opens the dispute window |
| `fund_basket_escrow(escrowId, buyer)` | Funds a basket escrow (pulls every token) |
| `mark_shipped(caller, escrowId, trackingId)` | Seller records shipment |
| `record_delivery(caller, escrowId)` | Records delivery, starts the auto-release window |
| `confirm_delivery(caller, escrowId)` | Buyer accepts delivery, releases funds |
| `auto_release(escrowId)` | Anyone releases funds once the windows elapse |
| `cancel_escrow(caller, escrowId)` / `mutual_cancel(escrowId)` | Cancel an escrow |
| `request_refund(caller, escrowId)` / `approve_refund(caller, escrowId)` | Buyer/seller refund handshake |
| `raise_dispute(caller, escrowId, reason, description, evidenceHash)` | Buyer opens a dispute |
| `resolve_dispute(caller, escrowId, resolution)` | Resolver settles the dispute |
| `rotate_resolver(caller, escrowId, newResolver)` | Swap an escrow's resolver |
| `post_message(escrowId, sender, content)` / `get_messages(escrowId, start, limit)` | Escrow message thread |
| `get_escrow(escrowId)` | Read the full `EscrowData` record |
| `get_dispute(escrowId)` | Read `DisputeData` (or `null`) |
| `get_escrows_by_buyer(buyer)` / `get_escrows_by_vendor(vendor)` | List escrow ids for an address |
| `get_stats()` / `get_public_config()` / `get_fee_config()` | Read aggregate/config state |

### React hooks

| Hook | Description |
|---|---|
| `useEscrow(transport, escrowId)` | Fetch an escrow. Returns `{ data, loading, error, refetch }` |
| `useDispute(transport, escrowId)` | Fetch the dispute record. Returns `{ data, loading, error, refetch }` |
| `useBasketTokens(transport, escrowId)` | Fetch a basket escrow's token/amount entries |
| `useFundEscrow(transport)` | Mutation. Returns `{ fund, loading, error, success, reset }` |
| `useFundBasketEscrow(transport)` | Mutation for basket escrows. Same shape as `useFundEscrow` |
| `useConfirmDelivery(transport)` | Mutation. Returns `{ confirm, loading, error, success, reset }` |
| `useRaiseDispute(transport)` | Mutation. Returns `{ raise, loading, error, success, reset }` |

Pass `null` as the transport (or escrow id) to keep a hook idle until the value
is ready. Memoize the transport (`useMemo`) so query hooks don't refetch on
every render.

### Error handling

Contract errors are surfaced as `ContractInvokeError` instances with a typed `code` property:

```ts
import { ContractInvokeError, ErrorCode } from "@trustlink/contract-bindings";

try {
  await client.fund_escrow(id, buyer);
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

All 45 contract error codes (`InvalidAmount` = 1 … `TooManyMessages` = 45) are
exported from `ErrorCode`, each with an entry in `ERROR_MESSAGES`. The enum is
kept byte-for-byte in sync with `contracts/escrow/src/errors.rs` by
`scripts/check-error-codes.mjs` in CI.

> The legacy `ContractError` enum in `types.ts` is a partial, historical copy
> and is deprecated — use `ErrorCode` from `@trustlink/contract-bindings/errors`.

---

### Simulating calls before submitting

Soroban can *simulate* a call before you sign and submit it, so you can surface
the exact error a transaction would produce without spending fees. Wrap any
client call in `simulateAndCatch` to get a structured result instead of a thrown
error:

```ts
import { simulateAndCatch, ErrorCode } from "@trustlink/contract-bindings";

const result = await simulateAndCatch(() => client.fund_escrow(id, buyer));

if (!result.ok) {
  // `result.code` is the typed ErrorCode (or null for non-contract failures),
  // `result.error` is a ContractInvokeError, `result.raw` is the original error.
  if (result.code === ErrorCode.ContractPaused) {
    alert("The contract is paused — try again later.");
  }
  return;
}

// result.value holds the (typed) return value — safe to submit for real.
await client.fund_escrow(id, buyer);
```

Companion helpers:

- `assertSimulationSucceeds(call)` — runs the simulation and **throws** the
  expected `ContractInvokeError` if it would fail, otherwise returns the value.
  Handy as a pre-submit guard inside an existing `try/catch`.
- `isSimulationError(result)` — type guard narrowing a `SimulationResult` to its
  failure variant.
- `createEscrowSimulator(transport)` — wraps a `ContractTransport` so
  `simulate(method, args)` returns a `SimulationResult` for any method by name.

Run the helper test suite with `npm test` (uses Node's built-in test runner).

---

## Dispute evidence hashing

`raise_dispute` takes a 32-byte `BytesN<32>` commitment to the evidence — the
evidence itself stays off chain. Build the commitment with `hashEvidence`
(SHA-256, via the built-in Web Crypto API — no dependency):

```ts
import { hashEvidence, verifyEvidence, EMPTY_EVIDENCE_HASH } from "@trustlink/contract-bindings/evidence";

const evidenceHash = await hashEvidence(await file.arrayBuffer());
await client.raise_dispute(buyer, id, "damaged", "Arrived broken", evidenceHash);

// Later, prove a file is the one that was committed to:
const ok = await verifyEvidence(await file.arrayBuffer(), dispute.evidence_hash);
```

Use `EMPTY_EVIDENCE_HASH` (all-zero) to raise a dispute with no attached
evidence. `toHex` / `fromHex` convert to and from a hex string;
`isValidEvidenceHash` / `isEmptyEvidenceHash` are guards.

---

## Batching calls (`multicall`)

A Stellar transaction carries one `InvokeHostFunction` op, so N contract calls
would otherwise be N transactions. `EscrowBatch` packs them into a single
`multicall`:

```ts
const [_, __] = await client
  .batch()
  .fund_escrow(id, buyer)
  .mark_shipped(seller, id, "TRK-001")
  .execute();
```

`createBatch(transport)` builds one without an `EscrowClient`;
`.pendingCalls()` returns a snapshot for inspection. Results come back in the
order the calls were added.

---

## Exported modules

```
@trustlink/contract-bindings
├── index.ts          — re-exports every module below
├── types.ts          — enums and data interfaces (EscrowState, EscrowData …)
├── client.ts         — EscrowClient, EscrowBatch, ContractTransport
├── abi.ts            — machine-readable contract manifest (contractAbi)
├── errors.ts         — ErrorCode, ERROR_MESSAGES, ContractInvokeError, parseContractError
├── evidence.ts       — hashEvidence, verifyEvidence, toHex/fromHex, EMPTY_EVIDENCE_HASH
├── simulation.ts     — simulateAndCatch, assertSimulationSucceeds, createEscrowSimulator
├── hooks.ts          — React hooks (useEscrow, useDispute, useFundEscrow …)
└── soroban-react.ts  — createSorobanTransport, createFreighterTransport
```

---

## Regenerating bindings

When the contract ABI changes, rebuild the Wasm and regenerate:

```bash
cargo build --target wasm32v1-none --release
stellar contract bindings typescript \
  --wasm ../target/wasm32v1-none/release/trustlink_escrow.wasm \
  --output-dir src \
  --overwrite
npm run typecheck
```

Commit the updated `src/` output alongside the contract change.

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `Freighter wallet is not installed` | Extension not present or not connected | Install [Freighter](https://www.freighter.app) and connect it |
| `Simulation failed: …` | Wrong `contractId` or network | Double-check `contractId` and `networkPassphrase` |
| `ContractInvokeError: NotAuthorized` | Wrong signer for the action | Use the correct role's address (buyer / seller / resolver) |
| Hook returns stale data | Transport reference changes every render | Memoize the transport with `useMemo` |
| TypeScript errors on `bigint` literals | Target < ES2020 | Set `"target": "ES2020"` (or higher) in your `tsconfig.json` |
