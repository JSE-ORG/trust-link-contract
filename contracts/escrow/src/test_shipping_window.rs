#![cfg(test)]

use crate::test_helpers::setup_contract;
use crate::{ContractError, Payee, MAX_SHIPPING_WINDOW, MIN_SHIPPING_WINDOW};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

fn make_payees(env: &Env, seller: &Address) -> soroban_sdk::Vec<Payee> {
    let mut payees = soroban_sdk::Vec::new(env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    payees
}

#[test]
fn test_shipping_window_zero_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let payees = make_payees(&env, &seller);

    let result = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &0_u32,
        &0_u64,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidShippingWindow)));
}

#[test]
fn test_shipping_window_min_is_accepted() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let payees = make_payees(&env, &seller);

    let id = client.create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &0_u32,
        &MIN_SHIPPING_WINDOW,
    );

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.shipping_window, MIN_SHIPPING_WINDOW);
}

#[test]
fn test_shipping_window_max_is_accepted() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let payees = make_payees(&env, &seller);

    let id = client.create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &0_u32,
        &MAX_SHIPPING_WINDOW,
    );

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.shipping_window, MAX_SHIPPING_WINDOW);
}

#[test]
fn test_shipping_window_above_max_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let payees = make_payees(&env, &seller);

    let result = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &0_u32,
        &(MAX_SHIPPING_WINDOW + 1),
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidShippingWindow)));
}

#[test]
fn test_shipping_window_u64_max_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let payees = make_payees(&env, &seller);

    let result = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &0_u32,
        &u64::MAX,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidShippingWindow)));
}
