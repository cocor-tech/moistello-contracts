#!/bin/bash
set -e

echo "=== Generating JS/TS Bindings ==="

WASM_DIR="target/wasm32v1-none/release"
OUTPUT_DIR="bindings"

mkdir -p "$OUTPUT_DIR"

# Check if wasm files exist, if not, build them
if [ ! -f "$WASM_DIR/circle.wasm" ]; then
    echo "WASM files not found. Building..."
    cargo build --target wasm32v1-none --release
fi

for wasm_path in "$WASM_DIR"/*.wasm; do
    [ -e "$wasm_path" ] || continue
    name=$(basename "$wasm_path" .wasm)
    echo "Generating bindings for $name..."
    stellar contract bindings --wasm "$wasm_path" --output-dir "$OUTPUT_DIR/$name" --overwrite
done

echo "=== Bindings generated successfully ==="
