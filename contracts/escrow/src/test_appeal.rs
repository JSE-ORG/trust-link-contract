#![cfg(test)]
//! Comprehensive coverage for `appeal_dispute` (#672).
//!
//! `test_finalize_dispute_appeal_boundary.rs` only pins the appeal-window
//! boundary ticks consumed by `finalize_dispute`. This module covers the
//! edge cases of `appeal_dispute` itself: exhausting `MAX_APPEALS`, calls by
//! a non-participant, calls after finalization has completed, and calls on
//! an escrow that was never disputed.

use crate::{
    ContractError, DisputeStatus, Escrow, EscrowClient, EscrowState, Payee, ResolutionType,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

const AMOUNT: i128 = 1_000;

struct Setup {
    env: Env,
    admin: Address,
    fee_collector: Address,
    token: Address,
    contract_id: Address,
    buyer: Address,
    seller: Address,
    resolver: Address,
    escrow_id: u64,
}

/// Creates and funds a single-resolver escrow, ready for `raise_dispute`.
fn setup_funded_escrow() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &50_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_address,
        &AMOUNT,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
    token_admin_client.mint(&buyer, &AMOUNT);
    client.fund_escrow(&escrow_id, &buyer);

    Setup {
        env,
        admin,
        fee_collector,
        token: token_address,
        contract_id,
        buyer,
        seller,
        resolver,
        escrow_id,
    }
}

/// Raises a dispute and resolves it (single resolver ⇒ threshold 1 ⇒
/// resolves immediately), leaving the escrow in `PendingFinalization`.
fn raise_and_resolve(setup: &Setup) {
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let reason = Symbol::new(&setup.env, "ITEM_NOT_RECEIVED");
    let description = soroban_sdk::Bytes::from_slice(&setup.env, b"Item never arrived");
    let evidence = BytesN::from_array(&setup.env, &[0xab; 32]);
    client.raise_dispute(
        &setup.buyer,
        &setup.escrow_id,
        &reason,
        &description,
        &evidence,
    );
    client.resolve_dispute(&setup.resolver, &setup.escrow_id, &ResolutionType::Release);
    assert_eq!(
        client.get_escrow(&setup.escrow_id).state,
        EscrowState::PendingFinalization,
    );
}

#[test]
fn appeal_fails_once_max_appeals_reached() {
    let setup = setup_funded_escrow();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    raise_and_resolve(&setup);

    // Exhaust all MAX_APPEALS appeal/resolve cycles.
    for expected_count in 1..=crate::MAX_APPEALS {
        client.appeal_dispute(&setup.buyer, &setup.escrow_id);
        assert_eq!(
            client
                .get_dispute(&setup.escrow_id)
                .expect("dispute exists")
                .appeal_count,
            expected_count
        );
        assert_eq!(
            client.get_escrow(&setup.escrow_id).state,
            EscrowState::Disputed,
        );
        client.resolve_dispute(&setup.resolver, &setup.escrow_id, &ResolutionType::Release);
        assert_eq!(
            client.get_escrow(&setup.escrow_id).state,
            EscrowState::PendingFinalization,
        );
    }

    // One more appeal attempt must be rejected — the cap has been reached.
    let res = client.try_appeal_dispute(&setup.buyer, &setup.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::MaxAppealsReached)));

    // State is unaffected by the rejected appeal.
    assert_eq!(
        client.get_escrow(&setup.escrow_id).state,
        EscrowState::PendingFinalization,
    );
    assert_eq!(
        client
            .get_dispute(&setup.escrow_id)
            .expect("dispute exists")
            .appeal_count,
        crate::MAX_APPEALS,
    );
}

#[test]
fn appeal_fails_for_non_participant() {
    let setup = setup_funded_escrow();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    raise_and_resolve(&setup);

    let stranger = Address::generate(&setup.env);
    let res = client.try_appeal_dispute(&stranger, &setup.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::NotAuthorized)));

    // Nothing changed — still awaiting finalization, no appeal recorded.
    assert_eq!(
        client.get_escrow(&setup.escrow_id).state,
        EscrowState::PendingFinalization,
    );
    assert_eq!(
        client
            .get_dispute(&setup.escrow_id)
            .expect("dispute exists")
            .appeal_count,
        0,
    );
}

#[test]
fn appeal_fails_after_finalization_completed() {
    let setup = setup_funded_escrow();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    raise_and_resolve(&setup);

    let resolved_at = client
        .get_dispute(&setup.escrow_id)
        .expect("dispute exists")
        .resolved_at;
    let appeal_deadline = resolved_at + crate::APPEAL_WINDOW;

    // Let the appeal window elapse, then finalize.
    setup.env.ledger().set_timestamp(appeal_deadline);
    client.finalize_dispute(&setup.resolver, &setup.escrow_id);
    assert_eq!(
        client.get_escrow(&setup.escrow_id).state,
        EscrowState::Completed,
    );

    // Appeal is no longer possible once finalization has completed.
    let res = client.try_appeal_dispute(&setup.buyer, &setup.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::NotPendingFinalization)));
    assert_eq!(
        client.get_dispute(&setup.escrow_id).unwrap().status,
        DisputeStatus::Resolved,
    );
}

#[test]
fn appeal_fails_on_non_disputed_escrow() {
    let setup = setup_funded_escrow();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    // Escrow is `Funded` — no dispute has ever been raised.
    assert_eq!(
        client.get_escrow(&setup.escrow_id).state,
        EscrowState::Funded,
    );

    let res = client.try_appeal_dispute(&setup.buyer, &setup.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::NotPendingFinalization)));
}

