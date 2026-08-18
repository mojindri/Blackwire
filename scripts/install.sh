#!/usr/bin/env bash
set -euo pipefail

REPO="${BLACKWIRE_REPO:-mojindri/Blackwire}"
VERSION="${VERSION:-latest}"
DOWNLOAD_BASE="${BLACKWIRE_DOWNLOAD_BASE:-}"
ACTION="${ACTION:-install}"
PREFIX="${PREFIX:-/usr/local}"
CONFIG_DIR="${CONFIG_DIR:-/etc/blackwire}"
STATE_DIR="${STATE_DIR:-/var/lib/blackwire}"
RUN_DIR="${RUN_DIR:-/run/blackwire}"
SERVICE_USER="${SERVICE_USER:-blackwire}"
SERVICE_GROUP="${SERVICE_GROUP:-blackwire}"
START_SERVICE="${START_SERVICE:-0}"
INSTALL_SYSTEMD="${INSTALL_SYSTEMD:-auto}"
INSTALL_BLACK_UI="${INSTALL_BLACK_UI:-0}"
# Keep the management panel private unless public exposure is explicitly chosen.
# An explicit switch is safer than an interactive prompt because this installer is
# also used by unattended provisioning.
BLACK_UI_EXPOSURE="${BLACK_UI_EXPOSURE:-private}"
BLACK_UI_LISTEN="${BLACK_UI_LISTEN:-}"
BLACK_UI_STATIC_DIR="${BLACK_UI_STATIC_DIR:-/usr/local/share/black-ui/frontend/dist}"
BLACK_UI_DATA_DIR="${BLACK_UI_DATA_DIR:-/var/lib/black-ui}"
RUNTIME_DATABASE_URL_FILE="${RUNTIME_DATABASE_URL_FILE:-}"
UI_DATABASE_URL_FILE="${UI_DATABASE_URL_FILE:-}"
MIGRATOR_DATABASE_URL_FILE="${MIGRATOR_DATABASE_URL_FILE:-}"
RUN_DB_MIGRATIONS="${RUN_DB_MIGRATIONS:-0}"

log() { printf 'blackwire-install: %s\n' "$*"; }
die() { printf 'blackwire-install: ERROR: %s\n' "$*" >&2; exit 1; }
need_cmd() { command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"; }
sudo_cmd() { if [ "$(id -u)" -eq 0 ]; then "$@"; else sudo "$@"; fi; }

configure_black_ui_exposure() {
    case "$BLACK_UI_EXPOSURE" in
        private)
            BLACK_UI_LISTEN="${BLACK_UI_LISTEN:-127.0.0.1:18080}"
            ;;
        public)
            # Bind every interface by default; do not couple an install to one
            # potentially changing public address. A caller may still provide
            # an explicit address through BLACK_UI_LISTEN.
            BLACK_UI_LISTEN="${BLACK_UI_LISTEN:-0.0.0.0:18080}"
            log "Black UI will be publicly reachable at ${BLACK_UI_LISTEN}; protect it with HTTPS and access controls"
            ;;
        *) die "BLACK_UI_EXPOSURE must be private or public" ;;
    esac
}

detect_asset() {
    case "$(uname -s):$(uname -m)" in
        Linux:x86_64|Linux:amd64) echo "blackwire-linux-x86_64.tar.gz" ;;
        Linux:aarch64|Linux:arm64) echo "blackwire-linux-arm64.tar.gz" ;;
        *) die "native installer supports Linux x86_64 and arm64 only" ;;
    esac
}

detect_ui_asset() {
    case "$(uname -m)" in
        x86_64|amd64) echo "black-ui-linux-x86_64.tar.gz" ;;
        aarch64|arm64) echo "black-ui-linux-arm64.tar.gz" ;;
        *) die "unsupported architecture for Black UI" ;;
    esac
}

download_url() {
    local asset="$1"
    if [ -n "$DOWNLOAD_BASE" ]; then echo "${DOWNLOAD_BASE%/}/${asset}"
    elif [ "$VERSION" = "latest" ]; then echo "https://github.com/${REPO}/releases/latest/download/${asset}"
    else echo "https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
    fi
}

