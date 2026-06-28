#![cfg(test)]

use crate::test_helpers::setup_contract;
use crate::types::{EscrowState, EscrowInput};
use crate::{ContractError, EscrowClient};
use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{token, Address, Env};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env.register_stellar_asset_contract(token_admin.clone());

    (
        env,
        admin,
        seller,
        buyer,
        resolver,
        token_address,
        fee_collector,
    )
}

#[test]
fn test_batch_create_success() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut escrows = soroban_sdk::Vec::new(&env);
    let amount1 = 1000;
    let amount2 = 2000;

    escrows.push_back(EscrowInput {
        buyer: Some(buyer.clone()),
        resolver: resolver.clone(),
        token: token.clone(),
        amount: amount1,
        fee_bps: 100,
        shipping_window: 3600,
        notes: None,
    });

    escrows.push_back(EscrowInput {
        buyer: None,
        resolver: resolver.clone(),
        token: token.clone(),
        amount: amount2,
        fee_bps: 100,
        shipping_window: 3600,
        notes: None,
    });

    let ids = client.batch_create_escrow(&seller, &escrows);
    assert_eq!(ids.len(), 2);
    
    // Check first escrow
    let escrow1 = client.get_escrow(&ids.get(0).unwrap());
    assert_eq!(escrow1.amount, amount1);
    assert_eq!(escrow1.buyer, Some(buyer.clone()));

    // Check second escrow
    let escrow2 = client.get_escrow(&ids.get(1).unwrap());
    assert_eq!(escrow2.amount, amount2);
    assert_eq!(escrow2.buyer, None);
}

#[test]
fn test_batch_create_partial_failure() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut escrows = soroban_sdk::Vec::new(&env);

    // Valid escrow
    escrows.push_back(EscrowInput {
        buyer: Some(buyer.clone()),
        resolver: resolver.clone(),
        token: token.clone(),
        amount: 1000,
        fee_bps: 100,
        shipping_window: 3600,
        notes: None,
    });

    // Invalid escrow (amount = 0)
    escrows.push_back(EscrowInput {
        buyer: None,
        resolver: resolver.clone(),
        token: token.clone(),
        amount: 0,
        fee_bps: 100,
        shipping_window: 3600,
        notes: None,
    });

    let res = client.try_batch_create_escrow(&seller, &escrows);
    assert_eq!(res, Err(Ok(ContractError::InvalidAmount)));
}
