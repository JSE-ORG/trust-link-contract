use crate::types::{BoundedPayees, Escrow, ResolutionType};
use crate::{ContractError, BASIS_POINTS};
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Floor of `amount * bps / BASIS_POINTS` (matches [`crate::internal::distribute_to_payees`]).
pub fn floored_payee_share(amount: i128, bps: u32) -> Result<i128, ContractError> {
    if amount < 0 {
        return Err(ContractError::InvalidAmount);
    }
    amount
        .checked_mul(i128::from(bps))
        .ok_or(ContractError::ArithmeticError)?
        .checked_div(i128::from(BASIS_POINTS))
        .ok_or(ContractError::ArithmeticError)
}

/// Settlement simulation for multi-payee escrows: mirrors on-chain distribution
/// but lists only payees that would receive a strictly positive transfer.
pub fn calculate_multi_payee_allocations(
    env: &Env,
    payees: &BoundedPayees,
    amount: i128,
) -> Result<Vec<TransferInstruction>, ContractError> {
    if amount < 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut remaining = amount;
    let mut transfers = Vec::new(env);

    for i in 1..payees.inner.len() {
        let payee = payees
            .inner
            .get(i)
            .ok_or(ContractError::PayeeIndexOutOfBounds)?;
        let payee_amount = floored_payee_share(amount, payee.bps)?;
        if payee_amount > 0 {
            transfers.push_back(TransferInstruction {
                recipient: payee.address.clone(),
                amount: payee_amount,
            });
        }
        remaining = remaining
            .checked_sub(payee_amount)
            .ok_or(ContractError::ArithmeticError)?;
    }

    let first_payee = payees
        .inner
        .get(0)
        .ok_or(ContractError::PayeeIndexOutOfBounds)?;
    if remaining > 0 {
        transfers.push_back(TransferInstruction {
            recipient: first_payee.address.clone(),
            amount: remaining,
        });
    }

    Ok(transfers)
}

/// Computes the protocol fee for `amount` at `fee_bps` basis points.
///
/// # Rounding policy: floor (round toward zero)
///
/// The fee is `floor(amount * fee_bps / 10_000)`. Integer division truncates,
/// so any sub-stroop remainder is dropped from the fee. Crucially, callers
/// derive the payout as `net = amount - fee` (see [`calculate_protocol_fee`]),
/// which means the truncated remainder is **not** lost — it stays in `net` and
/// is paid to the recipient (seller on release, buyer on refund). The `fee` is
/// forwarded directly to the configured fee collector rather than retained by
/// the contract. The invariant `net + fee == amount` therefore always holds
/// and no stroop is ever stranded.
///
/// Consequence of flooring: for amounts where `amount * fee_bps < 10_000` the
/// fee rounds down to `0` and is effectively waived. The `MIN_ESCROW_AMOUNT`
/// guard (`1_000_000` stroops) in `create_escrow` keeps escrows large enough that
/// a non-zero `fee_bps` always yields a meaningful, non-zero fee.
///
/// Floor is chosen deliberately over ceiling/round-half-up: it guarantees the
/// contract never owes more than it custodies and never over-collects fees at
/// the recipient's expense.
///
/// The computation is split (`amount / 10_000 * fee_bps` plus the remainder
/// term) to avoid overflowing `i128` for large amounts.
///
/// `fee_bps` beyond `BASIS_POINTS` (over 100%) is rejected with
/// `FeeExceedsMax`: without this bound the split-div trick that avoids i128
/// overflow can still yield a `fee` greater than `amount`, breaking the
/// fee-boundedness invariant every caller relies on.
pub fn calculate_fee(amount: i128, fee_bps: u32) -> Result<i128, ContractError> {
    if amount < 0 {
        return Err(ContractError::InvalidAmount);
    }
    if fee_bps > BASIS_POINTS {
        return Err(ContractError::FeeExceedsMax);
    }

    let part1 = amount
        .checked_div(i128::from(BASIS_POINTS))
        .ok_or(ContractError::FeeCalculationOverflow)?
        .checked_mul(i128::from(fee_bps))
        .ok_or(ContractError::FeeCalculationOverflow)?;

    let part2 = (amount % i128::from(BASIS_POINTS))
        .checked_mul(i128::from(fee_bps))
        .ok_or(ContractError::FeeCalculationOverflow)?
        .checked_div(i128::from(BASIS_POINTS))
        .ok_or(ContractError::FeeCalculationOverflow)?;

    part1
        .checked_add(part2)
        .ok_or(ContractError::FeeCalculationOverflow)
}

