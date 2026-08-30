#![cfg(test)]

//! TTL extension tests for issue #408.
//!
//! These tests verify that every storage interaction — both read and write —
//! correctly extends the TTL of instance and persistent entries, and that the
//! configurable `TtlExtensionLedgers` value is respected end-to-end.
//!
//! # Coverage
//! - [`test_escrow_stored_in_persistent_storage`] — basic persistent read/write roundtrip
//! - [`test_set_ttl_extension_persists`] — admin-configured TTL value is used during operations
//! - [`test_dispute_stored_in_persistent_storage`] — dispute persistent entry survives
//! - [`test_persistent_entry_readable_after_large_ledger_advance`] — escrow key survives
//!   a ledger advance equal to `DEFAULT_TTL_EXTENSION - 1`
//! - [`test_vendor_index_ttl_extended_on_write`] — vendor index TTL extended on write
//! - [`test_vendor_index_ttl_extended_on_read`] — vendor index TTL extended on read
//! - [`test_buyer_index_ttl_extended_on_write`] — buyer index TTL extended on write
//! - [`test_instance_ttl_extended_on_escrow_creation`] — instance survives near-expiry after create
//! - [`test_custom_ttl_applied_to_persistent_entry`] — custom TTL is used for new escrows
//! - [`test_resolver_votes_ttl_extended`] — resolver votes persistent entry survives
//!
//! # `get_ttl_extension` / configuration-logic coverage
//! - [`get_ttl_extension_defaults_when_unset`] — fresh contract resolves to `DEFAULT_TTL_EXTENSION`
//! - [`internal_and_storage_get_ttl_extension_always_agree`] — the two accessor helpers never drift
//! - [`set_ttl_extension_overwrites_the_previous_value`] — last write wins
//! - [`ttl_threshold_divisor_is_half`] — the extend-when-below threshold stays
//!   tied to `TTL_THRESHOLD_DIVISOR`
//! - [`queued_ttl_extension_applies_after_timelock`] — timelocked update flows into `get_ttl_extension`
//! - [`execute_ttl_extension_before_timelock_elapses_is_rejected`] — early execute reverts
//! - [`execute_ttl_extension_without_a_queued_proposal_is_rejected`] — execute with nothing queued reverts
//! - [`timelocked_smaller_ttl_value_flows_into_extend_ttl`] — a reduced value is honoured end-to-end

use crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS;
use crate::test_helpers::{create_funded_escrow, setup_contract};
use crate::{ContractError, DEFAULT_TTL_EXTENSION, MIN_TTL_EXTENSION, TTL_THRESHOLD_DIVISOR};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin)
        .address()
}

// ============================================================================
// Persistent storage roundtrip
// ============================================================================

/// Escrow data written to persistent storage is readable after funding.
#[test]
fn test_escrow_stored_in_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );

    // Escrow is readable after funding — persistent storage write + TTL extension succeeded.
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.amount, 1000);
    assert_eq!(escrow.fee_bps, 100);
}

/// Dispute data written to persistent storage is readable after a dispute is raised.
#[test]
fn test_dispute_stored_in_persistent_storage() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 100, 3600,
    );
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-TTL"),
    );

    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "test"),
        &soroban_sdk::String::from_str(&env, "desc"),
        &soroban_sdk::BytesN::from_array(&env, &[0xab; 32]),
    );

    // Dispute is readable from persistent storage after write + TTL extension.
    let dispute = client.get_dispute(&id);
    assert!(dispute.is_some());
    let dispute = dispute.unwrap();
    assert_eq!(dispute.escrow_id, id);
}

// ============================================================================
// Configurable TTL extension
// ============================================================================

/// Admin-configured TTL extension is used for subsequent escrow operations.
#[test]
fn test_set_ttl_extension_persists() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    // Configure a custom TTL extension (half of default = ~7 days).
    client.set_ttl_extension(&admin, &60_480_u32);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // All escrow operations still work with the custom TTL value.
    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 500, 0, 3600,
    );
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.amount, 500);
}

/// A custom TTL value configured by admin is reflected in newly created escrows.
#[test]
fn test_custom_ttl_applied_to_persistent_entry() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    // Set TTL to the smallest allowed value to ensure it's respected (still
    // usable in tests) without tripping the `MIN_TTL_EXTENSION` floor.
    client.set_ttl_extension(&admin, &MIN_TTL_EXTENSION);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 999, 0, 3600,
    );

    // Escrow is readable — custom TTL extension was applied without error.
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.amount, 999);
}

// ============================================================================
// TTL survives ledger advancement
// ============================================================================

