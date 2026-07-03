# JM-Orgue - Stand van Zaken

**Versie:** 0.1.0
**Datum:** 20 februari 2026
**Build:** `target\release\vpo-app.exe`

---

## Projectoverzicht

JM-Orgue is een virtuele pijporgel-applicatie gebouwd met Rust (backend) en Svelte (frontend), via Tauri 2.x als desktop framework. Het laadt orgel-samplesets en speelt deze af via MIDI-keyboard of computertoetsenbord.

---

## Architectuur

### Rust Workspace Crates

| Crate | Functie |
|-------|---------|
| `vpo-core` | Kernstructuren: orgel, stops, ranks, keyboards, temperamenten |
| `vpo-audio` | Audio-engine: cpal output, voice pool, mixer, reverb, effecten |
| `vpo-midi` | MIDI-invoer: device detectie, routing, channel mapping |
| `vpo-sampler` | Sample loading: WAV/MP3, preload systeem, GrandOrgue ODF, custom orgel scanner |
| `src-tauri` | Tauri app: commands, state management, audio bridge |

### Frontend (Svelte)

| Component | Functie |
|-----------|---------|
| `App.svelte` | Hoofdcomponent, keyboard handler, state coordinatie |
| `Header.svelte` | Titelbalk, audio start/stop, orgel naam |
| `Sidebar.svelte` | Audio/MIDI device selectie, volume, reverb |
| `Console.svelte` | Orgelbrowser, stop-knoppen, MIDI mapping, tremulant |
| `StatusBar.svelte` | Status indicators, foutmeldingen |
| `LayoutSettings.svelte` | Kleurthema's (Witte, Strumpler, Muller, Cavaille-Coll) |
| `PresetBar.svelte` | Preset knoppen (momenteel ongebruikt) |

---

## Gerealiseerde Functies

### Audio
- Preload-systeem: attack-gedeelte (~200ms) direct geladen, volledige samples op achtergrond
- Automatische resampling naar output sample rate
- Master volume regeling (dB)
- Convolutie reverb
- MP3 + WAV ondersteuning (via symphonia + hound)

### MIDI
- Automatische device detectie
- Per-divisie channel mapping met transpose
- Learn-functie: druk laagste en hoogste toets om range te bepalen
- Computertoetsenbord als fallback (Z-M = onderste octaaf, Q-I = bovenste octaaf)

### Orgel laden
- GrandOrgue ODF-bestanden (.organ)
- Mapstructuur scannen (JM-Rec compatibel):
  - `OrgelNaam/Divisie/Stop/036-c.mp3`
  - Automatische pitch-detectie uit bestandsnaam
  - Bass/Discant register herkenning
- Tremulant: `_trem` / `_TR` suffix mappen worden herkend
- Multi-mic: submappen per microfoonpositie (eerste submap wordt gebruikt)

### UI
- Donker thema met orgelpijp-icoon (JM-Rec stijl)
- Stop-knoppen per divisie
- Tremulant schakelaar per divisie (wanneer `_trem` samples aanwezig)
- Kleurpresets gebaseerd op historische orgelbouwers:
  1. Witte - Licht Hollands eiken
  2. Strumpler - Warm donker noten
  3. Muller - Goud en ebben barok
  4. Cavaille-Coll - Mahonie Frans romantisch
- Sidebar met device selectie (in/uitklapbaar)

---

## Mapstructuur Bronbestanden

```
C:\Bronbestanden\JM-Orgue\
  VirtualPipeOrgan\
    Cargo.toml              # Workspace root
    src-tauri\              # Tauri applicatie
      src\main.rs           # Entry point
      src\state.rs          # App state + organ loading
      src\audio.rs          # Audio thread + voice management
      src\commands.rs       # Tauri IPC commands
      icons\icon.ico        # App icoon
      tauri.conf.json       # Tauri configuratie
    ui\                     # Svelte frontend
      src\App.svelte
      src\styles.css
      src\components\       # UI componenten
    vpo-core\               # Kern datastructuren
    vpo-audio\              # Audio engine
    vpo-midi\               # MIDI verwerking
    vpo-sampler\            # Sample loader
    target\release\         # Build output
      vpo-app.exe           # Uitvoerbaar bestand
  testen\                   # Test samplesets
  create_icon.py            # Icoon generator script
```

---

## Compatibiliteit met JM-Rec

JM-Rec output formaat wordt volledig ondersteund:
- Mapstructuur: `OrgelNaam/Keyboard/Register/036-c.mp3`
- Tremulant varianten: `Register_trem/` naast `Register/`
- Multi-mic opnames: submappen per microfoon binnen stop-map
- GrandOrgue-compatibele naamgeving: `{MIDI_nummer}-{nootnaam}.mp3`

---

## Bekende Issues / TODO

- Bundle config: `"active": false` - nog geen installer/MSI
- Identifier waarschuwing: `nl.jm-orgue.app` eindigt op `.app` (macOS conflict)
- Diverse ongebruikte imports in Rust code (compiler warnings)
- PresetBar component ongebruikt
- Streaming playback (voor zeer grote samples) is geimplementeerd maar niet actief

---

## Snelkoppelingen

- **Bureaublad:** `JM-Orgue.lnk` -> `target\release\vpo-app.exe`
- **Icoon:** `src-tauri\icons\icon.ico` (orgelpijpen + speaker, JM-Rec stijl)
