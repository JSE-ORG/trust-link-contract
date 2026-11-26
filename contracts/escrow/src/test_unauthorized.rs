#![cfg(test)]
//! Systematic unauthorized-access tests for every admin-gated state-mutating
//! entry point (#19), plus role-conflict guards (#security).
//!
//! Each test invokes the entry point with a non-admin caller and asserts the
//! `try_*` client method returns an error. This validates the contract's
//! `caller != admin → NotAuthorized` guard, not just the host's `require_auth`
//! reject path.
//!
//! # Timelocked entry-point matrix (#1247)
//!
//! Parts A–C of issue #1247 require verified test coverage showing:
//!
//! - Every `queue_*` function returns `ContractError::NotAuthorized` when
//!   invoked by a non-admin.
//! - Every `execute_*` function returns `ContractError::TimelockNotElapsed`
//!   when the timelock has not yet elapsed.
//! - Every `execute_*` function returns `ContractError::InvalidProposalHash`
//!   when executed with parameters that differ from the queued proposal.
//! - Every `execute_*` function returns `ContractError::NotAuthorized` when
//!   invoked by a non-admin (or when no proposal is queued and the op was
//!   previously invalidated).

use crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS;
use crate::{ContractError, Escrow, EscrowClient, Payee};
use soroban_sdk::{
    testutils::{Address as _, BytesN as _, Ledger as _},
    Address, BytesN, Env, IntoVal, String, Vec,
};

/// Fresh contract with admin/fee_collector initialised. All auths are mocked
/// so tests can drive the API freely; each test then exercises authorization
/// via the explicit caller arg rather than by toggling mocks.
fn fresh_contract(env: &Env) -> (EscrowClient<'static>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    (client, admin)
}

#[test]
fn pause_contract_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_pause_contract(&intruder),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn unpause_contract_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.pause_contract(&admin);

    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_unpause_contract(&intruder),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn set_admin_rejects_unauthorized_caller() {
    // set_admin reads the current admin from storage and requires its auth.
    // With no mocked auths the host-level check fails and the call errors.
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let new_admin = Address::generate(&env);

    env.mock_auths(&[]);
    assert!(client.try_set_admin(&new_admin).is_err());
}

#[test]
fn set_fee_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_set_fee(&intruder, &100_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn set_ttl_extension_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_set_ttl_extension(&intruder, &60_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn set_arbitration_fee_rejects_unauthorized_caller() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_set_arbitration_fee(&intruder, &10_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

// ── Role-conflict guards ─────────────────────────────────────────────────────

/// Helper: register a SAC token and return its address.
fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

#[test]
fn create_escrow_rejects_resolver_equal_to_seller() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);

    let seller = Address::generate(&env);
    let token = register_token(&env);

    let seller_val = seller.clone().into_val(&env);
    assert_eq!(
        client.try_create_escrow_8(
            &seller_val,
            &None::<Address>,
            &seller, // resolver == seller
            &token,
            &100_i128,
            &0_u32,
            &3600_u64,
        ),
        Err(Ok(ContractError::ConflictingRoles)),
    );
}

#[test]
fn fund_escrow_rejects_buyer_equal_to_seller() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);

    // Mint tokens to the seller so the transfer would otherwise succeed.
    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&seller, &1000_i128);

    let mut payees_69 = Vec::new(&env);
    payees_69.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_69_val = payees_69.into_val(&env);
    let id = client.create_escrow(
        &payees_69_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    assert_eq!(
        client.try_fund_escrow(&id, &seller), // buyer == seller
        Err(Ok(ContractError::ConflictingRoles)),
    );
}

#[test]
fn fund_escrow_rejects_buyer_equal_to_resolver() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);

    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);

    // Mint tokens to the resolver so the transfer would otherwise succeed.
    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&resolver, &1000_i128);

    let mut payees_68 = Vec::new(&env);
    payees_68.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_68_val = payees_68.into_val(&env);
    let id = client.create_escrow(
        &payees_68_val,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    assert_eq!(
        client.try_fund_escrow(&id, &resolver), // buyer == resolver
        Err(Ok(ContractError::ConflictingRoles)),
    );
}

// ── Part A: queue_* → NotAuthorized for non-admin (#1247) ───────────────────

/// Advance the clock past one full timelock delay so a queued proposal is
/// immediately executable in the same test.
fn advance_past_timelock(env: &Env) {
    env.ledger()
        .with_mut(|li| li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS + 1);
}

