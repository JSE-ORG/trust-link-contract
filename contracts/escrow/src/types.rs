use soroban_sdk::{contracttype, Address, BytesN, String, Symbol, Vec};

/// Single unified storage key enum for all contract storage entries.
///
/// Storage-tier rationale:
/// - Instance keys store singleton/global configuration and counters.
/// - Persistent keys store per-escrow data and user indexes that must survive
///   contract instance TTL changes.
#[contracttype]
pub enum DataKey {
    // Instance storage: global singleton values.
    Admin,
    EscrowCounter,
    FeeCollector,
    FeeConfig,
    Paused,
    ActionPaused(Symbol),
    TtlExtensionLedgers,
    TokenAllowlistEnabled,
    TokenAllowlist,
    PlatformFeeBps,
    Treasury,
    MaxAmount,
    MinAmount,
    ApprovedResolvers,
    ResolverStrict,
    AccumulatedFees(Address),
    /// Schema version of the data currently in storage. Absent on contracts
    /// deployed before migrations existed, which is read as version 0.
    StorageVersion,

    // Persistent storage: per-escrow data and user indexes.
    Escrow(u64),
    EscrowStateHistory(u64),
    Dispute(u64),
    Messages(u64),
    PendingExpiry(u64),
    ResolverVotes(u64),
    BuyerEscrowIndex(Address),
    VendorEscrowIndex(Address),
    TotalArbitrationFees(Address),
    TotalCreated,
    TotalDisputed,
    TotalCompleted,
    TotalRefunded,
    /// Reserved for future use. Currently unused; may store per-escrow
    /// evidence audit logs if on-chain verification is added later.
    EvidenceLog(u64),
    BasketTokens(u64),
    DeliveryProposal(u64),
    TimelockOp(u32),
}

/// A token-amount pair for multi-token basket escrows.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenEntry {
    pub token: Address,
    pub amount: i128,
}

/// Optional per-escrow expiration schedule, set via `create_escrow_with_expiration`.
///
/// `expires_at` is the hard deadline: `fund_escrow` and `mark_shipped` both
/// reject once `now >= expires_at`. `grace_period` is additional buffer time
/// *after* `expires_at` during which `reclaim_expired` is still blocked, to
/// protect a fund/ship transaction that's still landing right at the
/// deadline. Only after `expires_at + grace_period` has passed can the buyer
/// actually reclaim funds.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpirySchedule {
    pub expires_at: u64,
    pub grace_period: u64,
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
            ResolverSet::Multi(m) => m.resolvers.len(),
            ResolverSet::Fallback(_) => 2,
        }
    }

    /// Checks if an address is in this resolver set.
    pub fn contains(&self, addr: &Address) -> bool {
        match self {
            ResolverSet::Single(resolver) => addr == resolver,
            ResolverSet::Multi(m) => crate::internal::contains(&m.resolvers, addr),
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

    /// Returns true if `addr` is authorized to act as a resolver *right now*.
    ///
    /// Differs from `contains()` (identity membership only, used for
    /// seller/buyer conflict checks at creation time) by additionally
    /// enforcing `FallbackResolver.dispute_deadline`: the primary resolver
    /// may always act, but the backup is only authorized once `now` has
    /// reached the deadline, so the backup can't preempt the primary's
    /// window to resolve.
    pub fn can_resolve_now(&self, addr: &Address, now: u64) -> bool {
        match self {
            ResolverSet::Fallback(f) => {
                if addr == &f.primary {
                    true
                } else if addr == &f.backup {
                    now >= f.dispute_deadline
                } else {
                    false
                }
            }
            _ => self.contains(addr),
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
    /// Resolution code: 0 = not resolved, 1 = Release, 2 = Refund
    pub resolution: u32,
    /// Which address made the resolution (the resolver who triggered finalization)
    pub resolved_by: Option<Address>,
    /// Number of times this dispute has been appealed
    pub appeal_count: u32,
    /// Timestamp when the resolution was made
    pub resolved_at: u64,
    /// Arbitration fee deducted when the resolution transition executed
    pub arbitration_fee: i128,
    /// Resolver fee paid out when the resolution transition executed
    pub resolver_fee: i128,
}

impl DisputeData {
    pub fn set_resolution(&mut self, r: ResolutionType) {
        self.resolution = match r {
            ResolutionType::Release => 1,
            ResolutionType::Refund => 2,
        };
    }

    pub fn get_resolution(&self) -> Option<ResolutionType> {
        match self.resolution {
            1 => Some(ResolutionType::Release),
            2 => Some(ResolutionType::Refund),
            _ => None,
        }
    }

    pub fn clear_resolution(&mut self) {
        self.resolution = 0;
        self.resolved_by = None;
        self.resolved_at = 0;
        self.arbitration_fee = 0;
        self.resolver_fee = 0;
    }
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
    pub arbitration_fee_bps: u32,
    pub paused: bool,
    pub escrow_count: u64,
}

/// Full contract configuration including privileged addresses.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractConfig {
    pub admin: Address,
    pub fee_bps: u32,
    pub arbitration_fee_bps: u32,
    pub fee_collector: Address,
    pub escrow_count: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowData {
    pub payees: Vec<Payee>,
    pub buyer: Option<Address>,
    pub resolvers: ResolverSet,
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

/// Identifies a specific privileged admin operation subject to the two-step
/// timelock delay. The numeric discriminant is used as the `TimelockOp(u32)`
/// storage key. Do not renumber existing entries; append new operations only.
#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum TimelockOperation {
    SetAdmin = 1,
    Upgrade = 2,
    SetProtocolFee = 3,
    SetArbitrationFee = 4,
    SetPlatformFee = 5,
    SetTreasury = 6,
    SetFeeCollector = 7,
    SetTtlExtension = 8,
    SetAmountLimits = 9,
    AddApprovedResolver = 10,
    RemoveApprovedResolver = 11,
    SetResolverStrict = 12,
    SetTokenAllowlistEnabled = 13,
    AddAllowedToken = 14,
    RemoveAllowedToken = 15,
    PauseContract = 16,
    UnpauseContract = 17,
}

/// A queued admin change awaiting the 24-hour timelock delay before it can be
/// executed. `params` is a contract-serialised `Vec<Val>` mirroring the
/// operation's function arguments (excluding `env` and `caller/admin`),
/// encoded with `IntoVal` / decoded with `TryFromVal` in the execute step.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelockProposal {
    pub operation: TimelockOperation,
    pub proposer: Address,
    pub params: Vec<soroban_sdk::Val>,
    pub queued_at: u64,
    pub ready_at: u64,
}
