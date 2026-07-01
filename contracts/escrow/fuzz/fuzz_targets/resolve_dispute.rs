#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Ledger, Env, Address, Symbol, String, BytesN, Vec};
use trustlink_escrow::{Escrow, Payee, ResolutionType};

fuzz_target!(|data: &[u8]| {
    if data.len() < 64 {
        return;
    }
    
    let mut env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token = Address::generate(&env);
    
    // Initialize contract
    let _ = Escrow::initialize(env.clone(), admin.clone(), fee_collector, 0);
    
    // Create, fund, and dispute an escrow
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    
    let mut payees = Vec::new(&env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    
    let escrow_id = Escrow::create_escrow(
        env.clone(),
        payees,
        Some(buyer.clone()),
        resolver.clone(),
        token.clone(),
        1000,
        100,
        0,
        604800,
    ).unwrap_or(1);
    
    let _ = Escrow::fund_escrow(env.clone(), escrow_id, buyer.clone());
    
    let reason = Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let description = String::from_str(&env, "Item not as described");
    let mut evidence_bytes = [0u8; 32];
    evidence_bytes.copy_from_slice(&data[data.len().saturating_sub(32)..]);
    let evidence_hash = BytesN::from_array(&env, &evidence_bytes);
    
    let _ = Escrow::raise_dispute(env.clone(), buyer.clone(), escrow_id, reason, description, evidence_hash);
    
    // Determine resolution type from fuzz data
    let resolution = if data[0] % 2 == 0 {
        ResolutionType::Release
    } else {
        ResolutionType::Refund
    };
    
    // Test resolve_dispute with fuzzed inputs
    let _ = Escrow::resolve_dispute(env.clone(), resolver, escrow_id, resolution);
});
