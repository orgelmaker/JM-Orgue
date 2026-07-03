//! Application state

use std::sync::Arc;
use std::path::PathBuf;
use std::thread;
use parking_lot::RwLock;
use crossbeam_channel::{Sender, Receiver, bounded};

use vpo_sampler::{OrganDefinition, LoadedOrgan};
use vpo_midi::{MidiInputManager, MidiMessage};

use crate::audio::{AudioPlayer, AudioCommand, AudioOutputConfig, resolve_host};
use crate::commands::OrganInfoDto;
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

/// Result of learning a keyboard range
#[derive(Debug, Clone)]
pub struct LearnedKeyboardRange {
    pub channel: u8,
    pub first_note: u8,
    pub last_note: u8,
    pub transpose: i8,
}

/// MIDI binding for a preset button
#[derive(Debug, Clone)]
pub struct MidiPresetBinding {
    /// Preset number (1-8)
    pub preset_num: u8,
    /// MIDI message type that triggers this preset
    pub trigger: MidiPresetTrigger,
}

/// Types of MIDI events that can trigger a preset
#[derive(Debug, Clone)]
pub enum MidiPresetTrigger {
    /// A specific note on any channel
    Note { note: u8 },
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
    /// Learn crescendo pedal CC binding
    LearnCrescendoPedal(Sender<Option<(u8, u8)>>), // returns (channel, cc_num)
    Shutdown,
}

/// Audio device info
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rates: Vec<u32>,
    pub channels: Vec<u16>,
}

/// Shared application state
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
    /// MIDI recording state
    pub midi_recording: Arc<RwLock<Option<MidiRecording>>>,
    /// Serialiseert orgel-loads en audio-wissels onderling: een wissel midden in
    /// een load gooit anders net-geregistreerde preload-buffers weg (nieuwe lege
    /// audio-thread) en mist de reload omdat current_organ_id nog niet gezet is;
    /// twee interleavende wissels kunnen elkaars player/prefs overschrijven.
    pub load_switch_gate: Arc<parking_lot::Mutex<()>>,
    /// Per-pipe voicing overrides: (stop_id, pipe_num) -> (volume_db, pitch_cents)
    pub pipe_voicings: Arc<RwLock<std::collections::HashMap<(u32, u32), (f32, f32)>>>,
    /// Per-divisie output-kanalen: lijst fysieke kanaalindices (leeg = standaard voorste paar 0/1).
    /// Opeenvolgende kanalen vormen (L,R)-paren; een los laatste kanaal krijgt mono.
    pub division_output_channels: Arc<RwLock<Vec<Vec<u8>>>>,
    /// C/Cis-lade spreiding aan/uit per divisie (even tonen ene kant, oneven andere).
    pub division_ccis_enabled: Arc<RwLock<Vec<bool>>>,
    /// Globale C/Cis-parameters: (sterkte 0..1, afval-met-toonhoogte 0..1, kanten omdraaien).
    pub ccis_spread: Arc<RwLock<(f32, f32, bool)>>,
    /// MP3-recorder (live opname van audio-output, vanuit het registerscherm).
    pub recorder: Arc<crate::recorder::Recorder>,
    /// Of de getrokken registratie ("laatste stand") bij het laden van een orgel
    /// hersteld wordt. Default false = schone start (geen registers aan bij opstart).
    pub restore_registration: Arc<RwLock<bool>>,
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

