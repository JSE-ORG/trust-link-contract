# trustlink-escrow-bindings

TypeScript bindings, React hooks, and a Soroban React adapter for the **TrustLink** escrow smart contract on Stellar.

---

## Installation

```bash
npm install trustlink-escrow-bindings
# peer deps for React hooks
npm install react
# peer deps for the Soroban React adapter
npm install @stellar/stellar-sdk @stellar/freighter-api @soroban-react/core @soroban-react/chains
```

> React and the Soroban SDK are optional peer dependencies — you only need them
> if you use `hooks.ts` and `soroban-react.ts` respectively.

---

## Quick start (< 15 min)

### 1 — Create a transport

The `EscrowClient` needs a `ContractTransport` that knows how to send calls
to the contract. Use one of the bundled factories or roll your own.

**Freighter (browser extension, no framework)**

```ts
import { createFreighterTransport } from "trustlink-escrow-bindings";

const transport = await createFreighterTransport({
  contractId: "C...YOUR_CONTRACT_ADDRESS",
  networkPassphrase: "Test SDF Network ; September 2015",
  rpcUrl: "https://soroban-testnet.stellar.org",
});
```

**@soroban-react/core**

```tsx
import { useSoroban } from "@soroban-react/core";
import { createSorobanTransport } from "trustlink-escrow-bindings";

const soroban = useSoroban();
const transport = createSorobanTransport({
  contractId: "C...YOUR_CONTRACT_ADDRESS",
  context: soroban,
});
```

### 2 — Use the client directly

```ts
import { EscrowClient } from "trustlink-escrow-bindings";

const client = new EscrowClient(transport);

// Read escrow
const escrow = await client.get_escrow(42n);
console.log(escrow.state); // "Funded"

// Fund an escrow
await client.fund_escrow(42n, "G...BUYER_ADDRESS");
```

### 3 — Use React hooks

```tsx
import { useEscrow, useFundEscrow, useDispute, useRaiseDispute } from "trustlink-escrow-bindings";

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

All contract entry points are available as typed async methods:

| Method | Description |
|---|---|
| `create_escrow(seller, resolver, token, amount, feeBps, shippingWindow)` | Creates a new escrow, returns `bigint` ID |
| `fund_escrow(escrowId, buyer)` | Buyer funds the escrow |
| `mark_shipped(caller, escrowId, trackingId)` | Seller marks item as shipped |
| `confirm_delivery(caller, escrowId)` | Buyer confirms delivery, releases funds |
| `raise_dispute(escrowId, reason, description, evidenceHash)` | Buyer raises a dispute |
| `resolve_dispute(escrowId, resolution)` | Resolver settles dispute |
| `auto_release(escrowId)` | Anyone triggers auto-release after window |
| `cancel_escrow(escrowId)` | Seller cancels pending escrow |
| `get_escrow(escrowId)` | Read escrow data |
| `get_dispute(escrowId)` | Read dispute data (or `null`) |
| `get_fee_config()` | Read fee configuration |

### React hooks

| Hook | Description |
|---|---|
| `useEscrow(transport, escrowId)` | Fetch + subscribe to an escrow. Returns `{ data, loading, error, refetch }` |
| `useDispute(transport, escrowId)` | Fetch dispute record. Returns `{ data, loading, error, refetch }` |
| `useFundEscrow(transport)` | Mutation hook. Returns `{ fund, loading, error, success, reset }` |
| `useConfirmDelivery(transport)` | Mutation hook. Returns `{ confirm, loading, error, success, reset }` |
| `useRaiseDispute(transport)` | Mutation hook. Returns `{ raise, loading, error, success, reset }` |

### Error handling

Contract errors are surfaced as `ContractInvokeError` instances with a typed `code` property:

```ts
import { ContractInvokeError, ErrorCode } from "trustlink-escrow-bindings";

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

All 24 error codes from the contract (`InvalidAmount` → `DisputeWindowClosed`) are exported from `ErrorCode` and come with a human-readable `message`.

---

## Exported modules

```
trustlink-escrow-bindings
├── types.ts          — enums and data interfaces (EscrowState, EscrowData …)
├── client.ts         — EscrowClient + ContractTransport interface
├── abi.ts            — contract ABI manifest
├── errors.ts         — ErrorCode enum, ContractInvokeError, parseContractError
├── hooks.ts          — React hooks (useEscrow, useDispute, useFundEscrow …)
└── soroban-react.ts  — createSorobanTransport, createFreighterTransport
```

---

## Regenerating bindings

When the contract ABI changes, rebuild the Wasm and regenerate:

```bash
cargo build --target wasm32-unknown-unknown --release
stellar contract bindings typescript \
  --wasm ../target/wasm32-unknown-unknown/release/trustlink_escrow.wasm \
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
