# JM-Orgue VST3 / CLAP plugin

**Status:** MVP / proof-of-concept (v0.3.0)

Eerste werkende versie van het JM-Orgue audio-engine als VST3 / CLAP plugin. Plug in elke moderne DAW, ontvang MIDI, hoor orgel-achtig geluid.

## Wat werkt nu

- ✅ Laadt als **VST3** in Reaper, Cakewalk, Cubase, Studio One, Bitwig, FL Studio, etc.
- ✅ Laadt als **CLAP** in moderne DAWs (Bitwig, Reaper 7+)
- ✅ MIDI in (NoteOn / NoteOff)
- ✅ Polyfoon (max 128 stemmen)
- ✅ 6 instelbare parameters via DAW automation:
  - Master Gain (−60 … +6 dB)
  - Stop Mix (Prestant ↔ Holpijp blend)
  - Attack (5–500 ms)
  - Release (20–2000 ms)
  - Tremulant rate (2–10 Hz)
  - Tremulant depth (0–1)
- ✅ Stereo output
- ✅ Sample-accurate parameter automation

## Wat **nog niet** werkt (MVP-beperkingen)

| Beperking | Status |
|-----------|--------|
| Echte sample-set afspelen | ⏳ Synthese met sine-oscillatoren als platform-test |
| GUI / eigen UI in het plugin-venster | ⏳ Alleen generic-params view via DAW |
| Stop-keuze laden (`.organ` file) | ⏳ Eerst engine-extractie nodig |
| Aparte koppels / divisies | ⏳ Plugin is nu single-instrument |
| Reverb / EQ / wind model | ⏳ Komt uit gedeelde engine |

Zie de roadmap onderaan voor wat er nog moet voordat dit productie-kwaliteit is.

## Bouwen

```bash
cd VirtualPipeOrgan
cargo build --release -p vpo-plugin
./vpo-plugin/bundle.sh
```

Dit maakt:
- `target/bundled/JM-Orgue.vst3/` (Windows VST3 bundle-map)
- `target/bundled/JM-Orgue.clap` (los CLAP-bestand)

## Installeren

### VST3 (Windows)
Kopieer de **gehele map** `JM-Orgue.vst3/` naar:
```
C:\Program Files\Common Files\VST3\
```
Of voeg de bundle-map als VST3-search-path toe in je DAW.

### CLAP (Windows)
Kopieer `JM-Orgue.clap` naar:
```
C:\Program Files\Common Files\CLAP\
```

### macOS / Linux
- macOS VST3: `~/Library/Audio/Plug-Ins/VST3/JM-Orgue.vst3/`
- macOS CLAP: `~/Library/Audio/Plug-Ins/CLAP/JM-Orgue.clap`
- Linux VST3:  `~/.vst3/JM-Orgue.vst3/`
- Linux CLAP:  `~/.clap/JM-Orgue.clap`

(macOS/Linux builds vereisen `cargo build --target=...` met de juiste target — zie nih-plug docs.)

## Testen zonder DAW

De plugin heeft een **standalone executable** voor lokaal testen:

```bash
cargo build --release -p vpo-plugin --features standalone --bin vpo-plugin-standalone
./target/release/vpo-plugin-standalone
```

Dit opent een venster met audio + MIDI host. Sluit een MIDI-keyboard aan en speel.

## Architectuur

```
┌─────────────────────────────────────────────────┐
│  DAW Host (Reaper / Cubase / FL Studio / ...)   │
│  ┌─────────────────────────────────────────┐    │
│  │  JM-Orgue plugin (vpo_plugin.dll)        │    │
│  │  ─────────────────────                   │    │
│  │  • nih-plug runtime                      │    │
│  │  • JmOrgue::process()                    │    │
│  │     ├─ MIDI events parsen                │    │
│  │     ├─ Voice pool (max 128)              │    │
│  │     ├─ OrganVoice synthesis              │    │
│  │     └─ Tremulant + master gain           │    │
│  └─────────────────────────────────────────┘    │
└─────────────────────────────────────────────────┘
```

### Belangrijke verschillen t.o.v. de standalone JM-Orgue app

| Aspect | Standalone app (Tauri) | Plugin |
|--------|------------------------|--------|
| Audio thread | Eigen via cpal | DAW bepaalt |
| MIDI input | Eigen via midir + BLE | DAW geeft events per buffer |
| UI | Tauri webview (Svelte) | DAW-provided generic params |
| Sample loading | Async met preload-buffer | Niet geïmplementeerd nog |
| Reverb / EQ / wind | Tauri commando's | Niet aanwezig nog |
| State persistence | `.jm-settings.json` per orgel | Via DAW project state |

## Roadmap voor productie-kwaliteit

### Fase A — Engine extractie (2-4 weken)
Maak een nieuwe crate `vpo-engine` die de audio-DSP loskoppelt van threading:
- Pure `process_block(midi_events, params, output)` interface
- Geen `Arc<RwLock<...>>` — alles owned
- Geen `cpal` of `tokio` afhankelijkheden
- Wordt geïmporteerd door zowel `src-tauri/audio.rs` als `vpo-plugin/lib.rs`

### Fase B — Sample playback (3-4 weken)
- Een file-picker parameter in de plugin voor `.organ` pad
- Async sample loading met preload (vergelijkbaar met huidige Tauri-flow)
- Per-instance pijp-cache (geen state delen tussen DAW-tracks)

### Fase C — Stop-keuze UI (4-6 weken)
- Schakel van generic-params naar eigen UI met `nih_plug_iced` of `nih_plug_egui`
- Register-knoppen, divisie-selectors, koppels — vergelijkbaar look-and-feel met Tauri app
- Compacte modus (alleen registers zichtbaar) voor inplugin-real-estate

### Fase D — Volledige feature-pariteit (8-12 weken)
- Multi-channel output (4/6/8 kanalen)
- Reverb, EQ, wind model
- Tremulant samples (i.p.v. LFO)
- Per-pijp voicing
- Crescendo + setzer
- MIDI learn binnen DAW

### Fase E — Codesigning + distributie (1-2 weken)
- Code-signing certificaat (Windows + macOS)
- Installer (.msi voor Windows, .pkg voor macOS)
- VST3 + AU + CLAP bundles per platform
- DAW-compatibiliteitstests

**Totaal Fase A–E: ongeveer 5-7 maanden.**

## Bekende issues

1. **VST3 GUID gepind** — `JMOrgueVST3Plug2` is hardcoded. Bij toekomstige major versies kan deze veranderen, wat betekent dat oude DAW-projecten de plugin opnieuw moeten kiezen.

2. **Sample rate fixed in voice** — `OrganVoice::start_release()` neemt aan dat sample rate 48 kHz is. Op andere sample rates kan de release iets sneller/langzamer zijn. Wordt gefixt in Fase A.

3. **Geen state save/restore** — DAW slaat alleen parameter-values op. Wijzigingen aan stop-mix etc. kunnen worden opgeslagen, maar er is nog geen "geladen sampleset" om in te bewaren.

4. **Mono stop-mix** — `stop_mix` interpoleert lineair tussen Prestant en Holpijp. Echte orgels hebben aparte stops met onafhankelijke aan/uit-states.

## Licentie

Zelfde als hoofdproject: MIT.
