#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, IntoVal,
};

// Ensure fund_escrow handles funded_at + DISPUTE_WINDOW overflow safely
#[test]
fn test_fund_escrow_dispute_deadline_overflow_is_handled() {
    let env = Env::default();
    env.ledger()
        .set_timestamp(u64::MAX - crate::DISPUTE_WINDOW + 1);
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let contract_id = env.register(Escrow, ());
    let client = super::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    // Create escrow with any fee and shipping_window
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );

    // Set ledger timestamp so funded_at + DISPUTE_WINDOW would overflow
    // funded_at = u64::MAX - DISPUTE_WINDOW + 1
    // moved to setup

    // Mint tokens and attempt to fund - expect ArithmeticOverflow error, not panic
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &1000_i128);
    let res = client.try_fund_escrow(&id, &buyer);
    assert_eq!(res, Err(Ok(ContractError::ArithmeticOverflow)));
}
