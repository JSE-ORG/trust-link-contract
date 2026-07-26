#![cfg(test)]
//! Boundary coverage for the appeal-window check in `finalize_dispute` (#570).
//!
//! `finalize_dispute` guards with `now < appeal_deadline` (where
//! `appeal_deadline = resolved_at + APPEAL_WINDOW`) and rejects with
//! `AppealWindowActive` while the window is still open. These tests pin the
//! exact three boundary ticks:
//! - `appeal_deadline - 1` — still inside the window, must be rejected
//! - `appeal_deadline`     — exactly at the deadline, must be allowed
//! - `appeal_deadline + 1` — past the deadline, must be allowed
//!
//! Because `appeal_deadline` is always `resolved_at + APPEAL_WINDOW`, it can
//! never be smaller than `APPEAL_WINDOW`, so the `- 1` tick can never
//! underflow — asserted explicitly below.

use crate::{ContractError, Escrow, EscrowClient, EscrowState, Payee, ResolutionType};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

const AMOUNT: i128 = 1_000;

/// Drives a single-resolver escrow to `PendingFinalization` with a decided
/// `Release` resolution, then returns the env, contract address, resolver, the
/// escrow id, and the computed `appeal_deadline`.
fn setup_pending_finalization() -> (Env, Address, Address, u64, u64) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &50_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_address,
        &AMOUNT,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
    token_admin_client.mint(&buyer, &AMOUNT);
    client.fund_escrow(&escrow_id, &buyer);

    // Raise + resolve the dispute (single resolver → threshold 1 → resolves
    // immediately and records `resolved_at`).
    let reason = Symbol::new(&env, "non_delivery");
    let description = String::from_str(&env, "Item never arrived");
    let evidence = BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &escrow_id, &reason, &description, &evidence);
    client.resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release);

    // Sanity: escrow is awaiting finalization.
    assert_eq!(
        client.get_escrow(&escrow_id).state,
        EscrowState::PendingFinalization,
    );

    let resolved_at = client
        .get_dispute(&escrow_id)
        .expect("dispute exists")
        .resolved_at;
    let appeal_deadline = resolved_at + crate::APPEAL_WINDOW;

    (env, contract_id, resolver, escrow_id, appeal_deadline)
}

#[test]
fn finalize_rejected_one_tick_before_deadline() {
    let (env, contract_id, resolver, escrow_id, appeal_deadline) = setup_pending_finalization();
    let client = EscrowClient::new(&env, &contract_id);

    // `appeal_deadline - 1` is always >= APPEAL_WINDOW - 1, so this tick can
    // never underflow the ledger clock.
    assert!(appeal_deadline >= crate::APPEAL_WINDOW);
    env.ledger().set_timestamp(appeal_deadline - 1);

    let res = client.try_finalize_dispute(&resolver, &escrow_id);
    assert_eq!(res, Err(Ok(ContractError::AppealWindowActive)));

    // Still awaiting finalization — nothing was released.
    assert_eq!(
        client.get_escrow(&escrow_id).state,
        EscrowState::PendingFinalization,
    );
}

#[test]
fn finalize_succeeds_exactly_at_deadline() {
    let (env, contract_id, resolver, escrow_id, appeal_deadline) = setup_pending_finalization();
    let client = EscrowClient::new(&env, &contract_id);

    // `now == appeal_deadline` fails the strict `now < appeal_deadline` guard,
    // so finalization is permitted at the exact boundary.
    env.ledger().set_timestamp(appeal_deadline);
    client.finalize_dispute(&resolver, &escrow_id);

    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Completed);
}

#[test]
fn finalize_succeeds_one_tick_after_deadline() {
    let (env, contract_id, resolver, escrow_id, appeal_deadline) = setup_pending_finalization();
    let client = EscrowClient::new(&env, &contract_id);

    env.ledger().set_timestamp(appeal_deadline + 1);
    client.finalize_dispute(&resolver, &escrow_id);

    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Completed);
}
