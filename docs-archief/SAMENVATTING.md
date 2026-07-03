# JM-Orgue Project Samenvatting

**Datum:** 30 januari 2026
**Locatie:** C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan

## Projectdoel

Herbouwen van Sweelinq Virtual Pipe Organ software als open-source alternatief met moderne technologie stack.

## Gekozen Technologie

- **Backend:** Rust (performance, memory safety)
- **Frontend:** Tauri + Svelte (lichtgewicht desktop app)
- **Audio:** cpal, realfft (convolutie reverb), rubato (resampling)
- **MIDI:** midir, midly

## Projectstructuur

```
VirtualPipeOrgan/
├── Cargo.toml              # Workspace configuratie
├── README.md               # Project documentatie
│
├── vpo-core/               # Core datastructuren
│   └── src/
│       ├── lib.rs          # Type aliases, MIDI-to-freq conversie
│       ├── error.rs        # Error types (thiserror)
│       ├── organ.rs        # Organ, Division, Tremulant, WindModel, etc.
│       ├── stop.rs         # Stop, StopFamily, StopRank, NoteRange
│       ├── rank.rs         # Rank, Pipe, Release, PipeType, AttackType
│       ├── keyboard.rs     # Keyboard, KeyboardType, VelocityCurve
│       ├── coupler.rs      # Coupler, CouplerType (NL/DE/FR naamgeving)
│       └── temperament.rs  # Equal, Werkmeister III, Kirnberger III, Vallotti, etc.
│
├── vpo-audio/              # Audio engine
│   └── src/
│       ├── lib.rs          # Constants (BLOCK_SIZE=256, MAX_POLYPHONY=512)
│       ├── device.rs       # AudioDeviceManager (cpal)
│       ├── buffer.rs       # StereoSample, AudioBuffer, RingBuffer
│       ├── voice.rs        # Voice, VoiceState, VoicePool (voice stealing)
│       ├── mixer.rs        # ChannelStrip, Mixer (gain smoothing, peak meters)
│       ├── effects.rs      # LFO, Tremulant, WindModel, Biquad filters
│       ├── convolution.rs  # Partitioned FFT convolution reverb
│       └── engine.rs       # AudioEngine (command/event channels)
│
├── vpo-midi/               # MIDI handling
│   └── src/
│       ├── lib.rs
│       ├── device.rs       # MidiInputManager, MidiOutputManager
│       ├── message.rs      # MidiMessage parsing, CC constants
│       └── router.rs       # MidiRouter, OrganAction, MIDI mapping
│
├── vpo-sampler/            # Sample management
│   └── src/
│       ├── lib.rs
│       ├── loader.rs       # WAV loading, resampling, normalisatie
│       ├── cache.rs        # SampleCache (4GB default, LRU eviction)
│       ├── pool.rs         # SamplePool, OrganLoader
│       └── grandorgue.rs   # GrandOrgue ODF parser & loader (NIEUW)
│
├── src-tauri/              # Tauri desktop app
│   ├── Cargo.toml
│   ├── tauri.conf.json     # App configuratie
│   ├── build.rs
│   └── src/
│       ├── main.rs         # App setup, state initialisatie
│       ├── state.rs        # AppState (audio, midi, organ)
│       └── commands.rs     # Tauri IPC commands
│
└── ui/                     # Svelte frontend
    ├── package.json
    ├── vite.config.js
    ├── index.html
    └── src/
        ├── main.js
        ├── App.svelte
        ├── styles.css      # Dark theme, stop kleuren
        └── components/
            ├── Header.svelte    # Titel, audio controls, meters
            ├── Sidebar.svelte   # Device selectie, volume, pistons
            ├── Console.svelte   # Stop knoppen per divisie
            └── StatusBar.svelte # Status indicators
```

## Belangrijke Features Geïmplementeerd

### vpo-core
- Complete orgel datamodel (Organ → Division → Stop → Rank → Pipe)
- Stop families met kleurcodering (Principal, Flute, String, Reed, etc.)
- Historische temperamenten (6 presets)
- Koppel systeem (Unison, Octave, Sub/Super, Melody, Bass)
- Nederlandse/Duitse/Franse koppelnamen

### vpo-audio
- Real-time audio met cpal
- Voice pool met intelligent voice stealing
- Smooth parameter changes (geen clicks)
- Equal-power panning
- Soft clipping limiter
- Peak metering met hold
- Partitioned FFT convolution reverb
- Tremulant met amplitude en pitch modulatie
- Wind model simulatie

