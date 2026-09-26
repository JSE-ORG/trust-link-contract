use soroban_sdk::{Address, Env, Vec};

use crate::types::Message;
use crate::{
    ContractError, DataKey, EscrowData, FeeConfig, TimelockProposal, DEFAULT_TTL_EXTENSION,
    TTL_THRESHOLD_DIVISOR,
};

// ============================================================================
// TTL CONSTANTS — documented here as the canonical reference.
//
// DEFAULT_TTL_EXTENSION (120_960 ledgers ≈ 13.7 days at 10 s/ledger) is the
// fallback used when no admin-configured value exists in instance storage.
// The threshold for triggering an extension is always set to
// `ext / TTL_THRESHOLD_DIVISOR` so that the key is kept alive as long as it
// continues to be accessed before crossing the configured threshold.
//
// Both instance and persistent entries use the same value so that all storage
// tiers expire on the same schedule.  Admins can override the value via
// `set_ttl_extension`.
// ============================================================================

/// Get the configured TTL extension from the contract, or use the default.
pub fn get_ttl_extension(env: &Env) -> u32 {
    use crate::DataKey;
    env.storage()
        .instance()
        .get(&DataKey::TtlExtensionLedgers)
        .unwrap_or(DEFAULT_TTL_EXTENSION)
}

/// Extend the instance-storage TTL.
///
/// Called on every public entry point so the singleton configuration keys
/// (Admin, FeeConfig, EscrowCounter, etc.) never expire between interactions.
pub fn extend_instance_ttl(env: &Env) {
    let ext = get_ttl_extension(env);
    env.storage()
        .instance()
        .extend_ttl(ext / TTL_THRESHOLD_DIVISOR, ext);
}

/// Helper to extend TTL on a persistent storage key.
fn extend_ttl_for_key(env: &Env, key: &DataKey) {
    let ext = get_ttl_extension(env);
    env.storage()
        .persistent()
        .extend_ttl(key, ext / TTL_THRESHOLD_DIVISOR, ext);
}

// ── Messages, stored one-per-key for true pagination ────────────────────────

/// Number of messages stored for `escrow_id`.
///
/// Falls back to the pre-paging `Messages(escrow_id)` vector's length for
/// contracts written before paging existed.
pub fn read_message_count(env: &Env, escrow_id: u64) -> u32 {
    let key = DataKey::MessageCount(escrow_id);
    let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);
    if count == 0 {
        let legacy_key = DataKey::Messages(escrow_id);
        if let Some(legacy) = env
            .storage()
            .persistent()
            .get::<DataKey, Vec<Message>>(&legacy_key)
        {
            return legacy.len();
        }
    }
    count
}

/// Reads the single message stored at `Message(escrow_id, index)`, if any.
/// This is the targeted read that makes `get_messages` pagination cheap: only
/// the requested page's slots are deserialised, never the full thread.
pub fn read_message_at(env: &Env, escrow_id: u64, index: u32) -> Option<Message> {
    let key = DataKey::Message(escrow_id, index);
    if let Some(message) = env.storage().persistent().get::<DataKey, Message>(&key) {
        extend_ttl_for_key(env, &key);
        return Some(message);
    }
    // Backward compatibility with the monolithic layout.
    let legacy_key = DataKey::Messages(escrow_id);
    env.storage()
        .persistent()
        .get::<DataKey, Vec<Message>>(&legacy_key)
        .and_then(|legacy| legacy.get(index))
}

/// Appends `message` as the next indexed entry for `escrow_id`, rejecting once
/// `cap` messages already exist. Only the new key and the count are written.
pub fn append_message(
    env: &Env,
    escrow_id: u64,
    message: &Message,
    cap: u32,
) -> Result<(), ContractError> {
    migrate_legacy_messages(env, escrow_id);
    let count_key = DataKey::MessageCount(escrow_id);
    let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
    if count >= cap {
        return Err(ContractError::TooManyMessages);
    }
    let key = DataKey::Message(escrow_id, count);
    env.storage().persistent().set(&key, message);
    extend_ttl_for_key(env, &key);
    env.storage().persistent().set(&count_key, &(count + 1));
    extend_ttl_for_key(env, &count_key);
    Ok(())
}

