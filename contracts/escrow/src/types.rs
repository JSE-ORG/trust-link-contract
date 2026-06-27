use soroban_sdk::{contracttype, Address, BytesN, Env, String, Symbol, Vec};

/// Storage keys for persisting escrow data and the global escrow counter.
#[contracttype]
pub enum DataKey {
    Admin,
    Escrow(u64),
    EscrowCounter,
    FeeCollector,
    Dispute(u64),
    Paused,
    DefaultFeeBps,
    TtlExtensionLedgers,
    ArbitrationFee,
    TotalArbitrationFees(Address),
    AccumulatedFees(Address),
    TotalCreated,
    TotalDisputed,
    TotalCompleted,
    Messages(u64),
    TotalRefunded,
    FeeConfig,
    BuyerEscrowIndex(Address),
    TokenAllowlistEnabled,
    TokenAllowlist,
    PlatformFeeBps,
    Treasury,
    MinAmount,
    MaxAmount,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Active,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeData {
    pub escrow_id: u64,
    pub reason: Symbol,
    pub description: String,
    pub evidence_hash: BytesN<32>,
    pub status: DisputeStatus,
    pub disputed_at: u64,
    pub tracking_id: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolutionType {
    Release,
    Refund,
}

/// Configuration for protocol and arbitration fee rates in basis points.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeConfig {
    pub protocol_fee_bps: u32,
    pub arbitration_fee_bps: u32,
}

/// Public-safe contract configuration (no sensitive addresses).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicContractConfig {
    pub fee_bps: u32,
    pub paused: bool,
    pub escrow_count: u64,
}

/// Full contract configuration including privileged addresses.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractConfig {
    pub admin: Address,
    pub fee_bps: u32,
    pub fee_collector: Address,
    pub escrow_count: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowData {
    pub seller: Address,
    pub buyer: Option<Address>,
    pub resolver: Address,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub shipping_window: u64,
    pub funded_at: u64,
    pub dispute_deadline: u64,
    pub state: EscrowState,
    pub shipped_at: u64,
    pub delivered_at: Option<u64>,
    pub tracking_id: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowInput {
    pub buyer: Option<Address>,
    pub resolver: Address,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub shipping_window: u64,
    pub notes: Option<String>,
}


#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub sender: Address,
    pub timestamp: u64,
    pub content: String,
}
/// On-chain counters for escrow lifecycle events.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractStats {
    pub total_created: u64,
    pub total_completed: u64,
    pub total_disputed: u64,
    pub total_refunded: u64,
}

/// Payee with address and basis points share.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payee {
    pub address: Address,
    pub bps: u32,
}

/// Lifecycle states of an escrow transaction.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowState {
    Pending,
    Funded,
    Shipped,
    Completed,
    Disputed,
    RefundRequested,
    Refunded,
    Canceled,
    PendingFinalization,
}
