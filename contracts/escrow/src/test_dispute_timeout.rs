#![cfg(test)]
//! Tests for the dispute-resolution timeout escape hatch (`#dispute-timeout`).
//!
//! A resolver can go permanently offline and leave an escrow stuck in
//! `Disputed`. `claim_dispute_timeout` lets either party force a Refund once
//! the admin-configured maximum dispute duration has elapsed.

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

fn setup(
    env: &Env,
) -> (
    EscrowClient<'_>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    token::StellarAssetClient::new(env, &token).mint(&buyer, &1000);

    (client, admin, seller, buyer, resolver, token)
}

fn disputed_single_resolver_escrow(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    buyer: &Address,
    resolver: &Address,
    token: &Address,
) -> u64 {
    let mut resolvers = Vec::new(env);
    resolvers.push_back(resolver.clone());
    let id = client.create_escrow_multi(
        seller,
        &Some(buyer.clone()),
        &resolvers,
        &1,
        token,
        &1000,
        &0,
        &3600,
    );
    client.fund_escrow(&id, buyer);
    client.raise_dispute(
        buyer,
        &id,
        &symbol_short!("OTHER"),
        &soroban_sdk::Bytes::from_slice(env, b"never arrived"),
        &BytesN::from_array(env, &[1; 32]),
    );
    id
}

#[test]
fn claim_before_timeout_is_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, seller, buyer, resolver, token) = setup(&env);
    let id = disputed_single_resolver_escrow(&env, &client, &seller, &buyer, &resolver, &token);

    let disputed_at = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(disputed_at + crate::DEFAULT_DISPUTE_TIMEOUT - 1);

    let res = client.try_claim_dispute_timeout(&buyer, &id);
    assert_eq!(res, Err(Ok(ContractError::DisputeTimeoutNotElapsed)));
    assert_eq!(client.get_escrow(&id).state, EscrowState::Disputed);
}

#[test]
fn buyer_forces_refund_after_timeout() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, seller, buyer, resolver, token) = setup(&env);
    let id = disputed_single_resolver_escrow(&env, &client, &seller, &buyer, &resolver, &token);

    let disputed_at = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(disputed_at + crate::DEFAULT_DISPUTE_TIMEOUT);

    // Either party may force the refund; the buyer does here.
    client.claim_dispute_timeout(&buyer, &id);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Refunded);
    assert_eq!(token::Client::new(&env, &token).balance(&buyer), 1000);
    let dispute = client.get_dispute(&id).unwrap();
    assert_eq!(dispute.arbitration_fee, 0);
    assert_eq!(dispute.resolver_fee, 0);
    assert_eq!(dispute.resolved_by, Some(client.address.clone()));
    assert_eq!(client.get_stats().total_refunded, 1);
}

#[test]
fn seller_can_also_force_refund_after_timeout() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, seller, buyer, resolver, token) = setup(&env);
    let id = disputed_single_resolver_escrow(&env, &client, &seller, &buyer, &resolver, &token);

    let disputed_at = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(disputed_at + crate::DEFAULT_DISPUTE_TIMEOUT);

    client.claim_dispute_timeout(&seller, &id);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Refunded);
}

