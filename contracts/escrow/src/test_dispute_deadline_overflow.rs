#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, IntoVal,
};

/// Pending escrow created with the ledger clock set to `timestamp`. Returns
/// `(env, client, token, buyer, escrow_id)`.
fn pending_escrow_at(timestamp: u64) -> (Env, EscrowClient<'static>, Address, Address, u64) {
    let env = Env::default();
    env.ledger().set_timestamp(timestamp);
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );

    (env, client, token, buyer, id)
}

// Ensure fund_escrow handles funded_at + DISPUTE_WINDOW overflow safely
#[test]
fn test_fund_escrow_dispute_deadline_overflow_is_handled() {
    // funded_at = u64::MAX - DISPUTE_WINDOW + 1
    let (env, client, token, buyer, id) = pending_escrow_at(u64::MAX - crate::DISPUTE_WINDOW + 1);

    // Mint tokens and attempt to fund - expect ArithmeticOverflow error, not panic
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &1000_i128);
    let res = client.try_fund_escrow(&id, &buyer);
    assert_eq!(res, Err(Ok(ContractError::ArithmeticOverflow)));
}

/// #1274: `funded_at` values that do not fit in the packed field's high 32
/// bits are rejected instead of silently truncated.
#[test]
fn test_fund_escrow_rejects_funded_at_above_u32_max() {
    let (env, client, token, buyer, id) = pending_escrow_at(u32::MAX as u64 + 1);

    token::StellarAssetClient::new(&env, &token).mint(&buyer, &1000_i128);
    let res = client.try_fund_escrow(&id, &buyer);
    assert_eq!(res, Err(Ok(ContractError::ArithmeticOverflow)));
}

/// #1274: `pack_timestamps` rejects either value above `u32::MAX`.
#[test]
fn test_pack_timestamps_rejects_values_above_u32_max() {
    let over = u32::MAX as u64 + 1;
    assert_eq!(
        EscrowData::pack_timestamps(over, 0),
        Err(ContractError::ArithmeticOverflow)
    );
    assert_eq!(
        EscrowData::pack_timestamps(0, over),
        Err(ContractError::ArithmeticOverflow)
    );
}

/// #1274: the largest representable values still pack and unpack correctly.
#[test]
fn test_pack_timestamps_accepts_u32_max_boundary() {
    let max = u32::MAX as u64;

    let (env, client, token, buyer, id) = pending_escrow_at(max - crate::DISPUTE_WINDOW);
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &1000_i128);
    client.fund_escrow(&id, &buyer);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.funded_at(), max - crate::DISPUTE_WINDOW);
    assert_eq!(escrow.dispute_deadline(), max);
}
