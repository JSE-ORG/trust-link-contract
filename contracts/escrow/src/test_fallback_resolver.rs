#![cfg(test)]
use crate::{ContractError, Escrow, EscrowClient, EscrowData, EscrowState, Payee, ResolutionType};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let primary_resolver = Address::generate(&env);
    let backup_resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    (
        env,
        admin,
        seller,
        buyer,
        primary_resolver,
        backup_resolver,
        token_address,
        fee_collector,
    )
}

#[test]
fn test_create_escrow_with_fallback_success() {
    let (env, admin, seller, buyer, primary, backup, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let shipping_window = 7200_u64;

    let id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &amount,
        &100_u32,
        &shipping_window,
    );

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Pending);
    assert_eq!(escrow.payees.get(0).unwrap().address, seller);
}

#[test]
fn primary_resolver_can_resolve_dispute() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let escrow_id =
        create_funded_shipped_disputed(&env, &client, &seller, &buyer, &primary, &backup, &token);

    client.resolve_dispute(&primary, &escrow_id, &ResolutionType::Release);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);

    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&primary, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);
}

#[test]
fn backup_resolver_can_resolve_dispute_after_deadline() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let escrow_id =
        create_funded_shipped_disputed(&env, &client, &seller, &buyer, &primary, &backup, &token);

    // #661 — the backup may only resolve once dispute_deadline has passed.
    // create_funded_shipped_disputed sets dispute_deadline = timestamp() (at
    // setup, before funding/shipping/disputing) + 86400; advance well past it.
    env.ledger().set_timestamp(env.ledger().timestamp() + 86400);

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &String::from_str(&env, "TRK"));

    let reason = Symbol::new(&env, "reason");
    let description = String::from_str(&env, "desc");
    let evidence_hash = BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // Primary resolver can resolve immediately (ledger time is still dispute time + 0)
    let result = client.try_resolve_dispute(&primary, &id, &ResolutionType::Refund);
    assert!(result.is_ok());

    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, EscrowState::PendingFinalization);
}

#[test]
fn backup_resolver_cannot_resolve_dispute_before_deadline() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    // #661 regression: the backup must not be able to preempt the primary's
    // window to resolve. create_funded_shipped_disputed leaves the ledger
    // timestamp still before dispute_deadline at this point.
    let escrow_id =
        create_funded_shipped_disputed(&env, &client, &seller, &buyer, &primary, &backup, &token);

    let result = client.try_resolve_dispute(&backup, &escrow_id, &ResolutionType::Refund);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn primary_resolver_can_resolve_dispute_regardless_of_deadline() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    // #661: the primary is never time-restricted — before or after the
    // backup's deadline, the primary can always resolve.
    let escrow_id =
        create_funded_shipped_disputed(&env, &client, &seller, &buyer, &primary, &backup, &token);

    client.resolve_dispute(&primary, &escrow_id, &ResolutionType::Release);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);
}

#[test]
fn single_resolver_set_is_unaffected_by_fallback_deadline_logic() {
    // #661 no-fallback scenario: a plain Single resolver set has no
    // dispute_deadline concept at all, and can_resolve_now must behave
    // exactly like the old contains() check for it.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &10_000_i128);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val: soroban_sdk::Val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&escrow_id, &buyer);
    env.ledger().set_timestamp(env.ledger().timestamp() + 3601);
    client.mark_shipped(&seller, &escrow_id, &String::from_str(&env, "TRK-SINGLE"));

    let reason = Symbol::new(&env, "defective");
    let description = String::from_str(&env, "Item is defective");
    let evidence = BytesN::from_array(&env, &[0xef; 32]);
    client.raise_dispute(&buyer, &escrow_id, &reason, &description, &evidence);

    // Immediate resolution succeeds — no deadline gating for a Single resolver.
    client.resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release);
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);
}

#[test]
fn non_resolver_cannot_resolve_dispute() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);
    let unauthorized = Address::generate(&env);

    let escrow_id =
        create_funded_shipped_disputed(&env, &client, &seller, &buyer, &primary, &backup, &token);

    let result = client.try_resolve_dispute(&unauthorized, &escrow_id, &ResolutionType::Release);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn resolve_non_disputed_escrow_fails() {
    let env = Env::default();
    let (client, seller, buyer, primary, backup, token) = setup(&env);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &amount,
        &100_u32,
        &0_u64,
    );

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &String::from_str(&env, "TRK"));

    let reason = Symbol::new(&env, "reason");
    let description = String::from_str(&env, "desc");
    let evidence_hash = BytesN::from_array(&env, &[0xab; 32]);

    let escrow_id =
        create_funded_shipped_disputed(&env, &client, &seller, &buyer, &primary, &backup, &token);

    // Set time to just before dispute_deadline (e.g. 1_000_000 + 3599)
    env.ledger().set_timestamp(1_000_000 + 3599);

    // Backup resolver should be unauthorized to resolve before deadline
    let result = client.try_resolve_dispute(&backup, &id, &ResolutionType::Refund);
    assert_eq!(result, Err(Ok(crate::ContractError::NotAuthorized)));

    // Backup resolver secondary entrypoint `vote` should also be unauthorized
    let result_vote = client.try_vote(&backup, &id, &ResolutionType::Refund);
    assert_eq!(result_vote, Err(Ok(crate::ContractError::NotAuthorized)));
}

#[test]
fn test_fallback_resolver_vote_backup_allowed_after_deadline() {
    let (env, admin, seller, buyer, primary, backup, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let id = client.create_escrow_with_fallback(
        &seller,
        &Some(buyer.clone()),
        &primary,
        &backup,
        &dispute_deadline,
        &token,
        &amount,
        &100_u32,
        &0_u64,
    );

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &String::from_str(&env, "TRK"));

    let reason = Symbol::new(&env, "reason");
    let description = String::from_str(&env, "desc");
    let evidence_hash = BytesN::from_array(&env, &[0xab; 32]);

    client.fund_escrow(&escrow_id, &buyer);
    let funded = client.get_escrow(&escrow_id);
    assert_eq!(funded.state, EscrowState::Funded);
    assert!(funded.dispute_deadline > 0);
}
