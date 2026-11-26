#[cfg(test)]
mod tests {
    use crate::{Escrow, EscrowClient, Payee, PlatformFeeCollected, ResolutionType};
    use soroban_sdk::{
        testutils::{Address as _, Events as _, Ledger},
        xdr::ContractEventBody,
        Address, Bytes, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
    };

    const DISPUTE_WINDOW: u64 = 172_800;

    #[allow(dead_code)]
    fn setup_contract(env: &Env) -> (EscrowClient<'_>, Address, Address) {
        let admin = Address::generate(env);
        let fee_collector = Address::generate(env);

        let token_admin = Address::generate(env);
        let token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let _token_client = soroban_sdk::token::StellarAssetClient::new(env, &token_id);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);

        (client, admin, fee_collector)
    }

    fn single_payee(env: &Env, address: &Address) -> Vec<Payee> {
        let mut payees = Vec::new(env);
        payees.push_back(Payee {
            address: address.clone(),
            bps: 10_000,
        });
        payees
    }

    fn platform_fee_events(env: &Env, contract_id: &Address) -> Vec<PlatformFeeCollected> {
        let mut found = Vec::new(env);
        for event in env
            .events()
            .all()
            .filter_by_contract(contract_id)
            .events()
            .iter()
        {
            let ContractEventBody::V0(body) = &event.body;
            let mut topics = body.topics.iter();
            let (Some(first), Some(second)) = (topics.next(), topics.next()) else {
                continue;
            };
            let (Ok(first), Ok(second)) = (
                Symbol::try_from_val(env, first),
                Symbol::try_from_val(env, second),
            ) else {
                continue;
            };
            if first != Symbol::new(env, "Treasury") || second != Symbol::new(env, "Fee") {
                continue;
            }
            let Ok(data) = Val::try_from_val(env, &body.data) else {
                continue;
            };
            if let Ok(decoded) = PlatformFeeCollected::try_from_val(env, &data) {
                found.push_back(decoded);
            }
        }
        found
    }

    /// Core regression: fee snapshotted at create_escrow time must be used
    /// at confirm_delivery even if admin raises the global fee in between.
    #[test]
    fn test_fee_change_does_not_affect_funded_escrow() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let resolver = Address::generate(&env);
        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
        token_client.mint(&buyer, &1_000_000_000_i128);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        // Initialize with 1% fee (100 bps).
        client.initialize(&admin, &fee_collector, &0_u32);
        client.set_protocol_fee(&admin, &100_u32);

        let amount = 1_000_000_i128;

        // Create escrow at 1% fee — this snapshots fee_bps = 100 into EscrowData.
        let payees = single_payee(&env, &seller);
        let payees_val = payees.into_val(&env);
        let escrow_id = client.create_escrow_8(
            &payees_val,
            &Some(buyer.clone()),
            &resolver,
            &token_id,
            &amount,
            &100_u32,
            &604_800_u64,
        );

        client.fund_escrow(&escrow_id, &buyer);

        // Admin raises fee to 3% between fund and release.
        client.set_protocol_fee(&admin, &300_u32);

        // Advance past dispute window so buyer can confirm.
        env.ledger().set_timestamp(DISPUTE_WINDOW + 1);
        client.mark_shipped(
            &seller,
            &escrow_id,
            &soroban_sdk::String::from_str(&env, "TRACK-FEE"),
        );
        client.confirm_delivery(&buyer, &escrow_id, &false);

        let tc = soroban_sdk::token::Client::new(&env, &token_id);
        let seller_balance = tc.balance(&seller);

        // Expected: 1% fee (snapshotted), NOT 3% (live).
        let expected_fee = amount * 100 / 10_000; // 100 bps = 1%
        let expected_net = amount - expected_fee;

        assert_eq!(
            seller_balance, expected_net,
            "seller should receive amount minus snapshotted 1% fee, not live 3% fee"
        );
        assert_eq!(
            client.get_accumulated_fees(&token_id),
            expected_fee,
            "protocol fee accrues at the snapshotted 1% rate, not live 3%"
        );
    }

    /// Auto-release must also use the snapshotted fee, not the live global fee.
    #[test]
    fn test_fee_change_does_not_affect_auto_release() {
        let env = Env::default();
        env.mock_all_auths();

        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let resolver = Address::generate(&env);
        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
        token_client.mint(&buyer, &1_000_000_000_i128);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);

        client.initialize(&admin, &fee_collector, &0_u32);
        client.set_protocol_fee(&admin, &100_u32);

        let amount = 1_000_000_i128;
        let shipping_window: u64 = 604_800;

        let payees = single_payee(&env, &seller);
        let payees_val = payees.into_val(&env);
        let escrow_id = client.create_escrow_8(
            &payees_val,
            &Some(buyer.clone()),
            &resolver,
            &token_id,
            &amount,
            &100_u32,
            &shipping_window,
        );

        client.fund_escrow(&escrow_id, &buyer);
        client.mark_shipped(
            &seller,
            &escrow_id,
            &soroban_sdk::String::from_str(&env, "TRACK-001"),
        );
        crate::test_helpers::record_delivery_timelocked(&env, &client, &admin, escrow_id);

        // Admin raises fee to 3% before auto_release is triggered.
        client.set_protocol_fee(&admin, &300_u32);

        // Advance past delivery release window.
        env.ledger()
            .set_timestamp(shipping_window + DISPUTE_WINDOW + 1);
        client.auto_release(&escrow_id);

        let tc = soroban_sdk::token::Client::new(&env, &token_id);
        let seller_balance = tc.balance(&seller);

        // auto_release uses the live protocol_fee_bps (300 = 3%), not the escrow's snapshotted fee_bps.
        let expected_fee = amount * 300 / 10_000;
        let expected_net = amount - expected_fee;

        assert_eq!(
            seller_balance, expected_net,
            "auto_release should use live protocol fee"
        );
        assert_eq!(
            client.get_accumulated_fees(&token_id),
            expected_fee,
            "auto_release accrues live protocol fee for later claim"
        );
    }

    /// Arbitration fee snapshotted at funding must not change when the admin
    /// raises the global arbitration fee before finalization (issue #1255).
    #[test]
    fn test_arbitration_fee_change_does_not_affect_funded_escrow() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let resolver = Address::generate(&env);
        let fee_collector = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
        token_client.mint(&buyer, &1_000_000_000_i128);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &100_u32);

        let amount = 1_000_000_i128;
        let payees_val = single_payee(&env, &seller).into_val(&env);
        let escrow_id = client.create_escrow_8(
            &payees_val,
            &Some(buyer.clone()),
            &resolver,
            &token_id,
            &amount,
            &0_u32,
            &604_800_u64,
        );

        client.fund_escrow(&escrow_id, &buyer);
        client.set_arbitration_fee(&admin, &500_u32);

        client.mark_shipped(
            &seller,
            &escrow_id,
            &soroban_sdk::String::from_str(&env, "TRACK-ARB-FEE"),
        );
        client.raise_dispute(
            &buyer,
            &escrow_id,
            &Symbol::new(&env, "OTHER"),
            &Bytes::from_slice(&env, b"desc"),
            &BytesN::from_array(&env, &[3u8; 32]),
        );
        client.resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release);
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
        client.finalize_dispute(&resolver, &escrow_id);

        let expected_arb_fee = amount * 100 / 10_000;
        let tc = soroban_sdk::token::Client::new(&env, &token_id);
        assert_eq!(
            tc.balance(&fee_collector),
            expected_arb_fee,
            "fee collector should receive snapshotted 1% arbitration fee, not live 5%"
        );
        assert_eq!(
            tc.balance(&seller),
            amount - expected_arb_fee,
            "seller payout uses snapshotted arbitration rate"
        );
    }

    /// Platform fee snapshotted at funding must not change when the admin
    /// raises the global platform fee before finalization (issue #1255).
    #[test]
    fn test_platform_fee_change_does_not_affect_funded_escrow() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let resolver = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let treasury = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
        token_client.mint(&buyer, &1_000_000_000_i128);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
        client.set_platform_fee(&admin, &100_u32);
        env.as_contract(&contract_id, || {
            crate::internal::write_treasury(&env, &treasury);
        });

        let amount = 1_000_000_i128;
        let payees_val = single_payee(&env, &seller).into_val(&env);
        let escrow_id = client.create_escrow_8(
            &payees_val,
            &Some(buyer.clone()),
            &resolver,
            &token_id,
            &amount,
            &100_u32,
            &604_800_u64,
        );

        client.fund_escrow(&escrow_id, &buyer);

        // Admin raises platform fee to the cap after funding (300 bps exceeds `MAX_PLATFORM_FEE_BPS`).
        client.set_platform_fee(&admin, &200_u32);

        client.mark_shipped(
            &seller,
            &escrow_id,
            &soroban_sdk::String::from_str(&env, "TRACK-PLATFORM-FEE"),
        );
        client.raise_dispute(
            &buyer,
            &escrow_id,
            &Symbol::new(&env, "OTHER"),
            &Bytes::from_slice(&env, b"desc"),
            &BytesN::from_array(&env, &[2u8; 32]),
        );
        client.resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release);
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
        client.finalize_dispute(&resolver, &escrow_id);

        let expected_platform_fee = amount * 100 / 10_000;
        let tc = soroban_sdk::token::Client::new(&env, &token_id);
        assert_eq!(
            tc.balance(&treasury),
            expected_platform_fee,
            "treasury should receive snapshotted 1% platform fee, not live 2%"
        );
    }

    #[test]
    fn finalize_dispute_emits_platform_fee_collected_event() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let resolver = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let treasury = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let token_id = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_id);
        token_client.mint(&buyer, &1_000_000_000_i128);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
        client.set_platform_fee(&admin, &100_u32);
        env.as_contract(&contract_id, || {
            crate::internal::write_treasury(&env, &treasury);
        });

        let amount = 1_000_000_i128;
        let payees_val = single_payee(&env, &seller).into_val(&env);
        let escrow_id = client.create_escrow_8(
            &payees_val,
            &Some(buyer.clone()),
            &resolver,
            &token_id,
            &amount,
            &100_u32,
            &604_800_u64,
        );

        client.fund_escrow(&escrow_id, &buyer);
        client.mark_shipped(
            &seller,
            &escrow_id,
            &soroban_sdk::String::from_str(&env, "TRACK-FEE-EVENT"),
        );
        client.raise_dispute(
            &buyer,
            &escrow_id,
            &Symbol::new(&env, "OTHER"),
            &Bytes::from_slice(&env, b"desc"),
            &BytesN::from_array(&env, &[1u8; 32]),
        );
        client.resolve_dispute(&resolver, &escrow_id, &ResolutionType::Release);
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + crate::APPEAL_WINDOW + 1);
        client.finalize_dispute(&resolver, &escrow_id);
        let events = platform_fee_events(&env, &contract_id);

        let expected_fee = amount * 100 / 10_000;
        let tc = soroban_sdk::token::Client::new(&env, &token_id);
        assert_eq!(tc.balance(&treasury), expected_fee);

        assert_eq!(events.len(), 1, "one platform fee transfer to treasury");
        let event = events.get(0).unwrap();
        assert_eq!(event.schema_version, crate::EVENT_SCHEMA_VERSION);
        assert_eq!(event.escrow_id, escrow_id);
        assert_eq!(event.token, token_id);
        assert_eq!(event.treasury, treasury);
        assert_eq!(event.fee_amount, expected_fee);
        assert_eq!(event.timestamp, env.ledger().timestamp());
    }

    #[test]
    fn settle_escrow_to_payees_emits_platform_fee_collected_event() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let resolver = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let treasury = Address::generate(&env);

        let primary_admin = Address::generate(&env);
        let primary_id = env
            .register_stellar_asset_contract_v2(primary_admin.clone())
            .address();
        let secondary_admin = Address::generate(&env);
        let secondary_id = env
            .register_stellar_asset_contract_v2(secondary_admin.clone())
            .address();
        let primary_token = soroban_sdk::token::StellarAssetClient::new(&env, &primary_id);
        let secondary_token = soroban_sdk::token::StellarAssetClient::new(&env, &secondary_id);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);
        client.initialize(&admin, &fee_collector, &0_u32);
        client.set_platform_fee(&admin, &200_u32);
        env.as_contract(&contract_id, || {
            crate::internal::write_treasury(&env, &treasury);
        });

        let primary_amount = 10_000_i128;
        let secondary_amount = 5_000_i128;
        let mut tokens = Vec::new(&env);
        tokens.push_back(primary_id.clone());
        tokens.push_back(secondary_id.clone());
        let mut amounts = Vec::new(&env);
        amounts.push_back(primary_amount);
        amounts.push_back(secondary_amount);
        let escrow_id = client.create_basket_escrow(
            &seller,
            &Some(buyer.clone()),
            &resolver,
            &tokens,
            &amounts,
            &100_u32,
            &604_800_u64,
            &None::<soroban_sdk::String>,
            &None::<u64>,
            &0_u64,
        );

        primary_token.mint(&buyer, &(primary_amount * 2));
        secondary_token.mint(&buyer, &(secondary_amount * 2));
        client.fund_basket_escrow(&escrow_id, &buyer);
        client.mark_shipped(
            &seller,
            &escrow_id,
            &soroban_sdk::String::from_str(&env, "TRACK-BASKET-FEE-EVENT"),
        );
        client.confirm_delivery(&buyer, &escrow_id, &true);
        let events = platform_fee_events(&env, &contract_id);

        let expected_fee = secondary_amount * 200 / 10_000;
        let primary_client = soroban_sdk::token::Client::new(&env, &primary_id);
        let secondary_client = soroban_sdk::token::Client::new(&env, &secondary_id);
        assert_eq!(
            primary_client.balance(&treasury),
            0,
            "no primary platform fee"
        );
        assert_eq!(secondary_client.balance(&treasury), expected_fee);

        assert_eq!(events.len(), 1, "one secondary-token platform fee transfer");
        let event = events.get(0).unwrap();
        assert_eq!(event.schema_version, crate::EVENT_SCHEMA_VERSION);
        assert_eq!(event.escrow_id, escrow_id);
        assert_eq!(event.token, secondary_id);
        assert_eq!(event.treasury, treasury);
        assert_eq!(event.fee_amount, expected_fee);
        assert_eq!(event.timestamp, env.ledger().timestamp());
    }
}
