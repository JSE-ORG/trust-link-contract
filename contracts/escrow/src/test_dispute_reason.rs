#![cfg(test)]

use crate::test_helpers::{create_funded_escrow, setup_contract};
use crate::{ContractError, EscrowClient, EscrowState, DISPUTE_REASONS};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String as SorobanString, Symbol};

fn setup(env: &Env) -> (EscrowClient<'_>, Address, u64) {
    env.mock_all_auths();
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(env);
    let seller = Address::generate(env);
    let buyer = Address::generate(env);
    let resolver = Address::generate(env);
    let id = create_funded_escrow(
        env, &client, &seller, &buyer, &resolver, &token, 100, 0, 3600,
    );
    (client, buyer, id)
}

fn try_raise(
    env: &Env,
    client: &EscrowClient,
    buyer: &Address,
    id: u64,
    reason: &str,
) -> Result<(), ContractError> {
    match client.try_raise_dispute(
        buyer,
        &id,
        &Symbol::new(env, reason),
        &SorobanString::from_str(env, "desc"),
        &BytesN::from_array(env, &[0u8; 32]),
    ) {
        Ok(_) => Ok(()),
        Err(Ok(e)) => Err(e),
        Err(Err(e)) => panic!("unexpected host error: {e:?}"),
    }
}

#[test]
fn every_predefined_reason_is_accepted_and_stored() {
    for reason in DISPUTE_REASONS {
        let env = Env::default();
        let (client, buyer, id) = setup(&env);
        assert_eq!(try_raise(&env, &client, &buyer, id, reason), Ok(()));
        assert_eq!(client.get_escrow(&id).state, EscrowState::Disputed);
        assert_eq!(
            client.get_dispute(&id).unwrap().reason,
            Symbol::new(&env, reason)
        );
    }
}

#[test]
fn unknown_reason_is_rejected() {
    let env = Env::default();
    let (client, buyer, id) = setup(&env);
    assert_eq!(
        try_raise(&env, &client, &buyer, id, "made_up_reason"),
        Err(ContractError::InvalidDisputeReason)
    );
    // The escrow must be left untouched so a valid dispute can still be raised.
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);
    assert_eq!(try_raise(&env, &client, &buyer, id, "OTHER"), Ok(()));
}

#[test]
fn reason_match_is_case_sensitive() {
    let env = Env::default();
    let (client, buyer, id) = setup(&env);
    assert_eq!(
        try_raise(&env, &client, &buyer, id, "damaged"),
        Err(ContractError::InvalidDisputeReason)
    );
}
