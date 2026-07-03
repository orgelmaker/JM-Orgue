# 📋 JM-Orgue — Session Handover

> Plak dit bestand of de inhoud aan het begin van een nieuwe Claude sessie om direct verder te kunnen.

---

## ⏭️ VOLGENDE SESSIE — START HIER (2026-06-08)

### ✅ Bugfix-ronde (7 punten door gebruiker gemeld) — AFGEROND 2026-06-08
Allemaal gefixt, gebouwd en door de gebruiker bevestigd ("werkt goed"):
1. **Tremulant bleef aan na deactiveren** (`Console.svelte setTremActive`): stop-commando wordt nu altijd verstuurd bij uitzetten (`getTremEnabled(div) || !val`).
2+3. **Tremulant- & windmodel-sliders: waarden volgden niet** — displays/inputs lazen via getter-functies die Svelte niet als dependency zag; nu lezen ze `tremLfoRate/AmpDepth/PitchDepth` resp. `windGroupConfig[gIdx]` **direct** → reactief. Windmodel `{#if}`/checkbox idem.
4. **Extra vensters bleven open bij orgelwissel** (`App.svelte`): nieuwe `closeExtraPanels()` (sluit alle `panel-…` webview-vensters), aangeroepen in `loadOrgan()` én `loadFromFolder()`.
5. **Registerknoppen schaalbaar**: `--stop-min-width` CSS-var op `.stops-grid` (+verticaal), `stopSize`-state (persistent), **−/+ knoppen** in de toolbar (`adjustStopSize`, 70-220px).
6. **Bas/discant-stops begonnen beide bij MIDI 36** (GrandOrgue): DTO gebruikt nu `manual.first_accessible_key_midi_note + (FirstAccessiblePipeLogicalKeyNumber-1)` → discant start correct (Burea geverifieerd: 60-91).
7. **Import: koppels/tremulanten + namen**:
   - **Echte koppels uit bestand** (`build_couplers_from_definition` in commands.rs): bouwt `CouplerDto` uit `definition.couplers` + manual-refs (id-prefix `real_coupler_`, standaard zichtbaar in UI via reactief blok). Hauptwerk: `KeyAction` mét `ConditionSwitchID` → `CouplerDef` (in `hauptwerk.rs`). Generieke koppel-namen ("Coupler N") → beschrijvend "bron + doel". Routing was al generiek. Burea (1 koppel) + Friesach (6 koppels) geverifieerd.
   - **Namen opgeschoond** (`clean_stop_name`/`clean_division_name`, nu pub in hauptwerk.rs, gebruikt door GrandOrgue-route): stript divisie-prefixes "HW /SW /P /SL /PED /GO …" uit registernamen en "N. "-ordinalen ("2. Hauptwerk") uit divisienamen — consistent voor UI én routing (manual-namen worden in `do_load_organ` opgeschoond vóór divisies/koppels).
   - **Tremulanten zichtbaar**: `DivisionDto.has_tremulant` (GrandOrgue: manual heeft tremulant-ref; custom: stop heeft trem-samples) → frontend zet LFO-tremulant standaard beschikbaar voor die divisie.

**Testset German GrandOrgue:** `C:\Bronbestanden\JM-Orgue\Sample set homemade\Friesach_GrandOrgue\Friesach.organ` (3 manualen HW/SW/SL+Pedal, 6 koppels, HW/SW-prefixes).
⚠️ **MCP `load_organ` time-out bij grote sets** (Friesach/Ledziny): de app laadt wél, maar de MCP-test-tool geeft "fetch failed" en herstart de app soms. Verifieer grote sets via de GUI of `curl` (gebruik **forward slashes** in het pad om JSON-escape-gedoe te vermijden).

### ✅ Zwelkast-import + Loop-tool — AFGEROND 2026-06-08 (gebruiker test zelf)
- **Zwelkast (Schwellwerk) uit GrandOrgue-ODF**: een Enclosure wordt via een WindchestGroup gerefereerd. `parse_windchests` leest nu de windchest-**Name** (i.p.v. Comment); `do_load_organ` matcht omsloten windchests op (opgeschoonde) naam aan de divisie → `DivisionDto.has_swell`. Frontend (`Console.svelte`) zet de zwelkast standaard aan voor `has_swell`-divisies. ⚠️ `[Organ]NumberOfEnclosures` is soms 0 (fout); de WindchestGroup-`Enclosure`-refs zijn autoritatief. Friesach: Schwellwerk → zwelkast.
- **Loop-tool voor eigen samplesets** (`src-tauri/src/loop_tool.rs`): veel home-made sets (Puttershoek) hebben **geen** loop-punten → noten stoppen. De tool detecteert per WAV de grondtoon-periode (autocorrelatie), kiest een fase-uitgelijnd loop-punt in het sustain-deel, **bakt een equal-power crossfade** in de naad, en schrijft de loop in de WAV `smpl`-chunk. Backup → sibling `<naam>_loopbackup`. WAV-only. Commands `scan_sampleset_loops`/`apply_sampleset_loops` (async/spawn_blocking); test-routes `POST /sampleset/loop_scan` + `/sampleset/loop_apply` (gebruik forward slashes in `directory`); UI in Algemene Instellingen → "Sampleset — loops" (scan + 'Loops aanmaken', optie "alleen zonder loop"). Unit-tests in `loop_tool::tests`. (Bestaande `optimize_loop_points` in loader.rs verfijnt alleen al-bestaande loops bij het laden — dat is iets anders.)
  - ⚠️ **Gebruiker test de loop-tool + zwelkast zelf** (op de echte Puttershoek/Friesach). Testkopie stond in `testen/_looptest/` (mag weg).

