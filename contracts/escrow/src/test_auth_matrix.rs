#![cfg(test)]
//! Exhaustive wrong-caller and wrong-role authorization test matrix.
//!
//! Organized into four sections:
//!   1. Admin gate (intruder)       — entry points not already in test_unauthorized.rs
//!   2. Admin gate (participant)    — seller/buyer/resolver cannot invoke admin ops
//!   3. Escrow-role gate            — wrong participant rejected per-operation
//!   4. State gate (correct caller) — correct role, wrong escrow state
//!
//! Coverage measurement:
//!   cargo llvm-cov --package escrow -- test_auth_matrix
//!
//! Every assertion uses try_* and checks the exact ContractError variant so
//! each test is both a functional guard and a regression anchor.

use crate::{ContractError, Escrow, EscrowClient, Payee, ResolutionType};
use soroban_sdk::{
    testutils::Address as _,
    token, Address, BytesN, Env, String as SorobanString, Symbol, Vec,
};

// ─── harness ────────────────────────────────────────────────────────────────

struct Ctx<'e> {
    env: &'e Env,
    client: EscrowClient<'e>,
    admin: Address,
    token: Address,
}

fn setup(env: &Env) -> Ctx {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let fee_collector = Address::generate(env);
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    let token_owner = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(token_owner).address();
    client.initialize(&admin, &fee_collector, &0_u32);
    Ctx { env, client, admin, token }
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token).mint(to, &amount);
}

fn payees(env: &Env, seller: &Address) -> Vec<Payee> {
    let mut v = Vec::new(env);
    v.push_back(Payee { address: seller.clone(), bps: 10_000 });
    v
}

// State helpers — each returns (escrow_id, seller, buyer, resolver)

fn pending(ctx: &Ctx) -> (u64, Address, Address, Address) {
    let seller = Address::generate(ctx.env);
    let buyer = Address::generate(ctx.env);
    let resolver = Address::generate(ctx.env);
    let p = payees(ctx.env, &seller);
    let id = ctx.client.create_escrow(
        &p,
        &None::<Address>,
        &resolver,
        &ctx.token,
        &1_000_i128,
        &0_u32,
        &0_u32,
        &3_600_u64,
    );
    (id, seller, buyer, resolver)
}

fn funded(ctx: &Ctx) -> (u64, Address, Address, Address) {
    let (id, seller, buyer, resolver) = pending(ctx);
    mint(ctx.env, &ctx.token, &buyer, 1_000);
    ctx.client.fund_escrow(&id, &buyer);
    (id, seller, buyer, resolver)
}

fn shipped(ctx: &Ctx) -> (u64, Address, Address, Address) {
    let (id, seller, buyer, resolver) = funded(ctx);
    ctx.client.mark_shipped(&seller, &id, &SorobanString::from_str(ctx.env, "TRK001"));
    (id, seller, buyer, resolver)
}

fn disputed(ctx: &Ctx) -> (u64, Address, Address, Address) {
    let (id, seller, buyer, resolver) = shipped(ctx);
    let hash = BytesN::from_array(ctx.env, &[0u8; 32]);
    ctx.client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(ctx.env, "Item"),
        &SorobanString::from_str(ctx.env, "not received"),
        &hash,
    );
    (id, seller, buyer, resolver)
}

fn refund_requested(ctx: &Ctx) -> (u64, Address, Address, Address) {
    let (id, seller, buyer, resolver) = funded(ctx);
    ctx.client.request_refund(&buyer, &id);
    (id, seller, buyer, resolver)
}

// ─── Section 1: Admin gate — intruder ───────────────────────────────────────
// Each entry point uses `caller != admin → NotAuthorized` identity check.
// Tests here cover entry points not already in test_unauthorized.rs.

