**STATUS: AF (2026-06-20)** — bewaard als naslag; het actuele plan is PLAN_eq_per_kanaal.md

# 📐 PLAN — Instellingen-herindeling + MIDI-opname (voor nieuwe sessie)

> Opgesteld na een read-only audit (workflow `plan-settings-and-midi`). Twee losse onderwerpen.
> Start een nieuwe sessie met: *"Voer PLAN_instellingen_en_midiopname.md uit, beginnend met onderdeel ___"*.

---

## ONDERDEEL 1 — Is de indeling Algemeen vs Orgel nog logisch?

### Korte conclusie
De **tab-indeling zelf** (Algemeen = app/hardware, Orgel = per-orgel) is grotendeels logisch.
Het echte probleem zit in de **persistentie**: heel veel instellingen die als "per orgel" gepresenteerd
worden, zijn in werkelijkheid in **globale localStorage** opgeslagen. Daardoor lekken ze tussen orgels
(orgel A stelt reverb in → orgel B krijgt 'm ook), en sommige worden helemaal niet bewaard.

### Wat al klopt (per-orgel of terecht globaal)
- **Backend `.jm-settings.json` (per orgel, correct):** presets, preset_bindings, swell_bindings,
  midi_mappings (kanaal/transpose/range), crescendo_binding (incl. min/max/invert), voicing, drawn_stops.
- **Per-orgel-hash localStorage (correct):** crescendo-config (`jm-orgue-crescendo-<hash>`), register-sortering (`jm-orgue-stop-order-<organId>`).
- **Terecht globaal:** audio host/device/buffer, MIDI-device, taal, kleuren/thema/font/achtergrond,
  autostart, "laatste registratie herstellen", sampleset-tools.

### De echte problemen (mismatches) — gepresenteerd als per-orgel, maar globaal/niet bewaard
| Instelling | Huidige opslag | Probleem | Doel |
|-----------|----------------|----------|------|
| **Master volume** | runtime-only | gaat verloren bij herladen | per orgel (`.jm-settings.json`) |
| **Reverb** (type/mix/preset/RT60/predelay/damping/room) | globale key `jm-orgue-reverb-config` | gedeeld over alle orgels | per orgel |
| **Equalizer** (aan + 3 banden) | globale key `jm-orgue-eq` | gedeeld | per orgel |
| **Temperament + fijnstemming** | alleen *custom* temp. globaal; **keuze + fine-tune niet bewaard** | keuze verdwijnt elke sessie | per orgel (keuze+fine-tune); custom-lijst mag globaal blijven |
| **Wind-systeem** (aantal groepen, groep-config, divisie→groep) | globale keys | gedeeld over orgels met andere divisies | per orgel |
| **Zwel aan/uit + min-dB + filter** per divisie | globale keys (op divisie-naam) | naam-botsing tussen orgels | per orgel |
| **Tremulant-LFO** aan/uit + rate/depth | globale key `jm-orgue-trem-lfo` | gedeeld | per orgel |
| **Koppel-zichtbaarheid** | globale key `jm-orgue-visible-couplers` | gedeeld | per orgel |
| **Divisie-pan** | globale key `jm-orgue-division-pans` | gedeeld | per orgel |
| **Output-kanalen** per divisie | globale key `jm-orgue-division-output-channels` | gedeeld | per orgel |
| **C/Cis-spreiding** (per-divisie aan + globale sterkte/afval/swap) | globale keys | gedeeld + onduidelijk of globaal of per-orgel | per orgel (besluit nodig, zie hieronder) |
| **Hoofdscherm-layout + knopgrootte** | globale keys | minder erg; UI-voorkeur | besluit: globaal laten of per orgel |
| **Oude `jm-orgue-wind-config`** | globaal, verouderd | dode code | verwijderen |

> ⚠️ Subtiel: veel van deze backend-velden bestaan al in `OrganSettings`/audio-thread (output-channels,
> ccis, wind-groups, swell-bindings) en worden bij load *toegepast*. Het gat zit in de **frontend
> localStorage-laag** die globaal is i.p.v. per orgel. Sommige (reverb/eq/master/temperament) ontbreken ook
> backend-zijdig in herstel.

### Voorstel: 3-lagen persistentie-model (consistent maken)
1. **Globaal (localStorage):** hardware (audio/MIDI device, buffer), uiterlijk (taal/kleur/font/achtergrond),
   app-gedrag (autostart, registratie-herstel), custom-temperament-bibliotheek.
2. **Per orgel (autoritatief = backend `.jm-settings.json`):** master volume, reverb, EQ, temperament-keuze
   + fine-tune, wind-systeem, zwel (min-dB/filter), tremulant-LFO, divisie-pan, output-kanalen, C/Cis,
   koppel-zichtbaarheid — plus wat er al staat (presets, bindings, voicing, drawn_stops).
3. **Per-orgel-hash (localStorage, lichtgewicht UI-state):** alleen waar backend overkill is
   (bv. layout/knopgrootte als we die per orgel willen).

**Aanpak (incrementeel, in deze volgorde):**
- **Fase 1 (snel, hoge waarde):** master volume + temperament-keuze + fine-tune toevoegen aan
  `OrganSettings` (library.rs) → opslaan in `save_current_organ_settings`, herstellen in
  `restore_organ_settings` + frontend toepassen bij load. (reverb/eq vergelijkbaar; backend al deels klaar.)
- **Fase 2 (medium):** de globale localStorage-keys voor per-divisie-instellingen migreren naar per-orgel.
  Twee opties — kies er één en houd 'm consistent:
  - **A. Backend** (`.jm-settings.json`) — robuust, overleeft localStorage wissen, mooi voor import/export.
  - **B. Per-orgel-hash localStorage** — minste code (volg het crescendo-patroon `…-<hash>`).
  Aanbeveling: **A** voor audio-DSP (reverb/eq/wind/pan/routing/ccis/swell), want de backend past die toch al toe;
  **B** voor pure UI-voorkeuren.
- **Fase 3 (opruimen):** verouderde `jm-orgue-wind-config` weg; één migratiepad zodat bestaande globale
  waarden 1x naar het eerste geladen orgel gekopieerd worden (geen verlies).
- **Fase 4 (UI-politoer, optioneel):** sub-koppen in Orgel-Instellingen groeperen: *Geluid* (master/reverb/EQ),
  *Stemming*, *Wind & dynamiek* (wind/crescendo), *Per klavier* (kanaal/zwel/tremulant/pan/kanalen/C-Cis/koppels),
  *Registers* (sortering/voicing), *Beheer* (import/export).

### Besluiten gebruiker (vastgelegd 2026-06-20)
- [x] **C/Cis sterkte/afval/swap**: **PER ORGEL** (zowel per-divisie aan/uit als de globale sterkte/afval/swap-parameters → per orgel opslaan).
- [x] **Layout + knopgrootte**: **PER ORGEL** onthouden (niet globaal).
- [x] **Persistentie-laag voor per-divisie DSP**: **backend `.jm-settings.json`** (optie A — reist mee met het orgel, overleeft herinstallatie/ander apparaat). Geldt voor reverb/EQ/wind/pan/routing/C-Cis/zwel. (Layout+knopgrootte mogen per-orgel-hash localStorage, want puur UI.)
- [x] **Custom temperamenten**: **gedeelde bibliotheek (globaal)** houden — één keer maken, beschikbaar voor alle orgels. (Wél: per orgel onthouden wélke temperament + fine-tune actief is.)

---

## ONDERDEEL 2 — MIDI-opname (opnemen, opslaan, laden, afspelen)

### Korte conclusie
**Afspelen werkt al volledig** en **opslaan (SMF-writer) bestaat**. De keten mist precies één kerncomponent:
**de opname legt geen events vast.** Daardoor is `stop_midi_recording()` → 0 events en `save_midi_recording()`
→ een leeg .mid. Verder ontbreekt de **knop/UI** en de **"opname → meteen laden"-flow**.

### Wat al bestaat (✅)
- **Afspelen:** `vpo-midi/src/player.rs` (`MidiFilePlayer`, SMF-parser via `midly`, play/pause/stop/seek/snelheid),
  commands `midi_play_file`/`midi_stop_playback`/`midi_pause`/`midi_resume`/`midi_seek`/`midi_set_speed`/`midi_player_status`,
  en UI `ui/src/components/MidiPlayer.svelte`. Afspeel-events lopen via dezelfde NoteOn-route als live MIDI → orgel klinkt.
- **Opslaan:** SMF-writer (formaat 0, 480 PPQ, variable-length delta's, tempo-meta) in `commands.rs` (~regel 1198-1253),
  command `save_midi_recording(path)`.
- **Datastruct:** `state.rs` `MidiRecording { events: Vec<(ts_us,status,d1,d2,ch)>, start_time }` +
  commands `start_midi_recording` / `stop_midi_recording` / `clear_midi_recording`.

### Het gat (❌)
1. **Geen capture:** in de MIDI-thread (`state.rs`, de 3 message-loops: USB ~759, BLE ~787, speler ~815) wordt
   **nergens** naar `midi_recording.events` geschreven. `midi_recording` wordt bovendien **niet** aan de thread
   doorgegeven. → kern van het werk.
2. **Geen opname-UI** in de console (de MP3-recorder heeft die wel; MIDI niet).
3. **Geen "opname → laden → afspelen"-flow** (los opslaan + los laden bestaat, maar niet aaneengeschakeld).
4. **Registratie niet vastgelegd** (optioneel maar belangrijk): een opname is "alleen noten" — bij afspelen
   blijft het orgel stil als er geen registers getrokken zijn. Stops/koppels/crescendo-stand worden niet bewaard.
5. **Tempo** staat vast op 120 BPM bij opslaan (acceptabel; timing relatief klopt).

### Implementatieplan
- **Stap 1 — capture (backend, vereist):** `midi_recording`-Arc doorgeven aan de MIDI-thread (of een
  `capture_event()`-helper op `AppState`). In de message-loops elk inkomend `MidiMessage` coderen naar
  `(elapsed_us, status, d1, d2, channel)` en pushen als er een opname loopt. Dekt NoteOn/Off, CC, PC, pitchbend.
  Beslis: alleen live MIDI vastleggen, of óók UI-toggles (knop-klikken op stops) — zie stap 4.
- **Stap 2 — opname-UI (frontend, vereist):** rode "MIDI opnemen"-knop in de toolbar (naast de MP3-knop),
  met teller (events/tijd) + status-poll; bij stop een opslagdialoog (Documenten/JM-Orgue-opnames/*.mid).
  Commands bestaan al (`start/stop/save_midi_recording`).
- **Stap 3 — flow (frontend):** na opslaan een knop "In speler laden" → `midi_play_file(path)` zodat je de
  zojuist opgenomen take direct terughoort. (MidiPlayer-component hergebruiken.)
- **Stap 4 — registratie meenemen:** ❌ **NIET DOEN** (besluit gebruiker 2026-06-20). Opname = alleen noten.
  Bij afspelen trekt de gebruiker zelf de registers. Scheelt veel werk en houdt het simpel.
- **Stap 5 — echte tempo (optioneel):** gemeten BPM als `MetaMessage::Tempo` opslaan i.p.v. vast 120.

### Onderscheid met de MP3-recorder
Verschillende systemen, kunnen tegelijk draaien:
- **MIDI-opname** = wat je *speelde* (events) → klein, herbruikbaar, transponeerbaar, re-synth.
- **MP3-opname** = wat je *hoorde* (audio) → groot, archief.
In de UI: twee aparte knoppen, duidelijk gelabeld.

### Te wijzigen bestanden
- `src-tauri/src/state.rs` — capture-hook + `midi_recording` aan de thread doorgeven.
- `ui/src/components/Console.svelte` — opname-knop + status + opslaan + "in speler laden".
- (optioneel) `src-tauri/src/commands.rs` — `save_midi_recording_with_registration` of begin-registratie-json;
  evt. echte tempo in de SMF-writer.

### Besluiten gebruiker (vastgelegd 2026-06-20)
- [x] Opname **zonder registratie** — alleen noten. (stap 4 vervalt)
- [x] Standaard-opslagmap voor .mid: `Documenten/JM-Orgue-opnames` (zelfde als MP3).

---

## Volgorde-advies voor de nieuwe sessie
1. **MIDI-opname** eerst (afgebakend, duidelijke "klaar"-definitie, hoge gebruikerswaarde, weinig risico).
2. **Instellingen-persistentie** daarna, in fasen (begin met master volume + temperament; dan migratie).
   Dit raakt veel code → liefst met een korte build/test-cyclus per fase.