### ✅ GrandOrgue-import werkend (streaming-load + ruisfilter) — AFGEROND 2026-06-08
**Status:** KLAAR & geverifieerd. Laden stalt niet meer; alleen echte registers verschijnen.

**Wat deze sessie is opgelost:**
- **Streaming-load** (`commands.rs` `do_load_organ`): de eager `load_grandorgue_organ` is vervangen door **attack-preload + achtergrond-stream** — exact het mechanisme van de custom-mappen (`do_load_samples_from_directory`). Pijp-paden (refs opgelost) → per **uniek** bestand één attack-preload (`load_audio_preload`) → `RegisterPreloadBuffers`; de volledige sample streamt op de achtergrond bij de eerste noot. Keys = `(stop.id, pipe_idx+1)`. **Dedup per uniek bestand** want rank-stops delen samples. `state.loaded_organ` wordt niet meer gevuld (was toch write-only).
  - *Oorzaak stall was:* eager laadde alle ~1000+ lange samples vol in RAM via `load_wav_manual` (leest hele data-chunk) → geheugendruk/swapping → "CPU idle" + onresponsief. De preload-route (`load_wav_preload`) heeft een eigen tolerante chunk-parser en leest **alleen de attack**, dus GreenPositiv's afwijkende `fmt`-chunks zijn geen probleem meer; de manuele full-parser draait nu alleen op de achtergrond voor de daadwerkelijk gespeelde noot.
- **Ruis/mechaniek-filter** (`grandorgue.rs`): `StopDef.accepts_retuning` toegevoegd (parst `AcceptsRetuning`) + `StopDef::is_noise_or_mechanical()`. Een "stop" wordt overgeslagen als `AcceptsRetuning=N` **OF** de naam een ruis-trefwoord bevat (noise/action/blower/bellows/ambient/motor + DE/FR-varianten). `do_load_organ` slaat die over in zowel preload als registerlijst. → Geen mechaniek-opnames (klep-/registergeluid, blaasbalg, ambient) meer als registerknop.

**Geverifieerd (MCP, nieuwe binary 18:13):**
- **GreenPositiv** (modern rank-ODF): laadt in **0,13 s** (143 unieke preloads, 280 keys), **5 echte registers** (14 ruis/mechaniek weggefilterd), akkoord speelt (voice_count 3, peak>0). Geen stall.
- **Burea** (inline-pijp-ODF, regressietest): 11 stops, audio speelt — geen regressie.

**Follow-ups nagekeken (2026-06-08):**
- ✅ **`harmonic_to_footage` gefixt**: volledige footage-tabel (feet = 64/harmonic) incl. mutaties (24→2 2/3', 40→1 3/5', 48→1 1/3', …) + decimale fallback i.p.v. "Nh". Unit-test uitgebreid.
- ✅ **Multi-rank onderzocht — geen bug voor huidige sets.** GreenPositiv's 2-rank-stops (Kwinta, Pryncypał 4') zijn *geleende/aaneengesloten* rank-segmenten (via `FirstAccessibleKeyNumber`, opeenvolgend & niet-overlappend), dus de concatenatie in `parse_stop_ranks` levert de juiste toets-mapping (56 pijpen → toetsen 36-91). `NumberOfAccessiblePipes=80` bij Kwinta is inconsistente/genegeerde metadata; de rank-slicing (44+12) is autoritatief.
  - ⚠️ **Echte beperking = compound-stops** (Mixtuur/Cornet/Sesquialter: meerdere ranks *tegelijk* per toets). Die zouden nu maar de eerste rank laten klinken; volledige ondersteuning vergt **multi-pijp-per-toets in de audio-engine** + een testset die zo'n stop bevat (komt mogelijk bij Hauptwerk/grotere sets). Niet kritisch voor GreenPositiv/Burea.
- Zelfde ruisfilter-idee evt. ook op de custom-map-route, indien nodig. Zie [[imports-only-real-registers]].

### ✅ USB-installer ververst — AFGEROND 2026-06-08
- Frontend (vite) + NSIS-installer opnieuw gebouwd mét `--features asio` (binary met de GrandOrgue-fix). Build via `cargo tauri build --features asio --bundles nsis --config '{"build":{"beforeBuildCommand":""}}'` vanuit `src-tauri` (config-override = de npm-pad-valkuil omzeild zónder tauri.conf.json te wijzigen; frontend vooraf apart gebouwd).
- Nieuwe installer: `target/release/bundle/nsis/JM-Orgue_0.3.0_x64-setup.exe` → gekopieerd naar `E:\JM-Orgue\` (oude vervangen, SHA256 geverifieerd). `Batz Puttershoek`-sampleset ongemoeid gelaten.
- (Onschuldige build-warning `__TAURI_BUNDLE_TYPE not found` = updater-plugin-metadata; app gebruikt geen updater.)

### ✅ Hauptwerk-import werkend — AFGEROND 2026-06-08
**Status:** KLAAR & geverifieerd op beide testsets.
- Nieuwe module **`vpo-sampler/src/hauptwerk.rs`** parseert `*.Organ_Hauptwerk_xml` (dependency-vrije flat-XML-scanner) en **vertaalt naar de bestaande `OrganDefinition`-struct** → de hele streaming-load + DTO + ruisfilter-pijplijn wordt hergebruikt. `commands.rs do_load_organ` dispatcht op `is_hauptwerk_path()`.
- Resolutieketen: `Stop → StopRank(RankID + MIDI-range) → Pipe_SoundEngine01(NormalMIDINoteNumber) → Layer → AttackSample → Sample(SampleFilename + InstallationPackageID)`. Pakketmap via marker `*InstallationPackageDefinition_Hauptwerk_xml` (werkt voor numerieke "002219" én named "Puttershoek Herv. Gem"). Per stop een toets-uitgelijnde pijplijst met eigen `first_midi_note` (nieuw veld op `StopDef`); ontbrekende noten = `PipeDef::Empty` (nieuw).
- HTML-entities gedecodeerd (`&#xDF;`=ß, `&#x119;`=ę, …), divisie-prefix gestript ("PED  "/"GO  "/"M  ").
- **Belangrijkste bug gefixt:** sommige HW-sets gebruiken self-closing lege tags `<Foo/>` (SaintJeanDeLuz) i.p.v. `<Foo></Foo>` (Ledziny) — de field-parser stopte daarop en miste alle volgende velden. Nu afgehandeld.
- **Geverifieerd** (smoke-test `smoke_load_real_hauptwerk`, env `JM_HW_TEST_ODF`, `--ignored`): Ledziny/Puttershoek 11 stops/2 div, 513 samples 0 ontbrekend; SaintJeanDeLuz 16 stops/3 div, 802 samples 0 ontbrekend; mutaties correct (Quinte=24→2 2/3', Tierce=40→1 3/5'). Ledziny ook in de app geladen (283 unieke preloads, 0,17s).
- **Beperkingen (toekomstig):** alleen de primaire attack-sample per pijp (release-samples + extra velocity/mic-layers genegeerd — engine doet eigen release-envelope); compound/meer-rank-per-toets-stops klinken alleen de eerste rank (zelfde engine-beperking als GrandOrgue). Hauptwerk-tremulanten nog niet gekoppeld.

