//! Application state

use std::sync::Arc;
use std::path::PathBuf;
use std::thread;
use parking_lot::RwLock;
use crossbeam_channel::{Sender, Receiver, bounded};

use vpo_sampler::{OrganDefinition, LoadedOrgan};
use vpo_midi::{MidiInputManager, MidiMessage};

use crate::audio::{AudioPlayer, AudioCommand, AudioOutputConfig, resolve_host};
use crate::commands::{OrganInfoDto, CouplerDto};

/// Teller van gedropte realtime audio-commando's (volle queue / consumer weg).
static RT_DROP_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Realtime send voor het notenpad: non-blocking `try_send` op een gekloonde
/// zender (dus BUITEN de player-read-lock). Bij een volle queue of ontbrekende
/// consumer wordt het commando gedropt en periodiek (elke 50 drops) gewaarschuwd
/// — een realtime-noot mag nooit tot 3s blokkeren en zo een audio-wissel ophouden.
#[inline]
fn rt_send(tx: &Sender<AudioCommand>, cmd: AudioCommand) {
    if tx.try_send(cmd).is_err() {
        let n = RT_DROP_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n % 50 == 0 {
            tracing::warn!("Realtime audio-commando's gedropt (queue vol of consumer weg): totaal {}", n);
        }
    }
}

/// Eén effectieve koppel-route na uitvouwen van (transitieve) koppels:
/// speel toetsen van `source_division` óók op `destination_division`, met
/// `pitch_offset` halve tonen verschuiving. `gate_type` is het type van de
/// eerste schakel ("melody"/"bass" filteren op de hoogste/laagste ingedrukte
/// toets van het bronklavier; overige typen gedragen zich als unisono).
#[derive(Debug, Clone)]
pub(crate) struct ExpandedCouplerRoute {
    pub source_division: String,
    pub destination_division: String,
    pub pitch_offset: i32,
    pub gate_type: String,
}
use crate::library::{OrganLibrary, load_library, save_library, TemperamentSettingsSaved, ReverbSettingsSaved, EqSettingsSaved};

/// MIDI device info (safe to share)
#[derive(Debug, Clone)]
pub struct MidiDeviceInfo {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

/// App-wide audio output preferences, persisted to `audio_config.json`.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioPrefs {
    /// Host/driver name, e.g. "WASAPI" or "ASIO" (None = default host)
    pub host: Option<String>,
    /// Output device name (None = default device)
    pub device: Option<String>,
    /// Buffer size in frames (None/0 = driver default)
    pub buffer_frames: Option<u32>,
    /// Polyfonie-kap (aantal gelijktijdige stemmen); None = standaard (1024).
    #[serde(default)]
    pub polyphony: Option<u32>,
    /// Stereo-samples als twee kanalen laden; None = aan. Uit = oude mono-mix
    /// (halveert RAM bij stereo-sets). Wijziging vergt herladen van het orgel.
    #[serde(default)]
    pub stereo_samples: Option<bool>,
    /// Afstandsbediening in het netwerk (0.7.38): aan/uit; None = uit.
    #[serde(default)]
    pub remote_enabled: Option<bool>,
    /// Poort van de afstandsbediening; None = 8766 (remote::DEFAULT_PORT).
    #[serde(default)]
    pub remote_port: Option<u16>,
    /// Toegangstoken van de afstandsbediening; None = nog nooit aangemaakt.
    #[serde(default)]
    pub remote_token: Option<String>,
}

/// Load audio preferences from `<app_data_dir>/audio_config.json` (defaults if absent).
pub fn load_audio_prefs(app_data_dir: &std::path::Path) -> AudioPrefs {
    let path = app_data_dir.join("audio_config.json");
    if let Ok(json) = std::fs::read_to_string(&path) {
        if let Ok(prefs) = serde_json::from_str::<AudioPrefs>(&json) {
            return prefs;
        }
    }
    AudioPrefs::default()
}

/// Save audio preferences to `<app_data_dir>/audio_config.json`.
pub fn save_audio_prefs(app_data_dir: &std::path::Path, prefs: &AudioPrefs) {
    let path = app_data_dir.join("audio_config.json");
    if let Ok(json) = serde_json::to_string_pretty(prefs) {
        if let Err(e) = std::fs::write(&path, json) {
            tracing::warn!("Failed to save audio_config.json: {}", e);
        }
    }
}

/// MIDI channel mapping for a division
#[derive(Debug, Clone, Default)]
pub struct MidiChannelMapping {
    /// Division name (e.g., "Pedaal", "Hoofdwerk")
    pub division: String,
    /// MIDI channel (0-15, None = all channels)
    pub channel: Option<u8>,
    /// Transpose in semitones
    pub transpose: i8,
    /// First MIDI note of the keyboard (learned)
    pub first_midi_note: Option<u8>,
    /// Last MIDI note of the keyboard (learned)
    pub last_midi_note: Option<u8>,
}

impl MidiChannelMapping {
    /// Accepteert dit klavier dit kanaal + deze (fysieke) noot?
    /// - channel None = expliciet "alle kanalen".
    /// - Het ingeleerde toetsbereik begrenst: noten erbuiten (bv. pistons die
    ///   MIDI-noten sturen op hetzelfde kanaal) horen NIET bij dit klavier en
    ///   mogen dus niet als orgelnoten klinken.
    pub fn accepts(&self, channel: u8, note: u8) -> bool {
        if let Some(ch) = self.channel {
            if ch != channel { return false; }
        }
        if let Some(f) = self.first_midi_note {
            if note < f { return false; }
        }
        if let Some(l) = self.last_midi_note {
            if note > l { return false; }
        }
        true
    }
}

/// Result of learning a keyboard range
#[derive(Debug, Clone)]
pub struct LearnedKeyboardRange {
    pub channel: u8,
    pub first_note: u8,
    pub last_note: u8,
    pub transpose: i8,
}

/// MIDI binding for a preset button
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPresetBinding {
    /// Preset number (1-8)
    pub preset_num: u8,
    /// MIDI message type that triggers this preset
    pub trigger: MidiPresetTrigger,
}

/// Types of MIDI events that can trigger a preset
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiPresetTrigger {
    /// Een specifieke noot. channel None = elk kanaal (oude, her-in-te-leren
    /// koppelingen); mét kanaal vuurt de preset alleen op dat kanaal — anders
    /// triggert een piston-noot ook bij dezelfde toets op een klavier
    /// (presets sprongen "alle kanten op" tijdens het spelen).
    Note { channel: Option<u8>, note: u8 },
    /// A control change message
    ControlChange { channel: Option<u8>, controller: u8, value: u8 },
    /// Program change
    ProgramChange { channel: Option<u8>, program: u8 },
}

/// Swell pedal MIDI binding for a division
#[derive(Debug, Clone)]
pub struct SwellBinding {
    pub division_name: String,
    pub division_index: u8,
    pub channel: u8,
    pub cc_num: u8,
    pub min_val: u8,
    pub max_val: u8,
    /// Spiegelbeeld: keert de pedaalrichting om (open ↔ dicht). Handig voor pedalen
    /// die fysiek de andere kant op werken dan de software verwacht.
    pub invert: bool,
    /// Laatst ontvangen ruwe CC-waarde van deze trede. Na een orgel-herlaad of
    /// audio-wissel (die de audio-gains hard op 1.0 zetten) wordt hieruit de
    /// zwelstand teruggezet — anders klonk een dichte kast na de wissel vol open
    /// en sprong het volume bij de eerste pedaalbeweging tot 20 dB.
    pub last_value: Option<u8>,
}

/// Zwelstand 0..1 uit een ruwe CC-waarde en het ingestelde bereik (min/max
/// worden geordend als vangnet; invert spiegelt). Eén implementatie voor de
/// MIDI-thread, het herstel na herlaad en de tests.
pub fn swell_gain_from_cc(value: u8, min_val: u8, max_val: u8, invert: bool) -> f32 {
    let lo = min_val.min(max_val) as f32;
    let hi = max_val.max(min_val) as f32;
    let range = (hi - lo).max(1.0);
    let mut normalized = ((value as f32 - lo) / range).clamp(0.0, 1.0);
    if invert { normalized = 1.0 - normalized; }
    normalized
}

/// Ruwe crescendo-CC → pedaalwaarde 0..127 binnen het ingestelde bereik
/// (min/max geordend als vangnet) en gespiegeld bij invert.
pub fn crescendo_mapped_value(value: u8, min_val: u8, max_val: u8, invert: bool) -> u8 {
    let lo = min_val.min(max_val) as f32;
    let hi = max_val.max(min_val) as f32;
    let span = (hi - lo).max(1.0);
    let scaled = (((value as f32 - lo) / span).clamp(0.0, 1.0) * 127.0) as u8;
    if invert { 127u8.saturating_sub(scaled) } else { scaled }
}

/// Pedaalwaarde 0..127 → trap 0..=n (0 = uit; dode zone 0..3; 4..=127 lineair
/// over 1..=n). Zonder hysterese — zie crescendo_next_stage.
pub fn crescendo_stage_for(mapped: u8, num_stages: usize) -> u8 {
    if num_stages == 0 || mapped < 4 { return 0; }
    let s = ((mapped as usize - 4) * num_stages) / 124 + 1;
    s.min(num_stages) as u8
}

/// Laagste en hoogste pedaalwaarde die (zonder hysterese) op `stage` uitkomen.
pub fn crescendo_stage_bounds(stage: u8, num_stages: usize) -> (u8, u8) {
    let mut lo = u8::MAX;
    let mut hi = 0u8;
    for m in 0..=127u8 {
        if crescendo_stage_for(m, num_stages) == stage {
            lo = lo.min(m);
            hi = hi.max(m);
        }
    }
    if lo == u8::MAX { (0, 0) } else { (lo, hi) }
}

/// Hysterese-breedte (Schmitt) in CC-eenheden rond een trapgrens: een potmeter
/// die op een grens ruist klapperde anders registers aan/uit.
pub const CRESCENDO_HYSTERESIS: u8 = 2;

/// Schmitt-beslissing: nieuwe trap voor pedaalwaarde `mapped` gegeven de
/// huidige trap, of None om te blijven staan. Omhoog pas H eenheden voorbij de
/// bovengrens van de huidige trap (of bij de volle pedaalweg), omlaag pas H
/// eenheden onder de ondergrens; de dode zone (mapped < 4 → trap 0) geldt
/// altijd, zodat een losgelaten trede altijd op 0 komt. Sprongen over meerdere
/// trappen blijven mogelijk (de doeltrap wordt uit `mapped` herberekend).
pub fn crescendo_next_stage(mapped: u8, current: u8, num_stages: usize) -> Option<u8> {
    let target = crescendo_stage_for(mapped, num_stages);
    if target == current { return None; }
    if (current as usize) > num_stages { return Some(target); } // matrix gekrompen
    let h = CRESCENDO_HYSTERESIS;
    if target > current {
        let (_, hi) = crescendo_stage_bounds(current, num_stages);
        if mapped == 127 || mapped > hi.saturating_add(h) { Some(target) } else { None }
    } else if target == 0 {
        Some(0)
    } else {
        let (lo, _) = crescendo_stage_bounds(current, num_stages);
        if mapped.saturating_add(h) < lo { Some(target) } else { None }
    }
}

/// Registers/koppels van trap `stage` (1-based; 0 = leeg). Een lege trap erft
/// de dichtstbijzijnde lagere gevulde trap, zodat een matrix waarvan alleen de
/// eerste k kolommen gevuld zijn bij dieper intrappen niet alles wegtrekt.
pub fn crescendo_effective_stops(stages: &[Vec<String>], stage: usize) -> Vec<String> {
    (1..=stage.min(stages.len())).rev()
        .find_map(|s| stages.get(s - 1).filter(|v| !v.is_empty()).cloned())
        .unwrap_or_default()
}

/// Commands to send to the MIDI thread
pub enum MidiCommand {
    ListDevices(Sender<Vec<MidiDeviceInfo>>),
    Connect(String, Sender<Result<(), String>>),
    ConnectAll(Sender<Result<usize, String>>),
    Disconnect,
    SetChannelMapping(Vec<MidiChannelMapping>),
    LearnChannel(String, Sender<Option<u8>>), // division name, returns learned channel (legacy)
    /// Learn keyboard range: first note (lowest), then second note (highest)
    /// Returns the learned channel, first note, last note, and calculated transpose
    LearnKeyboardRange(String, u8, Sender<Option<LearnedKeyboardRange>>), // division name, first sample note (e.g. 36 for C)
    SetPresetBindings(Vec<MidiPresetBinding>),
    LearnPresetBinding(u8, Sender<Option<MidiPresetTrigger>>), // preset number, returns learned trigger
    /// Learn swell pedal binding: division name, division index, response
    LearnSwellPedal(String, u8, Sender<Option<SwellBinding>>),
    /// Clear swell pedal binding for a division
    ClearSwellPedal(String),
    /// Wacht op een pedaalstand: beweeg + stilhouden → (kanaal, cc, waarde).
    /// Filter Some((ch,cc)) beperkt fase 2 tot dezelfde trede. Voor de
    /// stapsgewijze zwel-/crescendo-inleer-popup.
    LearnPedalPosition(Option<(u8, u8)>, Sender<Option<(u8, u8, u8)>>),
    /// Wacht op één toetsaanslag: (kanaal, noot). Voor de stapsgewijze
    /// klavier-inleer-popup (laagste toets → groen → hoogste toets).
    LearnSingleNote(Sender<Option<(u8, u8)>>),
    /// Learn crescendo pedal CC binding
    LearnCrescendoPedal(Sender<Option<(u8, u8)>>), // returns (channel, cc_num)
    /// Lopende trede-inleer afbreken (Annuleren/Opnieuw in de popup). De
    /// leerlus zelf kijkt elke 20 ms naar de `learn_cancel`-vlag; dit commando
    /// wordt ná de afgebroken lus verwerkt en wist de vlag weer, zodat een
    /// direct daarna gestarte nieuwe inleer niet met een oude cancel begint.
    CancelLearn,
    /// Lopende archief-take geforceerd afsluiten en synchroon wegschrijven
    /// (afsluiten app / test-API); antwoord = geschreven pad.
    ArchiveFlush(Sender<Option<std::path::PathBuf>>),
    Shutdown,
}

/// Audio device info
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rates: Vec<u32>,
    pub channels: Vec<u16>,
}

/// Eerlijk resultaat van een audio-uitvoerwissel. De frontend beslist hiermee:
/// orgel herladen wanneer `player_rebuilt` (nieuwe audio-thread = lege
/// sample-maps), profiel pas actief markeren wanneer `switched`, en anders de
/// `message` tonen. Vervangt de oude aanpak waarbij een fallback op de default
/// host als "gelukt" werd gemeld en het geluid stil op het verkeerde apparaat
/// belandde.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SwitchOutcome {
    /// De gevráágde configuratie draait nu echt.
    pub switched: bool,
    /// Er is een nieuwe audio-thread: de aanroeper moet het orgel herladen.
    pub player_rebuilt: bool,
    /// Wat er nu werkelijk speelt (host/apparaat).
    pub actual_host: String,
    pub actual_device: String,
    /// Huidig orgel-id (voor de herlaad-stap), indien geladen.
    pub organ_id: Option<String>,
    /// Nederlandse uitleg wanneer de wissel niet (volledig) slaagde.
    pub message: Option<String>,
    /// De wissel kon niet (op tijd) starten of afronden: er liep al een wissel
    /// (gate bezet) of de harde deadline werd bereikt. De UI reset dan zijn
    /// "bezig"-vlag en toont een geduld-melding i.p.v. te bevriezen. `switched`
    /// en `player_rebuilt` zijn dan false; de huidige uitgang blijft intact.
    #[serde(default)]
    pub busy: bool,
}

/// Bouw een AudioPlayer met een retry-ladder. ASIO4ALL en WASAPI geven hun
/// apparaten asynchroon vrij na het sluiten van een stream/driver; direct
/// opnieuw openen kan dan falen terwijl het een halve seconde later gewoon
/// lukt. `delays_ms[i]` is de wachttijd vóór poging i (eerste meestal 0).
/// Laatst-bewezen-werkende uitvoerconfiguratie (procesbreed). Gevuld bij elke
/// geslaagde player-build; gebruikt als herstel-anker wanneer een wissel faalt
/// en er geen levende player (meer) is om de "vorige uitgang" uit af te lezen.
static LAST_GOOD_OUTPUT: parking_lot::Mutex<Option<AudioOutputConfig>> = parking_lot::Mutex::new(None);

fn build_player_with_retries(cfg: &AudioOutputConfig, delays_ms: &[u64]) -> Result<AudioPlayer, String> {
    build_player_with_retries_deadline(cfg, delays_ms, None)
}

/// Als `deadline` gezet is en verstreken, worden de resterende (wacht-)pogingen
/// overgeslagen — maar er wordt altijd minstens één poging gedaan. Zo blijft een
/// enkele wissel binnen zijn tijdsbudget zonder ooit zonder poging te eindigen.
fn build_player_with_retries_deadline(cfg: &AudioOutputConfig, delays_ms: &[u64], deadline: Option<std::time::Instant>) -> Result<AudioPlayer, String> {
    let mut last_err = String::from("geen poging gedaan");
    for (i, delay) in delays_ms.iter().enumerate() {
        // Voorbij de deadline: geen verdere wacht-rungs meer (poging 0 is al gedaan).
        if i > 0 {
            if let Some(dl) = deadline {
                if std::time::Instant::now() >= dl {
                    tracing::warn!("Audio-wissel deadline bereikt; resterende pogingen voor {:?}/{:?} overgeslagen",
                        cfg.host_name, cfg.device_name);
                    break;
                }
            }
        }
        if *delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(*delay));
        }
        match AudioPlayer::new(cfg.clone()) {
            Ok(p) => {
                if i > 0 {
                    tracing::info!("Audio-uitgang {:?}/{:?} gebouwd bij poging {}",
                        cfg.host_name, cfg.device_name, i + 1);
                }
                *LAST_GOOD_OUTPUT.lock() = Some(cfg.clone());
                return Ok(p);
            }
            Err(e) => {
                tracing::warn!("Poging {}/{} voor {:?}/{:?} faalde: {}",
                    i + 1, delays_ms.len(), cfg.host_name, cfg.device_name, e);
                last_err = e;
            }
        }
    }
    Err(last_err)
}

/// Shared application state
/// Zie apply_crescendo_stage_inner: één toepassing tegelijk (procesbreed;
/// er is één AppState).
static CRESCENDO_APPLY: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

