#![cfg(test)]
//! Tests for partial funding / tranches (#362): fund_escrow_tranche lets a
//! buyer pay an escrow's agreed amount in multiple installments instead of
//! one lump sum.
//!
//! Covers both acceptance criteria directly:
//! - "Cannot mark shipped at 99%" -> `cannot_mark_shipped_at_ninety_nine_percent`
//! - "Refund returns exact funded amount" -> `cancel_refunds_exact_partial_amount`

use crate::{ContractError, DataKey, Escrow, EscrowClient, EscrowData, EscrowState};
use soroban_sdk::{
    testutils::Address as _, token, Address, Env, String,
};

struct Fx {
    env: Env,
    client: EscrowClient<'static>,
    contract_id: Address,
    escrow_id: u64,
    seller: Address,
    buyer: Address,
    token_addr: Address,
}

/// Creates a Pending escrow for 1_000 stroops; mints 1_000 to the buyer but
/// funds nothing yet, so tests can drive tranches explicitly.
fn setup() -> Fx {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let escrow_id = client.create_escrow(
        &single_payee(&env, &seller),
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &0_u32,
        &0_u64,
    );
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &1_000_i128);

    Fx { env, client, contract_id, escrow_id, seller, buyer, token_addr }
}

fn load_escrow(fx: &Fx) -> EscrowData {
    fx.env
        .as_contract(&fx.contract_id, || {
            fx.env.storage().persistent().get(&DataKey::Escrow(fx.escrow_id))
        })
        .expect("escrow exists")
}

// ── Acceptance criterion: cannot mark shipped before fully funded ──────────

#[test]
fn cannot_mark_shipped_at_ninety_nine_percent() {
    let fx = setup();
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &990_i128);

    let escrow = load_escrow(&fx);
    assert_eq!(escrow.state, EscrowState::Pending, "990/1000 is not fully funded");

    let tracking = String::from_str(&fx.env, "TRK-EARLY");
    let result = fx.client.try_mark_shipped(&fx.seller, &fx.escrow_id, &tracking);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}

#[test]
fn mark_shipped_succeeds_once_fully_funded_via_tranches() {
    let fx = setup();
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &600_i128);
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &400_i128);

    let escrow = load_escrow(&fx);
    assert_eq!(escrow.state, EscrowState::Funded);

    let tracking = String::from_str(&fx.env, "TRK-DONE");
    fx.client.mark_shipped(&fx.seller, &fx.escrow_id, &tracking);

    assert_eq!(load_escrow(&fx).state, EscrowState::Shipped);
}

// ── Acceptance criterion: refund returns exact funded amount ───────────────

#[test]
fn cancel_refunds_exact_partial_amount() {
    let fx = setup();
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &300_i128);

    let token_client = token::Client::new(&fx.env, &fx.token_addr);
    assert_eq!(token_client.balance(&fx.buyer), 700, "300 of 1000 has left the buyer");

    fx.client.cancel_escrow(&fx.buyer, &fx.escrow_id);

    assert_eq!(
        token_client.balance(&fx.buyer),
        1_000,
        "refund must return exactly the 300 that was funded, restoring the full original balance"
    );
    assert_eq!(token_client.balance(&fx.contract_id), 0);
    assert_eq!(load_escrow(&fx).funded_amount, 0);
}

#[test]
fn cancel_refunds_nothing_extra_beyond_what_was_funded() {
    let fx = setup();
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &150_i128);
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &200_i128);
    // 350 of 1000 funded - well short of the full 1000 agreed amount.

    let token_client = token::Client::new(&fx.env, &fx.token_addr);
    fx.client.cancel_escrow(&fx.buyer, &fx.escrow_id);

    assert_eq!(
        token_client.balance(&fx.buyer),
        1_000,
        "buyer started with 1000, paid in 350, must get exactly 350 back - not the full 1000 agreed amount"
    );
}

// ── Tranche accumulation and validation ─────────────────────────────────────

#[test]
fn multiple_tranches_accumulate_toward_full_funding() {
    let fx = setup();
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &200_i128);
    assert_eq!(load_escrow(&fx).funded_amount, 200);
    assert_eq!(load_escrow(&fx).state, EscrowState::Pending);

    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &300_i128);
    assert_eq!(load_escrow(&fx).funded_amount, 500);
    assert_eq!(load_escrow(&fx).state, EscrowState::Pending);

    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &500_i128);
    assert_eq!(load_escrow(&fx).funded_amount, 1_000);
    assert_eq!(load_escrow(&fx).state, EscrowState::Funded);

    let token_client = token::Client::new(&fx.env, &fx.token_addr);
    assert_eq!(token_client.balance(&fx.contract_id), 1_000);
}

#[test]
fn tranche_exceeding_remaining_amount_is_rejected() {
    let fx = setup();
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &800_i128);

    let result = fx.client.try_fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &300_i128);
    assert_eq!(result, Err(Ok(ContractError::TrancheExceedsRemaining)));

    // The rejected attempt must not have partially applied.
    assert_eq!(load_escrow(&fx).funded_amount, 800);
    let token_client = token::Client::new(&fx.env, &fx.token_addr);
    assert_eq!(token_client.balance(&fx.contract_id), 800);
}

#[test]
fn tranche_must_come_from_the_same_buyer() {
    let fx = setup();
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &400_i128);

    let impostor = Address::generate(&fx.env);
    token::StellarAssetClient::new(&fx.env, &fx.token_addr).mint(&impostor, &1_000_i128);

    let result = fx.client.try_fund_escrow_tranche(&fx.escrow_id, &impostor, &100_i128);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn tranche_amount_must_be_positive() {
    let fx = setup();
    let result = fx.client.try_fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &0_i128);
    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));

    let result = fx.client.try_fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &-50_i128);
    assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
}

#[test]
fn fund_escrow_after_a_partial_tranche_pays_exactly_the_remainder() {
    let fx = setup();
    fx.client.fund_escrow_tranche(&fx.escrow_id, &fx.buyer, &250_i128);

    // The plain lump-sum entrypoint should now mean "pay whatever's left",
    // not "pay the full original amount again".
    fx.client.fund_escrow(&fx.escrow_id, &fx.buyer);

    let token_client = token::Client::new(&fx.env, &fx.token_addr);
    assert_eq!(token_client.balance(&fx.buyer), 0, "exactly 1000 total left the buyer, not 1250");
    assert_eq!(token_client.balance(&fx.contract_id), 1_000);
    assert_eq!(load_escrow(&fx).state, EscrowState::Funded);
}

#[test]
fn fund_escrow_tranche_rejects_buyer_equal_to_seller() {
    let fx = setup();
    let result = fx.client.try_fund_escrow_tranche(&fx.escrow_id, &fx.seller, &100_i128);
    assert_eq!(result, Err(Ok(ContractError::ConflictingRoles)));
}