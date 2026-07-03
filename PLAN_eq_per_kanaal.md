# Implementatieplan: Multiband EQ per kanaal (JM-Orgue)

Status: definitief plan, MVP-eerst. Samengevoegd uit ontwerp + adversarial kritiek, geverifieerd tegen de code. Alle valide kritiekpunten zijn verwerkt; afwijzingen staan expliciet vermeld.

## 0. Beslissingen gebruiker (2026-06-24)

- **Legacy EQ: DIRECT VERWIJDEREN.** Het oude vaste 3-bands `eq`-veld + `set_parametric_eq` worden meteen verwijderd; **geen migratie** van bestaande opgeslagen 3-bands-EQ. Gevolg: EQ-instellingen in bestaande `.jm-settings.json` gaan verloren (bewust geaccepteerd). → De migratie-stappen in §2.4, §3.2 (legacy diffuse-EQ), §5 (migratie-regel + Q→bandbreedte-inverse) en §6.1/6.3 (migratie-tak) **vervallen**; verwijder `ParametricEq`/`EqSettingsSaved`/`OrganSettings.eq` i.p.v. te behouden. Open beslissing 2 = opgelost (direct verwijderen).
- **Makeup-gain: vóór `tanh`, geklemd op ±12 dB.** Open beslissing 3 = opgelost (aanbevolen variant).
- **Galm-kleuring (open beslissing 1): ONDERZOEK AFGEROND (2026-06-24).** Bevindingen (bronnen-gecheckt, GrandOrgue-deel code-geverifieerd tegen master):
  - De referentie-screenshot ("Channel: All", bandbreedte in octaven) is **níét** van GrandOrgue of Hauptwerk. GrandOrgue heeft helemaal géén EQ (bevestigd door de dev-lead, GH discussion #785); Hauptwerk heeft alleen per-rank voicing-shelves, geen mixer/output-EQ. De screenshot is een generieke DAW/plugin-EQ-layout — prima als UI-model, maar er bestaat geen "orgel-app-referentiegedrag" dat we exact moeten matchen.
  - Signaalvolgorde in beide referenties: **toonvorming op het dróge signaal eerst, galm daarna/als laatste.** GrandOrgue: per-device mix → convolutiegalm per kanaal (zelfde mono-IR op elk kanaal) → clamp (`GOSoundOutputTask::Run`). Hauptwerk: voicing per rank (upstream) → bus-reverb (downstream); geen post-reverb-EQ in-app.
  - **Gevolg voor dit plan:** de "kernfix" in §1 (EQ op post-mix `frame[c]` ná de galm-som, vangt droog+galm) is een DAW-insert-topologie, geen referentiegedrag. **TE HERZIEN bij start Fase A.** Aanbevolen alternatief (getrouw én eenvoudiger): per-kanaal EQ op het **droge** `frame[c]` direct na de kanaalrouting; galm daarná toevoegen (tail wordt dan niet per kanaal her-gekleurd). De galm-asymmetrie-risico-paragraaf in §9 vervalt dan grotendeels.
  - Los hiervan: dat onze mono-galm alleen front L/R bereikt is het échte niet-getrouwe deel (beide referenties spreiden galm over de kanalen) — aparte uitbreiding na MVP.
  - Definitieve keuze (dry-EQ vs insert-EQ) bij de start van Fase A voorleggen; **aanbeveling = dry-EQ**.
- **Volgorde deze sessie:** eerst **ASIO** fixen; EQ-implementatie (Fase A) in een latere sessie.

## 1. Doel

Vervang de huidige vaste 3-bands mono EQ door een GrandOrgue-achtige multiband parametrische EQ met **per-fysiek-kanaal** targeting. Eén platte, geordende bandenlijst; elk fysiek kanaal bouwt at-runtime zijn eigen cascade.

Geverifieerd in de code:
- Huidige EQ = vaste 3-bands mono cascade (low_shelf -> peaking -> high_shelf), `ParametricEq::process(f32)->f32` (effects.rs:307-343).
- Wordt **alleen** toegepast op het mono diffuse/galm-pad (audio.rs:1383-1386), waarna `diffuse = processed - dry_mono` (audio.rs:1389). Het droge, naar kanalen geroute signaal wordt dus **niet** ge-EQ'd.
- De soft-clip lus (audio.rs:1401-1405) doet `*s = s.tanh()` per kanaal + peak-meters op ch0/ch1.

De kernfix: pas de per-kanaal EQ toe op de **post-mix `frame[c]`** in de soft-clip lus, vóór `tanh()`. Dat vangt droog + galm voor elk kanaal en lost de dry-bypass gratis op.

## 2. Datamodel

### 2.1 DSP-laag (vpo-audio/effects.rs)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandType { Peak, LowPass, HighPass, BandPass, LowShelf, HighShelf }

// RBJ bandpass (ontbreekt nu): constant 0 dB peak (BPF).
//   b0=alpha, b1=0, b2=-alpha, a0=1+alpha, a1=-2cos w0, a2=1-alpha.
impl BiquadFilter {
    pub fn bandpass(center_hz: f32, q: f32, sr: SampleRate) -> Self { /* RBJ BPF */ }
    pub fn lowpass(center_hz: f32, q: f32, sr: SampleRate) -> Self { /* RBJ LPF */ }
    pub fn highpass(center_hz: f32, q: f32, sr: SampleRate) -> Self { /* RBJ HPF */ }

    pub fn from_band(t: BandType, freq: f32, gain_db: f32, bw_oct: f32, sr: SampleRate) -> Self {
        let q = bandwidth_oct_to_q(bw_oct);
        match t {
            BandType::Peak      => Self::peaking_eq(freq, gain_db, q, sr),
            BandType::LowPass   => Self::lowpass(freq, q, sr),
            BandType::HighPass  => Self::highpass(freq, q, sr),
            BandType::BandPass  => Self::bandpass(freq, q, sr),
            BandType::LowShelf  => Self::low_shelf(freq, gain_db, sr),   // S=1.0 (bugfix)
            BandType::HighShelf => Self::high_shelf(freq, gain_db, sr),  // S=1.0 (bugfix)
        }
    }
    /// State-preserving coefficient-update (Fase B, click-vrije retune). Overschrijft
    /// b0..a2 maar laat x1/x2/y1/y2 staan. Vereist nieuwe coeff-velden public/setter.
    pub fn set_coeffs_from_band(&mut self, t: BandType, freq: f32, gain_db: f32, bw_oct: f32, sr: SampleRate) { /* … */ }
}

/// analoge approx (display + fallback)
pub fn bandwidth_oct_to_q(bw: f32) -> f32 { let t = 2.0f32.powf(bw); t.sqrt() / (t - 1.0).max(1e-6) }
```

LowPass/HighPass-constructors zijn nodig (de huidige `BiquadFilter` heeft alleen peaking/shelf). Dit was impliciet in `from_band` maar niet als engine-change benoemd in het ontwerp — nu expliciet.

### 2.2 Engine (effects.rs) — RT-safe, fixed capacity

```rust
const MAX_SECTIONS: usize = 32;   // banden per kanaal-plafond
const MAX_CHANNELS: usize = 64;   // matcht touched-bitmask .min(63) in audio.rs

pub struct ProcessChain { sections: [BiquadFilter; MAX_SECTIONS], len: usize }
impl ProcessChain {
    #[inline] pub fn process(&mut self, x: f32) -> f32 { let mut y=x; for i in 0..self.len { y=self.sections[i].process(y);} y }
    pub fn reset(&mut self) { for s in &mut self.sections { s.reset(); } self.len = 0; }
}

pub struct MultibandEqEngine {
    pub enabled: bool,
    pub makeup_gain_lin: f32,          // 10^(makeup_db/20), met one-pole smoother (zie RT)
    makeup_target_lin: f32,
    chains: Vec<ProcessChain>,         // len == fysiek kanaalaantal (<=64)
    sample_rate: SampleRate,
}
impl MultibandEqEngine {
    pub fn new(channels: usize, sr: SampleRate) -> Self { /* één lege chain per kanaal, max 64 */ }
    pub fn rebuild(&mut self, enabled: bool, makeup_db: f32, bands: &[BandSpec]) { /* zie RT */ }
    #[inline] pub fn process_channel(&mut self, c: usize, s: f32) -> f32 {
        if !self.enabled { return s; }
        // makeup_gain_lin glijdt per sample naar makeup_target_lin (anti-zipper)
        let out = if let Some(ch)=self.chains.get_mut(c) { ch.process(s) } else { s };
        out * self.makeup_gain_lin
    }
    #[inline] pub fn tick_makeup(&mut self) { /* one-pole step richting target, per frame */ }
}
```

### 2.3 Command-payload (audio.rs) — heap-vrij per band

**Beslissing (verwerkt uit kritiek): coëfficiënten worden off-thread berekend.** De Tauri-command-thread (`set_multiband_eq`) berekent de `[b0,b1,b2,a1,a2]`-sets per kanaal-sectie; de audio-thread kopieert ze alleen. Dit elimineert tegelijk (a) de tot-2048 `sin/cos/sqrt/powf`-calls op de RT-thread bij een drag-burst, en (b) de noodzaak om biquads te reconstrueren (state-wipe = klik).

```rust
#[derive(Clone, Copy, Default)]
pub struct BiquadCoeffs { pub b0:f32,pub b1:f32,pub b2:f32,pub a1:f32,pub a2:f32 }

