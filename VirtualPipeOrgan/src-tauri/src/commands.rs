//! Tauri commands - API between frontend and backend

use std::path::Path;
use tauri::State;
use serde::{Serialize, Deserialize};
use tracing::{info, warn};

use crate::audio::AudioCommand;
use crate::state::AppState;
use crate::library::{self, OrganLibraryEntry, OrganSettings, PresetBindingSaved, SwellBindingSaved, MidiMappingSaved, PresetData};
use vpo_sampler::{OrganDefinition, clean_stop_name, clean_division_name};

/// Audio device info for frontend
#[derive(Debug, Serialize)]
pub struct AudioDeviceDto {
    pub name: String,
    pub is_default: bool,
    pub sample_rates: Vec<u32>,
    pub channels: Vec<u16>,
}

/// MIDI device info for frontend
#[derive(Debug, Serialize)]
pub struct MidiDeviceDto {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
    /// True if device name suggests Bluetooth/BLE MIDI
    pub is_bluetooth: bool,
}

/// Detect Bluetooth/BLE MIDI devices by name patterns
fn is_bluetooth_midi(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("bluetooth") ||
    lower.contains("ble") ||
    lower.contains("bt-") ||
    lower.contains("bt midi") ||
    lower.contains("esp32") ||
    lower.contains("widi") ||
    lower.contains("midiair") ||
    lower.contains("cme") ||
    lower.contains("yamaha md-bt") ||
    lower.contains("mi.1")
}

/// MIDI channel mapping for frontend
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct MidiMappingDto {
    pub division: String,
    pub channel: Option<u8>,
    pub transpose: i8,
    pub first_midi_note: Option<u8>,
    pub last_midi_note: Option<u8>,
}

/// Coupler info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct CouplerDto {
    pub id: String,
    pub name: String,
    pub source_division: String,
    pub destination_division: String,
    pub active: bool,
    pub display_in_division: String,
    pub midi_action_code: u8,
    /// "unison", "super", "sub", "ta" (tongwerken af)
    pub coupler_type: String,
    /// Pitch offset in semitones: 0=unison, +12=super, -12=sub
    pub pitch_offset: i32,
}

/// Organ info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct OrganInfoDto {
    pub id: String,
    pub name: String,
    pub builder: String,
    pub location: String,
    pub year: Option<String>,
    pub stop_count: usize,
    pub divisions: Vec<DivisionDto>,
    pub couplers: Option<Vec<CouplerDto>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DivisionDto {
    pub name: String,
    pub display_name: String,
    pub stops: Vec<StopDto>,
    /// True when the source organ defines a tremulant for this division (ODF
    /// tremulant ref or trem sample folders). The UI uses it to make the
    /// LFO-tremulant available by default so the tremulant isn't "missing".
    #[serde(default)]
    pub has_tremulant: bool,
    /// True when this division sits in a swell box (GrandOrgue Enclosure). The UI
    /// enables the swell box (expression) for it by default.
    #[serde(default)]
    pub has_swell: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StopDto {
    pub id: String,
    pub name: String,
    pub pitch: String,
    pub drawn: bool,
    pub color: Option<String>,
    /// Whether this stop has tremulant samples available
    pub has_tremulant: bool,
    /// MIDI action code for this stop (for MIDI learn, 150+)
    pub midi_action_code: u8,
    /// Internal stop ID (numeric) for audio lookup
    pub internal_stop_id: u32,
    /// First MIDI note this stop responds to
    pub first_midi_note: u8,
    /// Last MIDI note this stop responds to
    pub last_midi_note: u8,
    /// Whether this is a reed (tongwerk) stop — for T.A. coupler
    #[serde(skip_serializing)]
    pub is_reed: bool,
}

/// Division volume info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct DivisionVolumeDto {
    pub division: String,
    pub volume: f32,
}

/// Swell binding info for frontend
#[derive(Debug, Clone, Serialize)]
pub struct SwellBindingDto {
    pub division: String,
    pub channel: u8,
    pub cc_num: u8,
    pub min_val: u8,
    pub max_val: u8,
    pub invert: bool,
}

/// Application status
#[derive(Debug, Serialize)]
pub struct StatusDto {
    pub audio_running: bool,
    pub midi_connected: bool,
    pub organ_loaded: bool,
    pub voice_count: usize,
    pub peak_left: f32,
    pub peak_right: f32,
    pub sample_rate: u32,
    /// Actual audio host/driver in use (e.g. "WASAPI", "ASIO")
    pub audio_host: String,
    /// Actual audio output device in use
    pub audio_device: String,
    /// Actual buffer size in frames (0 = driver default)
    pub buffer_frames: u32,
}

// ============ Commands ============

#[tauri::command]
pub fn get_audio_devices(state: State<AppState>) -> Result<Vec<AudioDeviceDto>, String> {
    let devices = state.list_audio_devices();
    Ok(devices.into_iter().map(|d| AudioDeviceDto {
        name: d.name,
        is_default: d.is_default,
        sample_rates: d.sample_rates,
        channels: d.channels,
    }).collect())
}

/// List available audio hosts/drivers (e.g. "WASAPI"; "ASIO" when built with the asio feature).
#[tauri::command]
pub fn list_audio_hosts(state: State<AppState>) -> Vec<String> {
    state.list_audio_hosts()
}

/// List output devices for a given host (None = default host).
#[tauri::command]
pub fn list_output_devices(state: State<AppState>, host: Option<String>) -> Result<Vec<AudioDeviceDto>, String> {
    let devices = state.list_audio_devices_for_host(host.as_deref());
    Ok(devices.into_iter().map(|d| AudioDeviceDto {
        name: d.name,
        is_default: d.is_default,
        sample_rates: d.sample_rates,
        channels: d.channels,
    }).collect())
}

/// Switch the audio output host/device/buffer and persist the choice.
/// Returns the current organ id (if any) so the frontend can reload it, which
/// re-registers samples on the new audio thread and re-applies DSP settings.
#[tauri::command]
pub fn set_audio_output(
    state: State<AppState>,
    host: Option<String>,
    device: Option<String>,
    buffer_frames: Option<u32>,
) -> Result<Option<String>, String> {
    state.switch_audio_output(crate::audio::AudioOutputConfig {
        host_name: host,
        device_name: device,
        buffer_frames,
    })
}

/// Scan a sample folder for leading silence in WAV/MP3 files (read-only report).
#[tauri::command]
pub async fn scan_sampleset_silence(directory: String, threshold_db: Option<f32>) -> Result<crate::silence::ScanReport, String> {
    let thr = threshold_db.unwrap_or(-60.0);
    tokio::task::spawn_blocking(move || crate::silence::scan(&directory, thr))
        .await
        .map_err(|e| format!("scan task failed: {}", e))
}

/// Trim leading silence from all WAV/MP3 files in a sample folder.
/// Originals are backed up to a sibling "<name>_backup" folder first.
#[tauri::command]
pub async fn trim_sampleset_silence(directory: String, threshold_db: Option<f32>, preroll_ms: Option<f32>) -> Result<crate::silence::TrimReport, String> {
    let thr = threshold_db.unwrap_or(-60.0);
    let pre = preroll_ms.unwrap_or(5.0);
    tokio::task::spawn_blocking(move || crate::silence::trim(&directory, thr, pre))
        .await
        .map_err(|e| format!("trim task failed: {}", e))
}

/// Scan a sample folder: which WAVs lack a loop and which can get one (read-only).
#[tauri::command]
pub async fn scan_sampleset_loops(directory: String) -> Result<crate::loop_tool::LoopScanReport, String> {
    tokio::task::spawn_blocking(move || crate::loop_tool::scan(&directory))
        .await
        .map_err(|e| format!("loop scan task failed: {}", e))
}

/// Detect a sustain loop in each WAV, bake a crossfade at the seam, and write the
/// loop into the WAV `smpl` chunk. Originals are backed up to "<name>_loopbackup".
/// `only_missing` (default true) skips files that already have a loop.
#[tauri::command]
pub async fn apply_sampleset_loops(directory: String, only_missing: Option<bool>) -> Result<crate::loop_tool::LoopApplyReport, String> {
    let only = only_missing.unwrap_or(true);
    tokio::task::spawn_blocking(move || crate::loop_tool::apply(&directory, only))
        .await
        .map_err(|e| format!("loop apply task failed: {}", e))
}

#[tauri::command]
pub fn get_midi_devices(state: State<AppState>) -> Result<Vec<MidiDeviceDto>, String> {
    let devices = state.list_midi_devices();
    info!("Found {} MIDI devices", devices.len());
    Ok(devices.into_iter().map(|d| {
        let bt = is_bluetooth_midi(&d.name);
        info!("  MIDI device: {} (input={}, output={}, bluetooth={})", d.name, d.is_input, d.is_output, bt);
        MidiDeviceDto {
            name: d.name,
            is_input: d.is_input,
            is_output: d.is_output,
            is_bluetooth: bt,
        }
    }).collect())
}

// === Bluetooth LE MIDI commands ===

/// Bluetooth LE MIDI device DTO for frontend
#[derive(Debug, Clone, Serialize)]
pub struct BleMidiDeviceDto {
    pub id: String,
    pub name: String,
    pub address: String,
    pub rssi: Option<i16>,
    pub connected: bool,
}

/// Initialize BLE MIDI manager (lazy)
fn ensure_ble_manager(state: &State<AppState>) -> Result<(), String> {
    let mut ble = state.ble_midi.write();
    if ble.is_none() {
        let manager = vpo_midi::BleMidiManager::new(state.ble_message_tx.clone())?;
        *ble = Some(manager);
        info!("BLE MIDI manager initialized");
    }
    Ok(())
}