**Test-samplesets (op schijf):**
- GrandOrgue (ranks): `C:\Bronbestanden\JM-Orgue\testen\GreenPositiv_GrandOrgue_1_2\GreenPositiv.organ`
- GrandOrgue (inline, werkt al): `C:\Bronbestanden\JM-Orgue\testen\Burea_Funeral_Chapel\burea_gravkapell.organ`
- Hauptwerk (WERKT NU): `...\testen\LedzinyStClement.CompPkg.Hauptwerk\OrganDefinitions\Ledziny st Clement.Organ_Hauptwerk_xml` (Puttershoek-orgel) en `...\SaintJeanDeLuz_Choeur_1_04.CompPkg.Hauptwerk\OrganDefinitions\Saint-Jean-de-Luz (choeur).Organ_Hauptwerk_xml`.
- Laden via MCP: `load_organ {path:"..."}` (synchroon; bij grote sets kan de **MCP-fetch** time-outen terwijl de app wél laadt — check `get_logs`/`get_status`).

### ✅ Deze sessie afgerond + GEBOUWD (binary `target/release/vpo-app.exe`, mét `--features asio`):
1. **Audio-uitvoer UX**: apparaat-iconen (koptelefoon/speaker), schone namen, ASIO-hint, host-labels.
2. **Galm-staart** klonk af bij loslaten → fix in `audio.rs` (per-kanaal dry + diffuse i.p.v. `mix/dry`-schaal).
3. **Reverb werd nooit toegepast** → `Console.svelte` past opgeslagen reverb toe bij organ-load (`applyReverbToBackend`, key-guarded).
4. **Audio-callback freeze** → `App.svelte` werkt `organInfo` alleen bij JSON-wijziging bij (stopte 300ms-commando-storm).
5. **Denormals (kraak)** → FTZ/DAZ in audio-render-thread (`audio.rs`).
6. **Loop-tik** → equal-power crossfade + langere crossfade (`loop_xfade`/`read_voice` in `audio.rs`) + **loop-punt-optimalisatie** (`optimize_loop_points` in `loader.rs`, fase-match). Unit-tests: `audio::loop_seam_tests`, `loader::loop_opt_tests`.
7. **Tremulant als register-knop** → `Console.svelte`: "beschikbaar" (instellingen) vs "aan/uit" (register-knop), unified `setTremActive`, MIDI-leerbaar.
8. **3 door gebruiker gemelde UI-bugs** (wachten op visuele bevestiging gebruiker):
   - Bug 2 (tremulant licht niet op): `class:engaged={tremActive[div]===true}` i.p.v. functie-aanroep (Svelte-reactiviteit).
   - Bug 3 (registers onzichtbaar na instellingen-wissel): `get_organ_info` (commands.rs) vult `drawn` nu uit de autoritatieve `drawn_stops`-set.
   - Bug 1 (MIDI: register speelt op alle klavieren): MIDI-kanaal-dropdown nu reactief (`midiMapByDiv`) → handmatig instelbaar/corrigeerbaar. ⚠️ Vraag aan gebruiker stond open: tonen de Kanaal-dropdowns na inleren **verschillende** kanalen? Zo niet → keyboards op zelfde kanaal → routing-per-apparaat nodig (nieuwe feature).