/// Eén kanaal-cascade als kant-en-klare coëfficiënten (fixed cap, Copy).
#[derive(Clone, Copy)]
pub struct ChannelChainCoeffs { pub sections: [BiquadCoeffs; 32], pub len: usize }

/// Volledige compile-payload: per fysiek kanaal één cascade.
pub struct MultibandEqUpdate {
    pub enabled: bool,
    pub makeup_gain_db: f32,
    pub chains: Vec<ChannelChainCoeffs>,  // len == kanaalaantal; gebouwd op command-thread
    pub structure_hash: u64,              // type/aantal/kanaal-layout → bepaalt of state gereset moet
}
```

`structure_hash` laat de drain-handler beslissen: gelijke structuur => alleen coëfficiënten kopiëren (state behouden, click-vrij); gewijzigde structuur (band toegevoegd/verwijderd/type-wissel) => `reset()` + nieuwe `len`. Dit is de toegezegde click-vrije retune (kritiek: "commit to a click-free retune path").

### 2.4 Persistentie (library.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EqBandSaved {
    pub enabled: bool,
    pub r#type: String,      // "peak"|"lowpass"|"highpass"|"bandpass"|"lowshelf"|"highshelf"
    pub channel: i16,        // -1 = Alle, anders fysieke kanaalindex
    pub freq: f32,
    pub gain_db: f32,
    pub bandwidth_oct: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EqMultiSaved { pub enabled: bool, pub makeup_gain_db: f32, pub bands: Vec<EqBandSaved> }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EqPresetSaved { pub name: String, pub eq: EqMultiSaved }
```