/// One-time relocation of `escrow_id`'s messages from the pre-paging vector
/// into per-index keys, so an upgraded contract keeps its thread and can keep
/// appending.
fn migrate_legacy_messages(env: &Env, escrow_id: u64) {
    let count_key = DataKey::MessageCount(escrow_id);
    if env.storage().persistent().has(&count_key) {
        return;
    }
    let legacy_key = DataKey::Messages(escrow_id);
    if !env.storage().persistent().has(&legacy_key) {
        return;
    }
    let legacy: Vec<Message> = env
        .storage()
        .persistent()
        .get(&legacy_key)
        .unwrap_or(Vec::new(env));
    let mut index: u32 = 0;
    for message in legacy.iter() {
        let key = DataKey::Message(escrow_id, index);
        env.storage().persistent().set(&key, &message);
        extend_ttl_for_key(env, &key);
        index += 1;
    }
    env.storage().persistent().set(&count_key, &index);
    extend_ttl_for_key(env, &count_key);
    env.storage().persistent().remove(&legacy_key);
}

/// Typed keys for all contract storage entries.
///
/// Storage-tier rationale:
/// - Instance keys store singleton/global configuration and counters.
/// - Persistent keys store per-escrow data and user indexes that must survive
///   contract instance TTL changes.
///
/// Storage helpers use the unified `DataKey` enum defined in `types.rs`.
pub fn write_admin_address(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn read_admin_address(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Admin)
}

pub fn write_fee_config(env: &Env, fee_config: &FeeConfig) {
    env.storage()
        .instance()
        .set(&DataKey::FeeConfig, fee_config);
}

pub fn read_fee_config(env: &Env) -> Option<FeeConfig> {
    env.storage().instance().get(&DataKey::FeeConfig)
}

pub fn write_escrow_counter(env: &Env, counter: u64) {
    env.storage()
        .instance()
        .set(&DataKey::EscrowCounter, &counter);
}

pub fn read_escrow_counter(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::EscrowCounter)
        .unwrap_or(0)
}

pub fn write_escrow_data(env: &Env, escrow_id: u64, escrow: &EscrowData) {
    let key = DataKey::Escrow(escrow_id);
    env.storage().persistent().set(&key, escrow);
    extend_ttl_for_key(env, &key);
}

pub fn read_escrow_data(env: &Env, escrow_id: u64) -> Option<EscrowData> {
    let key = DataKey::Escrow(escrow_id); // Ensure this matches DataKey enum
    let result = env.storage().persistent().get(&key);
    if result.is_some() {
        extend_ttl_for_key(env, &key);
    }
    result
}

fn page_count(total: u32) -> u32 {
    total.div_ceil(crate::ESCROW_INDEX_PAGE_SIZE)
}

fn read_index_count(env: &Env, key: &DataKey) -> u32 {
    env.storage().persistent().get(key).unwrap_or(0)
}

// ── Vendor (seller) escrow index, sharded into fixed-size pages ──────────────

/// Appends `escrow_id` to `vendor`'s paged escrow index, touching only the
/// current tail page instead of rewriting the whole index.
pub fn append_vendor_escrow_index(env: &Env, vendor: &Address, escrow_id: u64) {
    migrate_legacy_vendor_index(env, vendor);
    let count = read_index_count(env, &DataKey::VendorEscrowCount(vendor.clone()));
    let page_key = DataKey::VendorEscrow(vendor.clone(), count / crate::ESCROW_INDEX_PAGE_SIZE);
    let mut page: Vec<u64> = env
        .storage()
        .persistent()
        .get(&page_key)
        .unwrap_or(Vec::new(env));
    page.push_back(escrow_id);
    env.storage().persistent().set(&page_key, &page);
    extend_ttl_for_key(env, &page_key);

    let count_key = DataKey::VendorEscrowCount(vendor.clone());
    env.storage().persistent().set(&count_key, &(count + 1));
    extend_ttl_for_key(env, &count_key);
}

/// Total number of escrow ids indexed for `vendor`.
pub fn read_vendor_escrow_count(env: &Env, vendor: &Address) -> u32 {
    read_index_count(env, &DataKey::VendorEscrowCount(vendor.clone()))
}

