#![cfg(test)]

use crate::{ContractError, EscrowClient, Payee};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, IntoVal, String as SorobanString, Vec};

#[allow(dead_code)]
fn single_payee(env: &Env, address: &Address) -> Vec<Payee> {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

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
    let seller_val = seller.clone().into_val(&env);
    let res = client.try_create_escrow_8(
        &seller_val,
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
    payees_2.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_2_val = payees_2.into_val(&env);
    let id1 = client.create_escrow_8(
        &payees_2_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &500,
        &100,
        &3600,
    );
    assert_eq!(id1, 1);

    // Test exactly maximum
    let mut payees_1 = Vec::new(&env);
    payees_1.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_1_val = payees_1.into_val(&env);
    let id2 = client.create_escrow_8(
        &payees_1_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &5000,
        &100,
        &3600,
    );
    assert_eq!(id2, 2);

    // Test above maximum
    let seller_val_2 = seller.clone().into_val(&env);
    let res = client.try_create_escrow_8(
        &seller_val_2,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &5001,
        &100,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::AmountExceedsMaximum)));
}

/// Issue #1229: every secondary token in a basket is validated against the
/// same canonical `min_amount`/`max_amount` bounds as the primary amount.
#[test]
fn test_basket_secondary_amount_limits_are_enforced() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let second_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    client.set_amount_limits(&admin, &500, &5000);

    let tokens = Vec::from_array(&env, [token.clone(), second_token.clone()]);

    // Secondary token below min_amount.
    let amounts = Vec::from_array(&env, [1_000_i128, 499_i128]);
    let result = client.try_create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &tokens,
        &amounts,
        &100_u32,
        &3_600_u64,
        &None::<SorobanString>,
        &None::<u64>,
        &0_u64,
    );
    assert_eq!(result, Err(Ok(ContractError::AmountBelowMinimum)));

    // Secondary token above max_amount.
    let amounts = Vec::from_array(&env, [1_000_i128, 5_001_i128]);
    let result = client.try_create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &tokens,
        &amounts,
        &100_u32,
        &3_600_u64,
        &None::<SorobanString>,
        &None::<u64>,
        &0_u64,
    );
    assert_eq!(result, Err(Ok(ContractError::AmountExceedsMaximum)));

    // Both entries exactly at the configured boundaries are accepted.
    let amounts = Vec::from_array(&env, [500_i128, 5_000_i128]);
    let id = client.create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &tokens,
        &amounts,
        &100_u32,
        &3_600_u64,
        &None::<SorobanString>,
        &None::<u64>,
        &0_u64,
    );
    assert_eq!(client.get_basket_tokens(&id).unwrap().len(), 2);
}

#[test]
fn test_basket_secondary_amount_limits_follow_dynamic_config() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let second_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let tokens = Vec::from_array(&env, [token.clone(), second_token.clone()]);
    let amounts = Vec::from_array(&env, [1_000_i128, 900_i128]);

    client.set_amount_limits(&admin, &100, &2_000);
    let accepted = client.create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &tokens,
        &amounts,
        &100_u32,
        &3_600_u64,
        &None::<SorobanString>,
        &None::<u64>,
        &0_u64,
    );

    // An admin limit change must apply immediately to secondary assets too.
    client.set_amount_limits(&admin, &1_000, &2_000);
    let result = client.try_create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &tokens,
        &amounts,
        &100_u32,
        &3_600_u64,
        &None::<SorobanString>,
        &None::<u64>,
        &0_u64,
    );
    assert_eq!(result, Err(Ok(ContractError::AmountBelowMinimum)));
    assert!(accepted > 0);
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

// ── Circuit breaker: rolling 24-hour volume limits ────────────────────────────

use soroban_sdk::{
    testutils::Ledger,
    token,
};

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

/// Helper: creates and funds one escrow for `amount`, returning the escrow id.
fn create_and_fund(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    buyer: &Address,
    resolver: &Address,
    token: &Address,
    amount: i128,
) -> u64 {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        resolver,
        token,
        &amount,
        &0_u32,
        &3_600_u64,
    );
    client.fund_escrow(&escrow_id, buyer);
    escrow_id
}

/// Admin can configure a per-token daily volume limit and the circuit breaker
/// fires once that limit is reached.
#[test]
fn test_daily_volume_limit_blocks_excess_funding() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);

    // Set a daily limit of 1_000 for our token.
    client.set_daily_volume_limit(&admin, &token, &1_000_i128);

    // Mint enough tokens for the buyer to cover all attempted deposits.
    mint(&env, &token, &buyer, 10_000);

    // First escrow of 600 — still under the limit.
    create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 600);

    // Second escrow of 400 — hits the limit exactly (600 + 400 = 1_000 ≤ 1_000).
    create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 400);

    // Third attempt of 1 — would push total to 1_001 which exceeds the limit.
    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);
    let over_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &1_i128,
        &0_u32,
        &3_600_u64,
    );
    let result = client.try_fund_escrow(&over_id, &buyer);
    assert_eq!(result, Err(Ok(ContractError::AmountExceedsMaximum)));
}

/// The volume counter resets automatically after the 24-hour window elapses,
/// allowing new deposits up to the configured limit again.
#[test]
fn test_daily_volume_window_resets_after_24h() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);
    client.set_daily_volume_limit(&admin, &token, &1_000_i128);

    mint(&env, &token, &buyer, 10_000);

    // Fill the daily limit in the first window.
    create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 1_000);

    // Advance ledger time by exactly 24 hours to roll the window.
    env.ledger().with_mut(|li| {
        li.timestamp += 86_400;
    });

    // Should succeed again: the window rolled and the accumulator is fresh.
    create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 1_000);
}

/// When no daily volume limit is set for a token the circuit breaker is
/// inactive and arbitrarily large deposits are accepted.
#[test]
fn test_no_daily_volume_limit_allows_any_deposit() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);
    // Deliberately do NOT call set_daily_volume_limit.

    mint(&env, &token, &buyer, 10_000_000);

    // A very large deposit should go through unchecked.
    create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 5_000_000);
}

/// A non-admin caller cannot set the daily volume limit.
#[test]
fn test_set_daily_volume_limit_requires_admin() {
    let (env, admin, seller, _buyer, _resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);

    let result = client.try_set_daily_volume_limit(&seller, &token, &1_000_i128);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

/// A zero or negative limit is rejected with `InvalidAmount`.
#[test]
fn test_set_daily_volume_limit_rejects_zero() {
    let (env, admin, _seller, _buyer, _resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);

    let result = client.try_set_daily_volume_limit(&admin, &token, &0_i128);
    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
}
