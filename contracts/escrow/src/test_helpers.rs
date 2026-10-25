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
fn create_funded_escrow_preserves_shipping_window() {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let issuer = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(issuer).address();
    let shipping_window = 86_400;

    let escrow_id = create_funded_escrow(
        &env,
        &client,
        &seller,
        &buyer,
        &resolver,
        &token,
        1_000,
        0,
        shipping_window,
    );

    assert_eq!(
        EscrowClient::new(&env, &contract_id)
            .get_escrow(&escrow_id)
            .shipping_window,
        shipping_window
    );
}
