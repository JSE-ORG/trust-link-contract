use soroban_sdk::{contracttype, Address, Bytes, BytesN, String, Symbol, Vec};

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
    /// Per-escrow log of additional dispute evidence (`Vec<EvidenceEntry>`),
    /// appended to by `submit_dispute_evidence` and capped at
    /// `MAX_EVIDENCE_ENTRIES` entries.
    EvidenceLog(u64),
    BasketTokens(u64),
    DeliveryProposal(u64),
    TimelockOp(u32),
    /// Admin-configured maximum duration (seconds) a dispute may remain
    /// unresolved before either party can force a refund via
    /// `claim_dispute_timeout`. Absent means `DEFAULT_DISPUTE_TIMEOUT`.
    DisputeTimeout,
    /// Sharded lifecycle counter, keyed by `(kind, bucket)`. Replaces the four
    /// singleton `Total*` keys, which serialized every create / complete /
    /// dispute / refund transition on a single instance-storage entry. See
    /// `internal::increment_sharded_counter`.
    ShardedCounter(u32, u32),

    // Paged storage keys. These shard collections that used to live in a single
    // unbounded `Vec` entry, so each storage read/write touches a bounded slot
    // instead of paying for (and eventually exceeding) the whole collection.
    // Appended at the end so existing variant discriminants are unchanged.
    /// A single message: `Message(escrow_id, index)`. Replaces the monolithic
    /// `Messages(escrow_id)` vector for targeted, O(page) reads.
    Message(u64, u32),
    /// Number of messages stored for an escrow. Drives the paged `Message`
    /// reads without loading the collection.
    MessageCount(u64),
    /// A bounded page of a buyer's escrow-id index: `(buyer, page_index)`.
    BuyerEscrow(Address, u32),
    /// Total number of escrow ids indexed for a buyer.
    BuyerEscrowCount(Address),
    /// A bounded page of a vendor's escrow-id index: `(vendor, page_index)`.
    VendorEscrow(Address, u32),
    /// Total number of escrow ids indexed for a vendor.
    VendorEscrowCount(Address),
    /// Pending fee collector address awaiting acceptance via `accept_fee_collector`.
    PendingFeeCollector,
    /// Pending treasury address awaiting acceptance via `accept_treasury`.
    PendingTreasury,
    /// Admin-configured maximum number of appeals per dispute.
    MaxAppeals,
    /// Admin-configured maximum number of tokens in a basket escrow.
    MaxBasketSize,
    // Appended after paging keys so all pre-existing discriminants are unchanged.
    /// Appeal fee in basis points charged to the appellant on `appeal_dispute`
    /// (issue #913). Absent means `DEFAULT_APPEAL_FEE_BPS` (0 = disabled).
    AppealFeeBps,
    /// Graceful-shutdown flag (issue #914). When true, `recovery_withdraw`
    /// lets buyers reclaim custodied funds without going through the standard
    /// state machine. Absent means false.
    RecoveryMode,
    /// Timelock delay (seconds) applied to privileged admin operations.
    /// Absent means `ADMIN_TIMELOCK_DELAY_SECONDS` (24 hours); valid values
    /// lie in `MIN_TIMELOCK_DELAY_SECONDS..=MAX_TIMELOCK_DELAY_SECONDS`.
    ///
    /// Changed only through the two-step `queue_set_timelock_delay` ->
    /// `execute_set_timelock_delay` flow, and that change waits
    /// `max(current_delay, new_delay)` (#1238): shortening the delay costs the
    /// full current delay, lengthening it costs the new one. Proposals queued
    /// before the change keep the `ready_at` they were queued with.
    AdminTimelockDelay,
    /// Every admin-tunable fee, limit and toggle, packed into one
    /// [`GlobalConfig`] entry so hot paths pay for a single instance-storage
    /// read instead of one per setting. Supersedes the individual
    /// `FeeCollector`, `Treasury`, `FeeConfig`, `PlatformFeeBps`,
    /// `AppealFeeBps`, `MinAmount`, `MaxAmount`, `DisputeTimeout`,
    /// `TtlExtensionLedgers`, `Paused`, `TokenAllowlistEnabled`,
    /// `ResolverStrict` and `RecoveryMode` keys, which are kept only so
    /// pre-v2 deployments can still be read and migrated.
    GlobalConfig,
    /// Capacity-bounded basket value ([`BoundedBasket`]). Written by
    /// `save_basket_tokens`; the legacy `BasketTokens(u64)` key holds the old
    /// unbounded `Vec<TokenEntry>` layout and is read/migrated on first load,
    /// so deploying this change does not require an explicit storage migration.
    BasketTokensV2(u64),
    /// Persistent on-chain log of contract upgrades. Each entry is an
    /// [`UpgradeRecord`] appended by `execute_upgrade`. Stored as a
    /// bounded `Vec` under a single persistent key; callers read it via
    /// `get_upgrade_history`.
    UpgradeHistory,
    /// Marks a proposal invalidated by an admin rotation until its slot is requeued.
    InvalidatedTimelockOp(u32),
}

