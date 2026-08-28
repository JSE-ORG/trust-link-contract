//! Core escrow lifecycle instructions: creation, funding, delivery
//! confirmation, cancellation, refunds, and batch/multicall entry points.

use crate::events::emit_message_posted;
use crate::internal::*;
use crate::types::Message;
use crate::*;
use soroban_sdk::{
    contractimpl, token, Address, BytesN, Env, IntoVal, String, Symbol, TryFromVal, TryIntoVal,
    Val, Vec,
};

#[contractimpl]
impl Escrow {
    /// Modern 9-argument production interface accepting either Address or Payee array variants.
    pub fn create_escrow(
        env: Env,
        seller_or_payees: Val,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
        resolver_fee_bps: u32,
        shipping_window: u64,
        notes: Option<String>,
    ) -> Result<u64, ContractError> {
        let payees = if let Ok(payees_vec) = Vec::<Payee>::try_from_val(&env, &seller_or_payees) {
            payees_vec
        } else if let Ok(seller_address) = Address::try_from_val(&env, &seller_or_payees) {
            let mut p_vec = Vec::new(&env);
            p_vec.push_back(Payee {
                address: seller_address,
                bps: 10_000,
            });
            p_vec
        } else {
            return Err(ContractError::InvalidAddress);
        };

        ensure_not_paused(&env)?;

        create_escrow_internal(
            &env,
            payees,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            resolver_fee_bps,
            shipping_window,
            notes,
        )
    }

    /// Explicit 8-argument method signature mapping for historical tests.
    pub fn create_escrow_8(
        env: Env,
        seller_or_payees: Val,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
    ) -> Result<u64, ContractError> {
        Self::create_escrow(
            env,
            seller_or_payees,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            0_u32, // Default resolver fee
            shipping_window,
            None,
        )
    }

    /// Explicit 7-argument method signature mapping for historical tests.
    pub fn create_escrow_7(
        env: Env,
        seller_or_payees: Val,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
    ) -> Result<u64, ContractError> {
        Self::create_escrow(
            env,
            seller_or_payees,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            0_u32,                          // Default resolver fee
            crate::DEFAULT_SHIPPING_WINDOW, // Default shipping window fallback
            None,
        )
    }

