#![no_main]
//! Fuzzes `raise_dispute` with arbitrary callers, descriptions and evidence
//! hashes, including the zero hash and descriptions past `MAX_DESCRIPTION_LEN`.

mod common;

use common::{Harness, Reader, DISPUTE_REASONS};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{BytesN, Symbol};

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
    // 320 straddles MAX_DESCRIPTION_LEN (256).
    let description = r.ascii_string(&h.env, 320);
    let evidence_hash = BytesN::from_array(&h.env, &r.bytes32());
    let caller = h.actor(r.u8());
    let target_id = if r.bool() { escrow_id } else { r.u64() };

    let _ = h
        .client
        .try_raise_dispute(&caller, &target_id, &reason, &description, &evidence_hash);

    // Raising the same dispute twice must not corrupt the dispute record.
    if r.bool() {
        let _ =
            h.client
                .try_raise_dispute(&caller, &target_id, &reason, &description, &evidence_hash);
    }
});
