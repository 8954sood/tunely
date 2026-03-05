#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "[ERROR] Run as root (use sudo)."
  exit 1
fi

for svc in tunely-agent tunely-relay; do
  if systemctl list-unit-files | grep -q "^${svc}\.service"; then
    systemctl disable --now "${svc}.service" || true
    rm -f "/etc/systemd/system/${svc}.service"
  fi
done

systemctl daemon-reload

rm -rf /opt/tunely
rm -rf /etc/tunely

echo "[OK] removed tunely services and files"
