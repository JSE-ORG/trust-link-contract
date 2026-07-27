/**
 * Machine-readable description of the TrustLink escrow contract surface.
 *
 * Mirrors every `pub fn` inside the `#[contractimpl]` block of
 * contracts/escrow/src/lib.rs.  The implicit `env: Env` first argument is not
 * listed — `inputs` contains only the arguments a caller actually supplies.
 *
 * NOTE: functions returning `Result<T, ContractError>` are listed with their
 * success type `T`; `void` means `Result<(), ContractError>` or `()`.
 */

export const contractName = "Escrow";

export interface AbiFunction {
  readonly name: string;
  readonly inputs: readonly string[];
  readonly output: string;
}

export const contractAbi = {
  contractName,
  functions: [
    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------
    { name: "get_version", inputs: [], output: "u32" },

    // -----------------------------------------------------------------------
    // Initialization & admin
    // -----------------------------------------------------------------------
    {
      name: "initialize",
      inputs: ["admin", "fee_collector", "arbitration_fee_bps"],
      output: "void",
    },
    { name: "set_admin", inputs: ["new_admin"], output: "void" },
    { name: "upgrade", inputs: ["caller", "new_wasm_hash"], output: "void" },

    // -----------------------------------------------------------------------
    // Pause controls
    // -----------------------------------------------------------------------
    { name: "pause_contract", inputs: ["caller"], output: "void" },
    { name: "unpause_contract", inputs: ["caller"], output: "void" },
    { name: "is_paused", inputs: [], output: "bool" },
    { name: "pause_action", inputs: ["caller", "action"], output: "void" },
    { name: "unpause_action", inputs: ["caller", "action"], output: "void" },
    { name: "is_action_paused", inputs: ["action"], output: "bool" },

    // -----------------------------------------------------------------------
    // Fee / treasury configuration
    // -----------------------------------------------------------------------
    { name: "set_fee", inputs: ["caller", "fee_bps"], output: "void" },
    { name: "set_protocol_fee", inputs: ["caller", "fee_bps"], output: "void" },
    { name: "set_platform_fee", inputs: ["caller", "fee_bps"], output: "void" },
    { name: "set_arbitration_fee", inputs: ["caller", "fee_bps"], output: "void" },
    { name: "set_fee_collector", inputs: ["new_collector"], output: "void" },
    { name: "set_treasury", inputs: ["caller", "treasury"], output: "void" },
    { name: "set_ttl_extension", inputs: ["caller", "ledgers"], output: "void" },
    {
      name: "set_amount_limits",
      inputs: ["caller", "min_amount", "max_amount"],
      output: "void",
    },
    { name: "get_fee_config", inputs: [], output: "FeeConfig" },
    { name: "get_platform_fee_bps", inputs: [], output: "u32" },
    { name: "get_arbitration_fee", inputs: [], output: "u32" },
    { name: "get_total_arbitration_fees", inputs: ["token"], output: "i128" },
    { name: "get_treasury", inputs: [], output: "Address" },

    // -----------------------------------------------------------------------
    // Escrow creation
    // -----------------------------------------------------------------------
    {
      name: "create_escrow",
      inputs: [
        "seller_or_payees",
        "buyer",
        "resolver",
        "token",
        "amount",
        "fee_bps",
        "resolver_fee_bps",
        "shipping_window",
        "notes",
      ],
      output: "u64",
    },
    {
      name: "create_escrow_8",
      inputs: [
        "seller_or_payees",
        "buyer",
        "resolver",
        "token",
        "amount",
        "fee_bps",
        "shipping_window",
      ],
      output: "u64",
    },
    {
      name: "create_escrow_7",
      inputs: ["seller_or_payees", "buyer", "resolver", "token", "amount", "fee_bps"],
      output: "u64",
    },
    {
      name: "create_escrow_with_expiration",
      inputs: [
        "seller",
        "buyer",
        "resolver",
        "token",
        "amount",
        "fee_bps",
        "shipping_window",
        "expires_at",
        "grace_period",
      ],
      output: "u64",
    },
    {
      name: "create_escrow_multi",
      inputs: [
        "seller",
        "buyer",
        "resolvers",
        "threshold",
        "token",
        "amount",
        "fee_bps",
        "shipping_window",
      ],
      output: "u64",
    },
    {
      name: "create_escrow_with_fallback",
      inputs: [
        "seller",
        "buyer",
        "primary_resolver",
        "backup_resolver",
        "dispute_deadline",
        "token",
        "amount",
        "fee_bps",
        "shipping_window",
      ],
      output: "u64",
    },
    {
      name: "create_basket_escrow",
      inputs: ["seller", "buyer", "resolver", "tokens", "amounts", "fee_bps", "shipping_window"],
      output: "u64",
    },
    { name: "batch_create_escrow", inputs: ["seller", "escrows"], output: "Vec<u64>" },

    // -----------------------------------------------------------------------
    // Escrow lifecycle
    // -----------------------------------------------------------------------
    { name: "fund_escrow", inputs: ["escrow_id", "buyer"], output: "void" },
    { name: "fund_basket_escrow", inputs: ["escrow_id", "buyer"], output: "void" },
    { name: "cancel_escrow", inputs: ["caller", "escrow_id"], output: "void" },
    { name: "mutual_cancel", inputs: ["escrow_id"], output: "void" },
    { name: "mark_shipped", inputs: ["caller", "escrow_id", "tracking_id"], output: "void" },
    { name: "record_delivery", inputs: ["caller", "escrow_id"], output: "void" },
    { name: "confirm_delivery", inputs: ["caller", "escrow_id"], output: "void" },
    { name: "co_signed_release", inputs: ["caller", "escrow_id"], output: "void" },
    { name: "auto_release", inputs: ["escrow_id"], output: "void" },
    { name: "request_refund", inputs: ["caller", "escrow_id"], output: "void" },
    { name: "approve_refund", inputs: ["caller", "escrow_id"], output: "void" },
    { name: "emergency_drain", inputs: ["escrow_id"], output: "void" },

    // -----------------------------------------------------------------------
    // Disputes & resolvers
    // -----------------------------------------------------------------------
    {
      name: "raise_dispute",
      inputs: ["caller", "escrow_id", "reason", "description", "evidence_hash"],
      output: "void",
    },
    {
      name: "resolve_dispute",
      inputs: ["caller", "escrow_id", "resolution"],
      output: "void",
    },
    { name: "vote", inputs: ["caller", "escrow_id", "resolution"], output: "void" },
    { name: "finalize_dispute", inputs: ["caller", "escrow_id"], output: "void" },
    { name: "appeal_dispute", inputs: ["caller", "escrow_id"], output: "void" },
    { name: "rotate_resolver", inputs: ["caller", "escrow_id", "new_resolver"], output: "void" },
    { name: "get_resolver_votes", inputs: ["escrow_id"], output: "Vec<ResolverVote>" },
    { name: "add_approved_resolver", inputs: ["caller", "resolver"], output: "void" },
    { name: "remove_approved_resolver", inputs: ["caller", "resolver"], output: "void" },
    { name: "set_resolver_strict", inputs: ["caller", "strict"], output: "void" },
    { name: "get_approved_resolvers", inputs: [], output: "Vec<Address>" },
    { name: "is_resolver_strict", inputs: [], output: "bool" },

    // -----------------------------------------------------------------------
    // Token allowlist
    // -----------------------------------------------------------------------
    { name: "set_token_allowlist_enabled", inputs: ["caller", "enabled"], output: "void" },
    { name: "add_allowed_token", inputs: ["caller", "token"], output: "void" },
    { name: "remove_allowed_token", inputs: ["caller", "token"], output: "void" },
    { name: "is_token_allowlist_enabled", inputs: [], output: "bool" },
    { name: "get_allowed_tokens", inputs: [], output: "Vec<Address>" },

    // -----------------------------------------------------------------------
    // Messaging
    // -----------------------------------------------------------------------
    { name: "post_message", inputs: ["escrow_id", "sender", "content"], output: "void" },
    { name: "get_messages", inputs: ["escrow_id", "start", "limit"], output: "Vec<Message>" },

    // -----------------------------------------------------------------------
    // Batching
    // -----------------------------------------------------------------------
    { name: "multicall", inputs: ["calls"], output: "Vec<Val>" },

    // -----------------------------------------------------------------------
    // Read accessors
    // -----------------------------------------------------------------------
    { name: "get_escrow", inputs: ["escrow_id"], output: "EscrowData" },
    { name: "get_dispute", inputs: ["escrow_id"], output: "Option<DisputeData>" },
    { name: "get_state_history", inputs: ["escrow_id"], output: "Vec<[EscrowState, u64]>" },
    { name: "get_escrows_by_buyer", inputs: ["buyer"], output: "Vec<u64>" },
    { name: "get_escrows_by_seller", inputs: ["seller"], output: "Vec<u64>" },
    { name: "get_escrows_by_vendor", inputs: ["vendor"], output: "Vec<u64>" },
    { name: "get_escrows_by_ids", inputs: ["ids"], output: "Vec<Option<EscrowData>>" },
    { name: "get_basket_tokens", inputs: ["escrow_id"], output: "Vec<TokenEntry>" },
    { name: "get_stats", inputs: [], output: "ContractStats" },
    { name: "get_public_config", inputs: [], output: "PublicContractConfig" },
    { name: "get_contract_config", inputs: [], output: "ContractConfig" },
  ] as const satisfies readonly AbiFunction[],
  types: [
    "ContractCall",
    "ContractConfig",
    "ContractError",
    "ContractStats",
    "DisputeData",
    "DisputeStatus",
    "EscrowData",
    "EscrowInput",
    "EscrowState",
    "FallbackResolver",
    "FeeConfig",
    "Message",
    "MultiResolver",
    "Payee",
    "PublicContractConfig",
    "ResolutionType",
    "ResolverSet",
    "ResolverVote",
    "TokenEntry",
  ],
} as const;

/** Every function name exposed by the contract. */
export type ContractFunctionName = (typeof contractAbi.functions)[number]["name"];
