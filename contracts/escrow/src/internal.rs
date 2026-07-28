//! Shared internal helpers used across the instructions, admin, disputes,
//! and queries modules: storage read/write, validation, fee math, and
//! resolver-vote tallying. Not part of the contract's public interface.

use crate::*;
use soroban_sdk::{token, Address, Env, String, Symbol, Vec};

pub(crate) fn load_resolver_votes(env: &Env, escrow_id: u64) -> Vec<ResolverVote> {
    use crate::DataKey;
    env.storage()
        .persistent()
        .get(&DataKey::ResolverVotes(escrow_id))
        .unwrap_or(Vec::new(env))
}

/// Save resolver votes to storage
pub(crate) fn save_resolver_votes(env: &Env, escrow_id: u64, votes: &Vec<ResolverVote>) {
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
pub(crate) fn add_or_update_vote(
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
///
/// # Deadlock Scenario (Issue #667)
///
/// **Known Issue**: Voting can deadlock with split votes when no side reaches
/// the threshold. For example, if `threshold=3` and you get 2 Release + 2 Refund
/// votes, neither reaches 3 — funds stay stuck in Disputed state indefinitely.
///
/// **Mitigation Recommendations**:
/// - Implement a timeout-based default resolution after all resolvers have voted
/// - Add a majority-rules fallback when all resolvers have voted but no threshold is met
/// - Consider adding an escalation mechanism for deadlocked disputes
///
/// This is a known limitation of the current M-of-N voting system and should be
/// addressed in a future contract upgrade.
pub(crate) fn tally_votes(votes: &Vec<ResolverVote>, threshold: u32) -> Option<ResolutionType> {
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

/// Validity matrix for escrow state transitions (#9).
///
/// Returns `Ok(())` if the move from `from` to `to` is legal under the
/// escrow lifecycle, `Err(InvalidStateTransition)` otherwise. Provided as a
/// pure helper alongside the existing inline guards so reviewers can audit
/// every legal edge in one place.
#[allow(dead_code)]
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

pub(crate) fn ensure_not_paused(env: &Env) -> Result<(), ContractError> {
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

pub(crate) fn ensure_action_not_paused(env: &Env, action: Symbol) -> Result<(), ContractError> {
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

pub(crate) fn require_admin(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(ContractError::NotAuthorized)
}

pub(crate) fn require_admin_caller(env: &Env, caller: &Address) -> Result<Address, ContractError> {
    let admin = require_admin(env)?;
    if caller != &admin {
        return Err(ContractError::NotAuthorized);
    }
    Ok(admin)
}

pub(crate) fn default_fee_config() -> FeeConfig {
    FeeConfig {
        protocol_fee_bps: 0,
        arbitration_fee_bps: 0,
    }
}

pub(crate) fn read_fee_config(env: &Env) -> FeeConfig {
    env.storage()
        .instance()
        .get(&DataKey::FeeConfig)
        .unwrap_or_else(default_fee_config)
}

pub(crate) fn write_fee_config(env: &Env, fee_config: &FeeConfig) {
    env.storage()
        .instance()
        .set(&DataKey::FeeConfig, fee_config);
}

pub(crate) fn contains(list: &soroban_sdk::Vec<Address>, target: &Address) -> bool {
    for item in list.iter() {
        if item == *target {
            return true;
        }
    }
    false
}

pub(crate) fn is_token_allowlist_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::TokenAllowlistEnabled)
        .unwrap_or(false)
}

pub(crate) fn is_token_allowed(env: &Env, token: &Address) -> Result<(), ContractError> {
    if !is_token_allowlist_enabled(env) {
        return Ok(());
    }
    let allowlist: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::TokenAllowlist)
        .unwrap_or(soroban_sdk::Vec::new(env));
    if contains(&allowlist, token) {
        return Ok(());
    }
    Err(ContractError::TokenNotAllowed)
}

pub(crate) fn read_platform_fee_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::PlatformFeeBps)
        .unwrap_or(0)
}

pub(crate) fn write_platform_fee_bps(env: &Env, fee_bps: u32) {
    env.storage()
        .instance()
        .set(&DataKey::PlatformFeeBps, &fee_bps);
}

pub(crate) fn read_treasury(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .instance()
        .get(&DataKey::Treasury)
        .ok_or(ContractError::NotAuthorized)
}

pub(crate) fn write_treasury(env: &Env, treasury: &Address) {
    env.storage().instance().set(&DataKey::Treasury, treasury);
}

pub(crate) fn validate_escrow_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_ESCROW_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

