#![cfg(test)]
//! `set_fee_collector` must reject the all-zero (empty) Stellar account address
//! and report a clear `ContractError::InvalidAddress` rather than persisting a
//! collector that can never sign for or receive fee withdrawals (#434).

use crate::test_helpers::setup_contract;
use crate::{ContractError, DataKey};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

/// Strkey for the all-zero ed25519 public key — the canonical "empty" address.
const ZERO_ADDRESS_STRKEY: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

fn zero_address(env: &Env) -> Address {
    Address::from_string(&String::from_str(env, ZERO_ADDRESS_STRKEY))
}

#[test]
fn set_fee_collector_rejects_zero_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, fee_collector) = setup_contract(&env);

    let res = client.try_set_fee_collector(&zero_address(&env));
    assert_eq!(
        res,
        Err(Ok(ContractError::InvalidAddress)),
        "set_fee_collector must reject the zero address with InvalidAddress",
    );

    // The collector must be unchanged after the rejected call.
    let stored: Address = env
        .as_contract(&client.address, || {
            env.storage().instance().get(&DataKey::FeeCollector)
        })
        .expect("fee collector still set");
    assert_eq!(stored, fee_collector);
}

#[test]
fn set_fee_collector_accepts_a_valid_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let new_collector = Address::generate(&env);
    client.set_fee_collector(&new_collector);

    let stored: Address = env
        .as_contract(&client.address, || {
            env.storage().instance().get(&DataKey::FeeCollector)
        })
        .expect("fee collector set");
    assert_eq!(stored, new_collector);
}
