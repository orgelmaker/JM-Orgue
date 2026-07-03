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
//! Per pijp gebruiken we de attack-sample van de hoofdlaag (PipeLayerNumber 1).
//! Daarnaast worden nu ook meegenomen:
//! - release-samples van de hoofdlaag (`Pipe_SoundEngine01_ReleaseSample`,
//!   incl. toetsduur-varianten via `ReleaseSelCriteria_LatestKeyReleaseTimeMs`);
//! - de tremulant-laag (PipeLayerNumber 2, "tremmed" opnamen zoals in
//!   Saint-Jean-de-Luz) → [`PipeExtra::tremulant_sample`];
//! - zwelkasten (`Enclosure`/`EnclosurePipe`) → per kast een [`WindchestDef`]
//!   met `enclosure_ids`, en `windchest_group` op de omkaste stops.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::grandorgue::{
    CouplerDef, EnclosureDef, ManualDef, OdfError, OrganDefinition, OrganInfo, PipeDef,
    PipeExtra, ReleaseDef, StopDef, WindchestDef,
};

/// Load a Hauptwerk organ definition and translate it to an [`OrganDefinition`].
pub fn load_hauptwerk(odf_path: &Path) -> Result<OrganDefinition, OdfError> {
    // Lees als bytes zodat we versleutelde/gecomprimeerde (binaire) sets — de
    // gangbare vorm bij commerciële Hauptwerk-distributie — met een duidelijke
    // melding kunnen weigeren i.p.v. een cryptische UTF-8/parse-fout.
    let bytes = std::fs::read(odf_path)?;
    let xml = ensure_readable_xml(bytes)?;

    // Resolve the package root: the ancestor directory that contains
    // `OrganInstallationPackages` (the ODF may live in `OrganDefinitions/` or at
    // the package top level).
    let root = find_package_root(odf_path).ok_or_else(|| {
        OdfError::MissingField("OrganInstallationPackages directory".into())
    })?;
    let pkg_dirs = scan_package_dirs(&root);

    build_definition(&xml, root, &pkg_dirs)
}

/// Foutmelding voor binaire/versleutelde Hauptwerk-bestanden.
const ENCRYPTED_MSG: &str =
    "Deze Hauptwerk-set is versleuteld/niet-leesbaar; alleen onversleutelde sets worden ondersteund";

/// Controleer dat de gelezen bytes leesbare Hauptwerk-XML zijn. Commerciële
/// sets zijn vaak versleuteld/gecomprimeerd (binair); die weigeren we netjes.
fn ensure_readable_xml(bytes: Vec<u8>) -> Result<String, OdfError> {
    let unreadable = || OdfError::ParseError { line: 0, message: ENCRYPTED_MSG.into() };
    // Binair (geen geldige UTF-8) → versleuteld/niet-leesbaar.
    let text = String::from_utf8(bytes).map_err(|_| unreadable())?;
    // Geldige sets beginnen (na eventuele BOM/witruimte) met een XML-tag en
    // hebben het <Hauptwerk ...>-rootelement vroeg in het bestand.
    let head: String = text.chars().take(4096).collect();
    let trimmed = head.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with('<') || !head.contains("<Hauptwerk") {
        return Err(unreadable());
    }
    Ok(text)
}

