#![cfg(test)]

use crate::{ContractError, Escrow, EscrowClient, Payee, MIN_ESCROW_AMOUNT};
use soroban_sdk::{testutils::Address as _, token, Address, Env, IntoVal, String, Vec};

fn setup(env: &Env) -> (Address, Address, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let fee_collector = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    (admin, seller, buyer, resolver, fee_collector, token)
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn single_payee(env: &Env, address: &Address) -> Vec<Payee> {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}

/// Verify that creating an escrow with zero amount throws an error.
#[test]
fn test_create_escrow_zero_amount_fails() {
    let env = Env::default();
    let (admin, seller, _buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let result = client.try_create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &0_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
}

/// Verify that creating an escrow below the minimum throws an error.
#[test]
fn test_create_escrow_below_minimum_fails() {
    let env = Env::default();
    let (admin, seller, _buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let below_minimum = MIN_ESCROW_AMOUNT - 1;
    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let result = client.try_create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &below_minimum,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
}

/// Verify that creating an escrow exactly at the minimum succeeds.
#[test]
fn test_create_escrow_at_minimum_succeeds() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    mint(&env, &token, &buyer, MIN_ESCROW_AMOUNT);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let result = client.try_create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &MIN_ESCROW_AMOUNT,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert!(matches!(result, Ok(_)));
}

/// Verify that creating an escrow above the minimum succeeds.
#[test]
fn test_create_escrow_above_minimum_succeeds() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let above_minimum = MIN_ESCROW_AMOUNT + 500_000;
    mint(&env, &token, &buyer, above_minimum);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let result = client.try_create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &above_minimum,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert!(matches!(result, Ok(_)));
}
