#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Ledger, Env, Address, String, Vec};
use trustlink_escrow::{Escrow, Payee};

fuzz_target!(|data: &[u8]| {
    if data.len() < 16 {
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
    let _ = Escrow::mark_shipped(env.clone(), seller.clone(), escrow_id, String::from_str(&env, "TRACK123"));
    
    // Set ledger time past shipping window from fuzz data
    let timestamp = u64::from_be_bytes(data[0..8].try_into().unwrap_or([0u8; 8]));
    env.ledger().set(timestamp.saturating_add(604800), 1);
    
    // Test auto_release with fuzzed inputs
    let _ = Escrow::auto_release(env.clone(), escrow_id);
});
