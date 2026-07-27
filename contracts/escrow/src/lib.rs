#![no_std]
#![allow(clippy::too_many_arguments)]
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, BytesN, Env, IntoVal, String, Symbol,
    TryFromVal, TryIntoVal, Val, Vec,
};

// Added import for Message
use crate::events::emit_message_posted;
use crate::types::Message;

pub mod errors;
pub mod events;
pub mod helpers;
pub mod storage;
pub mod types;
pub use crate::errors::ContractError;
pub use crate::events::{
    emit_action_paused, emit_action_unpaused, emit_admin_rotated, emit_allowlist_toggled,
    emit_amount_limits_updated, emit_arbitration_fee_updated, emit_auto_released,
    emit_basket_escrow_created, emit_contract_initialized, emit_contract_paused,
    emit_contract_unpaused, emit_contract_upgraded, emit_delivery_recorded, emit_dispute_appealed,
    emit_dispute_pending_finalization, emit_dispute_raised, emit_dispute_resolved,
    emit_emergency_drain, emit_escrow_cancelled, emit_escrow_completed, emit_escrow_created,
    emit_escrow_funded, emit_escrow_shipped, emit_fee_collector_updated, emit_fee_updated,
    emit_platform_fee_updated, emit_protocol_fee_updated, emit_refund_approved,
    emit_refund_requested, emit_resolver_approved, emit_resolver_removed, emit_resolver_rotated,
    emit_resolver_strict_updated, emit_resolver_vote_recorded, emit_storage_migrated,
    emit_token_allowlist_updated, emit_treasury_updated, emit_ttl_extension_updated,
    ActionPausedEvent, ActionUnpausedEvent, AdminRotated, AmountLimitsUpdated,
    ArbitrationFeeUpdated, AutoReleased, ContractInitialized, ContractPausedEvent,
    ContractUnpausedEvent, ContractUpgradedEvent, DeliveryRecorded, DisputeRaised,
    DisputeResolved, EmergencyDrain, EscrowCancelled, EscrowCompleted, EscrowCreated,
    EscrowFunded, EscrowShipped, FeeCollectorUpdated, FeeUpdated, ProtocolFeeUpdated,
    ResolverApproved, ResolverRemoved, ResolverRotated, ResolverStrictUpdated,
    ResolverVoteRecorded, TtlExtensionUpdated,
};
pub use crate::types::{
    ContractConfig, ContractStats, DataKey, DisputeData, DisputeStatus, EscrowData, EscrowInput,
    EscrowState, FeeConfig, Payee, PublicContractConfig, ResolutionType, ResolverSet, ResolverVote,
    TokenEntry,
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
#[allow(dead_code)]
const PENDING_EXPIRY_WINDOW: u64 = 604_800;

/// Maximum number of entries kept in an escrow's state history.
/// Once reached, the oldest entry is dropped for each new one appended,
/// bounding storage size for high-churn escrows (e.g. disputed <->
/// pending_finalization cycles).
const MAX_STATE_HISTORY_ENTRIES: u32 = 50;

/// Maximum length for user-supplied string fields.
/// - `tracking_id`: 64 characters
/// - `description` in `raise_dispute`: 256 characters
/// - `notes`: 500 characters
pub const MAX_TRACKING_ID_LEN: u32 = 64;
pub const MAX_DESCRIPTION_LEN: u32 = 256;
pub const MAX_NOTES_LEN: u32 = 500;

/// Minimum shipping window in seconds (1 second).
/// A value of 0 would allow an immediate dispute with no shipping time, which is invalid.
pub const MIN_SHIPPING_WINDOW: u64 = 1;

/// Maximum shipping window in seconds (approximately 2 years).
/// Prevents accidental or malicious use of u64::MAX which would lock funds indefinitely.
pub const MAX_SHIPPING_WINDOW: u64 = 63_072_000;

/// Maximum escrow amount intentionally capped to
/// preserve arithmetic safety for fee calculations
/// and aggregate accounting operations.
pub const MAX_ESCROW_AMOUNT: i128 = i128::MAX / BASIS_POINTS as i128;

/// Basis points denominator (100% = 10_000 basis points).
pub const BASIS_POINTS: u32 = 10_000;

// ============================================================================
// MULTI-RESOLVER VOTING HELPERS
// ============================================================================

/// Load resolver votes for an escrow from storage
fn load_resolver_votes(env: &Env, escrow_id: u64) -> Vec<ResolverVote> {
    use crate::DataKey;
    env.storage()
        .persistent()
        .get(&DataKey::ResolverVotes(escrow_id))
        .unwrap_or(Vec::new(env))
}

/// Save resolver votes to storage
fn save_resolver_votes(env: &Env, escrow_id: u64, votes: &Vec<ResolverVote>) {
    use crate::DataKey;
    env.storage()
        .persistent()
        .set(&DataKey::ResolverVotes(escrow_id), votes);
    // Extend TTL for votes
    let ext = get_ttl_extension(env);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::ResolverVotes(escrow_id), ext / 2, ext);
}

/// Add or update a vote from a resolver
fn add_or_update_vote(
    env: &Env,
    escrow_id: u64,
    resolver: &Address,
    resolution: ResolutionType,
) -> Vec<ResolverVote> {
    let mut votes = load_resolver_votes(env, escrow_id);
    let current_time = env.ledger().timestamp();

    // Check if this resolver already voted
    let mut found = false;
    for i in 0..votes.len() {
        if let Some(vote) = votes.get(i) {
            if vote.resolver == *resolver {
                // Update existing vote
                let mut updated = vote.clone();
                updated.resolution = resolution.clone();
                updated.voted_at = current_time;
                votes.set(i, updated);
                found = true;
                break;
            }
        }
    }

    if !found {
        // Add new vote
        votes.push_back(ResolverVote {
            resolver: resolver.clone(),
            resolution,
            voted_at: current_time,
        });
    }

    votes
}

/// Tally votes and determine if resolution should be executed
/// Returns the winning resolution if threshold is met
fn tally_votes(votes: &Vec<ResolverVote>, threshold: u32) -> Option<ResolutionType> {
    if votes.is_empty() {
        return None;
    }

    let mut release_count = 0u32;
    let mut refund_count = 0u32;

    for i in 0..votes.len() {
        if let Some(vote) = votes.get(i) {
            match vote.resolution {
                ResolutionType::Release => release_count = release_count.saturating_add(1),
                ResolutionType::Refund => refund_count = refund_count.saturating_add(1),
            }
        }
    }

    if release_count >= threshold {
        Some(ResolutionType::Release)
    } else if refund_count >= threshold {
        Some(ResolutionType::Refund)
    } else {
        None
    }
}

// ============================================================================
// STATE MACHINE VALIDATION
// ============================================================================

/// Validity matrix for escrow state transitions (#9).
///
/// Returns `Ok(())` if the move from `from` to `to` is legal under the
/// escrow lifecycle, `Err(InvalidStateTransition)` otherwise. Provided as a
/// pure helper alongside the existing inline guards so reviewers can audit
/// every legal edge in one place.
pub fn transition_state(from: &EscrowState, to: &EscrowState) -> Result<(), ContractError> {
    use EscrowState::*;
    let allowed = matches!(
        (from, to),
        (Pending, Funded)
            | (Pending, Canceled)
            | (Funded, Shipped)
            | (Funded, Disputed)
            | (Funded, Refunded)
            | (Funded, RefundRequested)
            | (RefundRequested, Refunded)
            | (Shipped, Completed)
            | (Shipped, Disputed)
            | (Shipped, Refunded)
            | (Disputed, Completed)
            | (Disputed, Refunded)
            | (Disputed, PendingFinalization)
            | (PendingFinalization, Completed)
            | (PendingFinalization, Refunded)
            | (PendingFinalization, Disputed)
    );
    if allowed {
        Ok(())
    } else {
        Err(ContractError::InvalidStateTransition)
    }
}

#[contract]
pub struct Escrow;

fn ensure_not_paused(env: &Env) -> Result<(), ContractError> {
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

fn ensure_action_not_paused(env: &Env, action: Symbol) -> Result<(), ContractError> {
    ensure_not_paused(env)?;
    let action_paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::ActionPaused(action))
        .unwrap_or(false);
    if action_paused {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

fn require_admin(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(ContractError::NotAuthorized)
}

fn require_admin_caller(env: &Env, caller: &Address) -> Result<Address, ContractError> {
    let admin = require_admin(env)?;
    if caller != &admin {
        return Err(ContractError::NotAuthorized);
    }
    Ok(admin)
}

fn default_fee_config() -> FeeConfig {
    FeeConfig {
        protocol_fee_bps: 0,
        arbitration_fee_bps: 0,
    }
}

fn read_fee_config(env: &Env) -> FeeConfig {
    env.storage()
        .instance()
        .get(&DataKey::FeeConfig)
        .unwrap_or_else(default_fee_config)
}

fn write_fee_config(env: &Env, fee_config: &FeeConfig) {
    env.storage()
        .instance()
        .set(&DataKey::FeeConfig, fee_config);
}

fn is_token_allowlist_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::TokenAllowlistEnabled)
        .unwrap_or(false)
}

fn is_token_allowed(env: &Env, token: &Address) -> Result<(), ContractError> {
    if !is_token_allowlist_enabled(env) {
        return Ok(());
    }
    let allowlist: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::TokenAllowlist)
        .unwrap_or(soroban_sdk::Vec::new(env));
    for allowed_token in allowlist.iter() {
        if allowed_token == *token {
            return Ok(());
        }
    }
    Err(ContractError::TokenNotAllowed)
}

fn read_platform_fee_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::PlatformFeeBps)
        .unwrap_or(0)
}

fn write_platform_fee_bps(env: &Env, fee_bps: u32) {
    env.storage()
        .instance()
        .set(&DataKey::PlatformFeeBps, &fee_bps);
}

fn read_treasury(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .instance()
        .get(&DataKey::Treasury)
        .ok_or(ContractError::NotAuthorized)
}

fn write_treasury(env: &Env, treasury: &Address) {
    env.storage().instance().set(&DataKey::Treasury, treasury);
}

fn validate_escrow_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_ESCROW_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

