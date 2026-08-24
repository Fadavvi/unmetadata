# unmetadata

Strip all metadata from images, documents, audio, video, and more - in one command.

Removes EXIF, IPTC, XMP, GPS coordinates, ICC profiles, embedded thumbnails, author fields, creation dates, camera info, comments, and any other identifying or privacy-sensitive metadata. Files are processed in parallel; originals are never corrupted (atomic write-then-rename).

---

## Supported Formats

| Category  | Formats | What gets stripped |
|-----------|---------|-------------------|
| Images    | JPEG, PNG, WebP, GIF, TIFF, HEIC/HEIF, AVIF, PSD | EXIF, IPTC, XMP, GPS, ICC profile, embedded thumbnails, comments |
| Documents | PDF, DOCX/XLSX/PPTX, ODT/ODS/ODP | Author, creator, company, revision history, custom properties |
| Audio     | MP3, FLAC, OGG (Vorbis/Opus), M4A | ID3v1, ID3v2, Vorbis comments, cover art |
| Video     | MP4, MKV | Title, encoder info, date/time, GPS, tags |
| Other     | SVG, ZIP | `<metadata>` / `<rdf:RDF>` elements, archive comment |

BMP files contain no metadata fields and are passed through unchanged.

---

## Installation

### Pre-built binaries

Download the appropriate binary from the `dist/` directory (or a release page) and place it on your `PATH`.

| Binary | Platform |
|--------|----------|
| `unmetadata-0.1.0-linux-x86_64` | Linux x86-64 (static musl, runs on any distro) |
| `unmetadata-0.1.0-linux-aarch64` | Linux ARM64 (static musl) |
| `unmetadata-0.1.0-windows-x86_64.exe` | Windows x86-64 |
| `unmetadata-0.1.0-windows-aarch64.exe` | Windows ARM64 |
| `unmetadata-0.1.0-freebsd-x86_64` | FreeBSD x86-64 |
| `unmetadata-0.1.0-macos-universal` | macOS universal (Intel + Apple Silicon) |
| `unmetadata-0.1.0-macos-x86_64` | macOS Intel only |
| `unmetadata-0.1.0-macos-aarch64` | macOS Apple Silicon only |

Linux binaries are statically linked (musl libc) - no runtime dependencies. On macOS, copy the universal binary:

```sh
sudo install -m 755 dist/unmetadata-0.1.0-macos-universal /usr/local/bin/unmetadata
```

On Linux:

```sh
sudo install -m 755 dist/unmetadata-0.1.0-linux-x86_64 /usr/local/bin/unmetadata
```

### Build from source

Requires Rust 1.75 or later.

```sh
git clone https://github.com/user/unmetadata
cd unmetadata
cargo build --release
# binary at: target/release/unmetadata
```

---

## Usage

```
unmetadata [OPTIONS] <PATH>...
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<PATH>...` | Files or directories to process. Directories are processed non-recursively by default. |

### Options

| Flag | Description |
|------|-------------|
| `-o, --output <PATH>` | Write cleaned copy to `PATH` (single-file input only) |
| `--output-dir <DIR>` | Write cleaned copies into `DIR`, preserving filenames |
| `-r, --recursive` | Recurse into subdirectories |
| `-n, --dry-run` | Show what would be stripped; no files are modified |
| `-v, --verbose` | Print each stripped item per file |
| `-q, --quiet` | Suppress all output except errors |
| `-j, --jobs <N>` | Worker threads (default: number of CPU cores) |
| `--serve` | Start the web UI on `http://localhost:3301` |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

### Examples

```sh
# Strip metadata in-place from a single file
unmetadata photo.jpg

# Preview what would be removed without modifying the file
unmetadata -n -v photo.jpg

# Write a clean copy to a different path
unmetadata -o photo-clean.jpg photo.jpg

# Strip all supported files in a folder (non-recursive)
unmetadata ./photos/

# Strip all supported files recursively
unmetadata -r ./documents/

# Write cleaned copies into a separate folder
unmetadata --output-dir ./clean/ ./photos/

# Recurse with clean copies
unmetadata -r --output-dir ./clean/ ./mixed-files/

# Show each stripped item per file
unmetadata -v ./photos/

# Use 4 worker threads
unmetadata -j 4 -r ./archive/
```

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | All files processed successfully |
| `1` | One or more files had errors (details printed to stderr) |
| `2` | Fatal CLI error (invalid arguments) |

