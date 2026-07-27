#![cfg(test)]

//! Coverage for the on-chain buyer/seller messaging thread.

use crate::test_helpers::{create_funded_escrow, setup_contract};
use crate::{ContractError, EscrowClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, String,
};

struct Fixture {
    env: Env,
    client: EscrowClient<'static>,
    admin: Address,
    seller: Address,
    buyer: Address,
    escrow_id: u64,
}

fn fixture() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, _fee_collector) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let escrow_id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 100, 0, 3_600,
    );
    Fixture {
        env,
        client,
        admin,
        seller,
        buyer,
        escrow_id,
    }
}

#[test]
fn buyer_can_post_a_message_and_it_is_stored_with_its_timestamp() {
    let f = fixture();
    f.env.ledger().set_timestamp(42);
    let content = String::from_str(&f.env, "Where is my order?");

    f.client.post_message(&f.escrow_id, &f.buyer, &content);

    let messages = f.client.get_messages(&f.escrow_id, &0, &10);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages.get(0).unwrap().sender, f.buyer);
    assert_eq!(messages.get(0).unwrap().timestamp, 42);
    assert_eq!(messages.get(0).unwrap().content, content);
}

#[test]
fn seller_can_post_a_message() {
    let f = fixture();
    let content = String::from_str(&f.env, "It ships tomorrow.");

    f.client.post_message(&f.escrow_id, &f.seller, &content);

    assert_eq!(
        f.client
            .get_messages(&f.escrow_id, &0, &1)
            .get(0)
            .unwrap()
            .sender,
        f.seller
    );
}

#[test]
fn messages_preserve_posting_order() {
    let f = fixture();
    let first = String::from_str(&f.env, "First");
    let second = String::from_str(&f.env, "Second");
    f.client.post_message(&f.escrow_id, &f.buyer, &first);
    f.client.post_message(&f.escrow_id, &f.seller, &second);

    let messages = f.client.get_messages(&f.escrow_id, &0, &10);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages.get(0).unwrap().content, first);
    assert_eq!(messages.get(1).unwrap().content, second);
}

#[test]
fn non_participant_cannot_post_a_message() {
    let f = fixture();
    let stranger = Address::generate(&f.env);
    let result = f.client.try_post_message(
        &f.escrow_id,
        &stranger,
        &String::from_str(&f.env, "Let me in"),
    );

    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
    assert_eq!(f.client.get_messages(&f.escrow_id, &0, &10).len(), 0);
}

#[test]
fn post_message_rejects_empty_content() {
    let f = fixture();
    let result = f
        .client
        .try_post_message(&f.escrow_id, &f.buyer, &String::from_str(&f.env, ""));

    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
}

#[test]
fn post_message_rejects_content_over_the_maximum_length() {
    let f = fixture();
    let content = String::from_str(&f.env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    assert_eq!(
        f.client.try_post_message(&f.escrow_id, &f.buyer, &content),
        Err(Ok(ContractError::InputTooLong))
    );
}

#[test]
fn get_messages_paginates_and_returns_empty_after_the_end() {
    let f = fixture();
    for text in ["one", "two", "three"] {
        f.client
            .post_message(&f.escrow_id, &f.buyer, &String::from_str(&f.env, text));
    }

    let page = f.client.get_messages(&f.escrow_id, &1, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(
        page.get(0).unwrap().content,
        String::from_str(&f.env, "two")
    );
    assert_eq!(f.client.get_messages(&f.escrow_id, &3, &1).len(), 0);
}

#[test]
fn get_messages_caps_the_page_size_at_fifty() {
    let f = fixture();
    for _ in 0..51 {
        f.client
            .post_message(&f.escrow_id, &f.buyer, &String::from_str(&f.env, "message"));
    }

    assert_eq!(f.client.get_messages(&f.escrow_id, &0, &99).len(), 50);
}

#[test]
fn posting_to_an_unknown_escrow_is_rejected() {
    let f = fixture();
    assert_eq!(
        f.client
            .try_post_message(&999_u64, &f.buyer, &String::from_str(&f.env, "Hello"),),
        Err(Ok(ContractError::EscrowNotFound))
    );
}

#[test]
fn posting_is_blocked_while_the_contract_is_paused() {
    let f = fixture();
    f.client.pause_contract(&f.admin);

    assert_eq!(
        f.client
            .try_post_message(&f.escrow_id, &f.buyer, &String::from_str(&f.env, "Hello"),),
        Err(Ok(ContractError::ContractPaused))
    );
}