    /// Creates an escrow with an optional expiration time.
    ///
    /// If `expires_at` is provided, the escrow must be funded via `fund_escrow`
    /// before `expires_at + grace_period` (ledger time) or `fund_escrow` will
    /// reject it with `EscrowExpired`. `grace_period` is ignored when
    /// `expires_at` is `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_escrow_with_expiration(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        resolver: Address,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
        expires_at: Option<u64>,
        grace_period: u64,
    ) -> Result<u64, ContractError> {
        let mut payees = Vec::new(&env);
        payees.push_back(Payee {
            address: seller,
            bps: 10_000,
        });
        let escrow_id = create_escrow_internal(
            &env,
            payees,
            buyer,
            resolver,
            token,
            amount,
            fee_bps,
            0,
            shipping_window,
            None,
        )?;

        if let Some(expires_at) = expires_at {
            if expires_at <= env.ledger().timestamp() {
                return Err(ContractError::InvalidExpiration);
            }
            expires_at
                .checked_add(grace_period)
                .ok_or(ContractError::ArithmeticOverflow)?;

            let schedule = crate::ExpirySchedule {
                expires_at,
                grace_period,
            };
            let key = DataKey::PendingExpiry(escrow_id);
            let ext = get_ttl_extension(&env);
            env.storage().persistent().set(&key, &schedule);
            env.storage().persistent().extend_ttl(&key, ext / 2, ext);
        }

        Ok(escrow_id)
    }

    /// Buyer reclaims tokens from a Funded/Shipped escrow that has passed its
    /// expiry schedule's grace period. Transitions the escrow to Expired.
    pub fn reclaim_expired(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_action_not_paused(&env, Symbol::new(&env, "RECLAIM"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        let schedule: crate::ExpirySchedule = env
            .storage()
            .persistent()
            .get(&DataKey::PendingExpiry(escrow_id))
            .ok_or(ContractError::InvalidState)?;

        let now = env.ledger().timestamp();
        if now < schedule.expires_at {
            return Err(ContractError::InvalidState);
        }
        let reclaimable_at = schedule
            .expires_at
            .checked_add(schedule.grace_period)
            .ok_or(ContractError::ArithmeticOverflow)?;
        if now < reclaimable_at {
            return Err(ContractError::GracePeriodNotElapsed);
        }

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        buyer.require_auth();

        token::Client::new(&env, &escrow.token).transfer(
            &env.current_contract_address(),
            &buyer,
            &escrow.amount,
        );
        payout_basket_tokens(&env, escrow_id, &buyer)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Expired;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        env.storage()
            .persistent()
            .remove(&DataKey::PendingExpiry(escrow_id));

        emit_escrow_expired(
            &env,
            escrow_id,
            buyer,
            escrow.amount,
            prev_state,
            EscrowState::Expired,
        );
        Ok(())
    }

    /// Cancels a `Pending` escrow that was never funded within
    /// `PENDING_EXPIRY_WINDOW` of its creation. Callable by anyone.
    pub fn auto_cancel_pending(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;
        crate::internal::ensure_not_expired(&env, escrow_id)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        let created_at = escrow_created_at(&env, escrow_id);
        let deadline = created_at
            .checked_add(PENDING_EXPIRY_WINDOW)
            .ok_or(ContractError::ArithmeticOverflow)?;
        if env.ledger().timestamp() <= deadline {
            return Err(ContractError::ShippingWindowNotElapsed);
        }

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Canceled;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        emit_escrow_auto_canceled(&env, escrow_id);
        Ok(())
    }

    /// Buyer funds a pending escrow. Transitions Pending → Funded.
    pub fn fund_escrow(env: Env, escrow_id: u64, buyer: Address) -> Result<(), ContractError> {
        buyer.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "FUND"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        let now = env.ledger().timestamp();

        if let Some(schedule) = env
            .storage()
            .persistent()
            .get::<DataKey, crate::ExpirySchedule>(&DataKey::PendingExpiry(escrow_id))
        {
            if now >= schedule.expires_at {
                return Err(ContractError::EscrowExpired);
            }
        }

        let created_at = crate::internal::escrow_created_at(&env, escrow_id);
        let blanket_deadline = created_at
            .checked_add(crate::PENDING_EXPIRY_WINDOW)
            .ok_or(ContractError::ArithmeticOverflow)?;
        if now > blanket_deadline {
            return Err(ContractError::EscrowExpired);
        }

        // Security: buyer must differ from seller and resolver.
        for i in 0..escrow.payees.len() {
            let payee = escrow
                .payees
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            if buyer == payee.address {
                return Err(ContractError::ConflictingRoles);
            }
        }
        if escrow.resolvers.contains(&buyer) {
            return Err(ContractError::ConflictingRoles);
        }
        if let Some(ref expected_buyer) = escrow.buyer {
            if &buyer != expected_buyer {
                return Err(ContractError::NotAuthorized);
            }
        }

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(&buyer, env.current_contract_address(), &escrow.amount);

        // Transfer additional basket tokens if this is a basket escrow
        let basket_tokens = load_basket_tokens(&env, escrow_id);
        for i in 0..basket_tokens.len() {
            let entry = basket_tokens
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            if entry.token != escrow.token && entry.amount > 0 {
                token::Client::new(&env, &entry.token).transfer(
                    &buyer,
                    env.current_contract_address(),
                    &entry.amount,
                );
            }
        }

        let now = env.ledger().timestamp();
        let prev_state = escrow.state.clone();
        escrow.buyer = Some(buyer.clone());
        escrow.state = EscrowState::Funded;
        escrow.funded_at = now;
        escrow.dispute_deadline = now
            .checked_add(DISPUTE_WINDOW)
            .ok_or(ContractError::ArithmeticOverflow)?;

        // Index the buyer for lookup.
        let mut buyer_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
            .unwrap_or(Vec::new(&env));
        buyer_escrows.push_back(escrow_id);
        let buyer_key = DataKey::BuyerEscrowIndex(buyer.clone());
        let ext = get_ttl_extension(&env);
        env.storage().persistent().set(&buyer_key, &buyer_escrows);
        env.storage()
            .persistent()
            .extend_ttl(&buyer_key, ext / 2, ext);

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        // Build basket_tokens event data if this is a basket escrow
        let basket_event_data = if basket_tokens.len() > 1 {
            let mut tuples = soroban_sdk::Vec::new(&env);
            for i in 0..basket_tokens.len() {
                let entry = basket_tokens
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?;
                tuples.push_back((entry.token, entry.amount));
            }
            Some(tuples)
        } else {
            None
        };

        emit_escrow_funded(
            &env,
            escrow_id,
            buyer,
            escrow.amount,
            crate::EscrowState::Pending,
            crate::EscrowState::Funded,
            basket_event_data,
        );
        Ok(())
    }

    /// Create escrow with multiple resolvers and M-of-N voting threshold.
    pub fn create_escrow_multi(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        resolvers: Vec<Address>,
        threshold: u32,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
    ) -> Result<u64, ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
        seller.require_auth();

        ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        if amount > MAX_ESCROW_AMOUNT {
            return Err(ContractError::AmountExceedsMaximum);
        }

        if amount < MIN_ESCROW_AMOUNT {
            return Err(ContractError::InvalidAmount);
        }

        validate_escrow_fee_bps(fee_bps)?;

        // Reject tokens not on the allowlist (mirrors create_escrow_internal).
        // is_token_allowed is a no-op when the allowlist is disabled, so this
        // adds no overhead for contracts that have not enabled the allowlist.
        is_token_allowed(&env, &token)?;

        // Validate multi-resolver configuration
        let resolver_set = ResolverSet::Multi(crate::types::MultiResolver {
            resolvers,
            threshold,
        });
        validate_resolvers(&resolver_set, &seller, &buyer)?;

        let escrow_id = crate::next_escrow_id(&env)?;

        let mut payees = Vec::new(&env);
        payees.push_back(Payee {
            address: seller.clone(),
            bps: 10_000,
        });
        let escrow = EscrowData {
            payees,
            buyer,
            resolvers: resolver_set.clone(),
            token,
            amount,
            fee_bps,
            resolver_fee_bps: 0,
            shipping_window,
            funded_at: 0,
            dispute_deadline: 0,
            state: EscrowState::Pending,
            shipped_at: 0,
            delivered_at: None,
            tracking_id: None,
            notes: None,
        };

        save_escrow(&env, escrow_id, &escrow, None);

        let mut vendor_escrows = storage::read_vendor_escrow_index(&env, &seller);
        vendor_escrows.push_back(escrow_id);
        storage::write_vendor_escrow_index(&env, &seller, &vendor_escrows);

        increment_counter(&env, &DataKey::TotalCreated)?;

        // Emit with first resolver for backward compat
        if let ResolverSet::Multi(ref m) = &resolver_set {
            let resolver_addr = m
                .resolvers
                .get(0)
                .ok_or(ContractError::IndexOutOfBounds)?
                .clone();
            emit_escrow_created(
                &env,
                escrow_id,
                seller,
                resolver_addr,
                escrow.token.clone(),
                escrow.amount,
                escrow.fee_bps,
                escrow.resolver_fee_bps,
                escrow.shipping_window,
                crate::EscrowState::Pending,
            );
        }

        Ok(escrow_id)
    }

    /// Posts a message for a given escrow. Messages are immutable and stored on-chain.
    pub fn post_message(
        env: Env,
        escrow_id: u64,
        sender: Address,
        content: String,
    ) -> Result<(), ContractError> {
        sender.require_auth();
        ensure_not_paused(&env)?;
        let escrow = load_escrow(&env, escrow_id)?;

        let mut is_payee = false;
        for payee in escrow.payees.iter() {
            if payee.address == sender {
                is_payee = true;
                break;
            }
        }
        if escrow.buyer.as_ref() != Some(&sender) && !is_payee {
            return Err(ContractError::NotAuthorized);
        }
        if content.is_empty() {
            return Err(ContractError::InvalidAmount);
        }
        if content.len() > MAX_MESSAGE_LEN {
            return Err(ContractError::InputTooLong);
        }

        let message = Message {
            sender: sender.clone(),
            timestamp: env.ledger().timestamp(),
            content,
        };
        let key = DataKey::Messages(escrow_id);
        let mut msgs: Vec<Message> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        if msgs.len() >= crate::MAX_MESSAGES_PER_ESCROW {
            return Err(ContractError::TooManyMessages);
        }

        msgs.push_back(message);
        env.storage().persistent().set(&key, &msgs);
        emit_message_posted(&env, escrow_id, sender);
        Ok(())
    }

    /// Creates an escrow configured with a fallback resolver scheme (`ResolverSet::Fallback`).
    ///
    /// The fallback scheme designates a primary resolver and a backup resolver:
    /// - **Primary Resolver**: Authorized to resolve disputes at any time once a dispute is raised.
    /// - **Backup Resolver**: Authorized to resolve disputes only when the ledger timestamp is at
    ///   or after `dispute_deadline`. This prevents deadlocks if the primary resolver is unresponsive.
    /// - **Threshold**: The voting threshold is 1 (either the primary resolver or the backup resolver
    ///   can unilaterally resolve the dispute once authorized).
    ///
    /// # Arguments
    /// * `env` - Soroban environment.
    /// * `seller` - Address of the seller (must authenticate this call).
    /// * `buyer` - Optional address of the designated buyer (if None, anyone can fund).
    /// * `primary_resolver` - Address of the primary dispute arbitrator.
    /// * `backup_resolver` - Address of the backup dispute arbitrator who takes over after `dispute_deadline`.
    /// * `dispute_deadline` - Unix timestamp in seconds after which the backup resolver becomes eligible to resolve disputes.
    /// * `token` - Address of the SPL/SEP-41 payment token.
    /// * `amount` - Escrow deposit amount in stroops (must be >= `MIN_ESCROW_AMOUNT` and <= `MAX_ESCROW_AMOUNT`).
    /// * `fee_bps` - Escrow fee in basis points (100 bps = 1%, max 300 bps).
    /// * `shipping_window` - Duration in seconds allocated for shipping before auto-cancellation or delivery.
    ///
    /// # Errors
    /// * `ContractError::ContractPaused` - Contract is currently paused.
    /// * `ContractError::InvalidAmount` - Amount is <= 0 or < `MIN_ESCROW_AMOUNT`.
    /// * `ContractError::AmountExceedsMaximum` - Amount exceeds `MAX_ESCROW_AMOUNT`.
    /// * `ContractError::InvalidFeeBps` - Fee exceeds maximum permitted cap (`MAX_ESCROW_FEE_BPS`).
    /// * `ContractError::ResolverRoleConflict` - `primary_resolver` or `backup_resolver` matches `seller` or `buyer`.
    /// * `ContractError::DuplicateResolver` - `primary_resolver` equals `backup_resolver`.
    /// * `ContractError::ResolverNotApproved` - Strict resolver mode is enabled and a resolver is not on the approved list.
    ///
    /// # Example
    /// ```rust,ignore
    /// let escrow_id = client.create_escrow_with_fallback(
    ///     &seller,
    ///     &Some(buyer),
    ///     &primary_resolver,
    ///     &backup_resolver,
    ///     &(env.ledger().timestamp() + 86_400), // backup eligible after 24h
    ///     &token,
    ///     &1_000_000_i128,
    ///     &100_u32, // 1%
    ///     &86_400_u64, // 24h shipping window
    /// );
    /// Creates an escrow whose dispute resolver is a **primary/backup pair**:
    /// the `primary_resolver` handles disputes, and if they go unresponsive the
    /// `backup_resolver` may step in once `dispute_deadline` is reached.
    ///
    /// This produces a [`ResolverSet::Fallback`] escrow. The gating is enforced
    /// by [`ResolverSet::can_resolve_now`] every time `resolve_dispute` / `vote`
    /// is called:
    ///
    /// - `primary_resolver` may resolve at any time.
    /// - `backup_resolver` is rejected with `NotAuthorized` while
    ///   `env.ledger().timestamp() < dispute_deadline`, and may resolve once
    ///   `timestamp() >= dispute_deadline`.
    /// - The deadline never revokes the primary; it only *adds* the backup.
    ///
    /// # Parameters
    ///
    /// - `seller` — sole payee (100% of the payout); must authorize the call.
    /// - `buyer` — `Some(addr)` to lock the escrow to one buyer, or `None` for
    ///   an open escrow that anyone may fund.
    /// - `primary_resolver` / `backup_resolver` — the resolver pair. Neither
    ///   may equal `seller` or `buyer` (`ConflictingRoles`).
    /// - `dispute_deadline` — **absolute ledger timestamp in Unix seconds** at
    ///   which `backup_resolver` becomes authorized. This is unrelated to
    ///   `EscrowData::dispute_deadline` (the buyer's dispute window, computed at
    ///   funding). It is **not** range-checked: a past value (or `0`) simply
    ///   co-authorizes the backup from the start; callers normally pass
    ///   `env.ledger().timestamp() + grace_seconds`.
    /// - `token`, `amount`, `fee_bps`, `shipping_window` — as for
    ///   `create_escrow` (`amount` within `[MIN_ESCROW_AMOUNT,
    ///   MAX_ESCROW_AMOUNT]`, `fee_bps <= MAX_ESCROW_FEE_BPS`).
    ///
    /// Returns the new escrow id. Emits `escrow_created` with
    /// `primary_resolver` as the resolver.
    ///
    /// # Backward compatibility
    ///
    /// This is an additive entry point. The resulting escrow always has
    /// `resolver_fee_bps = 0` and no `notes` / expiration schedule; use
    /// `create_escrow` if you need those. Existing single-resolver escrows are
    /// unaffected — `can_resolve_now` collapses to the old membership check for
    /// [`ResolverSet::Single`].
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Backup resolver may take over 72 hours after creation.
    /// let deadline = env.ledger().timestamp() + 72 * 60 * 60;
    /// let id = Escrow::create_escrow_with_fallback(
    ///     env.clone(),
    ///     seller,
    ///     Some(buyer),
    ///     primary_resolver,
    ///     backup_resolver,
    ///     deadline,
    ///     token,
    ///     10_000_000,        // amount
    ///     100,               // 1% fee
    ///     7 * 24 * 60 * 60,  // 7-day shipping window
    /// )?;
    /// ```
    pub fn create_escrow_with_fallback(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        primary_resolver: Address,
        backup_resolver: Address,
        dispute_deadline: u64,
        token: Address,
        amount: i128,
        fee_bps: u32,
        shipping_window: u64,
    ) -> Result<u64, ContractError> {
        // SECURITY:
        // Authenticate before any state reads.
        seller.require_auth();

        ensure_not_paused(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidAmount);
        }
        if amount > MAX_ESCROW_AMOUNT {
            return Err(ContractError::AmountExceedsMaximum);
        }
        if amount < MIN_ESCROW_AMOUNT {
            return Err(ContractError::InvalidAmount);
        }

        validate_escrow_fee_bps(fee_bps)?;

        let resolver_set = ResolverSet::Fallback(crate::types::FallbackResolver {
            primary: primary_resolver.clone(),
            backup: backup_resolver,
            dispute_deadline,
        });
        validate_resolvers(&resolver_set, &seller, &buyer)?;

        let escrow_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .ok_or(ContractError::NotInitialized)?;
        let next_id = escrow_id
            .checked_add(1)
            .ok_or(ContractError::ArithmeticError)?;
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &next_id);

        let ext = get_ttl_extension(&env);
        env.storage().instance().extend_ttl(ext / 2, ext);

        let mut payees = Vec::new(&env);
        payees.push_back(Payee {
            address: seller.clone(),
            bps: 10_000,
        });
        let escrow = EscrowData {
            payees,
            buyer,
            resolvers: resolver_set,
            token,
            amount,
            fee_bps,
            resolver_fee_bps: 0,
            shipping_window,
            funded_at: 0,
            dispute_deadline: 0,
            state: EscrowState::Pending,
            shipped_at: 0,
            delivered_at: None,
            tracking_id: None,
            notes: None,
        };

        save_escrow(&env, escrow_id, &escrow, None);

        let mut vendor_escrows = storage::read_vendor_escrow_index(&env, &seller);
        vendor_escrows.push_back(escrow_id);
        storage::write_vendor_escrow_index(&env, &seller, &vendor_escrows);

        increment_counter(&env, &DataKey::TotalCreated)?;

        emit_escrow_created(
            &env,
            escrow_id,
            seller,
            primary_resolver,
            escrow.token.clone(),
            escrow.amount,
            escrow.fee_bps,
            escrow.resolver_fee_bps,
            escrow.shipping_window,
            crate::EscrowState::Pending,
        );

        Ok(escrow_id)
    }

    /// Cancels an escrow. Callable by any payee (seller) or the buyer.
    /// A `Pending` (unfunded) escrow simply transitions to `Canceled`. A
    /// `Funded` escrow may only be cancelled by the buyer, which refunds the
    /// full amount (and any basket tokens) and transitions to `Refunded`.
    /// Reverts with `NotAuthorized` if `caller` is neither a payee nor the
    /// buyer, or `InvalidState` for any other escrow state. Emits
    /// `escrow_canceled`.
    pub fn cancel_escrow(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        crate::internal::ensure_not_expired(&env, escrow_id)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow.buyer.clone();
        let is_payee = {
            let mut found = false;
            for i in 0..escrow.payees.len() {
                if caller
                    == escrow
                        .payees
                        .get(i)
                        .ok_or(ContractError::IndexOutOfBounds)?
                        .address
                {
                    found = true;
                    break;
                }
            }
            found
        };

        if !is_payee && buyer.as_ref() != Some(&caller) {
            return Err(ContractError::NotAuthorized);
        }

        let prev_state = escrow.state.clone();
        if escrow.state == EscrowState::Pending {
            escrow.state = EscrowState::Canceled;
        } else if escrow.state == EscrowState::Funded && buyer.as_ref() == Some(&caller) {
            let token_client = token::Client::new(&env, &escrow.token);
            token_client.transfer(&env.current_contract_address(), &caller, &escrow.amount);
            payout_basket_tokens(&env, escrow_id, &caller)?;
            escrow.state = EscrowState::Refunded;
            increment_counter(&env, &DataKey::TotalRefunded)?;
        } else {
            return Err(ContractError::InvalidState);
        }

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        let first_payee_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        emit_escrow_canceled(
            &env,
            escrow_id,
            first_payee_addr,
            caller,
            prev_state,
            escrow.state.clone(),
        );
        Ok(())
    }

    /// Cancels a funded—but not yet shipped—escrow by mutual agreement and refunds the buyer in full.
    pub fn mutual_cancel(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;
        crate::internal::ensure_not_expired(&env, escrow_id)?;
        let mut escrow = load_escrow(&env, escrow_id)?;
        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;

        let seller_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        seller_addr.require_auth();
        buyer.require_auth();

        if escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        token::Client::new(&env, &escrow.token).transfer(
            &env.current_contract_address(),
            &buyer,
            &escrow.amount,
        );

        payout_basket_tokens(&env, escrow_id, &buyer)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Canceled;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        emit_escrow_canceled(
            &env,
            escrow_id,
            seller_addr,
            buyer,
            prev_state,
            crate::EscrowState::Canceled,
        );
        Ok(())
    }

    /// Seller marks an escrow as shipped. Transitions Funded → Shipped.
    pub fn mark_shipped(
        env: Env,
        caller: Address,
        escrow_id: u64,
        tracking_id: String,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        crate::internal::ensure_not_expired(&env, escrow_id)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let first_payee = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .clone();
        let is_authorized = {
            let mut found = false;
            for i in 0..escrow.payees.len() {
                let payee = escrow
                    .payees
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?;
                if caller == payee.address {
                    found = true;
                    break;
                }
            }
            found
        };

        if !is_authorized {
            return Err(ContractError::NotAuthorized);
        }

        // The seller may mark shipped from `Funded`, or from `RefundRequested`
        // to override an outstanding buyer refund request (issue #730).
        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::RefundRequested {
            return Err(ContractError::InvalidState);
        }

        if tracking_id.is_empty() {
            return Err(ContractError::InvalidTrackingId);
        }
        if tracking_id.len() > MAX_TRACKING_ID_LEN {
            return Err(ContractError::InputTooLong);
        }

        let shipped_at = env.ledger().timestamp();
        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Shipped;
        escrow.shipped_at = shipped_at;
        escrow.tracking_id = Some(tracking_id);
        let tracking = escrow
            .tracking_id
            .clone()
            .unwrap_or(String::from_str(&env, ""));

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        emit_escrow_shipped(
            &env,
            escrow_id,
            first_payee.address,
            tracking,
            prev_state,
            crate::EscrowState::Shipped,
        );
        Ok(())
    }

    /// Proposes recording delivery of an escrow, starting a 24-hour timelock. Callable by admin.
    pub fn propose_record_delivery(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)?;

        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let escrow = load_escrow(&env, escrow_id)?;
        if escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if escrow.delivered_at.is_some() {
            return Err(ContractError::DeliveryAlreadyRecorded);
        }

        let now = env.ledger().timestamp();
        let unlock_at = now
            .checked_add(DELIVERY_TIMELOCK)
            .ok_or(ContractError::ArithmeticOverflow)?;

        env.storage()
            .persistent()
            .set(&DataKey::DeliveryProposal(escrow_id), &unlock_at);

        emit_delivery_proposed(&env, escrow_id, now, unlock_at);
        Ok(())
    }

    /// Cancels a pending delivery proposal. Callable by admin.
    pub fn cancel_delivery_proposal(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)?;

        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let _ = load_escrow(&env, escrow_id)?;

        let key = DataKey::DeliveryProposal(escrow_id);
        if !env.storage().persistent().has(&key) {
            return Err(ContractError::DeliveryNotProposed);
        }

        env.storage().persistent().remove(&key);
        emit_delivery_proposal_cancelled(&env, escrow_id);
        Ok(())
    }

    /// Records the delivery of an escrow after the 24-hour timelock has elapsed. Callable by admin.
    pub fn record_delivery(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)?;

        if caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let mut escrow = load_escrow(&env, escrow_id)?;
        if escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        // Idempotency guard: prevent re-recording delivery
        if escrow.delivered_at.is_some() {
            return Err(ContractError::DeliveryAlreadyRecorded);
        }

        let key = DataKey::DeliveryProposal(escrow_id);
        let unlock_at: u64 = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::DeliveryNotProposed)?;

        let now = env.ledger().timestamp();
        if now < unlock_at {
            return Err(ContractError::TimelockNotElapsed);
        }

        env.storage().persistent().remove(&key);

        let delivered_at = now;
        escrow.delivered_at = Some(delivered_at);
        // escrow.state is untouched by this call, so the pre-mutation state
        // is simply the current one.
        save_escrow(&env, escrow_id, &escrow, Some(&escrow.state));

        emit_delivery_recorded(&env, escrow_id, delivered_at);
        Ok(())
    }

    /// Buyer confirms delivery and releases the escrowed funds to the payees.
    ///
    /// Only the escrow's `buyer` may call this, and only while the escrow is
    /// `Shipped` (`InvalidStateTransition` otherwise). The buyer's dispute
    /// window (`funded_at + DISPUTE_WINDOW`, stored as `dispute_deadline`) must
    /// have elapsed first: calling during the window returns
    /// `DisputeWindowStillOpen`, mirroring `raise_dispute`'s complementary
    /// `now < dispute_deadline` guard so the two entry points never overlap on
    /// the same ledger second.
    ///
    /// On success the protocol fee (using the escrow's snapshotted `fee_bps`)
    /// goes to the fee collector, the remainder is split across `payees`, any
    /// basket tokens are paid to the primary payee, and the escrow moves to
    /// `Completed`. Emits `escrow_completed`.
    pub fn confirm_delivery(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        crate::internal::ensure_not_expired(&env, escrow_id)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidStateTransition);
        }

        // The dispute window is still open until `dispute_deadline`; the buyer
        // can only confirm once it has closed. `DisputeWindowStillOpen` is the
        // error defined for exactly this case (`DeliveryBeforeDisputeWindow`
        // means the window has not *started*, which cannot happen for a
        // `Shipped` escrow — it is always funded).
        if env.ledger().timestamp() < escrow.dispute_deadline {
            return Err(ContractError::DisputeWindowStillOpen);
        }

        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotInitialized)?;

        let first_payee_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        let (protocol_fee, net_amount) =
            crate::helpers::payout::calculate_protocol_fee(escrow.amount, escrow.fee_bps)?;
        if protocol_fee > 0 {
            token::Client::new(&env, &escrow.token).transfer(
                &env.current_contract_address(),
                &fee_collector,
                &protocol_fee,
            );
        }
        distribute_to_payees(&env, &escrow.token, &escrow.payees, net_amount)?;
        payout_basket_tokens(&env, escrow_id, &first_payee_addr)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Completed;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalCompleted)?;

        emit_escrow_completed(
            &env,
            escrow_id,
            first_payee_addr,
            escrow.amount,
            escrow.fee_bps,
            prev_state,
            crate::EscrowState::Completed,
        );
        Ok(())
    }

    /// Releases funds early via mutual consent: requires auth from both the
    /// primary payee and the buyer in the same call. Only valid from
    /// `Funded` or `Shipped` state, and only if no dispute has been raised.
    /// Reverts with `InvalidState` otherwise. Transfers funds (minus
    /// protocol fee) and emits `escrow_completed`.
    pub fn co_signed_release(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        ensure_not_paused(&env)?;
        let escrow = load_escrow(&env, escrow_id)?;

        let first_payee = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        first_payee.require_auth();
        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        buyer.require_auth();

        // Allow early release from Funded or Shipped states, but not if disputed.
        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if load_dispute(&env, escrow_id).is_ok() {
            return Err(ContractError::InvalidState);
        }

        let fee_config = read_fee_config(&env);
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotInitialized)?;

        transfer_with_protocol_fee(
            &env,
            &escrow.token,
            &first_payee,
            &fee_collector,
            escrow.amount,
            fee_config.protocol_fee_bps,
        )?;
        payout_basket_tokens(&env, escrow_id, &first_payee)?;

        let prev_state = escrow.state.clone();
        let mut updated = escrow;
        updated.state = EscrowState::Completed;

        save_escrow(&env, escrow_id, &updated, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalCompleted)?;
        emit_escrow_completed(
            &env,
            escrow_id,
            first_payee,
            updated.amount,
            fee_config.protocol_fee_bps,
            prev_state,
            crate::EscrowState::Completed,
        );
        Ok(())
    }

    /// Releases funds to the payees once the shipping/delivery window has
    /// elapsed with no dispute raised. Callable by anyone. Reverts with
    /// `InvalidState` if the escrow is not `Funded`/`Shipped` or has an open
    /// dispute, `DeliveryNotRecorded` if `Shipped` with no `delivered_at`,
    /// `DeliveryBeforeDisputeWindow` if the buyer's dispute window hasn't
    /// opened yet, or `ShippingWindowNotElapsed` if the relevant window
    /// (delivery-release or shipping) hasn't elapsed. Deducts the protocol
    /// fee, distributes the remainder across `payees`, and transitions the
    /// escrow to `Completed`. Emits `auto_released`.
    pub fn auto_release(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;
        crate::internal::ensure_not_expired(&env, escrow_id)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if load_dispute(&env, escrow_id).is_ok() {
            return Err(ContractError::InvalidState);
        }

        let now = env.ledger().timestamp();

        if let Some(delivered_at) = escrow.delivered_at {
            let eligible_at = delivered_at
                .checked_add(DELIVERY_RELEASE_WINDOW)
                .ok_or(ContractError::ArithmeticOverflow)?;
            if now < eligible_at {
                return Err(ContractError::ShippingWindowNotElapsed);
            }
        } else if escrow.state == EscrowState::Shipped {
            return Err(ContractError::DeliveryNotRecorded);
        } else {
            if now < escrow.dispute_deadline {
                return Err(ContractError::DeliveryBeforeDisputeWindow);
            }
            let shipped_or_funded_at = if escrow.shipped_at > 0 {
                escrow.shipped_at
            } else {
                escrow.funded_at
            };
            let window_elapsed_at = shipped_or_funded_at
                .checked_add(escrow.shipping_window)
                .ok_or(ContractError::ArithmeticError)?;
            if now < window_elapsed_at {
                return Err(ContractError::ShippingWindowNotElapsed);
            }
        }

        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotInitialized)?;

        let first_payee_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        let (protocol_fee, net_amount) =
            crate::helpers::payout::calculate_protocol_fee(escrow.amount, escrow.fee_bps)?;
        if protocol_fee > 0 {
            token::Client::new(&env, &escrow.token).transfer(
                &env.current_contract_address(),
                &fee_collector,
                &protocol_fee,
            );
        }
        distribute_to_payees(&env, &escrow.token, &escrow.payees, net_amount)?;
        payout_basket_tokens(&env, escrow_id, &first_payee_addr)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Completed;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalCompleted)?;

        emit_auto_released(
            &env,
            escrow_id,
            first_payee_addr,
            escrow.amount,
            escrow.fee_bps,
            prev_state,
            crate::EscrowState::Completed,
        );
        Ok(())
    }

    /// Creates an escrow that pays out multiple tokens ("basket") to a single
    /// seller instead of the single-token flow used by `create_escrow`.
    /// `tokens` and `amounts` must be the same non-empty length and every
    /// token must pass the allowlist check (if enabled). The primary
    /// `EscrowData` record tracks `tokens[0]`/`amounts[0]`; the full basket
    /// is stored separately and readable via `get_basket_tokens`. Must be
    /// funded with `fund_basket_escrow`. Emits `basket_escrow_created`.
    pub fn create_basket_escrow(
        env: Env,
        seller: Address,
        buyer: Option<Address>,
        resolver: Address,
        tokens: soroban_sdk::Vec<Address>,
        amounts: soroban_sdk::Vec<i128>,
        fee_bps: u32,
        shipping_window: u64,
    ) -> Result<u64, ContractError> {
        seller.require_auth();
        ensure_not_paused(&env)?;

        if tokens.len() != amounts.len() || tokens.is_empty() {
            return Err(ContractError::BasketTokenMismatch);
        }

        validate_escrow_fee_bps(fee_bps)?;

        if resolver == seller {
            return Err(ContractError::ConflictingRoles);
        }
        if let Some(ref b) = buyer {
            if b == &seller || b == &resolver {
                return Err(ContractError::ConflictingRoles);
            }
        }

        for token in tokens.iter() {
            is_token_allowed(&env, &token)?;
        }

        let escrow_id = crate::next_escrow_id(&env)?;

        let primary_amount = amounts.get(0).ok_or(ContractError::InvalidAmount)?;
        let primary_token = tokens.get(0).ok_or(ContractError::InvalidAmount)?;

        let mut basket_payees = Vec::new(&env);
        basket_payees.push_back(Payee {
            address: seller.clone(),
            bps: 10_000,
        });
        let escrow = EscrowData {
            payees: basket_payees,
            buyer: buyer.clone(),
            resolvers: ResolverSet::Single(resolver.clone()),
            token: primary_token,
            amount: primary_amount,
            fee_bps,
            resolver_fee_bps: 0,
            shipping_window,
            funded_at: 0,
            dispute_deadline: 0,
            state: EscrowState::Pending,
            shipped_at: 0,
            delivered_at: None,
            tracking_id: None,
            notes: None,
        };

        save_escrow(&env, escrow_id, &escrow, None);

        // Persist all basket tokens/amounts alongside the primary EscrowData
        let mut basket_entries: Vec<TokenEntry> = Vec::new(&env);
        for i in 0..tokens.len() {
            let token = tokens.get(i).ok_or(ContractError::IndexOutOfBounds)?;
            let amount = amounts.get(i).ok_or(ContractError::IndexOutOfBounds)?;
            basket_entries.push_back(TokenEntry { token, amount });
        }
        save_basket_tokens(&env, escrow_id, &basket_entries);

        let mut vendor_escrows = storage::read_vendor_escrow_index(&env, &seller);
        vendor_escrows.push_back(escrow_id);
        storage::write_vendor_escrow_index(&env, &seller, &vendor_escrows);

        increment_counter(&env, &DataKey::TotalCreated)?;
        emit_basket_escrow_created(&env, escrow_id, seller, tokens.len());

        Ok(escrow_id)
    }

    /// Fund a basket escrow by transferring all tokens from the buyer.
    /// Can be used instead of individual `fund_escrow` calls for multi-token escrows.
    pub fn fund_basket_escrow(
        env: Env,
        escrow_id: u64,
        buyer: Address,
    ) -> Result<(), ContractError> {
        buyer.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "FUND"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        if let Some(ref expected_buyer) = escrow.buyer {
            if &buyer != expected_buyer {
                return Err(ContractError::NotAuthorized);
            }
        }

        let basket_tokens = load_basket_tokens(&env, escrow_id);
        if basket_tokens.is_empty() {
            return Err(ContractError::InvalidAmount);
        }

        for i in 0..basket_tokens.len() {
            let entry = basket_tokens
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            if entry.amount > 0 {
                token::Client::new(&env, &entry.token).transfer(
                    &buyer,
                    env.current_contract_address(),
                    &entry.amount,
                );
            }
        }

        let now = env.ledger().timestamp();
        let prev_state = escrow.state.clone();
        escrow.buyer = Some(buyer.clone());
        escrow.state = EscrowState::Funded;
        escrow.funded_at = now;
        escrow.dispute_deadline = now
            .checked_add(DISPUTE_WINDOW)
            .ok_or(ContractError::ArithmeticError)?;

        let mut buyer_escrows: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
            .unwrap_or(Vec::new(&env));
        buyer_escrows.push_back(escrow_id);
        let buyer_key = DataKey::BuyerEscrowIndex(buyer.clone());
        let ext = get_ttl_extension(&env);
        env.storage().persistent().set(&buyer_key, &buyer_escrows);
        env.storage()
            .persistent()
            .extend_ttl(&buyer_key, ext / 2, ext);

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        // Build basket_tokens event data (always Some for basket escrows)
        let mut basket_event_tuples = soroban_sdk::Vec::new(&env);
        for i in 0..basket_tokens.len() {
            let entry = basket_tokens
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            basket_event_tuples.push_back((entry.token, entry.amount));
        }

        emit_escrow_funded(
            &env,
            escrow_id,
            buyer.clone(),
            escrow.amount,
            crate::EscrowState::Pending,
            crate::EscrowState::Funded,
            Some(basket_event_tuples),
        );
        Ok(())
    }

    /// Rotates the resolver for an escrow. Callable by any payee or admin.
    /// New resolver must differ from current resolver, all payees, and buyer.
    pub fn rotate_resolver(
        env: Env,
        caller: Address,
        escrow_id: u64,
        new_resolver: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;

        let mut escrow = load_escrow(&env, escrow_id)?;
        let admin = require_admin(&env)?;

        let is_payee = {
            let mut found = false;
            for i in 0..escrow.payees.len() {
                let payee = escrow
                    .payees
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?;
                if caller == payee.address {
                    found = true;
                    break;
                }
            }
            found
        };

        if !is_payee && caller != admin {
            return Err(ContractError::NotAuthorized);
        }

        let is_terminal = matches!(
            escrow.state,
            EscrowState::Completed
                | EscrowState::Refunded
                | EscrowState::Canceled
                | EscrowState::Expired
        );
        if is_terminal {
            return Err(ContractError::InvalidState);
        }

        // Only support rotation for single-resolver escrows (backward compat)
        if let ResolverSet::Single(current_resolver) = &escrow.resolvers {
            if new_resolver == *current_resolver {
                return Err(ContractError::SameAddress);
            }
            // New resolver must differ from all payees
            for i in 0..escrow.payees.len() {
                let payee = escrow
                    .payees
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?;
                if new_resolver == payee.address {
                    return Err(ContractError::InvalidAddress);
                }
            }

            if escrow.buyer.as_ref() == Some(&new_resolver) {
                return Err(ContractError::InvalidAddress);
            }

            let old_resolver = current_resolver.clone();
            escrow.resolvers = ResolverSet::Single(new_resolver.clone());
            // escrow.state is untouched by rotation, so the pre-mutation
            // state is simply the current one.
            save_escrow(&env, escrow_id, &escrow, Some(&escrow.state));

            emit_resolver_rotated(&env, escrow_id, old_resolver, new_resolver);
            Ok(())
        } else {
            // Multi-resolver escrows don't support simple rotation
            // A separate function could be added for managing multi-resolver sets
            Err(ContractError::InvalidState)
        }
    }

    /// Buyer requests a refund on a `Funded` escrow, transitioning it to
    /// `RefundRequested` pending seller approval via `approve_refund`.
    /// Reverts with `NotAuthorized` if `caller` is not the buyer, or
    /// `InvalidStateTransition` if the escrow is not `Funded`. Emits
    /// `refund_requested`.
    pub fn request_refund(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        crate::internal::ensure_not_expired(&env, escrow_id)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidStateTransition);
        }

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::RefundRequested;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        // Build basket_tokens event data if this is a basket escrow
        let basket_tokens = load_basket_tokens(&env, escrow_id);
        let basket_event_data = if basket_tokens.len() > 1 {
            let mut tuples = soroban_sdk::Vec::new(&env);
            for i in 0..basket_tokens.len() {
                let entry = basket_tokens
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?;
                tuples.push_back((entry.token, entry.amount));
            }
            Some(tuples)
        } else {
            None
        };

        emit_refund_requested(
            &env,
            escrow_id,
            caller,
            prev_state,
            crate::EscrowState::RefundRequested,
            basket_event_data,
        );
        Ok(())
    }

    /// Seller (any payee) approves a pending refund request, transferring the
    /// full amount (and any basket tokens) back to the buyer. Reverts with
    /// `NotAuthorized` if `caller` is not a payee, or
    /// `InvalidStateTransition` if the escrow is not `RefundRequested`.
    /// Transitions the escrow to `Refunded`. Emits `refund_approved`.
    pub fn approve_refund(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        crate::internal::ensure_not_expired(&env, escrow_id)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let mut is_payee = false;
        for i in 0..escrow.payees.len() {
            if caller
                == escrow
                    .payees
                    .get(i)
                    .ok_or(ContractError::IndexOutOfBounds)?
                    .address
            {
                is_payee = true;
                break;
            }
        }
        if !is_payee {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::RefundRequested {
            return Err(ContractError::InvalidStateTransition);
        }

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(&env.current_contract_address(), &buyer, &escrow.amount);
        payout_basket_tokens(&env, escrow_id, &buyer)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Refunded;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        increment_counter(&env, &DataKey::TotalRefunded)?;

        emit_refund_approved(
            &env,
            escrow_id,
            caller,
            prev_state,
            crate::EscrowState::Refunded,
        );
        Ok(())
    }

    /// Creates multiple single-token escrows for `seller` in one call, each
    /// described by an `EscrowInput`. Returns the created escrow IDs in the
    /// same order as the input `escrows`. Each escrow still starts in
    /// `Pending` state and must be funded individually via `fund_escrow`.
    pub fn batch_create_escrow(
        env: Env,
        seller: Address,
        escrows: Vec<EscrowInput>,
    ) -> Result<Vec<u64>, ContractError> {
        seller.require_auth();
        ensure_not_paused(&env)?;

        let mut escrow_ids = Vec::new(&env);
        for input in escrows.into_iter() {
            let mut payees = Vec::new(&env);
            payees.push_back(Payee {
                address: seller.clone(),
                bps: 10_000,
            });
            let id = create_escrow_internal(
                &env,
                payees,
                input.buyer,
                input.resolver,
                input.token,
                input.amount,
                input.fee_bps,
                0, // resolver_fee_bps
                input.shipping_window,
                input.notes,
            )?;
            escrow_ids.push_back(id);
        }

        Ok(escrow_ids)
    }

    /// Executes a batch of contract calls in sequence, returning each call's
    /// result in the same order as `calls`. Supports a fixed allowlist of
    /// entry points (`initialize`, `pause_contract`, `unpause_contract`,
    /// `create_escrow`, `fund_escrow`, `mark_shipped`, `confirm_delivery`,
    /// `raise_dispute`, `resolve_dispute`, `auto_release`, `get_escrow`,
    /// `get_dispute`, `get_fee_config`, `set_arbitration_fee`,
    /// `get_arbitration_fee`, `rotate_resolver`, `cancel_escrow`); any other
    /// `function` name reverts the entire call with `NotAuthorized`, and a
    /// missing or undecodable argument reverts with `InvalidMulticallArg`.
    /// Authorization for each sub-call is enforced exactly as if it were
    /// called directly. Reverts with `ContractPaused` if the contract is
    /// paused.
    pub fn multicall(env: Env, calls: Vec<ContractCall>) -> Result<Vec<Val>, ContractError> {
        ensure_not_paused(&env)?;
        let mut results = Vec::new(&env);

        let s_initialize = Symbol::new(&env, "initialize");
        let s_pause_contract = Symbol::new(&env, "pause_contract");
        let s_unpause_contract = Symbol::new(&env, "unpause_contract");
        let s_create_escrow = Symbol::new(&env, "create_escrow");
        let s_fund_escrow = Symbol::new(&env, "fund_escrow");
        let s_mark_shipped = Symbol::new(&env, "mark_shipped");
        let s_confirm_delivery = Symbol::new(&env, "confirm_delivery");
        let s_raise_dispute = Symbol::new(&env, "raise_dispute");
        let s_resolve_dispute = Symbol::new(&env, "resolve_dispute");
        let s_auto_release = Symbol::new(&env, "auto_release");
        let s_get_escrow = Symbol::new(&env, "get_escrow");
        let s_get_dispute = Symbol::new(&env, "get_dispute");
        let s_get_fee_config = Symbol::new(&env, "get_fee_config");
        let s_set_arbitration_fee = Symbol::new(&env, "set_arbitration_fee");
        let s_get_arbitration_fee = Symbol::new(&env, "get_arbitration_fee");
        let s_rotate_resolver = Symbol::new(&env, "rotate_resolver");
        let s_cancel_escrow = Symbol::new(&env, "cancel_escrow");

        for call in calls.into_iter() {
            let res_val: Val = if call.function == s_fund_escrow {
                dispatch_fund_escrow(&env, &call.args)?
            } else if call.function == s_get_escrow {
                dispatch_get_escrow(&env, &call.args)?
            } else if call.function == s_mark_shipped {
                dispatch_mark_shipped(&env, &call.args)?
            } else if call.function == s_confirm_delivery {
                dispatch_confirm_delivery(&env, &call.args)?
            } else if call.function == s_raise_dispute {
                dispatch_raise_dispute(&env, &call.args)?
            } else if call.function == s_resolve_dispute {
                dispatch_resolve_dispute(&env, &call.args)?
            } else if call.function == s_auto_release {
                dispatch_auto_release(&env, &call.args)?
            } else if call.function == s_cancel_escrow {
                dispatch_cancel_escrow(&env, &call.args)?
            } else if call.function == s_rotate_resolver {
                dispatch_rotate_resolver(&env, &call.args)?
            } else if call.function == s_initialize {
                dispatch_initialize(&env, &call.args)?
            } else if call.function == s_pause_contract {
                dispatch_pause_contract(&env, &call.args)?
            } else if call.function == s_unpause_contract {
                dispatch_unpause_contract(&env, &call.args)?
            } else if call.function == s_get_dispute {
                dispatch_get_dispute(&env, &call.args)?
            } else if call.function == s_get_fee_config {
                dispatch_get_fee_config(&env, &call.args)?
            } else if call.function == s_set_arbitration_fee {
                dispatch_set_arbitration_fee(&env, &call.args)?
            } else if call.function == s_get_arbitration_fee {
                dispatch_get_arbitration_fee(&env, &call.args)?
            } else if call.function == s_create_escrow {
                dispatch_create_escrow(&env, &call.args)?
            } else {
                return Err(ContractError::NotAuthorized);
            };
            results.push_back(res_val);
        }
        Ok(results)
    }
}

