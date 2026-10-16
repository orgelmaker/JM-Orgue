//! Akoestische orgelconfiguratie (0.8.92): optioneel `JM-ACOUSTICS.json`
//! naast de `.organ`, aangekondigd via de sectie `[JMOrgueAcoustics]`. Het
//! beschrijft wat de gewone definitie niet kan uitdrukken: zwelkasten per
//! rank, audiogroepen, impulsresponsen, mengroutes, uitgangsbussen en
//! effectschakelaars met aparte aan-/uitopnamen.
//!
//! Neutraal contract: `format = "jm-orgue.acoustics"`, `schema_version = 1`.
//! De oudere identificatie `"jm-orgue.swop-acoustics"` (exporter 1.3/1.4)
//! wordt als hetzelfde model gelezen (adapter). Hier: lezen, valideren en
//! samenvatten. Geen DSP-activering zonder bekend modelprofiel: de
//! zwelkasten per rank worden wél aangesloten (kastidentiteit op de
//! pijpbinding, eigen trede per kast), de ruimtelijke routes nog niet.
//! Onbekende bronvelden (`source_fields`) blijven als ondoorzichtige JSON
//! bewaard en worden niet geïnterpreteerd.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Bovengrens voor het JSON-bestand (plan §2).
pub const MAX_JSON_BYTES: u64 = 64 * 1024 * 1024;
pub const FORMAT_NEUTRAL: &str = "jm-orgue.acoustics";
pub const FORMAT_LEGACY_SWOP: &str = "jm-orgue.swop-acoustics";
pub const SCHEMA_VERSION: u32 = 1;
/// Zwelkast-indices in `SetDivisionGain`/`SwellBinding` vanaf hier zijn
/// kastslots (index − 128) in plaats van divisies. De engine vertaalt ze naar
/// zijn kastbussen; zonder geregistreerde kasten worden ze genegeerd.
pub const ENCLOSURE_INDEX_BASE: u8 = 128;
/// Hoogstens zoveel kasten krijgen een eigen bus (32 bussen − divisies).
pub const MAX_ENCLOSURES: usize = 16;
/// Naam van de zwelkoppeling van een kast in `SwellBinding.division_name`.
pub const ENCLOSURE_NAME_PREFIX: &str = "enclosure:";

/// De verwijzing uit de `.organ` (`[JMOrgueAcoustics]`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AcousticsRef {
    pub format: String,
    pub schema_version: u32,
    pub configuration_file: String,
    pub activation: String,
}

impl AcousticsRef {
    /// Leest de sectie uit het orgelbestand; None = geen sectie (gewoon orgel).
    pub fn from_odf(odf_path: &Path) -> Option<Self> {
        let bytes = std::fs::read(odf_path).ok()?;
        Self::from_odf_text(&String::from_utf8_lossy(&bytes))
    }

    pub fn from_odf_text(text: &str) -> Option<Self> {
        let mut in_sectie = false;
        let mut gevonden = false;
        let mut r = AcousticsRef::default();
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with(';') { continue; }
            if l.starts_with('[') && l.ends_with(']') {
                in_sectie = l[1..l.len() - 1].eq_ignore_ascii_case("JMOrgueAcoustics");
                gevonden |= in_sectie;
                continue;
            }
            if !in_sectie { continue; }
            if let Some((k, v)) = l.split_once('=') {
                let v = v.trim();
                match k.trim().to_ascii_lowercase().as_str() {
                    "format" => r.format = v.to_string(),
                    "schemaversion" => r.schema_version = v.parse().unwrap_or(0),
                    "configurationfile" => r.configuration_file = v.to_string(),
                    "activation" => r.activation = v.to_string(),
                    _ => {}
                }
            }
        }
        if !gevonden { return None; }
        if r.configuration_file.is_empty() { r.configuration_file = "JM-ACOUSTICS.json".to_string(); }
        Some(r)
    }
}

