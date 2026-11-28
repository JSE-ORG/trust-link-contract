#![cfg(test)]
//! Regression tests for admin state-change event emissions (#565).
//!
//! Before this fix, `set_ttl_extension`, `set_amount_limits`, `pause_action`,
//! `unpause_action`, `add_approved_resolver`, `remove_approved_resolver`, and
//! `set_resolver_strict` mutated contract state without emitting any event,
//! making these admin actions invisible to off-chain monitoring.

extern crate std;

use crate::{
    ActionPausedEvent, ActionUnpausedEvent, AmountLimitsUpdated, ContractPausedEvent,
    ContractUnpausedEvent, Escrow, EscrowClient, ResolverApproved, ResolverRemoved,
    ResolverStrictUpdated, TimelockExecuted, TimelockOperation, TimelockQueued,
    TtlExtensionUpdated,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
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

/// Decodes the payload of the first contract event published under the
/// `(Timelock, topic2)` topic tuple into `T`.
fn timelock_payload<T>(env: &Env, contract_id: &Address, topic2: Symbol) -> Option<T>
where
    T: TryFromVal<Env, Val>,
{
    env.events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .iter()
        .find_map(|event| {
            let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
            let mut topics = v0.topics.iter();
            let topic1 = topics.next()?;
            let found_topic2 = topics.next()?;
            if Symbol::try_from_val(env, topic1).ok()? != symbol_short!("Timelock") {
                return None;
            }
            if Symbol::try_from_val(env, found_topic2).ok()? != topic2 {
                return None;
            }
            let data = Val::try_from_val(env, &v0.data).ok()?;
            T::try_from_val(env, &data).ok()
        })
}

#[test]
fn set_ttl_extension_emits_event_with_caller_and_new_value() {
    let (env, client, contract_id, admin) = setup();

    client.set_ttl_extension(&admin, &2_000_u32);

    assert!(event_emitted::<TtlExtensionUpdated, _>(
        &env,
        &contract_id,
        symbol_short!("TtlExt"),
        symbol_short!("Updated"),
        |ev| ev.new_ledgers == 2_000 && ev.caller == admin,
    ));
}

#[test]
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

/// `pause_contract`/`unpause_contract` are test-only helpers (`#[cfg(any(test,
/// feature = "testutils"))]`) that bypass the 24h admin timelock entirely. They
/// emit only the immediate `ContractPaused`/`ContractUnpaused` events — never
/// `TimelockQueued`/`TimelockExecuted` — unlike the production
/// `queue_pause_contract` -> `execute_pause_contract` flow below. A test that
/// calls these helpers and expects timelock topics will never see them.
#[test]
fn pause_contract_test_helper_emits_immediate_event_not_timelock_events() {
    let (env, client, contract_id, admin) = setup();

    client.pause_contract(&admin);

    assert!(event_emitted::<ContractPausedEvent, _>(
        &env,
        &contract_id,
        symbol_short!("Contract"),
        symbol_short!("Paused"),
        |ev| ev.admin == admin,
    ));
    assert!(!event_emitted::<TimelockQueued, _>(
        &env,
        &contract_id,
        symbol_short!("Timelock"),
        symbol_short!("Queued"),
        |_| true,
    ));
    assert!(!event_emitted::<TimelockExecuted, _>(
        &env,
        &contract_id,
        symbol_short!("Timelock"),
        symbol_short!("Executed"),
        |_| true,
    ));
}

#[test]
fn unpause_contract_test_helper_emits_immediate_event_not_timelock_events() {
    let (env, client, contract_id, admin) = setup();
    client.pause_contract(&admin);

    client.unpause_contract(&admin);

    assert!(event_emitted::<ContractUnpausedEvent, _>(
        &env,
        &contract_id,
        symbol_short!("Contract"),
        symbol_short!("Unpaused"),
        |ev| ev.admin == admin,
    ));
    assert!(!event_emitted::<TimelockQueued, _>(
        &env,
        &contract_id,
        symbol_short!("Timelock"),
        symbol_short!("Queued"),
        |_| true,
    ));
    assert!(!event_emitted::<TimelockExecuted, _>(
        &env,
        &contract_id,
        symbol_short!("Timelock"),
        symbol_short!("Executed"),
        |_| true,
    ));
}

/// The production path, by contrast, emits `TimelockQueued` on queue and both
/// `TimelockExecuted` and `ContractPaused` on execute.
#[test]
fn queue_and_execute_pause_contract_emits_timelock_events_then_contract_paused() {
    let (env, client, contract_id, admin) = setup();

    client.queue_pause_contract(&admin);

    assert!(event_emitted::<TimelockQueued, _>(
        &env,
        &contract_id,
        symbol_short!("Timelock"),
        symbol_short!("Queued"),
        |ev| ev.operation == TimelockOperation::PauseContract as u32 && ev.proposer == admin,
    ));

    env.ledger().with_mut(|li| {
        li.timestamp += crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS;
    });

    client.execute_pause_contract(&admin);

    assert!(event_emitted::<TimelockExecuted, _>(
        &env,
        &contract_id,
        symbol_short!("Timelock"),
        symbol_short!("Executed"),
        |ev| ev.operation == TimelockOperation::PauseContract as u32 && ev.executor == admin,
    ));
    assert!(event_emitted::<ContractPausedEvent, _>(
        &env,
        &contract_id,
        symbol_short!("Contract"),
        symbol_short!("Paused"),
        |ev| ev.admin == admin,
    ));
}

#[test]
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

    // Drain the event log so a later count reflects only events emitted from
    // this point onward.
    let _before = env.events().all().events().len();

    // Re-adding the same resolver is a no-op: no state change, so this call
    // itself must not publish another ResolverApproved event.
    client.add_approved_resolver(&admin, &resolver);
    let events_from_noop_call = env.events().all().events().len();

    assert_eq!(events_from_noop_call, 0);
}

