#![cfg(test)]

use crate::{ContractError, EscrowClient, ResolutionType};
use soroban_sdk::testutils::{Address as _, Events, Ledger as _};
use soroban_sdk::{token, Address, Env};

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

    (
        env,
        admin,
        seller,
        buyer,
        resolver,
        token_address,
        fee_collector,
    )
}

#[test]
fn test_accumulated_fees() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    
    // Set 1% protocol fee and 2% arbitration fee
    client.initialize(&admin, &fee_collector, &200_u32);
    client.set_protocol_fee(&admin, &100_u32);

    let amount = 1000;
    let id = client.create_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &100_u32, // Escrow fee 1%
        &3600,
    );

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);

    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "TRACK"));

    // Raise dispute
    let reason = soroban_sdk::Symbol::new(&env, "reason");
    let description = soroban_sdk::String::from_str(&env, "desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // Verify initial accumulated fees are 0
    let initial_fees = client.get_accumulated_fees(&token);
    assert_eq!(initial_fees, 0);

    // Resolve dispute -> deduct_and_transfer leaves fees in vault and updates AccumulatedFees
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);

    // Verify accumulated fees
    // Arbitration fee: 2% of 1000 = 20
    // Protocol fee is collected dynamically, escrow fee is 1% of 1000 = 10.
    // Total retained = 20 + 10 = 30
    let fees = client.get_accumulated_fees(&token);
    assert_eq!(fees, 30);
}
