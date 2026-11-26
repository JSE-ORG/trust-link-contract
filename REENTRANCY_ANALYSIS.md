# Reentrancy Analysis: Why Soroban Prevents External Reentrancy

## Purpose and Scope

This document evaluates the reentrancy risk profile of the TrustLink Soroban contract, establishing that the Soroban host environment fundamentally prevents classic external reentrancy attacks by design.

**Key Scope:**

- **What this covers:** External reentrancy attacks in which a called contract re-enters the calling contract before the first invocation completes.
- **What this does not cover:** Logic bugs, authorization mistakes, state ordering errors, or design flaws in application logic—all of which remain possible and require standard secure coding practices.

The analysis focuses on the core mechanisms that make the traditional EVM-style reentrancy pattern infeasible in Soroban, with specific reference to how TrustLink's token flows and state transitions operate within the Soroban execution model.

---

## Executive Summary

Soroban executes contract code within a host-managed environment that differs fundamentally from transaction-oriented networks like Ethereum. Because of this design:

1. **Host-controlled invocation frames** — The Soroban host (not user code) creates, manages, and bounds each contract invocation. Contract code cannot arbitrarily suspend or resume execution.

2. **Synchronous, bounded call stacks** — Cross-contract calls are explicit, synchronous, and strictly nested. Control always returns to the caller after the callee completes; there is no ambient execution context that allows a callee to interrupt or re-enter a caller's frame.

3. **Mediated authorization and state access** — Authorization checks (`require_auth()`) and state modifications are managed by the host in specific sequence. A contract cannot bypass these controls or re-arrange their order.

4. **No callback injection** — External contracts cannot register callbacks or register themselves for execution at arbitrary points. All invocations are explicit and visible in the contract code.

**Conclusion:** The classic external reentrancy attack—in which a malicious callee re-enters the original caller before the first call completes—is structurally prevented by Soroban's design. However, developers must still follow standard secure practices for state consistency, authorization, and cross-contract interaction.

---

## Soroban Execution Model

Soroban smart contracts do not execute in isolation triggered by arbitrary external events. Instead, they run within a host-managed execution environment with strict boundaries.

### Invocation Flow

A typical contract invocation follows this pattern:

1. **Entry point:** A user or external system submits a transaction to the Soroban host, naming a specific contract and function.
2. **Host initialization:** The host prepares an execution environment: it loads the contract code, validates the transaction's signature, and prepares an `Env` object that provides access to storage, ledger state, and cross-contract communication.
3. **Contract execution:** The contract function runs to completion within this environment.
4. **State finalization:** Any storage writes, events, or side effects are finalized only when the contract execution completes.
5. **Transaction settlement:** The Soroban host records the transaction result on the ledger.

### Key Properties

**Synchronous Execution:** Contract functions run to completion before any other contract code executes. There is no concurrency or interleaving of multiple contract frames at the same logical step.

**Bounded Call Stack:** The host tracks and limits the depth of nested contract calls. This prevents unbounded recursion and ensures that the call graph remains observable.

**Explicit Invocation:** Every time a contract calls another contract, the call is made via an explicit API call (e.g., `token::Client::transfer()`). There are no implicit callbacks, hooks, or registration mechanisms that allow a callee to trigger code in the caller without the caller explicitly invoking it.

**Mediated Storage Access:** All state changes (reads and writes) go through the `env.storage()` API, which is controlled by the host. The contract cannot access storage outside of the host-provided interface, and the host enforces atomicity and ordering guarantees.

---

## Call Stack Behavior

Understanding the exact call stack mechanics is essential to see why reentrancy attacks do not apply.

### Single Invocation Frame

When a transaction invokes `contract::function()`, the host creates a single invocation frame for that function call. The frame remains open until the function returns.

```
User Transaction
       │
       ▼
┌─────────────────────────────────┐
│  Host Invocation Frame          │
│  ┌─────────────────────────┐   │
│  │  Contract Function      │   │
│  │  (Running)              │   │
│  └─────────────────────────┘   │
└─────────────────────────────────┘
       │
       ▼
   (Transaction settles)
```

### Nested Call Behavior

