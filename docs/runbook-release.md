# Mainnet Release Runbook

## Overview

This runbook guides deployment of TrustLink escrow contract to Stellar mainnet. Follow each step sequentially without skipping.

## Pre-Deployment (48 hours before)

### 1. Code Freeze & Verification
```bash
# Ensure all changes are committed
git status
# Should show: "working tree clean"

# Verify main branch is up to date
git fetch origin
git log -1 --oneline
```

### 2. Build & Test Locally
```bash
# Full build and test
cargo xtask ci

# Verify bindings compile
cd bindings && npm run typecheck && cd ..

# Check WASM size
ls -lh target/wasm32v1-none/release/trustlink_escrow.wasm
# Should be < 1MB
```

### 3. Version Bump
```bash
# Update version in both files (e.g., 1.0.0 -> 1.0.1)
# contracts/escrow/Cargo.toml: version = "X.Y.Z"
# bindings/package.json: "version": "X.Y.Z"

# Update CHANGELOG.md with release notes

# Verify changes
git diff Cargo.toml bindings/package.json CHANGELOG.md
```

### 4. Testnet Smoke Test
```bash
# Build optimized WASM
cargo xtask build-wasm
./build.sh

# Deploy to testnet (requires Stellar CLI and funded testnet account)
stellar contract deploy \
  --network testnet \
  --source-account alice \
  --wasm target/wasm32v1-none/release/trustlink_escrow.wasm

# Record testnet contract ID and admin account
# Example: CACA...BABA

# Initialize contract on testnet
stellar contract invoke \
  --network testnet \
  --source-account alice \
  --id CACA...BABA \
  -- initialize \
  --admin G... \
  --fee_collector G... \
  --arbitration_fee_bps 300

# Quick smoke test: create and fund an escrow
stellar contract invoke \
  --network testnet \
  --source-account alice \
  --id CACA...BABA \
  -- create_escrow \
  --seller G... \
  --token CACA...TOKEN \
  --amount 1000 \
  --shipping_window 3600
```

### 5. Security Review
- [ ] Verify fee caps: `MAX_COMBINED_FEE_BPS` in code
- [ ] Review state transition matrix in `ARCHITECTURE.md`
- [ ] No new `unsafe` blocks introduced
- [ ] All `require_auth()` calls precede state reads
- [ ] No hardcoded addresses or keys in contract

### 6. Create Release Branch & PR
```bash
git checkout -b release/vX.Y.Z
git add Cargo.toml bindings/package.json CHANGELOG.md
git commit -m "release: v${VERSION}"
git push -u origin release/vX.Y.Z

# Create PR for final review
# Link to release checklist issue
```

---

## Mainnet Deployment (Execute in sequence)

### 1. Pre-Flight Check
```bash
# Verify no uncommitted changes
git status

# Confirm current branch
git branch --show-current
# Should show: release/vX.Y.Z

# Verify Stellar CLI is installed and configured
stellar version

# Verify mainnet account is funded
stellar account info --network public --source-account mainnet-deployer
```

### 2. Build Final WASM
```bash
# Clean build to eliminate caching issues
cargo clean
cargo xtask build-wasm
./build.sh

# Verify output file exists and size is reasonable
ls -lh target/wasm32v1-none/release/trustlink_escrow.wasm
```

### 3. Deploy Contract
```bash
# Deploy to mainnet (non-reversible)
MAINNET_WASM_HASH=$(sha256sum target/wasm32v1-none/release/trustlink_escrow.wasm | cut -d' ' -f1)
echo "WASM SHA256: $MAINNET_WASM_HASH"

stellar contract deploy \
  --network public \
  --source-account mainnet-deployer \
  --wasm target/wasm32v1-none/release/trustlink_escrow.wasm

# Save the contract ID returned
# Store in deployment log: MAINNET_CONTRACT_ID=CXXX...
```

### 4. Initialize Contract
```bash
# Set environment variables (obtain from team secure storage)
MAINNET_CONTRACT_ID="CXXX..."
MAINNET_ADMIN="G..."
MAINNET_FEE_COLLECTOR="G..."
ARBITRATION_FEE_BPS="300"

# Call initialize (only callable once)
stellar contract invoke \
  --network public \
  --source-account mainnet-deployer \
  --id ${MAINNET_CONTRACT_ID} \
  -- initialize \
  --admin ${MAINNET_ADMIN} \
  --fee_collector ${MAINNET_FEE_COLLECTOR} \
  --arbitration_fee_bps ${ARBITRATION_FEE_BPS}

# Wait for transaction confirmation
# Verify initialization event was emitted in Stellar Expert
```