- `OrganSettings` (library.rs:204) krijgt `#[serde(default)] pub eq_multi: Option<EqMultiSaved>` **naast** het legacy `eq`-veld (library.rs:226), dat blijft staan.
- `OrganLibrary` (library.rs:260) krijgt `#[serde(default)] pub eq_presets: Vec<EqPresetSaved>`. **Let op:** `OrganLibrary` heeft een handmatige `impl Default` (library.rs:266-273) — die moet ook `eq_presets: Vec::new()` krijgen, anders compileert het niet. (Dit detail miste in beide brondocumenten.)

**Opslag-locatie:** actieve `eq_multi` is **per orgel** (`.jm-settings.json` + library-map). Presets zijn een **globale** bibliotheek (`organ_library.json` via `eq_presets`). Geverifieerd: dit is de enige bestaande globale store, analoog aan eigen temperamenten.

Voorbeeld `.jm-settings.json`-fragment:
```json
"eq_multi": { "enabled": true, "makeup_gain_db": 0.0, "bands": [
  {"enabled":true,"type":"peak","channel":-1,"freq":5000,"gain_db":0.0,"bandwidth_oct":2.0},
  {"enabled":true,"type":"highshelf","channel":1,"freq":8000,"gain_db":-3.0,"bandwidth_oct":2.0}
]}
```

## 3. Engine-wijzigingen (audio.rs / effects.rs)

### 3.1 effects.rs
1. `BandType`-enum + `bandwidth_oct_to_q`.
2. Ontbrekende constructors: `bandpass`, `lowpass`, `highpass` (RBJ), mirror van `peaking_eq` (effects.rs:239).
3. `from_band(...)`-dispatcher + `set_coeffs_from_band(...)` (state-preserving).
4. **Shelf-slope-bugfix:** `low_shelf` (effects.rs:266) en `high_shelf` (effects.rs:289) gebruiken `(1.0/0.9 - 1.0)`; wijzig `0.9 -> 1.0` (S=1.0, GrandOrgue-default). Bundel met migratie-communicatie (verandert bestaande shelf-klank licht).
5. `ProcessChain`, `MultibandEqEngine`, `BiquadCoeffs`, `ChannelChainCoeffs`, `MultibandEqUpdate`.
6. `ParametricEq` (effects.rs:307) blijft bestaan voor backward-compat deserialisatie/fallback, maar wordt uit het dry-pad ontkoppeld.

