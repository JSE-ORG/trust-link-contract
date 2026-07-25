#!/bin/bash
set -e

# Make sure we're in the project root
cd "$(dirname "$0")"

echo "Building the contract..."
cargo build --target wasm32v1-none --release

echo "Generating TypeScript bindings..."
stellar contract bindings typescript \
    --wasm target/wasm32v1-none/release/trustlink_escrow.wasm \
    --output-dir bindings/src \
    --overwrite

echo "Bindings generated successfully in bindings/src/"
