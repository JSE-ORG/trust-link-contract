#![cfg(test)]

use crate::test_helpers::{advance_time, mint_token, setup_contract};
use crate::{
    Escrow, EscrowClient, EscrowState, Payee, ResolutionType, ResolverSet, TokenEntry,
};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env, IntoVal, String as SorobanString, Vec};

#[derive(Clone, Debug)]
struct GasSample {
    cpu_insns: u64,
    mem_bytes: u64,
}

fn take_budget_snapshot(env: &Env) -> GasSample {
    let b = env.budget();
    GasSample {
        cpu_insns: b.get_cpu_insns_count(),
        mem_bytes: b.get_mem_bytes_count(),
    }
}

fn diff(env: &Env, before: &GasSample) -> GasSample {
    let after = take_budget_snapshot(env);
    GasSample {
        cpu_insns: after.cpu_insns.saturating_sub(before.cpu_insns),
        mem_bytes: after.mem_bytes.saturating_sub(before.mem_bytes),
    }
}

fn print_gas(label: &str, sample: &GasSample) {
    println!(
        "gas_profile | {:<36} | cpu={:>12} | mem={:>10}",
        label, sample.cpu_insns, sample.mem_bytes
    );
}

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

fn make_payees(env: &Env, seller: &Address) -> Vec<Payee> {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    payees
}

#[test]
fn gas_profile_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let before = take_budget_snapshot(&env);
    client.initialize(&admin, &fee_collector, &0_u32);
    let sample = diff(&env, &before);
    print_gas("initialize", &sample);
    assert!(sample.cpu_insns > 0);
}

#[test]
fn gas_profile_set_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);
    let new_admin = Address::generate(&env);

    let before = take_budget_snapshot(&env);
    client.set_admin(&admin, &new_admin);
    let sample = diff(&env, &before);
    print_gas("set_admin", &sample);
}

#[test]
fn gas_profile_set_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    let before = take_budget_snapshot(&env);
    client.set_fee(&admin, &150_u32);
    let sample = diff(&env, &before);
    print_gas("set_fee", &sample);
}

#[test]
fn gas_profile_set_protocol_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    let before = take_budget_snapshot(&env);
    client.set_protocol_fee(&admin, &200_u32);
    let sample = diff(&env, &before);
    print_gas("set_protocol_fee", &sample);
}

#[test]
fn gas_profile_set_arbitration_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    let before = take_budget_snapshot(&env);
    client.set_arbitration_fee(&admin, &100_u32);
    let sample = diff(&env, &before);
    print_gas("set_arbitration_fee", &sample);
}

#[test]
fn gas_profile_create_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);

    let before = take_budget_snapshot(&env);
    let _id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    let sample = diff(&env, &before);
    print_gas("create_escrow", &sample);
}

#[test]
fn gas_profile_create_escrow_multi_resolver() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);
    let token = register_token(&env);
    let mut resolvers = Vec::new(&env);
    resolvers.push_back(r1);
    resolvers.push_back(r2);
    resolvers.push_back(r3);

    let before = take_budget_snapshot(&env);
    let _id = client.create_escrow_multi(
        &seller,
        &None::<Address>,
        &resolvers,
        &2_u32,
        &token,
        &1_000_000_i128,
        &100_u32,
        &3600_u64,
    );
    let sample = diff(&env, &before);
    print_gas("create_escrow_multi(3-of-3)", &sample);
}

#[test]
fn gas_profile_create_escrow_with_fallback() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let primary = Address::generate(&env);
    let backup = Address::generate(&env);
    let token = register_token(&env);

    let before = take_budget_snapshot(&env);
    let _id = client.create_escrow_with_fallback(
        &seller,
        &None::<Address>,
        &primary,
        &backup,
        &(env.ledger().timestamp() + 86_400),
        &token,
        &1_000_000_i128,
        &100_u32,
        &3600_u64,
    );
    let sample = diff(&env, &before);
    print_gas("create_escrow_with_fallback", &sample);
}

