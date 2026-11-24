#![cfg(test)]

use crate::{
    ContractError, DisputeStatus, Escrow, EscrowClient, EscrowInput, EscrowState, Payee,
    ResolutionType, MAX_GRACE_PERIOD,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Vec,
};

fn setup() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    EscrowClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    (
        env,
        admin,
        seller,
        buyer,
        resolver,
        token_address,
        fee_collector,
        client,
    )
}

fn create_and_fund(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    buyer: &Address,
    resolver: &Address,
    token: &Address,
    amount: i128,
) -> u64 {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(env);
    let id = client.create_escrow(
        &payees_val,
        &Some(buyer.clone()),
        resolver,
        token,
        &amount,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(buyer, &amount);
    client.fund_escrow(&id, buyer);
    id
}

// ===========================================================================
// Issue #995: admin_cancel_pending_finalization tests
// ===========================================================================

#[test]
fn test_admin_cancel_pending_finalization_succeeds() {
    let (env, admin, seller, buyer, resolver, token, _, client) = setup();
    let amount = 1_000_i128;
    let id = create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, amount);

    let reason = Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let desc = soroban_sdk::Bytes::from_slice(&env, b"item not received");
    let evidence = BytesN::from_array(&env, &[1u8; 32]);
    client.raise_dispute(&buyer, &id, &reason, &desc, &evidence);

    // Resolve dispute -> transitions to PendingFinalization
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    let escrow_before = client.get_escrow(&id);
    assert_eq!(escrow_before.state, EscrowState::PendingFinalization);

    let token_client = token::TokenClient::new(&env, &token);
    assert_eq!(token_client.balance(&buyer), 0);

    // Admin cancels pending finalization
    client.cancel_pending_finalization(&admin, &id);

    // Verify escrow state is Refunded
    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, EscrowState::Refunded);

    // Verify buyer received their funds back
    assert_eq!(token_client.balance(&buyer), amount);

    // Verify dispute is updated to Resolved
    let dispute = client.get_dispute(&id).unwrap();
    assert_eq!(dispute.status, DisputeStatus::Resolved);
}

#[test]
fn test_cancel_pending_finalization_rejects_non_admin() {
    let (env, _, seller, buyer, resolver, token, _, client) = setup();
    let id = create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 1_000);

    let reason = Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let desc = soroban_sdk::Bytes::from_slice(&env, b"item not received");
    let evidence = BytesN::from_array(&env, &[1u8; 32]);
    client.raise_dispute(&buyer, &id, &reason, &desc, &evidence);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund);

    let intruder = Address::generate(&env);
    let result = client.try_cancel_pending_finalization(&intruder, &id);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn test_cancel_pending_finalization_rejects_invalid_state() {
    let (env, admin, seller, buyer, resolver, token, _, client) = setup();
    let id = create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 1_000);

    // Calling while in Funded state must fail
    let result = client.try_cancel_pending_finalization(&admin, &id);
    assert_eq!(result, Err(Ok(ContractError::NotPendingFinalization)));

    // Calling while in Disputed state must fail
    let reason = Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let desc = soroban_sdk::Bytes::from_slice(&env, b"item not received");
    let evidence = BytesN::from_array(&env, &[1u8; 32]);
    client.raise_dispute(&buyer, &id, &reason, &desc, &evidence);

    let result = client.try_cancel_pending_finalization(&admin, &id);
    assert_eq!(result, Err(Ok(ContractError::NotPendingFinalization)));
}

#[test]
fn test_admin_cancel_finalization_alias_works() {
    let (env, admin, seller, buyer, resolver, token, _, client) = setup();
    let id = create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 1_000);

    let reason = Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let desc = soroban_sdk::Bytes::from_slice(&env, b"item not received");
    let evidence = BytesN::from_array(&env, &[1u8; 32]);
    client.raise_dispute(&buyer, &id, &reason, &desc, &evidence);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund);

    client.admin_cancel_finalization(&admin, &id);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Refunded);
}

// ===========================================================================
// Issue #996: MAX_GRACE_PERIOD cap tests
// ===========================================================================

