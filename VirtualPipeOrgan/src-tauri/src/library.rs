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
    /// Handmatig gekozen afbeelding (bibliotheekkaart -> "Afbeelding kiezen").
    /// Blokkeert het automatisch zoeken zodat een herlaad de keuze niet
    /// overschrijft. `#[serde(default)]`: bestaande organ_library.json blijft
    /// leesbaar.
    #[serde(default)]
    pub image_manual: bool,
    /// Versie van het zoekalgoritme waarmee voor het laatst naar een
    /// afbeelding is gezocht. Dit is de NEGATIEVE cache: zonder dit veld werd
    /// elke entry zonder afbeelding bij iedere opening van de bibliotheek
    /// (en per extra registervenster) opnieuw diep gescand.
    #[serde(default)]
    pub image_searched: Option<u32>,
}

/// Versie van het zoekalgoritme voor orgelafbeeldingen. Ophogen laat alle
/// entries zonder handmatige keuze opnieuw zoeken (negatieve cache vervalt).
pub const IMAGE_SEARCH_VERSION: u32 = 2;

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
    pub trigger_type: String, // "note", "cc", "program", "sysex", "ccbit"
    pub note: Option<u8>,
    pub channel: Option<u8>,
    pub controller: Option<u8>,
    pub value: Option<u8>,
    pub program: Option<u8>,
    /// System Exclusive-inhoud als hex ("7D 01 04"), leesbaar in het
    /// instellingenbestand. `default` houdt oudere bestanden leesbaar.
    #[serde(default)]
    pub sysex_hex: Option<String>,
    /// Bitnummer 0..6 binnen een CC-waarde (trigger_type "ccbit").
    #[serde(default)]
    pub bit: Option<u8>,
}

/// SysEx-inhoud → "7D 01 04". Eén plek, zodat opslag en UI dezelfde notatie
/// gebruiken.
pub fn sysex_naar_hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
}

/// "7D 01 04" / "7d0104" / "F0 7D 01 F7" → bytes. De omhullende F0/F7 worden
/// weggelaten: de trigger vergelijkt alleen de inhoud.
pub fn hex_naar_sysex(tekst: &str) -> Result<Vec<u8>, String> {
    let schoon: String = tekst.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if schoon.len() % 2 != 0 {
        return Err("Oneven aantal hex-tekens".to_string());
    }
    let mut bytes = Vec::with_capacity(schoon.len() / 2);
    for paar in schoon.as_bytes().chunks(2) {
        let s = std::str::from_utf8(paar).map_err(|e| e.to_string())?;
        bytes.push(u8::from_str_radix(s, 16).map_err(|e| e.to_string())?);
    }
    if bytes.first() == Some(&0xF0) { bytes.remove(0); }
    if bytes.last() == Some(&0xF7) { bytes.pop(); }
    if bytes.is_empty() { return Err("Geen SysEx-inhoud opgegeven".to_string()); }
    Ok(bytes)
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
    /// Laatst ontvangen pedaalstand (ruwe CC-waarde); hersteld bij laden zodat
    /// de zwelkast na een herstart/herlaad niet stilzwijgend vol open staat.
    #[serde(default)]
    pub last_value: Option<u8>,
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
    /// Kort octaaf (C/E) op dit klavier. `default` houdt oudere
    /// instellingenbestanden leesbaar.
    #[serde(default)]
    pub short_octave: bool,
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

/// Eén opgeslagen EQ-band (vrij formaat, GrandOrgue-stijl).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBandSaved {
    pub enabled: bool,
    /// "peak" | "lowpass" | "highpass" | "bandpass" | "lowshelf" | "highshelf"
    pub band_type: String,
    pub freq: f32,
    pub gain_db: f32,
    /// Bandbreedte in octaven
    pub bandwidth: f32,
    /// None = alle kanalen; anders 0-based fysiek uitgangskanaal
    pub channel: Option<u8>,
}