#[derive(Clone)]
pub struct AppState {
    /// Audio player (handles playback)
    pub audio_player: Arc<RwLock<Option<AudioPlayer>>>,
    /// MIDI command sender (thread-safe)
    pub midi_tx: Sender<MidiCommand>,
    /// MIDI connected status
    pub midi_connected: Arc<RwLock<bool>>,
    /// Organ definition (parsed ODF)
    pub organ_definition: Arc<RwLock<Option<OrganDefinition>>>,
    /// Loaded organ with samples
    pub loaded_organ: Arc<RwLock<Option<LoadedOrgan>>>,
    /// Organ info for UI
    pub loaded_organ_info: Arc<RwLock<Option<OrganInfoDto>>>,
    /// Currently drawn (active) stops
    pub drawn_stops: Arc<RwLock<Vec<String>>>,
    /// MIDI channel mappings per division
    pub midi_mappings: Arc<RwLock<Vec<MidiChannelMapping>>>,
    /// MIDI bindings for presets
    pub preset_bindings: Arc<RwLock<Vec<MidiPresetBinding>>>,
    /// Channel to notify frontend of preset triggers
    pub preset_trigger_tx: Sender<u8>,
    /// Receiver for preset triggers (frontend polls this)
    pub preset_trigger_rx: Receiver<u8>,
    /// Currently active couplers
    pub active_couplers: Arc<RwLock<Vec<String>>>,
    /// Per-division gain for swell boxes (0.0-1.0, indexed by division order)
    pub division_gains: Arc<RwLock<Vec<f32>>>,
    /// Swell pedal MIDI bindings
    pub swell_bindings: Arc<RwLock<Vec<SwellBinding>>>,
    /// Currently held MIDI notes: (channel, note, velocity) for re-triggering on stop toggle
    pub held_notes: Arc<RwLock<Vec<(u8, u8, u8)>>>,
    /// Organ library (persistent)
    pub organ_library: Arc<RwLock<OrganLibrary>>,
    /// App data directory for persistent storage
    pub app_data_dir: PathBuf,
    /// Currently loaded organ ID (for saving settings on close)
    pub current_organ_id: Arc<RwLock<Option<String>>>,
    /// Crescendo configuration: stages[i] = list of stop IDs that should be ON at stage i
    pub crescendo_stages: Arc<RwLock<Vec<Vec<String>>>>,
    /// Current crescendo stage (0 = off)
    pub crescendo_stage: Arc<RwLock<u8>>,
    /// Whether crescendo is enabled
    pub crescendo_enabled: Arc<RwLock<bool>>,
    /// Crescendo MIDI binding: (channel, cc_num, min_val, max_val)
    pub crescendo_binding: Arc<RwLock<Option<(u8, u8, u8, u8, bool)>>>,
    /// Stops currently activated by the crescendo (additive on top of manual stops)
    pub crescendo_active_stops: Arc<RwLock<Vec<String>>>,
    /// Aantal kolommen in de crescendo-editor (UI-voorkeur, per orgel
    /// opgeslagen; `crescendo_stages.len()` kan kleiner zijn).
    pub crescendo_num_stages: Arc<RwLock<u8>>,
    /// Annuleer-vlag voor een lopende trede-inleer (zie MidiCommand::CancelLearn).
    pub learn_cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Laatste zwelstanden (divisie → ruwe CC) van het orgel dat bij een
    /// (her)laad wordt vervangen, met zijn orgel-id. Herlaad/wissel van
    /// HETZELFDE orgel neemt ze over (de opgeslagen waarde kan ouder zijn dan
    /// de laatste pedaalbeweging); een ander orgel laat ze vallen.
    pub swell_last_stash: Arc<RwLock<Option<(String, Vec<(String, u8)>)>>>,
    /// MIDI recording state
    pub midi_recording: Arc<RwLock<Option<MidiRecording>>>,
    /// Live-notatie-scores per notatievenster. Elke Score is een bewerkbare
    /// partituur met lagen + takes; het notatievenster leest hem via commands
    /// (notation_get_score / notation_get_musicxml) en muteert via edit-
    /// commands. Tijdens opname schrijft `capture_midi_event` de events in de
    /// armed take van de armed layer van de armed Score (score_id).
    pub notation_scores: Arc<RwLock<std::collections::HashMap<u32, crate::notation::Score>>>,
    /// Score waarvoor de MIDI-opname op dit moment live opneemt (None = geen
    /// live-notatie). Slechts één Score tegelijk staat armed.
    pub notation_armed_score: Arc<RwLock<Option<u32>>>,
    /// Volgnummer voor de eerstvolgende nieuwe Score-ID.
    pub notation_next_id: Arc<RwLock<u32>>,
    /// Open note-events per (score_id, layer_id, take_id, channel, note) →
    /// starttijd in microseconden vanaf recording-start. Bij NoteOff pairen we
    /// deze met de eindtijd en emitten een `note-added` event.
    pub notation_open_notes: Arc<RwLock<std::collections::HashMap<(u32, u32, u32, u8, u8), u64>>>,
    /// Starttijd van de huidige notatie-opname (vanaf "arm & wait for first note",
    /// gezet bij de eerste inkomende NoteOn zodra armed). None = geen actieve opname.
    pub notation_start_time: Arc<RwLock<Option<std::time::Instant>>>,
    /// AppHandle-spiegel voor de MIDI-thread (die kent `state.app_handle` niet
    /// direct); wordt geset door commands.rs::notation_ready zodra de setup klaar is.
    pub notation_app_handle: Arc<RwLock<Option<tauri::AppHandle>>>,
    /// Score-id in stapinvoer-modus (notatievenster): de midi-thread emit dan
    /// step-note-events zodat gespeelde toetsen op de invoercursor geplaatst
    /// worden. None = geen stapinvoer actief.
    pub notation_step_input: Arc<RwLock<Option<u32>>>,
    /// Serialiseert orgel-loads en audio-wissels onderling: een wissel midden in
    /// een load gooit anders net-geregistreerde preload-buffers weg (nieuwe lege
    /// audio-thread) en mist de reload omdat current_organ_id nog niet gezet is;
    /// twee interleavende wissels kunnen elkaars player/prefs overschrijven.
    pub load_switch_gate: Arc<parking_lot::Mutex<()>>,
    /// Tauri AppHandle voor events naar de frontend (laad-voortgang e.d.).
    /// None bij headless gebruik (test-API vóór setup) — events worden dan
    /// stilletjes overgeslagen.
    pub app_handle: Arc<RwLock<Option<tauri::AppHandle>>>,
    /// Per-pipe voicing overrides: (stop_id, pipe_num) -> (volume_db, pitch_cents)
    pub pipe_voicings: Arc<RwLock<std::collections::HashMap<(u32, u32), (f32, f32)>>>,
    /// Per-divisie output-kanalen: lijst fysieke kanaalindices (leeg = standaard voorste paar 0/1).
    /// Opeenvolgende kanalen vormen (L,R)-paren; een los laatste kanaal krijgt mono.
    pub division_output_channels: Arc<RwLock<Vec<Vec<u8>>>>,
    /// Kanaal-override van het actieve uitvoerprofiel (speakers/hoofdtelefoon):
    /// (divisienaam → kanaallijst). Zolang deze actief is wint hij van de per-orgel
    /// opgeslagen routing (apply_saved_output_channels past hem als laatste toe) en
    /// wordt de per-orgel routing NIET overschreven bij opslaan. None = geen override.
    pub profile_channel_override: Arc<RwLock<Option<Vec<(String, Vec<u8>)>>>>,
    /// C/Cis-lade spreiding aan/uit per divisie (even tonen ene kant, oneven andere).
    pub division_ccis_enabled: Arc<RwLock<Vec<bool>>>,
    /// Globale C/Cis-parameters: (sterkte 0..1, afval-met-toonhoogte 0..1, kanten omdraaien).
    pub ccis_spread: Arc<RwLock<(f32, f32, bool)>>,
    /// MP3-recorder (live opname van audio-output, vanuit het registerscherm).
    pub recorder: Arc<crate::recorder::Recorder>,
    /// Of de getrokken registratie ("laatste stand") bij het laden van een orgel
    /// hersteld wordt. Default false = schone start (geen registers aan bij opstart).
    pub restore_registration: Arc<RwLock<bool>>,
    /// Live registratie-snapshot rond een audio-uitvoer-wissel:
    /// (orgel-id, getrokken registers, actieve koppels). De verplichte orgel-
    /// herlaad na een wissel wist drawn_stops onvoorwaardelijk en herstelde ze
    /// alleen wanneer de losse "herstel laatste registratie"-toggle aanstond —
    /// een hoofdtelefoon-wissel gooide dus stilletjes alle registers uit. De
    /// eerstvolgende load van HETZELFDE orgel consumeert dit snapshot en zet de
    /// live registratie terug, onafhankelijk van die toggle.
    pub pending_registration_restore: Arc<RwLock<Option<(Option<String>, Vec<String>, Vec<String>)>>>,
    /// Wind-groep toewijzing per divisie (0..31). Default: divisie i → groep i (elke divisie eigen groep).
    /// Divisies in dezelfde groep delen één wind-reservoir (gecombineerd voice-count).
    pub division_wind_groups: Arc<RwLock<Vec<u8>>>,
    /// Aantal actieve wind-groepen (1..8). Beperkt de keuze in de UI; audio thread blijft 32 supporten.
    pub num_wind_groups: Arc<RwLock<u8>>,
    /// MIDI file player (afspelen van .mid bestanden)
    pub midi_player: Arc<RwLock<Option<vpo_midi::MidiFilePlayer>>>,
    /// Bluetooth LE MIDI manager (lazy init on first scan)
    pub ble_midi: Arc<RwLock<Option<vpo_midi::BleMidiManager>>>,
    /// BLE MIDI message channel: sender (passed to BleMidiManager) + receiver (read by MIDI thread)
    pub ble_message_tx: crossbeam_channel::Sender<vpo_midi::MidiMessage>,
    /// BLE learn tap: receiver for learn handlers to read BLE CC messages while still routing to audio
    pub ble_learn_rx: crossbeam_channel::Receiver<vpo_midi::MidiMessage>,
    /// BLE MIDI message counter (for UI activity indicator)
    pub ble_message_count: Arc<std::sync::atomic::AtomicU64>,
    /// Master volume (dB) van het huidige orgel — runtime-spiegel zodat het per orgel
    /// opgeslagen kan worden in OrganSettings. None = nog niet gezet deze sessie.
    pub master_volume_db: Arc<RwLock<Option<f32>>>,
    /// Actieve temperament-keuze + fijnstemming van het huidige orgel (naam/cents/fine-tune),
    /// runtime-spiegel voor per-orgel opslag. None = nog niet gezet deze sessie.
    pub temperament_settings: Arc<RwLock<Option<TemperamentSettingsSaved>>>,
    /// Reverb-configuratie van het huidige orgel (type/mix/preset/rt60/...), runtime-spiegel
    /// voor per-orgel opslag. Door de frontend gezet via persist_reverb_config.
    pub reverb_settings: Arc<RwLock<Option<ReverbSettingsSaved>>>,
    /// Parametric-EQ-configuratie van het huidige orgel, runtime-spiegel voor per-orgel opslag.
    /// Door de frontend gezet via set_parametric_eq.
    pub eq_settings: Arc<RwLock<Option<EqSettingsSaved>>>,
    /// Per-divisie DSP-spiegels (per divisienaam) voor per-orgel opslag. Gewist bij orgelwissel
    /// (reset_division_settings) → geen naam-collisie tussen orgels. Door de frontend gezet via
    /// set_division_pan / set_swell_config / set_tremulant_lfo.
    /// pan: -1..1.
    pub division_pans: Arc<RwLock<std::collections::HashMap<String, f32>>>,
    /// (min_db, filter_cutoff) per divisie.
    pub division_swell_configs: Arc<RwLock<std::collections::HashMap<String, (f32, f32)>>>,
    /// (enabled, rate, amp_depth, pitch_depth) per divisie.
    pub division_tremulants: Arc<RwLock<std::collections::HashMap<String, (bool, f32, f32, f32)>>>,
    /// Wind-groep-config (enabled, reservoir, damping, max_sag) per groep-index (0..31).
    pub wind_group_configs: Arc<RwLock<std::collections::HashMap<u8, (bool, f32, f32, f32)>>>,
    /// MIDI-uit terugkoppeling naar de fysieke console (registerlampen/display).
    /// Fase 1: gevoed door de UI via feedback_apply. Zie feedback.rs.
    pub feedback: Arc<parking_lot::Mutex<crate::feedback::FeedbackManager>>,
    /// Automatisch MIDI-archief: instellingen + live status als atomics; de
    /// buffer zelf leeft lokaal in de MIDI-thread (ArchiveRecorder).
    pub midi_archive: Arc<crate::midi_archive::MidiArchiveShared>,
    // ---- Afstandsbediening in het netwerk (0.7.38) ----
    /// Divisienaam → tremulant live aan/uit (spiegel van de Console-state tremActive;
    /// gevuld door set_tremulant/set_tremulant_lfo, gewist bij orgelwissel).
    pub tremulant_live: Arc<RwLock<std::collections::HashMap<String, bool>>>,
    /// Setzerstand gerapporteerd door SetzerBar (hoofdvenster).
    pub setzer_mirror: Arc<RwLock<SetzerMirror>>,
    /// Draaiende remote-server (None = uit).
    pub remote_api: Arc<parking_lot::Mutex<Option<crate::test_api::ApiServer>>>,
    /// Laatste start-fout van de afstandsbediening (bind mislukt) voor de UI.
    pub remote_last_error: Arc<RwLock<Option<String>>>,
    /// Heartbeat van de piston-consument: tijdstip van de laatste poll_preset_trigger.
    /// SetzerBar pollt elke 100 ms zolang het Orgel-tabblad open staat; de
    /// afstandsbediening weigert acties (409) als er geen consument is.
    pub preset_poll_seen: Arc<RwLock<Option<std::time::Instant>>>,
    // ---- Gestapelde ranks en microfoonperspectieven (0.7.38) ----
    /// Perspectieven van het geladen orgel (labels in eerste-voorkomen-
    /// volgorde): aan/uit (= laden bij de volgende load), live gain, of het
    /// nú geladen is, het gain-slot in de audio-thread en het aantal pijpen.
    /// Gezet in beide laadroutes NÁ reset_organ_scoped_state; leeg bij
    /// orgels zonder perspectieven.
    pub perspectives: Arc<RwLock<Vec<PerspectiveRuntime>>>,
    /// Lagen per (niet-ruis-)stop van het geladen orgel — diagnose (/ranks,
    /// layered_stops), gevuld in beide laadroutes (ook custom sets, waar
    /// organ_definition None/verouderd is).
    pub rank_summary: Arc<RwLock<Vec<RankSummary>>>,
}

/// Runtime-staat van één microfoonperspectief (zie AppState::perspectives).
#[derive(Debug, Clone, PartialEq)]
pub struct PerspectiveRuntime {
    pub name: String,
    /// Gewenst: laden bij de (volgende) load.
    pub enabled: bool,
    pub gain_db: f32,
    /// Nu geladen (enabled op het moment van de load).
    pub loaded: bool,
    /// Gain-slot 1..15 in de audio-thread (0 = geen perspectief).
    pub slot: u8,
    /// Aantal niet-lege pijpen in lagen met dit label.
    pub pipe_count: usize,
}

/// Lagen van één stop (diagnose): laag 0 = primair met `primary_perspective`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RankSummary {
    pub stop_id: u32,
    pub dto_id: String,
    pub name: String,
    pub layers: Vec<RankLayerSummary>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RankLayerSummary {
    pub index: u8,
    pub name: String,
    pub perspective: Option<String>,
    pub pipes_nonempty: usize,
}

impl RankSummary {
    /// Stop met ≥1 echte extra rank (laag zonder perspectief).
    pub fn is_stacked(&self) -> bool {
        self.layers.iter().any(|l| l.index > 0 && l.perspective.is_none())
    }
}

/// MIDI recording state
pub struct MidiRecording {
    pub events: Vec<(u64, u8, u8, u8, u8)>, // (timestamp_us, status, data1, data2, channel)
    pub start_time: std::time::Instant,
    /// false zodra de gebruiker op stop drukt: capture stopt, maar de events
    /// blijven beschikbaar voor opslaan/afspelen tot clear of een nieuwe opname.
    pub active: bool,
    /// Duur op het moment van stoppen — bevriest de `seconds`-status zodat die
    /// na de stop niet eeuwig doorloopt (None zolang de opname actief is).
    pub stopped_elapsed: Option<f64>,
}

/// Spiegel van de setzerstand in het hoofdvenster (SetzerBar rapporteert via
/// report_setzer_state); gelezen door GET /state van de afstandsbediening.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SetzerMirror {
    pub level: u8,
    pub preset: i32,
    pub set_mode: bool,
    pub data: Vec<bool>,
}

impl Default for SetzerMirror {
    fn default() -> Self {
        Self { level: 1, preset: -1, set_mode: false, data: vec![false; 10] }
    }
}

