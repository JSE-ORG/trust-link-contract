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
    ActionPaused(Symbol),
    DefaultFeeBps,
    TtlExtensionLedgers,
    TotalArbitrationFees(Address),
    AccumulatedFees(Address),
    TotalCreated,
    TotalDisputed,
    TotalCompleted,
    Messages(u64),
    TotalRefunded,
    FeeConfig,
    BuyerEscrowIndex(Address),
    // Multi-resolver votes storage
    ResolverVotes(u64), // escrow_id -> Vec<ResolverVote>
    TokenAllowlistEnabled,
    TokenAllowlist,
    PlatformFeeBps,
    Treasury,
    MaxAmount,
    MinAmount,
    PendingExpiry(u64),
    ApprovedResolvers,
    ResolverStrict,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Active,
    Resolved,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiResolver {
    pub resolvers: Vec<Address>,
    pub threshold: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallbackResolver {
    pub primary: Address,
    pub backup: Address,
    pub dispute_deadline: u64,
}

/// Resolver configuration: either a single resolver (backward compat)
/// or multiple resolvers with a voting threshold.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverSet {
    /// Single resolver (backward compatible mode)
    Single(Address),
    /// Multiple resolvers with M-of-N voting threshold
    Multi(MultiResolver),
    /// Primary resolver with a backup that can resolve after a deadline
    Fallback(FallbackResolver),
}

impl ResolverSet {
    /// Returns the number of resolvers in this set.
    pub fn count(&self) -> u32 {
        match self {
            ResolverSet::Single(_) => 1,
            ResolverSet::Multi(m) => m.resolvers.len() as u32,
            ResolverSet::Fallback(_) => 2,
        }
    }

    /// Checks if an address is in this resolver set.
    pub fn contains(&self, addr: &Address) -> bool {
        match self {
            ResolverSet::Single(resolver) => addr == resolver,
            ResolverSet::Multi(m) => {
                for resolver in m.resolvers.clone() {
                    if resolver == *addr {
                        return true;
                    }
                }
                false
            },
            ResolverSet::Fallback(f) => addr == &f.primary || addr == &f.backup,
        }
    }

    /// Returns the threshold required for voting (1 for single, M for multi).
    pub fn threshold(&self) -> u32 {
        match self {
            ResolverSet::Single(_) => 1,
            ResolverSet::Multi(m) => m.threshold,
            ResolverSet::Fallback(_) => 1,
        }
    }
}

/// A vote from a resolver on a disputed escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolverVote {
    pub resolver: Address,
    pub resolution: ResolutionType,
    pub voted_at: u64,
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
    pub payees: Vec<Payee>,
    pub buyer: Option<Address>,
    pub resolver: Address,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub resolver_fee_bps: u32,
    pub shipping_window: u64,
    pub funded_at: u64,
    pub dispute_deadline: u64,
    pub shipped_at: u64,
    pub delivered_at: Option<u64>,
    pub tracking_id: Option<String>,
    pub state: EscrowState,
    pub notes: Option<String>,
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
    Expired,
}
