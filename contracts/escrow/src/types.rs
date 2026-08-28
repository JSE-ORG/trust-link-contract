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

/// Fallback resolver scheme consisting of a primary resolver and a backup resolver.
///
/// The primary resolver has immediate authority to resolve disputes. If the dispute
/// is not resolved before `dispute_deadline` (Unix timestamp in seconds), the backup
/// resolver becomes authorized to resolve the dispute, preventing permanent deadlock
/// or abandonment if the primary resolver becomes unresponsive.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallbackResolver {
    /// The primary dispute arbitrator who has authority to resolve disputes at any time.
    pub primary: Address,
    /// The backup dispute arbitrator who becomes authorized once ledger timestamp >= `dispute_deadline`.
    pub backup: Address,
    /// The Unix timestamp (in seconds) after which the backup resolver is permitted to resolve disputes.
    pub dispute_deadline: u64,
}

/// Resolver configuration: single resolver (backward compat),
/// multiple resolvers with M-of-N voting threshold, or
/// primary/backup fallback resolver with a takeover deadline.
/// Primary/backup resolver pair with a time-based handover.
///
/// Used by `create_escrow_with_fallback` for the "my resolver went dark"
/// case: the `primary` resolver is expected to handle disputes, and if they
/// don't act in time the `backup` is allowed to step in and resolve instead.
///
/// The handover is governed by the `dispute_deadline` field and enforced in
/// [`ResolverSet::can_resolve_now`]:
///
/// | Caller           | Before `dispute_deadline` | At / after `dispute_deadline` |
/// |------------------|---------------------------|-------------------------------|
/// | `primary`        | ✅ may resolve             | ✅ may resolve                 |
/// | `backup`         | ❌ `NotAuthorized`         | ✅ may resolve                 |
/// | anyone else      | ❌ `NotAuthorized`         | ❌ `NotAuthorized`             |
///
/// The primary is never time-gated — the deadline only *adds* the backup as
/// an authorized resolver, it never removes the primary.
///
/// # Example
///
/// ```ignore
/// // Backup may take over 3 days (259_200 s) after this escrow is created.
/// let created_at = env.ledger().timestamp();
/// let fallback = FallbackResolver {
///     primary: primary_resolver,
///     backup: backup_resolver,
///     dispute_deadline: created_at + 259_200,
/// };
/// ```
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallbackResolver {
    /// Resolver expected to handle disputes. Always authorized to resolve,
    /// regardless of the current ledger time.
    pub primary: Address,
    /// Stand-in resolver. Only authorized once the ledger timestamp has
    /// reached `dispute_deadline`.
    pub backup: Address,
    /// Absolute ledger timestamp (Unix seconds) at which `backup` becomes an
    /// authorized resolver. The comparison is `now >= dispute_deadline`, so
    /// the backup is authorized *exactly* at this instant.
    ///
    /// This is **not** the same value as `EscrowData::dispute_deadline`,
    /// which the contract computes at funding time (`funded_at +
    /// DISPUTE_WINDOW`) to bound the *buyer's* window to raise a dispute.
    /// This field is chosen by the caller of `create_escrow_with_fallback`
    /// and only controls *which resolver* may act.
    ///
    /// Not range-checked on creation: a value in the past (including `0`)
    /// simply means the backup is co-authorized with the primary from the
    /// start.
    pub dispute_deadline: u64,
}

/// Resolver configuration for an escrow, chosen at creation time.
///
/// - `Single` — one resolver (the original, backward-compatible mode; every
///   `create_escrow*` entry point except the multi/fallback ones produces this).
/// - `Multi` — an M-of-N resolver committee that votes (`create_escrow_multi`).
/// - `Fallback` — a primary resolver with a time-delayed backup
///   (`create_escrow_with_fallback`); see [`FallbackResolver`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolverSet {
    /// Single resolver (backward compatible mode)
    Single(Address),
    /// Multiple resolvers with M-of-N voting threshold
    Multi(MultiResolver),
    /// Primary resolver with a backup resolver that can take over after `dispute_deadline`
    /// Primary resolver with a backup that becomes authorized once the
    /// fallback's `dispute_deadline` (an absolute ledger timestamp) is
    /// reached. The primary is never time-gated. See [`FallbackResolver`].
    Fallback(FallbackResolver),
}

impl ResolverSet {
    /// Returns the number of resolvers in this set (`Fallback` counts both the
    /// primary and the backup, even though only one acts on any given dispute).
    pub fn count(&self) -> u32 {
        match self {
            ResolverSet::Single(_) => 1,
            ResolverSet::Multi(m) => m.resolvers.len(),
            ResolverSet::Fallback(_) => 2,
        }
    }

    /// Checks whether `addr` is one of this set's resolvers, ignoring any time
    /// gating. For a `Fallback` set this is true for both the primary and the
    /// backup regardless of `dispute_deadline` — it is the identity check used
    /// at creation time for seller/buyer conflict detection. Use
    /// [`Self::can_resolve_now`] to decide whether `addr` may resolve *now*.
    pub fn contains(&self, addr: &Address) -> bool {
        match self {
            ResolverSet::Single(resolver) => addr == resolver,
            ResolverSet::Multi(m) => crate::internal::contains(&m.resolvers, addr),
            ResolverSet::Fallback(f) => addr == &f.primary || addr == &f.backup,
        }
    }

    /// Number of matching votes needed to resolve a dispute: `1` for `Single`,
    /// `M` for an M-of-N `Multi` committee, and `1` for `Fallback` (whichever
    /// of the primary or backup is currently authorized decides alone).
    pub fn threshold(&self) -> u32 {
        match self {
            ResolverSet::Single(_) => 1,
            ResolverSet::Multi(m) => m.threshold,
            ResolverSet::Fallback(_) => 1,
        }
    }

    /// Returns true if `addr` is authorized to act as a resolver *right now*.
    ///
    /// Differs from [`Self::contains`] (identity membership only, used for
    /// seller/buyer conflict checks at creation time) by additionally
    /// enforcing `FallbackResolver::dispute_deadline`: the primary resolver
    /// may always act, but the backup is only authorized once `now` has
    /// reached the deadline (`now >= dispute_deadline`), so the backup can't
    /// preempt the primary's window to resolve. For `Single` and `Multi` sets
    /// there is no time gating and this is exactly `contains(addr)`.
    ///
    /// `now` is the current ledger timestamp in Unix seconds
    /// (`env.ledger().timestamp()`).
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

    /// Clears the recorded resolution so a fresh round of voting can begin
    /// after an appeal.
    ///
    /// `arbitration_fee` and `resolver_fee` are intentionally left in place:
    /// they record the amounts already deducted from the escrow for this
    /// dispute. The resolution transition reads them to charge those fees
    /// **once per dispute** rather than again for every appeal round (see
    /// `execute_resolution_transition`).
    /// Clears the recorded resolution so a fresh round of voting can begin
    /// after an appeal.
    ///
    /// `arbitration_fee` and `resolver_fee` are intentionally left in place:
    /// they record the amounts already deducted from the escrow for this
    /// dispute. The resolution transition reads them to charge those fees
    /// **once per dispute** rather than again for every appeal round (see
    /// `execute_resolution_transition`).
    pub fn clear_resolution(&mut self) {
        self.resolution = 0;
        self.resolved_by = None;
        self.resolved_at = 0;
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
