#![cfg(test)]

use crate::helpers::payout::{calculate_fee, calculate_protocol_fee};
use crate::{Escrow, EscrowClient, Payee, ResolutionType};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, IntoVal, Symbol, Vec,
};

fn setup(env: &Env) -> (Address, Address, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let fee_collector = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    (admin, seller, buyer, resolver, fee_collector, token)
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    token::Client::new(env, token).balance(who)
}

/// Issue #1260: for every non-zero principal amount from 1 through 10_000
/// stroops, a non-zero fee rate must collect at least 1 stroop and the
/// proportional fee must be floored otherwise. The conservation invariant
/// `net + fee == amount` must hold exactly for every case.
#[test]
fn test_calculate_fee_collects_minimum_one_stroop() {
    for fee_bps in [1_u32, 50, 100, 300] {
        for amount in 1_i128..=10_000 {
            let fee = calculate_fee(amount, fee_bps)
                .unwrap_or_else(|e| panic!("calculate_fee({amount}, {fee_bps}) failed: {e:?}"));
            let expected = core::cmp::max(1, amount * fee_bps as i128 / 10_000);

            assert_eq!(
                fee, expected,
                "fee mismatch for amount={amount}, fee_bps={fee_bps}"
            );
            assert!(
                fee >= 1,
                "a non-zero rate on a non-zero amount must not waive the fee: \
                 amount={amount}, fee_bps={fee_bps}"
            );

            let (protocol_fee, net) = calculate_protocol_fee(amount, fee_bps).unwrap_or_else(|e| {
                panic!("calculate_protocol_fee({amount}, {fee_bps}) failed: {e:?}")
            });
            assert_eq!(protocol_fee, fee);
            assert_eq!(
                net + fee,
                amount,
                "net + fee must equal amount: amount={amount}, fee_bps={fee_bps}, \
                 fee={fee}, net={net}"
            );
            assert!(net >= 0, "net must be non-negative");
        }
    }
}

/// A zero rate and a zero principal both legitimately produce a zero fee.
#[test]
fn test_calculate_fee_zero_rate_or_zero_principal() {
    for amount in [0_i128, 1, 10_000] {
        assert_eq!(calculate_fee(amount, 0).unwrap(), 0);
    }
    for fee_bps in [1_u32, 50, 300, 10_000] {
        assert_eq!(calculate_fee(0, fee_bps).unwrap(), 0);
    }
}

/// With amount = 1 stroop and any non-zero fee_bps the proportional fee would
/// truncate to 0; the minimum-fee rule now collects exactly 1 stroop and the
/// recipient receives 0.
#[test]
fn test_fee_minimum_on_one_stroop_confirm_delivery() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    client.set_protocol_fee(&admin, &300_u32);

    mint(&env, &token, &buyer, 1);

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
        &1_i128,
        &300_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-ONE"),
    );

    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.dispute_deadline() + 1);
    client.confirm_delivery(&buyer, &id, &false);

    // fee = max(1, 1 * 300 / 10_000) = 1  →  net = 0. The fee is accumulated
    // for the fee collector rather than transferred immediately.
    assert_eq!(balance(&env, &token, &seller), 0);
    assert_eq!(client.get_accumulated_fees(&token), 1);
    assert_eq!(balance(&env, &token, &contract_id), 1);

    // The collector can pull the 1-stroop fee out of the contract.
    client.claim_accumulated_fees(&fee_collector, &token);
    assert_eq!(balance(&env, &token, &fee_collector), 1);
    assert_eq!(balance(&env, &token, &contract_id), 0);
}

#[test]
fn test_fee_minimum_on_one_stroop_auto_release() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    // `auto_release` settles at the current global protocol rate.
    client.set_protocol_fee(&admin, &300_u32);

    mint(&env, &token, &buyer, 1);

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
        &1_i128,
        &300_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-FEE-AUTO"),
    );
    env.ledger().set_timestamp(1_700_000_000);
    crate::test_helpers::record_delivery_timelocked(&env, &client, &admin, id);

    // Advance 48 hours past delivery.
    let escrow = client.get_escrow(&id);
    env.ledger()
        .set_timestamp(escrow.delivered_at.unwrap() + 172_801);
    client.auto_release(&id);

    assert_eq!(balance(&env, &token, &seller), 0);
    assert_eq!(client.get_accumulated_fees(&token), 1);
    assert_eq!(balance(&env, &token, &contract_id), 1);

    client.claim_accumulated_fees(&fee_collector, &token);
    assert_eq!(balance(&env, &token, &fee_collector), 1);
    assert_eq!(balance(&env, &token, &contract_id), 0);
}

#[test]
fn test_fee_minimum_on_one_stroop_resolve_dispute_release() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    mint(&env, &token, &buyer, 1);

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
        &1_i128,
        &300_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-FEE-REL"),
    );
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "FRAUD"),
        &soroban_sdk::Bytes::from_slice(&env, b"desc"),
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &id);

    assert_eq!(balance(&env, &token, &seller), 0);
    assert_eq!(client.get_accumulated_fees(&token), 1);
    assert_eq!(balance(&env, &token, &contract_id), 1);

    client.claim_accumulated_fees(&fee_collector, &token);
    assert_eq!(balance(&env, &token, &fee_collector), 1);
    assert_eq!(balance(&env, &token, &contract_id), 0);
}

#[test]
fn test_fee_minimum_on_one_stroop_resolve_dispute_refund() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    mint(&env, &token, &buyer, 1);

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
        &1_i128,
        &300_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-FEE-REF"),
    );
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "FRAUD"),
        &soroban_sdk::Bytes::from_slice(&env, b"desc"),
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &id);

    // The protocol fee applies on refunds too: 1 stroop fee, 0 back to buyer.
    assert_eq!(balance(&env, &token, &buyer), 0);
    assert_eq!(client.get_accumulated_fees(&token), 1);
    assert_eq!(balance(&env, &token, &contract_id), 1);

    client.claim_accumulated_fees(&fee_collector, &token);
    assert_eq!(balance(&env, &token, &fee_collector), 1);
    assert_eq!(balance(&env, &token, &contract_id), 0);
}
