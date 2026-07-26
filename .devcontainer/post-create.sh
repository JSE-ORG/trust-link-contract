#!/usr/bin/env bash
#
# Runs once, after the devcontainer is created.
#
#   1. starts PostgreSQL and provisions the indexer database + schema
#   2. warms the Rust build cache (cargo build --lib)
#   3. installs the Node dependencies for bindings/, indexer/ and e2e/
#
# Safe to re-run:  bash .devcontainer/post-create.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PG_USER="${PGUSER:-trustlink}"
PG_PASSWORD="${PGPASSWORD:-trustlink}"
PG_DB="${PGDATABASE:-trustlink}"

log() { printf '\n\033[1;36m==> %s\033[0m\n' "$1"; }

# ---------------------------------------------------------------------------
# 1. PostgreSQL (for the indexer)
# ---------------------------------------------------------------------------
log "Starting PostgreSQL"
if sudo service postgresql start; then
  # Wait for the socket to accept connections.
  for _ in $(seq 1 30); do
    if sudo -u postgres pg_isready -q; then break; fi
    sleep 1
  done

  sudo -u postgres psql -tAc "SELECT 1 FROM pg_roles WHERE rolname='${PG_USER}'" | grep -q 1 \
    || sudo -u postgres psql -c "CREATE ROLE ${PG_USER} LOGIN SUPERUSER PASSWORD '${PG_PASSWORD}';"

  sudo -u postgres psql -tAc "SELECT 1 FROM pg_database WHERE datname='${PG_DB}'" | grep -q 1 \
    || sudo -u postgres createdb -O "${PG_USER}" "${PG_DB}"

  log "Applying indexer/schema.sql"
  PGPASSWORD="${PG_PASSWORD}" psql -h localhost -U "${PG_USER}" -d "${PG_DB}" -q -f indexer/schema.sql
else
  echo "WARNING: PostgreSQL failed to start — the indexer will not run until it does."
fi

# ---------------------------------------------------------------------------
# 2. Rust
# ---------------------------------------------------------------------------
log "Building the contract (cargo build --lib)"
cargo build --lib

# ---------------------------------------------------------------------------
# 3. Node workspaces
# ---------------------------------------------------------------------------
for pkg in bindings indexer e2e; do
  if [ -f "$pkg/package.json" ]; then
    log "npm install in $pkg/"
    (cd "$pkg" && npm install --no-audit --no-fund)
  fi
done

log "Done — try 'make check' (fmt + clippy + test)"