impl AppState {
    pub fn new(app_data_dir: PathBuf) -> Self {
        // Load organ library from disk
        let organ_library = Arc::new(RwLock::new(load_library(&app_data_dir)));
        // Load persisted audio output preferences and create the audio player
        let audio_prefs = load_audio_prefs(&app_data_dir);
        // Automatisch MIDI-archief: instellingen laden, gedeelde atomics aanmaken.
        let archive_prefs = crate::midi_archive::load_prefs(&app_data_dir);
        let midi_archive = Arc::new(crate::midi_archive::MidiArchiveShared::from_prefs(&archive_prefs));
        // Polyfonie-kap en stereo-laden vóór de eerste audio/orgel-load zetten.
        crate::audio::set_polyphony_target(
            audio_prefs.polyphony.map(|n| n as usize).unwrap_or(vpo_audio::DEFAULT_POLYPHONY));
        vpo_sampler::set_stereo_loading(audio_prefs.stereo_samples.unwrap_or(true));
        let saved_cfg = AudioOutputConfig {
            host_name: audio_prefs.host.clone(),
            device_name: audio_prefs.device.clone(),
            buffer_frames: audio_prefs.buffer_frames,
        };
        let wants_asio = audio_prefs.host.as_deref()
            .map(|h| h.eq_ignore_ascii_case("asio"))
            .unwrap_or(false);
        let audio_player = if wants_asio {
            // ASIO kóúd starten levert op ASIO4ALL een stille stream op: de init
            // slaagt, maar na een korte burst stoppen de callbacks — óók met een
            // WASAPI-warm-up van 1-5s vooraf (geverifieerd 2026-07-03). Dezelfde
            // wissel werkt wél betrouwbaar zodra de app volledig draait. Start
            // daarom altijd op de default host; main.rs voert ~10s na de start de
            // echte ASIO-wissel uit via switch_audio_output (de bewezen route).
            let _ = saved_cfg; // ASIO-voorkeur wordt door main.rs uitgesteld toegepast
            tracing::info!("ASIO-voorkeur gevonden: start op default host, ASIO-wissel volgt na de app-start");
            match AudioPlayer::new_default() {
                Ok(player) => Some(player),
                Err(e) => {
                    tracing::error!("Default audio init failed: {}", e);
                    None
                }
            }
        } else {
            match AudioPlayer::new(saved_cfg) {
                Ok(player) => {
                    tracing::info!("Audio player initialized");
                    Some(player)
                }
                Err(e) => {
                    // Saved host/device failed — fall back to the system default
                    // so the app always has working audio at startup.
                    tracing::warn!("Audio init with saved prefs failed ({}); falling back to default device", e);
                    match AudioPlayer::new_default() {
                        Ok(player) => Some(player),
                        Err(e2) => {
                            tracing::error!("Default audio init also failed: {}", e2);
                            None
                        }
                    }
                }
            }
        };

        // Create channel for MIDI commands
        let (midi_tx, midi_rx) = bounded::<MidiCommand>(64);

        // Create channel for preset triggers (from MIDI thread to frontend)
        let (preset_trigger_tx, preset_trigger_rx) = bounded::<u8>(16);

        // BLE MIDI message channel: BLE thread → MIDI thread
        let (ble_message_tx, ble_message_rx) = bounded::<vpo_midi::MidiMessage>(1024);

        // MIDI file player + zijn output receiver (gerouteerd naar MIDI thread net als live MIDI)
        let midi_file_player = vpo_midi::MidiFilePlayer::new();
        let player_rx = midi_file_player.out_receiver();
        let midi_player_arc: Arc<RwLock<Option<vpo_midi::MidiFilePlayer>>> = Arc::new(RwLock::new(Some(midi_file_player)));
        // Learn-mode tap: when active, BLE messages are also forwarded here for the learn handlers
        let (ble_learn_tx, ble_learn_rx) = bounded::<vpo_midi::MidiMessage>(256);
        let ble_learn_rx_for_state = ble_learn_rx.clone();
        let ble_message_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let ble_count_for_thread = ble_message_count.clone();

        // Shared state for MIDI thread
        let audio_player_arc = Arc::new(RwLock::new(audio_player));
        let midi_connected = Arc::new(RwLock::new(false));
        let organ_definition = Arc::new(RwLock::new(None));
        let loaded_organ_info = Arc::new(RwLock::new(None));
        let drawn_stops = Arc::new(RwLock::new(Vec::new()));
        let midi_mappings = Arc::new(RwLock::new(Vec::new()));
        let preset_bindings = Arc::new(RwLock::new(Vec::new()));
        let active_couplers = Arc::new(RwLock::new(Vec::new()));
        let division_gains = Arc::new(RwLock::new(vec![1.0; 32]));
        let swell_bindings = Arc::new(RwLock::new(Vec::new()));
        let held_notes: Arc<RwLock<Vec<(u8, u8, u8)>>> = Arc::new(RwLock::new(Vec::new()));
        // Crescendo state - shared between AppState and MIDI thread
        let crescendo_stages: Arc<RwLock<Vec<Vec<String>>>> = Arc::new(RwLock::new(Vec::new()));
        let crescendo_stage: Arc<RwLock<u8>> = Arc::new(RwLock::new(0));
        let crescendo_enabled: Arc<RwLock<bool>> = Arc::new(RwLock::new(false));
        let crescendo_binding: Arc<RwLock<Option<(u8, u8, u8, u8, bool)>>> = Arc::new(RwLock::new(None));
        let crescendo_active_stops: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
        let learn_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let learn_cancel_for_midi = learn_cancel.clone();
        // MIDI-opname state - gedeeld met de MIDI-thread (capture-hook in de message-loops)
        let midi_recording: Arc<RwLock<Option<MidiRecording>>> = Arc::new(RwLock::new(None));

        // Clone Arcs for MIDI thread
        let audio_for_midi = audio_player_arc.clone();
        let connected_for_midi = midi_connected.clone();
        let def_for_midi = organ_definition.clone();
        let info_for_midi = loaded_organ_info.clone();
        let stops_for_midi = drawn_stops.clone();
        let mappings_for_midi = midi_mappings.clone();
        let bindings_for_midi = preset_bindings.clone();
        let trigger_tx_for_midi = preset_trigger_tx.clone();
        let couplers_for_midi = active_couplers.clone();
        let gains_for_midi = division_gains.clone();
        let swell_for_midi = swell_bindings.clone();
        let held_for_midi = held_notes.clone();
        let cresc_stages_for_midi = crescendo_stages.clone();
        let cresc_stage_for_midi = crescendo_stage.clone();
        let cresc_enabled_for_midi = crescendo_enabled.clone();
        let cresc_binding_for_midi = crescendo_binding.clone();
        let cresc_active_for_midi = crescendo_active_stops.clone();
        let recording_for_midi = midi_recording.clone();
        let archive_for_midi = midi_archive.clone();
        // Live-notatie: extra Arcs voor de midi-thread (armed score/laag/take
        // volgen, open-notes pairen, note-added-events emitten naar het
        // notatievenster).
        let notation_scores = Arc::new(RwLock::new(std::collections::HashMap::<u32, crate::notation::Score>::new()));
        let notation_armed_score = Arc::new(RwLock::new(None::<u32>));
        let notation_open_notes = Arc::new(RwLock::new(std::collections::HashMap::<(u32, u32, u32, u8, u8), u64>::new()));
        let notation_start_time = Arc::new(RwLock::new(None::<std::time::Instant>));
        let notation_app_handle = Arc::new(RwLock::new(None::<tauri::AppHandle>));
        // Stapinvoer (0.7.15): score-id dat in stap-modus staat; de midi-thread
        // emit dan step-events naar het notatievenster (naast het gewone spelen).
        let notation_step_input = Arc::new(RwLock::new(None::<u32>));
        let notation_scores_for_midi = notation_scores.clone();
        let notation_armed_for_midi = notation_armed_score.clone();
        let notation_open_for_midi = notation_open_notes.clone();
        let notation_start_for_midi = notation_start_time.clone();
        let notation_app_for_midi = notation_app_handle.clone();
        let notation_step_for_midi = notation_step_input.clone();

        // Start MIDI management thread
        thread::spawn(move || {
            Self::midi_thread(
                midi_rx,
                audio_for_midi,
                connected_for_midi,
                def_for_midi,
                info_for_midi,
                stops_for_midi,
                mappings_for_midi,
                bindings_for_midi,
                trigger_tx_for_midi,
                couplers_for_midi,
                gains_for_midi,
                swell_for_midi,
                held_for_midi,
                ble_message_rx,
                ble_learn_tx,
                ble_learn_rx,
                ble_count_for_thread,
                cresc_stages_for_midi,
                cresc_stage_for_midi,
                cresc_enabled_for_midi,
                cresc_binding_for_midi,
                cresc_active_for_midi,
                learn_cancel_for_midi,
                player_rx,
                recording_for_midi,
                archive_for_midi,
                notation_scores_for_midi,
                notation_armed_for_midi,
                notation_open_for_midi,
                notation_start_for_midi,
                notation_app_for_midi,
                notation_step_for_midi,
            );
        });

        Self {
            audio_player: audio_player_arc,
            midi_tx,
            midi_connected,
            organ_definition,
            loaded_organ: Arc::new(RwLock::new(None)),
            loaded_organ_info,
            drawn_stops,
            midi_mappings,
            preset_bindings,
            preset_trigger_tx,
            preset_trigger_rx,
            active_couplers,
            division_gains,
            swell_bindings,
            held_notes,
            organ_library,
            app_data_dir,
            current_organ_id: Arc::new(RwLock::new(None)),
            crescendo_stages,
            crescendo_stage,
            crescendo_enabled,
            crescendo_binding,
            crescendo_active_stops,
            crescendo_num_stages: Arc::new(RwLock::new(15)),
            learn_cancel,
            swell_last_stash: Arc::new(RwLock::new(None)),
            midi_recording,
            midi_archive,
            notation_scores,
            notation_armed_score,
            notation_next_id: Arc::new(RwLock::new(1)),
            notation_open_notes,
            notation_start_time,
            notation_app_handle,
            notation_step_input,
            load_switch_gate: Arc::new(parking_lot::Mutex::new(())),
            app_handle: Arc::new(RwLock::new(None)),
            pipe_voicings: Arc::new(RwLock::new(std::collections::HashMap::new())),
            division_output_channels: Arc::new(RwLock::new(vec![Vec::new(); 32])),
            profile_channel_override: Arc::new(RwLock::new(None)),
            division_ccis_enabled: Arc::new(RwLock::new(vec![false; 32])),
            ccis_spread: Arc::new(RwLock::new((0.7, 0.6, false))),
            recorder: Arc::new(crate::recorder::Recorder::new()),
            restore_registration: Arc::new(RwLock::new(false)),
            pending_registration_restore: Arc::new(RwLock::new(None)),
            // Identity-default: divisie 0 → groep 0, divisie 1 → groep 1, ... (elke divisie eigen groep)
            division_wind_groups: Arc::new(RwLock::new((0..32u8).collect())),
            // Default: 8 groepen actief — gebruiker beperkt zelf naar wens
            num_wind_groups: Arc::new(RwLock::new(8u8)),
            midi_player: midi_player_arc,
            ble_midi: Arc::new(RwLock::new(None)),
            ble_message_tx,
            ble_learn_rx: ble_learn_rx_for_state,
            ble_message_count,
            master_volume_db: Arc::new(RwLock::new(None)),
            temperament_settings: Arc::new(RwLock::new(None)),
            reverb_settings: Arc::new(RwLock::new(None)),
            eq_settings: Arc::new(RwLock::new(None)),
            division_pans: Arc::new(RwLock::new(std::collections::HashMap::new())),
            division_swell_configs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            division_tremulants: Arc::new(RwLock::new(std::collections::HashMap::new())),
            wind_group_configs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            feedback: Arc::new(parking_lot::Mutex::new(crate::feedback::FeedbackManager::new())),
            tremulant_live: Arc::new(RwLock::new(std::collections::HashMap::new())),
            setzer_mirror: Arc::new(RwLock::new(SetzerMirror::default())),
            remote_api: Arc::new(parking_lot::Mutex::new(None)),
            remote_last_error: Arc::new(RwLock::new(None)),
            preset_poll_seen: Arc::new(RwLock::new(None)),
            perspectives: Arc::new(RwLock::new(Vec::new())),
            rank_summary: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Save the organ library to disk
    pub fn save_library(&self) {
        let lib = self.organ_library.read();
        save_library(&self.app_data_dir, &lib);
    }

    /// MIDI management thread - handles all MIDI operations
    fn midi_thread(
        cmd_rx: Receiver<MidiCommand>,
        audio_player: Arc<RwLock<Option<AudioPlayer>>>,
        midi_connected: Arc<RwLock<bool>>,
        organ_definition: Arc<RwLock<Option<OrganDefinition>>>,
        loaded_organ_info: Arc<RwLock<Option<OrganInfoDto>>>,
        drawn_stops: Arc<RwLock<Vec<String>>>,
        midi_mappings: Arc<RwLock<Vec<MidiChannelMapping>>>,
        preset_bindings: Arc<RwLock<Vec<MidiPresetBinding>>>,
        preset_trigger_tx: Sender<u8>,
        active_couplers: Arc<RwLock<Vec<String>>>,
        division_gains: Arc<RwLock<Vec<f32>>>,
        swell_bindings: Arc<RwLock<Vec<SwellBinding>>>,
        held_notes: Arc<RwLock<Vec<(u8, u8, u8)>>>,
        ble_message_rx: Receiver<MidiMessage>,
        ble_learn_tx: Sender<MidiMessage>,
        ble_learn_rx: Receiver<MidiMessage>,
        ble_message_count: Arc<std::sync::atomic::AtomicU64>,
        crescendo_stages: Arc<RwLock<Vec<Vec<String>>>>,
        crescendo_stage: Arc<RwLock<u8>>,
        crescendo_enabled: Arc<RwLock<bool>>,
        crescendo_binding: Arc<RwLock<Option<(u8, u8, u8, u8, bool)>>>,
        crescendo_active_stops: Arc<RwLock<Vec<String>>>,
        learn_cancel: Arc<std::sync::atomic::AtomicBool>,
        player_rx: Receiver<MidiMessage>,
        midi_recording: Arc<RwLock<Option<MidiRecording>>>,
        midi_archive: Arc<crate::midi_archive::MidiArchiveShared>,
        notation_scores: Arc<RwLock<std::collections::HashMap<u32, crate::notation::Score>>>,
        notation_armed_score: Arc<RwLock<Option<u32>>>,
        notation_open_notes: Arc<RwLock<std::collections::HashMap<(u32, u32, u32, u8, u8), u64>>>,
        notation_start_time: Arc<RwLock<Option<std::time::Instant>>>,
        app_handle_notation: Arc<RwLock<Option<tauri::AppHandle>>>,
        notation_step_input: Arc<RwLock<Option<u32>>>,
    ) {
        tracing::info!("MIDI thread started");

        // Try to create MIDI manager
        let mut manager = match MidiInputManager::new() {
            Ok(m) => {
                tracing::info!("MIDI manager initialized in thread");
                Some(m)
            }
            Err(e) => {
                tracing::error!("Failed to initialize MIDI manager: {}", e);
                None
            }
        };

        let mut midi_receiver: Option<Receiver<MidiMessage>> = None;
        // Automatisch MIDI-archief: buffer lokaal in deze thread, geen lock per event.
        let mut archive = crate::midi_archive::ArchiveRecorder::new(midi_archive);

        loop {
            // Check for commands (non-blocking if we have MIDI messages to process)
            let cmd = if midi_receiver.is_some() {
                cmd_rx.try_recv().ok()
            } else {
                // Niet permanent blokkeren: zonder verbonden USB-device zouden BLE-MIDI en
                // speler-events (afspelen) anders nooit verwerkt — en dus ook niet opgenomen —
                // worden tot er toevallig een command binnenkomt. Wek de loop periodiek zodat
                // die kanalen ook zonder USB-device gedraind worden.
                match cmd_rx.recv_timeout(std::time::Duration::from_millis(2)) {
                    Ok(c) => Some(c),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break, // Channel closed
                }
            };

            if let Some(cmd) = cmd {
                match cmd {
                    MidiCommand::ListDevices(response_tx) => {
                        let devices = if let Some(ref mgr) = manager {
                            mgr.list_devices().into_iter().map(|d| MidiDeviceInfo {
                                name: d.name,
                                is_input: d.is_input,
                                is_output: d.is_output,
                            }).collect()
                        } else {
                            Vec::new()
                        };
                        let _ = response_tx.send(devices);
                    }
                    MidiCommand::Connect(device_name, response_tx) => {
                        let result = if let Some(ref mut mgr) = manager {
                            match mgr.connect(&device_name) {
                                Ok(()) => {
                                    *midi_connected.write() = true;
                                    midi_receiver = Some(mgr.receiver());
                                    tracing::info!("Connected to MIDI device: {}", device_name);
                                    Ok(())
                                }
                                Err(e) => Err(e.to_string()),
                            }
                        } else {
                            Err("MIDI manager not available".to_string())
                        };
                        let _ = response_tx.send(result);
                    }
                    MidiCommand::ConnectAll(response_tx) => {
                        let result = if let Some(ref mut mgr) = manager {
                            match mgr.connect_all() {
                                Ok(count) => {
                                    if count > 0 {
                                        *midi_connected.write() = true;
                                        midi_receiver = Some(mgr.receiver());
                                        tracing::info!("Connected to {} MIDI devices", count);
                                    }
                                    Ok(count)
                                }
                                Err(e) => Err(e.to_string()),
                            }
                        } else {
                            Err("MIDI manager not available".to_string())
                        };
                        let _ = response_tx.send(result);
                    }
                    MidiCommand::Disconnect => {
                        if let Some(ref mut mgr) = manager {
                            mgr.disconnect_all();
                            *midi_connected.write() = false;
                            midi_receiver = None;
                        }
                    }
                    MidiCommand::SetChannelMapping(mappings) => {
                        *midi_mappings.write() = mappings;
                        tracing::info!("MIDI channel mappings updated");
                    }
                    MidiCommand::LearnChannel(division, response_tx) => {
                        // Wait for next MIDI message and return its channel
                        // This waits up to 10 seconds for a note
                        if let Some(ref rx) = midi_receiver {
                            if let Ok(msg) = rx.recv_timeout(std::time::Duration::from_secs(10)) {
                                let channel = match msg {
                                    MidiMessage::NoteOn { channel, .. } => Some(channel),
                                    MidiMessage::NoteOff { channel, .. } => Some(channel),
                                    _ => None,
                                };

                                // If we learned a channel, update the mapping
                                if let Some(ch) = channel {
                                    let mut mappings = midi_mappings.write();
                                    if let Some(existing) = mappings.iter_mut().find(|m| m.division == division) {
                                        existing.channel = Some(ch);
                                    } else {
                                        mappings.push(MidiChannelMapping {
                                            division: division.clone(),
                                            channel: Some(ch),
                                            transpose: 0,
                                            first_midi_note: None,
                                            last_midi_note: None,
                                        });
                                    }
                                    tracing::info!("Learned MIDI channel {} for division {}", ch, division);
                                }

                                let _ = response_tx.send(channel);
                            } else {
                                tracing::info!("MIDI learn timed out for division {}", division);
                                let _ = response_tx.send(None);
                            }
                        } else {
                            tracing::warn!("No MIDI connection for learning");
                            let _ = response_tx.send(None);
                        }
                    }
                    MidiCommand::LearnPedalPosition(filter, response_tx) => {
                        tracing::info!("Learning pedal position (filter={:?})", filter);
                        if let Some(rx) = midi_receiver.as_ref() { while rx.try_recv().is_ok() {} }
                        while ble_message_rx.try_recv().is_ok() {}
                        let result = match filter {
                            None => Self::learn_wait_settled(
                                midi_receiver.as_ref(), &ble_message_rx, std::time::Duration::from_secs(20),
                                &audio_player, &organ_definition, &loaded_organ_info, &drawn_stops,
                                &midi_mappings, &active_couplers, &swell_bindings, &division_gains, &held_notes,
                                &ble_message_count, &learn_cancel,
                            ),
                            Some((ch, cc)) => Self::learn_wait_settled_on(
                                midi_receiver.as_ref(), &ble_message_rx, ch, cc,
                                std::time::Duration::from_secs(20),
                                &audio_player, &organ_definition, &loaded_organ_info, &drawn_stops,
                                &midi_mappings, &active_couplers, &swell_bindings, &division_gains, &held_notes,
                                &ble_message_count, &learn_cancel,
                            ).map(|v| (ch, cc, v)),
                        };
                        let _ = response_tx.send(result);
                    }
                    MidiCommand::LearnSingleNote(response_tx) => {
                        tracing::info!("Learning single note: press a key");
                        if let Some(ref rx) = midi_receiver {
                            while rx.try_recv().is_ok() {}
                            let forward = |m: MidiMessage| Self::handle_midi_message(
                                m, &audio_player, &organ_definition, &loaded_organ_info, &drawn_stops,
                                &midi_mappings, &active_couplers, &swell_bindings, &division_gains, &held_notes,
                                false,
                            );
                            let result = Self::wait_for_note_on(rx, std::time::Duration::from_secs(20), &forward);
                            let _ = response_tx.send(result);
                        } else {
                            let _ = response_tx.send(None);
                        }
                    }
                    MidiCommand::LearnKeyboardRange(division, first_sample_note, response_tx) => {
                        // Learn keyboard range like the reference software:
                        // 1. Wait for first note (lowest key) - this gives us channel and start note
                        // 2. Wait for second note (highest key) - this gives us end note
                        // 3. Calculate transpose so lowest MIDI note maps to first sample note
                        tracing::info!("Learning keyboard range for {}: press lowest key, then highest key", division);

                        if let Some(ref rx) = midi_receiver {
                            // Releases die tijdens het leren binnenkomen tóch naar de audio.
                            let forward = |m: MidiMessage| Self::handle_midi_message(
                                m, &audio_player, &organ_definition, &loaded_organ_info, &drawn_stops,
                                &midi_mappings, &active_couplers, &swell_bindings, &division_gains, &held_notes,
                                false,
                            );
                            // Wait for first note (lowest key)
                            let first_result = Self::wait_for_note_on(rx, std::time::Duration::from_secs(30), &forward);

                            if let Some((channel, first_note)) = first_result {
                                tracing::info!("Learned first note: channel={}, note={}", channel, first_note);

                                // Wait for second note (highest key) - same channel
                                let second_result = Self::wait_for_note_on(rx, std::time::Duration::from_secs(30), &forward);

                                if let Some((_, second_note)) = second_result {
                                    // Determine which is lowest and highest
                                    let (low, high) = if first_note <= second_note {
                                        (first_note, second_note)
                                    } else {
                                        (second_note, first_note)
                                    };

                                    // Calculate transpose: first_sample_note - low
                                    // E.g., if samples start at C (36) and lowest key is MIDI 48,
                                    // transpose = 36 - 48 = -12
                                    let transpose = (first_sample_note as i16 - low as i16) as i8;

                                    tracing::info!(
                                        "Learned keyboard range for {}: channel={}, notes={}-{}, transpose={}",
                                        division, channel, low, high, transpose
                                    );

                                    // Update mapping
                                    let mut mappings = midi_mappings.write();
                                    if let Some(existing) = mappings.iter_mut().find(|m| m.division == division) {
                                        existing.channel = Some(channel);
                                        existing.first_midi_note = Some(low);
                                        existing.last_midi_note = Some(high);
                                        existing.transpose = transpose;
                                    } else {
                                        mappings.push(MidiChannelMapping {
                                            division: division.clone(),
                                            channel: Some(channel),
                                            transpose,
                                            first_midi_note: Some(low),
                                            last_midi_note: Some(high),
                                        });
                                    }

                                    let _ = response_tx.send(Some(LearnedKeyboardRange {
                                        channel,
                                        first_note: low,
                                        last_note: high,
                                        transpose,
                                    }));
                                } else {
                                    tracing::info!("MIDI learn timed out waiting for second note");
                                    let _ = response_tx.send(None);
                                }
                            } else {
                                tracing::info!("MIDI learn timed out waiting for first note");
                                let _ = response_tx.send(None);
                            }
                        } else {
                            tracing::warn!("No MIDI connection for learning keyboard range");
                            let _ = response_tx.send(None);
                        }
                    }
                    MidiCommand::SetPresetBindings(bindings) => {
                        *preset_bindings.write() = bindings;
                        tracing::info!("MIDI preset bindings updated");
                    }
                    MidiCommand::LearnPresetBinding(preset_num, response_tx) => {
                        // Wait for next MIDI message and learn it as a preset trigger
                        if let Some(ref rx) = midi_receiver {
                            if let Ok(msg) = rx.recv_timeout(std::time::Duration::from_secs(10)) {
                                let trigger = match msg {
                                    MidiMessage::NoteOn { channel, note, .. } => {
                                        Some(MidiPresetTrigger::Note { channel: Some(channel), note })
                                    }
                                    MidiMessage::ControlChange { channel, controller, value } => {
                                        Some(MidiPresetTrigger::ControlChange {
                                            channel: Some(channel),
                                            controller,
                                            value,
                                        })
                                    }
                                    MidiMessage::ProgramChange { channel, program } => {
                                        Some(MidiPresetTrigger::ProgramChange {
                                            channel: Some(channel),
                                            program,
                                        })
                                    }
                                    _ => None,
                                };

                                // If we learned a trigger, save the binding (max 4 per action)
                                if let Some(ref trig) = trigger {
                                    let mut bindings = preset_bindings.write();
                                    let count = bindings.iter().filter(|b| b.preset_num == preset_num).count();
                                    if count < 4 {
                                        bindings.push(MidiPresetBinding {
                                            preset_num,
                                            trigger: trig.clone(),
                                        });
                                        tracing::info!("Learned MIDI binding {}/{} for action {}: {:?}", count + 1, 4, preset_num, trig);
                                    } else {
                                        tracing::warn!("Max 4 MIDI bindings reached for action {}", preset_num);
                                    }
                                }

                                let _ = response_tx.send(trigger);
                            } else {
                                tracing::info!("MIDI learn timed out for preset {}", preset_num);
                                let _ = response_tx.send(None);
                            }
                        } else {
                            tracing::warn!("No MIDI connection for learning preset");
                            let _ = response_tx.send(None);
                        }
                    }
                    MidiCommand::LearnSwellPedal(division_name, division_index, response_tx) => {
                        tracing::info!("Learning swell pedal for {}: move pedal to lowest position", division_name);
                        // Drain anything pending so we start fresh
                        if let Some(rx) = midi_receiver.as_ref() { while rx.try_recv().is_ok() {} }
                        while ble_message_rx.try_recv().is_ok() {}
                        while ble_learn_rx.try_recv().is_ok() {}

                        // Phase 1: wait for low position settled
                        let first = Self::learn_wait_settled(
                            midi_receiver.as_ref(), &ble_message_rx, std::time::Duration::from_secs(15),
                            &audio_player, &organ_definition, &loaded_organ_info, &drawn_stops,
                            &midi_mappings, &active_couplers, &swell_bindings, &division_gains, &held_notes,
                            &ble_message_count, &learn_cancel,
                        );

                        if let Some((channel, cc_num, min_val)) = first {
                            tracing::info!("Swell low position: ch={}, cc={}, val={}", channel, cc_num, min_val);
                            // Drain again
                            if let Some(rx) = midi_receiver.as_ref() { while rx.try_recv().is_ok() {} }
                            while ble_message_rx.try_recv().is_ok() {}

                            // Phase 2: wait for high position on same CC settled
                            let second = Self::learn_wait_settled_on(
                                midi_receiver.as_ref(), &ble_message_rx, channel, cc_num,
                                std::time::Duration::from_secs(15),
                                &audio_player, &organ_definition, &loaded_organ_info, &drawn_stops,
                                &midi_mappings, &active_couplers, &swell_bindings, &division_gains, &held_notes,
                                &ble_message_count, &learn_cancel,
                            );

                            if let Some(max_val) = second {
                                // Laagste stand gaf een hógere CC-waarde dan de hoogste
                                // stand → trede is omgekeerd gemonteerd: min/max wisselen
                                // én invert zetten (zoals apply_learned_swell in commands.rs;
                                // hier stond invert altijd op false → omgekeerde lampjes/volume).
                                let (actual_min, actual_max, inverted) = if min_val <= max_val {
                                    (min_val, max_val, false)
                                } else {
                                    (max_val, min_val, true)
                                };
                                let binding = SwellBinding {
                                    division_name: division_name.clone(),
                                    division_index,
                                    channel,
                                    cc_num,
                                    min_val: actual_min,
                                    max_val: actual_max,
                                    invert: inverted,
                                    last_value: Some(max_val),
                                };
                                tracing::info!("Swell learned: {} → CC{} ch{} range {}-{}",
                                    division_name, cc_num, channel, actual_min, actual_max);
                                // Wederzijds exclusief met de generaal crescendo:
                                // een oude crescendo-koppeling op dezelfde CC
                                // "claimde" de trede en hield de zwelkast dood.
                                // Laatst ingeleerd wint.
                                {
                                    let mut cb = crescendo_binding.write();
                                    if let Some((bch, bcc, _, _, _)) = *cb {
                                        if bch == channel && bcc == cc_num {
                                            tracing::info!("Crescendo-koppeling op dezelfde CC gewist (zwel-inleer wint)");
                                            *cb = None;
                                        }
                                    }
                                }
                                let mut bindings = swell_bindings.write();
                                bindings.retain(|b| b.division_name != division_name);
                                bindings.push(binding.clone());
                                let _ = response_tx.send(Some(binding));
                            } else {
                                tracing::info!("Swell learn timed out (high position)");
                                let _ = response_tx.send(None);
                            }
                        } else {
                            tracing::info!("Swell learn timed out (low position)");
                            let _ = response_tx.send(None);
                        }
                    }
                    MidiCommand::ClearSwellPedal(division_name) => {
                        swell_bindings.write().retain(|b| b.division_name != division_name);
                        tracing::info!("Cleared swell binding for {}", division_name);
                    }
                    MidiCommand::LearnCrescendoPedal(response_tx) => {
                        tracing::info!("Learning crescendo pedal: move the crescendo pedal");
                        if let Some(rx) = midi_receiver.as_ref() { while rx.try_recv().is_ok() {} }
                        while ble_message_rx.try_recv().is_ok() {}
                        let first = Self::learn_wait_settled(
                            midi_receiver.as_ref(), &ble_message_rx, std::time::Duration::from_secs(15),
                            &audio_player, &organ_definition, &loaded_organ_info, &drawn_stops,
                            &midi_mappings, &active_couplers, &swell_bindings, &division_gains, &held_notes,
                            &ble_message_count, &learn_cancel,
                        );
                        if let Some((channel, cc_num, _val)) = first {
                            tracing::info!("Crescendo pedal learned: ch={}, cc={}", channel, cc_num);
                            let _ = response_tx.send(Some((channel, cc_num)));
                        } else {
                            tracing::info!("Crescendo learn timed out");
                            let _ = response_tx.send(None);
                        }
                    }
                    MidiCommand::ArchiveFlush(response_tx) => {
                        // Synchroon schrijven: de aanroeper (close-handler / exit /
                        // test-API) wacht op het pad; enkele ms.
                        let now = std::time::Instant::now();
                        let written = archive.force_finish(now)
                            .and_then(|t| crate::midi_archive::write_now(&archive.shared(), t));
                        let _ = response_tx.send(written);
                    }
                    MidiCommand::CancelLearn => {
                        // De afgebroken leerlus is al teruggekeerd (die zag de
                        // vlag); hier alleen de vlag wissen voor de volgende inleer.
                        learn_cancel.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    MidiCommand::Shutdown => break,
                }
            }

            // Process MIDI messages. Pedaal-bursts samenvoegen: één crescendo-
            // sweep levert per drain tientallen CC's, en élke tussenliggende
            // trapwissel stuurt (registers × toetsen × routes) audio-commando's.
            // Zolang de MIDI-thread daarin hangt worden binnenkomende note-offs
            // niet verwerkt (invoerqueue loopt over → hangers). Daarom telt per
            // drain alleen de LAATSTE crescendo-CC; tussenstanden slaan we over
            // (wel opnemen/preset-checken — alleen de trapwissel zelf vervalt).
            if let Some(ref rx) = midi_receiver {
                let mut batch: Vec<MidiMessage> = Vec::new();
                while let Ok(msg) = rx.try_recv() { batch.push(msg); }
                let last_cresc_idx = batch.iter()
                    .rposition(|m| Self::cc_claimed_by_crescendo(m, &crescendo_binding));
                for (i, msg) in batch.into_iter().enumerate() {
                    // Leg het ingespeelde event vast als er een MIDI-opname loopt
                    Self::capture_midi_event(&midi_recording, &msg);
                    // Automatisch MIDI-archief (live spel; de closure — één
                    // read-lock op loaded_organ_info — draait alleen bij take-start).
                    archive.on_event(&msg, std::time::Instant::now(),
                        || loaded_organ_info.read().as_ref().map(|o| o.name.clone()).unwrap_or_default());
                    Self::capture_notation_event(&msg, &notation_scores, &notation_armed_score,
                        &notation_open_notes, &notation_start_time, &app_handle_notation,
                        &midi_mappings);
                    Self::emit_step_input_event(&msg, &notation_step_input,
                        &notation_armed_score, &app_handle_notation);

                    // Check for preset triggers first
                    Self::check_preset_trigger(&msg, &preset_bindings, &preset_trigger_tx);

                    let cc_claim = Self::cc_claimed_by_crescendo(&msg, &crescendo_binding);
                    if cc_claim && Some(i) != last_cresc_idx {
                        continue; // tussenliggende pedaalstand — alleen de laatste telt
                    }

                    // Check crescendo CC
                    Self::process_crescendo_cc(
                        &msg, &crescendo_binding, &crescendo_enabled, &crescendo_stages,
                        &crescendo_stage, &crescendo_active_stops, &drawn_stops,
                        &audio_player, &loaded_organ_info,
                        &midi_mappings, &active_couplers, &held_notes,
                    );

                    // Then handle normal MIDI (notes, etc.)
                    Self::handle_midi_message(
                        msg,
                        &audio_player,
                        &organ_definition,
                        &loaded_organ_info,
                        &drawn_stops,
                        &midi_mappings,
                        &active_couplers,
                        &swell_bindings,
                        &division_gains,
                        &held_notes,
                        cc_claim,
                    );
                }
            }

            // Process BLE MIDI messages (from BleMidiManager)
            // Zelfde crescendo-CC-coalescing als de USB-lus hierboven.
            let mut ble_batch: Vec<MidiMessage> = Vec::new();
            while let Ok(msg) = ble_message_rx.try_recv() { ble_batch.push(msg); }
            let ble_last_cresc_idx = ble_batch.iter()
                .rposition(|m| Self::cc_claimed_by_crescendo(m, &crescendo_binding));
            for (i, msg) in ble_batch.into_iter().enumerate() {
                ble_message_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::trace!("BLE MIDI received: {:?}", msg);
                // Leg het ingespeelde event vast als er een MIDI-opname loopt
                Self::capture_midi_event(&midi_recording, &msg);
                // Automatisch MIDI-archief: BLE-klavieren (en de test-API-injectie)
                // tellen als live spel.
                archive.on_event(&msg, std::time::Instant::now(),
                    || loaded_organ_info.read().as_ref().map(|o| o.name.clone()).unwrap_or_default());
                // Zelfde notatie-hooks als de USB-lus: BLE-klavieren (en de
                // test-API-injectie) moeten ook in de live-notatie belanden.
                Self::capture_notation_event(&msg, &notation_scores, &notation_armed_score,
                    &notation_open_notes, &notation_start_time, &app_handle_notation,
                    &midi_mappings);
                Self::emit_step_input_event(&msg, &notation_step_input,
                    &notation_armed_score, &app_handle_notation);
                // Forward CC messages to learn-tap channel (non-blocking; drop if full)
                if matches!(msg, MidiMessage::ControlChange { .. }) {
                    let _ = ble_learn_tx.try_send(msg.clone());
                }
                Self::check_preset_trigger(&msg, &preset_bindings, &preset_trigger_tx);
                let cc_claim = Self::cc_claimed_by_crescendo(&msg, &crescendo_binding);
                if cc_claim && Some(i) != ble_last_cresc_idx {
                    continue; // tussenliggende pedaalstand — alleen de laatste telt
                }
                Self::process_crescendo_cc(
                    &msg, &crescendo_binding, &crescendo_enabled, &crescendo_stages,
                    &crescendo_stage, &crescendo_active_stops, &drawn_stops,
                    &audio_player, &loaded_organ_info,
                    &midi_mappings, &active_couplers, &held_notes,
                );
                Self::handle_midi_message(
                    msg,
                    &audio_player,
                    &organ_definition,
                    &loaded_organ_info,
                    &drawn_stops,
                    &midi_mappings,
                    &active_couplers,
                    &swell_bindings,
                    &division_gains,
                    &held_notes,
                    cc_claim,
                );
            }

            // Process MIDI file player messages (afspelen .mid bestanden)
            // Bewust NIET vastleggen in de opname én NIET in het automatische
            // MIDI-archief: dit is afspelen, geen live spel — anders zou een
            // opname tijdens playback de noten dubbel/terug-opnemen.
            // Zelfde crescendo-CC-coalescing + trapwissel als de live lussen:
            // een opname met crescendobewegingen speelt de trede ook af.
            let mut player_batch: Vec<MidiMessage> = Vec::new();
            while let Ok(msg) = player_rx.try_recv() { player_batch.push(msg); }
            let player_last_cresc_idx = player_batch.iter()
                .rposition(|m| Self::cc_claimed_by_crescendo(m, &crescendo_binding));
            for (i, msg) in player_batch.into_iter().enumerate() {
                // Ook bij afspelen: een opgenomen crescendo-CC mag het volume
                // niet dempen via de expressie-fallback.
                let cc_claim = Self::cc_claimed_by_crescendo(&msg, &crescendo_binding);
                if cc_claim && Some(i) != player_last_cresc_idx {
                    continue; // tussenliggende pedaalstand — alleen de laatste telt
                }
                Self::process_crescendo_cc(
                    &msg, &crescendo_binding, &crescendo_enabled, &crescendo_stages,
                    &crescendo_stage, &crescendo_active_stops, &drawn_stops,
                    &audio_player, &loaded_organ_info,
                    &midi_mappings, &active_couplers, &held_notes,
                );
                Self::handle_midi_message(
                    msg,
                    &audio_player,
                    &organ_definition,
                    &loaded_organ_info,
                    &drawn_stops,
                    &midi_mappings,
                    &active_couplers,
                    &swell_bindings,
                    &division_gains,
                    &held_notes,
                    cc_claim,
                );
            }

            // Automatisch MIDI-archief: take afsluiten bij stilte/uitzetten/limiet
            // (1 atomic-load + Instant::now per iteratie); schrijven op een eigen thread.
            if let Some(take) = archive.poll(std::time::Instant::now()) {
                crate::midi_archive::spawn_write(archive.shared(), take);
            }

            // Small sleep to prevent busy loop
            std::thread::sleep(std::time::Duration::from_micros(100));
        }

        tracing::info!("MIDI thread ended");
    }

    /// Leg een binnenkomend MIDI-bericht vast in de actieve opname (indien aanwezig).
    /// Alleen musicale channel-voice berichten (note on/off, CC, PC, pitchbend, aftertouch)
    /// worden opgeslagen; realtime/systeem/SysEx wordt overgeslagen. Wordt op alle live
    /// MIDI-loops aangeroepen. Snelle uitgang (read-lock, geen allocatie) als er niets opneemt.
    fn capture_midi_event(
        midi_recording: &Arc<RwLock<Option<MidiRecording>>>,
        msg: &MidiMessage,
    ) {
        // Snelle uitgang als er geen actieve opname loopt: alleen een read-lock,
        // geen werk/allocatie. Het timestamp wordt pas ONDER de write-lock
        // bepaald zodat het altijd hoort bij de opname die het event ontvangt
        // (start/stop kan tussen de read- en write-lock wisselen).
        if !matches!(midi_recording.read().as_ref(), Some(rec) if rec.active) {
            return;
        }
        let bytes = msg.to_bytes();
        let status = match bytes.first() {
            Some(&s) => s,
            None => return,
        };
        // Sla alleen channel-voice berichten op (0x80..0xE0); negeer realtime/systeem/SysEx.
        if !matches!(status & 0xF0, 0x80 | 0x90 | 0xA0 | 0xB0 | 0xC0 | 0xD0 | 0xE0) {
            return;
        }
        let data1 = bytes.get(1).copied().unwrap_or(0);
        let data2 = bytes.get(2).copied().unwrap_or(0);
        let channel = status & 0x0F;
        // Her-check onder de write-lock: de opname kan inmiddels gestopt of
        // vervangen zijn; elapsed hoort bij de opname die we hier vasthouden.
        if let Some(rec) = midi_recording.write().as_mut() {
            if rec.active {
                let elapsed_us = rec.start_time.elapsed().as_micros() as u64;
                rec.events.push((elapsed_us, status, data1, data2, channel));
            }
        }
    }

    /// Live-notatie: schrijf een MIDI-note in de armed take van de armed
    /// layer van de armed Score en emit `jm-orgue:notation:note-open` (NoteOn)
    /// of `jm-orgue:notation:note-added` (NoteOff) naar het notatievenster.
    ///
    /// Concurrency-model: één Score is armed tegelijk; slechts één laag +
    /// take binnen die Score is armed (zie NotationMode-uitleg in commands.rs).
    /// De starttijd staat op de eerste NoteOn die armed binnenkomt (zonder
    /// count-in, zoals afgesproken).
    #[allow(clippy::too_many_arguments)]
    fn capture_notation_event(
        msg: &MidiMessage,
        notation_scores: &Arc<RwLock<std::collections::HashMap<u32, crate::notation::Score>>>,
        notation_armed_score: &Arc<RwLock<Option<u32>>>,
        notation_open_notes: &Arc<RwLock<std::collections::HashMap<(u32, u32, u32, u8, u8), u64>>>,
        notation_start_time: &Arc<RwLock<Option<std::time::Instant>>>,
        app_handle: &Arc<RwLock<Option<tauri::AppHandle>>>,
        midi_mappings: &Arc<RwLock<Vec<MidiChannelMapping>>>,
    ) {
        use tauri::Emitter;
        // Snelle uitgang: geen armed Score → niets doen (het gros van de MIDI-events).
        let score_id = match *notation_armed_score.read() {
            Some(id) => id,
            None => return,
        };
        // Doellaag bepalen: eerst divisie-routering (kanaal+noot → divisie via de
        // inleer, → balk met die divisie toegewezen), anders de armed laag. De
        // take is de armed take van de doellaag (of diens eerste take). Zo komt
        // elke divisie tijdens het inspelen op zijn eigen balk terecht.
        let (layer_id, take_id) = {
            let chan_note = match msg {
                MidiMessage::NoteOn { channel, note, .. } => Some((*channel, *note)),
                MidiMessage::NoteOff { channel, note, .. } => Some((*channel, *note)),
                _ => None,
            };
            let division = chan_note.and_then(|(ch, nt)| {
                let maps = midi_mappings.read();
                maps.iter().find(|m| m.accepts(ch, nt)).map(|m| m.division.clone())
            });
            let scores = notation_scores.read();
            let Some(sc) = scores.get(&score_id) else { return };
            let layer = division.as_ref()
                .and_then(|d| sc.layers.iter().find(|l| l.divisions.iter().any(|x| x == d)))
                .or_else(|| sc.armed_layer.and_then(|lid| sc.layers.iter().find(|l| l.id == lid)));
            let Some(layer) = layer else { return };
            let Some(tid) = layer.armed_take.or_else(|| layer.takes.first().map(|t| t.id)) else { return };
            (layer.id, tid)
        };

        match msg {
            MidiMessage::NoteOn { channel, note, velocity } if *velocity > 0 => {
                // Starttijd instellen bij het allereerste event (geen count-in).
                let now = std::time::Instant::now();
                let start = {
                    let mut st = notation_start_time.write();
                    *st.get_or_insert(now)
                };
                let elapsed_us = now.saturating_duration_since(start).as_micros() as u64;
                let key = (score_id, layer_id, take_id, *channel, *note);
                notation_open_notes.write().insert(key, elapsed_us);
                // Live "note-open" event: UI kan (optioneel) een grijze cue tonen.
                if let Some(handle) = app_handle.read().as_ref() {
                    let _ = handle.emit("jm-orgue:notation:note-open", serde_json::json!({
                        "score": score_id, "layer": layer_id, "take": take_id,
                        "midi": *note, "channel": *channel, "at_us": elapsed_us,
                    }));
                }
            }
            MidiMessage::NoteOff { channel, note, .. }
            | MidiMessage::NoteOn { channel, note, velocity: 0 } => {
                let now = std::time::Instant::now();
                let start = {
                    let st = notation_start_time.read();
                    match *st { Some(s) => s, None => return }
                };
                let end_us = now.saturating_duration_since(start).as_micros() as u64;
                let key = (score_id, layer_id, take_id, *channel, *note);
                let start_us = match notation_open_notes.write().remove(&key) {
                    Some(s) => s,
                    None => return, // geen matchende NoteOn (bv. voor de opname)
                };
                let end_us = end_us.max(start_us + 20_000); // minimaal 20 ms
                // Event toevoegen aan de score-take en generation bump'en.
                let ev_id = {
                    let mut scores = notation_scores.write();
                    let Some(sc) = scores.get_mut(&score_id) else { return };
                    let ev_id = sc.new_event_id();
                    if let Some(layer) = sc.layers.iter_mut().find(|l| l.id == layer_id) {
                        if let Some(take) = layer.takes.iter_mut().find(|t| t.id == take_id) {
                            take.events.push(crate::notation::LayerEv {
                                id: ev_id, midi: *note, start_us, end_us,
                                channel: *channel, locked: false,
                            });
                        }
                    }
                    sc.bump_gen();
                    ev_id
                };
                if let Some(handle) = app_handle.read().as_ref() {
                    let _ = handle.emit("jm-orgue:notation:note-added", serde_json::json!({
                        "score": score_id, "layer": layer_id, "take": take_id,
                        "id": ev_id, "midi": *note, "channel": *channel,
                        "start_us": start_us, "end_us": end_us,
                    }));
                }
            }
            _ => {}
        }
    }

    /// Stapinvoer (0.7.15): staat het notatievenster in stap-modus (en loopt er
    /// géén live-opname), stuur de toetsaanslag dan als step-event naar het
    /// venster. De toets klinkt gewoon mee op het orgel (normale afhandeling
    /// gaat door); het venster plaatst de noot op de invoercursor met de daar
    /// gekozen nootwaarde en bundelt gelijktijdig ingedrukte toetsen tot een
    /// akkoord (commit zodra alles losgelaten is).
    fn emit_step_input_event(
        msg: &MidiMessage,
        notation_step_input: &Arc<RwLock<Option<u32>>>,
        notation_armed_score: &Arc<RwLock<Option<u32>>>,
        app_handle: &Arc<RwLock<Option<tauri::AppHandle>>>,
    ) {
        use tauri::Emitter;
        let Some(score_id) = *notation_step_input.read() else { return };
        if notation_armed_score.read().is_some() { return; } // live-opname gaat vóór
        let (event, channel, note) = match msg {
            MidiMessage::NoteOn { channel, note, velocity } if *velocity > 0 =>
                ("jm-orgue:notation:step-note-on", *channel, *note),
            MidiMessage::NoteOff { channel, note, .. }
            | MidiMessage::NoteOn { channel, note, velocity: 0 } =>
                ("jm-orgue:notation:step-note-off", *channel, *note),
            _ => return,
        };
        if let Some(handle) = app_handle.read().as_ref() {
            let _ = handle.emit(event, serde_json::json!({
                "score": score_id, "midi": note, "channel": channel,
            }));
        }
    }

    /// Wait for a NoteOn message with velocity > 0
    /// Simple version: just detect NoteOn and add a small delay
    fn wait_for_note_on(rx: &Receiver<MidiMessage>, timeout: std::time::Duration, forward: &dyn Fn(MidiMessage)) -> Option<(u8, u8)> {
        let start = std::time::Instant::now();

        // NoteOffs (en NoteOn vel=0) DOORSTUREN naar de audio: releases van
        // al-klinkende noten gingen hier voorheen verloren, waardoor stemmen
        // eeuwig in hun sustain-loop bleven hangen (auditbevinding 43).
        let is_release = |m: &MidiMessage| matches!(m, MidiMessage::NoteOff { .. } | MidiMessage::NoteOn { velocity: 0, .. });

        while start.elapsed() < timeout {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(MidiMessage::NoteOn { channel, note, velocity }) if velocity > 0 => {
                    tracing::info!("Got NoteOn: channel={}, note={}, velocity={}", channel, note, velocity);
                    // Wait 500ms to allow the user to release the key and prepare for the next
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    // Drain pending messages, maar releases doorsturen
                    while let Ok(m) = rx.try_recv() {
                        if is_release(&m) { forward(m); }
                    }
                    return Some((channel, note));
                }
                Ok(m) => {
                    // Releases doorsturen; overige berichten negeren (stille leer-modus)
                    if is_release(&m) { forward(m); }
                    continue;
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    continue;
                }
                Err(_) => {
                    // Channel disconnected
                    return None;
                }
            }
        }
        None
    }

    /// Receive a MIDI message from either USB or BLE channel with a timeout.
    /// This lets learn functions work with both USB and BLE MIDI devices.
    fn recv_any_midi(
        usb_rx: Option<&Receiver<MidiMessage>>,
        ble_rx: &Receiver<MidiMessage>,
        timeout: std::time::Duration,
    ) -> Result<MidiMessage, ()> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            // Try USB first if available
            if let Some(usb) = usb_rx {
                if let Ok(msg) = usb.try_recv() { return Ok(msg); }
            }
            if let Ok(msg) = ble_rx.try_recv() { return Ok(msg); }
            // Neither has a message, wait briefly
            let now = std::time::Instant::now();
            if now >= deadline { return Err(()); }
            let remaining = deadline - now;
            let sleep_dur = std::cmp::min(remaining, std::time::Duration::from_millis(20));
            std::thread::sleep(sleep_dur);
        }
    }

