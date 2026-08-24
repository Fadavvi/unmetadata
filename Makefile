VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*= *"//;s/".*//')
DIST    := dist

BINARY          := unmetadata
CROSS           := cross

# Linux musl targets (static, via cross)
LINUX_X64_TRIPLE   := x86_64-unknown-linux-musl
LINUX_ARM64_TRIPLE := aarch64-unknown-linux-musl

# Windows targets (via cross)
WIN_X64_TRIPLE    := x86_64-pc-windows-gnu
WIN_ARM64_TRIPLE  := aarch64-pc-windows-gnullvm

# FreeBSD targets (via cross)
FBSD_X64_TRIPLE   := x86_64-unknown-freebsd
FBSD_ARM64_TRIPLE := aarch64-unknown-freebsd

# macOS targets (native cargo, macOS host only)
MACOS_X64_TRIPLE   := x86_64-apple-darwin
MACOS_ARM64_TRIPLE := aarch64-apple-darwin

.PHONY: all linux windows freebsd macos release clean

## Build Linux + Windows + FreeBSD targets (default)
all: linux windows freebsd

## Build both Linux musl targets (x86_64 + aarch64) via cross
linux:
	$(CROSS) build --release --target $(LINUX_X64_TRIPLE)
	$(CROSS) build --release --target $(LINUX_ARM64_TRIPLE)

## Build both Windows targets (x86_64 + aarch64) via cross
windows:
	$(CROSS) build --release --target $(WIN_X64_TRIPLE)
	$(CROSS) build --release --target $(WIN_ARM64_TRIPLE)

## Build both FreeBSD targets (x86_64 + aarch64) via cross
freebsd:
	$(CROSS) build --release --target $(FBSD_X64_TRIPLE)
	$(CROSS) build --release --target $(FBSD_ARM64_TRIPLE)

## Build macOS targets and create universal fat binary.
## NOTE: Must be run on a macOS host; cargo does not support darwin cross-compilation on Linux.
macos:
	@if [ "$$(uname -s)" != "Darwin" ]; then \
		echo "ERROR: macOS targets must be built on a macOS host (uname -s is not Darwin)."; \
		exit 1; \
	fi
	cargo build --release --target $(MACOS_X64_TRIPLE)
	cargo build --release --target $(MACOS_ARM64_TRIPLE)
	mkdir -p $(DIST)
	lipo -create \
		-output $(DIST)/$(BINARY)-$(VERSION)-macos-universal \
		target/$(MACOS_X64_TRIPLE)/release/$(BINARY) \
		target/$(MACOS_ARM64_TRIPLE)/release/$(BINARY)
	@echo "Created $(DIST)/$(BINARY)-$(VERSION)-macos-universal"
	@file $(DIST)/$(BINARY)-$(VERSION)-macos-universal

## Build all non-macOS targets and copy renamed binaries to dist/
release: linux windows freebsd
	mkdir -p $(DIST)
	cp target/$(LINUX_X64_TRIPLE)/release/$(BINARY)   $(DIST)/$(BINARY)-$(VERSION)-linux-x86_64
	cp target/$(LINUX_ARM64_TRIPLE)/release/$(BINARY)  $(DIST)/$(BINARY)-$(VERSION)-linux-aarch64
	cp target/$(WIN_X64_TRIPLE)/release/$(BINARY).exe  $(DIST)/$(BINARY)-$(VERSION)-windows-x86_64.exe
	cp target/$(WIN_ARM64_TRIPLE)/release/$(BINARY).exe $(DIST)/$(BINARY)-$(VERSION)-windows-aarch64.exe
	cp target/$(FBSD_X64_TRIPLE)/release/$(BINARY)     $(DIST)/$(BINARY)-$(VERSION)-freebsd-x86_64
	cp target/$(FBSD_ARM64_TRIPLE)/release/$(BINARY)   $(DIST)/$(BINARY)-$(VERSION)-freebsd-aarch64
	@echo ""
	@echo "Release artifacts in $(DIST)/:"
	@ls -lh $(DIST)/

## Remove dist/ and target/
clean:
	rm -rf $(DIST) target
