#![cfg(test)]

use crate::{ContractError, Escrow, EscrowClient, EscrowState, Payee};
use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String as SorobanString, Symbol, TryFromVal, Val, Vec,
};

#[allow(dead_code)]
struct TestFixture {
    env: Env,
    client: EscrowClient<'static>,
    admin: Address,
    seller: Address,
    buyer: Address,
    resolver: Address,
    fee_collector: Address,
    token: Address,
}

fn setup_fixture() -> TestFixture {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    TestFixture {
        env,
        client,
        admin,
        seller,
        buyer,
        resolver,
        fee_collector,
        token,
    }
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    let tc = token::Client::new(env, token);
    tc.balance(who)
}

#[test]
fn test_auto_release_success_after_shipping_window() {
    let fix = setup_fixture();
    let amount = 10_000_i128;
    let shipping_window = 3600_u64;

    let seller_val = fix.seller.clone().into_val(&fix.env);
    let escrow_id = fix.client.create_escrow_8(
        &seller_val,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &fix.token,
        &amount,
        &0_u32,
        &shipping_window,
    );

    mint(&fix.env, &fix.token, &fix.buyer, amount);
    fix.client.fund_escrow(&escrow_id, &fix.buyer);

    let now = fix.env.ledger().timestamp();
    // Default dispute window is 172_800. Move past dispute_deadline and shipping window:
    fix.env
        .ledger()
        .set_timestamp(now + 172_800 + shipping_window + 10);

    fix.client.auto_release(&escrow_id);

    let escrow = fix.client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);
    assert_eq!(balance(&fix.env, &fix.token, &fix.seller), amount);
    assert_eq!(balance(&fix.env, &fix.token, &fix.client.address), 0);
}

#[test]
fn test_auto_release_exact_timestamp_boundary_funded() {
    let fix = setup_fixture();
    let amount = 5_000_i128;
    // Set a shipping window longer than the default dispute window (172_800)
    let shipping_window = 200_000_u64;

    let seller_val = fix.seller.clone().into_val(&fix.env);
    let escrow_id = fix.client.create_escrow_8(
        &seller_val,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &fix.token,
        &amount,
        &0_u32,
        &shipping_window,
    );

    mint(&fix.env, &fix.token, &fix.buyer, amount);
    let funded_time = 100_000_u64;
    fix.env.ledger().set_timestamp(funded_time);
    fix.client.fund_escrow(&escrow_id, &fix.buyer);

    // 1 second before dispute deadline (funded_time + 172_800 = 272_800):
    fix.env.ledger().set_timestamp(272_799);
    assert_eq!(
        fix.client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::DeliveryBeforeDisputeWindow))
    );

    // At dispute deadline (272_800), but before shipping window (funded_time + 200_000 = 300_000):
    fix.env.ledger().set_timestamp(272_800);
    assert_eq!(
        fix.client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::ShippingWindowNotElapsed))
    );

    // 1 second before shipping window elapsed:
    fix.env.ledger().set_timestamp(299_999);
    assert_eq!(
        fix.client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::ShippingWindowNotElapsed))
    );

    // Exactly at shipping window elapsed:
    fix.env.ledger().set_timestamp(300_000);
    assert!(fix.client.try_auto_release(&escrow_id).is_ok());

    let escrow = fix.client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);
    assert_eq!(balance(&fix.env, &fix.token, &fix.seller), amount);
}

