//! Dispute lifecycle: raising a dispute, resolver voting, appeal window,
//! and finalization.

use crate::internal::*;
use crate::*;
use soroban_sdk::{contractimpl, token, Address, BytesN, Env, String, Symbol, Vec};

#[contractimpl]
impl Escrow {
    /// Buyer raises a dispute on a funded or shipped escrow.
    ///
    /// `evidence_hash` is a commitment to evidence that stays off chain —
    /// conventionally the SHA-256 digest of the evidence file. `BytesN<32>`
    /// means the host rejects any other length while decoding the arguments,
    /// so no length check is needed here; the contract cannot verify the
    /// *content*, so consumers must re-hash the evidence and compare it against
    /// the stored digest before treating it as proof. The all-zero digest is
    /// accepted and conventionally means "no evidence attached".
    pub fn raise_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        reason: Symbol,
        description: String,
        evidence_hash: BytesN<32>,
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

        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if env.ledger().timestamp() >= escrow.dispute_deadline {
            // Code 24 (DisputeWindowStillOpen) is reused for both raise_dispute (window closed, too late)
            // and confirm_delivery (window still open, too early) to maintain ABI stability.
            // See ERROR_CODES.md for both use cases.
            return Err(ContractError::DisputeWindowStillOpen);
        }

        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(ContractError::InputTooLong);
        }

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Disputed;

        let dispute_data = DisputeData {
            escrow_id,
            reason: reason.clone(),
            description: description.clone(),
            evidence_hash: evidence_hash.clone(),
            status: DisputeStatus::Active,
            disputed_at: env.ledger().timestamp(),
            tracking_id: escrow.tracking_id.clone(),
            resolution: 0,
            resolved_by: None,
            appeal_count: 0,
            resolved_at: 0,
            arbitration_fee: 0,
            resolver_fee: 0,
        };

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        save_dispute(&env, escrow_id, &dispute_data);
        increment_counter(&env, &DataKey::TotalDisputed)?;
        emit_dispute_raised(
            &env,
            escrow_id,
            buyer,
            reason,
            description,
            evidence_hash,
            prev_state,
            crate::EscrowState::Disputed,
        );
        Ok(())
    }

    /// Casts `caller`'s vote for `resolution` on a disputed escrow. An alias
    /// for `vote` retained for backward compatibility. Reverts with
    /// `InvalidState` if the escrow is not `Disputed`, or `NotAuthorized` if
    /// `caller` is not an eligible resolver right now. Once enough votes are
    /// recorded to meet the resolver set's threshold, transitions the escrow
    /// to `PendingFinalization` (see `finalize_dispute`). Emits
    /// `resolver_vote_recorded`.
    pub fn resolve_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        resolution: ResolutionType,
    ) -> Result<(), ContractError> {
        crate::resolve_or_vote_internal(&env, caller, escrow_id, resolution)
    }

    /// Cast or change a vote on a disputed escrow.
    /// When threshold is reached, automatically transitions to PendingFinalization.
    pub fn vote(
        env: Env,
        caller: Address,
        escrow_id: u64,
        resolution: ResolutionType,
    ) -> Result<(), ContractError> {
        crate::resolve_or_vote_internal(&env, caller, escrow_id, resolution)
    }

    /// Get the resolver votes for a disputed escrow (for multi-resolver voting tracking)
    pub fn get_resolver_votes(env: Env, escrow_id: u64) -> Vec<ResolverVote> {
        load_resolver_votes(&env, escrow_id)
    }

    /// Pays out a resolved dispute once its appeal window has elapsed.
    /// Reverts with `NotPendingFinalization` if the escrow is not in
    /// `PendingFinalization`, or `AppealWindowActive` if `now <
    /// resolved_at + APPEAL_WINDOW`. Transfers the platform fee to the
    /// treasury (if configured) and the remainder — minus the protocol fee —
    /// to the winning party, then transitions the escrow to `Completed`
    /// (release) or `Refunded` (refund). Emits `dispute_resolved`.
    pub fn finalize_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::PendingFinalization {
            return Err(ContractError::NotPendingFinalization);
        }

        let mut dispute_data = load_dispute(&env, escrow_id)?;
        let now = env.ledger().timestamp();

        let resolution = dispute_data
            .get_resolution()
            .ok_or(ContractError::InvalidState)?;
        let resolved_by = dispute_data
            .resolved_by
            .clone()
            .ok_or(ContractError::InvalidState)?;

        let appeal_deadline = dispute_data
            .resolved_at
            .checked_add(APPEAL_WINDOW)
            .ok_or(ContractError::ArithmeticError)?;
        if now < appeal_deadline {
            return Err(ContractError::AppealWindowActive);
        }

        let _prev_state = escrow.state.clone();
        let recipient = match resolution {
            ResolutionType::Release => escrow
                .payees
                .get(0)
                .ok_or(ContractError::IndexOutOfBounds)?
                .address
                .clone(),
            ResolutionType::Refund => escrow
                .buyer
                .clone()
                .ok_or(ContractError::EscrowHasNoBuyer)?,
        };

        let fee_collector: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeCollector)
            .ok_or(ContractError::NotInitialized)?;

        let platform_fee_bps = read_platform_fee_bps(&env);
        let platform_fee = if platform_fee_bps > 0 {
            crate::helpers::payout::calculate_fee(escrow.amount, platform_fee_bps)?
        } else {
            0
        };

        let treasury = if platform_fee > 0 {
            Some(read_treasury(&env)?)
        } else {
            None
        };

        let seller_amount = escrow
            .amount
            .checked_sub(platform_fee)
            .ok_or(ContractError::ArithmeticError)?;

        if platform_fee > 0 {
            if let Some(ref treasury_addr) = treasury {
                let token_client = token::Client::new(&env, &escrow.token);
                token_client.transfer(
                    &env.current_contract_address(),
                    treasury_addr,
                    &platform_fee,
                );
            }
        }

        transfer_with_protocol_fee(
            &env,
            &escrow.token,
            &recipient,
            &fee_collector,
            seller_amount,
            escrow.fee_bps,
        )?;
        payout_basket_tokens(&env, escrow_id, &recipient)?;

        let prev_state = escrow.state.clone();
        let new_state = match resolution {
            ResolutionType::Release => EscrowState::Completed,
            ResolutionType::Refund => EscrowState::Refunded,
        };
        escrow.state = new_state.clone();

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        dispute_data.status = DisputeStatus::Resolved;
        save_dispute(&env, escrow_id, &dispute_data);

        match resolution {
            ResolutionType::Release => increment_counter(&env, &DataKey::TotalCompleted)?,
            ResolutionType::Refund => increment_counter(&env, &DataKey::TotalRefunded)?,
        };

        emit_dispute_resolved(
            &env,
            escrow_id,
            resolved_by,
            resolution,
            recipient,
            escrow.amount,
            dispute_data.arbitration_fee,
            dispute_data.resolver_fee,
            prev_state,
            new_state,
        );
        Ok(())
    }

    /// Reopens a resolved dispute for another round of voting. Callable by
    /// the escrow's buyer or seller while the escrow is `PendingFinalization`
    /// and the appeal window (`resolved_at + APPEAL_WINDOW`) has not yet
    /// elapsed. Reverts with `NotPendingFinalization` if the escrow isn't
    /// awaiting finalization, `MaxAppealsReached` if `appeal_count >=
    /// MAX_APPEALS`, `DisputeWindowStillOpen` if the appeal window has
    /// closed, or `NotAuthorized` if `caller` is neither the buyer nor
    /// seller. On success, transitions the escrow back to `Disputed`, clears
    /// the prior resolution and any recorded votes, and increments
    /// `appeal_count`. Emits `dispute_appealed`.
    pub fn appeal_dispute(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::PendingFinalization {
            return Err(ContractError::NotPendingFinalization);
        }

        let dispute_data = load_dispute(&env, escrow_id)?;
        let now = env.ledger().timestamp();

        if dispute_data.appeal_count >= crate::MAX_APPEALS {
            return Err(ContractError::MaxAppealsReached);
        }

        // Appeal window must still be active (based on resolved_at)
        let appeal_deadline = dispute_data
            .resolved_at
            .checked_add(APPEAL_WINDOW)
            .ok_or(ContractError::ArithmeticError)?;
        if now >= appeal_deadline {
            return Err(ContractError::DisputeWindowStillOpen);
        }

        // Only buyer or seller can appeal
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
        if caller != buyer && caller != seller_addr {
            return Err(ContractError::NotAuthorized);
        }

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Disputed;

        let mut updated_dispute = dispute_data;
        updated_dispute.status = DisputeStatus::Active;
        updated_dispute.clear_resolution();
        updated_dispute.appeal_count = updated_dispute
            .appeal_count
            .checked_add(1)
            .ok_or(ContractError::ArithmeticError)?;

        // Clear votes for Multi resolver sets so a fresh round begins
        if matches!(escrow.resolvers, ResolverSet::Multi(_)) {
            env.storage()
                .persistent()
                .remove(&DataKey::ResolverVotes(escrow_id));
        }

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        save_dispute(&env, escrow_id, &updated_dispute);

        emit_dispute_appealed(&env, escrow_id, caller);
        Ok(())
    }

    /// Returns the dispute record for `escrow_id`, or `None` if no dispute
    /// has ever been raised on it.
    pub fn get_dispute(env: Env, escrow_id: u64) -> Option<DisputeData> {
        load_dispute(&env, escrow_id).ok()
    }
}
