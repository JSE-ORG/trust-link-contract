#![cfg(test)]
//! `set_fee_collector` begins a 2-step change: the pending address is stored
//! but the active `FeeCollector` is unchanged until `accept_fee_collector` is
//! called by the pending address.  This prevents accidental lockouts from typos
//! (#434 / 2-step accept requirement).

use crate::test_helpers::setup_contract;
use crate::{ContractError, DataKey};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn set_fee_collector_rejects_zero_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, fee_collector) = setup_contract(&env);

    let res = client.try_set_fee_collector(&crate::zero_address(&env));
    assert_eq!(
        res,
        Err(Ok(ContractError::InvalidAddress)),
        "set_fee_collector must reject the zero address with InvalidAddress",
    );

    // The active collector must be unchanged after the rejected call.
    let stored: Address = env
        .as_contract(&client.address, || {
            env.storage().instance().get(&DataKey::FeeCollector)
        })
        .expect("fee collector still set");
    assert_eq!(stored, fee_collector);
}

#[test]
fn set_fee_collector_stores_pending_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, old_collector) = setup_contract(&env);

    let new_collector = Address::generate(&env);
    client.set_fee_collector(&new_collector);

    // Active fee collector must NOT have changed yet.
    let active: Address = env
        .as_contract(&client.address, || {
            env.storage().instance().get(&DataKey::FeeCollector)
        })
        .expect("active fee collector set");
    assert_eq!(active, old_collector, "active collector must be unchanged before accept");

    // The pending address must be set.
    let pending: Address = env
        .as_contract(&client.address, || {
            env.storage().instance().get(&DataKey::PendingFeeCollector)
        })
        .expect("pending fee collector set");
    assert_eq!(pending, new_collector);
}

#[test]
fn accept_fee_collector_finalizes_change() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _old_collector) = setup_contract(&env);

    let new_collector = Address::generate(&env);
    client.set_fee_collector(&new_collector);
    client.accept_fee_collector(&new_collector);

    // Active fee collector must now be updated.
    let active: Address = env
        .as_contract(&client.address, || {
            env.storage().instance().get(&DataKey::FeeCollector)
        })
        .expect("fee collector set");
    assert_eq!(active, new_collector);

    // Pending slot must be cleared.
    let pending: Option<Address> = env.as_contract(&client.address, || {
        env.storage().instance().get(&DataKey::PendingFeeCollector)
    });
    assert!(pending.is_none(), "pending fee collector must be cleared after accept");
}

#[test]
fn accept_fee_collector_rejects_wrong_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _old_collector) = setup_contract(&env);

    let new_collector = Address::generate(&env);
    let impostor = Address::generate(&env);
    client.set_fee_collector(&new_collector);

    let res = client.try_accept_fee_collector(&impostor);
    assert_eq!(res, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn accept_fee_collector_rejects_when_no_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let caller = Address::generate(&env);
    let res = client.try_accept_fee_collector(&caller);
    assert_eq!(res, Err(Ok(ContractError::NoPendingFeeCollector)));
}

#[test]
fn set_fee_collector_rejects_the_admin_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (_contract_id, client, admin, fee_collector) = setup_contract(&env);

    let res = client.try_set_fee_collector(&admin);
    assert_eq!(
        res,
        Err(Ok(ContractError::InvalidAddress)),
        "set_fee_collector must reject the admin address with InvalidAddress",
    );

    // The active collector must be unchanged after the rejected call.
    let stored: Address = env
        .as_contract(&client.address, || {
            env.storage().instance().get(&DataKey::FeeCollector)
        })
        .expect("fee collector still set");
    assert_eq!(stored, fee_collector);
}
