//! Shared internal helpers used across the instructions, admin, disputes,
//! and queries modules: storage read/write, validation, fee math, and
//! resolver-vote tallying. Not part of the contract's public interface.

use crate::*;
use soroban_sdk::{token, Address, Env, String, Symbol, Vec};

/// Maps an escrow's terminal state to the most specific rejection error, or
/// returns `fallback` for states that have no dedicated code.
///
/// Call sites read as `return Err(terminal_state_error(&escrow.state,
/// ContractError::InvalidState));` so that an action rejected because the
/// escrow is already `Completed`/`Refunded` surfaces `EscrowAlreadyCompleted`/
/// `EscrowAlreadyRefunded` instead of a generic `InvalidState`. Every other
/// state (including the other terminals, `Canceled` and `Expired`) keeps the
/// caller-supplied `fallback`.
pub(crate) fn terminal_state_error(state: &EscrowState, fallback: ContractError) -> ContractError {
    match state {
        EscrowState::Completed => ContractError::EscrowAlreadyCompleted,
        EscrowState::Refunded => ContractError::EscrowAlreadyRefunded,
        _ => fallback,
    }
}

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

/// Tally votes and determine if resolution should be executed.
/// Returns the winning resolution if threshold is met.
///
/// See [`crate::types::MultiResolver`] for the deadlock risk this threshold
/// check is subject to, and the escape hatches that bound it.
pub(crate) fn tally_votes(
    votes: &Vec<ResolverVote>,
    threshold: u32,
) -> Result<Option<ResolutionType>, ContractError> {
    if votes.is_empty() {
        return Ok(None);
    }

    let mut release_count = 0u32;
    let mut refund_count = 0u32;

    for i in 0..votes.len() {
        if let Some(vote) = votes.get(i) {
            match vote.resolution {
                ResolutionType::Release => {
                    release_count = release_count
                        .checked_add(1)
                        .ok_or(ContractError::ArithmeticError)?;
                }
                ResolutionType::Refund => {
                    refund_count = refund_count
                        .checked_add(1)
                        .ok_or(ContractError::ArithmeticError)?;
                }
            }
        }
    }

    if release_count >= threshold {
        Ok(Some(ResolutionType::Release))
    } else if refund_count >= threshold {
        Ok(Some(ResolutionType::Refund))
    } else {
        Ok(None)
    }
}

/// Simple-majority fallback used once a multi-resolver vote has been
/// deadlocked past `DISPUTE_DEADLOCK_WINDOW`. Returns whichever side holds
/// strictly more votes; a perfect tie (including a vote set with no clear
/// winner) resolves to `Refund`, which is the conservative default because it
/// returns the escrowed principal to the buyer rather than paying it out.
pub(crate) fn tally_votes_majority(votes: &Vec<ResolverVote>) -> ResolutionType {
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

    if release_count > refund_count {
        ResolutionType::Release
    } else {
        ResolutionType::Refund
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
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        return Err(ContractError::ContractPaused);
    }
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

/// Returns whether the token allowlist is currently enforced.
///
/// # Security: the allowlist is **off** unless an operator turns it on
///
/// The flag defaults to `false`, so out of the box *any* SEP-41 contract can
/// be used as an escrow token. That is a deliberate backward-compatibility
/// default, **not** a safe production setting: every payout in this contract is
/// an external call into the token contract (`payout`, `distribute_to_payees`,
/// `payout_basket_tokens`, fee transfers), and a hostile token can re-enter the
/// escrow, burn the budget, or misreport balances while settlement is in
/// flight. Escrows created against an untrusted token therefore inherit that
/// token's risk.
///
/// # Deployer requirement (do this before accepting real value)
///
/// 1. Call
///    [`Escrow::set_token_allowlist_enabled`](crate::Escrow::set_token_allowlist_enabled)
///    with `enabled = true` (admin-only; the timelocked pair
///    `queue_set_token_allowlist_enabled` /
///    `execute_set_token_allowlist_enabled` is available when the change should
///    be delayed).
/// 2. Add every vetted token with
///    [`Escrow::add_allowed_token`](crate::Escrow::add_allowed_token), and
///    verify the result with
///    [`Escrow::get_allowed_tokens`](crate::Escrow::get_allowed_tokens).
/// 3. Only then open the contract to users.
///
/// Once enabled, [`is_token_allowed`] rejects any token that is not present in
/// the allowlist with [`ContractError::TokenNotAllowed`], and the check is
/// applied on every escrow-creation path (`create_escrow`,
/// `create_escrow_with_expiration`, `batch_create_escrow`,
/// `create_basket_escrow`). See `SECURITY.md` ("Token Allowlisting") for the
/// full deployment checklist.
pub(crate) fn is_token_allowlist_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::TokenAllowlistEnabled)
        .unwrap_or(false)
}

