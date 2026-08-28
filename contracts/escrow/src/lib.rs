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
    emit_emergency_drain, emit_escrow_auto_canceled, emit_escrow_canceled, emit_escrow_completed,
    emit_escrow_created, emit_escrow_expired, emit_escrow_funded, emit_escrow_shipped,
    emit_fee_collector_updated, emit_fee_updated, emit_platform_fee_updated,
    emit_protocol_fee_updated, emit_refund_approved, emit_refund_requested, emit_resolver_approved,
    emit_resolver_removed, emit_resolver_rotated, emit_resolver_strict_updated,
    emit_resolver_vote_recorded, emit_storage_migrated, emit_timelock_cancelled,
    emit_timelock_executed, emit_timelock_queued, emit_token_allowlist_updated,
    emit_treasury_updated, emit_ttl_extension_updated, ActionPausedEvent, ActionUnpausedEvent,
    AdminRotated, AmountLimitsUpdated, ArbitrationFeeUpdated, AutoReleased, ContractInitialized,
    ContractPausedEvent, ContractUnpausedEvent, ContractUpgradedEvent, DeliveryProposalCancelled,
    DeliveryProposed, DeliveryRecorded, DisputeRaised, DisputeResolved, EscrowAutoCanceled,
    EscrowCanceled, EscrowCompleted, EscrowCreated, EscrowExpired, EscrowFunded, EscrowShipped,
    FeeUpdated, ProtocolFeeUpdated, ResolverApproved, ResolverRemoved, ResolverRotated,
    ResolverStrictUpdated, ResolverVoteRecorded, TimelockCancelled, TimelockExecuted,
    TimelockQueued, TtlExtensionUpdated,
};
pub use crate::types::{
    ContractConfig, ContractStats, DataKey, DisputeData, DisputeStatus, EscrowData, EscrowInput,
    EscrowState, ExpirySchedule, FeeConfig, Payee, PublicContractConfig, ResolutionType,
    ResolverSet, ResolverVote, TimelockOperation, TimelockProposal, TokenEntry,
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
pub const MIN_TTL_EXTENSION: u32 = 1_000;
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
pub const MAX_MESSAGES_PER_ESCROW: u32 = 100;

/// Minimum shipping window in seconds (1 second).
/// A value of 0 would allow an immediate dispute with no shipping time, which is invalid.
pub const MIN_SHIPPING_WINDOW: u64 = 1;

/// Maximum shipping window in seconds (approximately 2 years).
/// Prevents accidental or malicious use of u64::MAX which would lock funds indefinitely.
pub const MAX_SHIPPING_WINDOW: u64 = 63_072_000;

/// Default shipping window in seconds (3600 = 1 hour), used as a fallback by
/// the legacy 7-argument `create_escrow_7` entry point.
pub const DEFAULT_SHIPPING_WINDOW: u64 = 3600;

/// Maximum number of messages returned per page by `get_messages`, regardless
/// of the caller-supplied `limit`.
pub const MAX_MESSAGES_PER_PAGE: u64 = 50;

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

/// Construct the zero address once, avoiding repeated `String::from_str` calls.
pub(crate) fn zero_address(env: &Env) -> Address {
    Address::from_string(&soroban_sdk::String::from_str(env, ZERO_ADDRESS_STR))
}

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
    let next = current.checked_add(1).ok_or(ContractError::ArithmeticError)?;
    env.storage().instance().set(key, &next);
    Ok(())
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
        env.storage().instance().set(&DataKey::FeeCollector, &fee_collector);
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
        env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
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

        env.storage().instance().set(&DataKey::TtlExtensionLedgers, &ledgers);
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
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

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

        let new_accumulated = accumulated.checked_sub(amount).ok_or(ContractError::ArithmeticError)?;
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
        env.events().publish(
            ("FeeCollectorUpdated",),
            (old_collector, new_collector),
        );
        Ok(())
    }

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

        let escrow_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .ok_or(ContractError::IndexOutOfBounds)?;
        let next_id = escrow_id.checked_add(1).ok_or(ContractError::ArithmeticError)?;
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &next_id);
        // Extend instance storage TTL on every counter access so the counter key
        // cannot expire between a read and the subsequent write.
        let ext = get_ttl_extension(&env);
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
        };

        save_escrow(&env, escrow_id, &escrow);

        let mut vendor_escrows = storage::read_vendor_escrow_index(&env, &escrow.seller);
        vendor_escrows.push_back(escrow_id);
        // write_vendor_escrow_index now handles TTL extension automatically
        storage::write_vendor_escrow_index(&env, &escrow.seller, &vendor_escrows);

        increment_counter(&env, &DataKey::TotalCreated)?;
        emit_escrow_created(
            &env,
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

    pub fn fund_escrow(
        env: Env,
        escrow_id: u64,
        buyer: Address,
    ) -> Result<(), ContractError> {
        buyer.require_auth();
        ensure_not_paused(&env)?;

        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        escrow.buyer = Some(buyer.clone());
        escrow.state = EscrowState::Funded;
        escrow.funded_at = env.ledger().timestamp();
        escrow.dispute_deadline = escrow.funded_at + DISPUTE_WINDOW;

        let token_client = token::Client::new(&env, &escrow.token);
        let contract_address = env.current_contract_address();
        token_client.transfer(&buyer, &contract_address, &escrow.amount);

        save_escrow(&env, escrow_id, &escrow);

        let mut buyer_escrows: Vec<u64> = env.storage().persistent()
            .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
            .unwrap_or(Vec::new(&env));
        buyer_escrows.push_back(escrow_id);
        env.storage().persistent().set(&DataKey::BuyerEscrowIndex(buyer.clone()), &buyer_escrows);

        emit_escrow_funded(&env, escrow_id, buyer, escrow.amount);
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

        let buyer = escrow.buyer.clone().ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if env.ledger().timestamp() >= escrow.dispute_deadline {
            return Err(ContractError::DisputeWindowClosed);
        }

        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(ContractError::InputTooLong);
        }

        escrow.state = EscrowState::Disputed;
        let now = env.ledger().timestamp();

        let dispute = DisputeData {
            escrow_id,
            reason: reason.clone(),
            description: description.clone(),
            evidence_hash: evidence_hash.clone(),
            status: DisputeStatus::Active,
            disputed_at: now,
            tracking_id: escrow.tracking_id.clone(),
        };

        save_escrow(&env, escrow_id, &escrow);
        save_dispute(&env, escrow_id, &dispute);
        increment_counter(&env, &DataKey::TotalDisputed)?;
        emit_dispute_raised(&env, escrow_id, caller, reason, description, evidence_hash);
        Ok(())
    }

    pub fn cancel_escrow(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();

        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let is_seller = caller == escrow.seller;
        let is_buyer = Some(&caller) == escrow.buyer.as_ref();

        if !is_seller && !is_buyer {
            return Err(ContractError::NotAuthorized);
        }

        if is_buyer && escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        if is_seller && !is_buyer && escrow.state != EscrowState::Pending && escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        if is_buyer {
            if let Some(buyer) = &escrow.buyer {
                token::Client::new(&env, &escrow.token)
                    .transfer(&env.current_contract_address(), buyer, &(escrow.amount as i128));
            }
            if escrow.fee_bps > 0 {
                escrow.state = EscrowState::Refunded;
            } else {
                escrow.state = EscrowState::Canceled;
            }
        } else {
            escrow.state = EscrowState::Canceled;
        }

        save_escrow(&env, escrow_id, &escrow);

        emit_escrow_cancelled(&env, escrow_id, caller);
        Ok(())
    }

    /// Seller marks an escrow as shipped. Transitions Funded → Shipped.
    pub fn mark_shipped(env: Env, caller: Address, escrow_id: u64, tracking_id: String) -> Result<(), ContractError> {
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
        let tracking = escrow.tracking_id.clone().unwrap_or(String::from_str(&env, ""));
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

        if escrow.delivered_at.is_some() {
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

        let buyer = escrow.buyer.clone().ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if env.ledger().timestamp() < escrow.dispute_deadline {
            return Err(ContractError::DeliveryBeforeDisputeWindow);
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
        emit_escrow_completed(
            &env,
            escrow_id,
            escrow.seller.clone(),
            escrow.amount,
            fee_config.protocol_fee_bps,
        );
        Ok(())
    }



    pub fn resolve_dispute(env: Env, caller: Address, escrow_id: u64, resolution: ResolutionType) -> Result<(), ContractError> {
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
        let arbitration_fee = crate::helpers::payout::calculate_fee(escrow.amount, arbitration_fee_bps)?;

        if arbitration_fee > escrow.amount {
            return Err(ContractError::InsufficientBalance);
        }

        escrow.amount = escrow
            .amount
            .checked_sub(arbitration_fee)
            .ok_or(ContractError::ArithmeticError)?;

        let total_key = DataKey::TotalArbitrationFees(escrow.token.clone());
        let current_total: i128 = env.storage().instance().get(&total_key).unwrap_or(0);
        let next_total = current_total.checked_add(arbitration_fee).ok_or(ContractError::ArithmeticError)?;
        env.storage().instance().set(&total_key, &next_total);

        let recipient = match resolution {
            ResolutionType::Release => escrow.seller.clone(),
            ResolutionType::Refund => escrow.buyer.clone().ok_or(ContractError::EscrowHasNoBuyer)?,
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

        deduct_and_transfer(&env, &escrow.token, &recipient, escrow.amount, escrow.fee_bps)?;

        let mut updated = escrow;
        updated.state = match resolution {
            ResolutionType::Release => EscrowState::Completed,
            ResolutionType::Refund => EscrowState::Refunded,
        };

        let mut dispute_data = load_dispute(&env, escrow_id)?;
        dispute_data.status = DisputeStatus::Resolved;

        save_escrow(&env, escrow_id, &updated);
        save_dispute(&env, escrow_id, &dispute_data);

        match resolution {
            ResolutionType::Release => increment_counter(&env, &DataKey::TotalCompleted)?,
            ResolutionType::Refund => increment_counter(&env, &DataKey::TotalRefunded)?,
        };

        emit_dispute_resolved(
            &env,
            escrow_id,
            updated.resolver.clone(),
            resolution,
            recipient,
            updated.amount,
            arbitration_fee,
        );
        Ok(())
    }

    pub fn set_arbitration_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        let old_fee_bps = update_arbitration_fee(&env, &caller, fee_bps)?;
        emit_arbitration_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    pub fn get_arbitration_fee(env: Env) -> u32 {
        read_fee_config(&env).arbitration_fee_bps
    }

    pub fn get_total_arbitration_fees(env: Env, token: Address) -> i128 {
        env.storage().instance().get(&DataKey::TotalArbitrationFees(token)).unwrap_or(0)
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
        emit_auto_released(&env, escrow_id, escrow.seller, escrow.amount, escrow.fee_bps);
        Ok(())
    }

    pub fn get_escrow(env: Env, escrow_id: u64) -> Result<EscrowData, ContractError> {
        load_escrow(&env, escrow_id)
    }

    pub fn get_dispute(env: Env, escrow_id: u64) -> Option<DisputeData> {
        load_dispute(&env, escrow_id).ok()
    }

    pub fn get_escrows_by_buyer(env: Env, buyer: Address) -> Vec<u64> {
        if let Some(ids) = env.storage().persistent().get(&DataKey::BuyerEscrowIndex(buyer.clone())) {
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

    if let Some(final_resolution) = tally_votes(&votes, threshold)? {
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
mod test_amount_limits;
mod test_appeal;
mod test_arbitration_fee;
mod test_auth_matrix;
mod test_auth_ordering;
mod test_auto_release;
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
mod test_expiration;
mod test_fallback_resolver;
mod test_fee_calculation_accuracy;
mod test_fee_config;
mod test_fee_minimum;
mod test_fee_snapshot;
mod test_fee_update;
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
mod test_multi_asset;
mod test_multi_resolver;
mod test_multicall;
mod test_mutual_cancel;
mod test_not_found;
mod test_overflow;
mod test_pause;
mod test_pending_expiry;
mod test_refund_override;
mod test_resolution;
mod test_resolver_registry;
mod test_resolver_rotation;
mod test_sep41;
mod test_set_fee_boundary;
mod test_set_fee_collector;
mod test_shipping_window;
mod test_state_history;
mod test_storage_collision;
mod test_string_length;
mod test_ttl;
mod test_unauthorized;
mod test_upgrade_migration;
mod test_vote;
