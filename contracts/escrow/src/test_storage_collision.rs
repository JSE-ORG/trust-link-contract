#![cfg(test)]
//! Storage collision tests between distinct escrow entries.
//!
//! Each test verifies that modifying one escrow's state — through a state
//! transition or field update — leaves all other escrow storage entries
//! completely unchanged.  The key invariant exercised here is that
//! `DataKey::Escrow(id)` produces independent persistent storage slots: a
//! write to slot N must never bleed into slot M where N ≠ M.

use crate::{Escrow, EscrowClient, EscrowState};
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env, String,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn setup() -> (Env, EscrowClient, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    (env, client, admin, fee_collector, seller, resolver, token)
}

/// Mint `amount` tokens to `to` and fund an existing escrow.
fn fund(env: &Env, client: &EscrowClient, token: &Address, buyer: &Address, escrow_id: &u64) {
    let escrow = client.get_escrow(escrow_id);
    token::StellarAssetClient::new(env, token).mint(buyer, &escrow.amount);
    client.fund_escrow(escrow_id, buyer);
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// Cancelling escrow 1 (Pending → Canceled) must not affect escrow 2.
#[test]
fn cancel_escrow1_does_not_affect_escrow2() {
    let (env, client, _admin, _fee_collector, seller, resolver, token) = setup();

    let id1 = client.create_escrow(
        &seller, &None::<Address>, &resolver, &token, &500_i128, &0_u32, &3600_u64,
    );
    let id2 = client.create_escrow(
        &seller, &None::<Address>, &resolver, &token, &750_i128, &0_u32, &3600_u64,
    );

    // Snapshot escrow 2 before the mutation.
    let before = client.get_escrow(&id2);

    // Mutate escrow 1.
    client.cancel_escrow(&seller, &id1);

    // Escrow 1 advanced to Canceled.
    assert_eq!(client.get_escrow(&id1).state, EscrowState::Canceled);

    // Escrow 2 is completely unchanged.
    let after = client.get_escrow(&id2);
    assert_eq!(after.state, before.state);
    assert_eq!(after.amount, before.amount);
    assert_eq!(after.seller, before.seller);
    assert_eq!(after.resolver, before.resolver);
    assert_eq!(after.token, before.token);
    assert_eq!(after.fee_bps, before.fee_bps);
    assert_eq!(after.shipping_window, before.shipping_window);
}

/// Funding escrow 2 (Pending → Funded) must not affect escrow 1.
#[test]
fn fund_escrow2_does_not_affect_escrow1() {
    let (env, client, _admin, _fee_collector, seller, resolver, token) = setup();

    let buyer = Address::generate(&env);

    let id1 = client.create_escrow(
        &seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &3600_u64,
    );
    let id2 = client.create_escrow(
        &seller, &Some(buyer.clone()), &resolver, &token, &200_i128, &0_u32, &3600_u64,
    );

    let before = client.get_escrow(&id1);

    fund(&env, &client, &token, &buyer, &id2);

    assert_eq!(client.get_escrow(&id2).state, EscrowState::Funded);

    let after = client.get_escrow(&id1);
    assert_eq!(after.state, before.state);
    assert_eq!(after.amount, before.amount);
    assert_eq!(after.funded_at, before.funded_at);
}

/// Marking escrow 1 as shipped must not modify escrow 2's tracking or state.
#[test]
fn mark_shipped_escrow1_does_not_affect_escrow2() {
    let (env, client, _admin, _fee_collector, seller, resolver, token) = setup();

    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);

    let id1 = client.create_escrow(
        &seller, &Some(buyer1.clone()), &resolver, &token, &300_i128, &0_u32, &0_u64,
    );
    let id2 = client.create_escrow(
        &seller, &Some(buyer2.clone()), &resolver, &token, &400_i128, &0_u32, &0_u64,
    );

    fund(&env, &client, &token, &buyer1, &id1);
    fund(&env, &client, &token, &buyer2, &id2);

    let before = client.get_escrow(&id2);

    client.mark_shipped(&seller, &id1, &String::from_str(&env, "TRK-ALPHA"));

    assert_eq!(client.get_escrow(&id1).state, EscrowState::Shipped);

    let after = client.get_escrow(&id2);
    assert_eq!(after.state, before.state);
    assert!(after.tracking_id.is_none(), "escrow 2 tracking_id should still be None");
    assert_eq!(after.shipped_at, 0, "escrow 2 shipped_at should still be zero");
}

