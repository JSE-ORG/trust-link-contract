//! Comprehensive tests to ensure all mutable escrow functions are blocked when contract is paused.

#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger as _}, token, Address, Env, Symbol, String as SorobanString};

const DISPUTE_WINDOW: u64 = 172_800;

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract(token_admin.clone());

    let contract_id = env.register(Escrow, ());
    {
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
    }
    (env, admin, seller, buyer, resolver, token_address, fee_collector, contract_id)
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

#[test]
fn test_pause_blocks_all_mutations() {
    let (env, admin, seller, buyer, resolver, token, _fee_collector, contract_id) =
        setup_env();
    let client = EscrowClient::new(&env, &contract_id);

    // Pause the contract first
    client.pause_contract(&admin);

    // 1. try_create_escrow should fail
    let create_res = client.try_create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &36_00_u64,
    );
    assert!(matches!(create_res, Err(Ok(ContractError::ContractPaused))));

    // Need a valid escrow for subsequent tests; create without pause
    client.unpause_contract(&admin);
    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &100_i128,
        &0_u32,
        &36_00_u64,
    );
    client.pause_contract(&admin);

    // Mint tokens for buyer
    mint_tokens(&env, &token, &buyer, 100);

    // 2. try_fund_escrow should fail
    let fund_res = client.try_fund_escrow(&escrow_id, &buyer);
    assert!(matches!(fund_res, Err(Ok(ContractError::ContractPaused))));

    // 3. try_mark_shipped should fail
    let ship_res = client.try_mark_shipped(
        &seller,
        &escrow_id,
        &SorobanString::from_str(&env, "TRACK001"),
    );
    assert!(matches!(ship_res, Err(Ok(ContractError::ContractPaused))));

    // 4. try_confirm_delivery should fail
    env.ledger().set_timestamp(DISPUTE_WINDOW + 1);
    let confirm_res = client.try_confirm_delivery(&buyer, &escrow_id);
    assert!(matches!(confirm_res, Err(Ok(ContractError::ContractPaused))));

    // 5. try_raise_dispute should fail
    let hash = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
    let dispute_res = client.try_raise_dispute(
        &buyer,
        &escrow_id,
        &Symbol::new(&env, "fraud"),
        &SorobanString::from_str(&env, "desc"),
        &hash,
    );
    assert!(matches!(dispute_res, Err(Ok(ContractError::ContractPaused))));

    // 6. try_resolve_dispute should fail
    let resolve_res = client.try_resolve_dispute(&resolver, &escrow_id, &ResolutionType::Refund);
    assert!(matches!(resolve_res, Err(Ok(ContractError::ContractPaused))));

    // 7. try_auto_release should fail
    let auto_res = client.try_auto_release(&escrow_id);
    assert!(matches!(auto_res, Err(Ok(ContractError::ContractPaused))));

    // 8. try_withdraw_fees should fail
    let withdraw_res = client.try_withdraw_fees(&admin, &token, &admin, &1_i128);
    assert!(matches!(withdraw_res, Err(Ok(ContractError::ContractPaused))));

    // 9. try_cancel_escrow should fail (if exists)
    let cancel_res = client.try_cancel_escrow(&seller, &escrow_id);
    assert!(matches!(cancel_res, Err(Ok(ContractError::ContractPaused))));

    // Read‑only view should still work
    let _ = client.get_escrow(&escrow_id);
    let _ = client.get_fee_config();
    assert!(client.is_paused());
}