#[test]
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

#[test]
fn admin_rotation_emits_cancellation_and_mismatch_reason() {
    use crate::{TimelockCancellationReason, TimelockCancelled};
    let (env, client, contract_id, old_admin) = setup();
    let new_admin = Address::generate(&env);
    client.queue_set_platform_fee(&old_admin, &100);
    client.queue_set_admin(&old_admin, &new_admin);
    env.ledger()
        .with_mut(|li| li.timestamp += crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS);
    client.execute_set_admin(&old_admin, &new_admin);
    let op = TimelockOperation::SetPlatformFee as u32;
    assert!(event_emitted::<TimelockCancelled, _>(
        &env,
        &contract_id,
        symbol_short!("Timelock"),
        symbol_short!("Cancelled"),
        |event| event.operation == op
            && event.proposer == old_admin
            && event.canceller == new_admin,
    ));
    assert!(event_emitted::<TimelockCancellationReason, _>(
        &env,
        &contract_id,
        symbol_short!("Timelock"),
        symbol_short!("CancelWhy"),
        |event| event.operation == op
            && event.proposer == old_admin
            && event.reason == symbol_short!("AdmChange"),
    ));
}

mod golden_xdr {
    //! Golden XDR snapshots for every event payload. Each fixture serializes
    //! to ScVal XDR and must decode back to the identical struct. The digest
    //! pins the byte-level ABI: adding, removing, renaming, or reordering a
    //! field changes the digest and fails CI.
    //!
    //! Maintenance: when an event shape legitimately changes, bump
    //! `EVENT_SCHEMA_VERSION`, update the fixture fields and the digest in the
    //! same PR, and call out the indexer migration. Regenerate digests only
    //! after reviewing the decoded XDR diff — never to silence a failure.
    extern crate std;
    use crate::{events::*, EscrowState, ResolutionType, EVENT_SCHEMA_VERSION};
    use soroban_sdk::{
        testutils::Ledger as _,
        xdr::{FromXdr, ToXdr},
        Address, Bytes, BytesN, Env, IntoVal, String as SorobanString, Symbol, Val, Vec,
    };
    use std::fmt::Write as _;

    const GOLDEN_TIMESTAMP: u64 = 1_700_000_000;

    fn golden_env() -> Env {
        let env = Env::default();
        env.ledger().set_timestamp(GOLDEN_TIMESTAMP);
        env
    }

    fn golden_address(env: &Env, seed: u8) -> Address {
        let key = match seed {
            1 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            2 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4",
            3 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHK3M",
            5 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAK3IM",
            7 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAOLZM",
            16 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABB6KO",
            18 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABFO3O",
            21 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABKXA6",
            24 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABR4OP",
            25 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABTUG7",
            26 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABVM7P",
            31 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB6FV7",
            34 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACF2CY",
            37 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACKDZI",
            40 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACRIXZ",
            44 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACYJVZ",
            46 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAC4ZEZ",
            47 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAC6RMJ",
            50 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADFWKK",
            53 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADKPR2",
            60 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAADYF5L",
            62 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD4VML",
            64 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEADAU",
            65 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAECLIE",
            71 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEP23E",
            81 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCHAW",
            83 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFGXRW",
            84 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFJOKG",
            86 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFN63G",
            88 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFQNMH",
            92 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFZMOH",
            96 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGA3RQ",
            99 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGGDIA",
            101 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGLS3A",
            102 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGNKCQ",
            107 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGWBMB",
            110 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAG5IGR",
            120 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHQV5D",
            122 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHUFMD",
            127 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAH7MGT",
            128 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAQGN",
            129 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAICYO5",
            132 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIJREN",
            136 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIQSCM",
            141 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAI33I4",
            144 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJA4O7",
            153 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJSWCO",
            154 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJUO36",
            156 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJZ7I6",
            159 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAJ7HRO",
            161 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAKCA7Z",
            164 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAKJJVJ",
            167 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAKPRMZ",
            171 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAKWSKY",
            174 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAK53AI",
            175 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAK7TIY",
            176 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAALAE73",
            187 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAALW6CK",
            188 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAALZHZ2",
            196 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMIAGF",
            198 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMMQXF",
            199 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMOY7V",
            201 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMTLIU",
            202 => "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAMVTRE",
            _ => panic!("missing golden address seed"),
        };
        Address::from_str(env, key)
    }