/// Scan for BLE MIDI devices for ~5 seconds
/// Async to avoid blocking the UI during the scan duration.
#[tauri::command]
pub async fn scan_ble_midi(state: State<'_, AppState>) -> Result<Vec<BleMidiDeviceDto>, String> {
    ensure_ble_manager(&state)?;
    // Clone manager handle so we can move into spawn_blocking
    let ble_handle = state.ble_midi.clone();
    let result = tokio::task::spawn_blocking(move || {
        let ble = ble_handle.read();
        let manager = ble.as_ref().ok_or("BLE manager not initialized")?;
        manager.scan(5000)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    Ok(result.into_iter().map(|d| BleMidiDeviceDto {
        id: d.id, name: d.name, address: d.address, rssi: d.rssi, connected: d.connected,
    }).collect())
}

/// Connect to a BLE MIDI device by ID
#[tauri::command]
pub async fn connect_ble_midi(state: State<'_, AppState>, device_id: String) -> Result<String, String> {
    ensure_ble_manager(&state)?;
    let ble_handle = state.ble_midi.clone();
    tokio::task::spawn_blocking(move || {
        let ble = ble_handle.read();
        let manager = ble.as_ref().ok_or("BLE manager not initialized")?;
        manager.connect(&device_id)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Disconnect a BLE MIDI device
#[tauri::command]
pub async fn disconnect_ble_midi(state: State<'_, AppState>, device_id: String) -> Result<(), String> {
    let ble_handle = state.ble_midi.clone();
    tokio::task::spawn_blocking(move || {
        let ble = ble_handle.read();
        if let Some(manager) = ble.as_ref() {
            manager.disconnect(&device_id)?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

/// Get list of currently connected BLE MIDI devices
#[tauri::command]
pub fn get_connected_ble_midi(state: State<AppState>) -> Vec<(String, String)> {
    let ble = state.ble_midi.read();
    ble.as_ref().map(|m| m.connected_devices()).unwrap_or_default()
}

/// Get the BLE MIDI message counter (for activity indicator)
#[tauri::command]
pub fn get_ble_midi_count(state: State<AppState>) -> u64 {
    state.ble_message_count.load(std::sync::atomic::Ordering::Relaxed)
}

/// Refresh MIDI device list and reconnect any newly available devices
/// Useful after pairing a Bluetooth MIDI device while the app is running
#[tauri::command]
pub fn refresh_midi_devices(state: State<AppState>) -> Result<usize, String> {
    info!("Refreshing MIDI device list...");
    // Reconnect to all available devices (this will pick up newly paired BLE devices)
    let count = state.connect_all_midi()?;
    info!("Refreshed: {} MIDI devices connected", count);
    Ok(count)
}

#[tauri::command]
pub fn start_audio(_state: State<AppState>, device_name: Option<String>) -> Result<(), String> {
    if let Some(name) = device_name {
        info!("Starting audio with device: {}", name);
    }
    // Audio is already running (started in AppState::new())
    info!("Audio already running");
    Ok(())
}

#[tauri::command]
pub fn stop_audio(state: State<AppState>) -> Result<(), String> {
    state.send_audio_command(AudioCommand::AllNotesOff);
    info!("Audio stopped (all notes off)");
    Ok(())
}

#[tauri::command]
pub fn connect_midi(state: State<AppState>, device_name: String) -> Result<(), String> {
    info!("Connecting to MIDI device: {}", device_name);
    state.connect_midi(&device_name)?;
    info!("Connected to MIDI device: {}", device_name);
    Ok(())
}

#[tauri::command]
pub fn connect_all_midi(state: State<AppState>) -> Result<usize, String> {
    info!("Connecting to all MIDI devices...");
    let count = state.connect_all_midi()?;
    info!("Connected to {} MIDI devices", count);
    Ok(count)
}

#[tauri::command]
pub fn disconnect_midi(state: State<AppState>) -> Result<(), String> {
    info!("Disconnecting MIDI devices");
    state.disconnect_midi();
    Ok(())
}

#[tauri::command]
pub fn set_midi_mapping(state: State<AppState>, division: String, channel: Option<u8>, transpose: i8) -> Result<(), String> {
    info!("Setting MIDI mapping for {}: channel={:?}, transpose={}", division, channel, transpose);
    state.set_midi_mapping(&division, channel, transpose);
    Ok(())
}

#[tauri::command]
pub fn get_midi_mappings(state: State<AppState>) -> Vec<MidiMappingDto> {
    state.get_midi_mappings().into_iter().map(|m| MidiMappingDto {
        division: m.division,
        channel: m.channel,
        transpose: m.transpose,
        first_midi_note: m.first_midi_note,
        last_midi_note: m.last_midi_note,
    }).collect()
}

#[tauri::command]
pub fn learn_midi_channel(state: State<AppState>, division: String) -> Result<Option<u8>, String> {
    info!("Learning MIDI channel for division: {}", division);
    state.learn_midi_channel(&division)
}

/// Result of learning a keyboard range
#[derive(Debug, Clone, Serialize)]
pub struct LearnedKeyboardRangeDto {
    pub channel: u8,
    pub first_note: u8,
    pub last_note: u8,
    pub transpose: i8,
}

/// Learn keyboard range for a division (two-note)
/// Press lowest key, then highest key
#[tauri::command]
pub fn learn_keyboard_range(state: State<AppState>, division: String, first_sample_note: u8) -> Result<Option<LearnedKeyboardRangeDto>, String> {
    info!("Learning keyboard range for division: {} (first_sample_note={})", division, first_sample_note);
    info!("Press the LOWEST key on your keyboard, then the HIGHEST key");

    let result = state.learn_keyboard_range(&division, first_sample_note)?;

    Ok(result.map(|r| LearnedKeyboardRangeDto {
        channel: r.channel,
        first_note: r.first_note,
        last_note: r.last_note,
        transpose: r.transpose,
    }))
}

/// Preset binding info for frontend
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct PresetBindingDto {
    pub preset_num: u8,
    pub trigger_type: String,  // "note", "cc", "program"
    pub trigger_value: String, // human readable description
}

#[tauri::command]
pub fn get_preset_bindings(state: State<AppState>) -> Vec<PresetBindingDto> {
    use crate::state::MidiPresetTrigger;

    state.get_preset_bindings().into_iter().map(|b| {
        let (trigger_type, trigger_value) = match b.trigger {
            MidiPresetTrigger::Note { note } => {
                ("note".to_string(), format!("Note {}", note))
            }
            MidiPresetTrigger::ControlChange { channel, controller, value } => {
                let ch = channel.map_or("Any".to_string(), |c| format!("{}", c + 1));
                ("cc".to_string(), format!("CC{} >= {} (ch {})", controller, value, ch))
            }
            MidiPresetTrigger::ProgramChange { channel, program } => {
                let ch = channel.map_or("Any".to_string(), |c| format!("{}", c + 1));
                ("program".to_string(), format!("PC{} (ch {})", program, ch))
            }
        };
        PresetBindingDto {
            preset_num: b.preset_num,
            trigger_type,
            trigger_value,
        }
    }).collect()
}

#[tauri::command]
pub fn learn_preset_binding(state: State<AppState>, preset_num: u8) -> Result<Option<String>, String> {
    info!("Learning MIDI binding for preset: {}", preset_num);
    let result = state.learn_preset_binding(preset_num)?;
    Ok(result.map(|_| "Learned".to_string()))
}

#[tauri::command]
pub fn clear_preset_binding(state: State<AppState>, preset_num: u8) {
    info!("Clearing all MIDI bindings for action: {}", preset_num);
    state.clear_preset_bindings_for(preset_num);
}

#[tauri::command]
pub fn poll_preset_trigger(state: State<AppState>) -> Option<u8> {
    state.poll_preset_trigger()
}

#[tauri::command]
pub fn load_organ(state: State<AppState>, path: String) -> Result<OrganInfoDto, String> {
    do_load_organ(&state, &path)
}

/// Herbruikbare load-organ logic die zowel vanuit Tauri commands als vanuit de test API
/// kan worden aangeroepen. Neemt een `&AppState` (niet `State<AppState>`).
/// GrandOrgue AmplitudeLevel (procent; 100 = neutraal, 0-1000) → dB.
/// 0 of afwezig behandelen we als "niet gezet" → neutraal (0 dB).
fn ampl_to_db(ampl: f32) -> f32 {
    if ampl > 0.0 && (ampl - 100.0).abs() > f32::EPSILON {
        20.0 * (ampl / 100.0).log10()
    } else {
        0.0
    }
}

/// Is dit orgel-id een orgel-definitiebestand (GrandOrgue .organ of Hauptwerk
/// .Organ_Hauptwerk_xml) — en dus voor do_load_organ — of een sample-map (voor
/// do_load_samples_from_directory)? Gebruikt door alle herlaad-routes.
pub fn is_organ_file_id(id: &str) -> bool {
    let l = id.to_lowercase();
    l.ends_with(".organ") || l.ends_with(".organ_hauptwerk_xml")
}

pub fn do_load_organ(state: &AppState, path: &str) -> Result<OrganInfoDto, String> {
    // Serialiseer t.o.v. audio-wissels en andere loads (zie load_switch_gate):
    // een wissel midden in een load zou de net-geregistreerde preload-buffers
    // weggooien en het herladen missen (current_organ_id nog niet gezet).
    let _gate = state.load_switch_gate.lock();
    info!("Loading organ from: {}", path);

    let odf_path = Path::new(path);
    // Hauptwerk sets (*.Organ_Hauptwerk_xml) use a different format; route them to
    // the Hauptwerk importer, which translates to the same OrganDefinition so the
    // streaming-load + DTO + noise-filter below are shared with GrandOrgue.
    let mut definition = if vpo_sampler::is_hauptwerk_path(odf_path) {
        info!("Detected Hauptwerk organ definition");
        vpo_sampler::load_hauptwerk(odf_path)
            .map_err(|e| format!("Failed to parse Hauptwerk ODF: {}", e))?
    } else {
        OrganDefinition::load(odf_path)
            .map_err(|e| format!("Failed to parse ODF: {}", e))?
    };

    // Tidy division/manual names (strip "N. " ordinals like "2. Hauptwerk"). The
    // name is the routing key for divisions AND couplers, so cleaning it here keeps
    // everything downstream consistent. (Hauptwerk names are already clean → no-op.)
    for manual in &mut definition.manuals {
        manual.name = clean_division_name(&manual.name);
    }

    info!("Parsed organ: {} by {}", definition.organ.church_name, definition.organ.organ_builder);

    // Divisions sitting in a swell box: een divisie is 'enclosed' wanneer de
    // meerderheid van haar échte registers op een windchest staat die een
    // Enclosure refereert. Structureel (geen naam-match): fixt sets waar de
    // windchest-naam afwijkt van de divisienaam (bv. Bureå "Enclosed Manual
    // wind-chest") en dekt ook de Hauptwerk-import (enclosure-per-windchest).
    let enclosed_divs: std::collections::HashSet<String> = definition.manuals.iter()
        .filter(|m| {
            let (enclosed, total) = m.stop_ids.iter()
                .filter_map(|sid| definition.stops.iter().find(|s| s.id == *sid))
                .filter(|s| !s.is_noise_or_mechanical())
                .fold((0usize, 0usize), |(e, t), s| {
                    let enc = definition.windchests.iter()
                        .any(|w| w.number == s.windchest_group && !w.enclosure_ids.is_empty());
                    (e + enc as usize, t + 1)
                });
            total > 0 && enclosed * 2 >= total
        })
        .map(|m| m.name.clone())
        .collect();
    if !enclosed_divs.is_empty() {
        info!("Swell-enclosed divisions: {:?}", enclosed_divs);
    }

    // Load samples via het preload + achtergrond-stream-mechanisme (zelfde route
    // als de custom sample-mappen, do_load_samples_from_directory). We laden NIET
    // eager elke sample volledig in RAM — dat liet grote GrandOrgue-sets vastlopen
    // (honderden lange samples ineens → geheugendruk/swapping → app onresponsief).
    // In plaats daarvan: alleen de attack (~PRELOAD_SAMPLES frames) per uniek
    // bestand inlezen voor directe respons; de audio-thread streamt de volledige
    // sample op de achtergrond zodra een noot gespeeld wordt.
    {
        use std::collections::{HashMap, HashSet};
        use std::path::PathBuf;
        use std::sync::Arc;
        use rayon::prelude::*;
        use vpo_sampler::{load_audio_preload, PipeDef, PreloadBuffer};
        use crate::audio::PRELOAD_SAMPLES;

        // (stop_id, pipe_num) → concreet samplepad. Referenties (REF:...) worden
        // hier al opgelost zodat de achtergrond-loader rechtstreeks van het bestand
        // kan streamen. pipe_num = pijp-index+1 (zoals de NoteOn-route verwacht:
        // midi_note - first_midi_note + 1).
        let mut load_tasks: Vec<((u32, u32), PathBuf)> = Vec::new();
        // Percussieve stops (one-shot: klok e.d.) en ODF-looppunten per bestand
        // (fallback voor WAV's zonder smpl-chunk) — verzameld tijdens de loop.
        let mut percussive_stops: HashSet<u32> = HashSet::new();
        let mut odf_loops: HashMap<PathBuf, (u32, u32)> = HashMap::new();
        // ODF-intonatie per pijp: de GrandOrgue-hiërarchie Organ × Stop × Pipe
        // (AmplitudeLevel% en Gain-dB stapelen als dB-som; alle pitch-velden in
        // cents opgeteld). Gaat als aparte laag naar de engine, gescheiden van
        // de gebruikers-voicing.
        let mut odf_voicings: HashMap<(u32, u32), (f32, f32)> = HashMap::new();
        // Hauptwerk-tremulantlaag: aparte trem-opname per pijp.
        let mut trem_tasks: Vec<((u32, u32), PathBuf)> = Vec::new();
        let organ_db = ampl_to_db(definition.organ.amplitude_level) + definition.organ.gain_db;
        for stop in &definition.stops {
            // Sla mechaniek-/ruisopnamen over (klep-/registergeluid, blaasbalg,
            // ambient) — die horen geen speelbaar register te zijn.
            if stop.is_noise_or_mechanical() {
                continue;
            }
            if stop.percussive {
                percussive_stops.insert(stop.id);
            }
            let stop_db = ampl_to_db(stop.amplitude_level) + stop.gain_db;
            for (pipe_idx, pipe) in stop.pipes.iter().enumerate() {
                let pipe_num = (pipe_idx + 1) as u32;
                if let Some(resolved) = definition.resolve_reference(pipe) {
                    if let PipeDef::Sample { path, extra } = resolved {
                        if extra.percussive == Some(true) {
                            percussive_stops.insert(stop.id);
                        }
                        if let (Some(ls), Some(le)) = (extra.loop_start, extra.loop_end) {
                            if le > ls {
                                odf_loops.entry(path.clone()).or_insert((ls, le));
                            }
                        }
                        let pipe_db = extra.amplitude_level.map(ampl_to_db).unwrap_or(0.0)
                            + extra.gain_db.unwrap_or(0.0);
                        let total_db = organ_db + stop_db + pipe_db;
                        let total_cents = stop.pitch_correction
                            + extra.pitch_tuning_cents.unwrap_or(0.0)
                            + extra.pitch_correction_cents.unwrap_or(0.0);
                        if total_db.abs() > 0.01 || total_cents.abs() > 0.01 {
                            odf_voicings.insert((stop.id, pipe_num), (total_db, total_cents));
                        }
                        if let Some(trem_path) = &extra.tremulant_sample {
                            trem_tasks.push(((stop.id, pipe_num), trem_path.clone()));
                        }
                        load_tasks.push(((stop.id, pipe_num), path.clone()));
                    }
                }
            }
        }

        // GrandOrgue-stops delen vaak één rank: meerdere (stop,pijp)-keys wijzen
        // dan naar hetzelfde bestand. Laad de attack van elk UNIEK bestand maar
        // één keer en deel de Arc<PreloadBuffer> over alle keys (minder disk-I/O
        // en minder geheugen).
        let mut seen = HashSet::new();
        let unique_paths: Vec<PathBuf> = load_tasks
            .iter()
            .chain(trem_tasks.iter())
            .filter(|(_, p)| seen.insert(p.clone()))
            .map(|(_, p)| p.clone())
            .collect();

        info!(
            "GrandOrgue: {} pijp-keys, {} unieke samplebestanden — attack-preloads laden...",
            load_tasks.len(),
            unique_paths.len()
        );
        let start_time = std::time::Instant::now();
        let mut path_buffers: HashMap<PathBuf, Arc<PreloadBuffer>> = unique_paths
            .par_iter()
            .filter_map(|p| match load_audio_preload(p, PRELOAD_SAMPLES) {
                Ok(buf) => Some((p.clone(), Arc::new(buf))),
                Err(e) => {
                    warn!("Failed to preload {:?}: {}", p, e);
                    None
                }
            })
            .collect();

        // ODF-looppunten als fallback voor WAV's zonder smpl-chunk. Alleen de
        // preload-fase: de volledige achtergrond-load kent alleen smpl-punten
        // (ODF-loops zouden daar mee-geresampled moeten worden — zeldzaam geval).
        // De Arc's zijn hier nog uniek (net gebouwd), dus get_mut werkt.
        let mut odf_loops_applied = 0usize;
        for (path, (ls, le)) in &odf_loops {
            if let Some(buf) = path_buffers.get_mut(path) {
                if buf.loop_start.is_none() {
                    if let Some(b) = Arc::get_mut(buf) {
                        b.loop_start = Some(*ls as u64);
                        b.loop_end = Some(*le as u64);
                        odf_loops_applied += 1;
                    }
                }
            }
        }
        if odf_loops_applied > 0 {
            info!("ODF-looppunten toegepast op {} bestanden zonder smpl-chunk", odf_loops_applied);
        }
        info!(
            "Loaded {} unieke preload-buffers in {:.2}s",
            path_buffers.len(),
            start_time.elapsed().as_secs_f32()
        );

        // Fan-out naar per-key buffers (gedeelde Arc).
        let preload_buffers: HashMap<(u32, u32), Arc<PreloadBuffer>> = load_tasks
            .iter()
            .filter_map(|(key, p)| path_buffers.get(p).map(|buf| (*key, buf.clone())))
            .collect();

        // Hauptwerk-tremulantlaag: fan-out van de trem-samples naar per-key buffers.
        let trem_buffers: HashMap<(u32, u32), Arc<PreloadBuffer>> = trem_tasks
            .iter()
            .filter_map(|(key, p)| path_buffers.get(p).map(|buf| (*key, buf.clone())))
            .collect();

        info!("Registering {} preload buffers for instant playback", preload_buffers.len());
        {
            let player_lock = state.audio_player.read();
            if let Some(ref player) = *player_lock {
                player
                    .send_command(crate::audio::AudioCommand::RegisterPreloadBuffers(Arc::new(preload_buffers)))
                    .map_err(|e| format!("Failed to register preload buffers: {}", e))?;
                // Altijd sturen (ook leeg): vervangt de sets van het vorige orgel.
                let _ = player.send_command(crate::audio::AudioCommand::RegisterPercussiveStops(percussive_stops));
                if !trem_buffers.is_empty() {
                    info!("Tremulant-samplelaag: {} pijpen (echte trem-opnamen)", trem_buffers.len());
                }
                let _ = player.send_command(crate::audio::AudioCommand::RegisterTremulantBuffers(Arc::new(trem_buffers)));
                if !odf_voicings.is_empty() {
                    info!("ODF-intonatie: {} pijpen met gain/pitch uit de sampleset", odf_voicings.len());
                }
                let _ = player.send_command(crate::audio::AudioCommand::RegisterOdfVoicings(Arc::new(odf_voicings)));
            }
        }
    }

    // Build organ info for UI
    let mut divisions: Vec<DivisionDto> = Vec::new();
    let mut stop_action_code: u8 = 150; // Stops use action codes 150-255
    let mut skipped_noise = 0usize;

    for manual in &definition.manuals {
        let mut stops: Vec<StopDto> = Vec::new();

        for stop_id in &manual.stop_ids {
            if let Some(stop) = definition.stops.iter().find(|s| s.id == *stop_id) {
                // Mechaniek/ruis (klep-/registergeluid, blaasbalg, ambient) is geen
                // speelbaar register — niet als registerknop tonen.
                if stop.is_noise_or_mechanical() {
                    skipped_noise += 1;
                    continue;
                }
                let pitch = OrganDefinition::harmonic_to_footage(stop.harmonic_number);
                let color = get_stop_color(&stop.name);

                // MIDI range. Hauptwerk sets the stop's own first note directly.
                // For GrandOrgue, a stop can start partway up the keyboard (bass/
                // discant-split stops): its first pipe sounds at the manual's first
                // key + (FirstAccessiblePipeLogicalKeyNumber - 1). Honoring that puts
                // a discant half at e.g. MIDI 60 instead of wrongly at the manual's 36.
                let first_midi = stop.first_midi_note
                    .map(|n| n as u8)
                    .unwrap_or_else(|| (manual.first_accessible_key_midi_note
                        + stop.first_accessible_pipe_logical_key.saturating_sub(1)) as u8);
                let last_midi = first_midi.saturating_add(stop.number_of_pipes.saturating_sub(1) as u8);

                stops.push(StopDto {
                    id: format!("{}_{}", manual.number, stop.id),
                    name: clean_stop_name(&stop.name),
                    pitch,
                    drawn: false,
                    color: Some(color.to_string()),
                    has_tremulant: false,
                    midi_action_code: stop_action_code,
                    internal_stop_id: stop.id,
                    first_midi_note: first_midi,
                    last_midi_note: last_midi,
                    is_reed: is_reed_stop(&stop.name),
                });
                stop_action_code = stop_action_code.saturating_add(1);
            }
        }

        let display_name = if manual.number == 0 {
            "Pedaal".to_string()
        } else {
            format!("{} ({})", manual.name, roman_numeral(manual.number as usize))
        };

        divisions.push(DivisionDto {
            name: manual.name.clone(),
            display_name,
            stops,
            // ODF defines a tremulant for this manual (or its windchest).
            has_tremulant: !manual.tremulant_ids.is_empty(),
            // Division sits in a swell box (its windchest references an Enclosure).
            has_swell: enclosed_divs.contains(&manual.name),
        });
    }

    let stop_count: usize = divisions.iter().map(|d| d.stops.len()).sum();
    if skipped_noise > 0 {
        info!("Skipped {} mechaniek/ruis-'stops' (klep/register/blaasbalg/ambient) — {} echte registers", skipped_noise, stop_count);
    }
    // Prefer the organ's real couplers (from the ODF/Hauptwerk file); only fall
    // back to generic ones when the source defines none.
    let real_couplers = build_couplers_from_definition(&definition);
    let couplers = if real_couplers.is_empty() {
        generate_couplers(&divisions)
    } else {
        info!("Using {} real couplers from the organ definition", real_couplers.len());
        real_couplers
    };

    let organ_info = OrganInfoDto {
        id: path.to_string(),
        name: definition.organ.church_name.clone(),
        builder: definition.organ.organ_builder.clone(),
        location: definition.organ.recording_details.clone(),
        year: if definition.organ.organ_build_date.is_empty() {
            None
        } else {
            Some(definition.organ.organ_build_date.clone())
        },
        stop_count,
        divisions,
        couplers: Some(couplers),
    };

    // Reset bij orgelwissel: stop alle klinkende noten en wis de getrokken registratie +
    // crescendo-stops + koppels, zodat geen geluid of registratie van het vórige orgel
    // achterblijft (anders het symptoom "geluid is actief maar de knop niet"). De
    // "laatste stand" van dít orgel wordt hierna via restore_organ_settings hersteld.
    state.send_audio_command(AudioCommand::AllNotesOff);
    state.drawn_stops.write().clear();
    *state.crescendo_active_stops.write() = Vec::new();
    state.set_active_couplers(Vec::new());

    // Build stop→division map and register with audio thread
    let stop_div_map = build_stop_division_map(&organ_info.divisions);
    state.send_audio_command(AudioCommand::RegisterStopDivisionMap(stop_div_map));
    state.reset_division_gains(organ_info.divisions.len());
    state.reset_division_settings(organ_info.divisions.len());

    // ODF-tremulantparameters als default voor de per-divisie LFO: rate uit
    // Period (ms), diepte uit AmpModDepth (%). Eventueel per orgel opgeslagen
    // gebruikersinstellingen overschrijven dit daarna via de normale restore.
    for (div_idx, manual) in definition.manuals.iter().enumerate() {
        if let Some(tid) = manual.tremulant_ids.first() {
            if let Some(trem) = definition.tremulants.iter().find(|t| t.number == *tid) {
                if trem.period > 1.0 {
                    let rate = (1000.0 / trem.period).clamp(0.5, 12.0);
                    let amp_depth = if trem.amp_mod_depth > 0.0 {
                        trem.amp_mod_depth.clamp(1.0, 50.0)
                    } else {
                        10.0
                    };
                    info!("ODF-tremulant '{}': {:.1} Hz, amp-diepte {:.0}%", manual.name, rate, amp_depth);
                    state.send_audio_command(AudioCommand::SetTremulantLFO {
                        division_index: div_idx as u8,
                        active: false,
                        rate,
                        amp_depth,
                        pitch_depth: 15.0,
                    });
                }
            }
        }
    }

    {
        let mut current = state.loaded_organ_info.write();
        *current = Some(organ_info.clone());
    }

    {
        let mut def = state.organ_definition.write();
        *def = Some(definition);
    }

    {
        // We houden geen volledig in-RAM LoadedOrgan meer aan (samples streamen nu
        // via preload-buffers + achtergrond-load). Wis een eventueel eerder geladen
        // orgel zodat zijn samples vrijkomen.
        let mut loaded = state.loaded_organ.write();
        *loaded = None;
    }

    info!("Organ loaded: {} stops in {} divisions", stop_count, organ_info.divisions.len());

    // Add to library if not already present
    add_to_library_if_new(state, &organ_info, path, "organ_file");

    // Set current organ ID and restore settings
    {
        let mut id = state.current_organ_id.write();
        *id = Some(path.to_string());
    }
    restore_organ_settings(state, path);

    Ok(organ_info)
}

#[tauri::command]
pub fn get_organ_info(state: State<AppState>) -> Result<Option<OrganInfoDto>, String> {
    let organ = state.loaded_organ_info.read();
    match organ.clone() {
        Some(mut o) => {
            // Fill coupler active states from backend
            let active = state.get_active_couplers();
            if let Some(ref mut couplers) = o.couplers {
                for c in couplers.iter_mut() {
                    c.active = active.contains(&c.id);
                }
            }
            // Fill stop drawn states from the audio-authoritative drawn set, so the
            // UI's engaged display always matches what is actually sounding. (The
            // cached DTO can otherwise drift out of sync — e.g. its drawn flags are
            // rebuilt to false on a reload while the drawn set / audio keep playing.)
            {
                let drawn = state.drawn_stops.read();
                for div in o.divisions.iter_mut() {
                    for s in div.stops.iter_mut() {
                        s.drawn = drawn.contains(&s.id);
                    }
                }
            }
            Ok(Some(o))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn set_stop(state: State<AppState>, stop_id: String, active: bool) -> Result<(), String> {
    let mut organ = state.loaded_organ_info.write();

    if let Some(ref mut o) = *organ {
        for division in &mut o.divisions {
            if let Some(stop) = division.stops.iter_mut().find(|s| s.id == stop_id) {
                stop.drawn = active;
                state.set_stop_drawn(&stop_id, active);
                info!("Stop {} set to {}", stop_id, active);
                return Ok(());
            }
        }
    }

    Err(format!("Stop not found: {}", stop_id))
}

#[tauri::command]
pub fn toggle_stop(state: State<AppState>, stop_id: String) -> Result<bool, String> {
    let mut organ = state.loaded_organ_info.write();

    if let Some(ref mut o) = *organ {
        for division in &mut o.divisions {
            if let Some(stop) = division.stops.iter_mut().find(|s| s.id == stop_id) {
                stop.drawn = !stop.drawn;
                let drawn = stop.drawn;
                let internal_id = stop.internal_stop_id;
                let first_midi = stop.first_midi_note as u32;
                let last_midi = stop.last_midi_note as u32;
                state.set_stop_drawn(&stop_id, drawn);

                // When turning a stop OFF, release all its active voices immediately
                if !drawn {
                    for pipe_num in 1..=(last_midi - first_midi + 1) {
                        state.send_audio_command(AudioCommand::NoteOff {
                            stop_id: internal_id,
                            pipe_num,
                        });
                    }
                    info!("Stop {} turned OFF - released {} pipes", stop_id, last_midi - first_midi + 1);
                } else {
                    // When turning a stop ON, play all currently held notes through it
                    let div_name = division.name.clone();
                    let held = state.held_notes.read();
                    let mappings = state.midi_mappings.read();
                    let mapping = mappings.iter().find(|m| m.division == div_name);

                    let mut triggered = 0;
                    for &(ch, note, vel) in held.iter() {
                        // Check channel filter
                        let accepts = match mapping {
                            Some(m) => m.channel.map_or(true, |mch| mch == ch),
                            None => true,
                        };
                        if !accepts { continue; }

                        let transpose = mapping.map_or(0, |m| m.transpose);
                        let transposed = (note as i16 + transpose as i16).clamp(0, 127) as u8;
                        let t = transposed as u32;

                        if t >= first_midi && t <= last_midi {
                            let pipe_num = t - first_midi + 1;
                            state.send_audio_command(AudioCommand::NoteOn {
                                stop_id: internal_id,
                                pipe_num,
                                midi_note: transposed,
                                velocity: vel as f32 / 127.0,
                            });
                            triggered += 1;
                        }
                    }
                    info!("Stop {} turned ON - triggered {} held notes", stop_id, triggered);
                }

                return Ok(drawn);
            }
        }
    }

    Err(format!("Stop not found: {}", stop_id))
}

/// Set all drawn stops at once (for setzer/preset recall)
/// Stops in the list become drawn=true, all others become drawn=false
#[tauri::command]
pub fn set_drawn_stops(state: State<AppState>, stop_ids: Vec<String>) -> Result<(), String> {
    let mut organ = state.loaded_organ_info.write();

    if let Some(ref mut o) = *organ {
        for division in &mut o.divisions {
            for stop in &mut division.stops {
                let should_draw = stop_ids.contains(&stop.id);
                if stop.drawn != should_draw {
                    stop.drawn = should_draw;
                    state.set_stop_drawn(&stop.id, should_draw);
                }
            }
        }
        info!("set_drawn_stops: {} stops activated", stop_ids.len());
        Ok(())
    } else {
        Err("Geen orgel geladen".into())
    }
}

/// Play a note for a specific stop
#[tauri::command]
pub fn play_note(state: State<AppState>, stop_id: String, note: u8, velocity: f32) -> Result<(), String> {
    // Parse stop_id (format: "manual_stopid")
    let parts: Vec<&str> = stop_id.split('_').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid stop_id format: {}", stop_id));
    }

    let internal_stop_id: u32 = parts[1].parse()
        .map_err(|_| format!("Invalid stop ID: {}", parts[1]))?;

    let definition = state.organ_definition.read();

    // Get manual's first MIDI note
    let manual_first_midi = definition.as_ref()
        .and_then(|def| def.manuals.iter()
            .find(|m| m.stop_ids.contains(&internal_stop_id))
            .map(|m| m.first_accessible_key_midi_note))
        .unwrap_or(36);

    // Calculate pipe number (1-based)
    let pipe_num = if note as u32 >= manual_first_midi {
        (note as u32) - manual_first_midi + 1
    } else {
        return Ok(()); // Note below range
    };

    info!("play_note: stop={}, note={}, manual_first_midi={}, pipe_num={}", internal_stop_id, note, manual_first_midi, pipe_num);

    state.send_audio_command(AudioCommand::NoteOn {
        stop_id: internal_stop_id,
        pipe_num,
        midi_note: note,
        velocity,
    });

    Ok(())
}

/// Stop a note for a specific stop
#[tauri::command]
pub fn stop_note(state: State<AppState>, stop_id: String, note: u8) -> Result<(), String> {
    let parts: Vec<&str> = stop_id.split('_').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid stop_id format: {}", stop_id));
    }

    let internal_stop_id: u32 = parts[1].parse()
        .map_err(|_| format!("Invalid stop ID: {}", parts[1]))?;

    let definition = state.organ_definition.read();

    // Get manual's first MIDI note
    let manual_first_midi = definition.as_ref()
        .and_then(|def| def.manuals.iter()
            .find(|m| m.stop_ids.contains(&internal_stop_id))
            .map(|m| m.first_accessible_key_midi_note))
        .unwrap_or(36);

    // Calculate pipe number (1-based)
    if note as u32 >= manual_first_midi {
        let pipe_num = (note as u32) - manual_first_midi + 1;

        state.send_audio_command(AudioCommand::NoteOff {
            stop_id: internal_stop_id,
            pipe_num,
        });
    }

    Ok(())
}

/// Play a note for all drawn stops
#[tauri::command]
pub fn play_note_all_stops(state: State<AppState>, note: u8, velocity: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    let definition = state.organ_definition.read();
    let drawn_stops = state.drawn_stops.read();

    info!("play_note_all_stops: note={}, velocity={}, drawn_stops={:?}", note, velocity, *drawn_stops);

    if let (Some(ref o), Some(ref def)) = (&*organ, &*definition) {
        for division in &o.divisions {
            for stop in &division.stops {
                if drawn_stops.contains(&stop.id) {
                    // Get stop definition for pipe range info
                    let stop_def = def.get_stop(stop.internal_stop_id);

                    // Get manual's first MIDI note (usually 36 for manuals, lower for pedals)
                    let manual_first_midi = def.manuals.iter()
                        .find(|m| m.stop_ids.contains(&stop.internal_stop_id))
                        .map(|m| m.first_accessible_key_midi_note)
                        .unwrap_or(36);

                    // Calculate pipe number (1-based): pipe 1 = first MIDI note of the manual
                    let pipe_num = if note as u32 >= manual_first_midi {
                        (note as u32) - manual_first_midi + 1
                    } else {
                        continue; // Note is below this stop's range
                    };

                    // Check if pipe exists
                    let num_pipes = stop_def.map(|s| s.number_of_pipes).unwrap_or(61);
                    if pipe_num > num_pipes {
                        continue; // Note is above this stop's range
                    }

                    info!("Triggering stop {} (internal_id={}), note={}, manual_first_midi={}, pipe_num={}",
                          stop.id, stop.internal_stop_id, note, manual_first_midi, pipe_num);

                    state.send_audio_command(AudioCommand::NoteOn {
                        stop_id: stop.internal_stop_id,
                        pipe_num,
                        midi_note: note,
                        velocity,
                    });
                }
            }
        }
    }

    Ok(())
}

/// Stop a note for all drawn stops
#[tauri::command]
pub fn stop_note_all_stops(state: State<AppState>, note: u8) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    let definition = state.organ_definition.read();
    let drawn_stops = state.drawn_stops.read();

    if let (Some(ref o), Some(ref def)) = (&*organ, &*definition) {
        for division in &o.divisions {
            for stop in &division.stops {
                if drawn_stops.contains(&stop.id) {
                    // Get manual's first MIDI note
                    let manual_first_midi = def.manuals.iter()
                        .find(|m| m.stop_ids.contains(&stop.internal_stop_id))
                        .map(|m| m.first_accessible_key_midi_note)
                        .unwrap_or(36);

                    // Calculate pipe number (1-based)
                    if note as u32 >= manual_first_midi {
                        let pipe_num = (note as u32) - manual_first_midi + 1;

                        state.send_audio_command(AudioCommand::NoteOff {
                            stop_id: stop.internal_stop_id,
                            pipe_num,
                        });
                    }
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn set_master_volume(state: State<AppState>, db: f32) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetMasterGain(db));
    // Onthoud voor per-orgel opslag (save_current_organ_settings leest dit veld).
    *state.master_volume_db.write() = Some(db);
    Ok(())
}

#[tauri::command]
pub fn set_reverb_mix(state: State<AppState>, mix: f32) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetReverbMix(mix.clamp(0.0, 1.0)));
    Ok(())
}

/// Configure algorithmic FDN reverb
#[tauri::command]
pub fn set_algorithmic_reverb(state: State<AppState>, preset: Option<u8>, rt60: f32, pre_delay_ms: f32, damping: f32, room_size: f32, mix: f32) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetAlgorithmicReverb { preset, rt60, pre_delay_ms, damping, room_size, mix });
    Ok(())
}

/// Set crescendo stages configuration
#[tauri::command]
pub fn set_crescendo_config(state: State<AppState>, stages: Vec<Vec<String>>, enabled: bool) -> Result<(), String> {
    *state.crescendo_stages.write() = stages;
    *state.crescendo_enabled.write() = enabled;
    Ok(())
}

/// Set crescendo stage (0 = off, 1..N = active stage)
#[tauri::command]
pub fn set_crescendo_stage(state: State<AppState>, stage: u8) -> Result<Option<Vec<String>>, String> {
    // None = niet toepassen (crescendo uit / niet geconfigureerd);
    // Some(lijst) = toepassen, óók als de lijst leeg is — een lege stap of
    // stap 0 moet de registers juist wegtrekken.
    if !*state.crescendo_enabled.read() {
        return Ok(None);
    }
    let stages = state.crescendo_stages.read();
    if stages.is_empty() {
        return Ok(None);
    }
    let clamped = (stage as usize).min(stages.len());
    *state.crescendo_stage.write() = clamped as u8;

    // Get target stop IDs for this stage
    let target_stops: Vec<String> = if clamped == 0 {
        Vec::new()
    } else {
        stages[clamped - 1].clone()
    };

    Ok(Some(target_stops))
}

/// Get current crescendo config
#[tauri::command]
pub fn get_crescendo_config(state: State<AppState>) -> (bool, u8, Vec<Vec<String>>) {
    let enabled = *state.crescendo_enabled.read();
    let stage = *state.crescendo_stage.read();
    let stages = state.crescendo_stages.read().clone();
    (enabled, stage, stages)
}

/// Lightweight poll for crescendo live state (enabled, current stage, total stages)
#[tauri::command]
pub fn get_crescendo_state(state: State<AppState>) -> (bool, u8, u8) {
    let enabled = *state.crescendo_enabled.read();
    let stage = *state.crescendo_stage.read();
    let total = state.crescendo_stages.read().len() as u8;
    (enabled, stage, total)
}

/// Learn crescendo pedal MIDI binding.
/// Stille leer-modus: voordat we de MIDI-thread laten luisteren, wissen we de
/// bestaande crescendo-binding en stoppen we alle klinkende noten + crescendo-stops.
/// Anders zou de bewegende pedaal tijdens het leren een al-gebonden crescendo
/// activeren (registers worden gestoken/teruggetrokken), wat het settle-detectie
/// algoritme verstoort en de gebruiker een onstabiele inleer-ervaring geeft.
#[tauri::command]
pub fn learn_crescendo_pedal(state: State<AppState>) -> Result<Option<(u8, u8)>, String> {
    // Stille leer-modus
    *state.crescendo_binding.write() = None;
    {
        let mut active = state.crescendo_active_stops.write();
        for sid in active.iter() {
            state.set_stop_drawn(sid, false);
        }
        active.clear();
    }
    state.send_audio_command(AudioCommand::AllNotesOff);

    let (tx, rx) = crossbeam_channel::bounded(1);
    state.midi_tx.send(crate::state::MidiCommand::LearnCrescendoPedal(tx))
        .map_err(|e| e.to_string())?;
    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(result) => {
            if let Some((ch, cc)) = result {
                // Behoud bestaande invert-keuze als die er was; nieuwe binding
                // (channel, cc, min=0, max=127, invert=false)
                *state.crescendo_binding.write() = Some((ch, cc, 0, 127, false));
            }
            Ok(result)
        }
        Err(_) => Ok(None),
    }
}

/// Clear crescendo pedal binding
#[tauri::command]
pub fn clear_crescendo_binding(state: State<AppState>) {
    *state.crescendo_binding.write() = None;
}

/// Start MIDI recording
#[tauri::command]
pub fn start_midi_recording(state: State<AppState>) {
    let mut rec = state.midi_recording.write();
    *rec = Some(crate::state::MidiRecording {
        events: Vec::new(),
        start_time: std::time::Instant::now(),
        active: true,
        stopped_elapsed: None,
    });
    info!("MIDI recording started");
}

/// Status van de huidige MIDI-opname — voor de UI-poll (live teller events + tijd).
#[derive(serde::Serialize)]
pub struct MidiRecordingStatus {
    pub recording: bool,
    pub event_count: usize,
    pub seconds: f64,
}

/// Geef de live status van de MIDI-opname terug (loopt er een opname, hoeveel events, hoe lang).
#[tauri::command]
pub fn midi_recording_status(state: State<AppState>) -> MidiRecordingStatus {
    let rec = state.midi_recording.read();
    match rec.as_ref() {
        Some(r) => MidiRecordingStatus {
            recording: r.active,
            event_count: r.events.len(),
            // Na stop: bevroren duur i.p.v. eeuwig doorlopende wandkloktijd.
            seconds: r.stopped_elapsed.unwrap_or_else(|| r.start_time.elapsed().as_secs_f64()),
        },
        None => MidiRecordingStatus { recording: false, event_count: 0, seconds: 0.0 },
    }
}

/// Stop MIDI recording
#[tauri::command]
pub fn stop_midi_recording(state: State<AppState>) -> usize {
    // Zet de capture echt uit (anders belanden noten die tijdens de opslag-
    // dialoog gespeeld worden alsnog in het .mid-bestand); de events blijven
    // beschikbaar voor save/afspelen tot clear of een nieuwe opname.
    let mut rec = state.midi_recording.write();
    let count = match rec.as_mut() {
        Some(r) => {
            if r.active {
                r.active = false;
                r.stopped_elapsed = Some(r.start_time.elapsed().as_secs_f64());
            }
            r.events.len()
        }
        None => 0,
    };
    info!("MIDI recording stopped, {} events captured", count);
    count
}

/// Stel een standaard-opslagpad voor een MIDI-opname voor: tijdgestempeld .mid in
/// Documenten/JM-Orgue-opnames (zelfde map als de MP3-opnames). Maakt de map alvast aan
/// zodat de opslagdialoog er direct naartoe kan navigeren.
#[tauri::command]
pub fn suggest_midi_recording_path() -> String {
    let dir = recordings_default_dir();
    let _ = std::fs::create_dir_all(&dir);
    make_timestamped_recording_path("mid").to_string_lossy().to_string()
}

/// Codeer opgenomen events naar een Standard MIDI File (formaat 0, 480 PPQ, vast 120 BPM).
/// `events` = (timestamp_us, status, data1, data2, channel) zoals vastgelegd in de MIDI-thread.
pub fn encode_smf(events: &[(u64, u8, u8, u8, u8)]) -> Vec<u8> {
    let mut data: Vec<u8> = Vec::new();

    // Header chunk: MThd
    data.extend_from_slice(b"MThd");
    data.extend_from_slice(&6u32.to_be_bytes()); // chunk length
    data.extend_from_slice(&0u16.to_be_bytes()); // format 0
    data.extend_from_slice(&1u16.to_be_bytes()); // 1 track
    data.extend_from_slice(&480u16.to_be_bytes()); // 480 ticks per quarter note

    // Track chunk
    let mut track_data: Vec<u8> = Vec::new();

    // Tempo: 120 BPM = 500000 us/quarter
    track_data.extend_from_slice(&[0x00, 0xFF, 0x51, 0x03]);
    track_data.extend_from_slice(&500000u32.to_be_bytes()[1..4]);

    let mut prev_tick: u32 = 0;
    for &(timestamp_us, status, data1, data2, _ch) in events {
        // Convert microseconds to ticks (at 120 BPM, 480 ticks/quarter = 960 ticks/ms)
        let tick = (timestamp_us as f64 / 1_000_000.0 * 120.0 / 60.0 * 480.0) as u32;
        let delta = tick.saturating_sub(prev_tick);
        prev_tick = tick;

        // Write variable-length delta time
        write_var_len(&mut track_data, delta);
        // Write MIDI event
        track_data.push(status);
        track_data.push(data1);
        if status & 0xF0 != 0xC0 && status & 0xF0 != 0xD0 {
            track_data.push(data2);
        }
    }

    // End of track
    track_data.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);

    // Track header
    data.extend_from_slice(b"MTrk");
    data.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
    data.extend_from_slice(&track_data);

    data
}

/// Save MIDI recording as SMF (Standard MIDI File)
#[tauri::command]
pub fn save_midi_recording(state: State<AppState>, path: String) -> Result<(), String> {
    let rec = state.midi_recording.read();
    let recording = rec.as_ref().ok_or("Geen opname actief")?;

    if recording.events.is_empty() {
        return Err("Geen events opgenomen".into());
    }

    let data = encode_smf(&recording.events);

    // Zorg dat de doelmap bestaat (bv. JM-Orgue-opnames bij eerste gebruik).
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, &data)
        .map_err(|e| format!("Kan niet schrijven: {}", e))?;

    info!("MIDI recording saved to {} ({} bytes)", path, data.len());
    Ok(())
}

/// Clear MIDI recording
#[tauri::command]
pub fn clear_midi_recording(state: State<AppState>) {
    *state.midi_recording.write() = None;
}

/// Helper: write variable-length quantity
fn write_var_len(buf: &mut Vec<u8>, mut value: u32) {
    if value == 0 {
        buf.push(0);
        return;
    }
    let mut bytes = Vec::new();
    while value > 0 {
        bytes.push((value & 0x7F) as u8);
        value >>= 7;
    }
    bytes.reverse();
    for (i, b) in bytes.iter().enumerate() {
        if i < bytes.len() - 1 {
            buf.push(b | 0x80);
        } else {
            buf.push(*b);
        }
    }
}

/// Set per-pipe voicing adjustment (also stored in state for persistence)
#[tauri::command]
pub fn set_pipe_voicing(state: State<AppState>, stop_id: u32, pipe_num: u32, volume_db: f32, pitch_cents: f32) -> Result<(), String> {
    let v_db = volume_db.clamp(-20.0, 20.0);
    let p_cents = pitch_cents.clamp(-100.0, 100.0);
    // Store in state
    {
        let mut voicings = state.pipe_voicings.write();
        if v_db == 0.0 && p_cents == 0.0 {
            voicings.remove(&(stop_id, pipe_num));
        } else {
            voicings.insert((stop_id, pipe_num), (v_db, p_cents));
        }
    }
    // Apply to audio thread
    state.send_audio_command(AudioCommand::SetPipeVoicing {
        stop_id, pipe_num,
        volume_db: v_db,
        pitch_cents: p_cents,
    });
    Ok(())
}

/// Get all pipe voicings for current organ
#[tauri::command]
pub fn get_pipe_voicings(state: State<AppState>) -> Vec<(u32, u32, f32, f32)> {
    state.pipe_voicings.read()
        .iter()
        .map(|(&(s, p), &(v, c))| (s, p, v, c))
        .collect()
}

/// Pas opgeslagen divisie-naar-wind-groep toewijzing toe vanuit OrganSettings
#[tauri::command]
pub fn apply_saved_wind_groups(state: State<AppState>) -> Result<usize, String> {
    let organ_id = match state.current_organ_id.read().clone() {
        Some(id) => id,
        None => return Ok(0),
    };
    let (saved_groups, saved_num) = {
        let lib = state.organ_library.read();
        match lib.settings.get(&organ_id) {
            Some(s) => (s.division_wind_groups.clone(), s.num_wind_groups),
            None => (Vec::new(), 0),
        }
    };
    let organ = state.loaded_organ_info.read();
    let organ = match organ.as_ref() {
        Some(o) => o,
        None => return Ok(0),
    };
    // Eerst aantal groepen restoren (indien opgeslagen, niet 0)
    if saved_num >= 1 && saved_num <= 8 {
        *state.num_wind_groups.write() = saved_num;
    }
    let mut applied = 0;
    for entry in &saved_groups {
        if let Some(idx) = organ.divisions.iter().position(|d| d.name == entry.division) {
            let grp = entry.group.min(31);
            {
                let mut wg = state.division_wind_groups.write();
                if idx < wg.len() {
                    wg[idx] = grp;
                }
            }
            state.send_audio_command(AudioCommand::SetDivisionWindGroup {
                division_index: idx as u8,
                group: grp,
            });
            applied += 1;
        }
    }
    Ok(applied)
}

/// Migreer een oude output-pair-index (0..3) naar fysieke kanalen (gangbare 7.1-ordening).
fn migrate_pair_to_channels(pair: u8) -> Vec<u8> {
    match pair {
        1 => vec![4, 5], // Achter L/R
        2 => vec![6, 7], // Zij L/R
        3 => vec![2, 3], // Center/LFE als paar
        _ => vec![0, 1], // Voor L/R
    }
}

/// Pas opgeslagen divisie-naar-output-kanaal routing + C/Cis-spreiding toe vanuit OrganSettings
#[tauri::command]
pub fn apply_saved_output_channels(state: State<AppState>) -> Result<usize, String> {
    let organ_id = match state.current_organ_id.read().clone() {
        Some(id) => id,
        None => return Ok(0),
    };
    let (saved_routing, saved_ccis, saved_spread) = {
        let lib = state.organ_library.read();
        match lib.settings.get(&organ_id) {
            Some(s) => (s.division_output_pairs.clone(), s.division_ccis.clone(), s.ccis_spread.clone()),
            None => (Vec::new(), Vec::new(), None),
        }
    };
    let organ = state.loaded_organ_info.read();
    let organ = match organ.as_ref() {
        Some(o) => o,
        None => return Ok(0),
    };
    let mut applied = 0;
    for entry in &saved_routing {
        if let Some(idx) = organ.divisions.iter().position(|d| d.name == entry.division) {
            // Nieuw model = vrije kanaallijst; oude opslag: migreer pair → kanalen.
            let channels = entry.channels.clone().unwrap_or_else(|| migrate_pair_to_channels(entry.pair));
            {
                let mut chans = state.division_output_channels.write();
                if idx < chans.len() { chans[idx] = channels.clone(); }
            }
            state.send_audio_command(AudioCommand::SetDivisionOutputChannels {
                division_index: idx as u8,
                channels,
            });
            applied += 1;
        }
    }
    // C/Cis globale parameters
    if let Some(sp) = saved_spread {
        *state.ccis_spread.write() = (sp.strength, sp.falloff, sp.swap);
        state.send_audio_command(AudioCommand::SetCcisSpread {
            strength: sp.strength, falloff: sp.falloff, swap: sp.swap,
        });
    }
    // C/Cis per-divisie aan/uit
    for entry in &saved_ccis {
        if let Some(idx) = organ.divisions.iter().position(|d| d.name == entry.division) {
            {
                let mut en = state.division_ccis_enabled.write();
                if idx < en.len() { en[idx] = entry.enabled; }
            }
            state.send_audio_command(AudioCommand::SetDivisionCcis {
                division_index: idx as u8, enabled: entry.enabled,
            });
        }
    }
    Ok(applied)
}

/// Apply saved voicings from organ settings to state + audio thread
#[tauri::command]
pub fn apply_saved_voicings(state: State<AppState>) -> Result<usize, String> {
    let organ_id = match state.current_organ_id.read().clone() {
        Some(id) => id,
        None => return Ok(0),
    };

    let saved = {
        let lib = state.organ_library.read();
        lib.settings.get(&organ_id)
            .map(|s| s.pipe_voicings.clone())
            .unwrap_or_default()
    };

    {
        let mut voicings = state.pipe_voicings.write();
        voicings.clear();
        for v in &saved {
            voicings.insert((v.stop_id, v.pipe_num), (v.volume_db, v.pitch_cents));
        }
    }

    for v in &saved {
        state.send_audio_command(AudioCommand::SetPipeVoicing {
            stop_id: v.stop_id,
            pipe_num: v.pipe_num,
            volume_db: v.volume_db,
            pitch_cents: v.pitch_cents,
        });
    }

    Ok(saved.len())
}

/// Reset voicing for a single pipe
#[tauri::command]
pub fn reset_pipe_voicing(state: State<AppState>, stop_id: u32, pipe_num: u32) -> Result<(), String> {
    state.pipe_voicings.write().remove(&(stop_id, pipe_num));
    state.send_audio_command(AudioCommand::SetPipeVoicing {
        stop_id, pipe_num,
        volume_db: 0.0,
        pitch_cents: 0.0,
    });
    Ok(())
}

/// Reset voicing for all pipes of a stop
#[tauri::command]
pub fn reset_stop_voicing(state: State<AppState>, stop_id: u32) -> Result<(), String> {
    let pipes_to_reset: Vec<u32> = {
        let voicings = state.pipe_voicings.read();
        voicings.keys()
            .filter(|&&(s, _)| s == stop_id)
            .map(|&(_, p)| p)
            .collect()
    };
    {
        let mut voicings = state.pipe_voicings.write();
        for &p in &pipes_to_reset {
            voicings.remove(&(stop_id, p));
        }
    }
    for &p in &pipes_to_reset {
        state.send_audio_command(AudioCommand::SetPipeVoicing {
            stop_id, pipe_num: p,
            volume_db: 0.0,
            pitch_cents: 0.0,
        });
    }
    Ok(())
}

/// Route een divisie naar een vrije set fysieke output-kanalen.
/// `channels`: lijst kanaalindices (leeg = standaard voorste paar 0/1). Opeenvolgende
/// kanalen vormen (L,R)-paren; een los laatste kanaal krijgt mono. Zo kan een klavier
/// uit 1 t/m alle beschikbare kanalen klinken (eigen ideale mapping).
#[tauri::command]
pub fn set_division_output_channels(state: State<AppState>, division: String, channels: Vec<u8>) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            {
                let mut chans = state.division_output_channels.write();
                if idx < chans.len() { chans[idx] = channels.clone(); }
            }
            state.send_audio_command(AudioCommand::SetDivisionOutputChannels {
                division_index: idx as u8,
                channels,
            });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Lees alle divisie output-kanaal instellingen (in volgorde van de divisies)
#[tauri::command]
pub fn get_division_output_channels(state: State<AppState>) -> Vec<Vec<u8>> {
    state.division_output_channels.read().clone()
}

/// C/Cis-lade spreiding: globale parameters (sterkte 0..1, afval-met-toonhoogte 0..1, swap).
#[tauri::command]
pub fn set_ccis_spread(state: State<AppState>, strength: f32, falloff: f32, swap: bool) -> Result<(), String> {
    *state.ccis_spread.write() = (strength, falloff, swap);
    state.send_audio_command(AudioCommand::SetCcisSpread { strength, falloff, swap });
    Ok(())
}

/// Lees de globale C/Cis-spreiding parameters: (sterkte, afval, swap).
#[tauri::command]
pub fn get_ccis_spread(state: State<AppState>) -> (f32, f32, bool) {
    *state.ccis_spread.read()
}

/// Zet de C/Cis-lade spreiding aan/uit voor één divisie.
#[tauri::command]
pub fn set_division_ccis(state: State<AppState>, division: String, enabled: bool) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            {
                let mut en = state.division_ccis_enabled.write();
                if idx < en.len() { en[idx] = enabled; }
            }
            state.send_audio_command(AudioCommand::SetDivisionCcis { division_index: idx as u8, enabled });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Lees de C/Cis-aan/uit per divisie (in volgorde van de divisies).
#[tauri::command]
pub fn get_division_ccis(state: State<AppState>) -> Vec<bool> {
    state.division_ccis_enabled.read().clone()
}

/// Lees het aantal audio output-kanalen van het ACTUEEL geselecteerde device
/// (niet het systeem-default). Anders toont de UI een verkeerd kanaalaantal en
/// routet de gebruiker naar niet-bestaande kanalen → vreemd/stil gedrag.
#[tauri::command]
pub fn query_audio_channel_count(state: State<AppState>) -> Result<u16, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    // Lees het door de audio-player gekozen host/device.
    let (host_name, device_name) = {
        let ap = state.audio_player.read();
        match ap.as_ref() {
            Some(p) => (p.current_host.read().clone(), p.current_device.read().clone()),
            None => (String::new(), String::new()),
        }
    };
    let host = crate::audio::resolve_host(if host_name.is_empty() { None } else { Some(&host_name) });
    let device = (!device_name.is_empty())
        .then(|| {
            host.output_devices().ok().and_then(|mut devs| {
                devs.find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
            })
        })
        .flatten()
        .or_else(|| host.default_output_device())
        .ok_or_else(|| "Geen audio output device gevonden".to_string())?;
    let cfg = device.default_output_config()
        .map_err(|e| format!("Kan config niet ophalen: {}", e))?;
    Ok(cfg.channels())
}

/// Set per-division stereo pan
#[tauri::command]
pub fn set_division_pan(state: State<AppState>, division: String, pan: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            let clamped = pan.clamp(-1.0, 1.0);
            state.send_audio_command(AudioCommand::SetDivisionPan {
                division_index: idx as u8,
                pan: clamped,
            });
            // Per-orgel opslag-spiegel (verzameld door do_save_organ_settings).
            state.division_pans.write().insert(division.clone(), clamped);
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Configure 3-band parametric EQ
#[tauri::command]
pub fn set_parametric_eq(state: State<AppState>, enabled: bool, low_freq: f32, low_gain: f32, mid_freq: f32, mid_gain: f32, mid_q: f32, high_freq: f32, high_gain: f32) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetParametricEq {
        enabled, low_freq, low_gain, mid_freq, mid_gain, mid_q, high_freq, high_gain
    });
    // Onthoud voor per-orgel opslag (save_current_organ_settings leest dit veld).
    *state.eq_settings.write() = Some(library::EqSettingsSaved {
        enabled, low_freq, low_gain, mid_freq, mid_gain, mid_q, high_freq, high_gain,
    });
    Ok(())
}

/// Bewaar de volledige reverb-configuratie van het huidige orgel voor per-orgel opslag.
/// Apart van de losse set_reverb_*-audio-commando's: de frontend roept dit aan met de
/// complete configuratie zodat save_current_organ_settings die kan opslaan. Stuurt zelf
/// GEEN audio (dat doen de bestaande set_reverb_*-commando's al).
#[tauri::command]
pub fn persist_reverb_config(state: State<AppState>, reverb_type: String, preset: Option<u8>, rt60: f32, pre_delay_ms: f32, damping: f32, room_size: f32, mix: f32, ir_path: Option<String>) -> Result<(), String> {
    *state.reverb_settings.write() = Some(library::ReverbSettingsSaved {
        reverb_type, mix, algorithmic_preset: preset, rt60, pre_delay_ms, damping, room_size, ir_path,
    });
    Ok(())
}

/// Switch between convolution and algorithmic reverb
#[tauri::command]
pub fn set_reverb_type(state: State<AppState>, algorithmic: bool) -> Result<(), String> {
    state.send_audio_command(AudioCommand::SetReverbType { algorithmic });
    Ok(())
}

/// Set temperament: 12 cent offsets per note class [C..B] + global fine-tuning.
/// `name`/`a4_hz` zijn optioneel en dienen alleen voor per-orgel opslag (UI-herstel van de
/// gekozen stemming); de audio gebruikt enkel note_offsets + fine_tune.
#[tauri::command]
pub fn set_temperament(state: State<AppState>, note_offsets: Vec<f32>, fine_tune: f32, name: Option<String>, a4_hz: Option<f32>) -> Result<(), String> {
    let mut offsets = [0.0f32; 12];
    for (i, &v) in note_offsets.iter().take(12).enumerate() {
        offsets[i] = v;
    }
    state.send_audio_command(AudioCommand::SetTemperament { note_offsets: offsets, fine_tune });
    // Onthoud voor per-orgel opslag (save_current_organ_settings leest dit veld).
    *state.temperament_settings.write() = Some(library::TemperamentSettingsSaved {
        name: name.unwrap_or_default(),
        custom_cents: Some(offsets),
        fine_tune_cents: fine_tune,
        a4_hz: a4_hz.unwrap_or(440.0),
    });
    Ok(())
}

/// Export current scanned directory as .organ definition file
#[tauri::command]
pub fn export_organ_file(state: State<AppState>, directory: String, output_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&directory);
    if !path.exists() || !path.is_dir() {
        return Err(format!("Map niet gevonden: {}", directory));
    }
    let organ = scan_samples_directory(path, &path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Orgel".to_string()))
        .map_err(|e| format!("Scan mislukt: {}", e))?;

    let out = std::path::Path::new(&output_path);
    organ.export_organ_file(out)
        .map_err(|e| format!("Export mislukt: {}", e))?;

    info!("Exported organ definition to: {}", output_path);
    Ok(())
}

/// Load an impulse response WAV file for convolution reverb
#[tauri::command]
pub fn load_impulse_response(state: State<AppState>, path: String) -> Result<(), String> {
    let ir_path = std::path::PathBuf::from(&path);
    if !ir_path.exists() {
        return Err(format!("IR file not found: {}", path));
    }
    state.send_audio_command(AudioCommand::LoadImpulseResponse(ir_path));
    info!("Loading impulse response: {}", path);
    Ok(())
}

/// Enable or disable tremulant for a division
/// Finds all stops in the division that have tremulant samples and toggles them
#[tauri::command]
pub fn set_tremulant(state: State<AppState>, division_name: String, active: bool) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();

    if let Some(ref o) = *organ {
        if let Some(div) = o.divisions.iter().find(|d| d.name == division_name) {
            let stop_ids: Vec<u32> = div.stops.iter()
                .filter(|s| s.has_tremulant)
                .map(|s| s.internal_stop_id)
                .collect();

            if stop_ids.is_empty() {
                info!("No tremulant samples available for division '{}'", division_name);
                return Ok(());
            }

            info!("Tremulant {} for division '{}': {} stops",
                  if active { "ON" } else { "OFF" }, division_name, stop_ids.len());

            state.send_audio_command(AudioCommand::SetTremulant { stop_ids, active });
            return Ok(());
        }
    }

    Err(format!("Division not found: {}", division_name))
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Result<StatusDto, String> {
    let organ = state.loaded_organ_info.read();
    let usb_midi_connected = *state.midi_connected.read();
    let ble_midi_connected = state.ble_midi.read().as_ref()
        .map(|m| !m.connected_devices().is_empty())
        .unwrap_or(false);
    let peaks = state.peak_meters();
    let (sample_rate, audio_host, audio_device, buffer_frames) = state.audio_player.read().as_ref()
        .map(|p| (
            *p.sample_rate.read(),
            p.current_host.read().clone(),
            p.current_device.read().clone(),
            *p.current_buffer_frames.read(),
        ))
        .unwrap_or((0, String::new(), String::new(), 0));

    Ok(StatusDto {
        audio_running: state.audio_player.read().is_some(),
        midi_connected: usb_midi_connected || ble_midi_connected,
        organ_loaded: organ.is_some(),
        voice_count: state.voice_count(),
        peak_left: peaks.0,
        peak_right: peaks.1,
        sample_rate,
        audio_host,
        audio_device,
        buffer_frames,
    })
}

// ============ MIDI File Playback ============

#[derive(Debug, Serialize)]
pub struct MidiPlayerStatus {
    pub state: String,         // "stopped" | "playing" | "paused"
    pub position_ms: u64,
    pub duration_ms: u64,
    pub loaded: bool,
}

#[tauri::command]
pub fn midi_play_file(state: State<AppState>, path: String) -> Result<(), String> {
    let player = state.midi_player.read();
    let p = player.as_ref().ok_or_else(|| "MIDI player niet beschikbaar".to_string())?;
    p.load_and_play(std::path::Path::new(&path))?;
    Ok(())
}

#[tauri::command]
pub fn midi_stop_playback(state: State<AppState>) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.stop();
        // Stuur all-notes-off naar audio
        state.send_audio_command(crate::audio::AudioCommand::AllNotesOff);
    }
    Ok(())
}

#[tauri::command]
pub fn midi_pause_playback(state: State<AppState>) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.pause();
    }
    Ok(())
}

