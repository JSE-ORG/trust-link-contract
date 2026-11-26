use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::{DataKey, EscrowData, FeeConfig};

/// Number of escrow IDs stored in each user index page. Page zero contains IDs
/// `[0, PAGE_SIZE)`, page one `[PAGE_SIZE, 2 * PAGE_SIZE)`, and so on. Buyer
/// and vendor pages use different DataKey variants, so equal addresses and
/// page numbers can never alias.
pub const PAGE_SIZE: u64 = 10;

/// Default TTL extension (in ledgers) for persistent storage entries.
/// This matches the value used in lib.rs to ensure consistent behavior.
const DEFAULT_TTL_EXTENSION: u32 = 120_960;

/// Get the configured TTL extension from the contract, or use the default.
fn get_ttl_extension(env: &Env) -> u32 {
    use crate::DataKey;
    env.storage()
        .instance()
        .get(&DataKey::TtlExtensionLedgers)
        .unwrap_or(DEFAULT_TTL_EXTENSION)
}

/// Helper to extend TTL on a persistent storage key.
fn extend_ttl_for_key(env: &Env, key: &StorageKey) {
    let ext = get_ttl_extension(env);
    env.storage().persistent().extend_ttl(key, ext / 2, ext);
}

/// Typed keys for all contract storage entries.
///
/// Storage-tier rationale:
/// - Instance keys store singleton/global configuration and counters.
/// - Persistent keys store per-escrow data and user indexes that must survive
///   contract instance TTL changes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    // Instance storage: global singleton values.
    /// Stores the current admin address responsible for contract administration.
    AdminAddress,
    /// Stores the global fee configuration (protocol_fee_bps, arbitration_fee_bps).
    FeeConfig,
    /// Stores the monotonically incrementing counter used to generate unique escrow IDs.
    EscrowCounter,

    // Persistent storage: large, append-only, or user-scoped records.
    /// Stores complete escrow data (seller, buyer, token, amount, state, etc.) by escrow ID.
    EscrowData(u64),
    /// Stores list of escrow IDs associated with a vendor (seller) address for easy lookup.
    VendorEscrowIndex(Address),
    /// Stores list of escrow IDs associated with a buyer address for easy lookup.
    BuyerEscrowIndex(Address),
}

pub fn write_admin_address(env: &Env, admin: &Address) {
    env.storage()
        .instance()
        .set(&StorageKey::AdminAddress, admin);
}

pub fn read_admin_address(env: &Env) -> Option<Address> {
    env.storage().instance().get(&StorageKey::AdminAddress)
}

pub fn write_fee_config(env: &Env, fee_config: &FeeConfig) {
    env.storage()
        .instance()
        .set(&StorageKey::FeeConfig, fee_config);
}

pub fn read_fee_config(env: &Env) -> Option<FeeConfig> {
    env.storage().instance().get(&StorageKey::FeeConfig)
}

pub fn write_escrow_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&StorageKey::EscrowCounter, &counter);
}

pub fn read_escrow_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&StorageKey::EscrowCounter)
        .unwrap_or(0)
}

pub fn write_escrow_data(env: &Env, escrow_id: u64, escrow: &EscrowData) {
    let key = StorageKey::EscrowData(escrow_id);
    env.storage().persistent().set(&key, escrow);
    extend_ttl_for_key(env, &key);
}

pub fn read_escrow_data(env: &Env, escrow_id: u64) -> Option<EscrowData> {
    env.storage()
        .persistent()
        .get(&StorageKey::EscrowData(escrow_id))
}

pub fn write_vendor_escrow_index(env: &Env, vendor: &Address, escrow_ids: &Vec<u64>) {
    let key = StorageKey::VendorEscrowIndex(vendor.clone());
    env.storage().persistent().set(&key, escrow_ids);
    extend_ttl_for_key(env, &key);
}

pub fn read_vendor_escrow_index(env: &Env, vendor: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&StorageKey::VendorEscrowIndex(vendor.clone()))
        .unwrap_or(Vec::new(env))
}