fn parse_arg<T: TryFromVal<Env, Val>>(
    env: &Env,
    args: &Vec<Val>,
    idx: u32,
) -> Result<T, ContractError> {
    args.get(idx)
        .ok_or(ContractError::InvalidMulticallArg)?
        .try_into_val(env)
        .map_err(|_| ContractError::InvalidMulticallArg)
}

fn dispatch_fund_escrow(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let escrow_id: u64 = parse_arg(env, args, 0)?;
    let buyer: Address = parse_arg(env, args, 1)?;
    Escrow::fund_escrow(env.clone(), escrow_id, buyer)?;
    Ok(().into_val(env))
}

fn dispatch_get_escrow(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let escrow_id: u64 = parse_arg(env, args, 0)?;
    let res = Escrow::get_escrow(env.clone(), escrow_id)?;
    Ok(res.into_val(env))
}

fn dispatch_mark_shipped(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    let tracking_id: String = parse_arg(env, args, 2)?;
    Escrow::mark_shipped(env.clone(), caller, escrow_id, tracking_id)?;
    Ok(().into_val(env))
}

fn dispatch_confirm_delivery(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    Escrow::confirm_delivery(env.clone(), caller, escrow_id)?;
    Ok(().into_val(env))
}

fn dispatch_raise_dispute(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    let reason: Symbol = parse_arg(env, args, 2)?;
    let description: String = parse_arg(env, args, 3)?;
    let evidence_hash: BytesN<32> = parse_arg(env, args, 4)?;
    Escrow::raise_dispute(
        env.clone(),
        caller,
        escrow_id,
        reason,
        description,
        evidence_hash,
    )?;
    Ok(().into_val(env))
}