#[test]
fn gas_profile_create_basket_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let t1 = register_token(&env);
    let t2 = register_token(&env);
    let t3 = register_token(&env);
    let mut tokens = Vec::new(&env);
    tokens.push_back(t1);
    tokens.push_back(t2);
    tokens.push_back(t3);
    let mut amounts = Vec::new(&env);
    amounts.push_back(1_000_000_i128);
    amounts.push_back(500_000_i128);
    amounts.push_back(250_000_i128);

    let before = take_budget_snapshot(&env);
    let _id = client.create_basket_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &tokens,
        &amounts,
        &100_u32,
        &3600_u64,
    );
    let sample = diff(&env, &before);
    print_gas("create_basket_escrow(3 tokens)", &sample);
}

#[test]
fn gas_profile_fund_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );

    let before = take_budget_snapshot(&env);
    client.fund_escrow(&id, &buyer);
    let sample = diff(&env, &before);
    print_gas("fund_escrow", &sample);
}

#[test]
fn gas_profile_mark_shipped() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);

    let before = take_budget_snapshot(&env);
    client.mark_shipped(
        &seller,
        &id,
        &SorobanString::from_str(&env, "TRACK-ABC-123"),
    );
    let sample = diff(&env, &before);
    print_gas("mark_shipped", &sample);
}

#[test]
fn gas_profile_confirm_delivery() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "T-1"));
    advance_time(&env, 172_800 + 1);

    let before = take_budget_snapshot(&env);
    client.confirm_delivery(&buyer, &id);
    let sample = diff(&env, &before);
    print_gas("confirm_delivery", &sample);
}

#[test]
fn gas_profile_co_signed_release() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);

    let before = take_budget_snapshot(&env);
    client.co_signed_release(&buyer, &id);
    let sample = diff(&env, &before);
    print_gas("co_signed_release", &sample);
}

#[test]
fn gas_profile_auto_release() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);
    advance_time(&env, 172_800 + 3600 + 1);

    let before = take_budget_snapshot(&env);
    client.auto_release(&id);
    let sample = diff(&env, &before);
    print_gas("auto_release", &sample);
}

#[test]
fn gas_profile_cancel_escrow_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );

    let before = take_budget_snapshot(&env);
    client.cancel_escrow(&seller, &id);
    let sample = diff(&env, &before);
    print_gas("cancel_escrow (Pending)", &sample);
}

#[test]
fn gas_profile_mutual_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);

    let before = take_budget_snapshot(&env);
    client.mutual_cancel(&id);
    let sample = diff(&env, &before);
    print_gas("mutual_cancel", &sample);
}

#[test]
fn gas_profile_raise_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "T-1"));
    advance_time(&env, 1);

    let before = take_budget_snapshot(&env);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &SorobanString::from_str(&env, "item not as described"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );
    let sample = diff(&env, &before);
    print_gas("raise_dispute", &sample);
}

#[test]
fn gas_profile_resolve_dispute_release() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &SorobanString::from_str(&env, "x"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );

    let before = take_budget_snapshot(&env);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    let sample = diff(&env, &before);
    print_gas("resolve_dispute (Release)", &sample);
}

#[test]
fn gas_profile_resolve_dispute_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &SorobanString::from_str(&env, "x"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );

    let before = take_budget_snapshot(&env);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund);
    let sample = diff(&env, &before);
    print_gas("resolve_dispute (Refund)", &sample);
}

#[test]
fn gas_profile_vote_multi_resolver() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let mut resolvers = Vec::new(&env);
    resolvers.push_back(r1.clone());
    resolvers.push_back(r2.clone());
    resolvers.push_back(r3.clone());
    let id = client.create_escrow_multi(
        &seller,
        &None::<Address>,
        &resolvers,
        &2_u32,
        &token,
        &1_000_000_i128,
        &100_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &SorobanString::from_str(&env, "x"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );

    let before = take_budget_snapshot(&env);
    client.vote(&r1, &id, &ResolutionType::Release);
    let sample = diff(&env, &before);
    print_gas("vote (multi-resolver, first)", &sample);
}

#[test]
fn gas_profile_finalize_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &SorobanString::from_str(&env, "x"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    advance_time(&env, 86_400 + 1);

    let before = take_budget_snapshot(&env);
    client.finalize_dispute(&resolver, &id);
    let sample = diff(&env, &before);
    print_gas("finalize_dispute", &sample);
}

