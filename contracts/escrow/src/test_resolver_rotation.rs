#![cfg(test)]
//! Tests for `rotate_resolver`: seller and admin can rotate, buyer cannot,
//! same-address is rejected, and terminal states are rejected.

use crate::{ContractError, Escrow, EscrowClient, EscrowState, ResolutionType, ResolverRotated};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token, Address, BytesN, Env, String as SorobanString, Symbol, TryFromVal, Val,
};

struct Fx {
    env: Env,
    client: EscrowClient<'static>,
    admin: Address,
    seller: Address,
    buyer: Address,
    resolver: Address,
    escrow_id: u64,
}

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
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer, &1_000_i128);

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &500_i128,
        &0_u32,
        &0_u64,
    );

    Fx {
        env,
        client,
        admin,
        seller,
        buyer,
        resolver,
        escrow_id,
    }
}

#[test]
fn seller_can_rotate_resolver() {
    let fx = setup();
    let new_resolver = Address::generate(&fx.env);

    fx.client
        .rotate_resolver(&fx.seller, &fx.escrow_id, &new_resolver);

    use crate::{DataKey, EscrowData};
    let escrow: EscrowData = fx
        .env
        .as_contract(&fx.client.address, || {
            fx.env
                .storage()
                .persistent()
                .get(&DataKey::Escrow(fx.escrow_id))
        })
        .expect("escrow exists");
    assert_eq!(escrow.resolver, new_resolver);
}

#[test]
fn admin_can_rotate_resolver() {
    let fx = setup();
    let new_resolver = Address::generate(&fx.env);

    fx.client
        .rotate_resolver(&fx.admin, &fx.escrow_id, &new_resolver);

    use crate::{DataKey, EscrowData};
    let escrow: EscrowData = fx
        .env
        .as_contract(&fx.client.address, || {
            fx.env
                .storage()
                .persistent()
                .get(&DataKey::Escrow(fx.escrow_id))
        })
        .expect("escrow exists");
    assert_eq!(escrow.resolver, new_resolver);
}

#[test]
fn buyer_cannot_rotate_resolver() {
    let fx = setup();
    fx.client.fund_escrow(&fx.escrow_id, &fx.buyer);

    let new_resolver = Address::generate(&fx.env);
    let result = fx
        .client
        .try_rotate_resolver(&fx.buyer, &fx.escrow_id, &new_resolver);
    assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
}

#[test]
fn same_address_rejected() {
    let fx = setup();
    let result = fx
        .client
        .try_rotate_resolver(&fx.seller, &fx.escrow_id, &fx.resolver);
    assert_eq!(result, Err(Ok(ContractError::SameAddress)));
}

#[test]
fn new_resolver_cannot_be_seller() {
    let fx = setup();
    // resolver != seller (both generated independently), so passing seller as
    // new_resolver hits the InvalidAddress guard, not SameAddress.
    let result = fx
        .client
        .try_rotate_resolver(&fx.admin, &fx.escrow_id, &fx.seller);
    assert_eq!(result, Err(Ok(ContractError::InvalidAddress)));
}

#[test]
fn new_resolver_cannot_be_buyer() {
    let fx = setup();
    fx.client.fund_escrow(&fx.escrow_id, &fx.buyer);

    let result = fx
        .client
        .try_rotate_resolver(&fx.seller, &fx.escrow_id, &fx.buyer);
    assert_eq!(result, Err(Ok(ContractError::InvalidAddress)));
}

#[test]
fn terminal_state_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let fee_collector = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let escrow_id = client.create_escrow(
        &seller,
        &None::<Address>,
        &resolver,
        &token_addr,
        &100_i128,
        &0_u32,
        &0_u64,
    );

    // Cancel moves to Canceled (terminal)
    client.cancel_escrow(&seller, &escrow_id);

    let new_resolver = Address::generate(&env);
    let result = client.try_rotate_resolver(&seller, &escrow_id, &new_resolver);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}

