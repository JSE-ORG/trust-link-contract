#![no_main]
//! Fuzzes escrow expiration: `create_escrow_with_expiration` stores an
//! `expires_at` plus a grace period, and `reclaim_expired` / `auto_cancel_pending`
//! act on the resulting deadlines.
//!
//! Deadlines are derived by adding the grace period to the expiry, so arbitrary
//! values probe that arithmetic for overflow, and arbitrary ledger timestamps
//! probe both sides of every boundary.

mod common;

use common::{Harness, Reader, VALID_AMOUNT, VALID_FEE_BPS, VALID_SHIPPING_WINDOW};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Ledger as _;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let now = r.timestamp();
    h.env.ledger().set_timestamp(now);

    // A mix of near-future expiries (which the contract should accept) and
    // arbitrary ones (past, absent, or large enough to overflow when the grace
    // period is added).
    let expires_at = match r.u8() % 4 {
        0 => None,
        1 => Some(now.saturating_add(r.u32() as u64).saturating_add(1)),
        2 => Some(r.u64()),
        _ => Some(now),
    };
    let grace_period = if r.bool() { r.u32() as u64 } else { r.u64() };

    let Ok(Ok(escrow_id)) = h.client.try_create_escrow_with_expiration(
        &h.seller,
        &Some(h.buyer.clone()),
        &h.resolver,
        &h.token,
        &VALID_AMOUNT,
        &VALID_FEE_BPS,
        &VALID_SHIPPING_WINDOW,
        &expires_at,
        &grace_period,
    ) else {
        return;
    };

    if r.bool() {
        let _ = h.client.try_fund_escrow(&escrow_id, &h.buyer);
    }

    h.env.ledger().set_timestamp(r.timestamp());

    let target_id = r.target_id(escrow_id);

    // Both reclaim paths, in a fuzz-chosen order; neither may settle an escrow
    // twice or settle one that has not actually expired.
    let steps = r.len(4);
    for _ in 0..steps {
        if r.bool() {
            let _ = h.client.try_reclaim_expired(&target_id);
        } else {
            let _ = h.client.try_auto_cancel_pending(&target_id);
        }
        if r.bool() {
            h.env.ledger().set_timestamp(r.timestamp());
        }
    }
});
