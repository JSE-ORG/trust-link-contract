#![cfg(test)]
//! Calling `initialize` a second time must revert with `AlreadyInitialized`
//! and leave the storage values from the first call intact (#14).

use crate::{ContractError, DataKey, Escrow, EscrowClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn deploy_and_init(env: &Env) -> (EscrowClient, Address, Address) {
    env.mock_all_auths();
    let admin_a = Address::generate(env);
    let fee_collector_a = Address::generate(env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin_a, &100_u32, &fee_collector_a).unwrap();
    (client, admin_a, fee_collector_a)
}

#[test]
fn second_initialize_reverts_with_already_initialized() {
    let env = Env::default();
    let (client, _admin_a, _fc_a) = deploy_and_init(&env);
    let admin_b = Address::generate(&env);
    let fee_collector_b = Address::generate(&env);
    let res = client.try_initialize(&admin_b, &100_u32, &fee_collector_b);
    assert!(matches!(res, Err(Ok(ContractError::AlreadyInitialized))));
}

#[test]
fn storage_from_the_first_initialize_is_unchanged_after_a_failed_second_call() {
    let env = Env::default();
    let (client, admin_a, fee_collector_a) = deploy_and_init(&env);
    let admin_b = Address::generate(&env);
    let fee_collector_b = Address::generate(&env);

    let res = client.try_initialize(&admin_b, &100_u32, &fee_collector_b);
    assert!(matches!(res, Err(Ok(ContractError::AlreadyInitialized))));

    let stored_admin: Address = env
        .as_contract(&client.address, || env.storage().instance().get(&DataKey::Admin))
        .expect("admin set");
    let stored_collector: Address = env
        .as_contract(&client.address, || env.storage().instance().get(&DataKey::FeeCollector))
        .expect("fee collector set");

    assert_eq!(stored_admin, admin_a);
    assert_eq!(stored_collector, fee_collector_a);
}

#[test]
fn initialize_with_fee_bps_exceeding_max_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let res = client.try_initialize(&admin, &301_u32, &fee_collector);
    assert!(matches!(res, Err(Ok(ContractError::FeeExceedsMax))));
}

#[test]
fn initialize_sets_counter_to_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &0_u32, &fee_collector).unwrap();

    let counter: u64 = env
        .as_contract(&contract_id, || {
            env.storage().instance().get(&DataKey::EscrowCounter)
        })
        .expect("counter set");
    assert_eq!(counter, 0);
}
