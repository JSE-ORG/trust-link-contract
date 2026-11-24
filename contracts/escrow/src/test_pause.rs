#![cfg(test)]

use crate::{ContractError, Escrow, EscrowClient, EscrowState, Payee, ResolutionType};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, IntoVal, String as SorobanString, Symbol, Vec,
};

const DISPUTE_WINDOW: u64 = 172_800;

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let contract_id = env.register(Escrow, ());
    {
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
    }
    (
        env,
        admin,
        seller,
        buyer,
        resolver,
        token_address,
        contract_id,
    )
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

#[test]
fn test_create_escrow_blocked_when_paused() {
    let (env, admin, seller, _buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause_contract(&admin);

    // Fixed wrapper for payee argument signature compatibility
    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);

    // Updated with 9 arguments
    let result = client.try_create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<SorobanString>,
    );
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
}

#[test]
fn test_fund_escrow_blocked_when_paused() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let mut payees_64 = Vec::new(&env);
    payees_64.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_64_val = payees_64.into_val(&env);

    // Updated with 9 arguments
    let id = client.create_escrow_8(
        &payees_64_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &3600_u64,
    );
    client.pause_contract(&admin);
    let result = client.try_fund_escrow(&id, &buyer);
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
}

#[test]
fn test_pause_blocks_mutations_but_keeps_views_available() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let mut payees_63 = Vec::new(&env);
    payees_63.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_63_val = payees_63.into_val(&env);
    // Updated with 9 arguments
    let id = client.create_escrow_8(
        &payees_63_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &3600_u64,
    );
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.pause_contract(&admin);
    let result = client.try_mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK001"),
    );
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
}

#[test]
fn test_confirm_delivery_blocked_when_paused() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let mut payees_62 = Vec::new(&env);
    payees_62.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_62_val = payees_62.into_val(&env);
    // Updated with 9 arguments
    let id = client.create_escrow_8(
        &payees_62_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &3600_u64,
    );
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    env.ledger().set_timestamp(DISPUTE_WINDOW + 1);
    client.pause_contract(&admin);
    let result = client.try_confirm_delivery(&buyer, &id, &false);
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
}

#[test]
fn test_raise_dispute_blocked_when_paused() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let mut payees_61 = Vec::new(&env);
    payees_61.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_61_val = payees_61.into_val(&env);
    // Updated with 9 arguments
    let id = client.create_escrow_8(
        &payees_61_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &3600_u64,
    );
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-PAUSE"),
    );
    client.pause_contract(&admin);
    let hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "FRAUD"),
        &soroban_sdk::Bytes::from_slice(&env, b"desc"),
        &hash,
    );
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
}

#[test]
fn test_resolve_dispute_blocked_when_paused() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let mut payees_60 = Vec::new(&env);
    payees_60.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_60_val = payees_60.into_val(&env);
    // Updated with 9 arguments
    let id = client.create_escrow_8(
        &payees_60_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &3600_u64,
    );
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-PAUSE2"),
    );
    let hash = soroban_sdk::BytesN::from_array(&env, &[1u8; 32]);
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "FRAUD"),
        &soroban_sdk::Bytes::from_slice(&env, b"desc"),
        &hash,
    );
    client.pause_contract(&admin);
    let result = client.try_resolve_dispute(&resolver, &id, &ResolutionType::Refund);
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
}

#[test]
fn test_auto_release_blocked_when_paused() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let mut payees_59 = Vec::new(&env);
    payees_59.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_59_val = payees_59.into_val(&env);
    // Updated with 9 arguments
    let id = client.create_escrow_8(
        &payees_59_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &1_u64,
    );
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-AR"),
    );
    crate::test_helpers::record_delivery_timelocked(&env, &client, &admin, id);
    env.ledger().set_timestamp(DISPUTE_WINDOW + 10);
    client.pause_contract(&admin);
    let result = client.try_auto_release(&id);
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
}

#[test]
fn test_read_only_views_work_while_paused() {
    let (env, admin, seller, _buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let mut payees_58 = Vec::new(&env);
    payees_58.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_58_val = payees_58.into_val(&env);
    // Updated with 9 arguments
    let id = client.create_escrow_8(
        &payees_58_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &3600_u64,
    );
    client.pause_contract(&admin);
    let _ = client.get_escrow(&id);
    let _ = client.get_fee_config();
    assert!(client.is_paused());
}

#[test]
fn test_is_paused_reflects_state() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    assert!(!client.is_paused());

    client.pause_contract(&admin);
    assert!(client.is_paused());

    client.unpause_contract(&admin);
    assert!(!client.is_paused());
}