/// A token-amount pair for multi-token basket escrows.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenEntry {
    pub token: Address,
    pub amount: i128,
}

/// Hard capacity of a stored basket, mirroring the on-chain
/// [`crate::MAX_BASKET_SIZE`] ceiling.
///
/// The issue that introduced this wrapper described a capacity of 10; the
/// contract's hard ceiling has since grown to 20, so the type bound tracks the
/// canonical constant rather than a stale literal. Governance can tighten the
/// effective limit further via `MaxBasketSize`, but never beyond this bound.
pub const BASKET_CAPACITY: u32 = crate::MAX_BASKET_SIZE;

/// A capacity-bounded wrapper around `Vec<TokenEntry>` for basket storage.
///
/// Stored as a single-field `#[contracttype]` struct instead of a bare,
/// unbounded `Vec`, so the XDR shape is explicit and capacity is checked at
/// every construction point. Together with the bounded helper methods this
/// keeps host metering predictable for basket reads/writes and removes the
/// unbounded-vector length prefix from the storage value.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedBasket {
    pub inner: Vec<TokenEntry>,
}

impl BoundedBasket {
    /// Constructs a basket, rejecting oversized vectors (defence in depth for
    /// the `create_basket_escrow` size checks).
    pub fn try_new(inner: Vec<TokenEntry>) -> Result<Self, crate::ContractError> {
        if inner.len() > BASKET_CAPACITY {
            return Err(crate::ContractError::BasketTokenMismatch);
        }
        Ok(Self { inner })
    }

    /// Constructs a trusted basket (e.g. one just validated by the caller).
    /// Storage-boundary code uses [`Self::try_new`] instead.
    ///
    /// # Panics
    /// Panics if `inner` has more than [`BASKET_CAPACITY`] entries. Callers
    /// holding untrusted vectors must use [`Self::try_new`].
    pub fn new(inner: Vec<TokenEntry>) -> Self {
        assert!(
            inner.len() <= BASKET_CAPACITY,
            "basket exceeds BASKET_CAPACITY"
        );
        Self { inner }
    }

    pub fn get(&self, i: u32) -> Option<TokenEntry> {
        self.inner.get(i)
    }

