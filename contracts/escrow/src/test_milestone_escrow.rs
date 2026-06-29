#![cfg(test)]
//! Tests for milestone-based escrows (#351): create_milestone_escrow stages
//! a single funded balance across multiple sequential payouts instead of
//! one lump sum.
//!
//! Covers both acceptance criteria directly:
//! - "Released milestones non-replayable" -> `release_milestone_is_not_replayable`
//! - "Sum matches total balance" -> `funding_transfers_exact_sum_of_milestones`
//!   and `escrow_amount_tracks_unreleased_sum_after_partial_release`

use crate::{ContractError, Escrow, EscrowClient, EscrowData, EscrowState, DataKey, Milestone};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    token, vec, Address, Env, FromVal, Symbol, Vec,
};

const DISPUTE_WINDOW_SECS: u64 = 172_800; // 48h, matches the contract constant.

struct Fx {
    env: Env,
    client: EscrowClient<'static>,
    contract_id: Address,
    escrow_id: u64,
    seller: Address,
    buyer: Address,
    token_addr: Address,
    milestone_amounts: Vec<i128>,
}

/// Creates and funds a 3-stage milestone escrow (300 / 200 / 500 = 1000
/// total), then advances past the dispute window so releases are unblocked.
fn setup_funded_milestone_escrow() -> Fx {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let milestone_amounts: Vec<i128> = vec![&env, 300, 200, 500];
    let total: i128 = milestone_amounts.iter().sum();

    let escrow_id = client.create_milestone_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &milestone_amounts,
        &0_u32,
        &0_u64,
    );

    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &total);
    client.fund_escrow(&escrow_id, &buyer);

    // Clear the dispute window so release_milestone calls aren't blocked by
    // DeliveryBeforeDisputeWindow in tests that aren't specifically about it.
    env.ledger()
        .with_mut(|li| li.timestamp += DISPUTE_WINDOW_SECS + 1);

    Fx {
        env,
        client,
        contract_id,
        escrow_id,
        seller,
        buyer,
        token_addr,
        milestone_amounts,
    }
}

fn load_escrow(fx: &Fx) -> EscrowData {
    fx.env
        .as_contract(&fx.contract_id, || {
            fx.env
                .storage()
                .persistent()
                .get(&DataKey::Escrow(fx.escrow_id))
        })
        .expect("escrow exists")
}

// ── create_milestone_escrow ─────────────────────────────────────────────────

#[test]
fn create_milestone_escrow_computes_total_from_amounts() {
    let fx = setup_funded_milestone_escrow();
    let escrow = load_escrow(&fx);

    // Total is derived from the Vec, not a separately-supplied parameter -
    // this is what makes "sum matches total balance" hold by construction.
    assert_eq!(escrow.amount, 1_000);

    let milestones = escrow.milestones.expect("milestone escrow has milestones");
    assert_eq!(milestones.len(), 3);
    assert_eq!(milestones.get(0).unwrap(), Milestone { amount: 300, released: false });
    assert_eq!(milestones.get(1).unwrap(), Milestone { amount: 200, released: false });
    assert_eq!(milestones.get(2).unwrap(), Milestone { amount: 500, released: false });
}

#[test]
fn create_milestone_escrow_rejects_empty_vec() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(token_admin).address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let empty: Vec<i128> = vec![&env];
    let result = client.try_create_milestone_escrow(
        &seller, &None::<Address>, &resolver, &token_addr, &empty, &0_u32, &0_u64,
    );
    assert_eq!(result, Err(Ok(ContractError::EmptyMilestones)));
}

#[test]
fn create_milestone_escrow_rejects_too_many_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(token_admin).address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut too_many: Vec<i128> = Vec::new(&env);
    for _ in 0..=crate::MAX_MILESTONES {
        too_many.push_back(10);
    }

    let result = client.try_create_milestone_escrow(
        &seller, &None::<Address>, &resolver, &token_addr, &too_many, &0_u32, &0_u64,
    );
    assert_eq!(result, Err(Ok(ContractError::TooManyMilestones)));
}

