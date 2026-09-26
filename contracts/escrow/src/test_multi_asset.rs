#![cfg(test)]

use crate::{Escrow, EscrowClient, EscrowState, Payee};
use soroban_sdk::{testutils::Address as _, token, Address, Env, IntoVal, Vec};

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

/// Funding an escrow with a different token than the one held by the buyer
/// must fail, confirming the per-escrow token is enforced (issue #729).
#[test]
fn fund_with_wrong_token_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, _token, _admin) = setup(&env);
    let buyer = Address::generate(&env);

    // The escrow funds in `escrow_token`, but the buyer only holds `other_token`.
    let escrow_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let other_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let amount = 1_000_i128;
    token::StellarAssetClient::new(&env, &other_token).mint(&buyer, &amount);

    let payees_val = single_payee(&env, &seller).into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &escrow_token,
        &amount,
        &0_u32,
        &3_600_u64,
    );

    let result = client.try_fund_escrow(&escrow_id, &buyer);
    assert!(result.is_err(), "funding with the wrong token must revert");

    // Escrow is untouched by the failed funding attempt.
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Pending);
}

#[test]
fn distribute_to_payees_rounds_dust_to_primary_payee_for_equal_shares() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let amount = 1_001_i128;
    token::StellarAssetClient::new(&env, &token).mint(&contract_id, &amount);

    let mut payees = Vec::new(&env);
    for _ in 0..10 {
        payees.push_back(Payee {
            address: Address::generate(&env),
            bps: 1_000,
        });
    }

    env.as_contract(&contract_id, || {
        crate::internal::distribute_to_payees(&env, &token, &payees, amount).unwrap();
    });

    let token_client = token::Client::new(&env, &token);
    for i in 0..payees.len() {
        let payee = payees.get(i).unwrap();
        let expected = if i == 0 { 101_i128 } else { 100_i128 };
        assert_eq!(token_client.balance(&payee.address), expected);
    }
    assert_eq!(token_client.balance(&contract_id), 0);
}
