#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, Vec,
};
use crate::{ContractError, Escrow, EscrowClient, Payee};

fn setup(env: &Env) -> (EscrowClient, Address, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin.clone());

    let token_client = token::StellarAssetClient::new(env, &token);
    token_client.mint(&buyer, &10_000_i128);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    let id = client.create_escrow(&payees, &None::<Address>, &resolver, &token, &500_i128, &0_u32, &0_u32, &3600_u64);
    client.fund_escrow(&id, &buyer);

    (client, admin, seller, buyer, resolver, token)
}

// ── Cannot drain while unpaused ─────────────────────────────────────────────

#[test]
fn emergency_drain_fails_when_not_paused() {
    let env = Env::default();
    let (client, _admin, _seller, _buyer, _resolver, _token) = setup(&env);

    // contract is not paused — drain must fail
    let result = client.try_emergency_drain(&1_u64);
    assert_eq!(result, Err(Ok(ContractError::ContractNotPaused)));
}

// ── Both auths verified ──────────────────────────────────────────────────────

#[test]
fn emergency_drain_succeeds_when_paused_and_both_sign() {
    let env = Env::default();
    let (client, admin, _seller, buyer, _resolver, token) = setup(&env);

    client.pause_contract(&admin);
    let buyer_balance_before = token::Client::new(&env, &token).balance(&buyer);

    client.emergency_drain(&1_u64);

    let buyer_balance_after = token::Client::new(&env, &token).balance(&buyer);
    // full amount returned to buyer
    assert_eq!(buyer_balance_after - buyer_balance_before, 500_i128);

    // escrow state is now Refunded — cannot drain again
    let result = client.try_emergency_drain(&1_u64);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}

// ── Cannot drain in terminal / pre-fund states ───────────────────────────────

#[test]
fn emergency_drain_fails_on_pending_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = env.register_stellar_asset_contract(Address::generate(&env));

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    let id = client.create_escrow(&payees, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &0_u32, &3600_u64);

    client.pause_contract(&admin);

    // Pending escrow (no buyer, not funded) — cannot drain
    let result = client.try_emergency_drain(&id);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}

#[test]
fn emergency_drain_fails_on_completed_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());

    let token_client = token::StellarAssetClient::new(&env, &token);
    token_client.mint(&buyer, &10_000_i128);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    let id = client.create_escrow(&payees, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &0_u32, &1_u64);
    client.fund_escrow(&id, &buyer);

    // advance ledger past shipping + dispute windows so auto_release is possible
    env.ledger().set_timestamp(500_000);
    client.auto_release(&id);

    client.pause_contract(&admin);

    // Completed escrow — cannot drain
    let result = client.try_emergency_drain(&id);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}
