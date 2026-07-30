.PHONY: help build build-wasm test fmt clippy bench clean check check-error-codes doc audit indexer-test \
	testnet testnet-reset testnet-stop fuzz-build fuzz

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build all contracts (lib target, cross-platform)
	cargo build --lib --release

build-debug: ## Build all contracts in debug mode
	cargo build --lib

build-wasm: ## Build contract wasm for deployment
	cargo build --target wasm32v1-none --release

test: ## Run all tests (lib target, cross-platform)
	cargo test --lib

test-verbose: ## Run all tests with output
	cargo test --lib -- --nocapture

fmt: ## Format all code
	cargo fmt --all

fmt-check: ## Check formatting without modifying files
	cargo fmt --all -- --check

clippy: ## Run clippy lints
	cargo clippy --lib -- -D warnings

bench: ## Run benchmarks (if available)
	cargo test --release -- --ignored

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

check: fmt-check clippy test check-error-codes ## Run all checks (fmt + clippy + test + error-code drift)

doc: ## Generate and open documentation
	cargo doc --open

audit: ## Run cargo audit for security vulnerabilities
	cargo audit 2>/dev/null || echo "Install cargo-audit: cargo install cargo-audit"

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
	cd indexer && npm run typecheck && npm test

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

# Full setup
setup: bindings-install build test ## Full project setup (install + build + test)
	@echo "Setup complete!"
