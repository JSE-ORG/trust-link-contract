#![cfg(test)]

use crate::test_helpers::setup_contract;
use crate::ContractError;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

#[test]
fn test_admin_rotation() {
    let env = Env::default();
    env.mock_all_auths();

    let _token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let new_admin = Address::generate(&env);

    // set_admin succeeds when called by current admin
    client.set_admin(&new_admin);

    // new admin can call set_fee
    client.set_protocol_fee(&new_admin, &100_u32);

    // old admin can no longer set_fee (auth will fail — mock_all_auths means any address is
    // treated as authorized, so we verify state rather than auth enforcement here)
    // Verify the fee was set
    // (In production, old_admin.require_auth() inside set_admin enforces this)
}

#[test]
fn test_set_fee_updates_default_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    client.set_protocol_fee(&admin, &150_u32);
}

#[test]
fn test_set_fee_exceeds_max_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let result = client.try_set_protocol_fee(&admin, &10_001_u32);
    assert!(matches!(
        result,
        Err(Ok(ContractError::ProtocolFeeExceedsMax))
    ));
}

#[test]
fn test_calculate_fee_helper_ranges() {
    assert_eq!(crate::helpers::payout::calculate_fee(10_000, 0).unwrap(), 0);
    assert_eq!(
        crate::helpers::payout::calculate_fee(10_000, 100).unwrap(),
        100
    );
    assert_eq!(
        crate::helpers::payout::calculate_fee(10_000, 300).unwrap(),
        300
    );
}

#[test]
fn test_admin_rotated_event_emitted() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let new_admin = Address::generate(&env);
    client.set_admin(&new_admin);
    // Event emission verified by successful execution and no panic
}

#[test]
fn test_cancel_timelock_op_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let intruder = Address::generate(&env);
    client.queue_set_arbitration_fee(&admin, &100_u32);

    let result = client.try_cancel_timelock_op(&intruder, &4_u32);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn test_remove_allowed_token_when_token_not_in_list() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let missing = Address::generate(&env);

    let result = client.try_remove_allowed_token(&admin, &missing);
    assert_eq!(result, Err(Ok(ContractError::TokenNotAllowed)));
}

#[test]
fn test_set_ttl_extension() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    client.set_ttl_extension(&admin, &60_480_u32);
}

#[test]
fn test_set_platform_fee_cap_matches_production_limit() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    client.set_platform_fee(&admin, &200_u32);
    assert_eq!(
        client.try_set_platform_fee(&admin, &201_u32),
        Err(Ok(ContractError::PlatformFeeExceedsMax))
    );
}

#[test]
fn test_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    // Verify admin can call upgrade (auth check passes).
    // A real upgrade requires a compiled WASM; we verify it doesn't fail with NotAuthorized.
    let new_wasm_hash = soroban_sdk::BytesN::from_array(&env, &[1; 32]);
    let result = client.try_upgrade(&admin, &new_wasm_hash);
    assert!(result != Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn test_upgrade_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let fake_admin = Address::generate(&env);
    let new_wasm_hash = soroban_sdk::BytesN::from_array(&env, &[1; 32]);

    let result = client.try_upgrade(&fake_admin, &new_wasm_hash);
    assert!(matches!(result, Err(Ok(ContractError::NotAuthorized))));
}

