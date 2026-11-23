#![cfg(test)]
//! Re-entrancy / malicious-token defence suite (issue #402).
//!
//! These tests register the escrow against [`MaliciousToken`] — an adversarial
//! SEP-41 token that re-enters, always fails, blacklists a recipient, or burns
//! the CPU budget while the escrow is mid-execution — and assert that in every
//! case the escrow's accounting is left exactly as it was. Each adversarial
//! transfer must abort the whole call atomically; nothing may leak to sellers,
//! fee collectors, or buyers, and no escrow may advance to a state it did not
//! legitimately reach.

use crate::malicious_token::{Attack, MaliciousToken, MaliciousTokenClient};
use crate::{Escrow, EscrowState, Payee};
extern crate alloc;
extern crate std;
use alloc::boxed::Box;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env, IntoVal, String as SorobanString, Vec,
};

const AMOUNT: i128 = 1_000;

struct Fixture {
    contract_id: Address,
    client: crate::EscrowClient<'static>,
    mclient: MaliciousTokenClient<'static>,
    #[allow(dead_code)]
    admin: Address,
    seller: Address,
    buyer: Address,
    fee_collector: Address,
    id: u64,
    env: &'static mut Env,
}

/// Create an initialized escrow whose token is a freshly-minted malicious token,
/// with a single created (not yet funded) escrow paying the seller in full.
fn setup() -> Fixture {
    let env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = crate::EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mtoken = env.register(MaliciousToken, ());
    let mclient = MaliciousTokenClient::new(env, &mtoken);

    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);

    // Start benign so funding works; individual tests arm the attack later.
    mclient.set_attack(&Attack::None);
    mclient.mint(&buyer, &AMOUNT);

    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(env);
    let id = client.create_escrow(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &mtoken,
        &AMOUNT,
        &0_u32,
        &0_u32,
        &3_600_u64,
        &None::<SorobanString>,
    );

    Fixture {
        contract_id,
        client,
        mclient,
        admin,
        seller,
        buyer,
        fee_collector,
        id,
        env,
    }
}

/// Funds a created escrow (leaving it `Funded`, not yet shipped) with the
/// attack disabled.
#[allow(dead_code)]
fn fund_only(f: &Fixture) {
    f.mclient.set_attack(&Attack::None);
    f.client.fund_escrow(&f.id, &f.buyer);
    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
}

/// Drive a created escrow to the `Shipped` state with funds held by the
/// attack disabled, then advance past the buyer's dispute window so
/// `confirm_delivery` actually reaches the payout transfer where the
/// configured `Attack` fires.
fn fund_and_ship(f: &Fixture) {
    f.mclient.set_attack(&Attack::None);
    f.client.fund_escrow(&f.id, &f.buyer);
    f.client.mark_shipped(
        &f.seller,
        &f.id,
        &SorobanString::from_str(f.env, "TRACK-402"),
    );
    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);

    // `DISPUTE_WINDOW` is 172_800s; confirm_delivery is gated on it having
    // elapsed. Without this the adversarial tests below never reach payout.
    let funded_at = f.client.get_escrow(&f.id).funded_at();
    f.env.ledger().set_timestamp(funded_at + 172_801);
}

// 1. Re-entrancy during funding must revert and move no balances.
#[test]
fn reentrancy_during_fund_is_blocked() {
    let f = setup();

    f.mclient.set_reentry(&f.contract_id, &f.buyer, &f.id);
    f.mclient.set_attack(&Attack::ReenterConfirm);

    let result = f.client.try_fund_escrow(&f.id, &f.buyer);
    assert!(result.is_err(), "re-entrant fund_escrow must revert");

    // Accounting unaffected: still Pending, buyer keeps every token.
    assert_eq!(f.client.get_escrow(&f.id).state, EscrowState::Pending);
    assert_eq!(f.mclient.balance(&f.buyer), AMOUNT);
    assert_eq!(f.mclient.balance(&f.contract_id), 0);
}

// 2. Re-entrancy attempting to fund twice in one call must revert.
#[test]
fn reentrancy_attempting_double_fund_is_blocked() {
    let f = setup();

    f.mclient.set_reentry(&f.contract_id, &f.buyer, &f.id);
    f.mclient.set_attack(&Attack::ReenterFund);

    let result = f.client.try_fund_escrow(&f.id, &f.buyer);
    assert!(result.is_err(), "re-entrant double fund must revert");

    assert_eq!(f.client.get_escrow(&f.id).state, EscrowState::Pending);
    assert_eq!(f.mclient.balance(&f.buyer), AMOUNT);
    assert_eq!(f.mclient.balance(&f.contract_id), 0);
}

