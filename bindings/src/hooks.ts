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
import type { EscrowData, DisputeData } from "./types.js";
import type { ContractTransport } from "./client.js";
import { EscrowClient } from "./client.js";
import { parseContractError } from "./errors.js";

// ---------------------------------------------------------------------------
// Shared async-state helpers
// ---------------------------------------------------------------------------

interface AsyncState<T> {
  data: T | null;
  loading: boolean;
  error: Error | null;
}

type AsyncAction<T> =
  | { type: "loading" }
  | { type: "success"; payload: T }
  | { type: "error"; payload: Error };

function asyncReducer<T>(state: AsyncState<T>, action: AsyncAction<T>): AsyncState<T> {
  switch (action.type) {
    case "loading":
      return { data: state.data, loading: true, error: null };
    case "success":
      return { data: action.payload, loading: false, error: null };
    case "error":
      return { data: null, loading: false, error: action.payload };
  }
}

const initial = <T>(): AsyncState<T> => ({ data: null, loading: false, error: null });

function normalizeError(err: unknown): Error {
  const contractErr = parseContractError(err);
  if (contractErr) return contractErr;
  if (err instanceof Error) return err;
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
export function useEscrow(
  transport: ContractTransport | null,
  escrowId: bigint | null,
): AsyncState<EscrowData> & { refetch: () => void } {
  const [state, dispatch] = useReducer(asyncReducer<EscrowData>, undefined, initial);
  const clientRef = useRef<EscrowClient | null>(null);

  // Keep client in sync with transport without creating a new one each render
  if (transport && clientRef.current === null) {
    clientRef.current = new EscrowClient(transport);
  }

  const fetch = useCallback(async () => {
    if (!clientRef.current || escrowId === null) return;
    dispatch({ type: "loading" });
    try {
      const data = await clientRef.current.get_escrow(escrowId);
      dispatch({ type: "success", payload: data });
    } catch (err) {
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
export function useDispute(
  transport: ContractTransport | null,
  escrowId: bigint | null,
): AsyncState<DisputeData | null> & { refetch: () => void } {
  const [state, dispatch] = useReducer(asyncReducer<DisputeData | null>, undefined, initial);
  const clientRef = useRef<EscrowClient | null>(null);

  if (transport && clientRef.current === null) {
    clientRef.current = new EscrowClient(transport);
  }

  const fetch = useCallback(async () => {
    if (!clientRef.current || escrowId === null) return;
    dispatch({ type: "loading" });
    try {
      const data = await clientRef.current.get_dispute(escrowId);
      dispatch({ type: "success", payload: data });
    } catch (err) {
      dispatch({ type: "error", payload: normalizeError(err) });
    }
  }, [escrowId]);

  useEffect(() => {
    void fetch();
  }, [fetch]);

  return { ...state, refetch: fetch };
}

// ---------------------------------------------------------------------------
// useMutation — internal generic mutation helper
// ---------------------------------------------------------------------------

interface MutationState {
  loading: boolean;
  error: Error | null;
  success: boolean;
}

type MutationAction =
  | { type: "loading" }
  | { type: "success" }
  | { type: "error"; payload: Error }
  | { type: "reset" };

function mutationReducer(state: MutationState, action: MutationAction): MutationState {
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

const initialMutation: MutationState = { loading: false, error: null, success: false };

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
export function useFundEscrow(transport: ContractTransport | null): {
  fund: (escrowId: bigint, buyer: string) => Promise<void>;
  loading: boolean;
  error: Error | null;
  success: boolean;
  reset: () => void;
} {
  const [state, dispatch] = useReducer(mutationReducer, initialMutation);
  const clientRef = useRef<EscrowClient | null>(null);

  if (transport && clientRef.current === null) {
    clientRef.current = new EscrowClient(transport);
  }

  const fund = useCallback(
    async (escrowId: bigint, buyer: string) => {
      if (!clientRef.current) return;
      dispatch({ type: "loading" });
      try {
        await clientRef.current.fund_escrow(escrowId, buyer);
        dispatch({ type: "success" });
      } catch (err) {
        dispatch({ type: "error", payload: normalizeError(err) });
      }
    },
    [],
  );

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
export function useConfirmDelivery(transport: ContractTransport | null): {
  confirm: (caller: string, escrowId: bigint) => Promise<void>;
  loading: boolean;
  error: Error | null;
  success: boolean;
  reset: () => void;
} {
  const [state, dispatch] = useReducer(mutationReducer, initialMutation);
  const clientRef = useRef<EscrowClient | null>(null);

  if (transport && clientRef.current === null) {
    clientRef.current = new EscrowClient(transport);
  }

  const confirm = useCallback(
    async (caller: string, escrowId: bigint) => {
      if (!clientRef.current) return;
      dispatch({ type: "loading" });
      try {
        await clientRef.current.confirm_delivery(caller, escrowId);
        dispatch({ type: "success" });
      } catch (err) {
        dispatch({ type: "error", payload: normalizeError(err) });
      }
    },
    [],
  );

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
export function useRaiseDispute(transport: ContractTransport | null): {
  raise: (
    escrowId: bigint,
    reason: string,
    description: string,
    evidenceHash: Uint8Array,
  ) => Promise<void>;
  loading: boolean;
  error: Error | null;
  success: boolean;
  reset: () => void;
} {
  const [state, dispatch] = useReducer(mutationReducer, initialMutation);
  const clientRef = useRef<EscrowClient | null>(null);

  if (transport && clientRef.current === null) {
    clientRef.current = new EscrowClient(transport);
  }

  const raise = useCallback(
    async (escrowId: bigint, reason: string, description: string, evidenceHash: Uint8Array) => {
      if (!clientRef.current) return;
      dispatch({ type: "loading" });
      try {
        await clientRef.current.raise_dispute(escrowId, reason, description, evidenceHash);
        dispatch({ type: "success" });
      } catch (err) {
        dispatch({ type: "error", payload: normalizeError(err) });
      }
    },
    [],
  );

  const reset = useCallback(() => dispatch({ type: "reset" }), []);

  return { raise, loading: state.loading, error: state.error, success: state.success, reset };
}