#[test]
fn gas_profile_appeal_dispute() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &SorobanString::from_str(&env, "x"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);

    let before = take_budget_snapshot(&env);
    client.appeal_dispute(&buyer, &id);
    let sample = diff(&env, &before);
    print_gas("appeal_dispute", &sample);
}

#[test]
fn gas_profile_post_message() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);

    let before = take_budget_snapshot(&env);
    client.post_message(
        &id,
        &buyer,
        &SorobanString::from_str(&env, "Hello! Is this item still available?"),
    );
    let sample = diff(&env, &before);
    print_gas("post_message", &sample);
}

#[test]
fn gas_profile_rotate_resolver() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let new_resolver = Address::generate(&env);
    let token = register_token(&env);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );

    let before = take_budget_snapshot(&env);
    client.rotate_resolver(&seller, &id, &new_resolver);
    let sample = diff(&env, &before);
    print_gas("rotate_resolver", &sample);
}

#[test]
fn gas_profile_request_refund_and_approve() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);

    let b1 = take_budget_snapshot(&env);
    client.request_refund(&buyer, &id);
    let s1 = diff(&env, &b1);
    print_gas("request_refund", &s1);

    let b2 = take_budget_snapshot(&env);
    client.approve_refund(&seller, &id);
    let s2 = diff(&env, &b2);
    print_gas("approve_refund", &s2);
}

#[test]
fn gas_profile_get_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );

    let before = take_budget_snapshot(&env);
    let _e = client.get_escrow(&id);
    let sample = diff(&env, &before);
    print_gas("get_escrow (view)", &sample);
}

#[test]
fn gas_profile_get_stats() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let before = take_budget_snapshot(&env);
    let _s = client.get_stats();
    let sample = diff(&env, &before);
    print_gas("get_stats (view)", &sample);
}

#[test]
fn gas_profile_get_fee_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let before = take_budget_snapshot(&env);
    let _f = client.get_fee_config();
    let sample = diff(&env, &before);
    print_gas("get_fee_config (view)", &sample);
}

#[test]
fn gas_profile_pause_and_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    let b1 = take_budget_snapshot(&env);
    client.pause_contract(&admin);
    let s1 = diff(&env, &b1);
    print_gas("pause_contract", &s1);

    let b2 = take_budget_snapshot(&env);
    client.unpause_contract(&admin);
    let s2 = diff(&env, &b2);
    print_gas("unpause_contract", &s2);
}

#[test]
fn gas_profile_emergency_drain() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 1_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    client.fund_escrow(&id, &buyer);

    let before = take_budget_snapshot(&env);
    client.emergency_drain(&admin, &token);
    let sample = diff(&env, &before);
    print_gas("emergency_drain", &sample);
}

#[test]
fn gas_profile_batch_create_escrow_10() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let mut inputs = Vec::new(&env);
    for _ in 0..10 {
        inputs.push_back(crate::EscrowInput {
            buyer: None,
            resolver: resolver.clone(),
            token: token.clone(),
            amount: 1_000_000,
            fee_bps: 100,
            shipping_window: 3600,
            notes: None,
        });
    }

    let before = take_budget_snapshot(&env);
    let ids = client.batch_create_escrow(&seller, &inputs);
    let sample = diff(&env, &before);
    assert_eq!(ids.len(), 10);
    print_gas("batch_create_escrow(10)", &sample);
}

#[test]
fn gas_profile_multicall_3() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    mint_token(&env, &token, &buyer, 3_000_000);
    let payees = make_payees(&env, &seller);
    let payees_val = payees.into_val(&env);
    let id1 = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    let id2 = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    let id3 = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );

    let mut calls = Vec::new(&env);
    for id in [id1, id2, id3].iter() {
        let mut args = Vec::new(&env);
        args.push_back(id.into_val(&env));
        args.push_back(buyer.clone().into_val(&env));
        calls.push_back(crate::ContractCall {
            function: soroban_sdk::Symbol::new(&env, "fund_escrow"),
            args,
        });
    }

    let before = take_budget_snapshot(&env);
    let _r = client.multicall(&calls);
    let sample = diff(&env, &before);
    print_gas("multicall(fund x3)", &sample);
}
