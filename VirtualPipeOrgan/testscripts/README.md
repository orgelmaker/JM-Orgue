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
| `test_pedaal_0744.py` | Pedaalfixes 0.7.44: kast open bij verdrongen/gewiste zwelkoppeling, crescendo wissen → trap 0, erfregel bereik, spiegel meteen hoorbaar, dode zone 8, claims na audiowissel, herstart vanaf 0 na setzer, MIDI-CC-diagnose in het log. Zet de oorspronkelijke instellingen terug. |
| `test_tredefilter_0745.py` | Tredefilter 0.7.45: losse uitschieter haalt de zwelkast niet, rustwiebel laat de kast stilstaan, echte beweging komt aan, losse waarden met pauzes gelden meteen, en helemaal dicht/open wordt exact gehaald. Zet de oorspronkelijke zwelkoppelingen terug. |
| `test_midi_archive.py` | Automatisch MIDI-archief: start/stilte/min-noten/flush/lijst/speler. |
| `test_jmrec_import.py` | Map-import met `.organ` (JM-Rec) vs. mapscan, bibliotheeknaam "Kerk - Bouwer - Plaats" en de gevonden orgelafbeelding. |
| `maak_go_testtrem.py [map]` | Maakt `TestTrem.organ` + `TestTremWc.organ` (GrandOrgue-golfvormtremulant: trem-attack en trem-release met een eigen toonhoogte) in `%TEMP%\jm-orgue-testodf`. |
| `test_tremulant_samples.py` | Tremulant-OPNAMEN: registervlaggen, `POST /tremulant`, attack en release per stand (FFT-piek), crossfade tijdens het klinken, regressie zonder trem-samples. |
| `check_i18n_keys.py` / `i18n_merge_check.py` | Sleutelsets nl/en/fr/de gelijk; dekking van `$t()`-sleutels in de code. |
| `capture_window.ps1` / `click.ps1` | Screenshot (PrintWindow) en klik/scroll in het app-venster op venster-coördinaten. |

`test_tremulant_samples.py` onderscheidt de standen aan de toonhoogte: droge attack `f`,
tremulant-attack `2f`, droge release `1,26f` (een grote terts: een octaaf zou zijn tweede
boventoon met de grondtoon van de aanslag delen), tremulant-release `1,5f`. Een FFT-piek die op een
boventoon vastloopt geeft daar een FAIL met de gemeten Hz erbij — lees de waarden voordat je
concludeert dat de tremulant niet werkt.

Let op bij het beoordelen van metingen (lessen uit 0.7.39):
- Wachttijden op release-staarten zijn nu poll-lussen: natte sets (Friesach, Saint-Jean-de-Luz)
  klinken 5 tot 8 seconden na, en sinds 0.7.39 worden verse staarten niet meer weggekozen.
- Niveaumetingen op ÉÉN opname zeggen weinig: de samples zelf zwellen met een periode van
  ongeveer een seconde ±1,7 dB aan, en twee registers op dezelfde toonhoogte zweven tegen
  elkaar in. Meet een trapwissel daarom met registers van verschillende toonhoogte en
  vergelijk altijd met een rustmeting uit dezelfde opname.
- Tellers als `files_written` lopen per app-sessie door; reken met het verschil ten opzichte
  van de stand bij aanvang, niet met absolute waarden.
- Een FFT-piek is de sterkste partiaal, niet de grondtoon. Kies testtoonhoogtes die geen
  octaaf of boventoon van elkaar zijn.
Paden naar samplesets staan bovenin de scripts.