download_verified_asset() {
    local asset="$1" destination="$2" url
    url="$(download_url "$asset")"
    curl -fsSL "$url" -o "$destination/$asset"
    curl -fsSL "$url.sha256" -o "$destination/$asset.sha256"
    (cd "$destination" && awk -v asset="$asset" '{ print $1 "  " asset }' "$asset.sha256" | sha256sum -c -)
    tar -xzf "$destination/$asset" -C "$destination"
}

detect_legacy_installation() {
    local found=0 path
    for path in "$CONFIG_DIR/config.json" "$BLACK_UI_DATA_DIR/black-ui.db" "$BLACK_UI_DATA_DIR/black-ui.sqlite"; do
        if [ -e "$path" ]; then log "legacy file detected and left untouched: $path"; found=1; fi
    done
    if [ "$found" = 1 ]; then
        log "legacy JSON/SQLite data is incompatible with this MySQL-only release and is not imported"
    fi
}

reject_legacy_options() {
    [ -z "${CONFIG_PATH:-}" ] || die "CONFIG_PATH is no longer supported; seed or edit MySQL through Black UI"
    [ -z "${CONFIG_URL:-}" ] || die "CONFIG_URL is no longer supported; seed or edit MySQL through Black UI"
    [ -z "${INIT_SERVER:-}" ] || die "INIT_SERVER is no longer supported; use 'blackwire db seed PRESET'"
}

install_credential() {
    local source="$1" destination="$2"
    [ -n "$source" ] || die "missing required protected database URL file"
    [ -f "$source" ] || die "database URL file does not exist: $source"
    sudo_cmd install -m 0600 -o "$SERVICE_USER" -g "$SERVICE_GROUP" "$source" "$destination"
}

prepare_accounts_and_dirs() {
    if ! id "$SERVICE_USER" >/dev/null 2>&1; then
        sudo_cmd useradd --system --home "$STATE_DIR" --shell /usr/sbin/nologin "$SERVICE_USER"
    fi
    if ! getent group "$SERVICE_GROUP" >/dev/null 2>&1; then sudo_cmd groupadd --system "$SERVICE_GROUP"; fi
    sudo_cmd usermod -a -G "$SERVICE_GROUP" "$SERVICE_USER"
    sudo_cmd install -d -m 0750 -o "$SERVICE_USER" -g "$SERVICE_GROUP" "$CONFIG_DIR" "$STATE_DIR" "$RUN_DIR"
}

install_runtime_unit() {
    local unit
    unit="$(mktemp)"
    cat > "$unit" <<UNIT
[Unit]
Description=Blackwire MySQL-backed proxy runtime
After=network-online.target
Wants=network-online.target

[Service]
User=${SERVICE_USER}
Group=${SERVICE_GROUP}
ExecStart=${PREFIX}/bin/blackwire run
LoadCredential=database-url:${CONFIG_DIR}/runtime-database-url
WorkingDirectory=${STATE_DIR}
Restart=on-failure
RestartSec=5s
CapabilityBoundingSet=CAP_NET_BIND_SERVICE
AmbientCapabilities=CAP_NET_BIND_SERVICE
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${STATE_DIR} ${RUN_DIR}
PrivateTmp=true
NoNewPrivileges=true
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
UNIT
    sudo_cmd install -m 0644 "$unit" /etc/systemd/system/blackwire.service
    rm -f "$unit"
}

install_ui_unit() {
    local unit
    unit="$(mktemp)"
    cat > "$unit" <<UNIT
[Unit]
Description=Blackwire database-backed control panel
After=network-online.target
Wants=network-online.target

[Service]
User=${SERVICE_USER}
Group=${SERVICE_GROUP}
ExecStart=${PREFIX}/bin/black-ui
LoadCredential=database-url:${CONFIG_DIR}/ui-database-url
WorkingDirectory=${BLACK_UI_DATA_DIR}
Environment=BLACK_UI_LISTEN=${BLACK_UI_LISTEN}
Environment=BLACK_UI_STATIC_DIR=${BLACK_UI_STATIC_DIR}
Restart=on-failure
RestartSec=5s
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
UNIT
    sudo_cmd install -m 0644 "$unit" /etc/systemd/system/black-ui.service
    rm -f "$unit"
}

