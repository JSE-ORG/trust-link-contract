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
        assert_eq!(EVENT_SCHEMA_VERSION, 5, "bump schema version explicitly");
    }

    golden_event!(
        golden_fee_updated,
        FeeUpdated,
        |_env| FeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "48d1ec6b7abf92ecb491e500a6ff547f5479662d4c08b071389550505894677d"
    );
    golden_event!(
        golden_protocol_fee_updated,
        ProtocolFeeUpdated,
        |_env| ProtocolFeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "48d1ec6b7abf92ecb491e500a6ff547f5479662d4c08b071389550505894677d"
    );
    golden_event!(
        golden_arbitration_fee_updated,
        ArbitrationFeeUpdated,
        |_env| ArbitrationFeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "48d1ec6b7abf92ecb491e500a6ff547f5479662d4c08b071389550505894677d"
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
        "77a2e7395f68dd2eebfacbfd4698b7e172aa9b254cb2aada12ee09088524cc9f"
    );
    golden_event!(
        golden_contract_paused_event,
        ContractPausedEvent,
        |env| ContractPausedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            admin: golden_address(env, 60),
            timestamp: GOLDEN_TIMESTAMP
        },
        "eccb088ca71e35bc52093d0fe1ce4b314da62026b8dba1dad563af8da55e7495"
    );
    golden_event!(
        golden_contract_unpaused_event,
        ContractUnpausedEvent,
        |env| ContractUnpausedEvent {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            admin: golden_address(env, 21),
            timestamp: GOLDEN_TIMESTAMP
        },
        "9ff6f223839e7174a5d11f807fe4bdad34942e7e61b84525d4c112159cd595da"
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
        "1077873453a3e670f810cde9401f3bec9245f11a5f063ba01348e4a3823cd45f"
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
        "7649590160948a57bea054f3c94ea3a0788bb8ffe4448661ec372baf35a35a5e"
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
        "eb23f66de1257729a88f7aa86e2d77044b44c73a35cb5b1d2a18b1f5454f15d1"
    );
    golden_event!(
        golden_delivery_recorded,
        DeliveryRecorded,
        |_env| DeliveryRecorded {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            delivered_at: GOLDEN_TIMESTAMP
        },
        "4421925c81fc5367ac4cc7114bf538113a1cb93d3f5c9668852ec4f39cc7b53a"
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
            co_signed_payees: 0_u32,
            timestamp: GOLDEN_TIMESTAMP,
            prev_state: EscrowState::Shipped,
            new_state: EscrowState::Completed
        },
        "332bb7609fa35eff9916b7dcd2806cbe40f0eb27113fae1876798efd97507ffd"
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
        "36d7331aeaa513d725bcd0ca64f20ea5e6cec5c1a6b2ea5b6bfc11adee34667b"
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
        "814080ec3195a642d5eaff645e57b9ff0f4e0e86eda3843aaf4a156ccb925bd0"
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
        "2aae2b94022d7b09ee3b8b0614ea448330d56065b304f22d6bc097b965051282"
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
        "b20133a7d1df1c7f35d5fa595a0e0eac805fc82c3e3b593b3c529e149708ddf0"
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
        "778ced575c8352ce5fe82b9bd80beae13bc4468ad49e92710a7017f4f418e442"
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
        "a495a33378ebee3beb185871fd4a718ba836526e409fbab194231065b7a1da9d"
    );
    golden_event!(
        golden_escrow_auto_canceled,
        EscrowAutoCanceled,
        |_env| EscrowAutoCanceled {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            timestamp: GOLDEN_TIMESTAMP
        },
        "2f6fe6528eadcc1a3b398388af663e43ff871e6329c8debdf56afaf1580da674"
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
        "d0283c76900f451eba04b21b6e7206649f9015549053df5be64735390d9e2551"
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
        "bfe784ee68585d3c5ffe166037ab6330cb24167aeaa02a22f9c714f194569dac"
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
        "c4af41b169718e0e3dd3e1408f977c67e5df441d10ecd6df57a790d21af6c034"
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
        "3332127f48a795945b78f04f302a1195a25136925db3fbdd744447217736f212"
    );
    golden_event!(
        golden_token_allowlist_updated,
        TokenAllowlistUpdated,
        |_env| TokenAllowlistUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            added: true,
            timestamp: GOLDEN_TIMESTAMP
        },
        "f3769bd1cbd2ba7aef9ba2bd29639063f845bb9ed055af4b157f1bfb6832dfcd"
    );
    golden_event!(
        golden_allowlist_toggled,
        AllowlistToggled,
        |_env| AllowlistToggled {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            enabled: true,
            timestamp: GOLDEN_TIMESTAMP
        },
        "236087f0e40692cc3f4342beb399d04f13b468cf071e899f50b350994eb344fa"
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
        "4ce170854fab3492626953726d0b3a25021180f3e80b2141231d5385e19e1d9e"
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
        "c901d6ff8baca1c27288a4d2f9826a06226148a7e8cf3a82c022adcde727dc73"
    );
    golden_event!(
        golden_platform_fee_updated,
        PlatformFeeUpdated,
        |_env| PlatformFeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "48d1ec6b7abf92ecb491e500a6ff547f5479662d4c08b071389550505894677d"
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
        "b12c2460b76a3ec6096575b00d9632bbb32d54a54dc9aa3daa6cb958f14785c4"
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
        "d11a8b40701a3649fb4cfc96319e6a26535541c135b9e9e8ca666d968b0b36c7"
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
        "453de56d4ecb0e9fa50077e8f2e71f3ee8f6ac95bbfd16f21a448b5c4d11cd5b"
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
        "31ef05e818c6a79f114a75c57309aa95ca265306afc913423b727944064859f4"
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
        "101d78bf268f190e15fc9213725151e63ce966705f1eaadb1e2d0947ca10a89c"
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
        "ee34251a0eac2607ee47b835a4e3255e8c72b4e4c4fada5b7d84213c71590c83"
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
        "d18202dcb797259d3bb7ea8051bbd18cd0a5247d8f8e2076cca6f57cb1ea24ec"
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
        "8ea33f4ca16e33e8c6d71a49fdc50acf818bb794c3a8298acb14c831a525918c"
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
        "0aff670927fddcf24dc2b1e4c831da15b0c84f5b8f99580d903efe9a3567e88a"
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
        "6779568c20c3d476dc80c0ab6bbde6e6ef342f4d977b4baae7deffc8643431b2"
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
        "0b40baa7e45a549c0c0e497782adeea5809a01ddb17f572a1cc8933e3a36bead"
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
        "a0f12972ab00f957a9012ece7906c7cddb2d83500e8dfedb1d08696062eca46c"
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
        "4586e85b56097014d714bf9d59fc640e623a239ec5ee1e34f661ba75af85ca26"
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
        "dce003f0d8d904258bdb116bccad7cad7aa059a35fdb36397c864d3914d629c0"
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
        "ea5d3d163e07f69b975609f175c0a3070a141a94ce2942f229ff772a72f4b621"
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
        "c9f4e415a22a17d3473d7f005037b8693780bcc2c0325aead596a726d5502366"
    );
    golden_event!(
        golden_fee_collector_updated,
        FeeCollectorUpdated,
        |env| FeeCollectorUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_collector: golden_address(env, 132),
            timestamp: GOLDEN_TIMESTAMP
        },
        "9466e1e03acf8c9a787ab180da1537d30f8eae8470d31e1721d801f37b7d8034"
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
        "11b7ec234795c12d0fb2ec3b296c1723e092540ecee70c1025b99ba2c588b92d"
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
        "ce810d14c945271e3021bbc01f05a39d0b01c8ab6388b10dd122c82fa94426ea"
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
        "a8521b3ca5ed284e9f40d02634e16298687c65bdf3f4cbe6a764af14b739ffc1"
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
        "893fba4d204d74b822d838d4cf4b0151bb147eea8202c6fe7c0d344ed130caeb"
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
        "2f8fd4550a64c77ae8fd2eccc2aaf1dd0f71e37c9382ff009065c6d87e36a9d3"
    );
    golden_event!(
        golden_emergency_drain_blocked,
        EmergencyDrainBlocked,
        |_env| EmergencyDrainBlocked {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            timestamp: GOLDEN_TIMESTAMP
        },
        "2f6fe6528eadcc1a3b398388af663e43ff871e6329c8debdf56afaf1580da674"
    );
    golden_event!(
        golden_delivery_proposed,
        DeliveryProposed,
        |_env| DeliveryProposed {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            proposed_at: 1_700_005_000_u64,
            unlock_at: 1_700_010_000_u64
        },
        "7de51a76ca76d150ee67b52697ea026a7103eda6f4664357c8fa85cf4c3ade3a"
    );
    golden_event!(
        golden_delivery_proposal_cancelled,
        DeliveryProposalCancelled,
        |_env| DeliveryProposalCancelled {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            timestamp: GOLDEN_TIMESTAMP
        },
        "2f6fe6528eadcc1a3b398388af663e43ff871e6329c8debdf56afaf1580da674"
    );
    golden_event!(
        golden_pending_expiry_clear,
        PendingExpiryClear,
        |_env| PendingExpiryClear {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            escrow_id: 7_u64,
            timestamp: GOLDEN_TIMESTAMP
        },
        "2f6fe6528eadcc1a3b398388af663e43ff871e6329c8debdf56afaf1580da674"
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
        "fd86f5733b7fd6fbb91bdbc04121ca359da2426e12d247520df30cf12b95e04a"
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
        "bcf7d567a5aacef4d60fea2af648742f2ab953798f9d2525cfd1f843ed92e5cf"
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
        "cea977831f9f4f4aae5acd28c979a4f50047cc3ea45b82c8520bda580107e6c5"
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
        "129fe6f0af71fa2fe5df7044478961e0f0baaab5883a4f82e623c32155f5aab2"
    );
    golden_event!(
        golden_max_appeals_updated,
        MaxAppealsUpdated,
        |_env| MaxAppealsUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_max: 4_u32,
            new_max: 9_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "c62c4863efdac3ea34f88afa2a538750f50adf00afe8e8ae6c8fbe91cc4c0332"
    );
    golden_event!(
        golden_max_basket_size_updated,
        MaxBasketSizeUpdated,
        |_env| MaxBasketSizeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_max: 4_u32,
            new_max: 9_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "c62c4863efdac3ea34f88afa2a538750f50adf00afe8e8ae6c8fbe91cc4c0332"
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
        "0cb6ab4d50ab1a88292789ed13f20e668024f63bd7c3a2677ee631a732721dd7"
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
        "38c105ab8b04953b266435329b9171426289d11809c21720e77e5925b7c0b0dd"
    );
    golden_event!(
        golden_appeal_fee_updated,
        AppealFeeUpdated,
        |_env| AppealFeeUpdated {
            schema_version: crate::EVENT_SCHEMA_VERSION,
            old_fee_bps: 7_u32,
            new_fee_bps: 12_u32,
            timestamp: GOLDEN_TIMESTAMP
        },
        "48d1ec6b7abf92ecb491e500a6ff547f5479662d4c08b071389550505894677d"
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
        "9edfc231589f382af12a8c14d210b6c534a9c3c2b26ace1ad6ce914e59bf9e20"
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
        "d2fe7d74a5376e8b1396079e71118bed43300d003867a3dcb522caf68507fc0f"
    );
}
