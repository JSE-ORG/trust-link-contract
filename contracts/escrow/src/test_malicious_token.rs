#![cfg(test)]
//! Re-entrancy / malicious-token defence suite (issue #402).
//!
//! These tests register the escrow against [`MaliciousToken`] — an adversarial
//! SEP-41 token that re-enters, always fails, or burns the CPU budget while the
//! escrow is mid-execution — and assert that in every case the escrow's
//! accounting is left exactly as it was. Each adversarial transfer must abort
//! the whole call atomically; nothing may leak to sellers, fee collectors, or
//! buyers, and no escrow may advance to a state it did not legitimately reach.

use crate::malicious_token::{Attack, MaliciousToken, MaliciousTokenClient};
use crate::test_helpers::setup_contract;
use crate::{EscrowState, Payee};
use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString, Vec};

const AMOUNT: i128 = 1_000;

struct Fixture {
    env: Env,
    contract_id: Address,
    client: crate::EscrowClient<'static>,
    mclient: MaliciousTokenClient<'static>,
    seller: Address,
    buyer: Address,
    fee_collector: Address,
    id: u64,
}

/// Create an initialized escrow whose token is a freshly-minted malicious token,
/// with a single created (not yet funded) escrow paying the seller in full.
fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();

    let (contract_id, client, _admin, fee_collector) = setup_contract(&env);

    let mtoken = env.register(MaliciousToken, ());
    let mclient = MaliciousTokenClient::new(&env, &mtoken);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    // Start benign so funding works; individual tests arm the attack later.
    mclient.set_attack(&Attack::None);
    mclient.mint(&buyer, &AMOUNT);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let id = client.create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &mtoken,
        &AMOUNT,
        &0_u32,
        &0_u32,
        &3_600_u64,
    );

    Fixture {
        env,
        contract_id,
        client,
        mclient,
        seller,
        buyer,
        fee_collector,
        id,
    }
}

/// Drive a created escrow to the `Shipped` state with funds held by the
/// contract, leaving the attack disabled.
fn fund_and_ship(f: &Fixture) {
    f.mclient.set_attack(&Attack::None);
    f.client.fund_escrow(&f.id, &f.buyer);
    f.client.mark_shipped(
        &f.seller,
        &f.id,
        &SorobanString::from_str(&f.env, "TRACK-402"),
    );
    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
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

    let result = f.client.try_confirm_delivery(&f.buyer, &f.id);
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

    let result = f.client.try_confirm_delivery(&f.buyer, &f.id);
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

    let result = f.client.try_confirm_delivery(&f.buyer, &f.id);
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
    // guaranteed to exceed it and abort the invocation.
    f.env
        .cost_estimate()
        .budget()
        .reset_limits(50_000_000, 20_000_000);

    let result = f.client.try_confirm_delivery(&f.buyer, &f.id);
    assert!(
        result.is_err(),
        "budget-exhausting token must abort the call"
    );

    // Restore an unlimited budget so the post-conditions can be queried.
    f.env.cost_estimate().budget().reset_unlimited();

    assert_eq!(f.mclient.balance(&f.contract_id), AMOUNT);
    assert_eq!(f.mclient.balance(&f.seller), 0);
    assert_ne!(f.client.get_escrow(&f.id).state, EscrowState::Completed);
}