#[test]
fn appeal_fails_after_appeal_window_expires() {
    let setup = setup_funded_escrow();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    raise_and_resolve(&setup);

    let resolved_at = client
        .get_dispute(&setup.escrow_id)
        .expect("dispute exists")
        .resolved_at;
    let appeal_deadline = resolved_at + crate::APPEAL_WINDOW;

    // Exactly at the deadline the appeal window is considered closed
    // (`now >= appeal_deadline`), mirroring `finalize_dispute`'s
    // complementary `now < appeal_deadline` boundary.
    setup.env.ledger().set_timestamp(appeal_deadline);

    let res = client.try_appeal_dispute(&setup.buyer, &setup.escrow_id);
    assert_eq!(res, Err(Ok(ContractError::DisputeWindowStillOpen)));
    assert_eq!(
        client.get_escrow(&setup.escrow_id).state,
        EscrowState::PendingFinalization,
    );
}

#[test]
fn appeal_succeeds_by_seller_and_reopens_dispute() {
    let setup = setup_funded_escrow();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    raise_and_resolve(&setup);

    // The seller (not just the buyer) is a valid appellant.
    client.appeal_dispute(&setup.seller, &setup.escrow_id);

    assert_eq!(
        client.get_escrow(&setup.escrow_id).state,
        EscrowState::Disputed,
    );
    let dispute = client
        .get_dispute(&setup.escrow_id)
        .expect("dispute exists");
    assert_eq!(dispute.appeal_count, 1);
    assert_eq!(dispute.status, DisputeStatus::Active);
    assert_eq!(dispute.resolution, 0);
}

// ── Issue #1205: appeal fee escalation ──────────────────────────────────────

/// Event helper: true if a `DisputeAppealed` event with the given fee/round
/// was emitted by the most recent invocation.
fn appealed_event_matches(setup: &Setup, fee: i128, round: u32) -> bool {
    use soroban_sdk::{testutils::Events as _, TryFromVal, Val};
    let expected_topic = Symbol::new(&setup.env, "Dispute");
    setup
        .env
        .events()
        .all()
        .filter_by_contract(&setup.contract_id)
        .events()
        .iter()
        .any(|event| match &event.body {
            soroban_sdk::xdr::ContractEventBody::V0(v0) => {
                let Some(topic) = v0.topics.iter().next() else {
                    return false;
                };
                if Symbol::try_from_val(&setup.env, topic).ok() != Some(expected_topic.clone()) {
                    return false;
                }
                let Ok(data) = Val::try_from_val(&setup.env, &v0.data) else {
                    return false;
                };
                crate::events::DisputeAppealed::try_from_val(&setup.env, &data)
                    .map(|ev| ev.appeal_fee_charged == fee && ev.appeal_round == round)
                    .unwrap_or(false)
            }
        })
}

#[test]
fn appeal_fee_escalates_linearly_per_round() {
    let setup = setup_funded_escrow();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let token_client = token::Client::new(&setup.env, &setup.token);
    token::StellarAssetClient::new(&setup.env, &setup.token).mint(&setup.buyer, &10_000);

    const BASE_BPS: u32 = 100;
    client.set_appeal_fee(&setup.admin, &BASE_BPS);

    raise_and_resolve(&setup);

    let mut previous_fee = 0_i128;
    for round in 1..=crate::MAX_APPEALS {
        let amount = client.get_escrow(&setup.escrow_id).amount;
        let expected = crate::helpers::payout::calculate_fee(amount, BASE_BPS * round).unwrap();
        let collector_before = token_client.balance(&setup.fee_collector);
        let buyer_before = token_client.balance(&setup.buyer);

        client.appeal_dispute(&setup.buyer, &setup.escrow_id);
        // Must run before any further contract call, which resets the buffer.
        assert!(appealed_event_matches(&setup, expected, round));

        let charged = token_client.balance(&setup.fee_collector) - collector_before;
        assert_eq!(charged, expected, "round {round} fee");
        assert_eq!(buyer_before - token_client.balance(&setup.buyer), expected);
        assert!(charged > previous_fee, "fee must ascend each round");
        previous_fee = charged;

        client.resolve_dispute(&setup.resolver, &setup.escrow_id, &ResolutionType::Release);
    }
}

#[test]
fn appeal_fails_when_balance_covers_only_the_base_fee() {
    let setup = setup_funded_escrow();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let token_client = token::Client::new(&setup.env, &setup.token);

    const BASE_BPS: u32 = 500;
    client.set_appeal_fee(&setup.admin, &BASE_BPS);
    raise_and_resolve(&setup);

    let amount = client.get_escrow(&setup.escrow_id).amount;
    let round1 = crate::helpers::payout::calculate_fee(amount, BASE_BPS).unwrap();
    let round2 = crate::helpers::payout::calculate_fee(amount, BASE_BPS * 2).unwrap();
    assert!(round2 > round1);

    // Enough for round 1 (and one stroop short of round 2 afterwards).
    token::StellarAssetClient::new(&setup.env, &setup.token)
        .mint(&setup.buyer, &(round1 + round2 - 1));
    assert_eq!(token_client.balance(&setup.buyer), round1 + round2 - 1);

    client.appeal_dispute(&setup.buyer, &setup.escrow_id);
    client.resolve_dispute(&setup.resolver, &setup.escrow_id, &ResolutionType::Release);

    // Round 2 charges 2x, which the remaining balance cannot cover.
    assert_eq!(token_client.balance(&setup.buyer), round2 - 1);
    let res = client.try_appeal_dispute(&setup.buyer, &setup.escrow_id);
    assert!(res.is_err());
    assert_eq!(
        client.get_escrow(&setup.escrow_id).state,
        EscrowState::PendingFinalization
    );
}