### 3.2 audio.rs
- **~regel 723** (naast `parametric_eq`): `let channel_eq: Arc<RwLock<MultibandEqEngine>> = Arc::new(RwLock::new(MultibandEqEngine::new(channels, sample_rate)));`. **NB:** `channels` is hier nog niet gedefinieerd — het komt pas op audio.rs:799 (`let channels = stream_config.channels as usize;`). Verplaats die `let channels`-binding **omhoog** (vlak na `stream_config` op ~660) zodat de engine met het juiste post-switch kanaalaantal wordt gebouwd. Alle audio-state is lokaal in de audio-thread-functie en wordt vers gebouwd bij elke engine-start (geverifieerd: audio.rs:673-725), dus device-switch reconstrueert de engine automatisch met de nieuwe `channels`.
- **~regel 786** (naast `eq_clone`): `let ch_eq_clone = channel_eq.clone();`.
- **audio.rs:86** (AudioCommand): `SetMultibandEq(Box<MultibandEqUpdate>)`. Gebruik `Box` zodat de enum-variant klein blijft en de Vec/array op de command-thread wordt gealloceerd.
- **audio.rs:~1119** (drain-switch, naast `SetParametricEq`):
  ```rust
  AudioCommand::SetMultibandEq(update) => {
      let mut e = ch_eq_clone.write();
      e.apply_update(*update); // kopieert coëfficiënten; reset state alleen bij structuurwissel
  }
  ```
  Draait max. één keer per buffer, nooit per frame (zoals `SetParametricEq` op audio.rs:1119).
- **audio.rs:~1178** (vóór `for frame in data.chunks_mut(channels)`): `let mut ch_eq = ch_eq_clone.write();` — één lock, mirror van `swell_flt` (audio.rs:1180).
- **audio.rs:1401-1405** (soft-clip lus, stereo/multich tak):
  ```rust
  for (ci, s) in frame.iter_mut().enumerate() {
      *s = ch_eq.process_channel(ci, *s);
      *s = s.tanh();
      if ci == 0 { peak_l = peak_l.max(s.abs()); }
      else if ci == 1 { peak_r = peak_r.max(s.abs()); }
  }
  ch_eq.tick_makeup();
  ```
- **audio.rs:1406-1411** (mono tak): `let out = ch_eq.process_channel(0, frame[0] + diffuse).tanh();`.
- **audio.rs:1383-1386:** verwijder de legacy `eq.process` op het diffuse-pad (zie open beslissing 5). `parametric_eq`/`SetParametricEq` blijven compileren maar ongebruikt tot Fase D.

### 3.3 rebuild()/apply_update — RT-veiligheid (kritiek verwerkt)
- Coëfficiënten worden **op de command-thread** berekend (`set_multiband_eq`), niet in de callback. De callback kopieert alleen `[b0..a2]` in bestaande array-slots → geen `sin/cos/powf` op de RT-thread, geen 64×32-CPU-piek.
- `MultibandEqUpdate.chains` is een `Vec` die op de UI/command-thread wordt opgebouwd, in een `Box` over het kanaal gaat, en **gedropt** wordt op de callback-thread na drain. Dit is één `free()` op de RT-thread — bounded en consistent met bestaande `SetDivisionOutputChannels { channels: Vec<u8> }` (audio.rs:90, drain audio.rs:1101-1106). Bekend, geaccepteerd RT-compromis; gedocumenteerd. (Optioneel later: recycle-kanaal terug naar de command-thread.)
- Geen allocatie op de steady-state per-frame-pad: `ProcessChain` is `[BiquadFilter; 32]`, `chains` is één keer gesized bij `new`.
- **Makeup-gain anti-zipper:** `makeup_gain_lin` glijdt per frame via one-pole naar `makeup_target_lin` (kritiek: stap-discontinuïteit = klik). FTZ/DAZ staat al aan (audio.rs:808-813) en dekt de nieuwe biquads.
- Klem out-of-range kanalen in `apply_update`: een band met `channel >= chains.len()` wordt stil overgeslagen (device-downsize); de persistente waarde blijft behouden (zie 6.4).

## 4. Command-API (commands.rs + main.rs)

- **`set_multiband_eq(state, config: EqMultiSaved)`** (model: `set_parametric_eq`, commands.rs:1658):
  1. parse band-types (`String -> BandType`), bouw per fysiek kanaal de cascade-coëfficiënten (Alle-banden in lijstvolgorde, daarna `channel==c`-banden in lijstvolgorde) met de **exacte RBJ sinh** bw->Q (open beslissing 3). Kanaalaantal komt uit de live audio-player (nieuw helper `state.audio_channel_count()` of via opgeslagen sample-config).
  2. begrens band-aantal (reject/clamp absurde aantallen, zie security).
  3. `state.send_audio_command(AudioCommand::SetMultibandEq(Box::new(update)))`.
  4. `*state.eq_multi.write() = Some(config);` (mirror; `set_parametric_eq` doet dit ook zelf op commands.rs:1662-1665, dus geen aparte persist-command nodig).
  Accepteert de hele struct via serde (de huidige 8-positionele-arg-signatuur kan geen N banden dragen).
