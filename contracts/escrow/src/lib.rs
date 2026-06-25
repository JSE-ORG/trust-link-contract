#![allow(deprecated, unused_imports)]
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, BytesN, Env, String, Symbol, Vec,
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
    emit_admin_rotated, emit_arbitration_fee_updated, emit_auto_released,
    emit_contract_initialized, emit_contract_paused, emit_contract_unpaused,
    emit_delivery_recorded, emit_dispute_raised, emit_dispute_resolved, emit_escrow_cancelled,
    emit_escrow_completed, emit_escrow_created, emit_escrow_funded, emit_escrow_shipped,
    emit_fee_updated, emit_fees_withdrawn, emit_protocol_fee_updated, emit_resolver_rotated,
    AdminRotated, ArbitrationFeeUpdated, AutoReleased, ContractInitialized, ContractPausedEvent,
    ContractUnpausedEvent, DeliveryRecorded, DisputeRaised, DisputeResolved, EscrowCancelled,
    EscrowCompleted, EscrowCreated, EscrowFunded, EscrowShipped, FeeUpdated, FeesWithdrawn,
    ProtocolFeeUpdated, ResolverRotated,
    emit_admin_rotated, emit_auto_released, emit_contract_initialized, emit_contract_paused,
    emit_contract_unpaused, emit_delivery_recorded, emit_dispute_raised, emit_dispute_resolved,
    emit_escrow_cancelled, emit_escrow_completed, emit_escrow_created, emit_escrow_funded,
    emit_escrow_shipped, emit_fee_updated, emit_fees_withdrawn, emit_arbitration_fee_updated,
    emit_protocol_fee_updated, emit_resolver_rotated,
    emit_token_allowlist_updated, emit_allowlist_toggled,
    emit_dispute_pending_finalization, emit_dispute_appealed,
    emit_platform_fee_updated, emit_treasury_updated,
    emit_basket_escrow_created,
};
pub use crate::types::{
    ContractConfig, ContractStats, DataKey, DisputeData, DisputeStatus, EscrowData, EscrowState,
    FeeConfig, PublicContractConfig, ResolutionType,
};

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

/// Maximum length for user-supplied string fields.
/// - `tracking_id`: 64 characters
/// - `description` in `raise_dispute`: 256 characters
/// - `notes`: 500 characters
pub const MAX_TRACKING_ID_LEN: u32 = 64;
pub const MAX_DESCRIPTION_LEN: u32 = 256;
pub const MAX_NOTES_LEN: u32 = 500;

/// Maximum escrow amount intentionally capped to
/// preserve arithmetic safety for fee calculations
/// and aggregate accounting operations.
pub const MAX_ESCROW_AMOUNT: i128 = i128::MAX / 10_000;

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
            | (Shipped, Completed)
            | (Shipped, Disputed)
            | (Shipped, Refunded)
            | (Disputed, Completed)
            | (Disputed, Refunded)
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
    env.storage().instance().set(&DataKey::FeeConfig, fee_config);
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
    env.storage().instance().set(&DataKey::PlatformFeeBps, &fee_bps);
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowData {
    pub seller: Address,
    pub buyer: Option<Address>,
    pub resolver: Address,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub shipping_window: u64,
    pub funded_at: u64,
    pub dispute_deadline: u64,
    pub shipped_at: u64,
    pub delivered_at: Option<u64>,
    pub tracking_id: Option<String>,
    pub state: EscrowState,
    env.storage()
        .instance()
        .set(&DataKey::FeeConfig, fee_config);
}

