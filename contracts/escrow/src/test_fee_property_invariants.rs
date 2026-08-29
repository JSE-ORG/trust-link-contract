#![cfg(test)]

//! Property-style coverage for `calculate_fee`/`calculate_protocol_fee`.
//!
//! `test_fee_calculation_accuracy.rs` already checks a fixed table of
//! fee_bps values; this module adds the two invariants the split-div
//! implementation (helpers/payout.rs) is supposed to hold for any
//! amount/fee_bps pair *within the function's basis-points domain*
//! (`0 <= fee_bps <= BASIS_POINTS`, i.e. 0-100%, which is what every call
//! site validates before reaching `calculate_fee` — see
//! `validate_escrow_fee_bps`/`validate_resolver_fee_bps` and the combined-fee
//! cap in `lib.rs`):
//!
//! - `fee + net == amount` (no stroop is lost or created), and
//! - the fee never exceeds `amount` and rounds toward zero (floor).
//!
//! It exercises the boundary amounts called out in issue #822
//! (`MAX_ESCROW_AMOUNT`, `0`, `1`) plus a deterministic pseudo-random sweep
//! standing in for a proptest generator, without adding a new dev-dependency.
//!
//! A separate group of tests feeds `fee_bps` values outside that domain
//! (up to `u32::MAX`, i.e. "fees" over 100%) — nothing in the crate ever
//! constructs such a value, but `calculate_fee`/`calculate_protocol_fee` are
//! `pub fn`, so any caller could. Those tests only assert the functions
//! never panic, matching `fuzz/fuzz_targets/fee_calculation.rs`'s existing
//! policy of not enforcing bounds-invariants past `BASIS_POINTS` (see the
//! comment on the u32::MAX case there).

use crate::helpers::payout::{calculate_fee, calculate_protocol_fee};
use crate::{BASIS_POINTS, MAX_ESCROW_AMOUNT};

fn assert_invariants(amount: i128, fee_bps: u32) {
    assert!(
        fee_bps <= BASIS_POINTS,
        "test bug: assert_invariants is only valid within the basis-points domain"
    );

    let fee = calculate_fee(amount, fee_bps).expect("calculate_fee should not overflow");
    let (fee2, net) = calculate_protocol_fee(amount, fee_bps)
        .expect("calculate_protocol_fee should not overflow");

    assert_eq!(
        fee, fee2,
        "calculate_fee/calculate_protocol_fee disagree on fee"
    );
    assert_eq!(
        fee + net,
        amount,
        "fee + net must equal amount (amount={amount}, fee_bps={fee_bps}, fee={fee}, net={net})"
    );
    assert!(fee >= 0, "fee must be non-negative");
    assert!(net >= 0, "net must be non-negative");
    assert!(fee <= amount, "fee must not exceed amount");

    // Floor rounding: verify against amount * fee_bps / BASIS_POINTS computed
    // independently of the split-div implementation. Only checked when the
    // straightforward product fits in i128 — for larger amount/fee_bps
    // combinations the split-div trick exists precisely to avoid this
    // overflow, so there is no overflow-free independent formula to compare
    // against; the fee+net==amount and bounds checks above already cover
    // those cases.
    if let Some(exact_numerator) = amount.checked_mul(fee_bps as i128) {
        let exact_floor = exact_numerator / (BASIS_POINTS as i128);
        assert_eq!(
            fee, exact_floor,
            "fee must equal floor(amount * fee_bps / BASIS_POINTS)"
        );
    }
}

#[test]
fn zero_amount_yields_zero_fee_for_any_bps() {
    for fee_bps in [0_u32, 1, 300, 500, 1_000, BASIS_POINTS] {
        assert_invariants(0, fee_bps);
    }
}

#[test]
fn minimum_amount_one_stroop() {
    for fee_bps in [0_u32, 1, 50, 300, 500, 1_000, 9_999, BASIS_POINTS] {
        assert_invariants(1, fee_bps);
    }
}

#[test]
fn max_escrow_amount_boundary() {
    for fee_bps in [0_u32, 1, 50, 300, 500, 1_000, 9_999, BASIS_POINTS] {
        assert_invariants(MAX_ESCROW_AMOUNT, fee_bps);
    }
}

#[test]
fn negative_amount_is_rejected_not_panicking() {
    assert!(calculate_fee(-1, 300).is_err());
    assert!(calculate_protocol_fee(-1, 300).is_err());
}

/// `fee_bps` beyond `BASIS_POINTS` (over 100%) is outside every call site's
/// validated range, but the functions are public and take a bare `u32`, so
/// nothing stops a caller from passing one. They must not panic — returning
/// `Err` (as here, once the split-div product itself overflows i128) or an
/// `Ok` with an out-of-domain `fee > amount` are both acceptable; only a
/// panic would be a bug.
#[test]
fn fee_bps_beyond_basis_points_does_not_panic() {
    for fee_bps in [BASIS_POINTS + 1, 50_000, u32::MAX] {
        for amount in [0_i128, 1, MAX_ESCROW_AMOUNT] {
            let _ = calculate_fee(amount, fee_bps);
            let _ = calculate_protocol_fee(amount, fee_bps);
        }
    }
}

/// Small deterministic LCG so the sweep below is reproducible across runs
/// (a fixed seed with no external randomness/proptest dependency).
struct Lcg(u64);

impl Lcg {
    fn next_u64(&mut self) -> u64 {
        // Constants from Numerical Recipes.
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    fn amount(&mut self) -> i128 {
        let raw = (self.next_u64() as u128) | ((self.next_u64() as u128) << 64);
        (raw % (MAX_ESCROW_AMOUNT as u128 + 1)) as i128
    }

    /// Within the valid basis-points domain (0-100%), matching what every
    /// caller in the contract validates before reaching `calculate_fee`.
    fn fee_bps(&mut self) -> u32 {
        (self.next_u64() % (BASIS_POINTS as u64 + 1)) as u32
    }
}

#[test]
fn random_amounts_and_fee_bps_hold_invariants() {
    let mut rng = Lcg(0x5EED_1234_F00D_BA5E);
    for _ in 0..10_000 {
        let amount = rng.amount();
        let fee_bps = rng.fee_bps();
        assert_invariants(amount, fee_bps);
    }
}
