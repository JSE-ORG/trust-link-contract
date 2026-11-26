# Deployment Quick Start

## 60-Second Overview

**TrustLink mainnet deployment is a 6-step process:**

1. Build optimized WASM
2. Smoke test on testnet (48+ hours)
3. Version bump + PR
4. Deploy to mainnet (non-reversible)
5. Initialize contract
6. Verify + announce

**Time Required**: ~4 hours active work (48+ hours total for testnet validation)

---

## Prerequisites

### Install Stellar CLI
```bash
# Install from: https://github.com/stellar/stellar-cli

# Verify installation
stellar version
```

### Fund Mainnet Account
Obtain XLM to deploy from exchange or existing account:
```bash
stellar account info --network public --source-account mainnet-deployer
# Must show balance > 2 XLM
```

### Prepare Addresses
- **Admin Address** (G...): Contract admin, can pause if needed
- **Fee Collector** (G...): Receives arbitration fees
- Both must be valid Stellar addresses

---

## Testnet Smoke Test (First 48 Hours)

### Step 1: Build WASM
```bash
cargo xtask build-wasm
./build.sh
ls -lh target/wasm32v1-none/release/trustlink_escrow.wasm
```

### Step 2: Deploy to Testnet
```bash
./scripts/deploy.sh --network testnet --source alice
# Records: testnet contract ID
```

### Step 3: Initialize Contract
```bash
./scripts/deploy.sh \
  --network testnet \
  --source alice \
  --admin G... \
  --fee-collector G... \
  --arbitration-fee 300
```

### Step 4: Run Basic Escrow Flow
```bash
# Create escrow (seller) → Fund (buyer) → Confirm delivery (buyer)
# See: docs/NETWORKS.md for contract invoke examples
```

### Step 5: Verify Events
Check Stellar Expert for:
- Contract exists
- Initialize event emitted
- Create/fund events appear
- No error logs

**Link**: `https://stellar.expert/explorer/testnet/contract/<CONTRACT_ID>`

---

## Mainnet Deployment (Day 3)

### Step 1: Code Freeze
```bash
git status                    # must be clean
git checkout -b release/vX.Y.Z

# Bump versions
# - contracts/escrow/Cargo.toml
# - bindings/package.json
# Update CHANGELOG.md
```

### Step 2: Final Build
```bash
cargo clean
cargo xtask ci                # full check
cargo xtask build-wasm
./build.sh
```

### Step 3: Deploy to Mainnet
```bash
# This is the point of no return
./scripts/deploy.sh \
  --network public \
  --source mainnet-deployer \
  --admin G... \
  --fee-collector G...

# Records: mainnet contract ID
```

### Step 4: Verify on Explorer
```bash
# Open in browser:
https://stellar.expert/explorer/public/contract/<CONTRACT_ID>

# Verify:
# - Contract is live
# - Admin is correct
# - Fee collector is correct
```

### Step 5: Create GitHub Release
```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
git push origin vX.Y.Z

# Go to GitHub → Releases → Create Release
# Attach: target/wasm32v1-none/release/trustlink_escrow.wasm
```

### Step 6: Announce
Post to Discord/Twitter with:
- Version number
- Contract ID
- Stellar Expert link

---

## Deployment Checklist

**Pre-Deployment (48 hours before)**
- [ ] All changes committed to main
- [ ] `cargo xtask ci` passes
- [ ] Bindings typecheck passes
- [ ] Testnet deployed & smoke tested (24+ hours)
- [ ] Security review complete

**Deployment Day**
- [ ] Code freeze: version bump + CHANGELOG
- [ ] Release PR created & approved
- [ ] `cargo clean && cargo xtask ci` passes
- [ ] WASM built and size verified (< 1MB)
- [ ] Mainnet account funded & confirmed

**Execute Deployment**
- [ ] Deploy contract: `./scripts/deploy.sh --network public ...`
- [ ] Record contract ID
- [ ] Verify on Stellar Expert
- [ ] Create GitHub release
- [ ] Publish announcement

---

## Rollback?

**Contracts are immutable on Stellar.** If critical issues arise:

1. Deploy new contract to new address
2. Migrate liquidity (manual process)
3. Update all documentation & SDKs
4. Post incident report

**Prevention:**
- 48+ hours testnet live testing
- All escrow flows tested end-to-end
- Fee model audited
- Authorization logic reviewed

---

## Troubleshooting

**"WASM file not found"**
```bash
cargo xtask build-wasm && ./build.sh
```

**"Account not found"**
```bash
stellar account create --name mainnet-deployer
stellar account info --network public --source-account mainnet-deployer
```

**"Deployment failed"**
See [runbook-release.md](./runbook-release.md) Troubleshooting section

**"Events not showing"**
- Wait 5-10 seconds for indexing
- Check contract ID on Stellar Expert
- Verify network passphrase

---

## Next Steps

1. Complete testnet smoke test (48 hours)
2. Schedule mainnet deployment window
3. Read full runbook: [runbook-release.md](./runbook-release.md)
4. Set up monitoring for first week post-launch
5. Have incident response team on-call

---

## Support

- **Stellar Docs**: https://developers.stellar.org
- **Soroban Docs**: https://soroban.stellar.org
- **Issues**: GitHub Issues or Discord #soroban-contracts
- **Emergency**: Check status.stellar.org