- **`get_eq_presets(state) -> Vec<EqPresetSaved>`** — leest `OrganLibrary.eq_presets`.
- **`save_eq_preset(state, name, config)`** — upsert in `eq_presets` op naam + `state.save_library()` (state.rs:426). De "+"-knop.
- **`delete_eq_preset(state, name)`** — verwijder + `save_library()`.
- **`export_eq(state, path)` / `import_eq(state, path)`** — serde `EqMultiSaved` van/naar `.json`. Dun Rust-paar voor consistentie met `export_settings`/`import_settings`. **Security (kritiek):** schrijf/lees alleen het via plugin-dialog gekozen pad; valideer extensie `.json`; begrens `bands.len()` (bv. <= 256) en kanaalindex vóór engine-bouw, zodat een corrupt/oversized preset-bestand geen panic of buitensporig werk veroorzaakt.
- Registreer alle nieuwe commands in `main.rs` invoke_handler (main.rs:205-244, naast `set_parametric_eq` op regel 218). Niet-geregistreerde commands zijn onzichtbaar voor de frontend.
- **test_api.rs:** voeg `(Post, "/settings/eq-multi")`-route + handler toe (template `handle_set_eq`, test_api.rs:555) en breid `handle_settings_mirror` (test_api.rs:618) uit met `eq_multi` als `Option` (None vs Some onderscheiden), zodat de restore-isolatie-test lekkage kan detecteren.

## 5. Persistentie + migratie (commands.rs / state.rs)

- **state.rs:235** (naast `eq_settings`): `pub eq_multi: Arc<RwLock<Option<EqMultiSaved>>>,`. **state.rs:417** (`AppState::new`): `eq_multi: Arc::new(RwLock::new(None)),`. Niet toevoegen aan `reset_division_settings` (state.rs:1939) — het is een globale Option die in restore onvoorwaardelijk wordt overschreven.
- **do_save_organ_settings (commands.rs:3223-3233):** breid de `existing_*`-tuple uit met `existing_eq_multi` (lees `s.eq_multi.clone()`), dan `let eq_multi = state.eq_multi.read().clone().or(existing_eq_multi);`. Voeg `eq_multi,` toe aan het struct-literal (commands.rs:3256-3278) — er is **geen** `..Default::default()`, dus weglaten = compile-fout (gewenste vangrail). De comment op commands.rs:3219-3221 ("eq migreert nog niet, komt in een volgende fase") wordt nu waar gemaakt; werk die comment bij.
- **restore_organ_settings (commands.rs:2999-3012):** voeg `*state.eq_multi.write() = settings.eq_multi.clone();` toe (onvoorwaardelijk, incl. None — geen lek van vorig orgel). **Stuur hier GEEN audio** (mirror-only by design, comment commands.rs:2999-3004).
- **Migratie — één plek (kritiek verwerkt):** de 3-bands -> 3-Alle-banden migratie hoort in de **frontend** `loadAudioSettingsForOrgan` (Console.svelte:1259-1272), NIET in `restore_organ_settings` (dat stuurt geen audio en kan niet toepassen). Geen dubbele migratie in de mirror. Precedentie bij laden: **`eq_multi` heeft voorrang op `eq`**.

  Migratie-regel (frontend): als `s.eq_multi == null` en `s.eq` aanwezig: bouw
  `[lowshelf(low_freq,low_gain,Alle), peak(mid_freq,mid_gain,bw_from_Q(mid_q),Alle), highshelf(high_freq,high_gain,Alle)]`,
  `enabled = s.eq.enabled`, `makeup = 0`, dan `await updateEq()` (persisteert `eq_multi` bij de volgende save). Het legacy `eq`-veld blijft staan (open beslissing 2).

  **Q -> bandwidth_oct inverse (kritiek: was hand-waving).** De analoge approx `Q = sqrt(2^bw)/(2^bw-1)` is niet gesloten inverteerbaar. Gebruik de **exacte RBJ-relatie** (we hebben f en Fs in de frontend via de opgehaalde sample-rate):
  `bw = (2/ln2) * asinh( sin(w0) / (2*Q) ) / w0`, met `w0 = 2*PI*mid_freq/Fs`.
  Geverifieerd: de persistente `s.eq.mid_q` is de **echte Q** (de UI schaalt alleen met /10 bij verzenden, Console.svelte:589, en *10 bij laden, Console.svelte:1264) — de migratie leest dus de echte Q, geen extra /10. Documenteer dat de gemigreerde mid-bandbreedte exact is voor deze Fs.

