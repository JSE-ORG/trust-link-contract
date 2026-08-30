#![cfg(test)]
//! Coverage for the Phase 3 basket (multi-token) escrow feature (#571).
//!
//! Exercises the four public entry points end to end:
//! - `create_basket_escrow` — persists every token/amount, maps index 0 to the
//!   primary token/amount, rejects malformed baskets, validates every amount
//!   against MinAmount/MaxAmount (#807)
//! - `fund_basket_escrow`   — pulls every token from the buyer into escrow
//! - `get_basket_tokens`    — returns the stored token entries
//! - `payout_basket_tokens` — driven via `auto_release`, pays each token out
//!
//! Plus multi-token edge cases: more than one token type, uneven per-token
//! amounts, and a zero-amount token that funding and payout both skip.

use crate::{ContractError, Escrow, EscrowClient, EscrowState, MAX_BASKET_SIZE};
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
    assert_eq!(mismatched, Err(Ok(ContractError::BasketTokenMismatch)));

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
    assert_eq!(empty, Err(Ok(ContractError::BasketTokenMismatch)));
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
fn create_basket_escrow_rejects_zero_secondary_amount() {
    // Issue #807: a basket entry with amount 0 must be rejected at creation
    // time, not silently accepted and skipped later at transfer time.
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let zero = make_token(&fx.env);
    let c = make_token(&fx.env);

    let result = client.try_create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &zero.address, &c.address]),
        &vec_i128(&fx.env, &[1_000, 0, 200]),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
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

#[test]
fn cancel_escrow_refunds_basket_tokens_to_the_buyer() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);
    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &Some(fx.buyer.clone()),
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address]),
        &vec_i128(&fx.env, &[1_000, 500]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    a.admin.mint(&fx.buyer, &1_000);
    b.admin.mint(&fx.buyer, &500);
    client.fund_basket_escrow(&escrow_id, &fx.buyer);

    // Buyer cancels the funded (but not yet shipped) escrow.
    client.cancel_escrow(&fx.buyer, &escrow_id);

    assert_eq!(a.token.balance(&fx.buyer), 1_000);
    assert_eq!(b.token.balance(&fx.buyer), 500);
    assert_eq!(a.token.balance(&fx.contract_id), 0);
    assert_eq!(b.token.balance(&fx.contract_id), 0);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Refunded);
}

#[test]
fn mutual_cancel_refunds_basket_tokens_to_the_buyer() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);
    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &Some(fx.buyer.clone()),
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address]),
        &vec_i128(&fx.env, &[1_000, 500]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    a.admin.mint(&fx.buyer, &1_000);
    b.admin.mint(&fx.buyer, &500);
    client.fund_basket_escrow(&escrow_id, &fx.buyer);

    client.mutual_cancel(&escrow_id);

    assert_eq!(a.token.balance(&fx.buyer), 1_000);
    assert_eq!(b.token.balance(&fx.buyer), 500);
    assert_eq!(a.token.balance(&fx.contract_id), 0);
    assert_eq!(b.token.balance(&fx.contract_id), 0);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Canceled);
}

#[test]
fn co_signed_release_pays_out_basket_tokens_to_the_seller() {
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);
    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &Some(fx.buyer.clone()),
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address]),
        &vec_i128(&fx.env, &[1_000, 500]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    a.admin.mint(&fx.buyer, &1_000);
    b.admin.mint(&fx.buyer, &500);
    client.fund_basket_escrow(&escrow_id, &fx.buyer);

    // Early release by mutual consent, before the shipping/dispute windows elapse.
    client.co_signed_release(&fx.seller, &escrow_id);

    assert_eq!(a.token.balance(&fx.seller), 1_000);
    assert_eq!(b.token.balance(&fx.seller), 500);
    assert_eq!(a.token.balance(&fx.contract_id), 0);
    assert_eq!(b.token.balance(&fx.contract_id), 0);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Completed);
}

// ── Issue #807 – per-token amount validation ─────────────────────────────────

#[test]
fn create_basket_escrow_rejects_secondary_amount_below_minimum() {
    // When the admin configures a MinAmount, every basket entry must meet it.
    // Here the primary passes but the secondary is below the floor.
    let fx = setup();
    let admin = Address::generate(&fx.env);
    // Re-initialize with a known admin so we can call set_amount_limits.
    let fee_collector = Address::generate(&fx.env);
    let contract_id = fx.env.register(crate::Escrow, ());
    let c2 = EscrowClient::new(&fx.env, &contract_id);
    c2.initialize(&admin, &fee_collector, &0_u32);
    // Set a minimum of 100 stroops.
    c2.set_amount_limits(&admin, &100_i128, &(crate::MAX_ESCROW_AMOUNT));

    let a = make_token(&fx.env);
    let b = make_token(&fx.env);

    // amounts[0] = 1_000 (passes), amounts[1] = 50 (below 100 minimum).
    let result = c2.try_create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address]),
        &vec_i128(&fx.env, &[1_000, 50]),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    assert_eq!(result, Err(Ok(ContractError::AmountBelowMinimum)));
}

#[test]
fn create_basket_escrow_rejects_secondary_amount_above_maximum() {
    // When the admin configures a MaxAmount, every basket entry must stay at or
    // below it. Here the primary passes but the secondary is above the cap.
    let fx = setup();
    let admin = Address::generate(&fx.env);
    let fee_collector = Address::generate(&fx.env);
    let contract_id = fx.env.register(crate::Escrow, ());
    let c2 = EscrowClient::new(&fx.env, &contract_id);
    c2.initialize(&admin, &fee_collector, &0_u32);
    // Cap at 10_000 stroops.
    c2.set_amount_limits(&admin, &1_i128, &10_000_i128);

    let a = make_token(&fx.env);
    let b = make_token(&fx.env);

    // amounts[0] = 1_000 (passes), amounts[1] = 10_001 (exceeds 10_000 cap).
    let result = c2.try_create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address]),
        &vec_i128(&fx.env, &[1_000, 10_001]),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    assert_eq!(result, Err(Ok(ContractError::AmountExceedsMaximum)));
}

