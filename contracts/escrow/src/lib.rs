#![no_std]
#![allow(clippy::too_many_arguments)]
use crate::internal::{
    add_or_update_vote, ensure_action_not_paused, execute_resolution_transition, get_ttl_extension,
    load_escrow, save_resolver_votes, tally_votes,
};
use soroban_sdk::{contract, contracttype, Address, Env, Symbol, Val, Vec};

pub mod errors;
pub mod events;
pub mod helpers;
pub mod storage;
pub mod types;

mod admin;
mod disputes;
mod instructions;
mod internal;
mod queries;
pub use crate::errors::ContractError;
pub use crate::events::{
    emit_action_paused, emit_action_unpaused, emit_admin_rotated, emit_allowlist_toggled,
    emit_amount_limits_updated, emit_arbitration_fee_updated, emit_auto_released,
    emit_basket_escrow_created, emit_contract_initialized, emit_contract_paused,
    emit_contract_unpaused, emit_contract_upgraded, emit_delivery_proposal_cancelled,
    emit_delivery_proposed, emit_delivery_recorded, emit_dispute_appealed,
    emit_dispute_pending_finalization, emit_dispute_raised, emit_dispute_resolved,
    emit_emergency_drain, emit_escrow_auto_canceled, emit_escrow_cancelled, emit_escrow_completed,
    emit_escrow_created, emit_escrow_expired, emit_escrow_funded, emit_escrow_shipped,
    emit_fee_collector_updated, emit_fee_updated, emit_platform_fee_updated,
    emit_protocol_fee_updated, emit_refund_approved, emit_refund_requested, emit_resolver_approved,
    emit_resolver_removed, emit_resolver_rotated, emit_resolver_strict_updated,
    emit_resolver_vote_recorded, emit_storage_migrated, emit_token_allowlist_updated,
    emit_treasury_updated, emit_ttl_extension_updated, ActionPausedEvent, ActionUnpausedEvent,
    AdminRotated, AmountLimitsUpdated, ArbitrationFeeUpdated, AutoReleased, ContractInitialized,
    ContractPausedEvent, ContractUnpausedEvent, ContractUpgradedEvent, DeliveryProposalCancelled,
    DeliveryProposed, DeliveryRecorded, DisputeRaised, DisputeResolved, EscrowAutoCanceled,
    EscrowCancelled, EscrowCompleted, EscrowCreated, EscrowExpired, EscrowFunded, EscrowShipped,
    FeeUpdated, ProtocolFeeUpdated, ResolverApproved, ResolverRemoved, ResolverRotated,
    ResolverStrictUpdated, ResolverVoteRecorded, TtlExtensionUpdated,
};
pub use crate::types::{
    ContractConfig, ContractStats, DataKey, DisputeData, DisputeStatus, EscrowData, EscrowInput,
    EscrowState, ExpirySchedule, FeeConfig, Payee, PublicContractConfig, ResolutionType,
    ResolverSet, ResolverVote, TokenEntry,
};

/// A single call descriptor used by the `multicall` batching function.
/// `function` is the name of the contract method to invoke; `args` are its
/// Soroban-serialised arguments.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractCall {
    pub function: Symbol,
    pub args: Vec<Val>,
}

/// Maximum escrow fee in basis points (300 = 3%).
///
/// This applies to the per-escrow `fee_bps` value supplied at creation time,
/// and to the legacy `set_fee` helper that persists `DefaultFeeBps`.
const MAX_ESCROW_FEE_BPS: u32 = 300;

/// Maximum protocol fee in basis points (500 = 5%).
///
/// Protocol fees are deducted from escrow payouts during delivery/resolution.
/// Capped at 5% to ensure meaningful payouts to winners.
const MAX_PROTOCOL_FEE_BPS: u32 = 500;

/// Maximum arbitration fee in basis points (500 = 5%).
///
/// Arbitration fees are deducted from escrows during dispute resolution.
/// Capped at 5% to preserve incentive alignment in dispute outcomes.
const MAX_ARBITRATION_FEE_BPS: u32 = 500;

/// Maximum combined protocol + arbitration fee in basis points (1000 = 10%).
///
/// Ensures that protocol_fee_bps + arbitration_fee_bps cannot exceed 10%,
/// preventing the malicious admin attack where combined fees drain entire escrows.
const MAX_COMBINED_FEE_BPS: u32 = 1_000;

/// The semantic version of the contract.
pub const CONTRACT_VERSION: u32 = 1;

/// The on-chain storage schema version this build expects.
///
/// Bump this whenever the layout of a stored type changes, and extend
/// [`Escrow::migrate`] with the corresponding step. Contracts deployed before
/// versioning existed report `0`; see `docs/UPGRADES.md`.
pub const STORAGE_VERSION: u32 = 1;

/// Maximum platform fee in basis points (200 = 2%).
///
/// Platform fees are per-escrow fees forwarded to the treasury on successful release.
/// Capped at 2% to ensure meaningful payouts to sellers.
const MAX_PLATFORM_FEE_BPS: u32 = 200;

/// Appeal window duration in seconds (86400 = 24 hours).
///
/// After a dispute is resolved, the losing party has this window to appeal.
const APPEAL_WINDOW: u64 = 86_400;

/// Minimum escrow amount in stroops.
/// Keeps the contract from accepting zero or negative escrows.
pub const MIN_ESCROW_AMOUNT: i128 = 1;