fn dispatch_resolve_dispute(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    let resolution: ResolutionType = parse_arg(env, args, 2)?;
    Escrow::resolve_dispute(env.clone(), caller, escrow_id, resolution)?;
    Ok(().into_val(env))
}

fn dispatch_auto_release(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let escrow_id: u64 = parse_arg(env, args, 0)?;
    Escrow::auto_release(env.clone(), escrow_id)?;
    Ok(().into_val(env))
}

fn dispatch_cancel_escrow(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    Escrow::cancel_escrow(env.clone(), caller, escrow_id)?;
    Ok(().into_val(env))
}

fn dispatch_rotate_resolver(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    let new_resolver: Address = parse_arg(env, args, 2)?;
    Escrow::rotate_resolver(env.clone(), caller, escrow_id, new_resolver)?;
    Ok(().into_val(env))
}

fn dispatch_initialize(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let admin: Address = parse_arg(env, args, 0)?;
    let fee_collector: Address = parse_arg(env, args, 1)?;
    let arbitration_fee_bps: u32 = parse_arg(env, args, 2)?;
    Escrow::initialize(env.clone(), admin, fee_collector, arbitration_fee_bps)?;
    Ok(().into_val(env))
}

fn dispatch_pause_contract(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    Escrow::queue_pause_contract(env.clone(), caller)?;
    Ok(().into_val(env))
}

