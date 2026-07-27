# Migration Guide

Guide for upgrading between TrustLink contract versions and handling breaking changes.

## Overview

This document covers:
- ABI (Application Binary Interface) changes
- Storage schema migrations
- Event topic changes
- Backward compatibility strategy
- Version-specific migration paths

## Versioning Strategy

TrustLink follows semantic versioning (SEMVER):
- **Major** (X.0.0): Breaking changes requiring migration
- **Minor** (0.X.0): New features, backward compatible
- **Patch** (0.0.X): Bug fixes, fully compatible

## Backward Compatibility Policy

### Contract Upgrades
- **WASM updates**: Use `upgrade()` function to update contract code
- **Storage schema**: Migrations handled automatically when possible
- **Events**: New event versions emitted alongside old versions during transition period

### API Stability
- Public functions maintain signature compatibility within major versions
- New optional parameters added to the end
- Deprecated functions marked but maintained for one major version

## Migration Paths

### From v1.x to v2.0

#### Breaking Changes

**1. Function Signature Changes**

```rust
// v1.x
pub fn create_escrow(
    env: Env,
    depositor: Address,
    recipient: Address,
    amount: i128,
) -> String;

// v2.0
pub fn create_escrow(
    env: Env,
    depositor: Address,
    recipient: Address,
    amount: i128,
    deadline: u64,  // NEW: Required parameter
    token: Address, // NEW: Multi-token support
) -> String;
```

**Migration Steps:**
```typescript
// Old code (v1.x)
await contract.call('create_escrow', depositor, recipient, amount);

// New code (v2.0)
const deadline = Math.floor(Date.now() / 1000) + 86400; // 24 hours
const tokenAddress = USDC_ADDRESS;
await contract.call('create_escrow', depositor, recipient, amount, deadline, tokenAddress);
```

**2. Storage Schema Changes**

```rust
// v1.x storage
pub struct Escrow {
    depositor: Address,
    recipient: Address,
    amount: i128,
    released: bool,
}

// v2.0 storage (with migration)
pub struct EscrowV2 {
    depositor: Address,
    recipient: Address,
    amount: i128,
    deadline: u64,
    token: Address,
    status: EscrowStatus, // Changed from bool to enum
}
```

**Migration Function:**
```rust
// Included in v2.0 contract
pub fn migrate_escrow_v1_to_v2(env: Env) {
    let storage = env.storage().persistent();
    
    // Iterate through all v1 escrows
    for escrow_id in get_all_escrow_ids(&env) {
        if let Some(escrow_v1) = storage.get::<String, EscrowV1>(&escrow_id) {
            // Convert to v2 format
            let escrow_v2 = EscrowV2 {
                depositor: escrow_v1.depositor,
                recipient: escrow_v1.recipient,
                amount: escrow_v1.amount,
                deadline: u64::MAX, // No deadline for migrated escrows
                token: get_default_token(&env),
                status: if escrow_v1.released {
                    EscrowStatus::Released
                } else {
                    EscrowStatus::Active
                },
            };
            
            // Update storage
            storage.set(&escrow_id, &escrow_v2);
        }
    }
}
```

**Invoke Migration:**
```bash
# After deploying v2.0, run migration
stellar contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network mainnet \
  -- migrate_escrow_v1_to_v2
```

**3. Event Topic Changes**

```rust
// v1.x event
env.events().publish((
    symbol_short!("escrow"),
    symbol_short!("created")
), escrow_id);

// v2.0 event (with versioning)
env.events().publish((
    symbol_short!("escrow"),
    symbol_short!("v2"),
    symbol_short!("created")
), escrow_data);
```

**Frontend Migration:**
```typescript
// Support both v1 and v2 events
function parseEscrowEvent(event: any): EscrowEvent {
  const topics = event.topic;
  
  // Check version
  if (topics[1] === 'v2') {
    // Parse v2 event
    return {
      version: 2,
      type: topics[2],
      data: scValToNative(event.value),
    };
  } else {
    // Parse v1 event (legacy)
    return {
      version: 1,
      type: topics[1],
      escrowId: scValToNative(topics[2]),
    };
  }
}
```

### From v2.x to v3.0

#### Breaking Changes

**1. Multi-Token Support**
- All functions now require `token: Address` parameter
- Default XLM no longer assumed

**Migration:**
```typescript
// v2.x
await contract.call('release_escrow', escrow_id);

// v3.0
await contract.call('release_escrow', escrow_id, XLM_ADDRESS);
```

**2. Refund Policy Changes**
- Automatic refunds after deadline
- Manual refund requests deprecated

**Migration:**
No code changes required. Refunds happen automatically.

## Upgrade Procedure

### Step 1: Audit Current Integration

```bash
# Check current contract version
stellar contract invoke \
  --id $CONTRACT_ID \
  --network mainnet \
  -- get_version
```

### Step 2: Deploy New Version

```bash
# Build new version
cd contracts/escrow
stellar contract build

# Deploy with upgrade
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source admin \
  --network mainnet \
  --upgrade $EXISTING_CONTRACT_ID
```

### Step 3: Run Migrations

```bash
# Execute data migration if needed
stellar contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network mainnet \
  -- migrate_to_v2
```

### Step 4: Update Frontend

```typescript
// Update SDK bindings
npm install @trustlink/sdk@2.0.0

// Update contract calls with new signatures
import { EscrowClient } from '@trustlink/sdk';

const client = new EscrowClient({
  contractId: CONTRACT_ID,
  networkPassphrase: Networks.PUBLIC,
  rpcUrl: 'https://soroban-mainnet.stellar.org',
});

// Use new API
await client.createEscrow({
  depositor: publicKey,
  recipient: recipientKey,
  amount: BigInt(1000000),
  deadline: futureTimestamp,
  token: USDC_ADDRESS,
});
```

### Step 5: Monitor

```typescript
// Monitor for errors
try {
  await client.releaseEscrow(escrowId, tokenAddress);
} catch (error) {
  if (error.message.includes('InvalidToken')) {
    // Handle migration-related errors
    console.error('Token not supported in this version');
  }
}
```

## Compatibility Matrix

| Frontend SDK | Contract v1.x | Contract v2.x | Contract v3.x |
|--------------|---------------|---------------|---------------|
| v1.x         | ✅ Full       | ⚠️ Limited    | ❌ No         |
| v2.x         | ✅ Full       | ✅ Full       | ⚠️ Limited    |
| v3.x         | ⚠️ Limited    | ✅ Full       | ✅ Full       |

## Testing Migration

```bash
# Run migration tests
cd contracts/escrow
cargo test migration_

# Run integration tests against migrated contract
npm run test:integration:migration
```

## Rollback Strategy

If issues arise:

```bash
# Revert to previous WASM (if within upgrade window)
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow_v1.wasm \
  --source admin \
  --network mainnet \
  --upgrade $CONTRACT_ID
```

**Note:** Storage migrations cannot be rolled back. Always test thoroughly on testnet first.

## Support

For migration assistance:
- Open an issue on GitHub
- Join the Discord community
- Contact the core team

## Changelog

See [CHANGELOG.md](./CHANGELOG.md) for detailed version history and breaking changes.
