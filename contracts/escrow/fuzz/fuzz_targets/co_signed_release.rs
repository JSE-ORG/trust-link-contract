#![no_main]
//! Fuzzes `co_signed_release`, which requires both sides of the escrow to sign
//! off before funds move.
//!
//! Callers are drawn from every role including outsiders, and the call is
//! repeated, so the target covers one-sided releases, both-sided releases and
//! attempts to release an already-released escrow.

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

    // Several signatures in a fuzz-chosen order: the release may only settle
    // once, no matter how the co-signatures arrive.
    let signatures = r.len(4);
    for _ in 0..signatures {
        let caller = h.actor(r.u8());
        let _ = h.client.try_co_signed_release(&caller, &target_id);
    }

    // A disputed escrow must not be releasable by co-signature.
    if r.bool() {
        let reason = h.dispute_reason(&mut r);
        let description = r.ascii_string(&h.env, 32);
        let evidence_hash = soroban_sdk::BytesN::from_array(&h.env, &r.bytes32());
        let _ =
            h.client
                .try_raise_dispute(&h.buyer, &target_id, &reason, &description, &evidence_hash);
        let _ = h.client.try_co_signed_release(&h.buyer, &target_id);
    }
});