    pub fn len(&self) -> u32 {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the underlying vector (for public queries and legacy migration).
    pub fn into_inner(self) -> Vec<TokenEntry> {
        self.inner
    }
}

/// Pledged versus custodied balance for one basket token.
///
/// `pledged_amount` is the amount recorded at escrow creation;
/// `contract_balance` is the contract's current SEP-41 balance for `token`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenBalanceInfo {
    pub token: Address,
    pub pledged_amount: i128,
    pub contract_balance: i128,
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

/// An M-of-N resolver committee that votes on dispute resolutions
/// (`create_escrow_multi`).
///
/// # Deadlock risk (Issue #707 / related: #667)
///
/// **Known issue**: voting can permanently deadlock when split votes prevent
/// either side from reaching `threshold`. Example: `threshold=3` with 3
/// resolvers, getting 1 `Release` + 1 `Refund` + 1 abstention — neither side
/// reaches 3. Worse, if `threshold == N` (unanimous) and every resolver has
/// voted but votes are split (e.g. 2 `Release` + 1 `Refund` with
/// `threshold=3`), no additional votes are possible and funds remain frozen
/// in `Disputed` indefinitely.
///
/// **Escape hatches**: the deadlock above is bounded by two implemented
/// fallbacks:
///
/// 1. [`crate::Escrow::resolve_deadlocked_dispute`] — once a dispute has been
///    split (threshold not met) for `DISPUTE_DEADLOCK_WINDOW`, anyone may
///    trigger the simple-majority fallback (`tally_votes_majority`).
/// 2. [`crate::Escrow::claim_dispute_timeout`] — if the dispute stays
///    unresolved for the admin-configured `DisputeTimeout`, either party can
///    force a `Refund`.
///
/// Operators should still configure `threshold` carefully (e.g. avoid
/// `threshold == N` for `N > 1`) so that a majority is reachable in practice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiResolver {
    pub resolvers: Vec<Address>,
    pub threshold: u32,
}

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
/// # How `dispute_deadline` affects authorization
///
/// - **Checked at call time, not dispute time.** The comparison runs every
///   time `resolve_dispute` / `vote` is invoked, against the ledger timestamp
///   of *that* call. When the dispute was raised is irrelevant: if the
///   deadline has already passed when a dispute is raised, the backup may
///   resolve it immediately.
/// - **Absolute, not relative.** The deadline is fixed at creation and is not
///   extended by funding, shipping, raising a dispute, or an appeal. After the
///   deadline both resolvers stay authorized for the rest of the escrow's
///   life, including re-resolution after `appeal_dispute`.
/// - **Single-signer threshold.** [`ResolverSet::threshold`] is `1` for a
///   fallback set, so whichever authorized resolver acts first moves the
///   escrow to `PendingFinalization`. There is no co-signing between primary
///   and backup; once past the deadline they effectively race.
/// - **Identity checks ignore the deadline.** Creation-time conflict checks
///   (resolver ≠ seller/buyer) use [`ResolverSet::contains`], which treats
///   both addresses as members regardless of time.
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
    /// regardless of the current ledger time — before *and* after
    /// `dispute_deadline`.
    pub primary: Address,
    /// Stand-in resolver. Only authorized once the ledger timestamp has
    /// reached `dispute_deadline`; any earlier `resolve_dispute` / `vote`
    /// call from this address fails with `NotAuthorized`. Must differ from
    /// the seller and buyer (checked at creation).
    pub backup: Address,
    /// Absolute ledger timestamp (Unix seconds) at which `backup` becomes an
    /// authorized resolver. The comparison is `now >= dispute_deadline`, so
    /// the backup is authorized *exactly* at this instant.
    ///
    /// This is **not** the same value as `Escrow::dispute_deadline`,
    /// which the contract computes at funding time (`funded_at +
    /// DISPUTE_WINDOW`) to bound the *buyer's* window to raise a dispute.
    /// This field is chosen by the caller of `create_escrow_with_fallback`
    /// and only controls *which resolver* may act.
    ///
    /// Must be at least `MIN_FALLBACK_DEADLINE_OFFSET` (1 day, `86_400`
    /// seconds) past the creation timestamp so the primary is guaranteed a
    /// minimum exclusive resolution window, and no later than
    /// `MAX_FALLBACK_DEADLINE_OFFSET` (39 days) past it so an unresponsive
    /// primary cannot lock disputed funds indefinitely. Otherwise creation
    /// fails with `InvalidFallbackDeadline`. Past values (including `0`) are
    /// rejected.
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
    /// Multiple resolvers with M-of-N voting threshold. Split votes can
    /// permanently deadlock a dispute — see [`MultiResolver`] for the
    /// scenario and the implemented escape hatches.
    Multi(MultiResolver),
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
            ResolverSet::Single(_) | ResolverSet::Fallback(_) => 1,
            ResolverSet::Multi(m) => m.threshold,
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
    /// Raw bytes — UTF-8 validation is deferred to off-chain consumers.
    /// Avoids the per-load UTF-8 check that `String` incurs in the Soroban host.
    pub description: Bytes,
    pub evidence_hash: BytesN<32>,
    pub status: DisputeStatus,
    pub disputed_at: u64,
    pub tracking_id: Option<String>,
    /// Resolution code: 0 = not resolved, 1 = Release, 2 = Refund,
    /// `3 + bps` = `Split(bps)` (payee share in basis points, so the valid
    /// range is `3..=10_003`). See [`ResolutionType::to_code`].
    pub resolution: u32,
    /// Which address made the resolution (the resolver who triggered finalization)
    pub resolved_by: Option<Address>,
    /// Number of times this dispute has been appealed
    pub appeal_count: u32,
    /// Timestamp when the resolution was made
    pub resolved_at: u64,
    /// Provisional arbitration fee recorded by the resolution transition.
    /// Not transferred until `finalize_dispute`; cleared on appeal.
    pub arbitration_fee: i128,
    /// Provisional resolver fee recorded by the resolution transition.
    /// Not transferred until `finalize_dispute`; cleared on appeal.
    pub resolver_fee: i128,
    /// Whether the arbitration and resolver fees have been settled (paid out)
    /// for this dispute. Only set to `true` by `finalize_dispute`.
    pub fees_charged: bool,
    /// Resolvers to be compensated from `resolver_fee` (the voters who backed
    /// the winning resolution); provisional until finalization, cleared on
    /// appeal.
    pub compensated_resolvers: Vec<Address>,
    /// Standardized reason the resolver gave for the current resolution
    /// (e.g. `NON_DELIVERY`, `DAMAGED`, `MUTUAL`), or `None` when none was
    /// supplied. Cleared by `clear_resolution` on appeal.
    ///
    /// Entries written before this field existed lack the key in storage;
    /// `load_dispute` back-fills it as `None` (see `decode_dispute`).
    pub resolution_reason: Option<Symbol>,
    /// Optional free-form resolver memo (raw bytes, UTF-8 expected, at most
    /// `MAX_DESCRIPTION_LEN`). Empty when none was supplied. Cleared by
    /// `clear_resolution` on appeal. Back-filled as empty for legacy entries.
    pub resolution_memo: Bytes,
}

