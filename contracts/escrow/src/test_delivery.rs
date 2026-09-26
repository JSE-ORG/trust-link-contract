#![cfg(test)]

use crate::test_helpers::{advance_time, create_funded_escrow, setup_contract};
use crate::{ContractError, DeliveryRecorded, EscrowState, Payee};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    Address, Env, IntoVal, String as SorobanString, Symbol, TryFromVal, Val, Vec,
};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

fn has_event<T, F>(env: &Env, contract_id: &Address, topic: &str, predicate: F) -> bool
where
    T: TryFromVal<Env, Val>,
    F: Fn(&T) -> bool,
{
    let expected_topic = Symbol::new(env, topic);
    env.events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .iter()
        .any(|event| match &event.body {
            soroban_sdk::xdr::ContractEventBody::V0(v0) => {
                let Some(topic) = v0.topics.iter().next() else {
                    return false;
                };

                let Ok(topic) = Symbol::try_from_val(env, topic) else {
                    return false;
                };
                if topic != expected_topic {
                    return false;
                }

                let Ok(data) = Val::try_from_val(env, &v0.data) else {
                    return false;
                };

                T::try_from_val(env, &data)
                    .map(|event| predicate(&event))
                    .unwrap_or(false)
            }
        })
}

#[test]
fn test_mark_shipped_transitions_state() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    let expected_ts = env.ledger().timestamp();
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-001"));

    assert!(has_event::<crate::EscrowShipped, _>(
        &env,
        &contract_id,
        "Escrow",
        |event| {
            event.escrow_id == id
                && event.seller == seller
                && event.tracking_id == SorobanString::from_str(&env, "TRACK-001")
                && event.timestamp == expected_ts
        }
    ));

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Shipped);
    assert_eq!(escrow.shipped_at, expected_ts);
    assert_eq!(
        escrow.tracking_id,
        Some(SorobanString::from_str(&env, "TRACK-001"))
    );
}

#[test]
fn test_mark_shipped_rejects_empty_tracking_id() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    let res = client.try_mark_shipped(&seller, &id, &SorobanString::from_str(&env, ""));
    assert!(matches!(res, Err(Ok(ContractError::InvalidTrackingId))));

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Funded);
}

#[test]
fn test_record_delivery_sets_timestamp() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-002"));

    advance_time(&env, 60);
    client.propose_record_delivery(&admin, &id);
    advance_time(&env, crate::DELIVERY_TIMELOCK);
    let expected_ts = env.ledger().timestamp();

    client.record_delivery(&admin, &id);

    assert!(has_event::<DeliveryRecorded, _>(
        &env,
        &contract_id,
        "Escrow",
        |event| { event.escrow_id == id && event.delivered_at == expected_ts }
    ));

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Shipped);
    assert_eq!(escrow.delivered_at, Some(expected_ts));
}

#[test]
fn test_record_delivery_requires_shipped_state() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    let res = client.try_record_delivery(&admin, &id);
    assert!(matches!(res, Err(Ok(crate::ContractError::InvalidState))));
}

#[test]
fn test_confirm_delivery_after_mark_shipped() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-003"));

    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.dispute_deadline + 1);
    client.confirm_delivery(&buyer, &id);

    assert!(has_event::<crate::EscrowCompleted, _>(
        &env,
        &contract_id,
        "Escrow",
        |event| { event.escrow_id == id && event.recipient == seller }
    ));

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Completed);

    let balance = soroban_sdk::token::Client::new(&env, &token).balance(&seller);
    assert_eq!(balance, 1000);

    let _ = contract_id;
}

#[test]
fn test_confirm_delivery_during_dispute_window_reverts() {
    // Regression: confirming while the dispute window is still open must return
    // `DisputeWindowStillOpen` (not `DeliveryBeforeDisputeWindow`, which means
    // the window has not started — impossible for a Shipped escrow).
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-WIN"));

    // Ledger time is still well before `dispute_deadline` (funded_at + 172_800).
    let escrow = client.get_escrow(&id);
    assert!(env.ledger().timestamp() < escrow.dispute_deadline);

    assert_eq!(
        client.try_confirm_delivery(&buyer, &id),
        Err(Ok(ContractError::DisputeWindowStillOpen)),
    );
    assert_eq!(client.get_escrow(&id).state, EscrowState::Shipped);

    // One second before the deadline still rejects; at the deadline it succeeds.
    env.ledger().set_timestamp(escrow.dispute_deadline - 1);
    assert_eq!(
        client.try_confirm_delivery(&buyer, &id),
        Err(Ok(ContractError::DisputeWindowStillOpen)),
    );

    env.ledger().set_timestamp(escrow.dispute_deadline);
    client.confirm_delivery(&buyer, &id);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
}

