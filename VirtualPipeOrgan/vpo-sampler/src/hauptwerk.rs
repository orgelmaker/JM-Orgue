//! Hauptwerk ODF (`*.Organ_Hauptwerk_xml`) importer.
//!
//! Hauptwerk stores an organ as a big flat XML of typed object lists with an
//! ID-based relational model. To play a stop we follow:
//!
//! ```text
//! Stop ──(StopRank: RankID + MIDI range)──▶ Rank
//!   Rank ──▶ Pipe_SoundEngine01 (RankID, NormalMIDINoteNumber)
//!     Pipe ──▶ Pipe_SoundEngine01_Layer (PipeID)
//!       Layer ──▶ Pipe_SoundEngine01_AttackSample (LayerID, SampleID)
//!         Sample (SampleFilename + InstallationPackageID) ──▶ file on disk
//! ```
//!
//! Rather than introduce a second loading path, this module translates the
//! Hauptwerk model into the GrandOrgue [`OrganDefinition`] used elsewhere, so the
//! existing streaming preload + UI + noise filter all work unchanged. Each stop
//! becomes a [`StopDef`] with its own `first_midi_note` and a per-key pipe list
//! (absolute sample paths; missing keys become [`PipeDef::Empty`]).
//!
//! Only the primary attack sample of each pipe's first layer is used. Release
//! samples and multiple velocity/mic layers are ignored for now (the engine does
//! its own release envelope) — see the README/handover for the roadmap.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::grandorgue::{
    CouplerDef, ManualDef, OdfError, OrganDefinition, OrganInfo, PipeDef, StopDef,
};