### 📦 USB-installer
- USB = schijf **E:** (14,9 GB totaal), map `E:\JM-Orgue\`. Bevat nu: **verse installer** (19:11, mét GrandOrgue streaming + Hauptwerk + harmonic-fix + ruisfilter, SHA256 geverifieerd) + samplesets `Batz Puttershoek` (bestond al), `GreenPositiv_GrandOrgue_1_2` (1,2 GB), `Burea_Funeral_Chapel` (0,2 GB), `LedzinyStClement.CompPkg.Hauptwerk` (1,8 GB). ⚠️ **SaintJeanDeLuz (27 GB) past niet** op de USB (was 13,4 GB vrij).
- ⚠️ **robocopy lukt NIET vanuit git bash** (exit 16); gebruik **PowerShell** voor USB-kopieën (`robocopy <src> <dst> /E`, exit <8 = ok).
- ✅ **Verse installer + 3 samplesets staan op USB** (2026-06-08).
- Installer bouwen: `cd src-tauri && cargo tauri build --features asio --bundles nsis` (met `LIBCLANG_PATH`). ⚠️ `beforeBuildCommand` faalt als cwd ≠ src-tauri (npm-pad); **beproefde aanpak deze sessie:** bouw frontend apart (`cd ui && npm run build`) en draai daarna de bundle met `--config '{"build":{"beforeBuildCommand":""}}'` (overschrijft de pre-build zonder tauri.conf.json aan te raken).
- **ASIO4ALL meegebundeld**: `src-tauri/installers/asio4all_setup.exe` + `src-tauri/installer-hooks.nsh` (NSIS-hook biedt ASIO4ALL aan als er geen ASIO-driver is). Gekoppeld via `nsis.installerHooks` in `tauri.conf.json`.

---

## 🎯 Wat is dit project?

**JM-Orgue** — een gratis, open-source virtueel pijporgel software (zoals Hauptwerk/Sweelinq/GrandOrgue, maar gratis en in het Nederlands).

- **Architectuur**: Tauri 2.x + Svelte 4 frontend + Rust backend
- **Hoofdpad**: `C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan`
- **Frontend**: `VirtualPipeOrgan/ui/` (Svelte + Vite)
- **Backend**: Rust workspace met meerdere crates
- **Versie**: `0.3.0` (sfeer-systeem, voicing UI, MIDI playback, 2026-05-05)
- **Binary**: `VirtualPipeOrgan/target/release/vpo-app.exe`
- **Snelkoppeling**: bureaublad `JM-Orgue.lnk`

## 👤 Gebruiker info

- **Taal**: Nederlands (UI labels, comments, communicatie)
- **Email**: orgelmaker@live.nl
- **Werkwijze**: gebruiker test zelf — **geen `preview_start` aanroepen** (Tauri app, werkt niet in browser)
- **Build constraint**: stop `vpo-app.exe` voor build, anders STATUS_ACCESS_VIOLATION

## 🏗️ Project structuur

```
C:\Bronbestanden\JM-Orgue\
├── SESSION_HANDOVER.md     # ← dit bestand
├── gids_orgelsoftware_v2.html  # forumonderzoek concurrenten
├── insta_post_update.md    # Instagram post drafts
├── screenshots\            # Instagram carousel slides + app screenshots
└── VirtualPipeOrgan\       # de eigenlijke applicatie
    ├── README.md           # gebruikers-README
    ├── BUILDING.md         # developer setup
    ├── CHANGELOG.md        # versie historie
    ├── Cargo.toml          # Rust workspace (versie 0.2.0)
    ├── docs\
    │   └── VST_AU_FEASIBILITY.md  # VST/AU analyse
    ├── vpo-core\           # data types
    ├── vpo-audio\          # audio engine, effects, FDN reverb
    │   └── src\
    │       ├── effects.rs       # Tremulant, WindModel, ParametricEq, BiquadFilter
    │       ├── convolution.rs   # FFT-partitioned reverb
    │       └── reverb_fdn.rs    # FDN algorithmic reverb
    ├── vpo-midi\           # MIDI input
    │   └── src\
    │       ├── device.rs        # midir USB MIDI
    │       └── ble.rs           # btleplug BLE MIDI
    ├── vpo-sampler\        # WAV/MP3 loading, ODF parsing
    ├── src-tauri\          # Tauri app
    │   ├── tauri.conf.json
    │   ├── icons\               # alle icoonformaten incl. .icns
    │   └── src\
    │       ├── main.rs
    │       ├── audio.rs         # audio thread, voice mixing
    │       ├── state.rs         # AppState + MIDI thread
    │       ├── commands.rs      # Tauri IPC (~70 commands)
    │       └── library.rs       # persistent organ library
    └── ui\                 # Svelte 4 + Vite frontend
        └── src\
            ├── App.svelte
            ├── temperaments.js  # 50 historical temperaments
            ├── components\
            │   ├── Console.svelte    # main view + Settings (~2000 regels)
            │   ├── Header.svelte     # tabs + theme toggle
            │   ├── SetzerBar.svelte  # setzer + crescendo LED bar
            │   └── RegisterPanel.svelte  # extra panel window
            └── styles.css
```

## ✅ Wat is af (v0.2.0)

### Audio Engine
- Sample playback met preload buffer + background full-load
- Per-divisie windmodel (reservoir, demping, max sag)
- Tremulant: sample-switching crossfade ÉN LFO synthesis fallback
- FDN algorithmic reverb (6 presets: Kapel→Kathedraal)
- Convolutie reverb (IR loading)
- 3-band parametric EQ (low shelf + mid peak + high shelf)
- Aanpasbare zwelkast (volume + low-pass filter)
- Per-divisie stereo panning (constant-power, persistent)
- Per-pijp voicing (volume + pitch cents)
- 96 kHz support (device-driven)
- Soft clipping (tanh)

### MIDI
- USB MIDI via midir (auto-connect alle devices)
- **Bluetooth LE MIDI** via btleplug (directe GATT, geen Windows pairing)
- BLE worker thread architectuur (geen deadlocks)
- MIDI learn voor stops, koppels, setzer, swell, crescendo
- Per-divisie kanaal mapping + transpose
- MIDI file recording (Standard MIDI File writer)
- Activity counter UI

### Registratie
- 8 memory levels × 1000 presets = 8.000 combinaties
- Generaal Crescendo met live LED bar (rood-oranje, in setzer balk)
- Drag & drop register sortering
- Held-notes re-trigger bij stop toggle
- Koppels: unison, super (4'), sub (16'), Tongwerken Af, melody, bass

### Stemmingen
- 50 historische temperamenten in `ui/src/temperaments.js`
- Custom temperament editor (12 cent inputs)
- Fijnstemming ±50 cent

### UI/UX
- Donker/licht thema toggle (zon/maan icoon)
- Touchscreen optimalisatie (`@media pointer: coarse`, 44-56px targets)
- Keyboard shortcuts: F1=Orgel, F2=Instellingen, Esc=bibliotheek, ←→ tussen tabs
- ARIA accessibility (tablist, aria-pressed, aria-selected)
- Orgel bibliotheek met persistente settings per orgel
- Auto-save in `.jm-settings.json` naast .organ + AppData
- Export/import settings als JSON
- Apart register-paneel venster

### Bestandsformaten
- `.organ` ODF lezen + schrijven
- WAV + MP3 samples (smpl chunk loop points)
- Sample directory scanner met multi-mic + tremulant detectie
- Auto-resample via rubato

## 🔧 Build commands

```bash
# Frontend
cd /c/Bronbestanden/JM-Orgue/VirtualPipeOrgan/ui
npx.cmd vite build