#[test]
fn test_confirm_delivery_by_vendor_reverts() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-004"));

    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.dispute_deadline + 1);

    assert_eq!(
        client.try_confirm_delivery(&seller, &id),
        Err(Ok(ContractError::NotAuthorizedBuyer)),
    );
}

#[test]
fn test_confirm_delivery_by_third_party_reverts() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let intruder = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-005"));

    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.dispute_deadline + 1);

    assert_eq!(
        client.try_confirm_delivery(&intruder, &id),
        Err(Ok(ContractError::NotAuthorizedBuyer)),
    );
}

/// Tests that record_delivery records the exact timestamp from the current ledger.
/// This verifies that the delivered_at value matches the environment's timestamp
/// precisely at the moment of invocation with no offset or modification.
#[test]
fn test_record_delivery_timestamp_matches_ledger_timestamp() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK001"));

    advance_time(&env, 60);
    client.propose_record_delivery(&admin, &id);
    advance_time(&env, crate::DELIVERY_TIMELOCK);
    let expected_ts = env.ledger().timestamp();

    client.record_delivery(&admin, &id);

    assert!(has_event::<DeliveryRecorded, _>(
        &env,
        &contract_id,
        "Escrow",
        |event| { event.escrow_id == id && event.delivered_at == expected_ts }
    ));

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.delivered_at, Some(expected_ts));

    let _ = contract_id;
}

#[test]
fn test_record_delivery_rejects_zero_timestamp() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK001"));

    let _escrow_before = client.get_escrow(&id);
    env.ledger().set_timestamp(0);
    client.propose_record_delivery(&admin, &id);
    env.ledger().set_timestamp(crate::DELIVERY_TIMELOCK);

    client.record_delivery(&admin, &id);

    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.delivered_at, Some(crate::DELIVERY_TIMELOCK));
}

#[test]
fn test_record_delivery_accepts_maximum_valid_timestamp() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK001"));

    let max_ts: u64 = 100_000_000_000;
    env.ledger()
        .set_timestamp(max_ts - crate::DELIVERY_TIMELOCK);
    client.propose_record_delivery(&admin, &id);
    env.ledger().set_timestamp(max_ts);

    client.record_delivery(&admin, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.delivered_at, Some(max_ts));
}

#[test]
fn test_record_delivery_rejects_duplicate_call() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, admin, _fee) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK001"));

    env.ledger()
        .set_timestamp(1_700_000_100 - crate::DELIVERY_TIMELOCK);
    client.propose_record_delivery(&admin, &id);
    env.ledger().set_timestamp(1_700_000_100);
    client.record_delivery(&admin, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.delivered_at, Some(1_700_000_100));

    env.ledger().set_timestamp(1_700_000_200);
    let result = client.try_record_delivery(&admin, &id);
    assert_eq!(result, Err(Ok(ContractError::DeliveryAlreadyRecorded)));

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.delivered_at, Some(1_700_000_100));
    assert_eq!(escrow.state, EscrowState::Shipped);

    let _ = contract_id;
}

#[test]
fn test_record_delivery_timelock_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, admin, _fee) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    client.mark_shipped(
        &seller,
        &id,
        &SorobanString::from_str(&env, "TRACK-TIMELOCK"),
    );

    // Propose delivery
    env.ledger().set_timestamp(1_000_000);
    client.propose_record_delivery(&admin, &id);

    // Try recording before timelock elapses
    env.ledger()
        .set_timestamp(1_000_000 + crate::DELIVERY_TIMELOCK - 1);
    let res = client.try_record_delivery(&admin, &id);
    assert_eq!(res, Err(Ok(ContractError::TimelockNotElapsed)));

    // Try cancelling proposal by non-admin
    let res = client.try_cancel_delivery_proposal(&seller, &id);
    assert_eq!(res, Err(Ok(ContractError::NotAuthorized)));

    // Try recording after timelock elapses
    env.ledger()
        .set_timestamp(1_000_000 + crate::DELIVERY_TIMELOCK);
    client.record_delivery(&admin, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(
        escrow.delivered_at,
        Some(1_000_000 + crate::DELIVERY_TIMELOCK)
    );
}

