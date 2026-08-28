#![cfg(test)]
//! Fallback (primary/backup) resolver dispute tests (#661).

use crate::{ContractError, Escrow, EscrowClient, EscrowState, Payee, ResolutionType};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

struct Setup {
    env: Env,
    contract_id: Address,
    seller: Address,
    buyer: Address,
    primary: Address,
    backup: Address,
    token: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let primary = Address::generate(&env);
    let backup = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    Setup {
        env,
        contract_id,
        seller,
        buyer,
        primary,
        backup,
        token,
    }
}

/// Creates a fallback-resolver escrow, funds it, marks it shipped, and
/// raises a dispute, leaving the ledger timestamp unchanged (i.e. still
/// before the fallback's `dispute_deadline`, which is set to `now + 100`).
/// Returns the escrow id.
fn create_funded_shipped_disputed(setup: &Setup) -> u64 {
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let amount = 1000_i128;
    let now = setup.env.ledger().timestamp();
    let fallback_deadline = now + 100;

    let escrow_id = client.create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.primary,
        &setup.backup,
        &fallback_deadline,
        &setup.token,
        &amount,
        &0_u32,
        &3600_u64,
    );

    let sac = token::StellarAssetClient::new(&setup.env, &setup.token);
    sac.mint(&setup.buyer, &amount);
    client.fund_escrow(&escrow_id, &setup.buyer);
    client.mark_shipped(
        &setup.seller,
        &escrow_id,
        &String::from_str(&setup.env, "TRK"),
    );

    let reason = Symbol::new(&setup.env, "reason");
    let description = String::from_str(&setup.env, "desc");
    let evidence_hash = BytesN::from_array(&setup.env, &[0xab; 32]);
    client.raise_dispute(
        &setup.buyer,
        &escrow_id,
        &reason,
        &description,
        &evidence_hash,
    );

    escrow_id
}

#[test]
fn test_create_escrow_with_fallback_success() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let shipping_window = 7200_u64;

    let id = client.create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.primary,
        &setup.backup,
        &dispute_deadline,
        &setup.token,
        &amount,
        &100_u32,
        &shipping_window,
    );

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Pending);
    assert_eq!(escrow.payees.get(0).unwrap().address, setup.seller);
}

#[test]
fn primary_resolver_can_resolve_dispute() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);

    client.resolve_dispute(&setup.primary, &escrow_id, &ResolutionType::Release);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);

    let t = setup.env.ledger().timestamp();
    setup.env.ledger().set_timestamp(t + 86401);
    client.finalize_dispute(&setup.primary, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);
}

#[test]
fn backup_resolver_can_resolve_dispute_after_deadline() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);

    // #661 — the backup may only resolve once the fallback's dispute_deadline
    // has passed. `create_funded_shipped_disputed` sets that deadline to
    // `now + 100`; advance well past it.
    let t = setup.env.ledger().timestamp();
    setup.env.ledger().set_timestamp(t + 200);

    let result = client.try_resolve_dispute(&setup.backup, &escrow_id, &ResolutionType::Refund);
    assert!(result.is_ok());

    let escrow_after = client.get_escrow(&escrow_id);
    assert_eq!(escrow_after.state, EscrowState::PendingFinalization);
}

#[test]
fn backup_resolver_authorized_exactly_at_deadline() {
    // #661 boundary: `can_resolve_now` gates the backup with
    // `now >= dispute_deadline`, so the backup is authorized *at* the deadline
    // instant, not one second later. `create_funded_shipped_disputed` sets the
    // deadline to `now + 100` and the default test ledger starts at t = 0.
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);

    setup.env.ledger().set_timestamp(100);

    let result = client.try_resolve_dispute(&setup.backup, &escrow_id, &ResolutionType::Refund);
    assert!(result.is_ok());
    assert_eq!(
        client.get_escrow(&escrow_id).state,
        EscrowState::PendingFinalization,
    );
}

#[test]
fn backup_resolver_cannot_resolve_dispute_before_deadline() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    // #661 regression: the backup must not be able to preempt the primary's
    // window to resolve. `create_funded_shipped_disputed` leaves the ledger
    // timestamp still before the fallback's dispute_deadline at this point.
    let escrow_id = create_funded_shipped_disputed(&setup);

    let result = client.try_resolve_dispute(&setup.backup, &escrow_id, &ResolutionType::Refund);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn primary_resolver_can_resolve_dispute_regardless_of_deadline() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    // #661: the primary is never time-restricted — before or after the
    // backup's deadline, the primary can always resolve.
    let escrow_id = create_funded_shipped_disputed(&setup);

    client.resolve_dispute(&setup.primary, &escrow_id, &ResolutionType::Release);

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
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let unauthorized = Address::generate(&setup.env);

    let escrow_id = create_funded_shipped_disputed(&setup);

    let result = client.try_resolve_dispute(&unauthorized, &escrow_id, &ResolutionType::Release);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn resolve_non_disputed_escrow_fails() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let amount = 1000_i128;
    let dispute_deadline = 3600_u64;
    let escrow_id = client.create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.primary,
        &setup.backup,
        &dispute_deadline,
        &setup.token,
        &amount,
        &100_u32,
        &0_u64,
    );

    let sac = token::StellarAssetClient::new(&setup.env, &setup.token);
    sac.mint(&setup.buyer, &amount);
    client.fund_escrow(&escrow_id, &setup.buyer);
    client.mark_shipped(
        &setup.seller,
        &escrow_id,
        &String::from_str(&setup.env, "TRK"),
    );

    // No dispute has been raised — the escrow is still `Shipped`, so any
    // attempt to resolve it must fail with `InvalidState` regardless of who
    // calls it.
    let result = client.try_resolve_dispute(&setup.primary, &escrow_id, &ResolutionType::Refund);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));

    let result_vote = client.try_vote(&setup.backup, &escrow_id, &ResolutionType::Refund);
    assert_eq!(result_vote, Err(Ok(ContractError::InvalidState)));
}

