#!/usr/bin/env bash
# scripts/start-testnet.sh
#
# One-command local Soroban devnet for integration testing.
#
#   1. Starts (or reuses) a Stellar QuickStart container in local/standalone mode.
#   2. Registers the network + funded test identities with the `stellar` CLI.
#   3. Builds, deploys and initializes the escrow contract.
#   4. Seeds escrows covering the Pending / Funded / Shipped / Disputed states.
#
# The script is idempotent: every step detects existing state and reuses it, so
# re-running it is a cheap no-op that just re-prints the environment summary.
#
# Usage: ./scripts/start-testnet.sh [OPTIONS]
#
# Options:
#   --reset             Destroy the container and all cached local state first
#   --rebuild           Force a fresh `cargo build` of the contract wasm
#   --no-seed           Deploy and initialize, but do not create test escrows
#   --seed-count <n>    Number of escrows to seed (default: 4)
#   --stop              Stop and remove the devnet container, then exit
#   --help              Show this help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
STATE_DIR="${REPO_ROOT}/.stellar/local-testnet"
CONTRACT_ID_FILE="${STATE_DIR}/contract_id"
SEEDED_FILE="${STATE_DIR}/seeded_escrow_ids"
ENV_FILE="${STATE_DIR}/local-testnet.env"
WASM_PATH="${REPO_ROOT}/target/wasm32v1-none/release/trustlink_escrow.wasm"

CONTAINER="${TRUSTLINK_LOCALNET_CONTAINER:-trustlink-localnet}"
IMAGE="${TRUSTLINK_LOCALNET_IMAGE:-stellar/quickstart:latest}"
HOST_PORT="${TRUSTLINK_LOCALNET_PORT:-8000}"
NETWORK="${TRUSTLINK_LOCALNET_NETWORK:-local}"
PASSPHRASE="${TRUSTLINK_LOCALNET_PASSPHRASE:-Standalone Network ; February 2017}"
BASE_URL="http://localhost:${HOST_PORT}"
FRIENDBOT_URL="${BASE_URL}/friendbot"

ADMIN="tl_local_admin"
FEE_COLLECTOR="tl_local_fee_collector"
SELLER="tl_local_seller"
BUYER="tl_local_buyer"
RESOLVER="tl_local_resolver"
IDENTITIES=("$ADMIN" "$FEE_COLLECTOR" "$SELLER" "$BUYER" "$RESOLVER")

ARBITRATION_FEE_BPS=100
ESCROW_AMOUNT="${ESCROW_AMOUNT:-1000000}" # 0.1 XLM in stroops
SHIPPING_WINDOW=86400
FEE_BPS=50
RESOLVER_FEE_BPS=50

RESET=0
REBUILD=0
SEED=1
SEED_COUNT=4
STOP_ONLY=0

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
log()  { echo -e "${BLUE}[testnet]${NC} $*"; }
ok()   { echo -e "${GREEN}[  ok  ]${NC} $*"; }
warn() { echo -e "${YELLOW}[ warn ]${NC} $*" >&2; }
die()  { echo -e "${RED}[ fail ]${NC} $*" >&2; exit 1; }

show_help() { grep "^#" "$0" | grep -E "^#( |$)" | cut -c 3-; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --reset)      RESET=1; shift ;;
        --rebuild)    REBUILD=1; shift ;;
        --no-seed)    SEED=0; shift ;;
        --seed-count) SEED_COUNT="$2"; shift 2 ;;
        --stop)       STOP_ONLY=1; shift ;;
        --help|-h)    show_help; exit 0 ;;
        *)            die "unknown option: $1 (try --help)" ;;
    esac
done

require_cmd() { command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"; }

# ── docker container ─────────────────────────────────────────────────────────
container_state() {
    docker inspect -f '{{.State.Status}}' "$CONTAINER" 2>/dev/null || echo "absent"
}

remove_container() {
    if [ "$(container_state)" != "absent" ]; then
        log "removing container '${CONTAINER}'"
        docker rm -f "$CONTAINER" >/dev/null
    fi
}

start_container() {
    case "$(container_state)" in
        running)
            ok "container '${CONTAINER}' already running"
            ;;
        absent)
            log "starting ${IMAGE} as '${CONTAINER}' on port ${HOST_PORT}"
            docker run -d --name "$CONTAINER" \
                -p "${HOST_PORT}:8000" \
                "$IMAGE" --local --enable-soroban-rpc >/dev/null
            ;;
        *)
            log "restarting existing container '${CONTAINER}'"
            docker start "$CONTAINER" >/dev/null
            ;;
    esac
}

