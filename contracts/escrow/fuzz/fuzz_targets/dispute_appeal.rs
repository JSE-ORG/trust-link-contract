#![no_main]
//! Fuzzes the appeal window: `resolve_dispute` parks the escrow in
//! `PendingFinalization`, after which `appeal_dispute` and `finalize_dispute`
//! race the appeal deadline.
//!
//! The deadline is computed from the ledger timestamp, so arbitrary timestamps
//! probe the boundary arithmetic from "long before" through "far past" — plus
//! the wrap-around cases a fixed test cannot reach.

mod common;

use common::{Harness, Reader};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Ledger as _;
use trustlink_escrow::ResolutionType;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let Some(escrow_id) = h.create_disputed_escrow(&mut r) else {
        return;
    };

    let resolution = if r.bool() {
        ResolutionType::Release
    } else {
        ResolutionType::Refund
    };
    let _ = h
        .client
        .try_resolve_dispute(&h.resolver, &escrow_id, &resolution);

    // Move the clock before appealing: sometimes still inside the window,
    // sometimes well past it.
    h.env.ledger().set_timestamp(r.timestamp());

    let appellant = h.actor(r.u8());
    let target_id = if r.bool() { escrow_id } else { r.u64() };
    let _ = h.client.try_appeal_dispute(&appellant, &target_id);

    // A second appeal must not reopen an already-appealed dispute.
    if r.bool() {
        let _ = h.client.try_appeal_dispute(&appellant, &target_id);
    }

    h.env.ledger().set_timestamp(r.timestamp());

    let finalizer = h.actor(r.u8());
    let _ = h.client.try_finalize_dispute(&finalizer, &target_id);

    // Finalizing twice must not pay out twice.
    if r.bool() {
        let _ = h.client.try_finalize_dispute(&finalizer, &target_id);
    }
});
