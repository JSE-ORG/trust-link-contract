#![no_main]
//! Fuzzes multi-resolver voting: `create_escrow_multi` registers a resolver set
//! with a threshold, and `vote` tallies until the threshold is reached.
//!
//! Both the resolver count and the threshold come from the fuzz input, so the
//! target covers thresholds of zero, thresholds larger than the resolver set,
//! duplicate votes from one resolver and votes from addresses outside the set.

mod common;

use common::{Harness, Reader, VALID_AMOUNT, VALID_FEE_BPS, VALID_SHIPPING_WINDOW};
use libfuzzer_sys::fuzz_target;
use trustlink_escrow::ResolutionType;

/// Upper bound on the generated resolver set. Large enough to cross whatever
/// cap the contract enforces, small enough to keep each run cheap.
const MAX_RESOLVERS: usize = 6;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let resolvers = h.addresses(r.len(MAX_RESOLVERS));
    let threshold = (r.u8() as u32) % (MAX_RESOLVERS as u32 + 2);

    let Ok(Ok(escrow_id)) = h.client.try_create_escrow_multi(
        &h.seller,
        &Some(h.buyer.clone()),
        &resolvers,
        &threshold,
        &h.token,
        &VALID_AMOUNT,
        &VALID_FEE_BPS,
        &VALID_SHIPPING_WINDOW,
    ) else {
        return;
    };

    if h.client.try_fund_escrow(&escrow_id, &h.buyer).is_err() {
        return;
    }

    let reason = h.dispute_reason(&mut r);
    let description = r.ascii_string(&h.env, 32);
    let evidence_hash = soroban_sdk::BytesN::from_array(&h.env, &r.bytes32());
    let _ = h
        .client
        .try_raise_dispute(&h.buyer, &escrow_id, &reason, &description, &evidence_hash);

    // Cast up to one vote per registered resolver, plus occasional votes from
    // an address that was never registered.
    let rounds = r.len(MAX_RESOLVERS + 2);
    for _ in 0..rounds {
        let resolution = if r.bool() {
            ResolutionType::Release
        } else {
            ResolutionType::Refund
        };

        let voter = match resolvers.get(r.u8() as u32 % resolvers.len().max(1)) {
            Some(address) if !r.bool() => address,
            _ => h.actor(r.u8()),
        };

        let _ = h.client.try_vote(&voter, &escrow_id, &resolution);
    }
});