impl DisputeData {
    /// Reason recorded with the current resolution, if any.
    pub fn get_resolution_reason(&self) -> Option<Symbol> {
        self.resolution_reason.clone()
    }

    /// Memo recorded with the current resolution (empty when none).
    pub fn get_resolution_memo(&self) -> Bytes {
        self.resolution_memo.clone()
    }

    /// Records the reason and memo supplied alongside the current resolution.
    pub fn set_resolution_note(&mut self, reason: Option<Symbol>, memo: Bytes) {
        self.resolution_reason = reason;
        self.resolution_memo = memo;
    }

    /// Records `r` in the packed `resolution` code (see
    /// [`ResolutionType::to_code`]).
    pub fn set_resolution(&mut self, r: ResolutionType) {
        self.resolution = r.to_code();
    }

    /// Decodes the packed `resolution` code; `None` when unresolved or the
    /// code is not a valid resolution.
    pub fn get_resolution(&self) -> Option<ResolutionType> {
        ResolutionType::from_code(self.resolution)
    }

    /// Clears the recorded resolution so a fresh round of voting can begin
    /// after an appeal, together with the provisional fee allocations
    /// (`arbitration_fee`, `resolver_fee`, `compensated_resolvers`). No fee has
    /// been transferred at this point, so nothing needs to be reclaimed; the
    /// next resolution round records fresh allocations for the final resolver.
    /// `fees_charged` stays `false` until `finalize_dispute` settles the fees.
    pub fn clear_resolution(&mut self) {
        self.resolution_reason = None;
        self.resolution_memo = Bytes::new(self.resolution_memo.env());
        self.resolution = 0;
        self.resolved_by = None;
        self.resolved_at = 0;
        self.arbitration_fee = 0;
        self.resolver_fee = 0;
        self.compensated_resolvers = Vec::new(self.compensated_resolvers.env());
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolutionType {
    /// 100% of the net principal goes to the primary payee.
    Release,
    /// 100% of the net principal goes to the buyer.
    Refund,
    /// Partial settlement: the payee receives `bps / 10_000` of the net
    /// principal and the buyer receives the remainder. `bps` must be
    /// `<= 10_000` (enforced by [`ResolutionType::validate`]).
    Split(u32),
}

/// First `DisputeData::resolution` code used by `Split(bps)`; the code for a
/// split is `SPLIT_CODE_BASE + bps`.
const SPLIT_CODE_BASE: u32 = 3;

impl ResolutionType {
    /// Rejects a `Split` whose payee share exceeds `BASIS_POINTS`.
    pub fn validate(&self) -> Result<(), crate::ContractError> {
        match self {
            ResolutionType::Split(bps) if *bps > crate::BASIS_POINTS => {
                Err(crate::ContractError::InvalidSplitBps)
            }
            _ => Ok(()),
        }
    }

    /// Packs the resolution into the `DisputeData::resolution` code:
    /// `1` = Release, `2` = Refund, `3 + bps` = `Split(bps)`.
    /// (`0` is reserved for "not resolved".)
    pub fn to_code(&self) -> u32 {
        match self {
            ResolutionType::Release => 1,
            ResolutionType::Refund => 2,
            ResolutionType::Split(bps) => SPLIT_CODE_BASE.saturating_add(*bps),
        }
    }

    /// Inverse of [`ResolutionType::to_code`]; `None` for `0` or any code
    /// that does not decode to a valid resolution.
    pub fn from_code(code: u32) -> Option<ResolutionType> {
        match code {
            1 => Some(ResolutionType::Release),
            2 => Some(ResolutionType::Refund),
            c if (SPLIT_CODE_BASE..=SPLIT_CODE_BASE + crate::BASIS_POINTS).contains(&c) => {
                Some(ResolutionType::Split(c - SPLIT_CODE_BASE))
            }
            _ => None,
        }
    }
}

/// Configuration for protocol and arbitration fee rates in basis points.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeConfig {
    pub protocol_fee_bps: u32,
    pub arbitration_fee_bps: u32,
}

pub const GLOBAL_CONFIG_FLAG_PAUSED: u32 = 1 << 0;
pub const GLOBAL_CONFIG_FLAG_TOKEN_ALLOWLIST_ENABLED: u32 = 1 << 1;
pub const GLOBAL_CONFIG_FLAG_RESOLVER_STRICT: u32 = 1 << 2;
pub const GLOBAL_CONFIG_FLAG_RECOVERY_MODE: u32 = 1 << 3;
/// When set, direct calls to `unpause_contract` are rejected with
/// `ContractError::NotAuthorized`. Unpausing must go through the
/// timelocked `queue_unpause_contract` → `execute_unpause_contract` path.
/// This enforces the security asymmetry: immediate pause for threat
/// containment, mandatory timelock delay before resuming operations.
pub const GLOBAL_CONFIG_FLAG_TIMELOCKED_UNPAUSE_ONLY: u32 = 1 << 4;

/// Admin-tunable global configuration, stored under [`DataKey::GlobalConfig`].
///
/// Fields that were previously optional in storage carry their documented
/// defaults (see `storage::default_global_config`), so readers never need
/// per-field fallbacks.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalConfig {
    /// Receives protocol and arbitration fees. `None` only before `initialize`.
    pub fee_collector: Option<Address>,
    /// Receives platform fees. `None` until the admin sets one.
    pub treasury: Option<Address>,
    pub protocol_fee_bps: u32,
    pub arbitration_fee_bps: u32,
    pub platform_fee_bps: u32,
    pub appeal_fee_bps: u32,
    pub min_amount: i128,
    pub max_amount: i128,
    /// Maximum seconds a dispute may stay unresolved before a forced refund.
    pub dispute_timeout: u64,
    pub ttl_extension_ledgers: u32,
    /// Bit-packed state flags for the boolean admin toggles that previously
    /// lived as separate storage fields. The layout is a single `u32` mask.
    pub state_flags: u32,
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
pub struct Escrow {
    pub payees: BoundedPayees,
    pub buyer: Option<Address>,
    pub resolvers: ResolverSet,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    pub resolver_fee_bps: u32,
    pub shipping_window: u64,
    /// Packed timestamp field: high 32 bits = `funded_at`, low 32 bits = `dispute_deadline`.
    ///
    /// Both timestamps fit in 32 bits until year 2106 (Unix epoch < 2^32).
    /// Unpack helpers:
    ///   `funded_at`        = `packed_timestamps` >> 32
    ///   `dispute_deadline` = `packed_timestamps` & `0xFFFF_FFFF`
    pub packed_timestamps: u64,
    pub shipped_at: u64,
    pub delivered_at: Option<u64>,
    pub tracking_id: Option<String>,
    pub state: EscrowState,
    /// Free-form notes. Empty string is the "no notes" sentinel, eliminating
    /// the XDR option-wrapper overhead of `Option<String>`.
    pub notes: String,
    pub expires_at: Option<u64>,
    pub grace_period: u64,
}

impl Escrow {
    pub fn funded_at(&self) -> u64 {
        self.packed_timestamps >> 32
    }

