#![cfg(test)]
//! Differential tests between the legacy monolithic storage layout and the
//! paged layout introduced by the storage migration (issue #1321).
//!
//! The contract supports both layouts:
//! - messages: `Messages(id)` vector vs `Message(id, idx)` + `MessageCount(id)`
//! - buyer index: `BuyerEscrowIndex(addr)` vs `BuyerEscrow(addr, page)`
//! - vendor index: `VendorEscrowIndex(addr)` vs `VendorEscrow(addr, page)`
//!
//! Each test writes the *same logical data* into both layouts (directly, so no
//! write path can mask a read-path difference) and asserts that the public
//! read APIs return identical results for every pagination window, including
//! boundaries and empty states.

use crate::storage;
use crate::types::Message;
use crate::{DataKey, Escrow, EscrowClient, Payee};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, IntoVal, String, Vec,
};

const T0: u64 = 1_700_000_000;

struct Fixture {
    env: Env,
    contract_id: Address,
    token: Address,
    seller: Address,
    resolver: Address,
}

impl Fixture {
    fn client(&self) -> EscrowClient<'_> {
        EscrowClient::new(&self.env, &self.contract_id)
    }
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);

    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    Fixture {
        env,
        contract_id,
        token,
        seller,
        resolver,
    }
}

fn create_escrow(f: &Fixture, buyer: &Address) -> u64 {
    let mut payees = Vec::new(&f.env);
    payees.push_back(Payee {
        address: f.seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&f.env);
    f.client().create_escrow(
        &payees_val,
        &Some(buyer.clone()),
        &f.resolver,
        &f.token,
        &1_000_i128,
        &0_u32,
        &0_u32,
        &3_600_u64,
        &None::<String>,
    )
}

fn make_message(env: &Env, sender: &Address, index: u32) -> Message {
    Message {
        sender: sender.clone(),
        timestamp: T0 + u64::from(index),
        content: String::from_str(env, "message"),
    }
}

fn write_legacy_messages(f: &Fixture, escrow_id: u64, messages: &Vec<Message>) {
    f.env.as_contract(&f.contract_id, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::Messages(escrow_id), messages);
    });
}

fn write_paged_messages(f: &Fixture, escrow_id: u64, messages: &Vec<Message>) {
    // Spread the writes over several invocations so the test host's local
    // ledger-entry footprint limit (50 writes) is not tripped by a 50-entry
    // thread.
    let mut index = 0;
    while index < messages.len() {
        let chunk_end = (index + 10).min(messages.len());
        f.env.as_contract(&f.contract_id, || {
            let mut i = index;
            while i < chunk_end {
                if let Some(message) = messages.get(i) {
                    f.env
                        .storage()
                        .persistent()
                        .set(&DataKey::Message(escrow_id, i), &message);
                }
                i += 1;
            }
        });
        index = chunk_end;
    }
    f.env.as_contract(&f.contract_id, || {
        f.env
            .storage()
            .persistent()
            .set(&DataKey::MessageCount(escrow_id), &messages.len());
    });
}

#[test]
fn messages_legacy_and_paged_read_paths_are_equivalent() {
    let f = setup();
    let buyer = Address::generate(&f.env);
    let legacy_id = create_escrow(&f, &buyer);
    let paged_id = create_escrow(&f, &buyer);

    let count = 50_u32;
    let mut messages = Vec::new(&f.env);
    for index in 0..count {
        messages.push_back(make_message(&f.env, &f.seller, index));
    }

    write_legacy_messages(&f, legacy_id, &messages);
    write_paged_messages(&f, paged_id, &messages);

    let windows = [
        (0_u64, 0_u64),
        (0, 1),
        (0, 10),
        (0, 25),
        (0, 50),
        (0, 200),
        (2, 3),
        (25, 10),
        (49, 10),
        (50, 10),
        (75, 10),
    ];

    for (start, limit) in windows {
        let legacy = f.client().get_messages(&legacy_id, &start, &limit);
        let paged = f.client().get_messages(&paged_id, &start, &limit);
        assert_eq!(
            legacy, paged,
            "message read mismatch at start={start} limit={limit}"
        );
    }
}

#[test]
fn messages_empty_threads_are_equivalent() {
    let f = setup();
    let buyer = Address::generate(&f.env);
    let legacy_id = create_escrow(&f, &buyer);
    let paged_id = create_escrow(&f, &buyer);

    // Neither layout has any entries.
    for (start, limit) in [(0_u64, 0_u64), (0, 10), (5, 10)] {
        let legacy = f.client().get_messages(&legacy_id, &start, &limit);
        let paged = f.client().get_messages(&paged_id, &start, &limit);
        assert_eq!(legacy, paged);
        assert_eq!(legacy.len(), 0);
    }
}

