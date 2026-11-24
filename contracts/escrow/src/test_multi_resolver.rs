#![cfg(test)]
use super::*;
use crate::types::ResolutionType;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, Vec,
};

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
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

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
        &3600,
    );

    // Fund escrow
    client.fund_escrow(&escrow_id, &buyer);

    // Raise dispute
    client.raise_dispute(
        &buyer,
        &escrow_id,
        &symbol_short!("OTHER"),
        &soroban_sdk::Bytes::from_slice(&env, b"broken"),
        &BytesN::from_array(&env, &[1; 32]),
    );

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
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
    client.finalize_dispute(&admin, &escrow_id);

    // Check status - should be Completed
    let escrow_final = client.get_escrow(&escrow_id);
    assert_eq!(escrow_final.state, EscrowState::Completed);
}

#[test]
fn test_multi_resolver_validation() {
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
    assert_eq!(res, Err(Ok(ContractError::InvalidResolverThreshold)));

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
    assert_eq!(res, Err(Ok(ContractError::InvalidResolverThreshold)));

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
    assert_eq!(res, Err(Ok(ContractError::InvalidResolverThreshold)));

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
    assert_eq!(res, Err(Ok(ContractError::InvalidResolverThreshold)));

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

#[test]
fn test_multi_resolver_duplicate_returns_duplicate_resolver() {
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
    let token = Address::generate(&env);

    let mut resolvers = Vec::new(&env);
    resolvers.push_back(resolver_a.clone());
    resolvers.push_back(resolver_a);

    let res = client.try_create_escrow_multi(
        &seller,
        &Some(buyer),
        &resolvers,
        &2,
        &token,
        &1000,
        &0,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::DuplicateResolver)));
}

#[test]
fn test_multi_resolver_threshold_validation_errors() {
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
    let token = Address::generate(&env);

    let mut resolvers = Vec::new(&env);
    resolvers.push_back(resolver_a.clone());
    resolvers.push_back(resolver_b.clone());

    // Test threshold=0 returns InvalidResolverThreshold
    let res = client.try_create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &resolvers,
        &0,
        &token,
        &1000,
        &0,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::InvalidResolverThreshold)));

    // Test threshold>count returns InvalidResolverThreshold
    let res = client.try_create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &resolvers,
        &3,
        &token,
        &1000,
        &0,
        &3600,
    );
    assert_eq!(res, Err(Ok(ContractError::InvalidResolverThreshold)));

    // Test empty resolvers with threshold=1 returns InvalidResolverThreshold
    let empty_resolvers = Vec::new(&env);
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
    assert_eq!(res, Err(Ok(ContractError::InvalidResolverThreshold)));
}

/// Issue #966: Multi-Resolver Deadlock.
/// This test simulates a 2-of-2 multi-resolver setup where the resolvers split their votes
/// (one votes Release, the other votes Refund). Because the threshold (2) is not met for
/// either outcome, `tally_votes` returns None and the state correctly remains `Disputed`.
/// The escrow does NOT automatically resolve on the conflicting vote. It remains deadlocked
/// until a time-based escape hatch (like `resolve_deadlocked_dispute`) is triggered.
#[test]
fn test_multi_resolver_split_vote_deadlock() {
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
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    token::StellarAssetClient::new(&env, &token).mint(&buyer, &1000);

    let mut resolvers = Vec::new(&env);
    resolvers.push_back(resolver_a.clone());
    resolvers.push_back(resolver_b.clone());

    let threshold = 2; // 2-of-2 required, guarantees deadlock on split vote

    let escrow_id = client.create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &resolvers,
        &threshold,
        &token,
        &1000,
        &0,
        &3600,
    );

    client.fund_escrow(&escrow_id, &buyer);
    client.mark_shipped(
        &seller,
        &escrow_id,
        &soroban_sdk::String::from_str(&env, "TRK-001"),
    );

    let reason = symbol_short!("OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"Item broken");
    let evidence_hash = BytesN::from_array(&env, &[1u8; 32]);
    client.raise_dispute(&buyer, &escrow_id, &reason, &description, &evidence_hash);

    // Resolver A votes Release
    client.vote(&resolver_a, &escrow_id, &ResolutionType::Release);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Disputed);

    // Resolver B votes Refund
    client.vote(&resolver_b, &escrow_id, &ResolutionType::Refund);

    // Both have voted, but threshold is 2 for either outcome.
    // State MUST remain Disputed (deadlocked).
    let escrow_after = client.get_escrow(&escrow_id);
    assert_eq!(escrow_after.state, EscrowState::Disputed);
}

