#![cfg(test)]
#![allow(dead_code)]

use crate::{Escrow, EscrowClient, Payee};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, IntoVal, Vec,
};

pub fn setup_contract(env: &Env) -> (Address, EscrowClient<'_>, Address, Address) {
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    client.initialize(&admin, &fee_collector, &0_u32);
    (contract_id, client, admin, fee_collector)
}

pub fn mint_token(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

pub fn advance_time(env: &Env, seconds: u64) {
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + seconds);
}

pub fn create_funded_escrow(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    buyer: &Address,
    resolver: &Address,
    token: &Address,
    amount: i128,
    fee_bps: u32,
    shipping_window: u64,
) -> u64 {
    mint_token(env, token, buyer, amount);
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(env);
    let id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        resolver,
        token,
        &amount,
        &fee_bps,
        &shipping_window,
    );
    client.fund_escrow(&id, buyer);
    id
}

pub fn record_delivery_timelocked(
    env: &Env,
    client: &EscrowClient,
    admin: &Address,
    escrow_id: u64,
) {
    client.propose_record_delivery(admin, &escrow_id);
    advance_time(env, crate::DELIVERY_TIMELOCK);
    client.record_delivery(admin, &escrow_id);
}

#[test]
fn test_create_funded_escrow_forwards_shipping_window() {
    // Issue #1108: Assert that create_funded_escrow forwards shipping_window correctly.
    // The helper passes shipping_window to create_escrow_8, and we must verify
    // the stored EscrowData.shipping_window matches the passed value (not DEFAULT_SHIPPING_WINDOW).
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = Address::generate(&env);

    let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    token_client.mock_all_auths();

    // Use a non-default shipping window to verify forwarding
    let custom_shipping_window = 7200_u64; // 2 hours, not DEFAULT_SHIPPING_WINDOW (3600)
    let escrow_id = create_funded_escrow(
        &env,
        &client,
        &seller,
        &buyer,
        &resolver,
        &token,
        1000_0000000,
        100,
        custom_shipping_window,
    );

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(
        escrow.shipping_window,
        custom_shipping_window,
        "create_funded_escrow must forward shipping_window to create_escrow_8"
    );
}