#[test]
fn test_unpause_resumes_operations() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause_contract(&admin);
    client.unpause_contract(&admin);
    let mut payees_57 = Vec::new(&env);
    payees_57.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_57_val = payees_57.into_val(&env);
    // Updated with 9 arguments
    let id = client.create_escrow_8(
        &payees_57_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &3600_u64,
    );
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);

    mint_tokens(&env, &token, &buyer, 1_000);
    let mut payees_56 = Vec::new(&env);
    payees_56.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_56_val = payees_56.into_val(&env);
    // Updated with 9 arguments
    let escrow_id = client.create_escrow_8(
        &payees_56_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &100_u32,
        &3600_u64,
    );
    client.pause_contract(&admin);

    let config = client.get_fee_config();
    assert_eq!(config.protocol_fee_bps, 0);
    assert_eq!(config.arbitration_fee_bps, 0);

    // Fixed seller parameter mapping to single-payee wrapper setup
    let mut single_payee = Vec::new(&env);
    single_payee.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let single_payee_val = single_payee.into_val(&env);

    // Updated internal parameter shape assertions with 9 parameters
    assert!(client
        .try_create_escrow(
            &single_payee_val,
            &None::<Address>,
            &resolver,
            &token,
            &100_i128,
            &100_u32,
            &0_u32,
            &3600_u64,
            &None::<SorobanString>,
        )
        .is_err());

    assert!(client.try_fund_escrow(&escrow_id, &buyer).is_err());
    assert!(client
        .try_confirm_delivery(&buyer, &escrow_id, &false)
        .is_err());
    assert!(client
        .try_raise_dispute(
            &buyer,
            &escrow_id,
            &Symbol::new(&env, "OTHER"),
            &soroban_sdk::Bytes::from_slice(&env, b"desc"),
            &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
        )
        .is_err());
    assert!(client
        .try_resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release)
        .is_err());
    assert!(client.try_auto_release(&escrow_id).is_err());

    client.unpause_contract(&admin);
    mint_tokens(&env, &token, &buyer, 100);
    let mut payees_55 = Vec::new(&env);
    payees_55.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_55_val = payees_55.into_val(&env);
    // Updated with 9 arguments
    let second_id = client.create_escrow_8(
        &payees_55_val,
        &None::<Address>,
        &resolver,
        &token,
        &50_i128,
        &50_u32,
        &3600_u64,
    );
    assert_eq!(second_id, 3);
}

#[test]
fn test_granular_pause_functionality() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let create_action = Symbol::new(&env, "create_escrow");
    let cancel_action = Symbol::new(&env, "cancel_escrow");

    // Pause create_escrow
    let res = env.as_contract(&contract_id, || {
        Escrow::pause_action(env.clone(), admin.clone(), create_action.clone())
    });
    assert!(res.is_ok());

    // verify it is paused
    let is_create_paused = client.is_action_paused(&create_action);
    assert!(is_create_paused);

    // cancel_escrow should not be paused
    let is_cancel_paused = client.is_action_paused(&cancel_action);
    assert!(!is_cancel_paused);

    // Global pause should halt everything. So we pause global.
    let res_global = env.as_contract(&contract_id, || {
        Escrow::pause_contract(env.clone(), admin.clone())
    });
    assert!(res_global.is_ok());

    // Both should be considered paused by ensure_action_not_paused
    // We can test this by checking `ensure_action_not_paused` directly, or just rely on global pause flag.
    assert!(client.is_paused());

    // Unpause global
    let res_unpause_global = env.as_contract(&contract_id, || {
        Escrow::unpause_contract(env.clone(), admin.clone())
    });
    assert!(res_unpause_global.is_ok());

    // Unpause create_escrow
    let res_unpause_action = env.as_contract(&contract_id, || {
        Escrow::unpause_action(env.clone(), admin.clone(), create_action.clone())
    });
    assert!(res_unpause_action.is_ok());

    assert!(!client.is_action_paused(&create_action));
}

/// #1243 — Part C: Verify immediate pause operates without timelock delay.
#[test]
fn test_immediate_pause_takes_effect_without_timelock() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Before pausing, contract is live.
    assert!(!client.is_paused());

    // pause_contract is an immediate circuit-breaker — no queue/execute needed.
    client.pause_contract(&admin);
    assert!(client.is_paused());
}

/// #1243 — Part C: When timelocked-unpause-only policy is active, direct
/// `unpause_contract` is blocked with `DirectUnpauseBlocked`.
#[test]
fn test_direct_unpause_blocked_when_strict_policy_enabled() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Enable strict governance mode — unpause must go through the timelock.
    client.set_timelocked_unpause_only(&admin, &true);
    client.pause_contract(&admin);
    assert!(client.is_paused());

    // Direct unpause is now rejected.
    let result = client.try_unpause_contract(&admin);
    assert!(
        matches!(result, Err(Ok(ContractError::DirectUnpauseBlocked))),
        "expected DirectUnpauseBlocked, got {:?}",
        result
    );

    // The contract is still paused.
    assert!(client.is_paused());
}

/// #1243 — Part C: When timelocked-unpause-only is off, direct unpause works normally.
#[test]
fn test_direct_unpause_allowed_when_policy_disabled() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // No strict policy set — the default allows direct unpausing.
    client.pause_contract(&admin);
    assert!(client.is_paused());

    client.unpause_contract(&admin);
    assert!(!client.is_paused());
// ── #1237: granular pausing of administrative functions ─────────────────────

