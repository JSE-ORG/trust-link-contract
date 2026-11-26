#![cfg(test)]

use crate::{DisputeStatus, Escrow, EscrowClient, Payee};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::generate(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let fee_collector = Address::generate(&env);

    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    (
        env,
        admin,
        seller,
        buyer,
        resolver,
        token_address,
        fee_collector,
    )
}

#[test]
fn test_get_dispute_returns_accurate_data_after_raise() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let mut payees_22 = Vec::new(&env);
    payees_22.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_22.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let sac = token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    client.fund_escrow(&id, &buyer);

    let reason = Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let description = soroban_sdk::Bytes::from_slice(&env, b"Item never arrived");
    let evidence_hash = BytesN::from_array(&env, &[0xab; 32]);
    let timestamp = env.ledger().timestamp();

    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-001"),
    );
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    let result = client.get_dispute(&id);

    assert!(result.is_some());
    let dispute = result.unwrap();
    assert_eq!(dispute.escrow_id, id);
    assert_eq!(dispute.reason, reason);
    assert_eq!(dispute.description, description);
    assert_eq!(dispute.evidence_hash, evidence_hash);
    assert_eq!(dispute.status, DisputeStatus::Active);
    assert!(dispute.disputed_at >= timestamp);
}

#[test]
fn test_get_dispute_returns_none_when_no_dispute_exists() {
    let (env, admin, _seller, _buyer, _resolver, _token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    // Query for a dispute on an escrow ID that has no dispute
    let result = client.get_dispute(&999);
    assert_eq!(result, None);
}

// Verify disputes can be opened once the escrow has reached Shipped.
#[test]
fn test_dispute_allowed_after_shipping() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let mut payees_21 = Vec::new(&env);
    payees_21.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_21.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    // Use fixed deterministic timestamp
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-BOUNDARY"),
    );

    env.ledger().set_timestamp(1_700_172_798);

    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);

    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);
    let result = client.get_dispute(&id);
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.status, crate::DisputeStatus::Active);
}

// Verify disputes are still accepted on a later shipped escrow timestamp.
#[test]
fn test_dispute_allowed_on_late_shipped_escrow() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let mut payees_20 = Vec::new(&env);
    payees_20.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_20.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    // Use fixed deterministic timestamp
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(
        &seller,
        &id,
        &soroban_sdk::String::from_str(&env, "TRACK-LATE"),
    );
    env.ledger().set_timestamp(1_700_172_799);

    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);
    let result = client.get_dispute(&id);
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result.status, crate::DisputeStatus::Active);
}

// Verify disputes require the escrow to be shipped.
#[test]
fn test_dispute_requires_shipped_state() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let mut payees_19 = Vec::new(&env);
    payees_19.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_19.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);

    let escrow = client.get_escrow(&id);
    let funded_at = escrow.funded_at(); // Should be 1_700_000_000

    // T + 172800 seconds (48:00:00) - EXACTLY 48 HOURS
    env.ledger().set_timestamp(funded_at + 172_800);

    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);

    let result = client.try_raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);
    assert_eq!(
        result,
        Err(Ok(crate::ContractError::DisputeWindowStillOpen))
    );

    // Verify no state mutation on expired action
    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, crate::EscrowState::Funded);
}

// Verify dispute actions remain invalid after the 48-hour deadline passes.
#[test]
fn test_dispute_rejected_after_48h_deadline() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let mut payees_18 = Vec::new(&env);
    payees_18.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_18.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    // Use fixed deterministic timestamp
    client.fund_escrow(&id, &buyer);

    let escrow = client.get_escrow(&id);
    let funded_at = escrow.funded_at(); // Should be 1_700_000_000

    // T + 172801 seconds - AFTER DEADLINE
    env.ledger().set_timestamp(funded_at + 172_801);

    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);

    let result = client.try_raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);
    assert_eq!(
        result,
        Err(Ok(crate::ContractError::DisputeWindowStillOpen))
    );

    // Verify no state mutation on expired action
    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, crate::EscrowState::Funded);
}

