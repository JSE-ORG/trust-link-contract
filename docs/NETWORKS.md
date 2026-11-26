# Network Configuration

## Supported Networks

### Testnet (Development)
- **RPC**: `https://soroban-testnet.stellar.org`
- **Network Passphrase**: `Test SDF Network ; September 2015`
- **Horizon**: `https://horizon-testnet.stellar.org`
- **Type**: Full-reset every ~3 months
- **Use Case**: Integration testing, pre-release validation

#### Testnet Setup
```bash
# Fund testnet account via friendbot
curl "https://friendbot.stellar.org?addr=GXXXXXX"

# Deploy to testnet
stellar contract deploy \
  --network testnet \
  --source-account alice \
  --wasm target/wasm32v1-none/release/trustlink_escrow.wasm
```

---

### Mainnet (Production)
- **RPC**: `https://mainnet.stellar.org`
- **Network Passphrase**: `Public Global Stellar Network ; September 2015`
- **Horizon**: `https://horizon.stellar.org`
- **Explorer**: `https://stellar.expert/explorer/public`
- **Use Case**: Live trading, production escrows

#### Mainnet Contract
| Field | Value |
|-------|-------|
| Version | TBD (pre-mainnet) |
| Contract ID | TBD |
| Admin | TBD |
| Status | Not yet deployed |

**Deployment**: See [runbook-release.md](./runbook-release.md)

---

## Local Development

### Standalone Network (Docker)
```bash
# Start local network
make docker-up

# Network Details
# - RPC: http://localhost:8000
# - Horizon: http://localhost:8000
# - Network Passphrase: Standalone Network ; February 2021

# Funded accounts (auto-created)
# - alice: seed phrase available in logs
# - bob: seed phrase available in logs

# Stop network
make docker-down
```

---

## Deployment Commands Reference

### Build
```bash
# WASM target (required for mainnet)
cargo xtask build-wasm

# Optimize
./build.sh

# Verify size
ls -lh target/wasm32v1-none/release/trustlink_escrow.wasm
```

### Deploy

**Option 1: Automated Script**
```bash
# Testnet
./scripts/deploy.sh --network testnet --source alice

# Mainnet (requires confirmation)
./scripts/deploy.sh --network public --source mainnet-deployer
```

**Option 2: Manual Stellar CLI**
```bash
# Deploy
stellar contract deploy \
  --network testnet \
  --source-account alice \
  --wasm target/wasm32v1-none/release/trustlink_escrow.wasm

# Initialize (immediately after deployment)
stellar contract invoke \
  --network testnet \
  --source-account alice \
  --id CXXX... \
  -- initialize \
  --admin G... \
  --fee_collector G... \
  --arbitration_fee_bps 300
```

---

## Environment Variables

```bash
# Stellar CLI Configuration
export STELLAR_NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
export STELLAR_RPC_HOST="https://soroban-testnet.stellar.org"

# Deployment Configuration
export MAINNET_DEPLOYER_SK="S..."              # Deployer secret key
export MAINNET_ADMIN="G..."                    # Admin address
export MAINNET_FEE_COLLECTOR="G..."            # Fee recipient
```

---

## Verification

### Check Contract on Network
```bash
# Get contract spec
stellar contract invoke \
  --network testnet \
  --source-account alice \
  --id CXXX... \
  -- --help

# Query contract via API
curl https://stellar.expert/api/v2/contract/CXXX...

# View in Stellar Expert
# https://stellar.expert/explorer/testnet/contract/CXXX...
```

### Query Escrow State
```bash
stellar contract invoke \
  --network testnet \
  --source-account alice \
  --id CXXX... \
  -- get_escrow \
  --id 0
```

---

## Troubleshooting

### "Contract ID not found"
- Verify contract was deployed to the correct network
- Check contract ID spelling
- Try alternative explorer: https://soroban.stellar.org/explorer

### "Account not found"
- Create account first: `stellar account create --name alice`
- Testnet: Fund via https://friendbot.stellar.org
- Mainnet: Must be pre-funded from exchange/another account

### "Network error"
- Check https://status.stellar.org
- Verify RPC host is accessible
- Try different RPC endpoint

---

## Integration Guides

### TypeScript/JavaScript (Bindings)
```typescript
import { Contract } from '@trustlink/contract-bindings';
import { SorobanContextType, useSorobanReact } from '@soroban-react/core';

const contract = new Contract({
  contractId: 'CXXX...',
  rpcUrl: 'https://soroban-testnet.stellar.org',
  networkPassphrase: 'Test SDF Network ; September 2015'
});

// Create escrow
const escrowId = await contract.create_escrow({
  seller: 'G...',
  token: 'C...',
  amount: '1000',
  shipping_window: '3600'
});
```

### Python/Backend
```python
from stellar_sdk import Server, TransactionBuilder, Network

server = Server('https://soroban-testnet.stellar.org')
network = Network.testnet_network()

# Use soroban-py or stellar-sdk for contract invocation
```

---

## Further Reading

- [Stellar Docs](https://developers.stellar.org)
- [Soroban Documentation](https://soroban.stellar.org)
- [SEP-41 Token Standard](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)
- [Stellar Expert](https://stellar.expert)
