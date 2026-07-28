//! Admin-controlled contract configuration: pausing, fee parameters,
//! upgrades/migration, token allowlist, platform treasury, and the
//! approved-resolver registry.
//!
//! Critical admin operations no longer execute immediately. A 24-hour
//! timelock (`ADMIN_TIMELOCK_DELAY_SECONDS`) applies to every mutating
//! privileged function via a two-step `queue_OP -> execute_timelocked_OP`
//! workflow.

use crate::internal::*;
use crate::storage;
use crate::{
    emit_timelock_cancelled, emit_timelock_executed, emit_timelock_queued, ContractError, Escrow,
    EscrowState, TimelockOperation, TimelockProposal, *,
};
use soroban_sdk::{contractimpl, token, Address, BytesN, Env, String, Symbol, Vec, Val, IntoVal};

pub const ADMIN_TIMELOCK_DELAY_SECONDS: u64 = 24 * 60 * 60;

fn queue_timelock_op(
    env: &Env,
    caller: &Address,
    operation: TimelockOperation,
    params: Vec<Val>,
) -> Result<(), ContractError> {
    caller.require_auth();
    let admin = require_admin_caller(env, caller)?;
    
    let now = env.ledger().timestamp();
    let ready_at = now + ADMIN_TIMELOCK_DELAY_SECONDS;
    
    let proposal = TimelockProposal {
        operation: operation as u32,
        proposer: caller.clone(),
        params,
        queued_at: now,
        ready_at,
    };
    
    storage::write_timelock_proposal(env, operation as u32, &proposal);
    emit_timelock_queued(env, operation as u32, caller.clone(), now, ready_at);
    Ok(())
}

fn execute_timelock_op(
    env: &Env,
    caller: &Address,
    operation: TimelockOperation,
) -> Result<TimelockProposal, ContractError> {
    let proposal = storage::read_timelock_proposal(env, operation as u32)
        .ok_or(ContractError::InvalidState)?;
        
    let now = env.ledger().timestamp();
    if now < proposal.ready_at {
        return Err(ContractError::InvalidState); // Not ready yet
    }
    
    storage::remove_timelock_proposal(env, operation as u32);
    emit_timelock_executed(env, operation as u32, proposal.proposer.clone(), caller.clone());
    Ok(proposal)
}

#[contractimpl]
impl Escrow {
    pub fn get_version(_env: Env) -> u32 {
        CONTRACT_VERSION
    }

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