- Persisteer naar BOTH `.jm-settings.json` (commands.rs:3288) en `organ_library.json` via `save_library` — beide `to_string_pretty`, banden printen als multi-line objecten (acceptabel, zoals `division_pans`).

## 6. Frontend (Console.svelte / App.svelte)

### 6.1 State + apply
- **Console.svelte:575-579:** vervang de vlakke 3-bands vars door: `eqEnabled`, `eqMakeupGainDb`, `eqBands` (array `{enabled,type,channel,freq,gainDb,bandwidthOct}`), `eqSelectedBand`, `eqPresets`, `eqSelectedPreset`.
- **Console.svelte:584-595:** herschrijf `updateEq()` -> `invoke('set_multiband_eq', { config: { enabled, makeup_gain_db, bands: eqBands.map(toSnake) } })`. Eén command past toe én persisteert (zelfde round-trip-semantiek als nu).
- **Console.svelte:1259-1272 (loadAudioSettingsForOrgan):** lees `s.eq_multi` (snake_case) als aanwezig; anders migreer uit `s.eq` (zie 5); anders defaults. Dan `await updateEq()`.

### 6.2 Dynamisch kanaalaantal bij device-switch (kritiek verwerkt)
Geverifieerd: `query_audio_channel_count` wordt één keer opgehaald (Console.svelte:854) en nooit ververst; `audioChannelCount` is stale na een device-switch. `applyAudioOutput` (App.svelte:320-342) **herlaadt** het orgel na `set_audio_output` (App.svelte:332-338), wat `loadAudioSettingsForOrgan` -> `updateEq()` opnieuw triggert — dus de EQ-chains worden de facto opnieuw opgebouwd voor het nieuwe device. Maar de Channel-dropdown blijft het oude aantal tonen.
- **Fix:** na `set_audio_output` resolved in `applyAudioOutput`, re-invoke `query_audio_channel_count` en update `audioChannelCount` (forward naar Console via prop/event, of verplaats de fetch naar de organ-load-flow die Console al draait). Doe dit vóór de organ-reload zodat de dropdown klopt.
- Omdat het orgel herladen wordt en dat `updateEq()` aanroept, is er **geen** apart Rust-side re-apply nodig (de engine start leeg maar wordt direct opnieuw gevuld). Dit wijkt af van de kritiek-suggestie om een `apply_saved_eq_multi` toe te voegen: **afgewezen** omdat de bestaande organ-reload-na-switch (App.svelte:332-338) de re-apply al dekt; een extra Rust-apply zou dubbel sturen. Mocht de organ-reload ooit wegvallen, dan alsnog een `apply_saved_eq_multi` (model: `apply_saved_output_channels`, commands.rs:1414).

### 6.3 Markup (Console.svelte:2748-2790)
Vervang het vaste 3-bands `settings-block` door een EQ-paneel (Nederlandse labels): master "EQ aan"-checkbox (behoud `ACTION_EQ_ENABLE` MIDI-learn, regel 2758), SVG-responsgrafiek, band-editor, band toevoegen/verwijderen, presets-dropdown + "+", File-knop, verticale makeup-gain-slider. Hergebruik `swell-config-row` / `swell-config-sliders` markup + CSS-vars.

### 6.4 Band-editor + kanaal-dropdown
- Type-dropdown: Peak / Laagdoorlaat / Hoogdoorlaat / Banddoorlaat / Lage plank / Hoge plank.
- Channel-dropdown: "Alle" (=-1) + `Array(audioChannelCount)` via `channelLabel(ch, audioChannelCount)` (Console.svelte:857). **Out-of-range gedrag (kritiek):** een band met `channel >= audioChannelCount` toont een gemarkeerde optie `"Kanaal N (niet beschikbaar)"` i.p.v. stil te coercen naar 0/Alle; de oorspronkelijke `channel`-waarde blijft in `eqBands` bewaard en wordt bij save doorgegeven, zodat het grotere device de targeting herstelt.
- Gain-slider+box: -20..+20 dB, **disabled voor LP/HP/BP** (geen zinvolle gain).
- Bandwidth-slider+box: 0.1..4.0 oct, **disabled voor shelves** (open beslissing 4: shelf gebruikt S=1.0).
- "Band toevoegen" **blokkeert bij MAX_SECTIONS (32) per kanaal** met zichtbare waarschuwing; `apply_update` dropt overflow ook engine-side (geen stille data-loss-footgun, kritiek verwerkt).
- Band-tabs "1","2",... boven de editor; klik selecteert `eqSelectedBand` + sync grafiek-highlight.

