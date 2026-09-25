#![cfg(test)]
//! Tests for the expiration-based escape hatch for multi-resolver deadlocks
//! (`resolve_deadlocked_dispute`). When resolver votes are split and no
//! threshold is reachable, funds would otherwise stay frozen in `Disputed`
//! forever.

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

struct MultiSetup {
    client: EscrowClient<'static>,
    admin: Address,
    buyer: Address,
    resolvers: Vec<Address>,
    token: Address,
    id: u64,
}

fn setup(env: &Env, threshold: u32, resolver_count: u32) -> MultiSetup {
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    token::StellarAssetClient::new(env, &token).mint(&buyer, &1000);

    let mut resolvers = Vec::new(env);
    for _ in 0..resolver_count {
        resolvers.push_back(Address::generate(env));
    }

    let id = client.create_escrow_multi(
        &seller,
        &Some(buyer.clone()),
        &resolvers,
        &threshold,
        &token,
        &1000,
        &0,
        &3600,
    );
    client.fund_escrow(&id, &buyer);
    client.raise_dispute(
        &buyer,
        &id,
        &symbol_short!("item"),
        &String::from_str(env, "split committee"),
        &BytesN::from_array(env, &[0; 32]),
    );

    MultiSetup {
        client,
        admin,
        buyer,
        resolvers,
        token,
        id,
    }
}

impl MultiSetup {
    fn resolver(&self, index: u32) -> Address {
        self.resolvers.get(index).unwrap()
    }
}

#[test]
fn split_majority_release_resolves_after_window() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let s = setup(&env, 3, 3);

    // Two Release votes with a unanimous threshold of 3: no threshold, and
    // resolver C abstains.
    s.client
        .resolve_dispute(&s.resolver(0), &s.id, &ResolutionType::Release);
    s.client
        .resolve_dispute(&s.resolver(1), &s.id, &ResolutionType::Release);
    assert_eq!(s.client.get_escrow(&s.id).state, EscrowState::Disputed);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::DISPUTE_DEADLOCK_WINDOW);
    s.client.resolve_deadlocked_dispute(&s.id);
    assert_eq!(
        s.client.get_escrow(&s.id).state,
        EscrowState::PendingFinalization
    );

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
    s.client.finalize_dispute(&s.admin, &s.id);
    assert_eq!(s.client.get_escrow(&s.id).state, EscrowState::Completed);
}

#[test]
fn tied_vote_resolves_to_refund_after_window() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let s = setup(&env, 3, 3);

    s.client
        .resolve_dispute(&s.resolver(0), &s.id, &ResolutionType::Release);
    s.client
        .resolve_dispute(&s.resolver(1), &s.id, &ResolutionType::Refund);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::DISPUTE_DEADLOCK_WINDOW);
    s.client.resolve_deadlocked_dispute(&s.id);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
    s.client.finalize_dispute(&s.admin, &s.id);

    assert_eq!(s.client.get_escrow(&s.id).state, EscrowState::Refunded);
    assert_eq!(token::Client::new(&env, &s.token).balance(&s.buyer), 1000);
}

#[test]
fn deadlock_before_window_is_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let s = setup(&env, 3, 3);

    s.client
        .resolve_dispute(&s.resolver(0), &s.id, &ResolutionType::Release);
    s.client
        .resolve_dispute(&s.resolver(1), &s.id, &ResolutionType::Refund);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::DISPUTE_DEADLOCK_WINDOW - 1);
    assert_eq!(
        s.client.try_resolve_deadlocked_dispute(&s.id),
        Err(Ok(ContractError::DisputeNotDeadlocked))
    );
}

#[test]
fn no_votes_is_not_a_deadlock() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let s = setup(&env, 3, 3);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::DISPUTE_DEADLOCK_WINDOW);
    assert_eq!(
        s.client.try_resolve_deadlocked_dispute(&s.id),
        Err(Ok(ContractError::NoResolverVotes))
    );
}

#[test]
fn met_threshold_already_moved_to_pending_finalization() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    // Threshold 1: a single vote resolves immediately, so there is no deadlock.
    let s = setup(&env, 1, 2);

    s.client
        .resolve_dispute(&s.resolver(0), &s.id, &ResolutionType::Release);
    assert_eq!(
        s.client.get_escrow(&s.id).state,
        EscrowState::PendingFinalization
    );

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::DISPUTE_DEADLOCK_WINDOW);
    assert_eq!(
        s.client.try_resolve_deadlocked_dispute(&s.id),
        Err(Ok(ContractError::InvalidState))
    );
}
