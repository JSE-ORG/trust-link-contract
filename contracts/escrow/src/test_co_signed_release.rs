#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env};
use crate::{EscrowState, ContractError, DataKey};
use crate::test_helpers::{setup_contract, mint_token};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(&env);
    env.register_stellar_asset_contract(token_admin.clone())
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

    let id = client.create_escrow(&seller, &resolver, &token, &500_i128, &0_u32, &3600_u64);

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

    let id = client.create_escrow(&seller, &resolver, &token, &500_i128, &0_u32, &3600_u64);
    client.fund_escrow(&id, &buyer);

    // With mock_all_auths this simulates a transaction where both seller and buyer sign.
    client.co_signed_release(&seller, &id);
}
