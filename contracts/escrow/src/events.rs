#![allow(deprecated)]

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol};

use crate::ResolutionType;

/// Schema version stamped into every event payload.
///
/// Increment this constant whenever a field is added, removed, or renamed in
/// any event struct.  Consumers can use it to guard against decoding stale
/// snapshots with the wrong XDR shape.
pub const EVENT_SCHEMA_VERSION: u32 = 2;

/// Event topic/data schemas used by the escrow contract.
///
/// Each emitter publishes a single-symbol topic and a structured data payload.
/// The topic symbol is the canonical event name and the payload is the data XDR
/// stored by the Soroban event log.

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeUpdated {
    pub schema_version: u32,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Fee"), symbol_short!("Updated"),)`, data: `FeeUpdated`.
pub fn emit_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (symbol_short!("Fee"), symbol_short!("Updated")),
        FeeUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_fee_bps,
            new_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolFeeUpdated {
    pub schema_version: u32,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("ProtoFee"), symbol_short!("Updated"),)`, data: `ProtocolFeeUpdated`.
pub fn emit_protocol_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (symbol_short!("ProtoFee"), symbol_short!("Updated")),
        ProtocolFeeUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_fee_bps,
            new_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArbitrationFeeUpdated {
    pub schema_version: u32,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("ArbFee"), symbol_short!("Updated"),)`, data: `ArbitrationFeeUpdated`.
pub fn emit_arbitration_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (symbol_short!("ArbFee"), symbol_short!("Updated")),
        ArbitrationFeeUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_fee_bps,
            new_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRotated {
    pub schema_version: u32,
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Admin"), symbol_short!("Rotated"),)`, data: `AdminRotated`.
pub fn emit_admin_rotated(env: &Env, old_admin: Address, new_admin: Address) {
    env.events().publish(
        (symbol_short!("Admin"), symbol_short!("Rotated")),
        AdminRotated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_admin,
            new_admin,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractPausedEvent {
    pub schema_version: u32,
    pub admin: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Contract"), symbol_short!("Paused"), admin.clone(),)`, data: `ContractPausedEvent`.
pub fn emit_contract_paused(env: &Env, admin: Address) {
    env.events().publish(
        (
            symbol_short!("Contract"),
            symbol_short!("Paused"),
            admin.clone(),
        ),
        ContractPausedEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            admin,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractUnpausedEvent {
    pub schema_version: u32,
    pub admin: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Contract"), symbol_short!("Unpaused"), admin.clone(),)`, data: `ContractUnpausedEvent`.
pub fn emit_contract_unpaused(env: &Env, admin: Address) {
    env.events().publish(
        (
            symbol_short!("Contract"),
            symbol_short!("Unpaused"),
            admin.clone(),
        ),
        ContractUnpausedEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            admin,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCreated {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub seller: Address,
    pub resolver: Address,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub resolver_fee_bps: u32,
    pub shipping_window: u64,
    pub timestamp: u64,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Created"), seller.clone(),)`, data: `EscrowCreated`.
#[allow(clippy::too_many_arguments)]
pub fn emit_escrow_created(
    env: &Env,
    escrow_id: u64,
    seller: Address,
    resolver: Address,
    token: Address,
    amount: i128,
    fee_bps: u32,
    resolver_fee_bps: u32,
    shipping_window: u64,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Escrow"),
            symbol_short!("Created"),
            seller.clone(),
        ),
        EscrowCreated {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            seller,
            resolver,
            token,
            amount,
            fee_bps,
            resolver_fee_bps,
            shipping_window,
            timestamp: env.ledger().timestamp(),
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowFunded {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub buyer: Address,
    pub amount: i128,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Funded"), buyer.clone(),)`, data: `EscrowFunded`.
pub fn emit_escrow_funded(
    env: &Env,
    escrow_id: u64,
    buyer: Address,
    amount: i128,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Escrow"),
            symbol_short!("Funded"),
            buyer.clone(),
        ),
        EscrowFunded {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            buyer,
            amount,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowShipped {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub seller: Address,
    pub tracking_id: String,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Shipped"), seller.clone(),)`, data: `EscrowShipped`.
pub fn emit_escrow_shipped(
    env: &Env,
    escrow_id: u64,
    seller: Address,
    tracking_id: String,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Escrow"),
            symbol_short!("Shipped"),
            seller.clone(),
        ),
        EscrowShipped {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            seller,
            tracking_id,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryRecorded {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub delivered_at: u64,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Delivered"),)`, data: `DeliveryRecorded`.
pub fn emit_delivery_recorded(env: &Env, escrow_id: u64, delivered_at: u64) {
    env.events().publish(
        (symbol_short!("Escrow"), symbol_short!("Delivered")),
        DeliveryRecorded {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            delivered_at,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCompleted {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub recipient: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Completed"), recipient.clone(),)`, data: `EscrowCompleted`.
pub fn emit_escrow_completed(
    env: &Env,
    escrow_id: u64,
    recipient: Address,
    amount: i128,
    fee_bps: u32,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Escrow"),
            symbol_short!("Completed"),
            recipient.clone(),
        ),
        EscrowCompleted {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            recipient,
            amount,
            fee_bps,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRaised {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub buyer: Address,
    pub reason: Symbol,
    pub description: String,
    pub evidence_hash: BytesN<32>,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Dispute"), symbol_short!("Raised"), buyer.clone(),)`, data: `DisputeRaised`.
#[allow(clippy::too_many_arguments)]
pub fn emit_dispute_raised(
    env: &Env,
    escrow_id: u64,
    buyer: Address,
    reason: Symbol,
    description: String,
    evidence_hash: BytesN<32>,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Dispute"),
            symbol_short!("Raised"),
            buyer.clone(),
        ),
        DisputeRaised {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            buyer,
            reason,
            description,
            evidence_hash,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeResolved {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub resolver: Address,
    pub resolution: ResolutionType,
    pub recipient: Address,
    pub amount: i128,
    pub arbitration_fee: i128,
    pub resolver_fee: i128,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Dispute"), symbol_short!("Resolved"), resolver.clone(),)`, data: `DisputeResolved`.
#[allow(clippy::too_many_arguments)]
pub fn emit_dispute_resolved(
    env: &Env,
    escrow_id: u64,
    resolver: Address,
    resolution: ResolutionType,
    recipient: Address,
    amount: i128,
    arbitration_fee: i128,
    resolver_fee: i128,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Dispute"),
            symbol_short!("Resolved"),
            resolver.clone(),
        ),
        DisputeResolved {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            resolver,
            resolution,
            recipient,
            amount,
            arbitration_fee,
            resolver_fee,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverVoteRecorded {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub resolver: Address,
    pub resolution: ResolutionType,
    pub vote_count: u32,
    pub threshold: u32,
    pub voted_at: u64,
}

/// Topic: `(\"resolver_vote_recorded\",)`, data: `ResolverVoteRecorded`.
pub fn emit_resolver_vote_recorded(
    env: &Env,
    escrow_id: u64,
    resolver: Address,
    resolution: ResolutionType,
    vote_count: u32,
    threshold: u32,
) {
    env.events().publish(
        (Symbol::new(env, "resolver_vote_recorded"),),
        ResolverVoteRecorded {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            resolver,
            resolution,
            vote_count,
            threshold,
            voted_at: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoReleased {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub seller: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Released"), seller.clone(),)`, data: `AutoReleased`.
pub fn emit_auto_released(
    env: &Env,
    escrow_id: u64,
    seller: Address,
    amount: i128,
    fee_bps: u32,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Escrow"),
            symbol_short!("Released"),
            seller.clone(),
        ),
        AutoReleased {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            seller,
            amount,
            fee_bps,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCancelled {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub seller: Address,
    /// Address that actually initiated the cancellation (buyer or a payee/seller).
    /// `seller` always reflects the escrow's seller; `cancelled_by` reflects the caller.
    pub cancelled_by: Address,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Canceled"), cancelled_by.clone(),)`, data: `EscrowCancelled`.
///
/// `seller` is the escrow's seller (first payee). `cancelled_by` is the caller that
/// triggered the cancellation, which may be the buyer rather than the seller.
pub fn emit_escrow_cancelled(
    env: &Env,
    escrow_id: u64,
    seller: Address,
    cancelled_by: Address,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Escrow"),
            symbol_short!("Canceled"),
            cancelled_by.clone(),
        ),
        EscrowCancelled {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            seller,
            cancelled_by,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractInitialized {
    pub schema_version: u32,
    pub admin: Address,
    pub fee_collector: Address,
    pub arbitration_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Contract"), symbol_short!("Init"),)`, data: `ContractInitialized`.
pub fn emit_contract_initialized(
    env: &Env,
    admin: Address,
    fee_collector: Address,
    arbitration_fee_bps: u32,
) {
    env.events().publish(
        (symbol_short!("Contract"), symbol_short!("Init")),
        ContractInitialized {
            schema_version: EVENT_SCHEMA_VERSION,
            admin,
            fee_collector,
            arbitration_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverRotated {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub old_resolver: Address,
    pub new_resolver: Address,
    pub rotated_at: u64,
}

/// Topic: `(symbol_short!("Resolver"), symbol_short!("Rotated"),)`, data: `ResolverRotated`.
pub fn emit_resolver_rotated(
    env: &Env,
    escrow_id: u64,
    old_resolver: Address,
    new_resolver: Address,
) {
    env.events().publish(
        (symbol_short!("Resolver"), symbol_short!("Rotated")),
        ResolverRotated {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            old_resolver,
            new_resolver,
            rotated_at: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenAllowlistUpdated {
    pub schema_version: u32,
    pub token: Address,
    pub added: bool,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Token"), symbol_short!("Allowlist"), token.clone(),)`, data: `TokenAllowlistUpdated`.
pub fn emit_token_allowlist_updated(env: &Env, token: Address, added: bool) {
    env.events().publish(
        (
            symbol_short!("Token"),
            symbol_short!("Allowlist"),
            token.clone(),
        ),
        TokenAllowlistUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            token,
            added,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowlistToggled {
    pub schema_version: u32,
    pub enabled: bool,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Allowlist"), symbol_short!("Toggled"),)`, data: `AllowlistToggled`.
pub fn emit_allowlist_toggled(env: &Env, enabled: bool) {
    env.events().publish(
        (symbol_short!("Allowlist"), symbol_short!("Toggled")),
        AllowlistToggled {
            schema_version: EVENT_SCHEMA_VERSION,
            enabled,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputePendingFinalization {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub resolver: Address,
    pub resolution: crate::ResolutionType,
    pub amount: i128,
    pub appeal_deadline: u64,
    pub pending_at: u64,
}

/// Topic: `(symbol_short!("Dispute"), symbol_short!("Pending"), resolver.clone(),)`, data: `DisputePendingFinalization`.
pub fn emit_dispute_pending_finalization(
    env: &Env,
    escrow_id: u64,
    resolver: Address,
    resolution: crate::ResolutionType,
    amount: i128,
    appeal_deadline: u64,
) {
    env.events().publish(
        (
            symbol_short!("Dispute"),
            symbol_short!("Pending"),
            resolver.clone(),
        ),
        DisputePendingFinalization {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            resolver,
            resolution,
            amount,
            appeal_deadline,
            pending_at: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeAppealed {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub appellant: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Dispute"), symbol_short!("Appealed"), appellant.clone(),)`, data: `DisputeAppealed`.
pub fn emit_dispute_appealed(env: &Env, escrow_id: u64, appellant: Address) {
    env.events().publish(
        (
            symbol_short!("Dispute"),
            symbol_short!("Appealed"),
            appellant.clone(),
        ),
        DisputeAppealed {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            appellant,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeUpdated {
    pub schema_version: u32,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("PlatFee"), symbol_short!("Updated"),)`, data: `PlatformFeeUpdated`.
pub fn emit_platform_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (symbol_short!("PlatFee"), symbol_short!("Updated")),
        PlatformFeeUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_fee_bps,
            new_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryUpdated {
    pub schema_version: u32,
    pub old_treasury: Address,
    pub new_treasury: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Treasury"), symbol_short!("Updated"),)`, data: `TreasuryUpdated`.
pub fn emit_treasury_updated(env: &Env, old_treasury: Address, new_treasury: Address) {
    env.events().publish(
        (symbol_short!("Treasury"), symbol_short!("Updated")),
        TreasuryUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_treasury,
            new_treasury,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasketEscrowCreated {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub seller: Address,
    pub token_count: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Basket"), symbol_short!("Created"), seller.clone(),)`, data: `BasketEscrowCreated`.
pub fn emit_basket_escrow_created(env: &Env, escrow_id: u64, seller: Address, token_count: u32) {
    env.events().publish(
        (
            symbol_short!("Basket"),
            symbol_short!("Created"),
            seller.clone(),
        ),
        BasketEscrowCreated {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            seller,
            token_count,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagePosted {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub sender: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Message"), symbol_short!("Posted"), sender.clone(),)`, data: `MessagePosted`.
pub fn emit_message_posted(env: &Env, escrow_id: u64, sender: Address) {
    env.events().publish(
        (
            symbol_short!("Message"),
            symbol_short!("Posted"),
            sender.clone(),
        ),
        MessagePosted {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            sender,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundRequestedEvent {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub buyer: Address,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Refund"), symbol_short!("Requested"), buyer.clone(),)`, data: `RefundRequestedEvent`.
pub fn emit_refund_requested(
    env: &Env,
    escrow_id: u64,
    buyer: Address,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Refund"),
            symbol_short!("Requested"),
            buyer.clone(),
        ),
        RefundRequestedEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            buyer,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundApprovedEvent {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub seller: Address,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Refund"), symbol_short!("Approved"), seller.clone(),)`, data: `RefundApprovedEvent`.
pub fn emit_refund_approved(
    env: &Env,
    escrow_id: u64,
    seller: Address,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Refund"),
            symbol_short!("Approved"),
            seller.clone(),
        ),
        RefundApprovedEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            seller,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractUpgradedEvent {
    pub schema_version: u32,
    pub admin: Address,
    pub new_wasm_hash: soroban_sdk::BytesN<32>,
    pub timestamp: u64,
}

/// Topic: `("contract_upgraded",)`, data: `ContractUpgradedEvent`.
pub fn emit_contract_upgraded(env: &Env, admin: Address, new_wasm_hash: soroban_sdk::BytesN<32>) {
    env.events().publish(
        (Symbol::new(env, "contract_upgraded"),),
        ContractUpgradedEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            admin,
            new_wasm_hash,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageMigratedEvent {
    pub schema_version: u32,
    pub admin: Address,
    pub from_version: u32,
    pub to_version: u32,
    pub timestamp: u64,
}

/// Topic: `("storage_migrated",)`, data: `StorageMigratedEvent`.
pub fn emit_storage_migrated(env: &Env, admin: Address, from_version: u32, to_version: u32) {
    env.events().publish(
        (Symbol::new(env, "storage_migrated"),),
        StorageMigratedEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            admin,
            from_version,
            to_version,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtlExtensionUpdated {
    pub schema_version: u32,
    pub old_ledgers: u32,
    pub new_ledgers: u32,
    pub caller: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("TtlExt"), symbol_short!("Updated"),)`, data: `TtlExtensionUpdated`.
pub fn emit_ttl_extension_updated(env: &Env, old_ledgers: u32, new_ledgers: u32, caller: Address) {
    env.events().publish(
        (symbol_short!("TtlExt"), symbol_short!("Updated")),
        TtlExtensionUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_ledgers,
            new_ledgers,
            caller,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AmountLimitsUpdated {
    pub schema_version: u32,
    pub old_min_amount: i128,
    pub new_min_amount: i128,
    pub old_max_amount: i128,
    pub new_max_amount: i128,
    pub caller: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("AmtLimit"), symbol_short!("Updated"),)`, data: `AmountLimitsUpdated`.
#[allow(clippy::too_many_arguments)]
pub fn emit_amount_limits_updated(
    env: &Env,
    old_min_amount: i128,
    new_min_amount: i128,
    old_max_amount: i128,
    new_max_amount: i128,
    caller: Address,
) {
    env.events().publish(
        (symbol_short!("AmtLimit"), symbol_short!("Updated")),
        AmountLimitsUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_min_amount,
            new_min_amount,
            old_max_amount,
            new_max_amount,
            caller,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionPausedEvent {
    pub schema_version: u32,
    pub action: Symbol,
    pub caller: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Action"), symbol_short!("Paused"), action.clone(),)`, data: `ActionPausedEvent`.
pub fn emit_action_paused(env: &Env, action: Symbol, caller: Address) {
    env.events().publish(
        (
            symbol_short!("Action"),
            symbol_short!("Paused"),
            action.clone(),
        ),
        ActionPausedEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            action,
            caller,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionUnpausedEvent {
    pub schema_version: u32,
    pub action: Symbol,
    pub caller: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Action"), symbol_short!("Unpaused"), action.clone(),)`, data: `ActionUnpausedEvent`.
pub fn emit_action_unpaused(env: &Env, action: Symbol, caller: Address) {
    env.events().publish(
        (
            symbol_short!("Action"),
            symbol_short!("Unpaused"),
            action.clone(),
        ),
        ActionUnpausedEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            action,
            caller,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverApproved {
    pub schema_version: u32,
    pub resolver: Address,
    pub caller: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Resolver"), symbol_short!("Approved"), resolver.clone(),)`, data: `ResolverApproved`.
pub fn emit_resolver_approved(env: &Env, resolver: Address, caller: Address) {
    env.events().publish(
        (
            symbol_short!("Resolver"),
            symbol_short!("Approved"),
            resolver.clone(),
        ),
        ResolverApproved {
            schema_version: EVENT_SCHEMA_VERSION,
            resolver,
            caller,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverRemoved {
    pub schema_version: u32,
    pub resolver: Address,
    pub caller: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Resolver"), symbol_short!("Removed"), resolver.clone(),)`, data: `ResolverRemoved`.
pub fn emit_resolver_removed(env: &Env, resolver: Address, caller: Address) {
    env.events().publish(
        (
            symbol_short!("Resolver"),
            symbol_short!("Removed"),
            resolver.clone(),
        ),
        ResolverRemoved {
            schema_version: EVENT_SCHEMA_VERSION,
            resolver,
            caller,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverStrictUpdated {
    pub schema_version: u32,
    pub old_strict: bool,
    pub new_strict: bool,
    pub caller: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("ResStrct"), symbol_short!("Updated"),)`, data: `ResolverStrictUpdated`.
pub fn emit_resolver_strict_updated(
    env: &Env,
    old_strict: bool,
    new_strict: bool,
    caller: Address,
) {
    env.events().publish(
        (symbol_short!("ResStrct"), symbol_short!("Updated")),
        ResolverStrictUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_strict,
            new_strict,
            caller,
            timestamp: env.ledger().timestamp(),
        },
    );
}

// ── Fee Collector Updated ─────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeCollectorUpdated {
    pub schema_version: u32,
    pub old_collector: Address,
    pub new_collector: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("FeeColl"), symbol_short!("Updated"),)`, data: `FeeCollectorUpdated`.
pub fn emit_fee_collector_updated(env: &Env, old_collector: Address, new_collector: Address) {
    env.events().publish(
        (symbol_short!("FeeColl"), symbol_short!("Updated")),
        FeeCollectorUpdated {
            schema_version: EVENT_SCHEMA_VERSION,
            old_collector,
            new_collector,
            timestamp: env.ledger().timestamp(),
        },
    );
}

// ── Emergency Drain ───────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyDrain {
    pub schema_version: u32,
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Emergency"), symbol_short!("Drain"),)`, data: `EmergencyDrain`.
pub fn emit_emergency_drain(env: &Env, escrow_id: u64, buyer: Address, seller: Address) {
    env.events().publish(
        (symbol_short!("Emergency"), symbol_short!("Drain")),
        EmergencyDrain {
            schema_version: EVENT_SCHEMA_VERSION,
            escrow_id,
            buyer,
            seller,
            timestamp: env.ledger().timestamp(),
        },
    );
}
