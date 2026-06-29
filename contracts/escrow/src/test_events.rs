//! Tests to verify that each public function emits the expected event.

#![cfg(test)]

use crate::{
    EscrowClient, Escrow, ContractError, ResolutionType, ContractInitialized, FeeUpdated,
    ProtocolFeeUpdated, ArbitrationFeeUpdated, FeesWithdrawn, EscrowCreated, EscrowCancelled,
    EscrowShipped, DeliveryRecorded, EscrowCompleted, DisputeRaised, DisputeResolved,
    AutoReleased, ResolverRotated,
};
use soroban_sdk::{testutils::{Address as _, Ledger as _}, token, Address, Env, Symbol, BytesN, String as SorobanString};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let tokenadmin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract(tokenadmin.clone());
    let contract_id = env.register(Escrow, ());
    {
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
    }
    (env, admin, seller, buyer, resolver, token_address, contract_id)
}

fn minttokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

use soroban_sdk::{symbol_short, Val, Vec};

fn last_event_topics(env: &Env) -> Vec<Val> {
    let events = env.events().all();
    let (_, topics, _) = events.last().expect("no events emitted");
    topics
}

#[test]
fn test_initialize_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Contract").into_val(&env), symbol_short!("Init").into_val(&env)]);
}

#[test]
fn test_pause_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause_contract(&admin);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Contract").into_val(&env), symbol_short!("Paused").into_val(&env), admin.into_val(&env)]);
}

#[test]
fn test_unpause_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause_contract(&admin);
    client.unpause_contract(&admin);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Contract").into_val(&env), symbol_short!("Unpaused").into_val(&env), admin.into_val(&env)]);
}

#[test]
fn test_setadmin_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let newadmin = Address::generate(&env);
    client.setadmin(&newadmin);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Admin").into_val(&env), symbol_short!("Rotated").into_val(&env)]);
}

#[test]
fn test_set_fee_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.set_fee(&admin, 150);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Fee").into_val(&env), symbol_short!("Updated").into_val(&env)]);
}

#[test]
fn test_set_protocol_fee_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.set_protocol_fee(&admin, 100);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("ProtoFee").into_val(&env), symbol_short!("Updated").into_val(&env)]);
}

#[test]
fn test_set_arbitration_fee_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.set_arbitration_fee(&admin, 100);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("ArbFee").into_val(&env), symbol_short!("Updated").into_val(&env)]);
}

#[test]
fn test_withdraw_fees_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    // Populate some accumulated fees first via a dispute resolution
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow(&admin, &None::<Address>, &admin, &token, &100_i128, &0_u32, &6000_u64);
    client.fund_escrow(&id, &admin);
    client.mark_shipped(&admin, &id, &SorobanString::from_str(&env, "TRACK"));
    client.pause_contract(&admin);
    // Unpause to allow resolve
    client.unpause_contract(&admin);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(&admin, &id, &Symbol::new(&env, "fraud"), &SorobanString::from_str(&env, "desc"), &hash);
    client.resolve_dispute(&admin, &id, &ResolutionType::Refund).unwrap();
    // Now withdraw fees
    client.withdraw_fees(&admin, &token, &admin, &1_i128).unwrap();
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Fee").into_val(&env), symbol_short!("Withdrawn").into_val(&env), admin.into_val(&env)]);
}

#[test]
fn test_create_escrow_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.create_escrow_legacy(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Escrow").into_val(&env), symbol_short!("Created").into_val(&env), seller.into_val(&env)]);
}

#[test]
fn test_cancel_escrow_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow_legacy(&seller, &None::<Address>, &admin, &admin, &100_i128, &0_u32, &6000_u64);
    client.cancel_escrow(&seller, &id);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Escrow").into_val(&env), symbol_short!("Canceled").into_val(&env), seller.into_val(&env)]);
}

#[test]
fn test_mark_shipped_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow_legacy(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    minttokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK123"));
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Escrow").into_val(&env), symbol_short!("Shipped").into_val(&env), seller.into_val(&env)]);
}

#[test]
fn test_record_delivery_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow_legacy(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    minttokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK"));
    client.record_delivery(&admin, &id);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Escrow").into_val(&env), symbol_short!("Delivered").into_val(&env)]);
}

#[test]
fn test_confirm_delivery_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow_legacy(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    minttokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK"));
    env.ledger().set_timestamp(DISPUTE_WINDOW + 1);
    client.confirm_delivery(&buyer, &id);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Escrow").into_val(&env), symbol_short!("Completed").into_val(&env), buyer.into_val(&env)]);
}

#[test]
fn test_raise_dispute_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow_legacy(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    minttokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK"));
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(&buyer, &id, &Symbol::new(&env, "fraud"), &SorobanString::from_str(&env, "desc"), &hash);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Dispute").into_val(&env), symbol_short!("Raised").into_val(&env), buyer.into_val(&env)]);
}

#[test]
fn test_resolve_dispute_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow_legacy(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    minttokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK"));
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(&buyer, &id, &Symbol::new(&env, "fraud"), &SorobanString::from_str(&env, "desc"), &hash);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund).unwrap();
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Dispute").into_val(&env), symbol_short!("Resolved").into_val(&env), resolver.into_val(&env)]);
}

#[test]
fn test_auto_release_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    // escrow with no buyer; auto release after dispute window
    let id = client.create_escrow_legacy(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    env.ledger().set_timestamp(DISPUTE_WINDOW + 10);
    client.auto_release(&id);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Escrow").into_val(&env), symbol_short!("Released").into_val(&env), seller.into_val(&env)]);
}

#[test]
fn test_rotateresolver_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow_legacy(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    let newresolver = Address::generate(&env);
    client.rotateresolver(&admin, &id, &newresolver);
    let topics = last_event_topics(&env);
    assert_eq!(topics, soroban_sdk::vec![&env, symbol_short!("Resolver").into_val(&env), symbol_short!("Rotated").into_val(&env)]);
}
