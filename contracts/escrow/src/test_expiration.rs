#![cfg(test)]

use crate::{ContractError, Escrow, EscrowClient, EscrowState, ResolutionType};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, String as SorobanString, Symbol, BytesN,
};

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
    let contract_id = env.register(Escrow, ());
    {
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
    }
    (env, admin, seller, buyer, resolver, token_address, contract_id)
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

#[test]
fn test_create_escrow_with_invalid_expiration() {
    let (env, _admin, seller, _buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Current ledger timestamp is 0. An expiration of 0 (past/present) should fail.
    let res = client.try_create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(0_u64),
        &100_u64,
    );
    assert!(matches!(res, Err(Ok(ContractError::InvalidAmount))));
}

#[test]
fn test_cannot_fund_expired_escrow() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Create escrow that expires at timestamp 10.
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    // Advance time past expiration (to timestamp 10)
    env.ledger().set_timestamp(10);
    mint_tokens(&env, &token, &buyer, 1000);

    // Funding should fail with EscrowExpired.
    let res = client.try_fund_escrow(&id, &buyer);
    assert!(matches!(res, Err(Ok(ContractError::EscrowExpired))));
}

#[test]
fn test_cannot_ship_expired_escrow() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Create escrow that expires at timestamp 10.
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    mint_tokens(&env, &token, &buyer, 1000);
    client.fund_escrow(&id, &buyer);

    // Advance time past expiration (to timestamp 10)
    env.ledger().set_timestamp(10);

    // Shipping should fail with EscrowExpired.
    let res = client.try_mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK001"));
    assert!(matches!(res, Err(Ok(ContractError::EscrowExpired))));
}

#[test]
fn test_reclaim_lifecycle() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Create escrow with expiration at timestamp 10 and grace period 5.
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    mint_tokens(&env, &token, &buyer, 1000);
    client.fund_escrow(&id, &buyer);

    // Verify initial balance
    assert_eq!(token::Client::new(&env, &token).balance(&buyer), 0);
    assert_eq!(token::Client::new(&env, &token).balance(&contract_id), 1000);

    // 1. Try to reclaim immediately (before expiration) -> should fail with InvalidState
    let res = client.try_reclaim_expired(&id);
    assert!(matches!(res, Err(Ok(ContractError::InvalidState))));

    // 2. Advance to timestamp 10 (expired, but within grace period) -> should fail with GracePeriodNotElapsed
    env.ledger().set_timestamp(10);
    let res = client.try_reclaim_expired(&id);
    assert!(matches!(res, Err(Ok(ContractError::GracePeriodNotElapsed))));

    // 3. Advance to timestamp 15 (expired + grace period elapsed) -> should succeed
    env.ledger().set_timestamp(15);
    client.reclaim_expired(&id);

    // Verify state transition to Expired
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Expired);

    // Verify funds returned to buyer
    assert_eq!(token::Client::new(&env, &token).balance(&buyer), 1000);
    assert_eq!(token::Client::new(&env, &token).balance(&contract_id), 0);
}

#[test]
fn test_reclaim_with_active_dispute() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Create escrow with expiration at timestamp 10 and grace period 5.
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    mint_tokens(&env, &token, &buyer, 1000);
    client.fund_escrow(&id, &buyer);

    // Buyer raises a dispute at timestamp 5.
    env.ledger().set_timestamp(5);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "reason"),
        &SorobanString::from_str(&env, "description"),
        &hash,
    );

    // Advance to timestamp 15 (expired + grace period elapsed).
    env.ledger().set_timestamp(15);

    // Try to reclaim -> should fail with InvalidState because dispute is active.
    let res = client.try_reclaim_expired(&id);
    assert!(matches!(res, Err(Ok(ContractError::InvalidState))));

    // Resolver resolves dispute in favor of Refund.
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Refunded);

    // Subsequent reclaim should fail with InvalidState (not in Funded/Shipped state).
    let res2 = client.try_reclaim_expired(&id);
    assert!(matches!(res2, Err(Ok(ContractError::InvalidState))));
}
