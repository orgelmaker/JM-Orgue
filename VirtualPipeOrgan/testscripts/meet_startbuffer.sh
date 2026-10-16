#!/bin/bash
# Startbuffer-meting met een VERS proces per instelling (0.8.92): een eerder
# geladen orgel laat anders zijn geheugen nog even staan en vervuilt de
# RAM-meting. Gebruik: bash meet_startbuffer.sh "2000 500 200" [seconden] [orgelpad]
set -u
MS_LIJST="${1:-2000 500 200}"; SEC="${2:-60}"; ORGEL="${3:-}"; EXTRA="${4:-}"
APP="C:\\Bronbestanden\\JM-Orgue\\VirtualPipeOrgan\\target\\release\\vpo-app.exe"
cd "$(dirname "$0")"
for ms in $MS_LIJST; do
  powershell.exe -NoProfile -Command "Stop-Process -Name vpo-app -Force -ErrorAction SilentlyContinue"; sleep 2
  powershell.exe -NoProfile -Command "Start-Process -FilePath '$APP' -ArgumentList '--test-api' -WorkingDirectory 'C:\\Bronbestanden\\JM-Orgue\\VirtualPipeOrgan'"
  for i in $(seq 1 40); do curl -s http://127.0.0.1:8765/version >/dev/null 2>&1 && break; sleep 1; done
  sleep 3
  if [ -n "$ORGEL" ]; then python -I test_preload_rotterdam.py "$ms" "$SEC" "$ORGEL" $EXTRA; else python -I test_preload_rotterdam.py "$ms" "$SEC"; fi
  echo "-----"
done
powershell.exe -NoProfile -Command "Stop-Process -Name vpo-app -Force -ErrorAction SilentlyContinue"