/// Enforces the token allowlist when it is enabled, otherwise accepts any token.
///
/// When [`is_token_allowlist_enabled`] is `true`, `token` must be present in the
/// admin-managed allowlist ([`DataKey::TokenAllowlist`]) or the call fails with
/// [`ContractError::TokenNotAllowed`]. When the flag is `false` the check is a
/// no-op and every token is accepted — see the security notes on
/// [`is_token_allowlist_enabled`] for why operators should enable it before
/// mainnet.
pub(crate) fn is_token_allowed(env: &Env, token: &Address) -> Result<(), ContractError> {
    if !is_token_allowlist_enabled(env) {
        return Ok(());
    }
    let allowlist: soroban_sdk::Map<Address, bool> = env
        .storage()
        .instance()
        .get(&DataKey::TokenAllowlist)
        .unwrap_or(soroban_sdk::Map::new(env));
    if allowlist.contains_key(token.clone()) {
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
        .ok_or(ContractError::NotInitialized)
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

/// Validates resolver set to ensure no conflicts with seller/buyer, and for a
/// `Fallback` set that the backup's `dispute_deadline` is within
/// `MAX_FALLBACK_DEADLINE_OFFSET` of the current ledger timestamp.
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
            return Err(ContractError::InvalidResolverThreshold);
        }

        // Ensure all resolvers are unique
        let mut seen = soroban_sdk::Vec::new(m.resolvers.env());
        for resolver in m.resolvers.iter() {
            if contains(&seen, &resolver) {
                return Err(ContractError::ConflictingRoles);
            }
            seen.push_back(resolver);
        }
    } else if let ResolverSet::Fallback(f) = resolvers {
        if f.primary == f.backup {
            return Err(ContractError::ConflictingRoles);
        }

        // An unbounded deadline (e.g. u64::MAX) would never admit the backup,
        // leaving disputed funds locked if the primary stops responding.
        let max_deadline = f
            .primary
            .env()
            .ledger()
            .timestamp()
            .checked_add(MAX_FALLBACK_DEADLINE_OFFSET)
            .ok_or(ContractError::ArithmeticOverflow)?;
        if f.dispute_deadline > max_deadline {
            return Err(ContractError::InvalidFallbackDeadline);
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
        let payee = payees.get(i).ok_or(ContractError::PayeeIndexOutOfBounds)?;
        let bps = payee.bps;

        // Check for overflow
        total_bps = total_bps
            .checked_add(bps)
            .ok_or(ContractError::ArithmeticError)?;

        // Validate each payee address is not zero
        let zero = crate::zero_address(env);
        if payee.address == zero {
            return Err(ContractError::InvalidAddress);
        }
    }

    if total_bps != 10_000 {
        return Err(ContractError::PayeeBpsMismatch);
    }

    Ok(())
}

