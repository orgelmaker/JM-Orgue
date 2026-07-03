//! Organ library — persistent storage for organ entries and per-organ settings.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// A single organ entry in the library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganLibraryEntry {
    /// Unique ID (= normalized source_path)
    pub id: String,
    /// Display name
    pub name: String,
    /// Builder name
    pub builder: String,
    /// Location
    pub location: String,
    /// Build year
    pub year: Option<String>,
    /// Number of stops
    pub stop_count: usize,
    /// "organ_file" or "sample_directory"
    pub source_type: String,
    /// Absolute path to .organ file or sample directory
    pub source_path: String,
    /// Absolute path to discovered image file, if any
    pub image_path: Option<String>,
}

/// Saved preset data (stops + couplers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetData {
    pub stops: Vec<String>,
    pub couplers: Vec<String>,
}

/// Saved MIDI preset binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetBindingSaved {
    pub preset_num: u8,
    pub trigger_type: String, // "note", "cc", "program"
    pub note: Option<u8>,
    pub channel: Option<u8>,
    pub controller: Option<u8>,
    pub value: Option<u8>,
    pub program: Option<u8>,
}

/// Saved swell binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwellBindingSaved {
    pub division_name: String,
    pub division_index: u8,
    pub channel: u8,
    pub cc_num: u8,
    pub min_val: u8,
    pub max_val: u8,
    #[serde(default)]
    pub invert: bool,
}

/// Saved crescendo pedal binding (channel, cc, min, max, invert)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrescendoBindingSaved {
    pub channel: u8,
    pub cc_num: u8,
    #[serde(default)]
    pub min_val: u8,
    #[serde(default = "default_max_cc")]
    pub max_val: u8,
    #[serde(default)]
    pub invert: bool,
}

fn default_max_cc() -> u8 { 127 }

/// Saved MIDI channel mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiMappingSaved {
    pub division: String,
    pub channel: Option<u8>,
    pub transpose: i8,
    pub first_midi_note: Option<u8>,
    pub last_midi_note: Option<u8>,
}

/// Saved per-pipe voicing (volume + pitch detune)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipeVoicingSaved {
    pub stop_id: u32,
    pub pipe_num: u32,
    pub volume_db: f32,
    pub pitch_cents: f32,
}

/// Saved divisie-naar-output routing. `channels` (nieuw) = vrije lijst fysieke kanalen.
/// `pair` blijft alleen voor migratie van oudere opslag (0=Front,1=Rear,2=Side,3=Aux).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivisionOutputPairSaved {
    pub division: String,
    #[serde(default)]
    pub pair: u8,
    #[serde(default)]
    pub channels: Option<Vec<u8>>,
}

/// C/Cis-lade spreiding aan/uit per divisie
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivisionCcisSaved {
    pub division: String,
    pub enabled: bool,
}

/// Globale C/Cis-spreiding parameters (sterkte, afval-met-toonhoogte, kanten omdraaien)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CcisSpreadSaved {
    pub strength: f32,
    pub falloff: f32,
    pub swap: bool,
}

/// Saved reverb-instellingen (per orgel)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReverbSettingsSaved {
    /// "algorithmic" of "convolution"
    pub reverb_type: String,
    pub mix: f32,
    pub algorithmic_preset: Option<u8>,
    pub rt60: f32,
    pub pre_delay_ms: f32,
    pub damping: f32,
    pub room_size: f32,
    pub ir_path: Option<String>,
}

/// Saved parametric EQ instellingen
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EqSettingsSaved {
    pub enabled: bool,
    pub low_freq: f32,
    pub low_gain: f32,
    pub mid_freq: f32,
    pub mid_gain: f32,
    pub mid_q: f32,
    pub high_freq: f32,
    pub high_gain: f32,
}

/// Saved temperament + fine-tune instellingen
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemperamentSettingsSaved {
    pub name: String,
    pub custom_cents: Option<[f32; 12]>,
    pub fine_tune_cents: f32,
    pub a4_hz: f32,
}

/// Saved divisie-naar-wind-groep toewijzing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivisionWindGroupSaved {
    pub division: String,
    pub group: u8,
}

/// Per-divisie stereo-pan (-1..1, zoals set_division_pan ontvangt).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivisionPanSaved {
    pub division: String,
    pub pan: f32,
}

/// Per-divisie zwelkast-config: min-dB bij gesloten kast + filter-cutoff (Hz) bij gesloten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivisionSwellConfigSaved {
    pub division: String,
    pub min_db: f32,
    pub filter_cutoff: f32,
}

/// Per-divisie tremulant-LFO: beschikbaar (enabled) + rate/amp-depth/pitch-depth.
/// `active` (live aan/uit) wordt NIET bewaard — dat is een live speelstand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivisionTremulantSaved {
    pub division: String,
    pub enabled: bool,
    pub rate: f32,
    pub amp_depth: f32,
    pub pitch_depth: f32,
}

/// Per-wind-groep model-config: aan/uit + reservoir/demping/max-sag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindGroupConfigSaved {
    pub group: u8,
    pub enabled: bool,
    pub reservoir_size: f32,
    pub damping: f32,
    pub max_sag: f32,
}

