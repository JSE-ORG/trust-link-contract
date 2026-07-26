#![no_main]
//! Fuzzes `confirm_delivery` across arbitrary ledger timestamps, exercising the
//! dispute-window boundary from "far too early" to "long past".

mod common;

use common::{Harness, Reader};
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

    h.env.ledger().set_timestamp(r.timestamp());

    let caller = h.actor(r.u8());
    let target_id = if r.bool() { escrow_id } else { r.u64() };

    let _ = h.client.try_confirm_delivery(&caller, &target_id);
});
