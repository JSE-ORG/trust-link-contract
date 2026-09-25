#![cfg(test)]
//! Verifies the lifecycle counters are sharded across persistent buckets (to
//! avoid a single contended instance key) and re-aggregated by `get_stats`.

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

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
