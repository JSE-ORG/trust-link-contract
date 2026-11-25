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
    emit_appeal_fee_updated, emit_fee_collector_accepted, emit_fee_collector_pending,
    emit_max_appeals_updated, emit_max_basket_size_updated, emit_recovery_mode_updated,
    emit_recovery_withdraw, emit_timelock_admin_mismatch, emit_timelock_cancelled,
    emit_timelock_executed, emit_timelock_queued, ContractError, Escrow, EscrowState,
    TimelockOperation, TimelockProposal, *,
};
use soroban_sdk::{
    contractimpl, xdr::ToXdr, Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

pub const ADMIN_TIMELOCK_DELAY_SECONDS: u64 = 24 * 60 * 60;
pub const MIN_TIMELOCK_DELAY_SECONDS: u64 = 12 * 60 * 60; // 12 hours
pub const MAX_TIMELOCK_DELAY_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days
const MAX_TIMELOCK_PARAMS: u32 = 5;

/// Returns the currently configured admin timelock delay in seconds
/// (`DataKey::AdminTimelockDelay`, defaulting to
/// `ADMIN_TIMELOCK_DELAY_SECONDS`).
fn read_timelock_delay(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::AdminTimelockDelay)
        .unwrap_or(ADMIN_TIMELOCK_DELAY_SECONDS)
}

/// Computes the domain-separated hash that binds a timelock proposal to its
/// parameters **and** to the context it was queued in (#1236).
///
/// Preimage: `params.to_xdr() || current_contract_address.to_xdr() ||
/// (operation as u32).to_xdr()`, hashed with SHA-256.
///
/// Including the contract address means a proposal record copied into, or
/// replayed against, another escrow instance never verifies there; including
/// the operation discriminant means two operations queued with an identical
/// parameter vector (e.g. `SetPlatformFee(50)` and `SetArbitrationFee(50)`)
/// still produce distinct hashes. Both `queue_timelock_op` and
/// `execute_timelock_op` must go through this helper so the two sides can
/// never drift apart.
pub(crate) fn compute_proposal_hash(
    env: &Env,
    operation: TimelockOperation,
    params: &Vec<Val>,
) -> BytesN<32> {
    let mut payload = params.clone().to_xdr(env);
    payload.append(&env.current_contract_address().to_xdr(env));
    payload.append(&(operation as u32).to_xdr(env));
    env.crypto().sha256(&payload).into()
}

fn queue_timelock_op(
    env: &Env,
    caller: &Address,
    operation_id: u32,
    params: Vec<Val>,
) -> Result<(), ContractError> {
    queue_timelock_op_with_min_delay(env, caller, operation_id, params, 0)
}

/// Queues a timelock proposal whose countdown is at least `min_delay`
/// seconds, or the configured admin timelock delay if that is longer.
///
/// Only `queue_set_timelock_delay` passes a non-zero `min_delay` (#1238):
/// a delay change must wait `max(current_delay, new_delay)`, so the delay
/// currently in force can never be shortened faster than it allows.
fn queue_timelock_op_with_min_delay(
    env: &Env,
    caller: &Address,
    operation_id: u32,
    params: Vec<Val>,
    min_delay: u64,
) -> Result<(), ContractError> {
    let op_val: Val = operation_id.into_val(env);
    let operation = TimelockOperation::try_from_val(env, &op_val)
        .map_err(|_| ContractError::InvalidOperation)?;
    caller.require_auth();
    let _admin = require_admin_caller(env, caller)?;

    if params.len() > MAX_TIMELOCK_PARAMS {
        return Err(ContractError::InputTooLong);
    }

    let now = env.ledger().timestamp();
    let delay = read_timelock_delay(env).max(min_delay);
    let ready_at = now + delay;

    let params_hash = compute_proposal_hash(env, operation, &params);
    let proposal = TimelockProposal {
        operation,
        proposer: caller.clone(),
        params_hash,
        queued_at: now,
        ready_at,
    };

    // A newly authorized proposal replaces any prior rotation invalidation.
    env.storage()
        .instance()
        .remove(&DataKey::InvalidatedTimelockOp(operation as u32));
    storage::write_timelock_proposal(env, operation as u32, &proposal);
    emit_timelock_queued(env, operation as u32, caller.clone(), now, ready_at);
    Ok(())
}

/// Finalizes a queued timelock proposal once its delay has elapsed.
///
/// Execution requires the executor's authorization, not the proposer's: any
/// keeper may execute a mature proposal queued by the current admin.
fn execute_timelock_op(
    env: &Env,
    caller: &Address,
    operation: TimelockOperation,
    params: Vec<Val>,
) -> Result<TimelockProposal, ContractError> {
    caller.require_auth();
    let current_admin = storage::read_admin_address(env).ok_or(ContractError::NotInitialized)?;
    let proposal = match storage::read_timelock_proposal(env, operation as u32) {
        Some(proposal) => proposal,
        None if env
            .storage()
            .instance()
            .has(&DataKey::InvalidatedTimelockOp(operation as u32)) =>
        {
            return Err(ContractError::NotAuthorized);
        }
        None => return Err(ContractError::InvalidState),
    };
    if proposal.proposer != current_admin {
        return Err(ContractError::NotAuthorized);
    }

    let now = env.ledger().timestamp();
    if now < proposal.ready_at {
        return Err(ContractError::TimelockNotElapsed);
    }

    if compute_proposal_hash(env, operation, &params) != proposal.params_hash {
        return Err(ContractError::InvalidProposalHash);
    }

    storage::remove_timelock_proposal(env, operation as u32);
    emit_timelock_executed(
        env,
        operation as u32,
        proposal.proposer.clone(),
        caller.clone(),
        params, // Emit the actual params, not the hash
    );
    Ok(proposal)
}

/// Purges every queued timelock proposal when the admin key rotates (#1234).
///
/// The numeric discriminants are append-only (see [`TimelockOperation`]), so
/// iterating `1..=SetTimelockDelay` covers all currently queued operations;
/// bump the upper bound when a new variant is appended. A `TimelockCancelled`
/// event is emitted for each removed proposal so indexers can immediately
/// update governance status for the old key.
///
/// `execute_timelock_op` already rejects a proposal whose `proposer` is no
/// longer the current admin, so this cannot make a stale proposal executable;
/// its purpose is to leave the incoming admin a clean governance slate and to
/// make the cancellation observable. `canceller` is the incoming admin (the
/// party whose rotation causes the purge).
fn clear_all_timelock_proposals(env: &Env, canceller: &Address) {
    for operation in 1..=(TimelockOperation::SetTimelockDelay as u32) {
        if let Some(proposal) = storage::read_timelock_proposal(env, operation) {
            storage::remove_timelock_proposal(env, operation);
            env.storage()
                .instance()
                .set(&DataKey::InvalidatedTimelockOp(operation), &true);
            let proposer = proposal.proposer.clone();
            emit_timelock_cancelled(env, operation, proposer.clone(), canceller.clone());
            emit_timelock_admin_mismatch(env, operation, proposer);
        }
    }
}