        let zero = Address::from_string(&String::from_str(&env, crate::ZERO_ADDRESS_STR));
        if admin == zero || fee_collector == zero {
            return Err(ContractError::InvalidAddress);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &fee_collector);
        storage::write_fee_config(
            &env,
            &FeeConfig {
                protocol_fee_bps: 0,
                arbitration_fee_bps,
            },
        );
        env.storage().instance().set(&DataKey::EscrowCounter, &1u64);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);

        emit_contract_initialized(&env, admin, fee_collector, arbitration_fee_bps);
        Ok(())
    }

    pub fn get_storage_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::StorageVersion)
            .unwrap_or(0)
    }

    pub fn migrate(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin_caller(&env, &caller)?;

        let from = Self::get_storage_version(env.clone());
        if from >= STORAGE_VERSION {
            return Err(ContractError::AlreadyInitialized);
        }

        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);
        storage::extend_instance_ttl(&env);

        emit_storage_migrated(&env, admin, from, STORAGE_VERSION);
        Ok(())
    }
    
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }
    
    pub fn is_action_paused(env: Env, action: Symbol) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ActionPaused(action))
            .unwrap_or(false)
    }
    
    pub fn get_arbitration_fee(env: Env) -> u32 {
        storage::read_fee_config(&env).unwrap_or(FeeConfig { protocol_fee_bps: 0, arbitration_fee_bps: 0 }).arbitration_fee_bps
    }

    pub fn get_total_arbitration_fees(env: Env, token: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalArbitrationFees(token))
            .unwrap_or(0)
    }
    
    pub fn is_token_allowlist_enabled(env: Env) -> bool {
        crate::internal::is_token_allowlist_enabled(&env)
    }

    pub fn get_allowed_tokens(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }
    
    pub fn get_platform_fee_bps(env: Env) -> u32 {
        read_platform_fee_bps(&env)
    }

    pub fn get_treasury(env: Env) -> Result<Address, ContractError> {
        read_treasury(&env)
    }
    
    pub fn get_approved_resolvers(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env))
    }

    pub fn is_resolver_strict(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ResolverStrict)
            .unwrap_or(false)
    }
    
    pub fn cancel_timelock_op(env: Env, caller: Address, operation: u32) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin_caller(&env, &caller)?;
        
        let proposal = storage::read_timelock_proposal(&env, operation)
            .ok_or(ContractError::InvalidState)?;
            
        storage::remove_timelock_proposal(&env, operation);
        emit_timelock_cancelled(&env, operation, proposal.proposer, caller);
        Ok(())
    }

    // 1. SetAdmin
    pub fn queue_set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(new_admin.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetAdmin, params)
    }
    
    pub fn execute_set_admin(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetAdmin)?;
        let new_admin = Address::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let old_admin = require_admin(&env)?;
        if new_admin == old_admin {
            return Err(ContractError::SameAddress);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        emit_admin_rotated(&env, old_admin, new_admin);
        Ok(())
    }

    // 2. Upgrade
    pub fn queue_upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(new_wasm_hash.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::Upgrade, params)
    }
    
    pub fn execute_upgrade(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::Upgrade)?;
        let new_wasm_hash = BytesN::<32>::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let admin = require_admin(&env)?;
        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());
        emit_contract_upgraded(&env, admin, new_wasm_hash);
        Ok(())
    }

    // 3. SetProtocolFee
    pub fn queue_set_protocol_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetProtocolFee, params)
    }
    
    pub fn execute_set_protocol_fee(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetProtocolFee)?;
        let fee_bps = u32::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let old_fee_bps = update_protocol_fee(&env, &caller, fee_bps)?;
        emit_protocol_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    // 4. SetArbitrationFee
    pub fn queue_set_arbitration_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetArbitrationFee, params)
    }
    
    pub fn execute_set_arbitration_fee(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetArbitrationFee)?;
        let fee_bps = u32::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let old_fee_bps = update_arbitration_fee(&env, &caller, fee_bps)?;
        emit_arbitration_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    // 5. SetPlatformFee
    pub fn queue_set_platform_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetPlatformFee, params)
    }
    
    pub fn execute_set_platform_fee(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetPlatformFee)?;
        let fee_bps = u32::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        if fee_bps > MAX_PLATFORM_FEE_BPS {
            return Err(ContractError::PlatformFeeExceedsMax);
        }
        let old_fee = read_platform_fee_bps(&env);
        write_platform_fee_bps(&env, fee_bps);
        emit_platform_fee_updated(&env, old_fee, fee_bps);
        Ok(())
    }

    // 6. SetTreasury
    pub fn queue_set_treasury(env: Env, caller: Address, treasury: Address) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(treasury.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetTreasury, params)
    }
    
    pub fn execute_set_treasury(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetTreasury)?;
        let treasury = Address::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let zero = Address::from_string(&String::from_str(&env, crate::ZERO_ADDRESS_STR));
        if treasury == zero {
            return Err(ContractError::InvalidAddress);
        }
        let old_treasury = read_treasury(&env).unwrap_or_else(|_| zero.clone());
        write_treasury(&env, &treasury);
        emit_treasury_updated(&env, old_treasury, treasury);
        Ok(())
    }

    // 7. SetFeeCollector
    pub fn queue_set_fee_collector(env: Env, caller: Address, new_collector: Address) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(new_collector.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetFeeCollector, params)
    }
    
    pub fn execute_set_fee_collector(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetFeeCollector)?;
        let new_collector = Address::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let zero = Address::from_string(&String::from_str(&env, crate::ZERO_ADDRESS_STR));
        if new_collector == zero {
            return Err(ContractError::InvalidAddress);
        }
        let old_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;
        env.storage().instance().set(&DataKey::FeeCollector, &new_collector);
        emit_fee_collector_updated(&env, old_collector, new_collector);
        Ok(())
    }

    // 8. SetTtlExtension
    pub fn queue_set_ttl_extension(env: Env, caller: Address, ledgers: u32) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(ledgers.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetTtlExtension, params)
    }
    
    pub fn execute_set_ttl_extension(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetTtlExtension)?;
        let ledgers = u32::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let old_ledgers = storage::get_ttl_extension(&env);
        env.storage().instance().set(&DataKey::TtlExtensionLedgers, &ledgers);
        emit_ttl_extension_updated(&env, old_ledgers, ledgers, caller);
        Ok(())
    }

    // 9. SetAmountLimits
    pub fn queue_set_amount_limits(env: Env, caller: Address, min_amount: i128, max_amount: i128) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(min_amount.into_val(&env));
        params.push_back(max_amount.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetAmountLimits, params)
    }
    
    pub fn execute_set_amount_limits(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetAmountLimits)?;
        let min_amount = i128::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        let max_amount = i128::try_from_val(&env, &proposal.params.get(1).unwrap()).unwrap();
        
        if min_amount <= 0 || max_amount < min_amount {
            return Err(ContractError::InvalidAmount);
        }
        let old_min_amount = env.storage().instance().get(&DataKey::MinAmount).unwrap_or(MIN_ESCROW_AMOUNT);
        let old_max_amount = env.storage().instance().get(&DataKey::MaxAmount).unwrap_or(MAX_ESCROW_AMOUNT);

        env.storage().instance().set(&DataKey::MinAmount, &min_amount);
        env.storage().instance().set(&DataKey::MaxAmount, &max_amount);
        emit_amount_limits_updated(&env, old_min_amount, min_amount, old_max_amount, max_amount, caller);
        Ok(())
    }

    // 10. AddApprovedResolver
    pub fn queue_add_approved_resolver(env: Env, caller: Address, resolver: Address) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(resolver.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::AddApprovedResolver, params)
    }
    
    pub fn execute_add_approved_resolver(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::AddApprovedResolver)?;
        let resolver = Address::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let mut approved: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::ApprovedResolvers).unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        if crate::internal::contains(&approved, &resolver) {
            return Ok(());
        }
        approved.push_back(resolver.clone());
        env.storage().instance().set(&DataKey::ApprovedResolvers, &approved);
        emit_resolver_approved(&env, resolver, caller);
        Ok(())
    }

    // 11. RemoveApprovedResolver
    pub fn queue_remove_approved_resolver(env: Env, caller: Address, resolver: Address) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(resolver.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::RemoveApprovedResolver, params)
    }
    
    pub fn execute_remove_approved_resolver(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::RemoveApprovedResolver)?;
        let resolver = Address::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let approved: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::ApprovedResolvers).unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&approved, &resolver) {
            return Err(ContractError::InvalidAddress);
        }
        let mut new_approved = soroban_sdk::Vec::new(&env);
        for existing in approved.iter() {
            if existing != resolver {
                new_approved.push_back(existing);
            }
        }
        env.storage().instance().set(&DataKey::ApprovedResolvers, &new_approved);
        emit_resolver_removed(&env, resolver, caller);
        Ok(())
    }

    // 12. SetResolverStrict
    pub fn queue_set_resolver_strict(env: Env, caller: Address, strict: bool) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(strict.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetResolverStrict, params)
    }
    
    pub fn execute_set_resolver_strict(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetResolverStrict)?;
        let strict = bool::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let old_strict = env.storage().instance().get(&DataKey::ResolverStrict).unwrap_or(false);
        env.storage().instance().set(&DataKey::ResolverStrict, &strict);
        emit_resolver_strict_updated(&env, old_strict, strict, caller);
        Ok(())
    }

    // 13. SetTokenAllowlistEnabled
    pub fn queue_set_token_allowlist_enabled(env: Env, caller: Address, enabled: bool) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(enabled.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetTokenAllowlistEnabled, params)
    }
    
    pub fn execute_set_token_allowlist_enabled(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetTokenAllowlistEnabled)?;
        let enabled = bool::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        env.storage().instance().set(&DataKey::TokenAllowlistEnabled, &enabled);
        emit_allowlist_toggled(&env, enabled);
        Ok(())
    }

    // 14. AddAllowedToken
    pub fn queue_add_allowed_token(env: Env, caller: Address, token: Address) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(token.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::AddAllowedToken, params)
    }
    
    pub fn execute_add_allowed_token(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::AddAllowedToken)?;
        let token = Address::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let mut allowlist: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::TokenAllowlist).unwrap_or(soroban_sdk::Vec::new(&env));
        if crate::internal::contains(&allowlist, &token) {
            return Ok(());
        }
        allowlist.push_back(token.clone());
        env.storage().instance().set(&DataKey::TokenAllowlist, &allowlist);
        emit_token_allowlist_updated(&env, token, true);
        Ok(())
    }

    // 15. RemoveAllowedToken
    pub fn queue_remove_allowed_token(env: Env, caller: Address, token: Address) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(token.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::RemoveAllowedToken, params)
    }
    
    pub fn execute_remove_allowed_token(env: Env, caller: Address) -> Result<(), ContractError> {
        let proposal = execute_timelock_op(&env, &caller, TimelockOperation::RemoveAllowedToken)?;
        let token = Address::try_from_val(&env, &proposal.params.get(0).unwrap()).unwrap();
        
        let allowlist: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::TokenAllowlist).unwrap_or(soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&allowlist, &token) {
            return Err(ContractError::TokenNotAllowed);
        }
        let mut new_allowlist = soroban_sdk::Vec::new(&env);
        for allowed_token in allowlist.iter() {
            if allowed_token != token {
                new_allowlist.push_back(allowed_token);
            }
        }
        env.storage().instance().set(&DataKey::TokenAllowlist, &new_allowlist);
        emit_token_allowlist_updated(&env, token, false);
        Ok(())
    }

    // 16. PauseContract
    pub fn queue_pause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        let params = Vec::new(&env);
        queue_timelock_op(&env, &caller, TimelockOperation::PauseContract, params)
    }
    
    pub fn execute_pause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        execute_timelock_op(&env, &caller, TimelockOperation::PauseContract)?;
        
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        emit_contract_paused(&env, admin);
        Ok(())
    }

    // 17. UnpauseContract
    pub fn queue_unpause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        let params = Vec::new(&env);
        queue_timelock_op(&env, &caller, TimelockOperation::UnpauseContract, params)
    }
    
    pub fn execute_unpause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        execute_timelock_op(&env, &caller, TimelockOperation::UnpauseContract)?;
        
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        emit_contract_unpaused(&env, admin);
        Ok(())
    }
    
    /// Emergency drain: transfers all escrowed funds back to the buyer.
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

        env.events()
            .publish(("emergency_drain",), (escrow_id, buyer, seller));
        Ok(())
    }
}
