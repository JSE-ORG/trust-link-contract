#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Ledger, Env, Address, Vec};
use trustlink_escrow::{Escrow, Payee};

fuzz_target!(|data: &[u8]| {
    if data.len() < 32 {
        return;
    }
    
    let mut env = Env::default();
    env.mock_all_auths();
    
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token = Address::generate(&env);
    
    // Initialize contract
    let _ = Escrow::initialize(env.clone(), admin.clone(), fee_collector, 0);
    
    // Create an escrow first
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    
    let mut payees = Vec::new(&env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    
    let escrow_id = Escrow::create_escrow(
        env.clone(),
        payees,
        None,
        resolver,
        token.clone(),
        1000,
        100,
        0,
        604800,
    ).unwrap_or(1);
    
    // Extract buyer from fuzz data
    let fuzz_buyer = Address::generate(&env);
    
    // Test fund_escrow with fuzzed inputs
    let _ = Escrow::fund_escrow(env.clone(), escrow_id, fuzz_buyer);
});
