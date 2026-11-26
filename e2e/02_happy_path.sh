#!/usr/bin/env bash
# e2e/02_happy_path.sh
#
# Happy path: create -> fund -> ship -> deliver -> confirm.
# Asserts the on-chain escrow state after every step. A fresh escrow id is
# created on each run, so the script is safe to re-run.

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

log "seller marks shipped"
invoke "${SELLER}" -- mark_shipped \
  --caller "$(addr "${SELLER}")" \
  --escrow_id "${ESCROW_ID}" \
  --tracking_id "TRACK-${ESCROW_ID}" >/dev/null
assert_state "${ESCROW_ID}" "Shipped"

log "seller records delivery"
invoke "${SELLER}" -- record_delivery \
  --caller "$(addr "${SELLER}")" \
  --escrow_id "${ESCROW_ID}" >/dev/null

log "buyer confirms delivery (releases funds)"
invoke "${BUYER}" -- confirm_delivery \
  --caller "$(addr "${BUYER}")" \
  --escrow_id "${ESCROW_ID}" >/dev/null
assert_state "${ESCROW_ID}" "Completed"

ok "happy path complete for escrow ${ESCROW_ID}"