# Backend (release)
/c/Users/orgel/.cargo/bin/cargo.exe build --release

# Stop running app eerst
taskkill /f /im vpo-app.exe 2>/dev/null

# Bij linker crash
rm -f /c/Bronbestanden/JM-Orgue/VirtualPipeOrgan/target/release/deps/vpo_app*
```

Logs: `%APPDATA%\nl.jm-orgue.app\jm-orgue.log` (vers per app start).

## ⚠️ Bekende valkuilen

1. **`preview_start` werkt niet** — Tauri app, niet in browser laadbaar
2. **Build access denied** — kill `vpo-app.exe` eerst
3. **STATUS_ACCESS_VIOLATION** — `rm` deps en rebuild
4. **BLE op Windows** — gebruikt `btleplug` direct, niet midir (WinMM ziet BLE niet)
5. **Subscribe HRESULT 0x80000013** — wordt automatisch geretryd (3x), zie `vpo-midi/src/ble.rs`
6. **MIDI thread mag niet blokkeren** — gebruik `learn_wait_settled` helpers in state.rs voor learn-functies
7. **Geen orgelfoto in de bibliotheek** — `find_organ_image` (library.rs) zoekt in de orgelmap naar een afbeelding met naam `image/cover/organ/photo/picture/front` (.jpg/.jpeg/.png/.bmp/.webp), anders élke afbeelding in de top van de map. `get_organ_image` detecteert dit **live** per bibliotheek-render. Geen foto = geen afbeelding in de map. Oplossing: leg een foto (bv. `organ.jpg`) in de orgelmap. (Bv. Batz Puttershoek-map had geen afbeelding → placeholder; testorgels hebben `orgel.jpg`.)

## 🟡 Wat staat nog open

| Prioriteit | Item | Locatie / status |
|-----------|------|---------|
| 🔵 Wachten | macOS build | tauri.conf.json klaar, gebruiker zegt wanneer |
| 🔵 Wachten | Linux build | idem |
| 🟡 Te doen | **Console.svelte opsplitsen** | 2200+ regels — `OrgelView.svelte` + `InstellingenView.svelte` opsplitsen |
| 🟡 Te doen | Officiële release v0.3.0 | binary gebouwd, klaar voor distributie |
| 🟢 Te doen | Sample sets uitbreiden | zie `docs/SAMPLE_SETS.md` |
| 🟡 JM-Rec | Zwelwerk per klavier | `C:\Bronbestanden\JM-Rec\` (Python, apart project) |
| 🟡 JM-Rec | Koppels definiëren | idem |
| 🟡 JM-Rec | .organ export | idem |
| 🟢 In gang | **VST/AU plugin** — MVP werkt | `vpo-plugin/` — sine synthese, geen samples nog. Roadmap in `vpo-plugin/README.md` (5-7 maanden naar productie) |
| 🟡 Te doen | **Plugin → echte samples** | Fase A+B uit roadmap: engine extractie + sample playback |
| 🟢 In gang | **Multi-channel surround** — backend + UI | Werkt, alleen UI verschijnt bij ≥4ch device |
| 🔴 Lange termijn | JACK output (Linux/macOS) | als alternatief voor multi-app routing |

## ✨ Nieuw in v0.3.0 (2026-05-05)

### Sfeer & Layout
- **Vrij instelbaar layout-systeem** — alle kleuren, lettertype, achtergrond aanpasbaar via `LayoutSettings.svelte`
- **5 ingebouwde presets**: Basis (Witte orgelbouwer), Klassiek hout (Eiken), Modern minimalistisch, Nacht (Donker), Avond (Warm donker)
- **Eigen sferen opslaan** met naam, exporteerbaar als JSON
- **Donker/licht thema toggle weggehaald** — vervangen door vrij systeem
- **Volledige design tokens migratie** in `styles.css` — geen hardcoded kleuren meer in selectors

### Audio & MIDI
- **Per-pijp voicing UI** — `VoicingPanel.svelte` met sliders volume (±12 dB) + stemming (±50 ¢) per pijp
- **Voicing persistentie** in `.jm-settings.json`, `apply_saved_voicings` Tauri command, hersteld bij organ load
- **MIDI file playback** — `vpo-midi/src/player.rs` met SMF parser, `MidiPlayer.svelte` met play/pause/stop/seek/snelheid
- **Sample rate display** in Instellingen + `query_supported_sample_rates` command
- **52 historische temperamenten** (Lambert 1774, Barnes-Bach, Vallotti-Young toegevoegd)

### UX
- **Touchscreen-optimalisatie systematisch** via `--touch-target` token over knoppen, sliders, inputs, lijsten, context menus

## ✨ Toegevoegd in mei-sessie (na v0.3.0)

### Wind-systeem met groepen
- **Aantal groepen instelbaar** (1..8) via Instellingen → "Wind-systeem"
- **Per-groep configuratie**: Magazijn, Demping, Max verlies — voor elke actieve groep aparte sliders
- **Per-divisie groep-keuze** via "Windvoorziening Groep N" dropdown
- **Gedeeld reservoir**: divisies in dezelfde groep delen één wind-balg (gecombineerd voice-count)
- **Default identity**: elke divisie eigen groep — backward compatible
- **Persistentie** via `OrganSettings.division_wind_groups` + `num_wind_groups`
- Backend: `set_num_wind_groups`, `set_division_wind_group`, `apply_saved_wind_groups`, `get_num_wind_groups`, `get_division_wind_groups` Tauri commands

### Multi-channel surround output
- Per-divisie output-pair routing (Front, Rear, Side, Aux)
- Werkt op 4/6/8-kanaals audio devices
- Backend + UI + persistentie via `OrganSettings.division_output_pairs`

### VST/AU plugin MVP (in `vpo-plugin/`)
- VST3 + CLAP bundles via `cargo build --release -p vpo-plugin && ./vpo-plugin/bundle.sh`
- Sine synthesis (Prestant + Holpijp blend), polyfoon 128 voices, ADSR + tremulant
- Documentatie + roadmap naar productie in `vpo-plugin/README.md`

### MCP server voor self-testing (in `jm-orgue-mcp/`)
- Node.js MCP server in workspace via `.mcp.json` auto-geladen
- 12 tools: `start_app`, `stop_app`, `get_status`, `get_organ_info`, `toggle_stop`, `play_note`, `stop_note`, `play_chord`, `panic`, `get_logs`, `get_division_volumes`, `verify_audio`
- Spawnt app automatisch met `--test-api` flag
- Onderliggende HTTP test API in `src-tauri/src/test_api.rs` blijft direct te gebruiken via curl

### Instellingen splitsing (mei 2026)
- **3 tabs** in header (zichtbaar bij geladen orgel): `Orgel | Orgel-Instellingen | Algemene Instellingen`
- **Algemene Instellingen** = app-wide config:
  - Audio Uitvoer (device + sample rate info)
  - MIDI Invoer (USB + Bluetooth MIDI)
  - Sfeer & Layout (taal, kleur-presets, lettertype, achtergrond)
- **Orgel Instellingen** = per-orgel config (vereist geladen orgel):
  - Master volume + Reverb (FDN + IR)
  - Equalizer
  - Temperament + fine-tune
  - Wind-systeem (groepen)
  - Crescendo
  - Register sortering
  - MIDI Kanaal per Klavier (swell/transpose/tremulant/pan/output)
  - MIDI Speler
  - Pijp-voicing
  - Instellingen Beheer (import/export)
- **Bibliotheek-scherm**: extra knop **Algemene Instellingen** in organ-browser-actions toolbar
- **Keyboard shortcuts**: F1=Orgel, F2=Orgel-Instellingen, F3=Algemene Instellingen
- `OrganSettings` struct in `library.rs` uitgebreid met optionele velden `master_volume_db`, `reverb`, `eq`, `temperament` (voorbereid; volledige write/restore via frontend volgt later)
- **Basis preset omschrijving** opgeschoond: "Basis (Witte orgelbouwer)" → gewoon "Basis"

### Volledige i18n (mei 2026)
- **~200 vertaalsleutels** in `nl.json`, `en.json`, `fr.json`, `de.json` (uitgebreid van ~80)
- **Vertaald in alle componenten**: Header tabs, StatusBar, Sidebar, SetzerBar (tooltips), RegisterPanel (alerts + menu), VoicingPanel (titels + labels), MidiPlayer (alle controls), LayoutSettings (presets + font opties + kleur tokens + group titels), Console.svelte (sectie-titels orgel-instellingen + algemene-instellingen + alerts + dialog titles), App.svelte (loading messages)
- **`tx()` helper** voor imperatieve contexten (alerts, dialog titels)
- **Reactive `$t()` store** voor template content

### MIDI-learn uniformiteit
- **`use:midiLearn` Svelte action** in `ui/src/lib/midiLearn.js`:
  - **Rechtermuis** → MIDI-learn menu
  - **Touch long-press (3 sec)** → zelfde menu (haptische puls als browser supporteert)
  - **Bewegen tijdens vasthouden > 10px** = annuleren
- Toegepast op: alle Setzer-knoppen (SET, 0-9, ±1, ±10, G.C., M1-M8), Tremulant LFO per divisie, Stops, Couplers, **nu ook EQ enable + Crescendo on/off**
- Voorheen learnable maar zonder touch-support — werkt nu uniform op desktop én touchscreen

### Audit nog niet MIDI-learnable (toekomstig werk)
- Master volume / Reverb mix sliders — vereist continuous CC mapping (vergelijkbaar met swell-pedaal mechanisme), niet alleen toggle-action
- Sample bank navigatie / Open orgel browser — discutabel of dit MIDI-learnable hoort te zijn

## 📂 Belangrijke files (waar zit wat?)

| Wat zoek je? | File |
|--------------|------|
| AudioCommand enum (alle audio events) | `src-tauri/src/audio.rs` regel ~40 |
| Voice mixing loop | `src-tauri/src/audio.rs` rond regel 770 |
| MIDI handler (NoteOn/Off, CC routing) | `src-tauri/src/state.rs` `handle_midi_message` |
| BLE MIDI parser (timestamp+running status) | `vpo-midi/src/ble.rs` `parse_ble_midi_packet` |
| Crescendo CC processing | `src-tauri/src/state.rs` `process_crescendo_cc` |
| Tauri commands (frontend → backend) | `src-tauri/src/commands.rs` (alle `#[tauri::command]`) |
| Coupler routing (super/sub/T.A./melody/bass) | `src-tauri/src/state.rs` rond regel 870 |
| FDN reverb implementatie | `vpo-audio/src/reverb_fdn.rs` |
| Settings persistence | `src-tauri/src/library.rs` |
| Console (hoofdscherm) | `ui/src/components/Console.svelte` |
| Setzer + crescendo indicator | `ui/src/components/SetzerBar.svelte` |
| Temperaments | `ui/src/temperaments.js` |

