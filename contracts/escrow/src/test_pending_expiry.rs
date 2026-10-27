#![cfg(test)]
//! Tests for pending-escrow expiration (#373).
//!
//! Acceptance criteria:
//!  - `fund_escrow` post-expiry fails with `EscrowExpired`
//!  - `auto_cancel_pending` transitions the escrow to `Canceled`

use crate::{ContractError, Escrow, EscrowClient, EscrowState, Payee};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, Env, IntoVal, Vec,
};

fn single_payee(env: &Env, address: &Address) -> Vec<Payee> {
    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: address.clone(),
        bps: 10_000,
    });
    payees
}

const PENDING_EXPIRY_WINDOW: u64 = 604_800; // 7 days, must match lib.rs

fn setup(
    env: &Env,
) -> (
    EscrowClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let fee_collector = Address::generate(env);
    let token_admin = Address::generate(env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    token::StellarAssetClient::new(env, &token_addr).mint(&buyer, &10_000_i128);

    (client, seller, buyer, resolver, fee_collector, token_addr)
}

#[test]
fn fund_escrow_before_expiry_succeeds() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );

    // Fund one second before the window closes — should succeed.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW - 1;
    });

    client.fund_escrow(&escrow_id, &buyer);

    use crate::{DataKey, EscrowData};
    let data: EscrowData = env
        .as_contract(&client.address, || {
            env.storage().persistent().get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow exists");
    assert_eq!(data.state, EscrowState::Funded);
}

#[test]
fn fund_escrow_after_expiry_returns_escrow_expired() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );

    // Advance past the expiry window.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW + 1;
    });

    let result = client.try_fund_escrow(&escrow_id, &buyer);
    assert_eq!(
        result,
        Err(Ok(ContractError::EscrowExpired)),
        "fund_escrow post-expiry must return EscrowExpired",
    );
}

#[test]
fn auto_cancel_pending_before_expiry_is_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, _buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );

    // One second before the expiry deadline — cancel must be rejected.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW - 1;
    });

    let result = client.try_auto_cancel_pending(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::ShippingWindowNotElapsed)),
        "auto_cancel_pending must be rejected before the expiry deadline",
    );
}

#[test]
fn auto_cancel_pending_after_expiry_transitions_to_canceled() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, _buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );

    // One second past the expiry window.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW + 1;
    });

    client.auto_cancel_pending(&escrow_id);

    use crate::{DataKey, EscrowData};
    let data: EscrowData = env
        .as_contract(&client.address, || {
            env.storage().persistent().get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow exists");
    assert_eq!(
        data.state,
        EscrowState::Canceled,
        "auto_cancel_pending must transition state to Canceled",
    );
}

#[test]
fn auto_cancel_pending_on_funded_escrow_returns_invalid_state() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, _fee_collector, token_addr) = setup(&env);

    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let escrow_id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&escrow_id, &buyer);

    // Advance past expiry — but the escrow is already Funded, not Pending.
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW + 1;
    });

    let result = client.try_auto_cancel_pending(&escrow_id);
    assert_eq!(
        result,
        Err(Ok(ContractError::InvalidState)),
        "auto_cancel_pending on a non-Pending escrow must return InvalidState",
    );
}