    fn golden_basket(env: &Env) -> Vec<(Address, i128)> {
        let mut tokens = Vec::new(env);
        tokens.push_back((golden_address(env, 201), 1_000_i128));
        tokens.push_back((golden_address(env, 202), 2_000_i128));
        tokens
    }

    fn golden_params(env: &Env) -> Vec<Val> {
        let mut params = Vec::new(env);
        params.push_back(7_u32.into_val(env));
        params.push_back(Symbol::new(env, "golden").into_val(env));
        params
    }

    fn digest_hex(env: &Env, bytes: &Bytes) -> std::string::String {
        let digest = env.crypto().sha256(bytes);
        let mut out = std::string::String::new();
        for byte in digest.to_array() {
            write!(&mut out, "{byte:02x}").unwrap();
        }
        out
    }

    macro_rules! golden_event {
        ($test:ident, $ty:ty, $value:expr, $digest:expr) => {
            #[test]
            fn $test() {
                let env = golden_env();
                let event: $ty = $value(&env);
                assert_eq!(event.schema_version, EVENT_SCHEMA_VERSION);
                let bytes = event.clone().to_xdr(&env);
                let actual = digest_hex(&env, &bytes);
                assert_eq!(actual.as_str(), $digest, "golden XDR shape changed");
                let decoded = <$ty>::from_xdr(&env, &bytes).expect("golden XDR must decode");
                assert_eq!(&decoded, &event);
            }
        };
    }

    #[test]
    fn golden_schema_version_is_pinned() {
        assert_eq!(EVENT_SCHEMA_VERSION, 4, "bump schema version explicitly");
    }

