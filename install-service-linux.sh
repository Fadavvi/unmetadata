#!/usr/bin/env bash
# Installs unmetadata as a systemd user service that starts on login.
# Runs as the invoking user — no root required.
# The service starts 'unmetadata --serve' on http://localhost:3301.

set -euo pipefail

BINARY_NAME="unmetadata"
INSTALL_DIR="${HOME}/.local/bin"
SERVICE_DIR="${HOME}/.config/systemd/user"
SERVICE_FILE="${SERVICE_DIR}/unmetadata.service"

# ── locate binary ─────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="${SCRIPT_DIR}/../dist"
VERSION="0.1.0"
ARCH="$(uname -m)"

case "${ARCH}" in
  x86_64)  DIST_BINARY="${DIST_DIR}/${BINARY_NAME}-${VERSION}-linux-x86_64" ;;
  aarch64) DIST_BINARY="${DIST_DIR}/${BINARY_NAME}-${VERSION}-linux-aarch64" ;;
  *)
    echo "error: unsupported architecture: ${ARCH}" >&2
    exit 1
    ;;
esac

if [[ -f "${DIST_BINARY}" ]]; then
  SOURCE="${DIST_BINARY}"
elif command -v "${BINARY_NAME}" &>/dev/null; then
  SOURCE="$(command -v "${BINARY_NAME}")"
else
  echo "error: cannot find ${BINARY_NAME} binary." >&2
  echo "       expected: ${DIST_BINARY}" >&2
  echo "       or add it to PATH before running this script." >&2
  exit 1
fi

# ── install binary ────────────────────────────────────────────────────────────

mkdir -p "${INSTALL_DIR}"
install -m 755 "${SOURCE}" "${INSTALL_DIR}/${BINARY_NAME}"
echo "installed: ${INSTALL_DIR}/${BINARY_NAME}"

# ── write service unit ────────────────────────────────────────────────────────

mkdir -p "${SERVICE_DIR}"
cat > "${SERVICE_FILE}" <<EOF
[Unit]
Description=unmetadata — metadata stripper web UI
Documentation=https://github.com/user/unmetadata
After=default.target

[Service]
ExecStart=%h/.local/bin/unmetadata --serve
Restart=on-failure
RestartSec=5

# Harden the service
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only

[Install]
WantedBy=default.target
EOF

echo "service unit written: ${SERVICE_FILE}"

# ── enable and start ──────────────────────────────────────────────────────────

systemctl --user daemon-reload
systemctl --user enable --now unmetadata.service
echo "service enabled and started"

# ── linger (optional but recommended) ────────────────────────────────────────
# Without linger the service stops when the user's last session ends.
# With linger it keeps running even after logout (useful on servers/headless).

read -r -p "Enable linger so the service survives logout? [y/N] " LINGER
if [[ "${LINGER,,}" == "y" ]]; then
  loginctl enable-linger "${USER}"
  echo "linger enabled for ${USER}"
fi

# ── done ──────────────────────────────────────────────────────────────────────

echo ""
echo "unmetadata is running at http://localhost:3301"
echo ""
echo "Useful commands:"
echo "  systemctl --user status unmetadata"
echo "  systemctl --user stop   unmetadata"
echo "  systemctl --user start  unmetadata"
echo "  systemctl --user disable unmetadata   # remove from login autostart"
