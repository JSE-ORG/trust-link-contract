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
    
    // Create and fund an escrow
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
    
    // Create tracking_id from fuzz data
    let tracking_len = (data[0] as usize) % 65; // 0-64
    if tracking_len == 0 {
        return;
    }
    let tracking_str: String = String::from_str(&env, &String::from_utf8_lossy(&data[1..tracking_len.min(data.len())]));
    
    // Test mark_shipped with fuzzed inputs
    let _ = Escrow::mark_shipped(env.clone(), seller, escrow_id, tracking_str);
});