/// Reads every escrow id indexed for `vendor`, reassembling the pages in order.
pub fn read_vendor_escrow_index(env: &Env, vendor: &Address) -> Vec<u64> {
    let count = read_vendor_escrow_count(env, vendor);
    if count == 0 {
        return read_legacy_vendor_index(env, vendor);
    }
    let mut result = Vec::new(env);
    let mut page_idx = 0;
    while page_idx < page_count(count) {
        let key = DataKey::VendorEscrow(vendor.clone(), page_idx);
        if let Some(page) = env.storage().persistent().get::<DataKey, Vec<u64>>(&key) {
            for id in page.iter() {
                result.push_back(id);
            }
            extend_ttl_for_key(env, &key);
        }
        page_idx += 1;
    }
    result
}

/// Overwrites `vendor`'s index with `escrow_ids`, re-sharding it into pages.
pub fn write_vendor_escrow_index(env: &Env, vendor: &Address, escrow_ids: &Vec<u64>) {
    clear_pages(
        env,
        |page| DataKey::VendorEscrow(vendor.clone(), page),
        read_vendor_escrow_count(env, vendor),
    );
    write_pages(
        env,
        |page| DataKey::VendorEscrow(vendor.clone(), page),
        &DataKey::VendorEscrowCount(vendor.clone()),
        escrow_ids,
    );
}

fn read_legacy_vendor_index(env: &Env, vendor: &Address) -> Vec<u64> {
    let key = DataKey::VendorEscrowIndex(vendor.clone());
    let result: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    if env.storage().persistent().has(&key) {
        extend_ttl_for_key(env, &key);
    }
    result
}

/// Moves a pre-paging `VendorEscrowIndex(addr)` vector into the paged layout on
/// first write, so upgraded contracts keep their existing index.
fn migrate_legacy_vendor_index(env: &Env, vendor: &Address) {
    if read_vendor_escrow_count(env, vendor) != 0 {
        return;
    }
    let legacy_key = DataKey::VendorEscrowIndex(vendor.clone());
    if !env.storage().persistent().has(&legacy_key) {
        return;
    }
    let legacy: Vec<u64> = env
        .storage()
        .persistent()
        .get(&legacy_key)
        .unwrap_or(Vec::new(env));
    write_vendor_escrow_index(env, vendor, &legacy);
    env.storage().persistent().remove(&legacy_key);
}

// ── Buyer escrow index, sharded into fixed-size pages ───────────────────────

/// Appends `escrow_id` to `buyer`'s paged escrow index.
pub fn append_buyer_escrow_index(env: &Env, buyer: &Address, escrow_id: u64) {
    migrate_legacy_buyer_index(env, buyer);
    let count = read_index_count(env, &DataKey::BuyerEscrowCount(buyer.clone()));
    let page_key = DataKey::BuyerEscrow(buyer.clone(), count / crate::ESCROW_INDEX_PAGE_SIZE);
    let mut page: Vec<u64> = env
        .storage()
        .persistent()
        .get(&page_key)
        .unwrap_or(Vec::new(env));
    page.push_back(escrow_id);
    env.storage().persistent().set(&page_key, &page);
    extend_ttl_for_key(env, &page_key);

    let count_key = DataKey::BuyerEscrowCount(buyer.clone());
    env.storage().persistent().set(&count_key, &(count + 1));
    extend_ttl_for_key(env, &count_key);
}

/// Total number of escrow ids indexed for `buyer`.
pub fn read_buyer_escrow_count(env: &Env, buyer: &Address) -> u32 {
    read_index_count(env, &DataKey::BuyerEscrowCount(buyer.clone()))
}

/// Reads every escrow id indexed for `buyer`, reassembling the pages in order.
pub fn read_buyer_escrow_index(env: &Env, buyer: &Address) -> Vec<u64> {
    let count = read_buyer_escrow_count(env, buyer);
    if count == 0 {
        return read_legacy_buyer_index(env, buyer);
    }
    let mut result = Vec::new(env);
    let mut page_idx = 0;
    while page_idx < page_count(count) {
        let key = DataKey::BuyerEscrow(buyer.clone(), page_idx);
        if let Some(page) = env.storage().persistent().get::<DataKey, Vec<u64>>(&key) {
            for id in page.iter() {
                result.push_back(id);
            }
            extend_ttl_for_key(env, &key);
        }
        page_idx += 1;
    }
    result
}

