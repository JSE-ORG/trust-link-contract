# Deployment Log

Track all contract deployments to testnet and mainnet.

## Deployments

| Version | Network | Contract ID | Deployed By | Date | WASM SHA256 | Notes |
|---------|---------|-------------|-------------|------|------------|-------|
| — | testnet | — | — | — | — | Pre-mainnet (no live deployments) |

## Quick Links

- **Runbook**: [docs/runbook-release.md](./docs/runbook-release.md)
- **Network Config**: [docs/NETWORKS.md](./docs/NETWORKS.md)
- **Deployment Script**: [scripts/deploy.sh](./scripts/deploy.sh)
- **Stellar Expert**: https://stellar.expert/explorer/public

## Mainnet Checklist

Before deploying to mainnet, complete:

- [ ] Code freeze: all changes in main branch
- [ ] Build passes: `cargo xtask ci`
- [ ] Bindings compile: `cd bindings && npm run typecheck`
- [ ] Testnet smoke test: 72+ hours of live testing
- [ ] Security review: contracts, fees, state transitions
- [ ] Version bumped: Cargo.toml + bindings/package.json
- [ ] CHANGELOG.md updated
- [ ] Release PR approved
- [ ] Stellar CLI installed: `which stellar`
- [ ] Mainnet account funded: `stellar account info --network public`
- [ ] Admin & fee-collector addresses confirmed
