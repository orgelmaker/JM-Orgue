//! First version of the user organ builder. Project files reference sources;
//! they never rewrite organ definitions, audio, or source organ settings.
//!
//! Validation and compilation are deliberately independent of AppState. A bad
//! source or project must be rejected before the current organ is touched.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use vpo_sampler::{
    CouplerDef, ImportReport, ManualDef, OrganDefinition, OrganInfo, PipeDef, SchakelaarRegels,
    StopDef,
};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_PROJECT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DEFINITION_BYTES: u64 = 64 * 1024 * 1024;
/// Covers the largest physical organ consoles with headroom. Project actions
/// use their own wide identity range rather than the legacy 150..=255 window.
pub const MAX_STOPS: usize = 2048;
/// Sources remain independent; existing stop, enclosure and wind budgets apply
/// to the compiled project separately from this bounded source count.
pub const MAX_SOURCES: usize = 64;
fn schema_version() -> u32 {
    SCHEMA_VERSION
}
fn default_a() -> f64 {
    440.0
}
fn one() -> u32 {
    1
}
fn next_division() -> u32 {
    4
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TuningPolicy {
    #[default]
    Original,
    Normalize,
    /// Per pijp bijstemmen op de gemeten toonhoogte (zoals "Pijpen automatisch
    /// bijstemmen" bij een gewoon orgel), daarna naar de projectreferentie.
    /// Bewust zwevende stemmen houden hun zweving.
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganProject {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default = "default_a")]
    pub target_a4_hz: f64,
    #[serde(default = "one")]
    pub next_stop_id: u32,
    #[serde(default = "next_division")]
    pub next_division_id: u32,
    #[serde(default)]
    pub sources: Vec<ProjectSource>,
    #[serde(default)]
    pub divisions: Vec<ProjectDivision>,
    #[serde(default)]
    pub stops: Vec<ProjectStop>,
    #[serde(default)]
    pub couplers: Vec<ProjectCoupler>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSource {
    pub id: String,
    pub path: String,
    pub fingerprint: String,
    #[serde(default)]
    pub package_root: Option<String>,
    #[serde(default = "default_a")]
    pub reference_a4_hz: f64,
    #[serde(default)]
    pub tuning_policy: TuningPolicy,
    #[serde(default)]
    pub tuning_cents: f32,
    #[serde(default)]
    pub perspective: Option<String>,
    #[serde(default)]
    pub resources: Vec<ResourceStamp>,
}

/// Fast change detector, not a claim of cryptographic audio authentication.
/// The definition has a content fingerprint; selected audio has size/time stamps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceStamp {
    pub path: String,
    pub bytes: u64,
    pub modified_ns: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDivision {
    pub id: u32,
    pub name: String,
    pub midi_channel: u8,
    pub first_midi_note: u8,
    pub key_count: u32,
    #[serde(default)]
    pub pedal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStop {
    pub id: u32,
    pub source_id: String,
    pub source_stop_id: u32,
    pub division_id: u32,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCoupler {
    pub id: u32,
    pub name: String,
    pub source_division_id: u32,
    pub destination_division_id: u32,
    #[serde(default)]
    pub keyshift: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceStop {
    pub id: u32,
    pub name: String,
    pub manual_id: u32,
    pub manual_name: String,
    pub harmonic_number: u32,
    pub first_midi_note: u32,
    pub key_count: u32,
    pub perspectives: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceCatalog {
    pub path: String,
    pub name: String,
    pub format: String,
    pub fingerprint: String,
    pub stops: Vec<SourceStop>,
    pub valve_controls: Vec<vpo_sampler::windwerk::ValveControlDef>,
    pub warnings: Vec<String>,
    pub supports_original_pitch: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SourceTuning {
    pub policy: TuningPolicy,
    pub reference_a4_hz: f64,
    pub target_a4_hz: f64,
    pub cents: f32,
    pub force_tempered: bool,
}

pub struct CompiledProject {
    pub definition: OrganDefinition,
    pub import_report: ImportReport,
    pub per_stop_tuning: HashMap<u32, SourceTuning>,
    pub resource_stamps: HashMap<String, Vec<ResourceStamp>>,
    pub acoustics: Option<crate::acoustics::AcousticsConfig>,
    pub wind: Option<crate::windwerk_set::ProjectWindPlan>,
}

struct ParsedSource {
    path: PathBuf,
    definition: OrganDefinition,
    report: ImportReport,
    fingerprint: String,
    acoustics: Option<crate::acoustics::AcousticsConfig>,
    warnings: Vec<String>,
}

pub fn is_project_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jmorgan"))
}

fn text_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn absolute_path(path: &Path, parent: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        parent.join(path)
    }
}

fn canonical_file(path: &Path) -> Result<PathBuf, String> {
    let p = fs::canonicalize(path).map_err(|e| {
        format!(
            "Bronbestand ontbreekt of is niet leesbaar: {} ({e})",
            path.display()
        )
    })?;
    if !p.is_file() {
        return Err(format!("Geen bronbestand: {}", p.display()));
    }
    Ok(plain_path(p))
}

/// Houdt gewone Windows-paden in UI/JSON in plaats van de Win32-verbatim-notatie.
fn plain_path(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = text_path(&p);
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    p
}

fn project_parent(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    if !p.is_absolute() || !is_project_path(p) {
        return Err("Kies een absoluut projectpad met extensie .jmorgan".into());
    }
    p.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Projectpad heeft geen map".into())
}

fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn validate_name(name: &str, what: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.len() > 160 || name.chars().any(char::is_control) {
        Err(format!(
            "{what}: geef een naam van 1 tot 160 tekens zonder besturingstekens"
        ))
    } else {
        Ok(())
    }
}

pub fn validate_project(project: &OrganProject) -> Result<(), String> {
    if project.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "Projectversie {} wordt niet ondersteund (verwacht {SCHEMA_VERSION})",
            project.schema_version
        ));
    }
    if !uuid(&project.id) {
        return Err("Ongeldige project-UUID".into());
    }
    validate_name(&project.name, "Project")?;
    if !project.target_a4_hz.is_finite() || !(300.0..=550.0).contains(&project.target_a4_hz) {
        return Err("Doeltoonhoogte moet tussen 300 en 550 Hz liggen".into());
    }
    if project.sources.is_empty() || project.sources.len() > MAX_SOURCES {
        return Err(format!("Kies één tot {MAX_SOURCES} bronorgels"));
    }
    if project.divisions.is_empty() || project.divisions.len() > 8 {
        return Err("Kies één tot acht doeldivisies".into());
    }
    if project.stops.is_empty() || project.stops.len() > MAX_STOPS {
        return Err(format!("Kies één tot {MAX_STOPS} registers"));
    }
    if project.couplers.len() > 512 {
        return Err("Deze versie ondersteunt hoogstens 512 eigen koppels".into());
    }
    let mut source_ids = HashSet::new();
    for source in &project.sources {
        if !uuid(&source.id) || !source_ids.insert(source.id.to_ascii_lowercase()) {
            return Err("Bron-ID's moeten geldige, unieke UUID's zijn".into());
        }
        if source.path.is_empty() || source.fingerprint.is_empty() {
            return Err(
                "Bronpad en bronvingerafdruk zijn verplicht; inspecteer de bron opnieuw".into(),
            );
        }
        if !source.reference_a4_hz.is_finite() || !(300.0..=550.0).contains(&source.reference_a4_hz)
        {
            return Err("Brontoonhoogte moet tussen 300 en 550 Hz liggen".into());
        }
        if !source.tuning_cents.is_finite() || !(-1200.0..=1200.0).contains(&source.tuning_cents) {
            return Err("Bronfijnstemming moet tussen -1200 en +1200 cent liggen".into());
        }
        if source
            .perspective
            .as_ref()
            .is_some_and(|p| p.is_empty() || p.len() > 80)
        {
            return Err("Ongeldige opnamepositie".into());
        }
        let mut resource_paths = HashSet::new();
        for stamp in &source.resources {
            if !resource_paths.insert(&stamp.path) {
                return Err("Dubbel bronbestand in resource-manifest".into());
            }
        }
    }
    let mut division_ids = HashSet::new();
    let mut division_names = HashSet::new();
    let mut midi_channels = HashSet::new();
    let mut max_division = 0;
    for division in &project.divisions {
        if division.id == 0 || division.id > 65_535 || !division_ids.insert(division.id) {
            return Err("Doeldivisies moeten unieke ID's van 1 tot 65535 hebben".into());
        }
        validate_name(&division.name, "Divisie")?;
        if !division_names.insert(division.name.trim().to_lowercase()) {
            return Err("Doeldivisies moeten verschillende namen hebben".into());
        }
        if !(1..=16).contains(&division.midi_channel)
            || !midi_channels.insert(division.midi_channel)
        {
            return Err("Kies per divisie een uniek MIDI-kanaal van 1 tot 16".into());
        }
        if division.key_count == 0 || u32::from(division.first_midi_note) + division.key_count > 128
        {
            return Err("Ongeldig toetsbereik van doeldivisie".into());
        }
        max_division = max_division.max(division.id);
    }
    if project.divisions.iter().filter(|d| d.pedal).count() > 1 {
        return Err("Er kan hoogstens één pedaaldivisie zijn".into());
    }
    if project.next_division_id <= max_division || project.next_division_id > 65_536 {
        return Err("Volgende divisie-ID moet groter zijn dan alle bestaande ID's".into());
    }
    let mut stop_ids = HashSet::new();
    let mut max_stop = 0;
    for stop in &project.stops {
        if stop.id == 0 || stop.id > 65_535 || !stop_ids.insert(stop.id) {
            return Err("Projectregisters moeten unieke ID's van 1 tot 65535 hebben".into());
        }
        if !source_ids.contains(&stop.source_id.to_ascii_lowercase())
            || !division_ids.contains(&stop.division_id)
        {
            return Err("Register verwijst naar ontbrekende bron of doeldivisie".into());
        }
        if let Some(name) = &stop.name {
            validate_name(name, "Register")?;
        }
        max_stop = max_stop.max(stop.id);
    }
    if project.next_stop_id <= max_stop || project.next_stop_id > 65_536 {
        return Err("Volgende register-ID moet groter zijn dan alle bestaande ID's".into());
    }
    let mut coupler_ids = HashSet::new();
    for coupler in &project.couplers {
        validate_name(&coupler.name, "Koppel")?;
        if coupler.id == 0
            || coupler.id > 65_535
            || !coupler_ids.insert(coupler.id)
            || !division_ids.contains(&coupler.source_division_id)
            || !division_ids.contains(&coupler.destination_division_id)
            || !(-24..=24).contains(&coupler.keyshift)
        {
            return Err("Ongeldig projectkoppel".into());
        }
    }
    Ok(())
}

fn fingerprint(path: &Path) -> Result<String, String> {
    let meta =
        fs::metadata(path).map_err(|e| format!("Bron niet leesbaar: {} ({e})", path.display()))?;
    if meta.len() > MAX_DEFINITION_BYTES {
        return Err("Brondefinitie is groter dan 64 MiB".into());
    }
    let mut file = fs::File::open(path).map_err(|e| format!("Bron niet leesbaar: {e}"))?;
    let mut hash = 0xcbf29ce484222325u64;
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Bron niet leesbaar: {e}"))?;
        if n == 0 {
            break;
        }
        for byte in &buf[..n] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Ok(format!("fnv1a64:{hash:016x}:{}", meta.len()))
}

fn parse_source(path: &Path, package_root: Option<&Path>) -> Result<ParsedSource, String> {
    let path = canonical_file(path)?;
    if is_project_path(&path) {
        return Err("Een project kan geen ander project als sampleset gebruiken".into());
    }
    let mut fp = fingerprint(&path)?;
    let (mut definition, report) = if vpo_sampler::is_hauptwerk_path(&path) {
        let (result, report) = vpo_sampler::load_hauptwerk_met_verslag(&path, package_root);
        (
            result.map_err(|e| format!("Brondefinitie niet leesbaar: {e}"))?,
            report,
        )
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("organ"))
    {
        let definition = OrganDefinition::load(&path)
            .map_err(|e| format!("Brondefinitie niet leesbaar: {e}"))?;
        let report = ImportReport::uit_definitie(
            "grandorgue",
            "",
            &text_path(&path),
            &definition.organ.church_name,
            definition.stops.len(),
            definition.manuals.len(),
            definition.couplers.len(),
        );
        (definition, report)
    } else {
        return Err(
            "Kies een .organ, .Organ_Hauptwerk_xml of .CustomOrgan_Hauptwerk_xml als bron".into(),
        );
    };
    if report.onvolledig || report.niet_speelbaar || report.beschermd > 0 {
        return Err("Deze bron is onvolledig of bevat onleesbare samples. Herstel eerst de bronimport en ontbrekende klankpakketten".into());
    }
    if definition.organ.geen_originele_toonhoogte {
        return Err("Deze sampleset vereist berekende stemming zonder oorspronkelijke orgeltoonhoogte. Dat brontype wordt in deze eerste Orgelbouwer nog niet ondersteund".into());
    }
    if !report.tremulant_golfvormen.is_empty() {
        return Err("Deze bron bevat opgenomen tremulant-golfvormbesturing. Dit extra importmodel wordt nog niet ondersteund; gewone tremulantopnamen en aparte windnetwerken wel".into());
    }
    definition.verzamel_go_registermechaniek();
    definition.verzamel_go_toetsmechaniek();
    let mut acoustics = None;
    let mut warnings = Vec::new();
    if let Some(reference) = crate::acoustics::AcousticsRef::from_odf(&path) {
        let stop_layers = definition
            .stops
            .iter()
            .map(|stop| {
                (
                    stop.id,
                    stop.all_layers().map(|(_, _, pipes)| pipes.len()).collect(),
                )
            })
            .collect();
        let (configuration, summary) = crate::acoustics::laad_voor_odf(&path, &stop_layers);
        let configuration = configuration.ok_or_else(|| {
            let reason = summary
                .and_then(|s| s.error)
                .unwrap_or_else(|| "configuratie niet geldig".into());
            format!("Akoestische bronconfiguratie kan niet veilig worden gelezen: {reason}")
        })?;
        let config_path = crate::acoustics::veilig_pad(
            path.parent().ok_or("Bron heeft geen map")?,
            &reference.configuration_file,
        )?;
        // The settings that determine which ranks are safe are part of the
        // source identity, even when no audio sample or ODF byte changed.
        fp.push_str("|acoustics:");
        fp.push_str(&fingerprint(&config_path)?);
        warnings.push("Rankzwelkasten worden per pijp en bron overgenomen. Ruimtelijke JSON-routes zijn in de huidige engine niet DSP-actief en worden niet als stereoroutering of bronafhankelijke galm geïmporteerd.".into());
        acoustics = Some(configuration);
    }
    Ok(ParsedSource {
        path,
        definition,
        report,
        fingerprint: fp,
        acoustics,
        warnings,
    })
}

fn stop_manual<'a>(definition: &'a OrganDefinition, stop: &StopDef) -> Option<&'a ManualDef> {
    definition
        .manuals
        .iter()
        .find(|m| m.stop_ids.contains(&stop.id))
}

