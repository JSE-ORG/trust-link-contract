//! Admin-controlled contract configuration: pausing, fee parameters,
//! upgrades/migration, token allowlist, platform treasury, and the
//! approved-resolver registry.

use crate::internal::*;
use crate::*;
use soroban_sdk::{contractimpl, token, Address, BytesN, Env, String, Symbol};

#[contractimpl]
impl Escrow {
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

        let zero = Address::from_string(&String::from_str(&env, crate::ZERO_ADDRESS_STR));
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
    ///
    /// **Deprecated:** Use `set_protocol_fee` instead. This function now
    /// delegates to `set_protocol_fee` and emits `ProtocolFeeUpdated`.
    #[deprecated(note = "use set_protocol_fee instead")]
    pub fn set_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        Self::set_protocol_fee(env, caller, fee_bps)
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

        let zero = Address::from_string(&String::from_str(&env, crate::ZERO_ADDRESS_STR));
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
        env.events()
            .publish(("FeeCollectorUpdated",), (old_collector, new_collector));
        Ok(())
    }

    /// Sets the arbitration fee (in basis points) deducted from escrows
    /// during dispute resolution. Only callable by admin. Reverts with
    /// `FeeExceedsMax` if `fee_bps` exceeds `MAX_ARBITRATION_FEE_BPS`, or
    /// with the combined-fee cap if `protocol_fee_bps + fee_bps` would
    /// exceed `MAX_COMBINED_FEE_BPS`. Emits `arbitration_fee_updated`.
    pub fn set_arbitration_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        let old_fee_bps = update_arbitration_fee(&env, &caller, fee_bps)?;
        emit_arbitration_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    /// Returns the current arbitration fee in basis points.
    pub fn get_arbitration_fee(env: Env) -> u32 {
        read_fee_config(&env).arbitration_fee_bps
    }

    /// Returns the total arbitration fees accumulated for a token.
    pub fn get_total_arbitration_fees(env: Env, token: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalArbitrationFees(token))
            .unwrap_or(0)
    }

    /// Enables or disables the token allowlist. Only callable by admin.
    /// While enabled, `create_escrow` and related entry points reject any
    /// `token` not present in `get_allowed_tokens`. Emits
    /// `allowlist_toggled`.
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

    /// Adds `token` to the allowlist. Only callable by admin. A no-op
    /// (returns `Ok`) if the token is already present. Emits
    /// `token_allowlist_updated` with `allowed = true`.
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

        let mut allowlist: soroban_sdk::Map<Address, bool> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Map::new(&env));

        if allowlist.contains_key(token.clone()) {
            return Ok(());
        }

        allowlist.set(token.clone(), true);
        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlist, &allowlist);

        emit_token_allowlist_updated(&env, token, true);
        Ok(())
    }

    /// Removes `token` from the allowlist. Only callable by admin. Reverts
    /// with `TokenNotAllowed` if the token is not currently allowlisted.
    /// Emits `token_allowlist_updated` with `allowed = false`.
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

        let mut allowlist: soroban_sdk::Map<Address, bool> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Map::new(&env));

        if !allowlist.contains_key(token.clone()) {
            return Err(ContractError::TokenNotAllowed);
        }

        allowlist.remove(token.clone());

        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlist, &allowlist);

        emit_token_allowlist_updated(&env, token, false);
        Ok(())
    }

    /// Returns whether the token allowlist is currently enforced.
    pub fn is_token_allowlist_enabled(env: Env) -> bool {
        is_token_allowlist_enabled(&env)
    }

    /// Returns the full list of allowlisted tokens.
    pub fn get_allowed_tokens(env: Env) -> soroban_sdk::Vec<Address> {
        let allowlist: soroban_sdk::Map<Address, bool> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Map::new(&env));
        allowlist.keys()
    }

    /// Sets the platform fee (in basis points) charged on escrow settlements.
    /// Only callable by admin. Reverts with `PlatformFeeExceedsMax` if
    /// `fee_bps` exceeds `MAX_PLATFORM_FEE_BPS`. Emits `platform_fee_updated`.
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

    /// Sets the address that collects platform fees. Only callable by admin.
    /// Reverts with `InvalidAddress` if `treasury` is the zero address.
    /// Emits `treasury_updated`.
    pub fn set_treasury(env: Env, caller: Address, treasury: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let zero = Address::from_string(&String::from_str(&env, crate::ZERO_ADDRESS_STR));
        if treasury == zero {
            return Err(ContractError::InvalidAddress);
        }

        let old_treasury = read_treasury(&env).unwrap_or_else(|_| zero.clone());
        write_treasury(&env, &treasury);

        emit_treasury_updated(&env, old_treasury, treasury);
        Ok(())
    }

    /// Returns the current platform fee in basis points.
    pub fn get_platform_fee_bps(env: Env) -> u32 {
        read_platform_fee_bps(&env)
    }

    /// Returns the configured treasury address. Reverts with
    /// `NotInitialized` if no treasury has been set via `set_treasury`.
    pub fn get_treasury(env: Env) -> Result<Address, ContractError> {
        read_treasury(&env)
    }

    /// Sets the global minimum and maximum escrow amount. Only callable by
    /// admin. Reverts with `InvalidAmount` if `min_amount <= 0` or
    /// `max_amount < min_amount`.
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

        crate::events::emit_emergency_drain(&env, escrow_id, escrow.token.clone(), escrow.amount);
        Ok(())
    }
}
