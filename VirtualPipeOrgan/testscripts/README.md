# Testscripts (test-API, poort 8765)

Start de app met `vpo-app.exe --test-api` (of via de MCP-server) en draai de scripts met
Python 3 (numpy + ffmpeg op PATH voor de audiometingen). Ze zijn ontstaan in de
ontwikkelsessies van 0.7.36–0.7.38 en documenteren hoe de functies zijn geverifieerd.

| Script | Wat het meet |
|---|---|
| `test_stereo.py` | Stereo-weergave: L/R-correlatie van een opname (stereo-set ≠ 1, mono-set = 1). |
| `test_tutti.py [kap] [stereo|mono]` | Polyfonie/belasting bij een tutti (30 s, 10-noots akkoorden). |
| `licht.py` | Belasting bij 3 en 45 stemmen (referentie voor per-stem-kosten). |
| `maak_go_testodf.py` | Maakt `go_testodf/TestRanks.organ` (2 gestapelde ranks + PitchCorrection) op eigen samples via een junction. |
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
| `referentieproef.py` | **Vaste referentieproef (C3):** `midi` (vast MIDI-bestand `testdata/referentieproef.mid`), `opnemen` (laadt een orgel, trekt per register alleen dat register, speelt het bestand af en bewaart opname + `manifest.json`), `analyseer`, `vergelijk` (twee opnamen of mappen: JM-Orgue voor/na een wijziging, of tegen een referentie) en `zelftest` (controleert het hulpmiddel op synthetische opnamen met bekende waarden). Meet per noot toonhoogte (cent), niveau, aanzet- en uitsterftijd; richtwaarden uit `docs/AB_KLANKPROTOCOL.md`. Zie sectie 7 daarvan. |
| `ab_vergelijk.py` | De handmatige variant voor één noot en twee WAV's (`docs/AB_KLANKPROTOCOL.md`). |
| `check_i18n_keys.py` / `i18n_merge_check.py` | Sleutelsets nl/en/fr/de gelijk; dekking van `$t()`-sleutels in de code. |
| `capture_window.ps1` / `click.ps1` | Screenshot (PrintWindow) en klik/scroll in het app-venster op venster-coördinaten. |

`test_tremulant_samples.py` onderscheidt de standen aan de toonhoogte: droge attack `f`,
tremulant-attack `2f`, droge release `1,26f` (een grote terts: een octaaf zou zijn tweede
boventoon met de grondtoon van de aanslag delen), tremulant-release `1,5f`. Een FFT-piek die op een
boventoon vastloopt geeft daar een FAIL met de gemeten Hz erbij — lees de waarden voordat je
concludeert dat de tremulant niet werkt.

Let op bij het beoordelen van metingen (lessen uit 0.7.39):
- Wachttijden op release-staarten zijn nu poll-lussen: natte sets
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

## Orgelbouwer: achtergrondproeven

Deze scripts starten eigen headless appprocessen, gebruiken een afzonderlijk
profiel en maken synthetische bronnen. Geef `--exe` en `--work-dir` als absolute
paden op. De wind- en klepproef gebruiken een werkmap binnen
`testen/orgelbouwer-uitbreiding-2026-11-26` van de checkout; de feedbackproef
gebruikt `testen/orgelbouwer-feedback-2026-11-26`. Gebruik voor een gewijzigde
fixture een nieuwe submap. De scripts bedienen geen gebruikersvenster.
De meerbronnenproef gebruikt `testen/orgelbouwer-meerbronnen-2026-11-26`.

| Script | Wat het controleert |
|---|---|
| `organ_project_wind_background_test.py` | Twee bronwindnetwerken met gelijke compartiment-ID's, een geldige dynamische schakelklep met beginstand aan, onafhankelijke druk, herstel, uitschakelen, herstart en opruimen bij een gewoon orgel. |
| `organ_project_valve_background_test.py` | Schakel- en continue kleppen, bronbeginstanden, inversie, gelijke controller-ID's in twee bronnen, gekoppelde registers, zelfstandige bediening, directe veilige opslag en herstart. |
| `organ_project_feedback_background_test.py` | Mechanische balgfeedback en een aparte druklus, directe continue overdrachten, Willis/inertia en drempelschakelaar in twee onafhankelijke bronnen. Gebruikt een eigen submap onder `testen/orgelbouwer-feedback-2026-11-26`. |
| `organ_project_multisource_background_test.py` | Procedure met drie en daarna vier bronnen in dezelfde map, eigen pakketmappen en overlappende native ID's. Meet afzonderlijke stemming en windverbruik en controleert bronvolgorde, bron vervangen, stabiele console-/registerdoelen, 65-bronnenafwijzing en herstart. |
| `organ_project_multisource_ui_test.mjs` | Componentprocedure met echte Svelte en IPC-mocks: één beginvak, bronnen toevoegen, stemming/pakketmap per bron, keuzelijst vanaf vijf bronnen, labels na Z, grens 64/65 en compacte vensterbreedte. Native audio loopt via de achtergrondproef. |

