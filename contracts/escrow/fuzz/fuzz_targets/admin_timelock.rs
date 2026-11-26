#![no_main]
//! Fuzzes the admin timelock: every mutating admin action is queued, becomes
//! executable after a fixed delay, and can be cancelled before then.
//!
//! The queued values, the caller and the ledger timestamp all come from the
//! fuzz input, so the target covers early execution, execution long after the
//! window opens, cancellation races, re-queuing over a pending operation and
//! execution attempts by non-admins.

mod common;

use common::{Harness, Reader};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Ledger as _;

/// Mirrors `ADMIN_TIMELOCK_DELAY_SECONDS` in contracts/escrow/src/admin.rs.
/// The module is private, so the delay is restated here; the target only uses
/// it to bias timestamps toward the boundary, so a drift makes the fuzzing
/// less focused rather than incorrect.
const ADMIN_TIMELOCK_DELAY: u64 = 24 * 60 * 60;

/// `TimelockOperation::SetArbitrationFee`, the discriminant `cancel_timelock_op`
/// expects for the operation this target queues.
const OP_SET_ARBITRATION_FEE: u32 = 4;

fuzz_target!(|data: &[u8]| {
    let mut r = Reader::new(data);
    let h = Harness::new();

    let queued_at = r.timestamp();
    h.env.ledger().set_timestamp(queued_at);

    let caller = h.actor(r.u8());
    let fee_bps = if r.bool() { r.u8() as u32 } else { r.u32() };

    // Queue a mix of operations so the shared timelock slot machinery is
    // exercised, not just one code path.
    match r.u8() % 4 {
        0 => {
            let _ = h.client.try_queue_set_arbitration_fee(&caller, &fee_bps);
        }
        1 => {
            let _ = h.client.try_queue_set_platform_fee(&caller, &fee_bps);
        }
        2 => {
            let _ = h.client.try_queue_set_ttl_extension(&caller, &fee_bps);
        }
        _ => {
            let min = r.i128();
            let max = r.i128();
            let _ = h.client.try_queue_set_amount_limits(&caller, &min, &max);
        }
    }

    // Re-queuing before execution must overwrite or be rejected, never leave
    // two pending operations in one slot.
    if r.bool() {
        let _ = h
            .client
            .try_queue_set_arbitration_fee(&h.admin, &(r.u8() as u32));
    }

    // Bias half the runs to just past the delay so the execute path is reached
    // regularly; the rest explore the whole range, including before the delay.
    let executed_at = if r.bool() {
        queued_at
            .saturating_add(ADMIN_TIMELOCK_DELAY)
            .saturating_add(r.u32() as u64)
    } else {
        r.timestamp()
    };
    h.env.ledger().set_timestamp(executed_at);

    if r.bool() {
        let canceller = h.actor(r.u8());
        let _ = h
            .client
            .try_cancel_timelock_op(&canceller, &OP_SET_ARBITRATION_FEE);
    }

    let executor = h.actor(r.u8());
    let _ = h.client.try_execute_set_arbitration_fee(&executor);

    // A cancelled or already-executed operation must not execute again.
    if r.bool() {
        let _ = h.client.try_execute_set_arbitration_fee(&executor);
    }
});
