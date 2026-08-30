#![cfg(test)]

use crate::test_helpers::setup_contract;
use crate::Payee;
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

#[test]
fn test_get_escrows_by_seller_empty() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);

    let escrows = client.get_escrows_by_seller(&seller);
    assert_eq!(escrows.len(), 0);
}

#[test]
fn test_get_escrows_by_seller_single() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);

    let mut payees = soroban_sdk::Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_val = payees.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let escrows = client.get_escrows_by_seller(&seller);
    assert_eq!(escrows.len(), 1);
    assert_eq!(escrows.get(0).unwrap(), id);
}

#[test]
fn test_get_escrows_by_seller_multiple() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller1 = Address::generate(&env);
    let seller2 = Address::generate(&env);
    let resolver = Address::generate(&env);

    let mut payees1 = soroban_sdk::Vec::new(&env);
    payees1.push_back(Payee {
        address: seller1.clone(),
        bps: 10_000,
    });

    let payees1_val = payees1.into_val(&env);
    let id1 = client.create_escrow(
        &payees1_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let mut payees2 = soroban_sdk::Vec::new(&env);
    payees2.push_back(Payee {
        address: seller1.clone(),
        bps: 10_000,
    });

    let payees2_val = payees2.into_val(&env);
    let id2 = client.create_escrow(
        &payees2_val,
        &None::<Address>,
        &resolver,
        &token,
        &2000_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let mut payees3 = soroban_sdk::Vec::new(&env);
    payees3.push_back(Payee {
        address: seller2.clone(),
        bps: 10_000,
    });

    let payees3_val = payees3.into_val(&env);
    let id3 = client.create_escrow(
        &payees3_val,
        &None::<Address>,
        &resolver,
        &token,
        &3000_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let s1_escrows = client.get_escrows_by_seller(&seller1);
    assert_eq!(s1_escrows.len(), 2);
    assert_eq!(s1_escrows.get(0).unwrap(), id1);
    assert_eq!(s1_escrows.get(1).unwrap(), id2);

    let s2_escrows = client.get_escrows_by_seller(&seller2);
    assert_eq!(s2_escrows.len(), 1);
    assert_eq!(s2_escrows.get(0).unwrap(), id3);
}

#[test]
fn test_get_escrows_by_seller_matches_vendor() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);

    let mut payees = soroban_sdk::Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let payees_val = payees.into_val(&env);
    client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let by_seller = client.get_escrows_by_seller(&seller);
    let by_vendor = client.get_escrows_by_vendor(&seller);
    assert_eq!(by_seller, by_vendor);
}

#[test]
fn test_get_escrows_by_seller_unknown_returns_empty() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let unknown = Address::generate(&env);

    let escrows = client.get_escrows_by_seller(&unknown);
    assert_eq!(escrows.len(), 0);
}
