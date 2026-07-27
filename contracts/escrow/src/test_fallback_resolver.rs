#![cfg(test)]

use crate::{DisputeStatus, Escrow, EscrowClient, Payee, ResolverSet, FallbackResolver, DisputeData, EscrowData, EscrowState, ResolutionType};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let primary_resolver = Address::generate(&env);
    let backup_resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    (
        env,
        admin,
        seller,
        buyer,
        primary_resolver,
        backup_resolver,
        token_address,
        fee_collector,
    )
}

#[test]
fn test_create_escrow_with_fallback_success() {
    let (env, admin, seller, buyer, primary, backup, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let shipping_window = 7200_u64;

    let id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &amount,
        &100_u32,
        &shipping_window,
    );

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Pending);
    assert_eq!(escrow.amount, amount);
    
    // Verify registered resolvers in storage
    if let ResolverSet::Fallback(f) = escrow.resolvers {
        assert_eq!(f.primary, primary);
        assert_eq!(f.backup, backup);
        assert_eq!(f.dispute_deadline, dispute_deadline);
    } else {
        panic!("Expected Fallback resolver set");
    }
}

#[test]
fn test_fallback_resolver_vote_primary_always_allowed() {
    let (env, admin, seller, buyer, primary, backup, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &amount,
        &100_u32,
        &0_u64,
    );

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &String::from_str(&env, "TRK"));

    let reason = Symbol::new(&env, "reason");
    let description = String::from_str(&env, "desc");
    let evidence_hash = BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // Primary resolver can resolve immediately (ledger time is still dispute time + 0)
    let result = client.try_resolve_dispute(&primary, &id, &ResolutionType::Refund);
    assert!(result.is_ok());

    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, EscrowState::PendingFinalization);
}

#[test]
fn test_fallback_resolver_vote_backup_blocked_before_deadline() {
    let (env, admin, seller, buyer, primary, backup, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &amount,
        &100_u32,
        &0_u64,
    );

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &String::from_str(&env, "TRK"));

    let reason = Symbol::new(&env, "reason");
    let description = String::from_str(&env, "desc");
    let evidence_hash = BytesN::from_array(&env, &[0xab; 32]);

    env.ledger().set_timestamp(1_000_000);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // Set time to just before dispute_deadline (e.g. 1_000_000 + 3599)
    env.ledger().set_timestamp(1_000_000 + 3599);

    // Backup resolver should be unauthorized to resolve before deadline
    let result = client.try_resolve_dispute(&backup, &id, &ResolutionType::Refund);
    assert_eq!(result, Err(Ok(crate::ContractError::NotAuthorized)));

    // Backup resolver secondary entrypoint `vote` should also be unauthorized
    let result_vote = client.try_vote(&backup, &id, &ResolutionType::Refund);
    assert_eq!(result_vote, Err(Ok(crate::ContractError::NotAuthorized)));
}

#[test]
fn test_fallback_resolver_vote_backup_allowed_after_deadline() {
    let (env, admin, seller, buyer, primary, backup, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &amount,
        &100_u32,
        &0_u64,
    );

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &String::from_str(&env, "TRK"));

    let reason = Symbol::new(&env, "reason");
    let description = String::from_str(&env, "desc");
    let evidence_hash = BytesN::from_array(&env, &[0xab; 32]);

    env.ledger().set_timestamp(1_000_000);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // Set time to exactly dispute_deadline (e.g. 1_000_000 + 3600)
    env.ledger().set_timestamp(1_000_000 + 3600);

    // Backup resolver should be authorized once deadline is reached
    let result = client.try_resolve_dispute(&backup, &id, &ResolutionType::Refund);
    assert!(result.is_ok());

    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, EscrowState::PendingFinalization);
}