#[test]
fn queue_set_admin_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    let new_admin = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_admin(&intruder, &new_admin),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_upgrade_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    let wasm_hash = BytesN::<32>::random(&env);
    assert_eq!(
        client.try_queue_upgrade(&intruder, &wasm_hash),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_arbitration_fee_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_arbitration_fee(&intruder, &50_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_platform_fee_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_platform_fee(&intruder, &50_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_treasury_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    let treasury = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_treasury(&intruder, &treasury),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_fee_collector_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    let new_collector = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_fee_collector(&intruder, &new_collector),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_ttl_extension_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_ttl_extension(&intruder, &120_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_amount_limits_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_amount_limits(&intruder, &1_i128, &1_000_i128),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_add_approved_resolver_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    let resolver = Address::generate(&env);
    assert_eq!(
        client.try_queue_add_approved_resolver(&intruder, &resolver),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_remove_approved_resolver_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    let resolver = Address::generate(&env);
    assert_eq!(
        client.try_queue_remove_approved_resolver(&intruder, &resolver),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_resolver_strict_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_resolver_strict(&intruder, &true),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_allowlist_enabled_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_allowlist_enabled(&intruder, &true),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_add_allowed_token_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    assert_eq!(
        client.try_queue_add_allowed_token(&intruder, &token),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_remove_allowed_token_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    assert_eq!(
        client.try_queue_remove_allowed_token(&intruder, &token),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_pause_contract_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_pause_contract(&intruder),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_unpause_contract_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_unpause_contract(&intruder),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_max_appeals_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_max_appeals(&intruder, &5_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_max_basket_size_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_max_basket_size(&intruder, &5_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_appeal_fee_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_queue_set_appeal_fee(&intruder, &0_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn queue_set_timelock_delay_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    // 48h is a valid delay value; rejection must be purely authorization, not validation.
    let valid_delay = 48 * 60 * 60_u64;
    assert_eq!(
        client.try_queue_set_timelock_delay(&intruder, &valid_delay),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

// ── Part B: execute_* → TimelockNotElapsed before delay elapses (#1247) ─────

#[test]
fn execute_set_admin_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let new_admin = Address::generate(&env);
    client.queue_set_admin(&admin, &new_admin);
    // No time advance — timelock has not elapsed.
    assert_eq!(
        client.try_execute_set_admin(&admin, &new_admin),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_arbitration_fee_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_arbitration_fee(&admin, &10_u32);
    assert_eq!(
        client.try_execute_set_arbitration_fee(&admin, &10_u32),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_platform_fee_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_platform_fee(&admin, &50_u32);
    assert_eq!(
        client.try_execute_set_platform_fee(&admin, &50_u32),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_treasury_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let treasury = Address::generate(&env);
    client.queue_set_treasury(&admin, &treasury);
    assert_eq!(
        client.try_execute_set_treasury(&admin, &treasury),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_fee_collector_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let new_collector = Address::generate(&env);
    client.queue_set_fee_collector(&admin, &new_collector);
    assert_eq!(
        client.try_execute_set_fee_collector(&admin, &new_collector),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_ttl_extension_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_ttl_extension(&admin, &120_u32);
    assert_eq!(
        client.try_execute_set_ttl_extension(&admin, &120_u32),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_amount_limits_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_amount_limits(&admin, &1_i128, &1_000_i128);
    assert_eq!(
        client.try_execute_set_amount_limits(&admin, &1_i128, &1_000_i128),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_add_approved_resolver_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let resolver = Address::generate(&env);
    client.queue_add_approved_resolver(&admin, &resolver);
    assert_eq!(
        client.try_execute_add_approved_resolver(&admin, &resolver),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_remove_approved_resolver_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let resolver = Address::generate(&env);
    client.queue_remove_approved_resolver(&admin, &resolver);
    assert_eq!(
        client.try_execute_remove_approved_resolver(&admin, &resolver),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_resolver_strict_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_resolver_strict(&admin, &true);
    assert_eq!(
        client.try_execute_set_resolver_strict(&admin, &true),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_allowlist_enabled_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_allowlist_enabled(&admin, &true);
    assert_eq!(
        client.try_execute_set_allowlist_enabled(&admin, &true),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_add_allowed_token_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.queue_add_allowed_token(&admin, &token);
    assert_eq!(
        client.try_execute_add_allowed_token(&admin, &token),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_remove_allowed_token_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.queue_remove_allowed_token(&admin, &token);
    assert_eq!(
        client.try_execute_remove_allowed_token(&admin, &token),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_pause_contract_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_pause_contract(&admin);
    assert_eq!(
        client.try_execute_pause_contract(&admin),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_unpause_contract_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_unpause_contract(&admin);
    assert_eq!(
        client.try_execute_unpause_contract(&admin),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_max_appeals_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_max_appeals(&admin, &5_u32);
    assert_eq!(
        client.try_execute_set_max_appeals(&admin, &5_u32),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_max_basket_size_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_max_basket_size(&admin, &5_u32);
    assert_eq!(
        client.try_execute_set_max_basket_size(&admin, &5_u32),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_appeal_fee_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_appeal_fee(&admin, &0_u32);
    assert_eq!(
        client.try_execute_set_appeal_fee(&admin, &0_u32),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

#[test]
fn execute_set_timelock_delay_returns_timelock_not_elapsed() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let new_delay = 48 * 60 * 60_u64;
    client.queue_set_timelock_delay(&admin, &new_delay);
    // Even 24h is not enough — the new 48h delay applies (#1238).
    env.ledger()
        .with_mut(|li| li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS);
    assert_eq!(
        client.try_execute_set_timelock_delay(&admin, &new_delay),
        Err(Ok(ContractError::TimelockNotElapsed)),
    );
}

// ── Part B continued: execute_* → InvalidProposalHash on param mismatch ─────

#[test]
fn execute_set_admin_returns_invalid_proposal_hash_on_mismatch() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let new_admin = Address::generate(&env);
    let different_admin = Address::generate(&env);
    client.queue_set_admin(&admin, &new_admin);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_set_admin(&admin, &different_admin),
        Err(Ok(ContractError::InvalidProposalHash)),
    );
}

#[test]
fn execute_set_arbitration_fee_returns_invalid_proposal_hash_on_mismatch() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_arbitration_fee(&admin, &10_u32);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_set_arbitration_fee(&admin, &20_u32),
        Err(Ok(ContractError::InvalidProposalHash)),
    );
}

#[test]
fn execute_set_platform_fee_returns_invalid_proposal_hash_on_mismatch() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_platform_fee(&admin, &50_u32);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_set_platform_fee(&admin, &99_u32),
        Err(Ok(ContractError::InvalidProposalHash)),
    );
}

#[test]
fn execute_set_amount_limits_returns_invalid_proposal_hash_on_mismatch() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_amount_limits(&admin, &1_i128, &1_000_i128);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_set_amount_limits(&admin, &1_i128, &2_000_i128),
        Err(Ok(ContractError::InvalidProposalHash)),
    );
}

#[test]
fn execute_set_resolver_strict_returns_invalid_proposal_hash_on_mismatch() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    client.queue_set_resolver_strict(&admin, &true);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_set_resolver_strict(&admin, &false),
        Err(Ok(ContractError::InvalidProposalHash)),
    );
}

// ── Part B continued: execute_* → NotAuthorized for non-admin (#1247) ────────

#[test]
fn execute_set_admin_rejects_non_admin() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let new_admin = Address::generate(&env);
    let intruder = Address::generate(&env);
    client.queue_set_admin(&admin, &new_admin);
    advance_past_timelock(&env);
    // intruder is not the current admin so require_admin_caller rejects them.
    assert_eq!(
        client.try_execute_set_admin(&intruder, &new_admin),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn execute_set_arbitration_fee_rejects_non_admin() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    client.queue_set_arbitration_fee(&admin, &10_u32);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_set_arbitration_fee(&intruder, &10_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn execute_set_platform_fee_rejects_non_admin() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    client.queue_set_platform_fee(&admin, &50_u32);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_set_platform_fee(&intruder, &50_u32),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn execute_pause_contract_rejects_non_admin() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    client.queue_pause_contract(&admin);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_pause_contract(&intruder),
        Err(Ok(ContractError::NotAuthorized)),
    );
}

#[test]
fn execute_unpause_contract_rejects_non_admin() {
    let env = Env::default();
    let (client, admin) = fresh_contract(&env);
    let intruder = Address::generate(&env);
    client.queue_unpause_contract(&admin);
    advance_past_timelock(&env);
    assert_eq!(
        client.try_execute_unpause_contract(&intruder),
        Err(Ok(ContractError::NotAuthorized)),
    );
}
