#![cfg(test)]

use crate::{ContractError, Escrow, EscrowClient, EscrowState, Payee};
use soroban_sdk::{
    testutils::Address as _, token, Address, Env, IntoVal, String as SorobanString, Vec,
};

fn setup(env: &Env) -> (EscrowClient<'static>, Address, Address, Address, Address) {
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let resolver = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    let fee_collector = Address::generate(env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    (client, seller, resolver, token, admin)
}

fn single_payee(env: &Env, address: &Address) -> Vec<Payee> {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}

/// Creates and funds an escrow in the `Funded` state using `token`.
fn create_funded(
    env: &Env,
    client: &EscrowClient<'static>,
    seller: &Address,
    resolver: &Address,
    token: &Address,
    buyer: &Address,
) -> u64 {
    let amount = 1_000_i128;
    token::StellarAssetClient::new(env, token).mint(buyer, &amount);
    let payees_val = single_payee(env, seller).into_val(env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        resolver,
        token,
        &amount,
        &0_u32,
        &3_600_u64,
    );
    client.fund_escrow(&escrow_id, buyer);
    escrow_id
}

/// A seller may mark an escrow as shipped from the `RefundRequested` state,
/// overriding an outstanding buyer refund request (issue #730).
#[test]
fn mark_shipped_overrides_refund_request() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, token, _admin) = setup(&env);
    let buyer = Address::generate(&env);
    let escrow_id = create_funded(&env, &client, &seller, &resolver, &token, &buyer);

    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Funded);

    client.request_refund(&buyer, &escrow_id);
    assert_eq!(
        client.get_escrow(&escrow_id).state,
        EscrowState::RefundRequested
    );

    client.mark_shipped(
        &seller,
        &escrow_id,
        &SorobanString::from_str(&env, "TRACK-OVERRIDE-001"),
    );
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Shipped);

    // The override cancelled the refund: funds were never returned to the buyer.
    assert_eq!(
        token::StellarAssetClient::new(&env, &token).balance(&buyer),
        0_i128
    );
}

/// `mark_shipped` still rejects a `Pending` escrow that was never funded.
#[test]
fn mark_shipped_rejects_pending_even_after_refund_request_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, token, _admin) = setup(&env);
    let buyer = Address::generate(&env);

    let amount = 1_000_i128;
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &amount);
    let payees_val = single_payee(&env, &seller).into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &0_u32,
        &3_600_u64,
    );
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Pending);

    let result = client.try_mark_shipped(
        &seller,
        &escrow_id,
        &SorobanString::from_str(&env, "TRACK-002"),
    );
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}

/// Only the primary payee (`payees[0]`) may approve a pending refund request.
/// A secondary payee must be rejected with `NotAuthorized`, and the refund
/// must not go through as a side effect of the rejected call (issue #953).
#[test]
fn secondary_payee_cannot_approve_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, primary_payee, resolver, token, _admin) = setup(&env);
    let secondary_payee = Address::generate(&env);
    let buyer = Address::generate(&env);

    let amount = 1_000_i128;
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &amount);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: primary_payee.clone(),
        bps: 6_000,
    });
    payees.push_back(Payee {
        address: secondary_payee.clone(),
        bps: 4_000,
    });
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &0_u32,
        &3_600_u64,
    );
    client.fund_escrow(&escrow_id, &buyer);

    client.request_refund(&buyer, &escrow_id);
    assert_eq!(
        client.get_escrow(&escrow_id).state,
        EscrowState::RefundRequested
    );

    let result = client.try_approve_refund(&secondary_payee, &escrow_id);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));

    // The rejected call had no side effects: still awaiting approval, and no
    // funds moved back to the buyer.
    assert_eq!(
        client.get_escrow(&escrow_id).state,
        EscrowState::RefundRequested
    );
    assert_eq!(
        token::StellarAssetClient::new(&env, &token).balance(&buyer),
        0_i128
    );
}
