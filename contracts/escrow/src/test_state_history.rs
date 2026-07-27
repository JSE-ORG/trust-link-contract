#![cfg(test)]

use crate::{ContractError, Escrow, EscrowClient, EscrowState, Payee};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, IntoVal, String as SorobanString,
};

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

fn single_payee(env: &Env, address: &Address) -> soroban_sdk::Vec<Payee> {
    let mut payees = soroban_sdk::Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}

#[test]
fn state_history_records_refund_transitions_with_timestamps() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, token, _admin) = setup(&env);
    let buyer = Address::generate(&env);
    let amount = 1_000_i128;
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &amount);

    env.ledger().set_timestamp(100);
    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &0_u32,
        &3_600_u64,
    );

    env.ledger().set_timestamp(200);
    client.fund_escrow(&escrow_id, &buyer);

    env.ledger().set_timestamp(300);
    client.request_refund(&buyer, &escrow_id);

    env.ledger().set_timestamp(400);
    client.approve_refund(&seller, &escrow_id);

    let history = client.get_state_history(&escrow_id);
    assert_eq!(history.len(), 4);
    assert_eq!(history.get(0).unwrap(), (EscrowState::Pending, 100));
    assert_eq!(history.get(1).unwrap(), (EscrowState::Funded, 200));
    assert_eq!(history.get(2).unwrap(), (EscrowState::RefundRequested, 300));
    assert_eq!(history.get(3).unwrap(), (EscrowState::Refunded, 400));
}

#[test]
fn state_history_ignores_non_state_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, token, admin) = setup(&env);
    let buyer = Address::generate(&env);
    let amount = 1_000_i128;
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &amount);

    env.ledger().set_timestamp(1_000);
    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &0_u32,
        &3_600_u64,
    );

    env.ledger().set_timestamp(1_100);
    client.fund_escrow(&escrow_id, &buyer);

    env.ledger().set_timestamp(1_200);
    client.mark_shipped(
        &seller,
        &escrow_id,
        &SorobanString::from_str(&env, "TRACK-HISTORY-002"),
    );

    env.ledger().set_timestamp(1_300);
    client.record_delivery(&admin, &escrow_id);

    let history = client.get_state_history(&escrow_id);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap(), (EscrowState::Pending, 1_000));
    assert_eq!(history.get(1).unwrap(), (EscrowState::Funded, 1_100));
    assert_eq!(history.get(2).unwrap(), (EscrowState::Shipped, 1_200));
}

#[test]
fn state_history_rejects_unknown_escrow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _seller, _resolver, _token, _admin) = setup(&env);

    assert_eq!(
        client.try_get_state_history(&99_u64),
        Err(Ok(ContractError::EscrowNotFound))
    );
}

#[test]
fn state_history_prunes_oldest_entries_beyond_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, seller, resolver, token, _admin) = setup(&env);
    let buyer = Address::generate(&env);
    let amount = 1_000_i128;
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &amount);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &0_u32,
        &3_600_u64,
    );

    // Push far more than MAX_STATE_HISTORY_ENTRIES (50) transitions directly,
    // alternating between two states, to simulate a high-churn escrow.
    env.as_contract(&client.address, || {
        for i in 0..80u64 {
            let state = if i % 2 == 0 {
                EscrowState::Disputed
            } else {
                EscrowState::PendingFinalization
            };
            crate::internal::append_state_history(&env, escrow_id, &state);
        }
    });

    let history = client.get_state_history(&escrow_id);
    // 1 (creation) + 80 appended, capped at MAX_STATE_HISTORY_ENTRIES.
    assert_eq!(history.len(), crate::MAX_STATE_HISTORY_ENTRIES);
    // Oldest entries (including the initial Pending) must have been pruned.
    let oldest = history.get(0).unwrap();
    assert_ne!(oldest.0, EscrowState::Pending);
    // The most recent entry reflects the final appended transition.
    let newest = history.get(history.len() - 1).unwrap();
    assert_eq!(newest.0, EscrowState::PendingFinalization);
}
