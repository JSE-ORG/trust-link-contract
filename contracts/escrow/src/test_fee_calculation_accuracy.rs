#![cfg(test)]

use crate::helpers::payout::calculate_protocol_fee;
use crate::Payee;
use crate::{ContractError, ResolverSet};

/// Parameterized test that verifies fee calculation is mathematically correct
/// for various fee_bps values: 0, 50, 100, 150, 200, 250, 300.
///
/// Acceptance Criteria:
/// - Each test case verifies vendor payout + fee = original amount (no rounding loss)
/// - 0 bps: fee is exactly 0
/// - 300 bps: fee is exactly 3% of amount
/// - Test includes amounts at minimum (1 stroop) and large values

#[test]
fn test_fee_calculation_0_bps_minimum_amount() {
    // 0 bps = 0% fee
    let amount = 1_i128; // 1 stroop (minimum)
    let fee_bps = 0_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Verify fee is exactly 0
    assert_eq!(fee, 0);
    // Verify net + fee = original amount
    assert_eq!(net + fee, amount);
    // Verify net is the full amount
    assert_eq!(net, 1);
}

#[test]
fn test_fee_calculation_0_bps_large_amount() {
    // 0 bps = 0% fee
    let amount = 1_000_000_000_000_i128; // 1 trillion stroops
    let fee_bps = 0_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Verify fee is exactly 0
    assert_eq!(fee, 0);
    // Verify net + fee = original amount
    assert_eq!(net + fee, amount);
    // Verify net is the full amount
    assert_eq!(net, amount);
}

#[test]
fn test_fee_calculation_50_bps_minimum_amount() {
    // 50 bps = 0.5% fee
    let amount = 1_i128; // 1 stroop (minimum)
    let fee_bps = 50_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // For 1 stroop: fee = 1 * 50 / 10000 = 0 (rounds down)
    assert_eq!(fee, 0);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 1);
}

#[test]
fn test_fee_calculation_50_bps_large_amount() {
    // 50 bps = 0.5% fee
    let amount = 1_000_000_i128; // 1 million stroops
    let fee_bps = 50_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Expected fee: 1,000,000 * 0.005 = 5,000
    assert_eq!(fee, 5_000);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 995_000);
}

#[test]
fn test_fee_calculation_100_bps_minimum_amount() {
    // 100 bps = 1% fee
    let amount = 1_i128; // 1 stroop (minimum)
    let fee_bps = 100_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // For 1 stroop: fee = 1 * 100 / 10000 = 0 (rounds down)
    assert_eq!(fee, 0);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 1);
}

#[test]
fn test_fee_calculation_100_bps_large_amount() {
    // 100 bps = 1% fee
    let amount = 1_000_000_i128; // 1 million stroops
    let fee_bps = 100_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Expected fee: 1,000,000 * 0.01 = 10,000
    assert_eq!(fee, 10_000);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 990_000);
}

#[test]
fn test_fee_calculation_150_bps_minimum_amount() {
    // 150 bps = 1.5% fee
    let amount = 1_i128; // 1 stroop (minimum)
    let fee_bps = 150_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // For 1 stroop: fee = 1 * 150 / 10000 = 0 (rounds down)
    assert_eq!(fee, 0);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 1);
}

#[test]
fn test_fee_calculation_150_bps_large_amount() {
    // 150 bps = 1.5% fee
    let amount = 1_000_000_i128; // 1 million stroops
    let fee_bps = 150_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Expected fee: 1,000,000 * 0.015 = 15,000
    assert_eq!(fee, 15_000);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 985_000);
}

#[test]
fn test_fee_calculation_200_bps_minimum_amount() {
    // 200 bps = 2% fee
    let amount = 1_i128; // 1 stroop (minimum)
    let fee_bps = 200_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // For 1 stroop: fee = 1 * 200 / 10000 = 0 (rounds down)
    assert_eq!(fee, 0);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 1);
}

#[test]
fn test_fee_calculation_200_bps_large_amount() {
    // 200 bps = 2% fee
    let amount = 1_000_000_i128; // 1 million stroops
    let fee_bps = 200_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Expected fee: 1,000,000 * 0.02 = 20,000
    assert_eq!(fee, 20_000);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 980_000);
}

#[test]
fn test_fee_calculation_250_bps_minimum_amount() {
    // 250 bps = 2.5% fee
    let amount = 1_i128; // 1 stroop (minimum)
    let fee_bps = 250_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // For 1 stroop: fee = 1 * 250 / 10000 = 0 (rounds down)
    assert_eq!(fee, 0);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 1);
}

