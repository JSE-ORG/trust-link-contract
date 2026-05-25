use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::EscrowData;

#[contracttype]
pub enum DataKey {
    Escrow(u32),
    VendorEscrows(Address),
    BuyerEscrows(Address),
    EscrowCount,
    Admin,
    FeeConfig,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeConfig {
    pub recipient: Option<Address>,
    pub fee_bps: u32,
}

impl FeeConfig {
    pub fn disabled() -> Self {
        Self {
            recipient: None,
            fee_bps: 0,
        }
    }
}

// Instance storage is used for all helpers because these records are core
// contract-instance state. They must survive across ledgers with the contract,
// but they do not need temporary storage or cross-instance persistent buckets.
pub fn read_escrow(env: &Env, escrow_id: u32) -> Option<EscrowData> {
    env.storage().instance().get(&DataKey::Escrow(escrow_id))
}

pub fn require_escrow(env: &Env, escrow_id: u32) -> EscrowData {
    read_escrow(env, escrow_id).expect("escrow not found")
}

pub fn write_escrow(env: &Env, escrow_id: u32, escrow: &EscrowData) {
    env.storage()
        .instance()
        .set(&DataKey::Escrow(escrow_id), escrow);
}

pub fn read_escrow_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::EscrowCount)
        .unwrap_or(0)
}

pub fn write_escrow_count(env: &Env, count: u32) {
    env.storage().instance().set(&DataKey::EscrowCount, &count);
}

pub fn next_escrow_id(env: &Env) -> u32 {
    let next = read_escrow_count(env) + 1;
    write_escrow_count(env, next);
    next
}

pub fn read_vendor_index(env: &Env, vendor: &Address) -> Vec<u32> {
    env.storage()
        .instance()
        .get(&DataKey::VendorEscrows(vendor.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn write_vendor_index(env: &Env, vendor: &Address, escrow_ids: &Vec<u32>) {
    env.storage()
        .instance()
        .set(&DataKey::VendorEscrows(vendor.clone()), escrow_ids);
}

pub fn append_vendor_escrow(env: &Env, vendor: &Address, escrow_id: u32) {
    let mut escrow_ids = read_vendor_index(env, vendor);
    escrow_ids.push_back(escrow_id);
    write_vendor_index(env, vendor, &escrow_ids);
}

pub fn read_buyer_index(env: &Env, buyer: &Address) -> Vec<u32> {
    env.storage()
        .instance()
        .get(&DataKey::BuyerEscrows(buyer.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn write_buyer_index(env: &Env, buyer: &Address, escrow_ids: &Vec<u32>) {
    env.storage()
        .instance()
        .set(&DataKey::BuyerEscrows(buyer.clone()), escrow_ids);
}

pub fn append_buyer_escrow(env: &Env, buyer: &Address, escrow_id: u32) {
    let mut escrow_ids = read_buyer_index(env, buyer);
    escrow_ids.push_back(escrow_id);
    write_buyer_index(env, buyer, &escrow_ids);
}

pub fn read_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Admin)
}

pub fn write_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn read_fee_config(env: &Env) -> FeeConfig {
    env.storage()
        .instance()
        .get(&DataKey::FeeConfig)
        .unwrap_or_else(FeeConfig::disabled)
}

pub fn write_fee_config(env: &Env, config: &FeeConfig) {
    env.storage().instance().set(&DataKey::FeeConfig, config);
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::EscrowState;
    use soroban_sdk::testutils::Address as _;

    fn sample_escrow(env: &Env) -> EscrowData {
        EscrowData {
            seller: Address::generate(env),
            buyer: None,
            resolver: Address::generate(env),
            token: Address::generate(env),
            amount: 100,
            shipping_window: 3_600,
            funded_at: 0,
            state: EscrowState::Pending,
        }
    }

    #[test]
    fn escrow_helpers_roundtrip() {
        let env = Env::default();
        let contract_id = env.register(crate::Escrow, ());
        let escrow = sample_escrow(&env);

        env.as_contract(&contract_id, || {
            assert!(read_escrow(&env, 7).is_none());

            write_escrow(&env, 7, &escrow);
            assert_eq!(read_escrow(&env, 7), Some(escrow.clone()));
            assert_eq!(require_escrow(&env, 7), escrow);
        });
    }

    #[test]
    fn counter_helper_allocates_monotonic_ids() {
        let env = Env::default();
        let contract_id = env.register(crate::Escrow, ());

        env.as_contract(&contract_id, || {
            assert_eq!(read_escrow_count(&env), 0);
            assert_eq!(next_escrow_id(&env), 1);
            assert_eq!(next_escrow_id(&env), 2);
            assert_eq!(read_escrow_count(&env), 2);
        });
    }

    #[test]
    fn vendor_and_buyer_indexes_accumulate_ids() {
        let env = Env::default();
        let contract_id = env.register(crate::Escrow, ());
        let vendor = Address::generate(&env);
        let buyer = Address::generate(&env);

        env.as_contract(&contract_id, || {
            append_vendor_escrow(&env, &vendor, 1);
            append_vendor_escrow(&env, &vendor, 3);
            append_buyer_escrow(&env, &buyer, 2);

            let vendor_ids = read_vendor_index(&env, &vendor);
            assert_eq!(vendor_ids.len(), 2);
            assert_eq!(vendor_ids.get(0), Some(1));
            assert_eq!(vendor_ids.get(1), Some(3));

            let buyer_ids = read_buyer_index(&env, &buyer);
            assert_eq!(buyer_ids.len(), 1);
            assert_eq!(buyer_ids.get(0), Some(2));
        });
    }

    #[test]
    fn admin_and_fee_config_helpers_roundtrip() {
        let env = Env::default();
        let contract_id = env.register(crate::Escrow, ());
        let admin = Address::generate(&env);
        let recipient = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert_eq!(read_admin(&env), None);
            assert_eq!(read_fee_config(&env), FeeConfig::disabled());

            write_admin(&env, &admin);
            write_fee_config(
                &env,
                &FeeConfig {
                    recipient: Some(recipient.clone()),
                    fee_bps: 75,
                },
            );

            assert_eq!(read_admin(&env), Some(admin));
            assert_eq!(
                read_fee_config(&env),
                FeeConfig {
                    recipient: Some(recipient),
                    fee_bps: 75,
                }
            );
        });
    }
}
