#![cfg(test)]
//! Focused tests for `auto_release` rejection paths. The happy path and the
//! window/dispute-deadline cases live in `test.rs`
//! (`test_auto_release*`) and `test_dispute_window.rs`.

use crate::{ContractError, Escrow, EscrowClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, IntoVal, String as SorobanString,
};

// auto_release on a Shipped escrow with no recorded delivery is rejected.
#[test]
fn auto_release_without_delivery_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount: i128 = 1000;
    let seller_val = seller.clone().into_val(&env);
    let escrow_id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token_addr,
        &amount,
        &0_u32,
        &3600_u64,
    );
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &amount);
    client.fund_escrow(&escrow_id, &buyer);
    env.ledger().set_timestamp(env.ledger().timestamp() + 3601);
    client.mark_shipped(
        &seller,
        &escrow_id,
        &SorobanString::from_str(&env, "TRACK-X"),
    );

    // Do NOT record delivery. auto_release must reject with DeliveryNotRecorded.
    assert_eq!(
        client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::DeliveryNotRecorded)),
    );
}

// auto_release before the escrow is shipped (still Funded) is rejected because
// the buyer's dispute window has not opened yet.
#[test]
fn auto_release_called_while_funded_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount: i128 = 500;
    let seller_val = seller.clone().into_val(&env);
    let escrow_id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token_addr,
        &amount,
        &0_u32,
        &3600_u64,
    );
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &amount);
    client.fund_escrow(&escrow_id, &buyer);

    // Escrow is Funded but not Shipped - auto_release should reject with DeliveryBeforeDisputeWindow.
    assert_eq!(
        client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::DeliveryBeforeDisputeWindow)),
    );
}