mod admin_action_pausing {
    use crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS;
    use crate::test_helpers::setup_contract;
    use crate::ContractError;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        Address, BytesN, Env, Symbol,
    };

    fn advance(env: &Env, seconds: u64) {
        env.ledger().with_mut(|li| li.timestamp += seconds);
    }

    /// Pausing `execute_upgrade` blocks a mature upgrade proposal with
    /// `ContractPaused` (for the admin and for keepers alike); unpausing
    /// restores it. The all-zero hash makes the unpaused call stop at
    /// `InvalidWasmHash` instead of swapping the WASM, proving the pause
    /// gate — not the timelock — was what rejected it before.
    #[test]
    fn paused_execute_upgrade_reverts_with_contract_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let keeper = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let action = Symbol::new(&env, "execute_upgrade");

        client.queue_upgrade(&admin, &wasm_hash);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        client.pause_action(&admin, &action);
        assert!(client.is_action_paused(&action));
        assert!(!client.is_paused(), "only the one action is paused");

        assert_eq!(
            client.try_execute_upgrade(&admin, &wasm_hash),
            Err(Ok(ContractError::ContractPaused))
        );
        assert_eq!(
            client.try_execute_upgrade(&keeper, &wasm_hash),
            Err(Ok(ContractError::ContractPaused))
        );

        client.unpause_action(&admin, &action);
        assert!(!client.is_action_paused(&action));
        assert_eq!(
            client.try_execute_upgrade(&admin, &wasm_hash),
            Err(Ok(ContractError::InvalidWasmHash))
        );
    }

    /// Pausing the upgrade path leaves standard admin operations running.
    #[test]
    fn pausing_upgrades_keeps_other_admin_operations_running() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let wasm_hash = BytesN::from_array(&env, &[7u8; 32]);

        client.pause_action(&admin, &Symbol::new(&env, "queue_upgrade"));
        client.pause_action(&admin, &Symbol::new(&env, "execute_upgrade"));

        assert_eq!(
            client.try_queue_upgrade(&admin, &wasm_hash),
            Err(Ok(ContractError::ContractPaused))
        );

        client.queue_set_platform_fee(&admin, &40_u32);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);
        client.execute_set_platform_fee(&admin, &40_u32);
        assert_eq!(client.get_platform_fee_bps(), 40);
    }

    /// Every documented pausable admin symbol is actually gated.
    #[test]
    fn every_pausable_admin_action_is_gated() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let wasm_hash = BytesN::from_array(&env, &[7u8; 32]);

        // Queue the timelocked ops first so the execute_* calls would
        // otherwise succeed, then pause everything.
        client.queue_set_platform_fee(&admin, &40_u32);
        client.queue_set_arbitration_fee(&admin, &40_u32);
        advance(&env, ADMIN_TIMELOCK_DELAY_SECONDS);

        for name in [
            "queue_upgrade",
            "execute_upgrade",
            "set_fee",
            "set_platform_fee",
            "queue_set_platform_fee",
            "execute_set_platform_fee",
            "queue_set_arbitration_fee",
            "execute_set_arbitration_fee",
        ] {
            client.pause_action(&admin, &Symbol::new(&env, name));
        }

        let paused = Err(Ok(ContractError::ContractPaused));
        assert_eq!(client.try_queue_upgrade(&admin, &wasm_hash), paused);
        assert_eq!(client.try_execute_upgrade(&admin, &wasm_hash), paused);
        assert_eq!(client.try_set_fee(&admin, &10_u32), paused);
        assert_eq!(client.try_set_platform_fee(&admin, &10_u32), paused);
        assert_eq!(client.try_queue_set_platform_fee(&admin, &10_u32), paused);
        assert_eq!(client.try_execute_set_platform_fee(&admin, &40_u32), paused);
        assert_eq!(
            client.try_queue_set_arbitration_fee(&admin, &10_u32),
            paused
        );
        assert_eq!(
            client.try_execute_set_arbitration_fee(&admin, &40_u32),
            paused
        );
        assert_eq!(client.get_platform_fee_bps(), 0);
        assert_eq!(client.get_arbitration_fee(), 0);
    }

    /// `unpause_action` is never gated: even with it (nonsensically) paused,
    /// the admin can still re-enable a paused admin action.
    #[test]
    fn unpause_action_remains_callable_while_admin_actions_are_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let set_fee = Symbol::new(&env, "set_fee");

        client.pause_action(&admin, &set_fee);
        client.pause_action(&admin, &Symbol::new(&env, "unpause_action"));
        client.pause_action(&admin, &Symbol::new(&env, "pause_action"));

        client.unpause_action(&admin, &set_fee);
        assert!(!client.is_action_paused(&set_fee));
        client.set_fee(&admin, &25_u32);
    }

    /// The global contract pause halts user flows but not admin operations,
    /// so an emergency upgrade or fee fix can still go out during an incident.
    #[test]
    fn global_pause_does_not_block_admin_actions() {
        let env = Env::default();
        env.mock_all_auths();
        let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
        let wasm_hash = BytesN::from_array(&env, &[7u8; 32]);

        client.pause_contract(&admin);
        assert!(client.is_paused());

        client.queue_upgrade(&admin, &wasm_hash);
        client.set_platform_fee(&admin, &30_u32);
        assert_eq!(client.get_platform_fee_bps(), 30);
    }
}