// ───────────────────────── Getypeerd model ─────────────────────────

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Enclosure { pub id: String, pub name: String, pub source_fields: Value }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Rank {
    pub id: String,
    pub name: String,
    pub enclosure_id: Option<String>,
    pub audio_group_id: Option<String>,
    pub source_fields: Value,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SliderTarget { pub enclosure_id: Option<String>, pub source_fields: Value }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Slider { pub id: String, pub name: String, pub enclosure_targets: Vec<SliderTarget>, pub source_fields: Value }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct AudioGroup { pub id: String, pub name: String, pub source_gain_db: Option<f32>, pub source_fields: Value }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct IrWav {
    pub channels: u16,
    pub sample_rate_hz: u32,
    pub bits_per_sample: u16,
    pub format_tag: u16,
    pub frames: u64,
    pub channel_roles: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct ImpulseResponse {
    pub id: String,
    pub name: String,
    pub path: String,
    pub source_path: String,
    pub source_gain_db: Option<f32>,
    pub wav: IrWav,
    pub source_fields: Value,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct MixInput { pub audio_group_id: Option<String>, pub source_fields: Value }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct MixGroup {
    pub id: String,
    pub name: String,
    pub impulse_response_id: Option<String>,
    pub inputs: Vec<MixInput>,
    pub source_fields: Value,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct OutputInput { pub mix_group_id: Option<String>, pub source_fields: Value }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Output { pub id: String, pub name: String, pub inputs: Vec<OutputInput>, pub source_fields: Value }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SoundAsset {
    pub id: String,
    pub name: String,
    pub path: String,
    pub source_path: String,
    pub loop_regions_inclusive: Value,
    pub source_fields: Value,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SoundSwitch {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub on_sound_ids: Vec<String>,
    pub off_sound_ids: Vec<String>,
    pub original_transition_model: String,
    pub source_fields: Value,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct StopRow {
    pub source_id: String,
    pub source_name: String,
    pub stop_section: String,
    pub manual_section: String,
    pub source_fields: Value,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PipeBinding {
    pub stop_section: String,
    pub pipe_section: String,
    pub layer_index: u32,
    pub logical_pipe: u64,
    pub keyboard_midi_note: u64,
    pub sample_midi_note: u64,
    pub source_rank_id: String,
    pub attack_path: String,
    pub trem_attack_path: Option<String>,
    pub baked_audio_group_gain_db: f32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Unresolved { pub owner_kind: String, pub owner_id: String, pub relation: String, pub target_id: String }

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Activation {
    pub mode: String,
    pub automatic_dsp_activation: bool,
    pub fallback: String,
    pub unverified: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Compatibility {
    pub audio_group_gain_already_in_organ_pipe_gain: bool,
    pub swell_processing_in_organ: bool,
    pub spatial_processing_in_organ: bool,
    pub effect_on_loops_in_organ: bool,
    pub effect_off_samples_in_organ: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Source {
    pub package_id: String,
    pub organ_name: String,
    pub organ_definition_md5: String,
    pub metadata_path: String,
    pub metadata_md5: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct AcousticsConfig {
    pub format: String,
    pub schema_version: u32,
    pub definition_revision: u32,
    pub source: Source,
    pub activation: Activation,
    pub compatibility: Compatibility,
    pub summary: Value,
    pub enclosures: Vec<Enclosure>,
    pub ranks: Vec<Rank>,
    pub sliders: Vec<Slider>,
    pub audio_groups: Vec<AudioGroup>,
    pub impulse_responses: Vec<ImpulseResponse>,
    pub mix_groups: Vec<MixGroup>,
    pub outputs: Vec<Output>,
    pub sound_assets: Vec<SoundAsset>,
    pub sound_switches: Vec<SoundSwitch>,
    pub stops: Vec<StopRow>,
    pub pipe_bindings: Vec<PipeBinding>,
    pub unresolved_references: Vec<Unresolved>,
    /// Map van het orgel (niet uit de JSON; gezet bij het laden) — basis voor
    /// alle relatieve audiopaden.
    #[serde(skip)]
    pub organ_dir: PathBuf,
}

// ───────────────────────── Validatie ─────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Validatie {
    /// Essentiële fouten: het model wordt niet actief (terugval op de .organ).
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Pijpbindingen die op een bestaande stop/pijp van de geladen definitie
    /// wijzen, resp. niet.
    pub bound_pipes: usize,
    pub unbound_pipes: usize,
}

/// Parse `StopNNN` / `PipeNNN` / `ManualNNN` naar het getal.
pub fn sectie_nummer(naam: &str, prefix: &str) -> Option<u32> {
    let rest = naam.strip_prefix(prefix)?;
    rest.parse::<u32>().ok()
}

/// Relatief pad binnen de orgelmap: geen absolute paden, geen `..`, en na
/// het oplossen van symlinks nog steeds binnen de map (plan §2).
pub fn veilig_pad(organ_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    if rel.trim().is_empty() { return Err("leeg pad".into()); }
    let p = Path::new(rel);
    if p.is_absolute() || rel.starts_with('/') || rel.starts_with('\\')
        || p.components().next().map_or(false, |c| matches!(c, std::path::Component::Prefix(_)))
    {
        return Err(format!("absoluut pad niet toegestaan: {}", rel));
    }
    if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(format!("pad buiten de orgelmap: {}", rel));
    }
    let vol = organ_dir.join(p);
    if vol.exists() {
        if let (Ok(a), Ok(b)) = (vol.canonicalize(), organ_dir.canonicalize()) {
            if !a.starts_with(&b) {
                return Err(format!("pad wijst (via koppeling) buiten de orgelmap: {}", rel));
            }
        }
    }
    Ok(vol)
}

fn uniek<'a>(ids: impl Iterator<Item = &'a str>, soort: &str, errors: &mut Vec<String>) -> HashSet<String> {
    let mut set = HashSet::new();
    for id in ids {
        if id.is_empty() {
            errors.push(format!("{}: lege ID", soort));
        } else if !set.insert(id.to_string()) {
            errors.push(format!("{}: dubbele ID {}", soort, id));
        }
    }
    set
}

fn check_ref(waarde: &Option<String>, bekend: &HashSet<String>, wat: &str, errors: &mut Vec<String>) {
    if let Some(v) = waarde {
        if !bekend.contains(v) { errors.push(format!("{} verwijst naar onbekende {}", wat, v)); }
    }
}

/// Valideer tegen schema-afspraken én de werkelijk geladen definitie:
/// `odf_stops` = per stop (ODF-nummer, aantal pijpen per laag).
pub fn valideer(cfg: &AcousticsConfig, odf_stops: &HashMap<u32, Vec<usize>>) -> Validatie {
    let mut v = Validatie::default();
    let e = &mut v.errors;
    if cfg.format != FORMAT_NEUTRAL && cfg.format != FORMAT_LEGACY_SWOP {
        e.push(format!("onbekend formaat '{}' (verwacht {})", cfg.format, FORMAT_NEUTRAL));
    }
    if cfg.schema_version != SCHEMA_VERSION {
        e.push(format!("onbekende schemaversie {} (ondersteund: {})", cfg.schema_version, SCHEMA_VERSION));
    }
    if !e.is_empty() { return v; }

    let enc = uniek(cfg.enclosures.iter().map(|x| x.id.as_str()), "zwelkast", e);
    let ranks = uniek(cfg.ranks.iter().map(|x| x.id.as_str()), "rank", e);
    let groups = uniek(cfg.audio_groups.iter().map(|x| x.id.as_str()), "audiogroep", e);
    let irs = uniek(cfg.impulse_responses.iter().map(|x| x.id.as_str()), "impulsrespons", e);
    let mixes = uniek(cfg.mix_groups.iter().map(|x| x.id.as_str()), "mengroute", e);
    let _outs = uniek(cfg.outputs.iter().map(|x| x.id.as_str()), "uitgang", e);
    let _sliders = uniek(cfg.sliders.iter().map(|x| x.id.as_str()), "zweltrede", e);
    let sounds = uniek(cfg.sound_assets.iter().map(|x| x.id.as_str()), "geluid", e);
    let _switches = uniek(cfg.sound_switches.iter().map(|x| x.id.as_str()), "schakelaar", e);

    for r in &cfg.ranks {
        check_ref(&r.enclosure_id, &enc, &format!("rank {}", r.id), e);
        check_ref(&r.audio_group_id, &groups, &format!("rank {}", r.id), e);
    }
    for s in &cfg.sliders {
        for t in &s.enclosure_targets { check_ref(&t.enclosure_id, &enc, &format!("zweltrede {}", s.id), e); }
    }
    for m in &cfg.mix_groups {
        check_ref(&m.impulse_response_id, &irs, &format!("mengroute {}", m.id), e);
        for i in &m.inputs { check_ref(&i.audio_group_id, &groups, &format!("mengroute {}", m.id), e); }
    }
    for o in &cfg.outputs {
        for i in &o.inputs { check_ref(&i.mix_group_id, &mixes, &format!("uitgang {}", o.id), e); }
    }
    if cfg.enclosures.len() > MAX_ENCLOSURES {
        v.warnings.push(format!("{} zwelkasten; alleen de eerste {} krijgen een eigen trede", cfg.enclosures.len(), MAX_ENCLOSURES));
    }
    // Schakelgeluiden: ontbrekende optionele geluiden zijn een waarschuwing
    // (plan §2), mits de export ze zelf als onopgelost heeft gemeld.
    let gemeld: HashSet<(String, String)> = cfg.unresolved_references.iter()
        .map(|u| (u.owner_id.clone(), u.target_id.clone())).collect();
    let mut stil = 0usize;
    for sw in &cfg.sound_switches {
        for sid in sw.on_sound_ids.iter().chain(sw.off_sound_ids.iter()) {
            if !sounds.contains(sid) {
                stil += 1;
                if !gemeld.contains(&(sw.id.clone(), sid.clone())) {
                    v.warnings.push(format!("schakelaar '{}' verwijst naar ontbrekend geluid {} (niet als onopgelost gemeld)", sw.name, sid));
                }
            }
        }
    }
    if stil > 0 { v.warnings.push(format!("{} schakelverwijzingen naar ontbrekende geluiden blijven stil", stil)); }
    // Audiopaden: relatief en binnen de orgelmap; IR-bestanden moeten bestaan.
    for ir in &cfg.impulse_responses {
        match veilig_pad(&cfg.organ_dir, &ir.path) {
            Err(msg) => e.push(format!("impulsrespons '{}': {}", ir.name, msg)),
            Ok(p) => if !p.is_file() { v.warnings.push(format!("impulsrespons '{}' ontbreekt: {}", ir.name, ir.path)); },
        }
    }
    for sa in &cfg.sound_assets {
        if let Err(msg) = veilig_pad(&cfg.organ_dir, &sa.path) { e.push(format!("geluid '{}': {}", sa.name, msg)); }
    }
    // Pijpbindingen tegen de geladen definitie.
    let mut onbekend_rank = 0usize;
    for b in &cfg.pipe_bindings {
        if !ranks.contains(&b.source_rank_id) { onbekend_rank += 1; }
        let ok = match (sectie_nummer(&b.stop_section, "Stop"), sectie_nummer(&b.pipe_section, "Pipe")) {
            (Some(s), Some(p)) if p >= 1 => odf_stops.get(&s)
                .and_then(|lagen| lagen.get(b.layer_index as usize))
                .map_or(false, |&n| (p as usize) <= n),
            _ => false,
        };
        if ok { v.bound_pipes += 1; } else { v.unbound_pipes += 1; }
    }
    if onbekend_rank > 0 { e.push(format!("{} pijpbindingen verwijzen naar een onbekende rank", onbekend_rank)); }
    if v.unbound_pipes > 0 {
        v.warnings.push(format!("{} van {} pijpbindingen wijzen niet op een bestaande stop/pijp in de definitie",
            v.unbound_pipes, cfg.pipe_bindings.len()));
    }
    if !cfg.pipe_bindings.is_empty() && v.bound_pipes == 0 {
        e.push("geen enkele pijpbinding past op de geladen definitie".into());
    }
    v
}

// ───────────────────────── Samenvatting (DTO) ─────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnclosureDto {
    pub id: String,
    pub name: String,
    /// Slot = index in de kastenlijst; zweltrede-index = 128 + slot.
    pub slot: u8,
    /// Naam van de zwelkoppeling (`SwellBinding.division_name`).
    pub swell_name: String,
    pub ranks: usize,
    pub pipes: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImpulseResponseDto {
    pub id: String,
    pub name: String,
    /// Volledig pad (voor `load_impulse_response` en de galm-opslag per orgel).
    pub path: String,
    pub channels: u16,
    pub sample_rate_hz: u32,
    pub seconds: f32,
    pub exists: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcousticsSummaryDto {
    pub format: String,
    pub schema_version: u32,
    pub activation_mode: String,
    /// Model geaccepteerd (geen essentiële fouten).
    pub valid: bool,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub enclosures: Vec<EnclosureDto>,
    pub ranks: usize,
    pub ranks_in_enclosure: usize,
    pub pipe_bindings: usize,
    pub bound_pipes: usize,
    pub impulse_responses: Vec<ImpulseResponseDto>,
    pub audio_groups: usize,
    pub mix_groups: usize,
    pub outputs: usize,
    pub effect_registers: usize,
    pub unresolved_references: usize,
}

/// Kaart voor de audio-engine: pijpsleutel → kastslot, plus per slot de
/// divisie waarvan de kastbus routing/pan/wind erft (de divisie met de
/// meeste gebonden pijpen in die kast).
#[derive(Debug, Clone, Default)]
pub struct EnclosureEngineMap {
    pub pipe_slots: HashMap<(u32, u32), u8>,
    pub parent_division: Vec<u8>,
    pub names: Vec<String>,
}

pub fn bouw_engine_kaart(cfg: &AcousticsConfig, stop_division: &HashMap<u32, u8>) -> EnclosureEngineMap {
    let slot_van_kast: HashMap<&str, u8> = cfg.enclosures.iter().take(MAX_ENCLOSURES).enumerate()
        .map(|(i, e)| (e.id.as_str(), i as u8)).collect();
    let slot_van_rank: HashMap<&str, u8> = cfg.ranks.iter()
        .filter_map(|r| r.enclosure_id.as_deref().and_then(|e| slot_van_kast.get(e)).map(|&s| (r.id.as_str(), s)))
        .collect();
    let n = slot_van_kast.len();
    let mut kaart = EnclosureEngineMap {
        pipe_slots: HashMap::new(),
        parent_division: vec![0; n],
        names: cfg.enclosures.iter().take(n).map(|e| e.name.clone()).collect(),
    };
    let mut stemmen: Vec<HashMap<u8, usize>> = vec![HashMap::new(); n];
    for b in &cfg.pipe_bindings {
        let Some(&slot) = slot_van_rank.get(b.source_rank_id.as_str()) else { continue };
        let (Some(stop), Some(pipe)) = (sectie_nummer(&b.stop_section, "Stop"), sectie_nummer(&b.pipe_section, "Pipe")) else { continue };
        if pipe == 0 { continue; }
        let key_pipe = if b.layer_index == 0 { pipe } else { crate::audio::layer_key(pipe, b.layer_index.min(255) as u8) };
        kaart.pipe_slots.insert((stop, key_pipe), slot);
        if let Some(&d) = stop_division.get(&stop) {
            *stemmen[slot as usize].entry(d).or_insert(0) += 1;
        }
    }
    for (slot, telling) in stemmen.iter().enumerate() {
        kaart.parent_division[slot] = telling.iter().max_by_key(|(_, n)| **n).map(|(d, _)| *d).unwrap_or(0);
    }
    kaart
}

/// Zweltrede-index voor een kastnaam (`enclosure:<id>`), of None.
pub fn enclosure_index_from_name(name: &str, summary: Option<&AcousticsSummaryDto>) -> Option<u8> {
    let id = name.strip_prefix(ENCLOSURE_NAME_PREFIX)?;
    let s = summary?;
    s.enclosures.iter().find(|e| e.id == id).map(|e| ENCLOSURE_INDEX_BASE.saturating_add(e.slot))
}

/// Laad en valideer de configuratie bij een orgeldefinitie. Geeft het model
/// (alleen wanneer geldig) en altijd een samenvatting voor de UI; None, None
/// wanneer het orgel geen `[JMOrgueAcoustics]`-sectie heeft.
pub fn laad_voor_odf(odf_path: &Path, odf_stops: &HashMap<u32, Vec<usize>>) -> (Option<AcousticsConfig>, Option<AcousticsSummaryDto>) {
    let Some(r) = AcousticsRef::from_odf(odf_path) else { return (None, None) };
    let organ_dir = odf_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut dto = AcousticsSummaryDto { format: r.format.clone(), schema_version: r.schema_version, activation_mode: r.activation.clone(), ..Default::default() };
    let json_path = match veilig_pad(&organ_dir, &r.configuration_file) {
        Ok(p) => p,
        Err(msg) => { dto.error = Some(msg); warn!("Akoestische configuratie: {}", dto.error.as_deref().unwrap_or("")); return (None, Some(dto)); }
    };
    let grootte = match std::fs::metadata(&json_path) {
        Ok(m) => m.len(),
        Err(e) => { dto.error = Some(format!("{} niet gevonden ({})", r.configuration_file, e)); warn!("Akoestische configuratie: {}", dto.error.as_deref().unwrap_or("")); return (None, Some(dto)); }
    };
    if grootte > MAX_JSON_BYTES {
        dto.error = Some(format!("{} is {} MB; maximaal {} MB", r.configuration_file, grootte / 1_000_000, MAX_JSON_BYTES / 1_000_000));
        warn!("Akoestische configuratie: {}", dto.error.as_deref().unwrap_or(""));
        return (None, Some(dto));
    }
    let mut cfg: AcousticsConfig = match std::fs::read(&json_path).map_err(|e| e.to_string()).and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string())) {
        Ok(c) => c,
        Err(e) => { dto.error = Some(format!("{} onleesbaar: {}", r.configuration_file, e)); warn!("Akoestische configuratie: {}", dto.error.as_deref().unwrap_or("")); return (None, Some(dto)); }
    };
    cfg.organ_dir = organ_dir.clone();
    dto.format = cfg.format.clone();
    dto.schema_version = cfg.schema_version;
    if !cfg.activation.mode.is_empty() { dto.activation_mode = cfg.activation.mode.clone(); }
    let v = valideer(&cfg, odf_stops);
    dto.warnings = v.warnings.clone();
    dto.ranks = cfg.ranks.len();
    dto.ranks_in_enclosure = cfg.ranks.iter().filter(|x| x.enclosure_id.is_some()).count();
    dto.pipe_bindings = cfg.pipe_bindings.len();
    dto.bound_pipes = v.bound_pipes;
    dto.audio_groups = cfg.audio_groups.len();
    dto.mix_groups = cfg.mix_groups.len();
    dto.outputs = cfg.outputs.len();
    dto.effect_registers = cfg.sound_switches.iter().filter(|s| s.kind == "effect_register").count();
    dto.unresolved_references = cfg.unresolved_references.len();
    dto.impulse_responses = cfg.impulse_responses.iter().map(|ir| {
        let vol = veilig_pad(&organ_dir, &ir.path).unwrap_or_default();
        ImpulseResponseDto {
            id: ir.id.clone(), name: ir.name.clone(),
            path: vol.to_string_lossy().into_owned(),
            channels: ir.wav.channels, sample_rate_hz: ir.wav.sample_rate_hz,
            seconds: if ir.wav.sample_rate_hz > 0 { ir.wav.frames as f32 / ir.wav.sample_rate_hz as f32 } else { 0.0 },
            exists: vol.is_file(),
        }
    }).collect();
    let rank_slots: HashMap<&str, &str> = cfg.ranks.iter().filter_map(|x| x.enclosure_id.as_deref().map(|e| (x.id.as_str(), e))).collect();
    dto.enclosures = cfg.enclosures.iter().take(MAX_ENCLOSURES).enumerate().map(|(i, e)| EnclosureDto {
        id: e.id.clone(), name: e.name.clone(), slot: i as u8,
        swell_name: format!("{}{}", ENCLOSURE_NAME_PREFIX, e.id),
        ranks: rank_slots.values().filter(|k| **k == e.id).count(),
        pipes: cfg.pipe_bindings.iter().filter(|b| rank_slots.get(b.source_rank_id.as_str()) == Some(&e.id.as_str())).count(),
    }).collect();
    if !v.errors.is_empty() {
        dto.valid = false;
        dto.error = Some(v.errors.join("; "));
        warn!("Akoestische configuratie geweigerd ({}): {}", r.configuration_file, dto.error.as_deref().unwrap_or(""));
        return (None, Some(dto));
    }
    dto.valid = true;
    info!(
        "Akoestische configuratie geladen: {} (schema {}, {}): {} zwelkasten, {} ranks ({} in een kast), {} van {} pijpbindingen passend, {} impulsresponsen, {} mengroutes, {} uitgangen, {} effectregisters; {} opmerkingen",
        cfg.format, cfg.schema_version, dto.activation_mode, dto.enclosures.len(), dto.ranks, dto.ranks_in_enclosure,
        dto.bound_pipes, dto.pipe_bindings, dto.impulse_responses.len(), dto.mix_groups, dto.outputs, dto.effect_registers, dto.warnings.len()
    );
    for w in &dto.warnings { warn!("Akoestische configuratie: {}", w); }
    (Some(cfg), Some(dto))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basis_cfg() -> AcousticsConfig {
        let mut c = AcousticsConfig { format: FORMAT_LEGACY_SWOP.into(), schema_version: 1, ..Default::default() };
        c.enclosures.push(Enclosure { id: "14000".into(), name: "Pos".into(), ..Default::default() });
        c.ranks.push(Rank { id: "11000".into(), name: "GO - Principal 16".into(), enclosure_id: None, ..Default::default() });
        c.ranks.push(Rank { id: "11001".into(), name: "Pos - Bourdon 8".into(), enclosure_id: Some("14000".into()), ..Default::default() });
        c.pipe_bindings.push(PipeBinding { stop_section: "Stop001".into(), pipe_section: "Pipe001".into(), source_rank_id: "11000".into(), ..Default::default() });
        c.pipe_bindings.push(PipeBinding { stop_section: "Stop002".into(), pipe_section: "Pipe003".into(), source_rank_id: "11001".into(), ..Default::default() });
        c
    }
    fn stops() -> HashMap<u32, Vec<usize>> { HashMap::from([(1, vec![56]), (2, vec![56])]) }

    #[test]
    fn sectie_uit_odf_wordt_gelezen() {
        let txt = "[Organ]\nChurchName=X\n\n[JMOrgueAcoustics]\nFormat=jm-orgue.swop-acoustics\nSchemaVersion=1\nConfigurationFile=JM-ACOUSTICS.json\nActivation=ExportOnly\n";
        let r = AcousticsRef::from_odf_text(txt).unwrap();
        assert_eq!(r.format, FORMAT_LEGACY_SWOP);
        assert_eq!(r.schema_version, 1);
        assert_eq!(r.configuration_file, "JM-ACOUSTICS.json");
        assert!(AcousticsRef::from_odf_text("[Organ]\nChurchName=X\n").is_none());
    }

    #[test]
    fn geldige_configuratie_valideert_en_bindt_pijpen() {
        let v = valideer(&basis_cfg(), &stops());
        assert!(v.errors.is_empty(), "{:?}", v.errors);
        assert_eq!(v.bound_pipes, 2);
        assert_eq!(v.unbound_pipes, 0);
    }

    #[test]
    fn onbekende_verwijzingen_en_schema_zijn_fouten() {
        let mut c = basis_cfg();
        c.ranks[1].enclosure_id = Some("99".into());
        let v = valideer(&c, &stops());
        assert!(v.errors.iter().any(|e| e.contains("onbekende 99")), "{:?}", v.errors);
        let mut c2 = basis_cfg();
        c2.schema_version = 2;
        assert!(!valideer(&c2, &stops()).errors.is_empty());
        let mut c3 = basis_cfg();
        c3.format = "iets-anders".into();
        assert!(!valideer(&c3, &stops()).errors.is_empty());
    }

    #[test]
    fn pijpbinding_buiten_definitie_telt_als_ongebonden() {
        let mut c = basis_cfg();
        c.pipe_bindings[1].pipe_section = "Pipe099".into();
        let v = valideer(&c, &stops());
        assert_eq!(v.bound_pipes, 1);
        assert_eq!(v.unbound_pipes, 1);
        assert!(v.errors.is_empty());
    }

    #[test]
    fn paden_blijven_binnen_de_orgelmap() {
        let dir = std::env::temp_dir();
        assert!(veilig_pad(&dir, "IR/Original/x.wav").is_ok());
        assert!(veilig_pad(&dir, "../x.wav").is_err());
        assert!(veilig_pad(&dir, "C:\\x.wav").is_err());
        assert!(veilig_pad(&dir, "/x.wav").is_err());
        assert!(veilig_pad(&dir, "").is_err());
    }

    #[test]
    fn engine_kaart_koppelt_pijpen_aan_kastslot_en_ouderdivisie() {
        let c = basis_cfg();
        let stop_div = HashMap::from([(1u32, 0u8), (2u32, 1u8)]);
        let k = bouw_engine_kaart(&c, &stop_div);
        assert_eq!(k.pipe_slots.get(&(2, 3)), Some(&0));
        assert!(k.pipe_slots.get(&(1, 1)).is_none(), "rank zonder kast hoort niet in de kaart");
        assert_eq!(k.parent_division, vec![1]);
        assert_eq!(k.names, vec!["Pos".to_string()]);
    }

    #[test]
    fn kastnaam_geeft_trede_index() {
        let dto = AcousticsSummaryDto { enclosures: vec![EnclosureDto { id: "14000".into(), slot: 1, ..Default::default() }], ..Default::default() };
        assert_eq!(enclosure_index_from_name("enclosure:14000", Some(&dto)), Some(129));
        assert_eq!(enclosure_index_from_name("Hoofdwerk", Some(&dto)), None);
        assert_eq!(enclosure_index_from_name("enclosure:14000", None), None);
    }
}
