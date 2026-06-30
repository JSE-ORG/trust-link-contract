#!/usr/bin/env bash
# e2e/run_all.sh
#
# Runs the full end-to-end suite against the configured network (default:
# testnet) in order. Safe to re-run — setup/deploy is idempotent and each path
# creates a fresh escrow.

set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

bash "${HERE}/01_setup_and_deploy.sh"
bash "${HERE}/02_happy_path.sh"
bash "${HERE}/03_dispute_path.sh"
bash "${HERE}/04_cancel_path.sh"

printf '\n\033[0;32mAll e2e paths passed.\033[0m\n'