// 3. Re-entrancy during payout must not allow a double release.
#[test]
fn reentrancy_during_payout_cannot_double_release() {
    let f = setup();
    fund_and_ship(&f);

    f.mclient.set_reentry(&f.contract_id, &f.buyer, &f.id);
    f.mclient.set_attack(&Attack::ReenterConfirm);

    let result = f.client.try_confirm_delivery(&f.buyer, &f.id, &false);
    assert!(result.is_err(), "re-entrant payout must revert");

    // No double spend: funds stay escrowed, nobody is paid, not Completed.
    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
    assert_eq!(f.mclient.balance(&f.seller), 0);
    assert_eq!(f.mclient.balance(&f.fee_collector), 0);
    assert_ne!(f.client.get_escrow(&f.id).state, EscrowState::Completed);
}

// 4. Re-entrancy attempting to cancel mid-payout must revert with no leak.
#[test]
fn reentrancy_attempting_cancel_during_payout_is_blocked() {
    let f = setup();
    fund_and_ship(&f);

    f.mclient.set_reentry(&f.contract_id, &f.buyer, &f.id);
    f.mclient.set_attack(&Attack::ReenterCancel);

    let result = f.client.try_confirm_delivery(&f.buyer, &f.id, &false);
    assert!(result.is_err(), "re-entrant cancel must revert");

    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
    assert_eq!(f.mclient.balance(&f.seller), 0);
    assert_eq!(f.mclient.balance(&f.buyer), 0);
}

// 5. An always-failing token cannot fund an escrow.
#[test]
fn always_failing_token_cannot_fund() {
    let f = setup();

    f.mclient.set_attack(&Attack::Fail);

    let result = f.client.try_fund_escrow(&f.id, &f.buyer);
    assert!(
        result.is_err(),
        "funding through a failing token must revert"
    );

    assert_eq!(f.client.get_escrow(&f.id).state, EscrowState::Pending);
    assert_eq!(f.mclient.balance(&f.buyer), AMOUNT);
    assert_eq!(f.mclient.balance(&f.contract_id), 0);
}

// 6. An always-failing token on payout leaves the funds escrowed.
#[test]
fn always_failing_token_on_payout_preserves_escrowed_funds() {
    let f = setup();
    fund_and_ship(&f);

    f.mclient.set_attack(&Attack::Fail);

    let result = f.client.try_confirm_delivery(&f.buyer, &f.id, &false);
    assert!(
        result.is_err(),
        "payout through a failing token must revert"
    );

    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
    assert_eq!(f.mclient.balance(&f.seller), 0);
    assert_eq!(f.mclient.balance(&f.fee_collector), 0);
    assert_ne!(f.client.get_escrow(&f.id).state, EscrowState::Completed);
}

// 7. A budget-exhausting ("infinite-gas") token aborts the call cleanly.
#[test]
fn budget_exhausting_token_reverts_without_side_effects() {
    let f = setup();
    fund_and_ship(&f);

    f.mclient.set_attack(&Attack::BurnBudget);

    // Constrain the CPU/memory budget so the malicious metered loop is
    // guaranteed to exceed it and abort the invocation. Budget exhaustion
    // escalates to a host trap (not a catchable `ContractError`), so the trap
    // is caught at the Rust level to verify the rollback.
    f.env
        .cost_estimate()
        .budget()
        .reset_limits(50_000_000, 20_000_000);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.client.try_confirm_delivery(&f.buyer, &f.id, &false)
    }));
    assert!(
        result.is_err(),
        "budget-exhausting token must abort the call"
    );

    // Restore an unlimited budget so the balances can be queried.
    f.env.cost_estimate().budget().reset_unlimited();

    // In production a budget trap aborts the whole transaction, so nothing is
    // committed. The test host unwinds the panic without a transaction
    // boundary, so the escrow's partial write (Completed, set before the
    // transfer per CEI) is still visible here; the safety property we can
    // assert at this level is that no funds moved.
    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
    assert_eq!(f.mclient.balance(&f.seller), 0);
}

