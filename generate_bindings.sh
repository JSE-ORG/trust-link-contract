#!/bin/bash
set -e

# Make sure we're in the project root
cd "$(dirname "$0")"

echo "Building the contract..."
cargo build --target wasm32-unknown-unknown --release

echo "Generating TypeScript bindings..."
stellar contract bindings typescript \
    --wasm target/wasm32-unknown-unknown/release/trustlink_escrow.wasm \
    --output-dir bindings/src \
    --overwrite

echo "Bindings generated successfully in bindings/src/"
