use soroban_sdk::{Address, Env};
use crate::{DataKey, DEFAULT_TTL_EXTENSION};

pub fn read_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).expect("not initialized")
}

pub fn write_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn read_fee_collector(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::FeeCollector).expect("fee collector not set")
}

pub fn write_fee_collector(env: &Env, collector: &Address) {
    env.storage().instance().set(&DataKey::FeeCollector, collector);
}

pub fn read_arbitration_fee(env: &Env) -> i128 {
    env.storage().instance().get(&DataKey::ArbitrationFee).unwrap_or(0)
}

pub fn write_arbitration_fee(env: &Env, fee: &i128) {
    env.storage().instance().set(&DataKey::ArbitrationFee, fee);
}

pub fn read_escrow_counter(env: &Env) -> u64 {
    env.storage().instance().get(&DataKey::EscrowCounter).expect("counter initialized")
}

pub fn write_escrow_counter(env: &Env, counter: &u64) {
    env.storage().instance().set(&DataKey::EscrowCounter, counter);
}

pub fn read_paused(env: &Env) -> bool {
    env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
}

pub fn write_paused(env: &Env, paused: &bool) {
    env.storage().instance().set(&DataKey::Paused, paused);
}

pub fn read_ttl_extension(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::TtlExtensionLedgers)
        .unwrap_or(DEFAULT_TTL_EXTENSION)
}
