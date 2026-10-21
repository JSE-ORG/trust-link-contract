#![cfg(test)]

use crate::test_helpers::{create_funded_escrow, setup_contract};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

#[test]
fn test_get_escrows_by_ids_order_and_missing() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create three funded escrows
    let id1 = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 100, 0, 3600,
    );
    let id2 = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 200, 0, 3600,
    );
    let id3 = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 300, 0, 3600,
    );

    // Build query vector: [id2, missing(9999), id1, id3]
    let mut ids = Vec::new(&env);
    ids.push_back(id2);
    ids.push_back(9999_u64);
    ids.push_back(id1);
    ids.push_back(id3);

    let results = client.get_escrows_by_ids(&ids);
    assert_eq!(results.len(), 4);

    // slot 0 -> id2 present
    let maybe0 = results.get(0).unwrap();
    assert!(maybe0.is_some());
    assert_eq!(maybe0.unwrap().amount, 200_i128);

    // slot 1 -> missing
    let maybe1 = results.get(1).unwrap();
    assert!(maybe1.is_none());

    // slot 2 -> id1
    let maybe2 = results.get(2).unwrap();
    assert!(maybe2.is_some());
    assert_eq!(maybe2.unwrap().amount, 100_i128);

    // slot 3 -> id3
    let maybe3 = results.get(3).unwrap();
    assert!(maybe3.is_some());
    assert_eq!(maybe3.unwrap().amount, 300_i128);
}

#[test]
fn test_get_escrows_by_ids_truncates_beyond_cap_and_keeps_none_slots() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let real_id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 100, 0, 3600,
    );

    // Slot 0 is real, slots 1..cap are missing, and two extra ids past the cap
    // (one of them real) must be dropped by the documented truncate policy.
    let cap = crate::MAX_IDS_PER_QUERY;
    let mut ids = Vec::new(&env);
    ids.push_back(real_id);
    for i in 1..cap {
        ids.push_back(10_000 + u64::from(i));
    }
    ids.push_back(real_id);
    ids.push_back(20_000_u64);
    assert_eq!(ids.len(), cap + 2);

    let results = client.get_escrows_by_ids(&ids);
    assert_eq!(results.len(), cap);
    assert_eq!(results.get(0).unwrap().unwrap().amount, 100_i128);
    for i in 1..cap {
        assert!(results.get(i).unwrap().is_none());
    }

    // Exactly `cap` ids is not truncated.
    let mut exact = Vec::new(&env);
    for _ in 0..cap {
        exact.push_back(real_id);
    }
    assert_eq!(client.get_escrows_by_ids(&exact).len(), cap);

    // Empty input yields an empty result.
    assert_eq!(client.get_escrows_by_ids(&Vec::new(&env)).len(), 0);
}