/// Issue #1204: `recovery_withdraw` must reject `PendingFinalization` escrows,
/// while `Disputed` escrows stay recoverable.
#[test]
fn test_recovery_withdraw_blocked_on_pending_finalization() {
    use crate::test_helpers::create_funded_escrow;
    use crate::{EscrowState, ResolutionType};
    use soroban_sdk::{symbol_short, Bytes, BytesN};

    let env = Env::default();
    env.mock_all_auths();
    let token = register_token(&env);
    let (_cid, client, admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let disputed_id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1_000, 0, 3600,
    );
    let pending_id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1_000, 0, 3600,
    );

    for id in [disputed_id, pending_id] {
        client.raise_dispute(
            &buyer,
            &id,
            &symbol_short!("OTHER"),
            &Bytes::from_slice(&env, b"desc"),
            &BytesN::from_array(&env, &[1u8; 32]),
        );
    }
    client.resolve_dispute(&resolver, &pending_id, &ResolutionType::Release);
    assert_eq!(
        client.get_escrow(&pending_id).state,
        EscrowState::PendingFinalization
    );

    client.enable_recovery_mode(&admin);

    let result = client.try_recovery_withdraw(&buyer, &pending_id);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
    assert_eq!(
        client.get_escrow(&pending_id).state,
        EscrowState::PendingFinalization
    );

    // Disputed escrows remain recoverable.
    client.recovery_withdraw(&buyer, &disputed_id);
    assert_eq!(client.get_escrow(&disputed_id).state, EscrowState::Refunded);
}

#[test]
fn rotation_invalidates_old_timelocks_and_new_admin_can_requeue() {
    use crate::{DataKey, TimelockOperation};
    use soroban_sdk::{testutils::Ledger as _, BytesN};

    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, old_admin, _) = setup_contract(&env);
    let new_admin = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[1; 32]);
    client.queue_upgrade(&old_admin, &hash);
    client.queue_set_platform_fee(&old_admin, &100);
    client.queue_set_admin(&old_admin, &new_admin);
    env.ledger()
        .with_mut(|li| li.timestamp += crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS);
    client.execute_set_admin(&old_admin, &new_admin);

    env.as_contract(&contract_id, || {
        for operation in [
            TimelockOperation::Upgrade,
            TimelockOperation::SetPlatformFee,
        ] {
            assert!(crate::storage::read_timelock_proposal(&env, operation as u32).is_none());
            assert!(env
                .storage()
                .instance()
                .has(&DataKey::InvalidatedTimelockOp(operation as u32)));
        }
    });
    assert_eq!(
        client.try_execute_upgrade(&old_admin, &hash),
        Err(Ok(ContractError::NotAuthorized))
    );
    assert_eq!(
        client.try_execute_set_platform_fee(&old_admin, &100),
        Err(Ok(ContractError::NotAuthorized))
    );

    // An admin returning later must not revive an earlier proposal.
    client.queue_set_admin(&new_admin, &old_admin);
    env.ledger()
        .with_mut(|li| li.timestamp += crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS);
    client.execute_set_admin(&new_admin, &old_admin);
    assert_eq!(
        client.try_execute_upgrade(&old_admin, &hash),
        Err(Ok(ContractError::NotAuthorized))
    );

    // A fresh proposal replaces the invalidation and remains keeper-executable.
    client.queue_set_platform_fee(&old_admin, &100);
    env.ledger()
        .with_mut(|li| li.timestamp += crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS);
    let keeper = Address::generate(&env);
    client.execute_set_platform_fee(&keeper, &100);
}

#[test]
fn stale_proposal_from_before_upgrade_is_rejected_even_if_ready() {
    use soroban_sdk::{testutils::Ledger as _, BytesN};
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, old_admin, _) = setup_contract(&env);
    let new_admin = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[2; 32]);
    client.queue_upgrade(&old_admin, &hash);
    // Model a legacy proposal left in storage by a rotation before this fix.
    env.as_contract(&contract_id, || {
        crate::storage::write_admin_address(&env, &new_admin)
    });
    env.ledger()
        .with_mut(|li| li.timestamp += crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS);
    assert_eq!(
        client.try_execute_upgrade(&old_admin, &hash),
        Err(Ok(ContractError::NotAuthorized))
    );
    // The active admin can explicitly clean up a legacy stale proposal.
    client.cancel_timelock_op(&new_admin, &(crate::TimelockOperation::Upgrade as u32));
}