/// Validates resolver set to ensure no conflicts with seller/buyer.
fn validate_resolvers(
    resolvers: &ResolverSet,
    seller: &Address,
    buyer: &Option<Address>,
) -> Result<(), ContractError> {
    // Ensure resolvers are distinct from seller and buyer
    if resolvers.contains(seller) {
        return Err(ContractError::ConflictingRoles);
    }

    if let Some(ref b) = buyer {
        if resolvers.contains(b) {
            return Err(ContractError::ConflictingRoles);
        }
    }

    // For multi-resolver, validate threshold
    if let ResolverSet::Multi(m) = resolvers {
        let count = m.resolvers.len();
        if m.threshold == 0 || m.threshold > count {
            return Err(ContractError::InvalidAmount); // Use as proxy for invalid threshold
        }

        // Ensure all resolvers are unique
        for i in 0..m.resolvers.len() {
            for j in (i + 1)..m.resolvers.len() {
                if m.resolvers.get(i).ok_or(ContractError::IndexOutOfBounds)?
                    == m.resolvers.get(j).ok_or(ContractError::IndexOutOfBounds)?
                {
                    return Err(ContractError::ConflictingRoles);
                }
            }
        }
    }

    Ok(())
}

fn validate_resolver_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_ESCROW_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

fn validate_payees(env: &Env, payees: &Vec<Payee>) -> Result<(), ContractError> {
    if payees.is_empty() {
        return Err(ContractError::InvalidAddress);
    }

    let mut total_bps: u32 = 0;
    for i in 0..payees.len() {
        let payee = payees.get(i).ok_or(ContractError::IndexOutOfBounds)?;
        let bps = payee.bps;

        // Check for overflow
        total_bps = total_bps
            .checked_add(bps)
            .ok_or(ContractError::ArithmeticError)?;

        // Validate each payee address is not zero
        let zero = Address::from_string(&String::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ));
        if payee.address == zero {
            return Err(ContractError::InvalidAddress);
        }
    }

    if total_bps != BASIS_POINTS {
        return Err(ContractError::InvalidAmount);
    }

    Ok(())
}

/// Validates individual protocol/arbitration fees against their respective maximums.
///
/// Returns Err(FeeExceedsMax) if the value exceeds its cap.
fn validate_protocol_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_PROTOCOL_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

fn validate_arbitration_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_ARBITRATION_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

/// Validates that the combined protocol + arbitration fees don't exceed MAX_COMBINED_FEE_BPS.
///
/// This prevents the attack where an admin sets both fees to their maximum values,
/// draining entire escrows through fees.
fn validate_combined_fees(
    protocol_fee_bps: u32,
    arbitration_fee_bps: u32,
) -> Result<(), ContractError> {
    let combined = protocol_fee_bps
        .checked_add(arbitration_fee_bps)
        .ok_or(ContractError::ArithmeticError)?;
    if combined > MAX_COMBINED_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

fn update_protocol_fee(env: &Env, caller: &Address, fee_bps: u32) -> Result<u32, ContractError> {
    caller.require_auth();
    let admin = require_admin(env)?;
    if caller != &admin {
        return Err(ContractError::NotAuthorized);
    }
    validate_protocol_fee_bps(fee_bps)?;
    let mut config = read_fee_config(env);
    // Validate that new protocol fee + existing arbitration fee doesn't exceed combined cap
    validate_combined_fees(fee_bps, config.arbitration_fee_bps)?;
    let old_fee = config.protocol_fee_bps;
    config.protocol_fee_bps = fee_bps;
    write_fee_config(env, &config);
    Ok(old_fee)
}

/// Updates the arbitration fee. Requires admin auth.
/// Validates that arbitration fee + current protocol fee doesn't exceed combined cap.
fn update_arbitration_fee(env: &Env, caller: &Address, fee_bps: u32) -> Result<u32, ContractError> {
    caller.require_auth();
    let admin = require_admin(env)?;
    if caller != &admin {
        return Err(ContractError::NotAuthorized);
    }
    validate_arbitration_fee_bps(fee_bps)?;
    let mut config = read_fee_config(env);
    // Validate that new arbitration fee + existing protocol fee doesn't exceed combined cap
    validate_combined_fees(config.protocol_fee_bps, fee_bps)?;
    let old_fee = config.arbitration_fee_bps;
    config.arbitration_fee_bps = fee_bps;
    write_fee_config(env, &config);
    Ok(old_fee)
}

fn get_ttl_extension(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::TtlExtensionLedgers)
        .unwrap_or(DEFAULT_TTL_EXTENSION)
}

/// Saves the escrow and records a state-history entry if the state changed.
/// Callers that already know the pre-mutation state (most do — they hold it
/// from `load_escrow` before overwriting `escrow.state`) should pass it via
/// `prev_state` to avoid a redundant persistent read of the same key that
/// `load_escrow` already paid for. Pass `None` only when there is no prior
/// escrow to compare against (e.g. first save on creation).
fn save_escrow(env: &Env, id: u64, escrow: &EscrowData, prev_state: Option<&EscrowState>) {
    let key = DataKey::Escrow(id);
    let ext = get_ttl_extension(env);
    let state_changed = match prev_state {
        Some(prev) => *prev != escrow.state,
        None => {
            let previous: Option<EscrowData> = env.storage().persistent().get(&key);
            previous
                .as_ref()
                .map(|existing| existing.state != escrow.state)
                .unwrap_or(true)
        }
    };

    env.storage().persistent().set(&key, escrow);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);

    if state_changed {
        append_state_history(env, id, &escrow.state);
    }
}

fn load_escrow(env: &Env, id: u64) -> Result<EscrowData, ContractError> {
    let key = DataKey::Escrow(id);
    let escrow: EscrowData = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ContractError::EscrowNotFound)?;
    let ext = get_ttl_extension(env);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
    Ok(escrow)
}

fn append_state_history(env: &Env, id: u64, state: &EscrowState) {
    let key = DataKey::EscrowStateHistory(id);
    let ext = get_ttl_extension(env);
    let mut history: Vec<(EscrowState, u64)> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));

    history.push_back((state.clone(), env.ledger().timestamp()));
    while history.len() > MAX_STATE_HISTORY_ENTRIES {
        history.pop_front();
    }
    env.storage().persistent().set(&key, &history);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
}

fn load_state_history(env: &Env, id: u64) -> Vec<(EscrowState, u64)> {
    let key = DataKey::EscrowStateHistory(id);
    let ext = get_ttl_extension(env);
    let history = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));

    if !history.is_empty() {
        env.storage().persistent().extend_ttl(&key, ext / 2, ext);
    }
    history
}

fn save_dispute(env: &Env, id: u64, dispute: &DisputeData) {
    let key = DataKey::Dispute(id);
    let ext = get_ttl_extension(env);
    env.storage().persistent().set(&key, dispute);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
}

fn load_dispute(env: &Env, id: u64) -> Result<DisputeData, ContractError> {
    let key = DataKey::Dispute(id);
    let dispute: DisputeData = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ContractError::DisputeNotFound)?;
    let ext = get_ttl_extension(env);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
    Ok(dispute)
}

fn save_basket_tokens(env: &Env, escrow_id: u64, tokens: &soroban_sdk::Vec<TokenEntry>) {
    let key = DataKey::BasketTokens(escrow_id);
    let ext = get_ttl_extension(env);
    env.storage().persistent().set(&key, tokens);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
}

fn load_basket_tokens(env: &Env, escrow_id: u64) -> soroban_sdk::Vec<TokenEntry> {
    let key = DataKey::BasketTokens(escrow_id);
    if !env.storage().persistent().has(&key) {
        return soroban_sdk::Vec::new(env);
    }
    let ext = get_ttl_extension(env);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| soroban_sdk::Vec::new(env))
}

fn transfer_with_protocol_fee(
    env: &Env,
    token_addr: &Address,
    recipient: &Address,
    fee_collector: &Address,
    amount: i128,
    fee_bps: u32,
) -> Result<(i128, i128), ContractError> {
    let (fee, net) = crate::helpers::payout::calculate_protocol_fee(amount, fee_bps)?;
    let token_client = token::Client::new(env, token_addr);
    let contract_addr = env.current_contract_address();

    if net > 0 {
        token_client.transfer(&contract_addr, recipient, &net);
    }

    if fee > 0 {
        token_client.transfer(&contract_addr, fee_collector, &fee);
    }

    Ok((fee, net))
}

fn distribute_to_payees(
    env: &Env,
    token_addr: &Address,
    payees: &Vec<Payee>,
    amount: i128,
) -> Result<(), ContractError> {
    if amount < 0 {
        return Err(ContractError::InvalidAmount);
    }

    let token_client = token::Client::new(env, token_addr);
    let contract_addr = env.current_contract_address();

    let mut remaining = amount;

    // Calculate amounts for all payees except the first
    for i in 1..payees.len() {
        let payee = payees.get(i).ok_or(ContractError::IndexOutOfBounds)?;
        let payee_amount = amount
            .checked_mul(payee.bps as i128)
            .ok_or(ContractError::ArithmeticError)?
            .checked_div(BASIS_POINTS as i128)
            .ok_or(ContractError::ArithmeticError)?;

        if payee_amount > 0 {
            token_client.transfer(&contract_addr, &payee.address, &payee_amount);
        }

        remaining = remaining
            .checked_sub(payee_amount)
            .ok_or(ContractError::ArithmeticError)?;
    }

    // First payee gets the remainder (rounding goes to first payee)
    let first_payee = payees.get(0).ok_or(ContractError::IndexOutOfBounds)?;
    if remaining > 0 {
        token_client.transfer(&contract_addr, &first_payee.address, &remaining);
    }

    Ok(())
}

/// Transfer all non-primary basket tokens to a recipient after the primary
/// token has been paid out by the calling function.
fn payout_basket_tokens(
    env: &Env,
    escrow_id: u64,
    recipient: &Address,
) -> Result<(), ContractError> {
    let basket_tokens = load_basket_tokens(env, escrow_id);
    let contract_addr = env.current_contract_address();
    // Skip index 0 (primary token, already handled by caller)
    for i in 1..basket_tokens.len() {
        let entry = basket_tokens
            .get(i)
            .ok_or(ContractError::IndexOutOfBounds)?;
        if entry.amount > 0 {
            token::Client::new(env, &entry.token).transfer(
                &contract_addr,
                recipient,
                &entry.amount,
            );
        }
    }
    Ok(())
}

fn ensure_not_expired(_env: &Env, _escrow: &EscrowData) -> Result<(), ContractError> {
    // Expiry is checked at fund_escrow time via PendingExpiry(escrow_id).
    // Once funded (Funded state), the escrow is not subject to pending expiry.
    Ok(())
}