install_ui() {
    [ "$INSTALL_BLACK_UI" = 1 ] || return 0
    [ -n "$UI_DATABASE_URL_FILE" ] || die "INSTALL_BLACK_UI=1 requires UI_DATABASE_URL_FILE for a separate UI account"
    local work asset binary dist
    work="$(mktemp -d)"; asset="$(detect_ui_asset)"
    download_verified_asset "$asset" "$work"
    binary="$(find "$work" -type f -name black-ui -perm -111 | head -n 1)"
    [ -n "$binary" ] || die "black-ui binary not found in release asset"
    sudo_cmd install -m 0755 "$binary" "$PREFIX/bin/black-ui"
    dist="$(find "$work" -type d -path '*/frontend/dist' | head -n 1)"
    [ -n "$dist" ] || die "black-ui frontend bundle not found in release asset"
    sudo_cmd install -d -m 0755 "$BLACK_UI_STATIC_DIR"
    sudo_cmd cp -a "$dist"/. "$BLACK_UI_STATIC_DIR"/
    sudo_cmd install -d -m 0750 -o "$SERVICE_USER" -g "$SERVICE_GROUP" "$BLACK_UI_DATA_DIR"
    install_credential "$UI_DATABASE_URL_FILE" "$CONFIG_DIR/ui-database-url"
    install_ui_unit
    rm -rf "$work"
}

uninstall() {
    sudo_cmd systemctl disable --now blackwire black-ui >/dev/null 2>&1 || true
    sudo_cmd rm -f /etc/systemd/system/blackwire.service /etc/systemd/system/black-ui.service
    sudo_cmd rm -f "$PREFIX/bin/blackwire" "$PREFIX/bin/black-ui"
    sudo_cmd systemctl daemon-reload >/dev/null 2>&1 || true
    log "application binaries removed; MySQL data, credentials, and legacy files were retained"
}

main() {
    [ "$ACTION" != uninstall ] || { uninstall; exit 0; }
    case "$ACTION" in install|upgrade) ;; *) die "ACTION must be install, upgrade, or uninstall" ;; esac
    for command in curl tar install sha256sum find sed; do need_cmd "$command"; done
    if [ "$(id -u)" -ne 0 ]; then need_cmd sudo; fi
    reject_legacy_options
    configure_black_ui_exposure
    detect_legacy_installation
    [ -n "$RUNTIME_DATABASE_URL_FILE" ] || die "RUNTIME_DATABASE_URL_FILE is required; this installer never installs MySQL"
    prepare_accounts_and_dirs

    local work asset binary
    work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
    asset="$(detect_asset)"; download_verified_asset "$asset" "$work"
    binary="$(find "$work" -type f -name blackwire -perm -111 | head -n 1)"
    [ -n "$binary" ] || die "blackwire binary not found in release asset"
    sudo_cmd install -d -m 0755 "$PREFIX/bin"
    sudo_cmd install -m 0755 "$binary" "$PREFIX/bin/blackwire"
    install_credential "$RUNTIME_DATABASE_URL_FILE" "$CONFIG_DIR/runtime-database-url"

    if [ "$RUN_DB_MIGRATIONS" = 1 ]; then
        [ -n "$MIGRATOR_DATABASE_URL_FILE" ] || die "RUN_DB_MIGRATIONS=1 requires MIGRATOR_DATABASE_URL_FILE"
        install_credential "$MIGRATOR_DATABASE_URL_FILE" "$CONFIG_DIR/migrator-database-url"
        sudo_cmd env BLACKWIRE_DATABASE_URL_FILE="$CONFIG_DIR/migrator-database-url" "$PREFIX/bin/blackwire" db migrate
    fi
    sudo_cmd env BLACKWIRE_DATABASE_URL_FILE="$CONFIG_DIR/runtime-database-url" "$PREFIX/bin/blackwire" db status

    case "$INSTALL_SYSTEMD" in 0|false|no) ;; auto|1|true|yes) install_runtime_unit ;; *) die "invalid INSTALL_SYSTEMD" ;; esac
    install_ui
    sudo_cmd systemctl daemon-reload >/dev/null 2>&1 || true
    if [ "$START_SERVICE" = 1 ]; then
        sudo_cmd systemctl enable --now blackwire
        [ "$INSTALL_BLACK_UI" != 1 ] || sudo_cmd systemctl enable --now black-ui
    fi
    log "installed MySQL-only Blackwire; configure it with Black UI or named db seed presets"
    # `work` is local to main, while EXIT runs after main returns.  Clean it
    # here so strict mode does not dereference an unset local in the trap.
    rm -rf "$work"
    trap - EXIT
}

main "$@"
