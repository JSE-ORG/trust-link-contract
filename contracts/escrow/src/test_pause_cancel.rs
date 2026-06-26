#![cfg(test)]

use crate::{ContractError, EscrowClient};
use soroban_sdk::{testutils::{Address as _, Ledger as _}, token, Address, Env, String as SorobanString, Symbol};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let contract_id = env.register(crate::Escrow, ());
    {
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
    }
    (env, admin, seller, buyer, resolver, token_address, contract_id)
}

#[test]
fn test_cancel_escrow_blocked_when_paused() {
    let (env, admin, seller, _buyer, _resolver, _token, contract_id) = setup_env();
    let client = EscrowClient::new(&env, &contract_id);
    // create escrow in pending state
    let id = client.create_escrow(&seller, &None::<Address>, &admin, &env.register_stellar_asset_contract(admin.clone()), &100_i128, &0_u32, &3600_u64);
    // pause contract
    client.pause_contract(&admin);
    // attempt to cancel escrow should fail with ContractPaused
    let result = client.try_cancel_escrow(&seller, &id);
    assert!(matches!(result, Err(Ok(ContractError::ContractPaused))));
}