// Regression test for issue #200: Verify buyers can raise dispute from Funded state
#[test]
fn test_dispute_from_funded_state() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let mut payees_17 = Vec::new(&env);
    payees_17.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees_17.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);

    // Fund the escrow but don't mark as shipped
    client.fund_escrow(&id, &buyer);

    // Verify state is Funded
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, crate::EscrowState::Funded);

    // Buyer should be able to raise dispute from Funded state
    let reason = soroban_sdk::Symbol::new(&env, "ITEM_NOT_RECEIVED");
    let description = soroban_sdk::Bytes::from_slice(&env, b"Seller never shipped the item");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xcd; 32]);

    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // Verify dispute was raised successfully
    let dispute = client.get_dispute(&id);
    assert!(dispute.is_some());
    let dispute = dispute.unwrap();
    assert_eq!(dispute.status, crate::DisputeStatus::Active);
    assert_eq!(dispute.reason, reason);

    // Verify state transitioned to Disputed
    let escrow_after = client.get_escrow(&id);
    assert_eq!(escrow_after.state, crate::EscrowState::Disputed);
}

#[test]
fn test_dispute_from_pending_state() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &100_u32,
        &3600_u64,
    );

    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);

    // Attempt dispute from Pending
    let result = client.try_raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);
    assert_eq!(result, Err(Ok(crate::ContractError::InvalidState)));
}

#[test]
fn test_dispute_from_canceled_state() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &100_u32,
        &3600_u64,
    );

    client.cancel_escrow(&seller, &id);

    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);

    // Attempt dispute from Canceled
    let result = client.try_raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);
    assert_eq!(result, Err(Ok(crate::ContractError::InvalidState)));
}

#[test]
fn test_dispute_from_completed_state() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &100_u32,
        &3600_u64,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &soroban_sdk::String::from_str(&env, "TRK"));

    // Force completion by confirm_delivery
    env.ledger().set_timestamp(1_700_172_801);
    client.confirm_delivery(&buyer, &id, &false);

    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);

    // Attempt dispute from Completed — dedicated code (was InvalidState).
    let result = client.try_raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);
    assert_eq!(
        result,
        Err(Ok(crate::ContractError::EscrowAlreadyCompleted))
    );
}

#[test]
fn test_dispute_from_refunded_state() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &100_u32,
        &3600_u64,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);

    // To refund, dispute then resolve with refund.
    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);

    // Attempt dispute from Refunded
    let result = client.try_raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);
    assert_eq!(result, Err(Ok(crate::ContractError::InvalidState)));
}

#[test]
fn test_appeal_up_to_limit_happy_path() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &100_u32,
        &3600_u64,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);

    // Raise dispute
    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // Appeal 1
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);
    client.appeal_dispute(&buyer, &id);

    // Appeal 2
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);
    client.appeal_dispute(&seller, &id);

    // Appeal 3
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);
    client.appeal_dispute(&buyer, &id);

    // Verify it is back in Disputed state and appeal_count is 3
    let dispute = client.get_dispute(&id).unwrap();
    assert_eq!(dispute.appeal_count, 3);
    let escrow = client.get_escrow(&id);
    assert_eq!(escrow.state, crate::EscrowState::Disputed);
}

#[test]
fn test_appeal_exceeding_limit_fails() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &100_u32,
        &3600_u64,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);

    // Raise dispute
    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    // Appeal 1
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);
    client.appeal_dispute(&buyer, &id);

    // Appeal 2
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);
    client.appeal_dispute(&seller, &id);

    // Appeal 3
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);
    client.appeal_dispute(&buyer, &id);

    // Resolve again (PendingFinalization)
    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);

    // Appeal 4 (fails)
    let result = client.try_appeal_dispute(&buyer, &id);
    assert_eq!(result, Err(Ok(crate::ContractError::MaxAppealsReached)));
}

#[test]
fn test_appeal_non_participant_fails() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(crate::Escrow, ());
    let client = crate::EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let seller_val = seller.clone().into_val(&env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        &resolver,
        &token,
        &amount,
        &100_u32,
        &3600_u64,
    );

    let sac = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    sac.mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);

    // Raise dispute
    let reason = soroban_sdk::Symbol::new(&env, "OTHER");
    let description = soroban_sdk::Bytes::from_slice(&env, b"desc");
    let evidence_hash = soroban_sdk::BytesN::from_array(&env, &[0xab; 32]);
    client.raise_dispute(&buyer, &id, &reason, &description, &evidence_hash);

    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Refund);

    // A random non-participant tries to appeal
    let intruder = Address::generate(&env);
    let result = client.try_appeal_dispute(&intruder, &id);
    assert_eq!(result, Err(Ok(crate::ContractError::NotAuthorized)));
}

