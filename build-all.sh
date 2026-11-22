#!/usr/bin/env bash

set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"

BINARY="unmetadata"
DIST="dist"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*= *"//;s/".*//')

mkdir -p "$DIST"

FAILED=()

# --- helpers

ok()   { echo "    ok  → dist/$(basename "$1")"; }
fail() { echo "    FAIL: $1"; FAILED+=("$1"); }

need() {
    command -v "$1" &>/dev/null || { echo "ERROR: '$1' not found. $2"; exit 1; }
}

# --- prerequisite checks 

need cargo       "Install Rust: https://rustup.rs"
need cross       "cargo install cross --version 0.2.5  (Docker must be running)"
need cargo-zigbuild "cargo install cargo-zigbuild  (zig must also be in PATH)"
need zig         "Install zig: https://ziglang.org/download  or via system package manager"
need x86_64-w64-mingw32-gcc "Install mingw-w64 (e.g. pacman -S mingw-w64-gcc)"
need llvm-lipo   "Install llvm (e.g. pacman -S llvm)"

# --- Linux

echo ""
echo "==> Linux x86_64  [cross - Docker]"
if cross build --release --target x86_64-unknown-linux-musl 2>&1 | tail -2; then
    DST="$DIST/${BINARY}-${VERSION}-linux-x86_64"
    install -m755 target/x86_64-unknown-linux-musl/release/${BINARY} "$DST"
    ok "$DST"
else
    fail "x86_64-unknown-linux-musl"
fi

echo ""
echo "==> Linux aarch64  [cargo zigbuild - avoids cross Docker glibc mismatch]"
if cargo zigbuild --release --target aarch64-unknown-linux-musl 2>&1 | tail -2; then
    DST="$DIST/${BINARY}-${VERSION}-linux-aarch64"
    install -m755 target/aarch64-unknown-linux-musl/release/${BINARY} "$DST"
    ok "$DST"
else
    fail "aarch64-unknown-linux-musl"
fi

# --- FreeBSD

echo ""
echo "==> FreeBSD x86_64  [cross - Docker]"
if cross build --release --target x86_64-unknown-freebsd 2>&1 | tail -2; then
    DST="$DIST/${BINARY}-${VERSION}-freebsd-x86_64"
    install -m755 target/x86_64-unknown-freebsd/release/${BINARY} "$DST"
    ok "$DST"
else
    fail "x86_64-unknown-freebsd"
fi

# FreeBSD aarch64 skipped: tier-3, no prebuilt std on stable toolchain.

# --- Windows

echo ""
echo "==> Windows x86_64  [cargo + native MinGW - cross Docker has incompatible glibc]"
if CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
       cargo build --release --target x86_64-pc-windows-gnu 2>&1 | tail -2; then
    DST="$DIST/${BINARY}-${VERSION}-windows-x86_64.exe"
    install -m755 target/x86_64-pc-windows-gnu/release/${BINARY}.exe "$DST"
    ok "$DST"
else
    fail "x86_64-pc-windows-gnu"
fi

echo ""
echo "==> Windows aarch64  [cargo zigbuild - no cross Docker image for this triple]"
if cargo zigbuild --release --target aarch64-pc-windows-gnullvm 2>&1 | tail -2; then
    DST="$DIST/${BINARY}-${VERSION}-windows-aarch64.exe"
    install -m755 target/aarch64-pc-windows-gnullvm/release/${BINARY}.exe "$DST"
    ok "$DST"
else
    fail "aarch64-pc-windows-gnullvm"
fi

# --- macOS
# cargo zigbuild is used for all Darwin targets: zig bundles macOS libc headers,
# avoiding the CoreFoundation framework dependency that broke earlier lopdf builds.

echo ""
echo "==> macOS x86_64  [cargo zigbuild]"
if cargo zigbuild --release --target x86_64-apple-darwin 2>&1 | tail -2; then
    DST="$DIST/${BINARY}-${VERSION}-macos-x86_64"
    install -m755 target/x86_64-apple-darwin/release/${BINARY} "$DST"
    ok "$DST"
else
    fail "x86_64-apple-darwin"
fi

echo ""
echo "==> macOS aarch64  [cargo zigbuild]"
if cargo zigbuild --release --target aarch64-apple-darwin 2>&1 | tail -2; then
    DST="$DIST/${BINARY}-${VERSION}-macos-aarch64"
    install -m755 target/aarch64-apple-darwin/release/${BINARY} "$DST"
    ok "$DST"
else
    fail "aarch64-apple-darwin"
fi

echo ""
echo "==> macOS universal  [llvm-lipo]"
X64="target/x86_64-apple-darwin/release/${BINARY}"
ARM="target/aarch64-apple-darwin/release/${BINARY}"
if [[ -f "$X64" && -f "$ARM" ]]; then
    DST="$DIST/${BINARY}-${VERSION}-macos-universal"
    llvm-lipo -create "$X64" "$ARM" -output "$DST"
    chmod 755 "$DST"
    ok "$DST"
else
    echo "    SKIP: one or both darwin slices are missing (see failures above)"
    FAILED+=("macos-universal")
fi

# --- summary

echo ""
echo "==> Artifacts in ${DIST}/:"
ls -lh "$DIST/" | grep -v '^total'

if [[ ${#FAILED[@]} -gt 0 ]]; then
    echo ""
    echo "FAILED targets:"
    for t in "${FAILED[@]}"; do echo "  - ${t}"; done
    exit 1
fi

echo ""
echo "All targets built successfully."
