#!/usr/bin/env bash
# E2E-verificatie Fase 2: per-divisie DSP (pan + tremulant representatief) per orgel.
# Test op orgels die de webview niet toont, zodat er geen race is.
set -u
API="http://127.0.0.1:8765"
CURL="curl -s --max-time 90"
B="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel"
C="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel 2"
B_SET="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel/.jm-settings.json"
C_SET="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel 2/.jm-settings.json"

echo "schoonmaken + webview settelen..."; rm -f "$B_SET" "$C_SET"; sleep 6
show() { curl -s --max-time 10 "$API/settings/organ" | grep -oE '"division_pans":\[[^]]*\]|"division_tremulants":\[[^]]*\]'; }

echo "== Laad B + zet pan + tremulant op een divisie =="
$CURL -X POST "$API/load_directory" -d "{\"path\":\"$B\"}" >/dev/null
# Divisienaam uit het orgel halen:
DIV=$(curl -s --max-time 10 "$API/organ" | grep -oE '"name":"[^"]*"' | sed -n '2p' | sed 's/"name":"//;s/"//')
echo "testdivisie: ${DIV:-(onbekend)}"
$CURL -X POST "$API/settings/pan?division=${DIV// /%20}&pan=0.7" >/dev/null
$CURL -X POST "$API/settings/trem?division=${DIV// /%20}&rate=5.5" >/dev/null
$CURL -X POST "$API/settings/save" >/dev/null
echo -n "B na opslaan: "; show

echo "== Laad C — lek-check (geen pan/trem van B) =="
$CURL -X POST "$API/load_directory" -d "{\"path\":\"$C\"}" >/dev/null
echo -n "C: "; show

echo "== Herlaad B — herstel-check =="
$CURL -X POST "$API/load_directory" -d "{\"path\":\"$B\"}" >/dev/null
echo -n "B herladen: "; show