---

## Web UI

Start the local web server:

```sh
unmetadata --serve
```

Open `http://localhost:3301` in your browser.

- Drag and drop one file, multiple files, or a whole folder
- Files are cleaned and downloaded automatically
- Multiple files or a folder → delivered as a single ZIP archive
- Dark theme; no data leaves your machine

The server only binds to `127.0.0.1` - it is not accessible from the network.

---

## Run as a Service

### Linux (systemd user service)

Starts automatically when you log in. No root required.

```sh
sh scripts/install-service-linux.sh
```

The script:
- Installs the binary to `~/.local/bin/unmetadata`
- Creates `~/.config/systemd/user/unmetadata.service`
- Enables and starts the service immediately
- Optionally enables `loginctl linger` so the service persists after logout

Manage the service:

```sh
systemctl --user status unmetadata
systemctl --user stop   unmetadata
systemctl --user start  unmetadata
systemctl --user disable unmetadata   # remove from login autostart
journalctl --user -u unmetadata -f    # follow logs
```

### FreeBSD (rc.d service)

Requires root. Starts after `LOGIN` is ready.

```sh
sudo sh scripts/install-service-freebsd.sh
```

The script:
- Installs the binary to `/usr/local/bin/unmetadata`
- Writes `/usr/local/etc/rc.d/unmetadata`
- Appends `unmetadata_enable="YES"` to `/etc/rc.conf`
- Starts the service immediately

By default the service runs as `nobody`. To run as a specific user, add to `/etc/rc.conf`:

```sh
unmetadata_user="myuser"
```

then restart:

```sh
service unmetadata restart
```

Manage the service:

```sh
service unmetadata status
service unmetadata stop
service unmetadata start
tail -f /var/log/unmetadata.log
```

---

## How It Works

File types are detected by **magic bytes**, not file extensions. Processing is:

1. Read file into memory
2. Detect format from leading bytes
3. Dispatch to a format-specific handler that rebuilds the file without metadata
4. Write result atomically: write to a `.tmp` sibling, then `rename` onto the original (or the specified output path)

Parallel processing uses all available CPU cores by default (overridable with `-j`).

Unknown or unsupported file types are **skipped with a warning** - they are never modified.

### In-place safety

The atomic rename strategy means a crash mid-write leaves the original intact. The only scenario where data loss is possible is if the filesystem runs out of space between the write and the rename, which leaves the `.tmp` file behind and the original untouched.

---

## Building for All Targets

The project cross-compiles to 8 targets from a Linux host.

```sh
sh scripts/build-all.sh
```

Requirements:

| Tool | Used for |
|------|----------|
| `cross` (≥ 0.2.5) | Linux x86-64 musl, FreeBSD x86-64 |
| `cargo zigbuild` (≥ 0.23) | Linux ARM64 musl, macOS x86-64/ARM64, Windows ARM64 |
| `x86_64-w64-mingw32-gcc` | Windows x86-64 |
| `llvm-lipo` | macOS universal fat binary |
| Docker | Required by `cross` |

Install tools:

```sh
cargo install cross --version 0.2.5
cargo install cargo-zigbuild
rustup target add \
  x86_64-unknown-linux-musl \
  aarch64-unknown-linux-musl \
  x86_64-unknown-freebsd \
  x86_64-pc-windows-gnu \
  aarch64-pc-windows-gnullvm \
  x86_64-apple-darwin \
  aarch64-apple-darwin
```

Output binaries land in `dist/`.

> **FreeBSD ARM64** (`aarch64-unknown-freebsd`) is not built - it is a Rust tier-3 target with no prebuilt standard library available on the stable toolchain.

---

## Privacy Guarantee

`unmetadata` processes everything locally. It makes no network requests and has no telemetry. The `--serve` web UI binds only to `127.0.0.1` and cannot be reached from outside the machine.
