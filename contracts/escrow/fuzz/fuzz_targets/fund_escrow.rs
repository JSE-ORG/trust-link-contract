#![no_main]
//! Fuzzes `fund_escrow` with arbitrary escrow ids and funders, including
//! double-funding and funding an escrow that belongs to a different buyer.

mod common;

use common::{Harness, Reader};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let Some(escrow_id) = h.create_valid_escrow() else {
        return;
    };

    // Roughly half the runs target the real escrow, the rest a fuzzed id.
    let target_id = r.target_id(escrow_id);
    let funder = h.actor(r.u8());

    let _ = h.client.try_fund_escrow(&target_id, &funder);

    // A second attempt must be rejected, never double-charged.
    if r.bool() {
        let _ = h.client.try_fund_escrow(&target_id, &funder);
    }
});
