#![cfg(test)]

use crate::test_helpers::{setup_contract, mint_token};
use crate::types::EscrowState;
use crate::{ContractError, EscrowClient};
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{token, Address, Env};

#[test]
fn test_refund_flow_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client, _, _) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(token_admin);
}
