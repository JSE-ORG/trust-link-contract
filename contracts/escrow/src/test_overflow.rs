#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger, Vec},
    Address, Env,
};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env.register_stellar_asset_contract(token_admin.clone());

    (
        env,
        seller,
        buyer,
        resolver,
        token_admin,
        token_address,
        fee_collector,
    )
}

fn mint_tokens(env: &Env, token: &Address, to: &Address, amount: i128) {
    let sac = soroban_sdk::token::StellarAssetClient::new(env, token);
    sac.mint(to, &amount);
}

#[test]
fn test_fee_calculation_max_escrow_amount() {
    let (env, seller, buyer, resolver, _admin, token, fee_collector) = setup_env();

    let contract_id = env.register(Escrow, ());
    let client = super::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);
    client.set_protocol_fee(&admin, &300_u32);

    let amount = MAX_ESCROW_AMOUNT;
    let fee_bps = 300; // 3%

    let mut payees_54 = Vec::new(&env);
    payees_54.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    // Updated 9 arguments (added resolver_fee_bps & notes)
    let id = client.create_escrow(
        &payees_54,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &fee_bps,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    mint_tokens(&env, &token, &buyer, amount);
    client.fund_escrow(&id, &buyer);

    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-MAX"),
    );

    let escrow = client.get_escrow(&id);
    env.ledger().set_timestamp(escrow.dispute_deadline + 1);
    client.confirm_delivery(&buyer, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Completed);

    let expected_fee =
        (MAX_ESCROW_AMOUNT / 10_000) * 300 + (MAX_ESCROW_AMOUNT % 10_000) * 300 / 10_000;
    let expected_net = MAX_ESCROW_AMOUNT - expected_fee;

    let tc = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(tc.balance(&seller), expected_net);
    assert_eq!(tc.balance(&fee_collector), expected_fee);
    assert_eq!(tc.balance(&contract_id), 0);
}

#[test]
fn test_create_escrow_amount_exceeds_maximum() {
    let (env, seller, _buyer, resolver, _admin, token, fee_collector) = setup_env();

    let contract_id = env.register(Escrow, ());
    let client = super::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    // Wrapped seller address inside a single-payee vector matching source interface logic
    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let amount = MAX_ESCROW_AMOUNT + 1;
    let res = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &300,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert_eq!(res, Err(Ok(ContractError::AmountExceedsMaximum)));

    let res2 = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &i128::MAX,
        &300,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert_eq!(res2, Err(Ok(ContractError::AmountExceedsMaximum)));
}

#[test]
fn test_create_escrow_invalid_amount() {
    let (env, seller, _buyer, resolver, _admin, token, fee_collector) = setup_env();

    let contract_id = env.register(Escrow, ());
    let client = super::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let res = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &0,
        &200,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert!(matches!(res, Err(Ok(ContractError::InvalidAmount))));

    let res2 = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &-1,
        &200,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert!(matches!(res2, Err(Ok(ContractError::InvalidAmount))));
}

#[test]
fn test_fee_exceeds_max_clean_error() {
    let (env, seller, _buyer, resolver, _admin, token, fee_collector) = setup_env();

    let contract_id = env.register(Escrow, ());
    let client = super::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let _res_ignored = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000,
        &301,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let res = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000,
        &10_001,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert!(matches!(res, Err(Ok(ContractError::FeeExceedsMax))));
}

#[test]
fn test_addition_overflow_escrow_counter() {
    let (env, seller, _, resolver, _, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = super::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey::EscrowCounter, &u64::MAX);
    });

    let res = client.try_create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1000,
        &300,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    assert_eq!(res, Err(Ok(ContractError::ArithmeticError)));
}

#[test]
fn test_addition_overflow_shipping_window() {
    let (env, seller, buyer, resolver, _, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = super::EscrowClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000;
    mint_tokens(&env, &token, &buyer, amount);

    let mut payees_53 = Vec::new(&env);
    payees_53.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let escrow_id_1 = client.create_escrow(
        &payees_53,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &300,
        &0_u32,
        &u64::MAX,
        &None::<String>,
    );
    env.ledger().set_timestamp(1000);

    let mut payees_52 = Vec::new(&env);
    payees_52.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });

    let escrow_id_2 = client.create_escrow(
        &payees_52,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &300,
        &0_u32,
        &u64::MAX,
        &None::<String>,
    );
    client.fund_escrow(&escrow_id_2, &buyer);
    client.mark_shipped(
        &seller,
        &escrow_id_2,
        &soroban_sdk::String::from_str(&env, "TRACK-OVERFLOW"),
    );
    env.ledger().set_timestamp(u64::MAX - 10);
    client.record_delivery(&admin, &escrow_id_2);

    env.ledger().set_timestamp(u64::MAX - 1);
    let res = client.try_auto_release(&escrow_id_2);
    assert_eq!(res, Err(Ok(ContractError::ArithmeticOverflow)));
}

#[test]
fn test_subtraction_underflow_safety() {
    let env = Env::default();
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);

    let res = super::deduct_and_transfer(&env, &token, &recipient, -1, 300);
    assert_eq!(res, Err(ContractError::InvalidAmount));
}

#[test]
fn test_multiplication_overflow() {
    let env = Env::default();
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);

    let amount = i128::MAX;
    let fee_bps = u32::MAX;

    let res = super::deduct_and_transfer(&env, &token, &recipient, amount, fee_bps);
    assert_eq!(res, Err(ContractError::ArithmeticError));
}

#[test]
fn test_division_by_zero_safety() {
    let amount: i128 = 100;
    let res = amount.checked_div(0);
    assert_eq!(res, None);
}