    golden_event!(
        golden_fee_updated,
        FeeUpdated,
        |env| FeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "23a3edd649b835e0700cc190060c4233070d1600566c4bf15db5df815e3960bb"
    );
    golden_event!(
        golden_protocol_fee_updated,
        ProtocolFeeUpdated,
        |env| ProtocolFeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "23a3edd649b835e0700cc190060c4233070d1600566c4bf15db5df815e3960bb"
    );
    golden_event!(
        golden_arbitration_fee_updated,
        ArbitrationFeeUpdated,
        |env| ArbitrationFeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "23a3edd649b835e0700cc190060c4233070d1600566c4bf15db5df815e3960bb"
    );
    golden_event!(
        golden_admin_rotated,
        AdminRotated,
        |env| AdminRotated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_admin: golden_address(env, 92),
            new_admin: golden_address(env, 161),
            timestamp: GOLDEN_TIMESTAMP
        },
        "be9227d2dc531badb2fd92c00db62937eff74454b0af1ab16ad46a6f5adb938e"
    );
    golden_event!(
        golden_contract_paused_event,
        ContractPausedEvent,
        |env| ContractPausedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            admin: golden_address(env, 60),
            timestamp: GOLDEN_TIMESTAMP
        },
        "f45e57586fcd1709aca8b21d2619700dd6410e85bdeae9a2dbce100bfb5872a8"
    );
    golden_event!(
        golden_contract_unpaused_event,
        ContractUnpausedEvent,
        |env| ContractUnpausedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            admin: golden_address(env, 21),
            timestamp: GOLDEN_TIMESTAMP
        },
        "a28d11078c72e76d706124bbe58657ca4b5679f519a5ac0713391717f136cab9"
    );
    golden_event!(
        golden_escrow_created,
        EscrowCreated,
        |env| EscrowCreated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            seller: golden_address(env, 102),
            resolver: golden_address(env, 127),
            token: golden_address(env, 31),
            amount: 1_000_000_i128,
            fee_bps: 12_u32,
            resolver_fee_bps: 3_u32,
            shipping_window: 604_800_u64,
            expires_at: Some(1_800_000_000_u64),
            timestamp: GOLDEN_TIMESTAMP,
            new_state: EscrowState::Completed
        },
        "5a845d3792e4fd7bff638b168f7cd3b409dcb20a7b4e77180b95d256379cf579"
    );
    golden_event!(
        golden_escrow_funded,
        EscrowFunded,
        |env| EscrowFunded {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            buyer: golden_address(env, 46),
            amount: 1_000_000_i128,
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed,
            basket_tokens: Some(golden_basket(env))
        },
        "33d277c43afe2ffc8755b81f3cf86286dacc304eae2b6ad302cd082ee6d02a60"
    );
    golden_event!(
        golden_escrow_shipped,
        EscrowShipped,
        |env| EscrowShipped {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            seller: golden_address(env, 174),
            tracking_id: SorobanString::from_str(env, "golden"),
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "e6fe5703446f9984df059b1f558154cfe8cac0cd0b1f72d77276ed1ad7a56728"
    );
    golden_event!(
        golden_delivery_recorded,
        DeliveryRecorded,
        |env| DeliveryRecorded {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            delivered_at: GOLDEN_TIMESTAMP
        },
        "2384c6aa085b3959bda82bab16804cba883bba033befd07770f1ccf09dfd98fe"
    );
    golden_event!(
        golden_escrow_completed,
        EscrowCompleted,
        |env| EscrowCompleted {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            recipient: golden_address(env, 175),
            amount: 1_000_000_i128,
            fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "0b71e54692b870dc0ca546eca8d50676e6f323803eda8cfb120bab1e668f4521"
    );
    golden_event!(
        golden_dispute_raised,
        DisputeRaised,
        |env| DisputeRaised {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            buyer: golden_address(env, 83),
            reason: Symbol::new(env, "golden_reason"),
            description: Bytes::from_slice(env, b"golden"),
            evidence_hash: BytesN::from_array(env, &[7_u8; 32]),
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "f64254a89eee5210dd378857c202b8828ebc54ea5a3aabd77b5a2b780351a8c8"
    );
    golden_event!(
        golden_dispute_resolved,
        DisputeResolved,
        |env| DisputeResolved {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            resolver: golden_address(env, 7),
            resolution: ResolutionType::Release,
            recipient: golden_address(env, 40),
            amount: 1_000_000_i128,
            arbitration_fee: 10_000_i128,
            resolver_fee: 10_000_i128,
            platform_fee: 10_000_i128,
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "04db1cfb82aeabdaf41a6b47eac2f9c6188348eba5325cd3f3658db562db6af9"
    );
    golden_event!(
        golden_resolver_vote_recorded,
        ResolverVoteRecorded,
        |env| ResolverVoteRecorded {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            resolver: golden_address(env, 110),
            resolution: ResolutionType::Release,
            vote_count: 3_u32,
            threshold: 5_u32,
            voted_at: GOLDEN_TIMESTAMP
        },
        "5723352454d5df52890cf4e45310dade63714f87b6f3c20f0ec5ef7692ad5e2a"
    );
    golden_event!(
        golden_auto_released,
        AutoReleased,
        |env| AutoReleased {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            seller: golden_address(env, 141),
            amount: 1_000_000_i128,
            fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "688d6f7bcc018d7ff35355ad047c8f176852f1af6be1b1bacd771c04cbdc7eb7"
    );
    golden_event!(
        golden_escrow_canceled,
        EscrowCanceled,
        |env| EscrowCanceled {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            seller: golden_address(env, 34),
            cancelled_by: golden_address(env, 154),
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "13a7e3d8314651326e846c503a921e6d4d9db6e04bf6aa131c085c450751d544"
    );
    golden_event!(
        golden_escrow_expired,
        EscrowExpired,
        |env| EscrowExpired {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            buyer: golden_address(env, 62),
            amount: 1_000_000_i128,
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "7247e8b2d5ccfd59e5e53e3a9ec06698ed6e6ef6640ddb1e31abc2665df5fd0e"
    );
    golden_event!(
        golden_escrow_auto_canceled,
        EscrowAutoCanceled,
        |env| EscrowAutoCanceled {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            timestamp: GOLDEN_TIMESTAMP
        },
        "4ee1392ef31878aca7de0f97f9f08c97866c95a618860789324cfb3989c8034d"
    );
    golden_event!(
        golden_contract_initialized,
        ContractInitialized,
        |env| ContractInitialized {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            admin: golden_address(env, 175),
            fee_collector: golden_address(env, 101),
            arbitration_fee_bps: 11_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "7fa9c29221af0f532b1dafda63d2940edecfc8e0ba27b7dde0b6cd5410fd9916"
    );
    golden_event!(
        golden_resolver_rotated,
        ResolverRotated,
        |env| ResolverRotated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            old_resolver: golden_address(env, 144),
            new_resolver: golden_address(env, 127),
            rotated_at: GOLDEN_TIMESTAMP
        },
        "25efba56f44d184d000d1c86e1fe81be31f95259ba571e08a7257ed94761fba6"
    );
    golden_event!(
        golden_dispute_evidence_submitted,
        DisputeEvidenceSubmitted,
        |env| DisputeEvidenceSubmitted {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            submitter: golden_address(env, 53),
            evidence_hash: BytesN::from_array(env, &[7_u8; 32]),
            description: Bytes::from_slice(env, b"golden"),
            entry_count: 2_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "376f78c588fb23f744a8f8476a254ce72375cdbb867ec8892967bac0c0b62b61"
    );
    golden_event!(
        golden_resolver_sanctioned,
        ResolverSanctioned,
        |env| ResolverSanctioned {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            old_resolver: golden_address(env, 196),
            replacement_resolver: golden_address(env, 176),
            sanctioned_by: golden_address(env, 167),
            sanctioned_at: GOLDEN_TIMESTAMP
        },
        "37235229936d921a9fd5bc3f06171ca2aff1019f56702318e104a7cd9d19eb20"
    );
    golden_event!(
        golden_token_allowlist_updated,
        TokenAllowlistUpdated,
        |env| TokenAllowlistUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            added: true,
            timestamp: GOLDEN_TIMESTAMP
        },
        "98525bb625d804fd377a85e5a0b4fa249d750efba7848559a7abeb546365b7e0"
    );
    golden_event!(
        golden_allowlist_toggled,
        AllowlistToggled,
        |env| AllowlistToggled {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            enabled: true,
            timestamp: GOLDEN_TIMESTAMP
        },
        "98e25935eef7c8770c8ae27f0c087a4153e6232e36331ca4fe79b68493e1dbe8"
    );
    golden_event!(
        golden_dispute_pending_finalization,
        DisputePendingFinalization,
        |env| DisputePendingFinalization {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            resolver: golden_address(env, 25),
            resolution: ResolutionType::Release,
            amount: 1_000_000_i128,
            appeal_deadline: 1_700_030_000_u64,
            pending_at: GOLDEN_TIMESTAMP,
            resolution_reason: Some(Symbol::new(env, "other"))
        },
        "98fcf7d1587f04369cd481e2811b3a62bbd9ef3efbe1ab0e3ec4f4593c9f0903"
    );
    golden_event!(
        golden_dispute_appealed,
        DisputeAppealed,
        |env| DisputeAppealed {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            appellant: golden_address(env, 34),
            timestamp: GOLDEN_TIMESTAMP,
            appeal_fee_charged: 10_000_i128,
            appeal_round: 1_u32,
            fee_basis: 10_000_i128
        },
        "376fd473ea668709aa1698607eac7478235756f58f908f4b8f3593a52669316e"
    );
    golden_event!(
        golden_platform_fee_updated,
        PlatformFeeUpdated,
        |env| PlatformFeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "23a3edd649b835e0700cc190060c4233070d1600566c4bf15db5df815e3960bb"
    );
    golden_event!(
        golden_treasury_updated,
        TreasuryUpdated,
        |env| TreasuryUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_treasury: golden_address(env, 159),
            new_treasury: golden_address(env, 96),
            timestamp: GOLDEN_TIMESTAMP
        },
        "a7723e299ab8131d6ff21ea61cfe0731f06cbc9a06af9c973712e84f1a72a5f1"
    );
    golden_event!(
        golden_platform_fee_collected,
        PlatformFeeCollected,
        |env| PlatformFeeCollected {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            token: golden_address(env, 84),
            treasury: golden_address(env, 164),
            fee_amount: 10_000_i128,
            timestamp: GOLDEN_TIMESTAMP
        },
        "22b1401d0a224672f5b0e585fbe67ef6b59e2ce1ca57103a98a6189ce04cacc0"
    );
    golden_event!(
        golden_basket_escrow_created,
        BasketEscrowCreated,
        |env| BasketEscrowCreated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            seller: golden_address(env, 199),
            token_count: 2_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "73edfd797ee67dafa29b2bf8137c26b7a24b9bf3963d6a36114c23a68957778d"
    );
    golden_event!(
        golden_basket_token_payout,
        BasketTokenPayout,
        |env| BasketTokenPayout {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            token: golden_address(env, 2),
            recipient: golden_address(env, 88),
            net_amount: 990_000_i128,
            fee: 10_000_i128,
            timestamp: GOLDEN_TIMESTAMP
        },
        "bde0cfd16d26f95ec2d63bc1c8e2164218f6badd59e7ed9ea1ad9ed34ffa0a96"
    );
    golden_event!(
        golden_message_posted,
        MessagePosted,
        |env| MessagePosted {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            sender: golden_address(env, 18),
            timestamp: GOLDEN_TIMESTAMP
        },
        "5e650b194c5c9b9660a6df8b33fe31469fccb36d4740d64ddc84d6eeac9720b0"
    );
    golden_event!(
        golden_refund_requested_event,
        RefundRequestedEvent,
        |env| RefundRequestedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            buyer: golden_address(env, 156),
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed,
            basket_tokens: Some(golden_basket(env))
        },
        "f7a6f425156a66c9f6222bae023cd1fe5aeeeb4002ffcc7a303cc4f342291361"
    );
    golden_event!(
        golden_refund_approved_event,
        RefundApprovedEvent,
        |env| RefundApprovedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            seller: golden_address(env, 120),
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "50143a344c2be0e6ecb0c42ca617af6041e617f77ac7980964a7e5305af46c44"
    );
    golden_event!(
        golden_contract_upgraded_event,
        ContractUpgradedEvent,
        |env| ContractUpgradedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            admin: golden_address(env, 120),
            new_wasm_hash: BytesN::from_array(env, &[7_u8; 32]),
            timestamp: GOLDEN_TIMESTAMP
        },
        "17985d96e3b4f7ee26f258b8da21001edaae9a88d519bca4ebf49b5127d0ab8b"
    );
    golden_event!(
        golden_storage_migrated_event,
        StorageMigratedEvent,
        |env| StorageMigratedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            admin: golden_address(env, 99),
            from_version: 2_u32,
            to_version: 4_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "19c11d150fcff399c172e5c831b2fbd9153096d2051b2f5ca7e5c41b21ae7de2"
    );
    golden_event!(
        golden_ttl_extension_updated,
        TtlExtensionUpdated,
        |env| TtlExtensionUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_ledgers: 100_u32,
            new_ledgers: 200_u32,
            caller: golden_address(env, 122),
            timestamp: GOLDEN_TIMESTAMP
        },
        "4de50a1ce40750c8242a78ca1797ffc44fe9aecc29d5172db554239dae4126d5"
    );
    golden_event!(
        golden_amount_limits_updated,
        AmountLimitsUpdated,
        |env| AmountLimitsUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_min_amount: 100_i128,
            new_min_amount: 200_i128,
            old_max_amount: 1_000_000_i128,
            new_max_amount: 2_000_000_i128,
            caller: golden_address(env, 24),
            timestamp: GOLDEN_TIMESTAMP
        },
        "7e5400b1d7c90ccb2aa4a889a0fa1a5d324e913fa93bc35710349e7b7205e3b0"
    );
    golden_event!(
        golden_action_paused_event,
        ActionPausedEvent,
        |env| ActionPausedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            action: Symbol::new(env, "golden_reason"),
            caller: golden_address(env, 188),
            timestamp: GOLDEN_TIMESTAMP
        },
        "05e435dcb72968719889b7a1e268d5228a846c1893f3421e8486ab70cff444f9"
    );
    golden_event!(
        golden_action_unpaused_event,
        ActionUnpausedEvent,
        |env| ActionUnpausedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            action: Symbol::new(env, "golden_reason"),
            caller: golden_address(env, 3),
            timestamp: GOLDEN_TIMESTAMP
        },
        "cfb8c29f4db0181a3b9bd77bea0896126bebe72bad75b97601ecacd19f7c31d2"
    );
    golden_event!(
        golden_resolver_approved,
        ResolverApproved,
        |env| ResolverApproved {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            resolver: golden_address(env, 65),
            caller: golden_address(env, 1),
            timestamp: GOLDEN_TIMESTAMP
        },
        "e403901a9a4a43382bcd08eeb3b533f1fd9c5c747d423bc1e5f448d91f74c7c6"
    );
    golden_event!(
        golden_resolver_removed,
        ResolverRemoved,
        |env| ResolverRemoved {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            resolver: golden_address(env, 159),
            caller: golden_address(env, 187),
            timestamp: GOLDEN_TIMESTAMP
        },
        "80603b363362a7468d2a1da81c0328a19fa1155f1db494f45802035e3d806dc9"
    );
    golden_event!(
        golden_resolver_strict_updated,
        ResolverStrictUpdated,
        |env| ResolverStrictUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_strict: false,
            new_strict: true,
            caller: golden_address(env, 198),
            timestamp: GOLDEN_TIMESTAMP
        },
        "979dc5d504c00f9f4263ffe2304c32b354b72d084155e6dd83fcb9d58d5f28e5"
    );
    golden_event!(
        golden_fee_collector_updated,
        FeeCollectorUpdated,
        |env| FeeCollectorUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_collector: golden_address(env, 132),
            timestamp: GOLDEN_TIMESTAMP
        },
        "37b239e1a01f02b4788496396fd4e645d7f38f0acf06530cdd2c70f371bb440a"
    );
    golden_event!(
        golden_fee_collector_pending,
        FeeCollectorPending,
        |env| FeeCollectorPending {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_collector: golden_address(env, 5),
            pending_collector: golden_address(env, 64),
            timestamp: GOLDEN_TIMESTAMP
        },
        "b94502ccf9cba0caf31333f42b1895f6bc6076f553befff7cef2b6adb3e6565e"
    );
    golden_event!(
        golden_fee_collector_accepted,
        FeeCollectorAccepted,
        |env| FeeCollectorAccepted {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_collector: golden_address(env, 47),
            new_collector: golden_address(env, 81),
            timestamp: GOLDEN_TIMESTAMP
        },
        "cc2a0052cf163eb3feabe9e1b46e2c7646676ac6104e5de7088e594753614eb6"
    );
    golden_event!(
        golden_treasury_pending,
        TreasuryPending,
        |env| TreasuryPending {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_treasury: golden_address(env, 129),
            pending_treasury: golden_address(env, 107),
            timestamp: GOLDEN_TIMESTAMP
        },
        "efc9f6696d2f1d3e71eab5a96dd60f39c04827293a7c0d5c92c6ae7ed2b6c6b7"
    );
    golden_event!(
        golden_treasury_accepted,
        TreasuryAccepted,
        |env| TreasuryAccepted {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_treasury: golden_address(env, 50),
            new_treasury: golden_address(env, 1),
            timestamp: GOLDEN_TIMESTAMP
        },
        "24b943e55918242a659c873d7f66358c4d13b8e0d17501ed51a3c942f19d291a"
    );
    golden_event!(
        golden_emergency_drain,
        EmergencyDrain,
        |env| EmergencyDrain {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            token: golden_address(env, 62),
            amount: 1_000_000_i128,
            timestamp: GOLDEN_TIMESTAMP
        },
        "17c9548a4768fc12744a0017b5293e9f1f80c3dde0b1d5b6ec2ba5446488bdfb"
    );
    golden_event!(
        golden_emergency_drain_blocked,
        EmergencyDrainBlocked,
        |env| EmergencyDrainBlocked {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            timestamp: GOLDEN_TIMESTAMP
        },
        "4ee1392ef31878aca7de0f97f9f08c97866c95a618860789324cfb3989c8034d"
    );
    golden_event!(
        golden_delivery_proposed,
        DeliveryProposed,
        |env| DeliveryProposed {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            proposed_at: 1_700_005_000_u64,
            unlock_at: 1_700_010_000_u64
        },
        "05030971d6aef9256cc0561bd110dd2b32711396a41c11486b4652a106008338"
    );
    golden_event!(
        golden_delivery_proposal_cancelled,
        DeliveryProposalCancelled,
        |env| DeliveryProposalCancelled {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            timestamp: GOLDEN_TIMESTAMP
        },
        "4ee1392ef31878aca7de0f97f9f08c97866c95a618860789324cfb3989c8034d"
    );
    golden_event!(
        golden_pending_expiry_clear,
        PendingExpiryClear,
        |env| PendingExpiryClear {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            timestamp: GOLDEN_TIMESTAMP
        },
        "4ee1392ef31878aca7de0f97f9f08c97866c95a618860789324cfb3989c8034d"
    );
    golden_event!(
        golden_timelock_queued,
        TimelockQueued,
        |env| TimelockQueued {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            operation: 3_u32,
            proposer: golden_address(env, 171),
            queued_at: 1_700_005_000_u64,
            ready_at: 1_700_020_000_u64
        },
        "86a1ca3364cf6441fa4d909529376d42888705c60d53433b28f7fc105e674116"
    );
    golden_event!(
        golden_timelock_executed,
        TimelockExecuted,
        |env| TimelockExecuted {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            operation: 3_u32,
            proposer: golden_address(env, 71),
            executor: golden_address(env, 128),
            params: golden_params(env),
            executed_at: GOLDEN_TIMESTAMP
        },
        "a9a2fe03c737f012aaae681a4695fcc0e74721c3c168e2560e197b86e9198384"
    );
    golden_event!(
        golden_timelock_cancelled,
        TimelockCancelled,
        |env| TimelockCancelled {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            operation: 3_u32,
            proposer: golden_address(env, 86),
            canceller: golden_address(env, 37),
            cancelled_at: GOLDEN_TIMESTAMP
        },
        "df1a86e13f5314c09cf829b67d003603203a24e2ac0160582ba8ddc2a8cba1f8"
    );
    golden_event!(
        golden_timelock_cancellation_reason,
        TimelockCancellationReason,
        |env| TimelockCancellationReason {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            operation: 3_u32,
            proposer: golden_address(env, 16),
            reason: Symbol::new(env, "golden_reason"),
            cancelled_at: GOLDEN_TIMESTAMP
        },
        "8a1150665290b7c4289e4ce97711a741887fff108498c1fabf71722d35bb7696"
    );
    golden_event!(
        golden_max_appeals_updated,
        MaxAppealsUpdated,
        |env| MaxAppealsUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_max: 4_u32,
            new_max: 9_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "15f0e7cb9d1a3b874561d299015d321bf04fd9034e51fbc6785248c96c9730e9"
    );
    golden_event!(
        golden_max_basket_size_updated,
        MaxBasketSizeUpdated,
        |env| MaxBasketSizeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_max: 4_u32,
            new_max: 9_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "15f0e7cb9d1a3b874561d299015d321bf04fd9034e51fbc6785248c96c9730e9"
    );
    golden_event!(
        golden_recovery_withdraw,
        RecoveryWithdraw,
        |env| RecoveryWithdraw {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            buyer: golden_address(env, 46),
            amount: 1_000_000_i128,
            timestamp: GOLDEN_TIMESTAMP
        },
        "c02e841b8711f6c72b69a48fd1a795c05d078209d46bcb9299fb4e7fdc139764"
    );
    golden_event!(
        golden_timelock_delay_updated,
        TimelockDelayUpdated,
        |env| TimelockDelayUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_delay: 300_u64,
            new_delay: 600_u64,
            caller: golden_address(env, 153),
            timestamp: GOLDEN_TIMESTAMP
        },
        "18578178a8793fd6f472afefe03998a3290241d319e19a72e98737ff285cefec"
    );
    golden_event!(
        golden_appeal_fee_updated,
        AppealFeeUpdated,
        |env| AppealFeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "23a3edd649b835e0700cc190060c4233070d1600566c4bf15db5df815e3960bb"
    );
    golden_event!(
        golden_recovery_mode_updated,
        RecoveryModeUpdated,
        |env| RecoveryModeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            enabled: true,
            caller: golden_address(env, 26),
            timestamp: GOLDEN_TIMESTAMP
        },
        "bab8effd9df411f16c5cce5fabd0a82e00be390809b0e6d55728b0b083fad69b"
    );
    golden_event!(
        golden_accumulated_fees_claimed,
        AccumulatedFeesClaimed,
        |env| AccumulatedFeesClaimed {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            collector: golden_address(env, 136),
            token: golden_address(env, 44),
            amount: 1_000_000_i128,
            timestamp: GOLDEN_TIMESTAMP
        },
        "ee3d75ef0ad9d3aaaa8e15c26b4b81c89d02896deb5d2f9356541dbdc613a95b"
    );
}

