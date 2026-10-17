#![cfg(test)]
//! Verifies the lifecycle counters are sharded across persistent buckets (to
//! avoid a single contended instance key) and re-aggregated by `get_stats`.

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Bytes, BytesN, Env, IntoVal, String, Symbol, Vec,
};

#[test]
fn created_counter_is_sharded_and_aggregated() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let total = crate::COUNTER_SHARDS as u64 + 1;
    let mut resolvers = Vec::new(&env);
    resolvers.push_back(resolver.clone());
    for _ in 0..total {
        client.create_escrow_multi(
            &seller,
            &Some(buyer.clone()),
            &resolvers,
            &1,
            &token,
            &1000,
            &0,
            &3600,
        );
    }

    assert_eq!(client.get_stats().total_created, total);

    let non_empty_buckets = env.as_contract(&contract_id, || {
        let mut used = 0u32;
        for bucket in 0..crate::COUNTER_SHARDS {
            let key = DataKey::ShardedCounter(crate::internal::COUNTER_KIND_CREATED, bucket);
            let value: u64 = env.storage().persistent().get(&key).unwrap_or(0);
            if value > 0 {
                used += 1;
            }
        }
        used
    });
    assert!(
        non_empty_buckets > 1,
        "created counter should be spread across multiple shards"
    );
}

/// Shared fixture for the lifecycle-counter consistency tests below: a funded
/// escrow with a single seller payee and a single resolver.
struct CounterFixture<'a> {
    env: Env,
    contract_id: Address,
    client: EscrowClient<'a>,
    admin: Address,
    seller: Address,
    buyer: Address,
    resolver: Address,
    token: Address,
}

impl<'a> CounterFixture<'a> {
    fn new<'b>(env: &'b Env, arbitration_fee_bps: u32) -> CounterFixture<'b> {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let fee_collector = Address::generate(env);
        let seller = Address::generate(env);
        let buyer = Address::generate(env);
        let resolver = Address::generate(env);
        let token = env
            .register_stellar_asset_contract_v2(Address::generate(env))
            .address();
        token::StellarAssetClient::new(env, &token).mint(&buyer, &1_000);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(env, &contract_id);
        client.initialize(&admin, &fee_collector, &arbitration_fee_bps);

        let mut payees = Vec::new(env);
        payees.push_back(Payee {
            address: seller.clone(),
            bps: 10_000,
        });
        let escrow_id = client.create_escrow_8(
            &payees.into_val(env),
            &None::<Address>,
            &resolver,
            &token,
            &1_000_i128,
            &0_u32,
            &3600_u64,
        );
        client.fund_escrow(&escrow_id, &buyer);

        CounterFixture {
            env: env.clone(),
            contract_id,
            client,
            admin,
            seller,
            buyer,
            resolver,
            token,
        }
    }

    fn raise_and_resolve(&self, escrow_id: &u64) {
        self.client.mark_shipped(
            &self.seller,
            escrow_id,
            &String::from_str(&self.env, "TRK-COUNTER"),
        );
        let reason = Symbol::new(&self.env, "ITEM_NOT_RECEIVED");
        let description = Bytes::from_slice(&self.env, b"missing");
        let evidence = BytesN::from_array(&self.env, &[0u8; 32]);
        self.client
            .raise_dispute(&self.buyer, escrow_id, &reason, &description, &evidence);
        self.client
            .resolve_dispute(&self.resolver, escrow_id, &ResolutionType::Release);
    }
}

/// #1056: the `settle_escrow_to_payees` path (used by `confirm_delivery` and
/// `auto_release`) previously wrote **both** the legacy `TotalCompleted` key and
/// the sharded counter, so `get_stats().total_completed` counted a single
/// completion twice while `finalize_dispute`/`co_signed_release` counted once.
/// Every path must now go through the shard-only helper and count exactly one.
#[test]
fn completion_via_settle_counts_once() {
    let env = Env::default();
    let fx = CounterFixture::new(&env, 0);
    let id = 1_u64;

    fx.client
        .mark_shipped(&fx.seller, &id, &String::from_str(&env, "TRK-1"));
    fx.client.confirm_delivery(&fx.buyer, &id, &true);

    assert_eq!(
        fx.client.get_stats().total_completed,
        1,
        "a single confirm_delivery must count exactly one completion, not two"
    );
}

/// #1056: `co_signed_release` used to write only the legacy `TotalCompleted`
/// key (missed by the sharded reader's path), so it counted once but through a
/// different counter than every other completion path. It must use the same
/// single helper.
#[test]
fn co_signed_release_counts_once() {
    let env = Env::default();
    let fx = CounterFixture::new(&env, 0);
    let id = 1_u64;

    fx.client.co_signed_release(&fx.seller, &id);

    assert_eq!(fx.client.get_stats().total_completed, 1);
}

/// #1057: `emergency_drain` must advance `TotalRefunded` exactly once per drain
/// (no legacy dual-write, no double count).
#[test]
fn emergency_drain_counts_refund_once() {
    let env = Env::default();
    let fx = CounterFixture::new(&env, 0);
    let id = 1_u64;

    fx.client.pause_contract(&fx.admin);
    fx.client.emergency_drain(&id);

    assert_eq!(
        fx.client.get_stats().total_refunded,
        1,
        "a single emergency_drain must count exactly one refund"
    );
}

/// #1057: a dispute resolved to `Release` runs the arbitration/resolver fee
/// split once at resolve time and the completion at finalize time. `get_stats`
/// must reflect exactly one disputed escrow and one completion, and the
/// arbitration fee actually charged must be recorded in
/// `DataKey::TotalArbitrationFees(token)` so the refunded/collected split is
/// visible rather than undercounted.
#[test]
fn dispute_release_records_stats_and_arbitration_fee() {
    let env = Env::default();
    // 50 bps arbitration fee → 1_000 * 50 / 10_000 = 5.
    let fx = CounterFixture::new(&env, 50);
    let id = 1_u64;

    fx.raise_and_resolve(&id);

    let recorded_fee: i128 = env.as_contract(&fx.contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::TotalArbitrationFees(fx.token.clone()))
            .unwrap_or(0)
    });
    assert_eq!(
        recorded_fee, 5,
        "arbitration fee must be recorded exactly once at resolve time"
    );

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86_401);
    fx.client.finalize_dispute(&fx.resolver, &id);

    let stats = fx.client.get_stats();
    assert_eq!(stats.total_disputed, 1, "one dispute was raised");
    assert_eq!(
        stats.total_completed, 1,
        "a Release resolution finalizes as exactly one completion"
    );
    assert_eq!(stats.total_refunded, 0, "no refund on a Release resolution");
}

/// Issue #1068: incrementing a shard already at `u64::MAX` fails with
/// `ArithmeticError` and leaves the stored value untouched.
#[test]
fn sharded_counter_overflow_returns_arithmetic_error() {
    let env = Env::default();
    let contract_id = env.register(Escrow, ());
    let key = DataKey::ShardedCounter(crate::internal::COUNTER_KIND_CREATED, 0);

    env.as_contract(&contract_id, || {
        env.storage().persistent().set(&key, &u64::MAX);
        let res = crate::internal::increment_sharded_counter(
            &env,
            crate::internal::COUNTER_KIND_CREATED,
            0,
        );
        assert_eq!(res, Err(ContractError::ArithmeticError));
        let stored: u64 = env.storage().persistent().get(&key).unwrap();
        assert_eq!(stored, u64::MAX);
    });
}
