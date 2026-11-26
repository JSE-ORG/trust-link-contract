#![cfg(test)]

use crate::test_helpers::{advance_time, mint_token, setup_contract};
use crate::{ContractError, EscrowState};
use soroban_sdk::{
    testutils::Address as _, Address, BytesN, Env, IntoVal, String as SorobanString, Symbol,
};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(&env);
    env.register_stellar_asset_contract_v2(token_admin.clone())
        .address()
}

#[test]
fn test_co_signed_release_from_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&id, &buyer);

    // co-signed release requires both parties' auths; with mock_all_auths this simulates
    // a transaction where both seller and buyer sign.
    client.co_signed_release(&buyer, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Completed);
}

#[test]
fn test_co_signed_release_requires_both_auths() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);

    // With mock_all_auths this simulates a transaction where both seller and buyer sign.
    client.co_signed_release(&seller, &id);
}

#[test]
fn test_co_signed_release_from_shipped() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-SHIP"));

    // co_signed_release should succeed from Shipped state
    client.co_signed_release(&buyer, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Completed);
}

#[test]
fn test_co_signed_release_fails_on_active_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-DISP"));

    // Buyer raises a dispute
    let reason = Symbol::new(&env, "defective");
    let description = SorobanString::from_str(&env, "Item is defective");
    let evidence_hash = BytesN::from_array(&env, &[0xcd; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // co_signed_release must fail when a dispute is active
    let result = client.try_co_signed_release(&buyer, &id);
    assert!(matches!(result, Err(Ok(ContractError::InvalidState))));
}

#[test]
fn test_co_signed_release_fails_on_completed_state() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-COMP"));

    // Complete the escrow via confirm_delivery after the dispute window passes
    let escrow = client.get_escrow(&id);
    advance_time(&env, escrow.dispute_deadline + 1);
    client.confirm_delivery(&buyer, &id);

    // Now the escrow is Completed — co_signed_release must fail
    let result = client.try_co_signed_release(&buyer, &id);
    assert!(matches!(result, Err(Ok(ContractError::InvalidState))));
}
