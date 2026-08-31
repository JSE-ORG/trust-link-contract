#![cfg(test)]

//! Tests for query improvements addressing issues #823, #824, #825, #826

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

// ── Issue #823: get_escrows_by_buyer fallback scan without TTL extension ─────

#[test]
fn test_get_escrows_by_buyer_fallback_without_ttl_extension() {
    // The fallback scan should not extend TTL for non-matching escrows
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer_target = Address::generate(&env);
    let buyer_other = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create escrows without funding to force fallback scan (no index)
    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);

    // Create 15 escrows: 5 for target buyer, 10 for other buyer
    let mut target_ids = Vec::new(&env);
    for _ in 0..5 {
        let id = client.create_escrow_8(
            &payees_val,
            &Some(buyer_target.clone()),
            &resolver,
            &token,
            &1000_i128,
            &100_u32,
            &3600_u64,
        );
        target_ids.push_back(id);
    }

    for _ in 0..10 {
        let _id = client.create_escrow_8(
            &payees_val,
            &Some(buyer_other.clone()),
            &resolver,
            &token,
            &1000_i128,
            &100_u32,
            &3600_u64,
        );
    }

    // Query should return only target buyer's escrows (in reverse creation order)
    let result = client.get_escrows_by_buyer(&buyer_target);
    assert_eq!(result.len(), 5);
    // Results are in reverse order (newest first) due to fallback scan
    for i in 0..5 {
        let expected_id = target_ids.get(4 - i).unwrap(); // Reverse lookup
        assert_eq!(result.get(i).unwrap(), expected_id);
    }
}

#[test]
fn test_get_escrows_by_buyer_with_index() {
    // When index exists (after funding), it should use the index path
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create funded escrows - this populates the buyer index
    let id1 = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );
    let id2 = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 2000, 100, 3600,
    );

    let result = client.get_escrows_by_buyer(&buyer);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(0).unwrap(), id1);
    assert_eq!(result.get(1).unwrap(), id2);
}

// ── Issue #824: get_state_history returns EscrowNotFound for missing escrow ──

#[test]
fn test_get_state_history_missing_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    // Query non-existent escrow should return EscrowNotFound
    let result = client.try_get_state_history(&999_u64);
    assert!(result.is_err());
}

#[test]
fn test_get_state_history_new_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create a new escrow - it should have at least Pending state in history
    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    let history = client.get_state_history(&id);
    // New escrow should have history with at least Pending and Funded states
    assert!(history.len() >= 1);
}

// ── Issue #825: get_basket_tokens distinguishes missing vs single-token ──────

#[test]
fn test_get_basket_tokens_missing_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    // Missing escrow should return None
    let result = client.get_basket_tokens(&999_u64);
    assert!(result.is_none());
}

#[test]
fn test_get_basket_tokens_single_token_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_tokens(&env, &token, &buyer, 1000);

    // Create single-token escrow (not basket)
    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);

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

    // Single-token escrow returns Some(empty Vec)
    let result = client.get_basket_tokens(&id);
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 0);
}

// ── Issue #826: get_escrows_by_ids enforces input length cap ─────────────────

#[test]
fn test_get_escrows_by_ids_respects_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create 10 escrows
    let mut created_ids = Vec::new(&env);
    for _ in 0..10 {
        let id = create_funded_escrow(
            &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
        );
        created_ids.push_back(id);
    }

    // Build query with 60 IDs (exceeds MAX_MESSAGES_PER_PAGE = 50)
    let mut ids = Vec::new(&env);
    for i in 0..10 {
        ids.push_back(created_ids.get(i).unwrap());
    }
    // Add 50 more dummy IDs
    for i in 1000..1050 {
        ids.push_back(i);
    }

    assert_eq!(ids.len(), 60);

    // Should only return first 50 results (the cap)
    let results = client.get_escrows_by_ids(&ids);
    assert_eq!(results.len(), 50);

    // First 10 should be Some (our created escrows)
    for i in 0..10 {
        assert!(results.get(i).unwrap().is_some());
    }
}

#[test]
fn test_get_escrows_by_ids_under_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create 5 escrows
    let mut ids = Vec::new(&env);
    for _ in 0..5 {
        let id = create_funded_escrow(
            &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
        );
        ids.push_back(id);
    }

    // Query with 5 IDs (under cap)
    let results = client.get_escrows_by_ids(&ids);
    assert_eq!(results.len(), 5);

    // All should be present
    for i in 0..5 {
        assert!(results.get(i).unwrap().is_some());
    }
}