    pub fn dispute_deadline(&self) -> u64 {
        self.packed_timestamps & 0xFFFF_FFFF
    }

    /// Packs both timestamps into [`Escrow::packed_timestamps`].
    ///
    /// The layout reserves 32 bits per value, so each must fit in a `u32`
    /// (Unix timestamps do until 2106-02-07). A value above `u32::MAX` is
    /// rejected with [`crate::ContractError::ArithmeticOverflow`] rather than
    /// silently truncated, which would corrupt both unpacked timestamps.
    pub fn pack_timestamps(
        funded_at: u64,
        dispute_deadline: u64,
    ) -> Result<u64, crate::ContractError> {
        if funded_at > u64::from(u32::MAX) || dispute_deadline > u64::from(u32::MAX) {
            return Err(crate::ContractError::ArithmeticOverflow);
        }
        Ok((funded_at << 32) | (dispute_deadline & 0xFFFF_FFFF))
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowInput {
    pub buyer: Option<Address>,
    pub resolver: Address,
    pub token: Address,
    pub amount: i128,
    pub fee_bps: u32,
    /// Per-escrow resolver fee in basis points (issue #911). Validated with
    /// the same cap as `create_escrow`'s `resolver_fee_bps`; `0` means the
    /// resolver serves uncompensated.
    pub resolver_fee_bps: u32,
    pub shipping_window: u64,
    /// Empty string is the "no notes" sentinel (see `Escrow.notes`).
    pub notes: String,
}

/// One piece of dispute evidence submitted via `submit_dispute_evidence` by
/// the buyer, a payee or an assigned resolver. `evidence_hash` is a commitment
/// to off-chain evidence (conventionally its SHA-256 digest).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvidenceEntry {
    pub submitter: Address,
    pub timestamp: u64,
    pub evidence_hash: BytesN<32>,
    pub description: Bytes,
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
    /// Number of **unique escrows** that have ever been disputed. Incremented
    /// once by `raise_dispute`; appeals do not inflate it.
    pub total_disputed: u64,
    pub total_refunded: u64,
    /// Total appeal cycles across all escrows: incremented by every
    /// successful `appeal_dispute` (so one escrow appealed twice adds 2).
    pub total_appeals: u64,
}

/// Payee with address and basis points share.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payee {
    pub address: Address,
    pub bps: u32,
}

/// A single on-chain record written every time `execute_upgrade` runs.
/// Records are appended to `DataKey::UpgradeHistory` and are immutable
/// after insertion — the full list forms a tamper-evident audit ledger.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeRecord {
    /// The new WASM hash installed by this upgrade.
    pub wasm_hash: BytesN<32>,
    /// Ledger timestamp when the upgrade was executed.
    pub upgraded_at: u64,
    /// Admin address that triggered the upgrade.
    pub upgraded_by: Address,
    /// Contract version counter after the upgrade (monotonically increasing).
    pub version: u32,
}
/// Maximum number of payees per escrow. Enforced by [`BoundedPayees`] so
/// storage cost is bounded at creation time and the length-prefix overhead of
/// an unbounded `Vec` is eliminated.
pub const MAX_PAYEES: u32 = 10;