/// Validates resolver set to ensure no conflicts with seller/buyer.
pub(crate) fn validate_resolvers(
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
        if count == 0 || m.threshold == 0 || m.threshold > count {
            return Err(ContractError::InvalidAmount); // Use as proxy for invalid threshold
        }

        // Ensure all resolvers are unique
        let mut seen = soroban_sdk::Vec::new(m.resolvers.env());
        for resolver in m.resolvers.iter() {
            if contains(&seen, &resolver) {
                return Err(ContractError::ConflictingRoles);
            }
            seen.push_back(resolver);
        }
    }

    Ok(())
}

pub(crate) fn validate_resolver_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_ESCROW_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

pub(crate) fn validate_payees(env: &Env, payees: &Vec<Payee>) -> Result<(), ContractError> {
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
        let zero = Address::from_string(&String::from_str(env, crate::ZERO_ADDRESS_STR));
        if payee.address == zero {
            return Err(ContractError::InvalidAddress);
        }
    }

    if total_bps != 10_000 {
        return Err(ContractError::PayeeBpsMismatch);
    }

    Ok(())
}

/// Validates individual protocol/arbitration fees against their respective maximums.
///
/// Returns Err(FeeExceedsMax) if the value exceeds its cap.
pub(crate) fn validate_protocol_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_PROTOCOL_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

pub(crate) fn validate_arbitration_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_ARBITRATION_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
    }
    Ok(())
}

/// Validates that the combined protocol + arbitration fees don't exceed MAX_COMBINED_FEE_BPS.
///
/// This prevents the attack where an admin sets both fees to their maximum values,
/// draining entire escrows through fees.
pub(crate) fn validate_combined_fees(
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

pub(crate) fn update_protocol_fee(
    env: &Env,
    caller: &Address,
    fee_bps: u32,
) -> Result<u32, ContractError> {
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
pub(crate) fn update_arbitration_fee(
    env: &Env,
    caller: &Address,
    fee_bps: u32,
) -> Result<u32, ContractError> {
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

pub(crate) fn get_ttl_extension(env: &Env) -> u32 {
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
pub(crate) fn save_escrow(
    env: &Env,
    id: u64,
    escrow: &EscrowData,
    prev_state: Option<&EscrowState>,
) {
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

pub(crate) fn load_escrow(env: &Env, id: u64) -> Result<EscrowData, ContractError> {
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

pub(crate) fn append_state_history(env: &Env, id: u64, state: &EscrowState) {
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

pub(crate) fn load_state_history(env: &Env, id: u64) -> Vec<(EscrowState, u64)> {
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

pub(crate) fn save_dispute(env: &Env, id: u64, dispute: &DisputeData) {
    let key = DataKey::Dispute(id);
    let ext = get_ttl_extension(env);
    env.storage().persistent().set(&key, dispute);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
}

pub(crate) fn load_dispute(env: &Env, id: u64) -> Result<DisputeData, ContractError> {
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

pub(crate) fn save_basket_tokens(env: &Env, escrow_id: u64, tokens: &soroban_sdk::Vec<TokenEntry>) {
    let key = DataKey::BasketTokens(escrow_id);
    let ext = get_ttl_extension(env);
    env.storage().persistent().set(&key, tokens);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
}

pub(crate) fn load_basket_tokens(env: &Env, escrow_id: u64) -> soroban_sdk::Vec<TokenEntry> {
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

pub(crate) fn transfer_with_protocol_fee(
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

pub(crate) fn distribute_to_payees(
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
            .checked_div(10_000)
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
pub(crate) fn payout_basket_tokens(
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

pub(crate) fn ensure_not_expired(env: &Env, escrow_id: u64) -> Result<(), ContractError> {
    if let Some(schedule) = env
        .storage()
        .persistent()
        .get::<DataKey, crate::ExpirySchedule>(&DataKey::PendingExpiry(escrow_id))
    {
        if env.ledger().timestamp() >= schedule.expires_at {
            return Err(ContractError::EscrowExpired);
        }
    }
    Ok(())
}

pub(crate) fn increment_counter(env: &Env, key: &DataKey) -> Result<(), ContractError> {
    let current: u64 = env.storage().instance().get(key).unwrap_or(0);
    let next = current
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage().instance().set(key, &next);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn create_escrow_internal(
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
        if !contains(&approved, &resolver) {
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
pub(crate) fn execute_resolution_transition(
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

pub(crate) fn escrow_created_at(env: &Env, escrow_id: u64) -> u64 {
    load_state_history(env, escrow_id)
        .get(0)
        .map(|(_, ts)| ts)
        .unwrap_or(0)
}