#[test]
fn create_milestone_escrow_rejects_non_positive_milestone_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(token_admin).address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amounts: Vec<i128> = vec![&env, 100, 0, 50];
    let result = client.try_create_milestone_escrow(
        &seller, &None::<Address>, &resolver, &token_addr, &amounts, &0_u32, &0_u64,
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
}

// ── funding: sum matches total balance ──────────────────────────────────────

#[test]
fn funding_transfers_exact_sum_of_milestones() {
    let fx = setup_funded_milestone_escrow();
    let token_client = token::Client::new(&fx.env, &fx.token_addr);

    let expected_total: i128 = fx.milestone_amounts.iter().sum();
    assert_eq!(expected_total, 1_000);
    assert_eq!(token_client.balance(&fx.contract_id), expected_total);

    // And the escrow's own bookkeeping agrees with the actual token balance.
    let escrow = load_escrow(&fx);
    assert_eq!(escrow.amount, expected_total);
}

// ── release_milestone ────────────────────────────────────────────────────────

#[test]
fn release_milestone_pays_seller_and_marks_released() {
    let fx = setup_funded_milestone_escrow();
    let token_client = token::Client::new(&fx.env, &fx.token_addr);

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &0_u32);

    assert_eq!(token_client.balance(&fx.seller), 300);

    let escrow = load_escrow(&fx);
    assert_eq!(escrow.state, EscrowState::Funded, "escrow stays open until every stage is released");
    assert_eq!(escrow.amount, 700, "remaining balance excludes the released stage");

    let milestones = escrow.milestones.unwrap();
    assert!(milestones.get(0).unwrap().released);
    assert!(!milestones.get(1).unwrap().released);
    assert!(!milestones.get(2).unwrap().released);
}

#[test]
fn release_milestone_is_not_replayable() {
    let fx = setup_funded_milestone_escrow();
    let token_client = token::Client::new(&fx.env, &fx.token_addr);

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &0_u32);
    assert_eq!(token_client.balance(&fx.seller), 300);

    // Replaying the same milestone must fail, and must not pay out again.
    let result = fx.client.try_release_milestone(&fx.buyer, &fx.escrow_id, &0_u32);
    assert_eq!(result, Err(Ok(ContractError::MilestoneAlreadyReleased)));
    assert_eq!(
        token_client.balance(&fx.seller),
        300,
        "a replayed release must not transfer funds a second time"
    );

    let escrow = load_escrow(&fx);
    assert_eq!(escrow.amount, 700, "remaining balance is unchanged by the rejected replay");
}

#[test]
fn releasing_all_milestones_completes_the_escrow() {
    let fx = setup_funded_milestone_escrow();
    let token_client = token::Client::new(&fx.env, &fx.token_addr);

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &0_u32);
    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &1_u32);
    assert_eq!(load_escrow(&fx).state, EscrowState::Funded);

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &2_u32);

    assert_eq!(token_client.balance(&fx.seller), 1_000);

    let escrow = load_escrow(&fx);
    assert_eq!(escrow.state, EscrowState::Completed);
    assert_eq!(escrow.amount, 0);
    assert!(escrow.milestones.unwrap().iter().all(|m| m.released));

    // The completion counter increments exactly once, on the final release,
    // not once per milestone.
    let stats = fx.client.get_stats();
    assert_eq!(stats.total_completed, 1);
}

#[test]
fn milestones_can_be_released_out_of_order() {
    let fx = setup_funded_milestone_escrow();
    let token_client = token::Client::new(&fx.env, &fx.token_addr);

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &2_u32);
    assert_eq!(token_client.balance(&fx.seller), 500);

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &0_u32);
    assert_eq!(token_client.balance(&fx.seller), 800);

    let escrow = load_escrow(&fx);
    assert_eq!(escrow.amount, 200, "only the un-released middle stage remains");
    let milestones = escrow.milestones.unwrap();
    assert!(milestones.get(0).unwrap().released);
    assert!(!milestones.get(1).unwrap().released);
    assert!(milestones.get(2).unwrap().released);
}

