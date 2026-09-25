use crate::{ContractError, BASIS_POINTS};
use soroban_sdk::{token, Address, Env};

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
/// guard (1_000_000 stroops) in `create_escrow` keeps escrows large enough that
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
        .checked_div(BASIS_POINTS as i128)
        .ok_or(ContractError::FeeCalculationOverflow)?
        .checked_mul(fee_bps as i128)
        .ok_or(ContractError::FeeCalculationOverflow)?;

    let part2 = (amount % BASIS_POINTS as i128)
        .checked_mul(fee_bps as i128)
        .ok_or(ContractError::FeeCalculationOverflow)?
        .checked_div(BASIS_POINTS as i128)
        .ok_or(ContractError::FeeCalculationOverflow)?;

    part1
        .checked_add(part2)
        .ok_or(ContractError::FeeCalculationOverflow)
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
    let token_client = token::Client::new(env, token_addr);
    let contract_addr = env.current_contract_address();

    if net > 0 {
        token_client.transfer(&contract_addr, recipient, &net);
    }

    if fee > 0 {
        token_client.transfer(&contract_addr, fee_collector, &fee);
    }

    Ok((fee, net))
}
