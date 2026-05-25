#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, Vec};

pub mod storage;
pub use storage::{DataKey, FeeConfig};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowData {
    pub seller: Address,
    pub buyer: Option<Address>,
    pub resolver: Address,
    pub token: Address,
    pub amount: i128,
    pub shipping_window: u64,
    pub funded_at: u64,
    pub state: EscrowState,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowState {
    Pending,
    Funded,
    Completed,
    Disputed,
    Refunded,
}

#[contract]
pub struct Escrow;

#[contractimpl]
#[allow(deprecated)]
impl Escrow {
    pub fn create_escrow(
        env: Env,
        seller: Address,
        resolver: Address,
        token: Address,
        amount: i128,
        shipping_window: u64,
    ) -> u32 {
        seller.require_auth();

        let escrow_id = storage::next_escrow_id(&env);

        let escrow = EscrowData {
            seller: seller.clone(),
            buyer: None,
            resolver,
            token,
            amount,
            shipping_window,
            funded_at: 0,
            state: EscrowState::Pending,
        };

        storage::write_escrow(&env, escrow_id, &escrow);
        storage::append_vendor_escrow(&env, &seller, escrow_id);

        env.events().publish(("create_escrow",), escrow_id);
        escrow_id
    }

    pub fn fund_escrow(env: Env, escrow_id: u32, buyer: Address) {
        buyer.require_auth();

        let mut escrow: EscrowData = storage::require_escrow(&env, escrow_id);

        assert!(escrow.state == EscrowState::Pending, "escrow not pending");

        escrow.buyer = Some(buyer.clone());
        escrow.state = EscrowState::Funded;
        escrow.funded_at = env.ledger().timestamp();

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(&buyer, &env.current_contract_address(), &escrow.amount);

        storage::write_escrow(&env, escrow_id, &escrow);
        storage::append_buyer_escrow(&env, &buyer, escrow_id);
        env.events().publish(("fund_escrow",), escrow_id);
    }

    pub fn confirm_delivery(env: Env, escrow_id: u32) {
        let escrow: EscrowData = storage::require_escrow(&env, escrow_id);

        assert!(escrow.state == EscrowState::Funded, "escrow not funded");

        let buyer = escrow.buyer.clone().expect("escrow has no buyer");
        buyer.require_auth();

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &escrow.amount,
        );

        let mut updated = escrow;
        updated.state = EscrowState::Completed;

        storage::write_escrow(&env, escrow_id, &updated);
        env.events().publish(("confirm_delivery",), escrow_id);
    }

    pub fn raise_dispute(env: Env, escrow_id: u32) {
        let escrow: EscrowData = storage::require_escrow(&env, escrow_id);

        assert!(escrow.state == EscrowState::Funded, "escrow not funded");

        let buyer = escrow.buyer.clone().expect("escrow has no buyer");
        buyer.require_auth();

        let mut updated = escrow;
        updated.state = EscrowState::Disputed;

        storage::write_escrow(&env, escrow_id, &updated);
        env.events().publish(("raise_dispute",), escrow_id);
    }

    pub fn resolve_dispute(env: Env, escrow_id: u32, release_to_seller: bool) {
        let escrow: EscrowData = storage::require_escrow(&env, escrow_id);

        assert!(escrow.state == EscrowState::Disputed, "escrow not disputed");

        escrow.resolver.require_auth();

        let token_client = token::Client::new(&env, &escrow.token);
        if release_to_seller {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.seller,
                &escrow.amount,
            );
        } else {
            token_client.transfer(
                &env.current_contract_address(),
                &escrow.buyer.clone().expect("escrow has no buyer"),
                &escrow.amount,
            );
        }

        let mut updated = escrow;
        updated.state = if release_to_seller {
            EscrowState::Completed
        } else {
            EscrowState::Refunded
        };

        storage::write_escrow(&env, escrow_id, &updated);
        env.events()
            .publish(("resolve_dispute",), (escrow_id, release_to_seller));
    }

    pub fn auto_release(env: Env, escrow_id: u32) {
        let escrow: EscrowData = storage::require_escrow(&env, escrow_id);

        assert!(escrow.state == EscrowState::Funded, "escrow not funded");
        assert!(
            env.ledger().timestamp() >= escrow.funded_at + escrow.shipping_window,
            "shipping window not elapsed"
        );

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(
            &env.current_contract_address(),
            &escrow.seller,
            &escrow.amount,
        );

        let mut updated = escrow;
        updated.state = EscrowState::Completed;

        storage::write_escrow(&env, escrow_id, &updated);
        env.events().publish(("auto_release",), escrow_id);
    }

    pub fn get_escrow(env: Env, escrow_id: u32) -> EscrowData {
        storage::require_escrow(&env, escrow_id)
    }

    pub fn get_escrows_by_vendor(env: Env, vendor: Address) -> Vec<u32> {
        storage::read_vendor_index(&env, &vendor)
    }

    pub fn get_escrows_by_buyer(env: Env, buyer: Address) -> Vec<u32> {
        storage::read_buyer_index(&env, &buyer)
    }
}

mod test;