    /// Wait for CC messages until the value settles (no change for 1.5s)
    /// Returns (channel, controller, value) of the settled position
    fn wait_for_cc_settled(rx: &Receiver<MidiMessage>, timeout: std::time::Duration) -> Option<(u8, u8, u8)> {
        Self::wait_for_cc_settled_dual(Some(rx), &crossbeam_channel::never(), timeout)
    }

    /// Pedaal-CC van de generaal crescendo: ruwe waarde → bereik/spiegel →
    /// trap (met Schmitt-hysterese, zie crescendo_next_stage) → apply. Alle
    /// registratie-mutatie zit in apply_crescendo_stage_inner (één
    /// implementatie met de UI-klik/editor/uitschakelen).
    fn process_crescendo_cc(
        msg: &MidiMessage,
        crescendo_binding: &Arc<RwLock<Option<(u8, u8, u8, u8, bool)>>>,
        crescendo_enabled: &Arc<RwLock<bool>>,
        crescendo_stages: &Arc<RwLock<Vec<Vec<String>>>>,
        crescendo_stage: &Arc<RwLock<u8>>,
        crescendo_active_stops: &Arc<RwLock<Vec<String>>>,
        drawn_stops: &Arc<RwLock<Vec<String>>>,
        audio_player: &Arc<RwLock<Option<AudioPlayer>>>,
        loaded_organ_info: &Arc<RwLock<Option<OrganInfoDto>>>,
        midi_mappings: &Arc<RwLock<Vec<MidiChannelMapping>>>,
        active_couplers: &Arc<RwLock<Vec<String>>>,
        held_notes: &Arc<RwLock<Vec<(u8, u8, u8)>>>,
    ) {
        let (channel, controller, value) = match msg {
            MidiMessage::ControlChange { channel, controller, value } => (*channel, *controller, *value),
            _ => return,
        };

        let binding = *crescendo_binding.read();
        let Some((bound_ch, bound_cc, min_val, max_val, invert)) = binding else { return; };
        if channel != bound_ch || controller != bound_cc { return; }
        if !*crescendo_enabled.read() { return; }

        let num_stages = crescendo_stages.read().len();
        if num_stages == 0 { return; }

        let mapped = crescendo_mapped_value(value, min_val, max_val, invert);
        let current_stage = *crescendo_stage.read();
        let Some(new_stage) = crescendo_next_stage(mapped, current_stage, num_stages) else { return; };

        let (added, removed) = Self::apply_crescendo_stage_inner(
            new_stage, crescendo_stages, crescendo_stage, crescendo_active_stops, drawn_stops,
            active_couplers, audio_player, loaded_organ_info, midi_mappings, held_notes,
        );
        let active = crescendo_active_stops.read().len();
        tracing::info!("Crescendo: CC={} (pedaal {}) → stage {} → {} (+{} −{}, {} active stops)",
            value, mapped, current_stage, new_stage, added.len(), removed.len(), active);
    }

    /// Kern van de generaal crescendo — één implementatie voor pedaal, UI-klik
    /// op een trap, editor-wijziging, uitschakelen (trap 0) en herstel. Zet trap
    /// `new_stage` (geclamped op het aantal stappen) en werkt drawn_stops,
    /// active_couplers, de claimlijst (crescendo_active_stops), de trapteller en
    /// de DTO-vlaggen bij, inclusief voice-sync van ingedrukte toetsen.
    /// - Additief (GrandOrgue): wat de organist zelf trok wordt nooit geclaimd
    ///   en dus ook niet weggetrokken bij terugveren.
    /// - Koppels worden herkend op lidmaatschap van de koppellijst van het
    ///   orgel (ook echte ODF-koppels `real_coupler_N`), niet op een naamprefix.
    /// - Een lege trap erft de dichtstbijzijnde lagere gevulde trap; onbekende
    ///   IDs (matrix van een andere sampleset) worden genegeerd.
    /// Retourneert (bijgetrokken, weggetrokken) IDs.
    pub(crate) fn apply_crescendo_stage_inner(
        new_stage: u8,
        crescendo_stages: &Arc<RwLock<Vec<Vec<String>>>>,
        crescendo_stage: &Arc<RwLock<u8>>,
        crescendo_active_stops: &Arc<RwLock<Vec<String>>>,
        drawn_stops: &Arc<RwLock<Vec<String>>>,
        active_couplers: &Arc<RwLock<Vec<String>>>,
        audio_player: &Arc<RwLock<Option<AudioPlayer>>>,
        loaded_organ_info: &Arc<RwLock<Option<OrganInfoDto>>>,
        midi_mappings: &Arc<RwLock<Vec<MidiChannelMapping>>>,
        held_notes: &Arc<RwLock<Vec<(u8, u8, u8)>>>,
    ) -> (Vec<String>, Vec<String>) {
        // Serialiseer pedaal (MIDI-thread) en UI/editor/test-API (command-thread):
        // beide lezen claims/drawn/koppels, rekenen een diff en schrijven terug.
        // Onder deze lock wordt geen andere lock in omgekeerde volgorde
        // genomen; rt_send blokkeert niet, dus de wachttijd is microseconden.
        let _apply_guard = CRESCENDO_APPLY.lock();
        use std::collections::HashSet;

        let (new_stage, stage_stops): (u8, Vec<String>) = {
            let stages = crescendo_stages.read();
            let s = (new_stage as usize).min(stages.len());
            (s as u8, crescendo_effective_stops(&stages, s))
        };

        // Lidmaatschapstest voor koppels + geldige IDs van dít orgel.
        let (coupler_ids, stop_ids): (HashSet<String>, HashSet<String>) = {
            let organ = loaded_organ_info.read();
            match organ.as_ref() {
                Some(o) => (
                    o.couplers.as_ref().map(|cl| cl.iter().map(|c| c.id.clone()).collect()).unwrap_or_default(),
                    o.divisions.iter().flat_map(|d| d.stops.iter().map(|s| s.id.clone())).collect(),
                ),
                None => (HashSet::new(), HashSet::new()),
            }
        };
        let is_coupler = |id: &String| coupler_ids.contains(id);

        let old_active = crescendo_active_stops.read().clone();
        let new_active: Vec<String> = {
            let drawn = drawn_stops.read();
            let couplers_now = active_couplers.read();
            stage_stops.into_iter()
                .filter(|s| coupler_ids.contains(s) || stop_ids.contains(s))
                .filter(|s| old_active.contains(s)
                    || if is_coupler(s) { !couplers_now.contains(s) } else { !drawn.contains(s) })
                .collect()
        };

        let to_remove: Vec<String> = old_active.iter().filter(|s| !new_active.contains(s)).cloned().collect();
        let to_add: Vec<String> = new_active.iter().filter(|s| !old_active.contains(s)).cloned().collect();

        {
            let mut drawn = drawn_stops.write();
            for stop in to_remove.iter().filter(|s| !is_coupler(s)) {
                drawn.retain(|s| s != stop);
            }
            for stop in to_add.iter().filter(|s| !is_coupler(s)) {
                if !drawn.contains(stop) { drawn.push(stop.clone()); }
            }
        }
        // Snapshot van de koppelstand VÓÓR de wijziging (voice-sync hieronder).
        let couplers_before: Vec<String> = active_couplers.read().clone();
        {
            let mut ac = active_couplers.write();
            for id in to_remove.iter().filter(|s| is_coupler(s)) {
                ac.retain(|c| c != id);
            }
            for id in to_add.iter().filter(|s| is_coupler(s)) {
                if !ac.contains(id) { ac.push(id.clone()); }
            }
        }

        *crescendo_active_stops.write() = new_active;
        *crescendo_stage.write() = new_stage;

        // DTO-vlaggen onder een KORT write-lock; de voice-sync (potentieel
        // blokkerende audio-commando's) daarna onder een read-lock.
        {
            let drawn_now = drawn_stops.read().clone();
            let couplers_now = active_couplers.read().clone();
            let mut organ = loaded_organ_info.write();
            if let Some(ref mut o) = *organ {
                for div in &mut o.divisions {
                    for stop in &mut div.stops {
                        stop.drawn = drawn_now.contains(&stop.id);
                    }
                }
                if let Some(ref mut cl) = o.couplers {
                    for c in cl.iter_mut() {
                        c.active = couplers_now.contains(&c.id);
                    }
                }
            }
        }
        {
            let organ = loaded_organ_info.read();
            if let Some(ref o) = *organ {
                if let Some(ref player) = *audio_player.read() {
                    let mappings = midi_mappings.read();
                    let couplers = active_couplers.read();
                    let held = held_notes.read();
                    for removed_id in to_remove.iter().filter(|s| !is_coupler(s)) {
                        Self::sync_stop_voices_inner(o, player, &mappings, &couplers, &held, removed_id, false);
                    }
                    for added_id in to_add.iter().filter(|s| !is_coupler(s)) {
                        Self::sync_stop_voices_inner(o, player, &mappings, &couplers, &held, added_id, true);
                    }
                    Self::sync_coupler_voices_inner(o, player, &mappings, &held, &couplers_before, &couplers);
                }
            }
        }
        (to_add, to_remove)
    }

    /// Generaal crescendo op trap `stage` zetten vanaf de command-thread
    /// (UI-klik, editor, uitschakelen, herstel). Zelfde kern als het pedaal.
    pub fn apply_crescendo_stage(&self, stage: u8) -> (Vec<String>, Vec<String>) {
        let before = *self.crescendo_stage.read();
        let (added, removed) = Self::apply_crescendo_stage_inner(
            stage, &self.crescendo_stages, &self.crescendo_stage, &self.crescendo_active_stops,
            &self.drawn_stops, &self.active_couplers, &self.audio_player, &self.loaded_organ_info,
            &self.midi_mappings, &self.held_notes,
        );
        let after = *self.crescendo_stage.read();
        tracing::info!("Crescendo (UI): stage {} → {} (+{} −{})", before, after, added.len(), removed.len());
        (added, removed)
    }

    /// Handmatige registratiewijziging (knop/preset/koppel): een register of
    /// koppel dat de organist zelf omzet is vanaf nu handregistratie — de
    /// claim van de crescendotrede vervalt. Handmatig terugduwen laat het
    /// register bij doortrappen dus weer terugkomen; handmatig aanzetten van
    /// een geclaimd register houdt het staan bij terugveren.
    pub fn note_manual_change(&self, id: &str) {
        Self::note_manual_change_inner(&self.crescendo_active_stops, id);
    }

    pub(crate) fn note_manual_change_inner(claims: &Arc<RwLock<Vec<String>>>, id: &str) {
        let mut claims = claims.write();
        if claims.iter().any(|s| s == id) {
            claims.retain(|s| s != id);
        }
    }