/// Load a Hauptwerk organ definition and translate it to an [`OrganDefinition`].
pub fn load_hauptwerk(odf_path: &Path) -> Result<OrganDefinition, OdfError> {
    let xml = std::fs::read_to_string(odf_path)?;

    // Resolve the package root: the ancestor directory that contains
    // `OrganInstallationPackages` (the ODF may live in `OrganDefinitions/` or at
    // the package top level).
    let root = find_package_root(odf_path).ok_or_else(|| {
        OdfError::MissingField("OrganInstallationPackages directory".into())
    })?;
    let pkg_dirs = scan_package_dirs(&root);

    // --- Parse the object lists we need ---
    let general = extract_objects(&xml, "_General");
    let divisions = extract_objects(&xml, "Division");
    let stops = extract_objects(&xml, "Stop");
    let stopranks = extract_objects(&xml, "StopRank");
    let pipes = extract_objects(&xml, "Pipe_SoundEngine01");
    let layers = extract_objects(&xml, "Pipe_SoundEngine01_Layer");
    let attack_samples = extract_objects(&xml, "Pipe_SoundEngine01_AttackSample");
    let samples = extract_objects(&xml, "Sample");

    info!(
        "Hauptwerk parse: {} divisions, {} stops, {} stopranks, {} pipes, {} samples",
        divisions.len(), stops.len(), stopranks.len(), pipes.len(), samples.len()
    );

    // --- Build lookup indices ---

    // rank_id -> (normal_midi_note -> pipe_id)
    let mut pipes_by_rank: HashMap<u32, HashMap<u32, u32>> = HashMap::new();
    for p in &pipes {
        if let (Some(rid), Some(pid), Some(note)) = (
            p.get_u32("RankID"),
            p.get_u32("PipeID"),
            p.get_u32("NormalMIDINoteNumber"),
        ) {
            pipes_by_rank.entry(rid).or_default().insert(note, pid);
        }
    }
    // rank_id -> base pitch harmonic (from its first pipe), for footage display
    let mut rank_harmonic: HashMap<u32, u32> = HashMap::new();
    for p in &pipes {
        if let (Some(rid), Some(h)) = (
            p.get_u32("RankID"),
            p.get_u32("Pitch_Tempered_RankBasePitch64ftHarmonicNum"),
        ) {
            rank_harmonic.entry(rid).or_insert(h);
        }
    }
    // pipe_id -> layer_id (first layer)
    let mut layer_of_pipe: HashMap<u32, u32> = HashMap::new();
    for l in &layers {
        if let (Some(pid), Some(lid)) = (l.get_u32("PipeID"), l.get_u32("LayerID")) {
            layer_of_pipe.entry(pid).or_insert(lid);
        }
    }
    // layer_id -> sample_id (first attack sample)
    let mut sample_of_layer: HashMap<u32, u32> = HashMap::new();
    for a in &attack_samples {
        if let (Some(lid), Some(sid)) = (a.get_u32("LayerID"), a.get_u32("SampleID")) {
            sample_of_layer.entry(lid).or_insert(sid);
        }
    }
    // sample_id -> (installation_package_id, relative filename)
    let mut sample_file: HashMap<u32, (u32, String)> = HashMap::new();
    for s in &samples {
        if let (Some(sid), Some(fname)) =
            (s.get_u32("SampleID"), s.get("SampleFilename"))
        {
            let pkg = s.get_u32("InstallationPackageID").unwrap_or(0);
            sample_file.insert(sid, (pkg, fname.clone()));
        }
    }

    // Resolve a pipe to an absolute sample path via the full chain.
    let resolve_pipe_path = |pipe_id: u32| -> Option<PathBuf> {
        let layer_id = *layer_of_pipe.get(&pipe_id)?;
        let sample_id = *sample_of_layer.get(&layer_id)?;
        let (pkg, fname) = sample_file.get(&sample_id)?;
        let dir = pkg_dirs.get(pkg)?;
        Some(dir.join(fname.replace('\\', "/")))
    };

    // stop_id -> its stopranks
    let mut stopranks_by_stop: HashMap<u32, Vec<&FieldMap>> = HashMap::new();
    for sr in &stopranks {
        if let Some(sid) = sr.get_u32("StopID") {
            stopranks_by_stop.entry(sid).or_default().push(sr);
        }
    }

    // --- Build StopDefs (paired with their division id for grouping) ---
    let mut stop_entries: Vec<(u32, StopDef)> = Vec::new();
    for st in &stops {
        let stop_id = match st.get_u32("StopID") {
            Some(v) => v,
            None => continue,
        };
        let name = clean_stop_name(st.get("Name").map(|s| s.as_str()).unwrap_or(""));

        // Collect (division_note -> sample path) across all stopranks of this stop.
        let mut by_note: HashMap<u32, PathBuf> = HashMap::new();
        let mut harmonic = 0u32;
        for sr in stopranks_by_stop.get(&stop_id).into_iter().flatten() {
            let rank_id = match sr.get_u32("RankID") {
                Some(v) => v,
                None => continue,
            };
            let rank_pipes = match pipes_by_rank.get(&rank_id) {
                Some(m) => m,
                None => continue,
            };
            if harmonic == 0 {
                if let Some(h) = rank_harmonic.get(&rank_id) {
                    harmonic = *h;
                }
            }
            let first_div = sr.get_u32("MIDINoteNumOfFirstMappedDivisionInputNode").unwrap_or(36);
            let count = sr.get_u32("NumberOfMappedDivisionInputNodes").unwrap_or(0);
            let inc = sr.get_i32("MIDINoteNumIncrementFromDivisionToRank").unwrap_or(0);
            for k in 0..count {
                let div_note = first_div + k;
                let rank_note = (div_note as i64 + inc as i64) as u32;
                if let Some(pid) = rank_pipes.get(&rank_note) {
                    if let Some(path) = resolve_pipe_path(*pid) {
                        by_note.entry(div_note).or_insert(path);
                    }
                }
            }
        }

        if by_note.is_empty() {
            warn!("Hauptwerk stop '{}' (id {}) resolved no samples — skipping", name, stop_id);
            continue;
        }

        // Dense, keyboard-aligned pipe list from the stop's lowest to highest note;
        // gaps within the compass become Empty so key→pipe alignment is preserved.
        let min_note = *by_note.keys().min().unwrap();
        let max_note = *by_note.keys().max().unwrap();
        let mut pipe_list: Vec<PipeDef> = Vec::with_capacity((max_note - min_note + 1) as usize);
        for note in min_note..=max_note {
            match by_note.get(&note) {
                Some(path) => pipe_list.push(PipeDef::Sample {
                    path: path.clone(),
                    amplitude_level: None,
                    pitch_correction: None,
                }),
                None => pipe_list.push(PipeDef::Empty),
            }
        }

        let division_id = st.get_u32("DivisionID").unwrap_or(0);
        stop_entries.push((division_id, StopDef {
            id: stop_id,
            name,
            harmonic_number: if harmonic > 0 { harmonic } else { 8 },
            pitch_correction: 0,
            number_of_pipes: pipe_list.len() as u32,
            first_accessible_pipe_logical_key: 1,
            windchest_group: 1,
            amplitude_level: 100.0,
            percussive: false,
            accepts_retuning: true,
            first_midi_note: Some(min_note),
            pipes: pipe_list,
        }));
    }

    // --- Build manuals (one per division), in division order; pedal => number 0 ---
    let mut manuals: Vec<ManualDef> = Vec::new();
    let mut manual_counter = 1u32;
    let mut div_to_manual_number: HashMap<u32, u32> = HashMap::new();
    for d in &divisions {
        let division_id = match d.get_u32("DivisionID") {
            Some(v) => v,
            None => continue,
        };
        let div_name = clean_division_name(d.get("Name").map(|s| s.as_str()).unwrap_or(""));
        let is_pedal = {
            let l = div_name.to_lowercase();
            l.contains("pedal") || l.contains("pédale") || l.contains("pedale") || l.contains("ped")
        };
        let number = if is_pedal { 0 } else { let n = manual_counter; manual_counter += 1; n };

        // Stops of this division.
        let stop_ids: Vec<u32> = stop_entries.iter()
            .filter(|(did, _)| *did == division_id)
            .map(|(_, s)| s.id)
            .collect();
        if stop_ids.is_empty() {
            continue;
        }
        let first_key = stop_entries.iter()
            .filter(|(did, _)| *did == division_id)
            .filter_map(|(_, s)| s.first_midi_note)
            .min()
            .unwrap_or(36);

        div_to_manual_number.insert(division_id, number);
        manuals.push(ManualDef {
            number,
            name: div_name,
            midi_input_number: number + 1,
            number_of_logical_keys: 61,
            number_of_accessible_keys: 61,
            first_accessible_key_midi_note: first_key,
            stop_ids,
            coupler_ids: Vec::new(),
            tremulant_ids: Vec::new(),
        });
    }
    manuals.sort_by_key(|m| m.number);

    // --- Couplers from KeyAction routing ---
    // A KeyAction couples a source keyboard to a destination division. The base
    // (condition-less) actions tell us which division each keyboard normally
    // plays; the ones WITH a ConditionSwitchID are the actual user couplers
    // (e.g. "Pedal-Coppel"). MIDINoteNumberIncrement is the octave shift.
    let key_actions = extract_objects(&xml, "KeyAction");
    let mut kbd_to_div: HashMap<u32, u32> = HashMap::new();
    for ka in &key_actions {
        let cond = ka.get("ConditionSwitchID").map(|s| s.trim()).unwrap_or("");
        let dest_is_kbd = ka.get("DestIsKeyboardNotDivision").map(|s| s == "Y").unwrap_or(false);
        if cond.is_empty() && !dest_is_kbd {
            if let (Some(kid), Some(did)) = (ka.get_u32("SourceKeyboardID"), ka.get_u32("DestDivisionID")) {
                kbd_to_div.entry(kid).or_insert(did);
            }
        }
    }
    let mut coupler_defs: Vec<CouplerDef> = Vec::new();
    let mut coupler_num = 1u32;
    let mut manual_coupler_refs: HashMap<u32, Vec<u32>> = HashMap::new();
    for ka in &key_actions {
        let cond = ka.get("ConditionSwitchID").map(|s| s.trim()).unwrap_or("");
        let dest_is_kbd = ka.get("DestIsKeyboardNotDivision").map(|s| s == "Y").unwrap_or(false);
        if cond.is_empty() || dest_is_kbd {
            continue; // base routing or keyboard-to-keyboard, not a user coupler
        }
        let src_kbd = match ka.get_u32("SourceKeyboardID") { Some(v) => v, None => continue };
        let dest_div = match ka.get_u32("DestDivisionID") { Some(v) => v, None => continue };
        let src_div = match kbd_to_div.get(&src_kbd) { Some(v) => *v, None => continue };
        let src_manual = match div_to_manual_number.get(&src_div) { Some(v) => *v, None => continue };
        let dest_manual = match div_to_manual_number.get(&dest_div) { Some(v) => *v, None => continue };
        let offset = ka.get_i32("MIDINoteNumberIncrement").unwrap_or(0);
        let name = clean_stop_name(ka.get("Name").map(|s| s.as_str()).unwrap_or(""));
        coupler_defs.push(CouplerDef {
            number: coupler_num,
            name,
            source_manual: src_manual,
            destination_manual: dest_manual,
            destination_keyshift: offset,
            unison_off: false,
        });
        manual_coupler_refs.entry(src_manual).or_default().push(coupler_num);
        coupler_num += 1;
    }
    for m in &mut manuals {
        if let Some(ids) = manual_coupler_refs.get(&m.number) {
            m.coupler_ids = ids.clone();
        }
    }
    if !coupler_defs.is_empty() {
        info!("Hauptwerk: {} couplers from KeyAction routing", coupler_defs.len());
    }

    // --- Organ info from _General ---
    let g = general.first();
    let getg = |k: &str| g.and_then(|m| m.get(k)).cloned().unwrap_or_default();
    let organ = OrganInfo {
        church_name: clean(&getg("Identification_Name")),
        church_address: clean(&getg("OrganInfo_Location")),
        organ_builder: clean(&getg("OrganInfo_Builder")),
        organ_build_date: clean(&getg("OrganInfo_BuildDate")),
        organ_comments: clean(&getg("OrganInfo_Comments")),
        recording_details: clean(&getg("Control_OrganDefinitionSupplierName")),
        number_of_manuals: manuals.iter().filter(|m| m.number > 0).count() as u32,
        has_pedals: manuals.iter().any(|m| m.number == 0),
        number_of_enclosures: 0,
        number_of_tremulants: 0,
        number_of_windchest_groups: divisions.len() as u32,
        amplitude_level: 100.0,
    };

    let stop_defs: Vec<StopDef> = stop_entries.into_iter().map(|(_, s)| s).collect();
    let total_pipes: usize = stop_defs.iter().map(|s| s.pipes.len()).sum();
    info!(
        "Hauptwerk import: '{}' — {} stops in {} divisions, {} pipes",
        organ.church_name, stop_defs.len(), manuals.len(), total_pipes
    );

    Ok(OrganDefinition {
        base_path: root,
        organ,
        manuals,
        stops: stop_defs,
        windchests: Vec::new(),
        enclosures: Vec::new(),
        tremulants: Vec::new(),
        couplers: coupler_defs,
    })
}

