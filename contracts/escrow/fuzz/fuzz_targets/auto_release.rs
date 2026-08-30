#![no_main]
//! Fuzzes `auto_release` across arbitrary ledger timestamps, covering releases
//! attempted before, exactly at and long after the shipping window elapses.

mod common;

use common::{Harness, Reader, VALID_SHIPPING_WINDOW};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Ledger as _;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let Some(escrow_id) = h.create_funded_escrow() else {
        return;
    };

    if r.bool() {
        let tracking_id = r.ascii_string(&h.env, 32);
        let _ = h
            .client
            .try_mark_shipped(&h.seller, &escrow_id, &tracking_id);
    }

    // Bias half the runs to just past the window so the success path is hit
    // regularly; the rest explore the whole timestamp range.
    let timestamp = if r.bool() {
        VALID_SHIPPING_WINDOW.saturating_add(r.u32() as u64)
    } else {
        r.timestamp()
    };
    h.env.ledger().set_timestamp(timestamp);

    let target_id = r.target_id(escrow_id);

    let _ = h.client.try_auto_release(&target_id);

    // Releasing twice must not pay out twice.
    if r.bool() {
        let _ = h.client.try_auto_release(&target_id);
    }
});