    /// Learn-mode wait: drains both USB and BLE channels, FORWARDS messages to audio
    /// (so notes still play during learn), and tracks the most recent CC for settling detection.
    /// This avoids the deadlock where BLE messages pile up during a blocking learn.
    fn learn_wait_settled(
        usb_rx: Option<&Receiver<MidiMessage>>,
        ble_rx: &Receiver<MidiMessage>,
        timeout: std::time::Duration,
        audio_player: &Arc<RwLock<Option<AudioPlayer>>>,
        organ_definition: &Arc<RwLock<Option<OrganDefinition>>>,
        loaded_organ_info: &Arc<RwLock<Option<OrganInfoDto>>>,
        drawn_stops: &Arc<RwLock<Vec<String>>>,
        midi_mappings: &Arc<RwLock<Vec<MidiChannelMapping>>>,
        active_couplers: &Arc<RwLock<Vec<String>>>,
        swell_bindings: &Arc<RwLock<Vec<SwellBinding>>>,
        division_gains: &Arc<RwLock<Vec<f32>>>,
        held_notes: &Arc<RwLock<Vec<(u8, u8, u8)>>>,
        ble_message_count: &Arc<std::sync::atomic::AtomicU64>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Option<(u8, u8, u8)> {
        let start = std::time::Instant::now();
        let mut last_cc: Option<(u8, u8, u8)> = None;
        let mut last_change = std::time::Instant::now();
        let settle_time = std::time::Duration::from_millis(1500);
        // 14-bit-consoles sturen per pedaalstand een paar CC n (MSB) + CC n+32
        // (LSB); zonder filter "won" de LSB als laatst gezien bericht en werd
        // de fijnregel-CC ingeleerd. Een LSB (32-63) telt niet mee zodra zijn
        // MSB (n−32) in dezelfde inleerperiode gezien is.
        let mut seen_cc = [false; 128];
        let mut note_cc = |channel: u8, controller: u8, value: u8,
                           last_cc: &mut Option<(u8, u8, u8)>, last_change: &mut std::time::Instant| {
            let c = controller as usize & 127;
            seen_cc[c] = true;
            if (32..=63).contains(&c) && seen_cc[c - 32] { return; }
            *last_cc = Some((channel, controller, value));
            *last_change = std::time::Instant::now();
        };

        while start.elapsed() < timeout {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                tracing::info!("Trede-inleer afgebroken");
                return None;
            }
            // Drain USB
            if let Some(rx) = usb_rx {
                while let Ok(msg) = rx.try_recv() {
                    if let MidiMessage::ControlChange { channel, controller, value } = msg {
                        note_cc(channel, controller, value, &mut last_cc, &mut last_change);
                    }
                    Self::handle_midi_message(
                        msg, audio_player, organ_definition, loaded_organ_info, drawn_stops,
                        midi_mappings, active_couplers, swell_bindings, division_gains, held_notes,
                        false,
                    );
                }
            }
            // Drain BLE
            while let Ok(msg) = ble_rx.try_recv() {
                ble_message_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let MidiMessage::ControlChange { channel, controller, value } = msg {
                    note_cc(channel, controller, value, &mut last_cc, &mut last_change);
                }
                Self::handle_midi_message(
                    msg, audio_player, organ_definition, loaded_organ_info, drawn_stops,
                    midi_mappings, active_couplers, swell_bindings, division_gains, held_notes,
                    false,
                );
            }

            // Check if settled
            if last_cc.is_some() && last_change.elapsed() > settle_time {
                return last_cc;
            }

            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        last_cc
    }

    /// Learn-mode wait for a specific channel/cc, with audio forwarding.
    fn learn_wait_settled_on(
        usb_rx: Option<&Receiver<MidiMessage>>,
        ble_rx: &Receiver<MidiMessage>,
        target_channel: u8,
        target_cc: u8,
        timeout: std::time::Duration,
        audio_player: &Arc<RwLock<Option<AudioPlayer>>>,
        organ_definition: &Arc<RwLock<Option<OrganDefinition>>>,
        loaded_organ_info: &Arc<RwLock<Option<OrganInfoDto>>>,
        drawn_stops: &Arc<RwLock<Vec<String>>>,
        midi_mappings: &Arc<RwLock<Vec<MidiChannelMapping>>>,
        active_couplers: &Arc<RwLock<Vec<String>>>,
        swell_bindings: &Arc<RwLock<Vec<SwellBinding>>>,
        division_gains: &Arc<RwLock<Vec<f32>>>,
        held_notes: &Arc<RwLock<Vec<(u8, u8, u8)>>>,
        ble_message_count: &Arc<std::sync::atomic::AtomicU64>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Option<u8> {
        let start = std::time::Instant::now();
        let mut last_val: Option<u8> = None;
        let mut last_change = std::time::Instant::now();
        let settle_time = std::time::Duration::from_millis(1500);

        while start.elapsed() < timeout {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                tracing::info!("Trede-inleer (fase 2) afgebroken");
                return None;
            }
            if let Some(rx) = usb_rx {
                while let Ok(msg) = rx.try_recv() {
                    if let MidiMessage::ControlChange { channel, controller, value } = msg {
                        if channel == target_channel && controller == target_cc {
                            last_val = Some(value);
                            last_change = std::time::Instant::now();
                        }
                    }
                    Self::handle_midi_message(
                        msg, audio_player, organ_definition, loaded_organ_info, drawn_stops,
                        midi_mappings, active_couplers, swell_bindings, division_gains, held_notes,
                        false,
                    );
                }
            }
            while let Ok(msg) = ble_rx.try_recv() {
                ble_message_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let MidiMessage::ControlChange { channel, controller, value } = msg {
                    if channel == target_channel && controller == target_cc {
                        last_val = Some(value);
                        last_change = std::time::Instant::now();
                    }
                }
                Self::handle_midi_message(
                    msg, audio_player, organ_definition, loaded_organ_info, drawn_stops,
                    midi_mappings, active_couplers, swell_bindings, division_gains, held_notes,
                    false,
                );
            }

            if last_val.is_some() && last_change.elapsed() > settle_time {
                return last_val;
            }

            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        last_val
    }

    fn wait_for_cc_settled_dual(
        usb_rx: Option<&Receiver<MidiMessage>>,
        ble_rx: &Receiver<MidiMessage>,
        timeout: std::time::Duration,
    ) -> Option<(u8, u8, u8)> {
        let start = std::time::Instant::now();
        let mut last_cc: Option<(u8, u8, u8)> = None;
        let mut last_change = std::time::Instant::now();

        while start.elapsed() < timeout {
            match Self::recv_any_midi(usb_rx, ble_rx, std::time::Duration::from_millis(100)) {
                Ok(MidiMessage::ControlChange { channel, controller, value }) => {
                    last_cc = Some((channel, controller, value));
                    last_change = std::time::Instant::now();
                }
                Ok(_) => continue,
                Err(_) => {
                    if last_cc.is_some() && last_change.elapsed() > std::time::Duration::from_millis(1500) {
                        return last_cc;
                    }
                    continue;
                }
            }
        }
        last_cc
    }

    /// Wait for CC on a specific channel/controller until settled
    /// Returns the settled value
    fn wait_for_cc_on(rx: &Receiver<MidiMessage>, target_channel: u8, target_cc: u8, timeout: std::time::Duration) -> Option<u8> {
        Self::wait_for_cc_on_dual(Some(rx), &crossbeam_channel::never(), target_channel, target_cc, timeout)
    }

    fn wait_for_cc_on_dual(
        usb_rx: Option<&Receiver<MidiMessage>>,
        ble_rx: &Receiver<MidiMessage>,
        target_channel: u8,
        target_cc: u8,
        timeout: std::time::Duration,
    ) -> Option<u8> {
        let start = std::time::Instant::now();
        let mut last_val: Option<u8> = None;
        let mut last_change = std::time::Instant::now();

        while start.elapsed() < timeout {
            match Self::recv_any_midi(usb_rx, ble_rx, std::time::Duration::from_millis(100)) {
                Ok(MidiMessage::ControlChange { channel, controller, value })
                    if channel == target_channel && controller == target_cc =>
                {
                    last_val = Some(value);
                    last_change = std::time::Instant::now();
                }
                Ok(_) => continue,
                Err(_) => {
                    if last_val.is_some() && last_change.elapsed() > std::time::Duration::from_millis(1500) {
                        return last_val;
                    }
                    continue;
                }
            }
        }
        last_val
    }

    /// Check if a MIDI message triggers a preset
    fn check_preset_trigger(
        msg: &MidiMessage,
        preset_bindings: &Arc<RwLock<Vec<MidiPresetBinding>>>,
        preset_trigger_tx: &Sender<u8>,
    ) {
        let bindings = preset_bindings.read();

        for binding in bindings.iter() {
            let matches = match (&binding.trigger, msg) {
                (MidiPresetTrigger::Note { channel: trigger_ch, note: trigger_note }, MidiMessage::NoteOn { channel, note, velocity }) => {
                    trigger_ch.map_or(true, |c| c == *channel) && *note == *trigger_note && *velocity > 0
                }
                (MidiPresetTrigger::ControlChange { channel: trigger_ch, controller: trigger_cc, value: trigger_val },
                 MidiMessage::ControlChange { channel, controller, value }) => {
                    trigger_ch.map_or(true, |ch| ch == *channel) &&
                    *controller == *trigger_cc &&
                    *value >= *trigger_val
                }
                (MidiPresetTrigger::ProgramChange { channel: trigger_ch, program: trigger_prog },
                 MidiMessage::ProgramChange { channel, program }) => {
                    trigger_ch.map_or(true, |ch| ch == *channel) &&
                    *program == *trigger_prog
                }
                _ => false,
            };

            if matches {
                tracing::info!("MIDI triggered preset {}", binding.preset_num);
                // try_send: zonder gemounte consumer (orgel-load, orgelbrowser)
                // liep het bounded(16)-kanaal vol en BLOKKEERDE de hele
                // MIDI-thread op een blocking send; bovendien werden de
                // opgespaarde oude piston-acties later in één keer afgespeeld
                // (auditbevinding 9). Een gemiste trigger zonder zichtbare
                // setzer is het juiste gedrag.
                let _ = preset_trigger_tx.try_send(binding.preset_num);
            }
        }
    }

    /// Handle a single MIDI message
    /// Is dit CC-bericht de ingeleerde generaal-crescendo-pedaal? Zo ja, dan mag
    /// het NIET ook als zwelkast of als CC7/11-expressie verwerkt worden: de
    /// crescendotrede voegt registers toe, maar de expressie-fallback trok
    /// tegelijk het mastervolume omlaag ("registers erbij, geluid zachter").
    /// Bewust ongeacht crescendo_enabled: een als crescendo ingeleerde pedaal
    /// is nooit tegelijk een volumepedaal.
    fn cc_claimed_by_crescendo(
        msg: &MidiMessage,
        crescendo_binding: &Arc<RwLock<Option<(u8, u8, u8, u8, bool)>>>,
    ) -> bool {
        if let MidiMessage::ControlChange { channel, controller, .. } = msg {
            if let Some((bound_ch, bound_cc, _, _, _)) = *crescendo_binding.read() {
                return *channel == bound_ch && *controller == bound_cc;
            }
        }
        false
    }

    fn handle_midi_message(
        msg: MidiMessage,
        audio_player: &Arc<RwLock<Option<AudioPlayer>>>,
        organ_definition: &Arc<RwLock<Option<OrganDefinition>>>,
        loaded_organ_info: &Arc<RwLock<Option<OrganInfoDto>>>,
        drawn_stops: &Arc<RwLock<Vec<String>>>,
        midi_mappings: &Arc<RwLock<Vec<MidiChannelMapping>>>,
        active_couplers: &Arc<RwLock<Vec<String>>>,
        swell_bindings: &Arc<RwLock<Vec<SwellBinding>>>,
        division_gains: &Arc<RwLock<Vec<f32>>>,
        held_notes: &Arc<RwLock<Vec<(u8, u8, u8)>>>,
        cc_claimed_by_crescendo: bool,
    ) {
        match msg {
            MidiMessage::NoteOn { channel, note, velocity } => {
                if velocity == 0 {
                    // Treat velocity 0 as NoteOff
                    Self::handle_midi_message(
                        MidiMessage::NoteOff { channel, note, velocity: 0 },
                        audio_player, organ_definition, loaded_organ_info, drawn_stops, midi_mappings, active_couplers,
                        swell_bindings, division_gains, held_notes, cc_claimed_by_crescendo,
                    );
                    return;
                }

                // Retrigger-normalisatie: een NoteOn op een reeds vastgehouden
                // (kanaal, noot) zónder tussenliggende NoteOff (sommige oude
                // controllers/DAW's doen dit) → éérst een NoteOff sturen. De
                // audio-refcount per pijp (audio.rs::PlayingVoice.note_on_count)
                // eist gebalanceerde NoteOn/NoteOff-paren; anders klimt de
                // teller en blijft de pijp bij release doorklinken.
                let is_retrigger = {
                    let held = held_notes.read();
                    held.iter().any(|&(ch, n, _)| ch == channel && n == note)
                };
                if is_retrigger {
                    Self::handle_midi_message(
                        MidiMessage::NoteOff { channel, note, velocity: 0 },
                        audio_player, organ_definition, loaded_organ_info, drawn_stops, midi_mappings, active_couplers,
                        swell_bindings, division_gains, held_notes, cc_claimed_by_crescendo,
                    );
                }

                // Track held notes for re-triggering when stops are toggled on
                {
                    let mut held = held_notes.write();
                    held.retain(|&(ch, n, _)| !(ch == channel && n == note));
                    held.push((channel, note, velocity));
                }

                tracing::trace!("MIDI NoteOn: ch={}, note={}, vel={}", channel, note, velocity);

                let organ = loaded_organ_info.read();
                let definition = organ_definition.read();
                let stops = drawn_stops.read();
                let mappings = midi_mappings.read();

                // Check if we have an organ loaded
                let Some(ref o) = *organ else {
                    tracing::warn!("No organ loaded");
                    return;
                };

                // Pak de command-zender onder een KORTE read-lock en laat de lock
                // dan los: het hele noot-routing-blok hierna (per stop + koppels)
                // draait buiten de player-lock met non-blocking try_send, zodat een
                // gelijktijdige audio-wissel (write-lock) niet achter een reeks
                // sends van 3s hoeft te wachten.
                let cmd_tx = {
                    let player_guard = audio_player.read();
                    match player_guard.as_ref() {
                        Some(p) => p.command_sender(),
                        None => { tracing::warn!("No audio player"); return; }
                    }
                };

                // For custom sample sets without ODF, definition may be None
                // In that case, use default first_midi_note of 36 (C1)
                let has_definition = definition.is_some();

                for division in &o.divisions {
                    // Check if this division accepts this MIDI channel
                    let mapping = mappings.iter().find(|m| m.division == division.name);
                    let accepts_channel = match mapping {
                        Some(m) => m.accepts(channel, note),
                        // Geen inleer = geen respons: piston-/setzerknoppen die
                        // noten sturen mogen niet als orgelnoten klinken. Een
                        // koppeling met kanaal "alle" blijft mogelijk; het
                        // ingeleerde toetsbereik filtert pistons op hetzelfde
                        // kanaal er ook uit.
                        None => false,
                    };

                    if !accepts_channel {
                        continue;
                    }

                    // Get transpose from mapping
                    let transpose = mapping.map_or(0, |m| m.transpose);
                    let transposed_note = (note as i16 + transpose as i16).clamp(0, 127) as u8;

                    // Collect active T.A. divisions before playing stops
                    let ta_active = {
                        let ac = active_couplers.read();
                        if let Some(ref coupler_list) = o.couplers {
                            coupler_list.iter()
                                .filter(|c| c.coupler_type == "ta" && ac.contains(&c.id))
                                .any(|c| c.source_division == division.name)
                        } else {
                            false
                        }
                    };

                    for stop in &division.stops {
                        if stops.contains(&stop.id) {
                            // T.A.: skip reed stops when Tongwerken Af is active
                            if ta_active && stop.is_reed { continue; }

                            // Use the stop's own MIDI range for proper bass/treble handling
                            let stop_first_midi = stop.first_midi_note as u32;
                            let stop_last_midi = stop.last_midi_note as u32;

                            // Check if the note is within this stop's range
                            let transposed_u32 = transposed_note as u32;
                            if transposed_u32 >= stop_first_midi && transposed_u32 <= stop_last_midi {
                                let pipe_num = transposed_u32 - stop_first_midi + 1;
                                let vel_f = velocity as f32 / 127.0;

                                tracing::trace!("Playing stop={} '{}', note={}, pipe={}, range={}-{}",
                                    stop.internal_stop_id, stop.name, transposed_note, pipe_num,
                                    stop_first_midi, stop_last_midi);

                                rt_send(&cmd_tx, AudioCommand::NoteOn {
                                    stop_id: stop.internal_stop_id,
                                    pipe_num,
                                    midi_note: transposed_note,
                                    velocity: vel_f,
                                });
                            }
                        }
                    }
                }

                // Coupler routing: play coupled division stops. De routes zijn
                // transitief uitgevouwen (Pedaal→HW + HW→NW ⇒ ook Pedaal→NW).
                let couplers = active_couplers.read();
                if !couplers.is_empty() {
                    if let Some(ref coupler_list) = o.couplers {
                        // Collect active T.A. divisions
                        let ta_divisions: Vec<String> = coupler_list.iter()
                            .filter(|c| c.coupler_type == "ta" && couplers.contains(&c.id))
                            .map(|c| c.source_division.clone())
                            .collect();

                        for route in Self::expand_coupler_routes(coupler_list, &couplers) {
                            // Melody coupler: only couple if this is the highest held note on the source division
                            if route.gate_type == "melody" {
                                let held = held_notes.read();
                                let src_mapping_m = mappings.iter().find(|m| m.division == route.source_division);
                                let highest = held.iter()
                                    .filter(|&&(ch, n, _)| src_mapping_m.map_or(false, |m| m.accepts(ch, n)))
                                    .map(|&(_, n, _)| n)
                                    .max();
                                if highest != Some(note) { continue; }
                            }

                            // Bass coupler: only couple if this is the lowest held note on the source division
                            if route.gate_type == "bass" {
                                let held = held_notes.read();
                                let src_mapping_b = mappings.iter().find(|m| m.division == route.source_division);
                                let lowest = held.iter()
                                    .filter(|&&(ch, n, _)| src_mapping_b.map_or(false, |m| m.accepts(ch, n)))
                                    .map(|&(_, n, _)| n)
                                    .min();
                                if lowest != Some(note) { continue; }
                            }

                            // Check if source division accepts this MIDI channel
                            let src_mapping = mappings.iter().find(|m| m.division == route.source_division);
                            let src_accepts = match src_mapping {
                                Some(m) => m.accepts(channel, note),
                                None => false, // geen inleer = geen respons
                            };
                            if !src_accepts { continue; }
                            let src_transpose = src_mapping.map_or(0, |m| m.transpose);
                            // Apply source transpose + (gesommeerde) coupler pitch offset
                            let coupled_note = (note as i32 + src_transpose as i32 + route.pitch_offset).clamp(0, 127) as u8;
                            // Play through destination division's drawn stops
                            if let Some(dest_div) = o.divisions.iter().find(|d| d.name == route.destination_division) {
                                let dest_ta = ta_divisions.contains(&dest_div.name);
                                for stop in &dest_div.stops {
                                    if stops.contains(&stop.id) {
                                        // T.A. op de doeldivisie: geen tongwerken meekoppelen
                                        if dest_ta && stop.is_reed { continue; }
                                        let s_first = stop.first_midi_note as u32;
                                        let s_last = stop.last_midi_note as u32;
                                        let t = coupled_note as u32;
                                        if t >= s_first && t <= s_last {
                                            let pipe_num = t - s_first + 1;
                                            let vel_f = velocity as f32 / 127.0;
                                            rt_send(&cmd_tx, AudioCommand::NoteOn {
                                                stop_id: stop.internal_stop_id,
                                                pipe_num,
                                                midi_note: coupled_note,
                                                velocity: vel_f,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            MidiMessage::NoteOff { channel, note, .. } => {
                // Remove from held notes
                {
                    let mut held = held_notes.write();
                    held.retain(|&(ch, n, _)| !(ch == channel && n == note));
                }

                let organ = loaded_organ_info.read();
                let definition = organ_definition.read();
                let mappings = midi_mappings.read();

                let Some(ref o) = *organ else { return; };
                // Zender onder korte read-lock pakken; releases (per stop + koppels)
                // gaan daarna buiten de player-lock via non-blocking try_send.
                let cmd_tx = {
                    let player_guard = audio_player.read();
                    match player_guard.as_ref() { Some(p) => p.command_sender(), None => return }
                };

                let _has_definition = definition.is_some();

                // NoteOff must be sent to ALL stops (not just drawn ones!)
                // A stop may have been un-drawn while a note was held,
                // so we must always send NoteOff to release any active voices.
                for division in &o.divisions {
                    let mapping = mappings.iter().find(|m| m.division == division.name);
                    let accepts_channel = match mapping {
                        Some(m) => m.accepts(channel, note),
                        None => false, // geen inleer = geen respons (zie NoteOn)
                    };

                    if !accepts_channel {
                        continue;
                    }

                    let transpose = mapping.map_or(0, |m| m.transpose);
                    let transposed_note = (note as i16 + transpose as i16).clamp(0, 127) as u8;

                    for stop in &division.stops {
                        // Send NoteOff regardless of drawn state
                        let stop_first_midi = stop.first_midi_note as u32;
                        let stop_last_midi = stop.last_midi_note as u32;

                        let transposed_u32 = transposed_note as u32;
                        if transposed_u32 >= stop_first_midi && transposed_u32 <= stop_last_midi {
                            let pipe_num = transposed_u32 - stop_first_midi + 1;

                            rt_send(&cmd_tx, AudioCommand::NoteOff {
                                stop_id: stop.internal_stop_id,
                                pipe_num,
                            });
                        }
                    }
                }

                // Coupler routing for NoteOff - send to ALL stops (not just drawn).
                // Zelfde transitieve route-uitvouwing als NoteOn; melody/bass-
                // poorten worden hier NIET toegepast (de gekoppelde noot kan al
                // klinken terwijl hij inmiddels niet meer de hoogste/laagste is —
                // NoteOff moet hem altijd kunnen loslaten; NoteOff op een niet-
                // klinkende pijp is onschuldig).
                let couplers = active_couplers.read();
                if !couplers.is_empty() {
                    if let Some(ref coupler_list) = o.couplers {
                        for route in Self::expand_coupler_routes(coupler_list, &couplers) {
                            let src_mapping = mappings.iter().find(|m| m.division == route.source_division);
                            let src_accepts = match src_mapping {
                                Some(m) => m.accepts(channel, note),
                                None => false, // geen inleer = geen respons
                            };
                            if !src_accepts { continue; }
                            let src_transpose = src_mapping.map_or(0, |m| m.transpose);
                            let coupled_note = (note as i32 + src_transpose as i32 + route.pitch_offset).clamp(0, 127) as u8;
                            if let Some(dest_div) = o.divisions.iter().find(|d| d.name == route.destination_division) {
                                for stop in &dest_div.stops {
                                    let s_first = stop.first_midi_note as u32;
                                    let s_last = stop.last_midi_note as u32;
                                    let t = coupled_note as u32;
                                    if t >= s_first && t <= s_last {
                                        let pipe_num = t - s_first + 1;
                                        rt_send(&cmd_tx, AudioCommand::NoteOff {
                                            stop_id: stop.internal_stop_id,
                                            pipe_num,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            MidiMessage::ControlChange { channel, controller, value } => {
                // De generaal-crescendo-CC is exclusief voor de crescendotrede
                // (al verwerkt via process_crescendo_cc): niet ook als zwelkast
                // of expressie-fallback interpreteren.
                if cc_claimed_by_crescendo { return; }
                // Zwelkast-bindingen. Write-lock: de laatste ruwe pedaalstand
                // wordt per binding onthouden (herstel na herlaad/wissel).
                let mut bindings = swell_bindings.write();
                let mut handled = false;
                for binding in bindings.iter_mut() {
                    if binding.channel == channel && binding.cc_num == controller {
                        binding.last_value = Some(value);
                        let normalized = swell_gain_from_cc(value, binding.min_val, binding.max_val, binding.invert);
                        // Update AppState gains for frontend polling
                        {
                            let mut gains = division_gains.write();
                            if (binding.division_index as usize) < gains.len() {
                                gains[binding.division_index as usize] = normalized;
                            }
                        }
                        // Naar de audio-thread via het realtime pad (try_send):
                        // een volle queue mag de MIDI-thread niet 3 s blokkeren
                        // (note-offs bleven dan liggen → hangers); bij een drop
                        // is de volgende pedaal-CC toch de waarheid.
                        let cmd_tx = audio_player.read().as_ref().map(|p| p.command_sender());
                        if let Some(tx) = cmd_tx {
                            rt_send(&tx, AudioCommand::SetDivisionGain {
                                division_index: binding.division_index,
                                gain: normalized,
                            });
                        }
                        handled = true;
                    }
                }
                drop(bindings);

                // De vroegere CC#7/CC#11→master-expressie-fallback is VERWIJDERD:
                // consoles met meerdere zwel-/crescendotreden (die standaard CC7/11
                // sturen) kregen daardoor drie ongevraagde volumeregelaars zonder
                // dat er iets was ingeleerd. Een trede doet nu niets totdat hij
                // expliciet is ingeleerd als zwelkast (divisie) of als generaal
                // crescendo.
                let _ = handled;
            }
            _ => {}
        }
    }

    /// List output devices for the default host.
    pub fn list_audio_devices(&self) -> Vec<AudioDeviceInfo> {
        self.list_audio_devices_for_host(None)
    }

    /// List output devices for a given host name (None = default host).
    pub fn list_audio_devices_for_host(&self, host_name: Option<&str>) -> Vec<AudioDeviceInfo> {
        use cpal::traits::{DeviceTrait, HostTrait};
        // ASIO: alléén drivernamen uit het register — géén cpal-enumeratie.
        // Die laadt en init't elke driver, waarbij ASIO4ALL de WASAPI-endpoints
        // exclusief claimt: de lopende stream valt weg (watchdog-herbouw bij
        // elke start) en een volgende ASIO-init in dit proces kan falen met
        // "No audio output device found". De driver wordt nu pas geladen op
        // het moment dat er echt een ASIO-stream gebouwd wordt.
        if host_name.map(|h| h.eq_ignore_ascii_case("asio")).unwrap_or(false) {
            return crate::audio::asio_driver_names()
                .into_iter()
                .enumerate()
                .map(|(i, name)| AudioDeviceInfo {
                    is_default: i == 0,
                    name,
                    // Niet opgevraagd (zou de driver initialiseren); de
                    // gangbare rates volstaan voor de instellingen-UI.
                    sample_rates: vec![44100, 48000],
                    channels: vec![2],
                })
                .collect();
        }
        let host = resolve_host(host_name);
        let default_name = host.default_output_device().and_then(|d| d.name().ok());
        let mut out = Vec::new();
        if let Ok(devices) = host.output_devices() {
            for d in devices {
                let name = match d.name() {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                let mut rates: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
                let mut chans: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
                if let Ok(cfgs) = d.supported_output_configs() {
                    for c in cfgs {
                        let lo = c.min_sample_rate().0;
                        let hi = c.max_sample_rate().0;
                        for r in [44100u32, 48000, 88200, 96000, 176400, 192000] {
                            if r >= lo && r <= hi {
                                rates.insert(r);
                            }
                        }
                        rates.insert(lo);
                        rates.insert(hi);
                        chans.insert(c.channels());
                    }
                }
                out.push(AudioDeviceInfo {
                    is_default: default_name.as_deref() == Some(name.as_str()),
                    name,
                    sample_rates: rates.into_iter().collect(),
                    channels: chans.into_iter().collect(),
                });
            }
        }
        if out.is_empty() {
            tracing::warn!("No audio output devices found for host {:?}", host_name);
        }
        out
    }

    /// List available cpal host/driver names (e.g. "WASAPI", and "ASIO" when
    /// built with the `asio` feature).
    pub fn list_audio_hosts(&self) -> Vec<String> {
        cpal::available_hosts().into_iter().map(|h| h.name().to_string()).collect()
    }

    /// Switch the audio output (host/device/buffer) and persist the choice.
    ///
    /// The new audio thread starts with empty sample maps, so the caller (the
    /// frontend) must reload the current organ afterwards — that re-registers the
    /// preload buffers AND re-applies all per-organ DSP settings via the normal
    /// post-load path. De `SwitchOutcome` vertelt de aanroeper eerlijk wat er
    /// gebeurd is: gewisseld, hersteld naar de vorige uitgang, of teruggevallen.
    pub fn switch_audio_output(&self, cfg: AudioOutputConfig) -> Result<SwitchOutcome, String> {
        // Serialiseer t.o.v. orgel-loads en andere wissels (zie load_switch_gate).
        // Tweede wissel-verzoek terwijl er al één loopt: niet in de rij gaan staan
        // (dat gaf een eeuwig hangende "bezig"-vlag), maar direct Busy melden.
        let _gate = match self.load_switch_gate.try_lock_for(std::time::Duration::from_millis(100)) {
            Some(g) => g,
            None => {
                let (actual_host, actual_device) = self.audio_player.read().as_ref()
                    .map(|p| (p.current_host.read().clone(), p.current_device.read().clone()))
                    .unwrap_or_default();
                return Ok(SwitchOutcome {
                    switched: false, player_rebuilt: false,
                    actual_host, actual_device,
                    organ_id: self.current_organ_id.read().clone(),
                    message: Some("Er loopt al een audio-wissel — nog even geduld.".into()),
                    busy: true,
                });
            }
        };
        self.switch_audio_output_inner(cfg, true, false)
    }

    /// Voer de opgeslagen ASIO-voorkeur uitgesteld uit (aangeroepen door main.rs,
    /// ~10s na de start — een kóúde ASIO-start levert op ASIO4ALL een stille
    /// stream op). Herleest de prefs en checkt de actieve host BINNEN de gate,
    /// zodat een tussentijdse handmatige keuze van de gebruiker nooit wordt
    /// teruggedraaid en de wissel niet met een lopende orgel-load interleaved.
    pub fn apply_deferred_asio_pref(&self) -> Result<Option<SwitchOutcome>, String> {
        let _gate = self.load_switch_gate.lock();
        let prefs = load_audio_prefs(&self.app_data_dir);
        let still_asio = prefs.host.as_deref()
            .map(|h| h.eq_ignore_ascii_case("asio"))
            .unwrap_or(false);
        let already_asio = self.audio_player.read().as_ref()
            .map(|p| p.current_host.read().eq_ignore_ascii_case("asio"))
            .unwrap_or(false);
        if !still_asio || already_asio {
            return Ok(None);
        }
        self.switch_audio_output_inner(AudioOutputConfig {
            host_name: prefs.host.clone(),
            device_name: prefs.device.clone(),
            buffer_frames: prefs.buffer_frames,
        }, true, false).map(Some)
    }

    /// `persist`: sla de gevraagde configuratie op als voorkeur. Alleen wanneer
    /// de wissel een bewuste keuze is (UI/deferred ASIO) — het noodherstel mag
    /// de voorkeur van de gebruiker nooit overschrijven.
    /// `force_teardown`: breek de oude player altijd éérst af (noodherstel:
    /// de oude player is aantoonbaar kapot, "bouw eerst nieuw" heeft geen zin).
    fn switch_audio_output_inner(&self, cfg: AudioOutputConfig, persist: bool, force_teardown: bool) -> Result<SwitchOutcome, String> {
        // Live registratie vastleggen vóór de wissel: de verplichte orgel-
        // herlaad hierna wist drawn_stops/koppels; de eerstvolgende load van
        // hetzelfde orgel zet dit snapshot terug (zie do_load_organ). Zo
        // overleeft de registratie een speakers↔hoofdtelefoon-wissel.
        {
            let snapshot = (
                self.current_organ_id.read().clone(),
                self.drawn_stops.read().clone(),
                self.active_couplers.read().clone(),
            );
            if snapshot.0.is_some() && (!snapshot.1.is_empty() || !snapshot.2.is_empty()) {
                *self.pending_registration_restore.write() = Some(snapshot);
            }
        }
        let new_is_asio = cfg.host_name.as_deref()
            .map(|h| h.eq_ignore_ascii_case("asio"))
            .unwrap_or(false);
        let old_is_asio = self.audio_player.read().as_ref()
            .map(|p| p.current_host.read().eq_ignore_ascii_case("asio"))
            .unwrap_or(false);
        // Huidige configuratie onthouden: bij een mislukte wissel herstellen we
        // de VORIGE uitgang (niet zomaar de default — "geluid ineens uit de
        // laptopspeakers" terwijl de UI hoofdtelefoon toont is precies de bug).
        let old_cfg = self.audio_player.read().as_ref().map(|p| {
            let host = p.current_host.read().clone();
            let device = p.current_device.read().clone();
            let buffer = *p.current_buffer_frames.read();
            AudioOutputConfig {
                host_name: if host.is_empty() { None } else { Some(host) },
                device_name: if device.is_empty() { None } else { Some(device) },
                buffer_frames: if buffer > 0 { Some(buffer) } else { None },
            }
        });

        let outcome = |switched: bool, player_rebuilt: bool, message: Option<String>, state: &Self| {
            let (actual_host, actual_device) = state.audio_player.read().as_ref()
                .map(|p| (p.current_host.read().clone(), p.current_device.read().clone()))
                .unwrap_or_default();
            SwitchOutcome {
                switched,
                player_rebuilt,
                actual_host,
                actual_device,
                organ_id: state.current_organ_id.read().clone(),
                message,
                busy: false,
            }
        };
        // Harde deadline over de hele ladder: een enkele wissel mag nooit
        // onbeperkt blijven hangen (bevriezende UI). De herstel-rungen (oude
        // uitgang / default) blijven wél doorlopen — stilte is erger dan wachten.
        let overall_deadline = std::time::Instant::now() + std::time::Duration::from_secs(12);

        // Van of naar ASIO (of noodherstel): oude player éérst volledig sluiten.
        // ASIO4ALL claimt bij init de WASAPI-endpoints exclusief; zolang onze
        // eigen stream (of zijn watchdog, die elke 2s herbouwt!) het apparaat
        // vasthoudt, faalt de ASIO-init met "No audio output device found".
        // Andersom houdt een oude ASIO-player het endpoint exclusief vast,
        // waardoor een nieuwe WASAPI-stream op datzelfde apparaat niet kan
        // starten (0x8889000A, device in use). Empirisch geverifieerd op deze
        // hardware (probe 2026-07-07). De retry-ladder vangt het asynchrone
        // vrijgeven van endpoints/driver op.
        if new_is_asio || old_is_asio || force_teardown {
            // Guard EERST laten vallen via een losse let-binding: een if-let op
            // een blok-temporary hield de write-lock de volle 2s shutdown vast,
            // waardoor MIDI-verwerking en get_status blokkeerden (audit 59).
            let old_player = { self.audio_player.write().take() };
            if let Some(old) = old_player {
                old.shutdown_and_wait(std::time::Duration::from_secs(2));
                drop(old);
            }
            // ASIO-doel: ruime ladder (driver/endpoints komen asynchroon vrij).
            // Niet-ASIO-doel: kort — als het endpoint bezet is door de idle
            // ASIO-cache helpt wachten niet; de release-stap hieronder wel.
            let ladder: &[u64] = if new_is_asio { &[0, 500, 1200, 2500] } else { &[0, 400] };
            match build_player_with_retries_deadline(&cfg, ladder, Some(overall_deadline)) {
                Ok(p) => {
                    *self.audio_player.write() = Some(p);
                    if persist {
                        save_audio_prefs(&self.app_data_dir, &AudioPrefs {
                            host: cfg.host_name.clone(),
                            device: cfg.device_name.clone(),
                            buffer_frames: cfg.buffer_frames,
                            ..load_audio_prefs(&self.app_data_dir)
                        });
                    }
                    return Ok(outcome(true, true, None, self));
                }
                Err(e) => {
                    // Doel is niet-ASIO maar er hangt nog een (idle) gecachte
                    // ASIO-driver die het endpoint exclusief bezet kan houden
                    // (0x8889000A, device in use): driver vrijgeven en nog één
                    // keer proberen. Bewust pas hier — na vrijgave kan ASIO in
                    // dit proces meestal niet meer terugkomen.
                    if !new_is_asio && crate::audio::asio_release_cached_driver() {
                        if let Ok(p) = build_player_with_retries(&cfg, &[300, 900]) {
                            *self.audio_player.write() = Some(p);
                            if persist {
                                save_audio_prefs(&self.app_data_dir, &AudioPrefs {
                                    host: cfg.host_name.clone(),
                                    device: cfg.device_name.clone(),
                                    buffer_frames: cfg.buffer_frames,
                                    ..load_audio_prefs(&self.app_data_dir)
                                });
                            }
                            return Ok(outcome(true, true, None, self));
                        }
                    }
                    // ASIO-éénrichtingsdeur: de driver werkte eerder dit proces
                    // maar is nu onbruikbaar — óf vrijgegeven (voor een WASAPI-
                    // wissel; ASIO4ALL kan per proces maar één keer initialiseren),
                    // óf nog gecachet maar dood voor enumeratie (ESI GIGAPORT eX
                    // na een WASAPI-uitstap, testorgel-log 2026-08-22). Alleen
                    // dán lost een verse processtart het gegarandeerd op — de
                    // uitgestelde ASIO-wissel bij de start doet de wissel dan
                    // alsnog. Een gewoon afwezig/bezet apparaat (kaart uit,
                    // USB losgetrokken, andere app claimt hem) valt hier NIET
                    // onder: dat hoort een nette foutmelding te geven, geen
                    // herstart. Guard-bestand voorkomt een herstart-lus: bestaat
                    // hij al (vorige herstart hielp niet), dan vallen we door
                    // naar het eerlijke faalpad.
                    if new_is_asio
                        && crate::audio::asio_worked_this_process()
                        && crate::audio::asio_door_closed()
                    {
                        let guard_path = self.app_data_dir.join("asio-herstart-guard");
                        if !guard_path.exists() {
                            // Wens expliciet vastleggen zodat de nieuwe processtart
                            // hem oppakt (ook wanneer persist niet gezet was).
                            save_audio_prefs(&self.app_data_dir, &AudioPrefs {
                                host: cfg.host_name.clone(),
                                device: cfg.device_name.clone(),
                                buffer_frames: cfg.buffer_frames,
                                ..load_audio_prefs(&self.app_data_dir)
                            });
                            // Live registratie over de herstart heen tillen: het
                            // snapshot van deze wissel naar een one-shot-bestand;
                            // de start leest hem in pending_registration_restore
                            // en de orgel-load past hem toe.
                            if let Some(snap) = self.pending_registration_restore.read().clone() {
                                if let Ok(json) = serde_json::to_string(&snap) {
                                    let _ = std::fs::write(self.app_data_dir.join("herstart-registratie.json"), json);
                                }
                            }
                            let _ = std::fs::write(&guard_path, "1");
                            let handle = self.app_handle.read().clone();
                            if let Some(app) = handle {
                                tracing::warn!(
                                    "ASIO kan in dit proces niet opnieuw initialiseren; app herstart om de wissel naar {:?} af te maken",
                                    cfg.device_name);
                                app.restart(); // keert niet terug
                            }
                            // Geen AppHandle (headless test-API): guard weer weg
                            // en gewoon doorvallen naar het faalpad.
                            let _ = std::fs::remove_file(&guard_path);
                        } else {
                            // Eerdere herstart loste het niet op: guard opruimen
                            // (her-armen voor een latere poging) en eerlijk falen.
                            let _ = std::fs::remove_file(&guard_path);
                            tracing::warn!("ASIO-herstart hielp eerder niet; wissel faalt zonder nieuwe herstart");
                        }
                    }
                    tracing::warn!("Wissel naar {:?}/{:?} definitief mislukt ({}); vorige uitgang herstellen",
                        cfg.host_name, cfg.device_name, e);
                    // Vorige uitgang terugbouwen zodat de gebruiker hoorbaar
                    // blijft waar hij was; de voorkeur blijft onaangetast.
                    // Zonder levende oude player: terugvallen op de laatst-
                    // bewezen-werkende configuratie. Ladder ruimer (WASAPI
                    // geeft endpoints asynchroon vrij; USB/BT komt traag terug).
                    let herstel_cfg = old_cfg.or_else(|| LAST_GOOD_OUTPUT.lock().clone());
                    if let Some(oldc) = herstel_cfg {
                        if let Ok(p) = build_player_with_retries(&oldc, &[0, 600, 1500]) {
                            *self.audio_player.write() = Some(p);
                            return Ok(outcome(false, true, Some(format!(
                                "Wissel mislukt: {}. De vorige uitgang is hersteld.", e)), self));
                        }
                    }
                    // Vorige uitgang ook niet terug te bouwen → default host,
                    // er moet altijd een uitweg naar geluid blijven.
                    match build_player_with_retries(&AudioOutputConfig::default(), &[0, 600]) {
                        Ok(p) => {
                            *self.audio_player.write() = Some(p);
                            Ok(outcome(false, true, Some(format!(
                                "Wissel mislukt: {}. Audio speelt nu op het standaardapparaat.", e)), self))
                        }
                        Err(e2) => {
                            // Geen herlaad meer op komst: snapshot opruimen zodat
                            // het niet bij een willekeurige latere load van
                            // hetzelfde orgel alsnog toegepast wordt (audit 60).
                            *self.pending_registration_restore.write() = None;
                            Err(format!(
                            "Audio-wissel mislukt en geen fallback beschikbaar; geluid is uit tot een nieuwe wissel slaagt: {} / {}", e, e2))
                        }
                    }
                }
            }
        } else {
            // Puur WASAPI↔WASAPI: bouw de nieuwe player éérst, wissel daarna —
            // een mislukte wissel laat de huidige uitgang dan volledig intact.
            match AudioPlayer::new(cfg.clone()) {
                Ok(new_player) => {
                    // Swap onder een kort write-lock; de OUDE player pas afbreken
                    // ná het vrijgeven van de guard (parking_lot RwLock is niet
                    // re-entrant; Drop/Shutdown mag andere lezers niet blokkeren).
                    // shutdown_and_wait i.p.v. kale drop: Drop doet alleen een
                    // niet-blokkerende stop, waardoor de oude stream nog even
                    // doorleefde — kort dubbel geluid en apparaat-contentie
                    // direct na een speakers↔hoofdtelefoon-wissel. De nieuwe
                    // stream speelt al, dus dit wachten geeft geen stilte.
                    let old = {
                        let mut guard = self.audio_player.write();
                        guard.replace(new_player)
                    };
                    if let Some(old) = old {
                        old.shutdown_and_wait(std::time::Duration::from_millis(800));
                    }
                    if persist {
                        save_audio_prefs(&self.app_data_dir, &AudioPrefs {
                            host: cfg.host_name.clone(),
                            device: cfg.device_name.clone(),
                            buffer_frames: cfg.buffer_frames,
                            ..load_audio_prefs(&self.app_data_dir)
                        });
                    }
                    Ok(outcome(true, true, None, self))
                }
                Err(e) if old_cfg.is_some() => {
                    // Mogelijk houdt een idle gecachte ASIO-driver het gevraagde
                    // WASAPI-endpoint bezet: vrijgeven en één keer opnieuw.
                    if crate::audio::asio_release_cached_driver() {
                        if let Ok(new_player) = build_player_with_retries(&cfg, &[300, 900]) {
                            let old = {
                                let mut guard = self.audio_player.write();
                                guard.replace(new_player)
                            };
                            if let Some(old) = old {
                                old.shutdown_and_wait(std::time::Duration::from_millis(800));
                            }
                            if persist {
                                save_audio_prefs(&self.app_data_dir, &AudioPrefs {
                                    host: cfg.host_name.clone(),
                                    device: cfg.device_name.clone(),
                                    buffer_frames: cfg.buffer_frames,
                                    ..load_audio_prefs(&self.app_data_dir)
                                });
                            }
                            return Ok(outcome(true, true, None, self));
                        }
                    }
                    // Oude uitgang draait gewoon door: eerlijk melden, niets
                    // herladen, voorkeuren onaangetast. Snapshot opruimen — er
                    // volgt geen herlaad, dus het mag niet blijven hangen tot
                    // een willekeurige latere load van hetzelfde orgel.
                    *self.pending_registration_restore.write() = None;
                    Ok(outcome(false, false, Some(format!(
                        "Wissel mislukt: {}. De huidige uitgang blijft actief.", e)), self))
                }
                Err(e) => {
                    // Er wás geen player (audio lag er al uit): probeer de
                    // default host zodat er in elk geval weer geluid komt.
                    match build_player_with_retries(&AudioOutputConfig::default(), &[0, 600]) {
                        Ok(p) => {
                            *self.audio_player.write() = Some(p);
                            Ok(outcome(false, true, Some(format!(
                                "Wissel mislukt: {}. Audio speelt nu op het standaardapparaat.", e)), self))
                        }
                        Err(e2) => {
                            // Geen herlaad meer op komst: snapshot opruimen zodat
                            // het niet bij een willekeurige latere load van
                            // hetzelfde orgel alsnog toegepast wordt (audit 60).
                            *self.pending_registration_restore.write() = None;
                            Err(format!(
                            "Audio-wissel mislukt en geen fallback beschikbaar; geluid is uit tot een nieuwe wissel slaagt: {} / {}", e, e2))
                        }
                    }
                }
            }
        }
    }

    /// Noodherstel van de audio: de watchdog in de audio-thread kon de stream
    /// herhaaldelijk niet herbouwen (bv. apparaat verdwenen en de vervangende
    /// default heeft een ander mix-formaat dan waarop de render-thread is
    /// gebouwd). Bouw dan de HELE player opnieuw volgens de opgeslagen
    /// voorkeur — of de default host als dat faalt — zodat er weer geluid komt.
    /// Retourneert `Some(organ_id)` wanneer er hersteld is en het orgel herladen
    /// moet worden; `None` wanneer er niets te doen was.
    pub fn recover_audio_if_flagged(&self) -> Option<Option<String>> {
        // Goedkope voorcontrole zonder gate (dit draait elke 3 s). Een player
        // die helemaal WEG is (eerdere wissel/herstel volledig mislukt) telt
        // ook als herstel-behoefte — anders blijft de app voorgoed geluidloos.
        let maybe_flagged = {
            let guard = self.audio_player.read();
            match guard.as_ref() {
                Some(p) => p.restart_needed.load(std::sync::atomic::Ordering::Relaxed),
                None => true,
            }
        };
        if !maybe_flagged {
            return None;
        }

        // Serialiseer met wissels/orgel-loads en HERCHECK de vlag bínnen de
        // gate: een gelijktijdige handmatige wissel plaatst een verse player
        // (vlag=false) en die mag hier niet overschreven worden. NB: de gate is
        // niet reentrant, dus hieronder de inner-variant gebruiken.
        let _gate = self.load_switch_gate.lock();
        let flagged = {
            let guard = self.audio_player.read();
            match guard.as_ref() {
                Some(p) => p.restart_needed.swap(false, std::sync::atomic::Ordering::Relaxed),
                None => true,
            }
        };
        if !flagged {
            return None;
        }

        // Vals alarm uitsluiten (0.7.22): lopen de callbacks inmiddels gewoon
        // weer (de stilte was tijdelijk — bv. een zware orgel-load), dan niets
        // herbouwen. De vlag is hierboven al gewist; bij een échte stilstand
        // zet de watchdog hem vanzelf opnieuw en komen we hier terug.
        let alive_before = self.audio_player.read().as_ref()
            .map(|p| p.callbacks_seen.load(std::sync::atomic::Ordering::Relaxed));
        if let Some(c0) = alive_before {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let alive_after = self.audio_player.read().as_ref()
                .map(|p| p.callbacks_seen.load(std::sync::atomic::Ordering::Relaxed));
            if let Some(c1) = alive_after {
                if c1 > c0 {
                    tracing::info!("Audio-noodherstel overgeslagen: callbacks lopen weer ({} → {})", c0, c1);
                    return None;
                }
            }
        }

        tracing::warn!("Audio-noodherstel: volledige audio-herstart (watchdog kreeg de stream niet meer aan de praat)");
        // Voorkeur pas binnen de gate lezen: een zojuist gelukte handmatige
        // keuze wordt zo meegenomen in plaats van een verouderde voorkeur.
        // force_teardown: de oude player is aantoonbaar kapot — altijd eerst
        // afbreken, "bouw eerst nieuw" zou alleen maar om het apparaat vechten.
        let prefs = load_audio_prefs(&self.app_data_dir);
        let cfg = AudioOutputConfig {
            host_name: prefs.host,
            device_name: prefs.device,
            buffer_frames: prefs.buffer_frames,
        };
        match self.switch_audio_output_inner(cfg, false, true) {
            // Ook een niet-geslaagde wissel kan een werkende fallback-player
            // hebben opgeleverd (switched=false, player_rebuilt=true): het
            // orgel moet dan evengoed herladen worden.
            Ok(o) if o.player_rebuilt => Some(o.organ_id),
            Ok(_) => None,
            Err(e) => {
                tracing::error!("Audio-noodherstel volledig mislukt ({}); nieuwe poging volgt", e);
                None
            }
        }
    }

    /// Send audio command
    pub fn send_audio_command(&self, cmd: AudioCommand) {
        if let Some(ref player) = *self.audio_player.read() {
            if let Err(e) = player.send_command(cmd) {
                tracing::warn!("Failed to send audio command: {}", e);
            }
        }
    }

    /// Get voice count
    pub fn voice_count(&self) -> usize {
        self.audio_player.read()
            .as_ref()
            .map(|p| p.voice_count())
            .unwrap_or(0)
    }

    /// Get peak meters
    pub fn peak_meters(&self) -> (f32, f32) {
        self.audio_player.read()
            .as_ref()
            .map(|p| p.peak_meters())
            .unwrap_or((0.0, 0.0))
    }

    /// Check if a stop is drawn
    pub fn is_stop_drawn(&self, stop_id: &str) -> bool {
        self.drawn_stops.read().contains(&stop_id.to_string())
    }

    /// Check of een stop_id voorkomt in het huidige geladen orgel.
    fn stop_id_exists(&self, stop_id: &str) -> bool {
        let info = self.loaded_organ_info.read();
        let info = match info.as_ref() {
            Some(o) => o,
            None => return false,
        };
        info.divisions.iter()
            .any(|d| d.stops.iter().any(|s| s.id == stop_id))
    }

    /// Toggle a stop. Geeft `None` terug als de stop_id niet bestaat in het geladen orgel.
    pub fn toggle_stop(&self, stop_id: &str) -> Option<bool> {
        // Sta uitzetten van een al-gedraaide stop altijd toe (voor cleanup-situaties),
        // maar weiger aanzetten van onbekende IDs.
        let already_drawn = self.drawn_stops.read().contains(&stop_id.to_string());
        if !already_drawn && !self.stop_id_exists(stop_id) {
            tracing::warn!("toggle_stop geweigerd: onbekende stop_id '{}'", stop_id);
            return None;
        }
        let mut drawn = self.drawn_stops.write();
        if let Some(pos) = drawn.iter().position(|s| s == stop_id) {
            drawn.remove(pos);
            Some(false)
        } else {
            drawn.push(stop_id.to_string());
            Some(true)
        }
    }

    /// Vouw de actieve koppels uit tot effectieve routes, ínclusief doorkoppelen
    /// (transitief): is Pedaal→Hoofdwerk actief én Hoofdwerk→Nevenwerk, dan
    /// ontstaat ook de route Pedaal→Nevenwerk (met gesommeerde octaaf-offsets,
    /// bv. Pedaal→HW 4' + HW→NW 16' = Pedaal→NW unisono). Het poort-type
    /// (melody/bass) van de EERSTE schakel blijft gelden — die filtert op de
    /// fysiek ingedrukte toetsen van het bronklavier; tussenliggende schakels
    /// gedragen zich als unisono-doorvoer. Cykels (HW→NW én NW→HW actief)
    /// worden afgekapt via een bezocht-set en een dieptelimiet.
    pub(crate) fn expand_coupler_routes(
        coupler_list: &[CouplerDto],
        active: &[String],
    ) -> Vec<ExpandedCouplerRoute> {
        use std::collections::HashSet;
        const MAX_DEPTH: usize = 8;
        const MAX_ROUTES: usize = 64;

        let active_couplers: Vec<&CouplerDto> = coupler_list.iter()
            .filter(|c| c.coupler_type != "ta" && active.contains(&c.id))
            .collect();

        let mut routes: Vec<ExpandedCouplerRoute> = Vec::new();
        let mut seen: HashSet<(String, String, i32, String)> = HashSet::new();

        // Start met de directe (eerste-schakel) routes. De bool markeert of de
        // laatste schakel een zelf-koppel was (bron == doel, bv. een octaaf-
        // koppel op het eigen klavier).
        let mut frontier: Vec<(ExpandedCouplerRoute, bool)> = active_couplers.iter()
            .map(|c| (ExpandedCouplerRoute {
                source_division: c.source_division.clone(),
                destination_division: c.destination_division.clone(),
                pitch_offset: c.pitch_offset,
                gate_type: c.coupler_type.clone(),
            }, c.source_division == c.destination_division))
            .collect();

        let mut depth = 0;
        while !frontier.is_empty() && depth < MAX_DEPTH && routes.len() < MAX_ROUTES {
            let mut next: Vec<(ExpandedCouplerRoute, bool)> = Vec::new();
            for (r, last_hop_self) in frontier {
                let key = (r.source_division.clone(), r.destination_division.clone(), r.pitch_offset, r.gate_type.clone());
                if !seen.insert(key) {
                    continue;
                }
                // Doorkoppelen: koppels waarvan de bron gelijk is aan het doel
                // van deze route koppelen verder (transitief).
                for c in &active_couplers {
                    if c.source_division == r.destination_division {
                        let c_self = c.source_division == c.destination_division;
                        // Zelf-koppels cascaderen niet op elkaar: één octaaf-
                        // koppel HW 4' leverde anders +12, +24, ... +96 op
                        // (elke stap een andere pitch_offset ontwijkt de
                        // seen-set; auditbevinding 19). Ná een kruis-koppel is
                        // één zelf-koppel-hop wél toegestaan (Ped→HW + HW 4'
                        // ⇒ Ped→HW+12, zoals GrandOrgue).
                        if c_self && last_hop_self {
                            continue;
                        }
                        next.push((ExpandedCouplerRoute {
                            source_division: r.source_division.clone(),
                            destination_division: c.destination_division.clone(),
                            pitch_offset: r.pitch_offset + c.pitch_offset,
                            gate_type: r.gate_type.clone(),
                        }, c_self));
                    }
                }
                routes.push(r);
            }
            frontier = next;
            depth += 1;
        }
        routes
    }

    /// GrandOrgue-gedrag: een register dat aan/uit gaat terwijl toetsen zijn
    /// ingedrukt moet direct klinken/zwijgen. Bij AAN worden de ingedrukte
    /// toetsen door het register gespeeld — zowel via de directe klavier-route
    /// als via actieve koppels die naar de divisie van het register wijzen.
    /// Bij UIT worden alle pijpen van het register direct losgelaten.
    pub fn sync_stop_voices(&self, stop_id: &str, drawn: bool) {
        let organ = self.loaded_organ_info.read();
        let Some(ref o) = *organ else { return; };
        let player_guard = self.audio_player.read();
        let Some(ref player) = *player_guard else { return; };
        let mappings = self.midi_mappings.read();
        let couplers = self.active_couplers.read();
        let held = self.held_notes.read();
        Self::sync_stop_voices_inner(o, player, &mappings, &couplers, &held, stop_id, drawn);
    }

    /// Kern van sync_stop_voices, ook aanroepbaar vanaf de MIDI-thread
    /// (crescendo-pedaal) waar de state via losse Arcs beschikbaar is.
    pub(crate) fn sync_stop_voices_inner(
        o: &OrganInfoDto,
        player: &AudioPlayer,
        mappings: &[MidiChannelMapping],
        active_couplers: &[String],
        held: &[(u8, u8, u8)],
        stop_id: &str,
        drawn: bool,
    ) {
        // Vind het register + zijn divisie.
        let Some((div_name, stop)) = o.divisions.iter().find_map(|d| {
            d.stops.iter().find(|s| s.id == stop_id).map(|s| (d.name.as_str(), s))
        }) else { return; };

        let first = stop.first_midi_note as u32;
        let last = stop.last_midi_note as u32;
        if last < first { return; }
        // Realtime pad (try_send): een trapwissel/preset mag de MIDI-thread niet
        // tot 3 s per commando blokkeren bij een volle audio-queue (hangers).
        let tx = player.command_sender();

        if !drawn {
            // Register weggetrokken: alle klinkende pijpen in één commando
            // loslaten (een preset-wissel kan tientallen registers tegelijk
            // wegtrekken — per-pijp NoteOffs zouden de command-queue vol duwen).
            rt_send(&tx, AudioCommand::ReleaseStop {
                stop_id: stop.internal_stop_id,
            });
            return;
        }

        if held.is_empty() { return; }

        let send_note_on = |note: u8, vel: u8| {
            let t = note as u32;
            if t >= first && t <= last {
                rt_send(&tx, AudioCommand::NoteOn {
                    stop_id: stop.internal_stop_id,
                    pipe_num: t - first + 1,
                    midi_note: note,
                    velocity: vel as f32 / 127.0,
                });
            }
        };

        // T.A. (Tongwerken Af) actief op deze divisie? Dan geen tongwerken starten.
        let ta_active = o.couplers.as_ref().map_or(false, |cl| cl.iter().any(|c| {
            c.coupler_type == "ta" && active_couplers.contains(&c.id) && c.source_division == div_name
        }));

        // 1) Directe route: ingedrukte toetsen op het eigen klavier van het register.
        if !(ta_active && stop.is_reed) {
            let mapping = mappings.iter().find(|m| m.division == div_name);
            for &(ch, note, vel) in held {
                let accepts = mapping.map_or(false, |m| m.accepts(ch, note));
                if !accepts { continue; }
                let transpose = mapping.map_or(0, |m| m.transpose);
                let transposed = (note as i16 + transpose as i16).clamp(0, 127) as u8;
                send_note_on(transposed, vel);
            }
        }

        // 2) Koppel-routes: actieve koppels met de divisie van dit register als
        // bestemming spelen hun bronklavier-toetsen ook door dit register —
        // transitief uitgevouwen (Pedaal→HW + HW→NW telt ook als Pedaal→NW).
        if let Some(cl) = o.couplers.as_ref() {
            for route in Self::expand_coupler_routes(cl, active_couplers) {
                if route.destination_division != div_name { continue; }
                let src_mapping = mappings.iter().find(|m| m.division == route.source_division);
                let src_notes: Vec<(u8, u8)> = held.iter()
                    .filter(|&&(ch, n, _)| src_mapping.map_or(false, |m| m.accepts(ch, n)))
                    .map(|&(_, n, v)| (n, v))
                    .collect();
                for &(note, vel) in &src_notes {
                    // Melody/bass-poort (eerste schakel): alleen de hoogste
                    // resp. laagste toets van het bronklavier.
                    if route.gate_type == "melody"
                        && src_notes.iter().map(|&(n, _)| n).max() != Some(note) { continue; }
                    if route.gate_type == "bass"
                        && src_notes.iter().map(|&(n, _)| n).min() != Some(note) { continue; }
                    let src_transpose = src_mapping.map_or(0, |m| m.transpose);
                    let coupled = (note as i32 + src_transpose as i32 + route.pitch_offset).clamp(0, 127) as u8;
                    send_note_on(coupled, vel);
                }
            }
        }
    }

    /// Koppelstand gewijzigd terwijl toetsen zijn ingedrukt: stemmen synchroniseren.
    /// Zonder dit kregen stemmen die via een koppel klonken NOOIT meer een NoteOff
    /// zodra dat koppel uitging (NoteOff vouwt routes uit met de NIEUWE koppelstand)
    /// — de "hangers" bij crescendotrappen met koppels. Route-diff oud↔nieuw:
    /// - weggevallen routes → NoteOff naar álle registers van de doeldivisie
    ///   (drawn of niet; NoteOff op een stille pijp is onschuldig — zelfde
    ///   afspraak als het normale NoteOff-pad)
    /// - nieuwe routes → NoteOn voor getrokken registers van de doeldivisie
    ///   (GrandOrgue-gedrag: bijgekoppeld klavier klinkt direct mee)
    /// Refcount per pijp (audio.rs::PlayingVoice.note_on_count) zorgt dat een
    /// pijp die zowel via een blijvende route als via een vervallende koppel
    /// klonk, alleen zijn koppel-trigger verliest — de directe klank blijft
    /// door de nog-actieve refcount gewoon staan.
    pub(crate) fn sync_coupler_voices_inner(
        o: &OrganInfoDto,
        player: &AudioPlayer,
        mappings: &[MidiChannelMapping],
        held: &[(u8, u8, u8)],
        old_couplers: &[String],
        new_couplers: &[String],
    ) {
        if held.is_empty() || old_couplers == new_couplers { return; }
        let Some(ref coupler_list) = o.couplers else { return; };
        let tx = player.command_sender();

        let route_key = |r: &ExpandedCouplerRoute| (
            r.source_division.clone(), r.destination_division.clone(), r.pitch_offset, r.gate_type.clone()
        );
        let old_routes = Self::expand_coupler_routes(coupler_list, old_couplers);
        let new_routes = Self::expand_coupler_routes(coupler_list, new_couplers);
        let old_keys: std::collections::HashSet<_> = old_routes.iter().map(route_key).collect();
        let new_keys: std::collections::HashSet<_> = new_routes.iter().map(route_key).collect();

        // 1) Weggevallen routes → NoteOff (geen melody/bass-filter: de gekoppelde
        //    noot kan klinken terwijl hij inmiddels niet meer de hoogste/laagste is).
        for route in old_routes.iter().filter(|r| !new_keys.contains(&route_key(r))) {
            let src_mapping = mappings.iter().find(|m| m.division == route.source_division);
            let Some(dest_div) = o.divisions.iter().find(|d| d.name == route.destination_division) else { continue; };
            for &(ch, note, _v) in held {
                if !src_mapping.map_or(false, |m| m.accepts(ch, note)) { continue; }
                let src_transpose = src_mapping.map_or(0, |m| m.transpose);
                let t = (note as i32 + src_transpose as i32 + route.pitch_offset).clamp(0, 127) as u32;
                for stop in &dest_div.stops {
                    let s_first = stop.first_midi_note as u32;
                    let s_last = stop.last_midi_note as u32;
                    if t >= s_first && t <= s_last {
                        rt_send(&tx, AudioCommand::NoteOff {
                            stop_id: stop.internal_stop_id,
                            pipe_num: t - s_first + 1,
                        });
                    }
                }
            }
        }

        // 2) Nieuwe routes → NoteOn voor getrokken registers (met melody/bass-poort
        //    en T.A.-regel, zoals sync_stop_voices_inner).
        for route in new_routes.iter().filter(|r| !old_keys.contains(&route_key(r))) {
            let src_mapping = mappings.iter().find(|m| m.division == route.source_division);
            let src_notes: Vec<(u8, u8)> = held.iter()
                .filter(|&&(ch, n, _)| src_mapping.map_or(false, |m| m.accepts(ch, n)))
                .map(|&(_, n, v)| (n, v))
                .collect();
            if src_notes.is_empty() { continue; }
            let Some(dest_div) = o.divisions.iter().find(|d| d.name == route.destination_division) else { continue; };
            let ta_active = coupler_list.iter().any(|c| {
                c.coupler_type == "ta" && new_couplers.contains(&c.id) && c.source_division == dest_div.name
            });
            for &(note, vel) in &src_notes {
                if route.gate_type == "melody"
                    && src_notes.iter().map(|&(n, _)| n).max() != Some(note) { continue; }
                if route.gate_type == "bass"
                    && src_notes.iter().map(|&(n, _)| n).min() != Some(note) { continue; }
                let src_transpose = src_mapping.map_or(0, |m| m.transpose);
                let t = (note as i32 + src_transpose as i32 + route.pitch_offset).clamp(0, 127) as u32;
                for stop in dest_div.stops.iter().filter(|s| s.drawn) {
                    if ta_active && stop.is_reed { continue; }
                    let s_first = stop.first_midi_note as u32;
                    let s_last = stop.last_midi_note as u32;
                    if t >= s_first && t <= s_last {
                        rt_send(&tx, AudioCommand::NoteOn {
                            stop_id: stop.internal_stop_id,
                            pipe_num: t - s_first + 1,
                            midi_note: t as u8,
                            velocity: vel as f32 / 127.0,
                        });
                    }
                }
            }
        }
    }

    /// Wrapper voor de command-thread (toggle_coupler / set_active_couplers);
    /// de MIDI-thread (crescendo) roept sync_coupler_voices_inner direct aan.
    pub fn sync_coupler_voices(&self, old_couplers: &[String], new_couplers: &[String]) {
        let organ = self.loaded_organ_info.read();
        let Some(ref o) = *organ else { return; };
        let player_guard = self.audio_player.read();
        let Some(ref player) = *player_guard else { return; };
        let mappings = self.midi_mappings.read();
        let held = self.held_notes.read();
        Self::sync_coupler_voices_inner(o, player, &mappings, &held, old_couplers, new_couplers);
    }

    /// Set a stop drawn state
    pub fn set_stop_drawn(&self, stop_id: &str, drawn: bool) {
        let mut stops = self.drawn_stops.write();
        let exists = stops.contains(&stop_id.to_string());

        if drawn && !exists {
            stops.push(stop_id.to_string());
        } else if !drawn && exists {
            stops.retain(|s| s != stop_id);
        }
    }

    /// Automatisch MIDI-archief: lopende take geforceerd afsluiten en synchroon
    /// wegschrijven (afsluiten app / test-API). None als er geen take liep, de
    /// take afgekeurd is, of de MIDI-thread niet binnen `timeout` antwoordt
    /// (bv. midden in een leer-modus).
    pub fn midi_archive_flush(&self, timeout: std::time::Duration) -> Option<std::path::PathBuf> {
        let (tx, rx) = bounded(1);
        if self.midi_tx.send(MidiCommand::ArchiveFlush(tx)).is_err() {
            return None;
        }
        rx.recv_timeout(timeout).ok().flatten()
    }

    /// List MIDI devices
    pub fn list_midi_devices(&self) -> Vec<MidiDeviceInfo> {
        let (tx, rx) = bounded(1);
        if self.midi_tx.send(MidiCommand::ListDevices(tx)).is_ok() {
            // Timeout: de MIDI-thread kan tot ~65s in een leer-handler zitten;
            // een kale recv() blokkeerde dan alle Tauri-commands (audit 42).
            rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Connect to a MIDI device
    pub fn connect_midi(&self, device_name: &str) -> Result<(), String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::Connect(device_name.to_string(), tx))
            .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| "MIDI-thread bezet (leermodus actief?) — probeer zo opnieuw".to_string())?
    }

    /// Connect to all MIDI devices
    pub fn connect_all_midi(&self) -> Result<usize, String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::ConnectAll(tx))
            .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .map_err(|_| "MIDI-thread bezet (leermodus actief?) — probeer zo opnieuw".to_string())?
    }

    /// Disconnect all MIDI devices
    pub fn disconnect_midi(&self) {
        let _ = self.midi_tx.send(MidiCommand::Disconnect);
    }

    /// Set MIDI channel mapping for a division
    pub fn set_midi_mapping(&self, division: &str, channel: Option<u8>, transpose: i8) {
        let mut mappings = self.midi_mappings.write();

        // Update or add mapping
        if let Some(mapping) = mappings.iter_mut().find(|m| m.division == division) {
            mapping.channel = channel;
            mapping.transpose = transpose;
        } else {
            mappings.push(MidiChannelMapping {
                division: division.to_string(),
                channel,
                transpose,
                first_midi_note: None,
                last_midi_note: None,
            });
        }

        // Send to MIDI thread
        let _ = self.midi_tx.send(MidiCommand::SetChannelMapping(mappings.clone()));
    }

    /// Get MIDI mappings
    pub fn get_midi_mappings(&self) -> Vec<MidiChannelMapping> {
        self.midi_mappings.read().clone()
    }

    /// Learn MIDI channel for a division (waits for next MIDI input)
    pub fn learn_midi_channel(&self, division: &str) -> Result<Option<u8>, String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::LearnChannel(division.to_string(), tx))
            .map_err(|e| e.to_string())?;
        // Ruim boven de eigen leer-timeout van de handler, plus marge voor een
        // eventueel nog lopende ándere leer-sessie.
        rx.recv_timeout(std::time::Duration::from_secs(100))
            .map_err(|_| "MIDI-thread reageert niet (leermodus?)".to_string())
    }

    /// Learn keyboard range for a division (two-note: press lowest, then highest key)
    /// The first_sample_note is the MIDI note of the first sample (usually 36 for C)
    pub fn learn_keyboard_range(&self, division: &str, first_sample_note: u8) -> Result<Option<LearnedKeyboardRange>, String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::LearnKeyboardRange(division.to_string(), first_sample_note, tx))
            .map_err(|e| e.to_string())?;
        // Use a longer timeout since we need two notes
        rx.recv_timeout(std::time::Duration::from_secs(65))
            .map_err(|e| e.to_string())
    }

    /// Get preset bindings
    pub fn get_preset_bindings(&self) -> Vec<MidiPresetBinding> {
        self.preset_bindings.read().clone()
    }

    /// Set preset bindings
    pub fn set_preset_bindings(&self, bindings: Vec<MidiPresetBinding>) {
        *self.preset_bindings.write() = bindings.clone();
        let _ = self.midi_tx.send(MidiCommand::SetPresetBindings(bindings));
    }

    /// Learn MIDI binding for a preset (waits for next MIDI input)
    pub fn learn_preset_binding(&self, preset_num: u8) -> Result<Option<MidiPresetTrigger>, String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::LearnPresetBinding(preset_num, tx))
            .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(100))
            .map_err(|_| "MIDI-thread reageert niet (leermodus?)".to_string())
    }

    /// Clear all MIDI bindings for a specific action/preset number
    pub fn clear_preset_bindings_for(&self, preset_num: u8) {
        let mut bindings = self.preset_bindings.write();
        bindings.retain(|b| b.preset_num != preset_num);
        let updated = bindings.clone();
        let _ = self.midi_tx.send(MidiCommand::SetPresetBindings(updated));
    }

    /// Voeg handmatig een koppeling toe (zelfde regels als inleren: max 4 per
    /// actie, exacte duplicaten geweigerd). De MIDI-thread houdt een eigen
    /// kopie bij, dus na elke mutatie gaat de volledige lijst via
    /// SetPresetBindings mee.
    pub fn add_preset_binding(&self, binding: MidiPresetBinding) -> Result<(), String> {
        let mut bindings = self.preset_bindings.write();
        let count = bindings.iter().filter(|b| b.preset_num == binding.preset_num).count();
        if count >= 4 {
            return Err("Maximaal 4 MIDI-koppelingen per actie; verwijder er eerst één".to_string());
        }
        if bindings.iter().any(|b| *b == binding) {
            return Err("Deze MIDI-koppeling bestaat al voor deze actie".to_string());
        }
        bindings.push(binding);
        let updated = bindings.clone();
        drop(bindings);
        let _ = self.midi_tx.send(MidiCommand::SetPresetBindings(updated));
        Ok(())
    }

    /// Verwijder één specifieke koppeling: de `index`-de binding (0-based)
    /// binnen de bindingen van deze actiecode.
    pub fn remove_preset_binding_at(&self, preset_num: u8, index: usize) -> Result<(), String> {
        let mut bindings = self.preset_bindings.write();
        let pos = bindings.iter()
            .enumerate()
            .filter(|(_, b)| b.preset_num == preset_num)
            .map(|(i, _)| i)
            .nth(index)
            .ok_or_else(|| format!("Geen MIDI-koppeling #{} voor actie {}", index + 1, preset_num))?;
        bindings.remove(pos);
        let updated = bindings.clone();
        drop(bindings);
        let _ = self.midi_tx.send(MidiCommand::SetPresetBindings(updated));
        Ok(())
    }

    /// Vervang één specifieke koppeling in-place (bewerken in de UI).
    pub fn replace_preset_binding_at(&self, preset_num: u8, index: usize, new_binding: MidiPresetBinding) -> Result<(), String> {
        let mut bindings = self.preset_bindings.write();
        if bindings.iter().any(|b| *b == new_binding) {
            return Err("Deze MIDI-koppeling bestaat al".to_string());
        }
        let pos = bindings.iter()
            .enumerate()
            .filter(|(_, b)| b.preset_num == preset_num)
            .map(|(i, _)| i)
            .nth(index)
            .ok_or_else(|| format!("Geen MIDI-koppeling #{} voor actie {}", index + 1, preset_num))?;
        bindings[pos] = new_binding;
        let updated = bindings.clone();
        drop(bindings);
        let _ = self.midi_tx.send(MidiCommand::SetPresetBindings(updated));
        Ok(())
    }

    /// Poll for triggered presets (non-blocking)
    pub fn poll_preset_trigger(&self) -> Option<u8> {
        self.preset_trigger_rx.try_recv().ok()
    }

    /// Toggle a coupler on/off, returns new state
    pub fn toggle_coupler(&self, coupler_id: &str) -> bool {
        let mut couplers = self.active_couplers.write();
        if let Some(pos) = couplers.iter().position(|c| c == coupler_id) {
            couplers.remove(pos);
            false
        } else {
            couplers.push(coupler_id.to_string());
            true
        }
    }

    /// Get active couplers
    pub fn get_active_couplers(&self) -> Vec<String> {
        self.active_couplers.read().clone()
    }

    /// Set active couplers (for preset recall)
    pub fn set_active_couplers(&self, coupler_ids: Vec<String>) {
        *self.active_couplers.write() = coupler_ids;
    }

    /// Set division gain (from UI click on LED slider)
    pub fn set_division_gain(&self, division_index: u8, gain: f32) {
        {
            let mut gains = self.division_gains.write();
            if (division_index as usize) < gains.len() {
                gains[division_index as usize] = gain;
            }
        }
        self.send_audio_command(AudioCommand::SetDivisionGain { division_index, gain });
    }

    /// Get all division gains (for frontend polling)
    pub fn get_division_gains(&self) -> Vec<f32> {
        self.division_gains.read().clone()
    }

    /// Reset all division gains to 1.0 (fully open)
    pub fn reset_division_gains(&self, count: usize) {
        let gains = vec![1.0; count.max(32)];
        *self.division_gains.write() = gains;
    }

    /// Reset per-divisie instellingen (output-kanalen, C/Cis, windgroep) bij orgelwissel,
    /// zodat divisie-index i van het nieuwe orgel niets erft van het vorige. De opgeslagen
    /// waarden van dít orgel worden hierna door apply_saved_* opnieuw toegepast.
    pub fn reset_division_settings(&self, count: usize) {
        let n = count.max(32);
        *self.division_output_channels.write() = vec![Vec::new(); n];
        *self.division_ccis_enabled.write() = vec![false; n];
        *self.division_wind_groups.write() = (0..n as u8).collect();
        // Per-divisie DSP-spiegels wissen zodat het nieuwe orgel niets erft van het vorige.
        self.division_pans.write().clear();
        self.division_swell_configs.write().clear();
        self.division_tremulants.write().clear();
        self.wind_group_configs.write().clear();
        // Live tremulantstand (spiegel voor de afstandsbediening) hoort bij het vorige orgel.
        self.tremulant_live.write().clear();
    }

    /// Wacht op een pedaalstand (kanaal, cc, waarde) — stapsgewijze trede-inleer.
    pub fn learn_pedal_position(&self, filter: Option<(u8, u8)>) -> Result<Option<(u8, u8, u8)>, String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::LearnPedalPosition(filter, tx))
            .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(25))
            .map_err(|_| "MIDI-thread reageert niet (loopt er nog een leermodus?)".to_string())
    }

    /// Wacht op één toetsaanslag (kanaal, noot) — stapsgewijze klavier-inleer.
    pub fn learn_single_note(&self) -> Result<Option<(u8, u8)>, String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::LearnSingleNote(tx))
            .map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(25))
            .map_err(|_| "MIDI-thread reageert niet (loopt er nog een leermodus?)".to_string())
    }

    /// Learn swell pedal binding for a division (blocking, waits for CC)
    pub fn learn_swell_pedal(&self, division_name: &str, division_index: u8) -> Result<Option<SwellBinding>, String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::LearnSwellPedal(division_name.to_string(), division_index, tx))
            .map_err(|e| e.to_string())?;
        // Long timeout: two CC settle phases (15s each) + margin
        rx.recv_timeout(std::time::Duration::from_secs(35))
            .map_err(|e| e.to_string())
    }

    /// Clear swell pedal binding for a division
    pub fn clear_swell_binding(&self, division_name: &str) {
        let _ = self.midi_tx.send(MidiCommand::ClearSwellPedal(division_name.to_string()));
        self.swell_bindings.write().retain(|b| b.division_name != division_name);
    }

    /// Get all swell bindings
    pub fn get_swell_bindings(&self) -> Vec<SwellBinding> {
        self.swell_bindings.read().clone()
    }

    /// Replace all preset bindings (for library restore)
    pub fn set_preset_bindings_replace(&self, bindings: Vec<MidiPresetBinding>) {
        *self.preset_bindings.write() = bindings.clone();
        let _ = self.midi_tx.send(MidiCommand::SetPresetBindings(bindings));
    }

    /// Replace all swell bindings (for library restore)
    pub fn set_swell_bindings_replace(&self, bindings: Vec<SwellBinding>) {
        *self.swell_bindings.write() = bindings;
    }

    /// Zet handmatig kanaal + CC voor de zwelkast van een divisie. Bestaande
    /// min/max/invert blijven behouden; zonder bestaande binding worden de
    /// gangbare defaults gebruikt (volledig bereik, niet gespiegeld). De
    /// MIDI-thread leest deze gedeelde lijst rechtstreeks.
    pub fn set_swell_binding_manual(&self, division_name: &str, division_index: u8, channel: u8, cc_num: u8) {
        let mut bindings = self.swell_bindings.write();
        if let Some(b) = bindings.iter_mut().find(|b| b.division_name == division_name) {
            b.channel = channel;
            b.cc_num = cc_num;
            b.division_index = division_index;
        } else {
            bindings.push(SwellBinding {
                division_name: division_name.to_string(),
                division_index,
                channel,
                cc_num,
                min_val: 0,
                max_val: 127,
                invert: false,
                last_value: None,
            });
        }
    }

    /// Zet handmatig het toetsenbereik (laagste/hoogste MIDI-noot) van een
    /// divisie — het handmatige alternatief voor de twee-noten-inleer.
    pub fn set_midi_mapping_range(&self, division: &str, first: Option<u8>, last: Option<u8>) {
        let mut mappings = self.midi_mappings.write();
        if let Some(m) = mappings.iter_mut().find(|m| m.division == division) {
            m.first_midi_note = first;
            m.last_midi_note = last;
        } else {
            mappings.push(MidiChannelMapping {
                division: division.to_string(),
                channel: None,
                transpose: 0,
                first_midi_note: first,
                last_midi_note: last,
            });
        }
        let _ = self.midi_tx.send(MidiCommand::SetChannelMapping(mappings.clone()));
    }

    /// Set MIDI channel mappings (for library restore)
    pub fn set_midi_mappings(&self, mappings: Vec<MidiChannelMapping>) {
        *self.midi_mappings.write() = mappings.clone();
        let _ = self.midi_tx.send(MidiCommand::SetChannelMapping(mappings));
    }
}

#[cfg(test)]
mod coupler_route_tests {
    use super::*;

    fn coupler(id: &str, src: &str, dest: &str, ctype: &str, offset: i32) -> CouplerDto {
        CouplerDto {
            id: id.to_string(),
            name: id.to_string(),
            source_division: src.to_string(),
            destination_division: dest.to_string(),
            active: false,
            display_in_division: src.to_string(),
            midi_action_code: 0,
            coupler_type: ctype.to_string(),
            pitch_offset: offset,
        }
    }

    #[test]
    fn doorkoppelen_pedaal_via_hoofdwerk_naar_nevenwerk() {
        // Pedaal→HW en HW→NW actief ⇒ ook een route Pedaal→NW (transitief).
        let list = vec![
            coupler("c1", "Pedaal", "Hoofdwerk", "unison", 0),
            coupler("c2", "Hoofdwerk", "Nevenwerk", "unison", 0),
        ];
        let active = vec!["c1".to_string(), "c2".to_string()];
        let routes = AppState::expand_coupler_routes(&list, &active);
        assert!(routes.iter().any(|r| r.source_division == "Pedaal" && r.destination_division == "Hoofdwerk"));
        assert!(routes.iter().any(|r| r.source_division == "Hoofdwerk" && r.destination_division == "Nevenwerk"));
        assert!(routes.iter().any(|r| r.source_division == "Pedaal" && r.destination_division == "Nevenwerk" && r.pitch_offset == 0),
                "transitieve route Pedaal→Nevenwerk ontbreekt: {:?}", routes.iter().map(|r| (r.source_division.clone(), r.destination_division.clone())).collect::<Vec<_>>());
    }

    #[test]
    fn octaaf_offsets_sommeren_over_de_keten() {
        // Pedaal→HW 4' (+12) + HW→NW 16' (-12) ⇒ Pedaal→NW unisono (0).
        let list = vec![
            coupler("c1", "Pedaal", "Hoofdwerk", "super", 12),
            coupler("c2", "Hoofdwerk", "Nevenwerk", "sub", -12),
        ];
        let active = vec!["c1".to_string(), "c2".to_string()];
        let routes = AppState::expand_coupler_routes(&list, &active);
        assert!(routes.iter().any(|r| r.source_division == "Pedaal" && r.destination_division == "Nevenwerk" && r.pitch_offset == 0));
    }

    #[test]
    fn cykel_wordt_afgekapt() {
        // HW→NW én NW→HW actief: mag niet oneindig doorlopen; beide directe
        // routes bestaan, en de terug-route levert geen self-route-explosie op.
        let list = vec![
            coupler("c1", "Hoofdwerk", "Nevenwerk", "unison", 0),
            coupler("c2", "Nevenwerk", "Hoofdwerk", "unison", 0),
        ];
        let active = vec!["c1".to_string(), "c2".to_string()];
        let routes = AppState::expand_coupler_routes(&list, &active);
        assert!(routes.len() <= 64);
        assert!(routes.iter().any(|r| r.source_division == "Hoofdwerk" && r.destination_division == "Nevenwerk"));
        assert!(routes.iter().any(|r| r.source_division == "Nevenwerk" && r.destination_division == "Hoofdwerk"));
    }

    #[test]
    fn inactieve_en_ta_koppels_doen_niet_mee() {
        let list = vec![
            coupler("c1", "Pedaal", "Hoofdwerk", "unison", 0),
            coupler("c2", "Hoofdwerk", "Nevenwerk", "unison", 0),
            coupler("ta1", "Hoofdwerk", "Hoofdwerk", "ta", 0),
        ];
        let active = vec!["c1".to_string(), "ta1".to_string()]; // c2 NIET actief
        let routes = AppState::expand_coupler_routes(&list, &active);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].destination_division, "Hoofdwerk");
    }
}

#[cfg(test)]
mod recording_tests {
    use super::*;

    fn new_active() -> Arc<RwLock<Option<MidiRecording>>> {
        Arc::new(RwLock::new(Some(MidiRecording {
            events: Vec::new(),
            start_time: std::time::Instant::now(),
            active: true,
            stopped_elapsed: None,
        })))
    }

    #[test]
    fn captures_note_on_and_off_with_correct_bytes() {
        let rec = new_active();
        AppState::capture_midi_event(&rec, &MidiMessage::NoteOn { channel: 2, note: 60, velocity: 100 });
        AppState::capture_midi_event(&rec, &MidiMessage::NoteOff { channel: 2, note: 60, velocity: 0 });
        let g = rec.read();
        let evs = &g.as_ref().unwrap().events;
        assert_eq!(evs.len(), 2);
        // (timestamp_us, status, data1, data2, channel)
        assert_eq!(evs[0].1, 0x92); // NoteOn op kanaal 2
        assert_eq!(evs[0].2, 60);
        assert_eq!(evs[0].3, 100);
        assert_eq!(evs[0].4, 2);
        assert_eq!(evs[1].1, 0x82); // NoteOff op kanaal 2
        // Timestamps lopen niet terug.
        assert!(evs[1].0 >= evs[0].0);
    }

    #[test]
    fn captures_control_change_and_program_change() {
        let rec = new_active();
        AppState::capture_midi_event(&rec, &MidiMessage::ControlChange { channel: 0, controller: 7, value: 64 });
        AppState::capture_midi_event(&rec, &MidiMessage::ProgramChange { channel: 1, program: 5 });
        let g = rec.read();
        let evs = &g.as_ref().unwrap().events;
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].1, 0xB0); // CC
        assert_eq!(evs[0].2, 7);
        assert_eq!(evs[0].3, 64);
        assert_eq!(evs[1].1, 0xC1); // PC op kanaal 1
        assert_eq!(evs[1].2, 5);
    }

