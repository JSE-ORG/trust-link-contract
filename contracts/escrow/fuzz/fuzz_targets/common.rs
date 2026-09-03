//! Shared harness for the escrow fuzz targets.
//!
//! Every target registers the contract in a Soroban test `Env` and drives it
//! through the generated `EscrowClient` using the `try_*` methods, so declared
//! `ContractError`s are returned as values instead of unwinding. Anything that
//! still panics is a genuine finding rather than harness noise.
//!
//! The byte reader below never panics on short input: reads past the end of the
//! fuzz buffer are zero-filled, which keeps the harness itself out of the crash
//! reports.

#![allow(dead_code)]

use soroban_sdk::{
    testutils::Address as _, token, Address, BytesN, Env, IntoVal, String as SorobanString, Symbol,
    Val, Vec,
};
use trustlink_escrow::{Escrow, EscrowClient, Payee};

/// Balance minted to every generated account. Large enough that funding is
/// never the limiting factor, small enough to stay far from `i128` limits.
pub const MINT_AMOUNT: i128 = 1_000_000_000_000_000;

/// Escrow amount used whenever a target needs a *valid* escrow to reach the
/// state under test.
pub const VALID_AMOUNT: i128 = 1_000_000;
pub const VALID_FEE_BPS: u32 = 50;
pub const VALID_SHIPPING_WINDOW: u64 = 86_400;

/// Reason symbols accepted by `raise_dispute`. `Symbol::new` panics on
/// characters outside the Soroban symbol alphabet, so the reason is selected
/// from this fixed set rather than built from raw fuzz bytes.
pub const DISPUTE_REASONS: [&str; 4] =
    ["ITEM_NOT_RECEIVED", "NOT_AS_DESCRIBED", "DAMAGED", "OTHER"];

pub struct Harness {
    pub env: Env,
    pub client: EscrowClient<'static>,
    pub admin: Address,
    pub fee_collector: Address,
    pub seller: Address,
    pub buyer: Address,
    pub resolver: Address,
    pub outsider: Address,
    pub token: Address,
}

impl Harness {
    /// Registers and initializes the contract with funded participants.
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let fee_collector = Address::generate(&env);
        let seller = Address::generate(&env);
        let buyer = Address::generate(&env);
        let resolver = Address::generate(&env);
        let outsider = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let minter = token::StellarAssetClient::new(&env, &token);
        minter.mint(&buyer, &MINT_AMOUNT);
        minter.mint(&seller, &MINT_AMOUNT);

        let contract_id = env.register(Escrow, ());
        let client = EscrowClient::new(&env, &contract_id);
        let _ = client.try_initialize(&admin, &fee_collector, &0_u32);

        Self {
            env,
            client,
            admin,
            fee_collector,
            seller,
            buyer,
            resolver,
            outsider,
            token,
        }
    }

    /// A single-payee `Vec<Payee>` erased to `Val`, matching the polymorphic
    /// first argument of `create_escrow`.
    pub fn payees(&self) -> Val {
        let mut payees = Vec::new(&self.env);
        payees.push_back(Payee {
            address: self.seller.clone(),
            bps: 10_000,
        });
        payees.into_val(&self.env)
    }

    /// Creates an escrow that is guaranteed to be valid, returning its id.
    /// `None` means creation was rejected, in which case the caller should bail
    /// out rather than fuzz against a nonexistent escrow.
    pub fn create_valid_escrow(&self) -> Option<u64> {
        self.client
            .try_create_escrow(
                &self.payees(),
                &Some(self.buyer.clone()),
                &self.resolver,
                &self.token,
                &VALID_AMOUNT,
                &VALID_FEE_BPS,
                &0_u32,
                &VALID_SHIPPING_WINDOW,
                &Option::<SorobanString>::None,
            )
            .ok()?
            .ok()
    }

    /// Creates and funds an escrow, returning its id.
    pub fn create_funded_escrow(&self) -> Option<u64> {
        let id = self.create_valid_escrow()?;
        self.client.try_fund_escrow(&id, &self.buyer).ok()?.ok()?;
        Some(id)
    }

    /// Picks one of the known participants from a fuzz byte, so callers cover
    /// both the authorized and the unauthorized path.
    pub fn actor(&self, selector: u8) -> Address {
        match selector % 5 {
            0 => self.buyer.clone(),
            1 => self.seller.clone(),
            2 => self.resolver.clone(),
            3 => self.admin.clone(),
            _ => self.outsider.clone(),
        }
    }

    /// A fresh address that holds no role on any escrow.
    pub fn stranger(&self) -> Address {
        Address::generate(&self.env)
    }

    /// `n` generated addresses, for the multi-resolver entry points.
    pub fn addresses(&self, n: usize) -> Vec<Address> {
        let mut out = Vec::new(&self.env);
        for _ in 0..n {
            out.push_back(Address::generate(&self.env));
        }
        out
    }

    /// Registers a second Stellar Asset Contract, for basket escrows that need
    /// more than one distinct token.
    pub fn extra_token(&self) -> Address {
        let token_admin = Address::generate(&self.env);
        let token = self
            .env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let minter = token::StellarAssetClient::new(&self.env, &token);
        minter.mint(&self.buyer, &MINT_AMOUNT);
        minter.mint(&self.seller, &MINT_AMOUNT);
        token
    }

    /// Drives a funded escrow into `Disputed`, returning its id.
    ///
    /// `None` means an earlier step was rejected, in which case the caller
    /// should bail out rather than fuzz against the wrong state.
    pub fn create_disputed_escrow(&self, r: &mut Reader) -> Option<u64> {
        let id = self.create_funded_escrow()?;
        let reason = self.dispute_reason(r);
        let description = r.ascii_string(&self.env, 64);
        let evidence_hash = BytesN::from_array(&self.env, &r.bytes32());
        self.client
            .try_raise_dispute(&self.buyer, &id, &reason, &description, &evidence_hash)
            .ok()?
            .ok()?;
        Some(id)
    }

    /// One of the accepted dispute reason symbols, chosen by a fuzz byte.
    pub fn dispute_reason(&self, r: &mut Reader) -> Symbol {
        Symbol::new(
            &self.env,
            DISPUTE_REASONS[(r.u8() as usize) % DISPUTE_REASONS.len()],
        )
    }
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

