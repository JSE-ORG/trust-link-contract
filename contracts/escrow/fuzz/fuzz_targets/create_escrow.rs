#![no_main]
//! Fuzzes `create_escrow` with arbitrary amounts, fee splits and shipping
//! windows to check that every rejected combination returns a `ContractError`
//! rather than panicking.

mod common;

use common::{Harness, Reader};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::String as SorobanString;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let amount = r.i128();
    let fee_bps = r.u32();
    let resolver_fee_bps = r.u32();
    let shipping_window = r.u64();
    let with_buyer = r.bool();
    // 600 exceeds MAX_NOTES_LEN (500), so the length guard is exercised too.
    let notes = if r.bool() {
        Some(r.ascii_string(&h.env, 600))
    } else {
        None::<SorobanString>
    };

    let buyer = if with_buyer {
        Some(h.buyer.clone())
    } else {
        None
    };

    let _ = h.client.try_create_escrow(
        &h.payees(),
        &buyer,
        &h.resolver,
        &h.token,
        &amount,
        &fee_bps,
        &resolver_fee_bps,
        &shipping_window,
        &notes,
    );
});