#[test]
fn test_cancel_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, admin, _fee) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-CANCEL"));

    client.propose_record_delivery(&admin, &id);
    client.cancel_delivery_proposal(&admin, &id);

    // Even after timelock duration, recording delivery should fail because proposal was cancelled
    advance_time(&env, crate::DELIVERY_TIMELOCK);
    let res = client.try_record_delivery(&admin, &id);
    assert_eq!(res, Err(Ok(ContractError::DeliveryNotProposed)));
}

#[test]
fn test_confirm_delivery_from_pending_state_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create escrow with an explicit buyer so authorization passes.
    let mut payees_16 = Vec::new(&env);
    payees_16.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_16.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &1000_i128,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<SorobanString>,
    );

    let res = client.try_confirm_delivery(&buyer, &id);
    assert_eq!(res, Err(Ok(ContractError::InvalidStateTransition)));
}

#[test]
fn test_confirm_delivery_from_funded_state_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    let res = client.try_confirm_delivery(&buyer, &id);
    assert_eq!(res, Err(Ok(ContractError::InvalidStateTransition)));
}

#[test]
fn test_confirm_delivery_from_disputed_state_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "reason"),
        &SorobanString::from_str(&env, "dispute description"),
        &soroban_sdk::BytesN::from_array(&env, &[0; 32]),
    );

    let res = client.try_confirm_delivery(&buyer, &id);
    assert_eq!(res, Err(Ok(ContractError::InvalidStateTransition)));
}

#[test]
fn test_confirm_delivery_from_canceled_state_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create escrow with an explicit buyer.
    let mut payees_15 = Vec::new(&env);
    payees_15.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_15.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &1000_i128,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<SorobanString>,
    );

    client.cancel_escrow(&seller, &id);

    let res = client.try_confirm_delivery(&buyer, &id);
    assert_eq!(res, Err(Ok(ContractError::InvalidStateTransition)));
}

#[test]
fn test_confirm_delivery_from_completed_state_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-001"));

    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.dispute_deadline + 1);

    client.confirm_delivery(&buyer, &id);

    // Confirming again on a Completed escrow now gets the dedicated code
    // (was InvalidStateTransition).
    let res = client.try_confirm_delivery(&buyer, &id);
    assert_eq!(res, Err(Ok(ContractError::EscrowAlreadyCompleted)));
}

// ============================================================================
// DeliveryProposal cleanup on every exit from Shipped
// ============================================================================

fn has_delivery_proposal(env: &Env, client: &crate::EscrowClient, id: u64) -> bool {
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .has(&crate::DataKey::DeliveryProposal(id))
    })
}

/// Creates a funded escrow, ships it, and has the admin propose delivery.
/// Returns `(id, seller, buyer)`.
fn shipped_with_proposal(
    env: &Env,
    client: &crate::EscrowClient,
    admin: &Address,
) -> (u64, Address, Address) {
    let token = register_token(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let id = create_funded_escrow(
        env, client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );
    client.mark_shipped(&seller, &id, &SorobanString::from_str(env, "TRACK-PROP"));
    client.propose_record_delivery(admin, &id);
    assert!(has_delivery_proposal(env, client, id));
    (id, seller, buyer)
}

#[test]
fn confirm_delivery_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let (id, _seller, buyer) = shipped_with_proposal(&env, &client, &admin);

    advance_time(&env, crate::DISPUTE_WINDOW);
    client.confirm_delivery(&buyer, &id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
    assert!(!has_delivery_proposal(&env, &client, id));
}

#[test]
fn raise_dispute_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let (id, _seller, buyer) = shipped_with_proposal(&env, &client, &admin);

    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "damaged"),
        &SorobanString::from_str(&env, "arrived broken"),
        &soroban_sdk::BytesN::from_array(&env, &[0x11; 32]),
    );

    assert_eq!(client.get_escrow(&id).state, EscrowState::Disputed);
    assert!(!has_delivery_proposal(&env, &client, id));
}

