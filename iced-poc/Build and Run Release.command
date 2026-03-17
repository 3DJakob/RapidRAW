#!/bin/zsh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

cd "$SCRIPT_DIR"

echo "Building RapidRAW Iced POC in release mode..."
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

echo ""
echo "Launching release build..."
"$SCRIPT_DIR/target/release/rapidraw-iced-poc"