#[test]
fn test_create_escrow_with_expiration_rejects_grace_period_above_max() {
    let (env, _, seller, buyer, resolver, token, _, client) = setup();
    let now = env.ledger().timestamp();
    let expires_at = now + 1000;
    let grace_period = MAX_GRACE_PERIOD + 1;

    let result = client.try_create_escrow_with_expiration(
        &seller,
        &Some(buyer),
        &resolver,
        &token,
        &1_000_i128,
        &0_u32,
        &3600_u64,
        &Some(expires_at),
        &grace_period,
    );
    assert_eq!(result, Err(Ok(ContractError::GracePeriodTooLong)));
}

#[test]
fn test_create_escrow_with_expiration_accepts_max_grace_period() {
    let (env, _, seller, buyer, resolver, token, _, client) = setup();
    let now = env.ledger().timestamp();
    let expires_at = now + 1000;
    let grace_period = MAX_GRACE_PERIOD;

    let result = client.try_create_escrow_with_expiration(
        &seller,
        &Some(buyer),
        &resolver,
        &token,
        &1_000_i128,
        &0_u32,
        &3600_u64,
        &Some(expires_at),
        &grace_period,
    );
    assert!(result.is_ok());
}

// ===========================================================================
// Issue #997: batch_create_escrow duplicate notes check
// ===========================================================================

#[test]
fn test_batch_create_escrow_rejects_duplicate_notes() {
    let (env, _, seller, buyer, resolver, token, _, client) = setup();

    let mut escrows = Vec::new(&env);
    let input1 = EscrowInput {
        buyer: Some(buyer.clone()),
        resolver: resolver.clone(),
        token: token.clone(),
        amount: 1_000,
        fee_bps: 0,
        resolver_fee_bps: 0,
        shipping_window: 3600,
        notes: String::from_str(&env, "TRACKING-001"),
    };
    let input2 = EscrowInput {
        buyer: Some(buyer),
        resolver,
        token,
        amount: 2_000,
        fee_bps: 0,
        resolver_fee_bps: 0,
        shipping_window: 3600,
        notes: String::from_str(&env, "TRACKING-001"), // Duplicate!
    };
    escrows.push_back(input1);
    escrows.push_back(input2);

    let result = client.try_batch_create_escrow(&seller, &escrows);
    assert_eq!(result, Err(Ok(ContractError::DuplicateNotes)));
}

#[test]
fn test_batch_create_escrow_accepts_unique_or_empty_notes() {
    let (env, _, seller, buyer, resolver, token, _, client) = setup();

    let mut escrows = Vec::new(&env);
    let input1 = EscrowInput {
        buyer: Some(buyer.clone()),
        resolver: resolver.clone(),
        token: token.clone(),
        amount: 1_000,
        fee_bps: 0,
        resolver_fee_bps: 0,
        shipping_window: 3600,
        notes: String::from_str(&env, "TRACKING-001"),
    };
    let input2 = EscrowInput {
        buyer: Some(buyer.clone()),
        resolver: resolver.clone(),
        token: token.clone(),
        amount: 2_000,
        fee_bps: 0,
        resolver_fee_bps: 0,
        shipping_window: 3600,
        notes: String::from_str(&env, "TRACKING-002"),
    };
    let input3 = EscrowInput {
        buyer: Some(buyer),
        resolver,
        token,
        amount: 3_000,
        fee_bps: 0,
        resolver_fee_bps: 0,
        shipping_window: 3600,
        notes: String::from_str(&env, ""),
    };
    escrows.push_back(input1);
    escrows.push_back(input2);
    escrows.push_back(input3);

    let result = client.try_batch_create_escrow(&seller, &escrows);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().unwrap().len(), 3);
}

// ===========================================================================
// Issue #998: resolved_at >= disputed_at invariant
// ===========================================================================

#[test]
fn test_execute_resolution_transition_rejects_timestamp_before_disputed_at() {
    let (env, _, seller, buyer, resolver, token, _, client) = setup();
    let id = create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 1_000);

    // Raise dispute at timestamp 1_700_000_100
    env.ledger().set_timestamp(1_700_000_100);
    let reason = Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let desc = soroban_sdk::Bytes::from_slice(&env, b"item not received");
    let evidence = BytesN::from_array(&env, &[1u8; 32]);
    client.raise_dispute(&buyer, &id, &reason, &desc, &evidence);

    // Move timestamp backwards to simulate timestamp regression
    env.ledger().set_timestamp(1_700_000_050);

    let result = client.try_resolve_dispute(&resolver, &id, &ResolutionType::Refund);
    assert_eq!(result, Err(Ok(ContractError::TimestampInvariantViolated)));
}

