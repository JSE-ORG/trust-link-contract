#![cfg(test)]
//! Coverage for the Phase 3 basket (multi-token) escrow feature (#571).
//!
//! Exercises the four public entry points end to end:
//! - `create_basket_escrow` — persists every token/amount, maps index 0 to the
//!   primary token/amount, rejects malformed baskets
//! - `fund_basket_escrow`   — pulls every token from the buyer into escrow
//! - `get_basket_tokens`    — returns the stored token entries
//! - `payout_basket_tokens` — driven via `auto_release`, pays each token out
//!
//! Plus multi-token edge cases: more than one token type, uneven per-token
//! amounts, and a zero-amount token that funding and payout both skip.

use crate::{ContractError, Escrow, EscrowClient, EscrowState};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Vec,
};

const SHIPPING_WINDOW: u64 = 3_600;

struct Tk {
    address: Address,
    admin: token::StellarAssetClient<'static>,
    token: token::TokenClient<'static>,
}

fn make_token(env: &Env) -> Tk {
    let issuer = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let address = sac.address();
    Tk {
        admin: token::StellarAssetClient::new(env, &address),
        token: token::TokenClient::new(env, &address),
        address,
    }
}

struct Fx {
    env: Env,
    contract_id: Address,
    seller: Address,
    buyer: Address,
    resolver: Address,
}

fn setup() -> Fx {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &50_u32);

    Fx {
        env,
        contract_id,
        seller,
        buyer,
        resolver,
    }
}

fn vec_addr(env: &Env, addrs: &[&Address]) -> Vec<Address> {
    let mut v = Vec::new(env);
    for a in addrs {
        v.push_back((*a).clone());
    }
    v
}

fn vec_i128(env: &Env, amounts: &[i128]) -> Vec<i128> {
    let mut v = Vec::new(env);
    for a in amounts {
        v.push_back(*a);
    }
    v
}

#[test]
fn create_basket_escrow_persists_all_tokens() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);
    let c = make_token(&fx.env);

    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address, &c.address]),
        &vec_i128(&fx.env, &[1_000, 500, 200]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    // All three tokens/amounts are stored, in order.
    let entries = client.get_basket_tokens(&escrow_id);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries.get(0).unwrap().token, a.address);
    assert_eq!(entries.get(0).unwrap().amount, 1_000);
    assert_eq!(entries.get(1).unwrap().token, b.address);
    assert_eq!(entries.get(1).unwrap().amount, 500);
    assert_eq!(entries.get(2).unwrap().token, c.address);
    assert_eq!(entries.get(2).unwrap().amount, 200);

    // Index 0 becomes the escrow's primary token/amount; state is Pending.
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.token, a.address);
    assert_eq!(escrow.amount, 1_000);
    assert_eq!(escrow.state, EscrowState::Pending);
}

#[test]
fn create_basket_escrow_rejects_malformed_baskets() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);

    // tokens.len() != amounts.len()
    let mismatched = client.try_create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address]),
        &vec_i128(&fx.env, &[1_000]),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    assert_eq!(mismatched, Err(Ok(ContractError::InvalidAmount)));

    // Empty basket
    let empty = client.try_create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &Vec::<Address>::new(&fx.env),
        &Vec::<i128>::new(&fx.env),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    assert_eq!(empty, Err(Ok(ContractError::InvalidAmount)));
}

#[test]
fn fund_basket_escrow_pulls_every_token_from_buyer() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);
    let c = make_token(&fx.env);

    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address, &c.address]),
        &vec_i128(&fx.env, &[1_000, 500, 200]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    a.admin.mint(&fx.buyer, &1_000);
    b.admin.mint(&fx.buyer, &500);
    c.admin.mint(&fx.buyer, &200);

    client.fund_basket_escrow(&escrow_id, &fx.buyer);

    // Buyer fully drained; contract holds each token amount.
    assert_eq!(a.token.balance(&fx.buyer), 0);
    assert_eq!(b.token.balance(&fx.buyer), 0);
    assert_eq!(c.token.balance(&fx.buyer), 0);
    assert_eq!(a.token.balance(&fx.contract_id), 1_000);
    assert_eq!(b.token.balance(&fx.contract_id), 500);
    assert_eq!(c.token.balance(&fx.contract_id), 200);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Funded);
    assert_eq!(escrow.buyer, Some(fx.buyer.clone()));
}

#[test]
fn get_basket_tokens_empty_for_non_basket_escrow() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    // A never-created id has no basket entries.
    assert_eq!(client.get_basket_tokens(&999_u64).len(), 0);
}

#[test]
fn payout_pays_each_basket_token_to_seller() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);
    let c = make_token(&fx.env);

    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address, &c.address]),
        &vec_i128(&fx.env, &[1_000, 500, 200]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    a.admin.mint(&fx.buyer, &1_000);
    b.admin.mint(&fx.buyer, &500);
    c.admin.mint(&fx.buyer, &200);
    client.fund_basket_escrow(&escrow_id, &fx.buyer);

    // Advance past the dispute + shipping windows so auto_release is allowed.
    let escrow = client.get_escrow(&escrow_id);
    fx.env
        .ledger()
        .set_timestamp(escrow.dispute_deadline + SHIPPING_WINDOW + 1);
    client.auto_release(&escrow_id);

    // With protocol fee at its default 0, the seller receives every token in
    // full and the contract is drained of all three.
    assert_eq!(a.token.balance(&fx.seller), 1_000);
    assert_eq!(b.token.balance(&fx.seller), 500);
    assert_eq!(c.token.balance(&fx.seller), 200);
    assert_eq!(a.token.balance(&fx.contract_id), 0);
    assert_eq!(b.token.balance(&fx.contract_id), 0);
    assert_eq!(c.token.balance(&fx.contract_id), 0);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Completed);
}