#[test]
fn buyer_index_legacy_and_paged_read_paths_are_equivalent() {
    let f = setup();
    let ids = create_id_list(&f, 50);

    let paged_buyer = Address::generate(&f.env);
    let legacy_buyer = Address::generate(&f.env);

    f.env.as_contract(&f.contract_id, || {
        storage::write_buyer_escrow_index(&f.env, &paged_buyer, &ids);
        f.env
            .storage()
            .persistent()
            .set(&DataKey::BuyerEscrowIndex(legacy_buyer.clone()), &ids);
    });

    let legacy = f.client().get_escrows_by_buyer(&legacy_buyer);
    let paged = f.client().get_escrows_by_buyer(&paged_buyer);
    assert_eq!(legacy, paged);
    assert_eq!(paged.len(), 50);
}

#[test]
fn buyer_index_pagination_boundaries_are_equivalent() {
    let f = setup();

    for count in [1_u32, 10, 25, 50] {
        let ids = create_id_list(&f, count);
        let paged_buyer = Address::generate(&f.env);
        let legacy_buyer = Address::generate(&f.env);

        f.env.as_contract(&f.contract_id, || {
            storage::write_buyer_escrow_index(&f.env, &paged_buyer, &ids);
            f.env
                .storage()
                .persistent()
                .set(&DataKey::BuyerEscrowIndex(legacy_buyer.clone()), &ids);
        });

        assert_eq!(
            f.client().get_escrows_by_buyer(&legacy_buyer),
            f.client().get_escrows_by_buyer(&paged_buyer),
            "buyer index mismatch with {count} entries"
        );
    }
}

#[test]
fn buyer_index_empty_state_is_equivalent() {
    let f = setup();
    let paged_buyer = Address::generate(&f.env);
    let legacy_buyer = Address::generate(&f.env);

    let legacy = f.client().get_escrows_by_buyer(&legacy_buyer);
    let paged = f.client().get_escrows_by_buyer(&paged_buyer);
    assert_eq!(legacy, paged);
    assert_eq!(legacy.len(), 0);
}

#[test]
fn vendor_index_legacy_and_paged_read_paths_are_equivalent() {
    let f = setup();
    let ids = create_id_list(&f, 50);

    let paged_vendor = Address::generate(&f.env);
    let legacy_vendor = Address::generate(&f.env);

    f.env.as_contract(&f.contract_id, || {
        storage::write_vendor_escrow_index(&f.env, &paged_vendor, &ids);
        f.env
            .storage()
            .persistent()
            .set(&DataKey::VendorEscrowIndex(legacy_vendor.clone()), &ids);
    });

    let windows = [
        (0_u64, 0_u64),
        (0, 1),
        (0, 10),
        (0, 25),
        (0, 50),
        (10, 10),
        (25, 50),
        (49, 5),
        (50, 10),
        (60, 10),
    ];

    for (start, limit) in windows {
        let legacy = f
            .client()
            .get_escrows_by_vendor(&legacy_vendor, &start, &limit);
        let paged = f
            .client()
            .get_escrows_by_vendor(&paged_vendor, &start, &limit);
        assert_eq!(
            legacy, paged,
            "vendor index mismatch at start={start} limit={limit}"
        );
    }

    // `get_escrows_by_seller` is an alias and must agree with both layouts.
    for vendor in [&legacy_vendor, &paged_vendor] {
        assert_eq!(
            f.client().get_escrows_by_vendor(vendor, &0, &10),
            f.client().get_escrows_by_seller(vendor, &0, &10)
        );
    }
}

#[test]
fn vendor_index_empty_state_is_equivalent() {
    let f = setup();
    let paged_vendor = Address::generate(&f.env);
    let legacy_vendor = Address::generate(&f.env);

    for (start, limit) in [(0_u64, 0_u64), (0, 10), (5, 10)] {
        let legacy = f
            .client()
            .get_escrows_by_vendor(&legacy_vendor, &start, &limit);
        let paged = f
            .client()
            .get_escrows_by_vendor(&paged_vendor, &start, &limit);
        assert_eq!(legacy, paged);
        assert_eq!(legacy.len(), 0);
    }
}

fn create_id_list(f: &Fixture, count: u32) -> Vec<u64> {
    let buyer = Address::generate(&f.env);
    let mut ids = Vec::new(&f.env);
    for _ in 0..count {
        ids.push_back(create_escrow(f, &buyer));
    }
    ids
}
