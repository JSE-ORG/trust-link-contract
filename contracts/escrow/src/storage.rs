use soroban_sdk::{Address, Env, Vec};

use crate::{DataKey, EscrowData, FeeConfig};

/// Default TTL extension (in ledgers) for persistent storage entries.
/// This matches the value used in lib.rs to ensure consistent behavior.
const DEFAULT_TTL_EXTENSION: u32 = 120_960;

/// Get the configured TTL extension from the contract, or use the default.
fn get_ttl_extension(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::TtlExtensionLedgers)
        .unwrap_or(DEFAULT_TTL_EXTENSION)
}

/// Helper to extend TTL on a persistent storage key.
fn extend_ttl_for_key(env: &Env, key: &DataKey) {
    let ext = get_ttl_extension(env);
    env.storage().persistent().extend_ttl(key, ext / 2, ext);
}

/// Typed keys for all contract storage entries.
///
/// Storage-tier rationale:
/// - Instance keys store singleton/global configuration and counters.
/// - Persistent keys store per-escrow data and user indexes that must survive
///   contract instance TTL changes.
// Storage helpers use the unified `DataKey` enum defined in `types.rs`.

pub fn write_admin_address(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn read_admin_address(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Admin)
}

pub fn write_fee_config(env: &Env, fee_config: &FeeConfig) {
    env.storage().instance().set(&DataKey::FeeConfig, fee_config);
}

pub fn read_fee_config(env: &Env) -> Option<FeeConfig> {
    env.storage().instance().get(&DataKey::FeeConfig)
}

pub fn write_escrow_counter(env: &Env, counter: u64) {
    env.storage().instance().set(&DataKey::EscrowCounter, &counter);
}

pub fn read_escrow_counter(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::EscrowCounter).unwrap_or(0)
}

pub fn write_escrow_data(env: &Env, escrow_id: u64, escrow: &EscrowData) {
    let key = DataKey::Escrow(escrow_id);
    env.storage().persistent().set(&key, escrow);
    extend_ttl_for_key(env, &key);
}

pub fn read_escrow_data(env: &Env, escrow_id: u64) -> Option<EscrowData> {
    env.storage().persistent().get(&DataKey::Escrow(escrow_id))
}

pub fn write_vendor_escrow_index(env: &Env, vendor: &Address, escrow_ids: &Vec<u64>) {
    let key = DataKey::VendorEscrowIndex(vendor.clone());
    env.storage().persistent().set(&key, escrow_ids);
    extend_ttl_for_key(env, &key);
}

pub fn read_vendor_escrow_index(env: &Env, vendor: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::VendorEscrowIndex(vendor.clone()))
        .unwrap_or(Vec::new(env))
}

pub fn write_buyer_escrow_index(env: &Env, buyer: &Address, escrow_ids: &Vec<u64>) {
    let key = DataKey::BuyerEscrowIndex(buyer.clone());
    env.storage().persistent().set(&key, escrow_ids);
    extend_ttl_for_key(env, &key);
}

pub fn read_buyer_escrow_index(env: &Env, buyer: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
        .unwrap_or(Vec::new(env))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Escrow, EscrowClient};
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn admin_and_counter_helpers_roundtrip() {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let _client = EscrowClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.as_contract(&contract_id, || {
            write_admin_address(&env, &admin);
            write_escrow_counter(&env, 42);
        });

        let read_admin = env.as_contract(&contract_id, || read_admin_address(&env));
        let read_counter = env.as_contract(&contract_id, || read_escrow_counter(&env));

        assert_eq!(read_admin, Some(admin));
        assert_eq!(read_counter, 42);
    }

    #[test]
    fn vendor_and_buyer_index_helpers_roundtrip() {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let vendor = Address::generate(&env);
        let buyer = Address::generate(&env);

        let mut vendor_ids = Vec::new(&env);
        vendor_ids.push_back(1);
        vendor_ids.push_back(7);

        let mut buyer_ids = Vec::new(&env);
        buyer_ids.push_back(2);
        buyer_ids.push_back(9);

        env.as_contract(&contract_id, || {
            write_vendor_escrow_index(&env, &vendor, &vendor_ids);
            write_buyer_escrow_index(&env, &buyer, &buyer_ids);
        });

        let read_vendors =
            env.as_contract(&contract_id, || read_vendor_escrow_index(&env, &vendor));
        let read_buyers = env.as_contract(&contract_id, || read_buyer_escrow_index(&env, &buyer));

        assert_eq!(read_vendors, vendor_ids);
        assert_eq!(read_buyers, buyer_ids);
    }

    #[test]
    fn unified_key_enum_no_collision_between_buyer_and_vendor() {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let addr = Address::generate(&env);

        let mut vendor_ids = Vec::new(&env);
        vendor_ids.push_back(10u64);

        let mut buyer_ids = Vec::new(&env);
        buyer_ids.push_back(20u64);

        env.as_contract(&contract_id, || {
            write_vendor_escrow_index(&env, &addr, &vendor_ids);
            write_buyer_escrow_index(&env, &addr, &buyer_ids);
        });

        let got_vendor =
            env.as_contract(&contract_id, || read_vendor_escrow_index(&env, &addr));
        let got_buyer =
            env.as_contract(&contract_id, || read_buyer_escrow_index(&env, &addr));

        assert_eq!(got_vendor, vendor_ids, "VendorEscrowIndex and BuyerEscrowIndex must not collide for the same address");
        assert_eq!(got_buyer, buyer_ids, "BuyerEscrowIndex must be independent of VendorEscrowIndex");
    }
}