# QuickStart has served the RPC endpoint under both paths across releases;
# probe until one of them answers a getHealth request.
detect_rpc_url() {
    local candidate
    for candidate in "${BASE_URL}/rpc" "${BASE_URL}/soroban/rpc"; do
        if curl -fsS -m 5 -X POST -H 'Content-Type: application/json' \
            -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' "$candidate" 2>/dev/null \
            | grep -q '"status"'; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

wait_for_rpc() {
    local deadline=$((SECONDS + 300)) url=""
    log "waiting for the devnet RPC to become healthy (up to 5 min)"
    while [ "$SECONDS" -lt "$deadline" ]; do
        if url="$(detect_rpc_url)"; then
            RPC_URL="$url"
            ok "RPC healthy at ${RPC_URL}"
            return 0
        fi
        sleep 3
    done
    die "devnet RPC did not become healthy; check: docker logs ${CONTAINER}"
}

# ── stellar CLI wiring ───────────────────────────────────────────────────────
configure_network() {
    log "registering network '${NETWORK}' with the stellar CLI"
    stellar network add "$NETWORK" \
        --rpc-url "$RPC_URL" \
        --network-passphrase "$PASSPHRASE" \
        --global --overwrite >/dev/null 2>&1 ||
        stellar network add "$NETWORK" \
            --rpc-url "$RPC_URL" \
            --network-passphrase "$PASSPHRASE" \
            --global >/dev/null 2>&1 ||
        warn "network '${NETWORK}' could not be (re)registered; assuming it already exists"
}

addr() { stellar keys address "$1"; }

ensure_identity() {
    local name="$1" address
    if ! stellar keys address "$name" >/dev/null 2>&1; then
        log "creating identity '${name}'"
        stellar keys generate "$name" --no-fund --global >/dev/null
    fi
    address="$(addr "$name")"
    # Friendbot funding is a no-op top-up once the account exists.
    curl -fsS -m 30 "${FRIENDBOT_URL}?addr=${address}" >/dev/null 2>&1 ||
        warn "friendbot did not fund '${name}' (${address}); it is probably already funded"
}

# ── contract lifecycle ───────────────────────────────────────────────────────
build_wasm() {
    if [ "$REBUILD" -eq 1 ] || [ ! -f "$WASM_PATH" ]; then
        log "building contract wasm"
        (cd "$REPO_ROOT" && cargo build --target wasm32v1-none --release -p trustlink-escrow)
    else
        ok "reusing existing wasm at ${WASM_PATH}"
    fi
    [ -f "$WASM_PATH" ] || die "wasm not found after build: ${WASM_PATH}"
}

invoke() {
    local source="$1"; shift
    stellar contract invoke \
        --id "$CONTRACT_ID" \
        --source "$source" \
        --network "$NETWORK" \
        "$@"
}

contract_is_live() {
    [ -n "${CONTRACT_ID:-}" ] &&
        stellar contract invoke --id "$CONTRACT_ID" --source "$ADMIN" \
            --network "$NETWORK" -- get_version >/dev/null 2>&1
}

deploy_contract() {
    CONTRACT_ID="$([ -f "$CONTRACT_ID_FILE" ] && cat "$CONTRACT_ID_FILE" || true)"
    if contract_is_live; then
        ok "reusing deployed contract ${CONTRACT_ID}"
        return
    fi

    log "deploying contract"
    CONTRACT_ID="$(stellar contract deploy \
        --wasm "$WASM_PATH" \
        --source "$ADMIN" \
        --network "$NETWORK")"
    [ -n "$CONTRACT_ID" ] || die "deployment returned an empty contract id"
    echo "$CONTRACT_ID" >"$CONTRACT_ID_FILE"
    : >"$SEEDED_FILE" # a fresh deployment has no seeded escrows
    ok "deployed contract ${CONTRACT_ID}"

    log "initializing contract"
    invoke "$ADMIN" -- initialize \
        --admin "$(addr "$ADMIN")" \
        --fee_collector "$(addr "$FEE_COLLECTOR")" \
        --arbitration_fee_bps "$ARBITRATION_FEE_BPS" >/dev/null ||
        warn "initialize failed (already initialized?)"
    ok "initialized"
}

# ── seeding ──────────────────────────────────────────────────────────────────
native_token() { stellar contract id asset --asset native --network "$NETWORK"; }

create_escrow() {
    invoke "$SELLER" -- create_escrow \
        --seller_or_payees "$(addr "$SELLER")" \
        --buyer "$(addr "$BUYER")" \
        --resolver "$(addr "$RESOLVER")" \
        --token "$TOKEN_ID" \
        --amount "$ESCROW_AMOUNT" \
        --fee_bps "$FEE_BPS" \
        --resolver_fee_bps "$RESOLVER_FEE_BPS" \
        --shipping_window "$SHIPPING_WINDOW" \
        --notes "seeded by start-testnet.sh" | tr -d '"'
}

# seed_escrow <target-state> -> prints the new escrow id
seed_escrow() {
    local target="$1" id
    id="$(create_escrow)"
    [ -n "$id" ] || die "create_escrow returned an empty escrow id"

    if [ "$target" != "Pending" ]; then
        invoke "$BUYER" -- fund_escrow --escrow_id "$id" --buyer "$(addr "$BUYER")" >/dev/null
    fi
    case "$target" in
        Shipped)
            invoke "$SELLER" -- mark_shipped \
                --caller "$(addr "$SELLER")" \
                --escrow_id "$id" \
                --tracking_id "LOCAL-TRACK-${id}" >/dev/null
            ;;
        Disputed)
            invoke "$BUYER" -- raise_dispute \
                --caller "$(addr "$BUYER")" \
                --escrow_id "$id" \
                --reason ITEM_NOT_RECEIVED \
                --description "seeded dispute" \
                --evidence_hash "$(printf '00%.0s' {1..32})" >/dev/null
            ;;
    esac
    echo "$id"
}