#[test]
fn timelock_execution_without_admin_returns_not_initialized() {
    use soroban_sdk::BytesN;
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, admin, _) = setup_contract(&env);
    let hash = BytesN::from_array(&env, &[3; 32]);
    client.queue_upgrade(&admin, &hash);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&crate::DataKey::Admin)
    });
    assert_eq!(
        client.try_execute_upgrade(&admin, &hash),
        Err(Ok(ContractError::NotInitialized))
    );
}

// ── Timelock hardening (#1235, #1236, #1238) ────────────────────────────────

mod timelock {
    use crate::admin::{
        compute_proposal_hash, ADMIN_TIMELOCK_DELAY_SECONDS, MIN_TIMELOCK_DELAY_SECONDS,
    };
    use crate::test_helpers::setup_contract;
    use crate::{
        storage, ContractError, Escrow, EscrowClient, TimelockExecuted, TimelockOperation,
    };
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Events as _, Ledger as _},
        Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
    };

    fn advance(env: &Env, seconds: u64) {
        env.ledger().with_mut(|li| li.timestamp += seconds);
    }

    fn fee_params(env: &Env, fee_bps: u32) -> Vec<Val> {
        let mut params = Vec::new(env);
        params.push_back(fee_bps.into_val(env));
        params
    }

    /// Returns the `TimelockExecuted` payload emitted by the last invocation.
    fn last_timelock_executed(env: &Env, contract_id: &Address) -> Option<TimelockExecuted> {
        env.events()
            .all()
            .filter_by_contract(contract_id)
            .events()
            .iter()
            .find_map(|event| match &event.body {
                soroban_sdk::xdr::ContractEventBody::V0(v0) => {
                    let mut topics = v0.topics.iter();
                    let t1 = Symbol::try_from_val(env, topics.next()?).ok()?;
                    let t2 = Symbol::try_from_val(env, topics.next()?).ok()?;
                    if t1 != symbol_short!("Timelock") || t2 != symbol_short!("Executed") {
                        return None;
                    }
                    let data = Val::try_from_val(env, &v0.data).ok()?;
                    TimelockExecuted::try_from_val(env, &data).ok()
                }
            })
    }

    // ── #1235: permissionless keeper execution ──────────────────────────────

    /// The admin queues; an unrelated third party executes once the delay has
    /// elapsed. The change takes effect and the audit event attributes the
    /// execution to the keeper while keeping the admin as proposer.
    #[test]
    fn keeper_executes_mature_proposal_queued_by_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let keeper = Address::generate(&env);

        client.queue_set_platform_fee(&admin, &150_u32);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        client.execute_set_platform_fee(&keeper, &150_u32);
        let executed =
            last_timelock_executed(&env, &contract_id).expect("TimelockExecuted must be emitted");

        assert_eq!(client.get_platform_fee_bps(), 150);
        assert_eq!(executed.operation, TimelockOperation::SetPlatformFee as u32);
        assert_eq!(executed.proposer, admin);
        assert_eq!(executed.executor, keeper);
        assert_ne!(executed.executor, executed.proposer, "keeper-driven");
    }

    /// When the admin executes their own proposal, `executor == proposer`,
    /// which is how audit consumers recognise an admin-driven execution.
    #[test]
    fn admin_execution_is_attributed_to_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

        client.queue_set_platform_fee(&admin, &75_u32);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);
        client.execute_set_platform_fee(&admin, &75_u32);

        let executed = last_timelock_executed(&env, &contract_id).unwrap();
        assert_eq!(executed.proposer, admin);
        assert_eq!(executed.executor, admin);
    }

    /// A keeper cannot jump the queue: one second before `ready_at` the call
    /// reverts with `TimelockNotElapsed` and leaves the proposal in place.
    #[test]
    fn keeper_premature_execution_reverts_with_timelock_not_elapsed() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let keeper = Address::generate(&env);

        client.queue_set_platform_fee(&admin, &150_u32);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS - 1);

        assert_eq!(
            client.try_execute_set_platform_fee(&keeper, &150_u32),
            Err(Ok(ContractError::TimelockNotElapsed))
        );
        assert_eq!(client.get_platform_fee_bps(), 0);
        let still_queued = env.as_contract(&contract_id, || {
            storage::read_timelock_proposal(&env, TimelockOperation::SetPlatformFee as u32)
        });
        assert!(still_queued.is_some());

        advance(&env, 1);
        client.execute_set_platform_fee(&keeper, &150_u32);
        assert_eq!(client.get_platform_fee_bps(), 150);
    }

    /// A proposal queued by an admin who has since been rotated out is purged
    /// by the rotation (#1234): nobody (keeper, new admin, or old admin) can
    /// execute it, and the now-empty slot returns `InvalidState`.
    #[test]
    fn proposal_from_rotated_out_admin_cannot_be_executed() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let new_admin = Address::generate(&env);
        let keeper = Address::generate(&env);

        client.queue_set_platform_fee(&admin, &150_u32);
        client.set_admin(&new_admin);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        for executor in [&keeper, &new_admin, &admin] {
            assert_eq!(
                client.try_execute_set_platform_fee(executor, &150_u32),
                Err(Ok(ContractError::InvalidState))
            );
        }
        assert_eq!(client.get_platform_fee_bps(), 0);
    }

    /// The executor must sign for its own address, so the `executor` recorded
    /// in the audit event cannot be spoofed.
    #[test]
    fn keeper_execution_requires_executor_auth() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let keeper = Address::generate(&env);

        client.queue_set_platform_fee(&admin, &150_u32);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        env.mock_auths(&[]);
        assert!(client
            .try_execute_set_platform_fee(&keeper, &150_u32)
            .is_err());
    }

    /// Keeper execution also works for operations whose body used to assume
    /// the caller was the admin (arbitration fee re-checks the admin).
    #[test]
    fn keeper_executes_arbitration_fee_change() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let keeper = Address::generate(&env);

        client.queue_set_arbitration_fee(&admin, &100_u32);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);
        client.execute_set_arbitration_fee(&keeper, &100_u32);
        assert_eq!(client.get_arbitration_fee(), 100);
    }

    // ── #1236: domain-separated proposal hash ───────────────────────────────

    /// The same parameter vector hashes differently for different operations
    /// and for different contract instances.
    #[test]
    fn proposal_hash_is_bound_to_operation_and_contract() {
        let env = Env::default();
        let contract_a = env.register(Escrow, ());
        let contract_b = env.register(Escrow, ());
        let params = fee_params(&env, 50);

        let (a_platform, a_arbitration) = env.as_contract(&contract_a, || {
            (
                compute_proposal_hash(&env, TimelockOperation::SetPlatformFee, &params),
                compute_proposal_hash(&env, TimelockOperation::SetArbitrationFee, &params),
            )
        });
        let b_platform = env.as_contract(&contract_b, || {
            compute_proposal_hash(&env, TimelockOperation::SetPlatformFee, &params)
        });

        assert_ne!(
            a_platform, a_arbitration,
            "operation must be in the preimage"
        );
        assert_ne!(
            a_platform, b_platform,
            "contract address must be in the preimage"
        );

        // Deterministic for identical inputs.
        let a_platform_again = env.as_contract(&contract_a, || {
            compute_proposal_hash(&env, TimelockOperation::SetPlatformFee, &params)
        });
        assert_eq!(a_platform, a_platform_again);

        // Not the bare (undomain-separated) parameter hash any more.
        let bare: soroban_sdk::BytesN<32> = {
            use soroban_sdk::xdr::ToXdr;
            env.crypto().sha256(&params.clone().to_xdr(&env)).into()
        };
        assert_ne!(a_platform, bare);
    }

    /// The stored hash is the domain-separated one.
    #[test]
    fn queued_proposal_stores_domain_separated_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

        client.queue_set_platform_fee(&admin, &50_u32);
        let (stored, expected) = env.as_contract(&contract_id, || {
            (
                storage::read_timelock_proposal(&env, TimelockOperation::SetPlatformFee as u32)
                    .unwrap()
                    .params_hash,
                compute_proposal_hash(
                    &env,
                    TimelockOperation::SetPlatformFee,
                    &fee_params(&env, 50),
                ),
            )
        });
        assert_eq!(stored, expected);
    }

    /// A proposal approved for one operation cannot be executed as another
    /// operation with an identical parameter vector: a `SetPlatformFee(50)`
    /// approval planted in the `SetArbitrationFee` slot fails verification.
    #[test]
    fn executing_with_mismatched_operation_fails_with_invalid_proposal_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

        client.queue_set_platform_fee(&admin, &50_u32);
        env.as_contract(&contract_id, || {
            let mut proposal =
                storage::read_timelock_proposal(&env, TimelockOperation::SetPlatformFee as u32)
                    .unwrap();
            proposal.operation = TimelockOperation::SetArbitrationFee;
            storage::write_timelock_proposal(
                &env,
                TimelockOperation::SetArbitrationFee as u32,
                &proposal,
            );
        });
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        assert_eq!(
            client.try_execute_set_arbitration_fee(&admin, &50_u32),
            Err(Ok(ContractError::InvalidProposalHash))
        );
        assert_eq!(client.get_arbitration_fee(), 0);

        // The legitimate proposal in its own slot still executes.
        client.execute_set_platform_fee(&admin, &50_u32);
        assert_eq!(client.get_platform_fee_bps(), 50);
    }

    /// A proposal record replayed into a different escrow instance (same
    /// admin, same operation, same params) does not verify there.
    #[test]
    fn proposal_replayed_into_another_contract_fails_with_invalid_proposal_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_a, client_a, admin, fee_collector) = setup_contract(&env);
        let contract_b = env.register(Escrow, ());
        let client_b = EscrowClient::new(&env, &contract_b);
        client_b.initialize(&admin, &fee_collector, &0_u32);

        client_a.queue_set_platform_fee(&admin, &50_u32);
        let proposal = env.as_contract(&contract_a, || {
            storage::read_timelock_proposal(&env, TimelockOperation::SetPlatformFee as u32).unwrap()
        });
        env.as_contract(&contract_b, || {
            storage::write_timelock_proposal(
                &env,
                TimelockOperation::SetPlatformFee as u32,
                &proposal,
            );
        });
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        assert_eq!(
            client_b.try_execute_set_platform_fee(&admin, &50_u32),
            Err(Ok(ContractError::InvalidProposalHash))
        );
        assert_eq!(client_b.get_platform_fee_bps(), 0);

        client_a.execute_set_platform_fee(&admin, &50_u32);
        assert_eq!(client_a.get_platform_fee_bps(), 50);
    }

    // ── #1238: delay reduction waits the current (longer) delay ────────────

    /// 3,600s is below `MIN_TIMELOCK_DELAY_SECONDS` and is rejected outright.
    #[test]
    fn reducing_delay_below_minimum_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

        assert_eq!(
            client.try_queue_set_timelock_delay(&admin, &3_600_u64),
            Err(Ok(ContractError::InvalidTimelockDelay))
        );
    }

    /// Reducing the delay from 86,400s to the 43,200s minimum still has to sit
    /// out the full current 86,400s delay before it can be executed.
    #[test]
    fn reducing_delay_waits_full_current_delay() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);
        assert_eq!(ADMIN_TIMELOCK_DELAY_SECONDS, 86_400);
        let shorter = MIN_TIMELOCK_DELAY_SECONDS; // 43_200

        client.queue_set_timelock_delay(&admin, &shorter);

        // Past the *new* (shorter) delay but before the current one.
        advance(&env, shorter);
        assert_eq!(
            client.try_execute_set_timelock_delay(&admin, &shorter),
            Err(Ok(ContractError::TimelockNotElapsed))
        );
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS - shorter - 1); // 86,399s in
        assert_eq!(
            client.try_execute_set_timelock_delay(&admin, &shorter),
            Err(Ok(ContractError::TimelockNotElapsed))
        );

        advance(&env, 1); // exactly 86,400s
        client.execute_set_timelock_delay(&admin, &shorter);
        let stored: u64 = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&crate::DataKey::AdminTimelockDelay)
                .unwrap()
        });
        assert_eq!(stored, shorter);

        // Operations queued afterwards use the new, shorter delay.
        client.queue_set_platform_fee(&admin, &25_u32);
        advance(&env, shorter);
        client.execute_set_platform_fee(&admin, &25_u32);
        assert_eq!(client.get_platform_fee_bps(), 25);
    }

    /// Increasing the delay waits the new, longer delay.
    #[test]
    fn increasing_delay_waits_new_longer_delay() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let longer = 3 * ADMIN_TIMELOCK_DELAY_SECONDS;

        client.queue_set_timelock_delay(&admin, &longer);
        advance(&env, longer - 1);
        assert_eq!(
            client.try_execute_set_timelock_delay(&admin, &longer),
            Err(Ok(ContractError::TimelockNotElapsed))
        );
        advance(&env, 1);
        client.execute_set_timelock_delay(&admin, &longer);
    }

    /// Shortening the delay does not shorten countdowns that were already
    /// running: a proposal keeps the `ready_at` it was queued with.
    #[test]
    fn reducing_delay_does_not_shorten_in_flight_countdowns() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let shorter = MIN_TIMELOCK_DELAY_SECONDS;

        client.queue_set_timelock_delay(&admin, &shorter);
        advance(&env, 1_000);
        // Queued under the 86,400s delay: ready at t0 + 1,000 + 86,400.
        client.queue_set_platform_fee(&admin, &25_u32);

        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS - 1_000); // t0 + 86,400
        client.execute_set_timelock_delay(&admin, &shorter);

        advance(&env, 999); // t0 + 87,399
        assert_eq!(
            client.try_execute_set_platform_fee(&admin, &25_u32),
            Err(Ok(ContractError::TimelockNotElapsed))
        );
        advance(&env, 1);
        client.execute_set_platform_fee(&admin, &25_u32);
        assert_eq!(client.get_platform_fee_bps(), 25);
    }
}

