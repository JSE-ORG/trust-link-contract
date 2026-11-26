#!/usr/bin/env bash
# scripts/rotate_admin.sh
#
# Helper script to rotate the contract admin via the two-step timelock process.
#
# Usage: ./scripts/rotate_admin.sh --contract <id> --current-admin <identity> --new-admin <address> [--execute]

set -euo pipefail

CONTRACT_ID=""
CURRENT_ADMIN=""
NEW_ADMIN=""
EXECUTE=0
NETWORK="testnet"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --contract)      CONTRACT_ID="$2"; shift 2 ;;
        --current-admin) CURRENT_ADMIN="$2"; shift 2 ;;
        --new-admin)     NEW_ADMIN="$2"; shift 2 ;;
        --network)       NETWORK="$2"; shift 2 ;;
        --execute)       EXECUTE=1; shift ;;
        *)               echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [ -z "$CONTRACT_ID" ] || [ -z "$CURRENT_ADMIN" ] || [ -z "$NEW_ADMIN" ]; then
    echo "Usage: $0 --contract <id> --current-admin <identity> --new-admin <address> [--execute]"
    exit 1
fi

if [ "$EXECUTE" -eq 1 ]; then
    echo "Executing admin rotation..."
    stellar contract invoke \
        --id "$CONTRACT_ID" \
        --source "$CURRENT_ADMIN" \
        --network "$NETWORK" \
        -- execute_set_admin --caller "$(stellar keys address "$CURRENT_ADMIN")"
    echo "Admin rotation executed successfully."
else
    echo "Queuing admin rotation..."
    stellar contract invoke \
        --id "$CONTRACT_ID" \
        --source "$CURRENT_ADMIN" \
        --network "$NETWORK" \
        -- queue_set_admin --caller "$(stellar keys address "$CURRENT_ADMIN")" --new_admin "$NEW_ADMIN"
    echo "Admin rotation queued. Wait 24 hours then run with --execute."
fi
