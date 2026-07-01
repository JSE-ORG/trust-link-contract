#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Ledger, Env, Address, Symbol, String, BytesN, Vec};
use trustlink_escrow::{Escrow, ContractError, Payee, EscrowState, ResolutionType};

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
    
    // Extract parameters from fuzz data
    let amount = i128::from_be_bytes(
        data[0..16].try_into().unwrap_or([0u8; 16])
    );
    let fee_bps = u32::from_be_bytes(
        data[16..20].try_into().unwrap_or([0u8; 4])
    ) % 301; // Cap at 300 (MAX_ESCROW_FEE_BPS)
    let shipping_window = u64::from_be_bytes(
        data[20..28].try_into().unwrap_or([0u8; 8])
    );
    
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    
    // Test create_escrow with fuzzed inputs
    let mut payees = Vec::new(&env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    
    let _ = Escrow::create_escrow(
        env.clone(),
        payees,
        Some(buyer.clone()),
        resolver,
        token,
        amount,
        fee_bps,
        0,
        shipping_window,
    );
});
