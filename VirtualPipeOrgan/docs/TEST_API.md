# JM-Orgue HTTP-API

Een lichte HTTP-API waarmee scripts, dashboards, AI-agenten (bijvoorbeeld
een MCP-tool), stream-deck en de ingebouwde afstandsbediening met de app
kunnen praten. Bedoeld voor:

- Rooktests van de app vanuit Python/shell/C.
- Dashboards die de belasting of het geladen orgel willen tonen.
- MCP-tools die een AI het orgel laten bedienen (bv. "trek Prestant 8 en
  speel noot 60").
- Stream-deck- of foot-switch-knoppen die registers of koppels wisselen.
- De telefoon of tablet als tweede registreerscherm (QR-code in de app →
  opent de `/remote`-weergave).

## Twee ingangen

### A. Lokaal, zonder token (test-API)

Luistert op `http://127.0.0.1:8765`, alleen vanaf dezelfde computer. Alle
endpoints beschikbaar. Zet aan met:

```bat
rem Alleen deze keer:
vpo-app.exe --test-api

rem Of via een omgevingsvariabele (blijft aan):
set JM_ORGUE_TEST_API=1
vpo-app.exe
```

Voor een rooktest met een eigen bibliotheek, logboek en webview-opslag:

```bat
vpo-app.exe --test-api=18765 --test-data-dir "C:\testdata\jm-orgue-migratie"
```

Het mappad moet absoluut zijn en deze optie werkt alleen samen met `--test-api`.
Het hoofdvenster vermeldt `[testprofiel]`. Gebruik voor een test samplesets in
een eigen testmap: instellingen naast een geladen orgel worden daar nog steeds
opgeslagen. Extra vensters gebruiken eveneens een afzonderlijke testidentiteit.

Voor een achtergrondtest zonder appvensters of webviews:

```bat
vpo-app.exe --test-api=18768 --test-data-dir "C:\testdata\jm-orgue-achtergrond" --test-headless
```

`--test-headless` vereist de test-API en een expliciet absoluut testprofiel.
De gewone appstart blijft ongewijzigd. Gebruik een afzonderlijke kopie van de
testorgels: ook in deze modus worden zijbestanden naast een geladen definitie
bewaard. De lokale test-API, orgellader en instellingenopslag werken normaal.

Consolebouwer heeft de volgende **uitsluitend lokale test-endpoints**:

- `GET /test/profile` geeft `{app_data_dir, process_id}` om een geïsoleerd testprofiel te controleren.
- `GET /settings/console_layout?organ_id=URLENCODED_PAD` geeft `{layout}` voor precies het geladen orgel.
- `POST /settings/console_layout` met `{organ_id, layout}` past een gevalideerd ontwerp toe; `layout: null` wist het ontwerp. Het antwoord is `{ok: true, warning: null}`. Een melding in `warning` betekent dat de lokale bibliotheek veilig is opgeslagen, maar de kopie bij de sampleset niet kon worden geschreven.

Een ander orgel-ID, onbekende versie, ongeldige afmetingen of actieve ontwerpen
met verwijzingen naar ontbrekende registers worden afgewezen voordat de opslag wijzigt. De endpoints
zijn niet beschikbaar via de afstandsbediening. De runtimetest
`testscripts/console_builder_live_test.mjs` vereist de eigen gemarkeerde fixturemap
en controleert het profiel vóórdat hij instellingen schrijft:

```powershell
node testscripts/console_builder_live_test.mjs --base http://127.0.0.1:18766 --fixture-dir "C:\Bronbestanden\Prive\JM-Orgue-consolebouwer\testen\consolebouwer-2026-11-26\orgels"
```

Na herstart kan dezelfde opdracht met `--verify-existing` de twee opgeslagen
ontwerpen teruglezen zonder nieuwe ontwerpen toe te passen.

`--verify-texture` controleert na een GUI-import de gekozen eigen PNG en wijst
ongeldige textuurgegevens af zonder het geldige ontwerp te wijzigen. Voor een
volledige proef zonder vensters gebruikt
`testscripts/console_texture_background_test.py` uitsluitend eigen subprocessen
en een afzonderlijke kopie van de gemarkeerde fixtures:

```powershell
py testscripts/console_texture_background_test.py --exe "C:\Bronbestanden\Prive\JM-Orgue\VirtualPipeOrgan\target\debug\vpo-app.exe" --source-dir "C:\Bronbestanden\Prive\JM-Orgue-consolebouwer\testen\consolebouwer-2026-11-26" --work-dir "C:\Bronbestanden\Prive\JM-Orgue-consolebouwer\testen\consolebouwer-2026-11-26\achtergrond"
```

De bron moet twee reeds opgeslagen ontwerpen bevatten, waarvan A de gekozen
fixturetextuur gebruikt. De achtergrondproef kopieert alleen orgeldefinities en
WAV-bestanden, controleert A → B → A, herstart de app en vergelijkt de afbeelding,
bibliotheek en afzonderlijke zijbestanden opnieuw. Het bronprofiel blijft intact.

### B. Over het netwerk, met token (afstandsbediening)

Luistert op `0.0.0.0:<poort>`, bereikbaar vanaf elke machine in hetzelfde
wifi-net. Zet aan via Algemene instellingen → **Afstandsbediening**, of
API-matig:

```
POST /remote/enable?on=1&port=8766     # schakel aan, poort optioneel
GET  /remote/status                     # geeft {addresses, port, token}
POST /remote/new_token                  # roteer het token
```

Elk ander verzoek vereist een geldig token, pad-prefix óf header:

```
GET  /r/<token>/api/health
GET  /api/health          + header "X-Remote-Token: <token>"
```

De endpoints onder scope B zijn een afgebakende whitelist (zie "Volledige
endpoint-lijst" hieronder — de regels die zowel in Test als Remote
beschikbaar zijn). Zware acties als een orgel laden of samplesets
scannen blijven lokaal.

## Alles ook via `/api/…`

Vanaf 0.9.14 bestaat elk endpoint óók onder een `/api/`-prefix, als
canoniek pad voor externe scripts. Beide vormen werken:

```
GET /api/health      # aanbevolen voor externe scripts/MCP
GET /health          # werkt ook, blijft werken
```

Prefix is cosmetisch (bufferloos: gewoon pad-strippen); kan ook met het
token-pad: `GET /r/<token>/api/status`.

> **Veiligheid.** De lokale ingang (A) luistert alleen op loopback — wie
> bij localhost kan, kan sowieso lokale programma's starten; verdere
> authenticatie heeft geen zin. De netwerk-ingang (B) vereist altijd een
> token en biedt een afgebakende whitelist, geen hele API.

## Snel checken dat de app leeft

```
GET /api/health
```

```json
{
  "status": "ok",
  "version": "0.9.13",
  "organ": "Nitra demo",
  "drawn_stops": 7,
  "voice_frames_now": 436,
  "render_load": 0.39,
  "render_peak": 0.46,
  "render_overloads": 0,
  "midi_connected": true
}
```

- `status` — "ok" zodra de HTTP-poort én de audiostate antwoorden.
- `voice_frames_now` — hoeveel stemmen op dit moment klinken (ruwe teller).
- `render_load` / `render_peak` — gemiddelde en piek van de buffertijd die de
  mengloop neemt (0-1).

## Minimum om noten te spelen

```
POST /load_organ             body: {"path": "C:/sets/orgel.organ"}
POST /load_directory         body: {"path": "C:/sets/orgelmap"}
GET  /organ                  → divisies + registers + perspectieven
POST /stops/set              body: {"ids": ["0_12","0_34"]}
POST /notes/{noot}/on?velocity=100
POST /notes/{noot}/off
POST /midi/inject?note=60&on=1&velocity=100&channel=0
POST /midi/inject?cc=11&value=96&channel=0
```

Noot: `/notes/N/on` schiet rechtstreeks pijpen af (handig voor rooktests
zonder MIDI-mapping). `/midi/inject` gaat de hele MIDI-lus door — mapping,
zweltrede, crescendo, melodie-/baskoppel — en meet dus de écht gespeelde
state.

## Volledige endpoint-lijst (gegroepeerd)

### Overzicht

| Methode | Pad | Wat |
|---|---|---|
| GET | `/version` | `{"version":"0.9.13"}` |
| GET | `/health` | levensteken + belasting + geladen orgel |
| GET | `/status` | volledige audio-/MIDI-/perspectiefstatus |
| GET | `/organ` | geladen orgel met divisies, registers, koppels |
| GET | `/state` | registratie + koppelstand voor de afstandsbediening |
| GET | `/library` | bibliotheek-items met pad en naam |
| GET | `/logs?lines=200` | laatste logregels |

### Orgel laden en bedienen

| | | |
|---|---|---|
| POST | `/load_organ` | body `{"path":"..."}` |
| POST | `/load_directory` | body `{"path":"..."}` (map met opnamen) |
| GET  | `/stops/drawn` | lijst getrokken register-id's |
| POST | `/stops/set` | body `{"ids":[...]}` |
| POST | `/stops/{id}/toggle` | één register aan/uit |
| POST | `/couplers/set` | body `{"ids":[...]}` |
| POST | `/couplers/{id}/toggle` | één koppel aan/uit |
| POST | `/action/{code}` | setzer- en speciale acties (0, 1-9, SET, GC, M1-8, Cresc …) |
| POST | `/panic` | alle noten stil |
| POST | `/import/preflight` | body `{"path":"..."}` — importverslag van een CODM-definitie zonder iets te laden of te vervangen |
| GET  | `/import/report` | verslag van de laatste load (formaat, pakketten, verwacht/geladen, overgeslagen registers, meldingen) |
| GET  | `/pistons` | schakelaargedrag uit de definitie: pistons, standaard-aan, buiten de combinaties (CODM) |
| POST | `/pistons/{index}/druk?aan=1\|0` | shortcutpiston indrukken (`aan=1`) of loslaten (`aan=0`) |
| POST | `/settings/codm_random?on=1\|0` | willekeurige stemafwijking per pijp (CODM) aan/uit, geldt bij het volgende laden |

### Noten en MIDI

| | | |
|---|---|---|
| POST | `/notes/{n}/on?velocity=V` | NoteOn via de ingetekende route |
| POST | `/notes/{n}/off` | NoteOff |
| POST | `/midi/inject?note=N&on=1&velocity=V&channel=C` | door de echte MIDI-lus |
| POST | `/midi/inject?cc=N&value=V&channel=C` | ControlChange (zwel, crescendo) |
| POST | `/midi/mapping` | kanaal/bereik/transpose per divisie |

### Audio en instellingen

| | | |
|---|---|---|
| POST | `/master?db=-6.0` | hoofdvolume |
| POST | `/motor?on=1` | motor aan/uit |
| POST | `/polyphony?limit=16000` | polyfonie-grens |
| POST | `/preload_ms?ms=500` | startbuffer per opname |
| POST | `/audio_output` | audio-driver en apparaat kiezen |
| POST | `/reverb` | galm-instellingen |
| GET  | `/division_channels` | uitgangskanalen per klavier |
| POST | `/division_channels` | zelfde, zetten |
| GET  | `/division_volumes` | volume per divisie |
| POST | `/swell` | zweltrede live-waarde |

### Opname

| | | |
|---|---|---|
| POST | `/record/start` | MP3 starten |
| POST | `/record/stop` | MP3 stoppen |
| POST | `/midi/record/start` | MIDI-opname starten |
| GET  | `/midi/record/status` | opname-status |
| POST | `/midi/record/stop` | opname stoppen |
| POST | `/midi/record/save` | opname wegschrijven |

### Afstandsbediening (geen loopback-vereiste)

| | | |
|---|---|---|
| GET  | `/remote/status` | adressen + QR-token |
| POST | `/remote/enable?on=1` | aan/uit |
| GET  | `/remote/layout` | huidige UI-layout voor tablet |
| POST | `/remote/layout` | layout bijwerken |
| POST | `/remote/new_token` | token roteren |

### Overige

| | | |
|---|---|---|
| POST | `/debug/restart_needed` | audio-herstart afdwingen |
| POST | `/sampleset/scan` | mapstructuur in kaart brengen |
| POST | `/sampleset/trim` | opname afsnijden |
| POST | `/sampleset/loop_scan` | loop-punten voorstellen |
| POST | `/sampleset/loop_apply` | voorstel toepassen |
| POST | `/stereo_samples?on=1` | stereo-opnamen laden |

## Antwoorden

### Orgelbouwer (alleen lokale test-API)

- `POST /project/source` met `{ "path": "C:/sets/bron.organ", "package_root": null }`: broncatalogus en vingerafdruk, zonder het actieve orgel te vervangen.
- `POST /project/read` met `{ "path": "C:/projecten/mijn.jmorgan" }`: project met opgeloste bronpaden.
- `POST /project/save` met `{ "path": "C:/projecten/mijn.jmorgan", "project": { ... } }`: eerst bronnen en alle geselecteerde audio valideren, dan veilig opslaan. Manifesten mogen op deze route tot 16 MiB beslaan.
- `POST /load_organ` met een absoluut `.jmorgan`-pad gebruikt dezelfde projectcompiler en publiceert één orgelbundel. Projectregisters hebben stabiele DTO-ID's `project_<id>`.

`project.sources` bevat één tot 64 bronnen met ieder een eigen UUID, bronpad,
pakketmap, stemming en eventuele opnamepositie. Het schema blijft versie 1;
bestaande projecten met twee bronnen vereisen geen migratie. De gezamenlijke
budgetten blijven 2.048 registers, 16 gebruikte rankzwelkasten, 256 actieve
windcompartimenten, 4.096 windverbindingen en 4.096 windcontrollers. Een 65e bron
wordt bij projectvalidatie geweigerd; de bronlimiet verruimt de andere grenzen niet.

De test `testscripts/organ_project_background_test.py` start eigen headless processen
met een apart profiel en synthetische bronorgels. Hij gebruikt geen vensters of
productie-instellingen. Frequentiemetingen lopen via echte audio-opnamen.

Uitgebreide achtergrondproeven:

- `organ_project_advanced_background_test.py`: twee rankzwelkasten op hetzelfde manuaal, onafhankelijke audiometing, herstart, 2.048 registers en een volledige console.
- `organ_project_wind_background_test.py`: twee bronwindnetwerken met overlappende bron-ID's, drukverbruik en herstel, herstart en wissen bij een normaal orgel.
- `organ_project_valve_background_test.py`: dynamische schakel- en regelkleppen, omkering en beginstanden, bronisolatie, registerkoppeling, veilige opslag en herladen in eigen headless processen.
- `organ_project_feedback_background_test.py`: mechanische balgextensie en drukfeedback, een Willis/inertia-keten, drempelschakelaar, bronisolatie en herstart.
- `organ_project_multisource_background_test.py`: reproduceerbare proef met drie en daarna vier bronnen in dezelfde map, eigen pakketmappen, overlappende native ID's, afzonderlijke audiostemming en windverbruik, bronvolgorde wijzigen, bron vervangen, herstart en afwijzing van 65 bronnen. Deze procedure start eigen headless appprocessen en gebruikt een eigen profiel.
- `organ_project_multisource_ui_test.mjs`: reproduceerbare componentproef met de echte Svelte-interface en gesimuleerde IPC. Controleert het begin met één lege bronkaart, toevoegen van bronnen, afzonderlijke pakketmap/stemming, keuzelijst vanaf vijf bronnen, labels na Z, 64/65-grens en compacte weergave. De componentproef valideert geen native audio; daarvoor dient de achtergrondproef.
- `GET /project/wind`: actieve bronplanmetadata (`nodes`, `links`, `controls`, `pipe_count`, `warnings`) en audiometingen: `pressure_ratios` per knooppunt, `control_values` per controller, `valve_openings` en `link_flows` per verbinding. De arrays volgen de volgorde van het plan. `control_values` en `valve_openings` meten de actuele overgang; `link_flows` is de massastroom in kg/s. Per knooppunt zijn ook `bellows_extensions` (slagfractie 0..1), `bellows_positions` (meter) en `bellows_velocities` (m/s) beschikbaar. `control_tick_count` en `model_time_seconds` geven de voortgang van de audiomodeltijd; `source_model_requested` geeft de ingestelde bronwindvlag aan.
- `GET /project/wind/controls`: controllerdefinities en waarden in `values`, met de stabiele controller-ID als sleutel. Zelfstandige ingangen geven hun ingestelde doel; gekoppelde schakelaars zijn aan zolang minstens één `project_stop_ids`-register getrokken is. Controllers met `automatic: true` tonen de actuele berekende waarde uit de audiokern en zijn alleen uitleesbaar.
- `POST /settings/import` met `{ "path": "C:/testprofiel/export.json", "organ_id": "C:/projecten/mijn.jmorgan" }`: roept dezelfde instellingenimport aan als de desktopinterface. Zelfstandige klepstanden worden gevalideerd en meteen in de audiokern hersteld; onbekende, automatische of registergekoppelde opgeslagen klepwaarden worden verwijderd. Dit is uitsluitend een lokaal test-endpoint, buiten de netwerkwhitelist.
- `POST /project/wind/control` met `{ "organ_id": "C:/projecten/mijn.jmorgan", "control_id": "<bron-UUID>:continuous:9", "value": 0.5 }`: zelfstandige klepstand onmiddellijk per project bewaren en daarna naar audio sturen. `organ_id` is optioneel in de lokale test-API; weglaten gebruikt het geladen orgel. `value` moet eindig zijn en binnen 0..1 liggen. Een schakelaar gebruikt de grens 0.5; een continue regelaar behoudt de waarde. Onbekende controllers, verkeerde orgelidentiteiten, ongeldige waarden en handmatig bedienen van een automatische of registergekoppelde controller geven HTTP 400. Een mislukte lokale opslag laat de vorige stand behouden; een mislukte kopie naast het project geeft bij succes een `warning`.
- `POST /project/wind/enabled` met `{ "enabled": false }`: tijdelijke testschakelaar voor bronwind, zonder projectinstellingen te bewaren.
- `GET`/`POST /test/preset_bindings` met `{ "bindings": [...] }` en `GET /test/preset_triggers`: controle van brede MIDI-acties in het eigen testproces. Bedoeld voor achtergrondtests; deze routes staan niet op de afstandsbediening.

Controller-ID's hebben de vorm `<bron-UUID>:switch:<bron-ID>` of
`<bron-UUID>:continuous:<bron-ID>`; gelijke numerieke ID's uit twee bronnen of
controllertypen blijven onafhankelijk. Beginstand en schakelomkering komen uit
de Hauptwerk-definitie. De importer bewaart de volledige afhankelijke keten,
inclusief voorwaarden, verborgen bronconstanten en fysieke uitgangen. Ontbrekende
beginstanden mogen alleen van een bekende producent worden afgeleid; ontbrekende
vrije ingangen worden geweigerd. Verborgen bronconstanten met expliciet
`AccessibleForInput=N` en `Clickable=N` blijven alleen uitleesbaar.

De expliciete JM-benadering, zonder claim van identieke Hauptwerk-semantiek:

- Continue overdracht: `u = 127 * bronwaarde`; bij inversie `u = 127 - u`;
  daarna `doel = clamp((max(u + increment, 0) * coefficient)^index, 0, 127)`.
  De coëfficiënt is niet negatief en de macht is positief. Het eindresultaat
  wordt weer genormaliseerd naar 0..1.
- Willis (linktype 2): `dx/dt = (doel - x) / tau`, met `tau` in seconden gelijk
  aan `WillisTypeIncSpeedInMillisecondsPerStepWithOneStepDiff / 1000`.
  Binnen een halve ruwe controllerstap wordt het eindpunt exact bereikt.
- Inertia (linktype 3): `x'' = acceleration * (doel - x) - damping * x'`, met
  tijd in seconden. Zowel `InertiaModelType...` als `InertialModelType...`
  veldnamen worden gelezen; tegenstrijdige waarden worden geweigerd.
- Directe continue ketens volgen een vooraf bepaalde volgorde; voorwaarden en
  schakellussen lezen de vorige rekentik. Drempelschakelaars vergelijken de op
  hele 0..127-stappen afgeronde waarde en letten op de bewegingsrichting.
  Schakelacties 1/2 betekenen aan/uit; code 7 houdt in dit JM-model de stand vast.
- Sensorwaarden zijn de begrensde balgslagfractie, druk gedeeld door nominale
  druk, of doorstroming gedeeld door de opgegeven maximale stroom. Balgen
  gebruiken een mechanische positie en snelheid met begrensde slag. Bij
  vormcode 2 en bewegingscode 1 gebruikt de JM-wigbenadering de helft van het
  grondoppervlak als volume-oppervlak en de opgegeven zwaartepuntafstand als
  hefboom. Ontbrekende vormcodes gebruiken de rechthoekige JM-benadering.

Handbediende kleppen hebben een overgang van 10 ms; de klank valt onder 60%
van de nominale bronlagedruk geleidelijk weg. `ContinuousControlDoubleLinkage`,
balgveren, gedwongen balgstanden, onbekende geometrie/actiecodes en zuiver
directe continue lussen zijn niet ondersteund. Die en onvolledige gegevens
blokkeren laden vóór vervanging van het actieve orgel. Broninspectie vermeldt
de reden in `warnings` en waar van toepassing `valve_controls[].issues`.
Bij uitgeschakeld **Windwerk van de set** blijven de klepinstellingen beschikbaar,
maar vraagt activeren een herlaad.

Een geselecteerd register met een bronschakelaar die tevens door een
automatiseringsproducent wordt aangestuurd, wordt vóór laden geweigerd. Die
combinatie heeft nog geen ondersteunde koppeling met projectregistratie; de
compiler laat de registerbinding niet stilzwijgend weg.

De meerbronnenproeven zijn vanuit `VirtualPipeOrgan` als volgt te herhalen.
Vervang `C:/werk/JM-Orgue` door de checkout, geef de te testen appbinary op en
gebruik een nieuwe werkmap onder `testen/orgelbouwer-meerbronnen-2026-11-26`:

```powershell
py testscripts/organ_project_multisource_background_test.py --exe "C:/werk/JM-Orgue/VirtualPipeOrgan/target/debug/vpo-app.exe" --work-dir "C:/werk/JM-Orgue/testen/orgelbouwer-meerbronnen-2026-11-26/run-1"
```

Met `--prepare-only` maakt die proef alleen de bronfixtures. Voor de
componentproef moeten de UI-afhankelijkheden, Chrome en een Playwright-module
beschikbaar zijn. Vervang ook het modulepad in dit voorbeeld:

```powershell
node testscripts/organ_project_multisource_ui_test.mjs --playwright "C:/tools/playwright/index.mjs" --chrome "C:/Program Files/Google/Chrome/Application/chrome.exe" --output "C:/werk/JM-Orgue/testen/orgelbouwer-meerbronnen-2026-11-26/ui"
```

De achtergrondproef schrijft `multisource-result.json` en proceslogboeken; de
componentproef schrijft `result.json` en de beelden `multisource-wide.png` en
`multisource-narrow.png`.

Consoleontwerpen mogen 64 pagina's en in totaal 4.096 onderdelen bevatten. De
lokale `/settings/console_layout`-route accepteert daarom maximaal 8 MiB; de
afstandsbediening behoudt haar bestaande verzoeklimiet.

Alle antwoorden zijn JSON. Fouten komen met een HTTP-status (400 slechte
invoer, 404 pad onbekend, 409 geen orgel geladen, 500 onverwacht) en een
`{"error":"..."}` body.

## Niet-ingangen

- Netwerkverkeer naar andere hosts — alleen loopback (en `/remote` voor de
  afstandsbediening, met wegwerp-token).
- Encryptie — bedoeld voor localhost; voor externe toegang zet er een
  reverse proxy (nginx, Caddy) met TLS vóór.
- Authenticatie op de gewone endpoints — wie bij localhost kan is al in.
  `/remote` heeft wél een token om de telefoon-koppeling te beschermen.

## Voorbeeld: snelle rooktest in Python

```python
import json, time, urllib.request as u
def post(p, body=None):
    data = json.dumps(body).encode() if body else b""
    r = u.Request("http://127.0.0.1:8765" + p, data=data, method="POST",
                  headers={"Content-Type": "application/json"})
    return json.loads(u.urlopen(r, timeout=60).read() or b"{}")
def get(p): return json.loads(u.urlopen("http://127.0.0.1:8765" + p).read())

print(get("/health"))
post("/load_organ", {"path": r"C:\sets\mijn.organ"})
while not get("/status")["organ_loaded"]: time.sleep(0.5)
stops = [s["id"] for d in get("/organ")["divisions"] for s in d["stops"][:1]]
post("/stops/set", {"ids": stops})
post("/notes/60/on?velocity=100"); time.sleep(1); post("/notes/60/off")
```
