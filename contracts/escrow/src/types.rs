use soroban_sdk::{contracttype, Address, BytesN, String, Symbol, Vec};

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
    TotalCompleted,
    TotalDisputed,
    TotalRefunded,
    FeeConfig,
    BuyerEscrowIndex(Address),
    // Multi-resolver votes storage
    ResolverVotes(u64), // escrow_id -> Vec<ResolverVote>
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Active,
    Resolved,
}

/// Resolver configuration: either a single resolver (backward compat)
/// or multiple resolvers with a voting threshold.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverSet {
    /// Single resolver (backward compatible mode)
    Single(Address),
    /// Multiple resolvers with M-of-N voting threshold
    Multi {
        resolvers: Vec<Address>,
        threshold: u32, // minimum votes required (M in M-of-N)
    },
}

impl ResolverSet {
    /// Returns the number of resolvers in this set.
    pub fn count(&self) -> u32 {
        match self {
            ResolverSet::Single(_) => 1,
            ResolverSet::Multi { resolvers, .. } => resolvers.len() as u32,
        }
    }

    /// Checks if an address is in this resolver set.
    pub fn contains(&self, addr: &Address) -> bool {
        match self {
            ResolverSet::Single(resolver) => addr == resolver,
            ResolverSet::Multi { resolvers, .. } => {
                for resolver in resolvers {
                    if resolver == addr {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Returns the threshold required for voting (1 for single, M for multi).
    pub fn threshold(&self) -> u32 {
        match self {
            ResolverSet::Single(_) => 1,
            ResolverSet::Multi { threshold, .. } => *threshold,
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

/// On-chain counters for escrow lifecycle events.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractStats {
    pub total_created: u64,
    pub total_completed: u64,
    pub total_disputed: u64,
    pub total_refunded: u64,
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
    Refunded,
    Canceled,
}