/// Quick check: does this path look like a Hauptwerk organ definition?
pub fn is_hauptwerk_path(path: &Path) -> bool {
    path.to_string_lossy().to_lowercase().contains("hauptwerk")
}

// ============================================================================
// XML scanning (dependency-free; the format is flat typed object lists)
// ============================================================================

type FieldMap = HashMap<String, String>;

trait FieldMapExt {
    fn get_u32(&self, key: &str) -> Option<u32>;
    fn get_i32(&self, key: &str) -> Option<i32>;
}
impl FieldMapExt for FieldMap {
    fn get_u32(&self, key: &str) -> Option<u32> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }
    fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }
}

/// Extract every object of `object_type` from its `<ObjectList ObjectType="..">`
/// as a flat field map. Object lists are not nested and each object body holds
/// only flat `<field>value</field>` pairs.
fn extract_objects(xml: &str, object_type: &str) -> Vec<FieldMap> {
    let mut out = Vec::new();
    let list_open = format!("<ObjectList ObjectType=\"{}\">", object_type);
    let list_start = match xml.find(&list_open) {
        Some(i) => i + list_open.len(),
        None => return out,
    };
    let list_end = xml[list_start..]
        .find("</ObjectList>")
        .map(|i| list_start + i)
        .unwrap_or(xml.len());
    let body = &xml[list_start..list_end];

    let obj_open = format!("<{}>", object_type);
    let obj_close = format!("</{}>", object_type);
    let mut pos = 0;
    while let Some(s) = body[pos..].find(&obj_open) {
        let obj_start = pos + s + obj_open.len();
        let e = match body[obj_start..].find(&obj_close) {
            Some(i) => obj_start + i,
            None => break,
        };
        out.push(parse_fields(&body[obj_start..e]));
        pos = e + obj_close.len();
    }
    out
}

