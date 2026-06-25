#![no_std]
#![allow(deprecated, clippy::too_many_arguments)]

//! TrustLink Soroban escrow contract.
//!
//! The contract is split across modules: `types` (data types & storage keys),
//! `errors`, `events`, `storage`, `helpers`, and `escrow` (the contract logic).
//! Public items are re-exported here so the crate's external ABI and the test
//! suite's `crate::*` / `super::*` paths are unchanged.

pub mod errors;
pub mod escrow;
pub mod events;
pub mod helpers;
pub mod storage;
pub mod types;

pub use crate::errors::ContractError;
pub use crate::events::{
    AdminRotated, AutoReleased, ContractInitialized, ContractPausedEvent, ContractUnpausedEvent,
    DeliveryRecorded, DisputeRaised, DisputeResolved, EscrowCancelled, EscrowCompleted,
    EscrowCreated, EscrowFunded, EscrowShipped, FeeUpdated, FeesWithdrawn, ArbitrationFeeUpdated,
    ProtocolFeeUpdated, ResolverRotated,
    emit_admin_rotated, emit_auto_released, emit_contract_initialized, emit_contract_paused,
    emit_contract_unpaused, emit_delivery_recorded, emit_dispute_raised, emit_dispute_resolved,
    emit_escrow_cancelled, emit_escrow_completed, emit_escrow_created, emit_escrow_funded,
    emit_escrow_shipped, emit_fee_updated, emit_fees_withdrawn, emit_arbitration_fee_updated,
    emit_protocol_fee_updated, emit_resolver_rotated,
};
pub use crate::types::{
    ContractConfig, ContractStats, DataKey, DisputeData, DisputeStatus, EscrowData, EscrowState,
    FeeConfig, PublicContractConfig, ResolutionType,
};
pub use crate::escrow::{
    transition_state, Escrow, EscrowClient, MAX_DESCRIPTION_LEN, MAX_ESCROW_AMOUNT,
    MAX_TRACKING_ID_LEN, MIN_ESCROW_AMOUNT,
};
// Re-exported for the in-crate test modules that reference `super::deduct_and_transfer`.
#[cfg(test)]
pub(crate) use crate::escrow::deduct_and_transfer;

mod test;
mod test_edge_cases;
mod test_withdraw_fees;
mod test_dispute;
mod test_escrow_id;
mod test_resolution;
mod test_pause;
mod test_overflow;
mod test_fee_minimum;
mod test_minimum_amount_guard;
mod test_fee_calculation_accuracy;
mod test_arbitration_fee;
mod test_fee_config;
mod test_helpers;
mod test_admin;
mod test_ttl;
mod test_escrow_states;
mod test_admin_rotation;
mod test_auto_release;
mod test_initialize_twice;
mod test_initialize_zero_admin;
mod test_contract_config;
mod test_string_length;
mod test_get_escrows_by_buyer;
mod test_delivery;
mod test_auth_ordering;
mod test_dispute_flow;
mod test_set_fee_boundary;
mod test_cancel_restrictions;
mod test_dispute_window;
mod test_unauthorized;
mod test_concurrent_vendor_escrows;
mod test_not_found;
mod test_get_escrows_by_vendor;
mod test_resolver_rotation;
