.PHONY: help build test fmt clippy bench clean check doc audit

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

build: ## Build all contracts in release mode
	cargo build --release

build-debug: ## Build all contracts in debug mode
	cargo build

test: ## Run all tests
	cargo test

test-verbose: ## Run all tests with output
	cargo test -- --nocapture

fmt: ## Format all code
	cargo fmt --all

fmt-check: ## Check formatting without modifying files
	cargo fmt --all -- --check

clippy: ## Run clippy lints
	cargo clippy --all-targets --all-features -- -D warnings

bench: ## Run benchmarks (if available)
	cargo test --release -- --ignored

clean: ## Clean build artifacts
	cargo clean

check: fmt-check clippy test ## Run all checks (fmt + clippy + test)

doc: ## Generate and open documentation
	cargo doc --open

audit: ## Run cargo audit for security vulnerabilities
	cargo audit 2>/dev/null || echo "Install cargo-audit: cargo install cargo-audit"

# Contract-specific targets
build-escrow: ## Build only the escrow contract
	cargo build --release -p escrow

test-escrow: ## Test only the escrow contract
	cargo test -p escrow

test-escrow-verbose: ## Test escrow contract with output
	cargo test -p escrow -- --nocapture

# Bindings
bindings: ## Generate TypeScript bindings
	cd bindings && npm run build

bindings-test: ## Test TypeScript bindings
	cd bindings && npm test

bindings-install: ## Install bindings dependencies
	cd bindings && npm install

# Docker
docker-up: ## Start local Stellar network
	docker-compose up -d

docker-down: ## Stop local Stellar network
	docker-compose down

# Full setup
setup: bindings-install build test ## Full project setup (install + build + test)
	@echo "Setup complete!"