    #[test]
    fn skips_realtime_and_sysex() {
        let rec = new_active();
        AppState::capture_midi_event(&rec, &MidiMessage::Clock);
        AppState::capture_midi_event(&rec, &MidiMessage::Start);
        AppState::capture_midi_event(&rec, &MidiMessage::SysEx(vec![1, 2, 3]));
        assert_eq!(rec.read().as_ref().unwrap().events.len(), 0);
    }

    #[test]
    fn noop_when_not_recording() {
        let rec: Arc<RwLock<Option<MidiRecording>>> = Arc::new(RwLock::new(None));
        AppState::capture_midi_event(&rec, &MidiMessage::NoteOn { channel: 0, note: 60, velocity: 100 });
        assert!(rec.read().is_none());
    }
}

#[cfg(test)]
mod crescendo_tests {
    use super::*;
    use crate::commands::{DivisionDto, StopDto};

    fn stop(id: &str, n: u32) -> StopDto {
        StopDto {
            id: id.to_string(), name: id.to_string(), pitch: "8".to_string(), drawn: false,
            color: None, has_tremulant: false, midi_action_code: 0, internal_stop_id: n,
            first_midi_note: 36, last_midi_note: 96, is_reed: false,
        }
    }

    fn organ() -> OrganInfoDto {
        OrganInfoDto {
            id: "test".into(), name: "Test".into(), builder: String::new(), location: String::new(),
            year: None, stop_count: 3,
            divisions: vec![DivisionDto {
                name: "Hoofdwerk".into(), display_name: "Hoofdwerk".into(),
                stops: vec![stop("a", 1), stop("b", 2), stop("c", 3)],
                has_tremulant: false, has_swell: false,
            }],
            couplers: Some(vec![CouplerDto {
                id: "real_coupler_1".into(), name: "II/I".into(), source_division: "Hoofdwerk".into(),
                destination_division: "Hoofdwerk".into(), active: false, display_in_division: "Hoofdwerk".into(),
                midi_action_code: 0, coupler_type: "unison".into(), pitch_offset: 0,
            }]),
            retune_pipes: 0, retune_total: 0, perspectives: Vec::new(), layered_stops: 0,
        }
    }