## 🚀 Snelle start voor een nieuwe sessie

Plak dit aan het begin van een nieuwe sessie:

> Ik werk verder aan JM-Orgue, een virtuele pijporgel app in `C:\Bronbestanden\JM-Orgue\VirtualPipeOrgan`.
> Lees `C:\Bronbestanden\JM-Orgue\SESSION_HANDOVER.md` voor de volledige context.
> Versie 0.2.0 is gebouwd. [voeg hier je vervolgvraag toe]

## 📝 Recent grote fixes (referentie)

- **Sample-map inladen verbeterd (2026-06-05)** — `vpo-sampler/src/custom_organ.rs`:
  - Underscore-namen opgeschoond: `Prestant_8` → "Prestant" + pitch "8'" (underscores als scheidingsteken, trailing pitch correct afgesplitst).
  - **Bas/discant-splitsing**: een registermap met `*_bas`/`*_dis` submappen wordt nu in twee aparte registers gesplitst (bv. "Trompet (Bas)" 24-47 + "Trompet (Disc)" 48-77). Voorheen koos de multi-mic-logica alleen de eerste submap → discant verdween.
  - **"sterk"/rang-suffix**: `Mixtuur_4st` / `Cornet 4 sterk` tonen nu pitch "4st" i.p.v. "8'" (`pitch_label` veld in `CustomStop`, gebruikt in `commands.rs` DTO).
  - `format_stop_name` zet de pitch niet meer in de naam (UI toont pitch apart); unit-tests in `custom_organ.rs`.
  - **Koppels standaard verborgen**: `isCouplerVisible` in `Console.svelte` + `RegisterPanel.svelte` → default `false` (gebruiker zet zelf koppels aan in Orgel-Instellingen → Koppels). Koppels stonden al nooit actief na load.
