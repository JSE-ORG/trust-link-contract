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
    contract_id: Address,
    admin: Address,
    seller: Address,
    buyer: Address,
    escrow_id: u64,
}

impl Fixture {
    fn client(&self) -> EscrowClient<'_> {
        EscrowClient::new(&self.env, &self.contract_id)
    }
}

fn fixture() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let env2 = env.clone();
    let (contract_id, client, admin, _fee_collector) = setup_contract(&env2);
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
        contract_id,
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

    f.client().post_message(&f.escrow_id, &f.buyer, &content);

    let messages = f.client().get_messages(&f.escrow_id, &0, &10);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages.get(0).unwrap().sender, f.buyer);
    assert_eq!(messages.get(0).unwrap().timestamp, 42);
    assert_eq!(messages.get(0).unwrap().content, content);
}

#[test]
fn seller_can_post_a_message() {
    let f = fixture();
    let content = String::from_str(&f.env, "It ships tomorrow.");

    f.client().post_message(&f.escrow_id, &f.seller, &content);

    assert_eq!(
        f.client()
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
    f.client().post_message(&f.escrow_id, &f.buyer, &first);
    f.client().post_message(&f.escrow_id, &f.seller, &second);

    let messages = f.client().get_messages(&f.escrow_id, &0, &10);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages.get(0).unwrap().content, first);
    assert_eq!(messages.get(1).unwrap().content, second);
}

#[test]
fn non_participant_cannot_post_a_message() {
    let f = fixture();
    let stranger = Address::generate(&f.env);
    let result = f.client().try_post_message(
        &f.escrow_id,
        &stranger,
        &String::from_str(&f.env, "Let me in"),
    );

    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
    assert_eq!(f.client().get_messages(&f.escrow_id, &0, &10).len(), 0);
}

#[test]
fn post_message_rejects_empty_content() {
    let f = fixture();
    let result = f
        .client()
        .try_post_message(&f.escrow_id, &f.buyer, &String::from_str(&f.env, ""));

    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
}

#[test]
fn post_message_rejects_content_over_the_maximum_length() {
    let f = fixture();
    let content = String::from_str(&f.env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    assert_eq!(
        f.client()
            .try_post_message(&f.escrow_id, &f.buyer, &content),
        Err(Ok(ContractError::InputTooLong))
    );
}

#[test]
fn get_messages_paginates_and_returns_empty_after_the_end() {
    let f = fixture();
    for text in ["one", "two", "three"] {
        f.client()
            .post_message(&f.escrow_id, &f.buyer, &String::from_str(&f.env, text));
    }

    let page = f.client().get_messages(&f.escrow_id, &1, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(
        page.get(0).unwrap().content,
        String::from_str(&f.env, "two")
    );
    assert_eq!(f.client().get_messages(&f.escrow_id, &3, &1).len(), 0);
}

#[test]
fn posting_to_an_unknown_escrow_is_rejected() {
    let f = fixture();
    assert_eq!(
        f.client()
            .try_post_message(&999_u64, &f.buyer, &String::from_str(&f.env, "Hello"),),
        Err(Ok(ContractError::EscrowNotFound))
    );
}

#[test]
fn posting_is_blocked_while_the_contract_is_paused() {
    let f = fixture();
    f.client().pause_contract(&f.admin);

    assert_eq!(
        f.client()
            .try_post_message(&f.escrow_id, &f.buyer, &String::from_str(&f.env, "Hello"),),
        Err(Ok(ContractError::ContractPaused))
    );
}

#[test]
fn post_message_enforces_cap() {
    let f = fixture();
    let content = String::from_str(&f.env, "A message");
    for _ in 0..100 {
        f.client().post_message(&f.escrow_id, &f.buyer, &content);
    }
    assert_eq!(
        f.client()
            .try_post_message(&f.escrow_id, &f.buyer, &content),
        Err(Ok(ContractError::TooManyMessages))
    );
}

/// Issue #827: get_messages for non-existent escrow returns empty Vec,
/// same as valid escrow with no messages. Validate that both cases return
/// empty Vec but can be distinguished by checking escrow existence separately.
#[test]
fn get_messages_returns_empty_for_nonexistent_escrow() {
    let f = fixture();

    // Non-existent escrow returns empty Vec
    let messages_missing = f.client().get_messages(&999_u64, &0, &10);
    assert_eq!(messages_missing.len(), 0);

    // Valid escrow with no messages also returns empty Vec
    let messages_no_msgs = f.client().get_messages(&f.escrow_id, &0, &10);
    assert_eq!(messages_no_msgs.len(), 0);

    // Callers can distinguish by checking escrow existence
    assert!(f.client().try_get_escrow(&999_u64).is_err());
    assert!(f.client().try_get_escrow(&f.escrow_id).is_ok());
}
