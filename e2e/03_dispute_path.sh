#!/usr/bin/env bash
# e2e/03_dispute_path.sh
#
# Dispute path: create -> fund -> raise dispute -> resolve.
# Asserts the escrow reaches Disputed, then a terminal resolved state.

source "$(dirname "$0")/lib.sh"
require_cmd stellar
require_cmd jq

AMOUNT="${ESCROW_AMOUNT:-10000000}"

log "creating escrow (amount=${AMOUNT})"
ESCROW_ID="$(create_escrow "${AMOUNT}")"
ok "created escrow ${ESCROW_ID}"
assert_state "${ESCROW_ID}" "Pending"

log "buyer funds escrow"
invoke "${BUYER}" -- fund_escrow --escrow_id "${ESCROW_ID}" --buyer "$(addr "${BUYER}")" >/dev/null
assert_state "${ESCROW_ID}" "Funded"

log "buyer raises a dispute"
invoke "${BUYER}" -- raise_dispute \
  --caller "$(addr "${BUYER}")" \
  --escrow_id "${ESCROW_ID}" >/dev/null
assert_state "${ESCROW_ID}" "Disputed"

# Resolve in favour of the buyer (refund). The resolver decides the outcome;
# adjust --release_to_seller to exercise the seller-favoured branch.
log "resolver resolves the dispute (refund to buyer)"
invoke "${RESOLVER}" -- resolve_dispute \
  --caller "$(addr "${RESOLVER}")" \
  --escrow_id "${ESCROW_ID}" \
  --release_to_seller false >/dev/null

STATE="$(escrow_state "${ESCROW_ID}")"
case "${STATE}" in
  Refunded|Completed)
    ok "dispute resolved to terminal state '${STATE}' for escrow ${ESCROW_ID}" ;;
  *)
    die "escrow ${ESCROW_ID} expected a resolved terminal state but found '${STATE}'" ;;
esac