// ── submit_dispute_evidence ─────────────────────────────────────────────────

struct EvidenceFx {
    env: Env,
    client: EscrowClient<'static>,
    id: u64,
    seller: Address,
    buyer: Address,
    resolver: Address,
}

/// Builds a funded, shipped and disputed escrow (buyer raised the dispute).
fn disputed_fixture() -> EvidenceFx {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let amount = 1000_i128;
    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &amount,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &amount);
    client.fund_escrow(&id, &buyer);
    client.mark_shipped(&seller, &id, &String::from_str(&env, "TRACK-EV"));
    client.raise_dispute(
        &buyer,
        &id,
        &Symbol::new(&env, "ITEM_NOT_RECEIVED"),
        &soroban_sdk::Bytes::from_slice(&env, b"Item never arrived"),
        &BytesN::from_array(&env, &[0xab; 32]),
    );

    EvidenceFx {
        env,
        client,
        id,
        seller,
        buyer,
        resolver,
    }
}

#[test]
fn seller_can_submit_rebuttal_evidence() {
    let fx = disputed_fixture();
    let hash = BytesN::from_array(&fx.env, &[0xcd; 32]);
    let description = soroban_sdk::Bytes::from_slice(&fx.env, b"Signed delivery receipt");

    fx.client
        .submit_dispute_evidence(&fx.seller, &fx.id, &hash, &description);

    let log = fx.client.get_dispute_evidence(&fx.id);
    assert_eq!(log.len(), 1);
    let entry = log.get(0).unwrap();
    assert_eq!(entry.submitter, fx.seller);
    assert_eq!(entry.evidence_hash, hash);
    assert_eq!(entry.description, description);
    assert_eq!(entry.timestamp, fx.env.ledger().timestamp());

    // The buyer's original evidence on the dispute record is untouched.
    let dispute = fx.client.get_dispute(&fx.id).unwrap();
    let original_hash = BytesN::from_array(&fx.env, &[0xab; 32]);
    assert_eq!(dispute.evidence_hash, original_hash);
}

#[test]
fn buyer_and_resolver_can_submit_evidence() {
    let fx = disputed_fixture();
    let hash = BytesN::from_array(&fx.env, &[0x01; 32]);
    let description = soroban_sdk::Bytes::from_slice(&fx.env, b"more info");

    fx.client
        .submit_dispute_evidence(&fx.buyer, &fx.id, &hash, &description);
    fx.client
        .submit_dispute_evidence(&fx.resolver, &fx.id, &hash, &description);

    let log = fx.client.get_dispute_evidence(&fx.id);
    assert_eq!(log.len(), 2);
    assert_eq!(log.get(0).unwrap().submitter, fx.buyer);
    assert_eq!(log.get(1).unwrap().submitter, fx.resolver);
}

#[test]
fn outsider_cannot_submit_evidence() {
    let fx = disputed_fixture();
    let outsider = Address::generate(&fx.env);
    let result = fx.client.try_submit_dispute_evidence(
        &outsider,
        &fx.id,
        &BytesN::from_array(&fx.env, &[0x02; 32]),
        &soroban_sdk::Bytes::from_slice(&fx.env, b"spam"),
    );
    assert_eq!(result, Err(Ok(crate::ContractError::NotAuthorized)));
    assert_eq!(fx.client.get_dispute_evidence(&fx.id).len(), 0);
}

#[test]
fn evidence_requires_disputed_state() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let mut payees = Vec::new(&env);
    payees.push_back(Payee {
        address: seller.clone(),
        bps: 10_000,
    });
    let payees_val = payees.into_val(&env);
    let id = client.create_escrow(
        &payees_val,
        &None::<Address>,
        &resolver,
        &token,
        &1000_i128,
        &100_u32,
        &0_u32,
        &3600_u64,
        &None::<String>,
    );
    token::StellarAssetClient::new(&env, &token).mint(&buyer, &1000_i128);
    client.fund_escrow(&id, &buyer);

    // Funded, no dispute raised yet.
    let result = client.try_submit_dispute_evidence(
        &seller,
        &id,
        &BytesN::from_array(&env, &[0x03; 32]),
        &soroban_sdk::Bytes::from_slice(&env, b"too early"),
    );
    assert_eq!(result, Err(Ok(crate::ContractError::InvalidState)));
}

