#![cfg(test)]

use crate::{Payee, ContractError, EscrowState};
use crate::test_helpers::{setup_contract, mint_token};
use soroban_sdk::{testutils::Address as _, token, Address, Env};

#[test]
fn test_double_fund_reverts_with_invalid_state() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Mint enough for two potential fundings to detect double-deduction
    mint_token(&env, &token, &buyer, 200);

    let id = client.create_escrow(
            &single_payee(&env, &seller),
            &None::<Address>,
            &resolver,
            &token,
            &100_i128,
            &0_u32,
            &0_u32,
            &3600_u64
        );

    // First funding succeeds
    client.fund_escrow(&id, &buyer);
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);
    assert_eq!(token::Client::new(&env, &token).balance(&buyer), 100);

    // Second funding must revert with InvalidState
    let res = client.try_fund_escrow(&id, &buyer);
    assert!(matches!(res, Err(Ok(ContractError::InvalidState))));

    // Escrow remains Funded and buyer balance is unchanged (not deducted twice)
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);
    assert_eq!(token::Client::new(&env, &token).balance(&buyer), 100);
    assert_eq!(token::Client::new(&env, &token).balance(&contract_id), 100);
}

fn single_payee(env: &Env, address: &Address) -> soroban_sdk::Vec<Payee> {
    let mut payees = soroban_sdk::Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}
