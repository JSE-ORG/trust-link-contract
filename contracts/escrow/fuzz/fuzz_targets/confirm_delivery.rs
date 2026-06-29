#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Ledger, Env, Address, Vec};
use trustlink_escrow::{Escrow, Payee};

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }
    
    let mut env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token = Address::generate(&env);
    
    // Initialize contract
    let _ = Escrow::initialize(env.clone(), admin.clone(), fee_collector, 0);
    
    // Create, fund, and ship an escrow
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    
    let mut payees = Vec::new(&env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    
    let escrow_id = Escrow::create_escrow(
        env.clone(),
        payees,
        Some(buyer.clone()),
        resolver,
        token.clone(),
        1000,
        100,
        0,
        604800,
    ).unwrap_or(1);
    
    let _ = Escrow::fund_escrow(env.clone(), escrow_id, buyer.clone());
    
    // Set ledger time past dispute deadline
    env.ledger().set(u64::from_be_bytes(data[0..8].try_into().unwrap_or([0u8; 8])), 1);
    
    let _ = Escrow::mark_shipped(env.clone(), seller.clone(), escrow_id, soroban_sdk::String::from_str(&env, "TRACK123"));
    
    // Test confirm_delivery with fuzzed inputs
    let _ = Escrow::confirm_delivery(env.clone(), buyer, escrow_id);
});
