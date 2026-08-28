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
WASM_PATH="${REPO_ROOT}/target/wasm32v1-none/release/trustlink-escrow.wasm"

# Identity aliases used across the suite.
ADMIN="${ADMIN_IDENTITY:-tl_admin}"
FEE_COLLECTOR="${FEE_COLLECTOR_IDENTITY:-tl_fee_collector}"
SELLER="${SELLER_IDENTITY:-tl_seller}"
BUYER="${BUYER_IDENTITY:-tl_buyer}"
RESOLVER="${RESOLVER_IDENTITY:-tl_resolver}"

mkdir -p "${STATE_DIR}"

# ── logging ────────────────────────────────────────────────────────────────
# Args: $* — message to print. All four write to stdout except `warn`/`die`,
# which write to stderr so failures surface even when stdout is piped/captured
# (e.g. `ESCROW_ID="$(create_escrow "$AMOUNT")"`).
#
#   log "creating escrow"     # [e2e] creating escrow
#   ok "created escrow 3"     # [ ok] created escrow 3
#   warn "friendbot rate-limited"   # [warn] friendbot rate-limited (stderr)
#   die "escrow not found"    # [fail] escrow not found (stderr); exits 1
log()  { printf '\033[0;34m[e2e]\033[0m %s\n' "$*"; }
ok()   { printf '\033[0;32m[ ok]\033[0m %s\n' "$*"; }
warn() { printf '\033[0;33m[warn]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[0;31m[fail]\033[0m %s\n' "$*" >&2; exit 1; }

# require_cmd <name>
# Verifies a CLI tool is on PATH, or dies with a clear message. Call this at
# the top of every script for each external tool it shells out to (`stellar`,
# `jq`) so a missing dependency fails fast with a readable error instead of a
# confusing mid-script command-not-found.
#
#   require_cmd stellar
#   require_cmd jq
require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

# ── identities (idempotent) ──────────────────────────────────────────────────
# ensure_identity <name>
# Creates the named `stellar keys` identity if it doesn't already exist, then
# funds it via Friendbot. Safe to call every run: an existing keypair is
# reused as-is, and re-funding an already-funded account is a harmless no-op
# (funding failures — e.g. Friendbot rate limits — only `warn`, never `die`,
# since the account may already hold enough balance to proceed).
#
#   ensure_identity "${SELLER}"
# Typically called in a loop over all suite identities, as in
# 01_setup_and_deploy.sh:
#   for id in "${ADMIN}" "${FEE_COLLECTOR}" "${SELLER}" "${BUYER}" "${RESOLVER}"; do
#     ensure_identity "${id}"
#   done
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

# addr <identity-name> -> prints the identity's public address (G...).
# Thin wrapper over `stellar keys address` used to resolve one of the suite's
# named identities (${SELLER}, ${BUYER}, ...) to an address argument for a
# contract call.
#
#   invoke "${BUYER}" -- fund_escrow --escrow_id "${ESCROW_ID}" --buyer "$(addr "${BUYER}")"
addr() { stellar keys address "$1"; }

# ── contract invocation ──────────────────────────────────────────────────────
# contract_id -> prints the deployed contract id for the current ${NETWORK}.
# Reads the id cached by 01_setup_and_deploy.sh at
# `.state/<network>.contract_id`; dies with a pointer to that script if it
# hasn't been run yet. Used internally by `invoke`, but also useful directly
# when a script needs the raw id (e.g. for `stellar contract invoke` outside
# the `invoke` wrapper).
contract_id() {
  [ -f "${CONTRACT_ID_FILE}" ] || die "no deployed contract; run 01_setup_and_deploy.sh first"
  cat "${CONTRACT_ID_FILE}"
}

# invoke <source-identity> -- <function> [args...]
# Thin wrapper over `stellar contract invoke` that fills in the contract id
# (via `contract_id`) and ${NETWORK} so call sites only specify who is
# signing and which contract function/args to call. Prints whatever the
# invoked function returns (JSON-encoded), on stdout.
#
#   invoke "${SELLER}" -- cancel_escrow \
#     --caller "$(addr "${SELLER}")" \
#     --escrow_id "${ESCROW_ID}"
#
#   # Capture a return value:
#   STATE="$(invoke "${ADMIN}" -- get_escrow --escrow_id "${ESCROW_ID}" | jq -r .state)"
invoke() {
  local source="$1"; shift
  stellar contract invoke \
    --id "$(contract_id)" \
    --source "${source}" \
    --network "${NETWORK}" \
    "$@"
}

# ── assertions ───────────────────────────────────────────────────────────────
# escrow_state <escrow_id> -> prints the EscrowState variant (e.g. "Funded").
# Calls `get_escrow` and extracts `.state` (falling back to `.status` for
# older ABI shapes) from the JSON response. Prints nothing (empty string) if
# the escrow doesn't exist or the response can't be parsed — callers that
# need a hard failure on a missing escrow should use `assert_state` instead.
#
#   STATE="$(escrow_state "${ESCROW_ID}")"
#   [ "${STATE}" = "Disputed" ] || die "unexpected state ${STATE}"
escrow_state() {
  local escrow_id="$1"
  invoke "${ADMIN}" -- get_escrow --escrow_id "${escrow_id}" 2>/dev/null \
    | jq -r 'if type=="object" then (.state // .status) else . end' 2>/dev/null
}

# assert_state <escrow_id> <expected-variant>
# Fetches the escrow's current state via `escrow_state` and either prints an
# `ok` line (state matches) or `die`s with a diagnostic message (state
# differs). This is the standard checkpoint used after every lifecycle
# transition in the numbered path scripts, so a divergence from the expected
# on-chain state fails the script immediately with a clear cause instead of
# surfacing later as a confusing downstream error.
#
#   assert_state "${ESCROW_ID}" "Pending"
#   invoke "${BUYER}" -- fund_escrow --escrow_id "${ESCROW_ID}" --buyer "$(addr "${BUYER}")" >/dev/null
#   assert_state "${ESCROW_ID}" "Funded"
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
# native_token -> prints the Stellar Asset Contract id for the native asset
# (XLM) on ${NETWORK}. Used as the `--token` argument to `create_escrow` so
# every path script pays in the native asset without hardcoding a
# network-specific contract id.
#
#   TOKEN="$(native_token)"
native_token() {
  stellar contract id asset --asset native --network "${NETWORK}"
}

# create_escrow <amount> -> prints the new escrow id (a bare integer, quotes
# stripped from the JSON response).
# Convenience wrapper that calls the contract's modern 9-argument
# `create_escrow` entrypoint (`seller_or_payees, buyer, resolver, token,
# amount, fee_bps, resolver_fee_bps, shipping_window, notes`) using the
# suite's standard identities (${SELLER}/${BUYER}/${RESOLVER}) and the native
# token, with fixed fee/window values suitable for e2e assertions (50bps
# seller fee, 50bps resolver fee, 24h shipping window). If the deployed
# contract's ABI differs from this signature, update this function to match
# — see the "Notes / current limitations" section of README.md.
#
#   ESCROW_ID="$(create_escrow 10000000)"
#   assert_state "${ESCROW_ID}" "Pending"
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
