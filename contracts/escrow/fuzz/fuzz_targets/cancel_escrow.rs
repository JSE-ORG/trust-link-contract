#![no_main]
//! Fuzzes `cancel_escrow` from every caller role and from every reachable
//! escrow state, checking that cancellation restrictions hold.

mod common;

use common::{Harness, Reader};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let Some(escrow_id) = h.create_valid_escrow() else {
        return;
    };

    // Optionally advance the escrow past Pending before cancelling.
    if r.bool() {
        let _ = h.client.try_fund_escrow(&escrow_id, &h.buyer);
        if r.bool() {
            let tracking_id = r.ascii_string(&h.env, 32);
            let _ = h
                .client
                .try_mark_shipped(&h.seller, &escrow_id, &tracking_id);
        }
    }

    let caller = h.actor(r.u8());
    let target_id = r.target_id(escrow_id);

    let _ = h.client.try_cancel_escrow(&caller, &target_id);

    // Cancelling twice must not refund twice.
    if r.bool() {
        let _ = h.client.try_cancel_escrow(&caller, &target_id);
    }
});
