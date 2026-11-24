#![cfg(test)]

extern crate std;

use crate::test_helpers::{advance_time, mint_token, setup_contract};
use crate::{Escrow, EscrowClient, Payee, ResolutionType};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, IntoVal, String as SorobanString, Vec};

#[derive(Clone, Debug)]
struct GasSample {
    cpu_insns: u64,
    mem_bytes: u64,
}

/// Soroban meters each top-level invocation separately, so reset the tracker
/// before the call and read the accumulated cost afterwards. These helpers
/// keep the `take_budget_snapshot` / `diff` call pattern used by every profile
/// below while using the SDK 26 `cost_estimate()` budget API.
fn take_budget_snapshot(env: &Env) -> GasSample {
    env.cost_estimate().budget().reset_tracker();
    GasSample {
        cpu_insns: 0,
        mem_bytes: 0,
    }
}

fn diff(env: &Env, _before: &GasSample) -> GasSample {
// Soroban resets the budget for each top-level invocation.
fn reset_gas_meter(env: &Env) {
    env.cost_estimate().budget().reset_tracker();
}

fn read_gas_meter(env: &Env) -> GasSample {
    let b = env.cost_estimate().budget();
    GasSample {
        cpu_insns: b.cpu_instruction_cost(),
        mem_bytes: b.memory_bytes_cost(),
    }
}

fn print_gas(label: &str, sample: &GasSample) {
    std::println!(
        "gas_profile | {:<36} | cpu={:>12} | mem={:>10}",
        label,
        sample.cpu_insns,
        sample.mem_bytes
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

    reset_gas_meter(&env);
    client.initialize(&admin, &fee_collector, &0_u32);
    let sample = read_gas_meter(&env);
    print_gas("initialize", &sample);
    assert!(sample.cpu_insns > 0);
}

#[test]
fn gas_profile_set_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);
    let new_admin = Address::generate(&env);

    reset_gas_meter(&env);
    client.set_admin(&new_admin);
    let sample = read_gas_meter(&env);
    print_gas("set_admin", &sample);
}

#[test]
fn gas_profile_set_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    reset_gas_meter(&env);
    client.set_fee(&admin, &150_u32);
    let sample = read_gas_meter(&env);
    print_gas("set_fee", &sample);
}

#[test]
fn gas_profile_set_protocol_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    reset_gas_meter(&env);
    client.set_protocol_fee(&admin, &200_u32);
    let sample = read_gas_meter(&env);
    print_gas("set_protocol_fee", &sample);
}

#[test]
fn gas_profile_set_arbitration_fee() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    reset_gas_meter(&env);
    client.set_arbitration_fee(&admin, &100_u32);
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    let _id = client.create_escrow_7(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_000_i128,
        &100_u32,
    );
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
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
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
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
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    let _id = client.create_basket_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &tokens,
        &amounts,
        &100_u32,
        &3600_u64,
        &None::<SorobanString>,
        &None::<u64>,
        &0_u64,
    );
    let sample = read_gas_meter(&env);
    print_gas("create_basket_escrow(3 tokens)", &sample);
}

/// Issue #1232: basket reads/writes go through the capacity-bounded XDR value
/// instead of a bare vector. Assert the metered cost of a write
/// (`create_basket_escrow`) and a read (`get_basket_tokens`) stays bounded —
/// the wrapper must not add per-entry host calls or an unbounded decode path.
#[test]
fn gas_profile_bounded_basket_read_write() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let mut tokens = Vec::new(&env);
    for _ in 0..3 {
        tokens.push_back(register_token(&env));
    }
    let amounts = Vec::from_array(&env, [1_000_000_i128, 500_000, 250_000]);

    let before_write = take_budget_snapshot(&env);
    let id = client.create_basket_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &tokens,
        &amounts,
        &100_u32,
        &3_600_u64,
        &None::<SorobanString>,
        &None::<u64>,
        &0_u64,
    );
    let write = diff(&env, &before_write);
    print_gas("bounded_basket_write(3 tokens)", &write);

    let before_read = take_budget_snapshot(&env);
    let basket = client.get_basket_tokens(&id).expect("basket exists");
    let read = diff(&env, &before_read);
    print_gas("bounded_basket_read(3 tokens)", &read);

    assert_eq!(basket.len(), 3);
    assert!(write.cpu_insns > 0 && read.cpu_insns > 0);
    const WRITE_CPU_CEILING: u64 = 2_000_000;
    const READ_CPU_CEILING: u64 = 500_000;
    assert!(write.cpu_insns < WRITE_CPU_CEILING);
    assert!(read.cpu_insns < READ_CPU_CEILING);
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

    reset_gas_meter(&env);
    client.fund_escrow(&id, &buyer);
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-ABC-123"),
    );
    let sample = read_gas_meter(&env);
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
    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "T-1"));
    advance_time(&env, 172_800 + 1);

    reset_gas_meter(&env);
    client.confirm_delivery(&buyer, &id, &false);
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    client.co_signed_release(&buyer, &id);
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    client.auto_release(&id);
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    client.cancel_escrow(&seller, &id);
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    client.mutual_cancel(&id);
    let sample = read_gas_meter(&env);
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
    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "T-1"));
    advance_time(&env, 1);

    reset_gas_meter(&env);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &soroban_sdk::Bytes::from_slice(&env, b"item not as described"),
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );
    let sample = read_gas_meter(&env);
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
    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &soroban_sdk::Bytes::from_slice(&env, b"x"),
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );

    reset_gas_meter(&env);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    let sample = read_gas_meter(&env);
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
    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &soroban_sdk::Bytes::from_slice(&env, b"x"),
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );

    reset_gas_meter(&env);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund);
    let sample = read_gas_meter(&env);
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
    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &soroban_sdk::Bytes::from_slice(&env, b"x"),
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );

    reset_gas_meter(&env);
    client.vote(&r1, &id, &ResolutionType::Release);
    let sample = read_gas_meter(&env);
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
    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &soroban_sdk::Bytes::from_slice(&env, b"x"),
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    advance_time(&env, 86_400 + 1);

    reset_gas_meter(&env);
    client.finalize_dispute(&resolver, &id);
    let sample = read_gas_meter(&env);
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
    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "T-1"));
    advance_time(&env, 1);
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "DEFECTIVE"),
        &soroban_sdk::Bytes::from_slice(&env, b"x"),
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);

    reset_gas_meter(&env);
    client.appeal_dispute(&buyer, &id);
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    client.post_message(
        &id,
        &buyer,
        &SorobanString::from_str(&env, "Hello! Is this item still available?"),
    );
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    client.rotate_resolver(&seller, &id, &new_resolver);
    let sample = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    client.request_refund(&buyer, &id);
    let s1 = read_gas_meter(&env);
    print_gas("request_refund", &s1);

    reset_gas_meter(&env);
    client.approve_refund(&seller, &id);
    let s2 = read_gas_meter(&env);
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

    reset_gas_meter(&env);
    let _e = client.get_escrow(&id);
    let sample = read_gas_meter(&env);
    print_gas("get_escrow (view)", &sample);
}

