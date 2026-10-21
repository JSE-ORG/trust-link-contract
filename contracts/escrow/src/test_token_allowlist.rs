#![cfg(test)]
//! Enforcement coverage for the admin token allowlist (Issue #1052).
//!
//! `is_token_allowed` used to read `DataKey::TokenAllowlist` as a
//! `Map<Address, bool>` while every writer (`add_allowed_token`,
//! `remove_allowed_token`, and the timelocked variants) persisted it as a
//! `Vec<Address>`. With the allowlist enabled and populated, the first
//! `create_*` call trapped on XDR decode instead of rejecting with
//! `TokenNotAllowed`. These tests pin the corrected behaviour across every
//! creation path: allowed token succeeds, disallowed token is rejected.

use crate::test_helpers::setup_contract;
use crate::{ContractError, Payee};
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String, Vec};

fn register_token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn payees_val(env: &Env, seller: &Address) -> soroban_sdk::Val {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    payees.into_val(env)
}

/// `create_escrow` accepts an allowlisted token and rejects a token that was
/// never added, with `TokenNotAllowed` (27) — no XDR decode trap.
#[test]
fn allowlist_gates_create_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let allowed = register_token(&env);
    let disallowed = register_token(&env);

    client.set_token_allowlist_enabled(&admin, &true);
    client.add_allowed_token(&admin, &allowed);

    // allowlisted token: succeeds
    let ok = client.try_create_escrow(
        &payees_val(&env, &seller),
        &None::<Address>,
        &resolver,
        &allowed,
        &1_000_i128,
        &0_u32,
        &0_u32,
        &3_600_u64,
        &None::<String>,
    );
    assert!(ok.is_ok(), "allowlisted token must be accepted");

    // disallowed token: rejected with the dedicated error, not a host trap
    let rejected = client.try_create_escrow(
        &payees_val(&env, &seller),
        &None::<Address>,
        &resolver,
        &disallowed,
        &1_000_i128,
        &0_u32,
        &0_u32,
        &3_600_u64,
        &None::<String>,
    );
    assert_eq!(rejected, Err(Ok(ContractError::TokenNotAllowed)));
}

/// `create_basket_escrow` validates every entry; a single disallowed token in
/// the basket rejects the whole creation.
#[test]
fn allowlist_gates_create_basket_escrow() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let allowed = register_token(&env);
    let disallowed = register_token(&env);

    client.set_token_allowlist_enabled(&admin, &true);
    client.add_allowed_token(&admin, &allowed);

    let mut tokens = Vec::new(&env);
    tokens.push_back(allowed.clone());
    let mut amounts = Vec::new(&env);
    amounts.push_back(1_000_i128);
    assert!(client
        .try_create_basket_escrow(
            &seller,
            &None::<Address>,
            &resolver,
            &tokens,
            &amounts,
            &0_u32,
            &3_600_u64,
            &None::<String>,
            &None::<u64>,
        )
        .is_ok());

    let mut mixed = Vec::new(&env);
    mixed.push_back(allowed.clone());
    mixed.push_back(disallowed);
    let mut mixed_amounts = Vec::new(&env);
    mixed_amounts.push_back(1_000_i128);
    mixed_amounts.push_back(500_i128);
    assert_eq!(
        client.try_create_basket_escrow(
            &seller,
            &None::<Address>,
            &resolver,
            &mixed,
            &mixed_amounts,
            &0_u32,
            &3_600_u64,
            &None::<String>,
            &None::<u64>,
        ),
        Err(Ok(ContractError::TokenNotAllowed))
    );
}

/// `create_escrow_multi` enforces the allowlist before resolver validation.
#[test]
fn allowlist_gates_create_escrow_multi() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let allowed = register_token(&env);
    let disallowed = register_token(&env);

    client.set_token_allowlist_enabled(&admin, &true);
    client.add_allowed_token(&admin, &allowed);

    let mut resolvers = Vec::new(&env);
    resolvers.push_back(Address::generate(&env));
    resolvers.push_back(Address::generate(&env));
    let threshold = 2_u32;

    assert!(client
        .try_create_escrow_multi(
            &seller,
            &Some(buyer.clone()),
            &resolvers,
            &threshold,
            &allowed,
            &1_000_i128,
            &0_u32,
            &3_600_u64,
        )
        .is_ok());

    assert_eq!(
        client.try_create_escrow_multi(
            &seller,
            &Some(buyer.clone()),
            &resolvers,
            &threshold,
            &disallowed,
            &1_000_i128,
            &0_u32,
            &3_600_u64,
        ),
        Err(Ok(ContractError::TokenNotAllowed))
    );
}

/// A token removed from the allowlist is rejected again on the next creation.
#[test]
fn allowlist_remove_revokes_token() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_addr = register_token(&env);

    client.set_token_allowlist_enabled(&admin, &true);
    client.add_allowed_token(&admin, &token_addr);
    assert_eq!(client.get_allowed_tokens().len(), 1);

    client.remove_allowed_token(&admin, &token_addr);
    assert!(client.get_allowed_tokens().is_empty());

    assert_eq!(
        client.try_create_escrow(
            &payees_val(&env, &seller),
            &None::<Address>,
            &resolver,
            &token_addr,
            &1_000_i128,
            &0_u32,
            &0_u32,
            &3_600_u64,
            &None::<String>,
        ),
        Err(Ok(ContractError::TokenNotAllowed))
    );
}

/// With the allowlist disabled the check stays a no-op and any token is
/// accepted, preserving the backward-compatible default.
#[test]
fn allowlist_disabled_accepts_any_token() {
    let env = Env::default();
    env.mock_all_auths();

    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_addr = register_token(&env);

    assert!(client.get_allowed_tokens().is_empty());
    assert!(client
        .try_create_escrow(
            &payees_val(&env, &seller),
            &None::<Address>,
            &resolver,
            &token_addr,
            &1_000_i128,
            &0_u32,
            &0_u32,
            &3_600_u64,
            &None::<String>,
        )
        .is_ok());
}