If the contract explicitly calls another contract (e.g., via `token::Client::transfer()`), the host creates a nested frame for the callee. Crucially, control does not return to the caller until the nested call completes.

```
User Transaction
       │
       ▼
┌─────────────────────────────────┐
│  Host Frame: ContractA          │
│  ┌─────────────────────────┐   │
│  │  ContractA::function()  │   │
│  │  (Running)              │   │
│  │                         │   │
│  │    token::transfer()    │   │
│  │    (explicit call)      │   │
│  │         │               │   │
│  │         ▼               │   │
│  │  ┌──────────────────┐  │   │
│  │  │ Host Frame:      │  │   │
│  │  │ Token Contract   │  │   │
│  │  │                  │  │   │
│  │  │ (Running)        │  │   │
│  │  │ (No callbacks!)  │  │   │
│  │  └──────────────────┘  │   │
│  │         │               │   │
│  │    (control returns)     │   │
│  │         │               │   │
│  │    (resume execution)   │   │
│  └─────────────────────────┘   │
└─────────────────────────────────┘
       │
       ▼
   (Transaction settles)
```

**Critical Detail:** The callee (Token Contract in the example) executes in its own frame with its own storage context. When the callee completes, control returns directly to the point in the caller where the call was made. The callee cannot inject code into the caller's frame, nor can it cause the caller to suspend and resume at an arbitrary point.

### No Suspension or Resumption

The Soroban host does not provide mechanisms for:
- Suspending a contract's execution and resuming it later.
- Registering callbacks that execute automatically during another contract's frame.
- Saving a partial execution state and re-entering it from external code.

All of these are foundational to standard reentrancy attacks. Without them, the classic attack is impossible.

---

## Why Classic Reentrancy Does Not Apply

### The Classic Reentrancy Pattern (EVM Example)

On the Ethereum Virtual Machine, reentrancy exploits follow this pattern:

