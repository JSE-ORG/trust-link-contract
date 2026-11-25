#![cfg(test)]
//! `set_admin` rotates the admin key (#13). The new admin is allowed to call
//! admin-gated functions; the old admin is not.

use crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS;
use crate::{
    storage, ContractError, DataKey, Escrow, EscrowClient, TimelockCancelled, TimelockOperation,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, BytesN, Env, Symbol, TryFromVal, Val, Vec,
};

fn setup() -> (Env, EscrowClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    (env, client, admin)
}

#[test]
fn current_admin_can_rotate_to_a_new_admin() {
    let (env, client, _admin) = setup();
    let new_admin = Address::generate(&env);

    client.set_admin(&new_admin);

    // The stored admin is now the new one — confirm by reading storage.
    use crate::DataKey;
    let stored: Address = env
        .as_contract(&client.address, || {
            env.storage().instance().get(&DataKey::Admin)
        })
        .expect("admin is set");
    assert_eq!(stored, new_admin);
}

#[test]
fn new_admin_can_call_admin_functions_after_rotation() {
    let (env, client, _admin) = setup();
    let new_admin = Address::generate(&env);

    client.set_admin(&new_admin);

    // With all auths still mocked, the next admin-gated call succeeds because
    // `require_admin` resolves to the *new* admin and its auth is mocked.
    client.set_protocol_fee(&new_admin, &50_u32);

    let fee_config: crate::FeeConfig = env
        .as_contract(&client.address, || {
            Some(crate::storage::read_fee_config(&env))
        })
        .expect("fee config set");
    assert_eq!(fee_config.protocol_fee_bps, 50);
}

#[test]
fn old_admin_cannot_authorise_admin_functions_after_rotation() {
    let (env, client, _old_admin) = setup();
    let new_admin = Address::generate(&env);
    client.set_admin(&new_admin);

    // Strip the all-auths mock — admin-gated functions now require the *new*
    // admin's specific signature, which we have not provided.
    env.mock_auths(&[]);

    // Old-admin-era operations should now be rejected because the active
    // admin (new_admin) has not authorised this invocation.
    assert!(client.try_set_protocol_fee(&new_admin, &100_u32).is_err());
}

#[test]
#[should_panic]
fn rotation_requires_current_admin_authorisation() {
    let (env, client, _admin) = setup();
    let new_admin = Address::generate(&env);

    env.mock_auths(&[]);
    // Without admin auth, set_admin must panic (auth violation).
    client.set_admin(&new_admin);
}

/// Every operation discriminant currently in use. Timelock keys are
/// append-only, so this bound must track `TimelockOperation`'s last variant.
fn assert_no_timelock_proposals(env: &Env, contract_id: &Address) {
    env.as_contract(contract_id, || {
        for op in 1..=(TimelockOperation::SetTimelockDelay as u32) {
            assert!(
                storage::read_timelock_proposal(env, op).is_none(),
                "timelock proposal for operation {op} survived rotation"
            );
            let raw: Option<crate::TimelockProposal> =
                env.storage().instance().get(&DataKey::TimelockOp(op));
            assert!(raw.is_none(), "raw storage key for operation {op} survived");
        }
    });
}

/// Extracts `TimelockCancelled` payloads from the event log.
fn cancellation_events(env: &Env, contract_id: &Address) -> Vec<TimelockCancelled> {
    let mut found = Vec::new(env);
    for event in env
        .events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .iter()
    {
        {
            let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
            let mut topics = v0.topics.iter();
            let t1 = Symbol::try_from_val(env, topics.next().unwrap()).ok();
            let t2 = Symbol::try_from_val(env, topics.next().unwrap()).ok();
            if t1 != Some(symbol_short!("Timelock")) || t2 != Some(symbol_short!("Cancelled")) {
                continue;
            }
            let data = Val::try_from_val(env, &v0.data).ok();
            if let Some(cancellation) =
                data.and_then(|d| TimelockCancelled::try_from_val(env, &d).ok())
            {
                found.push_back(cancellation);
            }
        }
    }
    found
}

