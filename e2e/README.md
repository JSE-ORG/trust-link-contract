# End-to-End Tests (real Soroban testnet)

Unit tests run against the mock `Env`. These scripts exercise the deployed
escrow contract on a **real Soroban network** (Testnet by default) via the
`stellar` CLI, deploying the contract and driving each lifecycle path while
asserting the resulting on-chain ledger state.

## Prerequisites

- [`stellar` CLI](https://developers.stellar.org/docs/tools/cli) (tested with 25.x)
- `jq`
- A Rust toolchain with the `wasm32v1-none` target (for the build step)

No funded account is required up front — the scripts create and fund test
identities via Friendbot.

## Layout

| Script | Purpose |
|---|---|
| `lib.sh` | Shared helpers: idempotent identities, contract invoke wrapper, state assertions |
| `01_setup_and_deploy.sh` | Create/fund identities, build wasm, deploy + initialize (idempotent) |
| `02_happy_path.sh` | create → fund → ship → deliver → confirm → **Completed** |
| `03_dispute_path.sh` | create → fund → raise dispute → resolve → **Refunded/Completed** |
| `04_cancel_path.sh` | create → cancel → **Canceled** |
| `run_all.sh` | Runs setup + all paths in order |

## Usage

```bash
cd e2e
./run_all.sh
# or run a single path (after setup):
./01_setup_and_deploy.sh
./02_happy_path.sh
```

Override defaults with environment variables:

```bash
STELLAR_NETWORK=testnet ESCROW_AMOUNT=10000000 ./run_all.sh
```

## Idempotency

- **Identities** are created only if missing, then funded (Friendbot funding is
  safe to repeat).
- **Deployment** is cached in `e2e/.state/<network>.contract_id`. A re-run
  reuses the existing contract (verified live via `get_version`) and never
  re-initializes it.
- **Each path** creates a fresh escrow id, so re-running never collides with or
  mutates a previously created escrow.

The `e2e/.state/` directory holds local-only deployment state and is
git-ignored.

## What is asserted

Every step checks the escrow's on-chain state via `get_escrow` and fails loudly
on a mismatch, so each path "produces the expected ledger state":

- Happy path: `Pending → Funded → Shipped → Completed`
- Dispute path: `Pending → Funded → Disputed → Refunded` (or `Completed` if
  resolved in the seller's favour)
- Cancel path: `Pending → Canceled`

## Notes / current limitations

- The escrow `create_escrow` entrypoint takes the modern 9-argument interface
  (`seller_or_payees, buyer, resolver, token, amount, fee_bps,
  resolver_fee_bps, shipping_window, notes`). If the deployed ABI differs,
  adjust `create_escrow` in `lib.sh`.
- These scripts were authored and syntax-checked (`bash -n`) but **not executed
  end-to-end against testnet in CI** here; run `./run_all.sh` locally against
  testnet to capture live results. Note that `contracts/escrow/src/lib.rs`
  currently defines `create_escrow` twice in one `impl`, so the contract must
  build cleanly before deployment will succeed.
