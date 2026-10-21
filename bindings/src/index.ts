/**
 * `@trustlink/contract-bindings` — TypeScript bindings for the TrustLink
 * escrow contract on Stellar/Soroban.
 *
 * This entry point re-exports everything from the package's modules:
 *
 * - {@link EscrowClient} / `ContractTransport` — typed wrapper over every
 *   contract entry point ({@link ./client.js | client}).
 * - {@link EscrowBatch} / `createBatch` — fluent `multicall` batching.
 * - Data types and enums — `EscrowData`, `EscrowState`, … ({@link ./types.js | types}).
 * - `ErrorCode` / `ContractInvokeError` / `parseContractError` — typed error
 *   handling ({@link ./errors.js | errors}).
 * - `hashEvidence` and friends — dispute evidence hashing ({@link ./evidence.js | evidence}).
 * - `simulateAndCatch` and friends — pre-submit simulation ({@link ./simulation.js | simulation}).
 * - `useEscrow`, `useFundEscrow`, … — React hooks ({@link ./hooks.js | hooks}).
 * - `createSorobanTransport` / `createFreighterTransport` — wallet transports
 *   ({@link ./soroban-react.js | soroban-react}).
 * - `contractAbi` — a machine-readable manifest of the contract surface
 *   ({@link ./abi.js | abi}).
 *
 * Each module is also published as its own subpath export (e.g.
 * `@trustlink/contract-bindings/hooks`) so wallet/React code can be tree-shaken
 * away from a backend that only needs the client.
 *
 * @module @trustlink/contract-bindings
 */
export * from "./types.js";
export * from "./client.js";
export * from "./batch.js";
export * from "./abi.js";
export * from "./errors.js";
export * from "./evidence.js";
export * from "./simulation.js";
export * from "./hooks.js";
export * from "./soroban-react.js";
