#!/usr/bin/env bash
# Installs unmetadata as a systemd user service that starts on login.
# Runs as the invoking user - no root required.
# The service starts 'unmetadata --serve' on http://localhost:3301.

set -euo pipefail

BINARY_NAME="unmetadata"
GITHUB_REPO="fadavvi/unmetadata"          # change to your fork if needed
INSTALL_DIR="${HOME}/.local/bin"
SERVICE_DIR="${HOME}/.config/systemd/user"
SERVICE_FILE="${SERVICE_DIR}/unmetadata.service"

# temp-file cleanup

TMPFILE=""
cleanup() { [ -n "${TMPFILE}" ] && rm -f "${TMPFILE}"; }
trap cleanup EXIT

# locate binary 

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Works whether the script lives in the project root or in scripts/
if [[ -d "${SCRIPT_DIR}/dist" ]]; then
  DIST_DIR="${SCRIPT_DIR}/dist"
else
  DIST_DIR="${SCRIPT_DIR}/../dist"
fi
ARCH="$(uname -m)"

case "${ARCH}" in
  x86_64)  GLOB_PATTERN="${BINARY_NAME}-*-linux-x86_64" ;;
  aarch64) GLOB_PATTERN="${BINARY_NAME}-*-linux-aarch64" ;;
  *)
    echo "error: unsupported architecture: ${ARCH}" >&2
    exit 1
    ;;
esac

# Find the newest matching binary in dist/ (handles any version string)
DIST_BINARY="$(find "${DIST_DIR}" -maxdepth 1 -name "${GLOB_PATTERN}" | sort -V | tail -1)"

if [[ -n "${DIST_BINARY}" && -f "${DIST_BINARY}" ]]; then
  SOURCE="${DIST_BINARY}"
elif command -v "${BINARY_NAME}" &>/dev/null; then
  SOURCE="$(command -v "${BINARY_NAME}")"
else
  # fetch from GitHub releases
  echo "no local binary found - fetching latest release from github.com/${GITHUB_REPO}"
  ASSET_URL="$(curl -fsSL "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" \
    | grep '"browser_download_url"' \
    | grep "${GLOB_PATTERN}" \
    | sed 's|.*"browser_download_url": *"\([^"]*\)".*|\1|' \
    | head -1)"
  if [[ -z "${ASSET_URL}" ]]; then
    echo "error: could not find a release asset matching '${GLOB_PATTERN}'" >&2
    echo "       check https://github.com/${GITHUB_REPO}/releases or build from source." >&2
    exit 1
  fi
  TMPFILE="$(mktemp)"
  echo "downloading: ${ASSET_URL}"
  curl -fsSL -o "${TMPFILE}" "${ASSET_URL}"
  chmod 755 "${TMPFILE}"
  SOURCE="${TMPFILE}"
fi

#update existing installation

if systemctl --user is-enabled unmetadata.service &>/dev/null; then
  echo "existing installation detected - updating binary and restarting service"
  mkdir -p "${INSTALL_DIR}"
  install -m 755 "${SOURCE}" "${INSTALL_DIR}/${BINARY_NAME}"
  echo "updated:  ${INSTALL_DIR}/${BINARY_NAME}"
  systemctl --user restart unmetadata.service
  echo "service restarted"
  echo ""
  echo "unmetadata web UI is running at http://localhost:3301"
  echo "systemctl --user status unmetadata"
  exit 0
fi

# fresh install: copy binary 

mkdir -p "${INSTALL_DIR}"
install -m 755 "${SOURCE}" "${INSTALL_DIR}/${BINARY_NAME}"
echo "installed: ${INSTALL_DIR}/${BINARY_NAME}"

# write service unit

mkdir -p "${SERVICE_DIR}"
cat > "${SERVICE_FILE}" <<EOF
[Unit]
Description=unmetadata - metadata stripper web UI
Documentation=https://github.com/fadavvi/unmetadata
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

# enable and start

systemctl --user daemon-reload
systemctl --user enable --now unmetadata.service
echo "service enabled and started"

# linger (optional but recommended)           
# Without linger the service stops when the user's last session ends.
# With linger it keeps running even after logout (useful on servers/headless).

read -r -p "Enable linger so the service survives logout? [y/N] " LINGER
if [[ "${LINGER,,}" == "y" ]]; then
  loginctl enable-linger "${USER}"
  echo "linger enabled for ${USER}"
fi

# done

echo ""
echo "unmetadata is running at http://localhost:3301"
echo ""
echo "Useful commands:"
echo "  systemctl --user status unmetadata"
echo "  systemctl --user stop   unmetadata"
echo "  systemctl --user start  unmetadata"
echo "  systemctl --user disable unmetadata   # remove from login autostart"