/// Returns true if the contract emitted a `resolver_rotated` event whose
/// `old_resolver`/`new_resolver` match the expected addresses.
fn resolver_rotated_emitted(fx: &Fx, old: &Address, new: &Address) -> bool {
    let expected_topic = Symbol::new(&fx.env, "resolver_rotated");
    fx.env
        .events()
        .all()
        .filter_by_contract(&fx.client.address)
        .events()
        .iter()
        .any(|event| match &event.body {
            soroban_sdk::xdr::ContractEventBody::V0(v0) => {
                let Some(topic) = v0.topics.iter().next() else {
                    return false;
                };
                let Ok(topic) = Symbol::try_from_val(&fx.env, topic) else {
                    return false;
                };
                if topic != expected_topic {
                    return false;
                }
                let Ok(data) = Val::try_from_val(&fx.env, &v0.data) else {
                    return false;
                };
                ResolverRotated::try_from_val(&fx.env, &data)
                    .map(|ev| &ev.old_resolver == old && &ev.new_resolver == new)
                    .unwrap_or(false)
            }
            _ => false,
        })
}

/// Drives the escrow in the fixture all the way to the `Disputed` state.
fn drive_to_dispute(fx: &Fx) {
    fx.client.fund_escrow(&fx.escrow_id, &fx.buyer);
    fx.client
        .mark_shipped(&fx.seller, &fx.escrow_id, &SorobanString::from_str(&fx.env, "TRK-ROT"));
    fx.client.raise_dispute(
        &fx.buyer,
        &fx.escrow_id,
        &Symbol::new(&fx.env, "broken"),
        &SorobanString::from_str(&fx.env, "item arrived damaged"),
        &BytesN::from_array(&fx.env, &[1u8; 32]),
    );
}

/// Issue #: the admin must be able to rotate the resolver while a dispute is
/// active (i.e. before it is resolved). State remains `Disputed`.
#[test]
fn admin_can_rotate_resolver_during_active_dispute() {
    let fx = setup();
    drive_to_dispute(&fx);
    assert_eq!(fx.client.get_escrow(&fx.escrow_id).state, EscrowState::Disputed);

    let new_resolver = Address::generate(&fx.env);
    fx.client.rotate_resolver(&fx.admin, &fx.escrow_id, &new_resolver);

    let escrow = fx.client.get_escrow(&fx.escrow_id);
    assert_eq!(escrow.resolver, new_resolver);
    // Rotation does not change the lifecycle state.
    assert_eq!(escrow.state, EscrowState::Disputed);
}

/// Acceptance: rotation is only allowed before resolution. Once the dispute is
/// resolved the escrow is terminal and rotation is rejected.
#[test]
fn rotation_rejected_after_dispute_resolved() {
    let fx = setup();
    drive_to_dispute(&fx);

    // Resolve the dispute in the seller's favour → Completed (terminal).
    fx.client.resolve_dispute(&fx.admin, &fx.escrow_id, &ResolutionType::Release);
    assert_eq!(fx.client.get_escrow(&fx.escrow_id).state, EscrowState::Completed);

    let new_resolver = Address::generate(&fx.env);
    let result = fx.client.try_rotate_resolver(&fx.admin, &fx.escrow_id, &new_resolver);
    assert_eq!(result, Err(Ok(ContractError::InvalidState)));
}

/// Acceptance: a `resolver_rotated` event is emitted carrying the old and new
/// resolver addresses.
#[test]
fn rotation_emits_resolver_rotated_event() {
    let fx = setup();
    drive_to_dispute(&fx);

    let new_resolver = Address::generate(&fx.env);
    fx.client.rotate_resolver(&fx.admin, &fx.escrow_id, &new_resolver);

    assert!(
        resolver_rotated_emitted(&fx, &fx.resolver, &new_resolver),
        "expected a resolver_rotated event with the old and new resolver",
    );
}