### 6.5 SVG-responsgrafiek (Fase B)
- viewBox: log-X (20 Hz..20 kHz, gridlijnen 100/1k/10k) en lineaire Y (-20..+20 dB).
- Bereken de gecombineerde magnitude in JS: evalueer elke enabled band-biquad `|H(e^jw)|` op ~200 frequenties en som in dB. Repliceer de RBJ-coëfficiënten in JS (zelfde bw->Q-conventie; zie open beslissing 3 — engine gebruikt exacte sinh, grafiek mag analoge approx voor display, mits getest). Toon de effectieve curve voor het **geselecteerde kanaal** (Alle-banden + kanaalbanden).
- Sleepbare genummerde handles op `(logX(freq), dbY(gain))`. X->freq (inverse log), Y->gain_db; **verticaal slot voor LP/HP/BP** (gain genegeerd → grijs de Gain-control). `updateEq()` gedebounced.
- Geen canvas/charting-lib (bestaat niet in de codebase); pure SVG met CSS-var-kleuren.

### 6.6 Presets + File
- Dropdown `eqSelectedPreset`, gevuld via `get_eq_presets`; "+" -> naam-prompt -> `save_eq_preset({name,config})`; selecteren laadt banden in `eqBands` + `updateEq()`. Bewaar de geselecteerde **preset-naam** als per-orgel UI-pref via `organUiKey()`/`readOrganUiPref()` (Console.svelte:439-447) — puur UI.
- File import/export: hergebruik `exportSettings`/`importSettings`-patroon (Console.svelte:1372-1402) met plugin-dialog + `invoke('export_eq'/'import_eq', {path})`. Bij import: vervang `eqBands` + `updateEq()`.
- Alle labels/commentaar Nederlands (projectconventie).

## 7. Fasering (MVP-eerst)

### Fase A — Engine + commands + numerieke UI (MVP) — L
DSP generaliseren + per-kanaal end-to-end met platte numerieke bandenlijst (nog geen grafiek).
- effects.rs: `BandType`, `bandpass`/`lowpass`/`highpass`, `bandwidth_oct_to_q`, `from_band`, shelf-S-fix, `ProcessChain`, `MultibandEqEngine`, coeff-structs.
- audio.rs: `channel_eq` Arc, `channels` omhoog verplaatst, `SetMultibandEq(Box<…>)` + drain-handler (coeff-copy + structure_hash), één lock vóór frame-lus, toepassen in soft-clip lus (1401-1411), legacy diffuse-EQ verwijderd.
- library.rs: `EqBandSaved`/`EqMultiSaved` + `OrganSettings.eq_multi`.
- state.rs: `eq_multi` mirror + init.
- commands.rs: `set_multiband_eq` (off-thread coeff-compute) + do_save/restore-wiring + frontend-migratie-hook. main.rs: command registreren.
- Frontend: 3-bands blok -> numerieke bandenlijst (toevoegen/verwijderen, type/kanaal/freq/gain/bw, master enable, makeup), `set_multiband_eq`; load leest `eq_multi` of migreert uit `eq`.
- **Verificatie:** Tauri build + MCP `play_note`/screenshots; bevestig dat een links-only band alleen kanaal 0 beïnvloedt en oude `.jm-settings.json` nog laadt.

Deliverables: multiband-engine compileert + unit-test `bandwidth_oct_to_q` en `bandpass`-coëfficiënten; per-kanaal EQ op `frame[c]` vóór tanh; dry-pad niet langer gebypassed; apply+persist+migratie; numerieke UI met hoorbaar geverifieerde per-kanaal-targeting.

### Fase B — SVG-grafiek + sleepbare handles + click-vrije retune — M
Inline SVG (log-X/lin-dB grid), gecombineerde curve in JS, genummerde sleep-handles (verticaal slot voor LP/HP/BP), handle-select <-> band-tabs. **Plus** de toegezegde click-vrije retune: `set_coeffs_from_band`/`apply_update` met `structure_hash` (state behouden bij pure coëfficiëntwijziging) + makeup one-pole smoother. Band-editor-polish (type-afhankelijke enable/disable).

Deliverables: SVG-grafiek met gridlijnen 100/1k/10k Hz en -20..+20 dB; JS-biquad-magnitude-evaluator; **unit/integratie-test** dat de JS-evaluator binnen tolerantie matcht met effects.rs per `BandType`; drag-to-edit met debounced `updateEq()`; geen klik op gehouden akkoord bij coëfficiënt-only wijziging.

