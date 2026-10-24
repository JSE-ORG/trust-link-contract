#![cfg(test)]
//! Tests proving that admin can resolve disputes, not just the resolver.
//!
//! Covers: create → fund → ship → raise_dispute → resolve_dispute by admin.
//! After resolution the escrow must transition correctly and payouts must be accurate.

use crate::{DataKey, DisputeData, DisputeStatus, Escrow, EscrowClient, EscrowData, EscrowState, ResolutionType};
use soroban_sdk::{
    testutils::Address as _,
    token, Address, BytesN, Env, String, Symbol,
};

#[test]
fn admin_can_resolve_dispute_with_release() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    // SAC token used to fund the buyer + receive the seller payout.
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let arbitration_fee: u32 = 50;
    client.initialize(&admin, &fee_collector, &arbitration_fee);

    let amount: i128 = 1_000;
    // shipping_window = 0 so `mark_shipped` is permitted immediately.
    // fee_bps = 0 isolates the arbitration-fee accounting.
    let escrow_id = client.create_escrow(&seller, &None::<Address>, &resolver, &token_address, &amount, &0_u32, &0_u64);

    // Fund the buyer and the escrow.
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
    token_admin_client.mint(&buyer, &amount);
    client.fund_escrow(&escrow_id, &buyer);

    // Seller marks shipped.
    let tracking_id = String::from_str(&env, "TRK-ADMIN-001");
    client.mark_shipped(&seller, &escrow_id, &tracking_id);

    // Buyer raises a dispute.
    let reason = Symbol::new(&env, "non_delivery");
    let description = String::from_str(&env, "Item never arrived");
    let evidence = BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &escrow_id, &reason, &description, &evidence);

    // Verify state is Disputed before resolution.
    let before: EscrowData = env
        .as_contract(&contract_id, || env.storage().persistent().get(&DataKey::Escrow(escrow_id)))
        .expect("escrow exists");
    assert_eq!(before.state, EscrowState::Disputed);

    // Admin resolves the dispute in favor of vendor (Release).
    client.resolve_dispute(&admin, &escrow_id, &ResolutionType::Release);

    // ── Post-resolution assertions ─────────────────────────────────────────
    let token_client = token::TokenClient::new(&env, &token_address);

    // Vendor received the net amount (face value minus the arbitration fee).
    assert_eq!(
        token_client.balance(&seller),
        amount - 5,
        "seller should receive amount minus arbitration fee on Release",
    );

    // Buyer received no refund.
    assert_eq!(
        token_client.balance(&buyer),
        0,
        "buyer should not be refunded on a vendor-wins resolution",
    );

    // Escrow state advanced to Completed.
    let after: EscrowData = env
        .as_contract(&contract_id, || env.storage().persistent().get(&DataKey::Escrow(escrow_id)))
        .expect("escrow exists");
    assert_eq!(after.state, EscrowState::Completed);

    // Dispute record is marked Resolved.
    let dispute: DisputeData = env
        .as_contract(&contract_id, || env.storage().persistent().get(&DataKey::Dispute(escrow_id)))
        .expect("dispute exists");
    assert_eq!(dispute.status, DisputeStatus::Resolved);
}

#[test]
fn admin_can_resolve_dispute_with_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    // SAC token used to fund the buyer + receive the seller payout.
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token_address = sac.address();

    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    let arbitration_fee: u32 = 50;
    client.initialize(&admin, &fee_collector, &arbitration_fee);

    let amount: i128 = 1_000;
    // shipping_window = 0 so `mark_shipped` is permitted immediately.
    // fee_bps = 0 isolates the arbitration-fee accounting.
    let escrow_id = client.create_escrow(&seller, &None::<Address>, &resolver, &token_address, &amount, &0_u32, &0_u64);

    // Fund the buyer and the escrow.
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
    token_admin_client.mint(&buyer, &amount);
    client.fund_escrow(&escrow_id, &buyer);

    // Seller marks shipped.
    let tracking_id = String::from_str(&env, "TRK-ADMIN-002");
    client.mark_shipped(&seller, &escrow_id, &tracking_id);

    // Buyer raises a dispute.
    let reason = Symbol::new(&env, "damaged_item");
    let description = String::from_str(&env, "Item arrived damaged");
    let evidence = BytesN::from_array(&env, &[0xcd; 32]);
    client.raise_dispute(&buyer, &escrow_id, &reason, &description, &evidence);

    // Verify state is Disputed before resolution.
    let before: EscrowData = env
        .as_contract(&contract_id, || env.storage().persistent().get(&DataKey::Escrow(escrow_id)))
        .expect("escrow exists");
    assert_eq!(before.state, EscrowState::Disputed);

    // Admin resolves the dispute in favor of buyer (Refund).
    client.resolve_dispute(&admin, &escrow_id, &ResolutionType::Refund);

    // ── Post-resolution assertions ─────────────────────────────────────────
    let token_client = token::TokenClient::new(&env, &token_address);

    // Buyer received the net amount (face value minus the arbitration fee).
    assert_eq!(
        token_client.balance(&buyer),
        amount - 5,
        "buyer should receive amount minus arbitration fee on Refund",
    );

    // Seller received no payout.
    assert_eq!(
        token_client.balance(&seller),
        0,
        "seller should not be refunded on a buyer-wins resolution",
    );

    // Escrow state advanced to Refunded.
    let after: EscrowData = env
        .as_contract(&contract_id, || env.storage().persistent().get(&DataKey::Escrow(escrow_id)))
        .expect("escrow exists");
    assert_eq!(after.state, EscrowState::Refunded);

    // Dispute record is marked Resolved.
    let dispute: DisputeData = env
        .as_contract(&contract_id, || env.storage().persistent().get(&DataKey::Dispute(escrow_id)))
        .expect("dispute exists");
    assert_eq!(dispute.status, DisputeStatus::Resolved);
}
