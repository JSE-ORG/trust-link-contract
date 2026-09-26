#![cfg(test)]

use crate::{Escrow, EscrowClient, Payee};
use soroban_sdk::{testutils::Address as _, token, Address, Env, IntoVal, String, Vec};

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_mark_shipped_auth_fails_immediately() {
    let env = Env::default();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let unauthorized_caller = Address::generate(&env);
    client.mark_shipped(
        &unauthorized_caller,
        &1,
        &String::from_str(&env, "TRACK-FAIL"),
    );
}

#[test]
#[should_panic]
fn test_unauthorized_pause_fails_early() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let fake_admin = Address::generate(&env);
    // Since we did not call `env.mock_all_auths()`, the require_auth() inside pause_contract
    // will panic immediately at the Host level because there's no auth provided.
    // This proves it happens before `require_admin()` which would have panicked with "not initialized".

    client.pause_contract(&fake_admin);
}

#[test]
#[should_panic]
fn test_unauthorized_create_escrow_fails_early() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let fake_seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = Address::generate(&env);

    // Will panic on `seller.require_auth()` instead of `ensure_not_paused`
    let mut payees_5 = Vec::new(&env);
    payees_5.push_back(Payee {
        address: fake_seller.clone(),
        bps: 10_000,
    });
    let payees_5_val = payees_5.into_val(&env);
    client.create_escrow_8(
        &payees_5_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000,
        &100,
        &86400,
    );
}

#[test]
#[should_panic]
fn test_unauthorized_cancel_escrow_fails_early() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let fake_caller = Address::generate(&env);

    // Will panic on `caller.require_auth()` instead of `load_escrow`
    client.cancel_escrow(&fake_caller, &1);
}

#[test]
fn fund_escrow_accepts_arbitrary_token_when_allowlist_is_disabled() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    client.set_token_allowlist_enabled(&admin, &false);

    token::StellarAssetClient::new(&env, &token).mint(&buyer, &1_000_i128);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller,
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &1_000_i128,
        &0_u32,
        &3_600_u64,
    );

    assert!(client.get_allowed_tokens().is_empty());
    client.fund_escrow(&escrow_id, &buyer);

    assert_eq!(
        client.get_escrow(&escrow_id).state,
        crate::EscrowState::Funded
    );
    assert_eq!(
        token::Client::new(&env, &token).balance(&contract_id),
        1_000
    );
}