#[test]
fn escrow_amount_tracks_unreleased_sum_after_partial_release() {
    let fx = setup_funded_milestone_escrow();

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &1_u32); // releases 200

    let escrow = load_escrow(&fx);
    let unreleased_sum: i128 = escrow
        .milestones
        .as_ref()
        .unwrap()
        .iter()
        .filter(|m| !m.released)
        .map(|m| m.amount)
        .sum();

    // The core invariant: escrow.amount always equals the sum of whatever
    // hasn't been paid out yet, so confirm_delivery/resolve_dispute/
    // auto_release would settle exactly the right remainder if ever used
    // on a partially-released milestone escrow.
    assert_eq!(escrow.amount, unreleased_sum);
    assert_eq!(escrow.amount, 800);
}

#[test]
fn final_release_emits_escrow_completed_with_the_last_stage_amount_not_zero() {
    let fx = setup_funded_milestone_escrow();

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &0_u32);
    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &1_u32);

    fx.client.release_milestone(&fx.buyer, &fx.escrow_id, &2_u32); // final stage, amount 500

    // Find the escrow_completed event emitted by this call. Using a plain
    // for-loop (IntoIterator) rather than calling .len()/.iter()/.get() on
    // the events collection directly - those method names don't exist on
    // this SDK version's event-list type, but a for-loop only needs
    // IntoIterator, which is the safer bet without the SDK docs in hand.
    let mut completed: Option<(soroban_sdk::Address, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> = None;
    for (contract_id, topics, data) in fx.env.events().all() {
        let is_completed = topics
            .iter()
            .any(|t| Symbol::from_val(&fx.env, &t) == Symbol::new(&fx.env, "escrow_completed"));
        if is_completed {
            completed = Some((contract_id, topics, data));
        }
    }
    let (_, _, data) = completed.expect("escrow_completed event was emitted");

    use crate::EscrowCompleted;
    let payload = EscrowCompleted::from_val(&fx.env, &data);
    assert_eq!(
        payload.amount, 500,
        "completion event must report the final stage's amount, not the \
         already-zeroed escrow.amount"
    );
}

#[test]
fn release_milestone_rejects_invalid_index() {
    let fx = setup_funded_milestone_escrow();
    let result = fx.client.try_release_milestone(&fx.buyer, &fx.escrow_id, &99_u32);
    assert_eq!(result, Err(Ok(ContractError::MilestoneNotFound)));
}

#[test]
fn release_milestone_rejects_on_non_milestone_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(token_admin).address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees: Vec<crate::Payee> = Vec::new(&env);
    payees.push_back(crate::Payee { address: seller.clone(), bps: 10_000 });
    let escrow_id = client.create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &0_u32,
        &0_u64,
    );
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &1_000_i128);
    client.fund_escrow(&escrow_id, &buyer);
    env.ledger().with_mut(|li| li.timestamp += DISPUTE_WINDOW_SECS + 1);

    let result = client.try_release_milestone(&buyer, &escrow_id, &0_u32);
    assert_eq!(result, Err(Ok(ContractError::NotMilestoneEscrow)));
    let _ = contract_id;
}

#[test]
fn release_milestone_requires_buyer_authorization() {
    let fx = setup_funded_milestone_escrow();
    let impostor = Address::generate(&fx.env);
    let result = fx.client.try_release_milestone(&impostor, &fx.escrow_id, &0_u32);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn release_milestone_respects_the_dispute_window() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(token_admin).address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amounts: Vec<i128> = vec![&env, 300, 700];
    let escrow_id = client.create_milestone_escrow(
        &seller, &None::<Address>, &resolver, &token_addr, &amounts, &0_u32, &0_u64,
    );
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &1_000_i128);
    client.fund_escrow(&escrow_id, &buyer);

    // No time advance this time - still inside the dispute window.
    let result = client.try_release_milestone(&buyer, &escrow_id, &0_u32);
    assert_eq!(result, Err(Ok(ContractError::DeliveryBeforeDisputeWindow)));
    let _ = contract_id;
}