/// A capacity-bounded wrapper around `Vec<Payee>`.
///
/// Stored as a `#[contracttype]` struct so the XDR shape is a single-field
/// struct with explicit capacity enforcement rather than a bare, unbounded Vec.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundedPayees {
    pub inner: Vec<Payee>,
}

impl BoundedPayees {
    pub fn new(inner: Vec<Payee>) -> Self {
        Self { inner }
    }

    pub fn get(&self, i: u32) -> Option<Payee> {
        self.inner.get(i)
    }

    pub fn len(&self) -> u32 {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
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

impl EscrowState {
    /// Compact discriminant for packing state-history entries into a single
    /// `u64` (state code in the high byte, timestamp in the low 56 bits)
    /// instead of storing a full `(EscrowState, u64)` tuple per entry — see
    /// `internal::pack_history_entry`.
    pub(crate) fn to_history_code(&self) -> u8 {
        match self {
            EscrowState::Pending => 0,
            EscrowState::Funded => 1,
            EscrowState::Shipped => 2,
            EscrowState::Completed => 3,
            EscrowState::Disputed => 4,
            EscrowState::RefundRequested => 5,
            EscrowState::Refunded => 6,
            EscrowState::Canceled => 7,
            EscrowState::PendingFinalization => 8,
            EscrowState::Expired => 9,
        }
    }

    pub(crate) fn from_history_code(code: u8) -> Self {
        match code {
            0 => EscrowState::Pending,
            1 => EscrowState::Funded,
            2 => EscrowState::Shipped,
            3 => EscrowState::Completed,
            4 => EscrowState::Disputed,
            5 => EscrowState::RefundRequested,
            6 => EscrowState::Refunded,
            7 => EscrowState::Canceled,
            8 => EscrowState::PendingFinalization,
            _ => EscrowState::Expired,
        }
    }
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
    SetMaxAppeals = 18,
    SetMaxBasketSize = 19,
    SetAppealFee = 20,
    SetTimelockDelay = 21,
}

impl TimelockOperation {
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            1 => Some(Self::SetAdmin),
            2 => Some(Self::Upgrade),
            3 => Some(Self::SetProtocolFee),
            4 => Some(Self::SetArbitrationFee),
            5 => Some(Self::SetPlatformFee),
            6 => Some(Self::SetTreasury),
            7 => Some(Self::SetFeeCollector),
            8 => Some(Self::SetTtlExtension),
            9 => Some(Self::SetAmountLimits),
            10 => Some(Self::AddApprovedResolver),
            11 => Some(Self::RemoveApprovedResolver),
            12 => Some(Self::SetResolverStrict),
            13 => Some(Self::SetTokenAllowlistEnabled),
            14 => Some(Self::AddAllowedToken),
            15 => Some(Self::RemoveAllowedToken),
            16 => Some(Self::PauseContract),
            17 => Some(Self::UnpauseContract),
            18 => Some(Self::SetMaxAppeals),
            19 => Some(Self::SetMaxBasketSize),
            20 => Some(Self::SetAppealFee),
            21 => Some(Self::SetTimelockDelay),
            _ => None,
        }
    }
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
    pub params_hash: BytesN<32>,
    pub queued_at: u64,
    pub ready_at: u64,
}