/// Asserts the topic shape of every timelock event in the current event
/// buffer: exactly `(symbol_short!("Timelock"), kind)` with `kind` from the
/// three documented values. Soroban tops out at 10-character symbols, and
/// indexers decode by this two-symbol tuple.
fn assert_timelock_topic_shape(env: &Env, contract_id: &Address, expected: &[Symbol]) {
    let all = env.events().all();
    let mut found = 0_usize;
    for event in all.filter_by_contract(contract_id).events().iter() {
        let soroban_sdk::xdr::ContractEventBody::V0(v0) = &event.body;
        let mut topics = v0.topics.iter();
        let Some(topic1) = topics.next() else {
            continue;
        };
        let Ok(sym1) = Symbol::try_from_val(env, topic1) else {
            continue;
        };
        if sym1 != symbol_short!("Timelock") {
            continue;
        }
        found += 1;

        let topic2 = topics.next().expect("timelock event has a second topic");
        assert!(
            topics.next().is_none(),
            "timelock topics must be a two-symbol tuple"
        );
        let sym2 = Symbol::try_from_val(env, topic2).expect("topic decodes as Symbol");
        assert!(
            sym2 == symbol_short!("Queued")
                || sym2 == symbol_short!("Executed")
                || sym2 == symbol_short!("Cancelled"),
            "unexpected timelock topic"
        );
        assert!(
            expected.contains(&sym2),
            "unexpected timelock event kind for this call"
        );
    }
    assert_eq!(found, expected.len(), "unexpected timelock event count");
}

