# Building JM-Orgue

> Voor developers / contributors. Eindgebruikers gebruiken de installer.

## Prerequisites

### Windows
- [Rust](https://rustup.rs/) (1.70+) — `rustup-init.exe`
- [Node.js](https://nodejs.org/) (18+) — voor de Svelte frontend
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) — meegeleverd met Windows 10/11
- Visual Studio Build Tools 2022 (C++ build tools)

### macOS
- Rust + Node.js
- Xcode Command Line Tools (`xcode-select --install`)

### Linux
- Rust + Node.js
- `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libdbus-1-dev pkg-config libssl-dev`

## Workspace structuur

```
JM-Orgue/
├── vpo-core/           # Data types, organ definitions
├── vpo-audio/          # Audio engine: sample playback, mixing, effects
│   ├── effects.rs      # Tremulant, WindModel, Filters, ParametricEq
│   ├── convolution.rs  # FFT-partitioned convolution reverb
│   ├── reverb_fdn.rs   # FDN algorithmic reverb
│   └── voice.rs        # Voice management
├── vpo-midi/           # MIDI input handling
│   ├── device.rs       # midir USB MIDI
│   └── ble.rs          # btleplug BLE MIDI
├── vpo-sampler/        # Sample loading, ODF parsing
│   ├── loader.rs       # WAV/MP3 loading, loop points
│   ├── grandorgue.rs   # ODF parser
│   └── custom_organ.rs # Sample-folder scanner
├── vpo-plugin/         # VST3/CLAP plugin (MVP, zie vpo-plugin/README.md)
├── src-tauri/          # Tauri app backend
│   └── src/
│       ├── main.rs
│       ├── audio.rs    # Audio thread, voice mixing
│       ├── state.rs    # AppState, MIDI thread
│       ├── commands.rs # Tauri IPC commands
│       ├── library.rs  # Persistent organ library
│       ├── recorder.rs # MP3-opname van de live audio-output
│       ├── silence.rs  # Sampleset-tool: leading-silence trim
│       ├── loop_tool.rs # Sampleset-tool: loop points + crossfade (smpl chunk)
│       └── test_api.rs # HTTP test-API (--test-api, poort 8765)
└── ui/                 # Svelte 4 + Vite frontend
    └── src/
        ├── App.svelte
        ├── components/
        │   ├── Console.svelte    # Main console + Settings
        │   ├── Header.svelte     # Top bar + tabs
        │   ├── SetzerBar.svelte  # Setzer + crescendo indicator
        │   ├── PanelApp.svelte       # Extra registerscherm (secundaire App-shell)
        │   └── NotationWindow.svelte # Notatievenster (MIDI-opname → bladmuziek)
        ├── temperaments.js  # 52 historical temperaments
        └── styles.css
```

## Build

### Frontend
```bash
cd ui
npm install
npx.cmd vite build
```

### Backend (release)
```bash
cargo build --release
```

Output: `target/release/vpo-app.exe`

### Combined (Tauri bundle)
```bash
cargo tauri build
```

Output: `target/release/bundle/` (`<versie>` = workspace-versie uit `Cargo.toml`)
- Windows: `JM-Orgue_<versie>_x64-setup.exe` + `JM-Orgue_<versie>_x64_en-US.msi`
- macOS: `JM-Orgue.app` + `JM-Orgue_<versie>_x64.dmg`
- Linux: `jm-orgue_<versie>_amd64.deb` + `JM-Orgue_<versie>_amd64.AppImage`

### ASIO (optioneel, lage latency op Windows)

WASAPI shared-mode (de standaard) geeft ~20-40 ms uitvoer-latency op onboard
audio. Voor ~5-12 ms gebruik je een ASIO-driver. ASIO wordt **in de binary
gecompileerd** via cpal's `asio`-feature en is daarom standaard UIT. Eenmalige
setup om een ASIO-build te maken:

1. **libclang installeren** (voor bindgen) — geen admin nodig via pip:
   ```bash
   py -3 -m pip install libclang
   # libclang.dll staat dan in: <python>\Lib\site-packages\clang\native
   ```
   Bij het bouwen `LIBCLANG_PATH` naar die map laten wijzen (zie stap 2).
   (Alternatief: `winget install LLVM.LLVM` → `LIBCLANG_PATH=C:\Program Files\LLVM\bin`.)
2. **Bouwen met de feature** — de Steinberg ASIO SDK wordt door `asio-sys`
   automatisch gedownload (geen `CPAL_ASIO_DIR` nodig):
   ```bash
   LIBCLANG_PATH='C:\Users\<jij>\AppData\Local\Programs\Python\Python314\Lib\site-packages\clang\native' \
     cargo build --release -p vpo-app --features asio
   # of een bundle:
   cargo tauri build -- --features asio
   ```
   (Wil je de SDK liever zelf beheren: download van https://www.steinberg.net/developers/
   en zet `CPAL_ASIO_DIR` naar de uitgepakte map.)
3. **Driver**: installeer [ASIO4ALL](https://www.asio4all.org/) (wrapt de
   onboard Realtek) of gebruik een audio-interface met eigen ASIO-driver.
   Zonder ASIO-driver verschijnt "ASIO" wel in de lijst maar zonder apparaten.
4. In de app: **Algemene Instellingen → Audio Uitvoer** → kies host **ASIO**,
   je apparaat, en een buffergrootte (bv. 128/256). Klik **Toepassen**.

Zonder de `asio`-feature toont de host-keuze alleen "WASAPI" en blijft alles
werken zoals voorheen.

#### Problemen met ASIO4ALL

**Symptoom:** de stream start zonder fout maar blijft stil. In het log zie je
dan: `Audio-watchdog: callbacks GESTOPT`.

**Oorzaak:** ASIO4ALL levert bij een kóúde start (direct bij het opstarten van
de app) alleen een korte callback-burst en valt dan stil; dezelfde wissel werkt
wél betrouwbaar op een volledig draaiende app. Bij een opgeslagen ASIO-voorkeur
start de app daarom eerst op WASAPI en voert hij ±10 s na de start automatisch
de ASIO-wissel uit (het orgel wordt daarbij opnieuw geladen). In het log:
`Uitgestelde ASIO-wissel wordt uitgevoerd`.

**Werkt het dan nog niet:**
- sluit andere audio-apps;
- zet het apparaat online in het ASIO4ALL-configuratiescherm;
- of gebruik een audio-interface met eigen ASIO-driver (betrouwbaarste route).

## Release + automatische update (0.7.40)

### Routine voor een nieuwe versie

1. versie bumpen (`Cargo.toml` + `src-tauri/tauri.conf.json`), committen, pushen
2. `gh release create v<versie>` (een **gewone** release — een pre-release of
   concept komt niet onder `releases/latest/` te staan en wordt door de updater
   dus nooit gezien)
3. `.github/workflows/release-build.yml` bouwt Windows (NSIS + MSI, mét ASIO) en
   macOS (universal dmg), ondertekent de updater-artefacten en hangt alles aan
   de release
4. de derde taak `publish-updater-json` stelt `latest.json` samen en hangt die er
   als **laatste** bij — dat bestand is het startsein voor de updater in de app

> **Eerste keer**: 0.7.40 is de eerste versie mét updater aan boord. Wie 0.7.39
> of ouder draait, moet 0.7.40 één keer handmatig installeren; pas vanaf de
> versie daarná werkt de knop **Nu bijwerken**. Test de keten dus met een echte
> testrelease, niet met een lokaal gebouwde exe — de updater leidt het
> installatiepad af uit de locatie van de draaiende exe en werkt alleen
> betrouwbaar vanuit een echt geïnstalleerde versie.

### Hoe de automatische update werkt

De app (`ui/src/lib/updater.js`) haalt bij het starten en via de knop
**Controleer op updates** één bestand op:

```
https://github.com/orgelmaker/JM-Orgue/releases/latest/download/latest.json
```

Staat daar een nieuwere versie in, dan verschijnt de balk met **Nu bijwerken**.
Die knop downloadt de installer, controleert de handtekening, slaat de
instellingen op, sluit de audio netjes af (`prepare_for_update`) en start de
NSIS-setup. De setup draait **stil** (`plugins.updater.windows.installMode:
"quiet"` → `/S /UPDATE /R`) en start JM-Orgue daarna zelf weer op.

Waarom stil en niet `passive`: in de NSIS-sjabloon is passive géén silent. De
taalkeuze-dialoog (`displayLanguageSelector`) en de ASIO4ALL-vraag uit
`installer-hooks.nsh` zouden dan midden in een automatische update verschijnen,
terwijl de app al is afgesloten. In silent mode worden beide automatisch
overgeslagen (`/SD IDNO`). De installer is een *currentUser*-installatie, dus
er komt geen UAC-prompt aan te pas.

**Geen periodieke controle** — bewust: alleen bij het starten en via de knop.
Een melding die tijdens een dienst in beeld ploft, stoort.

**macOS** krijgt géén automatische update: de .app is niet ondertekend en niet
genotariseerd (geen Apple Developer-account). Daar blijft het bij de melding met
een knop naar de downloadpagina. `latest.json` bevat de macOS-sleutels wel
(`darwin-aarch64` én `darwin-x86_64` — `darwin-universal` bestáát niet in de
Tauri-2-updater) voor als dat ooit verandert.

### Ondertekening (minisign)

Dit staat los van Windows-codesigning en Apple-notarisatie: het is een eigen
sleutelpaar waarmee de app controleert dat een update écht van ons komt.

- **Publieke sleutel**: staat als tekst in `src-tauri/tauri.conf.json` onder
  `plugins.updater.pubkey` en zit dus in elke gebouwde binary.
- **Privésleutel**: `C:\Users\<jij>\.tauri\jm-orgue-updater.key` (+ `.password.txt`),
  en als GitHub-secrets `TAURI_SIGNING_PRIVATE_KEY` en
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` op de repo.
- De CI zet die twee als `env:` op de `tauri build`-stap; samen met
  `bundle.createUpdaterArtifacts: true` (alleen in `tauri.ci.conf.json`, zodat
  een lokale build zonder sleutel gewoon blijft werken) levert dat naast elke
  installer een `.sig`-bestand op.
- De client downloadt de update, controleert de handtekening en installeert pas
  daarna. Klopt de handtekening niet, dan gebeurt er **niets**.

Sleutelpaar (opnieuw) maken en de secrets zetten:

```bash
npx @tauri-apps/cli signer generate -w "$HOME/.tauri/jm-orgue-updater.key"
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo orgelmaker/JM-Orgue < "$HOME/.tauri/jm-orgue-updater.key"
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo orgelmaker/JM-Orgue < "$HOME/.tauri/jm-orgue-updater.password.txt"
```

### Sleutel kwijt — wat dan?

**Bewaar de privésleutel ook buiten deze pc** (bijvoorbeeld in een
wachtwoordkluis), het wachtwoord apart. Raakt hij kwijt, dan:

- accepteert **geen enkele al geïnstalleerde JM-Orgue** ooit nog een update:
  die apparaten controleren tegen de publieke sleutel die in hún binary zit;
- is de enige uitweg: een nieuw sleutelpaar maken, de nieuwe publieke sleutel in
  `tauri.conf.json` zetten, de secrets vervangen en een nieuwe versie
  uitbrengen die iedereen **handmatig** installeert. Vanaf die versie werkt de
  knop weer.
- Een oude sleutel is niet te "herstellen" en niet te vervangen door een
  server-side truc: de controle gebeurt op het apparaat van de gebruiker.

### Als `latest.json` ontbreekt of niet meer klopt

Is de derde taak niet meegelopen (dat gebeurt weleens bij *Re-run failed jobs*),
dan staan de installers er wel maar ziet de app geen update. Herstel met één
klik: **Actions → updater-json → Run workflow** met de tag, bijvoorbeeld
`v0.7.40`. Dat draait hetzelfde script (`.github/scripts/build-latest-json.sh`)
op de bestanden die al aan de release hangen.

**Ook na een losse herstart van `build-windows`** (*Re-run failed jobs* of één
taak opnieuw draaien) moet `updater-json` daarna opnieuw gedraaid worden. De
installer wordt dan namelijk opnieuw gebouwd en krijgt een nieuwe `.sig`,
terwijl de handtekening die in de al bestaande `latest.json` staat nog bij de
vorige installer hoort. De app downloadt dan de nieuwe installer, keurt de
handtekening af en installeert niets — bij iedereen "Bijwerken mislukt", tot
`latest.json` opnieuw is samengesteld. Vuistregel: is er ná het publiceren van
`latest.json` nog een installer opnieuw gebouwd, draai `updater-json`.

**Run geannuleerd?** Dan blijft `latest.json` achterwege (de derde taak draait
alleen als de run niet is afgebroken — `!cancelled()`, bewust geen `always()`).
Installers die al aan de release hangen zijn zonder `latest.json` onzichtbaar
voor de updater; wil je de release alsnog uitbrengen, draai dan `updater-json`
met de hand.

## Test-API (--test-api)

Alleen voor testen/automatisering. Start de app met de vlag `--test-api`
(optioneel `--test-api=PORT`); er draait dan een lokale HTTP-API op
`127.0.0.1`, standaard **poort 8765**. Endpoints o.a. `/status`, `/organ`,
`/stops/drawn`, `/stops/{id}/toggle`, `/notes/{noot}/on|off`, `/panic`,
`/logs`, `/load_organ` — zie `src-tauri/src/test_api.rs` voor de volledige
lijst. Niet bedoeld voor eindgebruikers; zonder de vlag draait er geen server.

## Run dev mode
```bash
cargo tauri dev
```

Hot-reload UI on every save.

## Logging

Default log location: `%APPDATA%\nl.jm-orgue.app\jm-orgue.log` (Windows)

Override log level with `RUST_LOG`:
```bash
$env:RUST_LOG="debug,vpo_midi=trace"
.\target\release\vpo-app.exe
```

## Known issues

1. **Linker crash** (STATUS_ACCESS_VIOLATION on rustc): clean and rebuild
   ```bash
   rm target/release/deps/vpo_app*
   cargo build --release
   ```

2. **Audio device locked** when app crashes: kill process via Task Manager.

3. **BLE MIDI HRESULT 0x80000013** ("object closed"): retry connect, app handles automatically with 3 attempts.

## Test workflow

1. Build & run
2. Load test organ from `C:\Bronbestanden\JM-Orgue\testen\JM-Orgue testorgel`
3. Test stops (klik registers)
4. Test MIDI keyboard (USB)
5. Test BLE (scan + verbind ESP32 of WIDI)
6. Test Crescendo learn + speel
7. Test temperament wisseling
8. Test export/import settings

## Dependencies overzicht

Belangrijkste crates:
- `tauri 2.x` — app framework
- `cpal 0.15` — cross-platform audio
- `midir 0.9` — cross-platform MIDI
- `btleplug 0.11` — cross-platform BLE
- `tokio` — async runtime (BLE worker)
- `parking_lot` — fast locks
- `crossbeam-channel` — threading
- `rubato` — resampling
- `realfft` — FFT for convolution
- `symphonia` — audio decoding (MP3)
- `dirs 5` — platform paths
