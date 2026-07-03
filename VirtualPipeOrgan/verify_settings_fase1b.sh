#!/usr/bin/env bash
# E2E-verificatie Fase 1 + 1b: master + temperament + EQ + reverb per orgel.
# Test op orgels die de live webview NIET toont (die staat op zijn sessie-orgel A=GreenPositiv
# en raakt B/C nooit aan), zodat de webview-startup-push de test niet racet.
set -u
API="http://127.0.0.1:8765"
CURL="curl -s --max-time 25"
B="C:/Bronbestanden/JM-Orgue/testen/Burea_Funeral_Chapel/burea_gravkapell.organ"
C="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel"
B_SET="C:/Bronbestanden/JM-Orgue/testen/Burea_Funeral_Chapel/.jm-settings.json"
C_SET="C:/Bronbestanden/JM-Orgue/testen/JM-Orgue testorgel/.jm-settings.json"

echo "schoonmaken + webview laten settelen..."; rm -f "$B_SET" "$C_SET"; sleep 6

show() { $CURL "$API/settings/organ" | grep -oE '"master_volume_db":[^,]*|"name":"[^"]*"|"fine_tune_cents":[^,}]*|"mid_gain":[^,]*|"reverb_type":"[^"]*"|"mix":[^,]*' | tr '\n' ' '; echo; }

echo "== Laad B + zet master=-4.5 / Kirnberger fine=3 / EQ gain=6.5 / reverb mix=55 algorithmic =="
$CURL -X POST "$API/load_organ" -d "{\"path\":\"$B\"}" >/dev/null
$CURL -X POST "$API/settings/master?db=-4.5" >/dev/null
$CURL -X POST "$API/settings/temperament?name=Kirnberger&fine=3" >/dev/null
$CURL -X POST "$API/settings/eq?gain=6.5" >/dev/null
$CURL -X POST "$API/settings/reverb?mix=55&type=algorithmic" >/dev/null
$CURL -X POST "$API/settings/save" >/dev/null
echo -n "B na opslaan: "; show

echo "== Laad C (testorgel) — lek-check: alles null =="
$CURL -X POST "$API/load_directory" -d "{\"path\":\"$C\"}" >/dev/null
echo -n "C: "; $CURL "$API/settings/organ" | grep -oE '"master_volume_db":[^,]*|"temperament":null|"eq":null|"reverb":null' | tr '\n' ' '; echo

echo "== Herlaad B — herstel-check =="
$CURL -X POST "$API/load_organ" -d "{\"path\":\"$B\"}" >/dev/null
echo -n "B herladen: "; show