#[test]
fn backup_resolver_vote_allowed_after_deadline() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);

    let t = setup.env.ledger().timestamp();
    setup.env.ledger().set_timestamp(t + 200);

    client.vote(&setup.backup, &escrow_id, &ResolutionType::Release);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);
}

#[test]
fn test_fallback_creation_role_conflicts() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let amount = 1000_i128;
    let deadline = 100_000_u64;

    // 1. Seller as primary resolver
    let res = client.try_create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.seller,
        &setup.backup,
        &deadline,
        &setup.token,
        &amount,
        &0_u32,
        &3600_u64,
    );
    assert_eq!(res, Err(Ok(ContractError::ConflictingRoles)));

    // 2. Seller as backup resolver
    let res = client.try_create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.primary,
        &setup.seller,
        &deadline,
        &setup.token,
        &amount,
        &0_u32,
        &3600_u64,
    );
    assert_eq!(res, Err(Ok(ContractError::ConflictingRoles)));

    // 3. Buyer as primary resolver
    let res = client.try_create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.buyer,
        &setup.backup,
        &deadline,
        &setup.token,
        &amount,
        &0_u32,
        &3600_u64,
    );
    assert_eq!(res, Err(Ok(ContractError::ConflictingRoles)));

    // 4. Buyer as backup resolver
    let res = client.try_create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.primary,
        &setup.buyer,
        &deadline,
        &setup.token,
        &amount,
        &0_u32,
        &3600_u64,
    );
    assert_eq!(res, Err(Ok(ContractError::ConflictingRoles)));

    // 5. Primary equals backup
    let res = client.try_create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.primary,
        &setup.primary,
        &deadline,
        &setup.token,
        &amount,
        &0_u32,
        &3600_u64,
    );
    assert_eq!(res, Err(Ok(ContractError::ConflictingRoles)));
}

#[test]
fn test_fallback_creation_amount_and_fee_boundaries() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let deadline = 100_000_u64;

    // Zero amount
    assert_eq!(
        client.try_create_escrow_with_fallback(
            &setup.seller,
            &Some(setup.buyer.clone()),
            &setup.primary,
            &setup.backup,
            &deadline,
            &setup.token,
            &0_i128,
            &0_u32,
            &3600_u64,
        ),
        Err(Ok(ContractError::InvalidAmount))
    );

    // Negative amount
    assert_eq!(
        client.try_create_escrow_with_fallback(
            &setup.seller,
            &Some(setup.buyer.clone()),
            &setup.primary,
            &setup.backup,
            &deadline,
            &setup.token,
            &-100_i128,
            &0_u32,
            &3600_u64,
        ),
        Err(Ok(ContractError::InvalidAmount))
    );

    // Exceeds max fee cap (300 bps)
    assert_eq!(
        client.try_create_escrow_with_fallback(
            &setup.seller,
            &Some(setup.buyer.clone()),
            &setup.primary,
            &setup.backup,
            &deadline,
            &setup.token,
            &1000_i128,
            &301_u32,
            &3600_u64,
        ),
        Err(Ok(ContractError::FeeExceedsMax))
    );
}

#[test]
fn test_fallback_primary_resolves_refund_and_finalizes() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);

    client.resolve_dispute(&setup.primary, &escrow_id, &ResolutionType::Refund);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);

    let t = setup.env.ledger().timestamp();
    setup.env.ledger().set_timestamp(t + 86401);
    client.finalize_dispute(&setup.buyer, &escrow_id);

    let escrow_after = client.get_escrow(&escrow_id);
    assert_eq!(escrow_after.state, EscrowState::Refunded);

    let tc = token::Client::new(&setup.env, &setup.token);
    assert_eq!(tc.balance(&setup.buyer), 1000);
}

