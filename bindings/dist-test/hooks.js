/**
 * React hook wrappers for the TrustLink escrow contract.
 *
 * These hooks depend only on React ≥ 18 and the `EscrowClient` /
 * `ContractTransport` abstractions — they have zero wallet-library opinion.
 * Plug in any transport (Soroban React, Freighter direct, etc.).
 *
 * @module hooks
 */
// NOTE: This file uses generic React types via JSDoc so the package does not
// force a peer-dep on @types/react. Users must install react + @types/react
// themselves. The hooks are written as plain functions that mirror the React
// hooks API; TypeScript will resolve the types from the consumer's node_modules.
import { useCallback, useEffect, useReducer, useRef } from "react";
import { EscrowClient } from "./client.js";
import { parseContractError } from "./errors.js";
function asyncReducer(state, action) {
    switch (action.type) {
        case "loading":
            return { data: state.data, loading: true, error: null };
        case "success":
            return { data: action.payload, loading: false, error: null };
        case "error":
            return { data: null, loading: false, error: action.payload };
    }
}
const initial = () => ({ data: null, loading: false, error: null });
function normalizeError(err) {
    const contractErr = parseContractError(err);
    if (contractErr)
        return contractErr;
    if (err instanceof Error)
        return err;
    return new Error(String(err));
}
// ---------------------------------------------------------------------------
// useEscrow
// ---------------------------------------------------------------------------
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
export function useEscrow(transport, escrowId) {
    const [state, dispatch] = useReducer((asyncReducer), undefined, initial);
    const clientRef = useRef(null);
    // Keep client in sync with transport without creating a new one each render
    if (transport && clientRef.current === null) {
        clientRef.current = new EscrowClient(transport);
    }
    const fetch = useCallback(async () => {
        if (!clientRef.current || escrowId === null)
            return;
        dispatch({ type: "loading" });
        try {
            const data = await clientRef.current.get_escrow(escrowId);
            dispatch({ type: "success", payload: data });
        }
        catch (err) {
            dispatch({ type: "error", payload: normalizeError(err) });
        }
    }, [escrowId]);
    useEffect(() => {
        void fetch();
    }, [fetch]);
    return { ...state, refetch: fetch };
}
// ---------------------------------------------------------------------------
// useDispute
// ---------------------------------------------------------------------------
/**
 * Fetch the dispute record for an escrow. Returns `null` data when no
 * dispute has been raised yet.
 *
 * @example
 * ```tsx
 * const { data: dispute, loading } = useDispute(transport, escrowId);
 * ```
 */
export function useDispute(transport, escrowId) {
    const [state, dispatch] = useReducer((asyncReducer), undefined, initial);
    const clientRef = useRef(null);
    if (transport && clientRef.current === null) {
        clientRef.current = new EscrowClient(transport);
    }
    const fetch = useCallback(async () => {
        if (!clientRef.current || escrowId === null)
            return;
        dispatch({ type: "loading" });
        try {
            const data = await clientRef.current.get_dispute(escrowId);
            dispatch({ type: "success", payload: data });
        }
        catch (err) {
            dispatch({ type: "error", payload: normalizeError(err) });
        }
    }, [escrowId]);
    useEffect(() => {
        void fetch();
    }, [fetch]);
    return { ...state, refetch: fetch };
}
function mutationReducer(state, action) {
    switch (action.type) {
        case "loading":
            return { loading: true, error: null, success: false };
        case "success":
            return { loading: false, error: null, success: true };
        case "error":
            return { loading: false, error: action.payload, success: false };
        case "reset":
            return { loading: false, error: null, success: false };
    }
}
const initialMutation = { loading: false, error: null, success: false };
// ---------------------------------------------------------------------------
// useFundEscrow
// ---------------------------------------------------------------------------
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
export function useFundEscrow(transport) {
    const [state, dispatch] = useReducer(mutationReducer, initialMutation);
    const clientRef = useRef(null);
    if (transport && clientRef.current === null) {
        clientRef.current = new EscrowClient(transport);
    }
    const fund = useCallback(async (escrowId, buyer) => {
        if (!clientRef.current)
            return;
        dispatch({ type: "loading" });
        try {
            await clientRef.current.fund_escrow(escrowId, buyer);
            dispatch({ type: "success" });
        }
        catch (err) {
            dispatch({ type: "error", payload: normalizeError(err) });
        }
    }, []);
    const reset = useCallback(() => dispatch({ type: "reset" }), []);
    return { fund, loading: state.loading, error: state.error, success: state.success, reset };
}
// ---------------------------------------------------------------------------
// useConfirmDelivery
// ---------------------------------------------------------------------------
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
export function useConfirmDelivery(transport) {
    const [state, dispatch] = useReducer(mutationReducer, initialMutation);
    const clientRef = useRef(null);
    if (transport && clientRef.current === null) {
        clientRef.current = new EscrowClient(transport);
    }
    const confirm = useCallback(async (caller, escrowId) => {
        if (!clientRef.current)
            return;
        dispatch({ type: "loading" });
        try {
            await clientRef.current.confirm_delivery(caller, escrowId);
            dispatch({ type: "success" });
        }
        catch (err) {
            dispatch({ type: "error", payload: normalizeError(err) });
        }
    }, []);
    const reset = useCallback(() => dispatch({ type: "reset" }), []);
    return { confirm, loading: state.loading, error: state.error, success: state.success, reset };
}
// ---------------------------------------------------------------------------
// useRaiseDispute
// ---------------------------------------------------------------------------
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
export function useRaiseDispute(transport) {
    const [state, dispatch] = useReducer(mutationReducer, initialMutation);
    const clientRef = useRef(null);
    if (transport && clientRef.current === null) {
        clientRef.current = new EscrowClient(transport);
    }
    const raise = useCallback(async (caller, escrowId, reason, description, evidenceHash) => {
        if (!clientRef.current)
            return;
        dispatch({ type: "loading" });
        try {
            await clientRef.current.raise_dispute(caller, escrowId, reason, description, evidenceHash);
            dispatch({ type: "success" });
        }
        catch (err) {
            dispatch({ type: "error", payload: normalizeError(err) });
        }
    }, []);
    const reset = useCallback(() => dispatch({ type: "reset" }), []);
    return { raise, loading: state.loading, error: state.error, success: state.success, reset };
}