#[test]
fn test_fee_calculation_250_bps_large_amount() {
    // 250 bps = 2.5% fee
    let amount = 1_000_000_i128; // 1 million stroops
    let fee_bps = 250_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Expected fee: 1,000,000 * 0.025 = 25,000
    assert_eq!(fee, 25_000);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 975_000);
}

#[test]
fn test_fee_calculation_300_bps_minimum_amount() {
    // 300 bps = 3% fee (maximum allowed)
    let amount = 1_i128; // 1 stroop (minimum)
    let fee_bps = 300_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // For 1 stroop: fee = 1 * 300 / 10000 = 0 (rounds down)
    assert_eq!(fee, 0);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 1);
}

#[test]
fn test_fee_calculation_300_bps_large_amount() {
    // 300 bps = 3% fee (maximum allowed)
    let amount = 1_000_000_i128; // 1 million stroops
    let fee_bps = 300_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Expected fee: 1,000,000 * 0.03 = 30,000 (exactly 3%)
    assert_eq!(fee, 30_000);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 970_000);
}

#[test]
fn test_fee_calculation_300_bps_exact_percentage() {
    // Verify that 300 bps produces exactly 3% fee
    let amount = 10_000_000_i128; // 10 million stroops
    let fee_bps = 300_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_ok());

    let (fee, net) = result.unwrap();

    // Expected fee: 10,000,000 * 0.03 = 300,000 (exactly 3%)
    assert_eq!(fee, 300_000);
    // Verify net + fee = original amount (no rounding loss)
    assert_eq!(net + fee, amount);
    assert_eq!(net, 9_700_000);
}

#[test]
fn test_fee_calculation_no_rounding_loss_various_amounts() {
    // Test various amounts to ensure no rounding loss
    let test_cases = [
        (1_i128, 0_u32),
        (1_i128, 50_u32),
        (1_i128, 100_u32),
        (1_i128, 150_u32),
        (1_i128, 200_u32),
        (1_i128, 250_u32),
        (1_i128, 300_u32),
        (100_i128, 0_u32),
        (100_i128, 50_u32),
        (100_i128, 100_u32),
        (100_i128, 150_u32),
        (100_i128, 200_u32),
        (100_i128, 250_u32),
        (100_i128, 300_u32),
        (10_000_i128, 0_u32),
        (10_000_i128, 50_u32),
        (10_000_i128, 100_u32),
        (10_000_i128, 150_u32),
        (10_000_i128, 200_u32),
        (10_000_i128, 250_u32),
        (10_000_i128, 300_u32),
        (1_000_000_i128, 0_u32),
        (1_000_000_i128, 50_u32),
        (1_000_000_i128, 100_u32),
        (1_000_000_i128, 150_u32),
        (1_000_000_i128, 200_u32),
        (1_000_000_i128, 250_u32),
        (1_000_000_i128, 300_u32),
        (1_000_000_000_i128, 0_u32),
        (1_000_000_000_i128, 50_u32),
        (1_000_000_000_i128, 100_u32),
        (1_000_000_000_i128, 150_u32),
        (1_000_000_000_i128, 200_u32),
        (1_000_000_000_i128, 250_u32),
        (1_000_000_000_i128, 300_u32),
    ];

    for (amount, fee_bps) in test_cases {
        let result = calculate_protocol_fee(amount, fee_bps);
        assert!(
            result.is_ok(),
            "Failed for amount={}, fee_bps={}",
            amount,
            fee_bps
        );

        let (fee, net) = result.unwrap();

        // Critical: verify no rounding loss (net + fee must equal original amount)
        assert_eq!(
            net + fee,
            amount,
            "Rounding loss detected: amount={}, fee_bps={}, fee={}, net={}, sum={}",
            amount,
            fee_bps,
            fee,
            net,
            net + fee
        );

        // Verify fee is non-negative
        assert!(
            fee >= 0,
            "Fee cannot be negative: amount={}, fee_bps={}, fee={}",
            amount,
            fee_bps,
            fee
        );

        // Verify net is non-negative
        assert!(
            net >= 0,
            "Net cannot be negative: amount={}, fee_bps={}, net={}",
            amount,
            fee_bps,
            net
        );
    }
}