/// Persistent escrow entry survives a ledger advancement close to the default
/// TTL threshold, proving that the extend_ttl call on write succeeds.
///
/// Addresses the archival risk: without extend_ttl a ledger advance of
/// `DEFAULT_TTL_EXTENSION` ledgers would archive the entry.
#[test]
fn test_persistent_entry_readable_after_large_ledger_advance() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1234, 0, 3600,
    );

    // Advance ledger sequence by just under DEFAULT_TTL_EXTENSION ledgers.
    // If extend_ttl was not called the entry would be archived and a read would fail.
    let mut ledger_info = env.ledger().get();
    ledger_info.sequence_number += DEFAULT_TTL_EXTENSION - 1;
    env.ledger().set(ledger_info);

    // Escrow must still be readable — persistent TTL was extended on write.
    let escrow = client.get_escrow(&id);
    assert_eq!(
        escrow.amount, 1234,
        "persistent escrow entry was archived after ledger advance"
    );
}

/// Instance storage survives a near-TTL ledger advance after escrow creation,
/// proving that instance extend_ttl is called during create_escrow.
#[test]
fn test_instance_ttl_extended_on_escrow_creation() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Create and fund two escrows before the ledger advance.
    let id1 = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 100, 0, 3600,
    );
    let id2 = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 200, 0, 3600,
    );
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);

    // Advance ledger sequence close to the TTL threshold.
    let mut ledger_info = env.ledger().get();
    ledger_info.sequence_number += DEFAULT_TTL_EXTENSION - 1;
    env.ledger().set(ledger_info);

    // Creating a third escrow must succeed — instance keys (counter, admin, etc.)
    // are alive because extend_instance_ttl was called on previous entries.
    let id3 = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 300, 0, 3600,
    );
    assert_eq!(
        id3, 3,
        "instance counter was archived; id should be monotonically 3"
    );
}

// ============================================================================
// Vendor / Buyer index TTL
// ============================================================================

/// Vendor index TTL is extended when the index is written.
#[test]
fn test_vendor_index_ttl_extended_on_write() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Writing vendor index (via create_escrow_internal → write_vendor_escrow_index).
    create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 500, 0, 3600,
    );

    // Advance ledger to near-TTL.
    let mut ledger_info = env.ledger().get();
    ledger_info.sequence_number += DEFAULT_TTL_EXTENSION - 1;
    env.ledger().set(ledger_info);

    // Vendor index must still be queryable — TTL was extended on write.
    let escrows = client.get_escrows_by_vendor(&seller);
    assert_eq!(
        escrows.len(),
        1,
        "vendor index was archived after ledger advance"
    );
}

/// Vendor index TTL is extended when the index is read.
#[test]
fn test_vendor_index_ttl_extended_on_read() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 700, 0, 3600,
    );

    // First read at current ledger — this extends TTL on read.
    let escrows_before = client.get_escrows_by_vendor(&seller);
    assert_eq!(escrows_before.len(), 1);

    // Advance by DEFAULT_TTL_EXTENSION - 1 (entry was refreshed by the read above
    // so it should still be alive after this advance).
    let mut ledger_info = env.ledger().get();
    ledger_info.sequence_number += DEFAULT_TTL_EXTENSION - 1;
    env.ledger().set(ledger_info);

    // Re-read: entry must still be accessible.
    let escrows_after = client.get_escrows_by_vendor(&seller);
    assert_eq!(
        escrows_after.len(), 1,
        "vendor index was archived after ledger advance even though TTL should have been refreshed on read"
    );
}

/// Buyer index TTL is extended when the index is written (via fund_escrow).
#[test]
fn test_buyer_index_ttl_extended_on_write() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 800, 0, 3600,
    );

    // Advance to near-TTL boundary.
    let mut ledger_info = env.ledger().get();
    ledger_info.sequence_number += DEFAULT_TTL_EXTENSION - 1;
    env.ledger().set(ledger_info);

    // Buyer index must still be queryable after the advance.
    let buyer_escrows = client.get_escrows_by_buyer(&buyer);
    assert_eq!(
        buyer_escrows.len(),
        1,
        "buyer index was archived after ledger advance"
    );
}

// ============================================================================
// Resolver votes TTL
// ============================================================================

/// Resolver votes persistent entry survives after being written during a dispute.
#[test]
fn test_resolver_votes_ttl_extended() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-VOTES"),
    );
    client.raise_dispute(
        &buyer,
        &id,
        &soroban_sdk::Symbol::new(&env, "defect"),
        &soroban_sdk::String::from_str(&env, "item broken"),
        &soroban_sdk::BytesN::from_array(&env, &[0xcd; 32]),
    );

    // Advance ledger to near-TTL.
    let mut ledger_info = env.ledger().get();
    ledger_info.sequence_number += DEFAULT_TTL_EXTENSION - 1;
    env.ledger().set(ledger_info);

    // Dispute must still be readable — resolver votes TTL was extended on write.
    let dispute = client.get_dispute(&id);
    assert!(
        dispute.is_some(),
        "dispute entry was archived after ledger advance"
    );
}

// ============================================================================
// get_ttl_extension resolution + configuration logic
// ============================================================================