#[test]
fn test_auto_release_with_delivered_at_and_boundary() {
    let fix = setup_fixture();
    let amount = 8_000_i128;
    let shipping_window = 3600_u64;

    let seller_val = fix.seller.clone().into_val(&fix.env);
    let escrow_id = fix.client.create_escrow_8(
        &seller_val,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &fix.token,
        &amount,
        &0_u32,
        &shipping_window,
    );

    mint(&fix.env, &fix.token, &fix.buyer, amount);
    fix.client.fund_escrow(&escrow_id, &fix.buyer);

    fix.env
        .ledger()
        .set_timestamp(fix.env.ledger().timestamp() + 10);
    fix.client.mark_shipped(
        &fix.seller,
        &escrow_id,
        &SorobanString::from_str(&fix.env, "TRACK123"),
    );

    // Propose delivery and record delivery after 24h timelock
    fix.client.propose_record_delivery(&fix.admin, &escrow_id);
    let prop_time = fix.env.ledger().timestamp();
    fix.env.ledger().set_timestamp(prop_time + 86_400);
    fix.client.record_delivery(&fix.admin, &escrow_id);

    let delivered_at = fix.env.ledger().timestamp();
    // DELIVERY_RELEASE_WINDOW is 172_800
    // 1 second before DELIVERY_RELEASE_WINDOW:
    fix.env.ledger().set_timestamp(delivered_at + 172_800 - 1);
    assert_eq!(
        fix.client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::ShippingWindowNotElapsed))
    );

    // Exactly at DELIVERY_RELEASE_WINDOW:
    fix.env.ledger().set_timestamp(delivered_at + 172_800);
    assert!(fix.client.try_auto_release(&escrow_id).is_ok());

    let escrow = fix.client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);
    assert_eq!(balance(&fix.env, &fix.token, &fix.seller), amount);
}

#[test]
fn test_auto_release_shipped_without_delivery_rejected() {
    let fix = setup_fixture();
    let amount = 2_000_i128;
    let seller_val = fix.seller.clone().into_val(&fix.env);
    let escrow_id = fix.client.create_escrow_8(
        &seller_val,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &fix.token,
        &amount,
        &0_u32,
        &3600_u64,
    );

    mint(&fix.env, &fix.token, &fix.buyer, amount);
    fix.client.fund_escrow(&escrow_id, &fix.buyer);
    fix.client.mark_shipped(
        &fix.seller,
        &escrow_id,
        &SorobanString::from_str(&fix.env, "TRK"),
    );

    fix.env
        .ledger()
        .set_timestamp(fix.env.ledger().timestamp() + 500_000);
    // When Shipped but no delivered_at recorded, returns DeliveryNotRecorded
    assert_eq!(
        fix.client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::DeliveryNotRecorded))
    );
}

#[test]
fn test_auto_release_multi_payees_distribution() {
    let fix = setup_fixture();
    let payee1 = Address::generate(&fix.env);
    let payee2 = Address::generate(&fix.env);
    let payee3 = Address::generate(&fix.env);

    let mut payees = Vec::new(&fix.env);
    payees.push_back(Payee {
        address: payee1.clone(),
        bps: 5000, // 50%
    });
    payees.push_back(Payee {
        address: payee2.clone(),
        bps: 3000, // 30%
    });
    payees.push_back(Payee {
        address: payee3.clone(),
        bps: 2000, // 20%
    });

    let amount = 10_000_i128;
    let payees_val: Val = payees.into_val(&fix.env);
    let escrow_id = fix.client.create_escrow_8(
        &payees_val,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &fix.token,
        &amount,
        &0_u32,
        &3600_u64,
    );

    mint(&fix.env, &fix.token, &fix.buyer, amount);
    fix.client.fund_escrow(&escrow_id, &fix.buyer);

    fix.env
        .ledger()
        .set_timestamp(fix.env.ledger().timestamp() + 200_000);
    fix.client.auto_release(&escrow_id);

    assert_eq!(balance(&fix.env, &fix.token, &payee1), 5000);
    assert_eq!(balance(&fix.env, &fix.token, &payee2), 3000);
    assert_eq!(balance(&fix.env, &fix.token, &payee3), 2000);
}

#[test]
fn test_auto_release_basket_escrow_payout() {
    let fix = setup_fixture();
    let token_b_admin = Address::generate(&fix.env);
    let sac_b = fix.env.register_stellar_asset_contract_v2(token_b_admin);
    let token_b = sac_b.address();

    let mut tokens = Vec::new(&fix.env);
    tokens.push_back(fix.token.clone());
    tokens.push_back(token_b.clone());

    let mut amounts = Vec::new(&fix.env);
    amounts.push_back(1000_i128);
    amounts.push_back(500_i128);

    let escrow_id = fix.client.create_basket_escrow(
        &fix.seller,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &tokens,
        &amounts,
        &0_u32,
        &3600_u64,
    );

    mint(&fix.env, &fix.token, &fix.buyer, 1000);
    mint(&fix.env, &token_b, &fix.buyer, 500);

    fix.client.fund_basket_escrow(&escrow_id, &fix.buyer);

    fix.env
        .ledger()
        .set_timestamp(fix.env.ledger().timestamp() + 200_000);
    fix.client.auto_release(&escrow_id);

    let escrow = fix.client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);

    // Primary token and basket token both paid to first payee (seller)
    assert_eq!(balance(&fix.env, &fix.token, &fix.seller), 1000);
    assert_eq!(balance(&fix.env, &token_b, &fix.seller), 500);
}

