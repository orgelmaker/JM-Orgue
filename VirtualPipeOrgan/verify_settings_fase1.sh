#!/usr/bin/env bash
# E2E-verificatie Fase 1: master volume + temperament per orgel (backend round-trip).
# Bewijst: opslaan verzamelt uit state, herstel leest per orgel, geen lek tussen orgels.
set -u
API="http://127.0.0.1:8765"
A="C:/Bronbestanden/JM-Orgue/testen/GreenPositiv_GrandOrgue_1_2/GreenPositiv.organ"
B="C:/Bronbestanden/JM-Orgue/testen/Burea_Funeral_Chapel/burea_gravkapell.organ"
A_SETTINGS="C:/Bronbestanden/JM-Orgue/testen/GreenPositiv_GrandOrgue_1_2/.jm-settings.json"

echo "== Laad orgel A (GreenPositiv) =="
curl -s -X POST "$API/load_organ" -d "{\"path\":\"$A\"}" >/dev/null; echo "geladen"
echo "== Zet master=-3.5 dB + temperament 'Werckmeister III' fine=7 op A =="
curl -s -X POST "$API/settings/master?db=-3.5" >/dev/null
curl -s -X POST "$API/settings/temperament?name=Werckmeister%20III&fine=7" >/dev/null
curl -s -X POST "$API/settings/save" >/dev/null; echo "opgeslagen"
echo "-- A na opslaan (verwacht master=-3.5, temperament.name=Werckmeister III, fine=7):"
curl -s "$API/settings/organ" | grep -oE '"master_volume_db":[^,]*|"name":"[^"]*"|"fine_tune_cents":[^,}]*'

echo "== .jm-settings.json op schijf voor A (bewijst persistentie naar bestand): =="
grep -oE '"master_volume_db":[^,]*|"name":"[^"]*"|"fine_tune_cents":[^,}]*' "$A_SETTINGS" 2>/dev/null | head -5

echo "== Laad orgel B (Burea) — verwacht GEEN lek van A's waarden =="
curl -s -X POST "$API/load_organ" -d "{\"path\":\"$B\"}" >/dev/null; echo "geladen"
echo "-- B (verwacht master null/anders dan -3.5, temperament null of eigen):"
curl -s "$API/settings/organ" | grep -oE '"master_volume_db":[^,]*|"temperament":[^,]*' | head -3

echo "== Laad A opnieuw — verwacht herstel van master=-3.5 + Werckmeister III =="
curl -s -X POST "$API/load_organ" -d "{\"path\":\"$A\"}" >/dev/null; echo "geladen"
echo "-- A na herladen:"
curl -s "$API/settings/organ" | grep -oE '"master_volume_db":[^,]*|"name":"[^"]*"|"fine_tune_cents":[^,}]*'