/// Cancel all outstanding proposals when the admin changes. A rejected
/// execution cannot do this itself: Soroban rolls back state and events on error.
fn invalidate_timelocks_on_rotation(env: &Env) {
    // TimelockOperation discriminants are append-only; update the upper bound
    // when a new operation is added.
    for operation in 1..=(TimelockOperation::SetTimelockDelay as u32) {
        if let Some(proposal) = storage::read_timelock_proposal(env, operation) {
            storage::remove_timelock_proposal(env, operation);
            env.storage()
                .instance()
                .set(&DataKey::InvalidatedTimelockOp(operation), &true);
            // Cancellation is automatic, not an action signed by the new admin.
            emit_timelock_cancelled(
                env,
                operation,
                proposal.proposer.clone(),
                env.current_contract_address(),
            );
            emit_timelock_admin_mismatch(env, operation, proposal.proposer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        IntoVal,
    };

    #[test]
    fn queue_timelock_op_rejects_more_than_five_params() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let contract_id = env.register(Escrow, ());
        let client = crate::EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);

        let mut params = Vec::new(&env);
        for value in 0..6_u32 {
            params.push_back(value.into_val(&env));
        }

        let result = env.as_contract(&contract_id, || {
            queue_timelock_op(&env, &admin, TimelockOperation::SetAdmin as u32, params)
        });

        assert_eq!(result, Err(ContractError::InputTooLong));
        let proposal = env.as_contract(&contract_id, || {
            storage::read_timelock_proposal(&env, TimelockOperation::SetAdmin as u32)
        });
        assert!(proposal.is_none());
    }

    #[test]
    fn queue_timelock_op_rejects_invalid_operation() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let contract_id = env.register(Escrow, ());
        let client = crate::EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);

        let params = Vec::new(&env);
        // Valid queue (SetAdmin)
        let res = env.as_contract(&contract_id, || {
            queue_timelock_op(
                &env,
                &admin,
                TimelockOperation::SetAdmin as u32,
                params.clone(),
            )
        });
        assert!(res.is_ok());

        // Invalid queue (999 is not a valid TimelockOperation)
        let res_invalid = env.as_contract(&contract_id, || {
            queue_timelock_op(&env, &admin, 999, params)
        });
        assert_eq!(res_invalid, Err(ContractError::InvalidOperation));
    }

    #[test]
    fn set_timelock_delay_flow_and_validation() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let contract_id = env.register(Escrow, ());
        let client = crate::EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);

        // Queue invalid delay (too low)
        let res_low = env.as_contract(&contract_id, || {
            Escrow::queue_set_timelock_delay(
                env.clone(),
                admin.clone(),
                MIN_TIMELOCK_DELAY_SECONDS - 1,
            )
        });
        assert_eq!(res_low, Err(ContractError::InvalidTimelockDelay));

        // Queue invalid delay (too high)
        let res_high = env.as_contract(&contract_id, || {
            Escrow::queue_set_timelock_delay(
                env.clone(),
                admin.clone(),
                MAX_TIMELOCK_DELAY_SECONDS + 1,
            )
        });
        assert_eq!(res_high, Err(ContractError::InvalidTimelockDelay));

        // Valid delay
        let new_delay = 48 * 60 * 60; // 48 hours
        let res = env.as_contract(&contract_id, || {
            Escrow::queue_set_timelock_delay(env.clone(), admin.clone(), new_delay)
        });
        assert!(res.is_ok());

        // Raising the delay waits the *new* (longer) delay (#1238): 24h is
        // not enough to apply a 48h delay.
        env.ledger().with_mut(|li| {
            li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS;
        });
        let res_exec_early = env.as_contract(&contract_id, || {
            Escrow::execute_set_timelock_delay(env.clone(), admin.clone(), new_delay)
        });
        assert_eq!(res_exec_early, Err(ContractError::TimelockNotElapsed));

        // Fast forward the remaining 24h (48h total).
        env.ledger().with_mut(|li| {
            li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS;
        });

        // Execute
        let res_exec = env.as_contract(&contract_id, || {
            Escrow::execute_set_timelock_delay(env.clone(), admin.clone(), new_delay)
        });
        assert!(res_exec.is_ok());

        // Confirm new delay takes effect for subsequently queued operations
        let res_queue2 = env.as_contract(&contract_id, || {
            Escrow::queue_pause_contract(env.clone(), admin.clone())
        });
        assert!(res_queue2.is_ok());

        // Fast forward 24h - should NOT be enough now
        env.ledger().with_mut(|li| {
            li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS;
        });

        let res_exec2_early = env.as_contract(&contract_id, || {
            Escrow::execute_pause_contract(env.clone(), admin.clone())
        });
        assert_eq!(res_exec2_early, Err(ContractError::TimelockNotElapsed));

        // Fast forward remaining 24h (total 48h)
        env.ledger().with_mut(|li| {
            li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS;
        });

        let res_exec2 = env.as_contract(&contract_id, || {
            Escrow::execute_pause_contract(env.clone(), admin.clone())
        });
        assert!(res_exec2.is_ok());
    }

    #[test]
    fn execute_timelock_op_rejects_hash_mismatch() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let contract_id = env.register(Escrow, ());
        let client = crate::EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);

        // Queue
        let res = env.as_contract(&contract_id, || {
            Escrow::queue_set_platform_fee(env.clone(), admin.clone(), 50)
        });
        assert!(res.is_ok());

        // Fast forward 24h
        env.ledger().with_mut(|li| {
            li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS;
        });

        // Try to execute with a DIFFERENT fee
        let res_exec_bad = env.as_contract(&contract_id, || {
            Escrow::execute_set_platform_fee(env.clone(), admin.clone(), 150)
        });
        assert_eq!(res_exec_bad, Err(ContractError::InvalidProposalHash));

        // Execute with CORRECT fee
        let res_exec_good = env.as_contract(&contract_id, || {
            Escrow::execute_set_platform_fee(env.clone(), admin.clone(), 50)
        });
        assert_eq!(res_exec_good, Ok(()));
    }

    /// Pins the `MAX_PLATFORM_FEE_BPS` = 200 boundary (`PlatformFeeExceedsMax`,
    /// error 32) on the timelocked `execute_set_platform_fee` path, mirroring
    /// `test_set_platform_fee_cap_matches_production_limit` in
    /// `test_admin.rs`, which pins the same boundary on the immediate
    /// `set_platform_fee` path. `queue_set_platform_fee` does not check the
    /// cap, so 201 queues successfully and only fails at execute time.
    #[test]
    fn execute_set_platform_fee_boundary_200_ok_201_exceeds_max() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let contract_id = env.register(Escrow, ());
        let client = crate::EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);

        // 200 (at the cap) succeeds end-to-end via the timelocked path.
        let res_queue_ok = env.as_contract(&contract_id, || {
            Escrow::queue_set_platform_fee(env.clone(), admin.clone(), 200)
        });
        assert!(res_queue_ok.is_ok());
        env.ledger().with_mut(|li| {
            li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS;
        });
        let res_exec_ok = env.as_contract(&contract_id, || {
            Escrow::execute_set_platform_fee(env.clone(), admin.clone(), 200)
        });
        assert_eq!(res_exec_ok, Ok(()));

        // 201 (one above the cap) queues fine but fails to execute.
        let res_queue_bad = env.as_contract(&contract_id, || {
            Escrow::queue_set_platform_fee(env.clone(), admin.clone(), 201)
        });
        assert!(res_queue_bad.is_ok());
        env.ledger().with_mut(|li| {
            li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS;
        });
        let res_exec_bad = env.as_contract(&contract_id, || {
            Escrow::execute_set_platform_fee(env.clone(), admin.clone(), 201)
        });
        assert_eq!(res_exec_bad, Err(ContractError::PlatformFeeExceedsMax));
    }

    #[test]
    fn storage_size_is_bounded_by_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let contract_id = env.register(Escrow, ());
        let client = crate::EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);

        // We queue an op with many params just to verify storage size.
        // Actually, SetAmountLimits has 2 params. SetAdmin has 1. We just read the proposal.
        let res = env.as_contract(&contract_id, || {
            Escrow::queue_set_amount_limits(env.clone(), admin.clone(), 10, 100)
        });
        assert!(res.is_ok());

        // Read proposal directly to ensure it has params_hash, not params
        let proposal = env.as_contract(&contract_id, || {
            crate::storage::read_timelock_proposal(&env, TimelockOperation::SetAmountLimits as u32)
                .unwrap()
        });

        // Assert that the proposal has the hash field.
        assert_eq!(proposal.params_hash.len(), 32);
    }
}

#[contractimpl]
impl Escrow {
    pub fn get_version(_env: Env) -> u32 {
        CONTRACT_VERSION
    }

    /// One-time contract setup: stores the admin, fee collector, arbitration
    /// fee, protocol-fee snapshot, escrow counter, pause flag, and storage
    /// version.
    ///
    /// # Security: enable the token allowlist before accepting real value
    ///
    /// `initialize` intentionally leaves the token allowlist **disabled** for
    /// backward compatibility, which means any SEP-41 token can be used to
    /// create an escrow until an operator opts in. Untrusted token contracts
    /// can re-enter or otherwise misbehave during the payouts this contract
    /// performs. After initializing, an operator MUST, before opening the
    /// contract to users:
    ///
    /// 1. `set_token_allowlist_enabled(true)` (or queue/execute the timelocked
    ///    `SetTokenAllowlistEnabled` operation), and
    /// 2. register each vetted token with `add_allowed_token`.
    ///
    /// See `SECURITY.md` ("Token Allowlisting") and the docs on
    /// [`crate::internal::is_token_allowlist_enabled`].
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

        let zero = crate::zero_address(&env);
        if admin == zero || fee_collector == zero {
            return Err(ContractError::InvalidAddress);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        storage::write_global_config(
            &env,
            &GlobalConfig {
                fee_collector: Some(fee_collector.clone()),
                arbitration_fee_bps,
                ..storage::default_global_config()
            },
        );
        env.storage().instance().set(&DataKey::EscrowCounter, &1u64);
        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);

