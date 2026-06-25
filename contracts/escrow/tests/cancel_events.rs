#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token, Address, Env, Symbol, TryFromVal,
};
use trustlink_escrow::{DataKey, Escrow, EscrowClient, EscrowData, EscrowState};

fn has_cancel_event(env: &Env, contract_id: &Address) -> bool {
    let expected_topic = Symbol::new(env, "escrow_cancelled");
    env.events()
        .all()
        .filter_by_contract(contract_id)
        .events()
        .iter()
        .any(|event| match &event.body {
            soroban_sdk::xdr::ContractEventBody::V0(v0) => {
                let Some(topic) = v0.topics.iter().next() else {
                    return false;
                };
                let Ok(topic) = Symbol::try_from_val(env, topic) else {
                    return false;
                };
                topic == expected_topic
            }
            _ => false,
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

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_i128,
        &0_u32,
        &3_600_u64,
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

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token,
        &1_000_i128,
        &0_u32,
        &3_600_u64,
    );

    let mut escrow: EscrowData = env
        .as_contract(&contract_id, || {
            env.storage().persistent().get(&DataKey::Escrow(escrow_id))
        })
        .expect("escrow exists");
    escrow.buyer = Some(buyer.clone());
    escrow.funded_at = env.ledger().timestamp();
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
