#![cfg(test)]
use crate::{
    ContractError, Escrow, EscrowClient, EscrowData, EscrowState, Payee, ResolutionType,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

fn setup(env: &Env) -> (EscrowClient, Address, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let primary = Address::generate(env);
    let backup = Address::generate(env);

    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    token::StellarAssetClient::new(env, &token).mint(&buyer, &10_000_i128);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    (client, seller, buyer, primary, backup, token)
}

fn create_funded_shipped_disputed(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    buyer: &Address,
    primary: &Address,
    backup: &Address,
    token: &Address,
) -> u64 {
    let dispute_deadline = env.ledger().timestamp() + 86400;
    let escrow_id = client.create_escrow_with_fallback(
        seller,
        &Some(buyer.clone()),
        primary,
        backup,
        &dispute_deadline,
        token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&escrow_id, buyer);

    env.ledger().set_timestamp(env.ledger().timestamp() + 3601);
    let tracking_id = String::from_str(env, "TRK-FB-001");
    client.mark_shipped(seller, &escrow_id, &tracking_id);

    let reason = Symbol::new(env, "defective");
    let description = String::from_str(env, "Item is defective");
    let evidence = BytesN::from_array(env, &[0xcd; 32]);
    client.raise_dispute(buyer, &escrow_id, &reason, &description, &evidence);

    escrow_id
}

#[test]
fn create_escrow_with_fallback_succeeds() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let dispute_deadline = env.ledger().timestamp() + 86400;
    let escrow_id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Pending);
    assert_eq!(
        escrow
            .payees
            .get(0)
            .unwrap()
            .address,
        seller
    );
}

#[test]
fn primary_resolver_can_resolve_dispute() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let escrow_id = create_funded_shipped_disputed(
        &env, &client, &seller, &buyer, &primary, &backup, &token,
    );

    client.resolve_dispute(&primary, &escrow_id, &ResolutionType::Release);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);

    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&primary, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);
}

#[test]
fn backup_resolver_can_resolve_dispute() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let escrow_id = create_funded_shipped_disputed(
        &env, &client, &seller, &buyer, &primary, &backup, &token,
    );

    client.resolve_dispute(&backup, &escrow_id, &ResolutionType::Refund);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);

    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&backup, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Refunded);
}

#[test]
fn non_resolver_cannot_resolve_dispute() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);
    let unauthorized = Address::generate(&env);

    let escrow_id = create_funded_shipped_disputed(
        &env, &client, &seller, &buyer, &primary, &backup, &token,
    );

    let result = client.try_resolve_dispute(&unauthorized, &escrow_id, &ResolutionType::Release);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn resolve_non_disputed_escrow_fails() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let dispute_deadline = env.ledger().timestamp() + 86400;
    let escrow_id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );
    client.fund_escrow(&escrow_id, &buyer);

    let result = client.try_resolve_dispute(&primary, &escrow_id, &ResolutionType::Release);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}

#[test]
fn duplicate_resolution_fails() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let escrow_id = create_funded_shipped_disputed(
        &env, &client, &seller, &buyer, &primary, &backup, &token,
    );

    client.resolve_dispute(&primary, &escrow_id, &ResolutionType::Release);

    let result = client.try_resolve_dispute(&primary, &escrow_id, &ResolutionType::Refund);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}

#[test]
fn fallback_dispute_deadline_is_stored() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let dispute_deadline = env.ledger().timestamp() + 86400;
    let escrow_id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Pending);

    client.fund_escrow(&escrow_id, &buyer);
    let funded = client.get_escrow(&escrow_id);
    assert_eq!(funded.state, EscrowState::Funded);
    assert!(funded.dispute_deadline > 0);
}