fn stop_first(definition: &OrganDefinition, stop: &StopDef) -> u32 {
    stop.first_midi_note.unwrap_or_else(|| {
        stop_manual(definition, stop)
            .map(|m| {
                m.first_accessible_key_midi_note
                    + stop.first_accessible_pipe_logical_key.saturating_sub(1)
            })
            .unwrap_or(36)
    })
}

fn perspectives(stop: &StopDef) -> Vec<String> {
    stop.all_layers()
        .filter_map(|(_, perspective, _)| perspective.map(str::to_owned))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Bind only matching source-local switch identities, retaining stable project
/// stop IDs even when the user reorders stops or adds another source with the
/// same numeric Hauptwerk switch ID. Unselected controls stay independent.
fn project_stop_controls(
    project: &OrganProject,
    source_id: &str,
    wind: &vpo_sampler::windwerk::WindwerkDef,
) -> HashMap<u32, Vec<u32>> {
    wind.controls.iter()
        .filter(|control| control.kind == vpo_sampler::windwerk::ValveControlKind::Switch)
        .filter_map(|control| {
            let mut ids: Vec<_> = project.stops.iter()
                .filter(|stop| stop.source_id == source_id && control.source_stop_ids.contains(&stop.source_stop_id))
                .map(|stop| stop.id).collect();
            ids.sort_unstable();
            ids.dedup();
            (!ids.is_empty()).then_some((control.id, ids))
        }).collect()
}

pub fn inspect_source(path: &str, package_root: Option<&str>) -> Result<SourceCatalog, String> {
    let parsed = parse_source(Path::new(path), package_root.map(Path::new))?;
    let definition = &parsed.definition;
    let stops = definition
        .stops
        .iter()
        .filter(|s| !s.is_noise_or_mechanical())
        .map(|stop| {
            let manual = stop_manual(definition, stop);
            SourceStop {
                id: stop.id,
                name: stop.name.clone(),
                manual_id: manual.map(|m| m.number).unwrap_or(0),
                manual_name: manual.map(|m| m.name.clone()).unwrap_or_default(),
                harmonic_number: stop.sounding_harmonic,
                first_midi_note: stop_first(definition, stop),
                key_count: stop.number_of_pipes,
                perspectives: perspectives(stop),
            }
        })
        .collect();
    let mut warnings = vec!["Opgenomen galm blijft behouden; bronstemming verandert geen opnameakoestiek. De opgegeven A-referentie wordt niet automatisch gemeten.".into()];
    warnings.extend(parsed.warnings);
    if let Some(wind) = parsed.report.windwerk.as_ref().filter(|wind| !wind.is_leeg()) {
        warnings.push(format!("Bronwindnetwerk: {} compartimenten en {} verbindingen. De bronparameters worden met het afzonderlijke JM-Orgue-windmodel gesimuleerd.", wind.laden.len(), wind.verbindingen.len()));
        if !wind.controls.is_empty() {
            let automatic = wind.controls.iter().filter(|control| control.automatic).count();
            warnings.push(format!("{} broncontrollers regelen dynamische windkleppen; {} volgen automatische bronregelingen. Registerschakelaars worden aan geselecteerde projectregisters gekoppeld; zelfstandige ingangen behouden een eigen bronbediening.", wind.controls.len(), automatic));
            if automatic > 0 {
                warnings.push("Automatische bronregeling gebruikt het JM-model: 127 − x bij inversie, dan (x + offset) × factor, machtsfunctie en begrenzing op 0..127; Willis als exponentiële tijdconstante en inertia als gedempte veer. Fysieke terugkoppeling en schakellussen gebruiken de vorige rekentik. Dit claimt geen identieke Hauptwerk-simulatie.".into());
            }
        }
        for issue in &wind.automation.issues {
            warnings.push(format!("Bronwindbesturing: {issue}. Dit project wordt vóór laden geweigerd."));
        }
        for control in &wind.controls {
            for issue in &control.issues {
                warnings.push(format!("Klepbediening {}: {issue}. Het project kan pas laden wanneer deze bronlogica wordt ondersteund.", control.name));
            }
        }
        for link in &wind.verbindingen {
            if let Some(vpo_sampler::windwerk::ValveControlRef::Unsupported { reason, .. }) = &link.valve_control {
                warnings.push(format!("Windverbinding {}: {reason}. Deze bron kan niet veilig als project laden.", link.naam));
            }
        }
    }
    if !definition.couplers.is_empty() || parsed.report.set_crescendo.is_some() {
        warnings.push("Bronkoppels en broncrescendo worden niet overgenomen; dit project gebruikt een eigen divisie-indeling en registratie.".into());
    }
    if !definition.extra_bijgeluiden.is_empty() || !definition.keyboard_clicks.is_empty() {
        warnings.push("Doorlopende bijgeluiden en klaviermechaniek worden niet geïmporteerd. Geselecteerde registeropnamen en registerklikken blijven behouden.".into());
    }
    Ok(SourceCatalog {
        path: text_path(&parsed.path),
        name: definition.organ.church_name.clone(),
        format: parsed.report.formaat,
        fingerprint: parsed.fingerprint,
        stops,
        valve_controls: parsed.report.windwerk.as_ref().map(|wind| wind.controls.clone()).unwrap_or_default(),
        warnings,
        supports_original_pitch: true,
    })
}

/// Resolve stored paths before handing a project to UI; no source is loaded here.
pub fn load_project(path: &str) -> Result<OrganProject, String> {
    let parent = project_parent(path)?;
    let meta = fs::metadata(path).map_err(|e| format!("Project niet leesbaar: {e}"))?;
    if meta.len() > MAX_PROJECT_BYTES {
        return Err("Projectbestand is groter dan 16 MiB".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("Project niet leesbaar: {e}"))?;
    let mut project: OrganProject =
        serde_json::from_slice(&bytes).map_err(|e| format!("Ongeldig Orgelbouwer-project: {e}"))?;
    validate_project(&project)?;
    resolve_project_paths(&mut project, &parent);
    Ok(project)
}

fn resolve_project_paths(project: &mut OrganProject, parent: &Path) {
    for source in &mut project.sources {
        source.path = text_path(&absolute_path(Path::new(&source.path), parent));
        source.package_root = source
            .package_root
            .as_ref()
            .map(|p| text_path(&absolute_path(Path::new(p), parent)));
        for resource in &mut source.resources {
            resource.path = text_path(&absolute_path(Path::new(&resource.path), parent));
        }
    }
}

fn stamp_resource(path: &Path) -> Result<ResourceStamp, String> {
    let meta =
        fs::metadata(path).map_err(|e| format!("Sample ontbreekt: {} ({e})", path.display()))?;
    if !meta.is_file() || meta.len() == 0 {
        return Err(format!(
            "Sample is geen leesbaar audiobestand: {}",
            path.display()
        ));
    }
    let modified_ns = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().to_string())
        .unwrap_or_default();
    Ok(ResourceStamp {
        path: text_path(path),
        bytes: meta.len(),
        modified_ns,
    })
}

fn collect_resources(pipe: &PipeDef, resources: &mut BTreeSet<PathBuf>) {
    if let PipeDef::Sample { path, extra } = pipe {
        resources.insert(path.clone());
        for release in extra.releases.iter().chain(extra.tremulant_releases.iter()) {
            resources.insert(release.path.clone());
        }
        for attack in extra.dry_attack.iter().chain(extra.tremulant_attack.iter()) {
            resources.insert(attack.path.clone());
        }
    }
}

fn flatten_pipe(
    definition: &OrganDefinition,
    stop: &StopDef,
    idx: usize,
    pipe: &PipeDef,
) -> Result<PipeDef, String> {
    if matches!(pipe, PipeDef::Empty) {
        return Ok(PipeDef::Empty);
    }
    let (owner, owner_idx, resolved) = definition
        .resolve_reference_owner(stop, idx, pipe)
        .ok_or_else(|| {
            format!(
                "Register '{}' heeft een onopgeloste of cyclische leenpijp {}",
                stop.name,
                idx + 1
            )
        })?;
    let PipeDef::Sample { path, extra } = resolved else {
        return Err("Onverwacht lege leenpijp".into());
    };
    let mut extra = extra.clone();
    extra.harmonic_number = extra.harmonic_number.or(Some(owner.harmonic_number));
    extra.key_midi_note = extra
        .key_midi_note
        .or(Some(stop_first(definition, owner) + owner_idx as u32));
    // Preserve the reference owner's retune semantics after removing REF nodes.
    // Normal playback gain/PitchTuning retains the selected stop's hierarchy.
    let wc_pc = definition
        .windchests
        .iter()
        .find(|w| w.number == owner.windchest_group)
        .map(|w| w.pitch_correction_cents)
        .unwrap_or(0.0);
    extra.pitch_correction_cents = Some(
        extra.pitch_correction_cents.unwrap_or(0.0)
            + definition.organ.pitch_correction_cents
            + wc_pc
            + owner.pitch_correction,
    );
    Ok(PipeDef::Sample {
        path: path.clone(),
        extra,
    })
}

/// Returns (new layer, original layer) so source-local wind/enclosure metadata
/// follows a chosen microphone layer even when it is promoted to layer zero.
fn choose_perspective(stop: &mut StopDef, desired: Option<&str>) -> Result<Vec<(u8, u8)>, String> {
    let available = perspectives(stop);
    if available.is_empty() {
        return Ok(stop
            .all_layers()
            .map(|(layer, _, _)| (layer, layer))
            .collect());
    }
    let chosen = desired
        .map(str::to_owned)
        .or_else(|| stop.primary_perspective.clone())
        .unwrap_or_else(|| available[0].clone());
    if !available.contains(&chosen) {
        return Err(format!(
            "Register '{}' heeft opnamepositie '{}' niet",
            stop.name, chosen
        ));
    }
    let keep_primary = stop
        .primary_perspective
        .as_ref()
        .is_none_or(|p| p == &chosen);
    let mut pipes = if keep_primary {
        stop.pipes.clone()
    } else {
        vec![PipeDef::Empty; stop.pipes.len()]
    };
    let mut layers = Vec::new();
    let mut primary_source = 0;
    for layer in &stop.layers {
        if layer.perspective.as_ref().is_none_or(|p| p == &chosen) {
            let mut layer = layer.clone();
            layer.perspective = None;
            layers.push(layer);
        }
    }
    if !keep_primary {
        if let Some(pos) = layers
            .iter()
            .position(|l| l.pipes.iter().any(|p| !matches!(p, PipeDef::Empty)))
        {
            let promoted = layers.remove(pos);
            primary_source = promoted.index;
            pipes = promoted.pipes;
        }
    }
    stop.pipes = pipes;
    stop.primary_perspective = None;
    let mut projection = vec![(0, primary_source)];
    for (idx, layer) in layers.iter_mut().enumerate() {
        projection.push(((idx + 1) as u8, layer.index));
        layer.index = (idx + 1) as u8;
    }
    stop.layers = layers;
    Ok(projection)
}

#[derive(Clone)]
struct SourceRankEnclosure {
    rank: String,
    enclosure: String,
}

fn source_rank_enclosures(
    configuration: &crate::acoustics::AcousticsConfig,
) -> HashMap<(u32, u32), SourceRankEnclosure> {
    let ranks: HashMap<&str, &crate::acoustics::Rank> = configuration
        .ranks
        .iter()
        .map(|rank| (rank.id.as_str(), rank))
        .collect();
    let mut bindings = HashMap::new();
    for binding in &configuration.pipe_bindings {
        let (Some(stop), Some(pipe), Some(rank)) = (
            crate::acoustics::sectie_nummer(&binding.stop_section, "Stop"),
            crate::acoustics::pijp_nummer(binding),
            ranks.get(binding.source_rank_id.as_str()),
        ) else {
            continue;
        };
        let Some(enclosure) = &rank.enclosure_id else {
            continue;
        };
        bindings.insert(
            (
                stop,
                crate::audio::layer_key(pipe, binding.layer_index.min(255) as u8),
            ),
            SourceRankEnclosure {
                rank: rank.id.clone(),
                enclosure: enclosure.clone(),
            },
        );
    }
    bindings
}

/// The selected rank has priority; a borrowed pipe otherwise keeps the physical
/// donor's enclosure. Source-local numeric IDs are never looked up in another set.
fn rank_enclosure_for_pipe<'a>(
    definition: &OrganDefinition,
    stop: &StopDef,
    idx: usize,
    layer: u8,
    pipe: &PipeDef,
    bindings: &'a HashMap<(u32, u32), SourceRankEnclosure>,
    depth: u8,
) -> Option<&'a SourceRankEnclosure> {
    if let Some(binding) = bindings.get(&(stop.id, crate::audio::layer_key(idx as u32 + 1, layer)))
    {
        return Some(binding);
    }
    if depth > 8 {
        return None;
    }
    let PipeDef::Reference {
        manual,
        stop: stop_number,
        pipe: pipe_number,
    } = pipe
    else {
        return None;
    };
    let owner = definition
        .manuals
        .iter()
        .find(|m| m.number == *manual)
        .and_then(|m| m.stop_ids.get((*stop_number as usize).saturating_sub(1)))
        .and_then(|id| definition.get_stop(*id))
        .or_else(|| definition.get_stop(*stop_number))?;
    let owner_idx = (*pipe_number as usize).saturating_sub(1);
    rank_enclosure_for_pipe(
        definition,
        owner,
        owner_idx,
        0,
        owner.pipes.get(owner_idx)?,
        bindings,
        depth + 1,
    )
}