#[test]
fn evidence_log_is_capped() {
    let fx = disputed_fixture();
    let hash = BytesN::from_array(&fx.env, &[0x04; 32]);
    let description = soroban_sdk::Bytes::from_slice(&fx.env, b"proof");

    for _ in 0..crate::MAX_EVIDENCE_ENTRIES {
        fx.client
            .submit_dispute_evidence(&fx.seller, &fx.id, &hash, &description);
    }
    let result = fx
        .client
        .try_submit_dispute_evidence(&fx.seller, &fx.id, &hash, &description);
    assert_eq!(result, Err(Ok(crate::ContractError::EvidenceLimitReached)));
    assert_eq!(
        fx.client.get_dispute_evidence(&fx.id).len(),
        crate::MAX_EVIDENCE_ENTRIES
    );
}

/// Issue #1206: an all-zero evidence hash is rejected; non-zero digests pass.
#[test]
fn test_raise_dispute_rejects_all_zero_evidence_hash() {
    use crate::test_helpers::{create_funded_escrow, setup_contract};
    use soroban_sdk::{symbol_short, testutils::Address as _, Address, Bytes, BytesN, Env};

    let env = Env::default();
    env.mock_all_auths();
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let (_cid, client, _admin, _fc) = setup_contract(&env);
    let seller = Address::generate(&env);
    let buyer = Address::generate(&env);
    let resolver = Address::generate(&env);
    let id = create_funded_escrow(
        &env, &client, &seller, &buyer, &resolver, &token, 1_000, 0, 3600,
    );

    let zero = BytesN::from_array(&env, &[0u8; 32]);
    let result = client.try_raise_dispute(
        &buyer,
        &id,
        &symbol_short!("OTHER"),
        &Bytes::from_slice(&env, b"desc"),
        &zero,
    );
    assert_eq!(result, Err(Ok(crate::ContractError::InvalidEvidenceHash)));
    assert_eq!(client.get_escrow(&id).state, crate::EscrowState::Funded);

    // A valid, non-zero digest still succeeds.
    let mut digest = [0u8; 32];
    digest[31] = 1;
    client.raise_dispute(
        &buyer,
        &id,
        &symbol_short!("OTHER"),
        &Bytes::from_slice(&env, b"desc"),
        &BytesN::from_array(&env, &digest),
    );
    assert_eq!(client.get_escrow(&id).state, crate::EscrowState::Disputed);
}

/// Creates a funded, disputed escrow with `amount` and a 1% protocol fee.
fn disputed_escrow_for_split(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    buyer: &Address,
    resolver: &Address,
    token: &Address,
    amount: i128,
) -> u64 {
    let seller_val = seller.clone().into_val(env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        resolver,
        token,
        &amount,
        &100_u32,
        &3600_u64,
    );
    token::StellarAssetClient::new(env, token).mint(buyer, &amount);
    client.fund_escrow(&id, buyer);
    client.raise_dispute(
        buyer,
        &id,
        &Symbol::new(env, "OTHER"),
        &soroban_sdk::Bytes::from_slice(env, b"desc"),
        &BytesN::from_array(env, &[0xab; 32]),
    );
    id
}

#[test]
fn test_split_resolution_50_50_pays_both_sides_with_no_stranded_funds() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    // Odd amount so the 1% fee and the 50/50 split both hit rounding.
    let amount = 10_001_i128;
    let id = disputed_escrow_for_split(&env, &client, &seller, &buyer, &resolver, &token, amount);

    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Split(5_000));
    assert_eq!(
        client.get_escrow(&id).state,
        crate::EscrowState::PendingFinalization
    );
    assert_eq!(
        client.get_dispute(&id).unwrap().get_resolution(),
        Some(crate::ResolutionType::Split(5_000))
    );

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
    client.finalize_dispute(&resolver, &id);

    assert_eq!(client.get_escrow(&id).state, crate::EscrowState::Completed);

    let tc = token::Client::new(&env, &token);
    let seller_bal = tc.balance(&seller);
    let buyer_bal = tc.balance(&buyer);
    let fee_bal = client.get_accumulated_fees(&token);
    let contract_bal = tc.balance(&contract_id);

    // Net principal = everything that is not a fee.
    let net = amount - fee_bal;
    assert_eq!(seller_bal, net * 5_000 / 10_000, "payee gets 50% of net");
    assert_eq!(buyer_bal, net - seller_bal, "buyer gets the remainder");

    // Invariants: payee_net + buyer_net == net, and zero stroops stranded.
    assert_eq!(seller_bal + buyer_bal, net);
    assert_eq!(seller_bal + buyer_bal + fee_bal, amount);
    assert_eq!(contract_bal, fee_bal);
}

