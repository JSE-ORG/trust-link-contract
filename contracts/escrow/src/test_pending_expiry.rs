#![cfg(test)]
//! Tests for pending-escrow expiration (#373).
//!
//! Acceptance criteria:
//!  - `fund_escrow` post-expiry fails with `EscrowExpired`
//!  - `auto_cancel_pending` transitions the escrow to `Canceled`

use crate::{ContractError, Escrow, EscrowClient, EscrowState};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env,
};

const PENDING_EXPIRY_WINDOW: u64 = 604_800; // 7 days, must match lib.rs

fn setup(env: &Env) -> (EscrowClient<'static>, Address, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let fee_collector = Address::generate(env);
    let token_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    token::StellarAssetClient::new(env, &token_addr).mint(&buyer, &10_000_i128);

    (client, seller, buyer, resolver, fee_collector, token_addr)
}

#[test]
fn fund_escrow_before_expiry_succeeds() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &0_u64,
    );

    // Fund one second before the window closes — should succeed.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW - 1;
    });

    client.fund_escrow(&escrow_id, &buyer);

    use crate::{DataKey, EscrowData};
    let data: EscrowData = env
        .as_contract(&client.address, || {
            env.storage().persistent().get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow exists");
    assert_eq!(data.state, EscrowState::Funded);
}

#[test]
fn fund_escrow_after_expiry_returns_escrow_expired() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &0_u64,
    );

    // Advance past the expiry window.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW + 1;
    });

    let result = client.try_fund_escrow(&escrow_id, &buyer);
    assert_eq!(
        result,
        Err(Ok(ContractError::EscrowExpired)),
        "fund_escrow post-expiry must return EscrowExpired",
    );
}

#[test]
fn auto_cancel_pending_before_expiry_is_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, _buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &0_u64,
    );

    // One second before the expiry deadline — cancel must be rejected.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW - 1;
    });

    let result = client.try_auto_cancel_pending(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::ShippingWindowNotElapsed)),
        "auto_cancel_pending must be rejected before the expiry deadline",
    );
}

#[test]
fn auto_cancel_pending_after_expiry_transitions_to_canceled() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, _buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &0_u64,
    );

    // One second past the expiry window.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW + 1;
    });

    client.auto_cancel_pending(&escrow_id);

    use crate::{DataKey, EscrowData};
    let data: EscrowData = env
        .as_contract(&client.address, || {
            env.storage().persistent().get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow exists");
    assert_eq!(
        data.state,
        EscrowState::Canceled,
        "auto_cancel_pending must transition state to Canceled",
    );
}

#[test]
fn auto_cancel_pending_on_funded_escrow_returns_invalid_state() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &0_u64,
    );

    client.fund_escrow(&escrow_id, &buyer);

    // Advance past expiry — but the escrow is already Funded, not Pending.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW + 1;
    });

    let result = client.try_auto_cancel_pending(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::InvalidState)),
        "auto_cancel_pending on a non-Pending escrow must return InvalidState",
    );
}
