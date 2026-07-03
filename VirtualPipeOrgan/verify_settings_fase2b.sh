#!/usr/bin/env bash
# Definitieve Fase 2-diagnostiek: isoleert restore (mirror-endpoint) van save/webview.
# Gebruikt een correct ge-encode echte divisienaam.
set -u
API="http://127.0.0.1:8765"
CURL="curl -s --max-time 90"
PY="/c/Users/orgel/AppData/Local/Programs/Python/Python314/python.exe"
command -v python >/dev/null 2>&1 && PY=python
B="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel"
C="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel 2"
B_SET="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel/.jm-settings.json"
C_SET="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel 2/.jm-settings.json"

rm -f "$B_SET" "$C_SET"; echo "webview settelen..."; sleep 6

echo "== Laad B =="
$CURL -X POST "$API/load_directory" -d "{\"path\":\"$B\"}" >/dev/null
DIV=$(curl -s --max-time 10 "$API/organ" | "$PY" -c "import sys,json; print(json.load(sys.stdin)['divisions'][0]['name'])")
DIV_ENC=$("$PY" -c "import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1]))" "$DIV")
echo "testdivisie: [$DIV]"
echo "== Zet pan=0.7 + tremulant rate=5.5 op die divisie =="
$CURL -X POST "$API/settings/pan?division=$DIV_ENC&pan=0.7" ; echo
$CURL -X POST "$API/settings/trem?division=$DIV_ENC&rate=5.5" ; echo
echo "-- mirror direct na zetten (set_* schreef de mirror): "
curl -s --max-time 10 "$API/settings/mirror"; echo
$CURL -X POST "$API/settings/save" >/dev/null
echo "-- op schijf na opslaan: "
grep -oE '"division_pans":\[[^]]*\]' "$B_SET" 2>/dev/null; echo

echo "== Laad C, herlaad B, lees mirror DIRECT (= wat restore invulde) =="
$CURL -X POST "$API/load_directory" -d "{\"path\":\"$C\"}" >/dev/null
$CURL -X POST "$API/load_directory" -d "{\"path\":\"$B\"}" >/dev/null
echo "-- mirror direct na herladen B (isoleert restore): "
curl -s --max-time 10 "$API/settings/mirror"; echo
