#![no_main]
//! Fuzzes `post_message` and the `get_messages` pagination that reads it back.
//!
//! Message content is length-validated on chain, and `get_messages` takes a
//! `start`/`limit` pair straight from the caller — arbitrary offsets past the
//! end of the log are the obvious way to index out of bounds, so both are
//! drawn from the fuzz input.

mod common;

use common::{Harness, Reader};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Ledger as _;

/// Comfortably past the contract's content cap, so over-long content is
/// rejected rather than truncated.
const MAX_CONTENT: usize = 320;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let Some(escrow_id) = h.create_funded_escrow() else {
        return;
    };

    let posts = r.len(4);
    for _ in 0..posts {
        h.env.ledger().set_timestamp(r.timestamp());

        let sender = h.actor(r.u8());
        let content = r.ascii_string(&h.env, MAX_CONTENT);
        let target_id = if r.bool() { escrow_id } else { r.u64() };

        let _ = h.client.try_post_message(&target_id, &sender, &content);
    }

    // Pagination must stay in bounds for any start/limit, including offsets
    // beyond the end of the log and limits large enough to overflow a naive
    // `start + limit`.
    let start = if r.bool() { r.u64() } else { r.u8() as u64 };
    let limit = if r.bool() { r.u64() } else { r.u8() as u64 };
    let _ = h.client.try_get_messages(&escrow_id, &start, &limit);

    // Reading a nonexistent escrow's log must not panic either.
    if r.bool() {
        let _ = h.client.try_get_messages(&r.u64(), &start, &limit);
    }
});
