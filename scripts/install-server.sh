#!/usr/bin/env bash
set -euo pipefail

# One-click Linux installer for the latest standalone AI Switch release.
# Override AI_SWITCH_VERSION (for example, v0.8.4) to install a specific tag.
REPOSITORY="${AI_SWITCH_REPOSITORY:-ijry/ai-switch}"
VERSION="${AI_SWITCH_VERSION:-}"
INSTALL_DIR="/opt/ai-switch"
ENV_FILE="/etc/ai-switch/server.env"
SERVICE_NAME="ai-switch-server.service"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}"
PORT="${AI_SWITCH_PORT:-19527}"
HOST="${AI_SWITCH_HOST:-127.0.0.1}"
ALLOW_INSECURE_HTTP="${AI_SWITCH_ALLOW_INSECURE_HTTP:-1}"

say() { printf '[ai-switch] %s\n' "$*"; }
die() { printf '[ai-switch] error: %s\n' "$*" >&2; exit 1; }

if [[ "$(uname -s)" != "Linux" ]]; then
  die "This installer only supports Linux."
fi
command -v curl >/dev/null 2>&1 || die "curl is required."
command -v unzip >/dev/null 2>&1 || die "unzip is required."
command -v systemctl >/dev/null 2>&1 || die "systemd/systemctl is required."

[[ "$PORT" =~ ^[0-9]+$ ]] || die "AI_SWITCH_PORT must be a number between 1 and 65535."
(( PORT >= 1 && PORT <= 65535 )) || die "AI_SWITCH_PORT must be a number between 1 and 65535."

if [[ "$(id -u)" -eq 0 ]]; then
  AS_ROOT=()
else
  command -v sudo >/dev/null 2>&1 || die "Run as root or install sudo."
  AS_ROOT=(sudo)
fi
run_root() { "${AS_ROOT[@]}" "$@"; }

case "$(uname -m)" in
  x86_64|amd64) ARCH="x86_64" ;;
  *) die "Unsupported Linux architecture: $(uname -m)" ;;
esac

if [[ -z "$VERSION" ]]; then
  release_json="$(curl -fsSL "https://api.github.com/repos/${REPOSITORY}/releases/latest")" || die "Could not query the latest GitHub release."
  VERSION="$(printf '%s\n' "$release_json" | sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  [[ -n "$VERSION" ]] || die "The latest GitHub release did not contain a tag."
fi
[[ "$VERSION" == v* ]] || VERSION="v${VERSION}"

ARCHIVE="ai-switch-server_${VERSION}_linux-${ARCH}.zip"
DOWNLOAD_URL="https://github.com/${REPOSITORY}/releases/download/${VERSION}/${ARCHIVE}"
TEMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "$TEMP_DIR"; }
trap cleanup EXIT
say "Downloading ${REPOSITORY}@${VERSION} for Linux ${ARCH}"
curl -fL --retry 3 --retry-delay 1 -o "${TEMP_DIR}/${ARCHIVE}" "$DOWNLOAD_URL"
unzip -q "${TEMP_DIR}/${ARCHIVE}" -d "$TEMP_DIR/package"

[[ -f "$TEMP_DIR/package/ai-switch-server" ]] || die "The release archive has no ai-switch-server binary."
[[ -f "$TEMP_DIR/package/web/index.html" ]] || die "The release archive has no web/index.html."
[[ -f "$TEMP_DIR/package/ai-switch-tsnet" ]] || die "The release archive has no ai-switch-tsnet sidecar."

if ldd "$TEMP_DIR/package/ai-switch-server" 2>/dev/null | grep -q "not found"; then
  say "Installing missing WebKitGTK runtime dependencies"
  if command -v apt-get >/dev/null 2>&1; then
    run_root env DEBIAN_FRONTEND=noninteractive apt-get update
    run_root env DEBIAN_FRONTEND=noninteractive apt-get install -y libwebkit2gtk-4.1-0
  else
    die "The server binary needs WebKitGTK 4.1 runtime libraries; install them with your distribution's package manager and retry."
  fi
  if ldd "$TEMP_DIR/package/ai-switch-server" 2>/dev/null | grep -q "not found"; then
    die "The server binary still has missing shared libraries after installing WebKitGTK."
  fi
fi

# Keep the existing token and environment on repeat installs. The server.env
# format is deliberately simple KEY=value lines; do not execute it as shell.
existing_token=""
if run_root test -r "$ENV_FILE"; then
  existing_token="$(run_root sed -n 's/^AI_SWITCH_TOKEN=//p' "$ENV_FILE" | head -n 1 || true)"
