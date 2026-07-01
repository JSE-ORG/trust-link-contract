#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};
use crate::types::{MultiResolver, ResolverSet, ResolutionType};

#[test]
fn test_multi_resolver_threshold_met() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver_a = Address::generate(&env);
    let resolver_b = Address::generate(&env);
    let resolver_c = Address::generate(&env);
    let token = Address::generate(&env);

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

    // Check status - should be Completed
    let escrow_final = client.get_escrow(&escrow_id);
    assert_eq!(escrow_final.state, EscrowState::Completed);
}

#[test]
fn test_unauthorized_resolver_cannot_vote() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Setup logic here...
    // Create escrow with Resolver A
    // Attempt to vote with Resolver Z (not in the list)
    // Assert result is Err(ContractError::NotAuthorized)
}
