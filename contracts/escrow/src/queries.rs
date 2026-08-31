//! Read-only views over escrow, fee, and contract-wide state.

use crate::internal::*;
use crate::types::Message;
use crate::*;
use soroban_sdk::{contractimpl, Address, Env, Vec};

#[contractimpl]
impl Escrow {
    /// Retrieves messages for a given escrow with pagination.
    pub fn get_messages(env: Env, escrow_id: u64, start: u64, limit: u64) -> Vec<Message> {
        let max_limit = if limit > crate::MAX_MESSAGES_PER_PAGE {
            crate::MAX_MESSAGES_PER_PAGE
        } else {
            limit
        };
        let key = DataKey::Messages(escrow_id);
        let msgs: Vec<Message> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));
        let total = msgs.len() as u64;
        let mut result = Vec::new(&env);
        if start >= total {
            return result;
        }
        let end = (start + max_limit).min(total);
        let mut i = start;
        while i < end {
            if let Some(m) = msgs.get(i as u32) {
                result.push_back(m.clone());
            }
            i += 1;
        }
        result
    }

    /// Returns the full list of tokens and amounts for a basket escrow.
    /// Returns `None` if the escrow does not exist. Returns `Some(empty Vec)`
    /// for single-token escrows (created via `create_escrow`). Returns
    /// `Some(Vec<TokenEntry>)` for basket escrows with additional tokens.
    pub fn get_basket_tokens(env: Env, escrow_id: u64) -> Option<Vec<TokenEntry>> {
        // Check if escrow exists first
        if load_escrow(&env, escrow_id).is_err() {
            return None;
        }
        // Return Some(Vec) - empty for single-token, populated for basket
        Some(load_basket_tokens(&env, escrow_id))
    }

    /// Returns the full escrow record for `escrow_id`. Reverts with
    /// `EscrowNotFound` if it does not exist.
    pub fn get_escrow(env: Env, escrow_id: u64) -> Result<EscrowData, ContractError> {
        load_escrow(&env, escrow_id)
    }

    /// Returns the full state transition history for an escrow as
    /// `(state, ledger_timestamp)` pairs, oldest first. Returns
    /// `EscrowNotFound` if the escrow does not exist.
    pub fn get_state_history(
        env: Env,
        escrow_id: u64,
    ) -> Result<Vec<(EscrowState, u64)>, ContractError> {
        // Verify escrow exists first - this returns EscrowNotFound if missing
        load_escrow(&env, escrow_id)?;
        // Load history without extending TTL for non-matching escrows
        Ok(load_state_history_no_ttl(&env, escrow_id))
    }

    /// Retrieves all escrow IDs associated with a specific buyer.
    /// Uses the buyer index if available; otherwise falls back to scanning
    /// up to 1000 most recent escrows (capped to avoid budget exhaustion).
    /// The fallback scan avoids extending TTL for non-matching escrows.
    pub fn get_escrows_by_buyer(env: Env, buyer: Address) -> Vec<u64> {
        if let Some(ids) = env
            .storage()
            .persistent()
            .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
        {
            return ids;
        }
        let mut result = Vec::new(&env);
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(1);

        let max_iterations = 1000;
        for (iterations, id) in (1..counter).rev().enumerate() {
            if iterations >= max_iterations {
                break;
            }
            // Check if escrow key exists without TTL extension
            let key = DataKey::Escrow(id);
            if env.storage().persistent().has(&key) {
                // Only extend TTL if this escrow matches the buyer
                if let Ok(escrow) = load_escrow(&env, id) {
                    if escrow.buyer.as_ref() == Some(&buyer) {
                        result.push_back(id);
                    }
                }
            }
        }
        result
    }

    /// Batch view: return escrows for the supplied IDs in the same order.
    /// Missing IDs return None in the corresponding slot. Input is capped at
    /// MAX_MESSAGES_PER_PAGE (50 IDs) to prevent resource exhaustion.
    pub fn get_escrows_by_ids(
        env: Env,
        ids: soroban_sdk::Vec<u64>,
    ) -> soroban_sdk::Vec<Option<EscrowData>> {
        let mut result: soroban_sdk::Vec<Option<EscrowData>> = soroban_sdk::Vec::new(&env);
        let max_ids = crate::MAX_MESSAGES_PER_PAGE as u32;
        let limit = if ids.len() > max_ids {
            max_ids
        } else {
            ids.len()
        };

        for i in 0..limit {
            let Some(id) = ids.get(i) else {
                result.push_back(None);
                continue;
            };
            match load_escrow(&env, id) {
                Ok(escrow) => result.push_back(Some(escrow)),
                Err(_) => result.push_back(None),
            }
        }
        result
    }

    /// Returns the current fee configuration.
    pub fn get_fee_config(env: Env) -> FeeConfig {
        read_fee_config(&env)
    }

    /// Retrieves all escrow IDs associated with a specific vendor (seller).
    pub fn get_escrows_by_vendor(env: Env, vendor: Address) -> Vec<u64> {
        storage::read_vendor_escrow_index(&env, &vendor)
    }

    /// Retrieves all escrow IDs associated with a specific seller.
    pub fn get_escrows_by_seller(env: Env, seller: Address) -> Vec<u64> {
        storage::read_vendor_escrow_index(&env, &seller)
    }

    /// Returns on-chain counters for escrow lifecycle events.
    pub fn get_stats(env: Env) -> ContractStats {
        ContractStats {
            total_created: env
                .storage()
                .instance()
                .get(&DataKey::TotalCreated)
                .unwrap_or(0),
            total_completed: env
                .storage()
                .instance()
                .get(&DataKey::TotalCompleted)
                .unwrap_or(0),
            total_disputed: env
                .storage()
                .instance()
                .get(&DataKey::TotalDisputed)
                .unwrap_or(0),
            total_refunded: env
                .storage()
                .instance()
                .get(&DataKey::TotalRefunded)
                .unwrap_or(0),
        }
    }

    /// Returns publicly-readable contract configuration: protocol fee,
    /// arbitration fee, pause state, and total escrow count. Callable by anyone.
    pub fn get_public_config(env: Env) -> PublicContractConfig {
        let fee_config = read_fee_config(&env);
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);

        let current_counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(1);
        let escrow_count = current_counter.saturating_sub(1);

        PublicContractConfig {
            fee_bps: fee_config.protocol_fee_bps,
            arbitration_fee_bps: fee_config.arbitration_fee_bps,
            paused,
            escrow_count,
        }
    }

    /// Returns admin-only contract configuration: admin address, protocol
    /// fee, arbitration fee, fee collector, and total escrow count. Requires
    /// the caller to authenticate as the current admin. Reverts with
    /// `NotInitialized` if the contract has not been initialized.
    pub fn get_contract_config(env: Env) -> Result<ContractConfig, ContractError> {
        let admin = require_admin(&env)?;
        admin.require_auth();

        let fee_config = read_fee_config(&env);
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotInitialized)?;
        let escrow_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(1u64)
            .saturating_sub(1);
        Ok(ContractConfig {
            admin,
            fee_bps: fee_config.protocol_fee_bps,
            arbitration_fee_bps: fee_config.arbitration_fee_bps,
            fee_collector,
            escrow_count,
        })
    }
}
