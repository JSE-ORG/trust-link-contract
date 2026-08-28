#![cfg(test)]
//! Storage-migration tests for the upgrade path.
//!
//! `upgrade` only swaps the contract WASM; storage survives untouched. The
//! risk is therefore not that data disappears but that a new build reads old
//! entries under a changed schema. These tests pin down the contract used to
//! manage that: `get_storage_version` reports what is on chain, and `migrate`
//! moves it forward exactly once, without disturbing existing escrows.
//!
//! Simulating a real WASM swap requires two compiled artifacts, which the unit
//! test environment does not have. Instead the "old deployment" is reproduced
//! by removing the version marker from instance storage — byte-for-byte what an
//! escrow written by a pre-versioning build looks like to the new code.

use crate::{
    ContractError, DataKey, Escrow, EscrowClient, EscrowData, EscrowState, Payee, STORAGE_VERSION,
};
use soroban_sdk::{
    testutils::Address as _, token, Address, Env, IntoVal, String as SorobanString, Vec,
};

struct Fixture {
    contract_id: Address,
    client: EscrowClient<'static>,
    admin: Address,
    seller: Address,
    buyer: Address,
    resolver: Address,
    token: Address,
}

fn setup(env: &Env) -> Fixture {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);

    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    Fixture {
        contract_id,
        client,
        admin,
        seller,
        buyer,
        resolver,
        token,
    }
}

fn single_payee(env: &Env, address: &Address) -> Vec<Payee> {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}

/// Creates and funds an escrow, returning its id.
fn funded_escrow(env: &Env, f: &Fixture, amount: i128) -> u64 {
    token::StellarAssetClient::new(env, &f.token).mint(&f.buyer, &amount);

    let payees = single_payee(env, &f.seller).into_val(env);
    let escrow_id = f.client.create_escrow_8(
        &payees,
        &Some(f.buyer.clone()),
        &f.resolver,
        &f.token,
        &amount,
        &0_u32,
        &3_600_u64,
    );
    f.client.fund_escrow(&escrow_id, &f.buyer);
    escrow_id
}

/// Reproduces a deployment made before storage versioning existed.
fn downgrade_to_unversioned(env: &Env, contract_id: &Address) {
    env.as_contract(contract_id, || {
        env.storage().instance().remove(&DataKey::StorageVersion);
    });
}

#[test]
fn fresh_deployment_is_already_at_current_storage_version() {
    let env = Env::default();
    let f = setup(&env);

    assert_eq!(f.client.get_storage_version(), STORAGE_VERSION);
    // Nothing to migrate, so the call must refuse rather than re-run steps.
    assert_eq!(
        f.client.try_migrate(&f.admin),
        Err(Ok(ContractError::AlreadyInitialized)),
    );
}

#[test]
fn migrate_preserves_escrow_data() {
    let env = Env::default();
    let f = setup(&env);

    let amount = 1_000_i128;
    let escrow_id = funded_escrow(&env, &f, amount);
    let before: EscrowData = f.client.get_escrow(&escrow_id);
    assert_eq!(before.state, EscrowState::Funded);

    downgrade_to_unversioned(&env, &f.contract_id);
    assert_eq!(f.client.get_storage_version(), 0);

    f.client.migrate(&f.admin);

    assert_eq!(f.client.get_storage_version(), STORAGE_VERSION);
    assert_eq!(
        f.client.get_escrow(&escrow_id),
        before,
        "migration must not alter stored escrow data",
    );
}

#[test]
fn migrated_escrow_remains_operable() {
    let env = Env::default();
    let f = setup(&env);

    let escrow_id = funded_escrow(&env, &f, 1_000_i128);
    downgrade_to_unversioned(&env, &f.contract_id);
    f.client.migrate(&f.admin);

    // The lifecycle continues from exactly where it left off.
    f.client.mark_shipped(
        &f.seller,
        &escrow_id,
        &SorobanString::from_str(&env, "TRACK-1"),
    );
    assert_eq!(f.client.get_escrow(&escrow_id).state, EscrowState::Shipped);
}

#[test]
fn migrate_is_idempotent() {
    let env = Env::default();
    let f = setup(&env);

    let escrow_id = funded_escrow(&env, &f, 1_000_i128);
    downgrade_to_unversioned(&env, &f.contract_id);

    f.client.migrate(&f.admin);
    let after_first: EscrowData = f.client.get_escrow(&escrow_id);

    // A retried deployment step must be rejected, not applied twice.
    assert_eq!(
        f.client.try_migrate(&f.admin),
        Err(Ok(ContractError::AlreadyInitialized)),
    );
    assert_eq!(f.client.get_escrow(&escrow_id), after_first);
    assert_eq!(f.client.get_storage_version(), STORAGE_VERSION);
}

#[test]
fn migrate_rejects_non_admin() {
    let env = Env::default();
    let f = setup(&env);

    downgrade_to_unversioned(&env, &f.contract_id);

    let intruder = Address::generate(&env);
    assert_eq!(
        f.client.try_migrate(&intruder),
        Err(Ok(ContractError::NotAuthorized)),
    );
    // The failed attempt must leave the version marker absent.
    assert_eq!(f.client.get_storage_version(), 0);
}