/// Create one complete, namespaced definition. This function has no AppState,
/// audio commands, source writes, or user-settings writes.
pub fn compile_project(path: &str, project: &OrganProject) -> Result<CompiledProject, String> {
    let parent = project_parent(path)?;
    validate_project(project)?;
    let mut project = project.clone();
    resolve_project_paths(&mut project, &parent);
    let mut parsed = BTreeMap::new();
    let mut canonical_sources = HashSet::new();
    for source in &project.sources {
        let value = parse_source(
            Path::new(&source.path),
            source.package_root.as_deref().map(Path::new),
        )?;
        if value.fingerprint != source.fingerprint {
            return Err(format!("Bron '{}' is gewijzigd. Inspecteer en bevestig de bron opnieuw voordat u dit project opslaat of laadt", value.definition.organ.church_name));
        }
        if !canonical_sources.insert(value.path.clone()) {
            return Err("Dezelfde brondefinitie is tweemaal toegevoegd".into());
        }
        parsed.insert(source.id.clone(), value);
    }
    let first = parsed.values().next().ok_or("Project heeft geen bron")?;
    let mut definition = first.definition.clone();
    definition.base_path = parent;
    definition.organ = OrganInfo {
        church_name: project.name.clone(),
        organ_builder: "JM-Orgue Orgelbouwer".into(),
        organ_comments: "Samengesteld project; originele samplesets blijven onaangeraakt".into(),
        amplitude_level: 100.0,
        ..OrganInfo::default()
    };
    definition.manuals.clear();
    definition.stops.clear();
    definition.windchests.clear();
    definition.enclosures.clear();
    definition.tremulants.clear();
    definition.couplers.clear();
    definition.extra_bijgeluiden.clear();
    definition.stop_action_clicks.clear();
    definition.keyboard_clicks.clear();
    definition.klavier_afkortingen.clear();
    definition.schakelaars = SchakelaarRegels::default();
    let mut division_numbers = HashMap::new();
    let mut divisions = project.divisions.iter().collect::<Vec<_>>();
    divisions.sort_by_key(|d| d.id);
    let mut manual_number = 1;
    for division in &divisions {
        let number = if division.pedal {
            0
        } else {
            let n = manual_number;
            manual_number += 1;
            n
        };
        division_numbers.insert(division.id, number);
        definition.manuals.push(ManualDef {
            number,
            name: division.name.trim().into(),
            midi_input_number: u32::from(division.midi_channel),
            number_of_logical_keys: division.key_count,
            number_of_accessible_keys: division.key_count,
            first_accessible_key_midi_note: u32::from(division.first_midi_note),
            stop_ids: Vec::new(),
            coupler_ids: Vec::new(),
            tremulant_ids: Vec::new(),
        });
    }
    let mut wc_ids = HashMap::new();
    let mut enclosure_ids = HashMap::new();
    let mut trem_ids = HashMap::new();
    for (source_id, value) in &parsed {
        for enclosure in &value.definition.enclosures {
            let mut enclosure = enclosure.clone();
            let id = definition.enclosures.len() as u32 + 1;
            enclosure_ids.insert((source_id.clone(), enclosure.number), id);
            enclosure.number = id;
            definition.enclosures.push(enclosure);
        }
        for tremulant in &value.definition.tremulants {
            let mut tremulant = tremulant.clone();
            let id = definition.tremulants.len() as u32 + 1;
            trem_ids.insert((source_id.clone(), tremulant.number), id);
            tremulant.number = id;
            definition.tremulants.push(tremulant);
        }
        for wc in &value.definition.windchests {
            let mut wc = wc.clone();
            let id = definition.windchests.len() as u32 + 1;
            wc_ids.insert((source_id.clone(), wc.number), id);
            wc.number = id;
            wc.enclosure_ids = wc
                .enclosure_ids
                .iter()
                .map(|n| {
                    enclosure_ids
                        .get(&(source_id.clone(), *n))
                        .copied()
                        .ok_or_else(|| "Ontbrekende bronzwelkast".to_string())
                })
                .collect::<Result<_, _>>()?;
            wc.tremulant_ids = wc
                .tremulant_ids
                .iter()
                .map(|n| {
                    trem_ids
                        .get(&(source_id.clone(), *n))
                        .copied()
                        .ok_or_else(|| "Ontbrekende brontremulant".to_string())
                })
                .collect::<Result<_, _>>()?;
            // Retune hierarchy is folded per pipe to preserve reference owners.
            wc.pitch_correction_cents = 0.0;
            definition.windchests.push(wc);
        }
    }
    if definition.enclosures.len() > 16 {
        return Err("Dit project heeft meer dan zestien bronzwelkasten".into());
    }
    let mut per_stop_tuning = HashMap::new();
    let rank_bindings: HashMap<String, _> = parsed
        .iter()
        .filter_map(|(source, parsed)| {
            parsed
                .acoustics
                .as_ref()
                .map(|cfg| (source.clone(), source_rank_enclosures(cfg)))
        })
        .collect();
    let mut project_enclosures = BTreeMap::new();
    let mut project_ranks = BTreeMap::new();
    let mut project_bindings = Vec::new();
    let wind_sources: Vec<_> = parsed
        .iter()
        .filter_map(|(source_id, parsed)| {
            parsed
                .report
                .windwerk
                .as_ref()
                .filter(|wind| !wind.is_leeg())
                .map(|wind| crate::windwerk_set::ProjectWindSource {
                    source_id: source_id.clone(),
                    windwerk: wind.clone(),
                    stop_controls: project_stop_controls(&project, source_id, wind),
                })
        })
        .collect();
    let mut wind_pipes = Vec::new();
    let mut resource_paths: HashMap<String, BTreeSet<PathBuf>> = HashMap::new();
    let mut mixed_divisions: BTreeSet<u32> = BTreeSet::new();
    let mut division_groups: HashMap<
        u32,
        (Option<(String, Vec<u32>)>, Option<(String, Vec<u32>)>),
    > = HashMap::new();
    for selected in &project.stops {
        let source = project
            .sources
            .iter()
            .find(|s| s.id == selected.source_id)
            .ok_or("Ontbrekende projectbron")?;
        let value = parsed.get(&source.id).ok_or("Ontbrekende bronanalyse")?;
        let source_stop = value
            .definition
            .stops
            .iter()
            .find(|s| s.id == selected.source_stop_id)
            .ok_or_else(|| format!("Bronregister {} bestaat niet meer", selected.source_stop_id))?;
        if source_stop.is_noise_or_mechanical() {
            return Err(
                "Mechaniek- en achtergrondgeluiden zijn geen selecteerbare registers".into(),
            );
        }
        let source_manual = stop_manual(&value.definition, source_stop);
        let source_wc = value
            .definition
            .windchests
            .iter()
            .find(|w| w.number == source_stop.windchest_group);
        let mut trem = source_manual
            .map(|m| m.tremulant_ids.clone())
            .unwrap_or_default();
        if let Some(wc) = source_wc {
            trem.extend(wc.tremulant_ids.iter().copied());
        }
        trem.sort_unstable();
        trem.dedup();
        let enclosures = source_wc
            .map(|w| w.enclosure_ids.clone())
            .unwrap_or_default();
        let groups = (
            if trem.is_empty() {
                None
            } else {
                Some((source.id.clone(), trem.clone()))
            },
            if enclosures.is_empty() {
                None
            } else {
                Some((source.id.clone(), enclosures))
            },
        );
        // Registers uit verschillende bronnen (of met verschillende bronkasten en
        // -tremulanten) mogen op dezelfde doeldivisie staan. De eigen speeltafel
        // kent per divisie één zwelkast en één tremulant; die gelden dan voor
        // alle registers van die divisie. Rankzwelkasten (JM-ACOUSTICS) blijven
        // per bron onafhankelijk. We tellen alleen hoeveel divisies gemengd zijn.
        match division_groups.get(&selected.division_id) {
            Some(existing) if existing != &groups => { mixed_divisions.insert(selected.division_id); }
            Some(_) => {}
            None => { division_groups.insert(selected.division_id, groups); }
        }
        let mut stop = source_stop.clone();
        stop.id = selected.id;
        if value.report.windwerk.as_ref().is_some_and(|wind| wind.controls.iter().any(|control|
            control.kind == vpo_sampler::windwerk::ValveControlKind::Switch
                && control.source_stop_ids.contains(&source_stop.id)
                && control.default_value >= 0.5)) {
            definition.schakelaars.stops_standaard_aan.push(selected.id);
        }
        if let Some(name) = &selected.name {
            stop.name = name.trim().into();
        }
        stop.first_midi_note = Some(stop_first(&value.definition, source_stop));
        stop.gain_db += value.definition.organ.gain_db;
        stop.amplitude_level *= value.definition.organ.amplitude_level / 100.0;
        stop.pitch_tuning_cents += value.definition.organ.pitch_tuning_cents;
        stop.pitch_correction = 0.0;
        stop.windchest_group = wc_ids
            .get(&(source.id.clone(), source_stop.windchest_group))
            .copied()
            .unwrap_or(0);
        stop.pipes = source_stop
            .pipes
            .iter()
            .enumerate()
            .map(|(idx, pipe)| flatten_pipe(&value.definition, source_stop, idx, pipe))
            .collect::<Result<_, _>>()?;
        for (layer, original) in stop.layers.iter_mut().zip(&source_stop.layers) {
            layer.pipes = original
                .pipes
                .iter()
                .enumerate()
                .map(|(idx, pipe)| flatten_pipe(&value.definition, source_stop, idx, pipe))
                .collect::<Result<_, _>>()?;
        }
        let layer_projection = choose_perspective(&mut stop, source.perspective.as_deref())?;
        if !stop
            .all_layers()
            .any(|(_, _, pipes)| pipes.iter().any(|p| matches!(p, PipeDef::Sample { .. })))
        {
            return Err(format!(
                "Register '{}' heeft geen speelbare pijpen in de gekozen opnamepositie",
                stop.name
            ));
        }
        let has_source_wind = value
            .report
            .windwerk
            .as_ref()
            .is_some_and(|wind| !wind.laden.is_empty());
        for (new_layer, old_layer) in layer_projection {
            let original_pipes = source_stop
                .all_layers()
                .find(|(layer, _, _)| *layer == old_layer)
                .map(|(_, _, pipes)| pipes)
                .ok_or("Interne fout: oorspronkelijke pijplaag ontbreekt")?;
            let pipes = stop
                .all_layers()
                .find(|(layer, _, _)| *layer == new_layer)
                .map(|(_, _, pipes)| pipes)
                .ok_or("Interne fout: gekozen pijplaag ontbreekt")?;
            for (idx, pipe) in pipes.iter().enumerate() {
                let PipeDef::Sample {
                    path: attack_path,
                    extra,
                } = pipe
                else {
                    continue;
                };
                let key = (stop.id, crate::audio::layer_key(idx as u32 + 1, new_layer));
                if has_source_wind {
                    let lade = extra.wind_lade.ok_or_else(|| {
                        format!(
                            "Register '{}' mist de bronwindlade voor pijp {}",
                            stop.name,
                            idx + 1
                        )
                    })?;
                    let kg_s = extra.wind_verbruik_kg_s.ok_or_else(|| {
                        format!(
                            "Register '{}' mist het bronwindverbruik voor pijp {}",
                            stop.name,
                            idx + 1
                        )
                    })?;
                    wind_pipes.push(crate::windwerk_set::ProjectWindPipe {
                        key,
                        source_id: source.id.clone(),
                        lade,
                        kg_s,
                        ref_inch: extra.wind_ref_inch,
                    });
                }
                let Some(bindings) = rank_bindings.get(&source.id) else {
                    continue;
                };
                let Some(original_pipe) = original_pipes.get(idx) else {
                    continue;
                };
                let Some(binding) = rank_enclosure_for_pipe(
                    &value.definition,
                    source_stop,
                    idx,
                    old_layer,
                    original_pipe,
                    bindings,
                    0,
                ) else {
                    continue;
                };
                let configuration = value
                    .acoustics
                    .as_ref()
                    .ok_or("Interne fout: rankkastconfiguratie ontbreekt")?;
                let enclosure_id = format!("project:{}:enclosure:{}", source.id, binding.enclosure);
                let rank_id = format!("project:{}:rank:{}", source.id, binding.rank);
                if !project_enclosures.contains_key(&enclosure_id) {
                    let mut enclosure = configuration
                        .enclosures
                        .iter()
                        .find(|enclosure| enclosure.id == binding.enclosure)
                        .cloned()
                        .ok_or("Bronrank verwijst naar ontbrekende zwelkast")?;
                    enclosure.id = enclosure_id.clone();
                    enclosure.name = format!(
                        "{} — {}",
                        value.definition.organ.church_name, enclosure.name
                    );
                    project_enclosures.insert(enclosure_id.clone(), enclosure);
                }
                if !project_ranks.contains_key(&rank_id) {
                    let mut rank = configuration
                        .ranks
                        .iter()
                        .find(|rank| rank.id == binding.rank)
                        .cloned()
                        .ok_or("Bronpijp verwijst naar ontbrekende rank")?;
                    rank.id = rank_id.clone();
                    rank.enclosure_id = Some(enclosure_id);
                    // Audio group gain has already been baked into source pipe
                    // gain. Spatial routes are not active in the source engine.
                    rank.audio_group_id = None;
                    project_ranks.insert(rank_id.clone(), rank);
                }
                project_bindings.push(crate::acoustics::PipeBinding {
                    stop_section: format!("Stop{:03}", stop.id),
                    pipe_section: format!("Pipe{:03}", idx + 1),
                    layer_index: u32::from(new_layer),
                    logical_pipe: (idx + 1) as u64,
                    keyboard_midi_note: u64::from(stop.first_midi_note.unwrap_or(36)) + idx as u64,
                    sample_midi_note: u64::from(extra.key_midi_note.unwrap_or(36)),
                    source_rank_id: rank_id,
                    attack_path: text_path(attack_path),
                    trem_attack_path: extra
                        .tremulant_attack
                        .as_ref()
                        .map(|attack| text_path(&attack.path)),
                    ..Default::default()
                });
            }
        }
        let resources = resource_paths.entry(source.id.clone()).or_default();
        for (_, _, pipes) in stop.all_layers() {
            for pipe in pipes {
                collect_resources(pipe, resources);
            }
        }
        if let Some(click) = value.definition.stop_action_clicks.get(&source_stop.id) {
            for path in click.attack.iter().chain(click.release.iter()) {
                resources.insert(path.clone());
            }
            definition.stop_action_clicks.insert(stop.id, click.clone());
        }
        if value
            .definition
            .schakelaars
            .stops_buiten_combinatie
            .contains(&source_stop.id)
        {
            definition.schakelaars.stops_buiten_combinatie.push(stop.id);
        }
        let manual = definition
            .manuals
            .iter_mut()
            .find(|m| m.number == division_numbers[&selected.division_id])
            .ok_or("Ontbrekende doeldivisie")?;
        manual.stop_ids.push(stop.id);
        for id in trem {
            if let Some(mapped) = trem_ids.get(&(source.id.clone(), id)) {
                if !manual.tremulant_ids.contains(mapped) {
                    manual.tremulant_ids.push(*mapped);
                }
            }
        }
        per_stop_tuning.insert(
            stop.id,
            SourceTuning {
                policy: source.tuning_policy,
                reference_a4_hz: source.reference_a4_hz,
                target_a4_hz: project.target_a4_hz,
                cents: source.tuning_cents,
                force_tempered: false,
            },
        );
        definition.stops.push(stop);
    }
    // Validate all selected audio before any loader/AppState mutation. Cache by
    // absolute path so shared ranks/releases are only decoded once.
    let mut checked_audio = HashSet::new();
    let mut resource_stamps = HashMap::new();
    for source in &project.sources {
        let previous: HashMap<&str, &ResourceStamp> = source
            .resources
            .iter()
            .map(|r| (r.path.as_str(), r))
            .collect();
        let mut stamps = Vec::new();
        for path in resource_paths.get(&source.id).into_iter().flatten() {
            let stamp = stamp_resource(path)?;
            if let Some(old) = previous.get(stamp.path.as_str()) {
                if old.bytes != stamp.bytes
                    || (!old.modified_ns.is_empty() && old.modified_ns != stamp.modified_ns)
                {
                    return Err(format!("Sample is gewijzigd: {}. Inspecteer de bron opnieuw voordat u het project bijwerkt", path.display()));
                }
            }
            if checked_audio.insert(path.clone()) {
                vpo_sampler::load_audio_preload_ms(path, 1)
                    .map_err(|e| format!("Sample niet leesbaar: {} ({e})", path.display()))?;
            }
            stamps.push(stamp);
        }
        resource_stamps.insert(source.id.clone(), stamps);
    }
    // Empty means the first-version default unison couplers. Their identities
    // use persistent project division IDs, not names or compact runtime manual
    // numbers, so MIDI/console bindings survive renaming or removing a manual.
    let default_couplers = if project.couplers.is_empty() {
        let mut generated = Vec::new();
        for source in &divisions {
            for destination in &divisions {
                if destination.pedal
                    || source.id == destination.id
                    || (!source.pedal && source.id > destination.id)
                {
                    continue;
                }
                generated.push(ProjectCoupler {
                    id: (source.id << 16) | destination.id,
                    name: format!("{}/{}", destination.name, source.name),
                    source_division_id: source.id,
                    destination_division_id: destination.id,
                    keyshift: 0,
                });
            }
        }
        generated
    } else {
        Vec::new()
    };
    let compiled_couplers = if project.couplers.is_empty() {
        &default_couplers
    } else {
        &project.couplers
    };
    for coupler in compiled_couplers {
        let src = division_numbers[&coupler.source_division_id];
        let dst = division_numbers[&coupler.destination_division_id];
        definition.couplers.push(CouplerDef {
            number: coupler.id,
            name: coupler.name.clone(),
            source_manual: src,
            destination_manual: dst,
            destination_keyshift: coupler.keyshift,
            unison_off: false,
            melodie_of_bas: false,
            melodie: false,
            cascade_toegestaan: true,
        });
        if let Some(manual) = definition.manuals.iter_mut().find(|m| m.number == src) {
            manual.coupler_ids.push(coupler.id);
        }
    }
    definition.organ.number_of_manuals =
        definition.manuals.iter().filter(|m| m.number != 0).count() as u32;
    definition.organ.has_pedals = project.divisions.iter().any(|d| d.pedal);
    definition.organ.number_of_enclosures = definition.enclosures.len() as u32;
    definition.organ.number_of_tremulants = definition.tremulants.len() as u32;
    if !mixed_divisions.is_empty() {
        tracing::info!(
            "Orgelproject: {} doeldivisie(s) combineren registers met verschillende bronkasten of -tremulanten; zwelkast en tremulant gelden per doeldivisie",
            mixed_divisions.len()
        );
    }
    definition.organ.number_of_windchest_groups = definition.windchests.len() as u32;
    if project_enclosures.len() > crate::acoustics::MAX_ENCLOSURES {
        return Err(format!("Het project gebruikt {} rankzwelkasten; maximaal {} onafhankelijke kasten passen in de audio-engine. Kies minder kasten of andere registers", project_enclosures.len(), crate::acoustics::MAX_ENCLOSURES));
    }
    project_bindings.sort_by_key(|binding| {
        (
            crate::acoustics::sectie_nummer(&binding.stop_section, "Stop").unwrap_or(0),
            binding.layer_index,
            binding.logical_pipe,
        )
    });
    let acoustics = if project_bindings.is_empty() {
        None
    } else {
        let cfg = crate::acoustics::AcousticsConfig {
            format: crate::acoustics::FORMAT_NEUTRAL.into(),
            schema_version: crate::acoustics::SCHEMA_VERSION,
            source: crate::acoustics::Source {
                organ_name: project.name.clone(),
                ..Default::default()
            },
            activation: crate::acoustics::Activation {
                mode: "project_rank_enclosures".into(),
                automatic_dsp_activation: true,
                fallback: "Project validation must succeed before replacing the active organ"
                    .into(),
                ..Default::default()
            },
            enclosures: project_enclosures.into_values().collect(),
            ranks: project_ranks.into_values().collect(),
            pipe_bindings: project_bindings,
            organ_dir: definition.base_path.clone(),
            ..Default::default()
        };
        let stop_layers = definition
            .stops
            .iter()
            .map(|stop| {
                (
                    stop.id,
                    stop.all_layers().map(|(_, _, pipes)| pipes.len()).collect(),
                )
            })
            .collect();
        let validation = crate::acoustics::valideer(&cfg, &stop_layers);
        if !validation.errors.is_empty() {
            return Err(format!(
                "Projectzwelkasten zijn niet geldig: {}",
                validation.errors.join("; ")
            ));
        }
        Some(cfg)
    };
    let wind = if wind_sources.is_empty() {
        None
    } else {
        Some(crate::windwerk_set::compile_project_wind(
            &wind_sources,
            &wind_pipes,
        )?)
    };
    let mut import_report = ImportReport::uit_definitie(
        "project",
        "1",
        path,
        &project.name,
        definition.stops.len(),
        definition.manuals.len(),
        definition.couplers.len(),
    );
    import_report.melding(vpo_sampler::Ernst::Info, "Orgelbouwer", "Complete geselecteerde registers; bronkoppels, broncrescendo en doorlopende bijgeluiden worden niet geïmporteerd. Opgenomen galm blijft behouden.");
    if let Some(plan) = &wind {
        for warning in &plan.warnings {
            import_report.melding(vpo_sampler::Ernst::Waarschuwing, "Bronwindnetwerk", warning.clone());
        }
    }
    Ok(CompiledProject {
        definition,
        import_report,
        per_stop_tuning,
        resource_stamps,
        acoustics,
        wind,
    })
}

