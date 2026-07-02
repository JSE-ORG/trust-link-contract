#![cfg(test)]

//! Tests for the buyer-initiated refund-request flow (issue #429).
//!
//! Flow: while an escrow is `Funded` (and not yet `Shipped`), the buyer may call
//! `request_refund`, moving it to the `RefundRequested` sub-state. From there a
//! payee (seller) may either `approve_refund` (refunding the buyer and moving to
//! `Refunded`) or `deny_refund` (returning the escrow to `Funded`).

use crate::test_helpers::{create_funded_escrow, setup_contract};
use crate::types::EscrowState;
use crate::ContractError;
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env, String,
};

const AMOUNT: i128 = 1_000_000;
const SHIPPING_WINDOW: u64 = 86_400;

struct Fixture {
    env: Env,
    client: crate::EscrowClient<'static>,
    contract_id: Address,
    seller: Address,
    buyer: Address,
    token: Address,
    escrow_id: u64,
}

fn funded_fixture() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let escrow_id = create_funded_escrow(
        &env,
        &client,
        &seller,
        &buyer,
        &resolver,
        &token,
        AMOUNT,
        0_u32,
        SHIPPING_WINDOW,
    );

    Fixture {
        env,
        client,
        contract_id,
        seller,
        buyer,
        token,
        escrow_id,
    }
}

#[test]
fn buyer_can_request_refund_while_funded() {
    let f = funded_fixture();

    assert_eq!(f.client.get_escrow(&f.escrow_id).state, EscrowState::Funded);

    f.client.request_refund(&f.buyer, &f.escrow_id);

    assert_eq!(
        f.client.get_escrow(&f.escrow_id).state,
        EscrowState::RefundRequested
    );
    // Funds remain locked in the contract until the request is approved.
    assert_eq!(token::Client::new(&f.env, &f.token).balance(&f.contract_id), AMOUNT);
}

#[test]
fn buyer_cannot_request_refund_after_shipment() {
    let f = funded_fixture();

    // Seller ships, moving the escrow out of `Funded`.
    f.client
        .mark_shipped(&f.seller, &f.escrow_id, &String::from_str(&f.env, "TRACK-1"));
    assert_eq!(f.client.get_escrow(&f.escrow_id).state, EscrowState::Shipped);

    // Buyer can no longer force a refund once shipped.
    let res = f.client.try_request_refund(&f.buyer, &f.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::InvalidStateTransition)));
    assert_eq!(f.client.get_escrow(&f.escrow_id).state, EscrowState::Shipped);
}

#[test]
fn non_buyer_cannot_request_refund() {
    let f = funded_fixture();
    let stranger = Address::generate(&f.env);

    let res = f.client.try_request_refund(&stranger, &f.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::NotAuthorized)));
    assert_eq!(f.client.get_escrow(&f.escrow_id).state, EscrowState::Funded);
}

#[test]
fn seller_can_approve_refund_and_buyer_is_paid_back() {
    let f = funded_fixture();
    let token_client = token::Client::new(&f.env, &f.token);

    f.client.request_refund(&f.buyer, &f.escrow_id);
    assert_eq!(token_client.balance(&f.buyer), 0);

    f.client.approve_refund(&f.seller, &f.escrow_id);

    assert_eq!(
        f.client.get_escrow(&f.escrow_id).state,
        EscrowState::Refunded
    );
    // Buyer is made whole; the contract no longer holds the funds.
    assert_eq!(token_client.balance(&f.buyer), AMOUNT);
    assert_eq!(token_client.balance(&f.contract_id), 0);
}

#[test]
fn seller_can_deny_refund_and_escrow_returns_to_funded() {
    let f = funded_fixture();
    let token_client = token::Client::new(&f.env, &f.token);

    f.client.request_refund(&f.buyer, &f.escrow_id);
    assert_eq!(
        f.client.get_escrow(&f.escrow_id).state,
        EscrowState::RefundRequested
    );

    // Seller denies: escrow goes back to `Funded`, funds stay in escrow.
    f.client.deny_refund(&f.seller, &f.escrow_id);

    assert_eq!(f.client.get_escrow(&f.escrow_id).state, EscrowState::Funded);
    assert_eq!(token_client.balance(&f.contract_id), AMOUNT);
    assert_eq!(token_client.balance(&f.buyer), 0);

    // After a denial the normal shipment flow can still proceed.
    f.client
        .mark_shipped(&f.seller, &f.escrow_id, &String::from_str(&f.env, "TRACK-2"));
    assert_eq!(f.client.get_escrow(&f.escrow_id).state, EscrowState::Shipped);
}

#[test]
fn non_payee_cannot_deny_refund() {
    let f = funded_fixture();
    f.client.request_refund(&f.buyer, &f.escrow_id);

    let stranger = Address::generate(&f.env);
    let res = f.client.try_deny_refund(&stranger, &f.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::NotAuthorized)));
    assert_eq!(
        f.client.get_escrow(&f.escrow_id).state,
        EscrowState::RefundRequested
    );
}

#[test]
fn deny_refund_requires_refund_requested_state() {
    let f = funded_fixture();

    // No refund requested yet -> escrow is still `Funded`.
    let res = f.client.try_deny_refund(&f.seller, &f.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::InvalidStateTransition)));
}