pub(crate) fn validate_arbitration_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_ARBITRATION_FEE_BPS {
        return Err(ContractError::ArbitrationFeeExceedsMax);
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

/// Validates a proposed new fee collector and returns the current one.
///
/// Applies the same invariants `initialize` enforces on the *initial*
/// collector, so the immediate (`set_fee_collector`) and timelocked
/// (`execute_set_fee_collector`) update paths cannot drift from them:
/// - it may not be the all-zero address (`InvalidAddress`);
/// - it may not equal the current admin (`InvalidAddress`) — the admin and
///   fee-collector roles are kept separate so a single compromised key
///   cannot both govern the contract and sweep its fees;
/// - it may not be a no-op change (`SameAddress`).
///
/// Returns the current collector so the caller can emit
/// `fee_collector_pending`.
pub(crate) fn validate_fee_collector_change(
    env: &Env,
    new_collector: &Address,
) -> Result<Address, ContractError> {
    if *new_collector == crate::zero_address(env) {
        return Err(ContractError::InvalidAddress);
    }

    let admin = require_admin(env)?;
    if *new_collector == admin {
        return Err(ContractError::InvalidAddress);
    }

    let old_collector: Address = env
        .storage()
        .instance()
        .get(&DataKey::FeeCollector)
        .ok_or(ContractError::NotAuthorized)?;

    if *new_collector == old_collector {
        return Err(ContractError::SameAddress);
    }

    Ok(old_collector)
}

/// Updates the arbitration fee. Validates that arbitration fee + current
/// protocol fee doesn't exceed combined cap.
///
/// Does not call `caller.require_auth()` — both callers (`set_arbitration_fee`,
/// `execute_set_arbitration_fee`) authenticate `caller` at their own top, per
/// the standardized require_auth-at-entry-point convention.
pub(crate) fn update_arbitration_fee(
    env: &Env,
    caller: &Address,
    fee_bps: u32,
) -> Result<u32, ContractError> {
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

/// Effective TTL extension (in ledgers) applied to every `extend_ttl` call:
/// the admin-configured `TtlExtensionLedgers` value, or `DEFAULT_TTL_EXTENSION`
/// when none has been set.
///
/// Thin re-export of [`crate::storage::get_ttl_extension`] so the instructions,
/// admin, and disputes modules — which reach for helpers through `internal` —
/// resolve the value through the exact same code path as the `storage` layer,
/// leaving one implementation to keep correct.
pub(crate) fn get_ttl_extension(env: &Env) -> u32 {
    crate::storage::get_ttl_extension(env)
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

/// Load state history without extending TTL - used by query functions that
/// should not have side effects on storage rent.
pub(crate) fn load_state_history_no_ttl(env: &Env, id: u64) -> Vec<(EscrowState, u64)> {
    let key = DataKey::EscrowStateHistory(id);
    env.storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env))
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
    match env.storage().persistent().get(&key) {
        Some(tokens) => {
            let ext = get_ttl_extension(env);
            env.storage().persistent().extend_ttl(&key, ext / 2, ext);
            tokens
        }
        None => soroban_sdk::Vec::new(env),
    }
}

/// Drops any pending admin delivery proposal (`propose_record_delivery`).
/// A proposal can only exist while the escrow is `Shipped` with no recorded
/// delivery, so every transition out of `Shipped` other than
/// `record_delivery` (which consumes it) must call this, or the entry is
/// orphaned in persistent storage. A no-op when no proposal exists.
pub(crate) fn clear_delivery_proposal(env: &Env, escrow_id: u64) {
    let key = DataKey::DeliveryProposal(escrow_id);
    if env.storage().persistent().has(&key) {
        env.storage().persistent().remove(&key);
        crate::events::emit_delivery_proposal_cancelled(env, escrow_id);
    }
}

/// Tops up the TTL of every persistent entry owned by `escrow_id`, and of the
/// contract instance, to the full configured extension without reading or
/// writing any value. Backs the permissionless `extend_escrow_ttl` entry point.
///
/// The opportunistic extensions on reads and writes only fire once an entry's
/// TTL drops below `ext / TTL_THRESHOLD_DIVISOR`; here the caller is
/// explicitly paying rent to keep a dormant escrow alive, so every entry is
/// extended to `ext` regardless. Entries the escrow never created (e.g.
/// `Dispute` on an undisputed escrow) are skipped. Entries that have already
/// been archived cannot be revived this way and need a `RestoreFootprint`
/// operation first.
pub(crate) fn extend_escrow_ttl(env: &Env, escrow_id: u64) -> Result<(), ContractError> {
    let persistent = env.storage().persistent();
    let escrow_key = DataKey::Escrow(escrow_id);
    if !persistent.has(&escrow_key) {
        return Err(ContractError::EscrowNotFound);
    }

    let ext = get_ttl_extension(env);
    persistent.extend_ttl(&escrow_key, ext, ext);
    for key in [
        DataKey::EscrowStateHistory(escrow_id),
        DataKey::Dispute(escrow_id),
        DataKey::Messages(escrow_id),
        DataKey::PendingExpiry(escrow_id),
        DataKey::ResolverVotes(escrow_id),
        DataKey::BasketTokens(escrow_id),
        DataKey::DeliveryProposal(escrow_id),
    ] {
        if persistent.has(&key) {
            persistent.extend_ttl(&key, ext, ext);
        }
    }

    // Paged messages are stored one key per entry, so extend the count and each
    // message key individually — otherwise a dormant escrow's thread would be
    // the one part of its storage allowed to expire.
    let count_key = DataKey::MessageCount(escrow_id);
    if persistent.has(&count_key) {
        persistent.extend_ttl(&count_key, ext, ext);
    }
    let count: u32 = persistent.get(&count_key).unwrap_or(0);
    let mut index = 0;
    while index < count {
        let key = DataKey::Message(escrow_id, index);
        if persistent.has(&key) {
            persistent.extend_ttl(&key, ext, ext);
        }
        index += 1;
    }

    // The escrow is unusable if the instance (config, fee collector, and the
    // contract code it pins) is archived, so keep that alive too.
    env.storage().instance().extend_ttl(ext, ext);
    Ok(())
}

pub(crate) use crate::helpers::payout::{payout, transfer_with_protocol_fee};

/// Transfers `amount` of `token_addr` from `from` to `to`.
///
/// This is the only place in the contract that constructs a `token::Client`
/// and calls `transfer` — every escrow funding, refund, payout, and fee
/// transfer routes through this one call site, so there is exactly one spot
/// to audit for the actual cross-contract token transfer, and no call site
/// can drift from the others' argument order or shape.
///
/// This is a thin, direction-agnostic wrapper: unlike [`payout`], it does not
/// skip non-positive amounts or fix `from`/`to` to the contract's own
/// address, since it also serves the "collect from caller" direction used by
/// `fund_escrow`/`fund_basket_escrow`. Callers paying *out* of the contract
/// should prefer [`payout`], which wraps this with that zero-skip behavior.
pub(crate) fn transfer_helper(
    env: &Env,
    token_addr: &Address,
    from: &Address,
    to: &Address,
    amount: i128,
) {
    token::Client::new(env, token_addr).transfer(from, to, &amount);
}

/// Distributes the specified `amount` among the `payees` proportionally based on their BPS shares.
///
/// **Rounding Strategy Documented:**
/// To ensure the exact `amount` is fully distributed without leaving dust in the contract,
/// the function calculates the truncated (floor) amount for payees 1 through N, subtracting
/// each from a `remaining` accumulator. The primary payee (index 0) receives the entire
/// `remaining` balance. Because integer division truncates, this strategy intentionally
/// accumulates all rounding dust and awards it to the primary payee. While this silently
/// favors the first payee by up to `N-1` stroops, it guarantees exactly 100% of the funds
/// are distributed and avoids complex sub-stroop accounting.
pub(crate) fn distribute_to_payees(
    env: &Env,
    token_addr: &Address,
    payees: &Vec<Payee>,
    amount: i128,
) -> Result<(), ContractError> {
    if amount < 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut remaining = amount;

    // Calculate amounts for all payees except the first
    for i in 1..payees.len() {
        let payee = payees.get(i).ok_or(ContractError::PayeeIndexOutOfBounds)?;
        let payee_amount = amount
            .checked_mul(payee.bps as i128)
            .ok_or(ContractError::ArithmeticError)?
            .checked_div(10_000)
            .ok_or(ContractError::ArithmeticError)?;

        crate::helpers::payout::payout(env, token_addr, &payee.address, payee_amount);

        remaining = remaining
            .checked_sub(payee_amount)
            .ok_or(ContractError::ArithmeticError)?;
    }

    // First payee gets the remainder (rounding goes to first payee)
    let first_payee = payees.get(0).ok_or(ContractError::PayeeIndexOutOfBounds)?;
    payout(env, token_addr, &first_payee.address, remaining);

    Ok(())
}

/// Transfer all non-primary basket tokens to a recipient after the primary
/// token has been paid out by the calling function.
///
/// # Index-0 Invariant (Issue #708)
///
/// `save_basket_tokens` always stores the primary token (the one recorded in
/// `EscrowData.token`) at index 0, mirroring the order passed to
/// `create_basket_escrow`.  Rather than relying on that positional assumption
/// this function skips any entry whose token address matches `EscrowData.token`
/// by value, so the primary token is never double-paid regardless of the order
/// the basket was originally saved.  This makes the function safe even if the
/// token list is ever persisted in a different order.
pub(crate) fn payout_basket_tokens(
    env: &Env,
    escrow_id: u64,
    recipient: &Address,
) -> Result<(), ContractError> {
    let escrow = load_escrow(env, escrow_id)?;
    let primary_token = &escrow.token;
    let basket_tokens = load_basket_tokens(env, escrow_id);
    for i in 0..basket_tokens.len() {
        let entry = basket_tokens
            .get(i)
            .ok_or(ContractError::BasketIndexOutOfBounds)?;
        // Skip the primary token — it is always paid out by the calling function.
        if &entry.token == primary_token {
            continue;
        }
        crate::helpers::payout::payout(env, &entry.token, recipient, entry.amount);
    }
    Ok(())
}

/// State and timing gate for [`Escrow::auto_release`](crate::Escrow::auto_release).
///
/// `auto_release` is permissionless — anyone may call it — so the safety of the
/// flow rests entirely on these checks. They live here, separate from the
/// payout, so the release-eligibility rules are in one place and can be
/// reasoned about (and tested) on their own.
///
/// Returns `Ok(())` only when every condition holds:
/// - the escrow is `Funded` or `Shipped` (`InvalidState` otherwise);
/// - no dispute has been raised (`InvalidState`);
/// - the applicable no-dispute window has fully elapsed:
///   - delivery recorded: `now >= delivered_at + DELIVERY_RELEASE_WINDOW`,
///     else `ShippingWindowNotElapsed`;
///   - `Shipped` but no recorded delivery: `DeliveryNotRecorded` (delivery must
///     be recorded first);
///   - otherwise the buyer's dispute window must have opened
///     (`DeliveryBeforeDisputeWindow`) and the shipping window measured from
///     `shipped_at` — or `funded_at` if the escrow was never shipped — must
///     have elapsed (`ShippingWindowNotElapsed`).
pub(crate) fn ensure_auto_release_eligible(
    env: &Env,
    escrow: &EscrowData,
    escrow_id: u64,
) -> Result<(), ContractError> {
    if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
        return Err(terminal_state_error(
            &escrow.state,
            ContractError::InvalidState,
        ));
    }

    if load_dispute(env, escrow_id).is_ok() {
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

    Ok(())
}

/// Shared settlement path for the two "pay the payees and close the escrow"
/// flows: [`Escrow::auto_release`](crate::Escrow::auto_release) and
/// [`Escrow::confirm_delivery`](crate::Escrow::confirm_delivery).
///
/// Takes the protocol fee off the top (paid to the fee collector), splits the
/// remainder across `escrow.payees`, forwards any non-primary basket tokens to
/// the primary payee, then flips the escrow to `Completed` and bumps the
/// completed-escrow counter.
///
/// `fee_bps` is passed in rather than read here because the callers source it
/// differently — `auto_release` uses the current global protocol fee,
/// `confirm_delivery` uses the rate snapshotted on the escrow — and that
/// difference is preserved deliberately.
///
/// Returns `(prev_state, first_payee)` so the caller can emit its own
/// completion event.
pub(crate) fn settle_escrow_to_payees(
    env: &Env,
    escrow: &mut EscrowData,
    escrow_id: u64,
    fee_bps: u32,
) -> Result<(EscrowState, Address), ContractError> {
    let fee_collector: Address = env
        .storage()
        .instance()
        .get(&DataKey::FeeCollector)
        .ok_or(ContractError::NotInitialized)?;

    let first_payee_addr = escrow
        .payees
        .get(0)
        .ok_or(ContractError::PayeeIndexOutOfBounds)?
        .address
        .clone();

    let prev_state = escrow.state.clone();
    escrow.state = EscrowState::Completed;
    save_escrow(env, escrow_id, escrow, Some(&prev_state));
    increment_sharded_counter(env, COUNTER_KIND_COMPLETED, escrow_id)?;
    // A delivery proposal can only exist while the escrow is Shipped, so every
    // completion transition must drop it or the entry is orphaned in storage.
    clear_delivery_proposal(env, escrow_id);
    increment_counter(env, &DataKey::TotalCompleted)?;

    let (protocol_fee, net_amount) =
        crate::helpers::payout::calculate_protocol_fee(escrow.amount, fee_bps)?;

    // ── INTERACTIONS (external token transfers) ──
    //
    // The `Completed` transition, the lifecycle counters, and the delivery
    // proposal cleanup are all persisted above, so a malicious or re-entrant
    // token invoked mid-transfer cannot observe an escrow that is still
    // `Funded`/`Shipped` while its funds are already moving. A re-entrant call
    // into `cancel_escrow`, `raise_dispute`, or `auto_release` therefore sees a
    // terminal `Completed` escrow and is rejected.
    if protocol_fee > 0 {
        payout(env, &escrow.token, &fee_collector, protocol_fee);
    }
    distribute_to_payees(env, &escrow.token, &escrow.payees, net_amount)?;
    payout_basket_tokens(env, escrow_id, &first_payee_addr)?;

    Ok((prev_state, first_payee_addr))
}

/// Check if an escrow has an active PendingExpiry scheduled (Issue #811).
/// This function only checks expiry for escrows still in Pending state; the
/// PendingExpiry key is semantically bound to Pending lifetime. Callers must
/// ensure they remove DataKey::PendingExpiry when transitioning away from Pending
/// (fund_escrow, fund_basket_escrow, reclaim_expired, cancel_escrow,
/// auto_cancel_pending). Without removal,
/// this check will incorrectly reject valid operations on funded escrows.
pub(crate) fn ensure_not_expired(env: &Env, escrow_id: u64) -> Result<(), ContractError> {
    let escrow = load_escrow(env, escrow_id)?;

    // Check custom expiration stored in EscrowData
    if let Some(expires_at) = escrow.expires_at {
        if env.ledger().timestamp() >= expires_at {
            return Err(ContractError::EscrowExpired);
        }
    }

    // Also check the automatic pending timeout (for unfunded escrows)
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

/// Counter kind discriminants used by [`increment_sharded_counter`] and
/// [`read_sharded_counter_total`]. Keep stable: they are persisted in storage.
pub(crate) const COUNTER_KIND_CREATED: u32 = 0;
pub(crate) const COUNTER_KIND_COMPLETED: u32 = 1;
pub(crate) const COUNTER_KIND_DISPUTED: u32 = 2;
pub(crate) const COUNTER_KIND_REFUNDED: u32 = 3;

/// Increments the legacy singleton lifecycle counter stored at `key`.
///
/// Kept alongside the sharded counters ([`increment_sharded_counter`]) for
/// backward compatibility: [`read_counter_total`] sums the legacy value and the
/// shards, so pre-sharding deployments' statistics are preserved while new
/// transitions spread their writes. Arithmetic overflow is reported as
/// `ArithmeticError`.
pub(crate) fn increment_counter(env: &Env, key: &DataKey) -> Result<(), ContractError> {
    let current: u64 = env.storage().instance().get(key).unwrap_or(0);
    let next = current
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage().instance().set(key, &next);
    Ok(())
}

/// Increments lifecycle counter `kind` in one of `crate::COUNTER_SHARDS`
/// persistent-storage buckets, chosen from `seed` (the escrow id). Spreading
/// writers across distinct keys avoids the serialization a single global
/// instance-storage counter imposes on every escrow transition.
pub(crate) fn increment_sharded_counter(
    env: &Env,
    kind: u32,
    seed: u64,
) -> Result<(), ContractError> {
    let bucket = (seed % crate::COUNTER_SHARDS as u64) as u32;
    let key = DataKey::ShardedCounter(kind, bucket);
    let current: u64 = env.storage().persistent().get(&key).unwrap_or(0);
    let next = current
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage().persistent().set(&key, &next);
    let ext = get_ttl_extension(env);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
    Ok(())
}

/// Sums every shard of lifecycle counter `kind`. Read-only: does not extend
/// TTL, since these are analytics counters rather than escrow-critical state.
pub(crate) fn read_sharded_counter_total(env: &Env, kind: u32) -> u64 {
    let mut total: u64 = 0;
    for bucket in 0..crate::COUNTER_SHARDS {
        let key = DataKey::ShardedCounter(kind, bucket);
        let value: u64 = env.storage().persistent().get(&key).unwrap_or(0);
        total = total.saturating_add(value);
    }
    total
}

/// Total for a lifecycle counter: the legacy singleton value (written by
/// deployments before sharding) plus the sum of its shards, so pre-existing
/// statistics are preserved across the upgrade.
pub(crate) fn read_counter_total(env: &Env, kind: u32, legacy_key: &DataKey) -> u64 {
    let legacy: u64 = env.storage().instance().get(legacy_key).unwrap_or(0);
    legacy.saturating_add(read_sharded_counter_total(env, kind))
}

/// Reads the admin-configured maximum dispute duration, falling back to
/// [`crate::DEFAULT_DISPUTE_TIMEOUT`] when the admin has not set one.
pub(crate) fn read_dispute_timeout(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::DisputeTimeout)
        .unwrap_or(crate::DEFAULT_DISPUTE_TIMEOUT)
}

pub(crate) fn write_dispute_timeout(env: &Env, timeout: u64) {
    env.storage()
        .instance()
        .set(&DataKey::DisputeTimeout, &timeout);
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
    expires_at: Option<u64>,
    grace_period: u64,
) -> Result<u64, ContractError> {
    // Does not call `payees[0].require_auth()` — every caller (`create_escrow`,
    // `create_escrow_with_expiration`, `batch_create_escrow`) authenticates
    // the seller/first payee at its own top, per the standardized
    // require_auth-at-entry-point convention. Do not call this helper from a
    // new entry point without adding that check there first.
    if payees.is_empty() {
        return Err(ContractError::InvalidAddress);
    }
    // Authentication is the entry point's responsibility: `create_escrow`,
    // `create_escrow_with_expiration`, and `batch_create_escrow` each call
    // `payees[0].require_auth()` (or `seller.require_auth()`) at their top
    // before delegating here. Requiring it again inside this helper would be a
    // second `require_auth` for the same address in the same invocation, which
    // the host rejects with `Error(Auth, ExistingValue)` ("frame is already
    // authorized"). Do not call this helper from a new entry point without
    // adding that check there first.

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
        let payee = payees.get(i).ok_or(ContractError::PayeeIndexOutOfBounds)?;
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

    // Issue #813: Use centralized next_escrow_id helper to consolidate counter
    // management. This function (create_escrow_internal) is the core implementation
    // used by create_escrow and other entry points. By using next_escrow_id here
    // instead of duplicating the counter increment + TTL extension logic, we ensure
    // all paths stay synchronized. create_escrow_with_fallback also duplicated
    // this logic and has been consolidated as well.
    let escrow_id = next_escrow_id(env)?;

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
        expires_at,
        grace_period,
    };

    save_escrow(env, escrow_id, &escrow, None);

    let first_payee_addr = payees
        .get(0)
        .ok_or(ContractError::PayeeIndexOutOfBounds)?
        .address
        .clone();
    storage::append_vendor_escrow_index(env, &first_payee_addr, escrow_id);

    increment_sharded_counter(env, COUNTER_KIND_CREATED, escrow_id)?;
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
        escrow.expires_at,
        crate::EscrowState::Pending,
    );
    Ok(escrow_id)
}

