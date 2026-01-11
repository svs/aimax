#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "Building aimax..."
cargo build --release

echo ""
echo "Build complete!"
echo "  TUI binary: target/release/aimax ($(du -h target/release/aimax | cut -f1))"
echo ""
echo "Run with: ./target/release/aimax"