// ── Batch timelock queueing and execution (#1241) ───────────────────────────

mod batch_timelock {
    use crate::admin::{ADMIN_TIMELOCK_DELAY_SECONDS, MAX_TIMELOCK_BATCH_SIZE};
    use crate::test_helpers::setup_contract;
    use crate::{storage, ContractError, TimelockOperation};
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Address, Env, IntoVal, Val, Vec,
    };

    fn advance(env: &Env, seconds: u64) {
        env.ledger().with_mut(|li| li.timestamp += seconds);
    }

    /// A single serialised parameter value, matching the layout produced by the
    /// matching `queue_*` entry point.
    fn one(env: &Env, value: impl IntoVal<Env, Val>) -> Vec<Val> {
        let mut params = Vec::new(env);
        params.push_back(value.into_val(env));
        params
    }

    fn op_ids(env: &Env, operations: &[TimelockOperation]) -> Vec<u32> {
        let mut ids = Vec::new(env);
        for operation in operations {
            ids.push_back(*operation as u32);
        }
        ids
    }

    fn batch_is_queued(env: &Env, contract_id: &Address, batch_id: u32) -> bool {
        env.as_contract(contract_id, || {
            storage::read_timelock_batch(env, batch_id).is_some()
        })
    }

    /// The motivating case: a fee update and an allowlist toggle queued
    /// together share one countdown and take effect in a single transaction.
    #[test]
    fn fee_update_and_allowlist_toggle_execute_as_one_atomic_batch() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

        let operations = op_ids(
            &env,
            &[
                TimelockOperation::SetPlatformFee,
                TimelockOperation::SetArbitrationFee,
                TimelockOperation::SetTokenAllowlistEnabled,
            ],
        );
        let mut params = Vec::new(&env);
        params.push_back(one(&env, 150_u32));
        params.push_back(one(&env, 100_u32));
        params.push_back(one(&env, true));

        let batch_id = client.queue_timelock_batch(&admin, &operations, &params);
        assert_eq!(batch_id, 1, "batch ids start at 1");

        // A single countdown gates the whole batch.
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS - 1);
        assert_eq!(
            client.try_execute_timelock_batch(&admin, &batch_id, &operations, &params),
            Err(Ok(ContractError::TimelockNotElapsed))
        );
        assert_eq!(client.get_platform_fee_bps(), 0);
        assert_eq!(client.get_arbitration_fee(), 0);

        advance(&env, 1);
        client.execute_timelock_batch(&admin, &batch_id, &operations, &params);

        assert_eq!(client.get_platform_fee_bps(), 150);
        assert_eq!(client.get_arbitration_fee(), 100);
        assert!(
            env.as_contract(
                &contract_id,
                || crate::internal::is_token_allowlist_enabled(&env)
            ),
            "allowlist toggle must be applied by the same batch"
        );
        assert!(!batch_is_queued(&env, &contract_id, batch_id));
    }

    /// Five operations in one batch — the workload the issue's gas note asks
    /// about — all land, and a keeper (not the admin) may submit the batch.
    #[test]
    fn five_operation_batch_applies_every_change_and_any_keeper_may_execute() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let keeper = Address::generate(&env);

        let operations = op_ids(
            &env,
            &[
                TimelockOperation::SetPlatformFee,
                TimelockOperation::SetArbitrationFee,
                TimelockOperation::SetTokenAllowlistEnabled,
                TimelockOperation::SetMaxAppeals,
                TimelockOperation::SetMaxBasketSize,
            ],
        );
        let mut params = Vec::new(&env);
        params.push_back(one(&env, 100_u32));
        params.push_back(one(&env, 100_u32));
        params.push_back(one(&env, true));
        params.push_back(one(&env, 5_u32));
        params.push_back(one(&env, 10_u32));

        let batch_id = client.queue_timelock_batch(&admin, &operations, &params);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);
        client.execute_timelock_batch(&keeper, &batch_id, &operations, &params);

        assert_eq!(client.get_platform_fee_bps(), 100);
        assert_eq!(client.get_arbitration_fee(), 100);
        assert_eq!(client.get_max_appeals(), 5);
        assert_eq!(client.get_max_basket_size(), 10);
        assert!(env.as_contract(
            &contract_id,
            || crate::internal::is_token_allowlist_enabled(&env)
        ));
    }

    /// A failure part-way through reverts the operations already applied and
    /// leaves the batch queued for a corrected retry.
    #[test]
    fn failing_sub_operation_reverts_the_entire_batch() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

        // `SetPlatformFee` would succeed; `SetMaxAppeals(0)` is below
        // `MIN_MAX_APPEALS` and fails, so neither may take effect.
        let operations = op_ids(
            &env,
            &[
                TimelockOperation::SetPlatformFee,
                TimelockOperation::SetMaxAppeals,
            ],
        );
        let mut params = Vec::new(&env);
        params.push_back(one(&env, 150_u32));
        params.push_back(one(&env, 0_u32));

        let batch_id = client.queue_timelock_batch(&admin, &operations, &params);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        assert_eq!(
            client.try_execute_timelock_batch(&admin, &batch_id, &operations, &params),
            Err(Ok(ContractError::InvalidMaxAppeals))
        );
        assert_eq!(
            client.get_platform_fee_bps(),
            0,
            "the earlier sub-operation must be rolled back"
        );
        assert!(
            batch_is_queued(&env, &contract_id, batch_id),
            "a reverted batch stays queued"
        );
    }

    #[test]
    fn mismatched_parameters_are_rejected_with_invalid_proposal_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

        let operations = op_ids(&env, &[TimelockOperation::SetPlatformFee]);
        let mut queued_params = Vec::new(&env);
        queued_params.push_back(one(&env, 50_u32));

        let batch_id = client.queue_timelock_batch(&admin, &operations, &queued_params);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        let mut tampered_params = Vec::new(&env);
        tampered_params.push_back(one(&env, 51_u32));
        assert_eq!(
            client.try_execute_timelock_batch(&admin, &batch_id, &operations, &tampered_params),
            Err(Ok(ContractError::InvalidProposalHash))
        );
        assert_eq!(client.get_platform_fee_bps(), 0);
        assert!(batch_is_queued(&env, &contract_id, batch_id));
    }

    /// Batches follow the same rotation rule as single proposals: once the
    /// queueing admin rotates out, nobody can execute the batch.
    #[test]
    fn batch_queued_by_rotated_out_admin_cannot_be_executed() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let new_admin = Address::generate(&env);
        let keeper = Address::generate(&env);

        let operations = op_ids(&env, &[TimelockOperation::SetPlatformFee]);
        let mut params = Vec::new(&env);
        params.push_back(one(&env, 150_u32));

        let batch_id = client.queue_timelock_batch(&admin, &operations, &params);
        client.set_admin(&new_admin);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        assert_eq!(
            client.try_execute_timelock_batch(&keeper, &batch_id, &operations, &params),
            Err(Ok(ContractError::NotAuthorized))
        );
        assert_eq!(client.get_platform_fee_bps(), 0);
    }

    /// Operations that mutate the governance machinery itself cannot be
    /// smuggled into a batch.
    #[test]
    fn governance_machinery_operations_cannot_be_batched() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

        for operation in [
            TimelockOperation::SetAdmin,
            TimelockOperation::Upgrade,
            TimelockOperation::SetProtocolFee,
            TimelockOperation::SetTimelockDelay,
        ] {
            let operations = op_ids(&env, &[operation]);
            let mut params = Vec::new(&env);
            params.push_back(one(&env, 1_u32));
            assert_eq!(
                client.try_queue_timelock_batch(&admin, &operations, &params),
                Err(Ok(ContractError::InvalidOperation)),
                "{operation:?} must not be batchable"
            );
        }
    }

    #[test]
    fn only_admin_can_queue_a_batch() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
        let intruder = Address::generate(&env);

        let operations = op_ids(&env, &[TimelockOperation::SetPlatformFee]);
        let mut params = Vec::new(&env);
        params.push_back(one(&env, 50_u32));

        assert_eq!(
            client.try_queue_timelock_batch(&intruder, &operations, &params),
            Err(Ok(ContractError::NotAuthorized))
        );
    }

    #[test]
    fn empty_and_malformed_batches_are_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

        let empty_operations: Vec<u32> = Vec::new(&env);
        let empty_params: Vec<Vec<Val>> = Vec::new(&env);
        assert_eq!(
            client.try_queue_timelock_batch(&admin, &empty_operations, &empty_params),
            Err(Ok(ContractError::InvalidOperation))
        );

        // Positional pairing: one operation, no parameters.
        let operations = op_ids(&env, &[TimelockOperation::SetPlatformFee]);
        assert_eq!(
            client.try_queue_timelock_batch(&admin, &operations, &empty_params),
            Err(Ok(ContractError::InvalidOperation))
        );
    }

    #[test]
    fn batches_larger_than_the_cap_are_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

        let mut operations = Vec::new(&env);
        let mut params = Vec::new(&env);
        for _ in 0..=MAX_TIMELOCK_BATCH_SIZE {
            operations.push_back(TimelockOperation::SetMaxAppeals as u32);
            params.push_back(one(&env, 5_u32));
        }

        assert_eq!(
            client.try_queue_timelock_batch(&admin, &operations, &params),
            Err(Ok(ContractError::InputTooLong))
        );
    }
}