    fn ids(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

    struct Rig {
        stages: Arc<RwLock<Vec<Vec<String>>>>,
        stage: Arc<RwLock<u8>>,
        claims: Arc<RwLock<Vec<String>>>,
        drawn: Arc<RwLock<Vec<String>>>,
        couplers: Arc<RwLock<Vec<String>>>,
        player: Arc<RwLock<Option<AudioPlayer>>>,
        organ: Arc<RwLock<Option<OrganInfoDto>>>,
        mappings: Arc<RwLock<Vec<MidiChannelMapping>>>,
        held: Arc<RwLock<Vec<(u8, u8, u8)>>>,
    }

    impl Rig {
        fn new(stages: Vec<Vec<String>>, drawn: Vec<String>) -> Self {
            Rig {
                stages: Arc::new(RwLock::new(stages)),
                stage: Arc::new(RwLock::new(0)),
                claims: Arc::new(RwLock::new(Vec::new())),
                drawn: Arc::new(RwLock::new(drawn)),
                couplers: Arc::new(RwLock::new(Vec::new())),
                player: Arc::new(RwLock::new(None)),
                organ: Arc::new(RwLock::new(Some(organ()))),
                mappings: Arc::new(RwLock::new(Vec::new())),
                held: Arc::new(RwLock::new(Vec::new())),
            }
        }
        fn apply(&self, s: u8) -> (Vec<String>, Vec<String>) {
            AppState::apply_crescendo_stage_inner(
                s, &self.stages, &self.stage, &self.claims, &self.drawn, &self.couplers,
                &self.player, &self.organ, &self.mappings, &self.held,
            )
        }
        fn sorted(v: &Arc<RwLock<Vec<String>>>) -> Vec<String> {
            let mut x = v.read().clone(); x.sort(); x
        }
    }

    #[test]
    fn trapformule_en_grenzen() {
        assert_eq!(crescendo_stage_for(0, 7), 0);
        assert_eq!(crescendo_stage_for(3, 7), 0);
        assert_eq!(crescendo_stage_for(4, 7), 1);
        assert_eq!(crescendo_stage_for(127, 7), 7);
        assert_eq!(crescendo_stage_for(64, 0), 0);
        assert_eq!(crescendo_stage_bounds(0, 7), (0, 3));
        assert_eq!(crescendo_stage_bounds(1, 7).0, 4);
        assert_eq!(crescendo_stage_bounds(7, 7).1, 127);
        // Trap 3 begint bij 40 (zie meetplan-model).
        assert_eq!(crescendo_stage_for(39, 7), 2);
        assert_eq!(crescendo_stage_for(40, 7), 3);
        assert_eq!(crescendo_stage_bounds(3, 7).0, 40);
        assert_eq!(crescendo_stage_bounds(2, 7).1, 39);
    }

    #[test]
    fn hysterese_klappert_niet_op_een_trapgrens() {
        let n = 7;
        // Omhoog pas H voorbij de bovengrens van trap 2 (39): 40 en 41 blijven staan.
        assert_eq!(crescendo_next_stage(40, 2, n), None);
        assert_eq!(crescendo_next_stage(41, 2, n), None);
        assert_eq!(crescendo_next_stage(42, 2, n), Some(3));
        // Omlaag pas H onder de ondergrens van trap 3 (40): 39 en 38 blijven staan.
        assert_eq!(crescendo_next_stage(39, 3, n), None);
        assert_eq!(crescendo_next_stage(38, 3, n), None);
        assert_eq!(crescendo_next_stage(37, 3, n), Some(2));
        // Ruis 39<->40 rond de grens: geen enkele wissel vanaf trap 2 of 3.
        for _ in 0..20 {
            assert_eq!(crescendo_next_stage(40, 2, n), None);
            assert_eq!(crescendo_next_stage(39, 2, n), None);
            assert_eq!(crescendo_next_stage(39, 3, n), None);
            assert_eq!(crescendo_next_stage(40, 3, n), None);
        }
        // Volle pedaalweg en dode zone gelden altijd; sprongen over trappen mogen.
        assert_eq!(crescendo_next_stage(127, 1, n), Some(7));
        assert_eq!(crescendo_next_stage(3, 5, n), Some(0));
        assert_eq!(crescendo_next_stage(0, 1, n), Some(0));
        assert_eq!(crescendo_next_stage(4, 0, n), None);
        assert_eq!(crescendo_next_stage(6, 0, n), Some(1));
        assert_eq!(crescendo_next_stage(64, crescendo_stage_for(64, n), n), None);
        // Gekrompen matrix: huidige trap > N -> direct naar de doeltrap.
        assert_eq!(crescendo_next_stage(64, 12, n), Some(crescendo_stage_for(64, n)));
        // Gespiegeld/bereik: 110 in bereik 20..110 = vol, 20 = 0.
        assert_eq!(crescendo_mapped_value(110, 20, 110, false), 127);
        assert_eq!(crescendo_mapped_value(20, 20, 110, false), 0);
        assert_eq!(crescendo_mapped_value(127, 110, 20, true), 0);
    }

    #[test]
    fn lege_trap_erft_lagere_gevulde_trap() {
        let stages = vec![ids(&["a"]), ids(&["a", "b"]), Vec::new(), Vec::new()];
        assert_eq!(crescendo_effective_stops(&stages, 0), Vec::<String>::new());
        assert_eq!(crescendo_effective_stops(&stages, 1), ids(&["a"]));
        assert_eq!(crescendo_effective_stops(&stages, 3), ids(&["a", "b"]));
        assert_eq!(crescendo_effective_stops(&stages, 4), ids(&["a", "b"]));
        assert_eq!(crescendo_effective_stops(&stages, 9), ids(&["a", "b"]));
    }

    #[test]
    fn apply_diff_claimt_alleen_wat_de_trede_bijtrekt() {
        let stages = vec![ids(&["a"]), ids(&["a", "b", "real_coupler_1", "onbekend"]), ids(&["a", "b", "c", "real_coupler_1"])];
        let rig = Rig::new(stages, ids(&["c"])); // c handmatig getrokken
        let (added, removed) = rig.apply(2);
        assert_eq!(*rig.stage.read(), 2);
        assert!(removed.is_empty());
        let mut a = added.clone(); a.sort();
        assert_eq!(a, ids(&["a", "b", "real_coupler_1"]));
        assert_eq!(Rig::sorted(&rig.drawn), ids(&["a", "b", "c"]));
        assert_eq!(Rig::sorted(&rig.couplers), ids(&["real_coupler_1"]), "echte ODF-koppel gaat via lidmaatschap aan");
        assert_eq!(Rig::sorted(&rig.claims), ids(&["a", "b", "real_coupler_1"]));
        assert!(!rig.claims.read().iter().any(|s| s == "onbekend"), "onbekende IDs worden genegeerd");
        // DTO-vlaggen volgen.
        let o = rig.organ.read().clone().unwrap();
        assert!(o.divisions[0].stops.iter().find(|s| s.id == "a").unwrap().drawn);
        assert!(o.couplers.as_ref().unwrap()[0].active);

        // Trap 3: c stond al handmatig -> niet geclaimd.
        rig.apply(3);
        assert_eq!(Rig::sorted(&rig.claims), ids(&["a", "b", "real_coupler_1"]));

        // Terug naar 0: handregistratie c blijft, trede-registers en koppel weg.
        let (_, removed) = rig.apply(0);
        assert_eq!(removed.len(), 3);
        assert_eq!(Rig::sorted(&rig.drawn), ids(&["c"]));
        assert!(rig.couplers.read().is_empty());
        assert!(rig.claims.read().is_empty());
        assert_eq!(*rig.stage.read(), 0);

        // Clamp: trap 9 op een matrix van 3 = trap 3.
        rig.apply(9);
        assert_eq!(*rig.stage.read(), 3);
    }

    #[test]
    fn handmatig_terugduwen_laat_claim_vallen() {
        let stages = vec![ids(&["a"]), ids(&["a", "b"])];
        let rig = Rig::new(stages, Vec::new());
        rig.apply(2);
        assert_eq!(Rig::sorted(&rig.claims), ids(&["a", "b"]));
        // Organist duwt a terug: claim weg (drawn zelf wordt door toggle_stop bijgewerkt).
        rig.drawn.write().retain(|s| s != "a");
        AppState::note_manual_change_inner(&rig.claims, "a");
        assert_eq!(Rig::sorted(&rig.claims), ids(&["b"]));
        // Bij opnieuw toepassen van trap 2 komt a als trede-register terug.
        let (added, _) = rig.apply(2);
        assert_eq!(added, ids(&["a"]));
        assert_eq!(Rig::sorted(&rig.claims), ids(&["a", "b"]));
        // Handmatig aanzetten van een geclaimd register -> geen claim meer, blijft staan bij trap 0.
        AppState::note_manual_change_inner(&rig.claims, "b");
        rig.apply(0);
        assert_eq!(Rig::sorted(&rig.drawn), ids(&["b"]));
    }

    #[test]
    fn zwelstand_uit_cc() {
        assert_eq!(swell_gain_from_cc(0, 0, 127, false), 0.0);
        assert_eq!(swell_gain_from_cc(127, 0, 127, false), 1.0);
        assert!((swell_gain_from_cc(40, 0, 127, false) - 40.0 / 127.0).abs() < 1e-6);
        assert_eq!(swell_gain_from_cc(0, 0, 127, true), 1.0);
        assert_eq!(swell_gain_from_cc(19, 20, 110, false), 0.0);
        assert_eq!(swell_gain_from_cc(111, 20, 110, false), 1.0);
        assert!((swell_gain_from_cc(65, 20, 110, false) - 0.5).abs() < 1e-6);
        // min > max wordt geordend (vangnet), geen stapfunctie.
        assert!((swell_gain_from_cc(65, 110, 20, false) - 0.5).abs() < 1e-6);
    }
}