/// Parse a flat object body (`<a>x</a><b>y</b>...`) into a field map, decoding
/// XML/HTML entities in the values.
fn parse_fields(obj_body: &str) -> FieldMap {
    let mut map = FieldMap::new();
    let mut rest = obj_body;
    while let Some(lt) = rest.find('<') {
        let after = &rest[lt + 1..];
        let gt = match after.find('>') {
            Some(j) => j,
            None => break,
        };
        let tag = &after[..gt];
        // Skip closing tags / malformed.
        if tag.starts_with('/') || tag.is_empty() {
            rest = &after[gt + 1..];
            continue;
        }
        // Self-closing empty field <Foo/> (some Hauptwerk sets use this instead of
        // <Foo></Foo>). Record it as empty and move on — NOT doing so made the
        // parser stop at the first such tag and drop every field after it.
        if let Some(name) = tag.strip_suffix('/') {
            map.insert(name.trim().to_string(), String::new());
            rest = &after[gt + 1..];
            continue;
        }
        let val_start = &after[gt + 1..];
        let close = format!("</{}>", tag);
        match val_start.find(&close) {
            Some(ce) => {
                let raw = &val_start[..ce];
                map.insert(tag.to_string(), decode_entities(raw));
                rest = &val_start[ce + close.len()..];
            }
            None => break,
        }
    }
    map
}