#[test]
fn test_auto_release_permissionless_caller() {
    let fix = setup_fixture();
    let _random_caller = Address::generate(&fix.env);
    let amount = 1000_i128;

    let seller_val = fix.seller.clone().into_val(&fix.env);
    let escrow_id = fix.client.create_escrow_8(
        &seller_val,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &fix.token,
        &amount,
        &0_u32,
        &3600_u64,
    );

    mint(&fix.env, &fix.token, &fix.buyer, amount);
    fix.client.fund_escrow(&escrow_id, &fix.buyer);

    fix.env
        .ledger()
        .set_timestamp(fix.env.ledger().timestamp() + 200_000);

    // Completely random 3rd party caller can trigger auto_release
    let res = fix.client.try_auto_release(&escrow_id);
    assert!(res.is_ok());

    let escrow = fix.client.get_escrow(&escrow_id);
    assert_eq!(escrow.state, EscrowState::Completed);
}

#[test]
fn test_auto_release_emits_event() {
    let fix = setup_fixture();
    let amount = 1234_i128;

    let seller_val = fix.seller.clone().into_val(&fix.env);
    let escrow_id = fix.client.create_escrow_8(
        &seller_val,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &fix.token,
        &amount,
        &50_u32,
        &3600_u64,
    );

    mint(&fix.env, &fix.token, &fix.buyer, amount);
    fix.client.fund_escrow(&escrow_id, &fix.buyer);

    fix.env
        .ledger()
        .set_timestamp(fix.env.ledger().timestamp() + 200_000);
    fix.client.auto_release(&escrow_id);

    let all_events = fix.env.events().all();
    let binding = all_events.filter_by_contract(&fix.client.address);
    let contract_events = binding.events();
    let auto_released_found = contract_events.iter().any(|event| match &event.body {
        soroban_sdk::xdr::ContractEventBody::V0(v0) => {
            let Ok(data) = Val::try_from_val(&fix.env, &v0.data) else {
                return false;
            };
            crate::AutoReleased::try_from_val(&fix.env, &data).is_ok()
        }
    });
    assert!(auto_released_found, "AutoReleased event must be emitted");
}

#[test]
fn test_auto_release_invalid_states_rejected() {
    let fix = setup_fixture();
    let amount = 1000_i128;
    let seller_val = fix.seller.clone().into_val(&fix.env);
    let escrow_id = fix.client.create_escrow_8(
        &seller_val,
        &Some(fix.buyer.clone()),
        &fix.resolver,
        &fix.token,
        &amount,
        &0_u32,
        &3600_u64,
    );

    // 1. Pending (unfunded) state:
    assert_eq!(
        fix.client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::InvalidState))
    );

    // Fund it
    mint(&fix.env, &fix.token, &fix.buyer, amount);
    fix.client.fund_escrow(&escrow_id, &fix.buyer);

    // 2. Disputed state:
    let reason = Symbol::new(&fix.env, "issue");
    let desc = SorobanString::from_str(&fix.env, "broken");
    let evidence = BytesN::from_array(&fix.env, &[1u8; 32]);
    fix.client
        .raise_dispute(&fix.buyer, &escrow_id, &reason, &desc, &evidence);

    assert_eq!(
        fix.client.try_auto_release(&escrow_id),
        Err(Ok(ContractError::InvalidState))
    );
}

#[test]
fn test_auto_release_non_existent_escrow_rejected() {
    let fix = setup_fixture();
    let missing_id = 999_999_u64;
    assert_eq!(
        fix.client.try_auto_release(&missing_id),
        Err(Ok(ContractError::EscrowNotFound))
    );
}