/// Splits `net` between the payee and the buyer for a `Split(split_bps)`
/// resolution: `payee_net = net * split_bps / 10_000` (floored) and
/// `buyer_net = net - payee_net`, so `payee_net + buyer_net == net` always
/// holds and no stroop is stranded (the rounding remainder goes to the buyer).
///
/// Computed as `net / 10_000 * bps + (net % 10_000) * bps / 10_000`, which is
/// the exact floor of `net * bps / 10_000` without risking an overflow of the
/// intermediate product for very large `net`.
pub fn split_net(net: i128, split_bps: u32) -> Result<(i128, i128), ContractError> {
    if split_bps > BASIS_POINTS {
        return Err(ContractError::InvalidSplitBps);
    }
    let bps = i128::from(split_bps);
    let denom = i128::from(BASIS_POINTS);
    let payee_net = (net / denom)
        .checked_mul(bps)
        .ok_or(ContractError::AmountCalculationOverflow)?
        .checked_add((net % denom) * bps / denom)
        .ok_or(ContractError::AmountCalculationOverflow)?;
    let buyer_net = net
        .checked_sub(payee_net)
        .ok_or(ContractError::AmountCalculationOverflow)?;
    Ok((payee_net, buyer_net))
}

pub fn calculate_protocol_fee(amount: i128, fee_bps: u32) -> Result<(i128, i128), ContractError> {
    let fee = calculate_fee(amount, fee_bps)?;
    let net = amount
        .checked_sub(fee)
        .ok_or(ContractError::AmountCalculationOverflow)?;
    Ok((fee, net))
}

/// Transfers `amount` from the contract to `recipient` after deducting the
/// protocol fee at `fee_bps` basis points, forwarding the fee to
/// `fee_collector`.
///
/// Returns `(fee, net)` where `fee + net == amount`.
pub(crate) fn transfer_with_protocol_fee(
    env: &Env,
    token_addr: &Address,
    recipient: &Address,
    fee_collector: &Address,
    amount: i128,
    fee_bps: u32,
) -> Result<(i128, i128), ContractError> {
    let (fee, net) = calculate_protocol_fee(amount, fee_bps)?;
    payout(env, token_addr, recipient, net);
    payout(env, token_addr, fee_collector, fee);
    Ok((fee, net))
}

/// Transfers one basket token entry to `recipient` after deducting the
/// protocol fee (`fee_bps`) and — only when a `treasury` is configured — the
/// platform fee (`platform_fee_bps`).
///
/// The recipient receives exactly `amount - protocol_fee - platform_fee`; the
/// protocol fee is forwarded to `fee_collector` and the platform fee to
/// `treasury`. This mirrors the fees charged on the escrow's primary token so
/// a large secondary basket balance cannot be used to evade them (Issue
/// #1219). Both fees are computed independently on the gross `amount` (no
/// compounding), and `payout` skips any fee that rounds to zero.
///
/// Returns `(net_amount, total_fee)`.
pub(crate) fn transfer_basket_with_fees(
    env: &Env,
    escrow_id: u64,
    token_addr: &Address,
    recipient: &Address,
    amount: i128,
    fee_bps: u32,
    platform_fee_bps: u32,
    _fee_collector: &Address,
    treasury: Option<&Address>,
) -> Result<(i128, i128), ContractError> {
    let protocol_fee = calculate_fee(amount, fee_bps)?;
    let platform_fee = match treasury {
        Some(_) => calculate_fee(amount, platform_fee_bps)?,
        None => 0,
    };
    let net_amount = amount
        .checked_sub(protocol_fee)
        .ok_or(ContractError::AmountCalculationOverflow)?
        .checked_sub(platform_fee)
        .ok_or(ContractError::AmountCalculationOverflow)?;
    let total_fee = protocol_fee
        .checked_add(platform_fee)
        .ok_or(ContractError::AmountCalculationOverflow)?;

    payout(env, token_addr, recipient, net_amount);
    crate::storage::add_accumulated_fee(env, token_addr, protocol_fee)?;
    if let Some(treasury_addr) = treasury {
        payout_platform_fee(env, escrow_id, token_addr, treasury_addr, platform_fee);
    }
    Ok((net_amount, total_fee))
}

/// Transfers a platform fee to `treasury` and publishes the matching
/// `PlatformFeeCollected` telemetry event. A fee that rounds down to zero is
/// not disbursed and therefore produces no event.
pub(crate) fn payout_platform_fee(
    env: &Env,
    escrow_id: u64,
    token_addr: &Address,
    treasury: &Address,
    fee_amount: i128,
) {
    if fee_amount <= 0 {
        return;
    }
    payout(env, token_addr, treasury, fee_amount);
    crate::events::emit_platform_fee_collected(
        env,
        escrow_id,
        token_addr.clone(),
        treasury.clone(),
        fee_amount,
    );
}

