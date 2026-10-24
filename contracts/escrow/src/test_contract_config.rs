#![cfg(test)]

use crate::{Escrow, EscrowClient, Payee};
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, Vec};

#[test]
fn test_get_public_config() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let arbitration_fee_bps = 500_u32;

    client.initialize(&admin, &fee_collector, &arbitration_fee_bps);

    let mut public = client.get_public_config();
    assert_eq!(public.fee_bps, 0);
    assert!(!public.paused);
    assert_eq!(public.escrow_count, 0);

    client.set_fee(&admin, &150_u32);
    public = client.get_public_config();
    assert_eq!(public.fee_bps, 150);

    // Update fee and check again
    client.set_protocol_fee(&admin, &150);
    let config = client.get_contract_config();
    assert_eq!(config.fee_bps, 150);

    // Create an escrow to increment the counter
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = Address::generate(&env);

    let mut payees_14 = Vec::new(&env);
    payees_14.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_14_val = payees_14.into_val(&env);
    client.create_escrow_8(
        &payees_14_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_0000000,
        &100,
        &86400,
    );

    public = client.get_public_config();
    assert_eq!(public.escrow_count, 1);

    client.pause_contract(&admin);
    public = client.get_public_config();
    assert!(public.paused);
}

#[test]
fn test_get_contract_config_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let arbitration_fee = 500_u32;

    client.initialize(&admin, &fee_collector, &arbitration_fee);

    let mut config = client.get_contract_config();
    assert_eq!(config.admin, admin);
    assert_eq!(config.fee_bps, 0);
    assert_eq!(config.fee_collector, fee_collector);
    assert_eq!(config.escrow_count, 0);

    client.set_fee(&admin, &150_u32);
    config = client.get_contract_config();
    assert_eq!(config.fee_bps, 150);

    let new_admin = Address::generate(&env);
    client.set_admin(&new_admin);
    config = client.get_contract_config();
    assert_eq!(config.admin, new_admin);
}

#[test]
fn test_global_config_defaults_match_lib_constants() {
    // Issue #1109: Ensure global config min/max defaults match lib.rs constants.
    // The global config is the canonical source for amount validation, and its
    // defaults must equal the constants documented in lib.rs to avoid drift.
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let config = client.get_contract_config();
    assert_eq!(
        config.min_amount,
        crate::MIN_ESCROW_AMOUNT,
        "Global config min_amount default must match lib.rs MIN_ESCROW_AMOUNT constant"
    );
    assert_eq!(
        config.max_amount,
        crate::MAX_ESCROW_AMOUNT,
        "Global config max_amount default must match lib.rs MAX_ESCROW_AMOUNT constant"
    );
}
