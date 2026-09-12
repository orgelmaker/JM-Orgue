# Testscripts (test-API, poort 8765)

Start de app met `vpo-app.exe --test-api` (of via de MCP-server) en draai de scripts met
Python 3 (numpy + ffmpeg op PATH voor de audiometingen). Ze zijn ontstaan in de
ontwikkelsessies van 0.7.36–0.7.38 en documenteren hoe de functies zijn geverifieerd.

| Script | Wat het meet |
|---|---|
| `test_stereo.py` | Stereo-weergave: L/R-correlatie van een opname (stereo-set ≠ 1, mono-set = 1). |
| `test_tutti.py [kap] [stereo|mono]` | Polyfonie/belasting bij een tutti (30 s, 10-noots akkoorden). |
| `licht.py` | Belasting bij 3 en 45 stemmen (referentie voor per-stem-kosten). |
| `maak_go_testodf.py` | Maakt `go_testodf/TestRanks.organ` (2 gestapelde ranks + PitchCorrection) op Puttershoek-samples via een junction. |
| `test_ranks_perspectives.py` | Gestapelde ranks en perspectieven (stemmen per toets, gains, HW-set, regressie). |
| `test_hertemperen.py` / `test_ranks_pitch.py` | Toonhoogte per modus (Origineel/ET/middentoon) via FFT-piek. |
| `test_crescendo_zwel.py` | Zweltrede (sweep, jitter, sprong, bereik/inversie) en generaal crescendo (hysterese, additiviteit, persistentie). |
| `test_remote.py [poort]` | Afstandsbediening: token, pagina, acties, uit/aan, nieuw token. |
| `test_midi_archive.py` | Automatisch MIDI-archief: start/stilte/min-noten/flush/lijst/speler. |
| `test_jmrec_import.py` | Map-import met `.organ` (JM-Rec) vs. mapscan. |
| `check_i18n_keys.py` / `i18n_merge_check.py` | Sleutelsets nl/en/fr/de gelijk; dekking van `$t()`-sleutels in de code. |
| `capture_window.ps1` / `click.ps1` | Screenshot (PrintWindow) en klik/scroll in het app-venster op venster-coördinaten. |

Let op: enkele drempels in `test_crescendo_zwel.py` en de release-wachttijd (2,5 s) in
`test_ranks_perspectives.py` zijn streng; Friesach-releases duren ~5 s en het hysterese-
model kan 1 CC-eenheid afwijken bij een trapgrens. Paden naar samplesets staan bovenin
de scripts.