/// Saved EQ instellingen. Nieuw formaat: vrije banden (`bands`). De losse
/// low/mid/high-velden zijn het oude 3-band formaat en blijven bestaan zodat
/// oudere .jm-settings.json-bestanden gemigreerd kunnen worden (bands leeg →
/// migreren uit de legacy-velden gebeurt in de frontend bij het laden).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EqSettingsSaved {
    pub enabled: bool,
    #[serde(default)]
    pub bands: Vec<EqBandSaved>,
    #[serde(default)]
    pub low_freq: f32,
    #[serde(default)]
    pub low_gain: f32,
    #[serde(default)]
    pub mid_freq: f32,
    #[serde(default)]
    pub mid_gain: f32,
    #[serde(default)]
    pub mid_q: f32,
    #[serde(default)]
    pub high_freq: f32,
    #[serde(default)]
    pub high_gain: f32,
}

/// Saved temperament + fine-tune instellingen
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemperamentSettingsSaved {
    pub name: String,
    pub custom_cents: Option<[f32; 12]>,
    pub fine_tune_cents: f32,
    pub a4_hz: f32,
    /// Hertemperen op gemeten pijptoonhoogte (0.7.38). None = bestand van
    /// vóór dit veld → behandelen als Origineel (geen klankverandering).
    #[serde(default)]
    pub retune: Option<bool>,
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

/// Microfoonperspectief (gestapelde ranks/perspectieven, 0.7.38): per label
/// of het geladen wordt (RAM; vraagt herladen) en het live volume in dB.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerspectiveSaved {
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub gain_db: f32,
}

/// Serde-default voor de "toon dit onderdeel"-vlaggen: een oudere
/// .jm-settings.json zonder deze velden toont alles (zoals vóór 0.7.39).
fn default_true() -> bool { true }

/// Indeling van de afstandsbediening (0.7.39): welke onderdelen het externe
/// scherm toont en in welke volgorde. Het HOOFDVENSTER publiceert dit
/// (set_remote_layout) zodat de afstandsbediening dezelfde indeling krijgt als
/// het registreerscherm — de UI-voorkeuren zelf blijven in localStorage van de
/// webview wonen. Elk veld heeft een serde-default zodat bestaande
/// .jm-settings.json-bestanden leesbaar blijven.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteLayoutSaved {
    /// Divisienamen in schermvolgorde (= selectie); leeg = alle divisies in
    /// de volgorde van het orgel.
    #[serde(default)]
    pub divisions: Vec<String>,
    /// Zichtbare koppels (id's, in orgelvolgorde) — precies de koppels die ook
    /// op het orgelscherm staan.
    #[serde(default)]
    pub visible_couplers: Vec<String>,
    /// "bar" (koppelbalk per klavier, standaard) of "division" (koppelknop
    /// tussen de registers van de divisie).
    #[serde(default)]
    pub coupler_placement: String,
    /// Registervolgorde per divisienaam (de gesleepte volgorde van het
    /// orgelscherm); onbekende registers komen achteraan.
    #[serde(default)]
    pub stop_order: HashMap<String, Vec<String>>,
    /// "rect" of "round" (vorm van de registerknoppen).
    #[serde(default)]
    pub knob_shape: String,
    #[serde(default = "default_true")]
    pub show_couplers: bool,
    #[serde(default = "default_true")]
    pub show_tremulant: bool,
    #[serde(default = "default_true")]
    pub show_setzer: bool,
    #[serde(default = "default_true")]
    pub show_volume: bool,
    #[serde(default = "default_true")]
    pub show_panic: bool,
}