Projecten gebruiken nog schema 1 en kunnen één tot 64 bronnen bevatten.
Bestaande projecten met twee bronnen blijven geldig. De globale grenzen blijven
2.048 registers, 16 gebruikte rankkasten, 256 actieve windcompartimenten en
4.096 windcontrollers; die worden niet per bron vermenigvuldigd. De interface
begint met één bronkaart, biedt **Bron toevoegen…** en vervolgt de bronletters
na Z met AA, AB, enzovoort.

Reproduceer de meerbronnenproeven vanuit `VirtualPipeOrgan`. Vervang in deze
voorbeelden de checkout en het Playwright-modulepad door de lokale absolute
paden en kies een nieuwe werkmap:

```powershell
py testscripts/organ_project_multisource_background_test.py --exe "C:/werk/JM-Orgue/VirtualPipeOrgan/target/debug/vpo-app.exe" --work-dir "C:/werk/JM-Orgue/testen/orgelbouwer-meerbronnen-2026-11-26/run-1"
node testscripts/organ_project_multisource_ui_test.mjs --playwright "C:/tools/playwright/index.mjs" --chrome "C:/Program Files/Google/Chrome/Application/chrome.exe" --output "C:/werk/JM-Orgue/testen/orgelbouwer-meerbronnen-2026-11-26/ui"
```

De achtergrondproef ondersteunt `--prepare-only` voor alleen het aanmaken van
fixtures en bewaart `multisource-result.json` met proceslogboeken. De
componentproef vereist de UI-afhankelijkheden, Chrome en Playwright en bewaart
`result.json`, `multisource-wide.png` en `multisource-narrow.png`.

De broncontroller-ID's zijn gescheiden per bron-UUID en type. Een gekoppelde
schakelaar volgt de OR van zijn projectregisters; zelfstandige waarden worden
direct per project bewaard. De lokale API biedt `GET /project/wind` voor
drukken, actuele controllerstanden, klepopeningen en massastromen;
`GET /project/wind/controls` geeft doelwaarden en
`POST /project/wind/control` bedient alleen zelfstandige controllers met een
eindige waarde binnen 0..1. Automatische controllers en verborgen bronconstanten
zijn alleen uitleesbaar. De feedbackproef gebruikt daarnaast balgpositie,
snelheid en slagfractie, en modeltijd uit de audiokern in plaats van alleen
kloktijd. Zie [TEST_API.md](../docs/TEST_API.md) voor de meetvelden en formules.

De tests vergelijken de eigen JM-simulatie, inclusief een handbediende
klepovergang van 10 ms en klankfade onder 60% bronlagedruk. Ze bewijzen geen
gelijkheid aan de Hauptwerk-windsolver. De JM-formules omvatten continue
inversie/offset/factor/macht, exponentiële Willis-regeling en een gedempte
inertievergelijking. Niet ondersteunde dubbele continue overdrachten,
balgveren en onbekende codes moeten vóór orgelvervanging worden geweigerd.

Een geselecteerd register met een tevens automatisch aangestuurde
bronschakelaar moet eveneens vóór laden worden geweigerd. Een bronproducent
mag een bestaande projectregisterbinding niet stilzwijgend vervangen.

De genegeerde Rust-proef
`installed_nitra_readable_definition_retains_complete_blower_and_bellows_closure`
kan aanvullend tegen een lokaal geïnstalleerde Nitra-demo draaien. Stel daarvoor
`JM_NITRA_DEFINITION` op het absolute pad van de leesbare ODF. De gewone
testsuite heeft die commerciële/demo-installatie niet nodig.
De eveneens genegeerde app-proef
`installed_nitra_automatic_wind_closure_runs_ten_seconds` laat de ingelezen
wind- en besturingsketen tien seconden draaien. Dit is een windproef zonder
geladen bronpijpen; het bewijst geen volledige orgelcompatibiliteit of
klankgelijkheid aan Hauptwerk.