/// Per-organ saved settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrganSettings {
    pub presets: HashMap<String, PresetData>,
    pub preset_bindings: Vec<PresetBindingSaved>,
    pub swell_bindings: Vec<SwellBindingSaved>,
    pub midi_mappings: Vec<MidiMappingSaved>,
    #[serde(default)]
    pub pipe_voicings: Vec<PipeVoicingSaved>,
    #[serde(default)]
    pub division_output_pairs: Vec<DivisionOutputPairSaved>,
    #[serde(default)]
    pub division_wind_groups: Vec<DivisionWindGroupSaved>,
    /// Aantal actieve wind-groepen (1..8). 0 = niet opgeslagen / default.
    #[serde(default)]
    pub num_wind_groups: u8,
    /// Master volume in dB (0.0 = stilte, getalig negatief = aftrek)
    #[serde(default)]
    pub master_volume_db: Option<f32>,
    /// Reverb instellingen (algoritmisch of IR)
    #[serde(default)]
    pub reverb: Option<ReverbSettingsSaved>,
    /// Parametric EQ instellingen
    #[serde(default)]
    pub eq: Option<EqSettingsSaved>,
    /// Temperament + fine-tune
    #[serde(default)]
    pub temperament: Option<TemperamentSettingsSaved>,
    /// C/Cis-lade spreiding aan/uit per divisie
    #[serde(default)]
    pub division_ccis: Vec<DivisionCcisSaved>,
    /// Globale C/Cis-spreiding parameters
    #[serde(default)]
    pub ccis_spread: Option<CcisSpreadSaved>,
    /// Laatste registratie: getrokken registers ("laatste stand") — hersteld bij laden.
    #[serde(default)]
    pub drawn_stops: Vec<String>,
    /// Laatst actieve koppels — hersteld bij laden.
    #[serde(default)]
    pub active_couplers: Vec<String>,
    /// Generaal-crescendo pedaalbinding (channel, cc, min, max, invert)
    #[serde(default)]
    pub crescendo_binding: Option<CrescendoBindingSaved>,
    /// Per-divisie stereo-pan
    #[serde(default)]
    pub division_pans: Vec<DivisionPanSaved>,
    /// Per-divisie zwelkast-config (min-dB + filter-cutoff)
    #[serde(default)]
    pub division_swell_configs: Vec<DivisionSwellConfigSaved>,
    /// Per-divisie tremulant-LFO (beschikbaar + parameters)
    #[serde(default)]
    pub division_tremulants: Vec<DivisionTremulantSaved>,
    /// Per-wind-groep model-config (reservoir/demping/sag)
    #[serde(default)]
    pub wind_group_configs: Vec<WindGroupConfigSaved>,
}

/// The entire library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganLibrary {
    pub organs: Vec<OrganLibraryEntry>,
    pub settings: HashMap<String, OrganSettings>,
}

impl Default for OrganLibrary {
    fn default() -> Self {
        Self {
            organs: Vec::new(),
            settings: HashMap::new(),
        }
    }
}

/// Load library from JSON file, or create empty if not found
pub fn load_library(app_data_dir: &Path) -> OrganLibrary {
    let path = app_data_dir.join("organ_library.json");
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(json) => {
                match serde_json::from_str(&json) {
                    Ok(lib) => {
                        info!("Loaded organ library from {:?}", path);
                        return lib;
                    }
                    Err(e) => {
                        warn!("Failed to parse organ library: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to read organ library: {}", e);
            }
        }
    }
    info!("Starting with empty organ library");
    OrganLibrary::default()
}

/// Save library to JSON file
pub fn save_library(app_data_dir: &Path, library: &OrganLibrary) {
    let path = app_data_dir.join("organ_library.json");
    match serde_json::to_string_pretty(library) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                warn!("Failed to write organ library: {}", e);
            } else {
                info!("Saved organ library to {:?}", path);
            }
        }
        Err(e) => {
            warn!("Failed to serialize organ library: {}", e);
        }
    }
}

/// Search for an image file in the organ's directory
pub fn find_organ_image(source_path: &str, source_type: &str) -> Option<String> {
    let dir = if source_type == "organ_file" {
        Path::new(source_path).parent()?.to_path_buf()
    } else {
        PathBuf::from(source_path)
    };

    if !dir.exists() {
        return None;
    }

    let image_extensions = ["jpg", "jpeg", "png", "bmp", "webp"];
    let preferred_names = ["image", "cover", "organ", "photo", "picture", "front"];

    // First pass: look for files with preferred names
    for name in &preferred_names {
        for ext in &image_extensions {
            let filename = format!("{}.{}", name, ext);
            let path = dir.join(&filename);
            if path.exists() {
                return Some(path.to_string_lossy().to_string());
            }
            // Also check uppercase
            let filename_upper = format!("{}.{}", name.to_uppercase(), ext.to_uppercase());
            let path = dir.join(&filename_upper);
            if path.exists() {
                return Some(path.to_string_lossy().to_string());
            }
        }
    }

    // Second pass: any image file in the directory
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if image_extensions.contains(&ext_lower.as_str()) {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

/// Read an image file and return it as a base64 data URL
pub fn image_to_base64(path: &str) -> Option<String> {
    use base64::Engine;
    let data = std::fs::read(path).ok()?;
    let ext = Path::new(path)
        .extension()?
        .to_string_lossy()
        .to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
    Some(format!("data:{};base64,{}", mime, b64))
}
