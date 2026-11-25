//! Dispute lifecycle: raising a dispute, resolver voting, appeal window,
//! and finalization.

use crate::internal::*;
use crate::types::EvidenceEntry;
use crate::*;
use soroban_sdk::{contractimpl, token, Address, Bytes, BytesN, Env, Symbol, Vec};

/// Dispute reason symbols accepted by `raise_dispute`. Clients parse the
/// stored `DisputeData::reason` against this fixed set, so anything outside it
/// is rejected with `InvalidDisputeReason`.
pub const DISPUTE_REASONS: [&str; 6] = [
    "ITEM_NOT_RECEIVED",
    "NOT_AS_DESCRIBED",
    "DAMAGED",
    "DEFECTIVE",
    "FRAUD",
    "OTHER",
];

/// Returns true if `reason` is one of [`DISPUTE_REASONS`].
fn is_valid_dispute_reason(env: &Env, reason: &Symbol) -> bool {
    DISPUTE_REASONS
        .iter()
        .any(|allowed| Symbol::new(env, allowed) == *reason)
}

/// Returns `(buyer, primary_payee)` for a dispute participant check. The
/// primary payee is treated as the "seller" side throughout the escrow.
fn buyer_and_seller(escrow: &EscrowData) -> Result<(Address, Address), ContractError> {
    let buyer = escrow
        .buyer
        .clone()
        .ok_or(ContractError::EscrowHasNoBuyer)?;
    let seller = escrow
        .payees
        .get(0)
        .ok_or(ContractError::IndexOutOfBounds)?
        .address
        .clone();
    Ok((buyer, seller))
}

