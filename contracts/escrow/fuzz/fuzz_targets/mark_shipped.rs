#![no_main]
//! Fuzzes `mark_shipped` with arbitrary callers and tracking ids, covering
//! empty and over-long identifiers around `MAX_TRACKING_ID_LEN`.

mod common;

use common::{Harness, Reader};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let Some(escrow_id) = h.create_funded_escrow() else {
        return;
    };

    let caller = h.actor(r.u8());
    // 96 straddles MAX_TRACKING_ID_LEN (64), so both sides of the bound run.
    let tracking_id = r.ascii_string(&h.env, 96);
    let target_id = r.target_id(escrow_id);

    let _ = h.client.try_mark_shipped(&caller, &target_id, &tracking_id);

    // Shipping twice must not advance the state a second time.
    if r.bool() {
        let _ = h.client.try_mark_shipped(&caller, &target_id, &tracking_id);
    }
});
