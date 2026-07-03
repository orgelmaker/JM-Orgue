#!/usr/bin/env bash
# End-to-end verificatie van de MIDI-opname-keten via de test-API (poort 8765).
# Injecteert noten door de échte BLE-MIDI-loop (die de capture-hook aanroept),
# controleert dat events worden vastgelegd, slaat op als .mid en valideert de header.
set -u
API="http://127.0.0.1:8765"
OUT="C:/Users/orgel/AppData/Local/Temp/jm-orgue-test-recording.mid"

echo "== /status =="
curl -s "$API/status" || { echo "Test-API niet bereikbaar"; exit 1; }
echo; echo "== start MIDI-opname =="
curl -s -X POST "$API/midi/record/start"; echo

# Speel een toonladdertje: elke noot on, dan off.
for n in 60 62 64 65 67; do
  curl -s -X POST "$API/midi/inject?note=$n&on=1&velocity=100" >/dev/null
  curl -s -X POST "$API/midi/inject?note=$n&on=0" >/dev/null
done

echo "== status na injectie =="
curl -s "$API/midi/record/status"; echo
echo "== stop =="
curl -s -X POST "$API/midi/record/stop"; echo
echo "== save -> $OUT =="
curl -s -X POST "$API/midi/record/save" -d "{\"path\":\"$OUT\"}"; echo

echo "== header (eerste 16 bytes) =="
od -A x -t x1z "$OUT" | head -2