#[tauri::command]
pub fn midi_resume_playback(state: State<AppState>) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.resume();
    }
    Ok(())
}

#[tauri::command]
pub fn midi_seek(state: State<AppState>, position_ms: u64) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.seek_to(position_ms * 1000);
        // Veiligheid: stop hangende noten op huidige positie
        state.send_audio_command(crate::audio::AudioCommand::AllNotesOff);
    }
    Ok(())
}

#[tauri::command]
pub fn midi_set_speed(state: State<AppState>, speed: f32) -> Result<(), String> {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.set_speed(speed);
    }
    Ok(())
}

#[tauri::command]
pub fn midi_player_status(state: State<AppState>) -> Result<MidiPlayerStatus, String> {
    let player = state.midi_player.read();
    let p = match player.as_ref() {
        Some(p) => p,
        None => return Ok(MidiPlayerStatus {
            state: "stopped".to_string(),
            position_ms: 0,
            duration_ms: 0,
            loaded: false,
        }),
    };
    let s = match p.state() {
        vpo_midi::PlayerState::Stopped => "stopped",
        vpo_midi::PlayerState::Playing => "playing",
        vpo_midi::PlayerState::Paused => "paused",
    };
    Ok(MidiPlayerStatus {
        state: s.to_string(),
        position_ms: p.position_micros() / 1000,
        duration_ms: p.duration_micros() / 1000,
        loaded: p.is_loaded(),
    })
}