impl Default for RemoteLayoutSaved {
    fn default() -> Self {
        Self {
            divisions: Vec::new(),
            visible_couplers: Vec::new(),
            coupler_placement: String::new(),
            stop_order: HashMap::new(),
            knob_shape: String::new(),
            show_couplers: true,
            show_tremulant: true,
            show_setzer: true,
            show_volume: true,
            show_panic: true,
        }
    }
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
    /// Generaal-crescendo matrix: per trap de register-/koppel-IDs (bron van
    /// waarheid sinds 0.7.38; voorheen alleen localStorage van de webview).
    #[serde(default)]
    pub crescendo_stages: Vec<Vec<String>>,
    /// Generaal crescendo ingeschakeld.
    #[serde(default)]
    pub crescendo_enabled: bool,
    /// Aantal kolommen in de crescendo-editor (0 = niet opgeslagen → 15).
    #[serde(default)]
    pub crescendo_num_stages: u8,
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
    /// Microfoonperspectieven: geladen (aan/uit) + volume per label.
    #[serde(default)]
    pub perspectives: Vec<PerspectiveSaved>,
    /// Alle opnameposities in het geheugen houden, ook de uitgeschakelde
    /// (0.7.47). Dan is wisselen tussen posities ogenblikkelijk in plaats van
    /// een herlaad — ten koste van geheugen (ruwweg maal het aantal posities).
    #[serde(default)]
    pub load_all_perspectives: bool,
    /// Koppels die de sampleset zelf niet heeft en die de organist erbij heeft
    /// gezet (0.7.47): de id's uit de door de app afgeleide koppellijst.
    #[serde(default)]
    pub extra_couplers: Vec<String>,
    /// Doorlopend klavier: toetsen buiten het opgenomen bereik spelen de pijp
    /// een octaaf hoger of lager in plaats van te zwijgen (0.7.47).
    #[serde(default)]
    pub continuous_keyboard: bool,
    /// Indeling van de afstandsbediening (0.7.39). None = nooit gepubliceerd →
    /// de afstandsbediening gebruikt dezelfde defaults als het orgelscherm.
    #[serde(default)]
    pub remote_layout: Option<RemoteLayoutSaved>,
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

/// Normaliseer pad-notaties en voeg duplicaten samen die door verschillende
/// notaties zijn ontstaan (forward slashes via de test-API/scripts vs
/// backslashes via de UI). Backslash-entries zijn de originelen en winnen bij
/// een botsing; settings onder een forward-slash-key verhuizen alleen mee als
/// er onder de genormaliseerde key nog niets staat.
fn normalize_library(lib: &mut OrganLibrary) {
    let norm = |s: &str| s.replace('/', "\\");
    let key = |s: &str| norm(s).to_lowercase();

    let organs = std::mem::take(&mut lib.organs);
    let before = organs.len();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let (originals, converted): (Vec<_>, Vec<_>) =
        organs.into_iter().partition(|o| !o.id.contains('/'));
    for mut o in originals.into_iter().chain(converted) {
        if seen.insert(key(&o.id)) {
            o.id = norm(&o.id);
            o.source_path = norm(&o.source_path);
            lib.organs.push(o);
        }
    }
    if lib.organs.len() < before {
        info!(
            "Bibliotheek geschoond: {} dubbele orgel-entries (pad-notatie) samengevoegd",
            before - lib.organs.len()
        );
    }

    // Settings-keys: merge óók hoofdletter-varianten, op de spelling van de
    // bijbehorende bibliotheek-entry (dat is wat current_organ_id wordt bij
    // laden via de bibliotheek) — anders splitsen instellingen zich per
    // pad-notatie (auditbevinding 49).
    let by_lower: std::collections::HashMap<String, String> = lib.organs.iter()
        .map(|o| (o.id.to_lowercase(), o.id.clone()))
        .collect();
    let settings = std::mem::take(&mut lib.settings);
    let (orig, conv): (Vec<_>, Vec<_>) =
        settings.into_iter().partition(|(k, _)| !k.contains('/'));
    for (k, v) in orig.into_iter().chain(conv) {
        let nk = norm(&k);
        let canon = by_lower.get(&nk.to_lowercase()).cloned().unwrap_or(nk);
        lib.settings.entry(canon).or_insert(v);
    }
}

/// Load library from JSON file, or create empty if not found
pub fn load_library(app_data_dir: &Path) -> OrganLibrary {
    let path = app_data_dir.join("organ_library.json");
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(json) => {
                match serde_json::from_str::<OrganLibrary>(&json) {
                    Ok(mut lib) => {
                        info!("Loaded organ library from {:?}", path);
                        normalize_library(&mut lib);
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

/// Bestandsextensies die we als afbeelding accepteren (ook in de
/// bestandsdialoog van "Afbeelding kiezen").
pub const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "bmp", "webp"];

/// Bestandsnamen die zo goed als zeker de orgelfoto zijn (map van de set).
const PREFERRED_NAMES: [&str; 13] = [
    "image", "cover", "organ", "orgel", "orgue", "photo", "foto", "picture",
    "front", "console", "church", "kerk", "main",
];

/// Naamdelen die in de diepe scan op een orgel-/consolefoto wijzen.
const NAME_HINTS: [&str; 14] = [
    "console", "organ", "orgel", "orgue", "church", "kerk", "front", "main",
    "background", "photo", "foto", "cover", "image", "picture",
];

/// Ondergrens waaronder een afbeelding zeker geen orgelfoto is maar een
/// knop-/toetsbitmap. Bestandsgrootte alleen werkt NIET: de echte foto PH.jpg
/// (24,7 kB) is even groot als een registerknop-bitmap. Afmeting wel:
/// PH.jpg = 278x338 (93 964 px), een koppelknop 100x99 (9 900 px).
const MIN_IMAGE_W: u32 = 200;
const MIN_IMAGE_H: u32 = 200;
const MIN_IMAGE_AREA: u64 = 60_000;

/// Bovengrens: een paneelachtergrond gaat als base64 (+33%) naar de UI, dus
/// bestanden daarboven slaan we over (SJdL heeft een 8 MB-achtergrond).
const MAX_IMAGE_BYTES: u64 = 12 * 1024 * 1024;

/// Budgetten voor de diepe scan (grote sets hebben honderden bitmaps).
const SCAN_MAX_DEPTH: usize = 4;
const SCAN_MAX_ENTRIES: usize = 20_000;
const SCAN_MAX_PROBES: usize = 1_500;

/// Afmetingen van een afbeelding uit de bestandskop (geen image-crate in dit
/// project). Ondersteunt PNG, JPEG, BMP en WebP; onbekend formaat -> None.
pub fn image_dimensions(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut b = vec![0u8; 65_536];
    let n = f.read(&mut b).ok()?;
    b.truncate(n);
    let be16 = |i: usize| -> Option<u32> { Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32) };
    let le16 = |i: usize| -> Option<u32> { Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]) as u32) };
    let be32 = |i: usize| -> Option<u32> {
        Some(u32::from_be_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
    };
    let le32 = |i: usize| -> Option<u32> {
        Some(u32::from_le_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
    };

    // PNG: IHDR staat altijd direct achter de 8-byte signatuur.
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some((be32(16)?, be32(20)?));
    }
    // BMP: BITMAPINFOHEADER, breedte/hoogte als i32 (hoogte kan negatief zijn).
    if b.starts_with(b"BM") {
        let w = le32(18)? as i32;
        let h = le32(22)? as i32;
        return Some((w.unsigned_abs(), h.unsigned_abs()));
    }
    // JPEG: doorloop de markers tot een SOFn-frame.
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2usize;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let m = b[i + 1];
            if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) {
                i += 2;
                continue;
            }
            let len = be16(i + 2)? as usize;
            let is_sof = (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC;
            if is_sof {
                return Some((be16(i + 7)?, be16(i + 5)?));
            }
            if len < 2 {
                return None;
            }
            i += 2 + len;
        }
        return None;
    }
    // WebP: RIFF-container met VP8X (uitgebreid), VP8 (lossy) of VP8L.
    if b.starts_with(b"RIFF") && b.len() > 30 && &b[8..12] == b"WEBP" {
        match &b[12..16] {
            b"VP8X" => {
                let w = 1 + (*b.get(24)? as u32 | (*b.get(25)? as u32) << 8 | (*b.get(26)? as u32) << 16);
                let h = 1 + (*b.get(27)? as u32 | (*b.get(28)? as u32) << 8 | (*b.get(29)? as u32) << 16);
                return Some((w, h));
            }
            b"VP8 " => {
                // Keyframe-header: 3 bytes frame-tag, dan 9D 01 2A, dan 2x u16.
                let w = le16(26)? & 0x3FFF;
                let h = le16(28)? & 0x3FFF;
                return Some((w, h));
            }
            b"VP8L" => {
                let bits = le32(21)?;
                return Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1));
            }
            _ => return None,
        }
    }
    None
}