#[test]
fn gas_profile_get_stats() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    reset_gas_meter(&env);
    let _s = client.get_stats();
    let sample = read_gas_meter(&env);
    print_gas("get_stats (view)", &sample);
}

#[test]
fn gas_profile_get_fee_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    reset_gas_meter(&env);
    let _f = client.get_fee_config();
    let sample = read_gas_meter(&env);
    print_gas("get_fee_config (view)", &sample);
}

#[test]
fn gas_profile_pause_and_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fc) = setup_contract(&env);

    reset_gas_meter(&env);
    client.pause_contract(&admin);
    let s1 = read_gas_meter(&env);
    print_gas("pause_contract", &s1);

    reset_gas_meter(&env);
    client.unpause_contract(&admin);
    let s2 = read_gas_meter(&env);
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
    client.pause_contract(&admin);

    reset_gas_meter(&env);
    client.emergency_drain(&id);
    let sample = read_gas_meter(&env);
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
            resolver_fee_bps: 0,
            shipping_window: 3600,
            notes: SorobanString::from_str(&env, ""),
        });
    }

    reset_gas_meter(&env);
    let ids = client.batch_create_escrow(&seller, &inputs);
    let sample = read_gas_meter(&env);
    assert_eq!(ids.len(), 10);
    print_gas("batch_create_escrow(10)", &sample);
}

#[test]
fn gas_profile_multicall_3() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fc) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyers = [
        Address::generate(&env),
        Address::generate(&env),
        Address::generate(&env),
    ];
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    for buyer in &buyers {
        mint_token(&env, &token, buyer, 1_000_000);
    }
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
    for (id, buyer) in [id1, id2, id3].iter().zip(buyers.iter()) {
        let mut args = Vec::new(&env);
        args.push_back(id.into_val(&env));
        args.push_back(buyer.clone().into_val(&env));
        calls.push_back(crate::ContractCall {
            function: soroban_sdk::Symbol::new(&env, "fund_escrow"),
            args,
        });
    }

    reset_gas_meter(&env);
    let _r = client.multicall(&calls);
    let sample = read_gas_meter(&env);
    print_gas("multicall(fund x3)", &sample);
}

/// `ensure_action_not_paused` runs at the top of every entry point
/// (create/fund/ship/confirm/dispute/...), so a regression there is paid by
/// the whole contract rather than one call. It is `pub(crate)`, not part of
/// the public client, so it is measured directly (inside `as_contract`)
/// rather than through a `EscrowClient` call.
#[test]
fn gas_profile_ensure_action_not_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, _client, _admin, _fc) = setup_contract(&env);
    let action = soroban_sdk::Symbol::new(&env, "create_escrow");

    reset_gas_meter(&env);
    env.as_contract(&contract_id, || {
        crate::internal::ensure_action_not_paused(&env, action.clone()).unwrap();
    });
    let sample = read_gas_meter(&env);
    print_gas("ensure_action_not_paused", &sample);

    // Today's cost is one `read_global_config` instance read plus one
    // `ActionPaused` instance read — no loops, no persistent reads. This
    // ceiling has generous headroom over that, so it only trips if a future
    // change adds real cost (e.g. an extra read or a scan) to a path every
    // entry point pays for. If it ever fires on a legitimate change, update
    // the ceiling to the new `print_gas` value plus headroom, not just raise
    // it blindly.
    const CPU_CEILING: u64 = 150_000;
    assert!(
        sample.cpu_insns < CPU_CEILING,
        "ensure_action_not_paused cost {} cpu instructions, exceeding the {CPU_CEILING} ceiling; \
         this check runs on every entry point so a regression here is expensive",
        sample.cpu_insns,
    );
}
