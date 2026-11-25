//! Internal dispute voting and state-transition logic.

use crate::internal::{
    load_dispute, load_resolver_votes, save_dispute, save_escrow, save_resolver_votes,
};
use crate::*;
use soroban_sdk::{Address, Env, Vec};

pub(crate) fn add_or_update_vote(
    env: &Env,
    escrow_id: u64,
    resolver: &Address,
    resolution: ResolutionType,
) -> Vec<ResolverVote> {
    let mut votes = load_resolver_votes(env, escrow_id);
    let current_time = env.ledger().timestamp();
    let mut found = false;
    for i in 0..votes.len() {
        if let Some(vote) = votes.get(i) {
            if vote.resolver == *resolver {
                let mut updated = vote.clone();
                updated.resolution = resolution.clone();
                updated.voted_at = current_time;
                votes.set(i, updated);
                found = true;
                break;
            }
        }
    }
    if !found {
        votes.push_back(ResolverVote {
            resolver: resolver.clone(),
            resolution,
            voted_at: current_time,
        });
    }
    votes
}

pub(crate) fn tally_votes(
    votes: &Vec<ResolverVote>,
    threshold: u32,
) -> Result<Option<ResolutionType>, ContractError> {
    if votes.is_empty() {
        return Ok(None);
    }
    let mut release_count = 0u32;
    let mut refund_count = 0u32;
    for i in 0..votes.len() {
        if let Some(vote) = votes.get(i) {
            match vote.resolution {
                ResolutionType::Release => {
                    release_count = release_count
                        .checked_add(1)
                        .ok_or(ContractError::ArithmeticError)?;
                }
                ResolutionType::Refund => {
                    refund_count = refund_count
                        .checked_add(1)
                        .ok_or(ContractError::ArithmeticError)?;
                }
                ResolutionType::Split(_) => {}
            }
        }
    }
    if release_count >= threshold {
        Ok(Some(ResolutionType::Release))
    } else if refund_count >= threshold {
        Ok(Some(ResolutionType::Refund))
    } else {
        Ok(None)
    }
}

pub(crate) fn tally_votes_majority(votes: &Vec<ResolverVote>) -> ResolutionType {
    let mut release_count = 0u32;
    let mut refund_count = 0u32;
    for i in 0..votes.len() {
        if let Some(vote) = votes.get(i) {
            match vote.resolution {
                ResolutionType::Release => release_count = release_count.saturating_add(1),
                ResolutionType::Refund => refund_count = refund_count.saturating_add(1),
                ResolutionType::Split(_) => {}
            }
        }
    }
    if release_count > refund_count {
        ResolutionType::Release
    } else {
        ResolutionType::Refund
    }
}

fn winning_resolvers(
    env: &Env,
    votes: &Vec<ResolverVote>,
    final_resolution: &ResolutionType,
    caller: &Address,
) -> Vec<Address> {
    let mut winners = Vec::new(env);
    for vote in votes.iter() {
        if &vote.resolution == final_resolution && !winners.contains(&vote.resolver) {
            winners.push_back(vote.resolver.clone());
        }
    }
    if winners.is_empty() {
        winners.push_back(caller.clone());
    }
    winners
}

pub(crate) fn execute_resolution_transition(
    env: &Env,
    escrow_id: u64,
    escrow: EscrowData,
    caller: Address,
    final_resolution: ResolutionType,
    votes: Vec<ResolverVote>,
    charge_fees: bool,
) -> Result<(), ContractError> {
    let mut dispute_data = load_dispute(env, escrow_id)?;
    let fees_already_charged = dispute_data.fees_charged;
    let (arbitration_fee, resolver_fee) = if fees_already_charged {
        (dispute_data.arbitration_fee, dispute_data.resolver_fee)
    } else if !charge_fees {
        (0, 0)
    } else {
        // Issue #1260: each fee applies a 1-stroop minimum for a non-zero
        // rate, so on a micro principal the two minimums can exceed the
        // amount. `calculate_dispute_fees` caps the resolver fee at the
        // remainder after arbitration, keeping the combined fee within the
        // principal and the `checked_sub` deductions below non-negative.
        crate::helpers::payout::calculate_dispute_fees(
            escrow.amount,
            escrow.arbitration_fee_bps,
            escrow.resolver_fee_bps,
        )?
    };

    let fee_collector = if fees_already_charged {
        None
    } else {
        Some(
            crate::storage::read_global_config(env)
                .fee_collector
                .ok_or(ContractError::NotInitialized)?,
        )
    };

    let prev_state = escrow.state.clone();
    let mut updated_escrow = escrow;
    if !fees_already_charged && charge_fees {
        updated_escrow.amount = updated_escrow
            .amount
            .checked_sub(arbitration_fee)
            .ok_or(ContractError::ArithmeticError)?
            .checked_sub(resolver_fee)
            .ok_or(ContractError::ArithmeticError)?;
        let total_key = DataKey::TotalArbitrationFees(updated_escrow.token.clone());
        let current_total: i128 = env.storage().instance().get(&total_key).unwrap_or(0);
        env.storage().instance().set(
            &total_key,
            &current_total
                .checked_add(arbitration_fee)
                .ok_or(ContractError::ArithmeticError)?,
        );
    }

    let now = env.ledger().timestamp();
    if now < dispute_data.disputed_at {
        return Err(ContractError::TimestampInvariantViolated);
    }
    let appeal_deadline = now
        .checked_add(crate::APPEAL_WINDOW)
        .ok_or(ContractError::ArithmeticError)?;
    dispute_data.set_resolution(final_resolution.clone());
    dispute_data.resolved_by = Some(caller.clone());
    dispute_data.resolved_at = now;
    dispute_data.arbitration_fee = arbitration_fee;
    dispute_data.resolver_fee = resolver_fee;
    if !fees_already_charged {
        dispute_data.compensated_resolvers =
            winning_resolvers(env, &votes, &final_resolution, &caller);
    }
    dispute_data.fees_charged = true;
    updated_escrow.state = EscrowState::PendingFinalization;

    save_escrow(env, escrow_id, &updated_escrow, Some(&prev_state));
    save_dispute(env, escrow_id, &dispute_data);
    save_resolver_votes(env, escrow_id, &votes);

    if let Some(fee_collector) = fee_collector {
        crate::helpers::payout::payout(env, &updated_escrow.token, &fee_collector, arbitration_fee);
        if resolver_fee > 0 {
            let winners = winning_resolvers(env, &votes, &final_resolution, &caller);
            let count = i128::from(winners.len());
            let share = resolver_fee / count;
            let dust = resolver_fee - share * count;
            for winner in winners.iter() {
                crate::helpers::payout::payout(env, &updated_escrow.token, &winner, share);
            }
            if dust > 0 {
                crate::helpers::payout::payout(env, &updated_escrow.token, &caller, dust);
            }
        }
    }

    emit_dispute_pending_finalization(
        env,
        escrow_id,
        caller,
        final_resolution,
        updated_escrow.amount,
        appeal_deadline,
        None,
    );
    Ok(())
}
