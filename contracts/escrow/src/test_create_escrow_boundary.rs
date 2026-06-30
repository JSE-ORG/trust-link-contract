#![cfg(test)]
//! Boundary tests for `create_escrow` fee_bps (#26).
//!
//! Covers:
//! - fee_bps = 0 (accepted)
//! - fee_bps = 300 (MAX_ESCROW_FEE_BPS, accepted)
//! - fee_bps = 301 (rejected with FeeExceedsMax)

use crate::{ContractError, Escrow, EscrowClient, Payee};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (EscrowClient<'static>, Address, Address, Address, Address) {
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let resolver = Address::generate(env);
    let token = env.register_stellar_asset_contract(Address::generate(env));
    let fee_collector = Address::generate(env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    (client, seller, resolver, token, contract_id)
}

fn single_payee(env: &Env, address: &Address) -> soroban_sdk::Vec<Payee> {
    let mut payees = soroban_sdk::Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}

#[test]
fn test_create_escrow_fee_bps_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, token, _) = setup(&env);
    let payees = single_payee(&env, &seller);

    // fee_bps = 0 should be accepted
    let id = client.create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32, // fee_bps
        &0_u32, // resolver_fee_bps
        &3600_u64,
    );
    assert_eq!(id, 1);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.fee_bps, 0);
}

#[test]
fn test_create_escrow_fee_bps_max() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, token, _) = setup(&env);
    let payees = single_payee(&env, &seller);

    // fee_bps = 300 (MAX) should be accepted
    let id = client.create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &300_u32, // fee_bps (MAX)
        &0_u32, // resolver_fee_bps
        &3600_u64,
    );
    assert_eq!(id, 1);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.fee_bps, 300);
}

#[test]
fn test_create_escrow_fee_bps_above_max() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, token, _) = setup(&env);
    let payees = single_payee(&env, &seller);

    // fee_bps = 301 should be rejected
    let res = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &301_u32, // fee_bps (MAX + 1)
        &0_u32, // resolver_fee_bps
        &3600_u64,
    );

    assert_eq!(res, Err(Ok(ContractError::FeeExceedsMax)));
}