- **Sampleset stilte-tool + register-sleep-fix (2026-06-05)**:
  - **Register herordenen**: HTML5 drag-and-drop werkte niet (vooral op touchscreen — native DnD vuurt niet op touch; ook synthetische muis-drag faalde). Opgelost met **▲▼ omhoog/omlaag-knoppen** per rij in Register-sortering (`moveStop()` in Console.svelte) — werkt op touch én muis. Plus `displayOrgan` nu bulletproof reactief: `$: displayOrgan = buildDisplayOrgan(organInfo, stopOrder)` (stopOrder als argument → Svelte trackt het). Geverifieerd via computer-use: knoppen herordenen direct, ook zichtbaar in console-view. (Oude drag-handlers staan er nog, schaden niet.)
  - **Sampleset aanloop-stilte-tool**: `src-tauri/src/silence.rs` (scan + trim). Detecteert leading silence (drempel dB, peak over kanalen), knipt met behoud van pre-roll. WAV = lossless (hound, native bitdiepte/kanalen behouden); MP3 = decode (symphonia, stereo) → re-encode (`mp3lame-encoder`, zelfde formaat). Backup naar sibling `<naam>_backup` vóór overschrijven. Volledig-stille bestanden worden overgeslagen (guard). Commands `scan_sampleset_silence`/`trim_sampleset_silence` (async/spawn_blocking); UI in Algemene Instellingen → "Sampleset — aanloop-stilte" (scan-rapport + knip-knop). Test-routes `POST /sampleset/scan` + `/sampleset/trim`.
  - **MP3-caveat**: re-encode laat ~50 ms codec-delay aan het begin staan (LAME encoder-delay); WAV is sample-exact (~pre-roll). Vereist `mp3lame-encoder` (bouwt LAME via cc — werkt met de MSVC build tools).
  - Geverifieerd: scan op Batz (889 files), trim op een kopie (WAV 3000→1002 ms exact; MP3 2040→864 ms; stil bestand onaangeroerd; backup compleet).
