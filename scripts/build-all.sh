#!/usr/bin/env bash
# build-all.sh — Build unmetadata for all supported targets.
#
# Requirements:
#   - cargo + rustup (always)
#   - cross (for Linux, Windows, FreeBSD targets): cargo install cross
#   - Docker running (used by cross)
#   - macOS host + lipo (for macOS targets only)
#
# Run from the repository root:
#   bash scripts/build-all.sh
set -euo pipefail

BINARY="unmetadata"
DIST="dist"

# Read version from Cargo.toml without requiring cargo-metadata
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*= *"//;s/".*//')

TARGETS_CROSS=(
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
    x86_64-pc-windows-gnu
    aarch64-pc-windows-gnullvm
    x86_64-unknown-freebsd
    aarch64-unknown-freebsd
)

TARGETS_MACOS=(
    x86_64-apple-darwin
    aarch64-apple-darwin
)

# ── Prerequisite checks ───────────────────────────────────────────────────────

check_command() {
    if ! command -v "$1" &>/dev/null; then
        echo "ERROR: '$1' not found in PATH. $2"
        exit 1
    fi
}

check_command cargo  "Install Rust: https://rustup.rs"
check_command cross  "Install cross: cargo install cross  (and ensure Docker is running)"

mkdir -p "$DIST"

# ── Helper: map triple → dist filename ───────────────────────────────────────

dist_name() {
    local triple="$1"
    case "$triple" in
        x86_64-unknown-linux-musl)   echo "${BINARY}-${VERSION}-linux-x86_64" ;;
        aarch64-unknown-linux-musl)  echo "${BINARY}-${VERSION}-linux-aarch64" ;;
        x86_64-pc-windows-gnu)       echo "${BINARY}-${VERSION}-windows-x86_64.exe" ;;
        aarch64-pc-windows-gnullvm)  echo "${BINARY}-${VERSION}-windows-aarch64.exe" ;;
        x86_64-unknown-freebsd)      echo "${BINARY}-${VERSION}-freebsd-x86_64" ;;
        aarch64-unknown-freebsd)     echo "${BINARY}-${VERSION}-freebsd-aarch64" ;;
        x86_64-apple-darwin)         echo "${BINARY}-${VERSION}-macos-x86_64" ;;
        aarch64-apple-darwin)        echo "${BINARY}-${VERSION}-macos-aarch64" ;;
        *)                           echo "${BINARY}-${VERSION}-${triple}" ;;
    esac
}

# Returns the binary name inside target/<triple>/release/ (with .exe for Windows)
release_bin() {
    local triple="$1"
    case "$triple" in
        *windows*) echo "${BINARY}.exe" ;;
        *)         echo "${BINARY}" ;;
    esac
}

# ── Build via cross ───────────────────────────────────────────────────────────

FAILED_TARGETS=()

echo "==> Building ${#TARGETS_CROSS[@]} targets via cross (version ${VERSION})"
for triple in "${TARGETS_CROSS[@]}"; do
    echo ""
    echo "--- Building ${triple} ---"
    if cross build --release --target "$triple"; then
        bin=$(release_bin "$triple")
        src="target/${triple}/release/${bin}"
        dst="${DIST}/$(dist_name "$triple")"
        cp "$src" "$dst"
        echo "    Copied -> ${dst}"
    else
        echo "WARN: Build FAILED for ${triple}, skipping."
        FAILED_TARGETS+=("$triple")
    fi
done

# ── Build macOS targets (Darwin host only) ────────────────────────────────────

if [[ "$(uname -s)" == "Darwin" ]]; then
    echo ""
    echo "==> macOS host detected; building darwin targets via cargo"

    MACOS_BUILT=()
    for triple in "${TARGETS_MACOS[@]}"; do
        echo ""
        echo "--- Building ${triple} ---"
        if cargo build --release --target "$triple"; then
            src="target/${triple}/release/${BINARY}"
            dst="${DIST}/$(dist_name "$triple")"
            cp "$src" "$dst"
            echo "    Copied -> ${dst}"
            MACOS_BUILT+=("$triple")
        else
            echo "WARN: Build FAILED for ${triple}, skipping."
            FAILED_TARGETS+=("$triple")
        fi
    done

    # Create universal fat binary if both darwin targets succeeded
    X64_BIN="target/x86_64-apple-darwin/release/${BINARY}"
    ARM_BIN="target/aarch64-apple-darwin/release/${BINARY}"
    if [[ -f "$X64_BIN" && -f "$ARM_BIN" ]]; then
        echo ""
        echo "--- Creating macOS universal binary ---"
        UNIVERSAL="${DIST}/${BINARY}-${VERSION}-macos-universal"
        lipo -create -output "$UNIVERSAL" "$X64_BIN" "$ARM_BIN"
        echo "Created ${UNIVERSAL}"
        file "$UNIVERSAL"
    else
        echo "WARN: Skipping universal binary — one or both darwin builds are missing."
    fi
else
    echo ""
    echo "NOTE: macOS targets (${TARGETS_MACOS[*]}) require a macOS host."
    echo "      Run this script on macOS, or use a GitHub Actions macOS runner."
fi

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo "==> Artifacts in ${DIST}/:"
ls -lh "$DIST/"

if [[ ${#FAILED_TARGETS[@]} -gt 0 ]]; then
    echo ""
    echo "WARN: The following targets failed to build:"
    for t in "${FAILED_TARGETS[@]}"; do
        echo "  - ${t}"
    done
    exit 1
fi

echo ""
echo "All targets built successfully."
