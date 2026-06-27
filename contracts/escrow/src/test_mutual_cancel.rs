#![cfg(test)]
//! Tests for `mutual_cancel` (#438): a no-dispute refund path for a funded but
//! unshipped escrow that requires both the seller and the buyer to sign.

use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, String as SorobanString,
};

use crate::test_helpers::{create_funded_escrow, setup_contract};
use crate::{ContractError, EscrowState};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract(token_admin)
}

/// Happy path: with both parties signing, the full amount is refunded to the
/// buyer and the escrow ends in `Canceled`.
#[test]
fn test_mutual_cancel_refunds_buyer_and_sets_canceled() {
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

    // Funds are locked in the contract; the buyer holds nothing.
    let tc = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(tc.balance(&buyer), 0);

    client.mutual_cancel(&id);

    // The full amount is returned to the buyer and the escrow is Canceled.
    assert_eq!(tc.balance(&buyer), 1000);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Canceled);
}

/// The seller's signature alone is not enough — the buyer must also sign.
#[test]
fn test_mutual_cancel_requires_buyer_signature() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );

    // Authorize only the seller for the mutual_cancel call.
    env.mock_auths(&[MockAuth {
        address: &seller,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "mutual_cancel",
            args: (id,).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    assert!(client.try_mutual_cancel(&id).is_err());
    // No funds moved and the escrow is untouched.
    assert_eq!(soroban_sdk::token::Client::new(&env, &token).balance(&buyer), 0);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);
}

/// The buyer's signature alone is not enough — the seller must also sign.
#[test]
fn test_mutual_cancel_requires_seller_signature() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1000, 0, 3600,
    );

    // Authorize only the buyer for the mutual_cancel call.
    env.mock_auths(&[MockAuth {
        address: &buyer,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "mutual_cancel",
            args: (id,).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    assert!(client.try_mutual_cancel(&id).is_err());
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);
}

/// A shipped escrow can no longer be mutually cancelled; the dispute/resolution
/// flow governs it from there.
#[test]
fn test_mutual_cancel_rejected_after_shipping() {
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

    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-001"));

    let res = client.try_mutual_cancel(&id);
    assert!(matches!(res, Err(Ok(ContractError::InvalidState))));
    assert_eq!(client.get_escrow(&id).state, EscrowState::Shipped);
}