- **Audio-uitvoer kiesbaar + ASIO-ondersteuning (2026-06-05)** — lagere latency:
  - `audio.rs`: `AudioOutputConfig` + `resolve_host()`; `AudioPlayer::new(cfg)` kiest host/device op naam met fallback en zet buffergrootte (`BufferSize::Fixed` alleen als binnen de door het device opgegeven range). Rapporteert actuele host/device/buffer terug.
  - `state.rs`: `AudioPrefs` persistentie in `audio_config.json`; echte device-enumeratie (`list_audio_devices_for_host`); `switch_audio_output()` bouwt nieuwe player → swap → persist (frontend herlaadt orgel daarna).
  - `commands.rs`: `list_audio_hosts`, `list_output_devices`, `set_audio_output`; `StatusDto` uitgebreid met host/device/buffer.
  - Frontend: host/device/buffer-keuze + "Toepassen" in Algemene Instellingen → Audio Uitvoer (App.svelte + Console.svelte); persist in localStorage; herlaadt orgel na wissel.
  - **ASIO** is optionele cargo-feature `asio` (= `cpal/asio`). **De huidige `vpo-app.exe` is mét ASIO gebouwd én ASIO is end-to-end geverifieerd werkend** (2026-06-06): ASIO4ALL v2 geïnstalleerd (winget `MichaelTippach.ASIO4ALL`), host ASIO → device "ASIO4ALL v2" @44100, geluid speelt (peak>0). Build-methode: `py -3 -m pip install libclang` (geen admin) → `LIBCLANG_PATH=...\site-packages\clang\native cargo build --release -p vpo-app --features asio`. `asio-sys` downloadt de Steinberg SDK zelf. Zie BUILDING.md.
  - **Audio robuustheid (nodig voor ASIO)** in `audio.rs`: (1) **format-aware stream** — bouwt nu in het native sample-format van het device (WASAPI=F32, ASIO vaak I32/I16; non-f32 rendert via f32-scratch + conversie). WASAPI-pad ongewijzigd. (2) **readiness-handshake** — `AudioPlayer::new` wacht (≤8s) tot de stream echt gebouwd is en geeft `Err` als dat faalt (geen stille no-audio meer; lost ook de host/device read-race op). (3) `AppState::new` valt terug op default-device als opgeslagen prefs (bv. ASIO) falen. `switch_audio_output` bouwt de nieuwe player vóór de swap, dus een mislukte ASIO-wissel laat WASAPI intact en persist niet. (4) **`send_command`/`load_samples` gebruiken `send_timeout(3s)`** i.p.v. blokkerend `send` → een vastgelopen audio-thread kan het laden van een sampleset niet meer laten "vasthangen" in de UI.
  - **⚠️ ASIO is exclusief**: draai NOOIT meerdere instanties van vpo-app tegelijk (ook niet MCP-test-instantie + eigen venster) — ze vechten om het ASIO-apparaat ("device no longer available") en dat kan audio/laden verstoren. Symptoom "sampleset laden loopt vast" trad op door meerdere gelijktijdige instanties; met één instantie laadt alles direct.
  - Test-hook: `POST /audio_output {"host","device","buffer_frames"}` in test_api (geverifieerd: live swap + reload + audio blijft spelen).
- **BLE deadlock**: MIDI thread blokkeerde tijdens learn → `learn_wait_settled` drainst nu BOTH USB+BLE en forwardt naar audio
- **HRESULT 0x80000013**: notifications stream-volgorde fix + 3x retry op subscribe
- **Crescendo deed niets**: `process_crescendo_cc` toegevoegd met additieve stops + tracking via `crescendo_active_stops`
- **Sample upgrade glitch**: positie-conversie tussen preload-rate en full-rate in `upgrade_to_full`
- **Status check zag BLE niet**: `get_status` checkt nu USB OR BLE connected

## 🛠️ Tools die werken / werken niet

- ✅ `Bash` (git bash op Windows), `PowerShell`, `Read`, `Edit`, `Write`, `Glob`, `Grep`
- ✅ `Agent` (Explore voor codeverkenning)
- ✅ `cargo build --release`, `npx.cmd vite build`
- ❌ `preview_start` (Tauri app)
- ❌ Chrome MCP voor Tauri windows (alleen voor Chrome tabs)
- ✅ PowerShell `Add-Type System.Windows.Forms` voor screen screenshots
- ✅ Chrome headless voor HTML→PNG (`chrome --headless --screenshot`)
- ✅ **computer-use MCP** voor de Tauri-UI (request_access met exe-naam `vpo-app.exe`; venster naar voren via `open_application "vpo-app"`). Handig om drag/UI echt te testen.
- ✅ **MCP jm-orgue** (`start_app`/`load_directory`/`play_note`/etc.) draait een instantie met `--test-api` (poort 8765). ⚠️ Sluit eigen vensters eerst — niet meerdere instanties tegelijk (ASIO exclusief).
- ✅ **Test-API extra routes**: `POST /audio_output {host,device,buffer_frames}`, `POST /sampleset/scan`, `POST /sampleset/trim` (handig om audio-switch en silence-tool via curl te testen).
- ✅ **ASIO-build**: `py -3 -m pip install libclang` → `LIBCLANG_PATH=...\site-packages\clang\native cargo build --release -p vpo-app --features asio`.

## 📅 Tijdlijn

- 2025-09-17: v0.1.0 initial
- 2026 Q1-Q2: massive update naar v0.2.0
- Later 2026: officiële release

---

**Laatste update van dit bestand**: 2026-06-08 (avond)
**Build status**: ✅ backend + NSIS-installer gebouwd **mét `--features asio`**. Deze sessie: **GrandOrgue streaming-load + ruis/mechaniek-filter**, **harmonic→footage mutatie-fix**, **Hauptwerk-import** (`hauptwerk.rs`), **bugfix-ronde van 7 punten** (tremulant, sliders, extra-vensters, knopgrootte, bas/discant, echte koppels + naam-opschoning), **zwelkast-import** (Enclosure→divisie) en een **loop-tool voor eigen samplesets** (`loop_tool.rs`: detect + crossfade-bake → smpl-chunk). Definitieve installer (alles) op USB (E:). ⚠️ Gebruiker test loop-tool + zwelkast zelf. ⚠️ 1 pre-existing test faalt (`streaming::test_resample_buffer`) — ongerelateerd. ⚠️ MCP `load_organ` time-out bij grote sets (app laadt wél; verifieer via GUI/curl met forward slashes).
**Belangrijk bij nieuwe sessie**: draai max. één vpo-app-instantie (ASIO exclusief). Voor de orgelfoto in de bieb: zie valkuil 7.
