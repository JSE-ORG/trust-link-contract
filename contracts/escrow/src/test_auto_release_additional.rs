#![cfg(test)]

use crate::{Escrow, EscrowClient, ContractError};
use soroban_sdk::{testutils::{Address as _, Ledger as _}, token, Address, Env, String as SorobanString};
use crate::test_helpers::{setup_contract, mint_token};

// Test: calling auto_release when no delivery recorded should fail (InvalidState)
#[test]
fn auto_release_without_delivery_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount: i128 = 1000;
    let escrow_id = client.create_escrow(&seller, &resolver, &token_addr, &amount, &0_u32, &0_u64);
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &amount);
    client.fund_escrow(&escrow_id, &buyer);
    client.mark_shipped(&seller, &escrow_id, &SorobanString::from_str(&env, "TRACK-X"));

    // Do NOT call record_delivery. Now auto_release must reject with InvalidState.
    assert_eq!(
        client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::InvalidState)),
    );
}

// Test: calling auto_release before shipped (e.g., Funded state) should fail (InvalidState)
#[test]
fn auto_release_called_while_funded_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount: i128 = 500;
    let escrow_id = client.create_escrow(&seller, &resolver, &token_addr, &amount, &0_u32, &0_u64);
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &amount);
    client.fund_escrow(&escrow_id, &buyer);

    // Escrow is Funded but not Shipped - auto_release should reject with InvalidState.
    assert_eq!(
        client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::InvalidState)),
    );
}