#[test]
fn test_ensure_funding_not_expired_helper_exercised_via_both_fund_paths() {
    // Issue #1170: Verify that ensure_funding_not_expired canonical helper
    // governs both fund_escrow and fund_basket_escrow across:
    // 1. Blanket PENDING_EXPIRY_WINDOW expiry
    // 2. Explicit PendingExpiry schedule expiry
    // 3. Custom escrow.expires_at expiry
    // 4. Successful funding before expiry
    use crate::{DataKey, EscrowData, ExpirySchedule};

    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, seller, buyer, resolver, _fee_collector, token_addr) = setup(&env);
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &100_000_i128);

    // ── 1. Blanket PENDING_EXPIRY_WINDOW expiry ─────────────────────────────
    let payees = single_payee(&env, &seller);
    let payees_val = payees.into_val(&env);
    let std_escrow_1 = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );
    let mut basket_tokens = Vec::new(&env);
    basket_tokens.push_back(token_addr.clone());
    let mut basket_amounts = Vec::new(&env);
    basket_amounts.push_back(1_000_i128);
    let basket_escrow_1 = client.create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &basket_tokens,
        &basket_amounts,
        &0_u32,
        &3600_u64,
        &None,
        &None,
        &0_u64,
    );

    // Advance past blanket 7-day window
    env.ledger().with_mut(|li| {
        li.timestamp = 1_000_000 + PENDING_EXPIRY_WINDOW + 1;
    });

    assert_eq!(
        client.try_fund_escrow(&std_escrow_1, &buyer),
        Err(Ok(ContractError::EscrowExpired)),
        "fund_escrow must be rejected after blanket expiry"
    );
    assert_eq!(
        client.try_fund_basket_escrow(&basket_escrow_1, &buyer),
        Err(Ok(ContractError::EscrowExpired)),
        "fund_basket_escrow must be rejected after blanket expiry"
    );

    // ── 2. Explicit PendingExpiry schedule expiry ───────────────────────────
    env.ledger().with_mut(|li| {
        li.timestamp = 2_000_000;
    });
    let std_escrow_2 = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );
    let basket_escrow_2 = client.create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &basket_tokens,
        &basket_amounts,
        &0_u32,
        &3600_u64,
        &None,
        &None,
        &0_u64,
    );

    let schedule = ExpirySchedule {
        expires_at: 2_000_500,
        grace_period: 0,
    };
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::PendingExpiry(std_escrow_2), &schedule);
        env.storage()
            .persistent()
            .set(&DataKey::PendingExpiry(basket_escrow_2), &schedule);
    });

    // Advance past the schedule's expires_at (but within blanket window)
    env.ledger().with_mut(|li| {
        li.timestamp = 2_000_501;
    });

    assert_eq!(
        client.try_fund_escrow(&std_escrow_2, &buyer),
        Err(Ok(ContractError::EscrowExpired)),
        "fund_escrow must be rejected after PendingExpiry schedule elapses"
    );
    assert_eq!(
        client.try_fund_basket_escrow(&basket_escrow_2, &buyer),
        Err(Ok(ContractError::EscrowExpired)),
        "fund_basket_escrow must be rejected after PendingExpiry schedule elapses"
    );

    // ── 3. Custom escrow.expires_at expiry ──────────────────────────────────
    env.ledger().with_mut(|li| {
        li.timestamp = 3_000_000;
    });
    let std_escrow_3 = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );
    let basket_escrow_3 = client.create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &basket_tokens,
        &basket_amounts,
        &0_u32,
        &3600_u64,
        &None,
        &None,
        &0_u64,
    );

    // Populate custom expires_at directly in escrow data
    env.as_contract(&client.address, || {
        let mut data_std: EscrowData = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(std_escrow_3))
            .unwrap();
        data_std.expires_at = Some(3_000_300);
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(std_escrow_3), &data_std);

        let mut data_basket: EscrowData = env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(basket_escrow_3))
            .unwrap();
        data_basket.expires_at = Some(3_000_300);
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(basket_escrow_3), &data_basket);
    });

    // Advance to custom expires_at
    env.ledger().with_mut(|li| {
        li.timestamp = 3_000_300;
    });

    assert_eq!(
        client.try_fund_escrow(&std_escrow_3, &buyer),
        Err(Ok(ContractError::EscrowExpired)),
        "fund_escrow must be rejected at/after custom expires_at"
    );
    assert_eq!(
        client.try_fund_basket_escrow(&basket_escrow_3, &buyer),
        Err(Ok(ContractError::EscrowExpired)),
        "fund_basket_escrow must be rejected at/after custom expires_at"
    );

    // ── 4. Both paths succeed before expiry ──────────────────────────────────
    env.ledger().with_mut(|li| {
        li.timestamp = 4_000_000;
    });
    let std_escrow_4 = client.create_escrow_8(
        &payees_val,
        &Some(buyer.clone()),
        &resolver,
        &token_addr,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );
    let basket_escrow_4 = client.create_basket_escrow(
        &seller,
        &Some(buyer.clone()),
        &resolver,
        &basket_tokens,
        &basket_amounts,
        &0_u32,
        &3600_u64,
        &None,
        &None,
        &0_u64,
    );

    let schedule_4 = ExpirySchedule {
        expires_at: 4_001_000,
        grace_period: 0,
    };
    env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .set(&DataKey::PendingExpiry(std_escrow_4), &schedule_4);
        env.storage()
            .persistent()
            .set(&DataKey::PendingExpiry(basket_escrow_4), &schedule_4);
    });

    // Fund before expiry
    env.ledger().with_mut(|li| {
        li.timestamp = 4_000_100;
    });

    client.fund_escrow(&std_escrow_4, &buyer);
    client.fund_basket_escrow(&basket_escrow_4, &buyer);

    assert_eq!(client.get_escrow(&std_escrow_4).state, EscrowState::Funded);
    assert_eq!(
        client.get_escrow(&basket_escrow_4).state,
        EscrowState::Funded
    );
}