### 5. Verification Tests
```bash
# Query escrow contract to verify initialization
stellar contract invoke \
  --network public \
  --source-account mainnet-deployer \
  --id ${MAINNET_CONTRACT_ID} \
  -- get_escrow \
  --id 0

# Should return error (no escrow with id 0), confirming contract is live
```

### 6. Record Deployment
Create or update `DEPLOYMENT.md` in repo root:

```markdown
# Deployment Log

## Mainnet v1.0.0 (2026-06-30)

| Field | Value |
|-------|-------|
| Version | 1.0.0 |
| Contract ID | CXXX... |
| Admin | G... |
| Fee Collector | G... |
| WASM SHA256 | abc123... |
| Deployment TX | (link to Stellar Expert) |
| Deployed By | (name/account) |
| Deployment Date | 2026-06-30 |
```

---

## Post-Deployment (Within 2 hours)

### 1. Public Verification
```bash
# Use Stellar Expert to verify:
# 1. Contract is live at contract ID
# 2. Initialize event exists
# 3. Admin account is set correctly
# 4. Fee collector is set correctly

curl https://stellar.expert/api/v2/contract/CXXX...
```

### 2. Documentation Updates
- [ ] Update README.md with mainnet contract ID and verified addresses
- [ ] Add mainnet contract link to docs/NETWORKS.md
- [ ] Update SDK integration docs with mainnet endpoints

### 3. Publish Release
```bash
# Tag the release
git tag -a vX.Y.Z -m "Release vX.Y.Z - Mainnet deployment"
git push origin vX.Y.Z

# Create GitHub Release with:
# - Title: "Release vX.Y.Z"
# - Body: Copy CHANGELOG.md section for this version
# - Attach: target/wasm32v1-none/release/trustlink_escrow.wasm
```

### 4. Publish Bindings (if public)
```bash
cd bindings
npm publish --access public
cd ..
```

### 5. Announcement
- [ ] Post announcement to community channels (Discord, Twitter, Stellar Dev Discord)
- [ ] Include: version, contract ID, link to contract on Stellar Expert
- [ ] Highlight: any breaking changes or new features

---

## Rollback Plan

If critical issues are discovered post-deployment:

### Not Possible - Contracts Are Immutable
Stellar contracts cannot be "rolled back" once deployed. Instead:

1. **Deploy New Contract** with fixes at a new address
2. **Migrate State** if contract data was corrupted (manual intervention required)
3. **Communicate** breaking change to all integrators

To minimize this risk:
- Extensive testnet testing before mainnet (72+ hours of live testing recommended)
- Staged rollout if possible (limited initial liquidity)
- Monitor event logs and contract calls 24/7 for first week

---

## Monitoring & Support (First Week)

### Daily Checks
```bash
# Monitor contract calls
curl https://api.stellar.expert/explorer/public/contract/CXXX.../operations

# Check transaction fees
# Verify no unexpected errors in logs

# Monitor token transfers to ensure escrows are functioning
```

### On-Call Support
- Designate on-call engineer for first 72 hours
- Monitor Discord/email for user issues
- Have deployment rollback comms ready (though cannot revert contract)

### Success Metrics
- Zero failed transactions for core flows (create → fund → release)
- Sub-2s response times for queries
- All events emitted correctly
- No auth errors for valid callers

---

## Troubleshooting

### Contract Deployment Fails
```bash
# Check account balance
stellar account info --network public --source-account mainnet-deployer

# Check network status
curl https://stellar.expert/api/v2/network/stats

# Retry with explicit fees
stellar contract deploy \
  --network public \
  --source-account mainnet-deployer \
  --wasm target/wasm32v1-none/release/trustlink_escrow.wasm \
  --base-fee 100000
```

### Initialize Fails (Already Called)
- Contract is already initialized
- Verify with: `curl https://stellar.expert/api/v2/contract/CXXX.../spec`
- If admin/fee_collector are wrong, deploy new contract instance

### Events Not Emitted
- Verify contract was called with correct parameters
- Check Stellar Expert logs under "Events" tab
- Ensure contract was initialized before creating escrows

---

## Appendix

### Required Environment Variables
```bash
MAINNET_DEPLOYER_SK="S..."      # Signer's secret key
MAINNET_ADMIN="G..."             # Contract admin address
MAINNET_FEE_COLLECTOR="G..."     # Fee recipient address
STELLAR_NETWORK_PASSPHRASE="Public Global Stellar Network ; September 2015"
```

### Useful Links
- Stellar Expert: https://stellar.expert/explorer/public
- Soroban Docs: https://soroban.stellar.org
- Stellar CLI: https://github.com/stellar/stellar-cli
- Test USDC: https://developers.stellar.org/learn/settle-payment-rails/test-payments

### Key Contacts
- Stellar Network Status: https://status.stellar.org
- Soroban Support: #soroban-contracts on Stellar Dev Discord
- Emergency: (contact info for on-call)