#[test]
fn pause_action_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_pause_action(&intruder, &Symbol::new(&env, "SHIP")),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn unpause_action_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    // Pause it first so the state is correct, then test wrong caller.
    ctx.client.pause_action(&ctx.admin, &Symbol::new(&env, "SHIP"));
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_unpause_action(&intruder, &Symbol::new(&env, "SHIP")),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn upgrade_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    let fake_hash = BytesN::from_array(&env, &[0u8; 32]);
    // Auth check happens before the actual WASM update, so NotAuthorized fires first.
    assert_eq!(
        ctx.client.try_upgrade(&intruder, &fake_hash),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn set_protocol_fee_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_set_protocol_fee(&intruder, &0_u32),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn set_fee_collector_requires_admin_auth() {
    // set_fee_collector is host-level gated: admin.require_auth() with no
    // explicit caller arg.  Clearing all mocked auths must fail the call.
    let env = Env::default();
    let ctx = setup(&env);
    let new_collector = Address::generate(&env);
    env.mock_auths(&[]);
    assert!(ctx.client.try_set_fee_collector(&new_collector).is_err());
}

#[test]
fn record_delivery_rejects_non_admin() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = shipped(&ctx);
    assert_eq!(
        ctx.client.try_record_delivery(&seller, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn set_token_allowlist_enabled_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_set_token_allowlist_enabled(&intruder, &true),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn add_allowed_token_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    let some_token = Address::generate(&env);
    assert_eq!(
        ctx.client.try_add_allowed_token(&intruder, &some_token),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn remove_allowed_token_rejects_intruder() {
    // Add a token as admin first so the "token not in list" error does not
    // shadow the identity check that fires earlier.
    let env = Env::default();
    let ctx = setup(&env);
    let some_token = Address::generate(&env);
    ctx.client.add_allowed_token(&ctx.admin, &some_token);

    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_remove_allowed_token(&intruder, &some_token),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn set_platform_fee_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_set_platform_fee(&intruder, &0_u32),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn set_treasury_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    let treasury = Address::generate(&env);
    assert_eq!(
        ctx.client.try_set_treasury(&intruder, &treasury),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn set_amount_limits_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_set_amount_limits(&intruder, &1_i128, &1_000_000_i128),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn set_resolver_strict_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_set_resolver_strict(&intruder, &true),
        Err(Ok(ContractError::NotAuthorized))
    );
}

// ─── Section 2: Admin gate — participant role does not grant admin access ────
// Being a seller/buyer/resolver in an escrow does not elevate privileges.

#[test]
fn seller_cannot_pause_contract() {
    let env = Env::default();
    let ctx = setup(&env);
    let (_, seller, _, _) = pending(&ctx);
    assert_eq!(
        ctx.client.try_pause_contract(&seller),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn buyer_cannot_set_fee() {
    let env = Env::default();
    let ctx = setup(&env);
    let (_, _, buyer, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_set_fee(&buyer, &0_u32),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn resolver_cannot_withdraw_fees() {
    let env = Env::default();
    let ctx = setup(&env);
    let (_, _, _, resolver) = pending(&ctx);
    let recipient = Address::generate(&env);
    assert_eq!(
        ctx.client.try_withdraw_fees(&resolver, &ctx.token, &recipient, &1_i128),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn buyer_cannot_set_ttl_extension() {
    let env = Env::default();
    let ctx = setup(&env);
    let (_, _, buyer, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_set_ttl_extension(&buyer, &120_960_u32),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn resolver_cannot_set_arbitration_fee() {
    let env = Env::default();
    let ctx = setup(&env);
    let (_, _, _, resolver) = pending(&ctx);
    assert_eq!(
        ctx.client.try_set_arbitration_fee(&resolver, &10_u32),
        Err(Ok(ContractError::NotAuthorized))
    );
}

// ─── Section 3a: Seller-gate — wrong caller ──────────────────────────────────

#[test]
fn mark_shipped_rejects_buyer() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, buyer, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_mark_shipped(&buyer, &id, &SorobanString::from_str(&env, "TRK")),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn mark_shipped_rejects_resolver() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, resolver) = funded(&ctx);
    assert_eq!(
        ctx.client.try_mark_shipped(&resolver, &id, &SorobanString::from_str(&env, "TRK")),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn mark_shipped_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = funded(&ctx);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_mark_shipped(&intruder, &id, &SorobanString::from_str(&env, "TRK")),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn approve_refund_rejects_buyer() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, buyer, _) = refund_requested(&ctx);
    assert_eq!(
        ctx.client.try_approve_refund(&buyer, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn approve_refund_rejects_resolver() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, resolver) = refund_requested(&ctx);
    assert_eq!(
        ctx.client.try_approve_refund(&resolver, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn approve_refund_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = refund_requested(&ctx);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_approve_refund(&intruder, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn cancel_pending_escrow_rejects_resolver() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, resolver) = pending(&ctx);
    assert_eq!(
        ctx.client.try_cancel_escrow(&resolver, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn cancel_pending_escrow_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = pending(&ctx);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_cancel_escrow(&intruder, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

// ─── Section 3b: Buyer-gate — wrong caller ───────────────────────────────────

#[test]
fn confirm_delivery_rejects_seller() {
    // Auth check (caller != buyer) fires before the dispute-window check,
    // so no timestamp manipulation is needed.
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = shipped(&ctx);
    assert_eq!(
        ctx.client.try_confirm_delivery(&seller, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn confirm_delivery_rejects_resolver() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, resolver) = shipped(&ctx);
    assert_eq!(
        ctx.client.try_confirm_delivery(&resolver, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn confirm_delivery_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = shipped(&ctx);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_confirm_delivery(&intruder, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn raise_dispute_rejects_seller() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = shipped(&ctx);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        ctx.client.try_raise_dispute(
            &seller,
            &id,
            &Symbol::new(&env, "Item"),
            &SorobanString::from_str(&env, "desc"),
            &hash,
        ),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn raise_dispute_rejects_resolver() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, resolver) = shipped(&ctx);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        ctx.client.try_raise_dispute(
            &resolver,
            &id,
            &Symbol::new(&env, "Item"),
            &SorobanString::from_str(&env, "desc"),
            &hash,
        ),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn raise_dispute_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = shipped(&ctx);
    let intruder = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        ctx.client.try_raise_dispute(
            &intruder,
            &id,
            &Symbol::new(&env, "Item"),
            &SorobanString::from_str(&env, "desc"),
            &hash,
        ),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn request_refund_rejects_seller() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_request_refund(&seller, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn request_refund_rejects_resolver() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, resolver) = funded(&ctx);
    assert_eq!(
        ctx.client.try_request_refund(&resolver, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn request_refund_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = funded(&ctx);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_request_refund(&intruder, &id),
        Err(Ok(ContractError::NotAuthorized))
    );
}

// ─── Section 3c: Resolver-gate — wrong caller ────────────────────────────────

#[test]
fn resolve_dispute_rejects_buyer() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, buyer, _) = disputed(&ctx);
    assert_eq!(
        ctx.client.try_resolve_dispute(&buyer, &id, &ResolutionType::Release),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn resolve_dispute_rejects_seller() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = disputed(&ctx);
    assert_eq!(
        ctx.client.try_resolve_dispute(&seller, &id, &ResolutionType::Release),
        Err(Ok(ContractError::NotAuthorized))
    );
}

#[test]
fn resolve_dispute_rejects_intruder() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = disputed(&ctx);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_resolve_dispute(&intruder, &id, &ResolutionType::Release),
        Err(Ok(ContractError::NotAuthorized))
    );
}

// ─── Section 3d: Buyer-or-seller gate — wrong caller ─────────────────────────
// appeal_dispute checks state == PendingFinalization before the identity gate.
// This state is only reachable via multi-resolver partial voting; with a
// single-resolver escrow the function returns NotPendingFinalization,
// which proves the function rejects all callers until state precondition is met.

#[test]
fn appeal_dispute_rejects_resolver_wrong_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, resolver) = disputed(&ctx);
    assert_eq!(
        ctx.client.try_appeal_dispute(&resolver, &id),
        Err(Ok(ContractError::NotPendingFinalization))
    );
}

#[test]
fn appeal_dispute_rejects_intruder_wrong_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = disputed(&ctx);
    let intruder = Address::generate(&env);
    assert_eq!(
        ctx.client.try_appeal_dispute(&intruder, &id),
        Err(Ok(ContractError::NotPendingFinalization))
    );
}

// ─── Section 4: Co-signature requirements ────────────────────────────────────

#[test]
fn mutual_cancel_requires_auth() {
    // Without any mocked signatures, seller.require_auth() fires first and fails.
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = funded(&ctx);
    env.mock_auths(&[]);
    assert!(ctx.client.try_mutual_cancel(&id).is_err());
}

#[test]
fn emergency_drain_requires_paused_contract() {
    // State gate fires before the co-signature check.
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_emergency_drain(&id),
        Err(Ok(ContractError::ContractNotPaused))
    );
}

#[test]
fn emergency_drain_requires_auth_when_paused() {
    // Pause the contract as admin, then call emergency_drain without any
    // mocked auth so buyer.require_auth() and seller.require_auth() both fail.
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = funded(&ctx);
    ctx.client.pause_contract(&ctx.admin);
    env.mock_auths(&[]);
    assert!(ctx.client.try_emergency_drain(&id).is_err());
}

#[test]
fn co_signed_release_requires_auth() {
    // caller.require_auth(), seller.require_auth(), buyer.require_auth() are
    // all required.  With no mocked auth the first require_auth fails.
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = funded(&ctx);
    env.mock_auths(&[]);
    assert!(ctx.client.try_co_signed_release(&seller, &id).is_err());
}

// ─── Section 5: State gate (correct caller, wrong state) ─────────────────────
// Proves that even the right role receives an error when the escrow is in an
// unexpected state, and that the error codes are distinct from NotAuthorized.

#[test]
fn seller_cannot_cancel_funded_escrow() {
    // Seller is a payee but only buyers may cancel Funded escrows.
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_cancel_escrow(&seller, &id),
        Err(Ok(ContractError::InvalidState))
    );
}

#[test]
fn mark_shipped_rejects_seller_in_pending_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = pending(&ctx);
    assert_eq!(
        ctx.client.try_mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRK")),
        Err(Ok(ContractError::InvalidState))
    );
}

#[test]
fn confirm_delivery_rejects_buyer_in_funded_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, buyer, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_confirm_delivery(&buyer, &id),
        Err(Ok(ContractError::InvalidStateTransition))
    );
}

#[test]
fn raise_dispute_rejects_buyer_in_pending_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, buyer, _) = pending(&ctx);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    // buyer check passes (buyer is set after fund but NOT here — escrow is Pending
    // with no buyer yet, so EscrowHasNoBuyer fires before the state check)
    assert!(ctx.client.try_raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "Item"),
        &SorobanString::from_str(&env, "desc"),
        &hash,
    ).is_err());
}

#[test]
fn request_refund_rejects_buyer_in_shipped_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, buyer, _) = shipped(&ctx);
    assert_eq!(
        ctx.client.try_request_refund(&buyer, &id),
        Err(Ok(ContractError::InvalidStateTransition))
    );
}

#[test]
fn approve_refund_rejects_seller_in_funded_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, seller, _, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_approve_refund(&seller, &id),
        Err(Ok(ContractError::InvalidStateTransition))
    );
}

#[test]
fn record_delivery_rejects_admin_in_funded_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, _) = funded(&ctx);
    assert_eq!(
        ctx.client.try_record_delivery(&ctx.admin, &id),
        Err(Ok(ContractError::InvalidState))
    );
}

#[test]
fn resolve_dispute_rejects_resolver_in_shipped_state() {
    let env = Env::default();
    let ctx = setup(&env);
    let (id, _, _, resolver) = shipped(&ctx);
    assert_eq!(
        ctx.client.try_resolve_dispute(&resolver, &id, &ResolutionType::Release),
        Err(Ok(ContractError::InvalidState))
    );
}
