#![cfg(test)]
//! Regression tests for `create_escrow_with_expiration` (#564).
//!
//! Previously `expires_at`/`grace_period` were accepted but silently ignored,
//! so escrows created through this entry point never actually expired.

use crate::{ContractError, Escrow, EscrowClient, EscrowState};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env,
};

fn setup(env: &Env) -> (EscrowClient<'static>, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let fee_collector = Address::generate(env);
    let token_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    token::StellarAssetClient::new(env, &token_addr).mint(&buyer, &10_000_i128);

    (client, seller, buyer, resolver, token_addr)
}

#[test]
fn expires_at_none_never_expires() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, token_addr) = setup(&env);

    let escrow_id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
        &None::<u64>,
        &0_u64,
    );

    // Advance far into the future — with no expiration set, funding must
    // still succeed.
    env.ledger()
        .set_timestamp(1_000_000 + crate::PENDING_EXPIRY_WINDOW - 1);
    client.fund_escrow(&escrow_id, &buyer);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Funded);
}

#[test]
fn fund_before_expiry_succeeds() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, token_addr) = setup(&env);

    let expires_at = 1_000_000 + 3600;
    let escrow_id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
        &Some(expires_at),
        &0_u64,
    );

    env.ledger().set_timestamp(expires_at - 1);
    client.fund_escrow(&escrow_id, &buyer);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Funded);
}

#[test]
fn fund_after_expiry_returns_escrow_expired() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, token_addr) = setup(&env);

    let expires_at = 1_000_000 + 3600;
    let escrow_id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
        &Some(expires_at),
        &0_u64,
    );

    env.ledger().set_timestamp(expires_at + 1);
    let result = client.try_fund_escrow(&escrow_id, &buyer);
    assert_eq!(result, Err(Ok(ContractError::EscrowExpired)));
}

#[test]
fn grace_period_overflow_is_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, _buyer, resolver, token_addr) = setup(&env);

    let result = client.try_create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
        &Some(u64::MAX - 1),
        &u64::MAX,
    );
    assert_eq!(result, Err(Ok(ContractError::ArithmeticOverflow)));
}
