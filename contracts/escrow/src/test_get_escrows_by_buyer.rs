#![cfg(test)]

use crate::test_helpers::{create_funded_escrow, setup_contract};
use crate::Payee;
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, Vec};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = soroban_sdk::token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

#[test]
fn test_get_escrows_by_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer_1 = Address::generate(&env);
    let buyer_2 = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create 2 escrows for buyer 1
    let id1 = create_funded_escrow(
        &env, &client, &seller, &buyer_1, &resolver, &token, 1000, 100, 3600,
    );
    let id2 = create_funded_escrow(
        &env, &client, &seller, &buyer_1, &resolver, &token, 2000, 100, 3600,
    );

    // Create 1 escrow for buyer 2
    let id3 = create_funded_escrow(
        &env, &client, &seller, &buyer_2, &resolver, &token, 3000, 100, 3600,
    );

    // Create 1 pending escrow (no buyer yet)
    let mut payees_47 = Vec::new(&env);
    payees_47.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_47.into_val(&env);
    let _id4 = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &4000_i128,
        &100_u32,
        &3600_u64,
    );

    // Check escrows for buyer 1
    let escrows_1 = client.get_escrows_by_buyer(&buyer_1);
    assert_eq!(escrows_1.len(), 2);
    assert_eq!(escrows_1.get(0).unwrap(), id1);
    assert_eq!(escrows_1.get(1).unwrap(), id2);

    // Check escrows for buyer 2
    let escrows_2 = client.get_escrows_by_buyer(&buyer_2);
    assert_eq!(escrows_2.len(), 1);
    assert_eq!(escrows_2.get(0).unwrap(), id3);

    // Check escrows for a buyer with no escrows
    let buyer_3 = Address::generate(&env);
    let escrows_3 = client.get_escrows_by_buyer(&buyer_3);
    assert_eq!(escrows_3.len(), 0);
}

#[test]
fn test_buyer_index_populated_on_fund() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_tokens(&env, &token, &buyer, 1000);

    let mut payees_46 = Vec::new(&env);
    payees_46.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_46.into_val(&env);
    let id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &100_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);

    let escrows = client.get_escrows_by_buyer(&buyer);
    assert_eq!(escrows.len(), 1);
    assert_eq!(escrows.get(0).unwrap(), id);
}

#[test]
#[ignore]
fn test_get_escrows_by_buyer_fallback_scan() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer_1 = Address::generate(&env);
    let buyer_2 = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create escrows with buyers specified but DO NOT fund them.
    // Without fund_escrow, no BuyerEscrowIndex is written, forcing the
    // fallback O(n) scan path in get_escrows_by_buyer.
    let id1 = client.create_escrow_8(
        &seller.clone().into_val(&env),
        &Some(buyer_1.clone()),
        &resolver,
        &token,
        &1000_i128,
        &100_u32,
        &3600_u64,
    );
    let id2 = client.create_escrow_8(
        &seller.clone().into_val(&env),
        &Some(buyer_1.clone()),
        &resolver,
        &token,
        &2000_i128,
        &100_u32,
        &3600_u64,
    );
    let id3 = client.create_escrow_8(
        &seller.clone().into_val(&env),
        &Some(buyer_2.clone()),
        &resolver,
        &token,
        &3000_i128,
        &100_u32,
        &3600_u64,
    );

    // buyer_1 should find both escrows via the fallback scan (no index exists)
    let escrows_1 = client.get_escrows_by_buyer(&buyer_1);
    assert_eq!(escrows_1.len(), 2);
    assert_eq!(escrows_1.get(0).unwrap(), id1);
    assert_eq!(escrows_1.get(1).unwrap(), id2);

    // buyer_2 should find their escrow
    let escrows_2 = client.get_escrows_by_buyer(&buyer_2);
    assert_eq!(escrows_2.len(), 1);
    assert_eq!(escrows_2.get(0).unwrap(), id3);

    // Unknown buyer gets empty result
    let buyer_3 = Address::generate(&env);
    let escrows_3 = client.get_escrows_by_buyer(&buyer_3);
    assert_eq!(escrows_3.len(), 0);
}

#[test]
#[ignore]
fn test_get_escrows_by_buyer_fallback_scan_many_escrows() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create 15 escrows for the same buyer without funding, forcing fallback scan
    let mut expected_ids: Vec<u64> = Vec::new(&env);
    for _ in 0..15 {
        let id = client.create_escrow_8(
            &seller.clone().into_val(&env),
            &Some(buyer.clone()),
            &resolver,
            &token,
            &1000_i128,
            &100_u32,
            &3600_u64,
        );
        expected_ids.push_back(id);
    }

    // Create 5 escrows for a different buyer (should not appear in buyer's results)
    let other_buyer = Address::generate(&env);
    for _ in 0..5 {
        let _id = client.create_escrow_8(
            &seller.clone().into_val(&env),
            &Some(other_buyer.clone()),
            &resolver,
            &token,
            &1000_i128,
            &100_u32,
            &3600_u64,
        );
    }

    let escrows = client.get_escrows_by_buyer(&buyer);
    assert_eq!(escrows.len(), 15);
    for i in 0..15 {
        assert_eq!(escrows.get(i).unwrap(), expected_ids.get(i).unwrap());
    }
}