1. **Vulnerable contract calls an external contract** before updating its own state.
2. **External contract (attacker's code) calls back** into the vulnerable contract.
3. **Vulnerable contract's second invocation sees the old state** because the first invocation has not yet updated storage.
4. **Attacker extracts value multiple times** before the first invocation's state update occurs.

Example:

```solidity
// Simplified EVM contract with a classic reentrancy bug
function withdraw(uint amount) public {
    require(balance[msg.sender] >= amount);
    
    // External call BEFORE state update — vulnerable
    (bool success, ) = msg.sender.call{value: amount}("");
    require(success);
    
    // State update happens AFTER external call
    balance[msg.sender] -= amount;  // Too late — attacker already re-entered
}
```

Exploit:

```solidity
// Attacker's contract
function attack() public {
    victim.withdraw(10);
}

// This function is called by the victim's withdraw()
receive() external payable {
    if (victim.balance > 0) {
        victim.withdraw(10);  // Re-enter! balance[msg.sender] has not been updated yet
    }
}
```

The re-entry happens because:
- The EVM allows arbitrary calldata execution from any address.
- The attacker controls when their contract's receive function returns.
- The victim contract's balance check sees stale state during the second withdrawal.

### Why This Pattern Fails on Soroban

Soroban's architecture prevents this pattern for fundamental reasons:

#### 1. No Calldata-Based Dispatch

On EVM, `msg.sender.call(payload)` allows arbitrary code execution at an address determined by user input. The called code can be anything, including a contract the attacker controls.

**Soroban:** Contracts do not accept arbitrary calldata. Cross-contract calls are made via explicit typed interfaces (e.g., `token::Client::transfer()`). The contract being called is determined by the caller's code, not by user input. This eliminates calldata-injection attacks and makes all cross-contract calls visible in the contract code.

#### 2. Synchronous Execution Model

On EVM, a contract can divide its execution: it calls another contract, the other contract runs, and then the first contract resumes. The attacker's code runs during this window.

**Soroban:** The host ensures that:
- Each contract invocation frame runs to completion without interruption.
- If a contract explicitly calls another contract, the callee's frame runs inside a nested scope.
- Control returns to the caller only after the callee's frame closes.

There is no window during which an attacker's code can execute while the caller is in a partially-updated state.

#### 3. Host-Imposed Authorization and Ordering

On EVM, the caller and callee coordinate through state (storage) alone. There is no builtin mechanism to enforce that re-entry is forbidden or to track authorization across frames.

**Soroban:** Authorization is explicit and checked before state modification:

```rust
pub fn fund_escrow(env: Env, escrow_id: u32, buyer: Address) {
    // Authorization check BEFORE any state change
    buyer.require_auth();

    // Load state
    let mut escrow: EscrowData = env
        .storage()
        .instance()
        .get(&DataKey::Escrow(escrow_id))
        .expect("escrow not found");

    // Verify precondition
    assert!(escrow.state == EscrowState::Pending, "escrow not pending");

    // Update state
    escrow.buyer = Some(buyer.clone());
    escrow.state = EscrowState::Funded;
    escrow.funded_at = env.ledger().timestamp();

    // Call external contract (token transfer)
    let token_client = token::Client::new(&env, &escrow.token);
    token_client.transfer(&buyer, &env.current_contract_address(), &escrow.amount);

    // Write state AFTER external call
    env.storage().instance().set(&DataKey::Escrow(escrow_id), &escrow);

    // Emit event
    env.events().publish(("fund_escrow",), escrow_id);
}
```

The key observations:
- `require_auth()` is checked first, and the host enforces it.
- State changes are atomic from the caller's perspective (all writes happen together when the frame closes).
- A re-entrant call would have a different `buyer` address (because `require_auth()` is per-frame) and would encounter a different state (because the earlier frame's writes are not visible until it completes).

#### 4. No Callback Registration

On EVM, a contract can emit an event and rely on off-chain watchers to call it back, or it can register a callback address during a call. This allows complex re-entry patterns.

**Soroban:** There is no callback registration API. A contract cannot ask another contract to call it back later. All cross-contract invocations are explicit in the code.

---

## Cross-Contract Security Rules

TrustLink performs cross-contract calls via the SEP-41 token interface. These calls are synchronous, bounded, and subject to Soroban's host-level security guarantees.

### SEP-41 Token Transfers

The TrustLink contract invokes `token::Client::transfer()` in four scenarios:

1. **fund_escrow:** `transfer(buyer → contract)` — Buyer deposits escrow funds.
2. **confirm_delivery:** `transfer(contract → seller)` — Release funds on delivery confirmation.
3. **auto_release:** `transfer(contract → seller)` — Auto-release after shipping window.
4. **resolve_dispute:** `transfer(contract → seller or buyer)` — Resolver's decision.

Each invocation follows the same pattern:

```rust
let token_client = token::Client::new(&env, &escrow.token);
token_client.transfer(&from, &to, &amount);
```

### Authorization Semantics

When `token::Client::transfer()` is called:

1. The Soroban host creates a nested invocation frame for the token contract.
2. The token contract executes its transfer logic within this frame.
3. The token contract may check authorization (e.g., that the `from` address has authorized this transaction).
4. Token state is updated (balances, allowances, etc.).
5. Control returns to the TrustLink contract.

**Crucially:** The token contract cannot re-enter TrustLink because:
- The token contract code is separate from TrustLink code.
- If the token contract were to call TrustLink's `fund_escrow()`, that would be a new explicit call visible in the token contract's code (which it does not make).
- Even if the token contract somehow did call back, the host would see two separate frames with two separate authorization contexts.

### State Visibility Across Frames

Each frame has its own view of storage. A nested frame cannot see uncommitted writes from the parent frame.

Example:

```rust
// TrustLink's fund_escrow (simplified)
escrow.state = EscrowState::Funded;  // State change is local to this frame

let token_client = token::Client::new(&env, &escrow.token);
token_client.transfer(...);  // Token contract runs in a nested frame
// Token cannot see the state change above; it has a separate storage context

env.storage().instance().set(&DataKey::Escrow(escrow_id), &escrow);  // Committed
```

This isolation prevents a re-entrant call from observing partial state updates.

---

## Practical Security Implications

Although Soroban's architecture prevents classic external reentrancy, developers must still implement secure coding practices:

### Order of Operations

Whenever possible, update state before making external calls. While reentrancy attacks are not feasible, this practice reduces the attack surface for logic bugs.

**Example in TrustLink:**

```rust
pub fn confirm_delivery(env: Env, escrow_id: u32) {
    let escrow: EscrowData = env
        .storage()
        .instance()
        .get(&DataKey::Escrow(escrow_id))
        .expect("escrow not found");

    assert!(escrow.state == EscrowState::Funded, "escrow not funded");
    let buyer = escrow.buyer.clone().expect("escrow has no buyer");
    buyer.require_auth();

    // Perform token transfer (external call) BEFORE state update
    let token_client = token::Client::new(&env, &escrow.token);
    token_client.transfer(
        &env.current_contract_address(),
        &escrow.seller,
        &escrow.amount,
    );

    // Update state AFTER external call
    let mut updated = escrow;
    updated.state = EscrowState::Completed;
    env.storage().instance().set(&DataKey::Escrow(escrow_id), &updated);
    env.events().publish(("confirm_delivery",), escrow_id);
}
```

In this case, the order (external call before state update) is acceptable because:
- The state is already validated before the call.
- The token contract has no way to re-enter `confirm_delivery()`.
- If the token contract somehow fails, the transaction is rolled back and the state change never happens.

### Authorization Isolation

Each frame has an independent authorization context. A contract frame checks `require_auth()` based on the transaction's signature, not on whether another contract called it.

**In TrustLink:**

```rust
pub fn fund_escrow(env: Env, escrow_id: u32, buyer: Address) {
    buyer.require_auth();  // This checks the transaction signature for 'buyer'
    // ...
}

pub fn confirm_delivery(env: Env, escrow_id: u32) {
    // ...
    buyer.require_auth();  // This is a separate check; the token contract cannot satisfy it
    // ...
}
```

Even if the token contract calls back to TrustLink, the second invocation would need its own `require_auth()` check, which the token contract cannot satisfy (because it is not the transaction signer).

### Input Validation

Always validate inputs and preconditions before external calls. Do not assume that a called contract will behave correctly.

**In TrustLink:**

```rust
pub fn fund_escrow(env: Env, escrow_id: u32, buyer: Address) {
    buyer.require_auth();

    let mut escrow: EscrowData = env
        .storage()
        .instance()
        .get(&DataKey::Escrow(escrow_id))
        .expect("escrow not found");

    // Precondition check
    assert!(escrow.state == EscrowState::Pending, "escrow not pending");

    // State updates
    escrow.buyer = Some(buyer.clone());
    escrow.state = EscrowState::Funded;
    escrow.funded_at = env.ledger().timestamp();

    // External call
    let token_client = token::Client::new(&env, &escrow.token);
    token_client.transfer(&buyer, &env.current_contract_address(), &escrow.amount);

    // Finalize
    env.storage()
        .instance()
        .set(&DataKey::Escrow(escrow_id), &escrow);
    env.events().publish(("fund_escrow",), escrow_id);
}
```

The state machine enforces that only a `Pending` escrow can be funded. This prevents a malicious caller from bypassing the logic via re-entry or other tricks.

### Invariant Preservation

Ensure that the contract's key invariants hold after each state transition, regardless of external calls.

**TrustLink invariants:**

- ESCROW_STATE_VALID: An escrow ID always has a valid state machine position.
- ONLY_ONE_TERMINAL: An escrow moves to `Completed` or `Refunded` and never transitions again.
- TOKEN_CONSERVATION: The total tokens held by the contract equals the sum of all active escrows' amounts.

These invariants are preserved because:
- State transitions are explicit and guarded by `assert!()` or mutual exclusion (e.g., `Pending` → `Funded` only).
- Terminal states (`Completed`, `Refunded`) have no outgoing transitions.
- Token transfers happen atomically within a single frame.

---

## Project-Specific Analysis

### Contract Entry Points

TrustLink exposes seven public functions:

| Function | Cross-Contract Calls |
|---|---|
| `create_escrow()` | None |
| `fund_escrow()` | `token::Client::transfer(buyer → contract)` |
| `confirm_delivery()` | `token::Client::transfer(contract → seller)` |
| `raise_dispute()` | None |
| `resolve_dispute()` | `token::Client::transfer(contract → seller/buyer)` |
| `auto_release()` | `token::Client::transfer(contract → seller)` |
| `get_escrow()` | None (read-only) |

Only four of seven functions make external calls, and all are to the same interface: the SEP-41 token contract passed at escrow creation time.

### State Transitions and Reentrancy Surface

The escrow state machine is:

```
Pending ──fund_escrow──► Funded ──confirm_delivery──► Completed
                           │
                           ├──raise_dispute──► Disputed ──resolve_dispute──► Completed or Refunded
                           │
                           └──auto_release (after shipping_window)──► Completed
```

**Reentrancy exposure analysis:**

- **fund_escrow (Pending → Funded):** Makes a token transfer from the buyer. The buyer's address comes from the transaction, not from user input, so an attacker cannot inject a malicious contract address. The token contract has no incentive or ability to re-enter `fund_escrow()` because doing so would require a different buyer address (due to `require_auth()`) and would encounter a state already in the `Funded` state (assertion would fail).

- **confirm_delivery (Funded → Completed):** Makes a token transfer to the seller. The seller's address is stored in the escrow, set at creation time. The token contract cannot re-enter and call `confirm_delivery()` again with the same escrow ID because the state is now `Completed`, failing the assertion `assert!(escrow.state == EscrowState::Funded)`.

- **resolve_dispute (Disputed → Completed/Refunded):** Makes a token transfer. Similar reasoning: the state machine prevents further transitions from terminal states.

- **auto_release (Shipped → Completed):** Makes a token transfer. The state machine prevents re-execution on the same escrow ID.

### No Callback Patterns

The contract does not register callbacks or use any mechanism that would allow the token contract or any external actor to trigger code in TrustLink asynchronously or implicitly.

All interactions are synchronous and explicit:

1. **Explicit calls only:** Cross-contract interactions are made via `token::Client::transfer()`, which is visible in the source code.
2. **No hooks:** The contract does not expose any `hook()` or `receive()` functions.
3. **No deferred execution:** Events are emitted for off-chain indexing, but the contract does not rely on off-chain callbacks.

### Conclusion for TrustLink

The TrustLink escrow contract's attack surface for reentrancy is **zero** because:

1. It makes external calls only to a well-defined token interface.
2. The state machine prevents repeated calls on the same resource.
3. Authorization is per-frame and cannot be spoofed by a called contract.
4. All cross-contract invocations are explicit and visible in the code.

---

## Conclusion

The Soroban host environment prevents classic external reentrancy attacks through fundamental architectural constraints:

1. **Synchronous, host-managed execution frames** isolate contract invocations and prevent arbitrary code injection or re-entry.
2. **Explicit cross-contract calls** ensure that all external interactions are visible in the contract code and cannot be triggered implicitly.
3. **Frame-local authorization** prevents bypassing security checks across invocation boundaries.
4. **Atomic state transitions** ensure that re-entrant calls see consistent state or fail.

**For the TrustLink contract specifically:** The combination of a strict state machine, explicit token transfers via SEP-41, and per-frame authorization results in a contract that is fundamentally immune to external reentrancy exploits.

However, developers must still apply standard secure coding practices:
- Validate inputs and preconditions.
- Check authorization before state modifications.
- Preserve contract invariants across all code paths.
- Assume called contracts may misbehave (even if re-entry is impossible).
- Test state transitions thoroughly.

The absence of reentrancy risk is a property of Soroban's design, not a substitute for rigorous code review, testing, and threat modeling. This document establishes the architectural foundation for secure escrow operations; implementation quality and logical correctness remain the developer's responsibility.

---

## References

- [Soroban Host Environment Documentation](https://developers.stellar.org/docs/learn/smart-contracts)
- [SEP-41: Stellar Asset Contract Interface](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)
- [TrustLink Contract Architecture](ARCHITECTURE.md)
- [TrustLink Contract Source Code](contracts/escrow/src/lib.rs)
