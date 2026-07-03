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
        │   └── RegisterPanel.svelte  # Extra panel window
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
dan: `Audio-watchdog: callbacks GESTOPT na init`.

**Oorzaak:** ASIO4ALL initialiseert na een koude start vaak niet goed. De app
doet daarom automatisch een korte WASAPI-warm-up wanneer ASIO de opgeslagen
host is.

**Werkt het dan nog niet:**
- sluit andere audio-apps;
- zet het apparaat online in het ASIO4ALL-configuratiescherm;
- of gebruik een audio-interface met eigen ASIO-driver (betrouwbaarste route).

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
