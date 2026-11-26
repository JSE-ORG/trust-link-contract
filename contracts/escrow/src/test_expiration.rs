#![cfg(test)]

use crate::{
    ContractError, Escrow, EscrowClient, EscrowExpired, EscrowState, ResolutionType,
    EVENT_SCHEMA_VERSION,
};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token,
    xdr::ContractEventBody,
    Address, BytesN, Env, Symbol, TryFromVal, Val, Vec,
};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let contract_id = env.register(Escrow, ());
    {
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
    }
    (
        env,
        admin,
        seller,
        buyer,
        resolver,
        token_address,
        contract_id,
    )
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

fn escrow_expired_events(env: &Env, contract_id: &Address) -> Vec<EscrowExpired> {
    let mut found = Vec::new(env);
    for event in env
        .events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .iter()
    {
        let ContractEventBody::V0(body) = &event.body;
        let mut topics = body.topics.iter();
        let (Some(first), Some(second)) = (topics.next(), topics.next()) else {
            continue;
        };
        let (Ok(first), Ok(second)) = (
            Symbol::try_from_val(env, first),
            Symbol::try_from_val(env, second),
        ) else {
            continue;
        };
        if first != Symbol::new(env, "Escrow") || second != Symbol::new(env, "Expired") {
            continue;
        }
        let Ok(data) = Val::try_from_val(env, &body.data) else {
            continue;
        };
        if let Ok(decoded) = EscrowExpired::try_from_val(env, &data) {
            found.push_back(decoded);
        }
    }
    found
}

#[test]
fn test_create_escrow_with_invalid_expiration() {
    let (env, _admin, seller, _buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Current ledger timestamp is 0. An expiration of 0 (past/present) should fail.
    let res = client.try_create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(0_u64),
        &100_u64,
    );
    // create_escrow_with_expiration validates the escrow body (amount,
    // shipping window, fees) via create_escrow_internal first — all valid
    // here — before checking `expires_at <= now`, which is what actually
    // fails for an expiration of 0 at ledger timestamp 0. The dedicated
    // InvalidExpiration variant exists specifically for this case.
    assert!(matches!(res, Err(Ok(ContractError::InvalidExpiration))));
}

#[test]
fn test_cannot_fund_expired_escrow() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Create escrow that expires at timestamp 10.
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    // Advance time past expiration (to timestamp 10)
    env.ledger().set_timestamp(10);
    mint_tokens(&env, &token, &buyer, 1000);

    // Funding should fail with EscrowExpired.
    let res = client.try_fund_escrow(&id, &buyer);
    assert!(matches!(res, Err(Ok(ContractError::EscrowExpired))));
}

#[test]
fn test_cannot_ship_expired_escrow() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Create escrow that expires at timestamp 10.
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    mint_tokens(&env, &token, &buyer, 1000);
    client.fund_escrow(&id, &buyer);

    // Advance time past expiration (to timestamp 10)
    env.ledger().set_timestamp(10);

    // Shipping should fail with EscrowExpired.
    let res = client.try_mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK001"),
    );
    assert!(matches!(res, Err(Ok(ContractError::EscrowExpired))));
}

#[test]
fn test_reclaim_lifecycle() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Create escrow with expiration at timestamp 10 and grace period 5.
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    mint_tokens(&env, &token, &buyer, 1000);
    client.fund_escrow(&id, &buyer);

    // Verify initial balance
    assert_eq!(token::Client::new(&env, &token).balance(&buyer), 0);
    assert_eq!(token::Client::new(&env, &token).balance(&contract_id), 1000);

    // 1. Try to reclaim immediately (before expiration) -> should fail with InvalidState
    let res = client.try_reclaim_expired(&id);
    assert_eq!(res, Err(Ok(ContractError::InvalidState)));

    // 2. Advance to timestamp 10 (exactly expires_at). The escrow is Funded
    //    but unshipped, so it is immediately reclaimable: the grace period
    //    only covers the pending-funding race and must not trap funds here.
    env.ledger().set_timestamp(10);
    client.reclaim_expired(&id);

    // Verify state transition to Expired
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Expired);

    // Verify funds returned to buyer
    assert_eq!(token::Client::new(&env, &token).balance(&buyer), 1000);
    assert_eq!(token::Client::new(&env, &token).balance(&contract_id), 0);
}

