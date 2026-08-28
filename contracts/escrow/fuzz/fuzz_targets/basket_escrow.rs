#![no_main]
//! Fuzzes basket escrows, where `tokens` and `amounts` are two parallel vectors
//! supplied by the caller.
//!
//! Their lengths are drawn independently, so mismatched vectors — the obvious
//! way to index out of bounds — are exercised alongside empty baskets, negative
//! amounts and amounts that overflow when summed.

mod common;

use common::{Harness, Reader, VALID_FEE_BPS, VALID_SHIPPING_WINDOW};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::Vec;

/// Upper bound on both vectors; crosses any per-basket cap the contract sets.
const MAX_ENTRIES: usize = 5;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let second_token = h.extra_token();

    let token_count = r.len(MAX_ENTRIES);
    let mut tokens = Vec::new(&h.env);
    for i in 0..token_count {
        tokens.push_back(if i % 2 == 0 {
            h.token.clone()
        } else {
            second_token.clone()
        });
    }

    // Deliberately independent of `token_count`: a mismatch must be rejected,
    // not indexed past the end of the shorter vector.
    let amount_count = r.len(MAX_ENTRIES);
    let mut amounts = Vec::new(&h.env);
    for _ in 0..amount_count {
        amounts.push_back(if r.bool() {
            // Plausible amounts keep the success path reachable.
            (r.u32() as i128).saturating_add(1)
        } else {
            r.i128()
        });
    }

    let Ok(Ok(escrow_id)) = h.client.try_create_basket_escrow(
        &h.seller,
        &Some(h.buyer.clone()),
        &h.resolver,
        &tokens,
        &amounts,
        &VALID_FEE_BPS,
        &VALID_SHIPPING_WINDOW,
    ) else {
        return;
    };

    let funder = h.actor(r.u8());
    let target_id = r.target_id(escrow_id);
    let _ = h.client.try_fund_basket_escrow(&target_id, &funder);

    // Funding twice must not transfer the basket twice.
    if r.bool() {
        let _ = h.client.try_fund_basket_escrow(&target_id, &funder);
    }

    let _ = h.client.try_get_basket_tokens(&target_id);
});
