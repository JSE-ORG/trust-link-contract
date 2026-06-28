#![cfg(test)]

use crate::test_helpers::setup_contract;
use crate::ContractError;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String as SorobanString, Symbol};

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
        &SorobanString::from_str(&env, "TRACK"),
    );
    assert!(matches!(res, Err(Ok(ContractError::EscrowNotFound))));
}

#[test]
fn test_confirm_delivery_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let buyer = Address::generate(&env);
    let res = client.try_confirm_delivery(&buyer, &MISSING_ID);
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
        &Symbol::new(&env, "reason"),
        &SorobanString::from_str(&env, "desc"),
        &BytesN::from_array(&env, &[0u8; 32]),
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
