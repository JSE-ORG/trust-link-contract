use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol};

use crate::{ResolutionType, ResolverVote};

/// Event topic/data schemas used by the escrow contract.
///
/// Each emitter publishes a single-symbol topic and a structured data payload.
/// The topic symbol is the canonical event name and the payload is the data XDR
/// stored by the Soroban event log.

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeUpdated {
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Fee"), symbol_short!("Updated"),)`, data: `FeeUpdated`.
pub fn emit_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (symbol_short!("Fee"), symbol_short!("Updated")),
        FeeUpdated {
            old_fee_bps,
            new_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolFeeUpdated {
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("ProtoFee"), symbol_short!("Updated"),)`, data: `ProtocolFeeUpdated`.
pub fn emit_protocol_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (symbol_short!("ProtoFee"), symbol_short!("Updated")),
        ProtocolFeeUpdated {
            old_fee_bps,
            new_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArbitrationFeeUpdated {
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("ArbFee"), symbol_short!("Updated"),)`, data: `ArbitrationFeeUpdated`.
pub fn emit_arbitration_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (symbol_short!("ArbFee"), symbol_short!("Updated")),
        ArbitrationFeeUpdated {
            old_fee_bps,
            new_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminRotated {
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Admin"), symbol_short!("Rotated"),)`, data: `AdminRotated`.
pub fn emit_admin_rotated(env: &Env, old_admin: Address, new_admin: Address) {
    env.events().publish(
        (symbol_short!("Admin"), symbol_short!("Rotated")),
        AdminRotated {
            old_admin,
            new_admin,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractPausedEvent {
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
            admin,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractUnpausedEvent {
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
            admin,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesWithdrawn {
    pub token: Address,
    pub to: Address,
    pub amount: i128,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Fee"), symbol_short!("Withdrawn"), to.clone(),)`, data: `FeesWithdrawn`.
pub fn emit_fees_withdrawn(env: &Env, token: Address, to: Address, amount: i128) {
    env.events().publish(
        (symbol_short!("Fee"), symbol_short!("Withdrawn"), to.clone()),
        FeesWithdrawn {
            token,
            to,
            amount,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCreated {
    pub escrow_id: u64,
    pub seller: Address,
    pub resolver: Address,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub resolver_fee_bps: u32,
    pub shipping_window: u64,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
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
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Escrow"),
            symbol_short!("Created"),
            seller.clone(),
        ),
        EscrowCreated {
            escrow_id,
            seller,
            resolver,
            token,
            amount,
            fee_bps,
            resolver_fee_bps,
            shipping_window,
            timestamp: env.ledger().timestamp(),
            prev_state,
            new_state,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowFunded {
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
    pub escrow_id: u64,
    pub delivered_at: u64,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Delivered"),)`, data: `DeliveryRecorded`.
pub fn emit_delivery_recorded(env: &Env, escrow_id: u64, delivered_at: u64) {
    env.events().publish(
        (symbol_short!("Escrow"), symbol_short!("Delivered")),
        DeliveryRecorded {
            escrow_id,
            delivered_at,
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowCompleted {
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
    pub escrow_id: u64,
    pub seller: Address,
    pub timestamp: u64,
    pub prev_state: crate::EscrowState,
    pub new_state: crate::EscrowState,
}

/// Topic: `(symbol_short!("Escrow"), symbol_short!("Canceled"), seller.clone(),)`, data: `EscrowCancelled`.
pub fn emit_escrow_cancelled(
    env: &Env,
    escrow_id: u64,
    seller: Address,
    prev_state: crate::EscrowState,
    new_state: crate::EscrowState,
) {
    env.events().publish(
        (
            symbol_short!("Escrow"),
            symbol_short!("Canceled"),
            seller.clone(),
        ),
        EscrowCancelled {
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
pub struct ContractInitialized {
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
            token,
            added,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowlistToggled {
    pub enabled: bool,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Allowlist"), symbol_short!("Toggled"),)`, data: `AllowlistToggled`.
pub fn emit_allowlist_toggled(env: &Env, enabled: bool) {
    env.events().publish(
        (symbol_short!("Allowlist"), symbol_short!("Toggled")),
        AllowlistToggled {
            enabled,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputePendingFinalization {
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
            escrow_id,
            appellant,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeUpdated {
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("PlatFee"), symbol_short!("Updated"),)`, data: `PlatformFeeUpdated`.
pub fn emit_platform_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32) {
    env.events().publish(
        (symbol_short!("PlatFee"), symbol_short!("Updated")),
        PlatformFeeUpdated {
            old_fee_bps,
            new_fee_bps,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryUpdated {
    pub old_treasury: Address,
    pub new_treasury: Address,
    pub timestamp: u64,
}

/// Topic: `(symbol_short!("Treasury"), symbol_short!("Updated"),)`, data: `TreasuryUpdated`.
pub fn emit_treasury_updated(env: &Env, old_treasury: Address, new_treasury: Address) {
    env.events().publish(
        (symbol_short!("Treasury"), symbol_short!("Updated")),
        TreasuryUpdated {
            old_treasury,
            new_treasury,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasketEscrowCreated {
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
            escrow_id,
            sender,
            timestamp: env.ledger().timestamp(),
        },
    );
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundRequestedEvent {
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
    pub admin: Address,
    pub new_wasm_hash: soroban_sdk::BytesN<32>,
    pub timestamp: u64,
}

/// Topic: `(\"contract_upgraded\",)`, data: `ContractUpgradedEvent`.
pub fn emit_contract_upgraded(env: &Env, admin: Address, new_wasm_hash: soroban_sdk::BytesN<32>) {
    env.events().publish(
        (Symbol::new(env, "contract_upgraded"),),
        ContractUpgradedEvent {
            admin,
            new_wasm_hash,
            timestamp: env.ledger().timestamp(),
        },
    );
}
