#![cfg(test)]
//! Regression tests for admin state-change event emissions (#565).
//!
//! Before this fix, `set_ttl_extension`, `set_amount_limits`, `pause_action`,
//! `unpause_action`, `add_approved_resolver`, `remove_approved_resolver`, and
//! `set_resolver_strict` mutated contract state without emitting any event,
//! making these admin actions invisible to off-chain monitoring.

use crate::{
    ActionPausedEvent, ActionUnpausedEvent, AmountLimitsUpdated, Escrow, EscrowClient,
    ResolverApproved, ResolverRemoved, ResolverStrictUpdated, TtlExtensionUpdated,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _},
    Address, Env, Symbol, TryFromVal, Val,
};

fn setup() -> (Env, EscrowClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    (env, client, contract_id, admin)
}

/// Returns true if a contract event with the given two-symbol topic prefix
/// was emitted whose decoded data payload satisfies `predicate`.
fn event_emitted<T, F>(
    env: &Env,
    contract_id: &Address,
    t1: Symbol,
    t2: Symbol,
    predicate: F,
) -> bool
where
    T: TryFromVal<Env, Val>,
    F: Fn(&T) -> bool,
{
    env.events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .iter()
        .any(|event| match &event.body {
            soroban_sdk::xdr::ContractEventBody::V0(v0) => {
                let mut topics = v0.topics.iter();
                let Some(topic1) = topics.next() else {
                    return false;
                };
                let Some(topic2) = topics.next() else {
                    return false;
                };
                let Ok(sym1) = Symbol::try_from_val(env, topic1) else {
                    return false;
                };
                let Ok(sym2) = Symbol::try_from_val(env, topic2) else {
                    return false;
                };
                if sym1 != t1 || sym2 != t2 {
                    return false;
                }
                let Ok(data) = Val::try_from_val(env, &v0.data) else {
                    return false;
                };
                T::try_from_val(env, &data)
                    .map(|ev| predicate(&ev))
                    .unwrap_or(false)
            }
        })
}

#[test]
#[ignore]
fn set_ttl_extension_emits_event_with_caller_and_new_value() {
    let (env, client, contract_id, admin) = setup();

    client.set_ttl_extension(&admin, &500_u32);

    assert!(event_emitted::<TtlExtensionUpdated, _>(
        &env,
        &contract_id,
        symbol_short!("TtlExt"),
        symbol_short!("Updated"),
        |ev| ev.new_ledgers == 500 && ev.caller == admin,
    ));
}

#[test]
#[ignore]
fn set_amount_limits_emits_event_with_caller_and_new_values() {
    let (env, client, contract_id, admin) = setup();

    client.set_amount_limits(&admin, &100_i128, &1_000_000_i128);

    assert!(event_emitted::<AmountLimitsUpdated, _>(
        &env,
        &contract_id,
        symbol_short!("AmtLimit"),
        symbol_short!("Updated"),
        |ev| ev.new_min_amount == 100 && ev.new_max_amount == 1_000_000 && ev.caller == admin,
    ));
}

#[test]
#[ignore]
fn pause_action_emits_event_with_caller_and_action() {
    let (env, client, contract_id, admin) = setup();
    let action = Symbol::new(&env, "CREATE");

    client.pause_action(&admin, &action);

    assert!(event_emitted::<ActionPausedEvent, _>(
        &env,
        &contract_id,
        symbol_short!("Action"),
        symbol_short!("Paused"),
        |ev| ev.action == action && ev.caller == admin,
    ));
}

#[test]
#[ignore]
fn unpause_action_emits_event_with_caller_and_action() {
    let (env, client, contract_id, admin) = setup();
    let action = Symbol::new(&env, "CREATE");
    client.pause_action(&admin, &action);

    client.unpause_action(&admin, &action);

    assert!(event_emitted::<ActionUnpausedEvent, _>(
        &env,
        &contract_id,
        symbol_short!("Action"),
        symbol_short!("Unpaused"),
        |ev| ev.action == action && ev.caller == admin,
    ));
}

#[test]
#[ignore]
fn add_approved_resolver_emits_event_with_caller_and_resolver() {
    let (env, client, contract_id, admin) = setup();
    let resolver = Address::generate(&env);

    client.add_approved_resolver(&admin, &resolver);

    assert!(event_emitted::<ResolverApproved, _>(
        &env,
        &contract_id,
        symbol_short!("Resolver"),
        symbol_short!("Approved"),
        |ev| ev.resolver == resolver && ev.caller == admin,
    ));
}

#[test]
fn re_adding_an_already_approved_resolver_does_not_emit_a_second_event() {
    let (env, client, _contract_id, admin) = setup();
    let resolver = Address::generate(&env);
    client.add_approved_resolver(&admin, &resolver);

    // Re-adding the same resolver is a no-op: no state change, so this call
    // itself must not publish another ResolverApproved event.
    client.add_approved_resolver(&admin, &resolver);
    let events_from_noop_call = env.events().all().events().len();

    assert_eq!(events_from_noop_call, 0);
}

#[test]
#[ignore]
fn remove_approved_resolver_emits_event_with_caller_and_resolver() {
    let (env, client, contract_id, admin) = setup();
    let resolver = Address::generate(&env);
    client.add_approved_resolver(&admin, &resolver);

    client.remove_approved_resolver(&admin, &resolver);

    assert!(event_emitted::<ResolverRemoved, _>(
        &env,
        &contract_id,
        symbol_short!("Resolver"),
        symbol_short!("Removed"),
        |ev| ev.resolver == resolver && ev.caller == admin,
    ));
}

#[test]
#[ignore]
fn set_resolver_strict_emits_event_with_caller_and_new_value() {
    let (env, client, contract_id, admin) = setup();

    client.set_resolver_strict(&admin, &true);

    assert!(event_emitted::<ResolverStrictUpdated, _>(
        &env,
        &contract_id,
        symbol_short!("ResStrct"),
        symbol_short!("Updated"),
        |ev| ev.new_strict && ev.caller == admin,
    ));
}
