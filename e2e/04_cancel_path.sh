#!/usr/bin/env bash
# e2e/04_cancel_path.sh
#
# Cancel path: create -> cancel (before funding).
# A Pending escrow can be cancelled by the seller; asserts it reaches Canceled.

source "$(dirname "$0")/lib.sh"
require_cmd stellar
require_cmd jq

AMOUNT="${ESCROW_AMOUNT:-10000000}"

log "creating escrow (amount=${AMOUNT})"
ESCROW_ID="$(create_escrow "${AMOUNT}")"
ok "created escrow ${ESCROW_ID}"
assert_state "${ESCROW_ID}" "Pending"

log "seller cancels the unfunded escrow"
invoke "${SELLER}" -- cancel_escrow \
  --caller "$(addr "${SELLER}")" \
  --escrow_id "${ESCROW_ID}" >/dev/null
assert_state "${ESCROW_ID}" "Canceled"

ok "cancel path complete for escrow ${ESCROW_ID}"