/// Lijst van sample rates die het standaard audio-device ondersteunt
#[tauri::command]
pub fn query_supported_sample_rates(state: State<AppState>) -> Result<Vec<u32>, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    // Kijk naar het ACTUEEL geselecteerde host/device (niet het systeem-default),
    // zelfde patroon als query_audio_channel_count.
    let (host_name, device_name) = {
        let ap = state.audio_player.read();
        match ap.as_ref() {
            Some(p) => (p.current_host.read().clone(), p.current_device.read().clone()),
            None => (String::new(), String::new()),
        }
    };
    let host = crate::audio::resolve_host(if host_name.is_empty() { None } else { Some(&host_name) });
    let device = (!device_name.is_empty())
        .then(|| {
            host.output_devices().ok().and_then(|mut devs| {
                devs.find(|d| d.name().map(|n| n == device_name).unwrap_or(false))
            })
        })
        .flatten()
        .or_else(|| host.default_output_device())
        .ok_or_else(|| "Geen output audio-device gevonden".to_string())?;

    let configs = device.supported_output_configs()
        .map_err(|e| format!("Kan ondersteunde configs niet ophalen: {}", e))?;

    let mut rates: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for cfg in configs {
        let min = cfg.min_sample_rate().0;
        let max = cfg.max_sample_rate().0;
        // Voeg gangbare rates toe die binnen min..=max vallen
        for &candidate in &[44100u32, 48000, 88200, 96000, 176400, 192000] {
            if candidate >= min && candidate <= max {
                rates.insert(candidate);
            }
        }
        // Plus de min/max zelf voor edge-cases
        rates.insert(min);
        rates.insert(max);
    }

    Ok(rates.into_iter().collect())
}

