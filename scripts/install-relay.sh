#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="tunely-relay"
RUN_USER="tunely"
RUN_GROUP="tunely"
INSTALL_DIR="/opt/tunely"
CONFIG_DIR="/etc/tunely"

RELAY_BIN_SOURCE=""
GITHUB_REPO="8954sood/tunely"
VERSION="latest"
ARCH="auto"
LISTEN="0.0.0.0:8080"
AUTH=""
REQUEST_TIMEOUT_SECS="60"
TMP_DIR=""

usage() {
  cat <<USAGE
Usage:
  sudo ./install-relay.sh --auth <tunnel=token[,tunnel2=token2]> [options]

Required:
  --auth <value>                 Example: demo=xxx,foo=bar

Options:
  --listen <host:port>           Default: 0.0.0.0:8080
  --request-timeout-secs <sec>   Default: 60
  --binary <path>                relay-server binary path
  --repo <owner/repo>            Default: 8954sood/tunely
  --version <tag|latest>         Default: latest
  --arch <amd64|arm64|auto>      Default: auto
  --install-dir <path>           Default: /opt/tunely
  --config-dir <path>            Default: /etc/tunely
  -h, --help                     Show help
USAGE
}

cleanup() {
  if [[ -n "${TMP_DIR}" && -d "${TMP_DIR}" ]]; then
    rm -rf "${TMP_DIR}"
  fi
}

require_root() {
  if [[ "${EUID}" -ne 0 ]]; then
    echo "[ERROR] Run as root (use sudo)."
    exit 1
  fi
}

detect_arch() {
  local raw
  raw="$(uname -m)"
  case "${raw}" in
    x86_64|amd64)
      echo "amd64"
      ;;
    aarch64|arm64)
      echo "arm64"
      ;;
    *)
      echo "[ERROR] Unsupported architecture: ${raw}" >&2
      exit 1
      ;;
  esac
}

download_file() {
  local url="$1"
  local out="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fL --retry 3 --connect-timeout 10 -o "${out}" "${url}"
  elif command -v wget >/dev/null 2>&1; then
    wget -O "${out}" "${url}"
  else
    echo "[ERROR] Neither curl nor wget is installed"
    exit 1
  fi
}

resolve_binary() {
  if [[ -n "${RELAY_BIN_SOURCE}" ]]; then
    if [[ ! -f "${RELAY_BIN_SOURCE}" ]]; then
      echo "[ERROR] relay-server binary not found: ${RELAY_BIN_SOURCE}"
      exit 1
    fi
    return
  fi

  if [[ "${ARCH}" == "auto" ]]; then
    ARCH="$(detect_arch)"
  fi

  if [[ "${ARCH}" != "amd64" && "${ARCH}" != "arm64" ]]; then
    echo "[ERROR] --arch must be one of: amd64, arm64, auto"
    exit 1
  fi

  local asset="tunely-linux-${ARCH}.tar.gz"
  local url
  if [[ "${VERSION}" == "latest" ]]; then
    url="https://github.com/${GITHUB_REPO}/releases/latest/download/${asset}"
  else
    url="https://github.com/${GITHUB_REPO}/releases/download/${VERSION}/${asset}"
  fi

  TMP_DIR="$(mktemp -d)"
  local archive_path="${TMP_DIR}/${asset}"

  echo "[INFO] Downloading ${url}"
  download_file "${url}" "${archive_path}"

  tar -xzf "${archive_path}" -C "${TMP_DIR}"

  if [[ -f "${TMP_DIR}/tunely-linux-${ARCH}/relay-server" ]]; then
    RELAY_BIN_SOURCE="${TMP_DIR}/tunely-linux-${ARCH}/relay-server"
  else
    RELAY_BIN_SOURCE="$(find "${TMP_DIR}" -type f -name relay-server | head -n 1)"
  fi

  if [[ -z "${RELAY_BIN_SOURCE}" || ! -f "${RELAY_BIN_SOURCE}" ]]; then
    echo "[ERROR] relay-server binary not found in downloaded archive"
    exit 1
  fi
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --auth)
        AUTH="${2:-}"
        shift 2
        ;;
      --listen)
        LISTEN="${2:-}"
        shift 2
        ;;
      --request-timeout-secs)
        REQUEST_TIMEOUT_SECS="${2:-}"
        shift 2
        ;;
      --binary)
        RELAY_BIN_SOURCE="${2:-}"
        shift 2
        ;;
      --repo)
        GITHUB_REPO="${2:-}"
        shift 2
        ;;
      --version)
        VERSION="${2:-}"
        shift 2
        ;;
      --arch)
        ARCH="${2:-}"
        shift 2
        ;;
      --install-dir)
        INSTALL_DIR="${2:-}"
        shift 2
        ;;
      --config-dir)
        CONFIG_DIR="${2:-}"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        echo "[ERROR] Unknown argument: $1"
        usage
        exit 1
        ;;
    esac
  done

  if [[ -z "${AUTH}" ]]; then
    echo "[ERROR] --auth is required"
    usage
    exit 1
  fi

  resolve_binary

  if [[ ! -x "${RELAY_BIN_SOURCE}" ]]; then
    chmod +x "${RELAY_BIN_SOURCE}"
  fi
}

ensure_user() {
  if ! id -u "${RUN_USER}" >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin "${RUN_USER}"
  fi
}

install_files() {
  install -d -m 0755 "${INSTALL_DIR}"
  install -d -m 0750 "${CONFIG_DIR}"

  install -m 0755 "${RELAY_BIN_SOURCE}" "${INSTALL_DIR}/relay-server"

  cat > "${CONFIG_DIR}/relay.env" <<ENV
LISTEN=${LISTEN}
AUTH=${AUTH}
REQUEST_TIMEOUT_SECS=${REQUEST_TIMEOUT_SECS}
ENV

  cat > "/etc/systemd/system/${SERVICE_NAME}.service" <<UNIT
[Unit]
Description=Tunely Relay Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${RUN_USER}
Group=${RUN_GROUP}
EnvironmentFile=${CONFIG_DIR}/relay.env
ExecStart=${INSTALL_DIR}/relay-server --listen \${LISTEN} --auth \${AUTH} --request-timeout-secs \${REQUEST_TIMEOUT_SECS}
Restart=always
RestartSec=2
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=${INSTALL_DIR} ${CONFIG_DIR}

[Install]
WantedBy=multi-user.target
UNIT

  chown -R "${RUN_USER}:${RUN_GROUP}" "${INSTALL_DIR}" "${CONFIG_DIR}"
}

start_service() {
  systemctl daemon-reload
  systemctl enable --now "${SERVICE_NAME}.service"
}

print_summary() {
  local first_tunnel="${AUTH%%=*}"
  first_tunnel="${first_tunnel%%,*}"

  echo
  echo "[OK] relay installed"
  echo "  service : ${SERVICE_NAME}.service"
  echo "  binary  : ${INSTALL_DIR}/relay-server"
  echo "  env     : ${CONFIG_DIR}/relay.env"
  echo
  echo "Check status:"
  echo "  systemctl status ${SERVICE_NAME}"
  echo "  journalctl -u ${SERVICE_NAME} -f"
  echo
  if [[ -n "${first_tunnel}" ]]; then
    echo "Client URL pattern:"
    echo "  http://<relay-ip-or-domain>:${LISTEN##*:}/t/${first_tunnel}/"
  fi
}

main() {
  trap cleanup EXIT
  require_root
  parse_args "$@"
  ensure_user
  install_files
  start_service
  print_summary
}

main "$@"
