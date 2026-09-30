#![cfg(test)]

use crate::{
    ContractError, DisputeStatus, Escrow, EscrowClient, EscrowInput, EscrowState, Payee,
    ResolutionType, MAX_GRACE_PERIOD,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
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
    let desc = String::from_str(&env, "item not received");
    let evidence = BytesN::from_array(&env, &[0u8; 32]);
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
    let desc = String::from_str(&env, "item not received");
    let evidence = BytesN::from_array(&env, &[0u8; 32]);
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
    let desc = String::from_str(&env, "item not received");
    let evidence = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(&buyer, &id, &reason, &desc, &evidence);

    let result = client.try_cancel_pending_finalization(&admin, &id);
    assert_eq!(result, Err(Ok(ContractError::NotPendingFinalization)));
}

#[test]
fn test_admin_cancel_finalization_alias_works() {
    let (env, admin, seller, buyer, resolver, token, _, client) = setup();
    let id = create_and_fund(&env, &client, &seller, &buyer, &resolver, &token, 1_000);

    let reason = Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let desc = String::from_str(&env, "item not received");
    let evidence = BytesN::from_array(&env, &[0u8; 32]);
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
        notes: Some(String::from_str(&env, "TRACKING-001")),
    };
    let input2 = EscrowInput {
        buyer: Some(buyer),
        resolver,
        token,
        amount: 2_000,
        fee_bps: 0,
        resolver_fee_bps: 0,
        shipping_window: 3600,
        notes: Some(String::from_str(&env, "TRACKING-001")), // Duplicate!
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
        notes: Some(String::from_str(&env, "TRACKING-001")),
    };
    let input2 = EscrowInput {
        buyer: Some(buyer.clone()),
        resolver: resolver.clone(),
        token: token.clone(),
        amount: 2_000,
        fee_bps: 0,
        resolver_fee_bps: 0,
        shipping_window: 3600,
        notes: Some(String::from_str(&env, "TRACKING-002")),
    };
    let input3 = EscrowInput {
        buyer: Some(buyer),
        resolver,
        token,
        amount: 3_000,
        fee_bps: 0,
        resolver_fee_bps: 0,
        shipping_window: 3600,
        notes: None,
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
    let desc = String::from_str(&env, "item not received");
    let evidence = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(&buyer, &id, &reason, &desc, &evidence);

    // Move timestamp backwards to simulate timestamp regression
    env.ledger().set_timestamp(1_700_000_050);

    let result = client.try_resolve_dispute(&resolver, &id, &ResolutionType::Refund);
    assert_eq!(result, Err(Ok(ContractError::TimestampInvariantViolated)));
}
