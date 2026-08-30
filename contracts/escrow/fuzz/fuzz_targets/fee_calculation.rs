#![no_main]
//! Fuzz target for fee arithmetic invariants.
//!
//! This harness exercises `calculate_fee` and `calculate_protocol_fee` with
//! arbitrary `amount` (i128) and `fee_bps` (u32) inputs to detect panics,
//! arithmetic overflows, or invariant violations without any Soroban host
//! environment — both functions are pure arithmetic and require no Env.

mod common;
use common::Reader;
use libfuzzer_sys::fuzz_target;
use trustlink_escrow::helpers::payout::{calculate_fee, calculate_protocol_fee};

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let amount = r.i128();   // full i128 range, including negatives
    let fee_bps = r.u32();   // full u32 range, including values > MAX_ESCROW_FEE_BPS (300)

    // Only assert invariants for non-negative amounts; negative amounts must
    // return Err (no panic). The functions return Err(InvalidAmount) for
    // amount < 0, which is an expected, non-panicking outcome.
    if amount >= 0 {
        // Feature: codebase-improvements, Property 1: fee non-negativity and boundedness
        // For any non-negative amount and any fee_bps, if calculate_fee returns Ok(fee),
        // then 0 <= fee <= amount.
        // Validates: Requirements 2.4
        if let Ok(fee) = calculate_fee(amount, fee_bps) {
            assert!(fee >= 0, "fee must be non-negative (amount={amount}, fee_bps={fee_bps}, fee={fee})");
            assert!(fee <= amount, "fee must not exceed amount (amount={amount}, fee_bps={fee_bps}, fee={fee})");
        }

        // Feature: codebase-improvements, Property 2: fee decomposition identity
        // For any non-negative amount and any fee_bps, if calculate_protocol_fee returns Ok((fee, net)),
        // then fee + net == amount, fee >= 0, and net >= 0.
        // Validates: Requirements 2.5
        if let Ok((fee, net)) = calculate_protocol_fee(amount, fee_bps) {
            assert_eq!(
                fee + net,
                amount,
                "fee + net must equal amount (amount={amount}, fee_bps={fee_bps}, fee={fee}, net={net})"
            );
            assert!(fee >= 0, "fee must be non-negative (amount={amount}, fee_bps={fee_bps}, fee={fee})");
            assert!(net >= 0, "net must be non-negative (amount={amount}, fee_bps={fee_bps}, net={net})");
        }

        // Zero-fee edge case: when fee_bps == 0, calculate_fee must return Ok(0).
        // Validates: Requirements 2.6
        if fee_bps == 0 {
            assert_eq!(
                calculate_fee(amount, 0),
                Ok(0),
                "fee must be 0 when fee_bps is 0 (amount={amount})"
            );
        }
    }
    // For fee_bps > MAX_ESCROW_FEE_BPS (300) but <= BASIS_POINTS (10_000): the
    // functions accept the value and compute a (larger) fee without panicking.
    // For fee_bps > BASIS_POINTS (over 100%), both functions return Err
    // instead of a fee that could exceed amount. Either way, reaching this
    // point without a panic satisfies Requirements 2.7.
});