/// Returns the address of the first resolver whose recorded vote matches
/// `resolution`, used to attribute a deadlock-fallback resolution.
fn voter_for(votes: &Vec<ResolverVote>, resolution: &ResolutionType) -> Option<Address> {
    for i in 0..votes.len() {
        if let Some(vote) = votes.get(i) {
            if &vote.resolution == resolution {
                return Some(vote.resolver.clone());
            }
        }
    }
    None
}

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
    /// rejected with `InvalidEvidenceHash`: a dispute must carry a non-zero
    /// evidence commitment.
    ///
    /// `reason` must be one of [`DISPUTE_REASONS`]; any other symbol reverts
    /// with `InvalidDisputeReason`.
    pub fn raise_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        reason: Symbol,
        description: Bytes,
        evidence_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "raise_dispute"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorizedBuyer);
        }

        if escrow.state != EscrowState::Funded && escrow.state != EscrowState::Shipped {
            return Err(terminal_state_error(
                &escrow.state,
                ContractError::InvalidState,
            ));
        }

        if env.ledger().timestamp() >= (escrow.packed_timestamps & 0xFFFF_FFFF) {
            // Code 24 (DisputeWindowStillOpen) is reused for both raise_dispute (window closed, too late)
            // and confirm_delivery (window still open, too early) to maintain ABI stability.
            // See ERROR_CODES.md for both use cases.
            return Err(ContractError::DisputeWindowStillOpen);
        }

        // An all-zero digest is the "no evidence" placeholder; reject it so a
        // dispute is always backed by a real evidence commitment (issue #1206).
        if evidence_hash.to_array() == [0u8; 32] {
            return Err(ContractError::InvalidEvidenceHash);
        }

        if !is_valid_dispute_reason(&env, &reason) {
            return Err(ContractError::InvalidDisputeReason);
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
            fees_charged: false,
            compensated_resolvers: Vec::new(&env),
            resolution_reason: None,
            resolution_memo: Bytes::new(&env),
        };

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        save_dispute(&env, escrow_id, &dispute_data);
        crate::internal::clear_delivery_proposal(&env, escrow_id);
        // Counts unique disputed escrows: bumped here exactly once per escrow
        // (an escrow can only be disputed from a non-disputed state). Appeals
        // (`appeal_dispute`) do not re-increment it; see `COUNTER_KIND_APPEALS`.
        increment_sharded_counter(&env, COUNTER_KIND_DISPUTED, escrow_id)?;
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

    /// Appends additional evidence to a dispute's on-chain evidence log, so a
    /// seller/payee can submit rebuttal proof (and a resolver or the buyer can
    /// add further material) before voting occurs. `evidence_hash` is a
    /// commitment to off-chain evidence, as in `raise_dispute`.
    ///
    /// `caller` must be the buyer, a payee, or an assigned resolver of the
    /// escrow, otherwise `NotAuthorized`. The escrow must be `Disputed`
    /// (`InvalidState` otherwise). `description` longer than
    /// `MAX_DESCRIPTION_LEN` reverts with `InputTooLong`, and a log already
    /// holding `MAX_EVIDENCE_ENTRIES` entries reverts with
    /// `EvidenceLimitReached`. Emits `dispute_evidence_submitted`.
    pub fn submit_dispute_evidence(
        env: Env,
        caller: Address,
        escrow_id: u64,
        evidence_hash: BytesN<32>,
        description: Bytes,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "submit_dispute_evidence"))?;
        let escrow = load_escrow(&env, escrow_id)?;

        let is_buyer = escrow.buyer.as_ref() == Some(&caller);
        let mut is_payee = false;
        for i in 0..escrow.payees.inner.len() {
            let payee = escrow
                .payees
                .inner
                .get(i)
                .ok_or(ContractError::IndexOutOfBounds)?;
            if payee.address == caller {
                is_payee = true;
                break;
            }
        }
        if !is_buyer && !is_payee && !escrow.resolvers.contains(&caller) {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Disputed {
            return Err(terminal_state_error(
                &escrow.state,
                ContractError::InvalidState,
            ));
        }

        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(ContractError::InputTooLong);
        }

        let entry = EvidenceEntry {
            submitter: caller.clone(),
            timestamp: env.ledger().timestamp(),
            evidence_hash: evidence_hash.clone(),
            description: description.clone(),
        };
        let entry_count =
            crate::storage::append_evidence(&env, escrow_id, &entry, MAX_EVIDENCE_ENTRIES)?;

        emit_dispute_evidence_submitted(
            &env,
            escrow_id,
            caller,
            evidence_hash,
            description,
            entry_count,
        );
        Ok(())
    }

    /// Returns the additional evidence entries submitted for `escrow_id` via
    /// `submit_dispute_evidence`, oldest first. The buyer's initial evidence
    /// from `raise_dispute` lives on the dispute record (`get_dispute`).
    pub fn get_dispute_evidence(env: Env, escrow_id: u64) -> Vec<EvidenceEntry> {
        crate::storage::read_evidence_log(&env, escrow_id)
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
        caller.require_auth();
        crate::resolve_or_vote_internal(&env, caller, escrow_id, resolution, None, Bytes::new(&env))
    }

    /// Like `resolve_dispute`, but also records *why* the resolver decided as
    /// they did: an optional standardized `reason` symbol (e.g.
    /// `NON_DELIVERY`, `DAMAGED`, `MUTUAL`) and a free-form `memo` (pass an
    /// empty `Bytes` for none). They are stored on `DisputeData`
    /// (`resolution_reason` / `resolution_memo`, readable via `get_dispute`)
    /// when this call moves the escrow to `PendingFinalization`, and the
    /// reason is included in the `DisputePendingFinalization` event. For a
    /// multi-resolver set that is the vote that reaches the threshold. They
    /// are cleared again if the resolution is appealed.
    ///
    /// Reverts with `InputTooLong` if `memo` exceeds `MAX_DESCRIPTION_LEN`
    /// bytes, plus every error `resolve_dispute` can return.
    pub fn resolve_dispute_with_reason(
        env: Env,
        caller: Address,
        escrow_id: u64,
        resolution: ResolutionType,
        reason: Option<Symbol>,
        memo: Bytes,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        if memo.len() > MAX_DESCRIPTION_LEN {
            return Err(ContractError::InputTooLong);
        }
        crate::resolve_or_vote_internal(&env, caller, escrow_id, resolution, reason, memo)
    }

    /// Cast or change a vote on a disputed escrow.
    /// When threshold is reached, automatically transitions to `PendingFinalization`.
    pub fn vote(
        env: Env,
        caller: Address,
        escrow_id: u64,
        resolution: ResolutionType,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        crate::resolve_or_vote_internal(&env, caller, escrow_id, resolution, None, Bytes::new(&env))
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
    #[allow(clippy::too_many_lines)]
    pub fn finalize_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "finalize_dispute"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::PendingFinalization {
            return Err(terminal_state_error(
                &escrow.state,
                ContractError::NotPendingFinalization,
            ));
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

        let prev_state = escrow.state.clone();
        // For `Split` the event's `recipient` is the primary payee; the buyer
        // is paid its share separately below (see `buyer_split`).
        let recipient = match resolution {
            ResolutionType::Release | ResolutionType::Split(_) => escrow
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
        let buyer_split = if matches!(resolution, ResolutionType::Split(_)) {
            Some(
                escrow
                    .buyer
                    .clone()
                    .ok_or(ContractError::EscrowHasNoBuyer)?,
            )
        } else {
            None
        };

        let fee_collector: Address = crate::storage::read_global_config(&env)
            .fee_collector
            .ok_or(ContractError::NotInitialized)?;

        // Arbitration and resolver fees were only recorded provisionally by
        // `execute_resolution_transition`; they are paid here, atomically with
        // the principal payout, so an appeal can never leak them.
        let platform_fee_bps = escrow.platform_fee_bps;

        // ── FEE BASIS (original funded principal) ──
        // `execute_resolution_transition` provisionally subtracted the
        // arbitration and resolver fees from `escrow.amount`. Charging the
        // platform and protocol fees on that reduced remainder would compound
        // every fee on a shrinking base, so reconstruct the original funded
        // principal by adding the two recorded fees back.
        let fee_basis = escrow
            .amount
            .checked_add(dispute_data.arbitration_fee)
            .ok_or(ContractError::ArithmeticError)?
            .checked_add(dispute_data.resolver_fee)
            .ok_or(ContractError::ArithmeticError)?;

        let platform_fee = if platform_fee_bps > 0 {
            crate::helpers::payout::calculate_fee(fee_basis, platform_fee_bps)?
        } else {
            0
        };

        let treasury = if platform_fee > 0 {
            Some(read_treasury(&env)?)
        } else {
            None
        };

        // The protocol fee is a flat percentage of the same principal and is
        // independent of the platform fee — no compounding on a reduced base.
        let protocol_fee = crate::helpers::payout::calculate_fee(fee_basis, escrow.fee_bps)?;

        // Net payout: the principal minus the provisional arbitration and
        // resolver fees (`escrow.amount` is already net of them), minus the
        // platform and protocol fees settled here.
        let total_fees = platform_fee
            .checked_add(protocol_fee)
            .ok_or(ContractError::FeeCalculationOverflow)?;
        if total_fees > escrow.amount {
            return Err(ContractError::InsufficientBalance);
        }
        let payout_amount = escrow
            .amount
            .checked_sub(total_fees)
            .ok_or(ContractError::InsufficientBalance)?;

        // ── EFFECTS (state mutations) — must precede all external calls (CEI) ──
        let new_state = match resolution {
            ResolutionType::Release | ResolutionType::Split(_) => EscrowState::Completed,
            ResolutionType::Refund => EscrowState::Refunded,
        };
        escrow.state = new_state.clone();
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        dispute_data.status = DisputeStatus::Resolved;
        dispute_data.fees_charged = true;
        save_dispute(&env, escrow_id, &dispute_data);

        // Arbitration-fee accounting is bumped only when the fee is settled.
        let arbitration_fee = dispute_data.arbitration_fee;
        if arbitration_fee > 0 {
            let total_key = DataKey::TotalArbitrationFees(escrow.token.clone());
            let current_total: i128 = env.storage().instance().get(&total_key).unwrap_or(0);
            env.storage().instance().set(
                &total_key,
                &current_total
                    .checked_add(arbitration_fee)
                    .ok_or(ContractError::ArithmeticError)?,
            );
        }

        if protocol_fee > 0 {
            crate::storage::add_accumulated_fee(&env, &escrow.token, protocol_fee)?;
        }

        match resolution {
            ResolutionType::Release | ResolutionType::Split(_) => {
                record_escrow_completed(&env, escrow_id)?;
            }
            ResolutionType::Refund => {
                record_escrow_refunded(&env, escrow_id)?;
            }
        }

        // ── INTERACTIONS (external token transfers) ──
        payout(
            &env,
            &escrow.token,
            &fee_collector,
            dispute_data.arbitration_fee,
        );

        // Resolver fee: split equally among every resolver whose vote matches
        // the final resolution (issue #1203). Dust from the integer division
        // goes to `resolved_by` so shares plus dust sum to `resolver_fee`.
        let resolver_fee = dispute_data.resolver_fee;
        if resolver_fee > 0 {
            let winners = if dispute_data.compensated_resolvers.is_empty() {
                let mut w = soroban_sdk::Vec::new(&env);
                w.push_back(resolved_by.clone());
                w
            } else {
                dispute_data.compensated_resolvers.clone()
            };
            let count = i128::from(winners.len());
            let share = resolver_fee / count;
            let dust = resolver_fee - share * count;
            for winner in winners.iter() {
                payout(&env, &escrow.token, &winner, share);
            }
            if dust > 0 {
                payout(&env, &escrow.token, &resolved_by, dust);
            }
        }

        if let Some(ref treasury_addr) = treasury {
            crate::helpers::payout::payout_platform_fee(
                &env,
                escrow_id,
                &escrow.token,
                treasury_addr,
                platform_fee,
            );
        }
        match (&resolution, &buyer_split) {
            (ResolutionType::Split(split_bps), Some(buyer)) => {
                // payee_net + buyer_net == payout_amount: nothing is stranded.
                let (payee_net, buyer_net) =
                    crate::helpers::payout::split_net(payout_amount, *split_bps)?;
                payout(&env, &escrow.token, &recipient, payee_net);
                payout(&env, &escrow.token, buyer, buyer_net);
                crate::internal::payout_basket_tokens_split(
                    &env,
                    escrow_id,
                    &recipient,
                    buyer,
                    *split_bps,
                    escrow.fee_bps,
                )?;
            }
            _ => {
                payout(&env, &escrow.token, &recipient, payout_amount);
                payout_basket_tokens(&env, escrow_id, &recipient, Some(escrow.fee_bps))?;
            }
        }

        emit_dispute_resolved(
            &env,
            escrow_id,
            resolved_by,
            resolution,
            recipient,
            escrow.amount,
            dispute_data.arbitration_fee,
            dispute_data.resolver_fee,
            platform_fee,
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
    ///
    /// # Appeal fee (issue #913)
    ///
    /// When the admin has configured a non-zero appeal fee with
    /// `set_appeal_fee`, the appellant pays
    /// `floor(escrow.amount * appeal_fee_bps / 10_000)` out of pocket to the
    /// fee collector on every appeal, so spamming appeals up to `MAX_APPEALS`
    /// always costs the griefer. A zero fee (the default) preserves the
    /// historical free-appeal behavior.
    ///
    /// # Fee escalation (issue #1205)
    ///
    /// The fee escalates linearly with the appeal round: round `n` (1-based)
    /// charges `n * appeal_fee_bps`, capped at `BASIS_POINTS` (100%). The
    /// exact amount deducted and the round are logged in `dispute_appealed`.
    pub fn appeal_dispute(env: Env, caller: Address, escrow_id: u64) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "appeal_dispute"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::PendingFinalization {
            return Err(terminal_state_error(
                &escrow.state,
                ContractError::NotPendingFinalization,
            ));
        }

        let dispute_data = load_dispute(&env, escrow_id)?;
        let now = env.ledger().timestamp();

        if dispute_data.appeal_count >= crate::read_max_appeals(&env) {
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

        // Resolve the out-of-pocket appeal fee before mutating state. The
        // lookup is a plain storage read (a Check, not an Interaction); the
        // transfer itself happens after the state mutations below, per CEI.
        // Escalation (issue #1205): round `n` charges `n * appeal_fee_bps`
        // (1x, 2x, 3x, ...), so repeated appeals cost progressively more. The
        // multiplier and the escalated rate are bounded by
        // `MAX_APPEAL_FEE_MULTIPLIER` and `BASIS_POINTS` respectively.
        let appeal_round = dispute_data
            .appeal_count
            .checked_add(1)
            .ok_or(ContractError::MaxAppealsReached)?;
        let multiplier = appeal_round.min(crate::MAX_APPEAL_FEE_MULTIPLIER);
        let appeal_fee_bps = read_appeal_fee_bps(&env)
            .checked_mul(multiplier)
            .ok_or(ContractError::ArithmeticError)?
            .min(crate::BASIS_POINTS);
        // Fee basis: the original funded principal. `escrow.amount` was
        // provisionally reduced by the arbitration and resolver fees, so add
        // them back (checked) — consistent with `finalize_dispute`. Skipped
        // entirely when the appeal fee is disabled.
        let (appeal_fee, fee_basis) = if appeal_fee_bps == 0 {
            (0, escrow.amount)
        } else {
            let fee_basis = escrow
                .amount
                .checked_add(dispute_data.arbitration_fee)
                .ok_or(ContractError::ArithmeticError)?
                .checked_add(dispute_data.resolver_fee)
                .ok_or(ContractError::ArithmeticError)?;
            (
                crate::helpers::payout::calculate_fee(fee_basis, appeal_fee_bps)?,
                fee_basis,
            )
        };
        let fee_collector: Option<Address> = if appeal_fee > 0 {
            Some(
                crate::storage::read_global_config(&env)
                    .fee_collector
                    .ok_or(ContractError::NotInitialized)?,
            )
        } else {
            None
        };

        let prev_state = escrow.state.clone();
        // The provisional arbitration/resolver fees were never transferred, so
        // restoring `escrow.amount` to the full principal keeps it equal to
        // what the contract custodies. `clear_resolution` drops the
        // provisional allocations; the next round records fresh ones.
        escrow.amount = escrow
            .amount
            .checked_add(dispute_data.arbitration_fee)
            .ok_or(ContractError::ArithmeticError)?
            .checked_add(dispute_data.resolver_fee)
            .ok_or(ContractError::ArithmeticError)?;
        escrow.state = EscrowState::Disputed;

        let mut updated_dispute = dispute_data;
        updated_dispute.status = DisputeStatus::Active;
        updated_dispute.clear_resolution();
        updated_dispute.appeal_count = updated_dispute
            .appeal_count
            .checked_add(1)
            .ok_or(ContractError::MaxAppealsReached)?;

        // Clear votes so a fresh round begins. `ResolverVotes(escrow_id)` holds
        // the whole tally as one entry, so a single `remove` drops every voter
        // for `Multi` and `Fallback` sets alike (and `Single`). Together with
        // `clear_resolution()` above (resolution = 0, resolved_by = None,
        // resolved_at = 0) no round-1 state survives. Resolver eligibility is
        // never cached — `can_resolve_now` is recomputed on every call — so the
        // resolver who ruled in round 1 (primary or backup) may rule again.
        env.storage()
            .persistent()
            .remove(&DataKey::ResolverVotes(escrow_id));

        // ── EFFECTS (state mutations) — must precede all external calls (CEI) ──
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        save_dispute(&env, escrow_id, &updated_dispute);
        // Accounting invariant: an appeal re-enters `Disputed` but is NOT a new
        // disputed escrow, so `COUNTER_KIND_DISPUTED` (unique disputed escrows,
        // bumped once in `raise_dispute`) is intentionally left untouched.
        // Appeal volume is tracked in its own counter instead.
        increment_sharded_counter(&env, COUNTER_KIND_APPEALS, escrow_id)?;

        // ── INTERACTIONS (external token transfers) ──
        // The state transition above is already persisted, so a re-entrant
        // token cannot re-appeal the same `PendingFinalization` round twice.
        // A failing transfer (e.g. appellant cannot cover the fee) reverts the
        // whole invocation, including the state changes.
        if let Some(fee_collector) = fee_collector {
            token::Client::new(&env, &escrow.token).transfer(&caller, &fee_collector, &appeal_fee);
        }

        emit_dispute_appealed(&env, escrow_id, caller, appeal_fee, appeal_round, fee_basis);
        Ok(())
    }

    /// Returns the dispute record for `escrow_id`, or `None` if no dispute
    /// has ever been raised on it.
    pub fn get_dispute(env: Env, escrow_id: u64) -> Option<DisputeData> {
        load_dispute(&env, escrow_id).ok()
    }

    /// Forced-refund escape hatch for a dispute that no resolver ever
    /// concludes. Callable by either party (buyer or the primary payee) once
    /// `dispute.disputed_at + dispute_timeout` has elapsed, where
    /// `dispute_timeout` is admin-configurable via `set_dispute_timeout`
    /// (default 30 days). This prevents a resolver who has gone permanently
    /// offline from locking the escrowed funds forever.
    ///
    /// Immediately refunds a disputed escrow after the timeout. No arbitration
    /// or resolver fee is charged because no resolver acted; configured
    /// protocol/platform refund fees, if any, are handled consistently with
    /// finalization. Reverts with `InvalidState` if the escrow is not
    /// `Disputed`, `NotAuthorized` if `caller` is neither party, or
    /// `DisputeTimeoutNotElapsed` before the deadline.
    pub fn claim_dispute_timeout(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "claim_dispute_timeout"))?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Disputed {
            return Err(ContractError::InvalidState);
        }

        let (buyer, seller) = buyer_and_seller(&escrow)?;
        if caller != buyer && caller != seller {
            return Err(ContractError::NotAuthorized);
        }

        let dispute_data = load_dispute(&env, escrow_id)?;
        let deadline = dispute_data
            .disputed_at
            .checked_add(read_dispute_timeout(&env))
            .ok_or(ContractError::ArithmeticOverflow)?;
        if env.ledger().timestamp() < deadline {
            return Err(ContractError::DisputeTimeoutNotElapsed);
        }

        let mut dispute_data = dispute_data;
        let fee_config = read_global_config(&env);
        let protocol_fee = crate::helpers::payout::calculate_fee(escrow.amount, escrow.fee_bps)?;
        let treasury = if escrow.platform_fee_bps > 0 {
            fee_config.treasury.clone()
        } else {
            None
        };
        let platform_fee = if treasury.is_some() {
            crate::helpers::payout::calculate_fee(escrow.amount, escrow.platform_fee_bps)?
        } else {
            0
        };
        let total_fees = protocol_fee
            .checked_add(platform_fee)
            .ok_or(ContractError::FeeCalculationOverflow)?;
        if total_fees > escrow.amount {
            return Err(ContractError::InsufficientBalance);
        }
        let payout_amount = escrow
            .amount
            .checked_sub(total_fees)
            .ok_or(ContractError::InsufficientBalance)?;
        let fee_collector = if protocol_fee > 0 {
            fee_config.fee_collector.clone()
        } else {
            None
        };
        if protocol_fee > 0 && fee_collector.is_none() {
            return Err(ContractError::NotInitialized);
        }

        let prev_state = escrow.state.clone();
        // The contract address is the stable event/data marker for a timeout:
        // no resolver acted, so neither the caller nor a dormant resolver is
        // misidentified as the resolving party.
        let timeout_resolver = env.current_contract_address();
        escrow.state = EscrowState::Refunded;
        dispute_data.status = DisputeStatus::Resolved;
        dispute_data.set_resolution(ResolutionType::Refund);
        dispute_data.resolved_by = Some(timeout_resolver.clone());
        dispute_data.resolved_at = env.ledger().timestamp();
        dispute_data.arbitration_fee = 0;
        dispute_data.resolver_fee = 0;
        dispute_data.fees_charged = true;
        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));
        save_dispute(&env, escrow_id, &dispute_data);
        record_escrow_refunded(&env, escrow_id)?;

        if protocol_fee > 0 {
            crate::storage::add_accumulated_fee(&env, &escrow.token, protocol_fee)?;
        }

        if let Some(treasury) = treasury {
            payout(&env, &escrow.token, &treasury, platform_fee);
        }
        payout(&env, &escrow.token, &buyer, payout_amount);
        // Secondary basket tokens are refunded gross, matching the existing
        // refund paths and ensuring no dormant resolver receives them.
        payout_basket_tokens(&env, escrow_id, &buyer, None)?;

        emit_dispute_resolved(
            &env,
            escrow_id,
            timeout_resolver,
            ResolutionType::Refund,
            buyer,
            escrow.amount,
            0,
            0,
            platform_fee,
            prev_state,
            EscrowState::Refunded,
        );
        Ok(())
    }

    /// Permissionless, time-based escape hatch for a multi-resolver deadlock.
    /// Once `DISPUTE_DEADLOCK_WINDOW` has elapsed since the dispute was raised
    /// with votes cast but no threshold reached, resolves by simple majority;
    /// a perfect tie resolves to `Refund`. The resolver fee (if any) is paid
    /// to a resolver who backed the winning side.
    ///
    /// Reverts with `InvalidState` if the escrow is not `Disputed` (including
    /// when a prior call already moved it to `PendingFinalization`) or the
    /// dispute is not `Active`, `DisputeNotDeadlocked` if the window has not
    /// strictly elapsed or the votes already meet the threshold, or
    /// `NoResolverVotes` when no resolver voted (that resolver-inaction case is
    /// handled by `claim_dispute_timeout`).
    ///
    /// Idempotency / CEI: every guard below runs before any tallying or event
    /// emission, and `execute_resolution_transition` persists the
    /// `PendingFinalization` state before any token transfer or event. A
    /// second (or re-entrant) call therefore fails immediately on the state
    /// check and cannot transition the escrow twice.
    pub fn resolve_deadlocked_dispute(env: Env, escrow_id: u64) -> Result<(), ContractError> {
        ensure_action_not_paused(&env, Symbol::new(&env, "resolve_deadlocked_dispute"))?;
        let escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Disputed {
            return Err(ContractError::InvalidState);
        }

        let dispute_data = load_dispute(&env, escrow_id)?;
        if dispute_data.status != DisputeStatus::Active {
            return Err(ContractError::InvalidState);
        }

        let deadline = dispute_data
            .disputed_at
            .checked_add(crate::DISPUTE_DEADLOCK_WINDOW)
            .ok_or(ContractError::ArithmeticOverflow)?;
        // The window must have strictly elapsed: `now == deadline` is rejected.
        if env.ledger().timestamp() <= deadline {
            return Err(ContractError::DisputeNotDeadlocked);
        }

        let votes = load_resolver_votes(&env, escrow_id);
        if votes.is_empty() {
            return Err(ContractError::NoResolverVotes);
        }

        // If the threshold is actually met the escrow would already have moved
        // to PendingFinalization when the deciding vote was cast.
        if tally_votes(&votes, escrow.resolvers.threshold())?.is_some() {
            return Err(ContractError::DisputeNotDeadlocked);
        }

        let resolution = tally_votes_majority(&votes);
        let actor = voter_for(&votes, &resolution).ok_or(ContractError::NoResolverVotes)?;
        execute_resolution_transition(
            &env,
            escrow_id,
            escrow,
            actor,
            resolution,
            votes,
            true,
            None,
            Bytes::new(&env),
        )
    }

    /// Admin override to cancel an escrow in `PendingFinalization`.
    ///
    /// Provides an escape hatch when appeals have maxed out or finalization bugs occur,
    /// preventing funds from remaining indefinitely locked in `PendingFinalization`.
    /// Transitions state to `Refunded`, refunds buyer principal and basket tokens,
    /// and emits `escrow_canceled`.
    pub fn cancel_pending_finalization(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_action_not_paused(&env, Symbol::new(&env, "cancel_pending_finalization"))?;
        require_admin_caller(&env, &caller)?;
        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::PendingFinalization {
            return Err(ContractError::NotPendingFinalization);
        }

        let buyer = escrow
            .buyer
            .clone()
            .ok_or(ContractError::EscrowHasNoBuyer)?;

        let prev_state = escrow.state.clone();
        escrow.state = EscrowState::Refunded;
        record_escrow_refunded(&env, escrow_id)?;

        if let Ok(mut dispute) = load_dispute(&env, escrow_id) {
            dispute.status = DisputeStatus::Resolved;
            dispute.resolved_by = Some(caller.clone());
            dispute.set_resolution(ResolutionType::Refund);
            dispute.resolved_at = env.ledger().timestamp();
            save_dispute(&env, escrow_id, &dispute);
        }

        save_escrow(&env, escrow_id, &escrow, Some(&prev_state));

        payout(&env, &escrow.token, &buyer, escrow.amount);
        payout_basket_tokens(&env, escrow_id, &buyer, None)?;

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

    /// Admin override alias for `cancel_pending_finalization`.
    pub fn admin_cancel_finalization(
        env: Env,
        caller: Address,
        escrow_id: u64,
    ) -> Result<(), ContractError> {
        Self::cancel_pending_finalization(env, caller, escrow_id)
    }
}
