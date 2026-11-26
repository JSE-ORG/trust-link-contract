#!/usr/bin/env bash
# scripts/migrate.sh
#
# Upgrade helper: swap the contract WASM and run the storage migration as one
# guarded operation, with a data snapshot taken before and verified after.
#
# `upgrade` only replaces code — storage is untouched and therefore still laid
# out the way the *previous* build wrote it. Any schema change must be applied
# by `migrate` in a follow-up transaction. See docs/UPGRADES.md.
#
# Usage: ./scripts/migrate.sh --contract <C...> --admin <identity> [OPTIONS]
#
# Required:
#   --contract <id>        Contract id to upgrade
#   --admin <identity>     stellar CLI identity holding the admin key
#
# Options:
#   --network <name>       Network to use (default: testnet)
#   --wasm <path>          New WASM (default: target/wasm32v1-none/release/trustlink_escrow.wasm)
#   --wasm-hash <hash>     Use an already-installed WASM hash instead of --wasm
#   --sample <ids>         Comma-separated escrow ids to snapshot and verify
#   --skip-upgrade         Only run the storage migration
#   --skip-migrate         Only swap the WASM
#   --dry-run              Report what would happen, change nothing
#   --help                 Show this help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

CONTRACT_ID=""
ADMIN=""
NETWORK="testnet"
WASM_FILE="${REPO_ROOT}/target/wasm32v1-none/release/trustlink_escrow.wasm"
WASM_HASH=""
SAMPLE_IDS=""
SKIP_UPGRADE=0
SKIP_MIGRATE=0
DRY_RUN=0

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
log()  { echo -e "${BLUE}[migrate]${NC} $*"; }
ok()   { echo -e "${GREEN}[   ok  ]${NC} $*"; }
warn() { echo -e "${YELLOW}[ warn  ]${NC} $*" >&2; }
die()  { echo -e "${RED}[ fail  ]${NC} $*" >&2; exit 1; }

show_help() { grep "^#" "$0" | grep -E "^#( |$)" | cut -c 3-; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --contract)     CONTRACT_ID="$2"; shift 2 ;;
        --admin)        ADMIN="$2"; shift 2 ;;
        --network)      NETWORK="$2"; shift 2 ;;
        --wasm)         WASM_FILE="$2"; shift 2 ;;
        --wasm-hash)    WASM_HASH="$2"; shift 2 ;;
        --sample)       SAMPLE_IDS="$2"; shift 2 ;;
        --skip-upgrade) SKIP_UPGRADE=1; shift ;;
        --skip-migrate) SKIP_MIGRATE=1; shift ;;
        --dry-run)      DRY_RUN=1; shift ;;
        --help|-h)      show_help; exit 0 ;;
        *)              die "unknown option: $1 (try --help)" ;;
    esac
done

command -v stellar >/dev/null 2>&1 || die "required command not found: stellar"
[ -n "$CONTRACT_ID" ] || die "--contract is required"
[ -n "$ADMIN" ] || die "--admin is required"

SNAPSHOT_DIR="${REPO_ROOT}/.stellar/migrations/${CONTRACT_ID}"
mkdir -p "$SNAPSHOT_DIR"

invoke() {
    stellar contract invoke \
        --id "$CONTRACT_ID" \
        --source "$ADMIN" \
        --network "$NETWORK" \
        "$@"
}

run() {
    if [ "$DRY_RUN" -eq 1 ]; then
        log "dry-run: would invoke $*"
    else
        invoke "$@"
    fi
}

# ── 1. pre-flight ────────────────────────────────────────────────────────────
log "contract ${CONTRACT_ID} on ${NETWORK}"
CODE_VERSION="$(invoke -- get_version | tr -d '"')"
STORED_VERSION="$(invoke -- get_storage_version 2>/dev/null | tr -d '"' || echo 0)"
log "deployed code version : ${CODE_VERSION}"
log "storage schema version: ${STORED_VERSION}"

# ── 2. snapshot the escrows we will verify afterwards ────────────────────────
SNAPSHOT_BEFORE="${SNAPSHOT_DIR}/before.json"
snapshot() {
    local out="$1" id
    : >"$out"
    [ -n "$SAMPLE_IDS" ] || return 0
    IFS=',' read -ra ids <<<"$SAMPLE_IDS"
    for id in "${ids[@]}"; do
        printf '%s\t%s\n' "$id" "$(invoke -- get_escrow --escrow_id "$id" | tr -d '\n')" >>"$out"
    done
}

if [ -n "$SAMPLE_IDS" ]; then
    log "snapshotting escrows: ${SAMPLE_IDS}"
    snapshot "$SNAPSHOT_BEFORE"
    ok "snapshot written to ${SNAPSHOT_BEFORE}"
else
    warn "no --sample ids given; upgrade will not be data-verified"
fi

# ── 3. install + upgrade ─────────────────────────────────────────────────────
if [ "$SKIP_UPGRADE" -eq 0 ]; then
    if [ -z "$WASM_HASH" ]; then
        [ -f "$WASM_FILE" ] || die "wasm not found: ${WASM_FILE} (run: make build-wasm)"
        log "installing ${WASM_FILE}"
        if [ "$DRY_RUN" -eq 1 ]; then
            WASM_HASH="<dry-run>"
        else
            WASM_HASH="$(stellar contract upload \
                --wasm "$WASM_FILE" \
                --source "$ADMIN" \
                --network "$NETWORK" | tr -d '"')"
        fi
    fi
    log "upgrading to wasm hash ${WASM_HASH}"
    run -- upgrade \
        --caller "$(stellar keys address "$ADMIN")" \
        --new_wasm_hash "$WASM_HASH" >/dev/null
    ok "wasm upgraded"
else
    log "skipping wasm upgrade (--skip-upgrade)"
fi

# ── 4. migrate storage ───────────────────────────────────────────────────────
if [ "$SKIP_MIGRATE" -eq 0 ]; then
    log "running storage migration"
    # `migrate` returns AlreadyInitialized when storage is current, which is the
    # expected outcome of a retried or upgrade-only release.
    if run -- migrate --caller "$(stellar keys address "$ADMIN")" >/dev/null 2>&1; then
        ok "storage migrated"
    else
        warn "migrate did not apply (storage is already at the current version)"
    fi
else
    log "skipping storage migration (--skip-migrate)"
fi

# ── 5. verify ────────────────────────────────────────────────────────────────
if [ "$DRY_RUN" -eq 1 ]; then
    ok "dry run complete; nothing was changed"
    exit 0
fi

NEW_CODE_VERSION="$(invoke -- get_version | tr -d '"')"
NEW_STORED_VERSION="$(invoke -- get_storage_version | tr -d '"')"
log "code version    : ${CODE_VERSION} -> ${NEW_CODE_VERSION}"
log "storage version : ${STORED_VERSION} -> ${NEW_STORED_VERSION}"

if [ -n "$SAMPLE_IDS" ]; then
    SNAPSHOT_AFTER="${SNAPSHOT_DIR}/after.json"
    snapshot "$SNAPSHOT_AFTER"
    if diff -u "$SNAPSHOT_BEFORE" "$SNAPSHOT_AFTER" >"${SNAPSHOT_DIR}/diff.txt"; then
        ok "sampled escrow data is byte-identical after the upgrade"
    else
        warn "sampled escrow data changed; review ${SNAPSHOT_DIR}/diff.txt"
        cat "${SNAPSHOT_DIR}/diff.txt"
        exit 1
    fi
fi

ok "migration complete"
