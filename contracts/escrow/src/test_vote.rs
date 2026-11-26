#![cfg(test)]
//! Dedicated coverage for the standalone `vote()` entry point (#569).
//!
//! `vote()` is the multi-resolver M-of-N voting path, distinct from the
//! single-resolver `resolve_dispute`. These tests drive it directly:
//! - threshold met (vote resolves the dispute)
//! - threshold not met (vote recorded, no resolution yet)
//! - duplicate vote from the same resolver (does not double-count)
//! - unauthorized caller (rejected)
//! - voting on a non-disputed escrow (rejected)
//! - a resolver changing their vote to flip the outcome
//!
//! Note on "duplicate vote": `vote()` is documented as "cast or change a
//! vote", so a repeat call from the same resolver *updates* that resolver's
//! single vote in place rather than returning an error. The meaningful
//! guarantee — that one resolver cannot reach an M-of-N threshold on their
//! own — is what `duplicate_vote_does_not_double_count` pins down.

use crate::{ContractError, Escrow, EscrowClient, EscrowState, ResolutionType};
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Symbol, Vec};

const AMOUNT: i128 = 1_000;

/// Builds a funded, three-resolver escrow with the given voting `threshold`.
/// Returns the env, contract address, the three resolver addresses, and the
/// escrow id. The escrow is left in `Funded` (not yet disputed).
fn setup_funded_multi(threshold: u32) -> (Env, Address, Address, [Address; 3], u64) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let r0 = Address::generate(&env);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &50_u32);

    let mut resolvers = Vec::new(&env);
    resolvers.push_back(r0.clone());
    resolvers.push_back(r1.clone());
    resolvers.push_back(r2.clone());

    let escrow_id = client.create_escrow_multi(
        &seller,
        &None::<Address>,
        &resolvers,
        &threshold,
        &token_address,
        &AMOUNT,
        &0_u32,
        &3600_u64,
    );

    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
    token_admin_client.mint(&buyer, &AMOUNT);
    client.fund_escrow(&escrow_id, &buyer);

    (env, contract_id, buyer, [r0, r1, r2], escrow_id)
}

/// As `setup_funded_multi`, but also raises a dispute so the escrow is in
/// `Disputed` and ready for `vote()`.
fn setup_disputed_multi(threshold: u32) -> (Env, Address, Address, [Address; 3], u64) {
    let (env, contract_id, buyer, resolvers, escrow_id) = setup_funded_multi(threshold);
    let client = EscrowClient::new(&env, &contract_id);

    let reason = Symbol::new(&env, "non_delivery");
    let description = String::from_str(&env, "Item never arrived");
    let evidence = BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &escrow_id, &reason, &description, &evidence);

    (env, contract_id, buyer, resolvers, escrow_id)
}

fn state_of(env: &Env, contract_id: &Address, escrow_id: u64) -> EscrowState {
    let client = EscrowClient::new(env, contract_id);
    client.get_escrow(&escrow_id).state
}

#[test]
fn vote_reaching_threshold_resolves_dispute() {
    let (env, contract_id, _buyer, r, escrow_id) = setup_disputed_multi(2);
    let client = EscrowClient::new(&env, &contract_id);

    // First vote: threshold (2) not yet met, dispute stays open.
    client.vote(&r[0], &escrow_id, &ResolutionType::Release);
    assert_eq!(
        state_of(&env, &contract_id, escrow_id),
        EscrowState::Disputed
    );

    // Second matching vote reaches the threshold and transitions the escrow.
    client.vote(&r[1], &escrow_id, &ResolutionType::Release);
    assert_eq!(
        state_of(&env, &contract_id, escrow_id),
        EscrowState::PendingFinalization,
    );
}

#[test]
fn vote_below_threshold_records_without_resolving() {
    let (env, contract_id, _buyer, r, escrow_id) = setup_disputed_multi(2);
    let client = EscrowClient::new(&env, &contract_id);

    client.vote(&r[0], &escrow_id, &ResolutionType::Release);

    // The vote is recorded but the dispute is not resolved.
    assert_eq!(client.get_resolver_votes(&escrow_id).len(), 1);
    assert_eq!(
        state_of(&env, &contract_id, escrow_id),
        EscrowState::Disputed
    );
}

#[test]
fn duplicate_vote_does_not_double_count() {
    let (env, contract_id, _buyer, r, escrow_id) = setup_disputed_multi(2);
    let client = EscrowClient::new(&env, &contract_id);

    // Same resolver votes the same way twice. The second call updates the
    // resolver's single vote in place rather than adding a new one, so it
    // cannot on its own satisfy a 2-of-3 threshold.
    client.vote(&r[0], &escrow_id, &ResolutionType::Release);
    client.vote(&r[0], &escrow_id, &ResolutionType::Release);

    assert_eq!(client.get_resolver_votes(&escrow_id).len(), 1);
    assert_eq!(
        state_of(&env, &contract_id, escrow_id),
        EscrowState::Disputed
    );
}

#[test]
fn unauthorized_resolver_cannot_vote() {
    let (env, contract_id, _buyer, _r, escrow_id) = setup_disputed_multi(2);
    let client = EscrowClient::new(&env, &contract_id);

    let stranger = Address::generate(&env);
    let res = client.try_vote(&stranger, &escrow_id, &ResolutionType::Release);
    assert_eq!(res, Err(Ok(ContractError::NotAuthorized)));

    // No vote should have been recorded for the rejected caller.
    assert_eq!(client.get_resolver_votes(&escrow_id).len(), 0);
}

#[test]
fn vote_on_non_disputed_escrow_is_rejected() {
    // Funded but never disputed: vote() must refuse to touch it.
    let (env, contract_id, _buyer, r, escrow_id) = setup_funded_multi(2);
    let client = EscrowClient::new(&env, &contract_id);

    let res = client.try_vote(&r[0], &escrow_id, &ResolutionType::Release);
    assert_eq!(res, Err(Ok(ContractError::InvalidState)));
}

#[test]
fn resolver_can_change_vote_to_flip_outcome() {
    let (env, contract_id, _buyer, r, escrow_id) = setup_disputed_multi(2);
    let client = EscrowClient::new(&env, &contract_id);

    // Split decision: one Release, one Refund — neither reaches threshold 2.
    client.vote(&r[0], &escrow_id, &ResolutionType::Release);
    client.vote(&r[1], &escrow_id, &ResolutionType::Refund);
    assert_eq!(
        state_of(&env, &contract_id, escrow_id),
        EscrowState::Disputed
    );

    // r0 changes their vote to Refund, giving Refund the 2 votes it needs.
    client.vote(&r[0], &escrow_id, &ResolutionType::Refund);
    assert_eq!(
        state_of(&env, &contract_id, escrow_id),
        EscrowState::PendingFinalization,
    );

    // The recorded resolution reflects the flipped outcome.
    let dispute = client.get_dispute(&escrow_id).expect("dispute exists");
    assert_eq!(dispute.get_resolution(), Some(ResolutionType::Refund));
}
