# Resolver schemes

TrustLink supports a single resolver, an M-of-N committee, and a primary/backup
fallback. The resolver set is chosen when the escrow is created. A resolver
decision does not pay out immediately: it records a `Release` or `Refund`
resolution and moves the escrow to `PendingFinalization`.

## Single resolver

`ResolverSet::Single` is the backward-compatible 1-of-1 scheme. The one
resolver may call `resolve_dispute(escrow_id, resolution)` (or the equivalent
`vote` entry point) while the escrow is `Disputed`. The decision enters
`PendingFinalization`; `finalize_dispute` can settle it only after the 24-hour
appeal window has elapsed.

## Multi resolver: M-of-N vote

`ResolverSet::Multi` records one current vote per resolver. The resolver list
must contain unique addresses, the threshold must satisfy `0 < M <= N`, and
resolvers cannot be the buyer or seller. Votes for `Release` and `Refund` are
tallied separately; either outcome reaching M moves the escrow to
`PendingFinalization`.

Calling `vote` again updates that resolver's existing vote rather than counting
the resolver twice. `get_resolver_votes` exposes the current votes. A split
vote can remain below quorum, but it is not a permanent lock: after
`DISPUTE_DEADLOCK_WINDOW` (7 days), anyone may call
`resolve_deadlocked_dispute`, which uses simple majority and resolves a tie as
`Refund`. If no resolver has voted, or the dispute remains unresolved until
the admin-configured dispute timeout (30 days by default), either buyer or
seller may call `claim_dispute_timeout` to create a `Refund` resolution.

## Fallback deadline gating

`ResolverSet::Fallback` has a primary and backup. The primary may resolve at
any time. The backup becomes authorized only when
`ledger.timestamp() >= dispute_deadline`; this is an absolute ledger timestamp
chosen at creation, not a duration restarted when a dispute is raised or
appealed. The deadline is bounded at creation so an unresponsive primary cannot
set an unbounded handover delay. Fallback still uses the 1-of-1 threshold: the
first authorized resolver to act moves the escrow to `PendingFinalization`.

## Deadlock risk

M-of-N schemes can stall when votes are split or no resolver acts. The contract
provides the deadlock and timeout recovery paths above, but operators should
still choose a reachable threshold and monitor resolver availability. An
appeal by the buyer or seller while `PendingFinalization` is active clears the
resolution, increments `appeal_count`, and clears Multi votes for a fresh round;
the same resolver rules then apply again. See
[MULTI_RESOLVER_SCHEME.md](../MULTI_RESOLVER_SCHEME.md) and
[ARCHITECTURE.md](../ARCHITECTURE.md).
