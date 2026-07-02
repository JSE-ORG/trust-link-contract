# e2e/lib.sh
# Shared helpers for the Soroban testnet end-to-end scripts.
#
# Design goals (issue #407):
#   - Idempotent: every script can be re-run without manual cleanup. Identities
#     are reused if they already exist, the contract is reused if already
#     deployed, and each flow creates a fresh escrow id so re-runs don't clash.
#   - Observable: every flow asserts the on-chain escrow state after each step,
#     so "all paths produce expected ledger state" is verified, not assumed.
#
# Source this file from the numbered scripts: `source "$(dirname "$0")/lib.sh"`.

set -euo pipefail

NETWORK="${STELLAR_NETWORK:-testnet}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${HERE}/.." && pwd)"
STATE_DIR="${HERE}/.state"
CONTRACT_ID_FILE="${STATE_DIR}/${NETWORK}.contract_id"
WASM_PATH="${REPO_ROOT}/target/wasm32-unknown-unknown/release/trustlink-escrow.wasm"

# Identity aliases used across the suite.
ADMIN="${ADMIN_IDENTITY:-tl_admin}"
FEE_COLLECTOR="${FEE_COLLECTOR_IDENTITY:-tl_fee_collector}"
SELLER="${SELLER_IDENTITY:-tl_seller}"
BUYER="${BUYER_IDENTITY:-tl_buyer}"
RESOLVER="${RESOLVER_IDENTITY:-tl_resolver}"

mkdir -p "${STATE_DIR}"

# ── logging ────────────────────────────────────────────────────────────────
log()  { printf '\033[0;34m[e2e]\033[0m %s\n' "$*"; }
ok()   { printf '\033[0;32m[ ok]\033[0m %s\n' "$*"; }
warn() { printf '\033[0;33m[warn]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[0;31m[fail]\033[0m %s\n' "$*" >&2; exit 1; }

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

# ── identities (idempotent) ──────────────────────────────────────────────────
# Create the keypair only if it does not already exist, then ensure it is funded
# (Friendbot funding is safe to repeat).
ensure_identity() {
  local name="$1"
  if stellar keys address "${name}" >/dev/null 2>&1; then
    log "identity '${name}' already exists ($(stellar keys address "${name}"))"
  else
    log "creating identity '${name}'"
    stellar keys generate "${name}" --network "${NETWORK}" >/dev/null
  fi
  # Fund (idempotent — re-funding an existing account is a no-op top-up).
  stellar keys fund "${name}" --network "${NETWORK}" >/dev/null 2>&1 || \
    warn "could not fund '${name}' (already funded or Friendbot rate-limited)"
}

addr() { stellar keys address "$1"; }

# ── contract invocation ──────────────────────────────────────────────────────
contract_id() {
  [ -f "${CONTRACT_ID_FILE}" ] || die "no deployed contract; run 01_setup_and_deploy.sh first"
  cat "${CONTRACT_ID_FILE}"
}

# invoke <source-identity> -- <function> [args...]
invoke() {
  local source="$1"; shift
  stellar contract invoke \
    --id "$(contract_id)" \
    --source "${source}" \
    --network "${NETWORK}" \
    "$@"
}

# ── assertions ───────────────────────────────────────────────────────────────
# escrow_state <escrow_id> -> prints the EscrowState variant (e.g. "Funded")
escrow_state() {
  local escrow_id="$1"
  invoke "${ADMIN}" -- get_escrow --escrow_id "${escrow_id}" 2>/dev/null \
    | jq -r 'if type=="object" then (.state // .status) else . end' 2>/dev/null
}

# assert_state <escrow_id> <expected-variant>
assert_state() {
  local escrow_id="$1" expected="$2" actual
  actual="$(escrow_state "${escrow_id}")"
  if [ "${actual}" = "${expected}" ]; then
    ok "escrow ${escrow_id} is in expected state '${expected}'"
  else
    die "escrow ${escrow_id} expected state '${expected}' but found '${actual}'"
  fi
}

# ── escrow helpers ───────────────────────────────────────────────────────────
# The native asset's Stellar Asset Contract id, used as the escrow payment token.
native_token() {
  stellar contract id asset --asset native --network "${NETWORK}"
}

# create_escrow <amount> -> prints the new escrow id.
# Uses the modern 9-argument create_escrow interface.
create_escrow() {
  local amount="$1"
  invoke "${SELLER}" -- create_escrow \
    --seller_or_payees "$(addr "${SELLER}")" \
    --buyer "$(addr "${BUYER}")" \
    --resolver "$(addr "${RESOLVER}")" \
    --token "$(native_token)" \
    --amount "${amount}" \
    --fee_bps 50 \
    --resolver_fee_bps 50 \
    --shipping_window 86400 \
    --notes "e2e" \
    | tr -d '"'
}
