#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    Address, Env, Vec,
};
use crate::{ContractError, Escrow, EscrowClient, Payee};

fn setup(
    env: &Env,
) -> (Address, Address, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let token = env.register_stellar_asset_contract(Address::generate(env));
    (admin, fee_collector, seller, buyer, resolver, token)
}

fn init_client(env: &Env) -> (EscrowClient, Address, Address, Address, Address, Address, Address) {
    let (admin, fee_collector, seller, buyer, resolver, token) = setup(env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    (client, admin, fee_collector, seller, buyer, resolver, token)
}

fn make_payees(env: &Env, seller: &Address) -> Vec<Payee> {
    let mut payees = Vec::new(env);
    payees.push_back(Payee { address: seller.clone(), bps: 10_000 });
    payees
}

// ── Non-strict mode (default) ───────────────────────────────────────────────

#[test]
fn non_strict_any_resolver_accepted() {
    let env = Env::default();
    let (client, _admin, _fee_collector, seller, _buyer, resolver, token) = init_client(&env);

    let payees = make_payees(&env, &seller);
    // default: strict = false → any resolver allowed
    client.create_escrow(&payees, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &0_u32, &3600_u64);
}

#[test]
fn non_strict_mode_is_default() {
    let env = Env::default();
    let (client, _admin, _fee_collector, _seller, _buyer, _resolver, _token) = init_client(&env);
    assert!(!client.is_resolver_strict());
}

// ── Strict mode management ──────────────────────────────────────────────────

#[test]
fn admin_can_enable_strict_mode() {
    let env = Env::default();
    let (client, admin, _fee_collector, _seller, _buyer, _resolver, _token) = init_client(&env);

    client.set_resolver_strict(&admin, &true);
    assert!(client.is_resolver_strict());
}

#[test]
fn admin_can_disable_strict_mode() {
    let env = Env::default();
    let (client, admin, _fee_collector, _seller, _buyer, _resolver, _token) = init_client(&env);

    client.set_resolver_strict(&admin, &true);
    client.set_resolver_strict(&admin, &false);
    assert!(!client.is_resolver_strict());
}

#[test]
fn non_admin_cannot_set_strict_mode() {
    let env = Env::default();
    let (client, _admin, _fee_collector, seller, _buyer, _resolver, _token) = init_client(&env);

    let result = client.try_set_resolver_strict(&seller, &true);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

// ── Approved resolver management ────────────────────────────────────────────

#[test]
fn admin_can_add_approved_resolver() {
    let env = Env::default();
    let (client, admin, _fee_collector, _seller, _buyer, resolver, _token) = init_client(&env);

    client.add_approved_resolver(&admin, &resolver);
    let approved = client.get_approved_resolvers();
    assert_eq!(approved.len(), 1);
    assert_eq!(approved.get(0).unwrap(), resolver);
}

#[test]
fn add_same_resolver_twice_is_noop() {
    let env = Env::default();
    let (client, admin, _fee_collector, _seller, _buyer, resolver, _token) = init_client(&env);

    client.add_approved_resolver(&admin, &resolver);
    client.add_approved_resolver(&admin, &resolver);
    assert_eq!(client.get_approved_resolvers().len(), 1);
}

#[test]
fn non_admin_cannot_add_resolver() {
    let env = Env::default();
    let (client, _admin, _fee_collector, seller, _buyer, resolver, _token) = init_client(&env);

    let result = client.try_add_approved_resolver(&seller, &resolver);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn admin_can_remove_approved_resolver() {
    let env = Env::default();
    let (client, admin, _fee_collector, _seller, _buyer, resolver, _token) = init_client(&env);

    client.add_approved_resolver(&admin, &resolver);
    client.remove_approved_resolver(&admin, &resolver);
    assert_eq!(client.get_approved_resolvers().len(), 0);
}

#[test]
fn remove_unknown_resolver_returns_error() {
    let env = Env::default();
    let (client, admin, _fee_collector, _seller, _buyer, resolver, _token) = init_client(&env);

    let result = client.try_remove_approved_resolver(&admin, &resolver);
    assert_eq!(result, Err(Ok(ContractError::InvalidAddress)));
}

// ── Strict mode + escrow creation ───────────────────────────────────────────

#[test]
fn strict_mode_rejects_unknown_resolver_at_creation() {
    let env = Env::default();
    let (client, admin, _fee_collector, seller, _buyer, resolver, token) = init_client(&env);

    client.set_resolver_strict(&admin, &true);
    let payees = make_payees(&env, &seller);

    let result = client.try_create_escrow(
        &payees, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &0_u32, &3600_u64,
    );
    assert_eq!(result, Err(Ok(ContractError::UnauthorizedResolver)));
}

#[test]
fn strict_mode_accepts_approved_resolver_at_creation() {
    let env = Env::default();
    let (client, admin, _fee_collector, seller, _buyer, resolver, token) = init_client(&env);

    client.add_approved_resolver(&admin, &resolver);
    client.set_resolver_strict(&admin, &true);
    let payees = make_payees(&env, &seller);

    client.create_escrow(
        &payees, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &0_u32, &3600_u64,
    );
}

#[test]
fn non_strict_mode_accepts_any_resolver_even_when_list_empty() {
    let env = Env::default();
    let (client, _admin, _fee_collector, seller, _buyer, resolver, token) = init_client(&env);

    let payees = make_payees(&env, &seller);
    // strict = false (default), no approved resolvers — should still work
    client.create_escrow(
        &payees, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &0_u32, &3600_u64,
    );
}