/// Builds a funded, disputed 3-of-5 multi-resolver escrow whose resolver fee
/// is `resolver_fee_bps` (the multi-resolver constructor fixes it at 0, so the
/// stored escrow is patched directly).
fn setup_disputed_five_resolver_escrow(
    env: &Env,
    amount: i128,
    resolver_fee_bps: u32,
) -> (EscrowClient<'_>, Address, u64, Vec<Address>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    token::StellarAssetClient::new(env, &token).mint(&buyer, &amount);

    let mut resolvers = Vec::new(env);
    for _ in 0..5 {
        resolvers.push_back(Address::generate(env));
    }
    let escrow_id = client.create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &resolvers,
        &3,
        &token,
        &amount,
        &0,
        &3600,
    );

    env.as_contract(&contract_id, || {
        let mut escrow = crate::internal::load_escrow(env, escrow_id).unwrap();
        escrow.resolver_fee_bps = resolver_fee_bps;
        crate::internal::save_escrow(env, escrow_id, &escrow, None);
    });

    client.fund_escrow(&escrow_id, &buyer);
    client.raise_dispute(
        &buyer,
        &escrow_id,
        &symbol_short!("OTHER"),
        &soroban_sdk::Bytes::from_slice(env, b"broken"),
        &BytesN::from_array(env, &[1; 32]),
    );
    (client, contract_id, escrow_id, resolvers, token)
}

/// Issue #1203: 3 of 5 resolvers vote Release; each of the 3 receives exactly
/// 1/3 of the resolver fee and the 2 losing voters receive nothing.
#[test]
fn test_multi_resolver_fee_split_equally_among_winning_voters() {
    let env = Env::default();
    let (client, _cid, escrow_id, resolvers, token) =
        setup_disputed_five_resolver_escrow(&env, 10_000, 300);
    let token_client = token::Client::new(&env, &token);

    // Two losing voters first, then the three winners (the last one crosses
    // the 3-of-5 threshold and finalizes the vote).
    client.vote(
        &resolvers.get(3).unwrap(),
        &escrow_id,
        &ResolutionType::Refund,
    );
    client.vote(
        &resolvers.get(4).unwrap(),
        &escrow_id,
        &ResolutionType::Refund,
    );
    client.vote(
        &resolvers.get(0).unwrap(),
        &escrow_id,
        &ResolutionType::Release,
    );
    client.vote(
        &resolvers.get(1).unwrap(),
        &escrow_id,
        &ResolutionType::Release,
    );
    client.vote(
        &resolvers.get(2).unwrap(),
        &escrow_id,
        &ResolutionType::Release,
    );

    let dispute = client.get_dispute(&escrow_id).unwrap();
    let resolver_fee = dispute.resolver_fee;
    assert_eq!(resolver_fee, 300); // 3% of 10_000
    assert_eq!(dispute.compensated_resolvers.len(), 3);

    let share = resolver_fee / 3;
    for i in 0..3 {
        assert_eq!(token_client.balance(&resolvers.get(i).unwrap()), share);
    }
    // Losing voters receive nothing.
    assert_eq!(token_client.balance(&resolvers.get(3).unwrap()), 0);
    assert_eq!(token_client.balance(&resolvers.get(4).unwrap()), 0);
}

/// Issue #1203: when the fee does not divide evenly, shares plus dust equal the
/// resolver fee exactly, with the dust going to the finalizing voter.
#[test]
fn test_multi_resolver_fee_dust_goes_to_finalizing_voter() {
    let env = Env::default();
    // 1% of 1_000 = 10 → 3 winners get 3 each, dust 1.
    let (client, _cid, escrow_id, resolvers, token) =
        setup_disputed_five_resolver_escrow(&env, 1_000, 100);
    let token_client = token::Client::new(&env, &token);

    client.vote(
        &resolvers.get(3).unwrap(),
        &escrow_id,
        &ResolutionType::Refund,
    );
    client.vote(
        &resolvers.get(0).unwrap(),
        &escrow_id,
        &ResolutionType::Release,
    );
    client.vote(
        &resolvers.get(1).unwrap(),
        &escrow_id,
        &ResolutionType::Release,
    );
    let finalizer = resolvers.get(2).unwrap();
    client.vote(&finalizer, &escrow_id, &ResolutionType::Release);

    let resolver_fee = client.get_dispute(&escrow_id).unwrap().resolver_fee;
    assert_eq!(resolver_fee, 10);

    let a = token_client.balance(&resolvers.get(0).unwrap());
    let b = token_client.balance(&resolvers.get(1).unwrap());
    let c = token_client.balance(&finalizer);
    assert_eq!(a, 3);
    assert_eq!(b, 3);
    assert_eq!(c, 4); // 3 + 1 dust
    assert_eq!(a + b + c, resolver_fee);
    assert_eq!(token_client.balance(&resolvers.get(3).unwrap()), 0);
    assert_eq!(token_client.balance(&resolvers.get(4).unwrap()), 0);
}