impl AppState {
    pub fn new(app_data_dir: PathBuf) -> Self {
        // Load organ library from disk
        let organ_library = Arc::new(RwLock::new(load_library(&app_data_dir)));
        // Load persisted audio output preferences and create the audio player
        let audio_prefs = load_audio_prefs(&app_data_dir);
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
                player_rx,
                recording_for_midi,
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
            midi_recording,
            load_switch_gate: Arc::new(parking_lot::Mutex::new(())),
            pipe_voicings: Arc::new(RwLock::new(std::collections::HashMap::new())),
            division_output_channels: Arc::new(RwLock::new(vec![Vec::new(); 32])),
            division_ccis_enabled: Arc::new(RwLock::new(vec![false; 32])),
            ccis_spread: Arc::new(RwLock::new((0.7, 0.6, false))),
            recorder: Arc::new(crate::recorder::Recorder::new()),
            restore_registration: Arc::new(RwLock::new(false)),
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
        player_rx: Receiver<MidiMessage>,
        midi_recording: Arc<RwLock<Option<MidiRecording>>>,
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
                    MidiCommand::LearnKeyboardRange(division, first_sample_note, response_tx) => {
                        // Learn keyboard range like the reference software:
                        // 1. Wait for first note (lowest key) - this gives us channel and start note
                        // 2. Wait for second note (highest key) - this gives us end note
                        // 3. Calculate transpose so lowest MIDI note maps to first sample note
                        tracing::info!("Learning keyboard range for {}: press lowest key, then highest key", division);

                        if let Some(ref rx) = midi_receiver {
                            // Wait for first note (lowest key)
                            let first_result = Self::wait_for_note_on(rx, std::time::Duration::from_secs(30));

                            if let Some((channel, first_note)) = first_result {
                                tracing::info!("Learned first note: channel={}, note={}", channel, first_note);

                                // Wait for second note (highest key) - same channel
                                let second_result = Self::wait_for_note_on(rx, std::time::Duration::from_secs(30));

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
                                    MidiMessage::NoteOn { note, .. } => {
                                        Some(MidiPresetTrigger::Note { note })
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
                            &ble_message_count,
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
                                &ble_message_count,
                            );

                            if let Some(max_val) = second {
                                let (actual_min, actual_max) = if min_val <= max_val {
                                    (min_val, max_val)
                                } else {
                                    (max_val, min_val)
                                };
                                let binding = SwellBinding {
                                    division_name: division_name.clone(),
                                    division_index,
                                    channel,
                                    cc_num,
                                    min_val: actual_min,
                                    max_val: actual_max,
                                    invert: false,
                                };
                                tracing::info!("Swell learned: {} → CC{} ch{} range {}-{}",
                                    division_name, cc_num, channel, actual_min, actual_max);
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
                            &ble_message_count,
                        );
                        if let Some((channel, cc_num, _val)) = first {
                            tracing::info!("Crescendo pedal learned: ch={}, cc={}", channel, cc_num);
                            let _ = response_tx.send(Some((channel, cc_num)));
                        } else {
                            tracing::info!("Crescendo learn timed out");
                            let _ = response_tx.send(None);
                        }
                    }
                    MidiCommand::Shutdown => break,
                }
            }

            // Process MIDI messages
            if let Some(ref rx) = midi_receiver {
                while let Ok(msg) = rx.try_recv() {
                    // Leg het ingespeelde event vast als er een MIDI-opname loopt
                    Self::capture_midi_event(&midi_recording, &msg);

                    // Check for preset triggers first
                    Self::check_preset_trigger(&msg, &preset_bindings, &preset_trigger_tx);

                    // Check crescendo CC
                    Self::process_crescendo_cc(
                        &msg, &crescendo_binding, &crescendo_enabled, &crescendo_stages,
                        &crescendo_stage, &crescendo_active_stops, &drawn_stops,
                        &audio_player, &loaded_organ_info,
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
                    );
                }
            }

            // Process BLE MIDI messages (from BleMidiManager)
            while let Ok(msg) = ble_message_rx.try_recv() {
                ble_message_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                tracing::debug!("BLE MIDI received: {:?}", msg);
                // Leg het ingespeelde event vast als er een MIDI-opname loopt
                Self::capture_midi_event(&midi_recording, &msg);
                // Forward CC messages to learn-tap channel (non-blocking; drop if full)
                if matches!(msg, MidiMessage::ControlChange { .. }) {
                    let _ = ble_learn_tx.try_send(msg.clone());
                }
                Self::check_preset_trigger(&msg, &preset_bindings, &preset_trigger_tx);
                Self::process_crescendo_cc(
                    &msg, &crescendo_binding, &crescendo_enabled, &crescendo_stages,
                    &crescendo_stage, &crescendo_active_stops, &drawn_stops,
                    &audio_player, &loaded_organ_info,
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
                );
            }

            // Process MIDI file player messages (afspelen .mid bestanden)
            // Bewust NIET vastleggen in de opname: dit is afspelen, geen live spel —
            // anders zou een opname tijdens playback de noten dubbel/terug-opnemen.
            while let Ok(msg) = player_rx.try_recv() {
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
                );
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

    /// Wait for a NoteOn message with velocity > 0
    /// Simple version: just detect NoteOn and add a small delay
    fn wait_for_note_on(rx: &Receiver<MidiMessage>, timeout: std::time::Duration) -> Option<(u8, u8)> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(MidiMessage::NoteOn { channel, note, velocity }) if velocity > 0 => {
                    tracing::info!("Got NoteOn: channel={}, note={}, velocity={}", channel, note, velocity);
                    // Wait 500ms to allow the user to release the key and prepare for the next
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    // Drain any pending messages (NoteOff etc)
                    while rx.try_recv().is_ok() {}
                    return Some((channel, note));
                }
                Ok(_) => {
                    // Ignore other messages (NoteOff, CC, etc)
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

    /// Process incoming MIDI CC for crescendo pedal: maps value → stage, applies stops additively.
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
    ) {
        // Extract CC fields
        let (channel, controller, value) = match msg {
            MidiMessage::ControlChange { channel, controller, value } => (*channel, *controller, *value),
            _ => return,
        };

        // Check binding match
        let binding = *crescendo_binding.read();
        let Some((bound_ch, bound_cc, min_val, max_val, invert)) = binding else { return; };
        if channel != bound_ch || controller != bound_cc { return; }

        // Check enabled
        if !*crescendo_enabled.read() { return; }

        // Get stages
        let stages = crescendo_stages.read();
        let num_stages = stages.len();
        if num_stages == 0 { return; }

        // Normaliseer de ruwe CC-waarde van het ingestelde [min,max]-bereik naar 0..127.
        let scaled = {
            let lo = min_val.min(max_val) as f32;
            let hi = max_val.max(min_val) as f32;
            let span = (hi - lo).max(1.0);
            (((value as f32 - lo) / span).clamp(0.0, 1.0) * 127.0) as u8
        };
        // Spiegelbeeld: keer de pedaalpositie om voordat we naar trap mappen.
        let mapped_value: u8 = if invert { 127u8.saturating_sub(scaled) } else { scaled };

        // Map CC value (0-127) to stage (0..=num_stages, where 0 = off)
        // Use a small dead-zone at the bottom (CC 0-3 = stage 0)
        let new_stage: u8 = if mapped_value < 4 {
            0
        } else {
            // Map 4..=127 linearly to 1..=num_stages
            let scaled = ((mapped_value as usize - 4) * num_stages) / 124 + 1;
            scaled.min(num_stages) as u8
        };

        let current_stage = *crescendo_stage.read();
        if new_stage == current_stage { return; }

        // Compute new active stops for this stage
        let new_active: Vec<String> = if new_stage == 0 {
            Vec::new()
        } else {
            stages.get((new_stage - 1) as usize).cloned().unwrap_or_default()
        };
        drop(stages);

        // Compute diff
        let old_active = crescendo_active_stops.read().clone();
        let to_remove: Vec<String> = old_active.iter()
            .filter(|s| !new_active.contains(s))
            .cloned()
            .collect();
        let to_add: Vec<String> = new_active.iter()
            .filter(|s| !old_active.contains(s))
            .cloned()
            .collect();

        // Update drawn_stops: remove stops we're deactivating, add new ones
        {
            let mut drawn = drawn_stops.write();
            for stop in &to_remove {
                drawn.retain(|s| s != stop);
            }
            for stop in &to_add {
                if !drawn.contains(stop) {
                    drawn.push(stop.clone());
                }
            }
        }

        // Update tracker
        *crescendo_active_stops.write() = new_active.clone();
        *crescendo_stage.write() = new_stage;

        // Update organ_info active state + send NoteOff for removed stops to release voices
        {
            let drawn_now = drawn_stops.read().clone();
            let mut organ = loaded_organ_info.write();
            if let Some(ref mut o) = *organ {
                for div in &mut o.divisions {
                    for stop in &mut div.stops {
                        stop.drawn = drawn_now.contains(&stop.id);
                    }
                }
                // Send NoteOff to released stops (release any sounding voices)
                if let Some(ref player) = *audio_player.read() {
                    for removed_id in &to_remove {
                        for div in &o.divisions {
                            if let Some(stop) = div.stops.iter().find(|s| &s.id == removed_id) {
                                let first = stop.first_midi_note as u32;
                                let last = stop.last_midi_note as u32;
                                for pipe_num in 1..=(last - first + 1) {
                                    let _ = player.send_command(AudioCommand::NoteOff {
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

        tracing::info!("Crescendo: CC={} → stage {} → {} ({} active stops)",
            value, current_stage, new_stage, new_active.len());
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
    ) -> Option<(u8, u8, u8)> {
        let start = std::time::Instant::now();
        let mut last_cc: Option<(u8, u8, u8)> = None;
        let mut last_change = std::time::Instant::now();
        let settle_time = std::time::Duration::from_millis(1500);

        while start.elapsed() < timeout {
            // Drain USB
            if let Some(rx) = usb_rx {
                while let Ok(msg) = rx.try_recv() {
                    if let MidiMessage::ControlChange { channel, controller, value } = msg {
                        last_cc = Some((channel, controller, value));
                        last_change = std::time::Instant::now();
                    }
                    Self::handle_midi_message(
                        msg, audio_player, organ_definition, loaded_organ_info, drawn_stops,
                        midi_mappings, active_couplers, swell_bindings, division_gains, held_notes,
                    );
                }
            }
            // Drain BLE
            while let Ok(msg) = ble_rx.try_recv() {
                ble_message_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let MidiMessage::ControlChange { channel, controller, value } = msg {
                    last_cc = Some((channel, controller, value));
                    last_change = std::time::Instant::now();
                }
                Self::handle_midi_message(
                    msg, audio_player, organ_definition, loaded_organ_info, drawn_stops,
                    midi_mappings, active_couplers, swell_bindings, division_gains, held_notes,
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
    ) -> Option<u8> {
        let start = std::time::Instant::now();
        let mut last_val: Option<u8> = None;
        let mut last_change = std::time::Instant::now();
        let settle_time = std::time::Duration::from_millis(1500);

        while start.elapsed() < timeout {
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
                (MidiPresetTrigger::Note { note: trigger_note }, MidiMessage::NoteOn { note, velocity, .. }) => {
                    *note == *trigger_note && *velocity > 0
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
                let _ = preset_trigger_tx.send(binding.preset_num);
            }
        }
    }

    /// Handle a single MIDI message
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
    ) {
        match msg {
            MidiMessage::NoteOn { channel, note, velocity } => {
                if velocity == 0 {
                    // Treat velocity 0 as NoteOff
                    Self::handle_midi_message(
                        MidiMessage::NoteOff { channel, note, velocity: 0 },
                        audio_player, organ_definition, loaded_organ_info, drawn_stops, midi_mappings, active_couplers,
                        swell_bindings, division_gains, held_notes,
                    );
                    return;
                }

                // Track held notes for re-triggering when stops are toggled on
                {
                    let mut held = held_notes.write();
                    held.retain(|&(ch, n, _)| !(ch == channel && n == note));
                    held.push((channel, note, velocity));
                }

                tracing::info!("MIDI NoteOn: ch={}, note={}, vel={}", channel, note, velocity);

                let organ = loaded_organ_info.read();
                let definition = organ_definition.read();
                let stops = drawn_stops.read();
                let mappings = midi_mappings.read();

                // Check if we have an organ loaded
                let Some(ref o) = *organ else {
                    tracing::warn!("No organ loaded");
                    return;
                };

                // Check if we have an audio player
                let player_guard = audio_player.read();
                let Some(ref player) = *player_guard else {
                    tracing::warn!("No audio player");
                    return;
                };

                // For custom sample sets without ODF, definition may be None
                // In that case, use default first_midi_note of 36 (C1)
                let has_definition = definition.is_some();

                for division in &o.divisions {
                    // Check if this division accepts this MIDI channel
                    let mapping = mappings.iter().find(|m| m.division == division.name);
                    let accepts_channel = match mapping {
                        Some(m) => m.channel.map_or(true, |ch| ch == channel),
                        None => true, // No mapping = accept all channels
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

                                tracing::info!("Playing stop={} '{}', note={}, pipe={}, range={}-{}",
                                    stop.internal_stop_id, stop.name, transposed_note, pipe_num,
                                    stop_first_midi, stop_last_midi);

                                let _ = player.send_command(AudioCommand::NoteOn {
                                    stop_id: stop.internal_stop_id,
                                    pipe_num,
                                    midi_note: transposed_note,
                                    velocity: vel_f,
                                });
                            }
                        }
                    }
                }

                // Coupler routing: play coupled division stops
                let couplers = active_couplers.read();
                if !couplers.is_empty() {
                    if let Some(ref coupler_list) = o.couplers {
                        // Collect active T.A. divisions
                        let ta_divisions: Vec<String> = coupler_list.iter()
                            .filter(|c| c.coupler_type == "ta" && couplers.contains(&c.id))
                            .map(|c| c.source_division.clone())
                            .collect();

                        for coupler in coupler_list {
                            if !couplers.contains(&coupler.id) { continue; }
                            // T.A. koppels are handled separately (they suppress reeds)
                            if coupler.coupler_type == "ta" { continue; }

                            // Melody coupler: only couple if this is the highest held note on the source division
                            if coupler.coupler_type == "melody" {
                                let held = held_notes.read();
                                let src_mapping_m = mappings.iter().find(|m| m.division == coupler.source_division);
                                let highest = held.iter()
                                    .filter(|&&(ch, _, _)| src_mapping_m.map_or(true, |m| m.channel.map_or(true, |mch| mch == ch)))
                                    .map(|&(_, n, _)| n)
                                    .max();
                                if highest != Some(note) { continue; }
                            }

                            // Bass coupler: only couple if this is the lowest held note on the source division
                            if coupler.coupler_type == "bass" {
                                let held = held_notes.read();
                                let src_mapping_b = mappings.iter().find(|m| m.division == coupler.source_division);
                                let lowest = held.iter()
                                    .filter(|&&(ch, _, _)| src_mapping_b.map_or(true, |m| m.channel.map_or(true, |mch| mch == ch)))
                                    .map(|&(_, n, _)| n)
                                    .min();
                                if lowest != Some(note) { continue; }
                            }

                            // Check if source division accepts this MIDI channel
                            let src_mapping = mappings.iter().find(|m| m.division == coupler.source_division);
                            let src_accepts = match src_mapping {
                                Some(m) => m.channel.map_or(true, |ch| ch == channel),
                                None => true,
                            };
                            if !src_accepts { continue; }
                            let src_transpose = src_mapping.map_or(0, |m| m.transpose);
                            // Apply source transpose + coupler pitch offset
                            let coupled_note = (note as i32 + src_transpose as i32 + coupler.pitch_offset).clamp(0, 127) as u8;
                            // Play through destination division's drawn stops
                            if let Some(dest_div) = o.divisions.iter().find(|d| d.name == coupler.destination_division) {
                                for stop in &dest_div.stops {
                                    if stops.contains(&stop.id) {
                                        let s_first = stop.first_midi_note as u32;
                                        let s_last = stop.last_midi_note as u32;
                                        let t = coupled_note as u32;
                                        if t >= s_first && t <= s_last {
                                            let pipe_num = t - s_first + 1;
                                            let vel_f = velocity as f32 / 127.0;
                                            let _ = player.send_command(AudioCommand::NoteOn {
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
                let player_guard = audio_player.read();
                let Some(ref player) = *player_guard else { return; };

                let _has_definition = definition.is_some();

                // NoteOff must be sent to ALL stops (not just drawn ones!)
                // A stop may have been un-drawn while a note was held,
                // so we must always send NoteOff to release any active voices.
                for division in &o.divisions {
                    let mapping = mappings.iter().find(|m| m.division == division.name);
                    let accepts_channel = match mapping {
                        Some(m) => m.channel.map_or(true, |ch| ch == channel),
                        None => true,
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

                            let _ = player.send_command(AudioCommand::NoteOff {
                                stop_id: stop.internal_stop_id,
                                pipe_num,
                            });
                        }
                    }
                }

                // Coupler routing for NoteOff - send to ALL stops (not just drawn)
                let couplers = active_couplers.read();
                if !couplers.is_empty() {
                    if let Some(ref coupler_list) = o.couplers {
                        for coupler in coupler_list {
                            if !couplers.contains(&coupler.id) { continue; }
                            if coupler.coupler_type == "ta" { continue; }
                            let src_mapping = mappings.iter().find(|m| m.division == coupler.source_division);
                            let src_accepts = match src_mapping {
                                Some(m) => m.channel.map_or(true, |ch| ch == channel),
                                None => true,
                            };
                            if !src_accepts { continue; }
                            let src_transpose = src_mapping.map_or(0, |m| m.transpose);
                            let coupled_note = (note as i32 + src_transpose as i32 + coupler.pitch_offset).clamp(0, 127) as u8;
                            if let Some(dest_div) = o.divisions.iter().find(|d| d.name == coupler.destination_division) {
                                for stop in &dest_div.stops {
                                    let s_first = stop.first_midi_note as u32;
                                    let s_last = stop.last_midi_note as u32;
                                    let t = coupled_note as u32;
                                    if t >= s_first && t <= s_last {
                                        let pipe_num = t - s_first + 1;
                                        let _ = player.send_command(AudioCommand::NoteOff {
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
                // Check swell pedal bindings first
                let bindings = swell_bindings.read();
                let mut handled = false;
                for binding in bindings.iter() {
                    if binding.channel == channel && binding.cc_num == controller {
                        let range = (binding.max_val as f32 - binding.min_val as f32).max(1.0);
                        let mut normalized = ((value as f32 - binding.min_val as f32) / range).clamp(0.0, 1.0);
                        if binding.invert { normalized = 1.0 - normalized; }
                        // Update AppState gains for frontend polling
                        {
                            let mut gains = division_gains.write();
                            if (binding.division_index as usize) < gains.len() {
                                gains[binding.division_index as usize] = normalized;
                            }
                        }
                        // Send to audio thread
                        if let Some(ref player) = *audio_player.read() {
                            let _ = player.send_command(AudioCommand::SetDivisionGain {
                                division_index: binding.division_index,
                                gain: normalized,
                            });
                        }
                        handled = true;
                    }
                }
                drop(bindings);

                // Fallback: CC#7/CC#11 → expressie-factor (only if not handled by swell).
                // Bewust NIET SetMasterGain: dat zou de master-volume-slider (en de
                // per-orgel-opslag daarvan) overschrijven. De expressie schaalt
                // multiplicatief op de slider: CC 127 = ×1.0 (sliderwaarde),
                // CC 1 ≈ -40 dB, CC 0 = stil.
                if !handled && (controller == 7 || controller == 11) {
                    if let Some(ref player) = *audio_player.read() {
                        let factor = if value == 0 {
                            0.0
                        } else {
                            let db = -40.0 * (1.0 - value as f32 / 127.0);
                            10.0_f32.powf(db / 20.0)
                        };
                        let _ = player.send_command(AudioCommand::SetMasterExpression(factor));
                    }
                }
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
    /// post-load path. `current_organ_id` is returned so the caller knows whether
    /// a reload is needed.
    pub fn switch_audio_output(&self, cfg: AudioOutputConfig) -> Result<Option<String>, String> {
        // Serialiseer t.o.v. orgel-loads en andere wissels (zie load_switch_gate).
        let _gate = self.load_switch_gate.lock();
        self.switch_audio_output_inner(cfg)
    }

    /// Voer de opgeslagen ASIO-voorkeur uitgesteld uit (aangeroepen door main.rs,
    /// ~10s na de start — een kóúde ASIO-start levert op ASIO4ALL een stille
    /// stream op). Herleest de prefs en checkt de actieve host BINNEN de gate,
    /// zodat een tussentijdse handmatige keuze van de gebruiker nooit wordt
    /// teruggedraaid en de wissel niet met een lopende orgel-load interleaved.
    pub fn apply_deferred_asio_pref(&self) -> Result<Option<String>, String> {
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
        })
    }

    fn switch_audio_output_inner(&self, cfg: AudioOutputConfig) -> Result<Option<String>, String> {
        // ASIO-drivers zijn single-client: zolang de oude player de driver vast-
        // houdt, ziet een tweede open geen apparaat ("No audio output device
        // found"). Bij een ASIO→ASIO-wissel (bv. andere buffergrootte) moet de
        // oude stream dus éérst dicht. Mislukt de nieuwe daarna, dan vallen we
        // terug op de default host zodat er altijd audio blijft.
        let new_is_asio = cfg.host_name.as_deref()
            .map(|h| h.eq_ignore_ascii_case("asio"))
            .unwrap_or(false);
        let old_is_asio = self.audio_player.read().as_ref()
            .map(|p| p.current_host.read().eq_ignore_ascii_case("asio"))
            .unwrap_or(false);

        if new_is_asio && old_is_asio {
            let old = { self.audio_player.write().take() };
            drop(old);
            let built = AudioPlayer::new(cfg.clone())
                .or_else(|e| {
                    tracing::warn!("ASIO→ASIO-wissel faalde ({}); terugvallen op default host", e);
                    AudioPlayer::new_default()
                })
                .or_else(|e| {
                    // De driver kan het apparaat met vertraging vrijgeven; één
                    // nieuwe poging na 500 ms dicht dat venster.
                    tracing::warn!("Fallback faalde ook ({}); nieuwe poging na 500 ms", e);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    AudioPlayer::new(cfg.clone()).or_else(|_| AudioPlayer::new_default())
                });
            match built {
                Ok(p) => *self.audio_player.write() = Some(p),
                // audio_player is nu leeg tot een volgende wissel slaagt — meld
                // dat expliciet i.p.v. stil geluidloos te blijven.
                Err(e) => return Err(format!(
                    "Audio-wissel mislukt en geen fallback beschikbaar; geluid is uit tot een nieuwe wissel slaagt: {}", e)),
            }
        } else {
            // 1) Build the new player BEFORE tearing down the old one, so a failure
            //    (e.g. ASIO device busy) leaves the existing audio intact. Voor
            //    WASAPI→ASIO is dit bovendien functioneel: ASIO4ALL initialiseert
            //    alleen betrouwbaar terwijl er al een stream op het apparaat draait
            //    (zelfde warm-up-effect als in AppState::new).
            let new_player = AudioPlayer::new(cfg.clone())?;

            // 2) Swap it in under a short write lock; drop the OLD player only AFTER
            //    releasing the guard so its Drop/Shutdown can't block other readers
            //    (parking_lot RwLock is not re-entrant).
            let old = {
                let mut guard = self.audio_player.write();
                guard.replace(new_player)
            };
            drop(old);
        }

        // 3) Persist the selection.
        save_audio_prefs(&self.app_data_dir, &AudioPrefs {
            host: cfg.host_name.clone(),
            device: cfg.device_name.clone(),
            buffer_frames: cfg.buffer_frames,
        });

        Ok(self.current_organ_id.read().clone())
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

    /// List MIDI devices
    pub fn list_midi_devices(&self) -> Vec<MidiDeviceInfo> {
        let (tx, rx) = bounded(1);
        if self.midi_tx.send(MidiCommand::ListDevices(tx)).is_ok() {
            rx.recv().unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Connect to a MIDI device
    pub fn connect_midi(&self, device_name: &str) -> Result<(), String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::Connect(device_name.to_string(), tx))
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
    }

    /// Connect to all MIDI devices
    pub fn connect_all_midi(&self) -> Result<usize, String> {
        let (tx, rx) = bounded(1);
        self.midi_tx.send(MidiCommand::ConnectAll(tx))
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?
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
        rx.recv().map_err(|e| e.to_string())
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
        rx.recv().map_err(|e| e.to_string())
    }

    /// Clear all MIDI bindings for a specific action/preset number
    pub fn clear_preset_bindings_for(&self, preset_num: u8) {
        let mut bindings = self.preset_bindings.write();
        bindings.retain(|b| b.preset_num != preset_num);
        let updated = bindings.clone();
        let _ = self.midi_tx.send(MidiCommand::SetPresetBindings(updated));
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

    /// Set MIDI channel mappings (for library restore)
    pub fn set_midi_mappings(&self, mappings: Vec<MidiChannelMapping>) {
        *self.midi_mappings.write() = mappings.clone();
        let _ = self.midi_tx.send(MidiCommand::SetChannelMapping(mappings));
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
