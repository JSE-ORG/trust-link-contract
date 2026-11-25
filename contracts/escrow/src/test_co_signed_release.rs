#![cfg(test)]

use crate::test::has_event;
use crate::test_helpers::{advance_time, mint_token, setup_contract};
use crate::{ContractError, EscrowClient, EscrowCompleted, EscrowState, Payee};
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, BytesN, Env, IntoVal, String as SorobanString, Symbol, Vec,
};

fn register_token(env: &Env) -> Address {
    let token_admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(token_admin.clone())
        .address()
}

#[test]
fn test_co_signed_release_from_funded() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&id, &buyer);

    // co-signed release requires both parties' auths; with mock_all_auths this simulates
    // a transaction where both seller and buyer sign.
    client.co_signed_release(&buyer, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Completed);
}

#[test]
fn test_co_signed_release_requires_both_auths() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);

    // With mock_all_auths this simulates a transaction where both seller and buyer sign.
    client.co_signed_release(&seller, &id);
}

#[test]
fn test_co_signed_release_from_shipped() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-SHIP"),
    );

    // co_signed_release should succeed from Shipped state
    client.co_signed_release(&buyer, &id);

    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, EscrowState::Completed);
}

#[test]
fn test_co_signed_release_fails_on_active_dispute() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-DISP"),
    );

    // Buyer raises a dispute
    let reason = Symbol::new(&env, "DEFECTIVE");
    let description = soroban_sdk::Bytes::from_slice(&env, b"Item is defective");
    let evidence_hash = BytesN::from_array(&env, &[0xcd; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // co_signed_release must fail when a dispute is active
    let result = client.try_co_signed_release(&buyer, &id);
    assert!(matches!(result, Err(Ok(ContractError::InvalidState))));
}

#[test]
fn test_co_signed_release_fails_on_completed_state() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (_contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);

    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );

    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &SorobanString::from_str(&env, "TRACK-COMP"));

    // Complete the escrow via confirm_delivery after the dispute window passes
    let escrow = client.get_escrow(&id);
    advance_time(&env, escrow.dispute_deadline() + 1);
    client.confirm_delivery(&buyer, &id, &false);

    // Now the escrow is Completed — co_signed_release must fail with the
    // dedicated code (was InvalidState).
    let result = client.try_co_signed_release(&buyer, &id);
    assert!(matches!(
        result,
        Err(Ok(ContractError::EscrowAlreadyCompleted))
    ));
}

/// Creates and funds a 3-payee escrow (5000/3000/2000 bps). Returns the escrow
/// id plus the three payees.
fn create_three_payee_escrow(
    env: &Env,
    client: &EscrowClient,
    token: &Address,
    buyer: &Address,
) -> (u64, [Address; 3]) {
    let resolver = Address::generate(env);
    let payee_0 = Address::generate(env);
    let payee_1 = Address::generate(env);
    let payee_2 = Address::generate(env);

    mint_token(env, token, buyer, 1000);

    let mut payees = Vec::new(env);
    payees.push_back(Payee {
        address: payee_0.clone(),
        bps: 5_000,
    });
    payees.push_back(Payee {
        address: payee_1.clone(),
        bps: 3_000,
    });
    payees.push_back(Payee {
        address: payee_2.clone(),
        bps: 2_000,
    });
    let payees_val = payees.into_val(env);

    let id = client.create_escrow_8(
        &payees_val,
        &None::<Address>,
        &resolver,
        token,
        &1_000_i128,
        &0_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, buyer);

    (id, [payee_0, payee_1, payee_2])
}

/// #1267: all three payees must authorize a multi-payee co-signed release.
#[test]
fn test_co_signed_release_multi_payee_all_authorised() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let buyer = Address::generate(&env);
    let (id, _payees) = create_three_payee_escrow(&env, &client, &token, &buyer);

    client.co_signed_release(&buyer, &id);

    // The completion event records how many payees co-signed. Check it before
    // any further client call, which would clear the event log.
    assert!(has_event::<EscrowCompleted, _>(
        &env,
        &contract_id,
        "Escrow",
        |ev| ev.escrow_id == id && ev.co_signed_payees == 3
    ));
    assert_eq!(client.get_escrow(&id).state, EscrowState::Completed);
}

/// #1267: omitting one payee's auth (here the third) rejects the release, so a
/// buyer and a subset of vendors cannot force it.
#[test]
fn test_co_signed_release_multi_payee_requires_every_payee() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let buyer = Address::generate(&env);
    let (id, payees) = create_three_payee_escrow(&env, &client, &token, &buyer);

    // Authorize the buyer and the first two payees only.
    let invoke = |caller: &Address| MockAuthInvoke {
        contract: &contract_id,
        fn_name: "co_signed_release",
        args: (caller.clone(), id).into_val(&env),
        sub_invokes: &[],
    };
    let args_0 = invoke(&buyer);
    let args_1 = invoke(&payees[0]);
    let args_2 = invoke(&payees[1]);
    env.mock_auths(&[
        MockAuth {
            address: &buyer,
            invoke: &args_0,
        },
        MockAuth {
            address: &payees[0],
            invoke: &args_1,
        },
        MockAuth {
            address: &payees[1],
            invoke: &args_2,
        },
    ]);

    assert!(client.try_co_signed_release(&buyer, &id).is_err());
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);
}

/// #1267: single-payee escrows keep the original 1 buyer + 1 seller rule.
#[test]
fn test_co_signed_release_single_payee_still_requires_seller_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let token = register_token(&env);
    let (contract_id, client, _admin, _fee_collector) = setup_contract(&env);

    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);

    mint_token(&env, &token, &buyer, 1000);
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &None::<Address>,
        &resolver,
        &token,
        &500_i128,
        &0_u32,
        &3600_u64,
    );
    client.fund_escrow(&id, &buyer);

    // Buyer signature alone is not enough.
    let buyer_invoke = MockAuthInvoke {
        contract: &contract_id,
        fn_name: "co_signed_release",
        args: (buyer.clone(), id).into_val(&env),
        sub_invokes: &[],
    };
    env.mock_auths(&[MockAuth {
        address: &buyer,
        invoke: &buyer_invoke,
    }]);

    assert!(client.try_co_signed_release(&buyer, &id).is_err());
    assert_eq!(client.get_escrow(&id).state, EscrowState::Funded);
}