// ===========================================================================
// Issue #1322: exhaustive 10x10 escrow state transition matrix
// ===========================================================================
//
// Every escrow state is exercised against every lifecycle action. A cell is
// either a valid transition (asserted to succeed and land in the documented
// target state) or an invalid transition (asserted to be rejected with an
// explicit state error while leaving the escrow untouched).
//
// Action       | Pending | Funded | Shipped | Completed | Disputed | RefundRequested | Refunded | Canceled | PendingFinalization | Expired
// -------------|---------|--------|---------|-----------|----------|-----------------|----------|----------|---------------------|--------
// fund         | Funded  |   -    |    -    |     -     |    -     |        -        |    -     |    -     |          -          |    -
// ship         |    -    | Shipped|    -    |     -     |    -     |     Shipped     |    -     |    -     |          -          |    -
// confirm      |    -    |   -    |Completed|     -     |    -     |        -        |    -     |    -     |          -          |    -
// auto_release |    -    |Completed|   -    |     -     |    -     |        -        |    -     |    -     |          -          |    -
// dispute      |    -    | Disputed|Disputed|     -     |    -     |        -        |    -     |    -     |          -          |    -
// resolve      |    -    |   -    |    -    |     -     |PendingFin.|       -        |    -     |    -     |          -          |    -
// appeal       |    -    |   -    |    -    |     -     |    -     |        -        |    -     |    -     |       Disputed      |    -
// cancel       | Canceled|   -    |    -    |     -     |    -     |        -        |    -     |    -     |          -          |    -
// reclaim      |    -    | Expired| Expired |     -     |    -     |        -        |    -     |    -     |          -          |    -
// finalize     |    -    |   -    |    -    |     -     |    -     |        -        |    -     |    -     |       Completed     |    -
//
// "-" means the transition is rejected. `auto_release` succeeds only from
// `Funded` under this fixture; from `Shipped` it requires a recorded delivery
// and is asserted rejected as `DeliveryNotRecorded`.

const MATRIX_T0: u64 = 1_700_000_000;
const MATRIX_EXPIRES_AT: u64 = MATRIX_T0 + 1_000_000;
const MATRIX_EXPIRY_NOW: u64 = MATRIX_EXPIRES_AT + 1;
const MATRIX_AUTO_RELEASE_NOW: u64 = MATRIX_T0 + 172_800 + 1;
const MATRIX_APPEAL_NOW: u64 = MATRIX_T0 + 1;
const MATRIX_FINALIZE_NOW: u64 = MATRIX_T0 + 86_400 + 1;

#[derive(Clone, Copy, Debug)]
enum LifecycleAction {
    Fund,
    Ship,
    Confirm,
    AutoRelease,
    Dispute,
    Resolve,
    Appeal,
    Cancel,
    Reclaim,
    Finalize,
}

const LIFECYCLE_ACTIONS: [LifecycleAction; 10] = [
    LifecycleAction::Fund,
    LifecycleAction::Ship,
    LifecycleAction::Confirm,
    LifecycleAction::AutoRelease,
    LifecycleAction::Dispute,
    LifecycleAction::Resolve,
    LifecycleAction::Appeal,
    LifecycleAction::Cancel,
    LifecycleAction::Reclaim,
    LifecycleAction::Finalize,
];

fn action_time(action: LifecycleAction) -> u64 {
    match action {
        LifecycleAction::AutoRelease => MATRIX_AUTO_RELEASE_NOW,
        LifecycleAction::Appeal => MATRIX_APPEAL_NOW,
        LifecycleAction::Finalize => MATRIX_FINALIZE_NOW,
        LifecycleAction::Reclaim => MATRIX_EXPIRY_NOW,
        _ => MATRIX_T0,
    }
}