/// Length of the dispute window in seconds (172_800 = 48 hours).
///
/// On `fund_escrow` the contract sets `dispute_deadline = funded_at +
/// DISPUTE_WINDOW`. Until that deadline the buyer may `raise_dispute`, and
/// `confirm_delivery` is rejected; once the deadline passes the funds become
/// releasable to the seller.
const DISPUTE_WINDOW: u64 = 172_800;
const DELIVERY_RELEASE_WINDOW: u64 = 172_800;
const DEFAULT_TTL_EXTENSION: u32 = 120_960;
/// Divisor used when computing the threshold for TTL extension.
/// TTL is extended to `ext / TTL_THRESHOLD_DIVISOR` on the low end,
/// giving the contract a window to re-extend before the entry expires.
const TTL_THRESHOLD_DIVISOR: u32 = 2;
/// How long (in seconds) a Pending escrow waits for funding before it can be
/// auto-cancelled.  Default: 7 days.
const PENDING_EXPIRY_WINDOW: u64 = 604_800;

/// Maximum number of entries kept in an escrow's state history.
/// Once reached, the oldest entry is dropped for each new one appended,
/// bounding storage size for high-churn escrows (e.g. disputed <->
/// pending_finalization cycles).
const MAX_STATE_HISTORY_ENTRIES: u32 = 50;

/// Basis points denominator (100% = 10_000 basis points).
pub const BASIS_POINTS: u32 = 10_000;
pub const DELIVERY_TIMELOCK: u64 = 86_400;

/// Maximum length for user-supplied string fields.
/// - `tracking_id`: 64 characters
/// - `description` in `raise_dispute`: 256 characters
/// - `notes`: 500 characters
pub const MAX_TRACKING_ID_LEN: u32 = 64;
pub const MAX_DESCRIPTION_LEN: u32 = 256;
pub const MAX_NOTES_LEN: u32 = 500;
pub const MAX_MESSAGE_LEN: u32 = 500;

/// Minimum shipping window in seconds (1 second).
/// A value of 0 would allow an immediate dispute with no shipping time, which is invalid.
pub const MIN_SHIPPING_WINDOW: u64 = 1;

/// Maximum shipping window in seconds (approximately 2 years).
/// Prevents accidental or malicious use of u64::MAX which would lock funds indefinitely.
pub const MAX_SHIPPING_WINDOW: u64 = 63_072_000;

/// Maximum escrow amount intentionally capped to
/// preserve arithmetic safety for fee calculations
/// and aggregate accounting operations.
pub const MAX_ESCROW_AMOUNT: i128 = i128::MAX / 10_000;

#[contract]
pub struct Escrow;

/// Maximum number of appeals allowed per dispute.
pub const MAX_APPEALS: u32 = 3;

/// Zero address string for the Stellar network.
pub const ZERO_ADDRESS_STR: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

pub(crate) fn next_escrow_id(env: &Env) -> Result<u64, ContractError> {
    let escrow_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::EscrowCounter)
        .unwrap_or(1u64);
    let next_id = escrow_id
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage()
        .instance()
        .set(&DataKey::EscrowCounter, &next_id);

    let ext = get_ttl_extension(env);
    env.storage().instance().extend_ttl(ext / 2, ext);

    Ok(escrow_id)
}

fn resolve_or_vote_internal(
    env: &Env,
    caller: Address,
    escrow_id: u64,
    resolution: ResolutionType,
) -> Result<(), ContractError> {
    caller.require_auth();
    ensure_action_not_paused(env, Symbol::new(env, "RESOLVE"))?;
    let escrow = load_escrow(env, escrow_id)?;

    if escrow.state != EscrowState::Disputed {
        return Err(ContractError::InvalidState);
    }

    if !escrow
        .resolvers
        .can_resolve_now(&caller, env.ledger().timestamp())
    {
        return Err(ContractError::NotAuthorized);
    }

    let votes = add_or_update_vote(env, escrow_id, &caller, resolution.clone());
    let threshold = escrow.resolvers.threshold();

    emit_resolver_vote_recorded(
        env,
        escrow_id,
        caller.clone(),
        resolution.clone(),
        votes.len(),
        threshold,
    );

    if let Some(final_resolution) = tally_votes(&votes, threshold) {
        execute_resolution_transition(env, escrow_id, escrow, caller, final_resolution, votes)?;
    } else {
        save_resolver_votes(env, escrow_id, &votes);
    }

    Ok(())
}

mod malicious_token;
mod test;
mod test_admin;
mod test_admin_event_emissions;
mod test_admin_rotation;
mod test_arbitration_fee;
mod test_auth_matrix;
mod test_auth_ordering;
mod test_auto_release;
mod test_auto_release_additional;
mod test_basket_escrow;
mod test_cancel_restrictions;
mod test_co_signed_release;
mod test_concurrent_vendor_escrows;
mod test_contract_config;
mod test_create_escrow_boundary;
mod test_create_escrow_with_expiration;
mod test_delivery;
mod test_dispute;
mod test_dispute_deadline_overflow;
mod test_dispute_flow;
mod test_dispute_window;
mod test_edge_cases;
mod test_emergency_drain;
mod test_escrow_id;
mod test_escrow_states;
mod test_expiration;
mod test_fallback_resolver;
mod test_fee_calculation_accuracy;
mod test_fee_config;
mod test_fee_minimum;
mod test_finalize_dispute_appeal_boundary;
mod test_get_escrows_by_buyer;
mod test_get_escrows_by_ids;
mod test_get_escrows_by_seller;
mod test_get_escrows_by_vendor;
mod test_helpers;
mod test_initialize_twice;
mod test_initialize_zero_admin;
mod test_malicious_token;
mod test_minimum_amount_guard;
mod test_not_found;
mod test_overflow;
mod test_pause;
mod test_pending_expiry;
mod test_resolution;
mod test_resolver_registry;
mod test_resolver_rotation;
mod test_set_fee_boundary;
mod test_set_fee_collector;
mod test_shipping_window;
mod test_state_history;
mod test_unauthorized;
mod test_upgrade_migration;
mod test_vote;