fn increment_counter(env: &Env, key: &DataKey) -> Result<(), ContractError> {
    let current: u64 = env.storage().instance().get(key).unwrap_or(0);
    let next = current
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage().instance().set(key, &next);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn create_escrow_internal(
    env: &Env,
    payees: Vec<Payee>,
    buyer: Option<Address>,
    resolver: Address,
    token: Address,
    amount: i128,
    fee_bps: u32,
    resolver_fee_bps: u32,
    shipping_window: u64,
    notes: Option<String>,
) -> Result<u64, ContractError> {
    if payees.is_empty() {
        return Err(ContractError::InvalidAddress);
    }
    let first_payee = payees.get(0).ok_or(ContractError::IndexOutOfBounds)?;
    first_payee.address.require_auth();

    ensure_action_not_paused(env, Symbol::new(env, "CREATE"))?;

    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let max_amount = env
        .storage()
        .instance()
        .get(&DataKey::MaxAmount)
        .unwrap_or(MAX_ESCROW_AMOUNT);
    if amount > max_amount {
        return Err(ContractError::AmountExceedsMaximum);
    }

    let min_amount = env
        .storage()
        .instance()
        .get(&DataKey::MinAmount)
        .unwrap_or(MIN_ESCROW_AMOUNT);
    if amount < min_amount {
        return Err(ContractError::AmountBelowMinimum);
    }

    if !(MIN_SHIPPING_WINDOW..=MAX_SHIPPING_WINDOW).contains(&shipping_window) {
        return Err(ContractError::InvalidShippingWindow);
    }

    validate_escrow_fee_bps(fee_bps)?;
    validate_resolver_fee_bps(resolver_fee_bps)?;
    validate_payees(env, &payees)?;

    // Validate notes length if present
    if let Some(ref n) = notes {
        if n.len() > MAX_NOTES_LEN {
            return Err(ContractError::InputTooLong);
        }
    }

    // Security: resolver must be distinct from all payees and buyer
    for i in 0..payees.len() {
        let payee = payees.get(i).ok_or(ContractError::IndexOutOfBounds)?;
        if resolver == payee.address {
            return Err(ContractError::ConflictingRoles);
        }
        if let Some(ref b) = buyer {
            if b == &payee.address {
                return Err(ContractError::ConflictingRoles);
            }
        }
    }
    if let Some(ref b) = buyer {
        if resolver == *b {
            return Err(ContractError::ConflictingRoles);
        }
    }

    // Issue #393: resolver registry — reject unknown resolvers in strict mode
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::ResolverStrict)
        .unwrap_or(false)
    {
        let approved: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or_else(|| soroban_sdk::Vec::new(env));
        let mut found = false;
        for r in approved.iter() {
            if r == resolver {
                found = true;
                break;
            }
        }
        if !found {
            return Err(ContractError::UnauthorizedResolver);
        }
    }

    // Token allowlist check
    is_token_allowed(env, &token)?;

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

    let resolvers = ResolverSet::Single(resolver.clone());
    let escrow = EscrowData {
        payees: payees.clone(),
        buyer,
        resolvers,
        token: token.clone(),
        amount,
        fee_bps,
        resolver_fee_bps,
        shipping_window,
        funded_at: 0,
        dispute_deadline: 0,
        state: EscrowState::Pending,
        shipped_at: 0,
        delivered_at: None,
        tracking_id: None,
        notes,
    };

    save_escrow(env, escrow_id, &escrow, None);

    let first_payee_addr = payees
        .get(0)
        .ok_or(ContractError::IndexOutOfBounds)?
        .address
        .clone();
    let mut vendor_escrows = storage::read_vendor_escrow_index(env, &first_payee_addr);
    vendor_escrows.push_back(escrow_id);
    storage::write_vendor_escrow_index(env, &first_payee_addr, &vendor_escrows);

    increment_counter(env, &DataKey::TotalCreated)?;
    emit_escrow_created(
        env,
        escrow_id,
        first_payee_addr,
        resolver,
        escrow.token.clone(),
        escrow.amount,
        escrow.fee_bps,
        escrow.resolver_fee_bps,
        escrow.shipping_window,
        crate::EscrowState::Pending,
    );
    Ok(escrow_id)
}

