#![no_main]
//! Fuzzes `batch_create_escrow`, which creates many escrows from one call.
//!
//! The batch length and every field of every entry come from the fuzz input, so
//! the target covers empty batches, batches past the contract's cap, and
//! batches where one bad entry must roll the whole call back rather than leave
//! half the escrows created.

mod common;

use common::{Harness, Reader, VALID_AMOUNT, VALID_FEE_BPS, VALID_SHIPPING_WINDOW};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{String as SorobanString, Vec};
use trustlink_escrow::EscrowInput;

/// Upper bound on the generated batch; crosses whatever cap the contract sets.
const MAX_BATCH: usize = 8;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let count = r.len(MAX_BATCH);
    let mut escrows = Vec::new(&h.env);
    for _ in 0..count {
        // Half the entries stay valid so the success path is reached
        // regularly; the rest probe validation.
        let valid = r.bool();
        escrows.push_back(EscrowInput {
            buyer: if r.bool() {
                Some(h.buyer.clone())
            } else {
                None
            },
            resolver: h.resolver.clone(),
            token: h.token.clone(),
            amount: if valid { VALID_AMOUNT } else { r.i128() },
            fee_bps: if valid { VALID_FEE_BPS } else { r.u32() },
            shipping_window: if valid {
                VALID_SHIPPING_WINDOW
            } else {
                r.u64()
            },
            notes: if r.bool() {
                Some(r.ascii_string(&h.env, 64))
            } else {
                Option::<SorobanString>::None
            },
        });
    }

    let caller = h.actor(r.u8());
    let Ok(Ok(ids)) = h.client.try_batch_create_escrow(&caller, &escrows) else {
        return;
    };

    // A successful batch must return exactly one id per input entry, and each
    // id must resolve to a real escrow.
    assert_eq!(
        ids.len() as usize,
        count,
        "batch_create_escrow returned {} ids for {} inputs",
        ids.len(),
        count
    );
    for id in ids.iter() {
        // `try_get_escrow` reports a missing escrow as `Ok(Err(..))`, so both
        // levels have to be checked for this to assert anything.
        assert!(
            matches!(h.client.try_get_escrow(&id), Ok(Ok(_))),
            "batch_create_escrow returned unreadable escrow id {id}"
        );
    }
});
