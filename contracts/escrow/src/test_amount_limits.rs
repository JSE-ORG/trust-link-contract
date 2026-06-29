#![cfg(test)]

use crate::{ContractError, EscrowClient, Payee};
use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{token, Address, Env, Vec};

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
fn test_amount_limits_enforced() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    client.initialize(&admin, &fee_collector, &0_u32);

    let min_limit = 500;
    let max_limit = 5000;

    client.set_amount_limits(&admin, &min_limit, &max_limit);

    // Test below minimum
    let res = client.try_create_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &499,
        &100,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::AmountBelowMinimum)));

    // Test exactly minimum
    let mut payees_2 = Vec::new(&env);
    payees_2.push_back(Payee { address: seller.clone(), bps: 10_000 });
    let id1 = client.create_escrow(
        &payees_2,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &500,
        &100,
        &0_u32,
        &3600,
    );
    assert_eq!(id1, 1);

    // Test exactly maximum
    let mut payees_1 = Vec::new(&env);
    payees_1.push_back(Payee { address: seller.clone(), bps: 10_000 });
    let id2 = client.create_escrow(
        &payees_1,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &5000,
        &100,
        &0_u32,
        &3600,
    );
    assert_eq!(id2, 2);

    // Test above maximum
    let res = client.try_create_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &5001,
        &100,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::AmountExceedsMaximum)));
}

#[test]
fn test_set_amount_limits_auth() {
    let (env, admin, seller, _buyer, _resolver, _token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    client.initialize(&admin, &fee_collector, &0_u32);

    // Seller tries to set limits
    let res = client.try_set_amount_limits(&seller, &500, &5000);
    assert_eq!(res, Err(Ok(ContractError::NotAuthorized)));
}
