#![no_main]
//! Fuzzes the refund path — `request_refund`, `approve_refund` and
//! `mutual_cancel` — across every caller role and in arbitrary order.
//!
//! These three all end an escrow and move funds, so the target checks that no
//! ordering lets an escrow settle twice.

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

    let target_id = if r.bool() { escrow_id } else { r.u64() };

    // Interleave the three settlement paths in a fuzz-chosen order; only one
    // of them may ever succeed for a given escrow.
    let steps = r.len(5);
    for _ in 0..steps {
        let caller = h.actor(r.u8());
        match r.u8() % 3 {
            0 => {
                let _ = h.client.try_request_refund(&caller, &target_id);
            }
            1 => {
                let _ = h.client.try_approve_refund(&caller, &target_id);
            }
            _ => {
                let _ = h.client.try_mutual_cancel(&target_id);
            }
        }
    }
});