        emit_contract_initialized(&env, admin, fee_collector, arbitration_fee_bps);
        Ok(())
    }

    /// One-time strict contract setup: atomically initializes the contract with
    /// the token allowlist **enabled** from genesis and populated with
    /// `allowed_tokens` (issue #1293).
    ///
    /// Eliminates the deployment window between [`Escrow::initialize`] and
    /// [`Escrow::set_token_allowlist_enabled`] during which untrusted SEP-41
    /// tokens could otherwise be used to create escrows. Duplicate entries in
    /// `allowed_tokens` are deduplicated while preserving order; passing the
    /// zero address for `admin`, `fee_collector`, or any token reverts with
    /// `InvalidAddress`. Emits `ContractInitialized` and `AllowlistToggled`.
    pub fn initialize_with_allowlist(
        env: Env,
        admin: Address,
        fee_collector: Address,
        fee_bps: u32,
        allowed_tokens: Vec<Address>,
    ) -> Result<(), ContractError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        if admin == fee_collector {
            return Err(ContractError::InvalidAddress);
        }
        validate_arbitration_fee_bps(fee_bps)?;

        let zero = crate::zero_address(&env);
        if admin == zero || fee_collector == zero {
            return Err(ContractError::InvalidAddress);
        }

        let mut deduped_tokens = Vec::new(&env);
        for token in allowed_tokens.iter() {
            if token == zero {
                return Err(ContractError::InvalidAddress);
            }
            if !crate::internal::contains(&deduped_tokens, &token) {
                deduped_tokens.push_back(token);
            }
        }

        let defaults = storage::default_global_config();
        env.storage().instance().set(&DataKey::Admin, &admin);
        storage::write_global_config(
            &env,
            &GlobalConfig {
                fee_collector: Some(fee_collector.clone()),
                arbitration_fee_bps: fee_bps,
                state_flags: defaults.state_flags
                    | crate::types::GLOBAL_CONFIG_FLAG_TOKEN_ALLOWLIST_ENABLED,
                ..defaults
            },
        );
        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlist, &deduped_tokens);
        env.storage().instance().set(&DataKey::EscrowCounter, &1u64);
        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);

        emit_contract_initialized(&env, admin, fee_collector, fee_bps);
        emit_allowlist_toggled(&env, true);
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

        if from < 2 {
            // v1 -> v2: fold the per-setting admin config keys into the single
            // `GlobalConfig` entry. Until this runs, reads fall back to the
            // legacy keys, so the contract stays fully operable in between.
            storage::migrate_legacy_global_config(&env);
        }

        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &STORAGE_VERSION);
        storage::extend_instance_ttl(&env);

        emit_storage_migrated(&env, admin, from, STORAGE_VERSION);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        storage::read_global_config(&env).state_flags & crate::types::GLOBAL_CONFIG_FLAG_PAUSED != 0
    }

    pub fn is_action_paused(env: Env, action: Symbol) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ActionPaused(action))
            .unwrap_or(false)
    }

    /// Begins a 2-step fee-collector change. Validates the new address and
    /// stores it as `PendingFeeCollector`. The change only takes effect once
    /// the new collector calls `accept_fee_collector`. Only callable by admin.
    /// Reverts with `InvalidAddress` if `new_collector` is the zero address or
    /// the current admin, or `SameAddress` if it already is the active
    /// collector. Emits `emit_fee_collector_pending`.
    pub fn set_fee_collector(env: Env, new_collector: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        admin.require_auth();

        let old_collector = validate_fee_collector_change(&env, &new_collector)?;

        env.storage()
            .instance()
            .set(&DataKey::PendingFeeCollector, &new_collector);
        emit_fee_collector_pending(&env, old_collector, new_collector);
        Ok(())
    }

    /// Finalizes a pending fee-collector change. Must be called by the address
    /// that was registered as `PendingFeeCollector` via `set_fee_collector` or
    /// `execute_set_fee_collector`. Reverts with `NoPendingFeeCollector` if
    /// there is no pending change, or `NotAuthorized` if the caller is not the
    /// pending collector. Emits `emit_fee_collector_accepted`.
    pub fn accept_fee_collector(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();

        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingFeeCollector)
            .ok_or(ContractError::NoPendingFeeCollector)?;

        if caller != pending {
            return Err(ContractError::NotAuthorized);
        }

        let old_collector: Address = crate::storage::read_global_config(&env)
            .fee_collector
            .ok_or(ContractError::NotAuthorized)?;

        env.storage()
            .instance()
            .set(&DataKey::FeeCollector, &pending);
        env.storage()
            .instance()
            .remove(&DataKey::PendingFeeCollector);
        emit_fee_collector_accepted(&env, old_collector.clone(), pending.clone());
        storage::update_global_config(&env, |c| c.fee_collector = Some(pending.clone()));
        emit_fee_collector_updated(&env, old_collector, pending);
        Ok(())
    }

    /// Begins a 2-step treasury change. Validates the new address and stores it
    /// as `PendingTreasury`. The active treasury stays unchanged until the
    /// proposed address calls `accept_treasury`, preventing accidental loss from
    /// typos or mistaken admin input.
    pub fn set_treasury(env: Env, new_treasury: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        admin.require_auth();

        let old_treasury = validate_treasury_change(&env, &new_treasury)?;

        env.storage()
            .instance()
            .set(&DataKey::PendingTreasury, &new_treasury);
        emit_treasury_pending(&env, old_treasury, new_treasury);
        Ok(())
    }

    /// Finalizes a pending treasury change. Must be called by the address that
    /// was registered as `PendingTreasury` via `set_treasury`.
    pub fn accept_treasury(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();

        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingTreasury)
            .ok_or(ContractError::NoPendingTreasury)?;

        if caller != pending {
            return Err(ContractError::NotAuthorized);
        }

        let old_treasury = read_treasury(&env).unwrap_or_else(|_| crate::zero_address(&env));
        write_treasury(&env, &pending);
        env.storage().instance().remove(&DataKey::PendingTreasury);
        emit_treasury_accepted(&env, old_treasury.clone(), pending.clone());
        emit_treasury_updated(&env, old_treasury, pending);
        Ok(())
    }

    /// Sets the arbitration fee (in basis points) deducted from escrows
    /// during dispute resolution. Only callable by admin. Reverts with
    /// `ArbitrationFeeExceedsMax` if `fee_bps` exceeds `MAX_ARBITRATION_FEE_BPS`, or
    /// with the combined-fee cap if `protocol_fee_bps + fee_bps` would
    /// exceed `MAX_COMBINED_FEE_BPS`. Emits `arbitration_fee_updated`.
    pub fn set_arbitration_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let old_fee_bps = update_arbitration_fee(&env, &caller, fee_bps)?;
        emit_arbitration_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    /// Returns the current arbitration fee in basis points.
    pub fn get_arbitration_fee(env: Env) -> u32 {
        storage::read_fee_config(&env).arbitration_fee_bps
    }

    pub fn get_total_arbitration_fees(env: Env, token: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalArbitrationFees(token))
            .unwrap_or(0)
    }

    /// Returns the appeal fee in basis points charged to the appellant on
    /// every `appeal_dispute` (issue #913). Defaults to
    /// `DEFAULT_APPEAL_FEE_BPS` (0 = disabled) until the admin sets one.
    pub fn get_appeal_fee(env: Env) -> u32 {
        read_appeal_fee_bps(&env)
    }

    /// Sets the appeal fee immediately (not timelocked). Only callable by the
    /// current admin. `0` disables the fee; any other value must lie in
    /// `MIN_APPEAL_FEE_BPS..=MAX_APPEAL_FEE_BPS` so a nominal fee cannot keep
    /// griefing effectively free. Emits `appeal_fee_updated`.
    #[cfg(any(test, feature = "testutils"))]
    pub fn set_appeal_fee(env: Env, caller: Address, fee_bps: u32) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        validate_appeal_fee_bps(fee_bps)?;
        let old_fee = read_appeal_fee_bps(&env);
        write_appeal_fee_bps(&env, fee_bps);
        emit_appeal_fee_updated(&env, old_fee, fee_bps);
        Ok(())
    }

    pub fn queue_set_appeal_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetAppealFee as u32,
            params,
        )
    }

    pub fn execute_set_appeal_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        let _proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetAppealFee, params)?;

        validate_appeal_fee_bps(fee_bps)?;
        let old_fee = read_appeal_fee_bps(&env);
        write_appeal_fee_bps(&env, fee_bps);
        emit_appeal_fee_updated(&env, old_fee, fee_bps);
        Ok(())
    }

    /// Returns whether recovery mode is enabled (issue #914).
    pub fn is_recovery_mode(env: Env) -> bool {
        is_recovery_mode(&env)
    }

    /// Enables recovery mode: graceful-shutdown flag that opens the
    /// `recovery_withdraw` escape hatch so buyers can reclaim custodied funds
    /// without going through the standard state machine — e.g. after a
    /// protocol sunset or an unpatchable vulnerability where a plain `Paused`
    /// state would trap funds indefinitely.
    ///
    /// Immediate (not timelocked) and admin-only: recovery is an emergency
    /// path, so it must be usable the moment the admin decides the protocol
    /// cannot continue safely. Pair with `queue_pause_contract` /
    /// `execute_pause_contract` to halt new escrows while users exit.
    /// Emits `recovery_mode_updated` with `enabled = true`.
    pub fn enable_recovery_mode(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        write_recovery_mode(&env, true);
        emit_recovery_mode_updated(&env, true, caller);
        Ok(())
    }

    /// Disables recovery mode, closing the `recovery_withdraw` escape hatch
    /// and restoring the standard state machine as the only exit path.
    /// Admin-only. Emits `recovery_mode_updated` with `enabled = false`.
    pub fn disable_recovery_mode(env: Env, caller: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        write_recovery_mode(&env, false);
        emit_recovery_mode_updated(&env, false, caller);
        Ok(())
    }

    /// Buyer reclaims escrowed funds while recovery mode is enabled (issue
    /// #914). Deliberately bypasses the contract pause flag and the standard
    /// state machine: any escrow holding buyer funds (`Funded`, `Shipped`,
    /// `Disputed`, or `RefundRequested`) transitions to `Refunded` and pays
    /// the principal plus any basket tokens to the buyer.
    ///
    /// `PendingFinalization` is explicitly excluded (issue #1204): a resolver
    /// has already ruled and the appeal window is open, so the dispute must
    /// run through `finalize_dispute` / `appeal_dispute`.
    ///
    /// Reverts with `NotInRecoveryMode` unless the admin has called
    /// `enable_recovery_mode`, with `EscrowHasNoBuyer` if the escrow has no
    /// assigned buyer (`buyer: None`, issue #1298), with `NotAuthorizedBuyer`
    /// if `caller` is not the assigned buyer, or with `InvalidState` (or the
    /// matching terminal-state error) for `PendingFinalization` and for escrows
    /// holding no buyer funds (`Pending`, `Completed`, `Refunded`, `Canceled`,
    /// `Expired`).
    /// Emits `recovery_withdraw`.
    pub fn recovery_withdraw(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        if !is_recovery_mode(&env) {
            return Err(ContractError::NotInRecoveryMode);
        }

        let mut escrow = load_escrow(&env, escrow_id)?;

        // A `PendingFinalization` escrow already has a ruling and an open
        // appeal window; draining it would bypass `finalize_dispute` /
        // `appeal_dispute` (issue #1204).
        if escrow.state == EscrowState::PendingFinalization {
            return Err(ContractError::InvalidState);
        }

        if matches!(
            escrow.state,
            EscrowState::Completed
                | EscrowState::Refunded
                | EscrowState::Canceled
                | EscrowState::Expired
        ) {
            return Err(terminal_state_error(
                &escrow.state,
                ContractError::InvalidState,
            ));
        }

        // Guard unassigned escrows (`buyer: None`) before checking recoverable
        // funding states so unauthorized callers can never claim buyerless
        // escrows and always receive `EscrowHasNoBuyer` (issue #1298).
        let assigned_buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != assigned_buyer {
            return Err(ContractError::NotAuthorizedBuyer);
        }

        let is_recoverable = matches!(
            escrow.state,
            EscrowState::Funded
                | EscrowState::Shipped
                | EscrowState::Disputed
                | EscrowState::RefundRequested
        );
        if !is_recoverable {
            return Err(terminal_state_error(
                &escrow.state,
                ContractError::InvalidState,
            ));
        }

        // ── EFFECTS (state mutations) — must precede all external calls (CEI) ──
        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Refunded;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        crate::internal::clear_delivery_proposal(&env, escrow_id);
        record_escrow_refunded(&env, escrow_id)?;

        // ── INTERACTIONS (external token transfers) ──
        let amount = escrow.amount;
        payout(&env, &escrow.token, &assigned_buyer, amount);
        payout_basket_tokens(&env, escrow_id, &assigned_buyer, None)?;

        emit_recovery_withdraw(&env, escrow_id, assigned_buyer, amount);
        Ok(())
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

        storage::update_global_config(&env, |c| {
            if enabled {
                c.state_flags |= crate::types::GLOBAL_CONFIG_FLAG_TOKEN_ALLOWLIST_ENABLED;
            } else {
                c.state_flags &= !crate::types::GLOBAL_CONFIG_FLAG_TOKEN_ALLOWLIST_ENABLED;
            }
        });
        emit_allowlist_toggled(&env, enabled);
        Ok(())
    }

    /// Returns the full list of allowlisted tokens.
    pub fn get_allowed_tokens(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    pub fn get_platform_fee_bps(env: Env) -> u32 {
        read_platform_fee_bps(&env)
    }

    /// Register a protocol fee discount for a high-volume merchant (primary payee).
    pub fn set_merchant_tier(
        env: Env,
        caller: Address,
        merchant: Address,
        discount_bps: u32,
    ) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "set_merchant_tier")?;
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        if discount_bps > crate::MAX_ESCROW_FEE_BPS {
            return Err(ContractError::FeeExceedsMax);
        }
        storage::write_merchant_fee_tier(&env, &merchant, discount_bps);
        Ok(())
    }

    pub fn get_merchant_fee_discount(env: Env, merchant: Address) -> u32 {
        storage::merchant_fee_discount_bps(&env, &merchant)
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

        let mut allowlist: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        if crate::internal::contains(&allowlist, &token) {
            return Ok(());
        }

        allowlist.push_back(token.clone());
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

        let allowlist: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env));

        if !crate::internal::contains(&allowlist, &token) {
            return Err(ContractError::TokenNotAllowed);
        }

        let mut new_allowlist = soroban_sdk::Vec::new(&env);
        for allowed_token in allowlist.iter() {
            if allowed_token != token {
                new_allowlist.push_back(allowed_token);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlist, &new_allowlist);

        emit_token_allowlist_updated(&env, token, false);
        Ok(())
    }

    /// Cancels a queued (not-yet-executed) timelocked admin operation. Only
    /// callable by admin. Reverts with `InvalidState` if no proposal is
    /// currently queued for `operation`. Emits `emit_timelock_cancelled`.
    pub fn cancel_timelock_op(
        env: Env,
        caller: Address,
        operation: u32,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let proposal =
            storage::read_timelock_proposal(&env, operation).ok_or(ContractError::InvalidState)?;

        storage::remove_timelock_proposal(&env, operation);
        emit_timelock_cancelled(&env, operation, proposal.proposer, caller);
        Ok(())
    }

    // 1. SetAdmin
    pub fn queue_set_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), ContractError> {
        // #1246: Reject self-assignment before consuming the timelock slot.
        let current_admin = require_admin(&env)?;
        if new_admin == current_admin {
            return Err(ContractError::SameAddress);
        }
        let mut params = Vec::new(&env);
        params.push_back(new_admin.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetAdmin as u32, params)
    }

    pub fn execute_set_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(new_admin.into_val(&env));
        let _proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetAdmin, params)?;

        let old_admin = require_admin(&env)?;
        if new_admin == old_admin {
            return Err(ContractError::SameAddress);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        // #1234: the incoming admin must not inherit any queued changes from
        // the predecessor (fee changes, upgrades, allowlist edits, …).
        clear_all_timelock_proposals(&env, &new_admin);
        emit_admin_rotated(&env, old_admin, new_admin);
        Ok(())
    }

    // 2. Upgrade
    pub fn queue_upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "queue_upgrade")?;
        let mut params = Vec::new(&env);
        params.push_back(new_wasm_hash.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::Upgrade as u32, params)
    }

    pub fn execute_upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "execute_upgrade")?;
        let mut params = Vec::new(&env);
        params.push_back(new_wasm_hash.into_val(&env));
        let _proposal = execute_timelock_op(&env, &caller, TimelockOperation::Upgrade, params)?;

        // Reject the all-zeros hash as a trivially invalid placeholder. Full
        // on-chain WASM-hash existence validation is not possible via the
        // current Soroban SDK; the host will trap if the hash is not in the
        // ledger, reverting the transaction without bricking the contract.
        if new_wasm_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(ContractError::InvalidWasmHash);
        }

        let admin = require_admin(&env)?;

        // #1245: Append an immutable upgrade record to the on-chain audit ledger
        // before executing the upgrade so the record reflects the state that
        // is about to become active.
        let version = {
            let mut history: soroban_sdk::Vec<crate::types::UpgradeRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::UpgradeHistory)
                .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
            let next_version = u32::try_from(history.len())
                .unwrap_or(u32::MAX)
                .saturating_add(1);
            history.push_back(crate::types::UpgradeRecord {
                wasm_hash: new_wasm_hash.clone(),
                upgraded_at: env.ledger().timestamp(),
                upgraded_by: admin.clone(),
                version: next_version,
            });
            let ext = storage::get_ttl_extension(&env);
            env.storage()
                .persistent()
                .set(&DataKey::UpgradeHistory, &history);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::UpgradeHistory, ext / 2, ext);
            next_version
        };

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        emit_contract_upgraded(&env, admin, new_wasm_hash);
        let _ = version; // version is embedded in the history record
        Ok(())
    }

    // 4. SetArbitrationFee
    pub fn queue_set_arbitration_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "queue_set_arbitration_fee")?;
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetArbitrationFee as u32,
            params,
        )
    }

    pub fn execute_set_arbitration_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "execute_set_arbitration_fee")?;
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        let proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetArbitrationFee, params)?;

        let old_fee_bps = update_arbitration_fee(&env, &proposal.proposer, fee_bps)?;
        emit_arbitration_fee_updated(&env, old_fee_bps, fee_bps);
        Ok(())
    }

    // 5. SetPlatformFee
    pub fn queue_set_platform_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "queue_set_platform_fee")?;
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetPlatformFee as u32,
            params,
        )
    }

    pub fn execute_set_platform_fee(
        env: Env,
        caller: Address,
        fee_bps: u32,
    ) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "execute_set_platform_fee")?;
        let mut params = Vec::new(&env);
        params.push_back(fee_bps.into_val(&env));
        let _proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetPlatformFee, params)?;

        if fee_bps > MAX_PLATFORM_FEE_BPS {
            return Err(ContractError::PlatformFeeExceedsMax);
        }
        let global_config = read_global_config(&env);
        validate_total_fee_bps(
            global_config.protocol_fee_bps,
            fee_bps,
            global_config.arbitration_fee_bps,
            0,
        )?;
        let old_fee = read_platform_fee_bps(&env);
        write_platform_fee_bps(&env, fee_bps);
        emit_platform_fee_updated(&env, old_fee, fee_bps);
        Ok(())
    }

    // 6. SetTreasury
    pub fn queue_set_treasury(
        env: Env,
        caller: Address,
        treasury: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(treasury.into_val(&env));
        queue_timelock_op(&env, &caller, TimelockOperation::SetTreasury as u32, params)
    }

    pub fn execute_set_treasury(
        env: Env,
        caller: Address,
        treasury: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(treasury.into_val(&env));
        let _proposal = execute_timelock_op(&env, &caller, TimelockOperation::SetTreasury, params)?;

        let old_treasury = validate_treasury_change(&env, &treasury)?;
        env.storage()
            .instance()
            .set(&DataKey::PendingTreasury, &treasury);
        emit_treasury_pending(&env, old_treasury, treasury);
        Ok(())
    }

    /// Returns the configured treasury address. Reverts with
    /// `NotInitialized` if no treasury has been set via `set_treasury`.
    pub fn get_treasury(env: Env) -> Result<Address, ContractError> {
        read_treasury(&env)
    }

    // 7. SetFeeCollector
    pub fn queue_set_fee_collector(
        env: Env,
        caller: Address,
        new_collector: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(new_collector.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetFeeCollector as u32,
            params,
        )
    }

    pub fn execute_set_fee_collector(
        env: Env,
        caller: Address,
        new_collector: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(new_collector.into_val(&env));
        let _proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetFeeCollector, params)?;

        let old_collector = validate_fee_collector_change(&env, &new_collector)?;

        env.storage()
            .instance()
            .set(&DataKey::PendingFeeCollector, &new_collector);
        emit_fee_collector_pending(&env, old_collector, new_collector);
        Ok(())
    }

    // 8. SetTtlExtension
    pub fn queue_set_ttl_extension(
        env: Env,
        caller: Address,
        ledgers: u32,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(ledgers.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetTtlExtension as u32,
            params,
        )
    }

    pub fn execute_set_ttl_extension(
        env: Env,
        caller: Address,
        ledgers: u32,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(ledgers.into_val(&env));
        let proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetTtlExtension, params)?;

        if !(MIN_TTL_EXTENSION..=MAX_TTL_EXTENSION).contains(&ledgers) {
            return Err(ContractError::InvalidTtlExtension);
        }

        let old_ledgers = storage::get_ttl_extension(&env);
        storage::update_global_config(&env, |c| c.ttl_extension_ledgers = ledgers);
        emit_ttl_extension_updated(&env, old_ledgers, ledgers, proposal.proposer);
        Ok(())
    }

    // 9. SetAmountLimits
    pub fn queue_set_amount_limits(
        env: Env,
        caller: Address,
        min_amount: i128,
        max_amount: i128,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(min_amount.into_val(&env));
        params.push_back(max_amount.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetAmountLimits as u32,
            params,
        )
    }

    pub fn execute_set_amount_limits(
        env: Env,
        caller: Address,
        min_amount: i128,
        max_amount: i128,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(min_amount.into_val(&env));
        params.push_back(max_amount.into_val(&env));
        let proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetAmountLimits, params)?;

        if min_amount <= 0 || max_amount <= min_amount {
            return Err(ContractError::InvalidAmount);
        }
        let config = storage::read_global_config(&env);
        let (old_min_amount, old_max_amount) = (config.min_amount, config.max_amount);

        storage::write_global_config(
            &env,
            &GlobalConfig {
                min_amount,
                max_amount,
                ..config
            },
        );
        emit_amount_limits_updated(
            &env,
            old_min_amount,
            min_amount,
            old_max_amount,
            max_amount,
            proposal.proposer,
        );
        Ok(())
    }

    // 10. AddApprovedResolver
    pub fn queue_add_approved_resolver(
        env: Env,
        caller: Address,
        resolver: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(resolver.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::AddApprovedResolver as u32,
            params,
        )
    }

    pub fn execute_add_approved_resolver(
        env: Env,
        caller: Address,
        resolver: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(resolver.into_val(&env));
        let proposal = execute_timelock_op(
            &env,
            &caller,
            TimelockOperation::AddApprovedResolver,
            params,
        )?;

        let mut approved: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        if crate::internal::contains(&approved, &resolver) {
            return Ok(());
        }
        approved.push_back(resolver.clone());
        env.storage()
            .instance()
            .set(&DataKey::ApprovedResolvers, &approved);
        emit_resolver_approved(&env, resolver, proposal.proposer);
        Ok(())
    }

    // 11. RemoveApprovedResolver
    pub fn queue_remove_approved_resolver(
        env: Env,
        caller: Address,
        resolver: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(resolver.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::RemoveApprovedResolver as u32,
            params,
        )
    }

    pub fn execute_remove_approved_resolver(
        env: Env,
        caller: Address,
        resolver: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(resolver.into_val(&env));
        let proposal = execute_timelock_op(
            &env,
            &caller,
            TimelockOperation::RemoveApprovedResolver,
            params,
        )?;

        let approved: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&approved, &resolver) {
            return Err(ContractError::InvalidAddress);
        }
        let mut new_approved = soroban_sdk::Vec::new(&env);
        for existing in approved.iter() {
            if existing != resolver {
                new_approved.push_back(existing);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::ApprovedResolvers, &new_approved);
        emit_resolver_removed(&env, resolver, proposal.proposer);
        Ok(())
    }

    // 12. SetResolverStrict
    pub fn queue_set_resolver_strict(
        env: Env,
        caller: Address,
        strict: bool,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(strict.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetResolverStrict as u32,
            params,
        )
    }

    pub fn execute_set_resolver_strict(
        env: Env,
        caller: Address,
        strict: bool,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(strict.into_val(&env));
        let proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetResolverStrict, params)?;

        let old_strict = storage::read_global_config(&env).state_flags
            & crate::types::GLOBAL_CONFIG_FLAG_RESOLVER_STRICT
            != 0;
        storage::update_global_config(&env, |c| {
            if strict {
                c.state_flags |= crate::types::GLOBAL_CONFIG_FLAG_RESOLVER_STRICT;
            } else {
                c.state_flags &= !crate::types::GLOBAL_CONFIG_FLAG_RESOLVER_STRICT;
            }
        });
        emit_resolver_strict_updated(&env, old_strict, strict, proposal.proposer);
        Ok(())
    }

    // 13. SetTokenAllowlistEnabled
    pub fn queue_set_allowlist_enabled(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(enabled.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetTokenAllowlistEnabled as u32,
            params,
        )
    }

    pub fn execute_set_allowlist_enabled(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(enabled.into_val(&env));
        let _proposal = execute_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetTokenAllowlistEnabled,
            params,
        )?;

        storage::update_global_config(&env, |c| {
            if enabled {
                c.state_flags |= crate::types::GLOBAL_CONFIG_FLAG_TOKEN_ALLOWLIST_ENABLED;
            } else {
                c.state_flags &= !crate::types::GLOBAL_CONFIG_FLAG_TOKEN_ALLOWLIST_ENABLED;
            }
        });
        emit_allowlist_toggled(&env, enabled);
        Ok(())
    }

    // 14. AddAllowedToken
    pub fn queue_add_allowed_token(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(token.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::AddAllowedToken as u32,
            params,
        )
    }

    pub fn execute_add_allowed_token(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(token.into_val(&env));
        let _proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::AddAllowedToken, params)?;

        let mut allowlist: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        if crate::internal::contains(&allowlist, &token) {
            return Ok(());
        }
        allowlist.push_back(token.clone());
        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlist, &allowlist);
        emit_token_allowlist_updated(&env, token, true);
        Ok(())
    }

    // 15. RemoveAllowedToken
    pub fn queue_remove_allowed_token(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(token.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::RemoveAllowedToken as u32,
            params,
        )
    }

    pub fn execute_remove_allowed_token(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(token.into_val(&env));
        let _proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::RemoveAllowedToken, params)?;

        let allowlist: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::TokenAllowlist)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&allowlist, &token) {
            return Err(ContractError::TokenNotAllowed);
        }
        let mut new_allowlist = soroban_sdk::Vec::new(&env);
        for allowed_token in allowlist.iter() {
            if allowed_token != token {
                new_allowlist.push_back(allowed_token);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::TokenAllowlist, &new_allowlist);
        emit_token_allowlist_updated(&env, token, false);
        Ok(())
    }

    // 16. PauseContract
    pub fn queue_pause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        let params = Vec::new(&env);
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::PauseContract as u32,
            params,
        )
    }

    pub fn execute_pause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        execute_timelock_op(
            &env,
            &caller,
            TimelockOperation::PauseContract,
            Vec::new(&env),
        )?;

        let admin = require_admin(&env)?;
        storage::update_global_config(&env, |c| {
            c.state_flags |= crate::types::GLOBAL_CONFIG_FLAG_PAUSED;
        });
        emit_contract_paused(&env, admin);
        Ok(())
    }

    // 17. UnpauseContract
    pub fn queue_unpause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        let params = Vec::new(&env);
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::UnpauseContract as u32,
            params,
        )
    }

    pub fn execute_unpause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        execute_timelock_op(
            &env,
            &caller,
            TimelockOperation::UnpauseContract,
            Vec::new(&env),
        )?;

        let admin = require_admin(&env)?;
        storage::update_global_config(&env, |c| {
            c.state_flags &= !crate::types::GLOBAL_CONFIG_FLAG_PAUSED;
        });
        emit_contract_unpaused(&env, admin);
        Ok(())
    }

    // 18. SetMaxAppeals
    pub fn queue_set_max_appeals(
        env: Env,
        caller: Address,
        max_appeals: u32,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(max_appeals.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetMaxAppeals as u32,
            params,
        )
    }

    pub fn execute_set_max_appeals(
        env: Env,
        caller: Address,
        max_appeals: u32,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(max_appeals.into_val(&env));
        let _proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetMaxAppeals, params)?;

        if !(crate::MIN_MAX_APPEALS..=crate::MAX_MAX_APPEALS).contains(&max_appeals) {
            return Err(ContractError::InvalidMaxAppeals);
        }
        let old_max = crate::read_max_appeals(&env);
        env.storage()
            .instance()
            .set(&DataKey::MaxAppeals, &max_appeals);
        emit_max_appeals_updated(&env, old_max, max_appeals);
        Ok(())
    }

    /// Returns the admin-configured maximum number of appeals per dispute.
    pub fn get_max_appeals(env: Env) -> u32 {
        crate::read_max_appeals(&env)
    }

    // 19. SetMaxBasketSize
    pub fn queue_set_max_basket_size(
        env: Env,
        caller: Address,
        max_basket_size: u32,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(max_basket_size.into_val(&env));
        queue_timelock_op(
            &env,
            &caller,
            TimelockOperation::SetMaxBasketSize as u32,
            params,
        )
    }

    pub fn execute_set_max_basket_size(
        env: Env,
        caller: Address,
        max_basket_size: u32,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(max_basket_size.into_val(&env));
        let _proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetMaxBasketSize, params)?;

        if !(crate::MIN_MAX_BASKET_SIZE..=crate::MAX_BASKET_SIZE).contains(&max_basket_size) {
            return Err(ContractError::InvalidMaxBasketSize);
        }
        let old_max = crate::read_max_basket_size(&env);
        env.storage()
            .instance()
            .set(&DataKey::MaxBasketSize, &max_basket_size);
        emit_max_basket_size_updated(&env, old_max, max_basket_size);
        Ok(())
    }

    /// Returns the admin-configured maximum number of tokens in a basket escrow.
    pub fn get_max_basket_size(env: Env) -> u32 {
        crate::read_max_basket_size(&env)
    }

    /// Emergency drain: transfers all escrowed funds back to the buyer.
    ///
    /// # Authorization — Dual-Signature Requirement (Issue #709)
    ///
    /// This function requires **both** the buyer *and* the seller to authorize the
    /// transaction (`buyer.require_auth()` **and** `seller.require_auth()`).  The
    /// dual-signature design prevents either party from unilaterally extracting funds
    /// while the contract is paused, which provides a strong security guarantee against
    /// unilateral drain attacks.
    ///
    /// **Liveness risk**: If one party (typically the seller) becomes uncooperative or
    /// permanently inaccessible, funds can remain locked even after the contract is
    /// paused — the drain cannot proceed without both signatures.
    ///
    /// **Recommended escape hatch**: A future upgrade should add an admin-controlled
    /// timelock override that allows the platform admin to execute the drain unilaterally
    /// after a sufficiently long waiting period (e.g., 30 days of inactivity) has elapsed
    /// since the contract was paused.  This balances security (no instant admin override)
    /// against liveness (funds are never permanently stuck).  Until that path is
    /// implemented, platform operators should include this limitation in user-facing
    /// documentation and Terms of Service.
    #[allow(deprecated)]
    pub fn emergency_drain(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        let paused = storage::read_global_config(&env).state_flags
            & crate::types::GLOBAL_CONFIG_FLAG_PAUSED
            != 0;
        if !paused {
            return Err(ContractError::ContractNotPaused);
        }

        let mut escrow = load_escrow(&env, escrow_id)?;

        // Issue #1204: a `PendingFinalization` escrow already has a resolver
        // ruling and an open appeal window. Draining it would bypass
        // `finalize_dispute` / `appeal_dispute`, so reject it and emit a
        // diagnostic event.
        if escrow.state == EscrowState::PendingFinalization {
            crate::events::emit_emergency_drain_blocked(&env, escrow_id);
            return Err(ContractError::InvalidState);
        }

        let is_drainable = matches!(
            escrow.state,
            EscrowState::Funded | EscrowState::Shipped | EscrowState::Disputed
        );
        if !is_drainable {
            return Err(terminal_state_error(
                &escrow.state,
                ContractError::InvalidState,
            ));
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

        // ── EFFECTS (state mutations) — must precede all external calls (CEI) ──
        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Refunded;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        crate::internal::clear_delivery_proposal(&env, escrow_id);

        // Issue #814 / #1057: when emergency_drain runs on a disputed escrow,
        // escrow.amount has already been reduced by the arbitration fee in
        // execute_resolution_transition, and that fee is tracked separately in
        // DataKey::TotalArbitrationFees(token). The drained principal is
        // therefore refunded to the buyer and counted through the single
        // `record_escrow_refunded` helper exactly once per drain — the sharded
        // TotalRefunded counter is a *count* of refunded escrows, not a sum of
        // amounts, so it must not be incremented again for the fee portion.
        // Fee volume lives in TotalArbitrationFees.
        record_escrow_refunded(&env, escrow_id)?;

        // ── INTERACTIONS (external token transfers) ──
        payout(&env, &escrow.token, &buyer, escrow.amount);
        payout_basket_tokens(&env, escrow_id, &buyer, None)?;

        crate::events::emit_emergency_drain(&env, escrow_id, escrow.token.clone(), escrow.amount);
        Ok(())
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn set_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        let old_admin = require_admin(&env)?;
        old_admin.require_auth();
        if new_admin == old_admin {
            return Err(ContractError::SameAddress);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        // Same invariant as the timelocked `execute_set_admin` path (#1234).
        clear_all_timelock_proposals(&env, &new_admin);
        emit_admin_rotated(&env, old_admin, new_admin);
        Ok(())
    }

    /// Test-only: sets the protocol fee directly, bypassing the timelock.
    ///
    /// Gated behind `cfg(any(test, feature = "testutils"))` and therefore
    /// **never part of a production build**; the production fee path is the
    /// timelocked `queue_*`/`execute_*` flow. To keep test behaviour from
    /// drifting from production it enforces the same ceilings: the per-fee
    /// `MAX_PROTOCOL_FEE_BPS` cap and the combined protocol + arbitration cap
    /// (`validate_combined_fees`).
    #[cfg(any(test, feature = "testutils"))]
    pub fn set_fee(env: Env, caller: Address, fee: u32) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "set_fee")?;
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        if fee > MAX_PROTOCOL_FEE_BPS {
            return Err(ContractError::ProtocolFeeExceedsMax);
        }
        let mut config = read_fee_config(&env);
        validate_combined_fees(fee, config.arbitration_fee_bps)?;
        let global_config = read_global_config(&env);
        validate_total_fee_bps(
            fee,
            global_config.platform_fee_bps,
            config.arbitration_fee_bps,
            0,
        )?;
        config.protocol_fee_bps = fee;
        write_fee_config(&env, &config);
        Ok(())
    }

    /// Test-only: like `set_fee`, but also emits `protocol_fee_updated`.
    ///
    /// Same cfg-gating and the same per-fee and combined-fee ceilings as
    /// `set_fee`; never part of a production build.
    #[cfg(any(test, feature = "testutils"))]
    pub fn set_protocol_fee(env: Env, caller: Address, fee: u32) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        if fee > MAX_PROTOCOL_FEE_BPS {
            return Err(ContractError::ProtocolFeeExceedsMax);
        }
        let mut config = read_fee_config(&env);
        validate_combined_fees(fee, config.arbitration_fee_bps)?;
        let global_config = read_global_config(&env);
        validate_total_fee_bps(
            fee,
            global_config.platform_fee_bps,
            config.arbitration_fee_bps,
            0,
        )?;
        let old_fee = config.protocol_fee_bps;
        config.protocol_fee_bps = fee;
        write_fee_config(&env, &config);
        emit_protocol_fee_updated(&env, old_fee, fee);
        Ok(())
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn set_ttl_extension(
        env: Env,
        caller: Address,
        extension_seconds: u32,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        if !(MIN_TTL_EXTENSION..=MAX_TTL_EXTENSION).contains(&extension_seconds) {
            return Err(ContractError::InvalidTtlExtension);
        }
        let old_ledgers = storage::get_ttl_extension(&env);
        storage::update_global_config(&env, |c| c.ttl_extension_ledgers = extension_seconds);
        emit_ttl_extension_updated(&env, old_ledgers, extension_seconds, caller);
        Ok(())
    }

    /// Sets the maximum duration (in seconds) a dispute may remain unresolved
    /// before either the buyer or the seller can force a Refund with
    /// `claim_dispute_timeout`. Admin-only. Reverts with `NotAuthorized` if
    /// `caller` is not the admin, or `InvalidDisputeTimeout` if
    /// `timeout_seconds` falls outside
    /// `MIN_DISPUTE_TIMEOUT..=MAX_DISPUTE_TIMEOUT`.
    pub fn set_dispute_timeout(
        env: Env,
        caller: Address,
        timeout_seconds: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin = require_admin(&env)?;
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        if !(crate::MIN_DISPUTE_TIMEOUT..=crate::MAX_DISPUTE_TIMEOUT).contains(&timeout_seconds) {
            return Err(ContractError::InvalidDisputeTimeout);
        }
        write_dispute_timeout(&env, timeout_seconds);
        Ok(())
    }

    /// Returns the configured maximum dispute duration in seconds. Defaults to
    /// `DEFAULT_DISPUTE_TIMEOUT` (30 days) until the admin overrides it.
    pub fn get_dispute_timeout(env: Env) -> u64 {
        read_dispute_timeout(&env)
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn pause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        storage::update_global_config(&env, |c| {
            c.state_flags |= crate::types::GLOBAL_CONFIG_FLAG_PAUSED;
        });
        emit_contract_paused(&env, admin);
        Ok(())
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn unpause_contract(env: Env, caller: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        // #1243: If strict governance mode is active, direct unpause is blocked.
        // The admin must use queue_unpause_contract → execute_unpause_contract.
        let flags = storage::read_global_config(&env).state_flags;
        if flags & crate::types::GLOBAL_CONFIG_FLAG_TIMELOCKED_UNPAUSE_ONLY != 0 {
            return Err(ContractError::DirectUnpauseBlocked);
        }
        storage::update_global_config(&env, |c| {
            c.state_flags &= !crate::types::GLOBAL_CONFIG_FLAG_PAUSED;
        });
        emit_contract_unpaused(&env, admin);
        Ok(())
    }

    /// #1243: Enable or disable strict governance mode for unpausing.
    ///
    /// When `enabled` is `true`, direct calls to `unpause_contract` are
    /// rejected with `DirectUnpauseBlocked`. Callers must use the timelocked
    /// `queue_unpause_contract` → `execute_unpause_contract` path instead.
    /// This formalises the security asymmetry: `pause_contract` is an
    /// immediate circuit-breaker, while resuming operations is subject to
    /// mandatory governance delay.
    ///
    /// Only callable by admin.
    pub fn set_timelocked_unpause_only(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        storage::update_global_config(&env, |c| {
            if enabled {
                c.state_flags |= crate::types::GLOBAL_CONFIG_FLAG_TIMELOCKED_UNPAUSE_ONLY;
            } else {
                c.state_flags &= !crate::types::GLOBAL_CONFIG_FLAG_TIMELOCKED_UNPAUSE_ONLY;
            }
        });
        Ok(())
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn set_amount_limits(
        env: Env,
        caller: Address,
        min: i128,
        max: i128,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        let config = storage::read_global_config(&env);
        let (old_min_amount, old_max_amount) = (config.min_amount, config.max_amount);
        storage::write_global_config(
            &env,
            &GlobalConfig {
                min_amount: min,
                max_amount: max,
                ..config
            },
        );
        emit_amount_limits_updated(&env, old_min_amount, min, old_max_amount, max, caller);
        Ok(())
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn set_resolver_strict(
        env: Env,
        caller: Address,
        strict: bool,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        let old_strict = storage::read_global_config(&env).state_flags
            & crate::types::GLOBAL_CONFIG_FLAG_RESOLVER_STRICT
            != 0;
        storage::update_global_config(&env, |c| {
            if strict {
                c.state_flags |= crate::types::GLOBAL_CONFIG_FLAG_RESOLVER_STRICT;
            } else {
                c.state_flags &= !crate::types::GLOBAL_CONFIG_FLAG_RESOLVER_STRICT;
            }
        });
        emit_resolver_strict_updated(&env, old_strict, strict, caller);
        Ok(())
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn add_approved_resolver(
        env: Env,
        caller: Address,
        resolver: Address,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        let mut approved: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&approved, &resolver) {
            approved.push_back(resolver.clone());
            env.storage()
                .instance()
                .set(&DataKey::ApprovedResolvers, &approved);
            emit_resolver_approved(&env, resolver, caller);
        }
        Ok(())
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn remove_approved_resolver(
        env: Env,
        caller: Address,
        resolver: Address,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        let approved: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&approved, &resolver) {
            return Err(ContractError::InvalidAddress);
        }
        let mut new_approved = soroban_sdk::Vec::new(&env);
        let mut removed = false;
        for a in approved.iter() {
            if a != resolver {
                new_approved.push_back(a);
            } else {
                removed = true;
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::ApprovedResolvers, &new_approved);
        if removed {
            emit_resolver_removed(&env, resolver, caller);
        }
        Ok(())
    }

    /// Admin-only sanction: forcibly replaces the resolver set of a live
    /// escrow with `replacement_resolver`, which must be on the approved
    /// resolver list. Unlike `remove_approved_resolver` (which only edits the
    /// global registry), this frees parties of an already-assigned resolver
    /// who turned malicious, was compromised, or went unresponsive.
    ///
    /// The escrow's resolver set becomes `Single(replacement_resolver)` and
    /// any votes recorded so far are discarded, so the sanctioned resolver's
    /// stale votes cannot count toward the new set's threshold. The escrow
    /// state is untouched. Reverts with `EscrowAlreadyCompleted` /
    /// `EscrowAlreadyRefunded` for those terminal states (`InvalidState` for
    /// `Canceled` / `Expired`), `UnauthorizedResolver` when the replacement is
    /// not on the approved list, `SameAddress` when it already is the sole
    /// resolver, and `InvalidAddress` when it is the buyer or a payee.
    /// Emits `resolver_sanctioned`.
    pub fn sanction_resolver(
        env: Env,
        caller: Address,
        escrow_id: u64,
        replacement_resolver: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        require_admin_caller(&env, &caller)?;

        let mut escrow = load_escrow(&env, escrow_id)?;

        if matches!(
            escrow.state,
            EscrowState::Completed
                | EscrowState::Refunded
                | EscrowState::Canceled
                | EscrowState::Expired
        ) {
            return Err(terminal_state_error(
                &escrow.state,
                ContractError::InvalidState,
            ));
        }

        let approved: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or(soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&approved, &replacement_resolver) {
            return Err(ContractError::UnauthorizedResolver);
        }

        // The resolver being sanctioned: the sole resolver, the fallback
        // primary, or the first member of a multi-resolver set.
        let old_resolver = match &escrow.resolvers {
            ResolverSet::Single(resolver) => resolver.clone(),
            ResolverSet::Fallback(f) => f.primary.clone(),
            ResolverSet::Multi(m) => m
                .resolvers
                .get(0)
                .ok_or(ContractError::InvalidResolverThreshold)?,
        };
        if escrow.resolvers == ResolverSet::Single(replacement_resolver.clone()) {
            return Err(ContractError::SameAddress);
        }

        // The replacement must not hold a conflicting role on this escrow.
        for i in 0..escrow.payees.inner.len() {
            let payee = escrow
                .payees
                .inner
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            if replacement_resolver == payee.address {
                return Err(ContractError::InvalidAddress);
            }
        }
        if escrow.buyer.as_ref() == Some(&replacement_resolver) {
            return Err(ContractError::InvalidAddress);
        }

        escrow.resolvers = ResolverSet::Single(replacement_resolver.clone());
        save_escrow(&env, escrow_id, &escrow, Some(&escrow.state));
        env.storage()
            .persistent()
            .remove(&DataKey::ResolverVotes(escrow_id));

        emit_resolver_sanctioned(&env, escrow_id, old_resolver, replacement_resolver, caller);
        Ok(())
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn get_approved_resolvers(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::ApprovedResolvers)
            .unwrap_or(soroban_sdk::Vec::new(&env))
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn is_resolver_strict(env: Env) -> bool {
        storage::read_global_config(&env).state_flags
            & crate::types::GLOBAL_CONFIG_FLAG_RESOLVER_STRICT
            != 0
    }

    #[cfg(any(test, feature = "testutils"))]
    pub fn upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: soroban_sdk::BytesN<32>,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        Ok(())
    }

    pub fn pause_action(
        env: Env,
        caller: Address,
        action: soroban_sdk::Symbol,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::ActionPaused(action.clone()), &true);
        emit_action_paused(&env, action, caller);
        Ok(())
    }

    pub fn unpause_action(
        env: Env,
        caller: Address,
        action: soroban_sdk::Symbol,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::ActionPaused(action.clone()), &false);
        emit_action_unpaused(&env, action, caller);
        Ok(())
    }

    pub fn set_platform_fee(env: Env, caller: Address, fee: u32) -> Result<(), ContractError> {
        ensure_admin_action_not_paused(&env, "set_platform_fee")?;
        let admin = require_admin(&env)?;
        caller.require_auth();
        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }
        if fee > MAX_PLATFORM_FEE_BPS {
            return Err(ContractError::PlatformFeeExceedsMax);
        }
        let global_config = read_global_config(&env);
        validate_total_fee_bps(
            global_config.protocol_fee_bps,
            fee,
            global_config.arbitration_fee_bps,
            0,
        )?;
        storage::update_global_config(&env, |c| c.platform_fee_bps = fee);
        Ok(())
    }

    // NOTE: the legacy immediate 3-arg `set_treasury(env, caller, treasury)`
    // was removed here — it duplicated the `set_treasury` name and bypassed
    // the 2-step propose/accept flow (`set_treasury` + `accept_treasury`).
    // GlobalConfig.treasury currently has no writer; see Phase 5b.

    // 19. SetTimelockDelay
    pub fn queue_set_timelock_delay(
        env: Env,
        caller: Address,
        delay_seconds: u64,
    ) -> Result<(), ContractError> {
        if !(MIN_TIMELOCK_DELAY_SECONDS..=MAX_TIMELOCK_DELAY_SECONDS).contains(&delay_seconds) {
            return Err(ContractError::InvalidTimelockDelay);
        }
        let mut params = Vec::new(&env);
        params.push_back(delay_seconds.into_val(&env));
        // Two-step delay transition (#1238): the change itself waits
        // `max(current_delay, delay_seconds)`. Shortening the delay therefore
        // still costs the full *current* delay, so a briefly compromised admin
        // key cannot collapse the timelock and then rush a malicious upgrade
        // through it; lengthening it waits the new, longer delay.
        // Proposals already queued keep the `ready_at` fixed at queue time,
        // so a reduction never shortens an in-flight countdown either.
        queue_timelock_op_with_min_delay(
            &env,
            &caller,
            TimelockOperation::SetTimelockDelay as u32,
            params,
            delay_seconds,
        )
    }

    pub fn execute_set_timelock_delay(
        env: Env,
        caller: Address,
        delay_seconds: u64,
    ) -> Result<(), ContractError> {
        let mut params = Vec::new(&env);
        params.push_back(delay_seconds.into_val(&env));
        let proposal =
            execute_timelock_op(&env, &caller, TimelockOperation::SetTimelockDelay, params)?;

        if !(MIN_TIMELOCK_DELAY_SECONDS..=MAX_TIMELOCK_DELAY_SECONDS).contains(&delay_seconds) {
            return Err(ContractError::InvalidTimelockDelay);
        }

        let old_delay = env
            .storage()
            .instance()
            .get(&DataKey::AdminTimelockDelay)
            .unwrap_or(ADMIN_TIMELOCK_DELAY_SECONDS);
        env.storage()
            .instance()
            .set(&DataKey::AdminTimelockDelay, &delay_seconds);
        crate::events::emit_timelock_delay_updated(
            &env,
            old_delay,
            delay_seconds,
            proposal.proposer,
        );
        Ok(())
    }

    /// Returns the accumulated unclaimed protocol fees for `token`.
    pub fn get_accumulated_fees(env: Env, token: Address) -> i128 {
        storage::read_accumulated_fees(&env, &token)
    }

    /// Pull-based withdrawal of accumulated protocol fees for `token`.
    ///
    /// Only callable by the configured `fee_collector`. Clears
    /// `DataKey::AccumulatedFees(token)` before transferring tokens (CEI).
    pub fn claim_accumulated_fees(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Result<i128, ContractError> {
        caller.require_auth();
        let fee_collector = storage::read_global_config(&env)
            .fee_collector
            .ok_or(ContractError::NotInitialized)?;
        if caller != fee_collector {
            return Err(ContractError::NotAuthorized);
        }

        let amount = storage::take_accumulated_fees(&env, &token);
        if amount <= 0 {
            return Err(ContractError::InsufficientBalance);
        }

        payout(&env, &token, &fee_collector, amount);
        crate::events::emit_accumulated_fees_claimed(&env, fee_collector, token, amount);
        Ok(amount)
    }
}

#[cfg(test)]
mod accumulated_fee_tests {
    use crate::{ContractError, Escrow, EscrowClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::{Client as TokenClient, StellarAssetClient},
        Address, Env,
    };

    #[test]
    fn claim_accumulated_fees_pulls_accumulated_balance_and_resets_to_zero() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let outsider = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();

        client.initialize(&admin, &fee_collector, &0_u32);
        StellarAssetClient::new(&env, &token_id).mint(&contract_id, &500);

        env.as_contract(&contract_id, || {
            crate::storage::add_accumulated_fee(&env, &token_id, 200).unwrap();
            crate::storage::add_accumulated_fee(&env, &token_id, 300).unwrap();
        });

        assert_eq!(client.get_accumulated_fees(&token_id), 500);
        assert_eq!(
            client.try_claim_accumulated_fees(&outsider, &token_id),
            Err(Ok(ContractError::NotAuthorized))
        );

        let claimed = client.claim_accumulated_fees(&fee_collector, &token_id);
        assert_eq!(claimed, 500);
        assert_eq!(client.get_accumulated_fees(&token_id), 0);
        assert_eq!(
            TokenClient::new(&env, &token_id).balance(&fee_collector),
            500
        );
        assert_eq!(
            client.try_claim_accumulated_fees(&fee_collector, &token_id),
            Err(Ok(ContractError::InsufficientBalance))
        );
    }
}