// 8. Re-entrancy during `mutual_cancel` must revert atomically: the CEI
// ordering flips the escrow to `Canceled` before the payout transfer, so a
// token that re-enters `mutual_cancel` mid-transfer must be rejected by the
// host's re-entrancy guard, leaving the escrow `Funded` and every balance
// untouched (issue #954).
#[test]
fn reentrancy_during_mutual_cancel_is_blocked() {
    // `mutual_cancel` requires a buyer, unlike the shared `setup()` fixture
    // (which creates a buyer-less escrow), so this test builds its own.
    let env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let contract_id = env.register(Escrow, ());
    let client = crate::EscrowClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mtoken = env.register(MaliciousToken, ());
    let mclient = MaliciousTokenClient::new(env, &mtoken);

    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);

    mclient.set_attack(&Attack::None);
    mclient.mint(&buyer, &AMOUNT);

    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(env);
    let id = client.create_escrow(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &mtoken,
        &AMOUNT,
        &0_u32,
        &0_u32,
        &3_600_u64,
        &None::<SorobanString>,
    );

    client.fund_escrow(&id, &buyer);
    assert_eq!(mclient.balance(&contract_id), AMOUNT);

    mclient.set_reentry(&contract_id, &buyer, &id);
    mclient.set_attack(&Attack::ReenterMutualCancel);

    let result = client.try_mutual_cancel(&id);
    assert!(result.is_err(), "re-entrant mutual_cancel must revert");

    // Accounting unaffected: still Funded, funds stay escrowed, nobody paid.
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);
    assert_eq!(mclient.balance(&contract_id), AMOUNT);
    assert_eq!(mclient.balance(&buyer), 0);
    assert_eq!(mclient.balance(&seller), 0);
}

// 9. Positive control: with a benign token and the dispute window elapsed,
// `confirm_delivery` reaches payout and completes. This guards the adversarial
// tests above against silently regressing into vacuous passes.
#[test]
fn benign_token_confirm_delivery_completes_after_dispute_window() {
    let f = setup();
    fund_and_ship(&f);

    f.mclient.set_attack(&Attack::None);
    f.client.confirm_delivery(&f.buyer, &f.id, &false);

    assert_eq!(f.client.get_escrow(&f.id).state, EscrowState::Completed);
    assert_eq!(f.mclient.balance(&f.contract_id), 0);
    assert_eq!(f.mclient.balance(&f.seller), AMOUNT);
}

// 10. A token that blacklists only the seller lets funding through (the token
// is transferred to the contract, not the seller) but aborts the payout.
#[test]
fn recipient_blocking_token_allows_funding_but_blocks_payout() {
    let f = setup();

    f.mclient.set_blocked_recipient(&f.seller);
    f.mclient.set_attack(&Attack::FailForRecipient);

    f.client.fund_escrow(&f.id, &f.buyer);
    assert_eq!(f.client.get_escrow(&f.id).state, EscrowState::Funded);
    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);

    f.client.mark_shipped(
        &f.seller,
        &f.id,
        &SorobanString::from_str(f.env, "TRACK-402"),
    );
    let funded_at = f.client.get_escrow(&f.id).funded_at();
    f.env.ledger().set_timestamp(funded_at + 172_801);

    let result = f.client.try_confirm_delivery(&f.buyer, &f.id, &false);
    assert!(result.is_err(), "payout to a blocked recipient must revert");

    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
    assert_eq!(f.mclient.balance(&f.seller), 0);
    assert_ne!(f.client.get_escrow(&f.id).state, EscrowState::Completed);
}

// 11. A recipient-blocking token aborts a buyer refund the same way, leaving
// the escrow Funded and the funds escrowed.
#[test]
fn recipient_blocking_token_blocks_buyer_refund() {
    let f = setup();
    f.mclient.set_attack(&Attack::None);
    f.client.fund_escrow(&f.id, &f.buyer);

    f.mclient.set_blocked_recipient(&f.buyer);
    f.mclient.set_attack(&Attack::FailForRecipient);

    let result = f.client.try_cancel_escrow(&f.buyer, &f.id);
    assert!(result.is_err(), "refund to a blocked buyer must revert");

    assert_eq!(f.client.get_escrow(&f.id).state, EscrowState::Funded);
    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
    assert_eq!(f.mclient.balance(&f.buyer), 0);
}