fn expected_target(action: LifecycleAction, from: &EscrowState) -> Option<EscrowState> {
    let pending = from == &EscrowState::Pending;
    let funded = from == &EscrowState::Funded;
    let shipped = from == &EscrowState::Shipped;
    let disputed = from == &EscrowState::Disputed;
    let refund_requested = from == &EscrowState::RefundRequested;
    let pending_finalization = from == &EscrowState::PendingFinalization;

    match action {
        LifecycleAction::Fund if pending => Some(EscrowState::Funded),
        LifecycleAction::Ship if funded || refund_requested => Some(EscrowState::Shipped),
        LifecycleAction::Confirm if shipped => Some(EscrowState::Completed),
        LifecycleAction::AutoRelease if funded => Some(EscrowState::Completed),
        LifecycleAction::Dispute if funded || shipped => Some(EscrowState::Disputed),
        LifecycleAction::Resolve if disputed => Some(EscrowState::PendingFinalization),
        LifecycleAction::Appeal if pending_finalization => Some(EscrowState::Disputed),
        LifecycleAction::Cancel if pending => Some(EscrowState::Canceled),
        LifecycleAction::Reclaim if funded || shipped => Some(EscrowState::Expired),
        LifecycleAction::Finalize if pending_finalization => Some(EscrowState::Completed),
        _ => None,
    }
}

fn is_explicit_state_rejection(error: &ContractError) -> bool {
    matches!(
        error,
        ContractError::InvalidState
            | ContractError::InvalidStateTransition
            | ContractError::EscrowAlreadyCompleted
            | ContractError::EscrowAlreadyRefunded
            | ContractError::NotPendingFinalization
            | ContractError::EscrowExpired
            | ContractError::ShippingWindowNotElapsed
            | ContractError::DeliveryBeforeDisputeWindow
            | ContractError::DeliveryNotRecorded
            | ContractError::DisputeWindowStillOpen
            | ContractError::AppealWindowActive
    )
}

fn matrix_fund(env: &Env, client: &EscrowClient, token: &Address, buyer: &Address, id: u64) {
    token::StellarAssetClient::new(env, token).mint(buyer, &1_000_i128);
    client.fund_escrow(&id, buyer);
}

fn matrix_ship(env: &Env, client: &EscrowClient, seller: &Address, id: u64) {
    client.mark_shipped(seller, &id, &String::from_str(env, "TRK-MATRIX"));
}

fn matrix_dispute(env: &Env, client: &EscrowClient, buyer: &Address, id: u64) {
    client.raise_dispute(
        buyer,
        &id,
        &Symbol::new(env, "DEFECTIVE"),
        &Bytes::from_slice(env, b"state matrix"),
        &BytesN::from_array(env, &[0u8; 32]),
    );
}

#[allow(clippy::too_many_arguments)]
fn build_matrix_state(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    buyer: &Address,
    resolver: &Address,
    token: &Address,
    state: &EscrowState,
) -> u64 {
    let id = client.create_escrow_with_expiration(
        seller,
        &Some(buyer.clone()),
        resolver,
        token,
        &1_000_i128,
        &0_u32,
        &3_600_u64,
        &Some(MATRIX_EXPIRES_AT),
        &0_u64,
    );

    match state {
        EscrowState::Pending => {}
        EscrowState::Funded => matrix_fund(env, client, token, buyer, id),
        EscrowState::Shipped => {
            matrix_fund(env, client, token, buyer, id);
            matrix_ship(env, client, seller, id);
        }
        EscrowState::RefundRequested => {
            matrix_fund(env, client, token, buyer, id);
            client.request_refund(buyer, &id);
        }
        EscrowState::Disputed => {
            matrix_fund(env, client, token, buyer, id);
            matrix_ship(env, client, seller, id);
            matrix_dispute(env, client, buyer, id);
        }
        EscrowState::PendingFinalization => {
            matrix_fund(env, client, token, buyer, id);
            matrix_ship(env, client, seller, id);
            matrix_dispute(env, client, buyer, id);
            client.resolve_dispute(resolver, &id, &ResolutionType::Release);
        }
        EscrowState::Completed => {
            matrix_fund(env, client, token, buyer, id);
            matrix_ship(env, client, seller, id);
            client.confirm_delivery(buyer, &id, &true);
        }
        EscrowState::Refunded => {
            matrix_fund(env, client, token, buyer, id);
            client.cancel_escrow(buyer, &id);
        }
        EscrowState::Canceled => {
            client.cancel_escrow(seller, &id);
        }
        EscrowState::Expired => {
            matrix_fund(env, client, token, buyer, id);
            env.ledger().set_timestamp(MATRIX_EXPIRY_NOW);
            client.reclaim_expired(&id);
        }
    }

    id
}