pub fn write_buyer_escrow_index(env: &Env, buyer: &Address, escrow_ids: &Vec<u64>) {
    let key = StorageKey::BuyerEscrowIndex(buyer.clone());
    env.storage().persistent().set(&key, escrow_ids);
    extend_ttl_for_key(env, &key);
}

pub fn read_buyer_escrow_index(env: &Env, buyer: &Address) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&StorageKey::BuyerEscrowIndex(buyer.clone()))
        .unwrap_or(Vec::new(env))
}

fn append_paged_index(env: &Env, user: &Address, escrow_id: u64, buyer: bool) {
    let count_key = if buyer {
        DataKey::BuyerEscrowCount(user.clone())
    } else {
        DataKey::VendorEscrowCount(user.clone())
    };
    let count: u64 = env.storage().persistent().get(&count_key).unwrap_or(0);
    let page = count / PAGE_SIZE;
    let page_key = if buyer {
        DataKey::BuyerEscrow(user.clone(), page as u32)
    } else {
        DataKey::VendorEscrow(user.clone(), page as u32)
    };
    let mut ids: Vec<u64> = env.storage().persistent().get(&page_key).unwrap_or(Vec::new(env));
    ids.push_back(escrow_id);
    env.storage().persistent().set(&page_key, &ids);
    env.storage().persistent().set(&count_key, &(count + 1));
    let ext = get_ttl_extension(env);
    env.storage().persistent().extend_ttl(&page_key, ext / 2, ext);
    env.storage().persistent().extend_ttl(&count_key, ext / 2, ext);
}

/// Append to the buyer namespace and its independent counter.
pub fn append_buyer_escrow(env: &Env, buyer: &Address, escrow_id: u64) {
    append_paged_index(env, buyer, escrow_id, true);
}

/// Append to the vendor namespace and its independent counter.
pub fn append_vendor_escrow(env: &Env, vendor: &Address, escrow_id: u64) {
    append_paged_index(env, vendor, escrow_id, false);
}

pub fn read_paged_index(env: &Env, user: &Address, page: u32, buyer: bool) -> Vec<u64> {
    let key = if buyer {
        DataKey::BuyerEscrow(user.clone(), page)
    } else {
        DataKey::VendorEscrow(user.clone(), page)
    };
    env.storage().persistent().get(&key).unwrap_or(Vec::new(env))
}

pub fn read_paged_index_count(env: &Env, user: &Address, buyer: bool) -> u64 {
    let key = if buyer {
        DataKey::BuyerEscrowCount(user.clone())
    } else {
        DataKey::VendorEscrowCount(user.clone())
    };
    env.storage().persistent().get(&key).unwrap_or(0)
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
    fn paged_buyer_and_vendor_indexes_are_isolated_at_rollover() {
        let env = Env::default();
        let contract_id = env.register(Escrow, ());
        let user = Address::generate(&env);
        env.as_contract(&contract_id, || {
            for id in 1..=20u64 {
                append_buyer_escrow(&env, &user, id);
                append_vendor_escrow(&env, &user, id + 100);
            }
        });

        let (buyers, vendors, buyer_count, vendor_count) = env.as_contract(&contract_id, || {
            (
                (read_paged_index(&env, &user, 0, true), read_paged_index(&env, &user, 1, true)),
                (read_paged_index(&env, &user, 0, false), read_paged_index(&env, &user, 1, false)),
                read_paged_index_count(&env, &user, true),
                read_paged_index_count(&env, &user, false),
            )
        });
        assert_eq!(buyers.0.len(), 10);
        assert_eq!(buyers.1.len(), 10);
        assert_eq!(vendors.0.len(), 10);
        assert_eq!(vendors.1.len(), 10);
        assert_eq!(buyers.0.get(0), Some(1));
        assert_eq!(buyers.1.get(0), Some(11));
        assert_eq!(vendors.0.get(0), Some(101));
        assert_eq!(vendors.1.get(0), Some(111));
        assert_eq!(buyer_count, 20);
        assert_eq!(vendor_count, 20);
    }
}
