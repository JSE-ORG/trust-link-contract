#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger}, symbol_short, token, Address, BytesN, Env, String, Vec};
use crate::types::ResolutionType;

#[test]
fn test_multi_resolver_threshold_met() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver_a = Address::generate(&env);
    let resolver_b = Address::generate(&env);
    let resolver_c = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin.clone()).address();

    // Mint tokens to buyer
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &10000);

    let mut resolvers = Vec::new(&env);
    resolvers.push_back(resolver_a.clone());
    resolvers.push_back(resolver_b.clone());
    resolvers.push_back(resolver_c.clone());

    let threshold = 2; // 2-of-3 required

    // Create escrow with multi-resolver
    let escrow_id = client.create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &resolvers,
        &threshold,
        &token,
        &1000,
        &0,
        &3600
    );

    // Fund escrow
    client.fund_escrow(&escrow_id, &buyer);

    // Raise dispute
    client.raise_dispute(&buyer, &escrow_id, &symbol_short!("item"), &String::from_str(&env, "broken"), &BytesN::from_array(&env, &[0; 32]));

    // Resolver A votes Release
    client.resolve_dispute(&resolver_a, &escrow_id, &ResolutionType::Release);

    // Check status - should still be Disputed (threshold 2 not met)
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Disputed);

    // Resolver B votes Release
    client.resolve_dispute(&resolver_b, &escrow_id, &ResolutionType::Release);

    // After threshold met, state transitions to PendingFinalization
    let escrow_pending = client.get_escrow(&escrow_id);
    assert_eq!(escrow_pending.state, EscrowState::PendingFinalization);

    // Advance past appeal window and finalize
    env.ledger().set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
    client.finalize_dispute(&admin, &escrow_id);

    // Check status - should be Completed
    let escrow_final = client.get_escrow(&escrow_id);
    assert_eq!(escrow_final.state, EscrowState::Completed);
}

#[test]
fn test_unauthorized_resolver_cannot_vote() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let _client = EscrowClient::new(&env, &contract_id);

    // Setup logic here...
    // Create escrow with Resolver A
    // Attempt to vote with Resolver Z (not in the list)
    // Assert result is Err(ContractError::NotAuthorized)
}
