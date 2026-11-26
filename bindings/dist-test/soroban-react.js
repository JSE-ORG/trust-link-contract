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
// ---------------------------------------------------------------------------
// Transport factory
// ---------------------------------------------------------------------------
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
export function createSorobanTransport(options) {
    const { contractId, context, rpcUrl, networkPassphrase } = options;
    return {
        invoke(method, args) {
            return invokeSoroban({
                contractId,
                method,
                args,
                rpcUrl: rpcUrl ?? context.serverHorizon ?? "",
                networkPassphrase: networkPassphrase ?? context.activeNetwork ?? "",
                callerAddress: context.address,
                signTransaction: context.signTransaction,
            });
        },
    };
}
/**
 * Simulate or sign-and-submit a contract call via Soroban RPC.
 *
 * For read-only contract methods (`get_*`) this performs a simulation and
 * returns the decoded result. For mutating methods it builds, signs, and
 * submits the transaction.
 *
 * @internal
 */
async function invokeSoroban({ contractId, method, args, rpcUrl, networkPassphrase, callerAddress, signTransaction, }) {
    // Dynamic import keeps Stellar SDK out of the module graph for consumers
    // that only use the type definitions.
    const { Contract, TransactionBuilder, Networks, BASE_FEE, nativeToScVal, scValToNative, rpc, Account, Keypair, } = await import("@stellar/stellar-sdk");
    const server = new rpc.Server(rpcUrl, { allowHttp: false });
    // Build a dummy source account when no wallet is connected (simulation only).
    const sourceAddress = callerAddress ?? Keypair.random().publicKey();
    const account = await server.getAccount(sourceAddress).catch(() => {
        return new Account(sourceAddress, "0");
    });
    const contract = new Contract(contractId);
    const scArgs = args.map((a) => nativeToScVal(a));
    const tx = new TransactionBuilder(account, {
        fee: BASE_FEE,
        networkPassphrase: networkPassphrase || Networks.TESTNET,
    })
        .addOperation(contract.call(method, ...scArgs))
        .setTimeout(30)
        .build();
    // Read-only path: simulate and decode
    const isReadOnly = method.startsWith("get_");
    if (isReadOnly || !signTransaction) {
        const sim = await server.simulateTransaction(tx);
        if (rpc.Api.isSimulationError(sim)) {
            throw new Error(`Simulation failed: ${sim.error}`);
        }
        if (!rpc.Api.isSimulationSuccess(sim) || !sim.result) {
            throw new Error("Simulation returned no result.");
        }
        return scValToNative(sim.result.retval);
    }
    // Mutating path: prepare → sign → submit → poll
    const prepared = await server.prepareTransaction(tx);
    const signedXdr = await signTransaction(prepared.toXDR(), {
        networkPassphrase,
    });
    const { Transaction } = await import("@stellar/stellar-sdk");
    const signedTx = new Transaction(signedXdr, networkPassphrase);
    const sendResult = await server.sendTransaction(signedTx);
    if (sendResult.status === "ERROR") {
        throw new Error(`Transaction failed: ${JSON.stringify(sendResult.errorResult)}`);
    }
    // Poll until finality
    let getResult = await server.getTransaction(sendResult.hash);
    while (getResult.status === "NOT_FOUND") {
        await sleep(1500);
        getResult = await server.getTransaction(sendResult.hash);
    }
    if (getResult.status === "FAILED") {
        throw new Error(`Transaction failed on-chain: ${sendResult.hash}`);
    }
    // Decode return value when present
    const returnVal = getResult["returnValue"];
    if (returnVal) {
        return scValToNative(returnVal);
    }
    return undefined;
}
function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}
// ---------------------------------------------------------------------------
// Freighter direct helper (no @soroban-react dependency)
// ---------------------------------------------------------------------------
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
export async function createFreighterTransport(opts) {
    const freighter = await import("@stellar/freighter-api");
    const isConnected = await freighter.isConnected();
    if (!isConnected)
        throw new Error("Freighter wallet is not installed or not accessible.");
    const { address } = await freighter.getAddress();
    const signTransaction = async (xdr) => {
        const result = await freighter.signTransaction(xdr, {
            networkPassphrase: opts.networkPassphrase,
        });
        // Freighter ≥ 2.x returns { signedTxXdr, signerAddress }
        if (typeof result === "string")
            return result;
        if (result && typeof result["signedTxXdr"] === "string") {
            return result["signedTxXdr"];
        }
        throw new Error("Unexpected Freighter signTransaction response shape.");
    };
    return createSorobanTransport({
        contractId: opts.contractId,
        rpcUrl: opts.rpcUrl,
        networkPassphrase: opts.networkPassphrase,
        context: {
            address,
            serverHorizon: opts.rpcUrl,
            activeNetwork: opts.networkPassphrase,
            signTransaction,
        },
    });
}
