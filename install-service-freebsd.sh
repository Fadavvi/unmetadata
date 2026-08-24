#!/bin/sh
# Installs unmetadata as a FreeBSD rc.d service that starts after login is ready.
# Must be run as root (needed to write to /usr/local/bin and /usr/local/etc/rc.d).
#
# The service starts 'unmetadata --serve' on http://localhost:3301.
# It runs as the user specified by $unmetadata_user in rc.conf (default: nobody).
# To run as a specific user, add to /etc/rc.conf:
#   unmetadata_user="myuser"

set -e

BINARY_NAME="unmetadata"
INSTALL_BIN="/usr/local/bin/${BINARY_NAME}"
RC_SCRIPT="/usr/local/etc/rc.d/${BINARY_NAME}"
RC_CONF="/etc/rc.conf"
PID_DIR="/var/run"
LOG_DIR="/var/log"

# ── must be root ──────────────────────────────────────────────────────────────

if [ "$(id -u)" -ne 0 ]; then
  echo "error: this script must be run as root" >&2
  echo "       su -c 'sh ${0}'" >&2
  exit 1
fi

# ── locate binary ─────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DIST_DIR="${SCRIPT_DIR}/../dist"
VERSION="0.1.0"
ARCH="$(uname -m)"

case "${ARCH}" in
  amd64|x86_64) DIST_BINARY="${DIST_DIR}/${BINARY_NAME}-${VERSION}-freebsd-x86_64" ;;
  *)
    echo "error: unsupported architecture: ${ARCH}" >&2
    echo "       only freebsd-x86_64 binaries are distributed." >&2
    exit 1
    ;;
esac

if [ -f "${DIST_BINARY}" ]; then
  SOURCE="${DIST_BINARY}"
elif command -v "${BINARY_NAME}" >/dev/null 2>&1; then
  SOURCE="$(command -v "${BINARY_NAME}")"
else
  echo "error: cannot find ${BINARY_NAME} binary." >&2
  echo "       expected: ${DIST_BINARY}" >&2
  exit 1
fi

# ── install binary ────────────────────────────────────────────────────────────

install -m 755 "${SOURCE}" "${INSTALL_BIN}"
echo "installed: ${INSTALL_BIN}"

# ── write rc.d script ─────────────────────────────────────────────────────────

cat > "${RC_SCRIPT}" <<'RCEOF'
#!/bin/sh
#
# PROVIDE: unmetadata
# REQUIRE: LOGIN
# KEYWORD: shutdown
#
# Add these lines to /etc/rc.conf to enable unmetadata:
#   unmetadata_enable="YES"
#   unmetadata_user="nobody"   # user to run as (optional, default: nobody)

. /etc/rc.subr

name="unmetadata"
rcvar="unmetadata_enable"

load_rc_config "${name}"

: "${unmetadata_enable:=NO}"
: "${unmetadata_user:=nobody}"
: "${unmetadata_logfile:=/var/log/unmetadata.log}"

command="/usr/local/bin/unmetadata"
command_args="--serve"
pidfile="/var/run/${name}.pid"

start_precmd="${name}_prestart"
start_cmd="${name}_start"
stop_cmd="${name}_stop"
status_cmd="${name}_status"

unmetadata_prestart()
{
    if ! id "${unmetadata_user}" >/dev/null 2>&1; then
        err 1 "user '${unmetadata_user}' does not exist"
    fi
}

unmetadata_start()
{
    echo "Starting ${name}."
    /usr/sbin/daemon -u "${unmetadata_user}" \
        -p "${pidfile}" \
        -o "${unmetadata_logfile}" \
        -r \
        "${command}" ${command_args}
}

unmetadata_stop()
{
    if [ -f "${pidfile}" ]; then
        pid="$(cat "${pidfile}")"
        echo "Stopping ${name} (pid ${pid})."
        kill "${pid}" && rm -f "${pidfile}"
    else
        echo "${name} is not running."
    fi
}

unmetadata_status()
{
    if [ -f "${pidfile}" ] && kill -0 "$(cat "${pidfile}")" 2>/dev/null; then
        echo "${name} is running as pid $(cat "${pidfile}")."
    else
        echo "${name} is not running."
        return 1
    fi
}

run_rc_command "$1"
RCEOF

chmod 755 "${RC_SCRIPT}"
echo "rc.d script written: ${RC_SCRIPT}"

# ── enable in rc.conf ─────────────────────────────────────────────────────────

if grep -q "^unmetadata_enable=" "${RC_CONF}" 2>/dev/null; then
  sed -i '' 's/^unmetadata_enable=.*/unmetadata_enable="YES"/' "${RC_CONF}"
  echo "updated ${RC_CONF}: unmetadata_enable=\"YES\""
else
  echo 'unmetadata_enable="YES"' >> "${RC_CONF}"
  echo "appended to ${RC_CONF}: unmetadata_enable=\"YES\""
fi

# ── start now ────────────────────────────────────────────────────────────────

service unmetadata start
echo ""
echo "unmetadata is running at http://localhost:3301"
echo "logs: ${LOG_DIR}/unmetadata.log"
echo ""
echo "Useful commands:"
echo "  service unmetadata status"
echo "  service unmetadata stop"
echo "  service unmetadata start"
echo "  service unmetadata restart"
echo ""
echo "To run as a specific user, add to /etc/rc.conf:"
echo '  unmetadata_user="myuser"'
echo "then: service unmetadata restart"