/// Execute the resolution transition when threshold is met.
/// Deducts arbitration and resolver fees, transitions to PendingFinalization.
fn execute_resolution_transition(
    env: &Env,
    escrow_id: u64,
    escrow: EscrowData,
    caller: Address,
    final_resolution: ResolutionType,
    votes: Vec<ResolverVote>,
) -> Result<(), ContractError> {
    let arbitration_fee_bps = read_fee_config(env).arbitration_fee_bps;
    let arbitration_fee =
        crate::helpers::payout::calculate_fee(escrow.amount, arbitration_fee_bps)?;

    let resolver_fee =
        crate::helpers::payout::calculate_fee(escrow.amount, escrow.resolver_fee_bps)?;

    let prev_state = escrow.state.clone();
    let mut updated_escrow = escrow;
    updated_escrow.amount = updated_escrow
        .amount
        .checked_sub(arbitration_fee)
        .ok_or(ContractError::ArithmeticError)?;
    updated_escrow.amount = updated_escrow
        .amount
        .checked_sub(resolver_fee)
        .ok_or(ContractError::ArithmeticError)?;

    // Update Accounting
    let total_key = DataKey::TotalArbitrationFees(updated_escrow.token.clone());
    let current_total: i128 = env.storage().instance().get(&total_key).unwrap_or(0);
    env.storage().instance().set(
        &total_key,
        &current_total
            .checked_add(arbitration_fee)
            .ok_or(ContractError::ArithmeticError)?,
    );

    let fee_collector: Address = env
        .storage()
        .instance()
        .get(&DataKey::FeeCollector)
        .expect("fee collector not set");

    if arbitration_fee > 0 {
        token::Client::new(env, &updated_escrow.token).transfer(
            &env.current_contract_address(),
            &fee_collector,
            &arbitration_fee,
        );
    }

    // Pay resolver fee immediately
    if resolver_fee > 0 {
        token::Client::new(env, &updated_escrow.token).transfer(
            &env.current_contract_address(),
            &caller,
            &resolver_fee,
        );
    }

    // Store resolution in dispute data and transition to PendingFinalization
    let now = env.ledger().timestamp();
    let appeal_deadline = now
        .checked_add(APPEAL_WINDOW)
        .ok_or(ContractError::ArithmeticError)?;

    let mut dispute_data = load_dispute(env, escrow_id)?;
    dispute_data.set_resolution(final_resolution.clone());
    dispute_data.resolved_by = Some(caller.clone());
    dispute_data.resolved_at = now;

    updated_escrow.state = EscrowState::PendingFinalization;

    save_escrow(env, escrow_id, &updated_escrow, Some(&prev_state));
    save_dispute(env, escrow_id, &dispute_data);
    save_resolver_votes(env, escrow_id, &votes);

    emit_dispute_pending_finalization(
        env,
        escrow_id,
        caller,
        final_resolution,
        updated_escrow.amount,
        appeal_deadline,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[contractimpl]
impl Escrow {
    /// Modern 9-argument production interface accepting either Address or Payee array variants.
    pub fn create_escrow(
        env: Env,
        seller_or_payees: Val,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
        resolver_fee_bps: u32,
        shipping_window: u64,
        notes: Option<String>,
    ) -> Result<u64, ContractError> {
        let payees = if let Ok(payees_vec) = Vec::<Payee>::try_from_val(&env, &seller_or_payees) {
            payees_vec
        } else if let Ok(seller_address) = Address::try_from_val(&env, &seller_or_payees) {
            let mut p_vec = Vec::new(&env);
            p_vec.push_back(Payee {
                address: seller_address,
                bps: BASIS_POINTS,
            });
            p_vec
        } else {
            return Err(ContractError::InvalidAddress);
        };

        ensure_not_paused(&env)?;

        create_escrow_internal(
            &env,
            payees,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            resolver_fee_bps,
            shipping_window,
            notes,
        )
    }

    /// Explicit 8-argument method signature mapping for historical tests.
    pub fn create_escrow_8(
        env: Env,
        seller_or_payees: Val,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
    ) -> Result<u64, ContractError> {
        Self::create_escrow(
            env,
            seller_or_payees,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            0_u32, // Default resolver fee
            shipping_window,
            None,
        )
    }

    /// Explicit 7-argument method signature mapping for historical tests.
    pub fn create_escrow_7(
        env: Env,
        seller_or_payees: Val,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
    ) -> Result<u64, ContractError> {
        Self::create_escrow(
            env,
            seller_or_payees,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            0_u32,    // Default resolver fee
            3600_u64, // Default shipping window fallback
            None,
        )
    }

    /// Returns the current version of the contract.
    pub fn get_version(_env: Env) -> u32 {
        CONTRACT_VERSION
    }

    /// Sets the protocol fee collector, admin address, and arbitration fee. Must be called once.
    pub fn initialize(
        env: Env,
        admin: Address,
        fee_collector: Address,
        arbitration_fee_bps: u32,
    ) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        if admin == fee_collector {
            return Err(ContractError::InvalidAddress);
        }
        validate_arbitration_fee_bps(arbitration_fee_bps)?;

        let zero = Address::from_string(&String::from_str(
            &env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ));
        if admin == zero || fee_collector == zero {
            return Err(ContractError::InvalidAddress);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &fee_collector);
        write_fee_config(
            &env,
            &FeeConfig {
                protocol_fee_bps: 0,
                arbitration_fee_bps,
            },
        );
        env.storage().instance().set(&DataKey::EscrowCounter, &1u64);
        env.storage().instance().set(&DataKey::Paused, &false);
        // Fresh deployments already match the current schema, so `migrate` is a
        // no-op for them.
        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);

        emit_contract_initialized(&env, admin, fee_collector, arbitration_fee_bps);
        Ok(())
    }

    /// Pauses the contract. Only callable by admin.
    pub fn pause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        env.storage().instance().set(&DataKey::Paused, &true);
        emit_contract_paused(&env, admin);
        Ok(())
    }

    /// Unpauses the contract. Only callable by admin.
    pub fn unpause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        env.storage().instance().set(&DataKey::Paused, &false);
        emit_contract_unpaused(&env, admin);
        Ok(())
    }

    /// Returns whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Pauses a specific action. Only callable by admin.
    pub fn pause_action(env: Env, caller: Address, action: Symbol) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::ActionPaused(action.clone()), &true);
        emit_action_paused(&env, action, caller);
        Ok(())
    }

    /// Unpauses a specific action. Only callable by admin.
    pub fn unpause_action(env: Env, caller: Address, action: Symbol) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::ActionPaused(action.clone()), &false);
        emit_action_unpaused(&env, action, caller);
        Ok(())
    }

    /// Returns whether a specific action is currently paused.
    pub fn is_action_paused(env: Env, action: Symbol) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ActionPaused(action))
            .unwrap_or(false)
    }

    /// Sets a new admin for the contract. Only callable by current admin.
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        let old_admin = require_admin(&env)?;
        old_admin.require_auth();
        if new_admin == old_admin {
            return Err(ContractError::SameAddress);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        emit_admin_rotated(&env, old_admin, new_admin);
        Ok(())
    }

    /// Upgrades the contract WASM. Only callable by admin.
    pub fn upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        emit_contract_upgraded(&env, admin, new_wasm_hash);
        Ok(())
    }

    /// Returns the schema version of the data currently in storage.
    ///
    /// Deployments that predate storage versioning report `0`. Compare against
    /// [`STORAGE_VERSION`] to decide whether [`Escrow::migrate`] must run.
    pub fn get_storage_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::StorageVersion)
            .unwrap_or(0)
    }

    /// Migrates storage to [`STORAGE_VERSION`] after a WASM upgrade.
    ///
    /// `upgrade` only swaps the code; any schema change must be applied here in
    /// a separate transaction immediately afterwards. The function is
    /// admin-only and idempotent: once storage is already at the current
    /// version it returns `AlreadyInitialized` instead of re-running steps, so
    /// a retried deployment cannot corrupt data.
    ///
    /// Each version bump appends one step below and never rewrites a previous
    /// one — see `docs/UPGRADES.md` for the full strategy.
    pub fn migrate(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin_caller(&env, &caller)?;

        let from = Self::get_storage_version(env.clone());
        if from >= STORAGE_VERSION {
            return Err(ContractError::AlreadyInitialized);
        }

        // v0 -> v1: versioning was introduced without changing any stored
        // layout, so existing `EscrowData` entries are read back unchanged and
        // only the version marker is written.

        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);
        storage::extend_instance_ttl(&env);

        emit_storage_migrated(&env, admin, from, STORAGE_VERSION);
        Ok(())
    }

    /// Updates the protocol fee. Only callable by admin.
    pub fn set_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        validate_escrow_fee_bps(fee_bps)?;
        let mut config = read_fee_config(&env);
        let old_fee = config.protocol_fee_bps;
        validate_combined_fees(fee_bps, config.arbitration_fee_bps)?;
        config.protocol_fee_bps = fee_bps;
        write_fee_config(&env, &config);
        emit_fee_updated(&env, old_fee, fee_bps);
        Ok(())
    }

    /// Updates the protocol fee configuration in basis points. Requires admin auth.
    pub fn set_protocol_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        let old_fee_bps = update_protocol_fee(&env, &caller, fee_bps)?;
        emit_protocol_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    /// Sets the TTL extension for storage entries. Only callable by admin.
    pub fn set_ttl_extension(env: Env, caller: Address, ledgers: u32) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let old_ledgers = get_ttl_extension(&env);
        env.storage()
            .instance()
            .set(&DataKey::TtlExtensionLedgers, &ledgers);
        emit_ttl_extension_updated(&env, old_ledgers, ledgers, caller);
        Ok(())
    }

    /// Sets a new fee collector address. Only callable by admin.
    ///
    /// Returns `Err(ContractError::InvalidAddress)` if `new_collector` is the
    /// zero address, which can never sign for or receive fee withdrawals.
    #[allow(deprecated)]
    pub fn set_fee_collector(env: Env, new_collector: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        admin.require_auth();

        let zero = Address::from_string(&String::from_str(
            &env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ));
        if new_collector == zero {
            return Err(ContractError::InvalidAddress);
        }

        let old_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;

        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &new_collector);
        emit_fee_collector_updated(&env, old_collector, new_collector);
        Ok(())
    }

    /// Creates an escrow with an optional expiration time.
    ///
    /// If `expires_at` is provided, the escrow must be funded via `fund_escrow`
    /// before `expires_at + grace_period` (ledger time) or `fund_escrow` will
    /// reject it with `EscrowExpired`. `grace_period` is ignored when
    /// `expires_at` is `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_escrow_with_expiration(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
        expires_at: Option<u64>,
        grace_period: u64,
    ) -> Result<u64, ContractError> {
        let mut payees = Vec::new(&env);
        payees.push_back(Payee {
            address: seller,
            bps: BASIS_POINTS,
        });
        let escrow_id = create_escrow_internal(
            &env,
            payees,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            0,
            shipping_window,
            None,
        )?;

        if let Some(expires_at) = expires_at {
            if expires_at <= env.ledger().timestamp() {
                return Err(ContractError::InvalidExpiration);
            }
            let effective_expiry = expires_at
                .checked_add(grace_period)
                .ok_or(ContractError::ArithmeticOverflow)?;

            let key = DataKey::PendingExpiry(escrow_id);
            let ext = get_ttl_extension(&env);
            env.storage().persistent().set(&key, &effective_expiry);
            env.storage().persistent().extend_ttl(&key, ext / 2, ext);
        }

        Ok(escrow_id)
    }

    /// Buyer funds a pending escrow. Transitions Pending → Funded.
    pub fn fund_escrow(env: Env, escrow_id: u64, buyer: Address) -> Result<(), ContractError> {
        buyer.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "FUND"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        if let Some(expires_at) = env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&DataKey::PendingExpiry(escrow_id))
        {
            if env.ledger().timestamp() > expires_at {
                return Err(ContractError::EscrowExpired);
            }
        }

        // Security: buyer must differ from seller and resolver.
        for i in 0..escrow.payees.len() {
            let payee = escrow
                .payees
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            if buyer == payee.address {
                return Err(ContractError::ConflictingRoles);
            }
        }
        if escrow.resolvers.contains(&buyer) {
            return Err(ContractError::ConflictingRoles);
        }
        if let Some(ref expected_buyer) = escrow.buyer {
            if &buyer != expected_buyer {
                return Err(ContractError::NotAuthorized);
            }
        }

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(&buyer, env.current_contract_address(), &escrow.amount);

        // Transfer additional basket tokens if this is a basket escrow
        let basket_tokens = load_basket_tokens(&env, escrow_id);
        for i in 0..basket_tokens.len() {
            let entry = basket_tokens
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            if entry.token != escrow.token && entry.amount > 0 {
                token::Client::new(&env, &entry.token).transfer(
                    &buyer,
                    env.current_contract_address(),
                    &entry.amount,
                );
            }
        }

        let now = env.ledger().timestamp();
        let prev_state = escrow.state.clone();
        escrow.buyer = Some(buyer.clone());
        escrow.state = EscrowState::Funded;
        escrow.funded_at = now;
        escrow.dispute_deadline = now
            .checked_add(DISPUTE_WINDOW)
            .ok_or(ContractError::ArithmeticOverflow)?;

        // Index the buyer for lookup.
        let mut buyer_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
            .unwrap_or(Vec::new(&env));
        buyer_escrows.push_back(escrow_id);
        let buyer_key = DataKey::BuyerEscrowIndex(buyer.clone());
        let ext = get_ttl_extension(&env);
        env.storage().persistent().set(&buyer_key, &buyer_escrows);
        env.storage()
            .persistent()
            .extend_ttl(&buyer_key, ext / 2, ext);

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        emit_escrow_funded(
            &env,
            escrow_id,
            buyer,
            escrow.amount,
            crate::EscrowState::Pending,
            crate::EscrowState::Funded,
        );
        Ok(())
    }

    /// Buyer raises a dispute on a funded or shipped escrow.
    pub fn raise_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        reason: Symbol,
        description: String,
        evidence_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if env.ledger().timestamp() >= escrow.dispute_deadline {
            return Err(ContractError::DisputeWindowStillOpen);
        }

        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(ContractError::InputTooLong);
        }

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Disputed;

        let dispute_data = DisputeData {
            escrow_id,
            reason: reason.clone(),
            description: description.clone(),
            evidence_hash: evidence_hash.clone(),
            status: DisputeStatus::Active,
            disputed_at: env.ledger().timestamp(),
            tracking_id: escrow.tracking_id.clone(),
            resolution: 0,
            resolved_by: None,
            appeal_count: 0,
            resolved_at: 0,
        };

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        save_dispute(&env, escrow_id, &dispute_data);
        increment_counter(&env, &DataKey::TotalDisputed)?;
        emit_dispute_raised(
            &env,
            escrow_id,
            buyer,
            reason,
            description,
            evidence_hash,
            prev_state,
            crate::EscrowState::Disputed,
        );
        Ok(())
    }

    /// Create escrow with multiple resolvers and M-of-N voting threshold.
    pub fn create_escrow_multi(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        resolvers: Vec<Address>,
        threshold: u32,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
    ) -> Result<u64, ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
        seller.require_auth();

        ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        if amount > MAX_ESCROW_AMOUNT {
            return Err(ContractError::AmountExceedsMaximum);
        }

        if amount < MIN_ESCROW_AMOUNT {
            return Err(ContractError::InvalidAmount);
        }

        validate_escrow_fee_bps(fee_bps)?;

        // Validate multi-resolver configuration
        let resolver_set = ResolverSet::Multi(crate::types::MultiResolver {
            resolvers,
            threshold,
        });
        validate_resolvers(&resolver_set, &seller, &buyer)?;

        let escrow_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .ok_or(ContractError::NotInitialized)?;
        let next_id = escrow_id
            .checked_add(1)
            .ok_or(ContractError::ArithmeticError)?;
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &next_id);
        // Extend instance storage TTL on every counter access
        let ext = get_ttl_extension(&env);
        env.storage().instance().extend_ttl(ext / 2, ext);

        let mut payees = Vec::new(&env);
        payees.push_back(Payee {
            address: seller.clone(),
            bps: BASIS_POINTS,
        });
        let escrow = EscrowData {
            payees,
            buyer,
            resolvers: resolver_set.clone(),
            token,
            amount,
            fee_bps,
            resolver_fee_bps: 0,
            shipping_window,
            funded_at: 0,
            dispute_deadline: 0,
            state: EscrowState::Pending,
            shipped_at: 0,
            delivered_at: None,
            tracking_id: None,
            notes: None,
        };

        save_escrow(&env, escrow_id, &escrow, None);

        let mut vendor_escrows = storage::read_vendor_escrow_index(&env, &seller);
        vendor_escrows.push_back(escrow_id);
        storage::write_vendor_escrow_index(&env, &seller, &vendor_escrows);

        increment_counter(&env, &DataKey::TotalCreated)?;

        // Emit with first resolver for backward compat
        if let ResolverSet::Multi(ref m) = &resolver_set {
            let resolver_addr = m
                .resolvers
                .get(0)
                .ok_or(ContractError::IndexOutOfBounds)?
                .clone();
            emit_escrow_created(
                &env,
                escrow_id,
                seller,
                resolver_addr,
                escrow.token.clone(),
                escrow.amount,
                escrow.fee_bps,
                escrow.resolver_fee_bps,
                escrow.shipping_window,
                crate::EscrowState::Pending,
            );
        }

        Ok(escrow_id)
    }
    /// Posts a message for a given escrow. Messages are immutable and stored on-chain.
    pub fn post_message(
        env: Env,
        escrow_id: u64,
        sender: Address,
        content: String,
    ) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;
        let _ = load_escrow(&env, escrow_id)?;

        let message = Message {
            sender: sender.clone(),
            timestamp: env.ledger().timestamp(),
            content,
        };
        let key = DataKey::Messages(escrow_id);
        let mut msgs: Vec<Message> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        msgs.push_back(message);
        env.storage().persistent().set(&key, &msgs);
        emit_message_posted(&env, escrow_id, sender);
        Ok(())
    }

    /// Retrieves messages for a given escrow with pagination.
    pub fn get_messages(env: Env, escrow_id: u64, start: u64, limit: u64) -> Vec<Message> {
        let max_limit = if limit > 50 { 50 } else { limit };
        let key = DataKey::Messages(escrow_id);
        let msgs: Vec<Message> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let total = msgs.len() as u64;
        let mut result = Vec::new(&env);
        if start >= total {
            return result;
        }
        let end = (start + max_limit).min(total);
        let mut i = start;
        while i < end {
            if let Some(m) = msgs.get(i as u32) {
                result.push_back(m.clone());
            }
            i += 1;
        }
        result
    }

    /// Create escrow with a fallback resolver that can take over after a deadline.
    pub fn create_escrow_with_fallback(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        primary_resolver: Address,
        backup_resolver: Address,
        dispute_deadline: u64,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
    ) -> Result<u64, ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
        seller.require_auth();

        ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        if amount > MAX_ESCROW_AMOUNT {
            return Err(ContractError::AmountExceedsMaximum);
        }
        if amount < MIN_ESCROW_AMOUNT {
            return Err(ContractError::InvalidAmount);
        }

        validate_escrow_fee_bps(fee_bps)?;

        let resolver_set = ResolverSet::Fallback(crate::types::FallbackResolver {
            primary: primary_resolver.clone(),
            backup: backup_resolver,
            dispute_deadline,
        });
        validate_resolvers(&resolver_set, &seller, &buyer)?;

        let escrow_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .ok_or(ContractError::NotInitialized)?;
        let next_id = escrow_id
            .checked_add(1)
            .ok_or(ContractError::ArithmeticError)?;
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &next_id);

        let ext = get_ttl_extension(&env);
        env.storage().instance().extend_ttl(ext / 2, ext);

        let mut payees = Vec::new(&env);
        payees.push_back(Payee {
            address: seller.clone(),
            bps: BASIS_POINTS,
        });
        let escrow = EscrowData {
            payees,
            buyer,
            resolvers: resolver_set,
            token,
            amount,
            fee_bps,
            resolver_fee_bps: 0,
            shipping_window,
            funded_at: 0,
            dispute_deadline: 0,
            state: EscrowState::Pending,
            shipped_at: 0,
            delivered_at: None,
            tracking_id: None,
            notes: None,
        };

        save_escrow(&env, escrow_id, &escrow, None);

        let mut vendor_escrows = storage::read_vendor_escrow_index(&env, &seller);
        vendor_escrows.push_back(escrow_id);
        storage::write_vendor_escrow_index(&env, &seller, &vendor_escrows);

        increment_counter(&env, &DataKey::TotalCreated)?;

        emit_escrow_created(
            &env,
            escrow_id,
            seller,
            primary_resolver,
            escrow.token.clone(),
            escrow.amount,
            escrow.fee_bps,
            escrow.resolver_fee_bps,
            escrow.shipping_window,
            crate::EscrowState::Pending,
        );

        Ok(escrow_id)
    }

    pub fn cancel_escrow(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow.buyer.clone();
        let is_payee = {
            let mut found = false;
            for i in 0..escrow.payees.len() {
                if caller
                    == escrow
                        .payees
                        .get(i)
                        .ok_or(ContractError::IndexOutOfBounds)?
                        .address
                {
                    found = true;
                    break;
                }
            }
            found
        };

        if !is_payee && buyer.as_ref() != Some(&caller) {
            return Err(ContractError::NotAuthorized);
        }

        let prev_state = escrow.state.clone();
        if escrow.state == EscrowState::Pending {
            escrow.state = EscrowState::Canceled;
        } else if escrow.state == EscrowState::Funded && buyer.as_ref() == Some(&caller) {
            let token_client = token::Client::new(&env, &escrow.token);
            token_client.transfer(&env.current_contract_address(), &caller, &escrow.amount);
            payout_basket_tokens(&env, escrow_id, &caller)?;
            escrow.state = EscrowState::Refunded;
            increment_counter(&env, &DataKey::TotalRefunded)?;
        } else {
            return Err(ContractError::InvalidState);
        }

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        let first_payee_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        emit_escrow_cancelled(
            &env,
            escrow_id,
            first_payee_addr,
            caller,
            prev_state,
            escrow.state.clone(),
        );
        Ok(())
    }

    /// Cancels a funded—but not yet shipped—escrow by mutual agreement and refunds the buyer in full.
    pub fn mutual_cancel(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;
        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;

        let seller_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        seller_addr.require_auth();
        buyer.require_auth();

        if escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        token::Client::new(&env, &escrow.token).transfer(
            &env.current_contract_address(),
            &buyer,
            &escrow.amount,
        );

        payout_basket_tokens(&env, escrow_id, &buyer)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Canceled;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        emit_escrow_cancelled(
            &env,
            escrow_id,
            seller_addr,
            buyer,
            prev_state,
            crate::EscrowState::Canceled,
        );
        Ok(())
    }

    /// Seller marks an escrow as shipped. Transitions Funded → Shipped.
    pub fn mark_shipped(
        env: Env,
        caller: Address,
        escrow_id: u64,
        tracking_id: String,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let first_payee = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .clone();
        let is_authorized = {
            let mut found = false;
            for i in 0..escrow.payees.len() {
                let payee = escrow
                    .payees
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?;
                if caller == payee.address {
                    found = true;
                    break;
                }
            }
            found
        };

        if !is_authorized {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        // Block shipping of expired escrows.
        ensure_not_expired(&env, &escrow)?;

        if tracking_id.is_empty() {
            return Err(ContractError::InvalidTrackingId);
        }
        if tracking_id.len() > MAX_TRACKING_ID_LEN {
            return Err(ContractError::InputTooLong);
        }

        let shipped_at = env.ledger().timestamp();
        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Shipped;
        escrow.shipped_at = shipped_at;
        escrow.tracking_id = Some(tracking_id);
        let tracking = escrow
            .tracking_id
            .clone()
            .unwrap_or(String::from_str(&env, ""));

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        emit_escrow_shipped(
            &env,
            escrow_id,
            first_payee.address,
            tracking,
            prev_state,
            crate::EscrowState::Shipped,
        );
        Ok(())
    }

    /// Records the delivery of an escrow. Callable by admin.
    pub fn record_delivery(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)?;

        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let mut escrow = load_escrow(&env, escrow_id)?;
        if escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        // Idempotency guard: prevent re-recording delivery
        if escrow.delivered_at.is_some() {
            return Err(ContractError::DeliveryAlreadyRecorded);
        }

        let delivered_at = env.ledger().timestamp();
        escrow.delivered_at = Some(delivered_at);
        // escrow.state is untouched by this call, so the pre-mutation state
        // is simply the current one.
        save_escrow(&env, escrow_id, &escrow, Some(&escrow.state));

        emit_delivery_recorded(&env, escrow_id, delivered_at);
        Ok(())
    }

    /// Confirms delivery and completes the escrow. Callable by the buyer.
    pub fn confirm_delivery(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidStateTransition);
        }

        if env.ledger().timestamp() < escrow.dispute_deadline {
            return Err(ContractError::DeliveryBeforeDisputeWindow);
        }

        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;

        let first_payee_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        let (protocol_fee, net_amount) =
            crate::helpers::payout::calculate_protocol_fee(escrow.amount, escrow.fee_bps)?;
        if protocol_fee > 0 {
            token::Client::new(&env, &escrow.token).transfer(
                &env.current_contract_address(),
                &fee_collector,
                &protocol_fee,
            );
        }
        distribute_to_payees(&env, &escrow.token, &escrow.payees, net_amount)?;
        payout_basket_tokens(&env, escrow_id, &first_payee_addr)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Completed;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalCompleted)?;

        emit_escrow_completed(
            &env,
            escrow_id,
            first_payee_addr,
            escrow.amount,
            escrow.fee_bps,
            prev_state,
            crate::EscrowState::Completed,
        );
        Ok(())
    }

    pub fn co_signed_release(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        ensure_not_paused(&env)?;
        let escrow = load_escrow(&env, escrow_id)?;

        let first_payee = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        first_payee.require_auth();
        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        buyer.require_auth();

        // Allow early release from Funded or Shipped states, but not if disputed.
        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if load_dispute(&env, escrow_id).is_ok() {
            return Err(ContractError::InvalidState);
        }

        let fee_config = read_fee_config(&env);
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotInitialized)?;

        transfer_with_protocol_fee(
            &env,
            &escrow.token,
            &first_payee,
            &fee_collector,
            escrow.amount,
            fee_config.protocol_fee_bps,
        )?;
        payout_basket_tokens(&env, escrow_id, &first_payee)?;

        let prev_state = escrow.state.clone();
        let mut updated = escrow;
        updated.state = EscrowState::Completed;

        save_escrow(&env, escrow_id, &updated, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalCompleted)?;
        emit_escrow_completed(
            &env,
            escrow_id,
            first_payee,
            updated.amount,
            fee_config.protocol_fee_bps,
            prev_state,
            crate::EscrowState::Completed,
        );
        Ok(())
    }

    pub fn resolve_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        resolution: ResolutionType,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "RESOLVE"))?;
        let escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Disputed {
            return Err(ContractError::InvalidState);
        }

        // Multi-Resolver Authorization
        if !escrow.resolvers.contains(&caller) {
            return Err(ContractError::NotAuthorized);
        }

        // Record Vote
        let votes = add_or_update_vote(&env, escrow_id, &caller, resolution.clone());
        let threshold = escrow.resolvers.threshold();

        emit_resolver_vote_recorded(
            &env,
            escrow_id,
            caller.clone(),
            resolution.clone(),
            votes.len(),
            threshold,
        );

        // Tally & Execute once threshold is met
        if let Some(final_resolution) = tally_votes(&votes, threshold) {
            execute_resolution_transition(
                &env,
                escrow_id,
                escrow,
                caller,
                final_resolution,
                votes,
            )?;
        } else {
            // Threshold not met, save votes and exit
            save_resolver_votes(&env, escrow_id, &votes);
        }

        Ok(())
    }

    /// Cast or change a vote on a disputed escrow.
    /// When threshold is reached, automatically transitions to PendingFinalization.
    pub fn vote(
        env: Env,
        caller: Address,
        escrow_id: u64,
        resolution: ResolutionType,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "RESOLVE"))?;
        let escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Disputed {
            return Err(ContractError::InvalidState);
        }

        if !escrow.resolvers.contains(&caller) {
            return Err(ContractError::NotAuthorized);
        }

        let votes = add_or_update_vote(&env, escrow_id, &caller, resolution.clone());
        let threshold = escrow.resolvers.threshold();

        emit_resolver_vote_recorded(
            &env,
            escrow_id,
            caller.clone(),
            resolution.clone(),
            votes.len(),
            threshold,
        );

        if let Some(final_resolution) = tally_votes(&votes, threshold) {
            execute_resolution_transition(
                &env,
                escrow_id,
                escrow,
                caller,
                final_resolution,
                votes,
            )?;
        } else {
            save_resolver_votes(&env, escrow_id, &votes);
        }

        Ok(())
    }

    pub fn set_arbitration_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        let old_fee_bps = update_arbitration_fee(&env, &caller, fee_bps)?;
        emit_arbitration_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    pub fn get_arbitration_fee(env: Env) -> u32 {
        read_fee_config(&env).arbitration_fee_bps
    }

    /// Get the resolver votes for a disputed escrow (for multi-resolver voting tracking)
    pub fn get_resolver_votes(env: Env, escrow_id: u64) -> Vec<ResolverVote> {
        load_resolver_votes(&env, escrow_id)
    }

    /// Returns the total arbitration fees accumulated for a token.
    pub fn get_total_arbitration_fees(env: Env, token: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalArbitrationFees(token))
            .unwrap_or(0)
    }

    pub fn auto_release(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if load_dispute(&env, escrow_id).is_ok() {
            return Err(ContractError::InvalidState);
        }

        let now = env.ledger().timestamp();

        if let Some(delivered_at) = escrow.delivered_at {
            let eligible_at = delivered_at
                .checked_add(DELIVERY_RELEASE_WINDOW)
                .ok_or(ContractError::ArithmeticOverflow)?;
            if now < eligible_at {
                return Err(ContractError::ShippingWindowNotElapsed);
            }
        } else if escrow.state == EscrowState::Shipped {
            return Err(ContractError::DeliveryNotRecorded);
        } else {
            if now < escrow.dispute_deadline {
                return Err(ContractError::DeliveryBeforeDisputeWindow);
            }
            let shipped_or_funded_at = if escrow.shipped_at > 0 {
                escrow.shipped_at
            } else {
                escrow.funded_at
            };
            let window_elapsed_at = shipped_or_funded_at
                .checked_add(escrow.shipping_window)
                .ok_or(ContractError::ArithmeticError)?;
            if now < window_elapsed_at {
                return Err(ContractError::ShippingWindowNotElapsed);
            }
        }

        let fee_config = read_fee_config(&env);
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;

        let first_payee_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        let (protocol_fee, net_amount) = crate::helpers::payout::calculate_protocol_fee(
            escrow.amount,
            fee_config.protocol_fee_bps,
        )?;
        if protocol_fee > 0 {
            token::Client::new(&env, &escrow.token).transfer(
                &env.current_contract_address(),
                &fee_collector,
                &protocol_fee,
            );
        }
        distribute_to_payees(&env, &escrow.token, &escrow.payees, net_amount)?;
        payout_basket_tokens(&env, escrow_id, &first_payee_addr)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Completed;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalCompleted)?;

        emit_auto_released(
            &env,
            escrow_id,
            first_payee_addr,
            escrow.amount,
            escrow.fee_bps,
            prev_state,
            crate::EscrowState::Completed,
        );
        Ok(())
    }

    pub fn finalize_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::PendingFinalization {
            return Err(ContractError::NotPendingFinalization);
        }

        let mut dispute_data = load_dispute(&env, escrow_id)?;
        let now = env.ledger().timestamp();

        let resolution = dispute_data
            .get_resolution()
            .ok_or(ContractError::InvalidState)?;
        let resolved_by = dispute_data
            .resolved_by
            .clone()
            .ok_or(ContractError::InvalidState)?;

        let appeal_deadline = dispute_data
            .resolved_at
            .checked_add(APPEAL_WINDOW)
            .ok_or(ContractError::ArithmeticError)?;
        if now < appeal_deadline {
            return Err(ContractError::AppealWindowActive);
        }

        let _prev_state = escrow.state.clone();
        let recipient = match resolution {
            ResolutionType::Release => escrow
                .payees
                .get(0)
                .ok_or(ContractError::IndexOutOfBounds)?
                .address
                .clone(),
            ResolutionType::Refund => escrow
                .buyer
                .clone()
                .ok_or(ContractError::EscrowHasNoBuyer)?,
        };

        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;

        let platform_fee_bps = read_platform_fee_bps(&env);
        let platform_fee = if platform_fee_bps > 0 {
            crate::helpers::payout::calculate_fee(escrow.amount, platform_fee_bps)?
        } else {
            0
        };

        let treasury = if platform_fee > 0 {
            Some(read_treasury(&env)?)
        } else {
            None
        };

        let seller_amount = escrow
            .amount
            .checked_sub(platform_fee)
            .ok_or(ContractError::ArithmeticError)?;

        if platform_fee > 0 {
            if let Some(ref treasury_addr) = treasury {
                let token_client = token::Client::new(&env, &escrow.token);
                token_client.transfer(
                    &env.current_contract_address(),
                    treasury_addr,
                    &platform_fee,
                );
            }
        }

        transfer_with_protocol_fee(
            &env,
            &escrow.token,
            &recipient,
            &fee_collector,
            seller_amount,
            escrow.fee_bps,
        )?;
        payout_basket_tokens(&env, escrow_id, &recipient)?;

        let prev_state = escrow.state.clone();
        let new_state = match resolution {
            ResolutionType::Release => EscrowState::Completed,
            ResolutionType::Refund => EscrowState::Refunded,
        };
        escrow.state = new_state.clone();

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        dispute_data.status = DisputeStatus::Resolved;
        save_dispute(&env, escrow_id, &dispute_data);

        match resolution {
            ResolutionType::Release => increment_counter(&env, &DataKey::TotalCompleted)?,
            ResolutionType::Refund => increment_counter(&env, &DataKey::TotalRefunded)?,
        };

        emit_dispute_resolved(
            &env,
            escrow_id,
            resolved_by,
            resolution,
            recipient,
            escrow.amount,
            0,
            0,
            prev_state,
            new_state,
        );
        Ok(())
    }

    pub fn appeal_dispute(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::PendingFinalization {
            return Err(ContractError::NotPendingFinalization);
        }

        let dispute_data = load_dispute(&env, escrow_id)?;
        let now = env.ledger().timestamp();

        // Appeal window must still be active (based on resolved_at)
        let appeal_deadline = dispute_data
            .resolved_at
            .checked_add(APPEAL_WINDOW)
            .ok_or(ContractError::ArithmeticError)?;
        if now >= appeal_deadline {
            return Err(ContractError::DisputeWindowStillOpen);
        }

        // Only buyer or seller can appeal
        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        let seller_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        if caller != buyer && caller != seller_addr {
            return Err(ContractError::NotAuthorized);
        }

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Disputed;

        let mut updated_dispute = dispute_data;
        updated_dispute.status = DisputeStatus::Active;
        updated_dispute.clear_resolution();
        updated_dispute.appeal_count += 1;

        // Clear votes for Multi resolver sets so a fresh round begins
        if matches!(escrow.resolvers, ResolverSet::Multi(_)) {
            env.storage()
                .persistent()
                .remove(&DataKey::ResolverVotes(escrow_id));
        }

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        save_dispute(&env, escrow_id, &updated_dispute);

        emit_dispute_appealed(&env, escrow_id, caller);
        Ok(())
    }

    pub fn set_token_allowlist_enabled(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlistEnabled, &enabled);
        emit_allowlist_toggled(&env, enabled);
        Ok(())
    }

    pub fn add_allowed_token(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let mut allowlist: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        for allowed_token in allowlist.iter() {
            if allowed_token == token {
                return Ok(());
            }
        }

        allowlist.push_back(token.clone());
        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlist, &allowlist);

        emit_token_allowlist_updated(&env, token, true);
        Ok(())
    }

    pub fn remove_allowed_token(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let allowlist: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        let mut found = false;
        let mut new_allowlist = soroban_sdk::Vec::new(&env);

        for allowed_token in allowlist.iter() {
            if allowed_token == token {
                found = true;
            } else {
                new_allowlist.push_back(allowed_token);
            }
        }

        if !found {
            return Err(ContractError::TokenNotAllowed);
        }

        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlist, &new_allowlist);

        emit_token_allowlist_updated(&env, token, false);
        Ok(())
    }

    pub fn is_token_allowlist_enabled(env: Env) -> bool {
        is_token_allowlist_enabled(&env)
    }

    pub fn get_allowed_tokens(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    pub fn set_platform_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        if fee_bps > MAX_PLATFORM_FEE_BPS {
            return Err(ContractError::PlatformFeeExceedsMax);
        }

        let old_fee = read_platform_fee_bps(&env);
        write_platform_fee_bps(&env, fee_bps);

        emit_platform_fee_updated(&env, old_fee, fee_bps);
        Ok(())
    }

    pub fn set_treasury(env: Env, caller: Address, treasury: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let zero = Address::from_string(&String::from_str(
            &env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        ));
        if treasury == zero {
            return Err(ContractError::InvalidAddress);
        }

        let old_treasury = read_treasury(&env).unwrap_or_else(|_| zero.clone());
        write_treasury(&env, &treasury);

        emit_treasury_updated(&env, old_treasury, treasury);
        Ok(())
    }

    pub fn get_platform_fee_bps(env: Env) -> u32 {
        read_platform_fee_bps(&env)
    }

    pub fn get_treasury(env: Env) -> Result<Address, ContractError> {
        read_treasury(&env)
    }

    pub fn create_basket_escrow(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        resolver: Address,
        tokens: soroban_sdk::Vec<Address>,
        amounts: soroban_sdk::Vec<i128>,
        fee_bps: u32,
        shipping_window: u64,
    ) -> Result<u64, ContractError> {
        seller.require_auth();
        ensure_not_paused(&env)?;

        if tokens.len() != amounts.len() || tokens.is_empty() {
            return Err(ContractError::InvalidAmount);
        }

        validate_escrow_fee_bps(fee_bps)?;

        if resolver == seller {
            return Err(ContractError::ConflictingRoles);
        }
        if let Some(ref b) = buyer {
            if b == &seller || b == &resolver {
                return Err(ContractError::ConflictingRoles);
            }
        }

        for token in tokens.iter() {
            is_token_allowed(&env, &token)?;
        }

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

        let ext = get_ttl_extension(&env);
        env.storage().instance().extend_ttl(ext / 2, ext);

        let primary_amount = amounts.get(0).ok_or(ContractError::InvalidAmount)?;
        let primary_token = tokens.get(0).ok_or(ContractError::InvalidAmount)?;

        let mut basket_payees = Vec::new(&env);
        basket_payees.push_back(Payee {
            address: seller.clone(),
            bps: BASIS_POINTS,
        });
        let escrow = EscrowData {
            payees: basket_payees,
            buyer: buyer.clone(),
            resolvers: ResolverSet::Single(resolver.clone()),
            token: primary_token,
            amount: primary_amount,
            fee_bps,
            resolver_fee_bps: 0,
            shipping_window,
            funded_at: 0,
            dispute_deadline: 0,
            state: EscrowState::Pending,
            shipped_at: 0,
            delivered_at: None,
            tracking_id: None,
            notes: None,
        };

        save_escrow(&env, escrow_id, &escrow, None);

        // Persist all basket tokens/amounts alongside the primary EscrowData
        let mut basket_entries: Vec<TokenEntry> = Vec::new(&env);
        for i in 0..tokens.len() {
            let token = tokens.get(i).ok_or(ContractError::IndexOutOfBounds)?;
            let amount = amounts.get(i).ok_or(ContractError::IndexOutOfBounds)?;
            basket_entries.push_back(TokenEntry { token, amount });
        }
        save_basket_tokens(&env, escrow_id, &basket_entries);

        let mut vendor_escrows = storage::read_vendor_escrow_index(&env, &seller);
        vendor_escrows.push_back(escrow_id);
        storage::write_vendor_escrow_index(&env, &seller, &vendor_escrows);

        increment_counter(&env, &DataKey::TotalCreated)?;
        emit_basket_escrow_created(&env, escrow_id, seller, tokens.len());

        Ok(escrow_id)
    }

    /// Fund a basket escrow by transferring all tokens from the buyer.
    /// Can be used instead of individual `fund_escrow` calls for multi-token escrows.
    pub fn fund_basket_escrow(
        env: Env,
        escrow_id: u64,
        buyer: Address,
    ) -> Result<(), ContractError> {
        buyer.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "FUND"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        if let Some(ref expected_buyer) = escrow.buyer {
            if &buyer != expected_buyer {
                return Err(ContractError::NotAuthorized);
            }
        }

        let basket_tokens = load_basket_tokens(&env, escrow_id);
        if basket_tokens.is_empty() {
            return Err(ContractError::InvalidAmount);
        }

        for i in 0..basket_tokens.len() {
            let entry = basket_tokens
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            if entry.amount > 0 {
                token::Client::new(&env, &entry.token).transfer(
                    &buyer,
                    env.current_contract_address(),
                    &entry.amount,
                );
            }
        }

        let now = env.ledger().timestamp();
        let prev_state = escrow.state.clone();
        escrow.buyer = Some(buyer.clone());
        escrow.state = EscrowState::Funded;
        escrow.funded_at = now;
        escrow.dispute_deadline = now
            .checked_add(DISPUTE_WINDOW)
            .ok_or(ContractError::ArithmeticError)?;

        let mut buyer_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
            .unwrap_or(Vec::new(&env));
        buyer_escrows.push_back(escrow_id);
        let buyer_key = DataKey::BuyerEscrowIndex(buyer.clone());
        let ext = get_ttl_extension(&env);
        env.storage().persistent().set(&buyer_key, &buyer_escrows);
        env.storage()
            .persistent()
            .extend_ttl(&buyer_key, ext / 2, ext);

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        emit_escrow_funded(
            &env,
            escrow_id,
            buyer.clone(),
            escrow.amount,
            crate::EscrowState::Pending,
            crate::EscrowState::Funded,
        );
        Ok(())
    }

    /// Returns the full list of tokens and amounts for a basket escrow.
    pub fn get_basket_tokens(env: Env, escrow_id: u64) -> Vec<TokenEntry> {
        load_basket_tokens(&env, escrow_id)
    }

    pub fn get_escrow(env: Env, escrow_id: u64) -> Result<EscrowData, ContractError> {
        load_escrow(&env, escrow_id)
    }

    pub fn get_state_history(
        env: Env,
        escrow_id: u64,
    ) -> Result<Vec<(EscrowState, u64)>, ContractError> {
        load_escrow(&env, escrow_id)?;
        Ok(load_state_history(&env, escrow_id))
    }

    pub fn get_dispute(env: Env, escrow_id: u64) -> Option<DisputeData> {
        load_dispute(&env, escrow_id).ok()
    }

    pub fn get_escrows_by_buyer(env: Env, buyer: Address) -> Vec<u64> {
        if let Some(ids) = env
            .storage()
            .persistent()
            .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
        {
            return ids;
        }
        let mut result = Vec::new(&env);
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(1);
        for id in 1..counter {
            if let Ok(escrow) = load_escrow(&env, id) {
                if escrow.buyer.as_ref() == Some(&buyer) {
                    result.push_back(id);
                }
            }
        }
        result
    }

    /// Batch view: return escrows for the supplied IDs in the same order.
    /// Missing IDs return None in the corresponding slot.
    pub fn get_escrows_by_ids(
        env: Env,
        ids: soroban_sdk::Vec<u64>,
    ) -> soroban_sdk::Vec<Option<EscrowData>> {
        let mut result: soroban_sdk::Vec<Option<EscrowData>> = soroban_sdk::Vec::new(&env);
        for i in 0..ids.len() {
            let Some(id) = ids.get(i) else {
                result.push_back(None);
                continue;
            };
            match load_escrow(&env, id) {
                Ok(escrow) => result.push_back(Some(escrow)),
                Err(_) => result.push_back(None),
            }
        }
        result
    }

    /// Returns the current fee configuration.
    pub fn get_fee_config(env: Env) -> FeeConfig {
        read_fee_config(&env)
    }

    /// Retrieves all escrow IDs associated with a specific vendor (seller).
    pub fn get_escrows_by_vendor(env: Env, vendor: Address) -> Vec<u64> {
        storage::read_vendor_escrow_index(&env, &vendor)
    }

    /// Retrieves all escrow IDs associated with a specific seller.
    pub fn get_escrows_by_seller(env: Env, seller: Address) -> Vec<u64> {
        storage::read_vendor_escrow_index(&env, &seller)
    }

    /// Returns on-chain counters for escrow lifecycle events.
    pub fn get_stats(env: Env) -> ContractStats {
        ContractStats {
            total_created: env
                .storage()
                .instance()
                .get(&DataKey::TotalCreated)
                .unwrap_or(0),
            total_completed: env
                .storage()
                .instance()
                .get(&DataKey::TotalCompleted)
                .unwrap_or(0),
            total_disputed: env
                .storage()
                .instance()
                .get(&DataKey::TotalDisputed)
                .unwrap_or(0),
            total_refunded: env
                .storage()
                .instance()
                .get(&DataKey::TotalRefunded)
                .unwrap_or(0),
        }
    }

    pub fn get_public_config(env: Env) -> PublicContractConfig {
        let fee_bps: u32 = read_fee_config(&env).protocol_fee_bps;
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);

        let current_counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(1);
        let escrow_count = current_counter.saturating_sub(1);

        PublicContractConfig {
            fee_bps,
            paused,
            escrow_count,
        }
    }

    pub fn get_contract_config(env: Env) -> Result<ContractConfig, ContractError> {
        let admin = require_admin(&env)?;
        admin.require_auth();

        let fee_bps: u32 = read_fee_config(&env).protocol_fee_bps;
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;
        let escrow_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(1u64)
            .saturating_sub(1);
        Ok(ContractConfig {
            admin,
            fee_bps,
            fee_collector,
            escrow_count,
        })
    }

    /// Rotates the resolver for an escrow. Callable by any payee or admin.
    /// New resolver must differ from current resolver, all payees, and buyer.
    pub fn rotate_resolver(
        env: Env,
        caller: Address,
        escrow_id: u64,
        new_resolver: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;

        let mut escrow = load_escrow(&env, escrow_id)?;
        let admin = require_admin(&env)?;

        let is_payee = {
            let mut found = false;
            for i in 0..escrow.payees.len() {
                let payee = escrow
                    .payees
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?;
                if caller == payee.address {
                    found = true;
                    break;
                }
            }
            found
        };

        if !is_payee && caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let is_terminal = matches!(
            escrow.state,
            EscrowState::Completed
                | EscrowState::Refunded
                | EscrowState::Canceled
                | EscrowState::Expired
        );
        if is_terminal {
            return Err(ContractError::InvalidState);
        }

        // Only support rotation for single-resolver escrows (backward compat)
        if let ResolverSet::Single(current_resolver) = &escrow.resolvers {
            if new_resolver == *current_resolver {
                return Err(ContractError::SameAddress);
            }
            // New resolver must differ from all payees
            for i in 0..escrow.payees.len() {
                let payee = escrow
                    .payees
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?;
                if new_resolver == payee.address {
                    return Err(ContractError::InvalidAddress);
                }
            }

            if escrow.buyer.as_ref() == Some(&new_resolver) {
                return Err(ContractError::InvalidAddress);
            }

            let old_resolver = current_resolver.clone();
            escrow.resolvers = ResolverSet::Single(new_resolver.clone());
            // escrow.state is untouched by rotation, so the pre-mutation
            // state is simply the current one.
            save_escrow(&env, escrow_id, &escrow, Some(&escrow.state));

            emit_resolver_rotated(&env, escrow_id, old_resolver, new_resolver);
            Ok(())
        } else {
            // Multi-resolver escrows don't support simple rotation
            // A separate function could be added for managing multi-resolver sets
            Err(ContractError::InvalidState)
        }
    }

    pub fn request_refund(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidStateTransition);
        }

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::RefundRequested;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        emit_refund_requested(
            &env,
            escrow_id,
            caller,
            prev_state,
            crate::EscrowState::RefundRequested,
        );
        Ok(())
    }

    pub fn approve_refund(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let mut is_payee = false;
        for i in 0..escrow.payees.len() {
            if caller
                == escrow
                    .payees
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?
                    .address
            {
                is_payee = true;
                break;
            }
        }
        if !is_payee {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::RefundRequested {
            return Err(ContractError::InvalidStateTransition);
        }

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(&env.current_contract_address(), &buyer, &escrow.amount);

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Refunded;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalRefunded)?;

        emit_refund_approved(
            &env,
            escrow_id,
            caller,
            prev_state,
            crate::EscrowState::Refunded,
        );
        Ok(())
    }

    pub fn batch_create_escrow(
        env: Env,
        seller: Address,
        escrows: Vec<EscrowInput>,
    ) -> Result<Vec<u64>, ContractError> {
        seller.require_auth();
        ensure_not_paused(&env)?;

        let mut escrow_ids = Vec::new(&env);
        for input in escrows.into_iter() {
            let mut payees = Vec::new(&env);
            payees.push_back(Payee {
                address: seller.clone(),
                bps: BASIS_POINTS,
            });
            let id = create_escrow_internal(
                &env,
                payees,
                input.buyer,
                input.resolver,
                input.token,
                input.amount,
                input.fee_bps,
                0, // resolver_fee_bps
                input.shipping_window,
                input.notes,
            )?;
            escrow_ids.push_back(id);
        }

        Ok(escrow_ids)
    }

    pub fn set_amount_limits(
        env: Env,
        caller: Address,
        min_amount: i128,
        max_amount: i128,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        if min_amount <= 0 || max_amount < min_amount {
            return Err(ContractError::InvalidAmount);
        }

        let old_min_amount = env
            .storage()
            .instance()
            .get(&DataKey::MinAmount)
            .unwrap_or(MIN_ESCROW_AMOUNT);
        let old_max_amount = env
            .storage()
            .instance()
            .get(&DataKey::MaxAmount)
            .unwrap_or(MAX_ESCROW_AMOUNT);

        env.storage()
            .instance()
            .set(&DataKey::MinAmount, &min_amount);
        env.storage()
            .instance()
            .set(&DataKey::MaxAmount, &max_amount);
        emit_amount_limits_updated(
            &env,
            old_min_amount,
            min_amount,
            old_max_amount,
            max_amount,
            caller,
        );
        Ok(())
    }

    pub fn multicall(env: Env, calls: Vec<ContractCall>) -> Result<Vec<Val>, ContractError> {
        ensure_not_paused(&env)?;
        let mut results = Vec::new(&env);

        let s_initialize = Symbol::new(&env, "initialize");
        let s_pause_contract = Symbol::new(&env, "pause_contract");
        let s_unpause_contract = Symbol::new(&env, "unpause_contract");
        let s_create_escrow = Symbol::new(&env, "create_escrow");
        let s_fund_escrow = Symbol::new(&env, "fund_escrow");
        let s_mark_shipped = Symbol::new(&env, "mark_shipped");
        let s_confirm_delivery = Symbol::new(&env, "confirm_delivery");
        let s_raise_dispute = Symbol::new(&env, "raise_dispute");
        let s_resolve_dispute = Symbol::new(&env, "resolve_dispute");
        let s_auto_release = Symbol::new(&env, "auto_release");
        let s_get_escrow = Symbol::new(&env, "get_escrow");
        let s_get_dispute = Symbol::new(&env, "get_dispute");
        let s_get_fee_config = Symbol::new(&env, "get_fee_config");
        let s_set_arbitration_fee = Symbol::new(&env, "set_arbitration_fee");
        let s_get_arbitration_fee = Symbol::new(&env, "get_arbitration_fee");
        let s_rotate_resolver = Symbol::new(&env, "rotate_resolver");
        let s_cancel_escrow = Symbol::new(&env, "cancel_escrow");

        for call in calls.into_iter() {
            let res_val: Val = if call.function == s_fund_escrow {
                let escrow_id: u64 = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let buyer: Address = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::fund_escrow(env.clone(), escrow_id, buyer)?;
                ().into_val(&env)
            } else if call.function == s_get_escrow {
                let escrow_id: u64 = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let res = Self::get_escrow(env.clone(), escrow_id)?;
                res.into_val(&env)
            } else if call.function == s_mark_shipped {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let tracking_id: String = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::mark_shipped(env.clone(), caller, escrow_id, tracking_id)?;
                ().into_val(&env)
            } else if call.function == s_confirm_delivery {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::confirm_delivery(env.clone(), caller, escrow_id)?;
                ().into_val(&env)
            } else if call.function == s_raise_dispute {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let reason: Symbol = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let description: String = call
                    .args
                    .get(3)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let evidence_hash: BytesN<32> = call
                    .args
                    .get(4)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::raise_dispute(
                    env.clone(),
                    caller,
                    escrow_id,
                    reason,
                    description,
                    evidence_hash,
                )?;
                ().into_val(&env)
            } else if call.function == s_resolve_dispute {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let resolution: ResolutionType = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::resolve_dispute(env.clone(), caller, escrow_id, resolution)?;
                ().into_val(&env)
            } else if call.function == s_auto_release {
                let escrow_id: u64 = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::auto_release(env.clone(), escrow_id)?;
                ().into_val(&env)
            } else if call.function == s_cancel_escrow {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::cancel_escrow(env.clone(), caller, escrow_id)?;
                ().into_val(&env)
            } else if call.function == s_rotate_resolver {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let new_resolver: Address = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::rotate_resolver(env.clone(), caller, escrow_id, new_resolver)?;
                ().into_val(&env)
            } else if call.function == s_initialize {
                let admin: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let fee_collector: Address = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let arbitration_fee_bps: u32 = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::initialize(env.clone(), admin, fee_collector, arbitration_fee_bps)?;
                ().into_val(&env)
            } else if call.function == s_pause_contract {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::pause_contract(env.clone(), caller)?;
                ().into_val(&env)
            } else if call.function == s_unpause_contract {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::unpause_contract(env.clone(), caller)?;
                ().into_val(&env)
            } else if call.function == s_get_dispute {
                let escrow_id: u64 = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let res = Self::get_dispute(env.clone(), escrow_id);
                res.into_val(&env)
            } else if call.function == s_get_fee_config {
                let res = Self::get_fee_config(env.clone());
                res.into_val(&env)
            } else if call.function == s_set_arbitration_fee {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let fee_bps: u32 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::set_arbitration_fee(env.clone(), caller, fee_bps)?;
                ().into_val(&env)
            } else if call.function == s_get_arbitration_fee {
                let res = Self::get_arbitration_fee(env.clone());
                res.into_val(&env)
            } else if call.function == s_create_escrow {
                let payees: Vec<Payee> = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let buyer: Option<Address> = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let resolver: Address = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let token: Address = call
                    .args
                    .get(3)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let amount: i128 = call
                    .args
                    .get(4)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let fee_bps: u32 = call
                    .args
                    .get(5)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let resolver_fee_bps: u32 = call
                    .args
                    .get(6)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let shipping_window: u64 = call
                    .args
                    .get(7)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let res = Self::create_escrow(
                    env.clone(),
                    payees.into_val(&env),
                    buyer,
                    resolver,
                    token,
                    amount,
                    fee_bps,
                    resolver_fee_bps,
                    shipping_window,
                    None,
                )?;
                res.into_val(&env)
            } else {
                return Err(ContractError::NotAuthorized);
            };
            results.push_back(res_val);
        }
        Ok(results)
    }

    // ── Issue #393: Resolver registry ──────────────────────────────────────

    /// Adds a resolver to the approved list. Only callable by admin.
    pub fn add_approved_resolver(
        env: Env,
        caller: Address,
        resolver: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        require_admin_caller(&env, &caller)?;

        let mut approved: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env));

        for existing in approved.iter() {
            if existing == resolver {
                return Ok(());
            }
        }
        approved.push_back(resolver.clone());
        env.storage()
            .instance()
            .set(&DataKey::ApprovedResolvers, &approved);
        emit_resolver_approved(&env, resolver, caller);
        Ok(())
    }

    /// Removes a resolver from the approved list. Only callable by admin.
    pub fn remove_approved_resolver(
        env: Env,
        caller: Address,
        resolver: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        require_admin_caller(&env, &caller)?;

        let approved: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env));

        let mut new_approved = soroban_sdk::Vec::new(&env);
        let mut found = false;
        for existing in approved.iter() {
            if existing == resolver {
                found = true;
            } else {
                new_approved.push_back(existing);
            }
        }

        if !found {
            return Err(ContractError::InvalidAddress);
        }
        env.storage()
            .instance()
            .set(&DataKey::ApprovedResolvers, &new_approved);
        emit_resolver_removed(&env, resolver, caller);
        Ok(())
    }

    /// Enables or disables strict resolver mode. Only callable by admin.
    /// When strict = true, create_escrow rejects resolvers not in the approved list.
    pub fn set_resolver_strict(
        env: Env,
        caller: Address,
        strict: bool,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        require_admin_caller(&env, &caller)?;
        let old_strict = env
            .storage()
            .instance()
            .get(&DataKey::ResolverStrict)
            .unwrap_or(false);
        env.storage()
            .instance()
            .set(&DataKey::ResolverStrict, &strict);
        emit_resolver_strict_updated(&env, old_strict, strict, caller);
        Ok(())
    }

    /// Returns the list of approved resolvers.
    pub fn get_approved_resolvers(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env))
    }

    /// Returns whether strict resolver mode is active.
    pub fn is_resolver_strict(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ResolverStrict)
            .unwrap_or(false)
    }

    // ── Issue #394: Emergency drain ─────────────────────────────────────────

    /// Emergency drain: transfers all escrowed funds back to the buyer.
    /// Requires the contract to be paused and both buyer and seller to co-sign.
    /// This is a last-resort escape hatch when the resolver is unavailable.
    #[allow(deprecated)]
    pub fn emergency_drain(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if !paused {
            return Err(ContractError::ContractNotPaused);
        }

        let mut escrow = load_escrow(&env, escrow_id)?;

        let is_drainable = matches!(
            escrow.state,
            EscrowState::Funded
                | EscrowState::Shipped
                | EscrowState::Disputed
                | EscrowState::PendingFinalization
        );
        if !is_drainable {
            return Err(ContractError::InvalidState);
        }

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        let seller = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();

        // Both parties must explicitly authorise the emergency drain.
        buyer.require_auth();
        seller.require_auth();

        token::Client::new(&env, &escrow.token).transfer(
            &env.current_contract_address(),
            &buyer,
            &escrow.amount,
        );
        payout_basket_tokens(&env, escrow_id, &buyer)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Refunded;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalRefunded)?;

        emit_emergency_drain(&env, escrow_id, buyer, seller);
        Ok(())
    }
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