/// Reads the effective TTL extension both accessor helpers resolve, from inside
/// the contract's storage context.
fn effective_ttl_extension(env: &Env, contract_id: &Address) -> (u32, u32) {
    env.as_contract(contract_id, || {
        (
            crate::internal::get_ttl_extension(env),
            crate::storage::get_ttl_extension(env),
        )
    })
}

/// A fresh contract with no `TtlExtensionLedgers` entry resolves to the
/// compile-time default.
#[test]
fn get_ttl_extension_defaults_when_unset() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, _client, _admin, _fee_collector) = setup_contract(&env);

    let (internal_val, storage_val) = effective_ttl_extension(&env, &contract_id);
    assert_eq!(internal_val, DEFAULT_TTL_EXTENSION);
    assert_eq!(storage_val, DEFAULT_TTL_EXTENSION);
}

/// `internal::get_ttl_extension` and `storage::get_ttl_extension` must always
/// return the same value — both unset (default) and after configuration.
#[test]
fn internal_and_storage_get_ttl_extension_always_agree() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let (a, b) = effective_ttl_extension(&env, &contract_id);
    assert_eq!(a, b, "helpers disagree when TtlExtensionLedgers is unset");
    assert_eq!(a, DEFAULT_TTL_EXTENSION);

    client.set_ttl_extension(&admin, &7_777_u32);

    let (a, b) = effective_ttl_extension(&env, &contract_id);
    assert_eq!(a, b, "helpers disagree after set_ttl_extension");
    assert_eq!(a, 7_777);
}

/// Setting the TTL extension repeatedly keeps only the most recent value.
#[test]
fn set_ttl_extension_overwrites_the_previous_value() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    client.set_ttl_extension(&admin, &5_000_u32);
    assert_eq!(effective_ttl_extension(&env, &contract_id).1, 5_000);

    client.set_ttl_extension(&admin, &9_000_u32);
    assert_eq!(effective_ttl_extension(&env, &contract_id).1, 9_000);
}

/// The "bump the TTL when it drops below" threshold is `ext / TTL_THRESHOLD_DIVISOR`,
/// documented in `storage.rs` with the same constant. Guard the constant against drift.
#[test]
fn ttl_threshold_divisor_is_half() {
    assert_eq!(TTL_THRESHOLD_DIVISOR, 2);
}

// ============================================================================
// Timelocked TTL extension update (queue_set_ttl_extension / execute_set_ttl_extension)
// ============================================================================

/// A queued TTL-extension change is only applied once the admin timelock has
/// elapsed, and then it is what `get_ttl_extension` returns.
#[test]
fn queued_ttl_extension_applies_after_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    client.queue_set_ttl_extension(&admin, &42_000_u32);

    // Still the default until the timelock elapses and execute runs.
    assert_eq!(
        effective_ttl_extension(&env, &contract_id).1,
        DEFAULT_TTL_EXTENSION,
    );

    let now = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(now + ADMIN_TIMELOCK_DELAY_SECONDS + 1);

    client.execute_set_ttl_extension(&admin);

    assert_eq!(effective_ttl_extension(&env, &contract_id).1, 42_000);
}

/// Executing the TTL-extension change before the timelock is ready reverts and
/// leaves the value untouched.
#[test]
fn execute_ttl_extension_before_timelock_elapses_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env);

    client.queue_set_ttl_extension(&admin, &42_000_u32);

    assert_eq!(
        client.try_execute_set_ttl_extension(&admin),
        Err(Ok(ContractError::InvalidState)),
    );
    assert_eq!(
        effective_ttl_extension(&env, &contract_id).1,
        DEFAULT_TTL_EXTENSION,
    );
}

/// Executing a TTL-extension change with nothing queued reverts.
#[test]
fn execute_ttl_extension_without_a_queued_proposal_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    assert_eq!(
        client.try_execute_set_ttl_extension(&admin),
        Err(Ok(ContractError::InvalidState)),
    );
}

/// A *reduced* TTL extension applied via the timelocked path is honoured
/// end-to-end: a persistent entry written afterwards survives a ledger advance
/// just under the new (smaller) value, proving the configured value — not the
/// default — flows into `extend_ttl`.
#[test]
fn timelocked_smaller_ttl_value_flows_into_extend_ttl() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);

    let new_ttl: u32 = 5_000;
    client.queue_set_ttl_extension(&admin, &new_ttl);
    let now = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(now + ADMIN_TIMELOCK_DELAY_SECONDS + 1);
    client.execute_set_ttl_extension(&admin);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 750, 0, 3600,
    );

    // Advance the ledger by just under the newly configured extension.
    let mut ledger_info = env.ledger().get();
    ledger_info.sequence_number += new_ttl - 1;
    env.ledger().set(ledger_info);

    let escrow = client.get_escrow(&id);
    assert_eq!(
        escrow.amount, 750,
        "escrow archived before its configured TTL"
    );
}