#[test]
fn rotation_purges_all_queued_timelock_proposals() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[7u8; 32]);

    // Queue a fee change and an upgrade by the outgoing admin.
    client.queue_set_platform_fee(&admin, &150_u32);
    client.queue_upgrade(&admin, &hash);
    env.as_contract(&client.address, || {
        assert!(storage::read_timelock_proposal(&env, 5).is_some()); // SetPlatformFee
        assert!(storage::read_timelock_proposal(&env, 2).is_some()); // Upgrade
    });

    // Test-only rotation helper must honour the same invariant as production.
    client.set_admin(&new_admin);

    // Read events before entering `as_contract` — each contract frame sees
    // only its own invocation's events in the SDK 26 test host.
    let cancellations = cancellation_events(&env, &client.address);
    assert_no_timelock_proposals(&env, &client.address);
    assert_eq!(cancellations.len(), 2);
    for cancellation in cancellations.iter() {
        assert_eq!(cancellation.proposer, admin);
        assert_eq!(cancellation.canceller, new_admin);
    }
}

#[test]
fn execute_set_admin_purges_queued_proposals() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[9u8; 32]);

    client.queue_set_platform_fee(&admin, &250_u32);
    client.queue_upgrade(&admin, &hash);
    client.queue_set_admin(&admin, &new_admin);

    env.ledger()
        .with_mut(|li| li.timestamp += ADMIN_TIMELOCK_DELAY_SECONDS);
    client.execute_set_admin(&admin, &new_admin);

    let cancellations = cancellation_events(&env, &client.address);
    assert_no_timelock_proposals(&env, &client.address);
    // The SetAdmin proposal was *executed*; only the other two are cancelled.
    assert_eq!(cancellations.len(), 2);
    for cancellation in cancellations.iter() {
        assert_eq!(cancellation.proposer, admin);
        assert_eq!(cancellation.canceller, new_admin);
    }

    // Previously queued changes can no longer be executed by anyone.
    assert!(client
        .try_execute_set_platform_fee(&new_admin, &250_u32)
        .is_err());
}

fn read_admin(env: &Env, contract_id: &Address) -> Option<Address> {
    env.as_contract(contract_id, || {
        env.storage().instance().get(&DataKey::Admin)
    })
}

fn read_pending_admin(env: &Env, contract_id: &Address) -> Option<Address> {
    env.as_contract(contract_id, || {
        env.storage().instance().get(&DataKey::PendingAdmin)
    })
}

#[test]
fn two_step_admin_transfer_keeps_old_admin_until_acceptance() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);

    client.set_pending_admin(&admin, &new_admin);

    // Proposal alone must not transfer ownership.
    assert_eq!(read_admin(&env, &client.address), Some(admin.clone()));
    assert_eq!(
        read_pending_admin(&env, &client.address),
        Some(new_admin.clone())
    );

    client.accept_admin(&new_admin);

    assert_eq!(read_admin(&env, &client.address), Some(new_admin.clone()));
    assert_eq!(read_pending_admin(&env, &client.address), None);

    // The incoming admin can act; the outgoing one can no longer.
    client.set_protocol_fee(&new_admin, &50_u32);
    env.mock_auths(&[]);
    assert!(client.try_set_protocol_fee(&admin, &100_u32).is_err());
}

#[test]
fn third_party_cannot_accept_admin_transfer() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);
    let impostor = Address::generate(&env);

    client.set_pending_admin(&admin, &new_admin);

    assert_eq!(
        client.try_accept_admin(&impostor),
        Err(Ok(ContractError::NotAuthorized))
    );
    // Ownership and the pending proposal are unchanged.
    assert_eq!(read_admin(&env, &client.address), Some(admin));
    assert_eq!(read_pending_admin(&env, &client.address), Some(new_admin));
}

#[test]
fn accept_admin_without_pending_transfer_is_not_authorized() {
    let (env, client, _admin) = setup();
    let caller = Address::generate(&env);

    assert_eq!(
        client.try_accept_admin(&caller),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn set_pending_admin_requires_the_current_admin() {
    let (env, client, _admin) = setup();
    let new_admin = Address::generate(&env);
    let impostor = Address::generate(&env);

    assert_eq!(
        client.try_set_pending_admin(&impostor, &new_admin),
        Err(Ok(ContractError::NotAuthorized))
    );
    assert_eq!(read_pending_admin(&env, &client.address), None);
}

#[test]
fn accept_admin_purges_queued_timelock_proposals() {
    let (env, client, admin) = setup();
    let new_admin = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[11u8; 32]);

    client.queue_set_platform_fee(&admin, &150_u32);
    client.queue_upgrade(&admin, &hash);
    client.set_pending_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    let cancellations = cancellation_events(&env, &client.address);
    assert_no_timelock_proposals(&env, &client.address);
    assert_eq!(cancellations.len(), 2);
    for cancellation in cancellations.iter() {
        assert_eq!(cancellation.proposer, admin);
        assert_eq!(cancellation.canceller, new_admin);
    }
}
