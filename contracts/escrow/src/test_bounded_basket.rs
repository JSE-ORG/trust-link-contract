#![cfg(test)]
//! Capacity-bounded basket storage (issue #1232).
//!
//! Verifies that `BoundedBasket` enforces the hard basket ceiling at its
//! construction boundary, that legacy unbounded-vector values are migrated on
//! load, and that the public query ABI is unchanged.

use crate::internal::{load_basket_tokens, save_basket_tokens};
use crate::test_helpers::setup_contract;
use crate::{BoundedBasket, ContractError, DataKey, TokenEntry, BASKET_CAPACITY};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn entry(env: &Env) -> TokenEntry {
    TokenEntry {
        token: Address::generate(env),
        amount: 1_000,
    }
}

#[test]
fn bounded_basket_rejects_over_capacity_vector() {
    let env = Env::default();
    let mut entries = Vec::new(&env);
    for _ in 0..BASKET_CAPACITY {
        entries.push_back(entry(&env));
    }
    let at_capacity = BoundedBasket::try_new(entries.clone()).expect("capacity is inclusive");
    assert_eq!(at_capacity.len(), BASKET_CAPACITY);

    entries.push_back(entry(&env));
    assert_eq!(
        BoundedBasket::try_new(entries),
        Err(ContractError::BasketTokenMismatch)
    );
}

#[test]
fn bounded_basket_accessors_match_the_underlying_vector() {
    let env = Env::default();
    let first = entry(&env);
    let second = entry(&env);
    let mut entries = Vec::new(&env);
    entries.push_back(first.clone());
    entries.push_back(second.clone());

    let basket = BoundedBasket::new(entries.clone());
    assert_eq!(basket.len(), 2);
    assert!(!basket.is_empty());
    assert_eq!(basket.get(0), Some(first));
    assert_eq!(basket.get(1), Some(second));
    assert_eq!(basket.get(2), None);
    assert_eq!(basket.into_inner(), entries);
    assert!(BoundedBasket::new(Vec::new(&env)).is_empty());
}

#[test]
fn legacy_basket_layout_is_migrated_on_load() {
    let env = Env::default();
    let (contract_id, _client, _admin, _fc) = setup_contract(&env);
    let first = entry(&env);
    let second = entry(&env);
    let mut legacy = Vec::new(&env);
    legacy.push_back(first.clone());
    legacy.push_back(second.clone());

    env.as_contract(&contract_id, || {
        // Deployed contracts hold the old `Vec<TokenEntry>` layout.
        env.storage()
            .persistent()
            .set(&DataKey::BasketTokens(7), &legacy);

        let loaded = load_basket_tokens(&env, 7);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get(0), Some(first.clone()));
        assert_eq!(loaded.get(1), Some(second.clone()));

        let stored: Option<BoundedBasket> =
            env.storage().persistent().get(&DataKey::BasketTokensV2(7));
        assert_eq!(stored, Some(loaded));
        assert!(!env.storage().persistent().has(&DataKey::BasketTokens(7)));
    });
}

#[test]
fn bounded_basket_round_trips_through_storage() {
    let env = Env::default();
    let (contract_id, _client, _admin, _fc) = setup_contract(&env);
    let mut entries = Vec::new(&env);
    entries.push_back(entry(&env));
    entries.push_back(entry(&env));

    env.as_contract(&contract_id, || {
        save_basket_tokens(&env, 42, &BoundedBasket::new(entries.clone()));
        let loaded = load_basket_tokens(&env, 42);
        assert_eq!(loaded.into_inner(), entries);
    });
}