/// Issue #1244: indexers key off `schema_version` in timelock payloads. Every
/// timelock event emitted by the contract must stamp the current
/// `EVENT_SCHEMA_VERSION` and survive an XDR round-trip in that shape.
#[test]
fn timelock_events_stamp_current_schema_version_and_standard_topics() {
    use crate::{TimelockCancelled, EVENT_SCHEMA_VERSION};
    use soroban_sdk::xdr::{FromXdr, ToXdr};

    let (env, client, contract_id, admin) = setup();

    client.queue_set_platform_fee(&admin, &50_u32);
    assert_timelock_topic_shape(&env, &contract_id, &[symbol_short!("Queued")]);
    let queued = timelock_payload::<TimelockQueued>(&env, &contract_id, symbol_short!("Queued"))
        .expect("TimelockQueued decodes from its emitted payload");
    assert_eq!(queued.schema_version, EVENT_SCHEMA_VERSION);
    assert_eq!(queued.operation, TimelockOperation::SetPlatformFee as u32);
    let queued_bytes = queued.clone().to_xdr(&env);
    assert_eq!(
        TimelockQueued::from_xdr(&env, &queued_bytes).expect("TimelockQueued XDR round-trip"),
        queued
    );

    env.ledger()
        .with_mut(|li| li.timestamp += crate::admin::ADMIN_TIMELOCK_DELAY_SECONDS);
    client.execute_set_platform_fee(&admin, &50_u32);
    assert_timelock_topic_shape(&env, &contract_id, &[symbol_short!("Executed")]);
    let executed =
        timelock_payload::<TimelockExecuted>(&env, &contract_id, symbol_short!("Executed"))
            .expect("TimelockExecuted decodes from its emitted payload");
    assert_eq!(executed.schema_version, EVENT_SCHEMA_VERSION);
    let executed_bytes = executed.clone().to_xdr(&env);
    assert_eq!(
        TimelockExecuted::from_xdr(&env, &executed_bytes).expect("TimelockExecuted XDR round-trip"),
        executed
    );

    client.queue_set_ttl_extension(&admin, &2_000_u32);
    client.cancel_timelock_op(&admin, &(TimelockOperation::SetTtlExtension as u32));
    assert_timelock_topic_shape(&env, &contract_id, &[symbol_short!("Cancelled")]);
    let cancelled =
        timelock_payload::<TimelockCancelled>(&env, &contract_id, symbol_short!("Cancelled"))
            .expect("TimelockCancelled decodes from its emitted payload");
    assert_eq!(cancelled.schema_version, EVENT_SCHEMA_VERSION);
    let cancelled_bytes = cancelled.clone().to_xdr(&env);
    assert_eq!(
        TimelockCancelled::from_xdr(&env, &cancelled_bytes)
            .expect("TimelockCancelled XDR round-trip"),
        cancelled
    );
}
