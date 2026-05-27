#![cfg(test)]

use crate::{ContractError, Escrow, EscrowClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    client.initialize(&admin, &fee_collector, &0_i128);

    let attacker = Address::generate(&env);
    let result = client.try_initialize(&attacker, &fee_collector, &0_i128);
    assert!(result.is_err());
}

#[test]
fn test_set_fee_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_i128);

    let result = client.try_set_fee(&200_u32);
    assert!(result.is_ok());
}

#[test]
fn test_set_fee_exceeds_max_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_i128);

    let result = client.try_set_fee(&500_u32);
    assert!(matches!(result, Err(Ok(ContractError::FeeExceedsMax))));
}