#[test]
fn test_fee_calculation_edge_case_amounts() {
    // Test edge cases with amounts that might cause rounding issues
    let edge_cases = [
        (9_999_i128, 300_u32),     // Just under 10,000
        (10_000_i128, 300_u32),    // Exactly 10,000
        (10_001_i128, 300_u32),    // Just over 10,000
        (99_999_i128, 300_u32),    // Just under 100,000
        (100_000_i128, 300_u32),   // Exactly 100,000
        (100_001_i128, 300_u32),   // Just over 100,000
        (999_999_i128, 300_u32),   // Just under 1,000,000
        (1_000_000_i128, 300_u32), // Exactly 1,000,000
        (1_000_001_i128, 300_u32), // Just over 1,000,000
    ];

    for (amount, fee_bps) in edge_cases {
        let result = calculate_protocol_fee(amount, fee_bps);
        assert!(
            result.is_ok(),
            "Failed for amount={}, fee_bps={}",
            amount,
            fee_bps
        );

        let (fee, net) = result.unwrap();

        // Verify no rounding loss
        assert_eq!(
            net + fee,
            amount,
            "Rounding loss at edge case: amount={}, fee_bps={}, fee={}, net={}",
            amount,
            fee_bps,
            fee,
            net
        );
    }
}

#[test]
fn test_fee_calculation_invalid_amount() {
    // Test that negative amounts return an error
    let amount = -1_i128;
    let fee_bps = 100_u32;

    let result = calculate_protocol_fee(amount, fee_bps);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ContractError::InvalidAmount);
}

