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
            0_u32,    // Default resolver fee
            3600_u64, // Default shipping window fallback
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
            let effective_expiry = expires_at
                .checked_add(grace_period)
                .ok_or(ContractError::ArithmeticOverflow)?;

            let key = DataKey::PendingExpiry(escrow_id);
            let ext = get_ttl_extension(&env);
            env.storage().persistent().set(&key, &effective_expiry);
            env.storage().persistent().extend_ttl(&key, ext / 2, ext);
        }

        Ok(escrow_id)
    }

    /// Buyer funds a pending escrow. Transitions Pending → Funded.
    pub fn fund_escrow(env: Env, escrow_id: u64, buyer: Address) -> Result<(), ContractError> {
        buyer.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "FUND"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        if let Some(expires_at) = env
            .storage()
            .persistent()
            .get::<DataKey, u64>(&DataKey::PendingExpiry(escrow_id))
        {
            if env.ledger().timestamp() > expires_at {
                return Err(ContractError::EscrowExpired);
            }
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
        emit_escrow_funded(
            &env,
            escrow_id,
            buyer,
            escrow.amount,
            crate::EscrowState::Pending,
            crate::EscrowState::Funded,
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

        // Validate multi-resolver configuration
        let resolver_set = ResolverSet::Multi(crate::types::MultiResolver {
            resolvers,
            threshold,
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
        // Extend instance storage TTL on every counter access
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
        ensure_not_paused(&env)?;
        let _ = load_escrow(&env, escrow_id)?;

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
        msgs.push_back(message);
        env.storage().persistent().set(&key, &msgs);
        emit_message_posted(&env, escrow_id, sender);
        Ok(())
    }

    /// Create escrow with a fallback resolver that can take over after a deadline.
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

    pub fn cancel_escrow(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
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
        emit_escrow_cancelled(
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

        emit_escrow_cancelled(
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

        if escrow.state != EscrowState::Funded {
            return Err(ContractError::InvalidState);
        }

        // Block shipping of expired escrows.
        ensure_not_expired(&env, &escrow)?;

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

    /// Confirms delivery and completes the escrow. Callable by the buyer.
    pub fn confirm_delivery(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
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

        if env.ledger().timestamp() < escrow.dispute_deadline {
            return Err(ContractError::DeliveryBeforeDisputeWindow);
        }

        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;

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

    pub fn auto_release(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_not_paused(&env)?;
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

        let fee_config = read_fee_config(&env);
        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotAuthorized)?;

        let first_payee_addr = escrow
            .payees
            .get(0)
            .ok_or(ContractError::IndexOutOfBounds)?
            .address
            .clone();
        let (protocol_fee, net_amount) = crate::helpers::payout::calculate_protocol_fee(
            escrow.amount,
            fee_config.protocol_fee_bps,
        )?;
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
            return Err(ContractError::InvalidAmount);
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

        let escrow_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EscrowCounter)
            .unwrap_or(1u64);
        let next_id = escrow_id
            .checked_add(1)
            .ok_or(ContractError::ArithmeticError)?;
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &next_id);

        let ext = get_ttl_extension(&env);
        env.storage().instance().extend_ttl(ext / 2, ext);

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
        emit_escrow_funded(
            &env,
            escrow_id,
            buyer.clone(),
            escrow.amount,
            crate::EscrowState::Pending,
            crate::EscrowState::Funded,
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

    pub fn request_refund(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
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

        emit_refund_requested(
            &env,
            escrow_id,
            caller,
            prev_state,
            crate::EscrowState::RefundRequested,
        );
        Ok(())
    }

    pub fn approve_refund(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
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
                let escrow_id: u64 = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let buyer: Address = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::fund_escrow(env.clone(), escrow_id, buyer)?;
                ().into_val(&env)
            } else if call.function == s_get_escrow {
                let escrow_id: u64 = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let res = Self::get_escrow(env.clone(), escrow_id)?;
                res.into_val(&env)
            } else if call.function == s_mark_shipped {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let tracking_id: String = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::mark_shipped(env.clone(), caller, escrow_id, tracking_id)?;
                ().into_val(&env)
            } else if call.function == s_confirm_delivery {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::confirm_delivery(env.clone(), caller, escrow_id)?;
                ().into_val(&env)
            } else if call.function == s_raise_dispute {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let reason: Symbol = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let description: String = call
                    .args
                    .get(3)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let evidence_hash: BytesN<32> = call
                    .args
                    .get(4)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::raise_dispute(
                    env.clone(),
                    caller,
                    escrow_id,
                    reason,
                    description,
                    evidence_hash,
                )?;
                ().into_val(&env)
            } else if call.function == s_resolve_dispute {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let resolution: ResolutionType = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::resolve_dispute(env.clone(), caller, escrow_id, resolution)?;
                ().into_val(&env)
            } else if call.function == s_auto_release {
                let escrow_id: u64 = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::auto_release(env.clone(), escrow_id)?;
                ().into_val(&env)
            } else if call.function == s_cancel_escrow {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::cancel_escrow(env.clone(), caller, escrow_id)?;
                ().into_val(&env)
            } else if call.function == s_rotate_resolver {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let escrow_id: u64 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let new_resolver: Address = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::rotate_resolver(env.clone(), caller, escrow_id, new_resolver)?;
                ().into_val(&env)
            } else if call.function == s_initialize {
                let admin: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let fee_collector: Address = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let arbitration_fee_bps: u32 = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::initialize(env.clone(), admin, fee_collector, arbitration_fee_bps)?;
                ().into_val(&env)
            } else if call.function == s_pause_contract {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::pause_contract(env.clone(), caller)?;
                ().into_val(&env)
            } else if call.function == s_unpause_contract {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::unpause_contract(env.clone(), caller)?;
                ().into_val(&env)
            } else if call.function == s_get_dispute {
                let escrow_id: u64 = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let res = Self::get_dispute(env.clone(), escrow_id);
                res.into_val(&env)
            } else if call.function == s_get_fee_config {
                let res = Self::get_fee_config(env.clone());
                res.into_val(&env)
            } else if call.function == s_set_arbitration_fee {
                let caller: Address = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let fee_bps: u32 = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                Self::set_arbitration_fee(env.clone(), caller, fee_bps)?;
                ().into_val(&env)
            } else if call.function == s_get_arbitration_fee {
                let res = Self::get_arbitration_fee(env.clone());
                res.into_val(&env)
            } else if call.function == s_create_escrow {
                let payees: Vec<Payee> = call
                    .args
                    .get(0)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let buyer: Option<Address> = call
                    .args
                    .get(1)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let resolver: Address = call
                    .args
                    .get(2)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let token: Address = call
                    .args
                    .get(3)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let amount: i128 = call
                    .args
                    .get(4)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let fee_bps: u32 = call
                    .args
                    .get(5)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let resolver_fee_bps: u32 = call
                    .args
                    .get(6)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let shipping_window: u64 = call
                    .args
                    .get(7)
                    .ok_or(ContractError::InvalidAmount)?
                    .try_into_val(&env)
                    .map_err(|_| ContractError::InvalidAmount)?;
                let res = Self::create_escrow(
                    env.clone(),
                    payees.into_val(&env),
                    buyer,
                    resolver,
                    token,
                    amount,
                    fee_bps,
                    resolver_fee_bps,
                    shipping_window,
                    None,
                )?;
                res.into_val(&env)
            } else {
                return Err(ContractError::NotAuthorized);
            };
            results.push_back(res_val);
        }
        Ok(results)
    }
}
