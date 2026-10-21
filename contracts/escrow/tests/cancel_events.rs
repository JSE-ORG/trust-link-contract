#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token, Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};
use trustlink_escrow::{DataKey, Escrow, EscrowClient, EscrowData, EscrowState, Payee};

fn has_cancel_event(env: &Env, contract_id: &Address) -> bool {
    let expected_class = Symbol::new(env, "Escrow");
    let expected_action = Symbol::new(env, "Canceled");
    env.events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .iter()
        .any(|event| match &event.body {
            soroban_sdk::xdr::ContractEventBody::V0(v0) => {
                let mut topics = v0.topics.iter();
                let (Some(first), Some(second)) = (topics.next(), topics.next()) else {
                    return false;
                };
                let Ok(first) = Symbol::try_from_val(env, first) else {
                    return false;
                };
                let Ok(second) = Symbol::try_from_val(env, second) else {
                    return false;
                };
                first == expected_class && second == expected_action
            }
        })
}

#[test]
fn seller_pending_cancel_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let seller = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees: Val = payees.into_val(&env);
    let escrow_id = client.create_escrow(
        &payees,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_i128,
        &0_u32,
        &0_u32,
        &3_600_u64,
        &None::<soroban_sdk::String>,
    );

    client.cancel_escrow(&seller, &escrow_id);

    assert!(has_cancel_event(&env, &contract_id));
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Canceled);
}

#[test]
fn buyer_funded_cancel_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(&env, &token);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    sac.mint(&buyer, &1_000_i128);

    let mut payees2 = Vec::new(&env);
    payees2.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees2: Val = payees2.into_val(&env);
    let escrow_id = client.create_escrow(
        &payees2,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_i128,
        &0_u32,
        &0_u32,
        &3_600_u64,
        &None::<soroban_sdk::String>,
    );

    let mut escrow: EscrowData = env
        .as_contract(&contract_id, || {
            env.storage().persistent().get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow exists");
    escrow.buyer = Some(buyer.clone());
    escrow.packed_timestamps =
        EscrowData::pack_timestamps(env.ledger().timestamp(), env.ledger().timestamp());
    escrow.state = EscrowState::Funded;
    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);
    });

    token::StellarAssetClient::new(&env, &token).transfer(&buyer, &contract_id, &1_000_i128);

    client.cancel_escrow(&buyer, &escrow_id);

    assert!(has_cancel_event(&env, &contract_id));
    assert_eq!(client.get_escrow(&escrow_id).state, EscrowState::Refunded);
    assert_eq!(sac.balance(&buyer), 1_000);
    assert_eq!(sac.balance(&contract_id), 0);
}