fn validate_escrow_fee_bps(fee_bps: u32) -> Result<(), ContractError> {
    if fee_bps > MAX_ESCROW_FEE_BPS {
        return Err(ContractError::FeeExceedsMax);
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

fn save_escrow(env: &Env, id: u64, escrow: &EscrowData) {
    let key = DataKey::Escrow(id);
    let ext = get_ttl_extension(env);
    env.storage().persistent().set(&key, escrow);
    env.storage().persistent().extend_ttl(&key, ext / 2, ext);
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

/// Deducts the protocol fee from `amount` and transfers the net to `recipient`,
/// leaving the fee in the contract vault for the admin to sweep via
/// `withdraw_fees`.
///
/// Rounding policy: **floor** — `fee = floor(amount * fee_bps / 10_000)` and
/// `net = amount - fee`, so the truncated remainder accrues to `recipient` and
/// the invariant `net + fee == amount` always holds (no stranded dust). This
/// mirrors [`helpers::payout::calculate_fee`]; the two implementations must stay
/// in sync. The calculation is split to avoid overflowing `i128` for large
/// amounts. Overflow surfaces as `ArithmeticError` (distinct from the helper's
/// `ArithmeticOverflow`).
fn deduct_and_transfer(
    env: &Env,
    token_addr: &Address,
    recipient: &Address,
    amount: i128,
    fee_bps: u32,
) -> Result<(), ContractError> {
    if amount < 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Split calculation to avoid overflow for large amounts
    let part1 = (amount / 10_000)
        .checked_mul(fee_bps as i128)
        .ok_or(ContractError::ArithmeticError)?;
    let part2 = (amount % 10_000)
        .checked_mul(fee_bps as i128)
        .ok_or(ContractError::ArithmeticError)?
        / 10_000;

    let fee = part1
        .checked_add(part2)
        .ok_or(ContractError::ArithmeticError)?;
    let net = amount
        .checked_sub(fee)
        .ok_or(ContractError::ArithmeticError)?;

    token::Client::new(env, token_addr).transfer(&env.current_contract_address(), recipient, &net);
    Ok(())
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

fn increment_counter(env: &Env, key: &DataKey) -> Result<(), ContractError> {
    let current: u64 = env.storage().instance().get(key).unwrap_or(0);
    let next = current
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage().instance().set(key, &next);
    Ok(())
}

fn create_escrow_internal(
    env: &Env,
    seller: Address,
    buyer: Option<Address>,
    resolver: Address,
    token: Address,
    amount: i128,
    fee_bps: u32,
    shipping_window: u64,
    notes: Option<String>,
) -> Result<u64, ContractError> {
    seller.require_auth();

    ensure_not_paused(env)?;

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

    // Validate notes length if present
    if let Some(ref n) = notes {
        if n.len() > MAX_NOTES_LEN {
            return Err(ContractError::InputTooLong);
        }
    }

    // Security: all three roles must be distinct to preserve the trustless
    // three-party separation.
    if resolver == seller {
        return Err(ContractError::ConflictingRoles);
    }
    if let Some(ref b) = buyer {
        if b == &seller || b == &resolver {
            return Err(ContractError::ConflictingRoles);
        }
    }

    let escrow_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::EscrowCounter)
        .expect("counter initialized");
    let next_id = escrow_id.checked_add(1).ok_or(ContractError::ArithmeticError)?;
    env.storage()
        .instance()
        .set(&DataKey::EscrowCounter, &next_id);
    // Extend instance storage TTL on every counter access so the counter key
    // cannot expire between a read and the subsequent write.
    let ext = get_ttl_extension(env);
    env.storage().instance().extend_ttl(ext / 2, ext);

    let escrow = EscrowData {
        seller,
        buyer,
        resolver,
        token,
        amount,
        fee_bps,
        shipping_window,
        funded_at: 0,
        dispute_deadline: 0,
        state: EscrowState::Pending,
        shipped_at: 0,
        delivered_at: None,
        tracking_id: None,
        notes,
    };

    save_escrow(env, escrow_id, &escrow);

    let mut vendor_escrows = storage::read_vendor_escrow_index(env, &escrow.seller);
    vendor_escrows.push_back(escrow_id);
    // write_vendor_escrow_index now handles TTL extension automatically
    storage::write_vendor_escrow_index(env, &escrow.seller, &vendor_escrows);

    increment_counter(env, &DataKey::TotalCreated)?;
    emit_escrow_created(
        env,
        escrow_id,
        escrow.seller.clone(),
        escrow.resolver.clone(),
        escrow.token.clone(),
        escrow.amount,
        escrow.fee_bps,
        escrow.shipping_window,
    );
    Ok(escrow_id)
}

#[allow(clippy::too_many_arguments)]
#[contractimpl]
impl Escrow {
    /// Sets the protocol fee collector, admin address, and arbitration fee. Must be called once.
    ///
    /// Returns `Err(ContractError::InvalidAddress)` if `admin` or `fee_collector` is the
    /// all-zero/empty Stellar account address (#55). Returning early on validation failure
    /// guarantees no storage entries (`Admin`, `FeeCollector`, `ArbitrationFee`,
    /// `EscrowCounter`, `Paused`) are written, leaving the contract uninitialized.
    pub fn initialize(
        env: Env,
        admin: Address,
        fee_collector: Address,
        arbitration_fee_bps: u32,
    ) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        // admin and fee_collector must be distinct keys: sharing one address
        // means compromising the admin key also compromises all fee revenue.
        if admin == fee_collector {
            return Err(ContractError::InvalidAddress);
        }
        // Validate arbitration fee against the strict 5% cap (MAX_ARBITRATION_FEE_BPS)
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

        emit_contract_initialized(&env, admin, fee_collector, arbitration_fee_bps);
        Ok(())
    }

    pub fn pause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
        caller.require_auth();

        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        env.storage().instance().set(&DataKey::Paused, &true);
        emit_contract_paused(&env, admin);
        Ok(())
    }

    pub fn unpause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
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

    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        let old_admin = require_admin(&env)?;
        old_admin.require_auth();
        // Reject no-op rotations to the same address so monitoring isn't polluted
        // with misleading AdminRotated events.
        if new_admin == old_admin {
            return Err(ContractError::SameAddress);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        emit_admin_rotated(&env, old_admin, new_admin);
        Ok(())
    }

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

    pub fn set_ttl_extension(env: Env, caller: Address, ledgers: u32) -> Result<(), ContractError> {
        caller.require_auth();

        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        env.storage()
            .instance()
            .set(&DataKey::TtlExtensionLedgers, &ledgers);
        Ok(())
    }

    pub fn withdraw_fees(
        env: Env,
        caller: Address,
        token: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
        caller.require_auth();

        ensure_not_paused(&env)?;
        let _admin = require_admin_caller(&env, &caller)?;

        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }

        // Only allow withdrawals up to the fees that have actually accumulated in
        // the vault from dispute resolutions. This prevents draining buyer funds
        // that are locked in active escrows.
        let fee_key = DataKey::AccumulatedFees(token.clone());
        let accumulated: i128 = env.storage().instance().get(&fee_key).unwrap_or(0);
        if amount > accumulated {
            return Err(ContractError::InsufficientBalance);
        }

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &to, &amount);

        let new_accumulated = accumulated
            .checked_sub(amount)
            .ok_or(ContractError::ArithmeticError)?;
        env.storage().instance().set(&fee_key, &new_accumulated);

        emit_fees_withdrawn(&env, token, to, amount);

        Ok(())
    }

    pub fn set_fee_collector(env: Env, new_collector: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        admin.require_auth();

        let old_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;

        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &new_collector);
        env.events()
            .publish(("FeeCollectorUpdated",), (old_collector, new_collector));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_escrow(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        resolver: Address,
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

        // Security: all three roles must be distinct to preserve the trustless
        // three-party separation.  A resolver that equals the seller or buyer can
        // unilaterally resolve disputes in their own favour; a buyer that equals
        // the seller makes the escrow a self-dealing no-op.
        if resolver == seller {
            return Err(ContractError::ConflictingRoles);
        }
        if let Some(ref b) = buyer {
            if b == &seller || b == &resolver {
                return Err(ContractError::ConflictingRoles);
            }
        }

        // Token allowlist check
        is_token_allowed(&env, &token)?;

        let escrow_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .expect("counter initialized");
        let next_id = escrow_id
            .checked_add(1)
            .ok_or(ContractError::ArithmeticError)?;
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &next_id);

        // Extend instance storage TTL on every counter access so the counter key
        // cannot expire between a read and the subsequent write.
        let ext = get_ttl_extension(&env);
        env.storage().instance().extend_ttl(ext / 2, ext);

        let escrow = EscrowData {
        create_escrow_internal(
            &env,
            seller,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            shipping_window,
            None,
        )
    }

    pub fn create_escrow_with_notes(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
        notes: Option<String>,
    ) -> Result<u64, ContractError> {
        create_escrow_internal(
            &env,
            seller,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            shipping_window,
            notes,
        )
    }

    /// Buyer funds a pending escrow. Transitions Pending → Funded.
    ///
    /// Transfers `escrow.amount` tokens from the buyer to the contract vault,
    /// records the buyer address, and starts the dispute-deadline clock.
    pub fn fund_escrow(env: Env, escrow_id: u64, buyer: Address) -> Result<(), ContractError> {
        buyer.require_auth();

        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        // Security: buyer must differ from seller and resolver.
        if buyer == escrow.seller {
            return Err(ContractError::ConflictingRoles);
        }
        if buyer == escrow.resolver {
            return Err(ContractError::ConflictingRoles);
        }
        // If an intended buyer was specified at creation, only that address may fund.
        if let Some(ref expected_buyer) = escrow.buyer {
            if &buyer != expected_buyer {
                return Err(ContractError::NotAuthorized);
            }
        }

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(&buyer, &env.current_contract_address(), &escrow.amount);

        let now = env.ledger().timestamp();
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
        env.storage().persistent().extend_ttl(&buyer_key, ext / 2, ext);

        save_escrow(&env, escrow_id, &escrow);
        emit_escrow_funded(&env, escrow_id, buyer, escrow.amount);
        Ok(())
    }

    /// Buyer raises a dispute on a funded or shipped escrow.
    ///
    /// Transitions Funded/Shipped → Disputed, stores dispute metadata,
    /// and emits the `dispute_raised` event.
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
            return Err(ContractError::DeliveryBeforeDisputeWindow);
        }

        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(ContractError::InputTooLong);
        }

        escrow.state = EscrowState::Disputed;

        let dispute_data = DisputeData {
            escrow_id,
            reason: reason.clone(),
            description: description.clone(),
            evidence_hash: evidence_hash.clone(),
            status: DisputeStatus::Active,
            disputed_at: env.ledger().timestamp(),
            tracking_id: escrow.tracking_id.clone(),
        };

        save_escrow(&env, escrow_id, &escrow);
        save_dispute(&env, escrow_id, &dispute_data);
        increment_counter(&env, &DataKey::TotalDisputed)?;
        emit_dispute_raised(
            &env,
            escrow_id,
            buyer,
            reason,
            description,
            evidence_hash,
        );
        Ok(())
    }

    /// Posts a message for a given escrow. Messages are immutable and stored on-chain.
    /// Returns an error if the contract is paused or the escrow does not exist.
    pub fn post_message(
        env: Env,
        escrow_id: u64,
        sender: Address,
        content: String,
    ) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;
        // Verify escrow exists
        let _ = load_escrow(&env, escrow_id)?;

        let message = Message {
            sender: sender.clone(),
            timestamp: env.ledger().timestamp(),
            content,
        };
        let key = DataKey::Messages(escrow_id);
        // Load existing messages or create new Vec
        let mut msgs: Vec<Message> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        msgs.push_back(message);
        env.storage().persistent().set(&key, &msgs);
        emit_message_posted(&env, escrow_id, sender.clone());
        Ok(())
    }

    /// Retrieves messages for a given escrow with pagination.
    /// `start` is the zero‑based index of the first message to return.
    /// `limit` caps the number of messages returned (max 50).
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

    pub fn cancel_escrow(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();

        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;
        let buyer = escrow.buyer.clone();

        let buyer = escrow.buyer.clone();
        if escrow.seller != caller && buyer.as_ref() != Some(&caller) {
            return Err(ContractError::NotAuthorized);
        }

        save_escrow(&env, escrow_id, &escrow);
        emit_escrow_cancelled(&env, escrow_id, caller);
        Ok(())
    }

    /// Cancels a funded—but not yet shipped—escrow by mutual agreement and
    /// refunds the buyer in full.
    ///
    /// Unlike `raise_dispute`/`resolve_dispute`, this provides a no-dispute exit
    /// for an order that both sides agree to call off while it is still in
    /// `Funded` (e.g. the seller can no longer fulfil it). Both the seller and
    /// the buyer must authorize the call; the full escrowed amount is returned
    /// to the buyer and the escrow transitions to `Canceled`.
    pub fn mutual_cancel(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;

        let mut escrow = load_escrow(&env, escrow_id)?;
        let buyer = escrow.buyer.clone().ok_or(ContractError::EscrowHasNoBuyer)?;

        // Require both parties to sign: a mutual cancellation is only valid with
        // the explicit consent of both the seller and the buyer.
        escrow.seller.require_auth();
        buyer.require_auth();

        // Only a funded, unshipped escrow can be mutually cancelled. Once it has
        // shipped or entered a dispute, the dispute/resolution flow governs the
        // outcome instead.
        if escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        // Return the locked funds to the buyer in full — no fee is taken on a
        // cancellation.
        token::Client::new(&env, &escrow.token).transfer(
            &env.current_contract_address(),
            &buyer,
            &escrow.amount,
        );

        escrow.state = EscrowState::Canceled;
        save_escrow(&env, escrow_id, &escrow);

        emit_escrow_cancelled(&env, escrow_id, escrow.seller.clone());
        Ok(())
    }

    /// Seller marks an escrow as shipped. Transitions Funded → Shipped.
    pub fn mark_shipped(
        env: Env,
        caller: Address,
        escrow_id: u64,
        tracking_id: String,
    ) -> Result<(), ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
        caller.require_auth();

        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.seller != caller {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        if tracking_id.is_empty() {
            return Err(ContractError::InvalidTrackingId);
        }
        if tracking_id.len() > MAX_TRACKING_ID_LEN {
            return Err(ContractError::InputTooLong);
        }

        let shipped_at = env.ledger().timestamp();
        escrow.state = EscrowState::Shipped;
        escrow.shipped_at = shipped_at;
        escrow.tracking_id = Some(tracking_id);
        let tracking = escrow
            .tracking_id
            .clone()
            .unwrap_or(String::from_str(&env, ""));
        save_escrow(&env, escrow_id, &escrow);
        emit_escrow_shipped(&env, escrow_id, escrow.seller, tracking);
        Ok(())
    }

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



        let delivered_at = env.ledger().timestamp();
        escrow.delivered_at = Some(delivered_at);
        save_escrow(&env, escrow_id, &escrow);

        emit_delivery_recorded(&env, escrow_id, delivered_at);
        Ok(())
    }

    pub fn confirm_delivery(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        // Authenticate before reading escrow state or performing any transfers.
        // This guarantees the buyer authorization check applies even if future
        // state branches are added here.
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

        transfer_with_protocol_fee(
            &env,
            &escrow.token,
            &escrow.seller,
            &fee_collector,
            escrow.amount,
            escrow.fee_bps,
        )?;

        escrow.state = EscrowState::Completed;
        save_escrow(&env, escrow_id, &escrow);
        increment_counter(&env, &DataKey::TotalCompleted)?;
        emit_escrow_completed(
            &env,
            escrow_id,
            escrow.seller.clone(),
            escrow.amount,
            escrow.fee_bps,
        );
        Ok(())
    }

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

        if escrow.state != EscrowState::Shipped && escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        if env.ledger().timestamp() >= escrow.dispute_deadline {
            return Err(ContractError::DisputeWindowClosed);
        }

        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(ContractError::InputTooLong);
        }

        escrow.state = EscrowState::Disputed;

        let dispute_data = DisputeData {
            escrow_id,
            reason,
            description,
            evidence_hash,
            status: DisputeStatus::Active,
            disputed_at: env.ledger().timestamp(),
            tracking_id: escrow.tracking_id.clone(),
        };

        save_escrow(&env, escrow_id, &escrow);
        save_dispute(&env, escrow_id, &dispute_data);
        increment_counter(&env, &DataKey::TotalDisputed)?;
        emit_dispute_raised(
            &env,
            escrow_id,
            buyer,
            dispute_data.reason.clone(),
            dispute_data.description.clone(),
            dispute_data.evidence_hash.clone(),
        );
        Ok(())
    }

    pub fn resolve_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        resolution: ResolutionType,
    ) -> Result<(), ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
        caller.require_auth();

        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;
        let admin = require_admin(&env)?;

        if caller != escrow.resolver && caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Disputed {
            return Err(ContractError::InvalidState);
        }

        let arbitration_fee_bps = read_fee_config(&env).arbitration_fee_bps;
        let arbitration_fee =
            crate::helpers::payout::calculate_fee(escrow.amount, arbitration_fee_bps)?;

        if arbitration_fee > escrow.amount {
            return Err(ContractError::InsufficientBalance);
        }

        escrow.amount = escrow
            .amount
            .checked_sub(arbitration_fee)
            .ok_or(ContractError::ArithmeticError)?;

        let total_key = DataKey::TotalArbitrationFees(escrow.token.clone());
        let current_total: i128 = env.storage().instance().get(&total_key).unwrap_or(0);
        let next_total = current_total
            .checked_add(arbitration_fee)
            .ok_or(ContractError::ArithmeticError)?;
        env.storage().instance().set(&total_key, &next_total);

        let now = env.ledger().timestamp();
        let appeal_deadline = now.checked_add(APPEAL_WINDOW).ok_or(ContractError::ArithmeticError)?;

        escrow.state = EscrowState::PendingFinalization;
        let recipient = match resolution {
            ResolutionType::Release => escrow.seller.clone(),
            ResolutionType::Refund => escrow
                .buyer
                .clone()
                .ok_or(ContractError::EscrowHasNoBuyer)?,
        };

        // Track the fees that will remain in the vault after deduct_and_transfer:
        // arbitration_fee (already deducted from escrow.amount above) plus the
        // per-escrow fee that deduct_and_transfer withholds from the payout.
        let escrow_fee = crate::helpers::payout::calculate_fee(escrow.amount, escrow.fee_bps)?;
        let fees_retained = arbitration_fee
            .checked_add(escrow_fee)
            .ok_or(ContractError::ArithmeticError)?;
        let acc_key = DataKey::AccumulatedFees(escrow.token.clone());
        let current_acc: i128 = env.storage().instance().get(&acc_key).unwrap_or(0);
        let new_acc = current_acc
            .checked_add(fees_retained)
            .ok_or(ContractError::ArithmeticError)?;
        env.storage().instance().set(&acc_key, &new_acc);

        deduct_and_transfer(
            &env,
            &escrow.token,
            &recipient,
            escrow.amount,
            escrow.fee_bps,
        )?;

        let mut updated = escrow;
        updated.state = match resolution {
            ResolutionType::Release => EscrowState::Completed,
            ResolutionType::Refund => EscrowState::Refunded,
        };

        let mut dispute_data = load_dispute(&env, escrow_id)?;
        dispute_data.status = DisputeStatus::Resolved;

        save_escrow(&env, escrow_id, &escrow);
        save_dispute(&env, escrow_id, &dispute_data);

        emit_dispute_pending_finalization(
            &env,
            escrow_id,
            escrow.resolver.clone(),
            resolution,
            escrow.amount,
            appeal_deadline,
        );
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

        // Path A: Admin-recorded delivery + delivery release window elapsed
        if let Some(delivered_at) = escrow.delivered_at {
            let eligible_at = delivered_at
                .checked_add(DELIVERY_RELEASE_WINDOW)
                .ok_or(ContractError::ArithmeticOverflow)?;
            if now < eligible_at {
                return Err(ContractError::ShippingWindowNotElapsed);
            }
        } else {
            // Path B: dispute deadline closed + shipping window elapsed from funding
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
                .ok_or(ContractError::ArithmeticOverflow)?;
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

        transfer_with_protocol_fee(
            &env,
            &escrow.token,
            &escrow.seller,
            &fee_collector,
            escrow.amount,
            fee_config.protocol_fee_bps,
        )?;

        escrow.state = EscrowState::Completed;
        save_escrow(&env, escrow_id, &escrow);
        increment_counter(&env, &DataKey::TotalCompleted)?;
        emit_auto_released(
            &env,
            escrow_id,
            escrow.seller,
            escrow.amount,
            escrow.fee_bps,
        );
        Ok(())
    }

    /// Finalizes a dispute resolution after the appeal window has passed.
    /// Transfers funds to the designated recipient.
    pub fn finalize_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        resolution: ResolutionType,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;

        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::PendingFinalization {
            return Err(ContractError::NotPendingFinalization);
        }

        let dispute_data = load_dispute(&env, escrow_id)?;
        let now = env.ledger().timestamp();

        // Appeal window must have passed
        let appeal_deadline = dispute_data.disputed_at.checked_add(APPEAL_WINDOW)
            .ok_or(ContractError::ArithmeticError)?;
        if now < appeal_deadline {
            return Err(ContractError::AppealWindowActive);
        }

        let recipient = match resolution {
            ResolutionType::Release => escrow.seller.clone(),
            ResolutionType::Refund => escrow.buyer.clone().ok_or(ContractError::EscrowHasNoBuyer)?,
        };

        let fee_config = read_fee_config(&env);
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;

        // Calculate platform fee
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

        let seller_amount = escrow.amount.checked_sub(platform_fee).ok_or(ContractError::ArithmeticError)?;

        // Transfer platform fee to treasury if applicable
        if platform_fee > 0 {
            if let Some(ref treasury_addr) = treasury {
                let token_client = token::Client::new(&env, &escrow.token);
                token_client.transfer(&env.current_contract_address(), treasury_addr, &platform_fee);
            }
        }

        // Transfer seller amount with protocol fee deduction
        transfer_with_protocol_fee(
            &env,
            &escrow.token,
            &recipient,
            &fee_collector,
            seller_amount,
            fee_config.protocol_fee_bps,
        )?;

        escrow.state = match resolution {
            ResolutionType::Release => EscrowState::Completed,
            ResolutionType::Refund => EscrowState::Refunded,
        };

        save_escrow(&env, escrow_id, &escrow);

        match resolution {
            ResolutionType::Release => increment_counter(&env, &DataKey::TotalCompleted)?,
            ResolutionType::Refund => increment_counter(&env, &DataKey::TotalRefunded)?,
        };

        emit_dispute_resolved(
            &env,
            escrow_id,
            escrow.resolver.clone(),
            resolution,
            recipient,
            escrow.amount,
            0, // arbitration fee already deducted
        );
        Ok(())
    }

    /// Appeals a dispute resolution within the appeal window.
    /// Re-opens the dispute for re-evaluation.
    pub fn appeal_dispute(
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

        let dispute_data = load_dispute(&env, escrow_id)?;
        let now = env.ledger().timestamp();

        // Appeal window must still be active
        let appeal_deadline = dispute_data.disputed_at.checked_add(APPEAL_WINDOW)
            .ok_or(ContractError::ArithmeticError)?;
        if now >= appeal_deadline {
            return Err(ContractError::DisputeWindowClosed);
        }

        // Only buyer or seller can appeal
        let buyer = escrow.buyer.clone().ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer && caller != escrow.seller {
            return Err(ContractError::NotAuthorized);
        }

        escrow.state = EscrowState::Disputed;

        let mut updated_dispute = dispute_data;
        updated_dispute.status = DisputeStatus::Active;

        save_escrow(&env, escrow_id, &escrow);
        save_dispute(&env, escrow_id, &updated_dispute);

        emit_dispute_appealed(&env, escrow_id, caller);
        Ok(())
    }

    /// Token Allowlist Management - Admin only

    /// Toggles the token allowlist on or off.
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

        env.storage().instance().set(&DataKey::TokenAllowlistEnabled, &enabled);
        emit_allowlist_toggled(&env, enabled);
        Ok(())
    }

    /// Adds a token to the allowlist.
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

        // Check if token already exists
        for allowed_token in allowlist.iter() {
            if allowed_token == token {
                return Ok(()); // Already exists, no-op
            }
        }

        allowlist.push_back(token.clone());
        env.storage().instance().set(&DataKey::TokenAllowlist, &allowlist);

        emit_token_allowlist_updated(&env, token, true);
        Ok(())
    }

    /// Removes a token from the allowlist.
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

        env.storage().instance().set(&DataKey::TokenAllowlist, &new_allowlist);

        emit_token_allowlist_updated(&env, token, false);
        Ok(())
    }

    /// Returns whether the token allowlist is enabled.
    pub fn is_token_allowlist_enabled(env: Env) -> bool {
        is_token_allowlist_enabled(&env)
    }

    /// Returns the list of allowed tokens.
    pub fn get_allowed_tokens(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    /// Platform Fee Management - Admin only

    /// Sets the platform fee in basis points.
    pub fn set_platform_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
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

    /// Sets the treasury address for platform fee collection.
    pub fn set_treasury(
        env: Env,
        caller: Address,
        treasury: Address,
    ) -> Result<(), ContractError> {
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

    /// Returns the platform fee in basis points.
    pub fn get_platform_fee_bps(env: Env) -> u32 {
        read_platform_fee_bps(&env)
    }

    /// Returns the treasury address.
    pub fn get_treasury(env: Env) -> Result<Address, ContractError> {
        read_treasury(&env)
    }

    /// Multi-Token Escrow (Basket Escrow) - Issue #383

    /// Creates an escrow with multiple tokens (basket escrow).
    /// Failure in token N reverts tokens 0..N-1.
    #[allow(clippy::too_many_arguments)]
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

        if tokens.len() != amounts.len() {
            return Err(ContractError::InvalidAmount);
        }

        if tokens.is_empty() {
            return Err(ContractError::InvalidAmount);
        }

        validate_escrow_fee_bps(fee_bps)?;

        // Security: all three roles must be distinct
        if resolver == seller {
            return Err(ContractError::ConflictingRoles);
        }
        if let Some(ref b) = buyer {
            if b == &seller || b == &resolver {
                return Err(ContractError::ConflictingRoles);
            }
        }

        // Token allowlist check for all tokens
        for token in tokens.iter() {
            is_token_allowed(&env, &token)?;
        }

        let escrow_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .expect("counter initialized");
        let next_id = escrow_id.checked_add(1).ok_or(ContractError::ArithmeticError)?;
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &next_id);

        let ext = get_ttl_extension(&env);
        env.storage().instance().extend_ttl(ext / 2, ext);

        // Calculate total amount for the first token (primary escrow)
        let primary_amount = amounts.get(0).ok_or(ContractError::InvalidAmount)?;
        let primary_token = tokens.get(0).ok_or(ContractError::InvalidAmount)?;

        let escrow = EscrowData {
            seller: seller.clone(),
            buyer: buyer.clone(),
            resolver: resolver.clone(),
            token: primary_token,
            amount: primary_amount,
            fee_bps,
            shipping_window,
            funded_at: 0,
            dispute_deadline: 0,
            state: EscrowState::Pending,
            shipped_at: 0,
            delivered_at: None,
            tracking_id: None,
        };

        save_escrow(&env, escrow_id, &escrow);

        let mut vendor_escrows = storage::read_vendor_escrow_index(&env, &seller);
        vendor_escrows.push_back(escrow_id);
        storage::write_vendor_escrow_index(&env, &seller, &vendor_escrows);

        increment_counter(&env, &DataKey::TotalCreated)?;
        emit_basket_escrow_created(&env, escrow_id, seller, tokens.len() as u32);

        Ok(escrow_id)
    }

    pub fn get_escrow(env: Env, escrow_id: u64) -> Result<EscrowData, ContractError> {
        load_escrow(&env, escrow_id)
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

    pub fn get_escrows_by_vendor(env: Env, vendor: Address) -> Vec<u64> {
        storage::read_vendor_escrow_index(&env, &vendor)
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

    /// Returns full contract configuration including privileged addresses. Requires admin auth.
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

    pub fn get_fee_config(env: Env) -> FeeConfig {
        read_fee_config(&env)
    }

    /// Rotates the resolver for an escrow. Callable by the seller or admin.
    /// New resolver must differ from current resolver, seller, and buyer.
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

        if caller != escrow.seller && caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        // Reject terminal states
        let is_terminal = matches!(
            escrow.state,
            EscrowState::Completed | EscrowState::Refunded | EscrowState::Canceled
        );
        if is_terminal {
            return Err(ContractError::InvalidState);
        }

        if new_resolver == escrow.resolver {
            return Err(ContractError::SameAddress);
        }

        if new_resolver == escrow.seller {
            return Err(ContractError::InvalidAddress);
        }

        if escrow.buyer.as_ref() == Some(&new_resolver) {
            return Err(ContractError::InvalidAddress);
        }

        let old_resolver = escrow.resolver.clone();
        escrow.resolver = new_resolver.clone();
        save_escrow(&env, escrow_id, &escrow);

        emit_resolver_rotated(&env, escrow_id, old_resolver, new_resolver);
        Ok(())
    }
}

mod test;
mod test_admin;
mod test_admin_rotation;
mod test_arbitration_fee;
mod test_auth_ordering;
mod test_auto_release;
mod test_cancel_restrictions;
mod test_concurrent_vendor_escrows;
mod test_contract_config;
mod test_delivery;
mod test_dispute;
mod test_dispute_flow;
mod test_dispute_window;
mod test_edge_cases;
mod test_escrow_id;
mod test_escrow_states;
mod test_fee_calculation_accuracy;
mod test_fee_config;
mod test_fee_minimum;
mod test_get_escrows_by_buyer;
mod test_get_escrows_by_vendor;
mod test_helpers;
mod test_initialize_twice;
mod test_initialize_zero_admin;
mod test_minimum_amount_guard;
mod test_not_found;
mod test_overflow;
mod test_pause;
mod test_resolution;
mod test_resolver_rotation;
mod test_mutual_cancel;
mod test_set_fee_boundary;
mod test_string_length;
mod test_ttl;
mod test_unauthorized;
mod test_withdraw_fees;
