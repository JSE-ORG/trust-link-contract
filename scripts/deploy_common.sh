#!/usr/bin/env bash

# Core Deployment Logic extracted from original deploy.sh
# This script can be sourced by platform-specific wrappers.

# Directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Defaults
NETWORK="testnet"
SOURCE="mainnet-deployer"
WASM_FILE="${REPO_ROOT}/target/wasm32v1-none/release/trustlink_escrow.wasm"
ADMIN=""
FEE_COLLECTOR=""
ARBITRATION_FEE="300"
VERIFY_ONLY=0
INITIALIZE=0

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Helper functions
log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[✓]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[✗]${NC} $*" >&2; }

show_help() {
    cat <<'EOF'
TrustLink Mainnet Deployment Script
Usage: ./scripts/deploy.sh [OPTIONS]

Options:
  --network <testnet|public>   Network to deploy to (default: testnet)
  --source <account>           Signer account name (default: mainnet-deployer)
  --wasm <path>                Path to WASM file (default: target/wasm32v1-none/release/trustlink_escrow.wasm)
  --admin <address>            Admin address for initialize
  --fee-collector <address>    Fee collector address for initialize
  --arbitration-fee <bps>      Arbitration fee in basis points (default: 300)
  --verify-only                Only verify, don't deploy
  --help                       Show this help
EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --network)
            NETWORK="$2"
            shift 2
            ;;
        --source)
            SOURCE="$2"
            shift 2
            ;;
        --wasm)
            WASM_FILE="$2"
            shift 2
            ;;
        --admin)
            ADMIN="$2"
            INITIALIZE=1
            shift 2
            ;;
        --fee-collector)
            FEE_COLLECTOR="$2"
            shift 2
            ;;
        --arbitration-fee)
            ARBITRATION_FEE="$2"
            shift 2
            ;;
        --verify-only)
            VERIFY_ONLY=1
            shift
            ;;
        --help)
            show_help
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Validate network
if [[ ! "$NETWORK" =~ ^(testnet|public)$ ]]; then
    log_error "Invalid network: $NETWORK (must be testnet or public)"
    exit 1
fi

# Pre-flight checks
log_info "Running pre-flight checks..."

# Check Stellar CLI
if ! command -v stellar &>/dev/null; then
    log_error "Stellar CLI not found. Install from https://github.com/stellar/stellar-cli"
    exit 1
fi
log_success "Stellar CLI found: $(stellar version)"

# Check WASM file
if [[ ! -f "$WASM_FILE" ]]; then
    log_error "WASM file not found: $WASM_FILE"
    log_info "Build with: cargo xtask build-wasm && ./build.sh"
    exit 1
fi
WASM_SIZE_KB=$(($(stat -f%z "$WASM_FILE" 2>/dev/null || stat -c%s "$WASM_FILE" 2>/dev/null || wc -c < "$WASM_FILE") / 1024))
log_success "WASM file found: ${WASM_SIZE_KB} KB"
if [[ $WASM_SIZE_KB -gt 1024 ]]; then
    log_warn "WASM exceeds 1MB. Deployment may fail."
fi
if command -v shasum &>/dev/null; then
    WASM_HASH=$(shasum -a 256 "$WASM_FILE" | cut -d' ' -f1)
else
    WASM_HASH=$(sha256sum "$WASM_FILE" | cut -d' ' -f1)
fi
log_success "WASM SHA256: $WASM_HASH"

# Check account
log_info "Verifying account: $SOURCE"
if ! stellar account info --network "$NETWORK" --source-account "$SOURCE" &>/dev/null; then
    log_error "Account not found or not configured: $SOURCE"
    log_info "Configure with: stellar account create --name $SOURCE"
    exit 1
fi
log_success "Account verified"

# If verify-only, exit here
if [[ $VERIFY_ONLY -eq 1 ]]; then
    log_success "Verification passed. Ready to deploy."
    exit 0
fi

# Deploy contract
log_info "Deploying to $NETWORK..."
if [[ "$NETWORK" == "public" ]]; then
    log_warn "⚠️  DEPLOYING TO MAINNET - This is irreversible!"
    read -p "Type 'mainnet' to confirm: " confirm
    if [[ "$confirm" != "mainnet" ]]; then
        log_error "Deployment cancelled"
        exit 1
    fi
fi

DEPLOY_OUTPUT=$(mktemp)
trap "rm -f $DEPLOY_OUTPUT" EXIT

if ! stellar contract deploy \
    --network "$NETWORK" \
    --source-account "$SOURCE" \
    --wasm "$WASM_FILE" \
    2>&1 | tee "$DEPLOY_OUTPUT"; then
    log_error "Deployment failed. See output above."
    exit 1
fi

# Extract contract ID
CONTRACT_ID=$(grep "Contract ID:" "$DEPLOY_OUTPUT" | grep -o '[a-zA-Z0-9]*$' | head -1)
if [[ -z "$CONTRACT_ID" ]]; then
    log_error "Could not extract contract ID from deployment output"
    exit 1
fi
log_success "Contract deployed: $CONTRACT_ID"

# Initialize if addresses provided
if [[ $INITIALIZE -eq 1 ]]; then
    if [[ -z "$ADMIN" ]] || [[ -z "$FEE_COLLECTOR" ]]; then
        log_error "Admin and fee-collector required for initialize"
        exit 1
    fi
    log_info "Initializing contract..."
    if ! stellar contract invoke \
        --network "$NETWORK" \
        --source-account "$SOURCE" \
        --id "$CONTRACT_ID" \
        -- initialize \
        --admin "$ADMIN" \
        --fee_collector "$FEE_COLLECTOR" \
        --arbitration_fee_bps "$ARBITRATION_FEE"; then
        log_error "Initialize failed"
        exit 1
    fi
    log_success "Contract initialized"
fi

# Output summary

echo ""
log_success "Deployment complete!"
echo ""
echo "Contract ID:    $CONTRACT_ID"
echo "Network:        $NETWORK"
echo "WASM SHA256:    $WASM_HASH"
echo "WASM Size:      ${WASM_SIZE_KB} KB"
echo ""
if [[ "$NETWORK" == "testnet" ]]; then
    echo "View at: https://stellar.expert/explorer/testnet/contract/$CONTRACT_ID"
else
    echo "View at: https://stellar.expert/explorer/public/contract/$CONTRACT_ID"
fi

# Log deployment
LOG_FILE="${REPO_ROOT}/DEPLOYMENT.md"
if [[ ! -f "$LOG_FILE" ]]; then
    cat > "$LOG_FILE" <<'EOF'
Deployment Log

## Deployments

| Version | Network | Contract ID | Date |
|---------|---------|-------------|------|
EOF
fi
DEPLOYED_BY=$(git config user.name || echo "unknown")
DEPLOY_DATE=$(date -u +"%Y-%m-%d %H:%M:%S UTC")
echo "| - | $NETWORK | \`$CONTRACT_ID\` | $DEPLOY_DATE | $DEPLOYED_BY |" >> "$LOG_FILE"
log_success "Deployment logged to $LOG_FILE"

# End of core deployment script
