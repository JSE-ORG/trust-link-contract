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
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let contract_id = env.register(Escrow, ());
    {
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
    }
    (env, admin, seller, buyer, resolver, token_address, contract_id)
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

fn last_event_symbol(env: &Env) -> Symbol {
    // env.events().all() returns a Vec of (contract_id, (topic...), data)
    let events = env.events().all();
    // Grab the last event's topic symbol
    let (_, topics, _) = events.last().expect("no events emitted");
    // topics is a tuple of symbols, we assume single symbol
    let symbol: Symbol = topics.clone().into_val(env);
    symbol
}

#[test]
fn test_initialize_emits_event() {
    let (env, admin, _seller, _buyer, _resolver, token, contract_id) = setup_env();
    let events = env.events().all();
    assert_eq!(events.len(), 1);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "contract_initialized"));
}

#[test]
fn test_pause_emits_event() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause_contract(&admin);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "contract_paused"));
}

#[test]
fn test_unpause_emits_event() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.pause_contract(&admin);
    client.unpause_contract(&admin);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "contract_unpaused"));
}

#[test]
fn test_set_admin_emits_event() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let new_admin = Address::generate(&env);
    client.set_admin(&new_admin);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "admin_rotated"));
}

#[test]
fn test_set_fee_emits_event() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.set_fee(&admin, 150);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "fee_updated"));
}

#[test]
fn test_set_protocol_fee_emits_event() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.set_protocol_fee(&admin, 100);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "protocol_fee_updated"));
}

#[test]
fn test_set_arbitration_fee_emits_event() {
    let (env, admin, _seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.set_arbitration_fee(&admin, 100);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "arbitration_fee_updated"));
}

#[test]
fn test_withdraw_fees_emits_event() {
    let (env, admin, _seller, _buyer, _resolver, token, contract_id) = setup_env();
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
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "fees_withdrawn"));
}

#[test]
fn test_create_escrow_emits_event() {
    let (env, _admin, seller, _buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "escrow_created"));
}

#[test]
fn test_cancel_escrow_emits_event() {
    let (env, admin, seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow(&seller, &None::<Address>, &admin, &admin, &100_i128, &0_u32, &6000_u64);
    client.cancel_escrow(&seller, &id);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "escrow_cancelled"));
}

#[test]
fn test_mark_shipped_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK123"));
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "escrow_shipped"));
}

#[test]
fn test_record_delivery_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK"));
    client.record_delivery(&admin, &id);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "delivery_recorded"));
}

#[test]
fn test_confirm_delivery_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK"));
    env.ledger().set_timestamp(DISPUTE_WINDOW + 1);
    client.confirm_delivery(&buyer, &id);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "escrow_completed"));
}

#[test]
fn test_raise_dispute_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK"));
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(&buyer, &id, &Symbol::new(&env, "fraud"), &SorobanString::from_str(&env, "desc"), &hash);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "dispute_raised"));
}

#[test]
fn test_resolve_dispute_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    mint_tokens(&env, &token, &buyer, 100);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK"));
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(&buyer, &id, &Symbol::new(&env, "fraud"), &SorobanString::from_str(&env, "desc"), &hash);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund).unwrap();
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "dispute_resolved"));
}

#[test]
fn test_auto_release_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    // escrow with no buyer; auto release after dispute window
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    env.ledger().set_timestamp(DISPUTE_WINDOW + 10);
    client.auto_release(&id);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "auto_released"));
}

#[test]
fn test_rotate_resolver_emits_event() {
    let (env, admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    let id = client.create_escrow(&seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &6000_u64);
    let new_resolver = Address::generate(&env);
    client.rotate_resolver(&admin, &id, &new_resolver);
    let symbol = last_event_symbol(&env);
    assert_eq!(symbol, Symbol::new(&env, "resolver_rotated"));
}
