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
    
    // Create an escrow
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    
    let mut payees = Vec::new(&env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    
    let escrow_id = Escrow::create_escrow(
        env.clone(),
        payees,
        Some(buyer),
        resolver,
        token,
        1000,
        100,
        0,
        604800,
    ).unwrap_or(1);
    
    // Test cancel_escrow with fuzzed inputs
    let _ = Escrow::cancel_escrow(env.clone(), seller, escrow_id);
});
