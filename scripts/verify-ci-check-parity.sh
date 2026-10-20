#!/usr/bin/env bash
# CI congruence guard: .github/workflows/ci.yml  <->  Makefile check target
#
# Fails (exit 1) when the CI check job does not run `make check` (or its
# individual components). This prevents drift where CI passes locally but
# fails in CI or vice versa.
#
# Usage:  bash scripts/verify-ci-check-parity.sh

set -euo pipefail

# The canonical check target from Makefile
CANONICAL_CHECK_TARGETS=("fmt-check" "clippy" "test" "check-error-codes")

# Extract all "make run" commands from the CI check job
CI_FILE=".github/workflows/ci.yml"
CI_MAKE_COMMANDS=$(grep -A 50 "jobs:" "$CI_FILE" | grep -A 40 "check:" | grep "run: make" | sed 's/.*run: make //' | sed 's/\s*#.*//')

# Convert to array
mapfile -t CI_TARGETS <<< "$CI_MAKE_COMMANDS"

# Check if CI runs the canonical check target directly
RUNS_CANONICAL_CHECK=false
for ci_target in "${CI_TARGETS[@]}"; do
  if [[ "$ci_target" == "check" ]]; then
    RUNS_CANONICAL_CHECK=true
    break
  fi
done

if [ "$RUNS_CANONICAL_CHECK" = true ]; then
  echo "OK — CI check job runs 'make check' directly."
  exit 0
fi

# If not, check if all canonical targets are present in CI
MISSING_IN_CI=()
for target in "${CANONICAL_CHECK_TARGETS[@]}"; do
  found=false
  for ci_target in "${CI_TARGETS[@]}"; do
    if [[ "$ci_target" == "$target" ]]; then
      found=true
      break
    fi
  done
  if [ "$found" = false ]; then
    MISSING_IN_CI+=("$target")
  fi
done

# Report errors
if [ ${#MISSING_IN_CI[@]} -gt 0 ]; then
  echo "Error: CI check job does not run 'make check' and is missing canonical targets"
  echo ""
  echo "Targets in 'make check' but missing in CI:"
  for target in "${MISSING_IN_CI[@]}"; do
    echo "  - $target"
  done
  echo ""
  echo "Fix: Run 'make check' in CI, or add missing targets to CI."
  exit 1
fi

echo "OK — CI check job runs all components of 'make check'."