/// Vertaal de (leesbare) Hauptwerk-XML naar een [`OrganDefinition`]. Apart van
/// [`load_hauptwerk`] zodat tests dit met een mini-XML-string en een nep-
/// packagemap kunnen aanroepen zonder bestanden op schijf.
fn build_definition(
    xml: &str,
    root: PathBuf,
    pkg_dirs: &HashMap<u32, PathBuf>,
) -> Result<OrganDefinition, OdfError> {
    // --- Parse the object lists we need ---
    let general = extract_objects(xml, "_General");
    let divisions = extract_objects(xml, "Division");
    let stops = extract_objects(xml, "Stop");
    let stopranks = extract_objects(xml, "StopRank");
    let pipes = extract_objects(xml, "Pipe_SoundEngine01");
    let layers = extract_objects(xml, "Pipe_SoundEngine01_Layer");
    let attack_samples = extract_objects(xml, "Pipe_SoundEngine01_AttackSample");
    let release_samples = extract_objects(xml, "Pipe_SoundEngine01_ReleaseSample");
    let samples = extract_objects(xml, "Sample");
    let enclosures_xml = extract_objects(xml, "Enclosure");
    let enclosure_pipes = extract_objects(xml, "EnclosurePipe");

    info!(
        "Hauptwerk parse: {} divisions, {} stops, {} stopranks, {} pipes, {} samples, {} releases, {} enclosures",
        divisions.len(), stops.len(), stopranks.len(), pipes.len(), samples.len(),
        release_samples.len(), enclosures_xml.len()
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
    // pipe_id -> lagen [(PipeLayerNumber, LayerID)], gesorteerd op laagnummer.
    // Laag 1 is de droge hoofdlaag; laag 2+ is in de praktijk (o.a. Saint-Jean-
    // de-Luz) een tweede opnamelaag mét tremulant ("tremmed", AT0/RT0-mappen).
    let mut layers_of_pipe: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
    for l in &layers {
        if let (Some(pid), Some(lid)) = (l.get_u32("PipeID"), l.get_u32("LayerID")) {
            let num = l.get_u32("PipeLayerNumber").unwrap_or(1);
            layers_of_pipe.entry(pid).or_default().push((num, lid));
        }
    }
    for v in layers_of_pipe.values_mut() {
        // Stabiele sort: bij gelijk laagnummer blijft documentvolgorde staan.
        v.sort_by_key(|(num, _)| *num);
    }
    // layer_id -> sample_id (first attack sample)
    let mut sample_of_layer: HashMap<u32, u32> = HashMap::new();
    for a in &attack_samples {
        if let (Some(lid), Some(sid)) = (a.get_u32("LayerID"), a.get_u32("SampleID")) {
            sample_of_layer.entry(lid).or_insert(sid);
        }
    }
    // layer_id -> release-samples [(sample_id, max_toetsduur_ms)].
    // `ReleaseSelCriteria_LatestKeyReleaseTimeMs` is de langste toetsduur
    // waarvoor deze release geldt (SJDL: R1=~360ms, R2=~575ms, R0=99999);
    // 99999 is Hauptwerks "geen limiet" → None (de default/langste release,
    // zelfde conventie als GrandOrgue MaxKeyPressTime=-1).
    let mut releases_by_layer: HashMap<u32, Vec<(u32, Option<i32>)>> = HashMap::new();
    for r in &release_samples {
        if let (Some(lid), Some(sid)) = (r.get_u32("LayerID"), r.get_u32("SampleID")) {
            let max_ms = r
                .get_i32("ReleaseSelCriteria_LatestKeyReleaseTimeMs")
                .filter(|v| (0..99999).contains(v));
            releases_by_layer.entry(lid).or_default().push((sid, max_ms));
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
    // pipe_id -> enclosure_id (zwelkast waar de pijp in staat; eerste wint).
    let mut enclosure_of_pipe: HashMap<u32, u32> = HashMap::new();
    for ep in &enclosure_pipes {
        if let (Some(pid), Some(eid)) = (ep.get_u32("PipeID"), ep.get_u32("EnclosureID")) {
            enclosure_of_pipe.entry(pid).or_insert(eid);
        }
    }

    // Resolve een sample-id naar een absoluut pad via de packagemap.
    let resolve_sample = |sample_id: u32| -> Option<PathBuf> {
        let (pkg, fname) = sample_file.get(&sample_id)?;
        let dir = pkg_dirs.get(pkg)?;
        Some(dir.join(fname.replace('\\', "/")))
    };

    // Resolve een pijp naar het hoofdsample-pad plus de extra's: release-samples
    // van de hoofdlaag en (indien aanwezig) de tremulant-laag.
    let resolve_pipe = |pipe_id: u32| -> Option<(PathBuf, PipeExtra)> {
        let lyrs = layers_of_pipe.get(&pipe_id)?;
        let (main_num, main_layer) = *lyrs.first()?;
        let sample_id = *sample_of_layer.get(&main_layer)?;
        let path = resolve_sample(sample_id)?;

        let mut extra = PipeExtra::default();
        // Release-samples van de hoofdlaag. Sommige sets (Ledziny) laten de
        // release naar hetzelfde bestand als de attack wijzen (release zit in
        // dezelfde wav); zo'n "release" slaan we over — de engine valt dan
        // terug op zijn eigen release-envelope.
        if let Some(rels) = releases_by_layer.get(&main_layer) {
            for (sid, max_ms) in rels {
                if let Some(rpath) = resolve_sample(*sid) {
                    if rpath != path {
                        extra.releases.push(ReleaseDef {
                            path: rpath,
                            max_key_press_time_ms: *max_ms,
                            // Hauptwerk's LoadSampleRange-posities hebben eigen
                            // typecodes met onduidelijke semantiek; cue/end
                            // laten we aan de WAV-markers over.
                            cue_point: None,
                            release_end: None,
                        });
                    }
                }
            }
            // Korte toetsduren eerst, de default (None = langste) als laatste.
            extra
                .releases
                .sort_by_key(|r| r.max_key_press_time_ms.map(i64::from).unwrap_or(i64::MAX));
        }
        // Tremulant-laag: attack-sample van de eerstvolgende hogere laag.
        // De hoofdlaag blijft het droge sample!
        if let Some((_, trem_layer)) = lyrs.iter().find(|(num, _)| *num > main_num) {
            extra.tremulant_sample = sample_of_layer
                .get(trem_layer)
                .and_then(|sid| resolve_sample(*sid));
        }
        Some((path, extra))
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

        // Collect (division_note -> pipe + sample path + extra's) across all
        // stopranks of this stop.
        let mut by_note: HashMap<u32, (u32, PathBuf, PipeExtra)> = HashMap::new();
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
                    // Eerste rank die een noot levert wint (zelfde gedrag als
                    // het eerdere or_insert); resolve alleen bij een lege slot.
                    if !by_note.contains_key(&div_note) {
                        if let Some((path, extra)) = resolve_pipe(*pid) {
                            by_note.insert(div_note, (*pid, path, extra));
                        }
                    }
                }
            }
        }

        if by_note.is_empty() {
            warn!("Hauptwerk stop '{}' (id {}) resolved no samples — skipping", name, stop_id);
            continue;
        }

        // Zwelkast: zit de meerderheid van de pijpen van deze stop in één
        // enclosure, dan hoort de stop bij die kast (windchest_group =
        // EnclosureID). Stops zonder kast houden windchest_group 0.
        let windchest_group = {
            let total = by_note.len();
            let mut counts: HashMap<u32, usize> = HashMap::new();
            for (pid, _, _) in by_note.values() {
                if let Some(eid) = enclosure_of_pipe.get(pid) {
                    *counts.entry(*eid).or_default() += 1;
                }
            }
            counts
                .into_iter()
                .find(|(_, n)| *n * 2 > total)
                .map(|(eid, _)| eid)
                .unwrap_or(0)
        };

        // Dense, keyboard-aligned pipe list from the stop's lowest to highest note;
        // gaps within the compass become Empty so key→pipe alignment is preserved.
        let min_note = *by_note.keys().min().unwrap();
        let max_note = *by_note.keys().max().unwrap();
        let mut pipe_list: Vec<PipeDef> = Vec::with_capacity((max_note - min_note + 1) as usize);
        for note in min_note..=max_note {
            match by_note.remove(&note) {
                Some((_, path, extra)) => pipe_list.push(PipeDef::Sample { path, extra }),
                None => pipe_list.push(PipeDef::Empty),
            }
        }

        let division_id = st.get_u32("DivisionID").unwrap_or(0);
        stop_entries.push((division_id, StopDef {
            id: stop_id,
            name,
            harmonic_number: if harmonic > 0 { harmonic } else { 8 },
            pitch_correction: 0.0,
            gain_db: 0.0,
            number_of_pipes: pipe_list.len() as u32,
            first_accessible_pipe_logical_key: 1,
            windchest_group,
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

    // --- Zwelkasten (Enclosures) ---
    // Downstream werkt de zwelkast-detectie op: stop.windchest_group verwijst
    // naar een WindchestDef met niet-lege enclosure_ids. Per Enclosure maken we
    // dus één windchest (nummer = EnclosureID, uniek) met die enclosure erin.
    let mut enclosure_defs: Vec<EnclosureDef> = Vec::new();
    let mut windchest_defs: Vec<WindchestDef> = Vec::new();
    for e in &enclosures_xml {
        let eid = match e.get_u32("EnclosureID") {
            Some(v) => v,
            None => continue,
        };
        // "Enclosure Grand Orgue" → "Grand Orgue": zonder het generieke prefix
        // matcht de naam ook op de divisienaam (naam-gebaseerde detectie).
        let raw = e.get("Name").map(|s| s.as_str()).unwrap_or("").trim();
        let name = raw
            .get(..10)
            .filter(|p| p.eq_ignore_ascii_case("enclosure "))
            .map(|_| raw[10..].trim())
            .unwrap_or(raw)
            .to_string();
        enclosure_defs.push(EnclosureDef {
            number: eid,
            name: name.clone(),
            amp_minimum_level: 0.0,
            midi_input_number: 0,
        });
        windchest_defs.push(WindchestDef {
            number: eid,
            comment: name,
            enclosure_ids: vec![eid],
            tremulant_ids: Vec::new(),
        });
    }
    if !enclosure_defs.is_empty() {
        info!(
            "Hauptwerk: {} zwelkast(en): {:?}",
            enclosure_defs.len(),
            enclosure_defs.iter().map(|e| e.name.as_str()).collect::<Vec<_>>()
        );
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
        number_of_enclosures: enclosure_defs.len() as u32,
        number_of_tremulants: 0,
        number_of_windchest_groups: divisions.len() as u32,
        amplitude_level: 100.0,
        // Hauptwerk kent geen orgel-brede Gain; neutraal (0 dB).
        gain_db: 0.0,
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
        windchests: windchest_defs,
        enclosures: enclosure_defs,
        tremulants: Vec::new(),
        couplers: coupler_defs,
    })
}

/// Snelle check: is dit pad een Hauptwerk-orgeldefinitie?
///
/// Primair op bestandsextensie (case-insensitief `*.organ_hauptwerk_xml`);
/// GrandOrgue-bestanden (`.organ`) worden nooit als Hauptwerk gerouteerd, ook
/// niet in een map met "hauptwerk" in de naam (bv. "X.CompPkg.Hauptwerk").
/// Voor overige paden (mappen e.d.) blijft de oude substring-check als vangnet.
pub fn is_hauptwerk_path(path: &Path) -> bool {
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if file.ends_with(".organ_hauptwerk_xml") {
        return true;
    }
    if file.ends_with(".organ") {
        return false;
    }
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

    #[test]
    fn test_is_hauptwerk_path_extensie() {
        // Detectie op extensie, case-insensitief.
        assert!(is_hauptwerk_path(Path::new(
            "C:/sets/OrganDefinitions/Ledziny st Clement.Organ_Hauptwerk_xml"
        )));
        assert!(is_hauptwerk_path(Path::new("/x/y/foo.ORGAN_HAUPTWERK_XML")));
        assert!(is_hauptwerk_path(Path::new("kaal.organ_hauptwerk_xml")));
        // Extensie werkt ook zonder "hauptwerk" ergens anders in het pad.
        assert!(is_hauptwerk_path(Path::new("C:/orgels/set1/Demo.Organ_Hauptwerk_xml")));
        // GrandOrgue-bestand blijft GrandOrgue, óók in een "Hauptwerk"-map.
        assert!(!is_hauptwerk_path(Path::new(
            "C:/sets/Demo.CompPkg.Hauptwerk/demo.organ"
        )));
        assert!(!is_hauptwerk_path(Path::new("C:/sets/demo.organ")));
        // Vangnet (oud gedrag): mapdetectie op pad-substring.
        assert!(is_hauptwerk_path(Path::new("C:/sets/MijnSet.CompPkg.Hauptwerk")));
        assert!(!is_hauptwerk_path(Path::new("C:/sets/GewoonEenMap")));
    }

    #[test]
    fn test_versleutelde_set_geweigerd() {
        // Binaire rommel (geen geldige UTF-8), zoals versleutelde/gecomprimeerde
        // commerciële Hauptwerk-sets.
        let p = std::env::temp_dir().join("vpo_test_encrypted.Organ_Hauptwerk_xml");
        std::fs::write(&p, [0x1fu8, 0x8b, 0x08, 0xff, 0xfe, 0x00, 0x99, 0xc0, 0x12]).unwrap();
        let err = load_hauptwerk(&p).unwrap_err();
        let _ = std::fs::remove_file(&p);
        assert!(
            err.to_string().contains("versleuteld"),
            "onverwachte fout: {err}"
        );

        // Wél leesbare tekst, maar geen Hauptwerk-XML → zelfde duidelijke fout.
        let p2 = std::env::temp_dir().join("vpo_test_geen_xml.Organ_Hauptwerk_xml");
        std::fs::write(&p2, "dit is geen xml maar platte tekst").unwrap();
        let err2 = load_hauptwerk(&p2).unwrap_err();
        let _ = std::fs::remove_file(&p2);
        assert!(
            err2.to_string().contains("versleuteld"),
            "onverwachte fout: {err2}"
        );

        // Geldige XML-kop passeert de leesbaarheidscheck.
        assert!(ensure_readable_xml(
            b"<Hauptwerk FileFormat=\"Organ\"><ObjectList></ObjectList></Hauptwerk>".to_vec()
        )
        .is_ok());
    }

    /// Mini-Hauptwerk-XML met twee stops: "Bourdon 8" (2 pijpen, in zwelkast 7,
    /// met R0/R1-releases en een tremulant-laag) en "Montre 8" (1 pijp, niet
    /// omkast, release wijst naar hetzelfde bestand als de attack → geskipt).
    fn mini_xml() -> String {
        r#"<Hauptwerk FileFormat="Organ" FileFormatVersion="4.0">
<ObjectList ObjectType="_General"><_General><Identification_Name>Testorgel</Identification_Name></_General></ObjectList>
<ObjectList ObjectType="Division"><Division><DivisionID>1</DivisionID><Name>Grand Orgue</Name></Division></ObjectList>
<ObjectList ObjectType="Stop"><Stop><StopID>1</StopID><Name>Bourdon 8</Name><DivisionID>1</DivisionID></Stop><Stop><StopID>2</StopID><Name>Montre 8</Name><DivisionID>1</DivisionID></Stop></ObjectList>
<ObjectList ObjectType="StopRank"><StopRank><StopID>1</StopID><RankID>1</RankID><MIDINoteNumOfFirstMappedDivisionInputNode>36</MIDINoteNumOfFirstMappedDivisionInputNode><NumberOfMappedDivisionInputNodes>2</NumberOfMappedDivisionInputNodes><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank></StopRank><StopRank><StopID>2</StopID><RankID>2</RankID><MIDINoteNumOfFirstMappedDivisionInputNode>36</MIDINoteNumOfFirstMappedDivisionInputNode><NumberOfMappedDivisionInputNodes>1</NumberOfMappedDivisionInputNodes><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank></StopRank></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01"><Pipe_SoundEngine01><PipeID>11</PipeID><RankID>1</RankID><NormalMIDINoteNumber>36</NormalMIDINoteNumber></Pipe_SoundEngine01><Pipe_SoundEngine01><PipeID>12</PipeID><RankID>1</RankID><NormalMIDINoteNumber>37</NormalMIDINoteNumber></Pipe_SoundEngine01><Pipe_SoundEngine01><PipeID>21</PipeID><RankID>2</RankID><NormalMIDINoteNumber>36</NormalMIDINoteNumber></Pipe_SoundEngine01></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_Layer"><Pipe_SoundEngine01_Layer><LayerID>111</LayerID><PipeID>11</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>511</LayerID><PipeID>11</PipeID><PipeLayerNumber>2</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>112</LayerID><PipeID>12</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>121</LayerID><PipeID>21</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_AttackSample"><Pipe_SoundEngine01_AttackSample><UniqueID>1</UniqueID><LayerID>111</LayerID><SampleID>1</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>2</UniqueID><LayerID>511</LayerID><SampleID>2</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>3</UniqueID><LayerID>112</LayerID><SampleID>3</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>4</UniqueID><LayerID>121</LayerID><SampleID>4</SampleID></Pipe_SoundEngine01_AttackSample></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_ReleaseSample"><Pipe_SoundEngine01_ReleaseSample><UniqueID>10</UniqueID><LayerID>111</LayerID><SampleID>5</SampleID><ReleaseSelCriteria_LatestKeyReleaseTimeMs>99999</ReleaseSelCriteria_LatestKeyReleaseTimeMs></Pipe_SoundEngine01_ReleaseSample><Pipe_SoundEngine01_ReleaseSample><UniqueID>11</UniqueID><LayerID>111</LayerID><SampleID>6</SampleID><ReleaseSelCriteria_LatestKeyReleaseTimeMs>750</ReleaseSelCriteria_LatestKeyReleaseTimeMs></Pipe_SoundEngine01_ReleaseSample><Pipe_SoundEngine01_ReleaseSample><UniqueID>12</UniqueID><LayerID>121</LayerID><SampleID>4</SampleID><ReleaseSelCriteria_LatestKeyReleaseTimeMs>99999</ReleaseSelCriteria_LatestKeyReleaseTimeMs></Pipe_SoundEngine01_ReleaseSample></ObjectList>
<ObjectList ObjectType="Sample"><Sample><SampleID>1</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/A0/036-c.wav</SampleFilename></Sample><Sample><SampleID>2</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/AT0/036-c.wav</SampleFilename></Sample><Sample><SampleID>3</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/A0/037-c#.wav</SampleFilename></Sample><Sample><SampleID>4</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Montre/036-c.wav</SampleFilename></Sample><Sample><SampleID>5</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/R0/036-c.wav</SampleFilename></Sample><Sample><SampleID>6</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/R1/036-c.wav</SampleFilename></Sample></ObjectList>
<ObjectList ObjectType="Enclosure"><Enclosure><EnclosureID>7</EnclosureID><Name>Enclosure Grand Orgue</Name></Enclosure></ObjectList>
<ObjectList ObjectType="EnclosurePipe"><EnclosurePipe><PipeID>11</PipeID><EnclosureID>7</EnclosureID></EnclosurePipe><EnclosurePipe><PipeID>12</PipeID><EnclosureID>7</EnclosureID></EnclosurePipe></ObjectList>
</Hauptwerk>"#.to_string()
    }

    fn mini_definition() -> OrganDefinition {
        let mut pkg_dirs = HashMap::new();
        pkg_dirs.insert(1u32, PathBuf::from("/pkg"));
        build_definition(&mini_xml(), PathBuf::from("/root"), &pkg_dirs).expect("build")
    }

    #[test]
    fn test_release_extractie_en_tremulant_laag() {
        let def = mini_definition();
        let bourdon = def.stops.iter().find(|s| s.name == "Bourdon 8").unwrap();
        let montre = def.stops.iter().find(|s| s.name == "Montre 8").unwrap();

        // Bourdon-pijp 36: R1 (750 ms) vóór R0 (default = None), met geresolvede
        // paden via de packagemap.
        let PipeDef::Sample { path, extra } = &bourdon.pipes[0] else {
            panic!("pijp 36 hoort een sample te zijn");
        };
        assert!(path.ends_with("Bourdon/A0/036-c.wav"), "hoofdlaag blijft droog: {path:?}");
        assert_eq!(extra.releases.len(), 2);
        assert_eq!(extra.releases[0].max_key_press_time_ms, Some(750));
        assert!(extra.releases[0].path.ends_with("Bourdon/R1/036-c.wav"));
        assert_eq!(extra.releases[1].max_key_press_time_ms, None); // 99999 → default
        assert!(extra.releases[1].path.ends_with("Bourdon/R0/036-c.wav"));
        // Tremulant-laag (PipeLayerNumber 2) → tremulant_sample, hoofdlaag droog.
        assert!(extra
            .tremulant_sample
            .as_ref()
            .unwrap()
            .ends_with("Bourdon/AT0/036-c.wav"));

        // Bourdon-pijp 37 heeft geen releases en geen tremulant-laag.
        let PipeDef::Sample { extra: e37, .. } = &bourdon.pipes[1] else {
            panic!("pijp 37 hoort een sample te zijn");
        };
        assert!(e37.releases.is_empty());
        assert!(e37.tremulant_sample.is_none());

        // Montre: release wijst naar hetzelfde bestand als de attack → geskipt.
        let PipeDef::Sample { extra: em, .. } = &montre.pipes[0] else {
            panic!("montre-pijp hoort een sample te zijn");
        };
        assert!(em.releases.is_empty());
        assert!(em.tremulant_sample.is_none());
    }

    #[test]
    fn test_enclosure_mapping() {
        let def = mini_definition();

        // Eén zwelkast, prefix "Enclosure " gestript, windchest met enclosure-id.
        assert_eq!(def.enclosures.len(), 1);
        assert_eq!(def.enclosures[0].name, "Grand Orgue");
        assert_eq!(def.windchests.len(), 1);
        assert_eq!(def.windchests[0].number, 7);
        assert_eq!(def.windchests[0].enclosure_ids, vec![7]);
        assert_eq!(def.organ.number_of_enclosures, 1);

        // Bourdon (beide pijpen in kast 7) → windchest_group 7;
        // Montre (niet omkast) → windchest_group 0.
        let bourdon = def.stops.iter().find(|s| s.name == "Bourdon 8").unwrap();
        let montre = def.stops.iter().find(|s| s.name == "Montre 8").unwrap();
        assert_eq!(bourdon.windchest_group, 7);
        assert_eq!(montre.windchest_group, 0);

        // De downstream-detectie: windchest van de stop heeft enclosure_ids.
        let wc = def
            .windchests
            .iter()
            .find(|w| w.number == bourdon.windchest_group)
            .unwrap();
        assert!(!wc.enclosure_ids.is_empty());
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
        println!("Enclosures: {:?}", def.enclosures.iter().map(|e| (e.number, e.name.as_str())).collect::<Vec<_>>());
        let mut sample_ok = 0;
        let mut sample_missing = 0;
        for s in &def.stops {
            let first = s.first_midi_note.unwrap_or(0);
            let n_real = s.pipes.iter().filter(|p| matches!(p, PipeDef::Sample { .. })).count();
            // Extra's: releases en tremulant-laag per stop samengevat.
            let (mut n_rel, mut n_trem) = (0usize, 0usize);
            for p in &s.pipes {
                if let PipeDef::Sample { extra, .. } = p {
                    n_rel += extra.releases.len();
                    n_trem += extra.tremulant_sample.is_some() as usize;
                }
            }
            println!("  [{}] {} — {} pipes, first_midi={}, harmonic={}, releases={}, trem={}",
                s.windchest_group, s.name, n_real, first, s.harmonic_number, n_rel, n_trem);
            for p in &s.pipes {
                if let PipeDef::Sample { path, extra } = p {
                    if path.exists() { sample_ok += 1; } else {
                        sample_missing += 1;
                        if sample_missing <= 3 { println!("    MISSING: {:?}", path); }
                    }
                    for r in &extra.releases {
                        if r.path.exists() { sample_ok += 1; } else {
                            sample_missing += 1;
                            if sample_missing <= 3 { println!("    MISSING rel: {:?}", r.path); }
                        }
                    }
                    if let Some(t) = &extra.tremulant_sample {
                        if t.exists() { sample_ok += 1; } else {
                            sample_missing += 1;
                            if sample_missing <= 3 { println!("    MISSING trem: {:?}", t); }
                        }
                    }
                }
            }
        }
        println!("Samples on disk: ok={} missing={}", sample_ok, sample_missing);
        assert!(sample_missing == 0, "all resolved samples should exist on disk");
        assert!(!def.stops.is_empty());
    }
}
