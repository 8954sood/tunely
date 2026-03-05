#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "[ERROR] Run as root (use sudo)."
  exit 1
fi

has_systemd() {
  command -v systemctl >/dev/null 2>&1 \
    && [[ -d /run/systemd/system ]] \
    && systemctl show-environment >/dev/null 2>&1
}

if has_systemd; then
  for svc in tunely-agent tunely-relay; do
    if systemctl list-unit-files | grep -q "^${svc}\.service"; then
      systemctl disable --now "${svc}.service" || true
      rm -f "/etc/systemd/system/${svc}.service"
    fi
  done
  systemctl daemon-reload
else
  rm -f /etc/systemd/system/tunely-agent.service
  rm -f /etc/systemd/system/tunely-relay.service
  echo "[INFO] systemd not available; skipped service stop/disable"
fi

rm -rf /opt/tunely
rm -rf /etc/tunely

echo "[OK] removed tunely services and files"
