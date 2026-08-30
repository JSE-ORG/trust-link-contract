#![cfg(test)]

use crate::{Escrow, EscrowClient, Payee, ResolutionType};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, IntoVal, String as SorobanString, Symbol, Vec,
};

fn setup(env: &Env) -> (Address, Address, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let fee_collector = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    (admin, seller, buyer, resolver, fee_collector, token)
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    token::Client::new(env, token).balance(who)
}

#[test]
fn test_arbitration_fee_deduction_on_resolve_release() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let arb_fee_bps = 500_u32; // 5% of 1000 = 50
    client.initialize(&admin, &fee_collector, &arb_fee_bps);

    let amount = 1000_i128;
    let fee_bps = 200; // 2%

    let mut payees_4 = Vec::new(&env);
    payees_4.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_4_val = payees_4.into_val(&env);
    let id = client.create_escrow_8(
        &payees_4_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &fee_bps,
        &3600_u64,
    );

    mint(&env, &token, &buyer, amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-ARB-1"));

    // Advance time to allow dispute
    env.ledger().set_timestamp(env.ledger().timestamp() + 10);

    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "reason"),
        &SorobanString::from_str(&env, "desc"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );

    // Initial total arbitration fees should be 0
    assert_eq!(client.get_total_arbitration_fees(&token), 0);
    assert_eq!(client.get_arbitration_fee(), arb_fee_bps);

    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &id);

    // Calculation:
    // 1. amount = 1000
    // 2. arbitration_fee = 50 (5% of 1000)
    // 3. remaining = 1000 - 50 = 950
    // 4. protocol_fee (2% of 950) = 950 * 200 / 10000 = 19
    // 5. final_net = 950 - 19 = 931

    assert_eq!(balance(&env, &token, &seller), 931);

    // arbitration fee (50) and protocol fee (19) go to fee_collector
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(balance(&env, &token, &fee_collector), 69);

    // Dedicated tracking variable should be updated
    assert_eq!(client.get_total_arbitration_fees(&token), 50);
}

#[test]
fn test_arbitration_fee_deduction_on_resolve_refund() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let arb_fee_bps = 500_u32; // 5% of 1000 = 50
    client.initialize(&admin, &fee_collector, &arb_fee_bps);

    let amount = 1000_i128;
    let fee_bps = 300; // 3%

    let mut payees_3 = Vec::new(&env);
    payees_3.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_3_val = payees_3.into_val(&env);
    let id = client.create_escrow_8(
        &payees_3_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &fee_bps,
        &3600_u64,
    );

    mint(&env, &token, &buyer, amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-ARB-2"));

    env.ledger().set_timestamp(env.ledger().timestamp() + 10);
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "reason"),
        &SorobanString::from_str(&env, "desc"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );

    client.resolve_dispute(&resolver, &id, &ResolutionType::Refund);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &id);

    // Calculation:
    // 1. amount = 1000
    // 2. arbitration_fee = 50 (5% of 1000)
    // 3. remaining = 1000 - 50 = 950
    // 4. protocol_fee (3% of 950) = 950 * 300 / 10000 = 28 (floor)
    // 5. final_net = 950 - 28 = 922

    assert_eq!(balance(&env, &token, &buyer), 922);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(balance(&env, &token, &fee_collector), 78);
    assert_eq!(client.get_total_arbitration_fees(&token), 50);
}

/// A dispute that is resolved, appealed, and resolved again must be charged
/// the arbitration fee only once — the appeal round reuses the fee already
/// deducted instead of taking a second cut out of the escrow.
#[test]
fn test_arbitration_fee_charged_once_across_appeal() {
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let arb_fee_bps = 500_u32; // 5% of 1000 = 50
    client.initialize(&admin, &fee_collector, &arb_fee_bps);

    let amount = 1000_i128;
    let fee_bps = 0_u32; // no protocol fee, keep the arithmetic obvious

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &fee_bps,
        &3600_u64,
    );

    mint(&env, &token, &buyer, amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &SorobanString::from_str(&env, "TRACK-ARB-APPEAL"),
    );

    env.ledger().set_timestamp(env.ledger().timestamp() + 10);
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "reason"),
        &SorobanString::from_str(&env, "desc"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );

    // First resolution: arbitration fee of 50 is deducted and forwarded.
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    assert_eq!(client.get_total_arbitration_fees(&token), 50);
    assert_eq!(balance(&env, &token, &fee_collector), 50);
    assert_eq!(client.get_dispute(&id).unwrap().arbitration_fee, 50);

    // Appeal within the appeal window, then resolve again.
    client.appeal_dispute(&buyer, &id);
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);

    // No second arbitration fee: totals and the fee collector are unchanged.
    assert_eq!(client.get_total_arbitration_fees(&token), 50);
    assert_eq!(balance(&env, &token, &fee_collector), 50);
    assert_eq!(client.get_dispute(&id).unwrap().arbitration_fee, 50);

    // Finalize once the appeal window has elapsed.
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &id);

    // Seller receives amount minus the single arbitration fee; nothing stranded.
    assert_eq!(balance(&env, &token, &seller), 950);
    assert_eq!(balance(&env, &token, &fee_collector), 50);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(client.get_total_arbitration_fees(&token), 50);
}

