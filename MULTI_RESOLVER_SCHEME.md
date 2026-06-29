# Multi-Resolver M-of-N Voting Scheme

## Overview

The TrustLink Escrow Contract now supports M-of-N multi-resolver dispute resolution, replacing the previous single-resolver architecture. This enhancement eliminates the single point of failure risk while maintaining backward compatibility.

## Backward Compatibility

**Single-resolver escrows remain fully compatible:**
- Existing `create_escrow()` function continues to work with a single resolver
- Single resolvers automatically wrapped in `ResolverSet::Single` internally
- Existing events and storage remain unchanged for single-resolver mode

## New Multi-Resolver Mode

### Creating Multi-Resolver Escrows

```rust
// Create escrow with 3 resolvers, requiring 2 votes (2-of-3 scheme)
contract.create_escrow_multi(
    seller,
    Some(buyer),
    vec![resolver1, resolver2, resolver3],
    2,  // threshold: minimum votes required
    token,
    amount,
    fee_bps,
    shipping_window
)
```

### Resolver Requirements

- **Unique:** All resolvers must be distinct addresses
- **Non-conflicting:** Resolvers cannot be the seller or buyer
- **Valid Threshold:** Must be > 0 and ≤ number of resolvers

## Voting Mechanism

### Deterministic Vote Tallying

When a resolver votes on a disputed escrow:

1. **Vote Recording:** The resolver's vote is recorded with resolution type (Release or Refund)
2. **Vote Counting:** Only the most recent vote from each resolver counts (updates override previous votes)
3. **Threshold Check:** System tallies votes separately for Release and Refund
4. **Auto-Execution:** When either count reaches the threshold, resolution executes automatically

**Example (2-of-3 scheme):**
- Resolver A votes Release
- Resolver B votes Release
- **Threshold met!** Automatically releases funds to seller
- (Resolver C vote no longer affects outcome)

### Vote Persistence

- Votes are stored in persistent contract storage with TTL extension
- Vote history is queryable via `get_resolver_votes(escrow_id)`
- Each vote includes: resolver address, resolution choice, and timestamp

## Events

### New Event: ResolverVoteRecorded

Emitted when a resolver submits a vote:

```rust
pub struct ResolverVoteRecorded {
    pub escrow_id: u64,
    pub resolver: Address,
    pub resolution: ResolutionType,  // Release or Refund
    pub vote_count: u32,             // Current count for this resolution type
    pub threshold: u32,              // Required threshold
    pub voted_at: u64,               // Ledger timestamp
}
```

This enables off-chain indexers to track voting progress in real-time.

### Existing Events (Enhanced)

- `DisputeResolved` - now emitted when threshold is reached (works for both single and multi-resolver)
- Other escrow lifecycle events unchanged

## API Reference

### New Contract Methods

#### `create_escrow_multi`
```rust
pub fn create_escrow_multi(
    env: Env,
    seller: Address,
    buyer: Option<Address>,
    resolvers: Vec<Address>,           // Multiple resolvers
    threshold: u32,                    // M-of-N voting threshold
    token: Address,
    amount: i128,
    fee_bps: u32,
    shipping_window: u64,
) -> Result<u64, ContractError>
```

#### `get_resolver_votes`
```rust
pub fn get_resolver_votes(env: Env, escrow_id: u64) -> Vec<ResolverVote>
```

Returns the complete voting history for a disputed escrow, including all resolver votes and their choices.

### Modified Contract Methods

#### `resolve_dispute`
Enhanced to support multi-resolver voting:

```rust
pub fn resolve_dispute(
    env: Env,
    caller: Address,
    escrow_id: u64,
    resolution: ResolutionType,  // Release or Refund
) -> Result<(), ContractError>
```

**Behavior:**
- **Single resolver:** Executes immediately (backward compatible)
- **Multi-resolver:** Records vote, checks threshold, auto-executes when threshold met

#### `rotate_resolver`
Enhanced to support rotation in single-resolver escrows only:

