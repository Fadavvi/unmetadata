#!/usr/bin/env bash
# make-universal-macos.sh — Combine x86_64 and aarch64 darwin binaries into a
# universal (fat) binary using lipo.
#
# Prerequisites:
#   - Must run on a macOS host (lipo is a macOS tool)
#   - Both darwin release builds must already exist:
#       target/x86_64-apple-darwin/release/unmetadata
#       target/aarch64-apple-darwin/release/unmetadata
#
# Usage (from repo root):
#   cargo build --release --target x86_64-apple-darwin
#   cargo build --release --target aarch64-apple-darwin
#   bash scripts/make-universal-macos.sh
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "ERROR: This script must be run on a macOS host (lipo is not available on Linux/FreeBSD)."
    exit 1
fi

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*= *"//;s/".*//')

X64="target/x86_64-apple-darwin/release/unmetadata"
ARM="target/aarch64-apple-darwin/release/unmetadata"
OUT="dist/unmetadata-${VERSION}-macos-universal"

if [[ ! -f "$X64" ]]; then
    echo "ERROR: x86_64 binary not found at ${X64}"
    echo "       Run: cargo build --release --target x86_64-apple-darwin"
    exit 1
fi

if [[ ! -f "$ARM" ]]; then
    echo "ERROR: aarch64 binary not found at ${ARM}"
    echo "       Run: cargo build --release --target aarch64-apple-darwin"
    exit 1
fi

mkdir -p dist
lipo -create -output "$OUT" "$X64" "$ARM"

echo "Created ${OUT}"
file "$OUT"