#[test]
fn test_split_resolution_extremes_match_release_and_refund() {
    for (bps, payee_gets_all) in [(10_000_u32, true), (0_u32, false)] {
        let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);

        let id =
            disputed_escrow_for_split(&env, &client, &seller, &buyer, &resolver, &token, 10_001);
        client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Split(bps));
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
        client.finalize_dispute(&resolver, &id);

        let tc = token::Client::new(&env, &token);
        if payee_gets_all {
            assert_eq!(tc.balance(&buyer), 0);
            assert!(tc.balance(&seller) > 0);
        } else {
            assert_eq!(tc.balance(&seller), 0);
            assert!(tc.balance(&buyer) > 0);
        }
        assert_eq!(
            tc.balance(&contract_id),
            client.get_accumulated_fees(&token)
        );
    }
}

#[test]
fn test_split_resolution_rejects_bps_above_10000() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);

    let id = disputed_escrow_for_split(&env, &client, &seller, &buyer, &resolver, &token, 10_001);

    assert_eq!(
        client.try_resolve_dispute(&resolver, &id, &crate::ResolutionType::Split(10_001)),
        Err(Ok(crate::ContractError::InvalidSplitBps))
    );
    assert_eq!(
        client.try_vote(&resolver, &id, &crate::ResolutionType::Split(u32::MAX)),
        Err(Ok(crate::ContractError::InvalidSplitBps))
    );
    assert_eq!(client.get_escrow(&id).state, crate::EscrowState::Disputed);
}

#[test]
fn test_resolution_code_roundtrip_including_split() {
    use crate::ResolutionType;

    assert_eq!(ResolutionType::from_code(0), None);
    assert_eq!(ResolutionType::from_code(1), Some(ResolutionType::Release));
    assert_eq!(ResolutionType::from_code(2), Some(ResolutionType::Refund));
    for bps in [0_u32, 1, 5_000, 9_999, 10_000] {
        let r = ResolutionType::Split(bps);
        assert_eq!(ResolutionType::from_code(r.to_code()), Some(r));
    }
    assert_eq!(ResolutionType::from_code(10_004), None);
}

// ── Resolution reason & memo ────────────────────────────────────────────────

/// Funded + disputed escrow (zero protocol fee) ready to be resolved.
fn disputed_escrow_for_reason(
    env: &Env,
    client: &EscrowClient,
    seller: &Address,
    buyer: &Address,
    resolver: &Address,
    token: &Address,
) -> u64 {
    let seller_val = seller.clone().into_val(env);
    let id = client.create_escrow_8(
        &seller_val,
        &Some(buyer.clone()),
        resolver,
        token,
        &1000_i128,
        &0_u32,
        &3600_u64,
    );
    token::StellarAssetClient::new(env, token).mint(buyer, &1000_i128);
    client.fund_escrow(&id, buyer);
    client.raise_dispute(
        buyer,
        &id,
        &Symbol::new(env, "OTHER"),
        &soroban_sdk::Bytes::from_slice(env, b"desc"),
        &BytesN::from_array(env, &[0xab; 32]),
    );
    id
}

#[test]
fn test_resolve_with_reason_persists_reason_and_memo() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    let id = disputed_escrow_for_reason(&env, &client, &seller, &buyer, &resolver, &token);

    // Before resolution: nothing recorded.
    let before = client.get_dispute(&id).unwrap();
    assert_eq!(before.get_resolution_reason(), None);
    assert_eq!(before.get_resolution_memo().len(), 0);

    let reason = Symbol::new(&env, "NON_DELIVERY");
    let memo = soroban_sdk::Bytes::from_slice(&env, b"Tracking shows package never left depot");
    client.resolve_dispute_with_reason(
        &resolver,
        &id,
        &crate::ResolutionType::Refund,
        &Some(reason.clone()),
        &memo,
    );

    let dispute = client.get_dispute(&id).unwrap();
    assert_eq!(
        dispute.get_resolution(),
        Some(crate::ResolutionType::Refund)
    );
    assert_eq!(dispute.get_resolution_reason(), Some(reason.clone()));
    assert_eq!(dispute.get_resolution_memo(), memo);
    assert_eq!(dispute.resolution_reason, Some(reason));
    assert_eq!(dispute.resolution_memo, memo);

    // An appeal clears the reason and memo along with the resolution.
    client.appeal_dispute(&buyer, &id);
    let appealed = client.get_dispute(&id).unwrap();
    assert_eq!(appealed.get_resolution_reason(), None);
    assert_eq!(appealed.get_resolution_memo().len(), 0);
}