/// Validate/compile before replacing the project file. Existing files may only
/// be updated by the same project UUID. A failed save leaves the old file intact.
pub fn save_project(path: &str, mut project: OrganProject) -> Result<OrganProject, String> {
    let parent = project_parent(path)?;
    resolve_project_paths(&mut project, &parent);
    let compiled = compile_project(path, &project)?;
    for source in &mut project.sources {
        source.resources = compiled
            .resource_stamps
            .get(&source.id)
            .cloned()
            .unwrap_or_default();
    }
    if Path::new(path).exists() {
        let old = load_project(path)?;
        if old.id != project.id {
            return Err(
                "Dit bestand hoort bij een ander orgelproject. Kies een andere bestandsnaam".into(),
            );
        }
        validate_update(&old, &project)?;
    }
    fs::create_dir_all(&parent).map_err(|e| format!("Projectmap niet schrijfbaar: {e}"))?;
    let mut stored = project.clone();
    // Bronpaden zijn gecanonicaliseerd (lange naam, zonder \\?\). De projectmap kan als
    // 8.3-kortenaam, symlink of subst-pad zijn opgegeven; vergelijk daarom met beide vormen,
    // anders worden die paden onnodig absoluut (en dus niet verhuisbaar) opgeslagen.
    let parent_real = fs::canonicalize(&parent)
        .map(plain_path)
        .unwrap_or_else(|_| parent.clone());
    let relative = |p: &Path| -> Option<String> {
        p.strip_prefix(&parent)
            .or_else(|_| p.strip_prefix(&parent_real))
            .ok()
            .map(text_path)
    };
    for source in &mut stored.sources {
        if let Some(rel) = relative(Path::new(&source.path)) {
            source.path = rel;
        }
        source.package_root = source
            .package_root
            .take()
            .map(|root| relative(Path::new(&root)).unwrap_or(root));
        for stamp in &mut source.resources {
            if let Some(rel) = relative(Path::new(&stamp.path)) {
                stamp.path = rel;
            }
        }
    }
    let data =
        serde_json::to_vec_pretty(&stored).map_err(|e| format!("Project niet op te slaan: {e}"))?;
    if data.len() as u64 > MAX_PROJECT_BYTES {
        return Err("Project is groter dan 16 MiB".into());
    }
    let file_name = Path::new(path)
        .file_name()
        .ok_or("Projectpad mist bestandsnaam")?
        .to_string_lossy();
    let temporary = parent.join(format!(".{file_name}.{:016x}.tmp", rand::random::<u64>()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|e| format!("Tijdelijk projectbestand niet schrijfbaar: {e}"))?;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Project niet volledig geschreven: {e}"))?;
        drop(file);
        if Path::new(path).exists() {
            fs::copy(path, parent.join(format!("{file_name}.bak")))
                .map_err(|e| format!("Projectbackup niet te maken: {e}"))?;
        }
        fs::rename(&temporary, path)
            .map_err(|e| format!("Projectbestand niet te vervangen: {e}"))?;
        Ok(project)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_update(old: &OrganProject, project: &OrganProject) -> Result<(), String> {
    if project.next_stop_id < old.next_stop_id || project.next_division_id < old.next_division_id {
        return Err("Project-ID-tellers mogen niet worden teruggezet; bestaande consolekoppelingen moeten behouden blijven".into());
    }
    for stop in &project.stops {
        match old.stops.iter().find(|s| s.id == stop.id) {
            Some(previous)
                if previous.source_id != stop.source_id
                    || previous.source_stop_id != stop.source_stop_id =>
            {
                return Err(format!("Projectregister {} hoort al bij een ander bronregister. Gebruik een nieuw projectregister-ID", stop.id));
            }
            None if stop.id < old.next_stop_id => {
                return Err(format!(
                    "Projectregister-ID {} is eerder gebruikt. Gebruik het volgende vrije ID",
                    stop.id
                ));
            }
            _ => {}
        }
    }
    for division in &project.divisions {
        if !old.divisions.iter().any(|d| d.id == division.id) && division.id < old.next_division_id
        {
            return Err(format!(
                "Divisie-ID {} is eerder gebruikt. Gebruik het volgende vrije ID",
                division.id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT_ID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const SOURCE_A: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    const SOURCE_B: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";

    struct Fixture {
        dir: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir()
                .join(format!("jm-project-test-{:016x}", rand::random::<u64>()));
            fs::create_dir(&dir).unwrap();
            for name in ["a.wav", "b.wav", "release.wav", "trem.wav"] {
                let mut wav = hound::WavWriter::create(
                    dir.join(name),
                    hound::WavSpec {
                        channels: 1,
                        sample_rate: 44_100,
                        bits_per_sample: 16,
                        sample_format: hound::SampleFormat::Int,
                    },
                )
                .unwrap();
                for frame in 0..2205 {
                    wav.write_sample::<i16>(((frame as f32 * 0.06).sin() * 4000.0) as i16)
                        .unwrap();
                }
                wav.finalize().unwrap();
            }
            for (name, sample) in [("A", "a.wav"), ("B", "b.wav")] {
                fs::write(dir.join(format!("{name}.organ")), format!(
                    "[Organ]\nChurchName=Bron {name}\nNumberOfManuals=1\nHasPedals=N\nNumberOfWindchestGroups=1\nNumberOfEnclosures=0\nNumberOfTremulants=0\nPitchCorrection=2\n\
                    [WindchestGroup001]\nName=Open\nNumberOfEnclosures=0\nNumberOfTremulants=0\nPitchCorrection=3\n\
                    [Manual001]\nName=Manual\nNumberOfLogicalKeys=1\nNumberOfAccessibleKeys=1\nFirstAccessibleKeyMIDINoteNumber=60\nNumberOfStops=2\nStop001=001\nStop002=002\nNumberOfCouplers=0\nNumberOfTremulants=0\n\
                    [Stop001]\nName=Prestant 8\nHarmonicNumber=8\nPitchCorrection=9\nWindchestGroup=1\nNumberOfLogicalPipes=1\nNumberOfAccessiblePipes=1\nFirstAccessiblePipeLogicalKeyNumber=1\nPipe001={sample}\n\
                    [Stop002]\nName=Octaaf 4\nHarmonicNumber=16\nWindchestGroup=1\nNumberOfLogicalPipes=1\nNumberOfAccessiblePipes=1\nFirstAccessiblePipeLogicalKeyNumber=1\nPipe001=REF:001:001:001\n"
                )).unwrap();
            }
            Self { dir }
        }

        fn path(&self) -> String {
            text_path(&self.dir.join("Eigen.jmorgan"))
        }

        fn add_source(&self, project: &mut OrganProject, index: u32) -> u32 {
            let sample = format!("source-{index}.wav");
            fs::copy(self.dir.join("a.wav"), self.dir.join(&sample)).unwrap();
            let source_path = self.dir.join(format!("Source-{index}.organ"));
            let definition = fs::read_to_string(self.dir.join("A.organ")).unwrap()
                .replace("Bron A", &format!("Bron {index}"))
                .replace("a.wav", &sample);
            fs::write(&source_path, definition).unwrap();
            let catalog = inspect_source(&text_path(&source_path), None).unwrap();
            let source_id = format!("dddddddd-dddd-4ddd-8ddd-{index:012x}");
            project.sources.push(ProjectSource {
                id: source_id.clone(), path: catalog.path, fingerprint: catalog.fingerprint,
                package_root: None, reference_a4_hz: 400.0 + f64::from(index),
                tuning_policy: if index % 2 == 0 { TuningPolicy::Normalize } else { TuningPolicy::Original },
                tuning_cents: index as f32 / 2.0, perspective: None, resources: Vec::new(),
            });
            let stop_id = project.next_stop_id;
            project.next_stop_id += 1;
            project.stops.push(ProjectStop { id: stop_id, source_id, source_stop_id: 1,
                division_id: project.divisions[0].id, name: Some(format!("Bron {index}")) });
            stop_id
        }

        fn project(&self) -> OrganProject {
            let sources = [(SOURCE_A, "A", 415.0), (SOURCE_B, "B", 440.0)]
                .into_iter()
                .map(|(id, name, reference)| {
                    let catalog =
                        inspect_source(&text_path(&self.dir.join(format!("{name}.organ"))), None)
                            .unwrap();
                    ProjectSource {
                        id: id.into(),
                        path: catalog.path,
                        fingerprint: catalog.fingerprint,
                        package_root: None,
                        reference_a4_hz: reference,
                        tuning_policy: TuningPolicy::Normalize,
                        tuning_cents: 0.0,
                        perspective: None,
                        resources: Vec::new(),
                    }
                })
                .collect();
            OrganProject {
                schema_version: 1,
                id: PROJECT_ID.into(),
                name: "Eigen orgel".into(),
                target_a4_hz: 440.0,
                next_stop_id: 104,
                next_division_id: 2,
                sources,
                divisions: vec![ProjectDivision {
                    id: 1,
                    name: "Hoofdwerk".into(),
                    midi_channel: 2,
                    first_midi_note: 36,
                    key_count: 61,
                    pedal: false,
                }],
                stops: vec![
                    ProjectStop {
                        id: 101,
                        source_id: SOURCE_A.into(),
                        source_stop_id: 1,
                        division_id: 1,
                        name: None,
                    },
                    ProjectStop {
                        id: 102,
                        source_id: SOURCE_B.into(),
                        source_stop_id: 1,
                        division_id: 1,
                        name: None,
                    },
                    ProjectStop {
                        id: 103,
                        source_id: SOURCE_A.into(),
                        source_stop_id: 2,
                        division_id: 1,
                        name: Some("Leenpijp".into()),
                    },
                ],
                couplers: vec![],
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // Only remove our freshly-created absolute test directory.
            if let (Ok(dir), Ok(temp)) = (
                fs::canonicalize(&self.dir),
                fs::canonicalize(std::env::temp_dir()),
            ) {
                if dir.parent() == Some(temp.as_path())
                    && dir
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("jm-project-test-"))
                {
                    let _ = fs::remove_dir_all(dir);
                }
            }
        }
    }

    #[test]
    fn valve_switch_mapping_keeps_source_namespaces_and_stable_project_stop_ids() {
        use vpo_sampler::windwerk::{ValveControlDef, ValveControlKind, WindwerkDef};
        let fixture = Fixture::new();
        let mut project = fixture.project();
        let wind = WindwerkDef {
            controls: vec![ValveControlDef {
                id: 7, kind: ValveControlKind::Switch, name: "Windmotor".into(), default_value: 1.0,
                source_stop_ids: vec![1], linkages: vec![], issues: vec![], automatic: false,
            }, ValveControlDef {
                id: 8, kind: ValveControlKind::Switch, name: "Niet geselecteerd".into(), default_value: 0.0,
                source_stop_ids: vec![999], linkages: vec![], issues: vec![], automatic: false,
            }],
            ..Default::default()
        };
        assert_eq!(project_stop_controls(&project, SOURCE_A, &wind)[&7], vec![101]);
        assert_eq!(project_stop_controls(&project, SOURCE_B, &wind)[&7], vec![102]);
        assert!(!project_stop_controls(&project, SOURCE_A, &wind).contains_key(&8));
        project.stops.reverse();
        assert_eq!(project_stop_controls(&project, SOURCE_A, &wind)[&7], vec![101]);
        project.stops.push(ProjectStop { id: 222, source_id: SOURCE_A.into(), source_stop_id: 1, division_id: 1, name: None });
        assert_eq!(project_stop_controls(&project, SOURCE_A, &wind)[&7], vec![101, 222]);
    }

    #[test]
    fn authored_wind_links_without_compartments_fail_preflight_and_preserve_project_file() {
        let fixture = Fixture::new();
        let mut project = save_project(&fixture.path(), fixture.project()).unwrap();
        let previous = fs::read(fixture.path()).unwrap();
        let package = fixture.dir.join("OrganInstallationPackages").join("000001");
        fs::create_dir_all(&package).unwrap();
        fs::copy(fixture.dir.join("a.wav"), package.join("tone.wav")).unwrap();
        fs::write(package.join("PackageID000001.InstallationPackageDefinition_Hauptwerk_xml"), "<Hauptwerk/>").unwrap();
        let source_path = fixture.dir.join("Missing wind compartments.Organ_Hauptwerk_xml");
        fs::write(&source_path, r#"<Hauptwerk FileFormat="Organ" FileFormatVersion="4.0">
<ObjectList ObjectType="_General"><_General><Identification_Name>Missing wind compartments</Identification_Name><EnablePlayingAtOriginalOrganPitch>Y</EnablePlayingAtOriginalOrganPitch></_General></ObjectList>
<ObjectList ObjectType="Division"><Division><DivisionID>1</DivisionID><Name>Manual</Name></Division></ObjectList>
<ObjectList ObjectType="Stop"><Stop><StopID>1</StopID><Name>Principal 8</Name><DivisionID>1</DivisionID></Stop></ObjectList>
<ObjectList ObjectType="StopRank"><StopRank><StopID>1</StopID><RankID>1</RankID><MIDINoteNumOfFirstMappedDivisionInputNode>69</MIDINoteNumOfFirstMappedDivisionInputNode><NumberOfMappedDivisionInputNodes>1</NumberOfMappedDivisionInputNodes></StopRank></ObjectList>
<ObjectList ObjectType="Rank"><Rank><RankID>1</RankID><Name>Principal 8</Name></Rank></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01"><Pipe_SoundEngine01><PipeID>1</PipeID><RankID>1</RankID><NormalMIDINoteNumber>69</NormalMIDINoteNumber><Pitch_Tempered_RankBasePitch64ftHarmonicNum>8</Pitch_Tempered_RankBasePitch64ftHarmonicNum><Pitch_OriginalOrgan_SpecificationMethodCode>2</Pitch_OriginalOrgan_SpecificationMethodCode><Pitch_OriginalOrgan_PitchHz>440</Pitch_OriginalOrgan_PitchHz><WindSupply_SourceWindCompartmentID>2</WindSupply_SourceWindCompartmentID><WindSupply_MassFlowRateKilogramsPerSecAtReferencePressureDiff>0.008</WindSupply_MassFlowRateKilogramsPerSecAtReferencePressureDiff><WindSupply_ReferencePressureDifferenceInches>4</WindSupply_ReferencePressureDifferenceInches></Pipe_SoundEngine01></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_Layer"><Pipe_SoundEngine01_Layer><LayerID>1</LayerID><PipeID>1</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_AttackSample"><Pipe_SoundEngine01_AttackSample><UniqueID>1</UniqueID><LayerID>1</LayerID><SampleID>1</SampleID></Pipe_SoundEngine01_AttackSample></ObjectList>
<ObjectList ObjectType="Sample"><Sample><SampleID>1</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>tone.wav</SampleFilename><Pitch_SpecificationMethodCode>4</Pitch_SpecificationMethodCode><Pitch_ExactSamplePitch>440</Pitch_ExactSamplePitch></Sample></ObjectList>
<ObjectList ObjectType="ContinuousControl"><ContinuousControl><ControlID>9</ControlID><DefaultValue>127</DefaultValue></ContinuousControl></ObjectList>
<ObjectList ObjectType="WindCompartmentLinkage"><WindCompartmentLinkage><FirstWindCompartmentID>1</FirstWindCompartmentID><SecondWindCompartmentID>2</SecondWindCompartmentID><MassFlowRateKilogramsPerSecAtReferencePressureDiff>0.02</MassFlowRateKilogramsPerSecAtReferencePressureDiff><ReferencePressureDifferenceInches>1</ReferencePressureDifferenceInches><ValveControlTypeCode>3</ValveControlTypeCode><ValveControllingContinuousControlID>9</ValveControllingContinuousControlID></WindCompartmentLinkage></ObjectList>
</Hauptwerk>"#).unwrap();
        let catalog = inspect_source(&text_path(&source_path), None).unwrap();
        assert_eq!(catalog.stops.len(), 1, "the fixture must import a complete playable source stop before validating wind");
        assert_eq!(catalog.valve_controls.len(), 1, "malformed wind must remain visible in the source catalog");
        project.sources.retain(|source| source.id == SOURCE_A);
        project.stops.retain(|stop| stop.source_id == SOURCE_A && stop.source_stop_id == 1);
        let source = &mut project.sources[0];
        source.path = catalog.path;
        source.fingerprint = catalog.fingerprint;
        source.resources.clear();
        let error = compile_project(&fixture.path(), &project).err().expect("invalid wind cannot enter the load path");
        assert!(error.contains("ontbrekende of gelijke begin-/eindlade"), "{error}");
        assert!(save_project(&fixture.path(), project).is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), previous);
    }

    #[test]
    fn two_sources_with_colliding_ids_keep_their_audio_and_tuning() {
        let fixture = Fixture::new();
        let project = fixture.project();
        let compiled = compile_project(&fixture.path(), &project).unwrap();
        let sample_path = |id| match &compiled.definition.get_stop(id).unwrap().pipes[0] {
            PipeDef::Sample { path, .. } => {
                path.file_name().unwrap().to_string_lossy().into_owned()
            }
            _ => panic!("not resolved"),
        };
        assert_eq!(sample_path(101), "a.wav");
        assert_eq!(sample_path(102), "b.wav");
        assert_eq!(compiled.per_stop_tuning[&101].reference_a4_hz, 415.0);
        assert_eq!(compiled.per_stop_tuning[&102].reference_a4_hz, 440.0);
        assert_ne!(
            compiled.definition.get_stop(101).unwrap().windchest_group,
            compiled.definition.get_stop(102).unwrap().windchest_group
        );
    }

    #[test]
    fn borrowed_pipe_keeps_owner_pitch_even_when_owner_not_selected() {
        let fixture = Fixture::new();
        let mut project = fixture.project();
        project.stops.retain(|s| s.id == 103);
        let compiled = compile_project(&fixture.path(), &project).unwrap();
        let stop = compiled.definition.get_stop(103).unwrap();
        let PipeDef::Sample { path, extra } = &stop.pipes[0] else {
            panic!("REF not resolved")
        };
        assert!(path.ends_with("a.wav"));
        assert_eq!(extra.harmonic_number, Some(8));
        assert_eq!(extra.key_midi_note, Some(60));
        assert_eq!(extra.pitch_correction_cents, Some(14.0));
        assert_eq!(stop.first_midi_note, Some(60));
    }

    #[test]
    fn three_sources_keep_audio_tuning_and_stable_ids_after_save_reload_and_reordering() {
        let fixture = Fixture::new();
        let mut project = fixture.project();
        let third = fixture.add_source(&mut project, 3);
        let initial = compile_project(&fixture.path(), &project).unwrap();
        assert_eq!(project.sources.len(), 3);
        assert!(initial.definition.get_pipe_sample(third, 1).unwrap().ends_with("source-3.wav"));
        assert_eq!(initial.per_stop_tuning[&third].reference_a4_hz, 403.0);
        assert_eq!(initial.per_stop_tuning[&third].policy, TuningPolicy::Original);
        assert_eq!(initial.per_stop_tuning[&third].cents, 1.5);
        assert_eq!(initial.per_stop_tuning[&101].reference_a4_hz, 415.0);
        assert_eq!(initial.per_stop_tuning[&102].reference_a4_hz, 440.0);
        save_project(&fixture.path(), project).unwrap();
        let mut reopened = load_project(&fixture.path()).unwrap();
        assert_eq!(reopened.sources.len(), 3);
        assert_eq!(reopened.schema_version, SCHEMA_VERSION);
        reopened.sources.reverse();
        reopened.stops.rotate_left(1);
        let reordered = compile_project(&fixture.path(), &reopened).unwrap();
        for id in [101, 102, 103, third] {
            assert_eq!(initial.definition.get_pipe_sample(id, 1), reordered.definition.get_pipe_sample(id, 1));
            assert_eq!(initial.definition.get_stop(id).unwrap().name, reordered.definition.get_stop(id).unwrap().name);
            assert_eq!(initial.definition.get_stop(id).unwrap().windchest_group, reordered.definition.get_stop(id).unwrap().windchest_group);
            assert_eq!(initial.per_stop_tuning[&id].policy, reordered.per_stop_tuning[&id].policy);
            assert_eq!(initial.per_stop_tuning[&id].reference_a4_hz, reordered.per_stop_tuning[&id].reference_a4_hz);
            assert_eq!(initial.per_stop_tuning[&id].cents, reordered.per_stop_tuning[&id].cents);
        }
    }

    #[test]
    fn source_budget_allows_one_through_sixty_four_and_rejects_zero_or_sixty_five() {
        let fixture = Fixture::new();
        let mut one = fixture.project();
        one.sources.retain(|source| source.id == SOURCE_A);
        one.stops.retain(|stop| stop.source_id == SOURCE_A);
        compile_project(&fixture.path(), &one).unwrap();
        one.sources.clear();
        assert!(validate_project(&one).unwrap_err().contains("bronorgels"));

        let mut project = fixture.project();
        for index in 3..=MAX_SOURCES as u32 { fixture.add_source(&mut project, index); }
        assert_eq!(project.sources.len(), MAX_SOURCES);
        let compiled = compile_project(&fixture.path(), &project).unwrap();
        assert_eq!(compiled.definition.stops.len(), MAX_SOURCES + 1);
        assert_eq!(compiled.per_stop_tuning.len(), MAX_SOURCES + 1);
        let previous = save_project(&fixture.path(), project.clone()).unwrap();
        assert_eq!(load_project(&fixture.path()).unwrap().sources.len(), MAX_SOURCES);
        let before = fs::read(fixture.path()).unwrap();
        fixture.add_source(&mut project, MAX_SOURCES as u32 + 1);
        assert_eq!(project.sources.len(), MAX_SOURCES + 1);
        let error = compile_project(&fixture.path(), &project).err().expect("source budget must reject before compilation");
        assert!(error.contains(&format!("één tot {MAX_SOURCES} bronorgels")), "{error}");
        assert!(save_project(&fixture.path(), project).is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), before);
        assert_eq!(previous.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn source_reordering_preserves_project_stop_identity() {
        let fixture = Fixture::new();
        let mut project = fixture.project();
        let first = compile_project(&fixture.path(), &project).unwrap();
        project.sources.reverse();
        project.stops.reverse();
        let second = compile_project(&fixture.path(), &project).unwrap();
        for id in [101, 102, 103] {
            let a = first.definition.get_stop(id).unwrap();
            let b = second.definition.get_stop(id).unwrap();
            assert_eq!(a.name, b.name);
            assert_eq!(a.windchest_group, b.windchest_group);
            assert_eq!(
                first.definition.get_pipe_sample(id, 1),
                second.definition.get_pipe_sample(id, 1)
            );
        }
    }

    #[test]
    fn save_and_reload_are_relative_on_disk_and_do_not_touch_sources() {
        let fixture = Fixture::new();
        let project = fixture.project();
        let before = fs::read(fixture.dir.join("A.organ")).unwrap();
        let saved = save_project(&fixture.path(), project).unwrap();
        assert!(!saved.sources[0].resources.is_empty());
        assert!(Path::new(&saved.sources[0].path).is_absolute());
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.path()).unwrap()).unwrap();
        assert_eq!(stored["sources"][0]["path"], "A.organ");
        let reopened = load_project(&fixture.path()).unwrap();
        assert_eq!(reopened.id, PROJECT_ID);
        assert_eq!(reopened.stops[0].id, 101);
        compile_project(&fixture.path(), &reopened).unwrap();
        assert_eq!(fs::read(fixture.dir.join("A.organ")).unwrap(), before);
        assert!(!fixture.dir.join(".jm-settings-A.organ.json").exists());
    }

    #[test]
    fn changed_definition_is_rejected_until_explicit_reinspection() {
        let fixture = Fixture::new();
        let project = fixture.project();
        let mut file = OpenOptions::new()
            .append(true)
            .open(fixture.dir.join("A.organ"))
            .unwrap();
        writeln!(file, "; bron aangepast").unwrap();
        let error = compile_project(&fixture.path(), &project).err().unwrap();
        assert!(error.contains("is gewijzigd"), "{error}");
    }

    #[test]
    fn changed_selected_audio_is_rejected_after_save() {
        let fixture = Fixture::new();
        let saved = save_project(&fixture.path(), fixture.project()).unwrap();
        let mut file = OpenOptions::new()
            .append(true)
            .open(fixture.dir.join("a.wav"))
            .unwrap();
        file.write_all(&[0, 0]).unwrap();
        let error = compile_project(&fixture.path(), &saved).err().unwrap();
        assert!(error.contains("Sample is gewijzigd"), "{error}");
    }

    #[test]
    fn missing_selected_audio_fails_before_project_replacement() {
        let fixture = Fixture::new();
        let saved = save_project(&fixture.path(), fixture.project()).unwrap();
        let old = fs::read(fixture.path()).unwrap();
        fs::remove_file(fixture.dir.join("b.wav")).unwrap();
        assert!(save_project(&fixture.path(), saved).is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), old);
    }

    #[test]
    fn invalid_edit_does_not_overwrite_existing_project() {
        let fixture = Fixture::new();
        let mut saved = save_project(&fixture.path(), fixture.project()).unwrap();
        let old = fs::read(fixture.path()).unwrap();
        saved.stops[0].source_stop_id = 9999;
        assert!(save_project(&fixture.path(), saved).is_err());
        assert_eq!(fs::read(fixture.path()).unwrap(), old);
    }

    #[test]
    fn another_project_cannot_overwrite_existing_identity() {
        let fixture = Fixture::new();
        let mut saved = save_project(&fixture.path(), fixture.project()).unwrap();
        let old = fs::read(fixture.path()).unwrap();
        saved.id = "dddddddd-dddd-4ddd-8ddd-dddddddddddd".into();
        assert!(save_project(&fixture.path(), saved)
            .unwrap_err()
            .contains("ander orgelproject"));
        assert_eq!(fs::read(fixture.path()).unwrap(), old);
    }

    #[test]
    fn retained_counter_prevents_accidental_id_reuse() {
        let fixture = Fixture::new();
        let mut project = fixture.project();
        project.next_stop_id = 103;
        assert!(validate_project(&project)
            .unwrap_err()
            .contains("Volgende register-ID"));
    }

    #[test]
    fn existing_console_target_cannot_be_reassigned_to_another_source_stop() {
        let fixture = Fixture::new();
        let old = fixture.project();
        let mut edited = old.clone();
        edited.stops[0].source_stop_id = 2;
        assert!(validate_update(&old, &edited).is_err());
        edited = old.clone();
        edited.stops[0].source_id = SOURCE_B.into();
        assert!(validate_update(&old, &edited).is_err());
    }

    #[test]
    fn removed_target_id_cannot_be_reused_after_a_later_save() {
        let fixture = Fixture::new();
        let mut old = fixture.project();
        old.stops.retain(|s| s.id != 102);
        let mut edited = old.clone();
        edited.stops.push(ProjectStop {
            id: 102,
            source_id: SOURCE_A.into(),
            source_stop_id: 2,
            division_id: 1,
            name: None,
        });
        assert!(validate_update(&old, &edited).is_err());
        edited.stops.last_mut().unwrap().id = 104;
        edited.next_stop_id = 105;
        assert!(validate_update(&old, &edited).is_ok());
        edited.next_stop_id = 103;
        assert!(validate_update(&old, &edited).is_err());
    }

    #[test]
    fn perspective_selection_preserves_nonperspective_ranks_and_release_metadata() {
        let fixture = Fixture::new();
        let parsed = parse_source(&fixture.dir.join("A.organ"), None).unwrap();
        let mut stop = parsed.definition.stops[0].clone();
        let mut rear = stop.pipes[0].clone();
        if let PipeDef::Sample { extra, .. } = &mut rear {
            extra.releases.push(vpo_sampler::ReleaseDef {
                path: fixture.dir.join("release.wav"),
                max_key_press_time_ms: Some(100),
                cue_point: Some(2),
                release_end: Some(50),
                crossfade_ms: Some(7),
            });
            extra.tremulant_attack = Some(vpo_sampler::AttackDef {
                path: fixture.dir.join("trem.wav"),
                ..Default::default()
            });
        }
        stop.primary_perspective = Some("front".into());
        stop.layers = vec![
            vpo_sampler::PipeLayer {
                index: 1,
                name: "Rear".into(),
                perspective: Some("rear".into()),
                pipes: vec![rear],
            },
            vpo_sampler::PipeLayer {
                index: 2,
                name: "Extra koor".into(),
                perspective: None,
                pipes: stop.pipes.clone(),
            },
        ];
        choose_perspective(&mut stop, Some("rear")).unwrap();
        assert_eq!(stop.layers.len(), 1);
        assert!(stop.all_layers().all(|(_, p, _)| p.is_none()));
        let PipeDef::Sample { extra, .. } = &stop.pipes[0] else {
            panic!("no rear")
        };
        assert_eq!(extra.releases[0].crossfade_ms, Some(7));
        assert!(extra.tremulant_attack.is_some());
        assert_eq!(extra.releases[0].max_key_press_time_ms, Some(100));
    }

    fn add_acoustics(fixture: &Fixture, enclosed_stop: Option<u32>) -> PathBuf {
        let mut odf = OpenOptions::new()
            .append(true)
            .open(fixture.dir.join("A.organ"))
            .unwrap();
        writeln!(odf, "[JMOrgueAcoustics]\nFormat=jm-orgue.acoustics\nSchemaVersion=1\nConfigurationFile=A-acoustics.json").unwrap();
        let config = serde_json::json!({
            "format": "jm-orgue.acoustics", "schema_version": 1,
            "enclosures": [{"id": "box", "name": "Bronkast"}],
            "ranks": [
                {"id": "rank1", "name": "Prestant", "enclosure_id": if enclosed_stop == Some(1) { Some("box") } else { None }},
                {"id": "rank2", "name": "Octaaf", "enclosure_id": if enclosed_stop == Some(2) { Some("box") } else { None }}
            ],
            "pipe_bindings": [
                {"stop_section": "Stop001", "pipe_section": "Pipe001", "layer_index": 0, "logical_pipe": 1, "source_rank_id": "rank1"},
                {"stop_section": "Stop002", "pipe_section": "Pipe001", "layer_index": 0, "logical_pipe": 1, "source_rank_id": "rank2"}
            ]
        });
        let path = fixture.dir.join("A-acoustics.json");
        fs::write(&path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
        path
    }

    #[test]
    fn mixed_acoustics_source_keeps_open_and_enclosed_registers() {
        let fixture = Fixture::new();
        add_acoustics(&fixture, Some(2));
        let catalog = inspect_source(&text_path(&fixture.dir.join("A.organ")), None).unwrap();
        assert_eq!(
            catalog.stops.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        let compiled = compile_project(&fixture.path(), &fixture.project()).unwrap();
        assert_eq!(compiled.definition.stops.len(), 3);
        let cfg = compiled.acoustics.unwrap();
        let map = crate::acoustics::bouw_engine_kaart(
            &cfg,
            &HashMap::from([(101, 0), (102, 0), (103, 0)]),
        );
        assert_eq!(map.pipe_slots.len(), 1);
        assert!(map.pipe_slots.contains_key(&(103, 1)));
        assert!(!map.pipe_slots.contains_key(&(101, 1)));
        assert!(!compiled
            .resource_stamps
            .values()
            .flatten()
            .any(|resource| resource.path.ends_with("A-acoustics.json")));
    }

    #[test]
    fn donor_rank_enclosure_follows_a_reference_when_donor_is_not_selected() {
        let fixture = Fixture::new();
        add_acoustics(&fixture, Some(1));
        let catalog = inspect_source(&text_path(&fixture.dir.join("A.organ")), None).unwrap();
        assert_eq!(catalog.stops.len(), 2);
        let mut project = fixture.project();
        project.stops.retain(|stop| stop.id == 103);
        let compiled = compile_project(&fixture.path(), &project).unwrap();
        let cfg = compiled.acoustics.unwrap();
        assert_eq!(cfg.enclosures.len(), 1);
        assert_eq!(cfg.ranks[0].id, format!("project:{SOURCE_A}:rank:rank1"));
        assert_eq!(cfg.pipe_bindings[0].stop_section, "Stop103");
        assert_eq!(cfg.pipe_bindings[0].layer_index, 0);
        assert_eq!(cfg.pipe_bindings[0].logical_pipe, 1);
    }

    #[test]
    fn registers_uit_bronnen_met_verschillende_kast_en_tremulant_delen_een_divisie() {
        // Bron A krijgt een zwelkast en een tremulant, bron B heeft geen van beide.
        // Tot 0.9.65 weigerde het project dit; nu staan ze samen op één doeldivisie.
        let fixture = Fixture::new();
        let a = fs::read_to_string(fixture.dir.join("A.organ")).unwrap()
            .replace(
                "NumberOfWindchestGroups=1\nNumberOfEnclosures=0\nNumberOfTremulants=0\nPitchCorrection=2",
                "NumberOfWindchestGroups=1\nNumberOfEnclosures=1\nNumberOfTremulants=1\nPitchCorrection=2",
            )
            .replace(
                "[WindchestGroup001]\nName=Open\nNumberOfEnclosures=0\nNumberOfTremulants=0",
                "[Enclosure001]\nName=Zwelkast\nAmpMinimumLevel=20\n[Tremulant001]\nName=Tremulant\nPeriod=160\nStartRate=8\nStopRate=8\nAmpModDepth=18\n[WindchestGroup001]\nName=Kast\nNumberOfEnclosures=1\nEnclosure001=001\nNumberOfTremulants=1\nTremulant001=001",
            );
        assert!(a.contains("Enclosure001=001") && a.contains("Tremulant001=001"));
        fs::write(fixture.dir.join("A.organ"), a).unwrap();
        let project = fixture.project();
        let division = project.divisions[0].id;
        let bronnen: BTreeSet<_> = project.stops.iter()
            .filter(|stop| stop.division_id == division)
            .map(|stop| stop.source_id.clone())
            .collect();
        assert_eq!(bronnen.len(), 2, "de fixture zet registers van beide bronnen op dezelfde divisie");
        let compiled = compile_project(&fixture.path(), &project)
            .expect("registers uit verschillende sets op één divisie moeten laden");
        let manual = compiled.definition.manuals.iter()
            .find(|manual| manual.stop_ids.len() == project.stops.len())
            .expect("alle registers op de ene doeldivisie");
        assert_eq!(manual.tremulant_ids.len(), 1, "de divisie krijgt de tremulant van bron A");
        let met_kast = compiled.definition.stops.iter().filter(|stop| {
            compiled.definition.windchests.iter()
                .find(|wc| wc.number == stop.windchest_group)
                .is_some_and(|wc| !wc.enclosure_ids.is_empty())
        }).count();
        assert!(met_kast >= 1 && met_kast < compiled.definition.stops.len(),
            "alleen de registers van bron A hangen aan de bronkast ({met_kast})");
        // Opslaan en opnieuw openen blijft werken.
        let saved = save_project(&fixture.path(), project).unwrap();
        compile_project(&fixture.path(), &saved).unwrap();
    }

    #[test]
    fn automatisch_gelijkstemmen_wordt_bewaard_en_per_register_doorgegeven() {
        let fixture = Fixture::new();
        let mut project = fixture.project();
        project.sources[0].tuning_policy = TuningPolicy::Auto;
        let saved = save_project(&fixture.path(), project).unwrap();
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.path()).unwrap()).unwrap();
        assert_eq!(stored["sources"][0]["tuning_policy"], "auto");
        let reopened = load_project(&fixture.path()).unwrap();
        assert_eq!(reopened.sources[0].tuning_policy, TuningPolicy::Auto);
        let compiled = compile_project(&fixture.path(), &saved).unwrap();
        let auto_bron = &saved.sources[0].id;
        for stop in &saved.stops {
            let verwacht = if &stop.source_id == auto_bron { TuningPolicy::Auto } else { TuningPolicy::Normalize };
            assert_eq!(compiled.per_stop_tuning[&stop.id].policy, verwacht);
        }
    }

    #[test]
    fn duplicate_enclosure_ids_from_two_sources_remain_independent_on_one_manual() {
        let fixture = Fixture::new();
        let a_cfg = add_acoustics(&fixture, Some(1));
        let mut b = OpenOptions::new()
            .append(true)
            .open(fixture.dir.join("B.organ"))
            .unwrap();
        writeln!(b, "[JMOrgueAcoustics]\nConfigurationFile=B-acoustics.json").unwrap();
        fs::copy(a_cfg, fixture.dir.join("B-acoustics.json")).unwrap();
        let mut project = fixture.project();
        let first = compile_project(&fixture.path(), &project).unwrap();
        let cfg = first.acoustics.unwrap();
        assert_eq!(cfg.enclosures.len(), 2);
        assert_eq!(
            cfg.enclosures[0].id,
            format!("project:{SOURCE_A}:enclosure:box")
        );
        assert_eq!(
            cfg.enclosures[1].id,
            format!("project:{SOURCE_B}:enclosure:box")
        );
        let summary = crate::acoustics::samenvatting(
            &cfg,
            &HashMap::from([(101, vec![1]), (102, vec![1]), (103, vec![1])]),
        );
        assert!(summary.valid, "{:?}", summary.error);
        assert_ne!(
            summary.enclosures[0].swell_name,
            summary.enclosures[1].swell_name
        );
        let map = crate::acoustics::bouw_engine_kaart(
            &cfg,
            &HashMap::from([(101, 0), (102, 0), (103, 0)]),
        );
        assert_ne!(map.pipe_slots[&(101, 1)], map.pipe_slots[&(102, 1)]);
        assert_eq!(map.pipe_slots[&(101, 1)], map.pipe_slots[&(103, 1)]);
        assert_eq!(map.parent_division, vec![0, 0]);
        project.sources.reverse();
        project.stops.reverse();
        let reordered = compile_project(&fixture.path(), &project)
            .unwrap()
            .acoustics
            .unwrap();
        let reordered_map = crate::acoustics::bouw_engine_kaart(
            &reordered,
            &HashMap::from([(101, 0), (102, 0), (103, 0)]),
        );
        assert_eq!(map.pipe_slots, reordered_map.pipe_slots);
    }

    #[test]
    fn chosen_rear_microphone_moves_its_enclosure_binding_to_primary_layer() {
        let fixture = Fixture::new();
        fs::write(fixture.dir.join("A.organ"), "[Organ]\nChurchName=Bron A\nNumberOfManuals=1\nNumberOfWindchestGroups=1\n[WindchestGroup001]\nNumberOfEnclosures=0\nNumberOfTremulants=0\n[Manual001]\nName=Manual\nNumberOfStops=1\nStop001=001\nNumberOfLogicalKeys=1\nNumberOfAccessibleKeys=1\nFirstAccessibleKeyMIDINoteNumber=60\n[Stop001]\nName=Prestant 8\nNumberOfRanks=2\nRank001=001\nRank002=002\nNumberOfLogicalPipes=1\nNumberOfAccessiblePipes=1\n[Rank001]\nName=Prestant 8 (front)\nWindchestGroup=1\nNumberOfLogicalPipes=1\nFirstMidiNoteNumber=60\nPipe001=a.wav\n[Rank002]\nName=Prestant 8 (rear)\nWindchestGroup=1\nNumberOfLogicalPipes=1\nFirstMidiNoteNumber=60\nPipe001=b.wav\n[JMOrgueAcoustics]\nConfigurationFile=A-acoustics.json\n").unwrap();
        let cfg = serde_json::json!({"format":"jm-orgue.acoustics","schema_version":1,
            "enclosures":[{"id":"front-box","name":"Front"},{"id":"rear-box","name":"Rear"}],
            "ranks":[{"id":"front","enclosure_id":"front-box"},{"id":"rear","enclosure_id":"rear-box"}],
            "pipe_bindings":[{"stop_section":"Stop001","pipe_section":"Pipe001","source_rank_id":"front","layer_index":0},
                {"stop_section":"Stop001","pipe_section":"Pipe001","source_rank_id":"rear","layer_index":1}]});
        fs::write(
            fixture.dir.join("A-acoustics.json"),
            serde_json::to_vec(&cfg).unwrap(),
        )
        .unwrap();
        let mut project = fixture.project();
        project.stops.retain(|stop| stop.id != 103);
        project.sources[0].perspective = Some("rear".into());
        let compiled = compile_project(&fixture.path(), &project).unwrap();
        let cfg = compiled.acoustics.unwrap();
        assert_eq!(cfg.enclosures.len(), 1);
        assert!(cfg.enclosures[0].id.ends_with(":rear-box"));
        assert_eq!(cfg.pipe_bindings.len(), 1);
        assert_eq!(cfg.pipe_bindings[0].layer_index, 0);
        assert_eq!(cfg.pipe_bindings[0].stop_section, "Stop101");
        assert_eq!(compiled.definition.get_stop(101).unwrap().layers.len(), 0);
    }

    #[test]
    fn enclosure_budget_counts_selected_boxes_and_never_silently_truncates() {
        let fixture = Fixture::new();
        let mut odf = "[Organ]\nChurchName=Veel kasten\nNumberOfManuals=1\nNumberOfWindchestGroups=1\n[WindchestGroup001]\nNumberOfEnclosures=0\n[Manual001]\nName=Manual\nNumberOfStops=17\nNumberOfLogicalKeys=1\nNumberOfAccessibleKeys=1\nFirstAccessibleKeyMIDINoteNumber=60\n".to_string();
        for id in 1..=17 {
            odf.push_str(&format!("Stop{id:03}={id:03}\n"));
        }
        for id in 1..=17 {
            odf.push_str(&format!("[Stop{id:03}]\nName=Register {id}\nWindchestGroup=1\nNumberOfLogicalPipes=1\nNumberOfAccessiblePipes=1\nPipe001=a.wav\n"));
        }
        odf.push_str("[JMOrgueAcoustics]\nConfigurationFile=A-acoustics.json\n");
        fs::write(fixture.dir.join("A.organ"), odf).unwrap();
        let cfg = serde_json::json!({"format":"jm-orgue.acoustics","schema_version":1,
            "enclosures":(1..=17).map(|id| serde_json::json!({"id":id.to_string(),"name":format!("Kast {id}")})).collect::<Vec<_>>(),
            "ranks":(1..=17).map(|id| serde_json::json!({"id":id.to_string(),"enclosure_id":id.to_string()})).collect::<Vec<_>>(),
            "pipe_bindings":(1..=17).map(|id| serde_json::json!({"stop_section":format!("Stop{id:03}"),"pipe_section":"Pipe001","source_rank_id":id.to_string()})).collect::<Vec<_>>()});
        fs::write(
            fixture.dir.join("A-acoustics.json"),
            serde_json::to_vec(&cfg).unwrap(),
        )
        .unwrap();
        let mut project = fixture.project();
        project.stops = (1..=17)
            .map(|id| ProjectStop {
                id,
                source_id: SOURCE_A.into(),
                source_stop_id: id,
                division_id: 1,
                name: None,
            })
            .collect();
        assert!(compile_project(&fixture.path(), &project)
            .err()
            .unwrap()
            .contains("17 rankzwelkasten"));
        project.stops.pop();
        let cfg = compile_project(&fixture.path(), &project)
            .unwrap()
            .acoustics
            .unwrap();
        assert_eq!(cfg.enclosures.len(), 16);
        assert_eq!(cfg.pipe_bindings.len(), 16);
    }

    #[test]
    fn source_uuid_case_cannot_create_colliding_enclosure_namespaces() {
        let fixture = Fixture::new();
        let mut project = fixture.project();
        project.sources[1].id = SOURCE_A.to_ascii_uppercase();
        assert!(validate_project(&project)
            .unwrap_err()
            .contains("unieke UUID"));
    }

    #[test]
    fn acoustics_without_active_rank_enclosure_remains_selectable_and_is_fingerprinted() {
        let fixture = Fixture::new();
        let config = add_acoustics(&fixture, None);
        let catalog = inspect_source(&text_path(&fixture.dir.join("A.organ")), None).unwrap();
        assert_eq!(catalog.stops.len(), 2);
        assert!(catalog
            .warnings
            .iter()
            .any(|w| w.contains("niet DSP-actief")));
        let project = fixture.project();
        compile_project(&fixture.path(), &project).unwrap();
        let mut data: serde_json::Value =
            serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        data["definition_revision"] = serde_json::json!(2);
        fs::write(&config, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(compile_project(&fixture.path(), &project)
            .err()
            .unwrap()
            .contains("is gewijzigd"));
        let refreshed = inspect_source(&text_path(&fixture.dir.join("A.organ")), None).unwrap();
        assert_ne!(catalog.fingerprint, refreshed.fingerprint);
    }

    #[test]
    fn missing_or_invalid_acoustics_is_rejected_instead_of_silently_ignored() {
        let fixture = Fixture::new();
        let config = add_acoustics(&fixture, None);
        fs::write(&config, b"{invalid json").unwrap();
        let error = inspect_source(&text_path(&fixture.dir.join("A.organ")), None)
            .err()
            .unwrap();
        assert!(error.contains("Akoestische bronconfiguratie"), "{error}");
        fs::remove_file(&config).unwrap();
        let error = inspect_source(&text_path(&fixture.dir.join("A.organ")), None)
            .err()
            .unwrap();
        assert!(error.contains("Akoestische bronconfiguratie"), "{error}");
    }

    #[test]
    fn project_capacity_accepts_2048_registers_and_rejects_overflow() {
        let fixture = Fixture::new();
        let mut project = fixture.project();
        let template = project.stops[0].clone();
        project.stops = (1..=2048)
            .map(|id| ProjectStop {
                id,
                ..template.clone()
            })
            .collect();
        project.next_stop_id = 2050;
        validate_project(&project).unwrap();
        project.stops.push(ProjectStop {
            id: 2049,
            ..template.clone()
        });
        let error = validate_project(&project).unwrap_err();
        assert!(error.contains("2048"), "{error}");
        project.stops.truncate(3);
        project.couplers = (1..=513)
            .map(|id| ProjectCoupler {
                id,
                name: format!("Koppel {id}"),
                source_division_id: 1,
                destination_division_id: 1,
                keyshift: 0,
            })
            .collect();
        assert!(validate_project(&project)
            .unwrap_err()
            .contains("512 eigen koppels"));
    }

    #[test]
    fn default_unison_couplers_keep_persistent_division_identity() {
        let fixture = Fixture::new();
        let mut project = fixture.project();
        project.divisions = vec![
            ProjectDivision {
                id: 1,
                name: "Pedaal".into(),
                midi_channel: 1,
                first_midi_note: 36,
                key_count: 30,
                pedal: true,
            },
            ProjectDivision {
                id: 2,
                name: "Hoofdwerk".into(),
                midi_channel: 2,
                first_midi_note: 36,
                key_count: 61,
                pedal: false,
            },
            ProjectDivision {
                id: 3,
                name: "Nevenwerk".into(),
                midi_channel: 3,
                first_midi_note: 36,
                key_count: 61,
                pedal: false,
            },
        ];
        project.next_division_id = 4;
        for stop in &mut project.stops {
            stop.division_id = 2;
        }
        let first = compile_project(&fixture.path(), &project).unwrap();
        let expected = vec![(1u32 << 16) | 2, (1u32 << 16) | 3, (2u32 << 16) | 3];
        assert_eq!(
            first
                .definition
                .couplers
                .iter()
                .map(|c| c.number)
                .collect::<Vec<_>>(),
            expected
        );
        let pedal_to_great = &first.definition.couplers[0];
        assert_eq!(
            (
                pedal_to_great.source_manual,
                pedal_to_great.destination_manual
            ),
            (0, 1)
        );
        assert_eq!(pedal_to_great.destination_keyshift, 0);
        project.divisions[1].name = "Concertwerk".into();
        project.divisions.reverse();
        let renamed = compile_project(&fixture.path(), &project).unwrap();
        assert_eq!(
            renamed
                .definition
                .couplers
                .iter()
                .map(|c| c.number)
                .collect::<Vec<_>>(),
            expected
        );
        project.divisions.retain(|d| d.id != 3);
        let reduced = compile_project(&fixture.path(), &project).unwrap();
        assert_eq!(reduced.definition.couplers.len(), 1);
        assert_eq!(reduced.definition.couplers[0].number, expected[0]);
        project.couplers = vec![ProjectCoupler {
            id: 77,
            name: "Eigen koppel".into(),
            source_division_id: 1,
            destination_division_id: 2,
            keyshift: 0,
        }];
        let explicit = compile_project(&fixture.path(), &project).unwrap();
        assert_eq!(explicit.definition.couplers.len(), 1);
        assert_eq!(explicit.definition.couplers[0].number, 77);
    }
}
