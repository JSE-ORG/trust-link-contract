#![cfg(test)]

//! SEP-41 token compatibility tests.
//!
//! The contract stores the token address in `EscrowData.token` and instantiates
//! `token::Client` from that address at runtime in both `fund_escrow` and every
//! payout path (`deduct_and_transfer`).  These tests verify that the full
//! lifecycle works correctly with a generic SEP-41 token that is not USDC.

use crate::EscrowState;
use crate::test_helpers::setup_contract;
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token, Address, Env, IntoVal, String as SorobanString, Symbol, TryFromVal, Val,
    BytesN,
};

/// Register a fresh Stellar asset contract (generic SEP-41 token).
fn register_sep41_token(env: &Env) -> Address {
    env.register_stellar_asset_contract(Address::generate(env))
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    token::Client::new(env, token).balance(who)
}

fn has_event<T, F>(env: &Env, contract_id: &Address, topic: &str, predicate: F) -> bool
where
    T: TryFromVal<Env, Val>,
    F: Fn(&T) -> bool,
{
    let expected_topic = Symbol::new(env, topic);
    env.events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .iter()
        .any(|event| match &event.body {
            soroban_sdk::xdr::ContractEventBody::V0(v0) => {
                let Some(topic) = v0.topics.iter().next() else {
                    return false;
                };

                let Ok(topic) = Symbol::try_from_val(env, topic) else {
                    return false;
                };
                if topic != expected_topic {
                    return false;
                }

                let Ok(data) = Val::try_from_val(env, &v0.data) else {
                    return false;
                };

                T::try_from_val(env, &data)
                    .map(|event| predicate(&event))
                    .unwrap_or(false)
            }
            _ => false,
        })
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

    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &500_i128, &100_u32, &3600_u64);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK001"));

    assert!(has_event::<crate::EscrowCreated, _>(&env, &contract_id, "escrow_created", |event| {
        event.escrow_id == id
    }));
    assert!(has_event::<crate::EscrowFunded, _>(&env, &contract_id, "escrow_funded", |event| {
        event.escrow_id == id && event.buyer == buyer
    }));
    assert!(has_event::<crate::EscrowShipped, _>(&env, &contract_id, "escrow_shipped", |event| {
        event.escrow_id == id && event.seller == seller
    }));

    assert_eq!(client.get_escrow(&id).state, EscrowState::Shipped);
    assert_eq!(balance(&env, &token, &buyer), 0);
    assert_eq!(balance(&env, &token, &contract_id), 500);

    client.confirm_delivery(&buyer, &id);

    assert!(has_event::<crate::EscrowCompleted, _>(&env, &contract_id, "escrow_completed", |event| {
        event.escrow_id == id && event.recipient == seller
    }));

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

    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &1000_i128, &0_u32, &3600_u64);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-AUTO"));
    env.ledger().set_timestamp(1_700_000_000);
    client.record_delivery(&admin, &id);

    // Advance 48 hours past delivery.
    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.delivered_at.unwrap() + 172_801);
    client.auto_release(&id);

    assert!(has_event::<crate::AutoReleased, _>(&env, &contract_id, "auto_released", |event| {
        event.escrow_id == id && event.seller == seller
    }));

    assert_eq!(balance(&env, &token, &seller), 1000);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
}

#[test]
fn test_sep41_dispute_and_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 800);

    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &800_i128, &0_u32, &3600_u64);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-DISPUTE"));

    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "defective"),
        &SorobanString::from_str(&env, "item was broken"),
        &BytesN::from_array(&env, &[0xde; 32]),
    );

    assert!(has_event::<crate::DisputeRaised, _>(&env, &contract_id, "dispute_raised", |event| {
        event.escrow_id == id && event.buyer == buyer
    }));

    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);

    assert!(has_event::<crate::DisputeResolved, _>(&env, &contract_id, "dispute_resolved", |event| {
        event.escrow_id == id && event.resolution == crate::ResolutionType::Refund && event.recipient == buyer
    }));

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

    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &3600_u64);
    // Verify the stored token address matches what was passed in
    assert_eq!(client.get_escrow(&id).token, token);
}

#[test]
fn test_sep41_cancel_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 1000);

    // Create escrow (starts in Pending state)
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &1000_i128, &0_u32, &3600_u64);

    let escrow_before = client.get_escrow(&id);
    assert_eq!(escrow_before.state, EscrowState::Pending);

    // Seller cancels the unfunded escrow
    client.cancel_escrow(&seller, &id);

    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, EscrowState::Canceled);

    // Verify escrow_cancelled event
    assert!(has_event::<crate::EscrowCancelled, _>(&env, &contract_id, "escrow_cancelled", |event| {
        event.escrow_id == id && event.seller == seller
    }));

    // Verify it cannot be funded
    let fund_result = client.try_fund_escrow(&id, &buyer);
    assert!(matches!(fund_result, Err(Ok(crate::ContractError::InvalidState))));
}

#[test]
fn test_sep41_dispute_and_release() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_sep41_token(&env);
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    // Set arbitration fee to 50 BPS (0.5%)
    client.set_arbitration_fee(&admin, &50_u32);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint(&env, &token, &buyer, 1000);

    // Create escrow with 1000 amount, 100 BPS (1.0%) fee
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &1000_i128, &100_u32, &3600_u64);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-RELEASE"));

    // Buyer raises a dispute
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "defective"),
        &SorobanString::from_str(&env, "item was defective"),
        &BytesN::from_array(&env, &[0xdf; 32]),
    );

    assert!(has_event::<crate::DisputeRaised, _>(&env, &contract_id, "dispute_raised", |event| {
        event.escrow_id == id && event.buyer == buyer
    }));

    // Resolver decides in favor of seller (Release)
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Release);

    assert!(has_event::<crate::DisputeResolved, _>(&env, &contract_id, "dispute_resolved", |event| {
        event.escrow_id == id && event.resolution == crate::ResolutionType::Release && event.recipient == seller
    }));

    // Calculations:
    // arbitration_fee = 1000 * 50 / 10000 = 5
    // escrow.amount becomes 995
    // escrow_fee = 995 * 100 / 10000 = 9
    // net payout = 995 - 9 = 986
    // fees retained in vault = 5 + 9 = 14
    assert_eq!(balance(&env, &token, &seller), 986);
    assert_eq!(balance(&env, &token, &buyer), 0);
    assert_eq!(balance(&env, &token, &contract_id), 14);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);

    // Verify fee tracking
    assert_eq!(client.get_total_arbitration_fees(&token), 5);

    // Admin withdraws accumulated fees
    let withdraw_to = Address::generate(&env);
    client.withdraw_fees(&admin, &token, &withdraw_to, &14_i128);
    assert_eq!(balance(&env, &token, &withdraw_to), 14);
    assert_eq!(balance(&env, &token, &contract_id), 0);
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

    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &1000_i128, &0_u32, &3600_u64);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-AUTO-FEES"));
    env.ledger().set_timestamp(1_700_000_000);
    client.record_delivery(&admin, &id);

    // Advance 48 hours past delivery.
    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.delivered_at.unwrap() + 172_801);
    client.auto_release(&id);

    assert!(has_event::<crate::AutoReleased, _>(&env, &contract_id, "auto_released", |event| {
        event.escrow_id == id && event.seller == seller
    }));

    // Calculation:
    // fee_bps = 100 BPS (1%)
    // fee = 1000 * 100 / 10000 = 10
    // net = 1000 - 10 = 990
    assert_eq!(balance(&env, &token, &seller), 990);
    assert_eq!(balance(&env, &token, &fee_collector), 10);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
}