#[test]
fn test_resolve_dispute_wrapper_defaults_reason_to_none() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    let id = disputed_escrow_for_reason(&env, &client, &seller, &buyer, &resolver, &token);

    client.resolve_dispute(&resolver, &id, &crate::ResolutionType::Release);

    let dispute = client.get_dispute(&id).unwrap();
    assert_eq!(
        dispute.get_resolution(),
        Some(crate::ResolutionType::Release)
    );
    assert_eq!(dispute.get_resolution_reason(), None);
    assert_eq!(dispute.get_resolution_memo().len(), 0);
}

#[test]
fn test_resolve_with_reason_accepts_reason_without_memo_and_max_len_memo() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    let id = disputed_escrow_for_reason(&env, &client, &seller, &buyer, &resolver, &token);

    // A memo of exactly MAX_DESCRIPTION_LEN bytes is accepted.
    let memo = soroban_sdk::Bytes::from_slice(&env, &[b'a'; crate::MAX_DESCRIPTION_LEN as usize]);
    client.resolve_dispute_with_reason(
        &resolver,
        &id,
        &crate::ResolutionType::Release,
        &None,
        &memo,
    );
    let dispute = client.get_dispute(&id).unwrap();
    assert_eq!(dispute.get_resolution_reason(), None);
    assert_eq!(
        dispute.get_resolution_memo().len(),
        crate::MAX_DESCRIPTION_LEN
    );
}

#[test]
fn test_resolve_with_reason_rejects_oversized_memo() {
    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    let id = disputed_escrow_for_reason(&env, &client, &seller, &buyer, &resolver, &token);

    let memo =
        soroban_sdk::Bytes::from_slice(&env, &[b'a'; crate::MAX_DESCRIPTION_LEN as usize + 1]);
    let result = client.try_resolve_dispute_with_reason(
        &resolver,
        &id,
        &crate::ResolutionType::Release,
        &Some(Symbol::new(&env, "DAMAGED")),
        &memo,
    );
    assert_eq!(result, Err(Ok(crate::ContractError::InputTooLong)));
    assert_eq!(client.get_escrow(&id).state, crate::EscrowState::Disputed);
}

#[test]
fn test_legacy_dispute_entry_without_reason_fields_still_loads() {
    use soroban_sdk::{Map, Val};

    let (env, admin, seller, buyer, resolver, token, fee_collector) = setup_env();
    let contract_id = env.register(Escrow, ());
    let client = EscrowClient::new(&env, &contract_id);
    client.initialize(&admin, &fee_collector, &0_u32);
    let id = disputed_escrow_for_reason(&env, &client, &seller, &buyer, &resolver, &token);

    // Rewrite the stored dispute in its pre-upgrade shape: no
    // `resolution_reason` / `resolution_memo` keys.
    env.as_contract(&contract_id, || {
        let key = crate::DataKey::Dispute(id);
        let mut raw: Map<Symbol, Val> = env.storage().persistent().get(&key).unwrap();
        raw.remove(Symbol::new(&env, "resolution_reason"));
        raw.remove(Symbol::new(&env, "resolution_memo"));
        env.storage().persistent().set(&key, &raw);
    });

    let dispute = client.get_dispute(&id).expect("legacy entry must decode");
    assert_eq!(dispute.status, DisputeStatus::Active);
    assert_eq!(dispute.get_resolution_reason(), None);
    assert_eq!(dispute.get_resolution_memo().len(), 0);

    // And it can still be resolved and re-saved in the new shape.
    client.resolve_dispute_with_reason(
        &resolver,
        &id,
        &crate::ResolutionType::Release,
        &Some(Symbol::new(&env, "MUTUAL")),
        &soroban_sdk::Bytes::new(&env),
    );
    assert_eq!(
        client.get_dispute(&id).unwrap().get_resolution_reason(),
        Some(Symbol::new(&env, "MUTUAL"))
    );
}