#[test]
fn zero_amount_token_is_skipped_on_fund_and_payout() {
    // Multi-token edge case: a basket entry with amount 0 must be transferred
    // neither on funding nor on payout (the buyer is never minted that token).
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let zero = make_token(&fx.env);
    let c = make_token(&fx.env);

    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &zero.address, &c.address]),
        &vec_i128(&fx.env, &[1_000, 0, 200]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    a.admin.mint(&fx.buyer, &1_000);
    c.admin.mint(&fx.buyer, &200);
    // Note: `zero` token is never minted — funding must not attempt to pull it.
    client.fund_basket_escrow(&escrow_id, &fx.buyer);

    assert_eq!(a.token.balance(&fx.contract_id), 1_000);
    assert_eq!(zero.token.balance(&fx.contract_id), 0);
    assert_eq!(c.token.balance(&fx.contract_id), 200);

    let escrow = client.get_escrow(&escrow_id);
    fx.env
        .ledger()
        .set_timestamp(escrow.dispute_deadline + SHIPPING_WINDOW + 1);
    client.auto_release(&escrow_id);

    // Seller receives the two funded tokens; the zero-amount token stays at 0.
    assert_eq!(a.token.balance(&fx.seller), 1_000);
    assert_eq!(zero.token.balance(&fx.seller), 0);
    assert_eq!(c.token.balance(&fx.seller), 200);
}

#[test]
fn fund_basket_escrow_rejects_a_buyer_other_than_the_expected_buyer() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let primary = make_token(&fx.env);
    let expected_buyer = Address::generate(&fx.env);
    let stranger = Address::generate(&fx.env);
    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &Some(expected_buyer.clone()),
        &fx.resolver,
        &vec_addr(&fx.env, &[&primary.address]),
        &vec_i128(&fx.env, &[100]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    primary.admin.mint(&stranger, &100);
    assert_eq!(
        client.try_fund_basket_escrow(&escrow_id, &stranger),
        Err(Ok(ContractError::NotAuthorized))
    );
    assert_eq!(primary.token.balance(&stranger), 100);
    assert_eq!(primary.token.balance(&fx.contract_id), 0);
}

#[test]
fn fund_basket_escrow_is_atomic_when_the_buyer_only_has_part_of_the_basket() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let primary = make_token(&fx.env);
    let additional = make_token(&fx.env);
    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &Some(fx.buyer.clone()),
        &fx.resolver,
        &vec_addr(&fx.env, &[&primary.address, &additional.address]),
        &vec_i128(&fx.env, &[100, 50]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    primary.admin.mint(&fx.buyer, &100);
    additional.admin.mint(&fx.buyer, &49);
    assert!(client
        .try_fund_basket_escrow(&escrow_id, &fx.buyer)
        .is_err());

    // The failed second transfer rolls back the first transfer and state update.
    assert_eq!(primary.token.balance(&fx.buyer), 100);
    assert_eq!(primary.token.balance(&fx.contract_id), 0);
    assert_eq!(additional.token.balance(&fx.buyer), 49);
    assert_eq!(additional.token.balance(&fx.contract_id), 0);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Pending);
}

#[test]
fn fund_basket_escrow_rejects_a_basket_already_funded_by_fund_escrow() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let primary = make_token(&fx.env);
    let additional = make_token(&fx.env);
    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &Some(fx.buyer.clone()),
        &fx.resolver,
        &vec_addr(&fx.env, &[&primary.address, &additional.address]),
        &vec_i128(&fx.env, &[100, 50]),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    primary.admin.mint(&fx.buyer, &100);
    additional.admin.mint(&fx.buyer, &50);
    client.fund_escrow(&escrow_id, &fx.buyer);

    assert_eq!(
        client.try_fund_basket_escrow(&escrow_id, &fx.buyer),
        Err(Ok(ContractError::InvalidState))
    );
    assert_eq!(primary.token.balance(&fx.contract_id), 100);
    assert_eq!(additional.token.balance(&fx.contract_id), 50);
}

#[test]
fn fund_basket_escrow_transfers_only_the_configured_amounts() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let primary = make_token(&fx.env);
    let additional = make_token(&fx.env);
    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &Some(fx.buyer.clone()),
        &fx.resolver,
        &vec_addr(&fx.env, &[&primary.address, &additional.address]),
        &vec_i128(&fx.env, &[100, 50]),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    primary.admin.mint(&fx.buyer, &150);
    additional.admin.mint(&fx.buyer, &75);

    client.fund_basket_escrow(&escrow_id, &fx.buyer);

    assert_eq!(primary.token.balance(&fx.contract_id), 100);
    assert_eq!(additional.token.balance(&fx.contract_id), 50);
    assert_eq!(primary.token.balance(&fx.buyer), 50);
    assert_eq!(additional.token.balance(&fx.buyer), 25);
}