### vpo-midi
- MIDI device discovery en connectie
- Complete MIDI message parsing
- Flexible routing naar orgel acties
- Default channel mapping voor Nederlandse orgels

### vpo-sampler
- WAV file loading (8/16/24/32 bit)
- Automatic resampling naar target rate
- LRU cache met configureerbare grootte
- Parallel loading met rayon
- **GrandOrgue ODF Parser** (NIEUW):
  - Parst .organ bestanden (INI-formaat)
  - Ondersteunt: Organ, Manual, Stop, Pipe, Windchest, Enclosure, Tremulant, Coupler secties
  - Pipe reference resolution (REF:manual:stop:pipe syntax)
  - Automatische sample loading met progress callback
  - Getest met Bureå Funeral Chapel sampleset

### UI
- Donker thema
- Interactieve stop knoppen met visuele feedback
- VU meters
- Device selectie
- Volume/reverb sliders
- General pistons

## Nog Te Doen

1. ~~**Sample loading integratie**~~ ✅ GrandOrgue ODF loader gemaakt
2. ~~**Orgel definitie formaat**~~ ✅ GrandOrgue .organ formaat ondersteund
3. **.swop reverse engineering** - Of eigen sample formaat (optioneel)
4. **Complete voice triggering** - MIDI → Voice → Sample chain
5. **Multi-channel output** - Surround support
6. **Recording** - Audio export naar WAV
7. **Sample set editor** - GUI tool voor sample sets maken
8. ~~**Rust installeren**~~ ✅ Rust 1.93.0 + MSVC Build Tools geïnstalleerd

## Build Instructies

```powershell
cd C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan

# Prerequisites
# - Rust: https://rustup.rs/
# - Node.js 18+: https://nodejs.org/

# Install frontend dependencies
cd ui
npm install

# Development build
cd ..
cargo tauri dev

# Production build
cargo tauri build
```

## Dependencies (Cargo.toml)

| Crate | Versie | Doel |
|-------|--------|------|
| cpal | 0.15 | Audio I/O |
| realfft | 3.3 | FFT voor convolutie |
| rubato | 0.14 | Resampling |
| hound | 3.5 | WAV loading |
| midir | 0.9 | MIDI I/O |
| midly | 0.5 | MIDI parsing |
| tauri | 2.0 | Desktop framework |
| serde | 1.0 | Serialization |
| tokio | 1.35 | Async runtime |
| parking_lot | 0.12 | Fast mutexes |
| crossbeam-channel | 0.5 | Lock-free channels |
| rayon | 1.8 | Parallel iteration |

## Architectuur Beslissingen

1. **Rust workspace** - Modulaire structuur, makkelijk te testen
2. **Lock-free audio** - crossbeam channels voor thread-safe communicatie
3. **Voice stealing** - Prioriteit op basis van leeftijd en state
4. **Partitioned convolution** - Lage latency reverb
5. **Sample caching** - Voorkom disk I/O tijdens playback
6. **Tauri i.p.v. Electron** - Kleiner, sneller, minder RAM

## Sweelinq Analyse

Originele Sweelinq (ter referentie):
- Qt5/QML frontend
- Custom C++ audio engine  
- .swop: 4GB+ gecomprimeerde sample packs
- .swgs/.swos/.swcl: Binaire configuratie
- €119.88/jaar subscription

## Test Sampleset

Een gratis GrandOrgue sampleset is gedownload voor testen:

```
C:\Bronbestanden\JM-Orgue\testen\Burea_Funeral_Chapel\
├── burea_gravkapell.organ      # Originele versie (6 stops)
├── burea_gravkapell_ext.organ  # Uitgebreide versie
├── Gedackt8/                   # 8' Gedackt samples
├── Salicional8/                # 8' Salicional samples
├── VoixCeleste8/               # 8' Voix Celeste samples
├── Rorflojt4/                  # 4' Rörflöjt samples
├── Principal2/                 # 2' Principal samples
├── Subbas16/                   # 16' Subbas (pedaal)
└── PedGedackt8/                # 8' Pedaal Gedackt
```

Test de loader met:
```powershell
cd C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan
cargo run --example test_grandorgue
```

---

*Project gestart: 30 januari 2026*
*Laatste update: 30 januari 2026*
*Stack: Rust + Tauri + Svelte*