/// Pad met één soort scheidingsteken. Het ODF-pad komt met `/` binnen
/// (`resolve_path`-normalisatie) terwijl de rest van de bibliotheek backslashes
/// gebruikt; gemengd werkt wel, maar leest en vergelijkt slecht.
fn tidy_path(p: &Path) -> String {
    let s = p.to_string_lossy().to_string();
    if std::path::MAIN_SEPARATOR == '\\' { s.replace('/', "\\") } else { s }
}

/// Heeft dit pad een afbeeldingsextensie die we ondersteunen?
pub fn has_image_extension(path: &Path) -> bool {
    match path.extension() {
        Some(e) => IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()),
        None => false,
    }
}

/// Bruikbaar als kaartafbeelding: bestaat, juiste extensie, niet te groot en
/// groot genoeg in pixels (scheidt foto's van knop-/toetsbitmaps).
fn usable_image(path: &Path) -> bool {
    if !has_image_extension(path) {
        return false;
    }
    let meta = match std::fs::metadata(path) {
        Ok(m) if m.is_file() => m,
        _ => return false,
    };
    if meta.len() == 0 || meta.len() > MAX_IMAGE_BYTES {
        return false;
    }
    match image_dimensions(path) {
        Some((w, h)) => w >= MIN_IMAGE_W && h >= MIN_IMAGE_H && (w as u64) * (h as u64) >= MIN_IMAGE_AREA,
        // Onleesbare kop: niet afkeuren op een formaat dat we niet kennen.
        None => true,
    }
}

