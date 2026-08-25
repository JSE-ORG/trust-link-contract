#![no_main]
//! Fuzzes `resolve_dispute` over both resolution types and every caller role,
//! including callers that are not the escrow's registered resolver.

mod common;

use common::{Harness, Reader, DISPUTE_REASONS};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{testutils::Ledger as _, BytesN, Symbol};
use trustlink_escrow::ResolutionType;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let Some(escrow_id) = h.create_funded_escrow() else {
        return;
    };

    let reason = Symbol::new(
        &h.env,
        DISPUTE_REASONS[(r.u8() as usize) % DISPUTE_REASONS.len()],
    );
    let description = r.ascii_string(&h.env, 64);
    let evidence_hash = BytesN::from_array(&h.env, &r.bytes32());
    let _ = h
        .client
        .try_raise_dispute(&h.buyer, &escrow_id, &reason, &description, &evidence_hash);

    h.env.ledger().set_timestamp(r.timestamp());

    let resolution = if r.bool() {
        ResolutionType::Release
    } else {
        ResolutionType::Refund
    };
    let caller = h.actor(r.u8());
    let target_id = if r.bool() { escrow_id } else { r.u64() };

    let _ = h
        .client
        .try_resolve_dispute(&caller, &target_id, &resolution);

    // A resolved dispute must not be resolvable again.
    if r.bool() {
        let _ = h
            .client
            .try_resolve_dispute(&caller, &target_id, &resolution);
    }
});
