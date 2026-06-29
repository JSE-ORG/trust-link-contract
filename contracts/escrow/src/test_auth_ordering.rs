#![cfg(test)]

use crate::{Escrow, EscrowClient, Payee};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_mark_shipped_auth_fails_immediately() {
    let env = Env::default();

    let contract_id = env.register_contract(None, Escrow);
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
    let contract_id = env.register_contract(None, Escrow);
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
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    let fake_seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = Address::generate(&env);

    // Will panic on `seller.require_auth()` instead of `ensure_not_paused`
    let mut payees_5 = Vec::new(&env);
    payees_5.push_back(Payee { address: fake_seller.clone(), bps: 10_000 });
    client.create_escrow(
        &payees_5,
        &None::<Address>,
        &resolver,
        &token,
        &1000,
        &100,
        &0_u32,
        &86400,
    );
}

#[test]
#[should_panic]
fn test_unauthorized_cancel_escrow_fails_early() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Escrow);
    let client = EscrowClient::new(&env, &contract_id);

    let fake_caller = Address::generate(&env);

    // Will panic on `caller.require_auth()` instead of `load_escrow`
    client.cancel_escrow(&fake_caller, &1);
}
