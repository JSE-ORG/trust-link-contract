//! Read-only views over escrow, fee, and contract-wide state.

use crate::internal::*;
use crate::types::Message;
use crate::*;
use soroban_sdk::{contractimpl, Address, Env, Vec};

#[contractimpl]
impl Escrow {
    /// Retrieves messages for a given escrow with pagination.
    ///
    /// Returns [`ContractError::EscrowNotFound`] when `escrow_id` does not
    /// exist, so a missing escrow is distinguishable from a valid escrow that
    /// simply has no messages (which returns an empty `Vec`).
    pub fn get_messages(
        env: Env,
        escrow_id: u64,
        start: u64,
        limit: u64,
    ) -> Result<Vec<Message>, ContractError> {
        load_escrow(&env, escrow_id)?;

        let max_limit = if limit > crate::MAX_MESSAGES_PER_PAGE {
            crate::MAX_MESSAGES_PER_PAGE
        } else {
            limit
        };
        // Only the message count is loaded up front; each message is then read
        // through its own `Message(escrow_id, index)` key. Cost is therefore
        // proportional to the requested page, not the whole thread.
        let total = u64::from(storage::read_message_count(&env, escrow_id));
        let mut result = Vec::new(&env);
        if start >= total {
            return Ok(result);
        }
        let end = (start + max_limit).min(total);
        let mut i = start;
        while i < end {
            // `total` came from a u32 count, so every index in range fits.
            let Ok(index) = u32::try_from(i) else { break };
            if let Some(message) = storage::read_message_at(&env, escrow_id, index) {
                result.push_back(message);
            }
            i += 1;
        }
        Ok(result)
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
    /// `(state, ledger_timestamp)` pairs, oldest first.
    ///
    /// Outcomes:
    /// - missing escrow => `EscrowNotFound` (never an empty `Vec`);
    /// - existing escrow => a **non-empty** `Vec`, because creation records a
    ///   `Pending` entry via `append_state_history`, so a freshly created
    ///   escrow always starts with `Pending`. Once an escrow has accumulated
    ///   more than `MAX_STATE_HISTORY_ENTRIES` transitions the oldest entries
    ///   are pruned, so `Pending` is only guaranteed first until that cap.
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
    ///
    /// Uses the paged `BuyerEscrowIndex` if it has entries; IDs are returned in
    /// ascending order. Otherwise falls back to scanning at most the 1000 most
    /// recent escrow IDs (`counter - 1` down to `counter - 1000`) to avoid
    /// budget exhaustion. The fallback therefore **silently omits** a buyer's
    /// escrows older than that window on contracts with more than 1000
    /// escrows, and still returns ascending order. The scan avoids extending
    /// TTL for non-matching escrows.
    ///
    /// Migration recipe: the index is populated on `fund_escrow`, so escrows
    /// funded before the index existed are only reachable via the fallback.
    /// To backfill, an upgrade/migration step should, for each buyer, collect
    /// their escrow IDs (e.g. from off-chain event history) in ascending order
    /// and call `storage::write_buyer_escrow_index(env, &buyer, &ids)`. Once
    /// every buyer is backfilled the fallback is never taken for them.
    pub fn get_escrows_by_buyer(env: Env, buyer: Address) -> Vec<u64> {
        // Preferred path: the paged buyer index, populated on funding.
        let indexed = storage::read_buyer_escrow_index(&env, &buyer);
        if !indexed.is_empty() {
            return indexed;
        }
        let mut result = Vec::new(&env);
        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(1);

        let max_iterations = 1000;
        // Scan the most recent max_iterations escrows. `zip` bounds the scan
        // without a manual counter. The `has` check avoids extending TTL for
        // non-matching escrows.
        for (id, _) in (1..counter).rev().zip(0..max_iterations) {
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
        // build in ascending order
        let mut ascending = Vec::new(&env);
        for i in (0..result.len()).rev() {
            if let Some(id) = result.get(i) {
                ascending.push_back(id);
            }
        }
        ascending
    }

    /// Batch view: return escrows for the supplied IDs in the same order.
    ///
    /// - A missing ID yields `None` in its slot, so the result is positionally
    ///   aligned with the (possibly truncated) input.
    /// - Input is capped at `MAX_IDS_PER_QUERY` (50). IDs beyond the cap are
    ///   **truncated**, not rejected: only the first `MAX_IDS_PER_QUERY` are
    ///   looked up and the result has at most that many slots.
    pub fn get_escrows_by_ids(
        env: Env,
        ids: soroban_sdk::Vec<u64>,
    ) -> soroban_sdk::Vec<Option<EscrowData>> {
        let mut result: soroban_sdk::Vec<Option<EscrowData>> = soroban_sdk::Vec::new(&env);
        let limit = ids.len().min(crate::MAX_IDS_PER_QUERY);

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

    /// Retrieves a page of escrow IDs associated with a specific vendor
    /// (seller).
    ///
    /// Ordering is **ascending** (oldest escrow first, i.e. creation order).
    /// `start` is a zero-based offset into that list and `limit` is clamped to
    /// `MAX_ESCROWS_PER_PAGE` (50). An out-of-range `start` or a zero `limit`
    /// returns an empty `Vec`. To walk a full index, call repeatedly with
    /// `start += returned.len()` until an empty page comes back.
    pub fn get_escrows_by_vendor(env: Env, vendor: Address, start: u64, limit: u64) -> Vec<u64> {
        storage::read_vendor_escrow_index_page(
            &env,
            &vendor,
            start,
            limit.min(crate::MAX_ESCROWS_PER_PAGE),
        )
    }

    /// Retrieves a page of escrow IDs associated with a specific seller.
    ///
    /// Alias of [`Self::get_escrows_by_vendor`]: same ascending (creation)
    /// order, same `(start, limit)` semantics, same `MAX_ESCROWS_PER_PAGE` cap.
    pub fn get_escrows_by_seller(env: Env, seller: Address, start: u64, limit: u64) -> Vec<u64> {
        storage::read_vendor_escrow_index_page(
            &env,
            &seller,
            start,
            limit.min(crate::MAX_ESCROWS_PER_PAGE),
        )
    }

    /// Returns on-chain counters for escrow lifecycle events.
    ///
    /// The counters are stored sharded across persistent-storage buckets (see
    /// `increment_sharded_counter`) to avoid a single contended instance key;
    /// this view re-aggregates them, including any value written by a
    /// pre-sharding deployment.
    pub fn get_stats(env: Env) -> ContractStats {
        ContractStats {
            total_created: read_counter_total(&env, COUNTER_KIND_CREATED, &DataKey::TotalCreated),
            total_completed: read_counter_total(
                &env,
                COUNTER_KIND_COMPLETED,
                &DataKey::TotalCompleted,
            ),
            total_disputed: read_counter_total(
                &env,
                COUNTER_KIND_DISPUTED,
                &DataKey::TotalDisputed,
            ),
            total_refunded: read_counter_total(
                &env,
                COUNTER_KIND_REFUNDED,
                &DataKey::TotalRefunded,
            ),
        }
    }

    /// Returns publicly-readable contract configuration: protocol fee,
    /// arbitration fee, pause state, and total escrow count. Callable by anyone.
    pub fn get_public_config(env: Env) -> PublicContractConfig {
        // Issue #828: Extend instance TTL to prevent archival during frequent
        // read-only queries. All mutating entry points extend TTL, and this
        // read-only query must do the same to keep singleton config alive.
        storage::extend_instance_ttl(&env);

        let fee_config = read_fee_config(&env);
        let paused = crate::storage::read_global_config(&env).state_flags
            & crate::types::GLOBAL_CONFIG_FLAG_PAUSED
            != 0;

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
        let fee_collector: Address = crate::storage::read_global_config(&env)
            .fee_collector
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