seed_escrows() {
    local shapes=(Pending Funded Shipped Disputed)
    local existing=0 i id shape
    [ -f "$SEEDED_FILE" ] && existing="$(wc -l <"$SEEDED_FILE" | tr -d ' ')"

    if [ "$existing" -ge "$SEED_COUNT" ]; then
        ok "already seeded ${existing} escrow(s); nothing to do"
        return
    fi

    TOKEN_ID="$(native_token)"
    log "seeding $((SEED_COUNT - existing)) escrow(s) with token ${TOKEN_ID}"
    for ((i = existing; i < SEED_COUNT; i++)); do
        shape="${shapes[$((i % ${#shapes[@]}))]}"
        id="$(seed_escrow "$shape")"
        echo "${id} ${shape}" >>"$SEEDED_FILE"
        ok "escrow ${id} seeded in state ${shape}"
    done
}

# ── env file + summary ───────────────────────────────────────────────────────
write_env_file() {
    cat >"$ENV_FILE" <<EOF
# Generated by scripts/start-testnet.sh — safe to source.
export STELLAR_NETWORK="${NETWORK}"
export STELLAR_RPC_URL="${RPC_URL}"
export STELLAR_NETWORK_PASSPHRASE="${PASSPHRASE}"
export STELLAR_FRIENDBOT_URL="${FRIENDBOT_URL}"
export TRUSTLINK_CONTRACT_ID="${CONTRACT_ID}"
export ADMIN_IDENTITY="${ADMIN}"
export FEE_COLLECTOR_IDENTITY="${FEE_COLLECTOR}"
export SELLER_IDENTITY="${SELLER}"
export BUYER_IDENTITY="${BUYER}"
export RESOLVER_IDENTITY="${RESOLVER}"
EOF
}

summary() {
    echo
    ok "local devnet ready"
    echo "  network      : ${NETWORK} (${PASSPHRASE})"
    echo "  rpc          : ${RPC_URL}"
    echo "  friendbot    : ${FRIENDBOT_URL}?addr=<G...>"
    echo "  contract id  : ${CONTRACT_ID}"
    echo "  identities   : ${IDENTITIES[*]}"
    if [ -s "$SEEDED_FILE" ]; then
        echo "  seeded       :"
        sed 's/^/                 escrow /' "$SEEDED_FILE"
    fi
    echo
    echo "  source ${ENV_FILE}"
    echo "  stop with: ${BASH_SOURCE[0]} --stop"
}

# ── main ─────────────────────────────────────────────────────────────────────
require_cmd docker
require_cmd curl

if [ "$STOP_ONLY" -eq 1 ]; then
    remove_container
    ok "devnet stopped"
    exit 0
fi

require_cmd stellar
require_cmd cargo

if [ "$RESET" -eq 1 ]; then
    remove_container
    rm -rf "$STATE_DIR"
fi

mkdir -p "$STATE_DIR"
touch "$SEEDED_FILE"

start_container
wait_for_rpc
configure_network

for id in "${IDENTITIES[@]}"; do
    ensure_identity "$id"
done

build_wasm
deploy_contract

if [ "$SEED" -eq 1 ]; then
    seed_escrows
else
    log "skipping seeding (--no-seed)"
fi

write_env_file
summary