/// Overwrites `buyer`'s index with `escrow_ids`, re-sharding it into pages.
pub fn write_buyer_escrow_index(env: &Env, buyer: &Address, escrow_ids: &Vec<u64>) {
    clear_pages(
        env,
        |page| DataKey::BuyerEscrow(buyer.clone(), page),
        read_buyer_escrow_count(env, buyer),
    );
    write_pages(
        env,
        |page| DataKey::BuyerEscrow(buyer.clone(), page),
        &DataKey::BuyerEscrowCount(buyer.clone()),
        escrow_ids,
    );
}

fn read_legacy_buyer_index(env: &Env, buyer: &Address) -> Vec<u64> {
    let key = DataKey::BuyerEscrowIndex(buyer.clone());
    let result: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    if env.storage().persistent().has(&key) {
        extend_ttl_for_key(env, &key);
    }
    result
}

/// Moves a pre-paging `BuyerEscrowIndex(addr)` vector into the paged layout on
/// first write.
fn migrate_legacy_buyer_index(env: &Env, buyer: &Address) {
    if read_buyer_escrow_count(env, buyer) != 0 {
        return;
    }
    let legacy_key = DataKey::BuyerEscrowIndex(buyer.clone());
    if !env.storage().persistent().has(&legacy_key) {
        return;
    }
    let legacy: Vec<u64> = env
        .storage()
        .persistent()
        .get(&legacy_key)
        .unwrap_or(Vec::new(env));
    write_buyer_escrow_index(env, buyer, &legacy);
    env.storage().persistent().remove(&legacy_key);
}

/// Removes every page belonging to an index whose previous entry count was
/// `count`, so a rewrite can't leave stale tail pages behind.
fn clear_pages(env: &Env, page_key: impl Fn(u32) -> DataKey, count: u32) {
    if count == 0 {
        return;
    }
    let mut page_idx = 0;
    while page_idx < page_count(count) {
        env.storage().persistent().remove(&page_key(page_idx));
        page_idx += 1;
    }
}

/// Writes `ids` into fixed-size pages and stores the total count under
/// `count_key`.
fn write_pages(env: &Env, page_key: impl Fn(u32) -> DataKey, count_key: &DataKey, ids: &Vec<u64>) {
    let total = ids.len();
    let mut i = 0;
    let mut page_idx = 0;
    while i < total {
        let mut page = Vec::new(env);
        let mut in_page = 0;
        while in_page < crate::ESCROW_INDEX_PAGE_SIZE && i < total {
            if let Some(id) = ids.get(i) {
                page.push_back(id);
            }
            i += 1;
            in_page += 1;
        }
        let key = page_key(page_idx);
        env.storage().persistent().set(&key, &page);
        extend_ttl_for_key(env, &key);
        page_idx += 1;
    }
    env.storage().persistent().set(count_key, &total);
    extend_ttl_for_key(env, count_key);
}

pub fn write_timelock_proposal(env: &Env, operation: u32, proposal: &TimelockProposal) {
    let key = DataKey::TimelockOp(operation);
    env.storage().instance().set(&key, proposal);
    extend_instance_ttl(env);
}

pub fn read_timelock_proposal(env: &Env, operation: u32) -> Option<TimelockProposal> {
    let key = DataKey::TimelockOp(operation);
    let result: Option<TimelockProposal> = env.storage().instance().get(&key);
    if result.is_some() {
        extend_instance_ttl(env);
    }
    result
}

pub fn remove_timelock_proposal(env: &Env, operation: u32) {
    let key = DataKey::TimelockOp(operation);
    if env.storage().instance().has(&key) {
        env.storage().instance().remove(&key);
    }
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

        let got_vendor = env.as_contract(&contract_id, || read_vendor_escrow_index(&env, &addr));
        let got_buyer = env.as_contract(&contract_id, || read_buyer_escrow_index(&env, &addr));

        assert_eq!(
            got_vendor, vendor_ids,
            "VendorEscrowIndex and BuyerEscrowIndex must not collide for the same address"
        );
        assert_eq!(
            got_buyer, buyer_ids,
            "BuyerEscrowIndex must be independent of VendorEscrowIndex"
        );
    }
}