```rust
pub fn rotate_resolver(
    env: Env,
    caller: Address,
    escrow_id: u64,
    new_resolver: Address,
) -> Result<(), ContractError>
```

**Behavior:**
- **Single resolver:** Rotates resolver (backward compatible)
- **Multi-resolver:** Returns `InvalidState` (not supported yet)

*Note: Multi-resolver set rotation would require a separate function.*

## Data Structures

### ResolverSet Enum

```rust
pub enum ResolverSet {
    /// Single resolver (backward compatible mode)
    Single(Address),
    /// Multiple resolvers with M-of-N voting threshold
    Multi {
        resolvers: Vec<Address>,
        threshold: u32,
    },
}
```

**Methods:**
- `count()` - Returns number of resolvers
- `contains(addr)` - Checks if address is a resolver
- `threshold()` - Returns required vote count

### ResolverVote Struct

```rust
pub struct ResolverVote {
    pub resolver: Address,
    pub resolution: ResolutionType,  // Release or Refund
    pub voted_at: u64,               // Ledger timestamp
}
```

### Modified EscrowData

```rust
pub struct EscrowData {
    // ... other fields ...
    pub resolvers: ResolverSet,  // Was: resolver: Address
    // ... other fields ...
}
```

## Security Considerations

### Authorization

- Only addresses in the resolver set can vote
- Admin can still resolve disputes (for operational control)
- Vote overrides are allowed (most recent vote wins)

### Voting Logic

- Votes are tallied deterministically (no off-chain randomness)
- No voting deadline (votes can be submitted indefinitely before resolution)
- Threshold met triggers automatic execution (no second approval needed)

### Arbitration Fees

- Applied once when threshold is reached (not per vote)
- Applied before payout calculation

## Migration Guide

### For Existing Integrations

1. **Backward Compatible:** Existing code using `create_escrow()` continues working unchanged
2. **No Breaking Changes:** Single-resolver escrows work with original API
3. **Opt-In:** Use new `create_escrow_multi()` for multi-resolver escrows

### For New Integrations

```typescript
// TypeScript binding (pseudo-code)
// Single resolver (backward compatible)
await contract.create_escrow({
    seller, buyer, resolver, token, amount, fee_bps, shipping_window
});

// Multi-resolver (new feature)
await contract.create_escrow_multi({
    seller, buyer,
    resolvers: [resolver1, resolver2, resolver3],
    threshold: 2,
    token, amount, fee_bps, shipping_window
});
```

## Future Enhancements

1. **Dynamic Resolver Set Management:** Add function to rotate/update resolver set
2. **Voting Time Windows:** Option to close voting after dispute deadline + N ledgers
3. **Vote Weights:** Support weighted voting (1 vote, 2 votes, etc.)
4. **Weighted Thresholds:** Threshold as percentage of total weight
5. **Off-Chain Coordination:** Signature-based voting coordination

## Testing

Key test scenarios:

- ✅ Single resolver escrows (backward compatibility)
- ✅ Multi-resolver escrows creation
- ✅ M-of-N voting with multiple thresholds
- ✅ Vote override and update
- ✅ Threshold met triggers auto-execution
- ✅ Deterministic vote tally
- ✅ Resolver role conflict detection
- ✅ Invalid threshold validation

## Storage Model

**New Storage Key:**
- `DataKey::ResolverVotes(escrow_id)` - Stores `Vec<ResolverVote>`

**TTL Extension:**
- Resolver votes use same TTL extension as escrows
- Extended on every vote submission

## Acceptance Criteria

✅ **Backward Compatibility Path**
- Single-resolver escrows use existing `create_escrow()` API
- Existing escrow data structure extended, not replaced
- All single-resolver operations unchanged

✅ **Votes Tallied Deterministically**
- No randomness or off-chain coordination required
- Vote counting is transparent and verifiable
- Threshold met → automatic execution (no race conditions)

✅ **M-of-N Voting**
- Support arbitrary M and N values
- Votes override previous votes from same resolver
- Each resolver has equal weight (1 vote each)
