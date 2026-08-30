# Indexer Fallback Resolver

> **Not the on-chain resolver.** This page documents `FallbackResolver` in
> `indexer/src/resolvers/fallback.ts` — an off-chain TypeScript helper that
> races indexer RPC calls against a timeout. It is unrelated to the
> contract's `ResolverSet::Fallback` / `FallbackResolver` type
> (`contracts/escrow/src/types.rs`), which is an on-chain primary/backup
> dispute-resolver pair with an absolute `dispute_deadline` timestamp — see
> [`ResolverSet::can_resolve_now`](../../contracts/escrow/src/types.rs) for
> that one. The two share a name and nothing else; if you're looking for how
> dispute resolvers are configured on an escrow, you want the contract type,
> not this file.

The indexer's Fallback Resolver manages automated network failovers across multiple ingest endpoints within the Stellar Wave Indexer framework. It ensures continuity by racing active data queries against a strict, configurable execution deadline constraint.

---

## Configuration Reference

### `FallbackResolverOptions`

Configuration object passed to the `FallbackResolver` constructor instance.

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `deadline` | `number` (optional) | `5000` | Specifies the duration limit in milliseconds before an open resolution execution times out and redirects to the next configured backup node interface. |

---

## Methods

### `getDeadline()`

Retrieves the active timeout window duration currently set for the instance.

* **Returns**: `number` — Deadline duration in milliseconds.

### `resolveWithDeadline<T>(task: Promise<T>)`

Races a processing operation task against the active configuration timeout boundary.

* **Parameters**:
  * `task`: `Promise<T>` — The pending asynchronous network operation or ingestion query loop.
* **Returns**: `Promise<T>` — Resolves with the operational payload data if successful before the deadline.
* **Throws**: `Error` — Rejects if the runtime execution timeline exceeds the configured millisecond window.

---

## Code Examples

### Default Configuration (Backward Compatibility)

If no explicit options are supplied, the instance defaults gracefully to an internal 5-second boundary.

```typescript
import { FallbackResolver } from '../src/resolvers/fallback';

// Initializes safely with the default 5000ms threshold
const resolver = new FallbackResolver();
console.log(resolver.getDeadline()); // Output: 5000
```

### Custom High-Responsiveness Failover Configuration

Override default behaviors to catch slow network responses aggressively during periods of heavy ingestion load.

```typescript
import { FallbackResolver } from '../src/resolvers/fallback';

// Restrict network query tasks to a strict 1.5-second processing window
const customResolver = new FallbackResolver({ deadline: 1500 });

try {
  const result = await customResolver.resolveWithDeadline(fetchStellarLedgerTask);
  // Process ledger payload data safely
} catch (error) {
  // Output: Error: Fallback resolver deadline exceeded after 1500ms
  console.error("Failover triggered:", error.message);
}
```
