#![cfg(test)]
//! Integration test for the full dispute → vendor-wins resolution flow (#17).
//!
//! Covers: create → fund → ship → raise_dispute → resolve_dispute(Release).
//! After resolution the escrow must be in `Completed`, the seller must
//! receive `amount - arbitration_fee`, the buyer must not be refunded, and
//! the on-chain dispute record must be marked `Resolved`.

use crate::{
    DataKey, DisputeData, DisputeStatus, Escrow, EscrowClient, EscrowData, EscrowState, Payee,
    ResolutionType,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

#[test]
fn full_dispute_release_to_vendor() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    // SAC token used to fund the buyer + receive the seller payout.
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let arbitration_fee: u32 = 50;
    client.initialize(&admin, &fee_collector, &arbitration_fee);

    let amount: i128 = 1_000;
    // fee_bps = 0 isolates the arbitration-fee accounting the issue specifies
    // (a non-zero protocol fee would further reduce the seller's payout).
    let mut payees_23 = Vec::new(&env);
    payees_23.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_23.into_val(&env);
    let escrow_id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_address,
        &amount,
        &0_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    // Fund the buyer and the escrow.
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
    token_admin_client.mint(&buyer, &amount);
    client.fund_escrow(&escrow_id, &buyer);

    // Advance time past shipping window so mark_shipped is permitted.
    env.ledger().set_timestamp(env.ledger().timestamp() + 3601);
    // Seller marks shipped.
    let tracking_id = String::from_str(&env, "TRK-001");
    client.mark_shipped(&seller, &escrow_id, &tracking_id);

    // Buyer raises a dispute.
    let reason = Symbol::new(&env, "non_delivery");
    let description = String::from_str(&env, "Item never arrived");
    let evidence = BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &escrow_id, &reason, &description, &evidence);

    // Sanity: state is now Disputed before resolution.
    let mid: EscrowData = env
        .as_contract(&contract_id, || {
            env.storage().persistent().get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow exists");
    assert_eq!(mid.state, EscrowState::Disputed);

    // Resolver decides in favour of the vendor.
    client.resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release);
    env.ledger().set_timestamp(env.ledger().timestamp() + 86401);
    client.finalize_dispute(&resolver, &escrow_id);

    // ── Post-resolution assertions ─────────────────────────────────────────
    let token_client = token::TokenClient::new(&env, &token_address);

    // Vendor received the net amount (face value minus the arbitration fee).
    assert_eq!(
        token_client.balance(&seller),
        amount - 5,
        "seller should receive amount minus arbitration fee on Release",
    );

    // Buyer received no refund.
    assert_eq!(
        token_client.balance(&buyer),
        0,
        "buyer should not be refunded on a vendor-wins resolution",
    );

    // Escrow state advanced to Completed.
    let after: EscrowData = env
        .as_contract(&contract_id, || {
            env.storage().persistent().get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow exists");
    assert_eq!(after.state, EscrowState::Completed);

    // Dispute record is marked Resolved.
    let dispute: DisputeData = env
        .as_contract(&contract_id, || {
            env.storage().persistent().get(&DataKey::Dispute(escrow_id))
        })
        .expect("dispute exists");
    assert_eq!(dispute.status, DisputeStatus::Resolved);
}

#[test]
fn dispute_resolution_with_zero_resolver_fee_and_appeal() {
    // Covers: execute_resolution_transition when both resolver_fee_bps and
    // arbitration_fee_bps are 0.
    //
    // The `fees_already_charged` guard inside execute_resolution_transition is:
    //
    //   let fees_already_charged = dispute_data.arbitration_fee > 0
    //                           || dispute_data.resolver_fee > 0;
    //
    // When both fees are zero this evaluates to `false` on every invocation —
    // even after an appeal, because clear_resolution() preserves the stored
    // fee fields and they remain 0. The deduction block therefore re-enters on
    // the second resolve round, but subtracts 0 each time (harmless).
    //
    // This test verifies:
    //   1. First resolve: no fees deducted, escrow.amount stays at 1_000.
    //   2. DisputeData fields arbitration_fee and resolver_fee both stored as 0.
    //   3. Appeal succeeds: state returns to Disputed, appeal_count = 1.
    //   4. Second resolve: no fees deducted again (same zero path).
    //   5. Finalize: seller receives the full 1_000 with no reduction.
    //   6. Buyer receives nothing.

    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    // Initialize with arbitration_fee_bps = 0 so no fees are charged at all.
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount: i128 = 1_000;

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);

    // Create escrow with fee_bps = 0, resolver_fee_bps = 0 — no fees anywhere.
    let escrow_id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_address,
        &amount,
        &0_u32, // fee_bps
        &0_u32, // resolver_fee_bps
        &3600_u64,
        &None::<String>,
    );

    // Mint and fund; funded_at ≈ 0, dispute_deadline = funded_at + 172_800.
    let sac_admin = token::StellarAssetClient::new(&env, &token_address);
    sac_admin.mint(&buyer, &amount);
    client.fund_escrow(&escrow_id, &buyer);

    // Mark shipped (no timestamp guard; just needs Funded state).
    env.ledger().set_timestamp(1_000);
    let tracking = String::from_str(&env, "TRK-ZERO");
    client.mark_shipped(&seller, &escrow_id, &tracking);

    // Raise dispute (t=1_000 < dispute_deadline=172_800 ✓).
    let reason = Symbol::new(&env, "no_delivery");
    let description = String::from_str(&env, "Item not received");
    let evidence = BytesN::from_array(&env, &[0u8; 32]);
    client.raise_dispute(&buyer, &escrow_id, &reason, &description, &evidence);

    // ── Round 1: resolve in favour of seller ────────────────────────────────
    env.ledger().set_timestamp(2_000);
    client.resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release);
    // resolved_at = 2_000; appeal_deadline = 2_000 + 86_400 = 88_400.

    // Verify no fees were deducted and the stored amounts are both 0.
    let dispute_after_r1: DisputeData = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::Dispute(escrow_id))
        })
        .expect("dispute record must exist");
    assert_eq!(
        dispute_after_r1.arbitration_fee, 0,
        "arbitration_fee must be 0 when arbitration_fee_bps is 0"
    );
    assert_eq!(
        dispute_after_r1.resolver_fee, 0,
        "resolver_fee must be 0 when resolver_fee_bps is 0"
    );

    // Escrow amount is untouched — no deductions were made.
    let escrow_after_r1: EscrowData = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow record must exist");
    assert_eq!(
        escrow_after_r1.state,
        EscrowState::PendingFinalization,
        "escrow must be PendingFinalization after resolve"
    );
    assert_eq!(
        escrow_after_r1.amount, amount,
        "escrow amount must not change when all fees are 0"
    );

    // ── Appeal ───────────────────────────────────────────────────────────────
    // t=3_000 < appeal_deadline=88_400 ✓, appeal_count starts at 0 < MAX_APPEALS=3 ✓.
    env.ledger().set_timestamp(3_000);
    client.appeal_dispute(&buyer, &escrow_id);

    let dispute_after_appeal: DisputeData = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::Dispute(escrow_id))
        })
        .expect("dispute record must exist after appeal");
    assert_eq!(
        dispute_after_appeal.appeal_count, 1,
        "appeal_count must increment to 1"
    );
    // clear_resolution preserves arbitration_fee and resolver_fee intentionally.
    assert_eq!(
        dispute_after_appeal.arbitration_fee, 0,
        "arbitration_fee preserved as 0 through appeal"
    );
    assert_eq!(
        dispute_after_appeal.resolver_fee, 0,
        "resolver_fee preserved as 0 through appeal"
    );

    let escrow_after_appeal: EscrowData = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow record must exist after appeal");
    assert_eq!(
        escrow_after_appeal.state,
        EscrowState::Disputed,
        "escrow must return to Disputed after appeal"
    );

    // ── Round 2: resolve again ───────────────────────────────────────────────
    // fees_already_charged = (0 > 0 || 0 > 0) = false — the deduction block
    // re-enters but subtracts 0 again. No double-charge occurs.
    env.ledger().set_timestamp(4_000);
    client.resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release);
    // resolved_at = 4_000; new appeal_deadline = 4_000 + 86_400 = 90_400.

    let dispute_after_r2: DisputeData = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::Dispute(escrow_id))
        })
        .expect("dispute record must exist after round-2 resolve");
    assert_eq!(
        dispute_after_r2.arbitration_fee, 0,
        "arbitration_fee still 0 after round-2 resolve"
    );
    assert_eq!(
        dispute_after_r2.resolver_fee, 0,
        "resolver_fee still 0 after round-2 resolve"
    );

    // Escrow amount must still be the original 1_000 — no fees on either round.
    let escrow_after_r2: EscrowData = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow record must exist after round-2 resolve");
    assert_eq!(
        escrow_after_r2.amount, amount,
        "escrow amount must remain 1_000 with zero fees across two resolve rounds"
    );
    assert_eq!(escrow_after_r2.state, EscrowState::PendingFinalization);

    // ── Finalize: advance past the appeal window (4_000 + 86_400 = 90_400) ──
    env.ledger().set_timestamp(91_000);
    client.finalize_dispute(&resolver, &escrow_id);

    // Seller must receive the full 1_000 (fee_bps=0, no platform fee either).
    let token_client = token::TokenClient::new(&env, &token_address);
    assert_eq!(
        token_client.balance(&seller),
        amount,
        "seller must receive the full amount when all fees are zero"
    );
    assert_eq!(
        token_client.balance(&buyer),
        0,
        "buyer must receive nothing on a Release resolution"
    );
    assert_eq!(
        token_client.balance(&fee_collector),
        0,
        "fee collector must receive nothing when all fees are zero"
    );

    // Escrow state must be Completed.
    let escrow_final: EscrowData = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow record must exist after finalize");
    assert_eq!(
        escrow_final.state,
        EscrowState::Completed,
        "escrow must be Completed after finalize"
    );
}
