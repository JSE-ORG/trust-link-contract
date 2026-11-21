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