// Regression test for issue #201: Verify dispute resolution fee is not discarded
#[test]
fn test_dispute_allocations_include_protocol_fee() {
    use crate::helpers::payout::calculate_dispute_allocations;
    use crate::{EscrowData, EscrowState, ResolutionType};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    let env = Env::default();
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    // Create mock escrow with 1,000,000 stroops and 100 bps (1%) fee
    // ==========================================
    // FIRST TEST TRANSFORMATION
    // ==========================================

    // 1. Setup a single payee vector representing 100% allocation for the seller
    let mut payees_53 = soroban_sdk::Vec::new(&env);
    payees_53.push_back(crate::types::Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let escrow = EscrowData {
        payees: payees_53, // Changed from seller: seller.clone()
        buyer: Some(buyer.clone()),
        resolvers: ResolverSet::Single(resolver.clone()),
        token: token.clone(),
        amount: 1_000_000_i128,
        fee_bps: 100_u32,        // 1%
        resolver_fee_bps: 0_u32, // Added missing field
        state: EscrowState::Disputed,
        shipping_window: 3600,
        funded_at: 0,
        dispute_deadline: 0,
        shipped_at: 0,
        delivered_at: None,
        tracking_id: None,
        notes: None, // Added missing field
    };

    let arbitration_fee = 50_000_i128; // 5% arbitration fee
    let resolution = ResolutionType::Release;

    let result =
        calculate_dispute_allocations(&env, &escrow, &resolution, arbitration_fee, &fee_collector);

    assert!(result.is_ok());
    let transfers = result.unwrap();

    // Should have 2 transfers: net to seller + protocol fee to fee_collector
    assert_eq!(transfers.len(), 2);

    // Verify amounts:
    // Total = 1,000,000
    // Arbitration fee = 50,000
    // Remaining = 950,000
    // Protocol fee (1%) = 9,500
    // Net to seller = 940,500

    let seller_transfer = &transfers.get(0).unwrap();
    assert_eq!(seller_transfer.recipient, seller);
    assert_eq!(seller_transfer.amount, 940_500);

    let fee_transfer = &transfers.get(1).unwrap();
    assert_eq!(fee_transfer.recipient, fee_collector);
    assert_eq!(fee_transfer.amount, 9_500);

    // Verify no funds are lost
    assert_eq!(
        seller_transfer.amount + fee_transfer.amount + arbitration_fee,
        escrow.amount
    );
}

#[test]
fn test_dispute_allocations_zero_fee_no_fee_transfer() {
    use crate::helpers::payout::calculate_dispute_allocations;
    use crate::types::{EscrowState, ResolutionType};
    use crate::EscrowData;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    let env = Env::default();
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    // ==========================================
    // SECOND TEST TRANSFORMATION
    // ==========================================

    let mut payees_52 = soroban_sdk::Vec::new(&env);
    payees_52.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let escrow = EscrowData {
        payees: payees_52,
        buyer: Some(buyer.clone()),
        resolvers: ResolverSet::Single(resolver.clone()),
        token: token.clone(),
        amount: 1_000_000_i128,
        fee_bps: 0_u32,          // 0% fee
        resolver_fee_bps: 0_u32, // Added missing field
        state: EscrowState::Disputed,
        shipping_window: 3600,
        funded_at: 0,
        dispute_deadline: 0,
        shipped_at: 0,
        delivered_at: None,
        tracking_id: None,
        notes: None, // Added missing field
    };

    let arbitration_fee = 50_000_i128;
    let resolution = ResolutionType::Refund;

    let result =
        calculate_dispute_allocations(&env, &escrow, &resolution, arbitration_fee, &fee_collector);

    assert!(result.is_ok());
    let transfers = result.unwrap();

    // With 0% fee, should only have 1 transfer (to buyer)
    assert_eq!(transfers.len(), 1);

    let buyer_transfer = &transfers.get(0).unwrap();
    assert_eq!(buyer_transfer.recipient, buyer);
    assert_eq!(buyer_transfer.amount, 950_000); // 1,000,000 - 50,000 arbitration
}

/// Tests that `calculate_fee` correctly floors sub-stroop remainders when
/// `amount * fee_bps` is not evenly divisible by `BASIS_POINTS` (10_000).
///
/// The algorithm splits the computation to avoid i128 overflow:
///
/// ```text
/// part1 = (amount / 10_000) * fee_bps          -- whole multiple contribution
/// part2 = (amount % 10_000) * fee_bps / 10_000 -- remainder contribution (floored)
/// fee   = part1 + part2
/// net   = amount - fee
/// ```
///
/// Because integer division truncates, any fractional stroop is dropped from
/// `fee` and stays in `net`. The invariant `net + fee == amount` always holds,
/// so no stroop is ever stranded in the contract.
///
/// Cases chosen to expose the rounding boundary:
///
/// | amount | fee_bps | exact fee | floored fee | dropped remainder |
/// |--------|---------|-----------|-------------|-------------------|
/// | 1      | 3       | 0.0003    | 0           | 0.0003 stroop     |
/// | 3_333  | 3       | 0.9999    | 0           | 0.9999 stroop     | ← largest waived fee
/// | 3_334  | 3       | 1.0002    | 1           | 0.0002 stroop     | ← first amount yielding 1 stroop
/// | 9_999  | 3       | 2.9997    | 2           | 0.9997 stroop     | ← issue's stated example
/// | 10_001 | 3       | 3.0003    | 3           | 0.0003 stroop     |
/// | 99_997 | 3       | 29.9991   | 29          | 0.9991 stroop     |
#[test]
fn test_fee_calculation_odd_amounts_sub_stroop_rounding_3bps() {
    // (amount, expected_fee, expected_net)
    // Derived via: fee = floor(amount * 3 / 10_000), net = amount - fee.
    let cases: &[(i128, i128, i128)] = &[
        // 1 * 3 / 10_000 = 0.0003 → fee = 0, all 1 stroop stays with recipient.
        (1, 0, 1),
        // 3_333 * 3 / 10_000 = 0.9999 → fee rounds down to 0 (largest waived fee at 3 bps).
        (3_333, 0, 3_333),
        // 3_334 * 3 / 10_000 = 1.0002 → fee = 1 (the first amount where 3 bps charges 1 stroop).
        (3_334, 1, 3_333),
        // 9_999 * 3 / 10_000 = 2.9997 → fee = 2, 0.9997 sub-stroop dropped.
        // This is the canonical example from the issue description.
        (9_999, 2, 9_997),
        // 10_001 * 3 / 10_000 = 3.0003 → fee = 3, 0.0003 sub-stroop dropped.
        (10_001, 3, 9_998),
        // 99_997 * 3 / 10_000 = 29.9991 → fee = 29, 0.9991 sub-stroop dropped.
        (99_997, 29, 99_968),
    ];

    for &(amount, expected_fee, expected_net) in cases {
        let (fee, net) = calculate_protocol_fee(amount, 3)
            .unwrap_or_else(|e| panic!("calculate_protocol_fee({amount}, 3) failed: {e:?}"));

        assert_eq!(
            fee, expected_fee,
            "fee mismatch for amount={amount} @ 3 bps: \
             expected {expected_fee}, got {fee}"
        );
        assert_eq!(
            net, expected_net,
            "net mismatch for amount={amount} @ 3 bps: \
             expected {expected_net}, got {net}"
        );
        // The core invariant: no stroop is ever stranded in the contract.
        assert_eq!(
            net + fee,
            amount,
            "invariant net+fee==amount violated for amount={amount} @ 3 bps: \
             net={net}, fee={fee}, sum={}",
            net + fee
        );
        // Rounding is always floor: fee must never exceed the exact rational value.
        // Equivalently: fee * 10_000 <= amount * 3.
        assert!(
            fee * 10_000 <= amount * 3,
            "fee {fee} exceeds exact rational value for amount={amount} @ 3 bps"
        );
        // The truncated remainder is strictly less than 1 stroop:
        // fee * 10_000 + 10_000 > amount * 3  (i.e. fee+1 would over-charge)
        assert!(
            fee * 10_000 + 10_000 > amount * 3,
            "fee {fee} is not the correct floor for amount={amount} @ 3 bps"
        );
    }
}