fi
if [[ -z "$existing_token" ]]; then
  if command -v openssl >/dev/null 2>&1; then
    existing_token="$(openssl rand -hex 32)"
  else
    existing_token="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')"
  fi
fi

run_root systemctl stop ai-switch-server.service >/dev/null 2>&1 || true

if ! getent passwd ai-switch >/dev/null 2>&1; then
  run_root useradd --system --home-dir /var/lib/ai-switch --create-home --shell /usr/sbin/nologin ai-switch
fi
run_root install -d -o ai-switch -g ai-switch -m 0755 "$INSTALL_DIR"
run_root install -d -o ai-switch -g ai-switch -m 0750 /var/lib/ai-switch
run_root cp "$TEMP_DIR/package/ai-switch-server" "$INSTALL_DIR/ai-switch-server"
run_root cp "$TEMP_DIR/package/ai-switch-tsnet" "$INSTALL_DIR/ai-switch-tsnet"
run_root install -d -o ai-switch -g ai-switch -m 0755 "$INSTALL_DIR/web"
run_root find "$INSTALL_DIR/web" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
run_root cp -R "$TEMP_DIR/package/web/." "$INSTALL_DIR/web/"
run_root chown -R ai-switch:ai-switch "$INSTALL_DIR"
run_root chmod 0755 "$INSTALL_DIR/ai-switch-server" "$INSTALL_DIR/ai-switch-tsnet"

ENV_TMP="$TEMP_DIR/server.env"
if ! run_root test -r "$ENV_FILE"; then
  printf 'AI_SWITCH_HOST=%s\nAI_SWITCH_PORT=%s\nAI_SWITCH_ALLOW_INSECURE_HTTP=%s\nAI_SWITCH_TOKEN=%s\nAI_SWITCH_STATIC_DIR=%s\nAI_SWITCH_TSNET_PATH=%s\n' \
    "$HOST" "$PORT" "$ALLOW_INSECURE_HTTP" "$existing_token" "$INSTALL_DIR/web" "$INSTALL_DIR/ai-switch-tsnet" > "$ENV_TMP"
  run_root install -D -o root -g root -m 0600 "$ENV_TMP" "$ENV_FILE"
else
  say "Keeping existing environment in ${ENV_FILE}"
fi
# The release carries this exact systemd unit. Keep a fallback so an older
# archive can still be installed safely; both paths retain the same defaults.
if [[ -f "$TEMP_DIR/package/$SERVICE_NAME" ]]; then
  run_root install -o root -g root -m 0644 "$TEMP_DIR/package/$SERVICE_NAME" "$SERVICE_FILE"
else
  SERVICE_TMP="$TEMP_DIR/$SERVICE_NAME"
  cat > "$SERVICE_TMP" <<'UNIT'
[Unit]
Description=AI Switch standalone server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ai-switch
Group=ai-switch
WorkingDirectory=/opt/ai-switch
Environment=HOME=/var/lib/ai-switch
EnvironmentFile=/etc/ai-switch/server.env
ExecStart=/opt/ai-switch/ai-switch-server
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
UNIT
  run_root install -o root -g root -m 0644 "$SERVICE_TMP" "$SERVICE_FILE"
fi
run_root systemctl daemon-reload
run_root systemctl enable --now ai-switch-server.service
if ! run_root systemctl is-active --quiet ai-switch-server.service; then
  run_root systemctl --no-pager --full status ai-switch-server.service
  die "The ai-switch-server service did not start."
fi

DISPLAY_HOST="$HOST"
[[ "$DISPLAY_HOST" == "0.0.0.0" ]] && DISPLAY_HOST="127.0.0.1"
if [[ "$DISPLAY_HOST" == *:* ]]; then
  PANEL_URL="http://[${DISPLAY_HOST}]:${PORT}"
else
  PANEL_URL="http://${DISPLAY_HOST}:${PORT}"
fi

say "Installed to ${INSTALL_DIR}."
say "Panel URL: ${PANEL_URL}"
say "Service status:"
run_root systemctl --no-pager --full status ai-switch-server.service
say "Access token: sudo cat ${ENV_FILE}"
if [[ "$ALLOW_INSECURE_HTTP" == "1" ]]; then
  say "Plaintext HTTP is enabled for this installer deployment; protect non-loopback access with an HTTPS reverse proxy."
fi
