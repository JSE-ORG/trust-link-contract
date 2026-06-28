#![cfg(test)]

//! Tests for the `multicall` batching entry-point (issue #379).
//!
//! `multicall` allows multiple contract calls to be dispatched within a single
//! Stellar transaction, reducing the total transaction count to 1.

use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env, IntoVal, Symbol, Vec,
};
use crate::{ContractCall, ContractError, Escrow, EscrowClient, EscrowState, Payee};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin        = Address::generate(&env);
    let seller       = Address::generate(&env);
    let buyer        = Address::generate(&env);
    let resolver     = Address::generate(&env);
    let token_admin  = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token        = env.register_stellar_asset_contract(token_admin);

    (env, admin, seller, buyer, resolver, token, fee_collector)
}

fn single_payee(env: &Env, address: &Address) -> Vec<Payee> {
    let mut p = Vec::new(env);
    p.push_back(Payee { address: address.clone(), bps: 10_000 });
    p
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `multicall` with a single `fund_escrow` call returns one result and the
/// escrow transitions from Pending to Funded.
#[test]
fn test_multicall_single_call_fund_escrow() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);

    // Create the escrow normally – we don't batch this step because create
    // returns a non-unit value we need to capture.
    let payees = single_payee(&env, &seller);
    let id = client.create_escrow(&payees, &Some(buyer.clone()), &resolver, &token, &1_000_i128, &0_u32, &0_u32, &3600_u64);

    // Mint tokens for the buyer.
    mint(&env, &token, &buyer, 1_000);

    // Build a one-element batch: fund_escrow(id, buyer)
    let mut args: Vec<soroban_sdk::Val> = Vec::new(&env);
    args.push_back(id.into_val(&env));
    args.push_back(buyer.clone().into_val(&env));

    let mut calls: Vec<ContractCall> = Vec::new(&env);
    calls.push_back(ContractCall {
        function: Symbol::new(&env, "fund_escrow"),
        args,
    });

    let results = client.multicall(&calls);
    assert_eq!(results.len(), 1);

    // Verify the escrow is now Funded.
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Funded);
}

/// `multicall` with two sequential read-only calls (get_escrow, get_escrow)
/// returns two results without mutating state.
#[test]
fn test_multicall_two_get_escrow_calls() {
    let (env, admin, seller, _buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let payees = single_payee(&env, &seller);
    let id = client.create_escrow(&payees, &None::<Address>, &resolver, &token, &1_000_i128, &0_u32, &0_u32, &3600_u64);

    let mut args1: Vec<soroban_sdk::Val> = Vec::new(&env);
    args1.push_back(id.into_val(&env));

    let mut args2: Vec<soroban_sdk::Val> = Vec::new(&env);
    args2.push_back(id.into_val(&env));

    let mut calls: Vec<ContractCall> = Vec::new(&env);
    calls.push_back(ContractCall { function: Symbol::new(&env, "get_escrow"), args: args1 });
    calls.push_back(ContractCall { function: Symbol::new(&env, "get_escrow"), args: args2 });

    let results = client.multicall(&calls);
    assert_eq!(results.len(), 2);
}

/// `multicall` is blocked when the contract is paused.
#[test]
fn test_multicall_blocked_when_paused() {
    let (env, admin, seller, _buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    // Create an escrow so there is something to call on.
    let payees = single_payee(&env, &seller);
    let id = client.create_escrow(&payees, &None::<Address>, &resolver, &token, &1_000_i128, &0_u32, &0_u32, &3600_u64);

    client.pause_contract(&admin);

    let mut args: Vec<soroban_sdk::Val> = Vec::new(&env);
    args.push_back(id.into_val(&env));

    let mut calls: Vec<ContractCall> = Vec::new(&env);
    calls.push_back(ContractCall { function: Symbol::new(&env, "get_escrow"), args });

    let result = client.try_multicall(&calls);
    assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
}

/// An empty `multicall` is a no-op that returns an empty vec.
#[test]
fn test_multicall_empty_batch() {
    let (env, admin, seller, _buyer, resolver, token, fee_collector) = setup_env();
    let _ = (seller, resolver, token, fee_collector);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let fee_collector2 = Address::generate(&env);
    client.initialize(&admin, &fee_collector2, &0_u32);

    let calls: Vec<ContractCall> = Vec::new(&env);
    let results = client.multicall(&calls);
    assert_eq!(results.len(), 0);
}

/// Batch two mutating calls: mark_shipped followed by get_escrow.
/// After the batch the escrow is in the Shipped state.
#[test]
fn test_multicall_mark_shipped_then_get_escrow() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let payees = single_payee(&env, &seller);
    let id = client.create_escrow(&payees, &Some(buyer.clone()), &resolver, &token, &1_000_i128, &0_u32, &0_u32, &3600_u64);
    mint(&env, &token, &buyer, 1_000);
    client.fund_escrow(&id, &buyer);

    // Batch: mark_shipped then get_escrow
    let tracking = soroban_sdk::String::from_str(&env, "BATCH-TRK-001");
    let mut args_ship: Vec<soroban_sdk::Val> = Vec::new(&env);
    args_ship.push_back(seller.clone().into_val(&env));
    args_ship.push_back(id.into_val(&env));
    args_ship.push_back(tracking.into_val(&env));

    let mut args_get: Vec<soroban_sdk::Val> = Vec::new(&env);
    args_get.push_back(id.into_val(&env));

    let mut calls: Vec<ContractCall> = Vec::new(&env);
    calls.push_back(ContractCall { function: Symbol::new(&env, "mark_shipped"), args: args_ship });
    calls.push_back(ContractCall { function: Symbol::new(&env, "get_escrow"), args: args_get });

    let results = client.multicall(&calls);
    assert_eq!(results.len(), 2);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Shipped);
}
