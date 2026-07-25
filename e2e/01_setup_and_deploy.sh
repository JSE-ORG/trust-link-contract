#!/usr/bin/env bash
# e2e/01_setup_and_deploy.sh
#
# Idempotent setup + deploy:
#   1. Ensure all test identities exist and are funded.
#   2. Build the contract wasm (skipped if already built).
#   3. Deploy + initialize once; reuse the existing deployment on re-runs.
#
# Re-running this script is safe: it never re-initializes an already-initialized
# contract and never re-deploys a working one.

source "$(dirname "$0")/lib.sh"

require_cmd stellar
require_cmd jq

log "network: ${NETWORK}"

# ── 1. identities ────────────────────────────────────────────────────────────
for id in "${ADMIN}" "${FEE_COLLECTOR}" "${SELLER}" "${BUYER}" "${RESOLVER}"; do
  ensure_identity "${id}"
done

# ── 2. build wasm (idempotent) ───────────────────────────────────────────────
if [ -f "${WASM_PATH}" ]; then
  log "wasm already built at ${WASM_PATH}"
else
  log "building contract wasm"
  ( cd "${REPO_ROOT}" && cargo build --target wasm32v1-none --release -p trustlink-escrow )
fi
[ -f "${WASM_PATH}" ] || die "wasm not found after build: ${WASM_PATH}"

# ── 3. deploy + initialize (idempotent) ──────────────────────────────────────
deploy_and_init() {
  log "deploying contract"
  local id
  id="$(stellar contract deploy \
    --wasm "${WASM_PATH}" \
    --source "${ADMIN}" \
    --network "${NETWORK}")"
  echo "${id}" > "${CONTRACT_ID_FILE}"
  ok "deployed contract ${id}"

  log "initializing contract"
  invoke "${ADMIN}" -- initialize \
    --admin "$(addr "${ADMIN}")" \
    --fee_collector "$(addr "${FEE_COLLECTOR}")" \
    --arbitration_fee_bps 100 >/dev/null
  ok "initialized"
}

if [ -f "${CONTRACT_ID_FILE}" ] && \
   stellar contract invoke --id "$(cat "${CONTRACT_ID_FILE}")" --source "${ADMIN}" \
     --network "${NETWORK}" -- get_version >/dev/null 2>&1; then
  ok "reusing existing deployment $(cat "${CONTRACT_ID_FILE}") (already initialized)"
else
  deploy_and_init
fi

ok "setup complete — contract id: $(contract_id)"
