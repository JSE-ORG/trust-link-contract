/**
 * Soroban React adapter — wires the Freighter wallet into `ContractTransport`.
 *
 * This module shows how to connect @soroban-react (or a Freighter-direct
 * approach) to the `EscrowClient` and the React hooks in `hooks.ts`.
 *
 * ## Dependencies
 * Install alongside this package:
 * ```bash
 * npm install @stellar/freighter-api @soroban-react/core @soroban-react/chains
 * npm install @stellar/stellar-sdk
 * ```
 *
 * @module soroban-react
 */
import type { ContractTransport } from "./client.js";
/**
 * Minimal interface of the Soroban React context value that this adapter
 * consumes. Keeps the adapter loosely coupled to the `@soroban-react/core`
 * package version.
 */
export interface SorobanContextLike {
    /** The RPC server base URL (e.g. `https://soroban-testnet.stellar.org`). */
    serverHorizon?: string;
    /** Current network passphrase. */
    activeNetwork?: string;
    /** Active wallet public key (Stellar G-address). */
    address?: string;
    /**
     * Sign and submit a transaction, returning the result XDR string.
     * `@soroban-react/core` exposes this as `server.sendTransaction` after
     * signing with the active connector.
     */
    signTransaction?: (xdr: string, opts?: Record<string, unknown>) => Promise<string>;
}
/**
 * Options for `createSorobanTransport`.
 */
export interface SorobanTransportOptions {
    /** Deployed contract address on the current network. */
    contractId: string;
    /** Soroban context (from `useSoroban()` or constructed manually). */
    context: SorobanContextLike;
    /**
     * Optional RPC URL override. Falls back to `context.serverHorizon`.
     */
    rpcUrl?: string;
    /** Network passphrase. Falls back to `context.activeNetwork`. */
    networkPassphrase?: string;
}
/**
 * Build a `ContractTransport` backed by a Soroban React context + Freighter.
 *
 * The returned transport calls the contract via JSON-RPC simulation for
 * read-only methods, and signs + submits for mutating methods.
 *
 * @example
 * ```tsx
 * import { useSoroban } from "@soroban-react/core";
 * import { createSorobanTransport } from "trustlink-escrow-bindings/soroban-react";
 * import { useEscrow } from "trustlink-escrow-bindings/hooks";
 *
 * const CONTRACT_ID = "C...YOUR_CONTRACT_ADDRESS";
 *
 * export function EscrowView({ id }: { id: bigint }) {
 *   const soroban = useSoroban();
 *   const transport = createSorobanTransport({ contractId: CONTRACT_ID, context: soroban });
 *   const { data, loading, error } = useEscrow(transport, id);
 *
 *   if (loading) return <p>Loading…</p>;
 *   if (error)   return <p>Error: {error.message}</p>;
 *   return <pre>{JSON.stringify(data, null, 2)}</pre>;
 * }
 * ```
 */
export declare function createSorobanTransport(options: SorobanTransportOptions): ContractTransport;
/**
 * Build a transport using Freighter's browser extension API directly,
 * without `@soroban-react/core`.
 *
 * @example
 * ```ts
 * import { createFreighterTransport } from "trustlink-escrow-bindings/soroban-react";
 *
 * const transport = await createFreighterTransport({
 *   contractId: "C...",
 *   networkPassphrase: "Test SDF Network ; September 2015",
 *   rpcUrl: "https://soroban-testnet.stellar.org",
 * });
 * ```
 */
export declare function createFreighterTransport(opts: {
    contractId: string;
    networkPassphrase: string;
    rpcUrl: string;
}): Promise<ContractTransport>;
//# sourceMappingURL=soroban-react.d.ts.map