/// Three escrows: modifying the middle one (escrow 2) must leave
/// escrow 1 and escrow 3 completely intact.
#[test]
fn modifying_middle_escrow_leaves_neighbors_unchanged() {
    let (_env, client, _admin, _fee_collector, seller, resolver, token) = setup();

    let id1 = client.create_escrow(
        &seller, &None::<Address>, &resolver, &token, &100_i128, &0_u32, &3600_u64,
    );
    let id2 = client.create_escrow(
        &seller, &None::<Address>, &resolver, &token, &200_i128, &0_u32, &3600_u64,
    );
    let id3 = client.create_escrow(
        &seller, &None::<Address>, &resolver, &token, &300_i128, &0_u32, &3600_u64,
    );

    let before1 = client.get_escrow(&id1);
    let before3 = client.get_escrow(&id3);

    // Cancel the middle escrow.
    client.cancel_escrow(&seller, &id2);
    assert_eq!(client.get_escrow(&id2).state, EscrowState::Canceled);

    // Neighbors are unmodified.
    let after1 = client.get_escrow(&id1);
    let after3 = client.get_escrow(&id3);

    assert_eq!(after1.state, before1.state);
    assert_eq!(after1.amount, before1.amount);
    assert_eq!(after3.state, before3.state);
    assert_eq!(after3.amount, before3.amount);
}

/// All fields written at creation time are stored in their own slot and are
/// not overwritten when a second escrow with different parameters is created.
#[test]
fn independent_escrows_store_correct_fields() {
    let (env, client, _admin, _fee_collector, seller, resolver, token) = setup();

    let seller2 = Address::generate(&env);
    let resolver2 = Address::generate(&env);
    let buyer2 = Address::generate(&env);

    // Escrow 1: open buyer, amount=111, fee=50, window=1800
    let id1 = client.create_escrow(
        &seller, &None::<Address>, &resolver, &token, &111_i128, &50_u32, &1800_u64,
    );

    // Escrow 2: locked buyer, amount=999, fee=100, window=7200
    let id2 = client.create_escrow(
        &seller2,
        &Some(buyer2.clone()),
        &resolver2,
        &token,
        &999_i128,
        &100_u32,
        &7200_u64,
    );

    let e1 = client.get_escrow(&id1);
    let e2 = client.get_escrow(&id2);

    // Escrow 1 fields are unaffected by escrow 2 creation.
    assert_eq!(e1.seller, seller);
    assert_eq!(e1.resolver, resolver);
    assert_eq!(e1.amount, 111);
    assert_eq!(e1.fee_bps, 50);
    assert_eq!(e1.shipping_window, 1800);
    assert!(e1.buyer.is_none());
    assert_eq!(e1.state, EscrowState::Pending);

    // Escrow 2 has its own distinct fields.
    assert_eq!(e2.seller, seller2);
    assert_eq!(e2.resolver, resolver2);
    assert_eq!(e2.amount, 999);
    assert_eq!(e2.fee_bps, 100);
    assert_eq!(e2.shipping_window, 7200);
    assert_eq!(e2.buyer, Some(buyer2));
    assert_eq!(e2.state, EscrowState::Pending);
}

/// Cancelling escrow 1 does not affect the funded_at timestamp that was
/// written to escrow 2 when it was funded.
#[test]
fn funded_at_of_escrow2_unchanged_after_cancelling_escrow1() {
    let (env, client, _admin, _fee_collector, seller, resolver, token) = setup();

    let buyer = Address::generate(&env);

    let id1 = client.create_escrow(
        &seller, &None::<Address>, &resolver, &token, &50_i128, &0_u32, &3600_u64,
    );
    let id2 = client.create_escrow(
        &seller, &Some(buyer.clone()), &resolver, &token, &50_i128, &0_u32, &3600_u64,
    );

    fund(&env, &client, &token, &buyer, &id2);

    let funded_at_before = client.get_escrow(&id2).funded_at;
    assert!(funded_at_before > 0, "funded_at should have been set");

    // Cancel unrelated escrow 1.
    client.cancel_escrow(&seller, &id1);

    // Escrow 2's funded_at is preserved.
    let funded_at_after = client.get_escrow(&id2).funded_at;
    assert_eq!(funded_at_after, funded_at_before);
    assert_eq!(client.get_escrow(&id2).state, EscrowState::Funded);
}
