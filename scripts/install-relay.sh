#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="tunely-relay"
RUN_USER="tunely"
RUN_GROUP="tunely"
INSTALL_DIR="/opt/tunely"
CONFIG_DIR="/etc/tunely"
CONFIG_FILE_NAME="relay.yaml"

RELAY_BIN_SOURCE=""
TUNELY_BIN_SOURCE=""
GITHUB_REPO="8954sood/tunely"
VERSION="latest"
ARCH="auto"
SYSTEMD_MODE="auto"
SYSTEMD_ENABLED="false"
TMP_DIR=""

usage() {
  cat <<USAGE
Usage:
  sudo ./install-relay.sh [options]

Options:
  --binary <path>                relay-server binary path
  --tunely-binary <path>         tunely binary path
  --repo <owner/repo>            Default: 8954sood/tunely
  --version <tag|latest>         Default: latest
  --arch <amd64|arm64|auto>      Default: auto
  --install-dir <path>           Default: /opt/tunely
  --config-dir <path>            Default: /etc/tunely
  --no-systemd                   Skip systemd service setup/start
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

has_systemd() {
  command -v systemctl >/dev/null 2>&1 \
    && [[ -d /run/systemd/system ]] \
    && systemctl show-environment >/dev/null 2>&1
}

resolve_systemd_mode() {
  if [[ "${SYSTEMD_MODE}" == "disabled" ]]; then
    SYSTEMD_ENABLED="false"
    return
  fi

  if has_systemd; then
    SYSTEMD_ENABLED="true"
  else
    SYSTEMD_ENABLED="false"
    echo "[INFO] systemd is not available; service setup will be skipped"
    echo "[INFO] use --no-systemd to suppress this notice"
  fi
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

  if [[ -z "${TUNELY_BIN_SOURCE}" ]]; then
    if [[ -f "${TMP_DIR}/tunely-linux-${ARCH}/tunely" ]]; then
      TUNELY_BIN_SOURCE="${TMP_DIR}/tunely-linux-${ARCH}/tunely"
    else
      TUNELY_BIN_SOURCE="$(find "${TMP_DIR}" -type f -name tunely | head -n 1)"
    fi
  fi
}

resolve_tunely_binary() {
  if [[ -n "${TUNELY_BIN_SOURCE}" ]]; then
    if [[ ! -f "${TUNELY_BIN_SOURCE}" ]]; then
      echo "[ERROR] tunely binary not found: ${TUNELY_BIN_SOURCE}"
      exit 1
    fi
    return
  fi

  local relay_dir
  relay_dir="$(dirname "${RELAY_BIN_SOURCE}")"
  if [[ -f "${relay_dir}/tunely" ]]; then
    TUNELY_BIN_SOURCE="${relay_dir}/tunely"
    return
  fi

  if [[ -f "${INSTALL_DIR}/tunely" ]]; then
    TUNELY_BIN_SOURCE="${INSTALL_DIR}/tunely"
    return
  fi

  echo "[ERROR] tunely binary not found (expected in release archive)."
  echo "[ERROR] Pass --tunely-binary <path> explicitly if needed."
  exit 1
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --binary)
        RELAY_BIN_SOURCE="${2:-}"
        shift 2
        ;;
      --tunely-binary)
        TUNELY_BIN_SOURCE="${2:-}"
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
      --no-systemd)
        SYSTEMD_MODE="disabled"
        shift
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

  resolve_binary
  resolve_tunely_binary

  if [[ ! -x "${RELAY_BIN_SOURCE}" ]]; then
    chmod +x "${RELAY_BIN_SOURCE}"
  fi
  if [[ ! -x "${TUNELY_BIN_SOURCE}" ]]; then
    chmod +x "${TUNELY_BIN_SOURCE}"
  fi
}

ensure_user() {
  if ! id -u "${RUN_USER}" >/dev/null 2>&1; then
    useradd --system --no-create-home --shell /usr/sbin/nologin "${RUN_USER}"
  fi
}

ensure_config_files() {
  local config_file="${CONFIG_DIR}/${CONFIG_FILE_NAME}"

  if [[ ! -f "${config_file}" ]]; then
    cat > "${config_file}" <<YAML
# Tunely relay config
# Remove '#' and edit values to enable service start.
#
# listen: "0.0.0.0:8080"
# auth_tokens:
#   - "xxx"
#   - "yyy"
# request_timeout_secs: 60
YAML
  fi
}

has_meaningful_yaml_content() {
  local file="$1"
  grep -Eq '^[[:space:]]*[^#[:space:]]' "${file}"
}

write_systemd_unit() {
  local config_file="${CONFIG_DIR}/${CONFIG_FILE_NAME}"

  cat > "/etc/systemd/system/${SERVICE_NAME}.service" <<UNIT
[Unit]
Description=Tunely Relay Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${RUN_USER}
Group=${RUN_GROUP}
ExecStart=${INSTALL_DIR}/tunely relay --config ${config_file}
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
}

install_files() {
  install -d -m 0755 "${INSTALL_DIR}"
  install -d -m 0750 "${CONFIG_DIR}"

  install -m 0755 "${RELAY_BIN_SOURCE}" "${INSTALL_DIR}/relay-server"
  install -m 0755 "${TUNELY_BIN_SOURCE}" "${INSTALL_DIR}/tunely"
  ln -sf "${INSTALL_DIR}/tunely" /usr/local/bin/tunely
  ensure_config_files

  if [[ "${SYSTEMD_ENABLED}" == "true" ]]; then
    write_systemd_unit
  fi

  chown -R "${RUN_USER}:${RUN_GROUP}" "${INSTALL_DIR}" "${CONFIG_DIR}"
}

start_service_if_config_ready() {
  local config_file="${CONFIG_DIR}/${CONFIG_FILE_NAME}"

  if [[ "${SYSTEMD_ENABLED}" != "true" ]]; then
    return
  fi

  systemctl daemon-reload
  systemctl enable "${SERVICE_NAME}.service"

  if has_meaningful_yaml_content "${config_file}"; then
    systemctl restart "${SERVICE_NAME}.service"
    echo "[OK] ${SERVICE_NAME}.service started"
  else
    echo "[INFO] ${config_file} is empty (or comments only); service was enabled but not started"
  fi
}

print_summary() {
  local config_file="${CONFIG_DIR}/${CONFIG_FILE_NAME}"

  echo
  echo "[OK] relay installed"
  echo "  command : /usr/local/bin/tunely"
  echo "  tunely  : ${INSTALL_DIR}/tunely"
  echo "  binary  : ${INSTALL_DIR}/relay-server"
  echo "  config  : ${config_file}"

  if [[ "${SYSTEMD_ENABLED}" == "true" ]]; then
    echo "  service : ${SERVICE_NAME}.service"
    echo
    echo "Next steps:"
    echo "  1) Fill ${config_file} (remove '#' from required keys)"
    echo "  2) Start relay: systemctl start ${SERVICE_NAME}"
    echo
    echo "Check status/logs:"
    echo "  systemctl status ${SERVICE_NAME}"
    echo "  journalctl -u ${SERVICE_NAME} -f"
  else
    echo
    echo "systemd not in use. Run manually after filling config:"
    echo "  tunely relay --config ${config_file}"
  fi
}

main() {
  trap cleanup EXIT
  require_root
  parse_args "$@"
  resolve_systemd_mode
  ensure_user
  install_files
  start_service_if_config_ready
  print_summary
}

main "$@"