/// Toggle a coupler on/off
#[tauri::command]
pub fn toggle_coupler(state: State<AppState>, coupler_id: String) -> Result<bool, String> {
    info!("Toggle coupler: {}", coupler_id);
    let active = state.toggle_coupler(&coupler_id);

    // Update coupler active state in organ info
    let mut organ = state.loaded_organ_info.write();
    if let Some(ref mut o) = *organ {
        if let Some(ref mut couplers) = o.couplers {
            if let Some(c) = couplers.iter_mut().find(|c| c.id == coupler_id) {
                c.active = active;
            }
        }
    }

    Ok(active)
}

/// Set all active couplers at once (for preset recall)
#[tauri::command]
pub fn set_active_couplers(state: State<AppState>, coupler_ids: Vec<String>) -> Result<(), String> {
    info!("set_active_couplers: {:?}", coupler_ids);
    state.set_active_couplers(coupler_ids.clone());

    // Update coupler active states in organ info
    let mut organ = state.loaded_organ_info.write();
    if let Some(ref mut o) = *organ {
        if let Some(ref mut couplers) = o.couplers {
            for c in couplers.iter_mut() {
                c.active = coupler_ids.contains(&c.id);
            }
        }
    }

    Ok(())
}

// ============ Swell Box (Zwelkast) Commands ============

