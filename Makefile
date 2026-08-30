.PHONY: help build build-wasm test fmt clippy bench clean check check-error-codes doc audit indexer-test xtask-test \
	testnet testnet-reset testnet-stop fuzz-build fuzz fuzz-check setup hooks-uninstall

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build all contracts (lib target, cross-platform)
	cargo build --lib --release

build-debug: ## Build all contracts in debug mode
	cargo build --lib

build-wasm: ## Build contract wasm for deployment
	cargo build --target wasm32v1-none --release

test: ## Run library and integration tests (cross-platform)
	cargo test --lib
	cargo test --tests

test-all: ## Run the full project test suite including Rust and JS toolchain checks
	$(MAKE) test
	$(MAKE) bindings-typecheck
	$(MAKE) indexer-test
	$(MAKE) xtask-test

test-verbose: ## Run all tests with output
	cargo test --lib -- --nocapture

fmt: ## Format all code
	cargo fmt --all

fmt-check: ## Check formatting without modifying files
	cargo fmt --all -- --check

clippy: ## Run clippy lints
	cargo clippy --lib -- -D warnings

check: fmt-check clippy test check-error-codes bindings-typecheck indexer-test xtask-test ## Run all checks (fmt + clippy + test + drift checks + JS/Rust toolchain checks)

bench: ## Run benchmarks (if available)
	cargo test --release -- --ignored

fuzz-check: ## Type-check every fuzz target on stable (no cargo-fuzz needed)
	cargo check --manifest-path contracts/escrow/fuzz/Cargo.toml --all-targets

fuzz-build: ## Compile every fuzz target (requires nightly + cargo-fuzz)
	cd contracts/escrow && cargo fuzz build --release

fuzz: ## Run every fuzz target for FUZZ_TIME seconds each (default 60)
	cd contracts/escrow && for t in $$(cargo fuzz list); do \
		echo "fuzzing $$t"; \
		cargo fuzz run --release $$t -- -max_total_time=$${FUZZ_TIME:-60} || exit 1; \
	done

clean: ## Clean build artifacts
	cargo clean

check-error-codes: ## Verify errors.rs and bindings/src/errors.ts agree
	node scripts/check-error-codes.mjs

bindings-typecheck: ## Type-check the generated TypeScript bindings
	cd bindings && npm install && npm run typecheck

# Contract-specific targets
build-escrow: ## Build only the escrow contract
	cargo build --lib --release -p escrow

test-escrow: ## Test only the escrow contract
	cargo test --lib -p escrow

test-escrow-verbose: ## Test escrow contract with output
	cargo test --lib -p escrow -- --nocapture

# Bindings
bindings: ## Generate TypeScript bindings
	cd bindings && npm run build

bindings-test: ## Test TypeScript bindings
	cd bindings && npm test

bindings-install: ## Install bindings dependencies
	cd bindings && npm install

# Indexer
indexer-test: ## Typecheck and test the event indexer
	cd indexer && npm install && npm run typecheck && npm test

# Developer CLI (xtask is its own workspace, so root cargo test does not reach it)
xtask-test: ## Test the cargo xtask developer CLI
	cd xtask && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test

# Detect Docker Compose v1 vs v2
DOCKER_COMPOSE := $(shell docker compose version >/dev/null 2>&1 && echo "docker compose" || echo "docker-compose")

# Docker
docker-up: ## Start local Stellar network
	$(DOCKER_COMPOSE) up -d

docker-down: ## Stop local Stellar network
	$(DOCKER_COMPOSE) down

# Local devnet
testnet: ## Start local devnet, deploy the contract and seed test escrows
	./scripts/start-testnet.sh

testnet-reset: ## Recreate the local devnet from scratch
	./scripts/start-testnet.sh --reset

testnet-stop: ## Stop and remove the local devnet container
	./scripts/start-testnet.sh --stop

# Git hooks
setup: bindings-install build test hooks-install ## Full project setup (install + build + test + git hooks)
	@echo "Setup complete!"

hooks-install: ## Install pre-commit git hooks
	@echo "Installing git hooks..."
	@chmod +x .githooks/pre-commit
	@git config core.hooksPath .githooks
	@echo "Git hooks installed successfully."
	@echo "Pre-commit hook runs fmt + clippy checks that match CI."

hooks-uninstall: ## Uninstall git hooks
	@git config --unset core.hooksPath
	@echo "Git hooks uninstalled. Using default .git/hooks/ directory."
.PHONY: e2e
e2e:
	./e2e/01_setup_and_deploy.sh
	./e2e/02_happy_path.sh
	./e2e/03_dispute_path.sh
	./e2e/04_cancel_path.sh