#[test]
fn timeout_refunds_basket_without_arbitration_or_resolver_fees() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &500_u32);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let primary = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let secondary = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    token::StellarAssetClient::new(&env, &primary).mint(&buyer, &1_000);
    token::StellarAssetClient::new(&env, &secondary).mint(&buyer, &400);

    let mut tokens = Vec::new(&env);
    tokens.push_back(primary.clone());
    tokens.push_back(secondary.clone());
    let mut amounts = Vec::new(&env);
    amounts.push_back(1_000);
    amounts.push_back(400);
    let id = client.create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &tokens,
        &amounts,
        &0_u32,
        &3600_u64,
        &None::<String>,
        &None::<u64>,
        &0_u64,
    );
    client.fund_basket_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &String::from_str(&env, "TRK-TIMEOUT"));
    client.raise_dispute(
        &buyer,
        &id,
        &symbol_short!("OTHER"),
        &soroban_sdk::Bytes::from_slice(&env, b"timeout"),
        &BytesN::from_array(&env, &[7; 32]),
    );

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::DEFAULT_DISPUTE_TIMEOUT);
    client.claim_dispute_timeout(&buyer, &id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Refunded);
    let dispute = client.get_dispute(&id).unwrap();
    assert_eq!(dispute.arbitration_fee, 0);
    assert_eq!(dispute.resolver_fee, 0);
    assert_eq!(token::Client::new(&env, &primary).balance(&buyer), 1_000);
    assert_eq!(token::Client::new(&env, &secondary).balance(&buyer), 400);
    assert_eq!(token::Client::new(&env, &primary).balance(&resolver), 0);
    assert_eq!(
        token::Client::new(&env, &primary).balance(&fee_collector),
        0
    );
    assert_eq!(client.get_stats().total_refunded, 1);
}

#[test]
fn third_party_cannot_claim_timeout() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, seller, buyer, resolver, token) = setup(&env);
    let id = disputed_single_resolver_escrow(&env, &client, &seller, &buyer, &resolver, &token);

    let disputed_at = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(disputed_at + crate::DEFAULT_DISPUTE_TIMEOUT);
    let stranger = Address::generate(&env);

    let res = client.try_claim_dispute_timeout(&stranger, &id);
    assert_eq!(res, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn timeout_configurable_by_admin() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, admin, seller, buyer, resolver, token) = setup(&env);
    let id = disputed_single_resolver_escrow(&env, &client, &seller, &buyer, &resolver, &token);

    let custom = 7_200_u64;
    client.set_dispute_timeout(&admin, &custom);
    assert_eq!(client.get_dispute_timeout(), custom);

    let disputed_at = env.ledger().timestamp();
    env.ledger().set_timestamp(disputed_at + custom - 1);
    assert_eq!(
        client.try_claim_dispute_timeout(&buyer, &id),
        Err(Ok(ContractError::DisputeTimeoutNotElapsed))
    );

    env.ledger().set_timestamp(disputed_at + custom);
    client.claim_dispute_timeout(&buyer, &id);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Refunded);
}

#[test]
fn set_dispute_timeout_validates_range_and_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let fresh = EscrowClient::new(&env, &contract_id);
    fresh.initialize(&admin, &Address::generate(&env), &0_u32);

    let too_small = crate::MIN_DISPUTE_TIMEOUT - 1;
    assert_eq!(
        fresh.try_set_dispute_timeout(&admin, &too_small),
        Err(Ok(ContractError::InvalidDisputeTimeout))
    );

    let too_large = crate::MAX_DISPUTE_TIMEOUT + 1;
    assert_eq!(
        fresh.try_set_dispute_timeout(&admin, &too_large),
        Err(Ok(ContractError::InvalidDisputeTimeout))
    );

    let stranger = Address::generate(&env);
    assert_eq!(
        fresh.try_set_dispute_timeout(&stranger, &crate::MIN_DISPUTE_TIMEOUT),
        Err(Ok(ContractError::NotAuthorized))
    );

    // Default is returned until the admin overrides it.
    assert_eq!(fresh.get_dispute_timeout(), crate::DEFAULT_DISPUTE_TIMEOUT);
    fresh.set_dispute_timeout(&admin, &crate::MIN_DISPUTE_TIMEOUT);
    assert_eq!(fresh.get_dispute_timeout(), crate::MIN_DISPUTE_TIMEOUT);
}

#[test]
fn claim_on_undisputed_escrow_is_invalid_state() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _admin, seller, buyer, resolver, token) = setup(&env);

    let mut resolvers = Vec::new(&env);
    resolvers.push_back(resolver.clone());
    let id = client.create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &resolvers,
        &1,
        &token,
        &1000,
        &0,
        &3600,
    );
    client.fund_escrow(&id, &buyer);

    assert_eq!(
        client.try_claim_dispute_timeout(&buyer, &id),
        Err(Ok(ContractError::InvalidState))
    );
}