#[test]
fn test_set_and_get_arbitration_fee() {
    let env = Env::default();
    let (admin, _seller, _buyer, _resolver, fee_collector, _token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &50_u32);
    assert_eq!(client.get_arbitration_fee(), 50);

    client.set_arbitration_fee(&admin, &150_u32);
    assert_eq!(client.get_arbitration_fee(), 150);
}

#[test]
fn test_resolution_transition_min_amount_max_fees_does_not_underflow() {
    // Issue #819: at the minimum escrow amount with arbitration_fee_bps and
    // resolver_fee_bps both at their individually validated maximums
    // (MAX_ARBITRATION_FEE_BPS=500, MAX_ESCROW_FEE_BPS=300 for resolver fee),
    // execute_resolution_transition must resolve cleanly rather than
    // underflowing. Floor rounding means both fees round to 0 at amount=1,
    // so the seller receives the full amount and no fees are charged.
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let arb_fee_bps = 500_u32; // MAX_ARBITRATION_FEE_BPS
    client.initialize(&admin, &fee_collector, &arb_fee_bps);

    let amount = 1_i128; // MIN_ESCROW_AMOUNT
    let resolver_fee_bps = 300_u32; // MAX_ESCROW_FEE_BPS cap for resolver fee

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);

    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &0_u32,
        &resolver_fee_bps,
        &3600_u64,
        &None::<SorobanString>,
    );

    mint(&env, &token, &buyer, amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-MIN"));

    env.ledger().set_timestamp(env.ledger().timestamp() + 10);
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "reason"),
        &SorobanString::from_str(&env, "desc"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );

    // Must not panic or underflow when resolving at min amount + max fees.
    client.resolve_dispute(&resolver, &id, &ResolutionType::Release);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &id);

    assert_eq!(balance(&env, &token, &seller), 1);
    assert_eq!(balance(&env, &token, &contract_id), 0);
    assert_eq!(balance(&env, &token, &fee_collector), 0);
    assert_eq!(client.get_total_arbitration_fees(&token), 0);
}

#[test]
fn test_execute_resolution_transition_rejects_fees_exceeding_amount() {
    // Issue #819: execute_resolution_transition relied on checked_sub alone
    // to guard the fee deduction, but checked_sub only fails on i128
    // overflow — a negative result (fees exceeding the amount) is a valid
    // i128 and would be accepted silently. There must be an explicit
    // amount >= arbitration_fee + resolver_fee check before deducting.
    //
    // Every public entry point caps arbitration_fee_bps at
    // MAX_ARBITRATION_FEE_BPS (500) and resolver_fee_bps at
    // MAX_ESCROW_FEE_BPS (300), so combined fees can never exceed the amount
    // through the public API today (300+500 = 800bps = 8% max). This test
    // exercises execute_resolution_transition directly — as
    // test_fee_config.rs does for validate_combined_fees — by writing an
    // out-of-range resolver_fee_bps straight into storage, so the guard
    // itself is pinned independent of what any future entry point allows.
    let env = Env::default();
    let (admin, seller, buyer, resolver, fee_collector, token) = setup(&env);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &500_u32); // arbitration_fee_bps = 5%

    let amount = 100_i128;
    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);

    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<SorobanString>,
    );

    mint(&env, &token, &buyer, amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &SorobanString::from_str(&env, "TRACK-FEE-CAP"),
    );
    env.ledger().set_timestamp(env.ledger().timestamp() + 10);
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "reason"),
        &SorobanString::from_str(&env, "desc"),
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );

    // Bypass every public validation path to force resolver_fee_bps = 100%.
    // arbitration_fee (5% of 100 = 5) + resolver_fee (100% of 100 = 100) = 105 > 100.
    // Direct storage/internal-function access requires the contract's own
    // execution context (see test_ttl.rs's `effective_ttl_extension` for the
    // same pattern).
    let (result, escrow_after, dispute_after) = env.as_contract(&contract_id, || {
        let mut escrow = crate::internal::load_escrow(&env, id).unwrap();
        escrow.resolver_fee_bps = 10_000;
        crate::internal::save_escrow(&env, id, &escrow, None);

        let votes: Vec<crate::ResolverVote> = Vec::new(&env);
        let result = crate::internal::execute_resolution_transition(
            &env,
            id,
            escrow,
            resolver.clone(),
            ResolutionType::Release,
            votes,
        );

        let escrow_after = crate::internal::load_escrow(&env, id).unwrap();
        let dispute_after = crate::internal::load_dispute(&env, id).unwrap();
        (result, escrow_after, dispute_after)
    });

    assert_eq!(result, Err(crate::ContractError::FeeExceedsMax));

    // No partial mutation: escrow/dispute state and balances are untouched.
    assert_eq!(escrow_after.amount, amount);
    assert_eq!(escrow_after.state, crate::EscrowState::Disputed);
    assert_eq!(dispute_after.arbitration_fee, 0);
    assert_eq!(dispute_after.resolver_fee, 0);

    assert_eq!(balance(&env, &token, &contract_id), amount);
    assert_eq!(balance(&env, &token, &resolver), 0);
    assert_eq!(balance(&env, &token, &fee_collector), 0);
}
