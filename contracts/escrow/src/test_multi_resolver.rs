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
fn test_multi_resolver_validation() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver_a = Address::generate(&env);
    let token = Address::generate(&env);

    let mut valid_resolvers = Vec::new(&env);
    valid_resolvers.push_back(resolver_a.clone());

    let empty_resolvers = Vec::new(&env);

    // 1. Empty multi-resolver list with threshold=0 rejected
    let res = client.try_create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &empty_resolvers,
        &0,
        &token,
        &1000,
        &0,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::InvalidAmount)));

    // 2. Empty multi-resolver list with threshold=1 rejected
    let res = client.try_create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &empty_resolvers,
        &1,
        &token,
        &1000,
        &0,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::InvalidAmount)));

    // 3. Non-empty resolver list with threshold=0 rejected
    let res = client.try_create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &valid_resolvers,
        &0,
        &token,
        &1000,
        &0,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::InvalidAmount)));

    // 4. Threshold greater than resolver count rejected
    let res = client.try_create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &valid_resolvers,
        &2,
        &token,
        &1000,
        &0,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::InvalidAmount)));

    // 5. Valid config (threshold=1, resolvers=[resolver_a]) works
    let escrow_id = client.create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &valid_resolvers,
        &1,
        &token,
        &1000,
        &0,
        &3600,
    );
    assert_eq!(escrow_id, 1);
}

