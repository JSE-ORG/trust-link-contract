# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows semantic versioning once tagged releases begin.

Every pull request must update the `Unreleased` section unless it is strictly
non-user-facing repository maintenance.

## [Unreleased]

### Added

- Added `EVENT_SCHEMA_VERSION` constant (`u32 = 1`) to `contracts/escrow/src/events.rs`
  and stamped it as the `schema_version` field on every event payload struct.
  Indexers can use this field to detect breaking schema changes without inspecting
  topics.  The versioning policy and a per-version changelog table are documented
  in `docs/events.md`.
- Added `co_signed_release_by_signature` for buyer-authorized Ed25519 releases
  submitted by the primary payee.
- Added a complete storage layout reference in `docs/storage.md` documenting
  every `DataKey`/`StorageKey` entry with its type, storage tier, and TTL
  strategy.
- Added README status badges for CI, test count, code coverage, and crate
  version.
- Added a `coverage` job to the CI workflow that uploads `cargo-llvm-cov`
  results to Codecov.
- Added event schema reference for indexer developers in `docs/events.md`.
- Added Soroban SDK and environment compatibility matrix in
  `docs/soroban-compatibility.md`.
- Added formal escrow lifecycle state-machine specification in
  `docs/state-machine.md`.
- Added this Keep a Changelog file.

### Changed

- Added the non-null `resolved_by` address to `DisputeResolved` event payloads;
  timeout refunds are attributed to the escrow contract. Bumped the event
  schema version to 5 and updated indexer documentation.
- Rewrote `CONTRIBUTING.md`: corrected the project structure, build/test
  commands, and wasm target (`wasm32v1-none`) to match the current workspace
  layout, and added a Quick Start section so new contributors can set up in
  under 30 minutes.

### Deprecated

- Nothing yet.

### Removed

- Nothing yet.

### Fixed

- Strict resolver mode now checks current registry approval on every dispute
  resolution and vote, immediately blocking resolvers removed during an active
  dispute, and excludes de-listed resolvers' previous votes from the quorum.

### Security

- Nothing yet.