/// Decode the XML/HTML entities Hauptwerk uses (numeric `&#xHH;` / `&#NN;` plus
/// the five predefined named ones).
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(semi) = after.find(';') {
            let ent = &after[1..semi];
            let decoded = if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = ent.strip_prefix('#') {
                dec.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => None,
                }
            };
            match decoded {
                Some(c) => {
                    out.push(c);
                    rest = &after[semi + 1..];
                }
                None => {
                    // Unknown entity — keep the '&' and continue past it.
                    out.push('&');
                    rest = &after[1..];
                }
            }
        } else {
            out.push('&');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    out
}

fn clean(s: &str) -> String {
    s.trim().to_string()
}

/// Known organ-division abbreviations that some sample sets prepend to stop
/// names (Hauptwerk/Schwellwerk/Pedal/Récit/…). Used to strip the prefix even
/// when only a single space separates it from the name.
const DIVISION_ABBREVIATIONS: &[&str] = &[
    "HW", "SW", "OW", "BW", "RP", "GO", "PED", "REC", "POS", "CH", "SO",
    "PO", "GW", "NW", "ECH", "BOM", "P", "M", "R", "G",
];

/// Clean a stop name: strip a leading division prefix like "P  ", "M  ",
/// "PED  ", "GO  ", or "HW " / "SW " (known abbreviation + 1+ spaces; any other
/// 1-4 letter token only when followed by 2+ spaces, to avoid false positives).
/// Shared with the GrandOrgue route (German sets prefix register names too).
pub fn clean_stop_name(raw: &str) -> String {
    let s = raw.trim();
    let bytes = s.as_bytes();
    // Count leading ASCII letters.
    let letters = bytes.iter().take_while(|b| b.is_ascii_alphabetic()).count();
    if (1..=4).contains(&letters) {
        let prefix = s[..letters].to_uppercase();
        let after = &s[letters..];
        let spaces = after.bytes().take_while(|b| *b == b' ').count();
        let known = DIVISION_ABBREVIATIONS.contains(&prefix.as_str());
        if spaces >= 2 || (known && spaces >= 1) {
            return after[spaces..].trim().to_string();
        }
    }
    s.to_string()
}

/// Clean a division name: drop a leading "N. " ordinal (e.g. "1. Pédale",
/// "2. Hauptwerk"). Shared with the GrandOrgue route.
pub fn clean_division_name(raw: &str) -> String {
    let s = raw.trim();
    if let Some(dot) = s.find(". ") {
        if s[..dot].chars().all(|c| c.is_ascii_digit()) && dot <= 2 {
            return s[dot + 2..].trim().to_string();
        }
    }
    s.to_string()
}

