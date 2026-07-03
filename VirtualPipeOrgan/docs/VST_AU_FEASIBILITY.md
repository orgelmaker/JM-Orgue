# VST/AU Plugin haalbaarheid voor JM-Orgue

## Update mei 2026: MVP plugin werkt

Een MVP-versie van de VST3/CLAP plugin is gebouwd in `vpo-plugin/` (zie [vpo-plugin/README.md](../vpo-plugin/README.md)). De plugin laadt in DAWs, ontvangt MIDI, en produceert orgel-achtig geluid (sine-synthese, geen samples nog).

De volledige roadmap naar productie-kwaliteit staat in dat README — geschat 5-7 maanden voor feature-pariteit met de standalone app.

## Originele conclusie (september 2025)
**Niet haalbaar in de huidige Tauri-architectuur. Mogelijk in toekomstige hybrid-architectuur.**

## Waarom niet (nu)

JM-Orgue is gebouwd als een **standalone desktop applicatie** met:
- Tauri 2 (UI in webview, backend in Rust)
- Eigen audio thread via `cpal` met WASAPI/CoreAudio/ALSA
- Eigen MIDI thread via `midir`
- Eigen window management

VST3, AU, AAX en LV2 plugins werken **fundamenteel anders**:
- Geen eigen window — host (DAW) bepaalt de UI parent
- Geen eigen audio thread — host roept `process()` callback aan
- Geen eigen MIDI thread — host levert MIDI events per buffer
- Plugin moet `state save/restore` implementeren via host
- Plugin moet `bypass`, `gain compensation`, `latency reporting` ondersteunen

## Alternatieven

### Optie A: Aparte plugin-build (veel werk, ~6 maanden)
Splits de audio engine af tot een library en bouw een plugin-shell met `nih-plug` (Rust VST3 framework):
- Pro's: native VST3/CLAP/AU support
- Contra's: nieuwe UI nodig (egui of slint i.p.v. webview), dubbele codebase
- Geschat werk: 4-6 maanden

### Optie B: JACK Audio Connection Kit (Linux/macOS)
Voeg JACK-output toe naast cpal voor virtuele patchbay routing:
- Pro's: redelijk snel te implementeren in Rust (`jack` crate)
- Contra's: alleen Linux/macOS, vereist JACK runtime, niet voor Windows-DAWs
- Geschat werk: 1-2 weken

### Optie C: Virtual Audio Cable + Loopback MIDI (Windows-specifiek)
Documenteer hoe gebruikers VB-Cable / VoiceMeeter + loopMIDI kunnen gebruiken:
- Pro's: 0 ontwikkelwerk
- Contra's: gebruiker moet third-party tools installeren
- Geschat werk: 1 dag (alleen documentatie)

### Optie D: NDI Audio (network audio)
Stuur audio via NDI naar netwerk-clients:
- Pro's: cross-platform, geen DAW lock-in
- Contra's: niche, hoge latentie
- Geschat werk: 2-3 weken

## Aanbeveling
1. **Korte termijn**: Optie C documenteren (loopback voor advanced users)
2. **Middellange termijn**: Optie B implementeren (JACK)
3. **Lange termijn**: Optie A overwegen als er voldoende vraag is

## Documentatie voor gebruikers (Optie C)

### Windows: VoiceMeeter + loopMIDI

**Audio routing:**
1. Installeer [VoiceMeeter Banana](https://vb-audio.com/Voicemeeter/banana.htm)
2. Stel JM-Orgue's audio output in op "VoiceMeeter Input"
3. In je DAW: kies "VoiceMeeter Output" als input

**MIDI routing:**
1. Installeer [loopMIDI](https://www.tobias-erichsen.de/software/loopmidi.html)
2. Maak een virtuele MIDI port "JM-Orgue MIDI"
3. In JM-Orgue: scan MIDI devices → "JM-Orgue MIDI" verschijnt
4. In je DAW: stuur MIDI naar "JM-Orgue MIDI"

### macOS: BlackHole + IAC Driver

**Audio:**
- Installeer [BlackHole 2ch](https://existential.audio/blackhole/)
- JM-Orgue → BlackHole 2ch → DAW

**MIDI:**
- Open Audio MIDI Setup → IAC Driver → enable
- DAW → IAC Driver → JM-Orgue

### Linux: JACK (eens Optie B is geïmplementeerd)
Native ondersteuning via JACK patch panel.
