#![cfg(test)]
//! Adversarial SEP-41 token used by the re-entrancy / malicious-token defence
//! suite (issue #402).
//!
//! This contract is **test-only**: the whole module is gated behind
//! `#[cfg(test)]`, so it is never compiled into the escrow contract's WASM and
//! does not change the public ABI.
//!
//! It implements just enough of the SEP-41 surface the escrow actually calls
//! (`transfer`, plus `balance`/`mint` for assertions) and, depending on the
//! configured [`Attack`], misbehaves while the escrow is mid-execution:
//!
//! * [`Attack::Fail`] — always panics, modelling a broken / hostile token.
//! * [`Attack::BurnBudget`] — runs a long metered loop, modelling an
//!   "infinite-gas" token that tries to wedge the call.
//! * `Attack::Reenter*` — calls back into the still-executing escrow, modelling
//!   a classic re-entrancy attack (target configured via [`set_reentry`]).
//!
//! Soroban's host forbids re-entrancy and meters every host call, so each of
//! these aborts the *entire* invocation atomically. The accompanying tests in
//! `test_malicious_token.rs` assert that the escrow's accounting is therefore
//! left completely untouched.

use crate::EscrowClient;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Bytes, Env};

/// The misbehaviour the token should exhibit on its next `transfer`.
///
/// All variants are unit-like so the type is a valid `#[contracttype]` enum;
/// the re-entrancy target (escrow / actor / id) is supplied separately via
/// [`MaliciousToken::set_reentry`].
#[contracttype]
#[derive(Clone)]
pub enum Attack {
    /// Behave like an ordinary token (move balances, no tricks).
    None,
    /// Panic unconditionally, simulating a broken or hostile token.
    Fail,
    /// Burn the CPU budget with a long metered loop ("infinite gas").
    BurnBudget,
    /// Re-enter `confirm_delivery(actor, escrow_id)` during the transfer.
    ReenterConfirm,
    /// Re-enter `fund_escrow(escrow_id, actor)` during the transfer.
    ReenterFund,
    /// Re-enter `cancel_escrow(actor, escrow_id)` during the transfer.
    ReenterCancel,
}

/// Re-entrancy target: which escrow to call back into, as whom, for which id.
#[contracttype]
#[derive(Clone)]
pub struct Reentry {
    pub escrow: Address,
    pub actor: Address,
    pub escrow_id: u64,
}

#[contracttype]
#[derive(Clone)]
enum Key {
    Balance(Address),
    Attack,
    Reentry,
}

#[contract]
pub struct MaliciousToken;

#[contractimpl]
impl MaliciousToken {
    /// Configure the misbehaviour applied on the next `transfer`.
    pub fn set_attack(env: Env, attack: Attack) {
        env.storage().instance().set(&Key::Attack, &attack);
    }

    /// Configure the escrow / actor / id used by the `Reenter*` attacks.
    pub fn set_reentry(env: Env, escrow: Address, actor: Address, escrow_id: u64) {
        env.storage().instance().set(
            &Key::Reentry,
            &Reentry {
                escrow,
                actor,
                escrow_id,
            },
        );
    }

    /// Credit `amount` to `to` (test-only minting helper).
    pub fn mint(env: Env, to: Address, amount: i128) {
        let key = Key::Balance(to);
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + amount));
    }

    /// SEP-41 balance query.
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&Key::Balance(id))
            .unwrap_or(0)
    }

    /// SEP-41 transfer — the hook every attack rides in on. The escrow invokes
    /// this both when pulling funds in (`fund_escrow`) and when paying funds out
    /// (`confirm_delivery`, refunds, cancellations).
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();

        let attack: Attack = env
            .storage()
            .instance()
            .get(&Key::Attack)
            .unwrap_or(Attack::None);

        match attack {
            Attack::None => {}
            Attack::Fail => panic!("malicious token: transfer rejected"),
            Attack::BurnBudget => {
                // Each `sha256` is a metered host call. Under a constrained CPU
                // budget (set by the test) this exhausts the budget long before
                // the loop finishes, so the host aborts and the escrow
                // operation reverts atomically. The bound is only a safety net
                // to guarantee termination if the budget were ever unlimited.
                let data = Bytes::from_array(&env, &[7u8; 32]);
                let mut i: u32 = 0;
                while i < 1_000_000 {
                    let _ = env.crypto().sha256(&data);
                    i += 1;
                }
            }
            Attack::ReenterConfirm => {
                // Re-entering the still-executing escrow is forbidden by the
                // host: this call traps, reverting the whole transfer.
                let r = Self::reentry(&env);
                EscrowClient::new(&env, &r.escrow).confirm_delivery(&r.actor, &r.escrow_id);
            }
            Attack::ReenterFund => {
                let r = Self::reentry(&env);
                EscrowClient::new(&env, &r.escrow).fund_escrow(&r.escrow_id, &r.actor);
            }
            Attack::ReenterCancel => {
                let r = Self::reentry(&env);
                EscrowClient::new(&env, &r.escrow).cancel_escrow(&r.actor, &r.escrow_id);
            }
        }

        // Only reached when the configured attack does not abort the call.
        Self::do_transfer(&env, &from, &to, amount);
    }
}

// Plain (non-`#[contractimpl]`) impl: these helpers are internal and are *not*
// exported as contract functions.
impl MaliciousToken {
    fn reentry(env: &Env) -> Reentry {
        env.storage()
            .instance()
            .get(&Key::Reentry)
            .expect("reentry target not configured")
    }

    fn do_transfer(env: &Env, from: &Address, to: &Address, amount: i128) {
        let from_key = Key::Balance(from.clone());
        let from_balance: i128 = env.storage().persistent().get(&from_key).unwrap_or(0);
        if from_balance < amount {
            panic!("malicious token: insufficient balance");
        }
        env.storage()
            .persistent()
            .set(&from_key, &(from_balance - amount));

        let to_key = Key::Balance(to.clone());
        let to_balance: i128 = env.storage().persistent().get(&to_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&to_key, &(to_balance + amount));
    }
}
