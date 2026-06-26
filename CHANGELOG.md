# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows semantic versioning once tagged releases begin.

Every pull request must update the `Unreleased` section unless it is strictly
non-user-facing repository maintenance.

## [Unreleased]

### Added

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

- Rewrote `CONTRIBUTING.md`: corrected the project structure, build/test
  commands, and wasm target (`wasm32v1-none`) to match the current workspace
  layout, and added a Quick Start section so new contributors can set up in
  under 30 minutes.

### Deprecated

- Nothing yet.

### Removed

- Nothing yet.

### Fixed

- Nothing yet.

### Security

- Nothing yet.