fn dispatch_unpause_contract(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    Escrow::queue_unpause_contract(env.clone(), caller)?;
    Ok(().into_val(env))
}

fn dispatch_get_dispute(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let escrow_id: u64 = parse_arg(env, args, 0)?;
    let res = Escrow::get_dispute(env.clone(), escrow_id);
    Ok(res.into_val(env))
}

fn dispatch_get_fee_config(env: &Env, _args: &Vec<Val>) -> Result<Val, ContractError> {
    let res = Escrow::get_fee_config(env.clone());
    Ok(res.into_val(env))
}

fn dispatch_set_arbitration_fee(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let fee_bps: u32 = parse_arg(env, args, 1)?;
    Escrow::set_arbitration_fee(env.clone(), caller, fee_bps)?;
    Ok(().into_val(env))
}

fn dispatch_get_arbitration_fee(env: &Env, _args: &Vec<Val>) -> Result<Val, ContractError> {
    let res = Escrow::get_arbitration_fee(env.clone());
    Ok(res.into_val(env))
}

fn dispatch_create_escrow(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let payees: Vec<Payee> = parse_arg(env, args, 0)?;
    let buyer: Option<Address> = parse_arg(env, args, 1)?;
    let resolver: Address = parse_arg(env, args, 2)?;
    let token: Address = parse_arg(env, args, 3)?;
    let amount: i128 = parse_arg(env, args, 4)?;
    let fee_bps: u32 = parse_arg(env, args, 5)?;
    let resolver_fee_bps: u32 = parse_arg(env, args, 6)?;
    let shipping_window: u64 = parse_arg(env, args, 7)?;
    let res = Escrow::create_escrow(
        env.clone(),
        payees.into_val(env),
        buyer,
        resolver,
        token,
        amount,
        fee_bps,
        resolver_fee_bps,
        shipping_window,
        None,
    )?;
    Ok(res.into_val(env))
}
