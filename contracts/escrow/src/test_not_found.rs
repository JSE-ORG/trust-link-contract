#![cfg(test)]

use crate::test_helpers::setup_contract;
use crate::{ContractError, Payee};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, IntoVal, Symbol, Vec,
};

const MISSING_ID: u64 = 999_999;

#[test]
fn test_get_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let res = client.try_get_escrow(&MISSING_ID);
    assert!(matches!(res, Err(Ok(ContractError::EscrowNotFound))));
}

/// A fresh contract has created no escrows, so any randomly chosen ID must
/// return a clean `EscrowNotFound` error instead of panicking (#458).
#[test]
fn test_get_escrow_random_non_existent_ids() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    // A spread of arbitrary IDs across the u64 range, plus both boundaries.
    let random_ids: [u64; 8] = [0, 1, 7, 42, 1_000, 123_456_789, 9_876_543_210, u64::MAX];

    for id in random_ids {
        let res = client.try_get_escrow(&id);
        assert!(
            matches!(res, Err(Ok(ContractError::EscrowNotFound))),
            "get_escrow({id}) must return EscrowNotFound, not panic",
        );
    }
}

#[test]
fn test_fund_escrow_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let buyer = Address::generate(&env);
    let res = client.try_fund_escrow(&MISSING_ID, &buyer);
    assert!(matches!(res, Err(Ok(ContractError::EscrowNotFound))));
}

#[test]
fn test_mark_shipped_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let res = client.try_mark_shipped(
        &seller,
        &MISSING_ID,
        &soroban_sdk::String::from_str(&env, "TRACK"),
    );
    assert!(matches!(res, Err(Ok(ContractError::EscrowNotFound))));
}

#[test]
fn test_confirm_delivery_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let buyer = Address::generate(&env);
    let res = client.try_confirm_delivery(&buyer, &MISSING_ID, &false);
    assert!(matches!(res, Err(Ok(ContractError::EscrowNotFound))));
}

#[test]
fn test_raise_dispute_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let buyer = Address::generate(&env);
    let res = client.try_raise_dispute(
        &buyer,
        &MISSING_ID,
        &Symbol::new(&env, "OTHER"),
        &soroban_sdk::Bytes::from_slice(&env, b"desc"),
        &BytesN::from_array(&env, &[1u8; 32]),
    );
    assert!(matches!(res, Err(Ok(ContractError::EscrowNotFound))));
}

#[test]
fn test_resolve_dispute_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let resolver = Address::generate(&env);
    let res = client.try_resolve_dispute(&resolver, &MISSING_ID, &crate::ResolutionType::Release);
    assert!(matches!(res, Err(Ok(ContractError::EscrowNotFound))));
}

/// Settlement with a non-zero protocol fee must fail gracefully when no fee
/// collector is configured (#1258).
#[test]
fn settlement_without_fee_collector_returns_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &10_000_i128);

    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    client.set_protocol_fee(&admin, &100_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let id = client.create_escrow_8(
        &payees.into_val(&env),
        &Some(buyer.clone()),
        &resolver,
        &token,
        &1_000_i128,
        &100_u32,
        &604_800_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-1258"),
    );

    env.as_contract(&contract_id, || {
        crate::storage::update_global_config(&env, |c| c.fee_collector = None);
    });

    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.dispute_deadline() + 1);

    let res = client.try_confirm_delivery(&buyer, &id, &false);
    assert!(
        matches!(res, Err(Ok(ContractError::NotInitialized))),
        "confirm_delivery must return NotInitialized when fee collector is unset, got {res:?}",
    );
}