/// Execute the resolution transition once a resolution has been determined.
/// Transitions the escrow to `PendingFinalization`.
///
/// When `charge_fees` is true, deducts the arbitration and resolver fees from
/// the escrow (once per dispute, see below) and pays the resolver fee to
/// `caller`. Timeout fallbacks pass `false`, since no resolver actually
/// decided the outcome and charging a resolver fee would be inappropriate.
///
/// # Fee timing
/// Arbitration and resolver fees are charged to the escrow **once per
/// dispute**, not once per appeal round. `fees_charged` on the dispute record
/// means a prior round already deducted and paid them out (`clear_resolution`
/// deliberately preserves it and the recorded amounts), so later rounds reuse
/// the recorded amounts and skip the deduction, the accounting bump, and the
/// transfers. A dedicated flag is required: the recorded amounts can
/// legitimately be zero, and inferring "not yet charged" from that would let
/// an appeal pick up a since-raised fee config.
pub(crate) fn execute_resolution_transition(
    env: &Env,
    escrow_id: u64,
    escrow: EscrowData,
    caller: Address,
    final_resolution: ResolutionType,
    votes: Vec<ResolverVote>,
    charge_fees: bool,
) -> Result<(), ContractError> {
    let mut dispute_data = load_dispute(env, escrow_id)?;
    let fees_already_charged = dispute_data.fees_charged;

    let (arbitration_fee, resolver_fee) = if fees_already_charged {
        (dispute_data.arbitration_fee, dispute_data.resolver_fee)
    } else if !charge_fees {
        (0, 0)
    } else {
        let arbitration_fee_bps = read_fee_config(env).arbitration_fee_bps;
        let arbitration_fee =
            crate::helpers::payout::calculate_fee(escrow.amount, arbitration_fee_bps)?;
        let resolver_fee =
            crate::helpers::payout::calculate_fee(escrow.amount, escrow.resolver_fee_bps)?;

        let combined_fee = arbitration_fee
            .checked_add(resolver_fee)
            .ok_or(ContractError::ArithmeticError)?;
        if combined_fee > escrow.amount {
            return Err(ContractError::FeeExceedsMax);
        }

        (arbitration_fee, resolver_fee)
    };

    // fee_collector is only needed for the transfer below, but the lookup
    // itself is a Check (a plain storage read), not an Interaction — resolved
    // up front alongside the other fallible reads, before any state mutates.
    let fee_collector: Option<Address> = if fees_already_charged {
        None
    } else {
        Some(
            env.storage()
                .instance()
                .get(&DataKey::FeeCollector)
                .ok_or(ContractError::NotInitialized)?,
        )
    };

    let prev_state = escrow.state.clone();
    let mut updated_escrow = escrow;

    if !fees_already_charged && charge_fees {
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
    }

    // Store resolution in dispute data and transition to PendingFinalization
    let now = env.ledger().timestamp();
    let appeal_deadline = now
        .checked_add(APPEAL_WINDOW)
        .ok_or(ContractError::ArithmeticError)?;

    dispute_data.set_resolution(final_resolution.clone());
    dispute_data.resolved_by = Some(caller.clone());
    dispute_data.resolved_at = now;
    dispute_data.arbitration_fee = arbitration_fee;
    dispute_data.resolver_fee = resolver_fee;
    dispute_data.fees_charged = true;

    updated_escrow.state = EscrowState::PendingFinalization;

    // ── EFFECTS (state mutations) — must precede all external calls (CEI) ──
    save_escrow(env, escrow_id, &updated_escrow, Some(&prev_state));
    save_dispute(env, escrow_id, &dispute_data);
    save_resolver_votes(env, escrow_id, &votes);

    // ── INTERACTIONS (external token transfers) ──
    if let Some(fee_collector) = fee_collector {
        crate::helpers::payout::payout(env, &updated_escrow.token, &fee_collector, arbitration_fee);
        crate::helpers::payout::payout(env, &updated_escrow.token, &caller, resolver_fee);
    }

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
