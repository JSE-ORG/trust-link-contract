/**
 * React hook wrappers for the TrustLink escrow contract.
 *
 * These hooks depend only on React ≥ 18 and the `EscrowClient` /
 * `ContractTransport` abstractions — they have zero wallet-library opinion.
 * Plug in any transport (Soroban React, Freighter direct, etc.).
 *
 * @module hooks
 */
import type { EscrowData, DisputeData } from "./types.js";
import type { ContractTransport } from "./client.js";
interface AsyncState<T> {
    data: T | null;
    loading: boolean;
    error: Error | null;
}
/**
 * Fetch a single escrow by ID. Re-fetches whenever `escrowId` changes.
 *
 * @example
 * ```tsx
 * const { data, loading, error, refetch } = useEscrow(transport, 42n);
 * if (loading) return <Spinner />;
 * if (error) return <p>{error.message}</p>;
 * return <p>State: {data?.state}</p>;
 * ```
 */
export declare function useEscrow(transport: ContractTransport | null, escrowId: bigint | null): AsyncState<EscrowData> & {
    refetch: () => void;
};
/**
 * Fetch the dispute record for an escrow. Returns `null` data when no
 * dispute has been raised yet.
 *
 * @example
 * ```tsx
 * const { data: dispute, loading } = useDispute(transport, escrowId);
 * ```
 */
export declare function useDispute(transport: ContractTransport | null, escrowId: bigint | null): AsyncState<DisputeData | null> & {
    refetch: () => void;
};
/**
 * Mutation hook for funding an escrow.
 *
 * @example
 * ```tsx
 * const { fund, loading, error, success } = useFundEscrow(transport);
 *
 * const handleFund = () => fund(42n, "G...BUYER_ADDRESS");
 * ```
 */
export declare function useFundEscrow(transport: ContractTransport | null): {
    fund: (escrowId: bigint, buyer: string) => Promise<void>;
    loading: boolean;
    error: Error | null;
    success: boolean;
    reset: () => void;
};
/**
 * Mutation hook for confirming delivery (buyer releases funds to seller).
 *
 * @example
 * ```tsx
 * const { confirm, loading, error } = useConfirmDelivery(transport);
 *
 * const handleConfirm = () => confirm("G...BUYER", 42n);
 * ```
 */
export declare function useConfirmDelivery(transport: ContractTransport | null): {
    confirm: (caller: string, escrowId: bigint) => Promise<void>;
    loading: boolean;
    error: Error | null;
    success: boolean;
    reset: () => void;
};
/**
 * Mutation hook for raising a dispute on a shipped/funded escrow.
 *
 * @example
 * ```tsx
 * const { raise, loading, error } = useRaiseDispute(transport);
 *
 * raise(42n, "not_received", "Item never arrived", evidenceHashBytes);
 * ```
 */
export declare function useRaiseDispute(transport: ContractTransport | null): {
    raise: (caller: string, escrowId: bigint, reason: string, description: string, evidenceHash: Uint8Array) => Promise<void>;
    loading: boolean;
    error: Error | null;
    success: boolean;
    reset: () => void;
};
export {};
//# sourceMappingURL=hooks.d.ts.map