### Fase C — Presets (globaal) + File import/export — S
`OrganLibrary.eq_presets` + manuele `Default`-impl bijwerken; get/save/delete-commands; presets-dropdown + "+" + verwijderen; preset-naam als per-orgel UI-pref. File import/export via plugin-dialog met security-validatie (pad/extensie/band-cap).

Deliverables: `eq_presets` in `organ_library.json`; commands geregistreerd; dropdown + save + delete; File import/export van `EqMultiSaved` JSON met validatie.

### Fase D — Opruimen + tests + legacy-retirement — S
Beslis lot van `set_parametric_eq` / `EqSettingsSaved` / `OrganSettings.eq` (behoud voor read-migratie, stop met schrijven). test_api `/settings/eq-multi`-route + mirror-exposure (`eq_multi` als Option). Restore-isolatie-regressietest (laad orgel A met kanaal-1-band, switch naar B zonder eq_multi, assert `state.eq_multi` mirror == None — geen lek). Headless verificatie.

Deliverables: test_api eq-multi route + mirror-dump; restore-isolatie-regressietest gekoppeld aan de echte mirror; beslissing op legacy `eq`-veld/command toegepast.

## 8. Open beslissingen (met aanbeveling)
Zie de gestructureerde lijst; alle aanbevelingen zijn in dit plan al verwerkt.

## 9. Risico's + RT-veiligheid
- **Klankverandering (regressie):** EQ verhuist van diffuse-only naar post-mix `frame[c]` — elk orgel met EQ aan klinkt anders (raakt nu ook droog). De legacy-migratie maakt Alle-banden (bedoelde fix). A/B-test tegen een opgeslagen orgel.
- **Galm-asymmetrie (kritiek, expliciet gedocumenteerd):** de mono diffuse galm wordt alleen in `frame[0]/frame[1]` gesommeerd (audio.rs:1391-1394) (+ ch2 center-fill op audio.rs:1397-1398). Een Alle-band kleurt de galm dus **alleen op de voorste kanalen**; droog wordt op álle kanalen ge-EQ'd. Per-kanaal EQ op achter-kanalen raakt de galm nooit. Dit is **niet** equivalent aan het oude gedrag en niet aan een puur per-kanaal GrandOrgue-model. **Beslissing:** galm-kleuring is bewust front-only; documenteer in de UI-help. (Alternatief, niet in MVP: diffuse vóór de EQ-lus naar alle touched-kanalen routen.)
- **tanh na EQ+makeup:** de soft-clipper zit nu na EQ+makeup. Bij vlakke 0 dB-banden + 0 dB makeup is de keten unity (veilig). Positieve makeup of boost duwt meer in tanh-saturatie dan voorheen → andere distortie-onset. Documenteer; overweeg makeup-range te klemmen. Makeup geldt op **alle** fysieke kanalen (incl. ch2 center-fill, ch3 LFE) — matcht GrandOrgue output-gain; documenteer dat een leeg-EQ-kanaal toch door makeup beïnvloed wordt als master EQ aan staat.
- **RT-CPU:** coëfficiënt-berekening (sin/cos/sqrt/powf) staat op de **command-thread**, niet de callback — geen 64×32-piek op de RT-thread. Callback kopieert alleen `[b0..a2]`.
- **RT-dealloc:** `Box<MultibandEqUpdate>` (met `Vec`) wordt gedropt op de callback-thread na drain — één `free()`, bounded, consistent met `SetDivisionOutputChannels`. Bekend compromis.
- **Zipper/klik:** `structure_hash` behoudt z-state bij coëfficiënt-only retune; makeup-gain one-pole-smoothed. Reset alleen bij structuurwissel (band add/remove/type).
- **Device-downsize:** band die kanaal 5 target op 2ch-device wordt in `apply_update` overgeslagen; persistente waarde blijft; UI toont "Kanaal N (niet beschikbaar)".
- **Restore-isolatie:** `eq_multi` is globale Option, onvoorwaardelijk overschreven in restore — regressietest in Fase D.
- **JS-grafiek vs engine:** documenteer bw->Q-conventieverschil (engine exact sinh, grafiek analoge approx) + tolerantie-test (Fase B).
- **Lock-discipline:** `.write()` één keer vóór de frame-lus (mirror `swell_filters`, audio.rs:1180); geen lock per frame; `process_channel` is `#[inline]` met enkel `if !enabled`-branch + `get_mut`. Geen nieuwe contentie (command-thread schrijft alleen tijdens drain, in dezelfde callback).
- **Backward-compat:** elk nieuw `OrganSettings`-veld vereist `#[serde(default)]` én expliciete vermelding in het `do_save`-struct-literal (geen `..Default::default()`); `OrganLibrary` heeft handmatige `Default` die `eq_presets` moet bevatten.