#[test]
fn create_basket_escrow_with_all_valid_amounts_succeeds() {
    // Happy-path smoke test for the validation introduced in #807: a basket
    // where every amount is > 0 and within the default limits must be created.
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

    let entries = client.get_basket_tokens(&escrow_id);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries.get(0).unwrap().amount, 1_000);
    assert_eq!(entries.get(1).unwrap().amount, 500);
    assert_eq!(entries.get(2).unwrap().amount, 200);
}

// ── Issue #808 – ConflictingRoles check for basket funding ───────────────────

#[test]
fn fund_basket_escrow_rejects_buyer_equal_to_seller() {
    // INVARIANTS.md I4: buyer and seller must be distinct.
    // fund_basket_escrow must enforce this the same way fund_escrow does.
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let primary = make_token(&fx.env);

    // Create a basket escrow with no pre-set buyer so anyone can attempt to fund.
    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&primary.address]),
        &vec_i128(&fx.env, &[1_000]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    primary.admin.mint(&fx.seller, &1_000);

    // The seller attempts to fund their own escrow — must be rejected.
    assert_eq!(
        client.try_fund_basket_escrow(&escrow_id, &fx.seller),
        Err(Ok(ContractError::ConflictingRoles))
    );

    // No funds should have moved; escrow stays Pending.
    assert_eq!(primary.token.balance(&fx.seller), 1_000);
    assert_eq!(primary.token.balance(&fx.contract_id), 0);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Pending);
}

#[test]
fn fund_basket_escrow_rejects_buyer_equal_to_resolver() {
    // INVARIANTS.md I4: buyer and resolver must be distinct.
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let primary = make_token(&fx.env);

    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&primary.address]),
        &vec_i128(&fx.env, &[1_000]),
        &0_u32,
        &SHIPPING_WINDOW,
    );

    primary.admin.mint(&fx.resolver, &1_000);

    // The resolver attempts to fund — must be rejected.
    assert_eq!(
        client.try_fund_basket_escrow(&escrow_id, &fx.resolver),
        Err(Ok(ContractError::ConflictingRoles))
    );

    assert_eq!(primary.token.balance(&fx.resolver), 1_000);
    assert_eq!(primary.token.balance(&fx.contract_id), 0);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Pending);
}

#[test]
fn create_basket_escrow_accepts_basket_at_max_size() {
    // Issue #820: MAX_BASKET_SIZE itself must still be accepted.
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);

    let mut addrs = Vec::new(&fx.env);
    let mut amounts = Vec::new(&fx.env);
    for _ in 0..MAX_BASKET_SIZE {
        addrs.push_back(Address::generate(&fx.env));
        amounts.push_back(100_i128);
    }

    let escrow_id = client.create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &addrs,
        &amounts,
        &0_u32,
        &SHIPPING_WINDOW,
    );

    assert_eq!(client.get_basket_tokens(&escrow_id).len(), MAX_BASKET_SIZE);
}

#[test]
fn create_basket_escrow_rejects_basket_over_max_size() {
    // Issue #820: create_basket_escrow must cap basket length rather than
    // allowing unbounded iteration in save_basket_tokens/payout_basket_tokens.
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);

    let mut addrs = Vec::new(&fx.env);
    let mut amounts = Vec::new(&fx.env);
    for _ in 0..(MAX_BASKET_SIZE + 1) {
        addrs.push_back(Address::generate(&fx.env));
        amounts.push_back(100_i128);
    }

    let result = client.try_create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &addrs,
        &amounts,
        &0_u32,
        &SHIPPING_WINDOW,
    );
    assert_eq!(result, Err(Ok(ContractError::BasketTokenMismatch)));
}

#[test]
fn create_basket_escrow_rejects_duplicate_secondary_token() {
    // Issue #821: create_basket_escrow must reject duplicate tokens so that
    // fund_escrow/payout_basket_tokens (which transfer/pay out every basket
    // entry individually) cannot double-fund or double-pay a repeated token.
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);

    let result = client.try_create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address, &b.address]),
        &vec_i128(&fx.env, &[1_000, 300, 300]),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    assert_eq!(result, Err(Ok(ContractError::BasketTokenMismatch)));
}

#[test]
fn create_basket_escrow_rejects_duplicate_primary_token() {
    // A duplicated primary token (index 0 repeated later in the basket) must
    // also be rejected — fund_escrow only ever transfers the primary amount
    // once (via escrow.amount) and skips every basket entry matching it, so
    // a duplicate would be silently dropped rather than double-handled, but
    // rejecting all duplicates uniformly keeps the invariant simple to audit.
    let fx = setup();
    let client = EscrowClient::new(&fx.env, &fx.contract_id);
    let a = make_token(&fx.env);
    let b = make_token(&fx.env);

    let result = client.try_create_basket_escrow(
        &fx.seller,
        &None::<Address>,
        &fx.resolver,
        &vec_addr(&fx.env, &[&a.address, &b.address, &a.address]),
        &vec_i128(&fx.env, &[1_000, 300, 200]),
        &0_u32,
        &SHIPPING_WINDOW,
    );
    assert_eq!(result, Err(Ok(ContractError::BasketTokenMismatch)));
}