/// Zero-padding cursor over the fuzz input.
pub struct Reader<'d> {
    data: &'d [u8],
    pos: usize,
}

impl<'d> Reader<'d> {
    pub fn new(data: &'d [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn fill(&mut self, out: &mut [u8]) {
        for slot in out.iter_mut() {
            *slot = self.data.get(self.pos).copied().unwrap_or(0);
            self.pos = self.pos.saturating_add(1);
        }
    }

    pub fn u8(&mut self) -> u8 {
        let mut buf = [0u8; 1];
        self.fill(&mut buf);
        buf[0]
    }

    pub fn u32(&mut self) -> u32 {
        let mut buf = [0u8; 4];
        self.fill(&mut buf);
        u32::from_be_bytes(buf)
    }

    pub fn u64(&mut self) -> u64 {
        let mut buf = [0u8; 8];
        self.fill(&mut buf);
        u64::from_be_bytes(buf)
    }

    pub fn i128(&mut self) -> i128 {
        let mut buf = [0u8; 16];
        self.fill(&mut buf);
        i128::from_be_bytes(buf)
    }

    pub fn bytes32(&mut self) -> [u8; 32] {
        let mut buf = [0u8; 32];
        self.fill(&mut buf);
        buf
    }

    /// A uniformly distributed boolean: the low bit of a fuzzed byte. `u8` is
    /// drawn uniformly from the input (zero-padded past the end), so `% 2 == 0`
    /// yields an exact 50/50 split across both branches.
    pub fn bool(&mut self) -> bool {
        self.u8() % 2 == 0
    }

    /// A ledger timestamp bounded to 63 bits. Full-width `u64::MAX` timestamps
    /// are excluded because they exercise host clock limits rather than
    /// contract logic; every deadline the contract computes still fits.
    pub fn timestamp(&mut self) -> u64 {
        self.u64() >> 1
    }

    /// A collection length in `0..=max`, so targets cover the empty case, the
    /// documented cap and everything in between.
    pub fn len(&mut self, max: usize) -> usize {
        (self.u8() as usize) % (max + 1)
    }

    /// Roughly half the time returns `real_id` (the escrow the target already
    /// set up); the rest of the time returns an arbitrary fuzzed id. Nearly
    /// every target uses this to choose between exercising the real escrow
    /// and probing the not-found / wrong-id path.
    pub fn target_id(&mut self, real_id: u64) -> u64 {
        if self.bool() {
            real_id
        } else {
            self.u64()
        }
    }

    /// An ASCII string of up to `max` characters, exercising the contract's
    /// length validation from empty through over-long.
    pub fn ascii_string(&mut self, env: &Env, max: usize) -> SorobanString {
        let len = (self.u8() as usize) % (max + 1);
        let mut buf = std::string::String::with_capacity(len);
        for _ in 0..len {
            buf.push(char::from(b'a' + (self.u8() % 26)));
        }
        SorobanString::from_str(env, &buf)
    }
}
