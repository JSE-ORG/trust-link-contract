#![cfg(test)]

//! SEP-41 token compatibility tests.
//!
//! The contract stores the token address in `EscrowData.token` and instantiates
//! `token::Client` from that address at runtime in both `fund_escrow` and every
//! payout path (`transfer_with_protocol_fee`).  These tests verify that the full
//! lifecycle works correctly with a generic SEP-41 token that is not USDC.

use crate::test_helpers::{record_delivery_timelocked, setup_contract};
use crate::{EscrowState, Payee};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, IntoVal, String as SorobanString, Symbol, Vec,
};

/// Register a fresh Stellar asset contract (generic SEP-41 token).
fn register_sep41_token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    token::Client::new(env, token).balance(who)
}

#[test]
fn test_sep41_fund_and_confirm_delivery() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (contract_id, client, admin, fee_collector) = setup_contract(&env);
    client.set_protocol_fee(&admin, &100_u32);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 500);

    let mut payees1 = Vec::new(&env);
    payees1.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees1_val = payees1.into_val(&env);
    let id = client.create_escrow_8(
        &payees1_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &100_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK001"));

    assert_eq!(client.get_escrow(&id).state, EscrowState::Shipped);
    assert_eq!(balance(&env, &token, &buyer), 0);
    assert_eq!(balance(&env, &token, &contract_id), 500);

    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.dispute_deadline + 1);
    client.confirm_delivery(&buyer, &id);

    // 1% fee on 500 = 5 routed to the fee collector; 495 to seller
    assert_eq!(balance(&env, &token, &seller), 495);
    assert_eq!(balance(&env, &token, &fee_collector), 5);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
}

#[test]
fn test_sep41_auto_release() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 1000);

    let mut payees2 = Vec::new(&env);
    payees2.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees2_val = payees2.into_val(&env);
    let id = client.create_escrow_8(
        &payees2_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-AUTO"));
    env.ledger().set_timestamp(1_700_000_000);
    record_delivery_timelocked(&env, &client, &admin, id);

    // Advance 48 hours past delivery.
    let escrow = client.get_escrow(&id);
    env.ledger()
        .set_timestamp(escrow.delivered_at.unwrap() + 172_801);
    client.auto_release(&id);

    assert_eq!(balance(&env, &token, &seller), 1000);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
}

#[test]
fn test_sep41_dispute_and_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 800);

    let mut payees3 = Vec::new(&env);
    payees3.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees3_val = payees3.into_val(&env);
    let id = client.create_escrow_8(
        &payees3_val,
        &None::<Address>,
        &resolver,
        &token,
        &800_i128,
        &0_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &SorobanString::from_str(&env, "TRACK-DISPUTE"),
    );

    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "defective"),
        &SorobanString::from_str(&env, "item was broken"),
        &BytesN::from_array(&env, &[0xde; 32]),
    );

    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);

    // Advance past appeal window and finalize
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
    client.finalize_dispute(&admin, &id);

    // Zero fee — full 800 back to buyer
    assert_eq!(balance(&env, &token, &buyer), 800);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Refunded);
}

#[test]
fn test_sep41_token_address_stored_in_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);

    let mut payees4 = Vec::new(&env);
    payees4.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees4_val = payees4.into_val(&env);
    let id = client.create_escrow_8(
        &payees4_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &3600_u64,
    );
    // Verify the stored token address matches what was passed in
    assert_eq!(client.get_escrow(&id).token, token);
}

#[test]
fn test_sep41_cancel_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 1000);

    // Create escrow (starts in Pending state)
    let mut payees5 = Vec::new(&env);
    payees5.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees5_val = payees5.into_val(&env);
    let id = client.create_escrow_8(
        &payees5_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );

    let escrow_before = client.get_escrow(&id);
    assert_eq!(escrow_before.state, EscrowState::Pending);

    // Seller cancels the unfunded escrow
    client.cancel_escrow(&seller, &id);

    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, EscrowState::Canceled);

    // Verify it cannot be funded
    let fund_result = client.try_fund_escrow(&id, &buyer);
    assert!(matches!(
        fund_result,
        Err(Ok(crate::ContractError::InvalidState))
    ));
}

#[test]
fn test_sep41_dispute_and_release() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (contract_id, client, admin, fee_collector) = setup_contract(&env);

    // Set arbitration fee to 50 BPS (0.5%)
    client.set_arbitration_fee(&admin, &50_u32);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 1000);

    // Create escrow with 1000 amount, 100 BPS (1.0%) fee
    let mut payees6 = Vec::new(&env);
    payees6.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees6_val = payees6.into_val(&env);
    let id = client.create_escrow_8(
        &payees6_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &100_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &SorobanString::from_str(&env, "TRACK-RELEASE"),
    );

    // Buyer raises a dispute
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "defective"),
        &SorobanString::from_str(&env, "item was defective"),
        &BytesN::from_array(&env, &[0xdf; 32]),
    );

    // Resolver decides in favor of seller (Release)
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Release);

    // Advance past appeal window and finalize
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
    client.finalize_dispute(&admin, &id);

    // Calculations:
    // arbitration_fee = 1000 * 50 / 10000 = 5 → fee_collector
    // remaining = 995
    // escrow_fee = 995 * 100 / 10000 = 9 → fee_collector
    // net payout = 995 - 9 = 986 → seller
    // fee_collector total = 5 + 9 = 14
    assert_eq!(balance(&env, &token, &seller), 986);
    assert_eq!(balance(&env, &token, &buyer), 0);
    assert_eq!(balance(&env, &token, &fee_collector), 14);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);

    // Verify fee tracking
    assert_eq!(client.get_total_arbitration_fees(&token), 5);
}

#[test]
fn test_sep41_auto_release_with_fees() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (contract_id, client, admin, fee_collector) = setup_contract(&env);

    // Set global protocol fee rate to 100 BPS (1%)
    client.set_protocol_fee(&admin, &100_u32);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 1000);

    let mut payees7 = Vec::new(&env);
    payees7.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees7_val = payees7.into_val(&env);
    let id = client.create_escrow_8(
        &payees7_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &SorobanString::from_str(&env, "TRACK-AUTO-FEES"),
    );
    env.ledger().set_timestamp(1_700_000_000);
    record_delivery_timelocked(&env, &client, &admin, id);

    // Advance 48 hours past delivery.
    let escrow = client.get_escrow(&id);
    env.ledger()
        .set_timestamp(escrow.delivered_at.unwrap() + 172_801);
    client.auto_release(&id);

    // Calculation:
    // fee_bps = 100 BPS (1%)
    // fee = 1000 * 100 / 10000 = 10
    // net = 1000 - 10 = 990
    assert_eq!(balance(&env, &token, &seller), 990);
    assert_eq!(balance(&env, &token, &fee_collector), 10);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
}