/// Find the ancestor directory containing `OrganInstallationPackages`.
fn find_package_root(odf_path: &Path) -> Option<PathBuf> {
    let mut dir = odf_path.parent();
    while let Some(d) = dir {
        if d.join("OrganInstallationPackages").is_dir() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Map each InstallationPackageID to its on-disk folder by scanning
/// `OrganInstallationPackages/*/` for an `*InstallationPackageDefinition_Hauptwerk_xml`
/// marker (handles both numeric "000690" and named "Puttershoek Herv. Gem"
/// folders). Falls back to a sole subfolder.
fn scan_package_dirs(root: &Path) -> HashMap<u32, PathBuf> {
    let mut map = HashMap::new();
    let base = root.join("OrganInstallationPackages");
    let entries: Vec<PathBuf> = match std::fs::read_dir(&base) {
        Ok(rd) => rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect(),
        Err(_) => return map,
    };
    for dir in &entries {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for f in rd.flatten() {
                let fname = f.file_name().to_string_lossy().to_lowercase();
                if fname.contains("installationpackagedefinition") {
                    // packageid000690.* / packageid002219.* — take the digit run.
                    if let Some(id) = first_number(&fname) {
                        map.insert(id, dir.clone());
                    }
                }
            }
        }
    }
    // Fallback: a single package folder with no parsable marker.
    if map.is_empty() && entries.len() == 1 {
        // 0 is the "unknown package" key used when a Sample omits the id.
        map.insert(0, entries[0].clone());
    }
    map
}

/// First run of digits in `s` parsed as u32 (e.g. "packageid000690..." -> 690).
fn first_number(s: &str) -> Option<u32> {
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let end = s[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map(|i| start + i)
        .unwrap_or(s.len());
    s[start..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_entities() {
        assert_eq!(decode_entities("Subba&#xDF; 16"), "Subbaß 16");
        assert_eq!(decode_entities("L&#x119;dziny"), "Lędziny");
        assert_eq!(decode_entities("Fl&#xFB;te"), "Flûte");
        assert_eq!(decode_entities("a &amp; b"), "a & b");
        assert_eq!(decode_entities("plain"), "plain");
    }

    #[test]
    fn test_clean_stop_name() {
        assert_eq!(clean_stop_name("P  Subbass 16"), "Subbass 16");
        assert_eq!(clean_stop_name("PED  Soubasse 16"), "Soubasse 16");
        assert_eq!(clean_stop_name("GO  Bourdon 8"), "Bourdon 8");
        // Known division abbreviations are stripped even with a single space.
        assert_eq!(clean_stop_name("HW Prinzipal 8"), "Prinzipal 8");
        assert_eq!(clean_stop_name("SW Gedackt 8"), "Gedackt 8");
        // Long names and unknown 1-space prefixes are left intact.
        assert_eq!(clean_stop_name("Principal 8"), "Principal 8");
        assert_eq!(clean_stop_name("Open Diapason"), "Open Diapason");
    }

    #[test]
    fn test_clean_division_name() {
        assert_eq!(clean_division_name("1. Pédale"), "Pédale");
        assert_eq!(clean_division_name("2. Grand Orgue"), "Grand Orgue");
        assert_eq!(clean_division_name("Pedal"), "Pedal");
    }

    #[test]
    fn test_extract_objects() {
        let xml = r#"<Hauptwerk><ObjectList ObjectType="Stop"><Stop><StopID>1</StopID><Name>A</Name></Stop><Stop><StopID>2</StopID><Name>B</Name></Stop></ObjectList></Hauptwerk>"#;
        let stops = extract_objects(xml, "Stop");
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].get_u32("StopID"), Some(1));
        assert_eq!(stops[1].get("Name").map(|s| s.as_str()), Some("B"));
    }

    #[test]
    fn test_first_number() {
        assert_eq!(first_number("packageid000690.installationpackagedefinition"), Some(690));
        assert_eq!(first_number("packageid002219.foo"), Some(2219));
        assert_eq!(first_number("noidea"), None);
    }

    /// Local smoke test against a real Hauptwerk set. Set JM_HW_TEST_ODF to the
    /// `.Organ_Hauptwerk_xml` path and run with `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn smoke_load_real_hauptwerk() {
        let path = match std::env::var("JM_HW_TEST_ODF") {
            Ok(p) => p,
            Err(_) => return,
        };
        let def = load_hauptwerk(Path::new(&path)).expect("load");
        println!("Organ: {} by {}", def.organ.church_name, def.organ.organ_builder);
        println!("Manuals: {}", def.manuals.len());
        let mut sample_ok = 0;
        let mut sample_missing = 0;
        for s in &def.stops {
            let first = s.first_midi_note.unwrap_or(0);
            let n_real = s.pipes.iter().filter(|p| matches!(p, PipeDef::Sample { .. })).count();
            println!("  [{}] {} — {} pipes, first_midi={}, harmonic={}",
                s.windchest_group, s.name, n_real, first, s.harmonic_number);
            for p in &s.pipes {
                if let PipeDef::Sample { path, .. } = p {
                    if path.exists() { sample_ok += 1; } else {
                        sample_missing += 1;
                        if sample_missing <= 3 { println!("    MISSING: {:?}", path); }
                    }
                }
            }
        }
        println!("Samples on disk: ok={} missing={}", sample_ok, sample_missing);
        assert!(sample_missing == 0, "all resolved samples should exist on disk");
        assert!(!def.stops.is_empty());
    }
}