/// Transfers `amount` of `token_addr` from this contract to `recipient`.
///
/// This is the single choke point every payout path shares — protocol and
/// arbitration fees, treasury platform fees, payee distributions, basket
/// tokens, refunds, and emergency drains — so the "only move funds when there
/// is actually something to move" rule lives in exactly one place. Non-positive
/// amounts (a fee that rounded down to zero, a zero-share basket entry, an
/// already-fully-deducted remainder) are skipped instead of issuing a
/// zero-value `transfer`, which keeps the emitted token events clean and saves
/// the cross-contract call.
///
/// A failing transfer panics, which reverts the whole invocation: there is no
/// partial-payout path, and callers are expected to have persisted their
/// `EscrowState` transition *before* calling this helper (see the CEI ordering
/// in `settle_escrow_to_payees`, `reclaim_expired`, and `finalize_dispute`).
pub(crate) fn payout(env: &Env, token_addr: &Address, recipient: &Address, amount: i128) {
    if amount <= 0 {
        return;
    }
    crate::internal::transfer_helper(
        env,
        token_addr,
        &env.current_contract_address(),
        recipient,
        amount,
    );
}

/// Like [`transfer_basket_with_fees`], but for a `Split(split_bps)` dispute
/// resolution: the protocol and platform fees are taken from the gross
/// `amount` exactly as for a full settlement, then the remaining net is split
/// between `payee` (`split_bps`) and `buyer` (the remainder) via [`split_net`].
pub(crate) fn transfer_basket_split_with_fees(
    env: &Env,
    token_addr: &Address,
    payee: &Address,
    buyer: &Address,
    amount: i128,
    split_bps: u32,
    fee_bps: u32,
    platform_fee_bps: u32,
    _fee_collector: &Address,
    treasury: Option<&Address>,
) -> Result<(), ContractError> {
    let protocol_fee = calculate_fee(amount, fee_bps)?;
    let platform_fee = match treasury {
        Some(_) => calculate_fee(amount, platform_fee_bps)?,
        None => 0,
    };
    let net_amount = amount
        .checked_sub(protocol_fee)
        .ok_or(ContractError::AmountCalculationOverflow)?
        .checked_sub(platform_fee)
        .ok_or(ContractError::AmountCalculationOverflow)?;
    let (payee_net, buyer_net) = split_net(net_amount, split_bps)?;

    payout(env, token_addr, payee, payee_net);
    payout(env, token_addr, buyer, buyer_net);
    crate::storage::add_accumulated_fee(env, token_addr, protocol_fee)?;
    if let Some(treasury_addr) = treasury {
        payout(env, token_addr, treasury_addr, platform_fee);
    }
    Ok(())
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferInstruction {
    pub recipient: Address,
    pub amount: i128,
}

/// Computes the transfer allocations for a resolved dispute.
///
/// # Fee Basis Alignment (#1250)
///
/// `finalize_dispute` computes protocol fees on the **full funded principal**
/// (`escrow.amount + arbitration_fee + resolver_fee`), reconstructing it from
/// the already-reduced `escrow.amount`. This function mirrors that basis so
/// that simulation outputs match on-chain settlement exactly:
///
///   `fee_basis` = `escrow.amount` + `arbitration_fee` + `resolver_fee`
///   `protocol_fee` = `fee_basis` * `escrow.fee_bps` / `10_000`
///   `net` = `escrow.amount` - `platform_fee` - `protocol_fee`
///
/// The invariant `net + protocol_fee + arbitration_fee + resolver_fee + platform_fee
/// == fee_basis` holds by construction.
pub fn calculate_dispute_allocations(
    env: &Env,
    escrow: &Escrow,
    resolution: &ResolutionType,
    arbitration_fee: i128,
    resolver_fee: i128,
    fee_collector: &Address,
) -> Result<Vec<TransferInstruction>, ContractError> {
    if escrow.amount < 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Reconstruct the full funded principal — matches finalize_dispute's fee_basis.
    let fee_basis = escrow
        .amount
        .checked_add(arbitration_fee)
        .ok_or(ContractError::AmountCalculationOverflow)?
        .checked_add(resolver_fee)
        .ok_or(ContractError::AmountCalculationOverflow)?;

    let (protocol_fee, _) = calculate_protocol_fee(fee_basis, escrow.fee_bps)?;

    if protocol_fee > escrow.amount {
        return Err(ContractError::InsufficientBalance);
    }

    let net_amount = escrow
        .amount
        .checked_sub(protocol_fee)
        .ok_or(ContractError::InsufficientBalance)?;

    if net_amount < 0 {
        return Err(ContractError::InsufficientBalance);
    }

    let payee = escrow
        .payees
        .get(0)
        .ok_or(ContractError::PayeeIndexOutOfBounds)?
        .address
        .clone();

    let mut transfers = Vec::new(env);
    match resolution {
        ResolutionType::Release => transfers.push_back(TransferInstruction {
            recipient: payee,
            amount: net_amount,
        }),
        ResolutionType::Refund => transfers.push_back(TransferInstruction {
            recipient: escrow
                .buyer
                .clone()
                .ok_or(ContractError::EscrowHasNoBuyer)?,
            amount: net_amount,
        }),
        ResolutionType::Split(split_bps) => {
            // Two net payouts: payee_net + buyer_net == net_amount.
            let (payee_net, buyer_net) = split_net(net_amount, *split_bps)?;
            let buyer = escrow
                .buyer
                .clone()
                .ok_or(ContractError::EscrowHasNoBuyer)?;
            if payee_net > 0 {
                transfers.push_back(TransferInstruction {
                    recipient: payee,
                    amount: payee_net,
                });
            }
            if buyer_net > 0 {
                transfers.push_back(TransferInstruction {
                    recipient: buyer,
                    amount: buyer_net,
                });
            }
        }
    }

    if protocol_fee > 0 {
        transfers.push_back(TransferInstruction {
            recipient: fee_collector.clone(),
            amount: protocol_fee,
        });
    }

    Ok(transfers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env};

    /// Minimal SEP-41-shaped token that fails if a zero-value transfer is
    /// attempted, so the test below can prove `payout` short-circuits.
    #[contract]
    pub struct ZeroRejectingToken;

    #[contractimpl]
    impl ZeroRejectingToken {
        pub fn transfer(_env: Env, _from: Address, _to: Address, amount: i128) {
            assert!(amount > 0, "zero-value transfer attempted");
        }
    }

    /// Issue #1064: a zero fee (or an all-fee/zero-net amount) must not issue a
    /// zero-value token transfer.
    #[test]
    fn transfer_with_protocol_fee_skips_zero_value_transfers() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(crate::Escrow, ());
        let token = env.register(ZeroRejectingToken, ());
        let recipient = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        // amount = 1 at 300 bps => fee = 0, net = 1. The zero fee transfer is
        // skipped; only the positive net transfer reaches the token.
        let (fee, net) = env
            .as_contract(&contract_id, || {
                transfer_with_protocol_fee(&env, &token, &recipient, &fee_collector, 1, 300)
            })
            .unwrap();
        assert_eq!((fee, net), (0, 1));

        // amount = 0 => fee = 0 and net = 0. No token transfer is attempted,
        // so the zero-rejecting token is never called.
        let (fee, net) = env
            .as_contract(&contract_id, || {
                transfer_with_protocol_fee(&env, &token, &recipient, &fee_collector, 0, 300)
            })
            .unwrap();
        assert_eq!((fee, net), (0, 0));
    }

    #[test]
    fn calculate_dispute_allocations_returns_insufficient_balance_when_fees_exceed_amount() {
        let env = Env::default();
        let payee = Address::generate(&env);
        let buyer = Address::generate(&env);
        let resolver = Address::generate(&env);
        let token = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        let mut payees = Vec::new(&env);
        payees.push_back(crate::types::Payee {
            address: payee,
            bps: BASIS_POINTS,
        });

        let escrow = Escrow {
            payees: crate::types::BoundedPayees { inner: payees },
            buyer: Some(buyer),
            resolvers: crate::types::ResolverSet::Single(resolver),
            token,
            amount: 100,
            fee_bps: 300,
            resolver_fee_bps: 0,
            platform_fee_bps: 0,
            arbitration_fee_bps: 0,
            shipping_window: 100,
            packed_timestamps: 0,
            shipped_at: 0,
            delivered_at: None,
            tracking_id: None,
            state: crate::types::EscrowState::PendingFinalization,
            notes: soroban_sdk::String::from_str(&env, ""),
            expires_at: None,
            grace_period: 0,
        };

        // fee_basis = 100 + 100_000 = 100_100; protocol_fee = 3_003 > escrow.amount (100)
        let res = calculate_dispute_allocations(
            &env,
            &escrow,
            &ResolutionType::Release,
            100_000,
            0,
            &fee_collector,
        );
        assert_eq!(res, Err(ContractError::InsufficientBalance));
    }
}
