#![cfg(test)]

//! Regression tests for the storage-pagination and batch-limit fixes:
//!
//! - Messages are stored one-per-key (`Message(escrow_id, index)`) so
//!   `get_messages` paginates with targeted reads rather than deserialising a
//!   single `Vec` on every page.
//! - Buyer/vendor escrow indexes are sharded into fixed-size pages instead of
//!   one unbounded `Vec` that can exceed the storage entry limit.
//! - `multicall` is bounded by `MAX_MULTICALL_BATCH_SIZE`.
//! - `MAX_BASKET_SIZE` is lowered to stay within the cross-contract transfer
//!   budget of a single Soroban transaction.

use crate::test_helpers::{create_funded_escrow, setup_contract};
use crate::types::Message;
use crate::{
    ContractCall, ContractError, DataKey, EscrowClient, Payee, ESCROW_INDEX_PAGE_SIZE,
    MAX_BASKET_SIZE, MAX_MULTICALL_BATCH_SIZE,
};
use soroban_sdk::{testutils::Address as _, Address, Env, IntoVal, String, Symbol, Vec};

fn register_token(env: &Env) -> Address {
    env.register_stellar_asset_contract_v2(Address::generate(env))
        .address()
}

fn single_payee(env: &Env, address: &Address) -> Vec<Payee> {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}

fn create_escrow(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    resolver: &Address,
    token: &Address,
    amount: i128,
) -> u64 {
    client.create_escrow_8(
        &single_payee(env, seller).into_val(env),
        &None::<Address>,
        resolver,
        token,
        &amount,
        &0_u32,
        &3_600_u64,
    )
}

fn has_key(env: &Env, contract_id: &Address, key: &DataKey) -> bool {
    env.as_contract(contract_id, || env.storage().persistent().has(key))
}

fn message_count(env: &Env, contract_id: &Address, escrow_id: u64) -> u32 {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .get(&DataKey::MessageCount(escrow_id))
            .unwrap_or(0)
    })
}

// ── Messages: one key per entry, targeted pagination ────────────────────────

#[test]
fn messages_are_stored_per_index_not_in_one_vec() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let escrow_id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 100, 0, 3_600,
    );

    for i in 0..3 {
        client.post_message(
            &escrow_id,
            &buyer,
            &String::from_str(
                &env,
                if i == 0 {
                    "a"
                } else if i == 1 {
                    "b"
                } else {
                    "c"
                },
            ),
        );
    }

    for index in 0..3u32 {
        assert!(
            has_key(&env, &contract_id, &DataKey::Message(escrow_id, index)),
            "each message must live at its own Message(escrow, index) key",
        );
    }
    assert!(
        !has_key(&env, &contract_id, &DataKey::Messages(escrow_id)),
        "the monolithic Messages(escrow) vector must no longer be written",
    );
    assert_eq!(message_count(&env, &contract_id, escrow_id), 3);
}

#[test]
fn messages_paginate_across_many_entries_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let escrow_id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 100, 0, 3_600,
    );

    let total = 25u32;
    for _ in 0..total {
        client.post_message(&escrow_id, &buyer, &String::from_str(&env, "msg"));
    }

    // Page through in chunks and verify every slice is present and ordered.
    let mut seen = 0u32;
    let mut start = 0u64;
    while seen < total {
        let page = client.get_messages(&escrow_id, &start, &10);
        assert!(page.len() <= 10);
        for m in page.iter() {
            assert_eq!(m.content, String::from_str(&env, "msg"));
        }
        seen += page.len();
        start += 10;
    }
    assert_eq!(seen, total);
    // Past the end returns empty.
    assert_eq!(
        client.get_messages(&escrow_id, &(total as u64), &10).len(),
        0
    );
}

#[test]
fn legacy_message_vector_is_migrated_on_next_post() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let escrow_id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 100, 0, 3_600,
    );

    // Reproduce a pre-paging deployment: one Vec under Messages(escrow).
    env.as_contract(&contract_id, || {
        let mut legacy: Vec<Message> = Vec::new(&env);
        for text in ["old-1", "old-2"] {
            legacy.push_back(Message {
                sender: buyer.clone(),
                timestamp: 1,
                content: String::from_str(&env, text),
            });
        }
        env.storage()
            .persistent()
            .set(&DataKey::Messages(escrow_id), &legacy);
    });

    // Reads still see the legacy thread.
    assert_eq!(client.get_messages(&escrow_id, &0, &10).len(), 2);

    // The next post migrates the legacy entries, then appends.
    client.post_message(&escrow_id, &buyer, &String::from_str(&env, "new"));

    assert_eq!(message_count(&env, &contract_id, escrow_id), 3);
    assert!(!has_key(&env, &contract_id, &DataKey::Messages(escrow_id)));
    assert!(has_key(&env, &contract_id, &DataKey::Message(escrow_id, 0)));
    assert!(has_key(&env, &contract_id, &DataKey::Message(escrow_id, 2)));

    let messages = client.get_messages(&escrow_id, &0, &10);
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages.get(0).unwrap().content,
        String::from_str(&env, "old-1")
    );
    assert_eq!(
        messages.get(2).unwrap().content,
        String::from_str(&env, "new")
    );
}

// ── multicall bound ─────────────────────────────────────────────────────────