/// Stam zonder extensie, kleine letters.
fn stem_lower(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Is dit een "orgel.jpg"-achtige naam: precies een voorkeursnaam, eventueel
/// met een cijfer/streepje erachter (orgel2.jpg, console-1.png)?
fn preferred_rank(stem: &str) -> Option<usize> {
    PREFERRED_NAMES.iter().position(|n| {
        stem == *n
            || (stem.starts_with(n)
                && stem[n.len()..]
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '_' || c == '-' || c == ' '))
    })
}

/// Voorkeursnaam-afbeelding direct in een map (niet recursief,
/// hoofdletter-ongevoelig via read_dir).
fn preferred_in_dir(dir: &Path) -> Option<String> {
    let mut best: Option<(usize, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if let Some(rank) = preferred_rank(&stem_lower(&p)) {
            if usable_image(&p) && best.as_ref().map(|(r, _)| rank < *r).unwrap_or(true) {
                best = Some((rank, p));
            }
        }
    }
    best.map(|(_, p)| p.to_string_lossy().to_string())
}

/// Beste afbeelding direct in een map: voorkeursnaam wint, anders de grootste
/// bruikbare afbeelding (pakketroot van een Hauptwerk-set: PH.jpg).
fn best_in_dir(dir: &Path) -> Option<String> {
    if let Some(p) = preferred_in_dir(dir) {
        return Some(p);
    }
    let mut best: Option<(u64, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if !usable_image(&p) {
            continue;
        }
        let area = image_dimensions(&p).map(|(w, h)| (w as u64) * (h as u64)).unwrap_or(0);
        if best.as_ref().map(|(a, _)| area > *a).unwrap_or(true) {
            best = Some((area, p));
        }
    }
    best.map(|(_, p)| p.to_string_lossy().to_string())
}

