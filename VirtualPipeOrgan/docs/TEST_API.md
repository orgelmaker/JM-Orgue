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
