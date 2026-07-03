# JM-Orgue

> Een gratis, open-source virtueel pijporgel — door en voor organisten.

[![Versie](https://img.shields.io/badge/versie-0.5.0-blue.svg)]()
[![Status](https://img.shields.io/badge/status-pre--release-orange.svg)]()
[![Platform](https://img.shields.io/badge/platform-Windows-brightgreen.svg)]()
[![Licentie](https://img.shields.io/badge/licentie-Open-green.svg)]()

## 🎹 Wat is JM-Orgue?

JM-Orgue is een professionele virtuele pijporgel-applicatie die het opneemt tegen Hauptwerk, Sweelinq, GrandOrgue en Organteq — maar dan **gratis, open en in het Nederlands**.

> **Status: Sneak Preview**
> De grootste update tot nu toe is in de maak. Release later in 2026.

## ✨ Hoofdfeatures

### 🔊 Audio Engine
- Per-noot sample playback met preload buffer (instant response)
- Disk streaming — geen volledige RAM-load nodig
- Onbeperkte polyfonie
- 96 kHz HD audio support
- Soft clipping limiter
- **MP3-opname** van de live audio-uitvoer (320 kbps stereo)
- **ASIO-ondersteuning** (optionele buildfeature) voor lage latency naast WASAPI

### 🌬️ Realisme
- **Windmodel per werk** — realistisch reservoir, demping en drukverlies
- **Tremulant**: sample-switching met crossfade én LFO synthese fallback
- **Aanpasbare zwelkast** — volume + frequentie-filter zoals echte zwelkast
- **Per-pijp voicing** — volume + pitch (cent) per individuele pijp

### ⛪ Akoestiek
- **FDN algoritmische galm** met 6 ruimtepresets (Kapel → Gotische Kathedraal)
- **Convolutie reverb** met IR-loading (eigen kerk-IR's mogelijk)
- **3-band Parametric EQ** voor ruimtecorrectie
- **Per-divisie stereo panning** (constant-power)

### 🎼 Registratie
- **8 memory levels × 1000 presets** = 8.000 combinaties
- **Generaal Crescendo** met live LED-balk + matrix-editor
- **MIDI Learn** voor stops, koppels, setzer, swell, crescendo
- **Drag & drop register sortering**
- **Held-notes re-trigger** — register inschakelen tijdens spelen werkt direct

### 🎨 Stemmingen
- **52 historische temperamenten** — Werckmeister, Kirnberger, Vallotti, middentoon, Pythagorisch, etc.
- **Eigen stemming-editor** — 12 cent-waarden zelf instellen
- **Fijnstemming** ±50 cent

### 🎹 MIDI
- **Bluetooth LE MIDI** — directe BLE GATT verbinding (ESP32, WIDI, etc.)
- **Auto-connect** alle USB MIDI bij opstarten
- **MIDI-opname** (Standard MIDI File) + **afspelen** — neem je spel op als `.mid` en hoor het terug op hetzelfde orgel
- **MIDI file playback** met play/pause/stop, seek-balk en snelheidsregelaar
- **Universele MIDI Learn** — overal via rechtermuisklik (desktop) of 3 sec vasthouden (touch)
- **Per-divisie kanaal mapping** + transpose

### 🔗 Koppels
- Unison koppels (8')
- **Superoctaaf** (4', +12 halftonen)
- **Suboctaaf / Octave Grave** (16', -12 halftonen)
- **Tongwerken Af** (T.A.)
- **Melodiekoppel** (alleen hoogste noot)
- **Baskoppel** (alleen laagste noot)

### 🎨 UI/UX
- **Sfeer & Layout** — vrij instelbaar kleuren/lettertype-systeem met presets en eigen op te slaan sferen
- **Touchscreen-vriendelijk** (44px+ touch targets)
- **Keyboard shortcuts** — F1/F2 navigatie, Esc voor orgel-selectie
- **ARIA accessibility labels**
- **Apart register-paneel venster** voor multi-monitor setups
- **Orgel bibliotheek** met persistente instellingen per orgel
- **Meertalige UI** (NL/EN/FR/DE), realtime wisselbaar
- **Per-orgel instellingen-persistentie** — mastervolume, temperament, reverb, EQ, zwelkast, tremulant, pan, wind-groepen en UI-voorkeuren reizen mee met het orgel (`.jm-settings.json`)

### 📁 Bestandsformaten
- **`.organ` definitiebestanden** (lezen + schrijven)
- **WAV en MP3 samples** (loop points uit smpl chunk)
- **WAV impuls-respons** voor convolutie reverb
- **Auto-resample** naar audio device sample rate
- **Sample directory scanner** (nested mappen, multi-mic, tremulant variants)
- **Sampleset-tools** — stilte-trim (verwijdert aanloopstilte uit samples, met backup) en loop-tool (vindt automatisch loop points, bakt crossfade en schrijft ze naar de WAV `smpl` chunk)

## 🆚 Vergelijking met de concurrentie

| Feature | JM-Orgue | Hauptwerk | Sweelinq | GrandOrgue |
|---------|----------|-----------|----------|------------|
| Prijs | **Gratis** | $599+ | €11/mnd | Gratis |
| BLE MIDI direct | ✅ | ❌ | ❌ | ❌ |
| Algoritmische galm | ✅ | ❌ | ❌ | ❌ |
| 3-band Parametric EQ | ✅ | ❌ | ❌ | ❌ |
| Disk streaming | ✅ | ❌ | onbekend | ❌ |
| MP3 samples | ✅ | ❌ | ❌ | ❌ |
| Drag & drop registers | ✅ | ❌ | ❌ | ❌ |
| Tongwerken Af | ✅ | sampleset-afh. | ❌ | ❌ |
| Sfeer-systeem (thema's) | ✅ | ❌ | ❌ | ❌ |
| Geen dongle / abo | ✅ | iLok | Abo | ✅ |
| Nederlands | ✅ | ❌ | ✅ | ❌ |
| 52 stemmingen | ✅ | ✅ | beperkt | ✅ |

## 🚀 Installatie

### Voor gebruikers (na release)

1. Download `JM-Orgue-Setup.exe` (Windows) van de releases-pagina
2. Voer de installer uit
3. Snelkoppeling op bureaublad
4. Start de app, kies een orgel uit de bibliotheek of laad een eigen sample-map

### Voor developers

Zie [BUILDING.md](BUILDING.md) voor build-instructies.

## 📁 Compatibiliteit

JM-Orgue ondersteunt:
- `.organ` definitiebestanden
- WAV en MP3 samples met de structuur:
  ```
  Orgelnaam/
    Klavier1/
      Register1/
        036-c.wav
        037-cis.wav
        ...
      Register2/
        ...
    Pedaal/
      ...
  ```
- Tremulant variants in `Register_trem/` submap
- Multi-mic recordings: `Register/Front/`, `Register/Midden/`, `Register/Achter/`

## 🎚️ Hardware aanbevelingen

**Minimum:**
- Windows 10/11 (64-bit)
- 8 GB RAM
- 2-core CPU
- Geluidskaart met ASIO of WASAPI driver
- USB MIDI keyboard

**Aanbevolen:**
- 16+ GB RAM
- 4+ core CPU (audio-thread + UI-thread + BLE thread)
- SSD voor sample-opslag
- Stereo of meer-kanaals geluidskaart
- MIDI keyboard met expressie-pedaal

**Optioneel:**
- ESP32 of WIDI dongle voor draadloos MIDI
- Touchscreen monitor (auto-detect)
- Tweede monitor voor extra register-paneel

## 📜 Licentie

Open source — exacte licentie volgt bij release.

## 🙏 Credits

Gebouwd door en voor organisten. Door en voor liefhebbers van klassieke en romantische orgelmuziek.

---

**🔗 Links**

- Project locatie: `C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan`
- Documentatie: zie `docs/` map
- Versie-historie: `CHANGELOG.md`
- VST/AU haalbaarheid: `docs/VST_AU_FEASIBILITY.md`

**📅 Roadmap**

- [x] Audio engine + sample playback
- [x] MIDI input + auto-connect
- [x] Bluetooth LE MIDI
- [x] Generaal Crescendo
- [x] 52 stemmingen
- [x] Sfeer & Layout (vrij instelbare thema's met presets)
- [x] Toegankelijkheid (ARIA + keyboard)
- [ ] macOS build (Q2 2026)
- [ ] Linux build (Q2 2026)
- [ ] Officiële release (later 2026)
- [ ] JM-Rec integratie (export .organ vanuit opname-tool)
- [ ] Plugin support — VST3/CLAP-MVP bestaat al in `vpo-plugin` (laadt in DAW's, MIDI in, synthese als platform-test); volledige sample-engine in de plugin volgt