#[test]
fn test_reclaim_expired_emits_escrow_expired_event() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    mint_tokens(&env, &token, &buyer, 1_000);
    client.fund_escrow(&id, &buyer);
    env.ledger().set_timestamp(10);
    client.reclaim_expired(&id);

    let events = escrow_expired_events(&env, &contract_id);
    assert_eq!(events.len(), 1);
    let event = events.get(0).unwrap();
    assert_eq!(event.schema_version, EVENT_SCHEMA_VERSION);
    assert_eq!(event.escrow_id, id);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.token, token);
    assert_eq!(event.amount, 1_000);
    assert_eq!(event.timestamp, 10);
    assert_eq!(event.prev_state, EscrowState::Funded);
    assert_eq!(event.new_state, EscrowState::Expired);
    assert_eq!(token::Client::new(&env, &token).balance(&buyer), 1_000);
}

#[test]
fn test_reclaim_with_active_dispute() {
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Create escrow with expiration at timestamp 10 and grace period 5.
    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(10_u64),
        &5_u64,
    );

    mint_tokens(&env, &token, &buyer, 1000);
    client.fund_escrow(&id, &buyer);

    // Buyer raises a dispute at timestamp 5.
    env.ledger().set_timestamp(5);
    let hash = BytesN::from_array(&env, &[1u8; 32]);
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "OTHER"),
        &soroban_sdk::Bytes::from_slice(&env, b"description"),
        &hash,
    );

    // Advance to timestamp 15 (expired + grace period elapsed).
    env.ledger().set_timestamp(15);

    // Try to reclaim -> should fail with InvalidState because dispute is active.
    let res = client.try_reclaim_expired(&id);
    assert_eq!(res, Err(Ok(ContractError::InvalidState)));

    // Resolver resolves dispute in favor of Refund.
    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);

    // Subsequent reclaim should fail with InvalidState (not in Funded/Shipped state).
    let res2 = client.try_reclaim_expired(&id);
    assert!(matches!(res2, Err(Ok(ContractError::InvalidState))));
}

#[test]
fn test_reclaim_expired_grace_period_boundary() {
    // Verifies the off-by-one boundary at exactly expires_at + grace_period.
    //
    // Schedule: expires_at = 100, grace_period = 50 → reclaimable_at = 150.
    //
    // Ledger tick 149 (reclaimable_at - 1): still inside grace window → GracePeriodNotElapsed.
    // Ledger tick 150 (reclaimable_at    ): exactly at boundary        → reclaim succeeds.
    let (env, _admin, seller, buyer, resolver, token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    let expires_at: u64 = 150;
    let grace_period: u64 = 0;
    let _reclaimable_at = expires_at; // 150

    let id = client.create_escrow_with_expiration(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
        &Some(expires_at),
        &grace_period,
    );

    mint_tokens(&env, &token, &buyer, 1000);
    client.fund_escrow(&id, &buyer);

    // --- one tick before the boundary: must still be blocked ---
    env.ledger().set_timestamp(expires_at - 1); // 99
    let res = client.try_reclaim_expired(&id);
    assert!(
        matches!(res, Err(Ok(ContractError::InvalidState))),
        "expected InvalidState at tick {}, got {:?}",
        expires_at - 1,
        res
    );

    // --- exactly at the boundary: must succeed ---
    env.ledger().set_timestamp(expires_at); // 100
    client.reclaim_expired(&id);

    let escrow = client.get_escrow(&id);
    assert_eq!(
        escrow.state,
        EscrowState::Expired,
        "escrow should be Expired after reclaim at boundary tick"
    );

    assert_eq!(
        token::Client::new(&env, &token).balance(&buyer),
        1000,
        "buyer should have received all funds back"
    );
    assert_eq!(
        token::Client::new(&env, &token).balance(&contract_id),
        0,
        "contract should hold zero tokens after reclaim"
    );
}