#[test]
fn co_signed_release_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let (id, seller, _buyer) = shipped_with_proposal(&env, &client, &admin);

    client.co_signed_release(&seller, &id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
    assert!(!has_delivery_proposal(&env, &client, id));
}

#[test]
fn emergency_drain_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let (id, _seller, _buyer) = shipped_with_proposal(&env, &client, &admin);

    client.pause_contract(&admin);
    client.emergency_drain(&id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Refunded);
    assert!(!has_delivery_proposal(&env, &client, id));
}

#[test]
fn reclaim_expired_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let token = register_token(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    crate::test_helpers::mint_token(&env, &token, &buyer, 1000);

    let expires_at = env.ledger().timestamp() + 3600;
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(expires_at),
        &0_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-EXP"));
    client.propose_record_delivery(&admin, &id);
    assert!(has_delivery_proposal(&env, &client, id));

    env.ledger().set_timestamp(expires_at);
    client.reclaim_expired(&id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Expired);
    assert!(!has_delivery_proposal(&env, &client, id));
}

// ============================================================================
// DeliveryProposal cleanup on every exit from Shipped
// ============================================================================

fn has_delivery_proposal(env: &Env, client: &crate::EscrowClient, id: u64) -> bool {
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .has(&crate::DataKey::DeliveryProposal(id))
    })
}

/// Creates a funded escrow, ships it, and has the admin propose delivery.
/// Returns `(id, seller, buyer)`.
fn shipped_with_proposal(
    env: &Env,
    client: &crate::EscrowClient,
    admin: &Address,
) -> (u64, Address, Address) {
    let token = register_token(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let id = create_funded_escrow(
        env, client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );
    client.mark_shipped(&seller, &id, &SorobanString::from_str(env, "TRACK-PROP"));
    client.propose_record_delivery(admin, &id);
    assert!(has_delivery_proposal(env, client, id));
    (id, seller, buyer)
}

#[test]
fn confirm_delivery_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let (id, _seller, buyer) = shipped_with_proposal(&env, &client, &admin);

    advance_time(&env, crate::DISPUTE_WINDOW);
    client.confirm_delivery(&buyer, &id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
    assert!(!has_delivery_proposal(&env, &client, id));
}

#[test]
fn raise_dispute_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let (id, _seller, buyer) = shipped_with_proposal(&env, &client, &admin);

    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "damaged"),
        &SorobanString::from_str(&env, "arrived broken"),
        &soroban_sdk::BytesN::from_array(&env, &[0x11; 32]),
    );

    assert_eq!(client.get_escrow(&id).state, EscrowState::Disputed);
    assert!(!has_delivery_proposal(&env, &client, id));
}

#[test]
fn co_signed_release_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let (id, seller, _buyer) = shipped_with_proposal(&env, &client, &admin);

    client.co_signed_release(&seller, &id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
    assert!(!has_delivery_proposal(&env, &client, id));
}

#[test]
fn emergency_drain_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let (id, _seller, _buyer) = shipped_with_proposal(&env, &client, &admin);

    client.pause_contract(&admin);
    client.emergency_drain(&id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Refunded);
    assert!(!has_delivery_proposal(&env, &client, id));
}

#[test]
fn reclaim_expired_clears_delivery_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let token = register_token(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    crate::test_helpers::mint_token(&env, &token, &buyer, 1000);

    let expires_at = env.ledger().timestamp() + 3600;
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(expires_at),
        &0_u64,
    );
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-EXP"));
    client.propose_record_delivery(&admin, &id);
    assert!(has_delivery_proposal(&env, &client, id));

    env.ledger().set_timestamp(expires_at);
    client.reclaim_expired(&id);

    assert_eq!(client.get_escrow(&id).state, EscrowState::Expired);
    assert!(!has_delivery_proposal(&env, &client, id));
}