fn attempt_action(
    env: &Env,
    client: &EscrowClient,
    action: LifecycleAction,
    seller: &Address,
    buyer: &Address,
    resolver: &Address,
    token: &Address,
    id: u64,
) -> Option<ContractError> {
    let outcome = match action {
        LifecycleAction::Fund => {
            token::StellarAssetClient::new(env, token).mint(buyer, &1_000_i128);
            client.try_fund_escrow(&id, buyer)
        }
        LifecycleAction::Ship => {
            client.try_mark_shipped(seller, &id, &String::from_str(env, "TRK-MATRIX"))
        }
        LifecycleAction::Confirm => client.try_confirm_delivery(buyer, &id, &true),
        LifecycleAction::AutoRelease => client.try_auto_release(&id),
        LifecycleAction::Dispute => client.try_raise_dispute(
            buyer,
            &id,
            &Symbol::new(env, "DEFECTIVE"),
            &Bytes::from_slice(env, b"state matrix"),
            &BytesN::from_array(env, &[0u8; 32]),
        ),
        LifecycleAction::Resolve => {
            client.try_resolve_dispute(resolver, &id, &ResolutionType::Release)
        }
        LifecycleAction::Appeal => client.try_appeal_dispute(buyer, &id),
        LifecycleAction::Cancel => client.try_cancel_escrow(seller, &id),
        LifecycleAction::Reclaim => client.try_reclaim_expired(&id),
        LifecycleAction::Finalize => client.try_finalize_dispute(resolver, &id),
    };

    match outcome {
        Ok(_) => None,
        Err(Ok(error)) => Some(error),
        Err(Err(_)) => panic!("{action:?} trapped the host instead of returning a contract error"),
    }
}

fn run_state_matrix(from: EscrowState) {
    let (env, _admin, seller, buyer, resolver, token, _fee_collector, client) = setup();

    for action in LIFECYCLE_ACTIONS {
        env.ledger().set_timestamp(MATRIX_T0);
        let id = build_matrix_state(&env, &client, &seller, &buyer, &resolver, &token, &from);
        env.ledger().set_timestamp(action_time(action));

        let error = attempt_action(
            &env, &client, action, &seller, &buyer, &resolver, &token, id,
        );
        let expected = expected_target(action, &from);

        match expected {
            Some(target) => {
                assert!(
                    error.is_none(),
                    "{action:?} from {from:?} should succeed but was rejected with {error:?}"
                );
                assert_eq!(
                    client.get_escrow(&id).state,
                    target,
                    "{action:?} from {from:?} landed in the wrong state"
                );
            }
            None => {
                let rejection = error.unwrap_or_else(|| {
                    panic!("{action:?} from {from:?} should be rejected but succeeded")
                });
                assert!(
                    is_explicit_state_rejection(&rejection),
                    "{action:?} from {from:?} returned a non-state error: {rejection:?}"
                );
                assert_eq!(
                    client.get_escrow(&id).state,
                    from,
                    "{action:?} from {from:?} mutated state while being rejected"
                );
            }
        }
    }
}

macro_rules! state_matrix_test {
    ($name:ident, $state:expr) => {
        #[test]
        fn $name() {
            run_state_matrix($state);
        }
    };
}

state_matrix_test!(matrix_from_pending, EscrowState::Pending);
state_matrix_test!(matrix_from_funded, EscrowState::Funded);
state_matrix_test!(matrix_from_shipped, EscrowState::Shipped);
state_matrix_test!(matrix_from_completed, EscrowState::Completed);
state_matrix_test!(matrix_from_disputed, EscrowState::Disputed);
state_matrix_test!(matrix_from_refund_requested, EscrowState::RefundRequested);
state_matrix_test!(matrix_from_refunded, EscrowState::Refunded);
state_matrix_test!(matrix_from_canceled, EscrowState::Canceled);
state_matrix_test!(
    matrix_from_pending_finalization,
    EscrowState::PendingFinalization
);
state_matrix_test!(matrix_from_expired, EscrowState::Expired);
