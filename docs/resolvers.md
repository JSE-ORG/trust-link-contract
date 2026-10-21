# Resolver schemes

TrustLink supports a single resolver, an M-of-N vote, and a primary/backup
fallback. Resolver configuration is selected when the escrow is created;
ordinary delivery release remains governed by delivery and shipping windows.

## Single resolver

`ResolverSet::Single` is the backward-compatible 1-of-1 scheme. The resolver
authorizes `Release` or `Refund`; the decision enters `PendingFinalization`
until the appeal window closes.

## Multi resolver: M-of-N vote

`ResolverSet::Multi` records one vote per resolver and executes when either
outcome reaches threshold M of N. Resolvers must be unique and non-conflicting
with the escrow parties. A vote that never reaches quorum leaves the dispute
open.

## Fallback deadline gating

`ResolverSet::Fallback` has primary and backup resolvers. The primary may
resolve immediately; the backup may resolve only when
`ledger.timestamp() >= dispute_deadline`. This does not bypass the appeal
window or `PendingFinalization` checks.

## Deadlock risk

M-of-N schemes can deadlock when quorum is unavailable. Fallback schemes can
deadlock when the backup is not authorized or the deadline is misconfigured.
Use monitoring, tested signer rotation, and a live backup path. See
[MULTI_RESOLVER_SCHEME.md](../MULTI_RESOLVER_SCHEME.md) and
[ARCHITECTURE.md](../ARCHITECTURE.md).