fn multicall_calls(env: &Env, escrow_id: u64, count: u32) -> Vec<ContractCall> {
    let mut calls = Vec::new(env);
    for _ in 0..count {
        let mut args: Vec<soroban_sdk::Val> = Vec::new(env);
        args.push_back(escrow_id.into_val(env));
        calls.push_back(ContractCall {
            function: Symbol::new(env, "get_escrow"),
            args,
        });
    }
    calls
}

#[test]
fn multicall_accepts_exactly_the_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let escrow_id = create_escrow(&env, &client, &seller, &resolver, &token, 1_000);

    let results = client.multicall(&multicall_calls(&env, escrow_id, MAX_MULTICALL_BATCH_SIZE));
    assert_eq!(results.len(), MAX_MULTICALL_BATCH_SIZE);
}

#[test]
fn multicall_rejects_batches_over_the_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);
    let escrow_id = create_escrow(&env, &client, &seller, &resolver, &token, 1_000);

    let over = multicall_calls(&env, escrow_id, MAX_MULTICALL_BATCH_SIZE + 1);
    assert_eq!(
        client.try_multicall(&over),
        Err(Ok(ContractError::MulticallBatchTooLarge)),
    );
}

// ── MAX_BASKET_SIZE ─────────────────────────────────────────────────────────

#[test]
fn max_basket_size_is_safely_bounded() {
    assert!(
        (1..=5).contains(&MAX_BASKET_SIZE),
        "MAX_BASKET_SIZE must stay within the per-transaction cross-contract transfer budget",
    );
}

#[test]
fn basket_above_the_lowered_limit_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let count = MAX_BASKET_SIZE + 1;
    let mut tokens = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..count {
        tokens.push_back(register_token(&env));
        amounts.push_back(1_000_i128);
    }

    assert_eq!(
        client.try_create_basket_escrow(
            &seller,
            &Some(buyer),
            &resolver,
            &tokens,
            &amounts,
            &0_u32,
            &3_600_u64,
        ),
        Err(Ok(ContractError::BasketTokenMismatch)),
    );
}

// ── Paged vendor/buyer indexes ──────────────────────────────────────────────

#[test]
fn vendor_index_is_sharded_and_reads_back_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let vendor = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);

    let total = ESCROW_INDEX_PAGE_SIZE + 5;
    let mut expected = Vec::new(&env);
    for _ in 0..total {
        expected.push_back(create_escrow(
            &env, &client, &vendor, &resolver, &token, 1_000,
        ));
    }

    let got = client.get_escrows_by_vendor(&vendor);
    assert_eq!(got.len(), total);
    for i in 0..total {
        assert_eq!(got.get(i).unwrap(), expected.get(i).unwrap());
    }

    assert!(
        has_key(
            &env,
            &contract_id,
            &DataKey::VendorEscrow(vendor.clone(), 0)
        ),
        "first page must exist",
    );
    assert!(
        has_key(
            &env,
            &contract_id,
            &DataKey::VendorEscrow(vendor.clone(), 1)
        ),
        "second page must exist once the first page filled up",
    );
    assert!(
        !has_key(
            &env,
            &contract_id,
            &DataKey::VendorEscrowIndex(vendor.clone())
        ),
        "the monolithic VendorEscrowIndex must no longer be written",
    );
}

#[test]
fn buyer_index_is_sharded_and_reads_back_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);

    let total = ESCROW_INDEX_PAGE_SIZE + 3;
    let mut expected = Vec::new(&env);
    for _ in 0..total {
        expected.push_back(create_funded_escrow(
            &env, &client, &seller, &buyer, &resolver, &token, 1_000, 0, 3_600,
        ));
    }

    let got = client.get_escrows_by_buyer(&buyer);
    assert_eq!(got.len(), total);
    for i in 0..total {
        assert_eq!(got.get(i).unwrap(), expected.get(i).unwrap());
    }

    assert!(has_key(
        &env,
        &contract_id,
        &DataKey::BuyerEscrow(buyer.clone(), 0)
    ));
    assert!(has_key(
        &env,
        &contract_id,
        &DataKey::BuyerEscrow(buyer.clone(), 1)
    ));
    assert!(!has_key(
        &env,
        &contract_id,
        &DataKey::BuyerEscrowIndex(buyer.clone())
    ));
}

#[test]
fn legacy_vendor_index_is_migrated_on_next_write() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);
    let vendor = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = register_token(&env);

    // Reproduce a pre-paging deployment: one Vec under VendorEscrowIndex.
    env.as_contract(&contract_id, || {
        let mut legacy: Vec<u64> = Vec::new(&env);
        legacy.push_back(100);
        legacy.push_back(200);
        env.storage()
            .persistent()
            .set(&DataKey::VendorEscrowIndex(vendor.clone()), &legacy);
    });

    let new_id = create_escrow(&env, &client, &vendor, &resolver, &token, 1_000);

    let got = client.get_escrows_by_vendor(&vendor);
    assert_eq!(got.len(), 3);
    assert_eq!(got.get(0).unwrap(), 100);
    assert_eq!(got.get(1).unwrap(), 200);
    assert_eq!(got.get(2).unwrap(), new_id);
    assert!(!has_key(
        &env,
        &contract_id,
        &DataKey::VendorEscrowIndex(vendor.clone())
    ));
}