#[test]
fn test_fallback_backup_resolves_release_and_finalizes() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);

    // Advance past fallback dispute_deadline (now + 100)
    let t = setup.env.ledger().timestamp();
    setup.env.ledger().set_timestamp(t + 101);

    client.resolve_dispute(&setup.backup, &escrow_id, &ResolutionType::Release);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);

    setup.env.ledger().set_timestamp(t + 101 + 86401);
    client.finalize_dispute(&setup.seller, &escrow_id);

    let escrow_after = client.get_escrow(&escrow_id);
    assert_eq!(escrow_after.state, EscrowState::Completed);

    let tc = token::Client::new(&setup.env, &setup.token);
    assert_eq!(tc.balance(&setup.seller), 1000);
}

#[test]
fn test_fallback_backup_exact_deadline_boundary() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);
    let now = setup.env.ledger().timestamp();
    // In create_funded_shipped_disputed, fallback deadline is now + 100

    // 1 second before deadline:
    setup.env.ledger().set_timestamp(now + 99);
    assert_eq!(
        client.try_resolve_dispute(&setup.backup, &escrow_id, &ResolutionType::Release),
        Err(Ok(ContractError::NotAuthorized))
    );

    // Exactly at deadline:
    setup.env.ledger().set_timestamp(now + 100);
    assert!(client
        .try_resolve_dispute(&setup.backup, &escrow_id, &ResolutionType::Release)
        .is_ok());

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::PendingFinalization);
}

#[test]
fn test_fallback_get_resolver_votes_query() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);

    client.resolve_dispute(&setup.primary, &escrow_id, &ResolutionType::Release);

    let votes = client.get_resolver_votes(&escrow_id);
    assert_eq!(votes.len(), 1);
    let vote = votes.get(0).unwrap();
    assert_eq!(vote.resolver, setup.primary);
    assert_eq!(vote.resolution, ResolutionType::Release);
}

#[test]
fn test_fallback_appeal_flow_reopens_dispute() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);

    let escrow_id = create_funded_shipped_disputed(&setup);

    // Primary resolves Release
    client.resolve_dispute(&setup.primary, &escrow_id, &ResolutionType::Release);
    assert_eq!(
        client.get_escrow(&escrow_id).state,
        EscrowState::PendingFinalization
    );

    // Buyer appeals before appeal window ends
    client.appeal_dispute(&setup.buyer, &escrow_id);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Disputed);

    // Advance time past fallback deadline:
    let now = setup.env.ledger().timestamp();
    setup.env.ledger().set_timestamp(now + 200);

    // Backup resolver now resolves as Refund
    client.resolve_dispute(&setup.backup, &escrow_id, &ResolutionType::Refund);
    assert_eq!(
        client.get_escrow(&escrow_id).state,
        EscrowState::PendingFinalization
    );

    // Finalize after appeal window
    setup.env.ledger().set_timestamp(now + 200 + 86401);
    client.finalize_dispute(&setup.buyer, &escrow_id);
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Refunded);

    let tc = token::Client::new(&setup.env, &setup.token);
    assert_eq!(tc.balance(&setup.buyer), 1000);
}

#[test]
fn test_fallback_rotate_resolver_fails() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let new_res = Address::generate(&setup.env);

    let escrow_id = create_funded_shipped_disputed(&setup);

    // rotate_resolver is only supported for Single resolver escrows
    assert_eq!(
        client.try_rotate_resolver(&setup.seller, &escrow_id, &new_res),
        Err(Ok(ContractError::InvalidState))
    );
}

#[test]
fn test_fallback_mutual_cancel_success() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let amount = 1000_i128;
    let deadline = 100_000_u64;

    let escrow_id = client.create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.primary,
        &setup.backup,
        &deadline,
        &setup.token,
        &amount,
        &0_u32,
        &3600_u64,
    );

    let sac = token::StellarAssetClient::new(&setup.env, &setup.token);
    sac.mint(&setup.buyer, &amount);
    client.fund_escrow(&escrow_id, &setup.buyer);

    // Both parties agree to cancel
    client.mutual_cancel(&escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Canceled);

    let tc = token::Client::new(&setup.env, &setup.token);
    assert_eq!(tc.balance(&setup.buyer), amount);
}

#[test]
fn test_fallback_co_signed_release_success() {
    let setup = setup();
    let client = EscrowClient::new(&setup.env, &setup.contract_id);
    let amount = 1000_i128;
    let deadline = 100_000_u64;

    let escrow_id = client.create_escrow_with_fallback(
        &setup.seller,
        &Some(setup.buyer.clone()),
        &setup.primary,
        &setup.backup,
        &deadline,
        &setup.token,
        &amount,
        &0_u32,
        &3600_u64,
    );

    let sac = token::StellarAssetClient::new(&setup.env, &setup.token);
    sac.mint(&setup.buyer, &amount);
    client.fund_escrow(&escrow_id, &setup.buyer);

    // Both parties co-sign release
    client.co_signed_release(&setup.seller, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);

    let tc = token::Client::new(&setup.env, &setup.token);
    assert_eq!(tc.balance(&setup.seller), amount);
}