/// Set division volume from UI (LED slider click)
#[tauri::command]
pub fn set_division_volume(state: State<AppState>, division: String, volume: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            state.set_division_gain(idx as u8, volume.clamp(0.0, 1.0));
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Get all division volumes (for LED display polling)
#[tauri::command]
pub fn get_division_volumes(state: State<AppState>) -> Vec<DivisionVolumeDto> {
    let organ = state.loaded_organ_info.read();
    let gains = state.get_division_gains();
    match *organ {
        Some(ref o) => o.divisions.iter().enumerate().map(|(i, d)| {
            DivisionVolumeDto {
                division: d.name.clone(),
                volume: gains.get(i).copied().unwrap_or(1.0),
            }
        }).collect(),
        None => Vec::new(),
    }
}

/// Configure tremulant LFO for a division (fallback when no trem samples)
#[tauri::command]
pub fn set_tremulant_lfo(state: State<AppState>, division: String, active: bool, rate: f32, amp_depth: f32, pitch_depth: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            state.send_audio_command(AudioCommand::SetTremulantLFO {
                division_index: idx as u8,
                active,
                rate: rate.clamp(3.0, 10.0),
                amp_depth: amp_depth.clamp(0.0, 0.3),
                pitch_depth: pitch_depth.clamp(0.0, 50.0),
            });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Bewaar de tremulant-LFO-config van een divisie voor per-orgel opslag (beschikbaarheid +
/// parameters; NIET de live aan/uit-stand). Stuurt zelf geen audio — set_tremulant_lfo doet dat.
#[tauri::command]
pub fn persist_tremulant_config(state: State<AppState>, division: String, enabled: bool, rate: f32, amp_depth: f32, pitch_depth: f32) -> Result<(), String> {
    state.division_tremulants.write().insert(division, (enabled, rate, amp_depth, pitch_depth));
    Ok(())
}

/// Bewaar de wind-model-config van een wind-groep voor per-orgel opslag. Stuurt zelf geen audio
/// — set_wind_model doet dat. `group` = wind-groep-index (0..31).
#[tauri::command]
pub fn persist_wind_group_config(state: State<AppState>, group: u8, enabled: bool, reservoir_size: f32, damping: f32, max_sag: f32) -> Result<(), String> {
    state.wind_group_configs.write().insert(group, (enabled, reservoir_size, damping, max_sag));
    Ok(())
}

/// Configure wind model for a division
#[tauri::command]
pub fn set_wind_model(state: State<AppState>, division: String, enabled: bool, reservoir_size: f32, damping: f32, max_sag: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            // Vertaal divisie-index naar groep-index via de huidige toewijzing.
            let group = state.division_wind_groups.read()
                .get(idx).copied().unwrap_or(idx as u8);
            state.send_audio_command(AudioCommand::SetWindModel {
                division_index: group, // audio thread interpreteert dit als groep-index
                enabled,
                reservoir_size,
                damping,
                max_sag,
            });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Wijs een divisie aan een wind-groep toe (0..31).
/// Default identity: divisie i hoort bij groep i.
/// Twee divisies met dezelfde groep delen één wind-reservoir.
#[tauri::command]
pub fn set_division_wind_group(state: State<AppState>, division: String, group: u8) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            let clamped = group.min(31);
            {
                let mut wg = state.division_wind_groups.write();
                if idx < wg.len() {
                    wg[idx] = clamped;
                }
            }
            state.send_audio_command(AudioCommand::SetDivisionWindGroup {
                division_index: idx as u8,
                group: clamped,
            });
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Lees per-divisie wind-groep toewijzing (in volgorde van organ.divisions).
#[tauri::command]
pub fn get_division_wind_groups(state: State<AppState>) -> Vec<u8> {
    state.division_wind_groups.read().clone()
}

/// Lees het aantal actieve wind-groepen (1..8).
#[tauri::command]
pub fn get_num_wind_groups(state: State<AppState>) -> u8 {
    *state.num_wind_groups.read()
}

/// Stel het aantal actieve wind-groepen in (1..8).
/// Divisies die boven het max liggen worden teruggeklemd naar de hoogste geldige groep.
#[tauri::command]
pub fn set_num_wind_groups(state: State<AppState>, count: u8) -> Result<(), String> {
    let clamped = count.clamp(1, 8);
    *state.num_wind_groups.write() = clamped;
    // Clamp bestaande toewijzingen naar het nieuwe max
    let mut wg = state.division_wind_groups.write();
    let max_idx = clamped - 1;
    for slot in wg.iter_mut() {
        if *slot > max_idx {
            *slot = max_idx;
        }
    }
    // Stuur de aangepaste toewijzingen naar de audio thread
    let snapshot: Vec<u8> = wg.clone();
    drop(wg);
    for (i, &grp) in snapshot.iter().enumerate() {
        state.send_audio_command(AudioCommand::SetDivisionWindGroup {
            division_index: i as u8,
            group: grp,
        });
    }
    Ok(())
}

/// Configure swell box parameters for a division
#[tauri::command]
pub fn set_swell_config(state: State<AppState>, division: String, min_db: f32, filter_cutoff: f32) -> Result<(), String> {
    let organ = state.loaded_organ_info.read();
    if let Some(ref o) = *organ {
        if let Some(idx) = o.divisions.iter().position(|d| d.name == division) {
            let mdb = min_db.clamp(-30.0, -5.0);
            let cutoff = filter_cutoff.clamp(200.0, 5000.0);
            state.send_audio_command(AudioCommand::SetSwellConfig {
                division_index: idx as u8,
                min_db: mdb,
                filter_cutoff_closed: cutoff,
            });
            // Per-orgel opslag-spiegel (verzameld door do_save_organ_settings).
            state.division_swell_configs.write().insert(division.clone(), (mdb, cutoff));
            return Ok(());
        }
    }
    Err(format!("Division not found: {}", division))
}

/// Learn swell pedal MIDI binding for a division
#[tauri::command]
pub fn learn_swell_pedal(state: State<AppState>, division: String) -> Result<Option<SwellBindingDto>, String> {
    let organ = state.loaded_organ_info.read();
    let division_index = match *organ {
        Some(ref o) => o.divisions.iter().position(|d| d.name == division),
        None => None,
    };
    drop(organ);

    let idx = division_index.ok_or_else(|| format!("Division not found: {}", division))?;
    info!("Learning swell pedal for {} (index={})", division, idx);

    let result = state.learn_swell_pedal(&division, idx as u8)?;
    Ok(result.map(|b| SwellBindingDto {
        division: b.division_name,
        channel: b.channel,
        cc_num: b.cc_num,
        min_val: b.min_val,
        max_val: b.max_val,
        invert: b.invert,
    }))
}

/// Clear swell pedal MIDI binding for a division
#[tauri::command]
pub fn clear_swell_binding(state: State<AppState>, division: String) {
    info!("Clearing swell binding for {}", division);
    state.clear_swell_binding(&division);
}

/// Get all swell pedal bindings
#[tauri::command]
pub fn get_swell_bindings(state: State<AppState>) -> Vec<SwellBindingDto> {
    state.get_swell_bindings().into_iter().map(|b| SwellBindingDto {
        division: b.division_name,
        channel: b.channel,
        cc_num: b.cc_num,
        min_val: b.min_val,
        max_val: b.max_val,
        invert: b.invert,
    }).collect()
}

/// Zet/wissel het spiegelbeeld (invert) voor de zwelpedaal van één divisie.
#[tauri::command]
pub fn set_swell_invert(state: State<AppState>, division: String, invert: bool) -> Result<(), String> {
    let mut bindings = state.get_swell_bindings();
    let mut found = false;
    for b in bindings.iter_mut() {
        if b.division_name == division { b.invert = invert; found = true; }
    }
    if !found { return Err(format!("Geen zwelbinding voor {}", division)); }
    state.set_swell_bindings_replace(bindings);
    Ok(())
}

/// Zet/wissel het spiegelbeeld (invert) voor de generaal-crescendo pedaal.
#[tauri::command]
pub fn set_crescendo_invert(state: State<AppState>, invert: bool) -> Result<(), String> {
    let mut b = state.crescendo_binding.write();
    match *b {
        Some((ch, cc, mn, mx, _)) => { *b = Some((ch, cc, mn, mx, invert)); Ok(()) }
        None => Err("Nog geen crescendo-binding".to_string()),
    }
}

/// Stel het CC-bereik (min/max pedaalstand) voor de generaal-crescendo handmatig in.
#[tauri::command]
pub fn set_crescendo_range(state: State<AppState>, min_val: u8, max_val: u8) -> Result<(), String> {
    let mut b = state.crescendo_binding.write();
    match *b {
        Some((ch, cc, _, _, invert)) => { *b = Some((ch, cc, min_val.min(127), max_val.min(127), invert)); Ok(()) }
        None => Err("Nog geen crescendo-binding".to_string()),
    }
}

/// Stel het CC-bereik (min/max pedaalstand) voor de zwelpedaal van één divisie handmatig in.
/// De MIDI-thread leest min/max live per CC-bericht, dus de wijziging werkt direct.
#[tauri::command]
pub fn set_swell_range(state: State<AppState>, division: String, min_val: u8, max_val: u8) -> Result<(), String> {
    let mut bindings = state.get_swell_bindings();
    let mut found = false;
    for b in bindings.iter_mut() {
        if b.division_name == division { b.min_val = min_val.min(127); b.max_val = max_val.min(127); found = true; }
    }
    if !found { return Err(format!("Geen zwelbinding voor {}", division)); }
    state.set_swell_bindings_replace(bindings);
    Ok(())
}

/// Lees de crescendo-binding (channel, cc, min, max, invert).
#[tauri::command]
pub fn get_crescendo_binding(state: State<AppState>) -> Option<(u8, u8, u8, u8, bool)> {
    *state.crescendo_binding.read()
}

// ============ Helper functions ============

/// Build a mapping from internal stop_id → division index
/// Used by the audio thread for per-division gain (swell box)
fn build_stop_division_map(divisions: &[DivisionDto]) -> std::collections::HashMap<u32, u8> {
    let mut map = std::collections::HashMap::new();
    for (div_idx, division) in divisions.iter().enumerate() {
        for stop in &division.stops {
            map.insert(stop.internal_stop_id, div_idx as u8);
        }
    }
    map
}

/// Detect reed (tongwerk) stops by name
fn is_reed_stop(name: &str) -> bool {
    let lower = name.to_lowercase();
    // Dutch, German, French, English reed stop names
    lower.contains("trompet") || lower.contains("trumpet") ||
    lower.contains("bazuin") || lower.contains("posaune") ||
    lower.contains("hobo") || lower.contains("oboe") || lower.contains("hautbois") ||
    lower.contains("schalmei") || lower.contains("chalumeau") ||
    lower.contains("cromorne") || lower.contains("cromhorne") || lower.contains("krummhorn") ||
    lower.contains("fagot") || lower.contains("basson") || lower.contains("bassoon") ||
    lower.contains("dulcian") || lower.contains("dulciaan") ||
    lower.contains("ranket") || lower.contains("regal") ||
    lower.contains("bombard") || lower.contains("tuba") ||
    lower.contains("clairon") || lower.contains("clarion") ||
    lower.contains("trompette") || lower.contains("clarinette") || lower.contains("clarinet") ||
    lower.contains("vox humana") || lower.contains("voix humaine") ||
    lower.contains("serpent") || lower.contains("musette")
}

fn get_stop_color(name: &str) -> &'static str {
    let lower = name.to_lowercase();

    if lower.contains("trompet") || lower.contains("bazuin") || lower.contains("hobo") ||
       lower.contains("schalmei") || lower.contains("cromorne") {
        "#d47070"
    } else if lower.contains("fluit") || lower.contains("gedekt") || lower.contains("gedackt") ||
              lower.contains("bourdon") || lower.contains("holpijp") || lower.contains("rörflöjt") {
        "#f5deb3"
    } else if lower.contains("viola") || lower.contains("gamba") || lower.contains("celeste") ||
              lower.contains("salicional") || lower.contains("voix") || lower.contains("aeoline") {
        "#90c090"
    } else if lower.contains("mixtuur") || lower.contains("cymbel") || lower.contains("sesquialter") ||
              lower.contains("cornet") || lower.contains("scherp") {
        "#b8b8d0"
    } else if lower.contains("koppel") || lower.contains("coupler") {
        "#c8a0c8"
    } else if lower.contains("tremulant") {
        "#88b8c8"
    } else if lower.contains("subbas") || lower.contains("bourdon 16") {
        "#a89070"
    } else {
        "#e8e8e0"
    }
}

fn roman_numeral(n: usize) -> &'static str {
    match n {
        1 => "I",
        2 => "II",
        3 => "III",
        4 => "IV",
        5 => "V",
        6 => "VI",
        _ => "",
    }
}

/// Build the organ's REAL couplers from the parsed definition: each manual's
/// referenced couplers (GrandOrgue `CouplerDef`; the Hauptwerk importer fills the
/// same structures from its KeyAction routing). IDs are prefixed `real_coupler_`
/// so the UI shows them by default. Returns empty when the source defines no
/// couplers — the caller then falls back to the generic `generate_couplers`.
fn build_couplers_from_definition(definition: &OrganDefinition) -> Vec<CouplerDto> {
    let mut couplers = Vec::new();
    let mut action_code: u8 = 100;
    for manual in &definition.manuals {
        let src_name = manual.name.clone();
        for cid in &manual.coupler_ids {
            let cdef = match definition.couplers.iter().find(|c| c.number == *cid) {
                Some(c) => c,
                None => continue,
            };
            let dest_name = definition.manuals.iter()
                .find(|m| m.number == cdef.destination_manual)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| src_name.clone());
            let offset = cdef.destination_keyshift;
            let coupler_type = if offset >= 12 { "super" }
                else if offset <= -12 { "sub" }
                else { "unison" };
            // Many ODFs name couplers generically ("Coupler 1"); replace those with
            // a descriptive "source + destination" label (matching the app's own
            // coupler naming), keeping any meaningful ODF name as-is.
            let raw = cdef.name.trim();
            let rl = raw.to_lowercase();
            let is_generic = raw.is_empty()
                || rl.strip_prefix("coupler").map_or(false, |r| r.trim().chars().all(|c| c.is_ascii_digit()))
                || rl.strip_prefix("koppel").map_or(false, |r| r.trim().chars().all(|c| c.is_ascii_digit()));
            let suffix = if offset >= 12 { " 4'" } else if offset <= -12 { " 16'" } else { "" };
            let name = if is_generic {
                format!("{}{}\n+\n{}", src_name, suffix, dest_name)
            } else {
                raw.to_string()
            };
            couplers.push(CouplerDto {
                id: format!("real_coupler_{}", cdef.number),
                name,
                source_division: src_name.clone(),
                destination_division: dest_name,
                active: false,
                display_in_division: src_name.clone(),
                midi_action_code: action_code,
                coupler_type: coupler_type.into(),
                pitch_offset: offset,
            });
            action_code = action_code.saturating_add(1);
        }
    }
    couplers
}

/// Generate couplers based on division structure
fn generate_couplers(divisions: &[DivisionDto]) -> Vec<CouplerDto> {
    let mut couplers = Vec::new();
    let mut action_code: u8 = 100;

    // Detect pedal division
    let pedal_idx = divisions.iter().position(|d| {
        let lower = d.name.to_lowercase();
        lower.contains("pedal") || lower.contains("pedaal")
    });

    // Manual (non-pedal) divisions
    let manual_indices: Vec<usize> = (0..divisions.len())
        .filter(|&i| Some(i) != pedal_idx)
        .collect();

    // Pedal couplers: pedal keyboard → each manual's stops
    if let Some(ped_idx) = pedal_idx {
        let ped_name = &divisions[ped_idx].name;
        for &man_idx in &manual_indices {
            let man_name = &divisions[man_idx].name;
            let id = format!("coupler_{}_{}",
                man_name.to_lowercase().replace(' ', "_"),
                ped_name.to_lowercase().replace(' ', "_"));
            couplers.push(CouplerDto {
                id,
                name: format!("{}\n+\n{}", ped_name, man_name),
                source_division: ped_name.clone(),
                destination_division: man_name.clone(),
                active: false,
                display_in_division: ped_name.clone(),
                midi_action_code: action_code,
                coupler_type: "unison".into(),
                pitch_offset: 0,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    // Manual couplers: each manual → each other manual (unison)
    for &src_idx in &manual_indices {
        for &dest_idx in &manual_indices {
            if src_idx == dest_idx { continue; }
            let src_name = &divisions[src_idx].name;
            let dest_name = &divisions[dest_idx].name;
            let id = format!("coupler_{}_{}",
                dest_name.to_lowercase().replace(' ', "_"),
                src_name.to_lowercase().replace(' ', "_"));
            couplers.push(CouplerDto {
                id,
                name: format!("{}\n+\n{}", src_name, dest_name),
                source_division: src_name.clone(),
                destination_division: dest_name.clone(),
                active: false,
                display_in_division: src_name.clone(),
                midi_action_code: action_code,
                coupler_type: "unison".into(),
                pitch_offset: 0,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    // Super-octave couplers (+12 semitones / 4') for each manual pair
    for &src_idx in &manual_indices {
        for &dest_idx in &manual_indices {
            if src_idx == dest_idx { continue; }
            let src_name = &divisions[src_idx].name;
            let dest_name = &divisions[dest_idx].name;
            let id = format!("coupler_super_{}_{}",
                dest_name.to_lowercase().replace(' ', "_"),
                src_name.to_lowercase().replace(' ', "_"));
            couplers.push(CouplerDto {
                id,
                name: format!("{} 4'\n+\n{}", src_name, dest_name),
                source_division: src_name.clone(),
                destination_division: dest_name.clone(),
                active: false,
                display_in_division: src_name.clone(),
                midi_action_code: action_code,
                coupler_type: "super".into(),
                pitch_offset: 12,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    // Sub-octave couplers (-12 semitones / 16') for each manual pair
    for &src_idx in &manual_indices {
        for &dest_idx in &manual_indices {
            if src_idx == dest_idx { continue; }
            let src_name = &divisions[src_idx].name;
            let dest_name = &divisions[dest_idx].name;
            let id = format!("coupler_sub_{}_{}",
                dest_name.to_lowercase().replace(' ', "_"),
                src_name.to_lowercase().replace(' ', "_"));
            couplers.push(CouplerDto {
                id,
                name: format!("{} 16'\n+\n{}", src_name, dest_name),
                source_division: src_name.clone(),
                destination_division: dest_name.clone(),
                active: false,
                display_in_division: src_name.clone(),
                midi_action_code: action_code,
                coupler_type: "sub".into(),
                pitch_offset: -12,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    // Intra-manual super/sub octave couplers (self-coupling at different pitch)
    for &man_idx in &manual_indices {
        let name = &divisions[man_idx].name;
        let lower = name.to_lowercase().replace(' ', "_");
        // Super 4'
        couplers.push(CouplerDto {
            id: format!("coupler_super_{0}_{0}", lower),
            name: format!("{} 4'", name),
            source_division: name.clone(),
            destination_division: name.clone(),
            active: false,
            display_in_division: name.clone(),
            midi_action_code: action_code,
            coupler_type: "super".into(),
            pitch_offset: 12,
        });
        action_code = action_code.saturating_add(1);
        // Sub 16'
        couplers.push(CouplerDto {
            id: format!("coupler_sub_{0}_{0}", lower),
            name: format!("{} 16'", name),
            source_division: name.clone(),
            destination_division: name.clone(),
            active: false,
            display_in_division: name.clone(),
            midi_action_code: action_code,
            coupler_type: "sub".into(),
            pitch_offset: -12,
        });
        action_code = action_code.saturating_add(1);
    }

    // Tongwerken Af (T.A.) per division — disables reed stops
    for div in divisions {
        let lower = div.name.to_lowercase().replace(' ', "_");
        couplers.push(CouplerDto {
            id: format!("ta_{}", lower),
            name: format!("T.A. {}", div.name),
            source_division: div.name.clone(),
            destination_division: div.name.clone(),
            active: false,
            display_in_division: div.name.clone(),
            midi_action_code: action_code,
            coupler_type: "ta".into(),
            pitch_offset: 0,
        });
        action_code = action_code.saturating_add(1);
    }

    // Melody couplers: only highest held note coupled (per manual)
    for &man_idx in &manual_indices {
        let name = &divisions[man_idx].name;
        let lower = name.to_lowercase().replace(' ', "_");
        couplers.push(CouplerDto {
            id: format!("melody_{}", lower),
            name: format!("Melodie\n{}", name),
            source_division: name.clone(),
            destination_division: name.clone(),
            active: false,
            display_in_division: name.clone(),
            midi_action_code: action_code,
            coupler_type: "melody".into(),
            pitch_offset: 0,
        });
        action_code = action_code.saturating_add(1);
    }

    // Bass couplers: only lowest held note coupled (per manual → pedal)
    if let Some(ped_idx) = pedal_idx {
        let ped_name = &divisions[ped_idx].name;
        for &man_idx in &manual_indices {
            let man_name = &divisions[man_idx].name;
            let lower = man_name.to_lowercase().replace(' ', "_");
            couplers.push(CouplerDto {
                id: format!("bass_{}_{}", lower, ped_name.to_lowercase().replace(' ', "_")),
                name: format!("Bas\n{}", man_name),
                source_division: man_name.clone(),
                destination_division: ped_name.clone(),
                active: false,
                display_in_division: man_name.clone(),
                midi_action_code: action_code,
                coupler_type: "bass".into(),
                pitch_offset: 0,
            });
            action_code = action_code.saturating_add(1);
        }
    }

    couplers
}

// ============ Custom Organ Commands ============

use vpo_sampler::custom_organ::{CustomOrgan, CustomDivision, CustomStop, StopFamily, scan_samples_directory};

/// DTO for custom organ info
#[derive(Debug, Clone, Serialize)]
pub struct CustomOrganDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub builder: String,
    pub stop_count: usize,
    pub pipe_count: usize,
}

/// DTO for custom division
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CustomDivisionDto {
    pub id: String,
    pub name: String,
    pub midi_channel: Option<u8>,
    pub first_midi_note: u8,
    pub last_midi_note: u8,
    pub stops: Vec<CustomStopDto>,
}

/// DTO for custom stop
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct CustomStopDto {
    pub id: String,
    pub name: String,
    pub pitch_feet: f32,
    pub family: String,
    pub pipe_count: usize,
}

/// Scan a directory for samples and create a custom organ
#[tauri::command]
pub fn scan_samples(directory: String) -> Result<CustomOrganDto, String> {
    let path = Path::new(&directory);
    if !path.exists() {
        return Err("Directory does not exist".to_string());
    }

    let organ_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Custom Organ".to_string());

    let organ = scan_samples_directory(path, &organ_name)
        .map_err(|e| format!("Failed to scan directory: {}", e))?;

    Ok(CustomOrganDto {
        id: organ.id.clone(),
        name: organ.name.clone(),
        description: organ.description.clone(),
        builder: organ.builder.clone(),
        stop_count: organ.stop_count(),
        pipe_count: organ.pipe_count(),
    })
}

/// Create a new empty custom organ
#[tauri::command]
pub fn create_custom_organ(name: String) -> CustomOrganDto {
    let organ = CustomOrgan::new(&name);
    CustomOrganDto {
        id: organ.id.clone(),
        name: organ.name.clone(),
        description: organ.description.clone(),
        builder: organ.builder.clone(),
        stop_count: organ.stop_count(),
        pipe_count: organ.pipe_count(),
    }
}

/// Save a custom organ to a file
#[tauri::command]
pub fn save_custom_organ(organ_json: String, path: String) -> Result<(), String> {
    let organ: CustomOrgan = serde_json::from_str(&organ_json)
        .map_err(|e| format!("Invalid organ JSON: {}", e))?;

    organ.save(Path::new(&path))
        .map_err(|e| format!("Failed to save organ: {}", e))
}

/// Load a custom organ from a file
#[tauri::command]
pub fn load_custom_organ(path: String) -> Result<String, String> {
    let organ = CustomOrgan::load(Path::new(&path))
        .map_err(|e| format!("Failed to load organ: {}", e))?;

    serde_json::to_string(&organ)
        .map_err(|e| format!("Failed to serialize organ: {}", e))
}

/// Scan a directory and load preload buffers (attack portions) for instant playback
/// Full samples are loaded on-demand in the background when notes are played
#[tauri::command]
pub fn load_samples_from_directory(state: State<AppState>, directory: String) -> Result<OrganInfoDto, String> {
    do_load_samples_from_directory(&state, &directory)
}

/// Herbruikbare directory-scan + load voor zowel Tauri commands als test API.
pub fn do_load_samples_from_directory(state: &AppState, directory: &str) -> Result<OrganInfoDto, String> {
    // Serialiseer t.o.v. audio-wissels en andere loads (zie load_switch_gate).
    let _gate = state.load_switch_gate.lock();
    use std::collections::HashMap;
    use std::sync::Arc;
    use rayon::prelude::*;
    use vpo_sampler::{load_audio_preload, PreloadBuffer};
    use crate::audio::PRELOAD_SAMPLES;

    let path = Path::new(&directory);
    if !path.exists() {
        return Err("Directory does not exist".to_string());
    }

    let organ_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Custom Organ".to_string());

    info!("Scanning directory for samples: {}", directory);

    let custom_organ = scan_samples_directory(path, &organ_name)
        .map_err(|e| format!("Failed to scan directory: {}", e))?;

    info!("Found {} stops with {} pipes", custom_organ.stop_count(), custom_organ.pipe_count());

    // Collect all sample paths with their keys
    let mut load_tasks: Vec<((u32, u32), std::path::PathBuf)> = Vec::new();
    let mut trem_load_tasks: Vec<((u32, u32), std::path::PathBuf)> = Vec::new();
    let mut stop_id_counter = 1u32;
    let mut stop_action_code: u8 = 150; // Stops use action codes 150-255

    // Build organ info for UI
    let mut divisions: Vec<DivisionDto> = Vec::new();

    for (div_idx, division) in custom_organ.divisions.iter().enumerate() {
        let mut stops: Vec<StopDto> = Vec::new();

        for stop in &division.stops {
            let internal_stop_id = stop_id_counter;
            stop_id_counter += 1;

            // Find the first and last MIDI note for THIS stop (not the division)
            let stop_first_note = stop.pipes.first().map(|p| p.midi_note).unwrap_or(36);
            let stop_last_note = stop.pipes.last().map(|p| p.midi_note).unwrap_or(96);

            // Collect sample paths for preloading (dry samples)
            for pipe in &stop.pipes {
                let full_path = path.join(&pipe.sample_path);
                let pipe_num = (pipe.midi_note as u32).saturating_sub(stop_first_note as u32) + 1;
                load_tasks.push(((internal_stop_id, pipe_num), full_path));
            }

            // Collect tremulant sample paths (same key mapping as dry)
            if stop.has_tremulant {
                for pipe in &stop.tremulant_pipes {
                    let full_path = path.join(&pipe.sample_path);
                    let pipe_num = (pipe.midi_note as u32).saturating_sub(stop_first_note as u32) + 1;
                    trem_load_tasks.push(((internal_stop_id, pipe_num), full_path));
                }
            }

            let color = get_stop_color(&stop.name);

            info!("Stop '{}': internal_id={}, MIDI range {}-{}, trem={}",
                  stop.name, internal_stop_id, stop_first_note, stop_last_note, stop.has_tremulant);

            let reed = matches!(stop.family, vpo_sampler::custom_organ::StopFamily::Reed) || is_reed_stop(&stop.name);
            stops.push(StopDto {
                id: format!("{}_{}", div_idx, internal_stop_id),
                name: stop.name.clone(),
                pitch: stop.pitch_label.clone()
                    .unwrap_or_else(|| format!("{}'", stop.pitch_feet as u32)),
                drawn: false,
                color: Some(color.to_string()),
                has_tremulant: stop.has_tremulant,
                midi_action_code: stop_action_code,
                internal_stop_id,
                first_midi_note: stop_first_note,
                last_midi_note: stop_last_note,
                is_reed: reed,
            });
            stop_action_code = stop_action_code.saturating_add(1);
        }

        let display_name = if division.name.to_lowercase().contains("pedaal") ||
                             division.name.to_lowercase().contains("pedal") {
            "Pedaal".to_string()
        } else {
            format!("{} (I)", division.name)
        };

        let div_has_trem = stops.iter().any(|s| s.has_tremulant);
        divisions.push(DivisionDto {
            name: division.name.clone(),
            display_name,
            stops,
            has_tremulant: div_has_trem,
            has_swell: false, // custom sample folders carry no enclosure info
        });
    }

    // Load preload buffers in parallel (only attack portion ~200ms per sample)
    info!("Loading {} preload buffers (attack portions)...", load_tasks.len());
    let start_time = std::time::Instant::now();

    let preload_results: Vec<_> = load_tasks.par_iter()
        .filter_map(|(key, path)| {
            match load_audio_preload(path, PRELOAD_SAMPLES) {
                Ok(preload) => Some((*key, Arc::new(preload))),
                Err(e) => {
                    warn!("Failed to preload {:?}: {}", path, e);
                    None
                }
            }
        })
        .collect();

    let elapsed = start_time.elapsed();
    info!("Loaded {} preload buffers in {:.2}s", preload_results.len(), elapsed.as_secs_f32());

    // Convert to HashMap
    let preload_buffers: HashMap<(u32, u32), Arc<PreloadBuffer>> = preload_results.into_iter().collect();

    // Register preload buffers with audio player
    info!("Registering preload buffers for instant playback");
    {
        let player_lock = state.audio_player.read();
        if let Some(ref player) = *player_lock {
            player.send_command(crate::audio::AudioCommand::RegisterPreloadBuffers(Arc::new(preload_buffers)))
                .map_err(|e| format!("Failed to register preload buffers: {}", e))?;
            // Custom sample-mappen kennen geen percussieve stops of ODF-intonatie:
            // wis de sets van een eventueel vorig (GrandOrgue/Hauptwerk-)orgel.
            let _ = player.send_command(crate::audio::AudioCommand::RegisterPercussiveStops(Default::default()));
            let _ = player.send_command(crate::audio::AudioCommand::RegisterOdfVoicings(Arc::new(Default::default())));
        }
    }

    // Load tremulant preload buffers (if any)
    if !trem_load_tasks.is_empty() {
        info!("Loading {} tremulant preload buffers...", trem_load_tasks.len());
        let trem_start = std::time::Instant::now();

        let trem_results: Vec<_> = trem_load_tasks.par_iter()
            .filter_map(|(key, path)| {
                match load_audio_preload(path, PRELOAD_SAMPLES) {
                    Ok(preload) => Some((*key, Arc::new(preload))),
                    Err(e) => {
                        warn!("Failed to preload tremulant {:?}: {}", path, e);
                        None
                    }
                }
            })
            .collect();

        let trem_elapsed = trem_start.elapsed();
        info!("Loaded {} tremulant buffers in {:.2}s", trem_results.len(), trem_elapsed.as_secs_f32());

        let trem_buffers: HashMap<(u32, u32), Arc<PreloadBuffer>> = trem_results.into_iter().collect();

        {
            let player_lock = state.audio_player.read();
            if let Some(ref player) = *player_lock {
                player.send_command(crate::audio::AudioCommand::RegisterTremulantBuffers(Arc::new(trem_buffers)))
                    .map_err(|e| format!("Failed to register tremulant buffers: {}", e))?;
            }
        }
    }

    let stop_count: usize = divisions.iter().map(|d| d.stops.len()).sum();
    let couplers = generate_couplers(&divisions);

    let organ_info = OrganInfoDto {
        id: directory.to_string(),
        name: organ_name,
        builder: "Custom Samples".to_string(),
        location: directory.to_string(),
        year: None,
        stop_count,
        divisions,
        couplers: Some(couplers),
    };

    // Reset bij orgelwissel: stop alle klinkende noten en wis de getrokken registratie +
    // crescendo-stops + koppels, zodat geen geluid of registratie van het vórige orgel
    // achterblijft (anders het symptoom "geluid is actief maar de knop niet"). De
    // "laatste stand" van dít orgel wordt hierna via restore_organ_settings hersteld.
    state.send_audio_command(AudioCommand::AllNotesOff);
    state.drawn_stops.write().clear();
    *state.crescendo_active_stops.write() = Vec::new();
    state.set_active_couplers(Vec::new());

    // Build stop→division map and register with audio thread
    let stop_div_map = build_stop_division_map(&organ_info.divisions);
    state.send_audio_command(AudioCommand::RegisterStopDivisionMap(stop_div_map));
    state.reset_division_gains(organ_info.divisions.len());
    state.reset_division_settings(organ_info.divisions.len());

    {
        let mut current = state.loaded_organ_info.write();
        *current = Some(organ_info.clone());
    }

    info!("Loaded custom organ: {} stops (preload buffers ready)", stop_count);

    // Add to library if not already present
    add_to_library_if_new(state, &organ_info, directory, "sample_directory");

    // Set current organ ID and restore settings
    {
        let mut id = state.current_organ_id.write();
        *id = Some(directory.to_string());
    }
    restore_organ_settings(state, directory);

    Ok(organ_info)
}

// ============ Library Helper Functions ============

fn add_to_library_if_new(state: &AppState, organ_info: &OrganInfoDto, source_path: &str, source_type: &str) {
    let mut lib = state.organ_library.write();
    // Check if already in library
    if lib.organs.iter().any(|o| o.source_path == source_path) {
        info!("Organ already in library: {}", source_path);
        return;
    }
    // Find image
    let image_path = library::find_organ_image(source_path, source_type);
    info!("Organ image: {:?}", image_path);

    let entry = OrganLibraryEntry {
        id: source_path.to_string(),
        name: organ_info.name.clone(),
        builder: organ_info.builder.clone(),
        location: organ_info.location.clone(),
        year: organ_info.year.clone(),
        stop_count: organ_info.stop_count,
        source_type: source_type.to_string(),
        source_path: source_path.to_string(),
        image_path,
    };
    lib.organs.push(entry);
    drop(lib);
    state.save_library();
}

fn restore_organ_settings(state: &AppState, organ_id: &str) {
    use crate::state::{MidiPresetBinding, MidiPresetTrigger, MidiChannelMapping, SwellBinding};

    let lib = state.organ_library.read();
    let settings = match lib.settings.get(organ_id) {
        Some(s) => s.clone(),
        None => return,
    };
    drop(lib);

    info!("Restoring settings for organ: {}", organ_id);

    // Restore MIDI channel mappings
    if !settings.midi_mappings.is_empty() {
        let mappings: Vec<MidiChannelMapping> = settings.midi_mappings.iter().map(|m| {
            MidiChannelMapping {
                division: m.division.clone(),
                channel: m.channel,
                transpose: m.transpose,
                first_midi_note: m.first_midi_note,
                last_midi_note: m.last_midi_note,
            }
        }).collect();
        state.set_midi_mappings(mappings);
        info!("Restored {} MIDI mappings", settings.midi_mappings.len());
    }

    // Restore preset bindings
    if !settings.preset_bindings.is_empty() {
        let bindings: Vec<MidiPresetBinding> = settings.preset_bindings.iter().filter_map(|b| {
            let trigger = match b.trigger_type.as_str() {
                "note" => MidiPresetTrigger::Note { note: b.note? },
                "cc" => MidiPresetTrigger::ControlChange {
                    channel: b.channel,
                    controller: b.controller?,
                    value: b.value.unwrap_or(64),
                },
                "program" => MidiPresetTrigger::ProgramChange {
                    channel: b.channel,
                    program: b.program?,
                },
                _ => return None,
            };
            Some(MidiPresetBinding {
                preset_num: b.preset_num,
                trigger,
            })
        }).collect();
        let count = bindings.len();
        state.set_preset_bindings_replace(bindings);
        info!("Restored {} preset bindings", count);
    }

    // Restore swell bindings
    if !settings.swell_bindings.is_empty() {
        let bindings: Vec<SwellBinding> = settings.swell_bindings.iter().map(|b| {
            SwellBinding {
                division_name: b.division_name.clone(),
                division_index: b.division_index,
                channel: b.channel,
                cc_num: b.cc_num,
                min_val: b.min_val,
                max_val: b.max_val,
                invert: b.invert,
            }
        }).collect();
        let count = bindings.len();
        state.set_swell_bindings_replace(bindings);
        info!("Restored {} swell bindings", count);
    }

    // Restore de "laatste stand": getrokken registratie + actieve koppels. Alleen de
    // registratie-set wordt gezet (geen audio getriggerd — er klinkt nog niets vlak na
    // het laden); zodra de gebruiker speelt klinken meteen de juiste registers.
    if !settings.drawn_stops.is_empty() && *state.restore_registration.read() {
        // Filter op registers die in dit orgel bestaan, voor het geval het bestand wijzigde.
        let valid: std::collections::HashSet<String> = {
            let organ = state.loaded_organ_info.read();
            match organ.as_ref() {
                Some(o) => o.divisions.iter().flat_map(|d| d.stops.iter().map(|s| s.id.clone())).collect(),
                None => std::collections::HashSet::new(),
            }
        };
        let restored: Vec<String> = settings.drawn_stops.iter()
            .filter(|id| valid.contains(*id))
            .cloned()
            .collect();
        let n = restored.len();
        let saved_count = settings.drawn_stops.len();
        *state.drawn_stops.write() = restored;
        info!("Restored {} drawn stops (laatste stand)", n);
        if n < saved_count {
            warn!("Registratie deels niet hersteld: {} van {} registers niet gevonden in dit orgel (gewijzigde sampleset?)", saved_count - n, saved_count);
        }
    }
    if !settings.active_couplers.is_empty() {
        state.set_active_couplers(settings.active_couplers.clone());
        info!("Restored {} active couplers", settings.active_couplers.len());
    }

    // Crescendo-pedaalbinding herstellen (channel/cc/min/max/invert).
    if let Some(b) = settings.crescendo_binding.clone() {
        *state.crescendo_binding.write() = Some((b.channel, b.cc_num, b.min_val, b.max_val, b.invert));
        info!("Restored crescendo binding: ch={}, cc={}, invert={}", b.channel, b.cc_num, b.invert);
    }

    // Audio-DSP (master volume, temperament, reverb, EQ): zet ALLEEN de runtime-spiegels op
    // de waarde van DIT orgel (ook als None — zo lekt de waarde van het vorige orgel niet door
    // bij een opslag voordat de frontend pusht). We sturen hier bewust GEEN audio-commando's:
    //  - de frontend past deze vier toe bij load (loadAudioSettingsForOrgan), en
    //  - extra sends tijdens het laden geven onnodige contentie op het audio-commandokanaal.
    // Voor een orgel zonder opgeslagen waarde stuurt de frontend zijn default.
    *state.master_volume_db.write() = settings.master_volume_db;
    *state.temperament_settings.write() = settings.temperament.clone();
    *state.reverb_settings.write() = settings.reverb.clone();
    *state.eq_settings.write() = settings.eq.clone();
    if settings.master_volume_db.is_some() || settings.temperament.is_some()
        || settings.reverb.is_some() || settings.eq.is_some() {
        info!("Restored audio-DSP settings mirror (master/temperament/reverb/eq)");
    }

    // Per-divisie DSP-spiegels herstellen (mirror-only; de frontend past audio toe bij load).
    // De maps zijn net door reset_division_settings gewist, dus we vullen ze vers met DIT orgel.
    {
        let mut pans = state.division_pans.write();
        for p in &settings.division_pans { pans.insert(p.division.clone(), p.pan); }
    }
    {
        let mut sw = state.division_swell_configs.write();
        for s in &settings.division_swell_configs { sw.insert(s.division.clone(), (s.min_db, s.filter_cutoff)); }
    }
    {
        let mut tr = state.division_tremulants.write();
        for t in &settings.division_tremulants {
            tr.insert(t.division.clone(), (t.enabled, t.rate, t.amp_depth, t.pitch_depth));
        }
    }
    {
        let mut wg = state.wind_group_configs.write();
        for w in &settings.wind_group_configs {
            wg.insert(w.group, (w.enabled, w.reservoir_size, w.damping, w.max_sag));
        }
    }
}

// ============ Library Commands ============

#[tauri::command]
pub fn get_organ_library(state: State<AppState>) -> Vec<OrganLibraryEntry> {
    state.organ_library.read().organs.clone()
}

#[tauri::command]
pub fn remove_from_library(state: State<AppState>, id: String) {
    info!("Removing organ from library: {}", id);
    let mut lib = state.organ_library.write();
    lib.organs.retain(|o| o.id != id);
    lib.settings.remove(&id);
    drop(lib);
    state.save_library();
}

#[derive(Debug, Deserialize)]
pub struct SaveSettingsDto {
    pub presets: HashMap<String, PresetData>,
}

use std::collections::HashMap;

#[tauri::command]
pub fn save_current_organ_settings(state: State<AppState>, presets: HashMap<String, PresetData>) {
    do_save_organ_settings(&state, presets);
}

/// Kern van het opslaan van per-orgel instellingen — herbruikbaar buiten de Tauri-command
/// (o.a. de test-API). Verzamelt de huidige state in OrganSettings en schrijft naar library
/// + .jm-settings.json.
pub fn do_save_organ_settings(state: &AppState, presets: HashMap<String, PresetData>) {
    use crate::state::MidiPresetTrigger;

    let organ_id = state.current_organ_id.read().clone();
    let organ_id = match organ_id {
        Some(id) => id,
        None => {
            warn!("No organ loaded, cannot save settings");
            return;
        }
    };

    info!("Saving settings for organ: {}", organ_id);

    // Gather current MIDI mappings
    let midi_mappings: Vec<MidiMappingSaved> = state.get_midi_mappings().into_iter().map(|m| {
        MidiMappingSaved {
            division: m.division,
            channel: m.channel,
            transpose: m.transpose,
            first_midi_note: m.first_midi_note,
            last_midi_note: m.last_midi_note,
        }
    }).collect();

    // Gather current preset bindings
    let preset_bindings: Vec<PresetBindingSaved> = state.get_preset_bindings().into_iter().map(|b| {
        match b.trigger {
            MidiPresetTrigger::Note { note } => PresetBindingSaved {
                preset_num: b.preset_num,
                trigger_type: "note".to_string(),
                note: Some(note),
                channel: None,
                controller: None,
                value: None,
                program: None,
            },
            MidiPresetTrigger::ControlChange { channel, controller, value } => PresetBindingSaved {
                preset_num: b.preset_num,
                trigger_type: "cc".to_string(),
                note: None,
                channel,
                controller: Some(controller),
                value: Some(value),
                program: None,
            },
            MidiPresetTrigger::ProgramChange { channel, program } => PresetBindingSaved {
                preset_num: b.preset_num,
                trigger_type: "program".to_string(),
                note: None,
                channel,
                controller: None,
                value: None,
                program: Some(program),
            },
        }
    }).collect();

    // Gather current swell bindings
    let swell_bindings: Vec<SwellBindingSaved> = state.get_swell_bindings().into_iter().map(|b| {
        SwellBindingSaved {
            division_name: b.division_name,
            division_index: b.division_index,
            channel: b.channel,
            cc_num: b.cc_num,
            min_val: b.min_val,
            max_val: b.max_val,
            invert: b.invert,
        }
    }).collect();

    // Gather current pipe voicings
    let pipe_voicings: Vec<library::PipeVoicingSaved> = state.pipe_voicings.read()
        .iter()
        .map(|(&(stop_id, pipe_num), &(volume_db, pitch_cents))| library::PipeVoicingSaved {
            stop_id, pipe_num, volume_db, pitch_cents,
        })
        .collect();

    // Gather current division-output-channel routing
    let division_output_pairs: Vec<library::DivisionOutputPairSaved> = {
        let chans = state.division_output_channels.read();
        let organ = state.loaded_organ_info.read();
        if let Some(ref o) = *organ {
            o.divisions.iter().enumerate()
                .filter_map(|(i, d)| {
                    chans.get(i).map(|c| library::DivisionOutputPairSaved {
                        division: d.name.clone(),
                        pair: 0,
                        channels: Some(c.clone()),
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    };

    // Gather current C/Cis-lade spreiding (per-divisie aan/uit + globale parameters)
    let division_ccis: Vec<library::DivisionCcisSaved> = {
        let en = state.division_ccis_enabled.read();
        let organ = state.loaded_organ_info.read();
        if let Some(ref o) = *organ {
            o.divisions.iter().enumerate()
                .filter_map(|(i, d)| {
                    en.get(i).map(|&e| library::DivisionCcisSaved {
                        division: d.name.clone(),
                        enabled: e,
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    };
    let ccis_spread = {
        let (strength, falloff, swap) = *state.ccis_spread.read();
        Some(library::CcisSpreadSaved { strength, falloff, swap })
    };

    // Laatste stand: getrokken registratie + actieve koppels (hersteld bij laden)
    let drawn_stops = state.drawn_stops.read().clone();
    let active_couplers = state.get_active_couplers();

    // Generaal-crescendo pedaalbinding (channel/cc/min/max/invert) — persistent
    let crescendo_binding = state.crescendo_binding.read().map(|(ch, cc, mn, mx, inv)| library::CrescendoBindingSaved {
        channel: ch, cc_num: cc, min_val: mn, max_val: mx, invert: inv,
    });

    // Gather current division-wind-group toewijzing
    let division_wind_groups: Vec<library::DivisionWindGroupSaved> = {
        let groups = state.division_wind_groups.read();
        let organ = state.loaded_organ_info.read();
        if let Some(ref o) = *organ {
            o.divisions.iter().enumerate()
                .filter_map(|(i, d)| {
                    groups.get(i).map(|&g| library::DivisionWindGroupSaved {
                        division: d.name.clone(),
                        group: g,
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    };

    let num_wind_groups = *state.num_wind_groups.read();

    // Alle audio-DSP-instellingen worden uit de runtime-mirrors verzameld (gezet door de
    // set_*/persist_*-commands); de reeds opgeslagen waarde dient als fallback wanneer
    // deze sessie nog niets gezet heeft (.or(existing_*)).
    let (existing_master, existing_reverb, existing_eq, existing_temperament) = {
        let lib = state.organ_library.read();
        match lib.settings.get(&organ_id) {
            Some(s) => (s.master_volume_db, s.reverb.clone(), s.eq.clone(), s.temperament.clone()),
            None => (None, None, None, None),
        }
    };
    let master_volume_db = (*state.master_volume_db.read()).or(existing_master);
    let temperament = state.temperament_settings.read().clone().or(existing_temperament);
    let reverb = state.reverb_settings.read().clone().or(existing_reverb);
    let eq = state.eq_settings.read().clone().or(existing_eq);

    // Per-divisie DSP-spiegels → opslag (alleen divisies/groepen die de gebruiker zette;
    // de mirrors zijn per orgel gewist bij load, dus geen naam-collisie tussen orgels).
    let division_pans: Vec<library::DivisionPanSaved> = state.division_pans.read().iter()
        .map(|(division, &pan)| library::DivisionPanSaved { division: division.clone(), pan })
        .collect();
    let division_swell_configs: Vec<library::DivisionSwellConfigSaved> = state.division_swell_configs.read().iter()
        .map(|(division, &(min_db, filter_cutoff))| library::DivisionSwellConfigSaved {
            division: division.clone(), min_db, filter_cutoff,
        })
        .collect();
    let division_tremulants: Vec<library::DivisionTremulantSaved> = state.division_tremulants.read().iter()
        .map(|(division, &(enabled, rate, amp_depth, pitch_depth))| library::DivisionTremulantSaved {
            division: division.clone(), enabled, rate, amp_depth, pitch_depth,
        })
        .collect();
    let wind_group_configs: Vec<library::WindGroupConfigSaved> = state.wind_group_configs.read().iter()
        .map(|(&group, &(enabled, reservoir_size, damping, max_sag))| library::WindGroupConfigSaved {
            group, enabled, reservoir_size, damping, max_sag,
        })
        .collect();

    let settings = OrganSettings {
        presets,
        preset_bindings,
        swell_bindings,
        midi_mappings,
        pipe_voicings,
        division_output_pairs,
        division_wind_groups,
        num_wind_groups,
        master_volume_db,
        reverb,
        eq,
        temperament,
        division_ccis,
        ccis_spread,
        drawn_stops,
        active_couplers,
        crescendo_binding,
        division_pans,
        division_swell_configs,
        division_tremulants,
        wind_group_configs,
    };

    // Save to organ directory as well (.jm-settings.json next to the organ)
    let organ_dir = if organ_id.ends_with(".organ") {
        std::path::Path::new(&organ_id).parent().map(|p| p.to_path_buf())
    } else {
        Some(std::path::PathBuf::from(&organ_id))
    };
    if let Some(dir) = organ_dir {
        let settings_path = dir.join(".jm-settings.json");
        match serde_json::to_string_pretty(&settings) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&settings_path, json) {
                    warn!("Failed to write organ settings to {:?}: {}", settings_path, e);
                } else {
                    info!("Saved organ settings to {:?}", settings_path);
                }
            }
            Err(e) => warn!("Failed to serialize organ settings: {}", e),
        }
    }

    {
        let mut lib = state.organ_library.write();
        lib.settings.insert(organ_id, settings);
    }
    state.save_library();
}

/// Get saved settings for the current organ (from library or organ directory)
#[tauri::command]
pub fn get_organ_settings(state: State<AppState>) -> Option<OrganSettings> {
    let organ_id = state.current_organ_id.read().clone()?;

    // First try library (AppData)
    {
        let lib = state.organ_library.read();
        if let Some(settings) = lib.settings.get(&organ_id) {
            return Some(settings.clone());
        }
    }

    // Fallback: try .jm-settings.json in organ directory
    let organ_dir = if organ_id.ends_with(".organ") {
        std::path::Path::new(&organ_id).parent().map(|p| p.to_path_buf())
    } else {
        Some(std::path::PathBuf::from(&organ_id))
    };
    if let Some(dir) = organ_dir {
        let settings_path = dir.join(".jm-settings.json");
        if let Ok(json) = std::fs::read_to_string(&settings_path) {
            if let Ok(settings) = serde_json::from_str::<OrganSettings>(&json) {
                info!("Loaded organ settings from {:?}", settings_path);
                return Some(settings);
            }
        }
    }

    None
}

/// Export all settings to a JSON file
#[tauri::command]
pub fn export_settings(state: State<AppState>, path: String) -> Result<(), String> {
    let organ_id = state.current_organ_id.read().clone()
        .ok_or("Geen orgel geladen")?;
    let lib = state.organ_library.read();
    let settings = lib.settings.get(&organ_id)
        .ok_or("Geen instellingen gevonden")?;
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Serialisatie fout: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Kan niet schrijven naar {}: {}", path, e))?;
    info!("Exported settings to {}", path);
    Ok(())
}

/// Import settings from a JSON file
#[tauri::command]
pub fn import_settings(state: State<AppState>, path: String) -> Result<OrganSettings, String> {
    let json = std::fs::read_to_string(&path)
        .map_err(|e| format!("Kan niet lezen: {}", e))?;
    let settings: OrganSettings = serde_json::from_str(&json)
        .map_err(|e| format!("Ongeldig formaat: {}", e))?;

    // Save to library
    let organ_id = state.current_organ_id.read().clone()
        .ok_or("Geen orgel geladen")?;
    {
        let mut lib = state.organ_library.write();
        lib.settings.insert(organ_id, settings.clone());
    }
    state.save_library();
    info!("Imported settings from {}", path);
    Ok(settings)
}

#[tauri::command]
pub fn get_organ_image(source_path: String) -> Option<String> {
    // Try to find the image path from the source
    let source_type = if source_path.ends_with(".organ") { "organ_file" } else { "sample_directory" };
    let image_path = library::find_organ_image(&source_path, source_type)?;
    library::image_to_base64(&image_path)
}

#[tauri::command]
pub fn get_saved_presets(state: State<AppState>) -> HashMap<String, PresetData> {
    let organ_id = state.current_organ_id.read().clone();
    let organ_id = match organ_id {
        Some(id) => id,
        None => return HashMap::new(),
    };
    let lib = state.organ_library.read();
    lib.settings.get(&organ_id).map(|s| s.presets.clone()).unwrap_or_default()
}

#[tauri::command]
pub fn restore_preset_bindings(state: State<AppState>, bindings: Vec<PresetBindingSaved>) {
    use crate::state::{MidiPresetBinding, MidiPresetTrigger};

    let midi_bindings: Vec<MidiPresetBinding> = bindings.into_iter().filter_map(|b| {
        let trigger = match b.trigger_type.as_str() {
            "note" => MidiPresetTrigger::Note { note: b.note? },
            "cc" => MidiPresetTrigger::ControlChange {
                channel: b.channel,
                controller: b.controller?,
                value: b.value.unwrap_or(64),
            },
            "program" => MidiPresetTrigger::ProgramChange {
                channel: b.channel,
                program: b.program?,
            },
            _ => return None,
        };
        Some(MidiPresetBinding {
            preset_num: b.preset_num,
            trigger,
        })
    }).collect();
    state.set_preset_bindings_replace(midi_bindings);
}

#[tauri::command]
pub fn restore_swell_bindings(state: State<AppState>, bindings: Vec<SwellBindingSaved>) {
    use crate::state::SwellBinding;

    let swell: Vec<SwellBinding> = bindings.into_iter().map(|b| {
        SwellBinding {
            division_name: b.division_name,
            division_index: b.division_index,
            channel: b.channel,
            cc_num: b.cc_num,
            min_val: b.min_val,
            max_val: b.max_val,
            invert: b.invert,
        }
    }).collect();
    state.set_swell_bindings_replace(swell);
}

// ============ MP3-recorder ============

/// Standaardmap voor opnames: ~/Documents/JM-Orgue-opnames (Windows: Mijn Documenten).
fn recordings_default_dir() -> std::path::PathBuf {
    if let Some(docs) = dirs::document_dir() {
        return docs.join("JM-Orgue-opnames");
    }
    std::path::PathBuf::from("JM-Orgue-opnames")
}

/// Genereer een tijdgestempeld bestandspad voor een nieuwe opname.
/// Formaat: jm-orgue-YYYYMMDD-HHMMSS.mp3 (UTC — zie epoch_to_ymdhms).
fn make_recording_path(custom: Option<String>) -> std::path::PathBuf {
    if let Some(p) = custom { return std::path::PathBuf::from(p); }
    make_timestamped_recording_path("mp3")
}

/// Bouw een tijdgestempeld opnamepad met de gegeven extensie in de standaard opnamemap.
/// Formaat: jm-orgue-YYYYMMDD-HHMMSS.<ext> (UTC — epoch_to_ymdhms doet geen tijdzone).
fn make_timestamped_recording_path(ext: &str) -> std::path::PathBuf {
    // Geen chrono-afhankelijkheid: gebruik epoch-seconden + simpele kalender-conversie.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Converteer naar UTC datetime-componenten (eenvoudige Gregoriaanse conversie).
    let (y, mo, d, h, mi, s) = epoch_to_ymdhms(secs);
    let base = format!("jm-orgue-{:04}{:02}{:02}-{:02}{:02}{:02}", y, mo, d, h, mi, s);
    let dir = recordings_default_dir();
    // Voorkom overschrijven als twee opnames binnen dezelfde seconde starten: voeg een
    // teller toe (-1, -2, ...) als het bestand al bestaat.
    let mut path = dir.join(format!("{}.{}", base, ext));
    let mut counter = 1;
    while path.exists() && counter < 1000 {
        path = dir.join(format!("{}-{}.{}", base, counter, ext));
        counter += 1;
    }
    path
}

/// Zet epoch-seconden om naar (jaar, maand, dag, uur, min, sec) — UTC.
fn epoch_to_ymdhms(epoch: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = (epoch % 60) as u32;
    let m = ((epoch / 60) % 60) as u32;
    let h = ((epoch / 3600) % 24) as u32;
    let mut days = (epoch / 86400) as i64;
    // 1970-01-01 = donderdag, dag 0.
    let mut year = 1970i32;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let dy = if leap { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mdays = [31, if leap {29} else {28}, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0usize;
    while month < 12 && days >= mdays[month] {
        days -= mdays[month];
        month += 1;
    }
    (year as u32, (month + 1) as u32, (days + 1) as u32, h, m, s)
}

/// Start een MP3-opname van de live audio-output. Default-pad indien `path` leeg.
#[tauri::command]
pub fn start_recording(state: State<AppState>, path: Option<String>) -> Result<String, String> {
    let out_path = make_recording_path(path);

    // Sample rate uit de audio-player lezen (anders 48 kHz fallback).
    let sr: u32 = {
        let ap = state.audio_player.read();
        ap.as_ref().map(|p| *p.sample_rate.read()).unwrap_or(48000)
    };

    // Koppel de recorder aan de audio-thread (globaal static, één opname tegelijk).
    *crate::audio::RECORDER.write() = Some(state.recorder.clone());

    state.recorder.start(out_path.clone(), sr)?;
    Ok(out_path.to_string_lossy().to_string())
}

/// Stop de actieve opname, flush + sluit het bestand.
#[tauri::command]
pub fn stop_recording(state: State<AppState>) -> Result<crate::recorder::RecorderStatus, String> {
    state.recorder.stop()?;
    let s = state.recorder.status();
    // Laat de globale RECORDER hangen — een vervolgopname herstart de sender opnieuw.
    Ok(s)
}

/// Status van de huidige (of laatste) opname — voor de UI-poll.
#[tauri::command]
pub fn get_recording_status(state: State<AppState>) -> crate::recorder::RecorderStatus {
    state.recorder.status()
}

/// Zet of de getrokken registratie ("laatste stand") bij orgel-load hersteld wordt.
/// Default = false (schone start). De frontend zet dit bij opstart op basis van een
/// gebruikersinstelling.
#[tauri::command]
pub fn set_restore_registration(state: State<AppState>, enabled: bool) {
    *state.restore_registration.write() = enabled;
}

#[tauri::command]
pub fn get_restore_registration(state: State<AppState>) -> bool {
    *state.restore_registration.read()
}

/// Lees de standaard-opnamemap zodat de UI hem kan tonen / openen.
#[tauri::command]
pub fn get_recordings_dir() -> String {
    recordings_default_dir().to_string_lossy().to_string()
}

// ============ Autostart bij Windows + Computer afsluiten ============

/// Naam waaronder we autostart registreren in HKCU\Software\Microsoft\Windows\CurrentVersion\Run.
const AUTOSTART_VALUE_NAME: &str = "JM-Orgue";

/// Bepaal het pad naar de geïnstalleerde vpo-app.exe (huidige executable).
/// GEEN letterlijke aanhalingstekens toevoegen: std::process::Command quote't de
/// argument zelf correct op de Windows-commandline, zodat reg.exe het BARE pad
/// (inclusief spaties) als registry-waarde opslaat. Zelf quotes toevoegen zou
/// letterlijke aanhalingstekens in de waarde zetten en autostart laten falen.
fn current_exe_path() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.to_string_lossy().to_string())
}

/// Lees of JM-Orgue ingesteld staat om automatisch te starten bij Windows-aanmelding.
#[tauri::command]
pub fn get_autostart_enabled() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("reg")
            .args(["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run", "/v", AUTOSTART_VALUE_NAME])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(output.status.success())
    }
    #[cfg(not(target_os = "windows"))]
    { Ok(false) }
}

/// Zet/wis de autostart-registratie. Op niet-Windows platforms is dit een no-op.
#[tauri::command]
pub fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if enabled {
            let exe = current_exe_path()?;
            let status = std::process::Command::new("reg")
                .args(["add", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                       "/v", AUTOSTART_VALUE_NAME, "/t", "REG_SZ", "/d", &exe, "/f"])
                .status()
                .map_err(|e| e.to_string())?;
            if !status.success() { return Err("reg add failed".to_string()); }
            info!("Autostart enabled: {}", exe);
        } else {
            // Negeer fout als de waarde toch al weg was.
            let _ = std::process::Command::new("reg")
                .args(["delete", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                       "/v", AUTOSTART_VALUE_NAME, "/f"])
                .status();
            info!("Autostart disabled");
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    { let _ = enabled; Err("Autostart is alleen op Windows ondersteund".to_string()) }
}

/// Sluit de software en de computer netjes af. Op Windows: `shutdown /s /t 0`.
/// Houdt rekening met opslaan van de laatste stand: de frontend roept eerst
/// flushAutoSave + persistOrganSettings aan voordat deze command wordt aangeroepen.
#[tauri::command]
pub fn shutdown_computer() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // /s = shutdown, /t 0 = direct, /f = force (sluit apps zonder prompts).
        let status = std::process::Command::new("shutdown")
            .args(["/s", "/t", "0", "/f"])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() { return Err("shutdown command faalde".to_string()); }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    { Err("Computer afsluiten is alleen op Windows ondersteund".to_string()) }
}

#[cfg(test)]
mod smf_tests {
    use super::encode_smf;

    /// Lees een big-endian u32 op offset.
    fn be_u32(d: &[u8], off: usize) -> u32 {
        u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
    }

    #[test]
    fn encodes_valid_smf_header_and_trailer() {
        // Twee noten (on/off) op kanaal 0.
        let events = vec![
            (0u64, 0x90u8, 60u8, 100u8, 0u8),
            (500_000u64, 0x80u8, 60u8, 0u8, 0u8),
        ];
        let data = encode_smf(&events);

        // Header chunk
        assert_eq!(&data[0..4], b"MThd");
        assert_eq!(be_u32(&data, 4), 6); // header length
        assert_eq!(&data[8..10], &[0x00, 0x00]); // format 0
        assert_eq!(&data[10..12], &[0x00, 0x01]); // 1 track
        assert_eq!(&data[12..14], &[0x01, 0xE0]); // 480 PPQ

        // Track chunk
        assert_eq!(&data[14..18], b"MTrk");
        let track_len = be_u32(&data, 18) as usize;
        let track = &data[22..];
        assert_eq!(track.len(), track_len, "MTrk-lengteveld moet kloppen met de werkelijke trackbytes");
        // Eindigt op end-of-track meta-event.
        assert_eq!(&track[track.len() - 3..], &[0xFF, 0x2F, 0x00]);
        // Bevat het NoteOn-statusbyte ergens in de track.
        assert!(track.contains(&0x90));
    }

    #[test]
    fn program_change_omits_second_data_byte() {
        // PC heeft maar één databyte; de writer mag data2 niet meeschrijven.
        let events = vec![(0u64, 0xC0u8, 5u8, 0u8, 0u8)];
        let data = encode_smf(&events);
        let track_len = be_u32(&data, 18) as usize;
        let track = &data[22..22 + track_len];
        // Track: [tempo-meta (7 bytes)] [delta=0 (1)] [0xC0] [05] [eot (4)]
        // Zoek het 0xC0-event en controleer dat het direct gevolgd wordt door 0x05 en daarna de eot-delta.
        let pc_pos = track.iter().position(|&b| b == 0xC0).expect("PC-event aanwezig");
        assert_eq!(track[pc_pos + 1], 0x05);
        // Na de PC (1 databyte) volgt de end-of-track delta (0x00) + 0xFF 0x2F 0x00.
        assert_eq!(&track[pc_pos + 2..pc_pos + 6], &[0x00, 0xFF, 0x2F, 0x00]);
    }
}
