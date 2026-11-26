Contract Repository: `trustlink-contracts`
* **Role in Architecture:** The Core Protocol Engine (The "Judge")
* **Key Language & Tech:** Rust, Soroban SDK, cargo, stellar-cli

### 🔍 Deep Technical Overview
This repository contains the WASM-compiled smart contract logic running on Stellar’s Soroban virtual machine. It acts as the non-custodial, decentralized ledger vault that holds the physical escrow tokens (e.g., Stellar USDC or other custom SAC tokens) until release criteria are cryptographically validated.

### ⚙️ Core Functions & Modules
* **Escrow State Machine:** Tracks the immutable lifecycle of each link transaction:
  `Pending` ➔ `Funded` ➔ `Shipped` ➔ `Completed` / `Disputed`.
* **State Verification & Auth:** Strict enforcement of `require_auth()` boundaries ensuring only the designated `buyer` can deposit, and only authorized admins or verified delivery triggers can release.
* **Storage Optimization:** Implements temporary and persistent ledger instance storage to systematically manage TTL parameters, keeping gas fees highly predictable.
* **Events Framework:** Generates real-time, queryable on-chain events (`EscrowFunded`, `EscrowReleased`, `DisputeOpened`) which serve as the data feeds for the backend oracle.
