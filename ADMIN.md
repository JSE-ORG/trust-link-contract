# Admin Operations

Reference for operators of the TrustLink escrow contract's privileged
functions. For the full timelock model see
[ARCHITECTURE.md → Timelock Permission Model](ARCHITECTURE.md#timelock-permission-model);
for threat analysis see [THREAT_MODEL.md](THREAT_MODEL.md) (Threats 14 and 14a).

## Timelocked operations

Every mutating admin operation is a two-step `queue_<op>` → `execute_<op>`
flow:

| Step | Who | Rules |
|---|---|---|
| `queue_<op>(caller, params)` | Current admin (`require_auth`) | Stores a proposal with `ready_at = now + delay` and a domain-separated `params_hash`. |
| `execute_<op>(caller, params)` | **Anyone** (admin or keeper), signing for their own address | Reverts with `TimelockNotElapsed` before `ready_at`, `NotAuthorized` if the proposer is no longer admin, `InvalidProposalHash` if `params` differ from what was queued. |
| `cancel_timelock_op(caller, op)` | Current admin | Removes a pending proposal. |

### Running a keeper

Once a proposal's `ready_at` (from the `TimelockQueued` event) has passed, a
keeper can call `execute_<op>` with **its own address** as `caller` and the
exact parameters that were queued. The keeper only needs to sign for itself.
The `TimelockExecuted` event records both `proposer` (the admin) and
`executor`; `executor != proposer` marks a keeper-driven execution.

### Timelock delay

`DataKey::AdminTimelockDelay` defaults to 24 hours and can be set between
12 hours (`MIN_TIMELOCK_DELAY_SECONDS`) and 30 days
(`MAX_TIMELOCK_DELAY_SECONDS`) via `queue_set_timelock_delay` →
`execute_set_timelock_delay`. The change itself waits
`max(current_delay, new_delay)`:

- shortening 24h → 12h waits 24h;
- lengthening 24h → 72h waits 72h;
- proposals queued before the change keep their original `ready_at`.

## Pausing

There are two independent pause mechanisms.

| Mechanism | Calls | Affects |
|---|---|---|
| Global pause | `queue_pause_contract` / `execute_pause_contract` (and unpause) | User flows (`create_escrow`, `fund_escrow`, …). Admin operations keep working so a fix can still ship. |
| Action pause | `pause_action(caller, Symbol)` / `unpause_action(caller, Symbol)` — immediate, admin only | Only the named action. Query with `is_action_paused(Symbol)`. |

A paused action reverts with `ContractError::ContractPaused` (14).
`pause_action` and `unpause_action` are never themselves gated, so a paused
action can always be re-enabled.

### Pausable admin actions

Each of these entry points checks its own name, so pausing it takes effect
immediately for both admins and keepers:

| Symbol | Function | Notes |
|---|---|---|
| `queue_upgrade` | `queue_upgrade` | Stop new WASM upgrades from being queued. |
| `execute_upgrade` | `execute_upgrade` | Freeze an already-queued upgrade while you `cancel_timelock_op` it or rotate the key. |
| `queue_set_platform_fee` | `queue_set_platform_fee` | |
| `execute_set_platform_fee` | `execute_set_platform_fee` | |
| `set_platform_fee` | `set_platform_fee` | Immediate platform-fee setter. |
| `queue_set_arbitration_fee` | `queue_set_arbitration_fee` | |
| `execute_set_arbitration_fee` | `execute_set_arbitration_fee` | |
| `set_fee` | `set_fee` | Test/`testutils` builds only. |

Example (Stellar CLI):

```sh
stellar contract invoke --id "$ESCROW" --source admin -- \
  pause_action --caller "$ADMIN" --action execute_upgrade

stellar contract invoke --id "$ESCROW" -- \
  is_action_paused --action execute_upgrade   # true

stellar contract invoke --id "$ESCROW" --source admin -- \
  unpause_action --caller "$ADMIN" --action execute_upgrade
```

User-facing action symbols (`create_escrow`, `fund_escrow`, …) are paused the
same way; those checks also honour the global pause.

### Emergency playbook: suspected admin key exposure

1. `pause_action execute_upgrade` (and `queue_upgrade`) so nothing queued can
   be executed, even by a keeper.
2. `cancel_timelock_op` any proposal you did not queue.
3. Rotate the admin (`queue_set_admin` → `execute_set_admin`); proposals from
   the old admin can no longer execute after rotation.
4. `unpause_action` once the new key is in place.