/// Hauptwerk: de pakketmappen onder `OrganInstallationPackages`.
fn hauptwerk_package_dirs(odf_path: &Path) -> Vec<PathBuf> {
    let root = match vpo_sampler::find_package_root(odf_path) {
        Some(r) => r,
        None => return Vec::new(),
    };
    match std::fs::read_dir(root.join("OrganInstallationPackages")) {
        Ok(rd) => rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Gescoorde scan in de breedte (ondiepe mappen eerst) over meerdere wortels.
/// Score: naamtreffer > mapnaam-treffer > ondiep; gelijk -> grootste vlak,
/// daarna het kleinste bestand (Images70 boven FullImageResolution).
fn scan_scored(roots: &[PathBuf]) -> Option<String> {
    use std::collections::{HashSet, VecDeque};
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
    for r in roots {
        if r.is_dir() && seen.insert(r.to_string_lossy().to_lowercase()) {
            queue.push_back((r.clone(), 0));
        }
    }
    let mut entries = 0usize;
    let mut probes = 0usize;
    // (score, vlak, -bestandsgrootte) -> pad
    let mut best: Option<((i64, u64, i64), String)> = None;

    while let Some((dir, depth)) = queue.pop_front() {
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        let dir_name = dir.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let dir_bonus = if dir_name.contains("image") || dir_name.contains("console") || dir_name.contains("organinfo") {
            4
        } else {
            0
        };
        for e in rd.flatten() {
            entries += 1;
            if entries > SCAN_MAX_ENTRIES || probes > SCAN_MAX_PROBES {
                break;
            }
            let p = e.path();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                if depth + 1 <= SCAN_MAX_DEPTH && seen.insert(p.to_string_lossy().to_lowercase()) {
                    queue.push_back((p, depth + 1));
                }
                continue;
            }
            if !has_image_extension(&p) {
                continue;
            }
            let size = match std::fs::metadata(&p) {
                Ok(m) if m.len() > 0 && m.len() <= MAX_IMAGE_BYTES => m.len(),
                _ => continue,
            };
            probes += 1;
            let (w, h) = match image_dimensions(&p) {
                Some(d) => d,
                None => continue,
            };
            let area = (w as u64) * (h as u64);
            if w < MIN_IMAGE_W || h < MIN_IMAGE_H || area < MIN_IMAGE_AREA {
                continue;
            }
            let stem = stem_lower(&p);
            let mut score: i64 = dir_bonus - depth as i64;
            if preferred_rank(&stem).is_some() {
                score += 20;
            } else if NAME_HINTS.iter().any(|n| stem.contains(n)) {
                score += 10;
            }
            let key = (score, area, -(size as i64));
            if best.as_ref().map(|(k, _)| key > *k).unwrap_or(true) {
                best = Some((key, p.to_string_lossy().to_string()));
            }
        }
        if entries > SCAN_MAX_ENTRIES || probes > SCAN_MAX_PROBES {
            break;
        }
    }
    best.map(|(_, p)| p)
}

/// Zoek de afbeelding voor een bibliotheekkaart (zoekversie
/// `IMAGE_SEARCH_VERSION`). Gelaagd, stopt bij de eerste treffer:
/// (A) voorkeursnaam in de map van de set zelf (orgel.jpg, console.png, ...);
/// (B) de definitie zelf: GrandOrgue `[Image001] Image=` (paneelachtergrond),
///     Hauptwerk de afbeelding in de pakketroot (PH.jpg);
/// (C) gescoorde scan tot diepte 4 (GO bewaart beelden in `Data - X/Images..`,
///     Hauptwerk in `<pakket>/Images/`), met een minimumafmeting zodat
///     knop-/toetsbitmaps afvallen.
pub fn find_organ_image(source_path: &str, source_type: &str) -> Option<String> {
    let is_file = source_type == "organ_file";
    let src = Path::new(source_path);
    let dir = if is_file { src.parent()?.to_path_buf() } else { src.to_path_buf() };
    if !dir.is_dir() {
        return None;
    }

    // (A) map van de set zelf.
    if let Some(p) = preferred_in_dir(&dir) {
        return Some(p);
    }

    // (B) wat de definitie zelf aanwijst.
    let mut roots: Vec<PathBuf> = vec![dir.clone()];
    if is_file {
        if vpo_sampler::is_hauptwerk_path(src) {
            let pkgs = hauptwerk_package_dirs(src);
            for pkg in &pkgs {
                if let Some(p) = best_in_dir(pkg) {
                    return Some(p);
                }
            }
            roots.extend(pkgs);
        } else if let Some(p) = vpo_sampler::panel_image_path(src) {
            if usable_image(&p) {
                return Some(tidy_path(&p));
            }
        }
    }

    // (C) gescoorde scan.
    scan_scored(&roots)
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

#[cfg(test)]
mod library_tests {
    use super::*;

    fn entry(id: &str) -> OrganLibraryEntry {
        OrganLibraryEntry {
            id: id.to_string(),
            name: "Test".into(),
            builder: String::new(),
            location: String::new(),
            year: None,
            stop_count: 1,
            source_type: "sample_directory".into(),
            source_path: id.to_string(),
            image_path: None,
            image_manual: false,
            image_searched: None,
        }
    }

    #[test]
    fn test_normalize_library_dedupliceert_padnotaties() {
        // Duplicaten door forward vs backslash-notatie moeten samenvouwen;
        // de backslash-variant (UI-origineel) wint, incl. zijn settings.
        let mut lib = OrganLibrary::default();
        lib.organs.push(entry("C:/Orgels/Batz"));
        lib.organs.push(entry(r"C:\Orgels\Batz"));
        lib.organs.push(entry("C:/Orgels/Uniek"));
        lib.settings.insert(r"C:\Orgels\Batz".into(), OrganSettings { master_volume_db: Some(-3.0), ..Default::default() });
        lib.settings.insert("C:/Orgels/Batz".into(), OrganSettings { master_volume_db: Some(-9.9), ..Default::default() });

        normalize_library(&mut lib);

        assert_eq!(lib.organs.len(), 2, "duplicaat samengevoegd, unieke blijft");
        assert!(lib.organs.iter().all(|o| !o.id.contains('/')), "alle ids op backslash-vorm");
        assert_eq!(lib.settings.len(), 1);
        assert_eq!(
            lib.settings.get(r"C:\Orgels\Batz").and_then(|s| s.master_volume_db),
            Some(-3.0),
            "settings van de UI-variant winnen"
        );
    }

    // ---- Afbeelding-zoektocht v2 ----

    /// Minimale, geldige PNG-kop met de gevraagde afmetingen (de zoektocht
    /// leest alleen IHDR, dus de rest hoeft geen echte pixeldata te zijn).
    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
        v
    }

    fn jpeg_bytes(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08];
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        v
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn test_image_dimensions_png_en_jpeg() {
        let d = tmp("jm_imgdim");
        let p = d.join("a.png");
        std::fs::write(&p, png_bytes(1344, 896)).unwrap();
        assert_eq!(image_dimensions(&p), Some((1344, 896)));
        let j = d.join("b.jpg");
        std::fs::write(&j, jpeg_bytes(278, 338)).unwrap();
        assert_eq!(image_dimensions(&j), Some((278, 338)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_find_image_knopbitmaps_vallen_af() {
        // Alleen kleine knop-bitmaps (100x99) -> geen kaartafbeelding.
        let d = tmp("jm_img_knoppen");
        std::fs::create_dir_all(d.join("Images")).unwrap();
        for n in ["Coupler-1.png", "Stop-3.png"] {
            std::fs::write(d.join("Images").join(n), png_bytes(100, 99)).unwrap();
        }
        assert_eq!(find_organ_image(&d.to_string_lossy(), "sample_directory"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_find_image_vindt_achtergrond_diep() {
        // Achtergrond op diepte 3, zoals Friesach (Data - X/Images/Console/0).
        let d = tmp("jm_img_diep");
        let deep = d.join("Data - X").join("Images60").join("Console");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Background.png"), png_bytes(1152, 648)).unwrap();
        std::fs::write(d.join("Data - X").join("Images60").join("Knop.png"), png_bytes(60, 60)).unwrap();
        let got = find_organ_image(&d.to_string_lossy(), "sample_directory").unwrap();
        assert!(got.ends_with("Background.png"), "kreeg {}", got);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_find_image_root_wint_en_is_hoofdletter_ongevoelig() {
        // "Orgel.JPG" in de setmap wint van een diepe paneelachtergrond.
        let d = tmp("jm_img_root");
        let deep = d.join("Images");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Background.png"), png_bytes(1600, 974)).unwrap();
        std::fs::write(d.join("Orgel.JPG"), jpeg_bytes(459, 500)).unwrap();
        let got = find_organ_image(&d.to_string_lossy(), "sample_directory").unwrap();
        assert!(got.ends_with("Orgel.JPG"), "kreeg {}", got);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_find_image_odf_paneelverwijzing() {
        // GrandOrgue: [Image001] wijst de console-achtergrond aan; die ligt
        // niet in de setmap en heeft geen voorkeursnaam.
        let d = tmp("jm_img_odf");
        let sub = d.join("Data - Y").join("Images70");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("Achtergrond1.png"), png_bytes(1344, 896)).unwrap();
        let odf = d.join("Test.organ");
        std::fs::write(
            &odf,
            b"[Image001]\r\nImage=Data - Y\\Images70\\Achtergrond1.png\r\n\r\n[Organ]\r\nChurchName=Y\r\n".to_vec(),
        )
        .unwrap();
        let got = find_organ_image(&odf.to_string_lossy(), "organ_file").unwrap();
        assert!(got.ends_with("Achtergrond1.png"), "kreeg {}", got);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_find_image_te_groot_bestand_wordt_overgeslagen() {
        let d = tmp("jm_img_groot");
        let mut big = png_bytes(2000, 1500);
        big.resize((MAX_IMAGE_BYTES + 1024) as usize, 0);
        std::fs::write(d.join("console.png"), big).unwrap();
        assert_eq!(find_organ_image(&d.to_string_lossy(), "sample_directory"), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod sysex_hex_tests {
    use super::{hex_naar_sysex, sysex_naar_hex};

    #[test]
    fn heen_en_terug() {
        assert_eq!(sysex_naar_hex(&[0x7D, 0x01, 0x04]), "7D 01 04");
        assert_eq!(hex_naar_sysex("7D 01 04").unwrap(), vec![0x7D, 0x01, 0x04]);
    }

    #[test]
    fn scheidingstekens_en_kleine_letters_mogen() {
        assert_eq!(hex_naar_sysex("7d-01:04").unwrap(), vec![0x7D, 0x01, 0x04]);
        assert_eq!(hex_naar_sysex("7d0104").unwrap(), vec![0x7D, 0x01, 0x04]);
    }

    #[test]
    fn omhullende_f0_en_f7_worden_weggelaten() {
        // De trigger vergelijkt alleen de INHOUD; plakt iemand het hele bericht,
        // dan moet dat evengoed werken.
        assert_eq!(hex_naar_sysex("F0 7D 01 04 F7").unwrap(), vec![0x7D, 0x01, 0x04]);
    }

    #[test]
    fn onzin_geeft_een_nette_fout() {
        assert!(hex_naar_sysex("7D 0").is_err());   // oneven
        assert!(hex_naar_sysex("").is_err());        // leeg
        assert!(hex_naar_sysex("F0 F7").is_err());   // geen inhoud
    }
}
