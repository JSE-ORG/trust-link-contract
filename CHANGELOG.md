# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows semantic versioning once tagged releases begin.

Every pull request must update the `Unreleased` section unless it is strictly
non-user-facing repository maintenance.

## [Unreleased]

### Added

- Added event schema reference for indexer developers in `docs/events.md`.
- Added Soroban SDK and environment compatibility matrix in
  `docs/soroban-compatibility.md`.
- Added formal escrow lifecycle state-machine specification in
  `docs/state-machine.md`.
- Added this Keep a Changelog file.

### Changed

- Split the escrow contract out of the monolithic `lib.rs` into `escrow.rs`
  (contract logic) and moved the `EscrowData` type into `types.rs`, matching the
  multi-file layout described in `CONTRIBUTING.md`. No ABI or behaviour change:
  public function signatures and event topics are unchanged and all items are
  re-exported from the crate root.

### Deprecated

- Nothing yet.

### Removed

- Nothing yet.

### Fixed

- Nothing yet.

### Security

- Nothing yet.

