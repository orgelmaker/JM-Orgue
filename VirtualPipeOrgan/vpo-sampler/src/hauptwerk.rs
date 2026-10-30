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
//!   Saint-Jean-de-Luz) → [`PipeExtra::tremulant_attack`], inclusief de
//!   release-samples van díe laag → [`PipeExtra::tremulant_releases`];
//! - zwelkasten (`Enclosure`/`EnclosurePipe`) → per kast een [`WindchestDef`]
//!   met `enclosure_ids`, en `windchest_group` op de omkaste stops;
//! - microfoonperspectieven: een stop met meerdere StopRanks over dezelfde
//!   noten (SJDL: "(front)/(rear)/(dry)") krijgt per extra StopRank een
//!   [`PipeLayer`] met een canoniek perspectief-label (perspective.rs); de
//!   eerste StopRank in documentvolgorde is laag 0 / `primary_perspective`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::grandorgue::{
    CouplerDef, EnclosureDef, ManualDef, OdfError, OrganDefinition, OrganInfo, PipeDef,
    AttackDef, PipeExtra, PipeLayer, ReleaseDef, StopDef, WindchestDef,
};
use crate::hauptwerk_compact::GecomprimeerdeOdf;

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

/// Het eerste gehele getal in een tekst, voor het manuaalnummer uit een
/// keyboardnaam als "1. manual [lowest]" → 1, "2. manual" → 2.
fn eerste_getal(s: &str) -> Option<u32> {
    let mut cijfers = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            cijfers.push(c);
        } else if !cijfers.is_empty() {
            break;
        }
    }
    cijfers.parse().ok()
}

/// Leidt per divisie het manuaalnummer af uit de KEYBOARDS in plaats van uit de
/// lijstvolgorde van de divisies (0.9.29). Sommige sets — met name compacte
/// Sonus-Paradisi-demo's — zetten de divisies in een andere volgorde dan de
/// fysieke manualen, terwijl de keyboards ("1. manual [lowest]", "2. manual",
/// …) de juiste nummering wél dragen. Dan toonde JM-Orgue de manualen in de
/// verkeerde volgorde (Recit I, Positif II, GO III i.p.v. andersom).
///
/// Geeft alleen `Some(division_id → manuaalnummer)` terug wanneer élke niet-
/// pedaaldivisie een eigen, uniek manuaalnummer uit de keyboards krijgt; anders
/// `None`, zodat de aanroeper terugvalt op de oude, volgorde-gebaseerde
/// nummering (geen regressie op sets waar dat al klopte).
///
/// Koppeling keyboard → divisie: eerst `Hint_PrimaryAssociatedDivisionID`, dan
/// gelijke ID's (keyboard-ID == divisie-ID, de Hauptwerk-conventie). Het
/// manuaalnummer per keyboard komt uit de naam ("n. manual") wanneer álle
/// niet-pedaalklavieren een eigen nummer dragen; anders uit de ID-volgorde.
fn manual_numbers_from_keyboards(
    keyboards: &[FieldMap],
    divisions: &[FieldMap],
) -> Option<HashMap<u32, u32>> {
    let is_pedaal = |naam: &str| {
        let l = naam.to_lowercase();
        l.contains("pedal") || l.contains("pédale") || l.contains("pedale") || l.contains("ped")
    };

    // Niet-pedaaldivisies (de manualen die een nummer moeten krijgen).
    let mut non_pedal_divs: Vec<u32> = Vec::new();
    for d in divisions {
        let Some(id) = d.get_u32("DivisionID") else { continue };
        let naam = clean_division_name(d.get("Name").map(|s| s.as_str()).unwrap_or(""));
        if !is_pedaal(&naam) {
            non_pedal_divs.push(id);
        }
    }
    if non_pedal_divs.is_empty() {
        return None;
    }

    // Keyboards (zonder het pedaalklavier) met hun geassocieerde divisie en het
    // nummer uit hun naam.
    struct Kbd {
        id: u32,
        div: Option<u32>,
        idx: Option<u32>,
    }
    let mut manual_kbds: Vec<Kbd> = Vec::new();
    for k in keyboards {
        let Some(id) = k.get_u32("Sys_ObjectID") else { continue };
        let naam = k.get("Name").map(|s| s.as_str()).unwrap_or("");
        if is_pedaal(naam) {
            continue;
        }
        manual_kbds.push(Kbd {
            id,
            div: k.get_u32("Hint_PrimaryAssociatedDivisionID"),
            idx: eerste_getal(naam),
        });
    }
    if manual_kbds.is_empty() {
        return None;
    }

    // Manuaalnummer per keyboard: uit de naam wanneer álle klavieren een eigen,
    // uniek nummer hebben; anders oplopend op ID.
    let alle_idx = manual_kbds.iter().all(|k| k.idx.is_some());
    let unieke_idx = {
        let s: std::collections::HashSet<u32> = manual_kbds.iter().filter_map(|k| k.idx).collect();
        s.len() == manual_kbds.len()
    };
    let kbd_manual: HashMap<u32, u32> = if alle_idx && unieke_idx {
        manual_kbds.iter().map(|k| (k.id, k.idx.unwrap())).collect()
    } else {
        let mut op_id: Vec<&Kbd> = manual_kbds.iter().collect();
        op_id.sort_by_key(|k| k.id);
        op_id.iter().enumerate().map(|(i, k)| (k.id, i as u32 + 1)).collect()
    };

    // Divisie → manuaalnummer, via de keyboard die deze divisie speelt.
    let mut div_to_manual: HashMap<u32, u32> = HashMap::new();
    for &div in &non_pedal_divs {
        let kbd = manual_kbds
            .iter()
            .find(|k| k.div == Some(div))
            .or_else(|| manual_kbds.iter().find(|k| k.id == div))?;
        let nummer = *kbd_manual.get(&kbd.id)?;
        div_to_manual.insert(div, nummer);
    }
    // Elke divisie een eigen manuaalnummer? Anders onbetrouwbaar → terugval.
    let unieke: std::collections::HashSet<u32> = div_to_manual.values().copied().collect();
    if unieke.len() != div_to_manual.len() {
        return None;
    }
    Some(div_to_manual)
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
    // Een "compacted" ODF schrijft elk object als <o> met letter-aliassen; de
    // vertaling daarvan naar de lange veldnamen zit in hauptwerk_compact.
    // `_General` blijft in beide vormen leesbaar.
    let compact = GecomprimeerdeOdf::detecteer(xml);
    let lees = |t: &str| match &compact {
        Some(c) => c.objecten(xml, t),
        None => extract_objects(xml, t),
    };
    let general = extract_objects(xml, "_General");
    let divisions = lees("Division");
    let stops = lees("Stop");
    let mut stopranks = lees("StopRank");
    let ranks_xml = lees("Rank");
    let pipes = lees("Pipe_SoundEngine01");
    let layers = lees("Pipe_SoundEngine01_Layer");
    let attack_samples = lees("Pipe_SoundEngine01_AttackSample");
    let release_samples = lees("Pipe_SoundEngine01_ReleaseSample");
    let samples = lees("Sample");
    let enclosures_xml = lees("Enclosure");
    let enclosure_pipes = lees("EnclosurePipe");
    // Definities zonder StopRank-objecten (0.8.3): de verbinding register →
    // pijp zit dan in het schakelnetwerk. Daaruit synthetiseren we de
    // StopRanks, zodat de rest van de importer ongewijzigd kan doorlopen.
    if stopranks.is_empty() && !stops.is_empty() && !pipes.is_empty() {
        stopranks = synth_stopranks_via_switches(
            &stops, &pipes, &lees("SwitchLinkage"), &lees("DivisionInput"), &ranks_xml,
        );
    }

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
    // rank_id -> Rank-Name (SJDL: "01. GO  Bourdon 16 (front)"); terugval voor
    // de perspectiefdetectie wanneer de StopRank zelf geen Name draagt.
    let mut rank_name: HashMap<u32, String> = HashMap::new();
    for r in &ranks_xml {
        if let (Some(rid), Some(n)) = (r.get_u32("RankID"), r.get("Name")) {
            rank_name.entry(rid).or_insert_with(|| n.clone());
        }
    }
    // rank_id -> grondtoon-harmonische van de LAAGSTE toets, voor de voetmaat
    // onder de knop. Was: de eerste pijp in bestandsvolgorde. Bij een gewone
    // rang maakt dat niets uit (alle pijpen hebben dezelfde), maar bij een
    // repeterend mengwerk verschilt hij per toets en werd het willekeurig;
    // de voetmaat van een mengwerk is die op de laagste toets (0.7.66).
    let mut rank_harmonic: HashMap<u32, (u32, u32)> = HashMap::new(); // (toets, harmonische)
    for p in &pipes {
        if let (Some(rid), Some(h)) = (
            p.get_u32("RankID"),
            p.get_u32("Pitch_Tempered_RankBasePitch64ftHarmonicNum"),
        ) {
            let toets = p.get_u32("NormalMIDINoteNumber").unwrap_or(u32::MAX);
            let e = rank_harmonic.entry(rid).or_insert((toets, h));
            if toets < e.0 {
                *e = (toets, h);
            }
        }
    }
    // pipe_id -> toonhoogte-metadata (hertemperen): toets, harmonisch,
    // gemeten originele pijptoon (Hz → cents) en behoud-percentage.
    let mut pipe_pitch: HashMap<u32, HwPipePitch> = HashMap::new();
    for p in &pipes {
        if let (Some(pid), Some(key)) = (p.get_u32("PipeID"), p.get_u32("NormalMIDINoteNumber")) {
            let harmonic = p
                .get_u32("Pitch_Tempered_RankBasePitch64ftHarmonicNum")
                .filter(|h| *h > 0);
            // Pitch_OriginalOrgan_PitchHz = de toon van de pijp op het echte
            // orgel (vóór samplen). Hauptwerk gebruikt het verschil
            // (origineel − samplepitch) om vervangen of hergestemde pijpen op de
            // originele pijptoon te zetten, ook in "Original organ tuning".
            // Tot 0.7.93 lazen we het veld alleen bij SpecificationMethodCode 2;
            // de sets van Piotr Grabowski (Nitra, Nowa Ruda — vele andere) gebruiken
            // code 1 met de Hz-waarde gewoon gevuld, dus daar werd elke vervangen
            // pijp op bestandstoon gespeeld (Copula maior g'' klonk als f''). Een
            // Hz-waarde die er staat telt — ongeacht de methodecode. Een lege of
            // 0-waarde laat de pijp op bestandstoon (de huidige terugval).
            let original_cents = p
                .get_f32("Pitch_OriginalOrgan_PitchHz")
                .filter(|hz| *hz > 0.0)
                .map(hz_to_cents);
            // BaseTuningSchemeCode zegt hoe de pijp zich tot het temperament
            // verhoudt; de Deviation heeft per code een andere eenheid.
            //   1 = precies op het temperament (geen afwijking)
            //   3 = op het temperament + Deviation in cents (bewuste zweving)
            //   4 = Deviation = % van de originele afwijking dat blijft
            // Code 1 en 3 zijn een expliciete opdracht: ze gaan vóór de
            // naamherkenning van zwevende stemmen (anders hield Unda maris met
            // code 3 / +13 ct de opname-afwijking i.p.v. +13 ct). Andere codes
            // blijven onbekend (None).
            let deviation = p.get_f32("Pitch_Tempered_BaseTuningDeviation");
            let (keep_pct, tuning_offset_cents) = match p.get_u32("Pitch_Tempered_BaseTuningSchemeCode") {
                Some(1) => (Some(0.0), None),
                Some(3) => (Some(0.0), Some(deviation.unwrap_or(0.0))),
                Some(4) => (deviation, None),
                _ => (None, None),
            };
            pipe_pitch.insert(pid, HwPipePitch { key, harmonic, original_cents, keep_pct, tuning_offset_cents });
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
    // Hauptwerk bewaart zijn eigen meegeleverde sets in het gesloten
    // .hbw-formaat. Het bestand is dan wél leesbaar en het orgel komt met
    // registers en al binnen — er komt alleen geen noot uit, want geen enkele
    // opname laat zich openen. Dat liever meteen zeggen dan een stom orgel
    // opleveren.
    if !sample_file.is_empty() {
        let hbw = sample_file
            .values()
            .filter(|(_, f)| f.to_lowercase().ends_with(".hbw"))
            .count();
        if hbw * 2 > sample_file.len() {
            // De code tussen [haken] laat de app deze uitleg in de gekozen
            // taal tonen (App.svelte: vertaalFout). De Nederlandse zin erachter
            // blijft staan voor het logboek en voor oudere app-versies.
            return Err(OdfError::ParseError {
                line: 0,
                message: format!(
                    "[gesloten-formaat {}/{}] deze set bewaart zijn opnamen in \
                     Hauptwerks eigen .hbw-formaat ({} van de {}); dat formaat \
                     is gesloten en kan alleen door Hauptwerk zelf worden \
                     afgespeeld",
                    hbw,
                    sample_file.len(),
                    hbw,
                    sample_file.len()
                ),
            });
        }
    }

    // sample_id -> gemeten samplepitch in cents (Pitch_SpecificationMethodCode:
    // 4 = exacte frequentie in Hz; 2/3 = MIDI-noot (+ harmonisch); 1/leeg =
    // smpl-chunk van het bestand → geen entry, de loader leest die zelf).
    let mut sample_pitch_cents: HashMap<u32, f32> = HashMap::new();
    for s in &samples {
        let Some(sid) = s.get_u32("SampleID") else { continue };
        let cents = match s.get_u32("Pitch_SpecificationMethodCode") {
            Some(4) => s.get_f32("Pitch_ExactSamplePitch").filter(|hz| *hz > 0.0).map(hz_to_cents),
            Some(2) | Some(3) => s
                .get_u32("Pitch_NormalMIDINoteNumber")
                .filter(|n| *n > 0)
                .map(|n| {
                    let h = s.get_u32("Pitch_RankBasePitch64ftHarmonicNum").filter(|h| *h > 0);
                    100.0 * n as f32 + h.map_or(0.0, |h| 1200.0 * (h as f32 / 8.0).log2())
                }),
            _ => None,
        };
        if let Some(c) = cents {
            sample_pitch_cents.insert(sid, c);
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
        // Toonhoogte-metadata voor hertemperen (sample-veld > smpl-chunk >
        // originele pijptoon; de keuze valt in de retune-opbouw).
        extra.sample_pitch_cents = sample_pitch_cents.get(&sample_id).copied();
        if let Some(pp) = pipe_pitch.get(&pipe_id) {
            extra.key_midi_note = Some(pp.key);
            extra.harmonic_number = pp.harmonic;
            extra.original_pitch_cents = pp.original_cents;
            extra.retune_keep_pct = pp.keep_pct;
            // Zelfde betekenis als GO PitchCorrection: doelafwijking die
            // alléén bij hertemperen meetelt.
            extra.pitch_correction_cents = pp.tuning_offset_cents;
        }
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
                            crossfade_ms: None,
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
        // De hoofdlaag blijft het droge sample! De release-samples van die
        // laag horen er ook bij: met tremulant aan klinkt bij loslaten de
        // tremulant-release (vroeger altijd de droge).
        if let Some((_, trem_layer)) = lyrs.iter().find(|(num, _)| *num > main_num) {
            if let Some(&trem_sid) = sample_of_layer.get(trem_layer) {
              if let Some(tp) = resolve_sample(trem_sid) {
                if let Some(rels) = releases_by_layer.get(trem_layer) {
                    for (sid, max_ms) in rels {
                        if let Some(rpath) = resolve_sample(*sid) {
                            // Zelfde filter als de hoofdlaag: een "release" die
                            // naar het attack-bestand zelf wijst is er geen.
                            if rpath != tp {
                                extra.tremulant_releases.push(ReleaseDef {
                                    path: rpath,
                                    max_key_press_time_ms: *max_ms,
                                    cue_point: None,
                                    release_end: None,
                                    crossfade_ms: None,
                                });
                            }
                        }
                    }
                    extra
                        .tremulant_releases
                        .sort_by_key(|r| r.max_key_press_time_ms.map(i64::from).unwrap_or(i64::MAX));
                }
                extra.tremulant_attack = Some(AttackDef {
                    path: tp,
                    // De tremulantlaag is een EIGEN opname: gebruik háár eigen
                    // ODF-samplepitch (Pitch_ExactSamplePitch), niet de WAV-
                    // koptekst en niet die van de droge laag (0.9.24). Ontbreekt
                    // de ODF-opgave, dan blijft het None en beslist de smpl-chunk.
                    sample_pitch_cents: sample_pitch_cents.get(&trem_sid).copied(),
                    ..Default::default()
                });
              }
            }
        }
        Some((path, extra))
    };

    // stop_id -> its stopranks. ActionEffectCode 2/3 = een eenmalig geluid
    // bij in- of uitschakelen (register-, koppel-, tremulant- of toetsklik),
    // geen pijplaag; anders klonk zo'n klik als extra laag mee bij het spelen
    // en verschenen hulpregisters als "CouplerEffect_1006" als registerknop.
    let mut stopranks_by_stop: HashMap<u32, Vec<&FieldMap>> = HashMap::new();
    let mut effect_stopranks = 0usize;
    for sr in &stopranks {
        if sr.get_u32("ActionEffectCode").map_or(false, |c| c != 1) {
            effect_stopranks += 1;
            continue;
        }
        if let Some(sid) = sr.get_u32("StopID") {
            stopranks_by_stop.entry(sid).or_default().push(sr);
        }
    }
    if effect_stopranks > 0 {
        info!("Hauptwerk: {} StopRanks met een in-/uitschakelgeluid niet als pijplaag geladen", effect_stopranks);
    }

    // --- Build StopDefs (paired with their division id for grouping) ---
    let mut stop_entries: Vec<(u32, StopDef)> = Vec::new();
    for st in &stops {
        let stop_id = match st.get_u32("StopID") {
            Some(v) => v,
            None => continue,
        };
        // Stop-Name is in de praktijk al schoon ("GO  Bourdon 16"); het
        // perspectief-suffix zit op de StopRank/Rank. De ruwe naam wordt pas
        // ná de laag-analyse van een eventueel perspectief-suffix ontdaan
        // (alleen als er écht perspectieven zijn — anders werd 'Trompet
        // Direct' ten onrechte 'Trompet'); de meldingen hieronder gebruiken
        // de ruwe naam.
        // Sommige makers zetten het eigen StopID vóór de naam ("2001_Contrabass
        // 16'" bij StopID 2001); in Hauptwerk zelf is die interne naam
        // onzichtbaar. In sommige sets (Augustine's Dreischor/Urk) is de
        // interne prefix overgenomen van een naburig register en komt dus
        // niet overeen met het eigen StopID ("2119_TrompetDisc" bij StopID
        // 2121). Daarom strippen we 1-4 cijfers gevolgd door een
        // underscore wanneer er na die prefix nog een letter of streep
        // volgt — geen enkel echt registernaam begint met een reeks
        // cijfers plus "_".
        let ruwe_naam = st.get("Name").map(|s| s.as_str()).unwrap_or("");
        let ruwe_naam = strip_numeric_prefix(ruwe_naam);
        let name = clean_stop_name(ruwe_naam);

        // Collect (division_note -> pipe + sample path + extra's) PER StopRank,
        // in documentvolgorde. StopRanks met disjuncte noten (splice, geleende
        // bas) smelten samen in laag 0 (eerste wint, zoals voorheen); een
        // StopRank die noten van laag 0 overlapt — de microfoonperspectieven
        // front/rear/dry van SJDL — wordt een eigen PipeLayer.
        let mut by_note: HashMap<u32, (u32, PathBuf, PipeExtra)> = HashMap::new();
        let mut first_rank_label: Option<String> = None;
        let mut extra_layers: Vec<(String, HashMap<u32, (u32, PathBuf, PipeExtra)>)> = Vec::new();
        let mut harmonic = 0u32;
        let mut klinkend = 0u32;
        for sr in stopranks_by_stop.get(&stop_id).into_iter().flatten() {
            let rank_id = match sr.get_u32("RankID") {
                Some(v) => v,
                None => continue,
            };
            let rank_pipes = match pipes_by_rank.get(&rank_id) {
                Some(m) => m,
                None => continue,
            };
            // Perspectief-bron: StopRank-Name ("PED  Soubasse 16 (front)"),
            // anders de Rank-Name, anders een neutrale naam.
            let sr_name = sr
                .get("Name")
                .filter(|n| !n.trim().is_empty())
                .cloned()
                .or_else(|| rank_name.get(&rank_id).cloned())
                .unwrap_or_else(|| format!("Rank {}", rank_id));
            let first_div = sr.get_u32("MIDINoteNumOfFirstMappedDivisionInputNode").unwrap_or(36);
            let inc = sr.get_i32("MIDINoteNumIncrementFromDivisionToRank").unwrap_or(0);
            // 0 (of afwezig) = niet aan een toetsbereik gebonden, zoals het
            // motorgeluid: dan loopt de StopRank tot het eind van zijn rang.
            let count = match sr.get_u32("NumberOfMappedDivisionInputNodes") {
                Some(n) if n > 0 => n,
                _ => rank_pipes.keys().max().map_or(0, |&hoogste| {
                    (hoogste as i64 - inc as i64 - first_div as i64 + 1).max(0) as u32
                }),
            };
            // De voetmaat van de rang én die van wat je hoort. Een pedaal-
            // Bourdon 8 kan een 16'-rang een octaaf hoger aanspreken; dan
            // stond er "Bourdon 8" met "16'" eronder.
            if harmonic == 0 {
                if let Some((_, h)) = rank_harmonic.get(&rank_id) {
                    harmonic = *h;
                    klinkend = klinkende_harmonische(*h, inc);
                }
            }
            let mut own: HashMap<u32, (u32, PathBuf, PipeExtra)> = HashMap::new();
            for k in 0..count {
                let div_note = first_div + k;
                let rank_note = (div_note as i64 + inc as i64) as u32;
                if let Some(pid) = rank_pipes.get(&rank_note) {
                    if !own.contains_key(&div_note) {
                        if let Some((path, extra)) = resolve_pipe(*pid) {
                            own.insert(div_note, (*pid, path, extra));
                        }
                    }
                }
            }
            // Tremmed rang: dezelfde toetsen, maar opgenomen mét tremulant.
            //
            // Hauptwerk kent hier twee manieren voor. Saint-Jean-de-Luz zet
            // de tremulant-opname als tweede laag óp de pijp; dat leest
            // `resolve_pipe` hierboven al. Rotterdam en Utrecht gebruiken de
            // andere: een áparte rang ("4 Gedekt 8 (rear tremmed)") waar een
            // schakelaar het register naartoe zet. Die route stond tot nu toe
            // stil, dus die sets klonken nooit met tremulant.
            //
            // Beide komen op dezelfde plek terecht — `tremulant_attack` en
            // `tremulant_releases` — zodat de rest van de keten niet hoeft te
            // weten waar de opname vandaan kwam.
            if let Some(alt_id) = sr.get_u32("AlternateRankID") {
                if let Some(alt_pipes) = pipes_by_rank.get(&alt_id) {
                    let mut gevuld = 0usize;
                    for (div_note, (_, _, extra)) in own.iter_mut() {
                        let rank_note = (*div_note as i64 + inc as i64) as u32;
                        let Some(apid) = alt_pipes.get(&rank_note) else { continue };
                        let Some((apath, aextra)) = resolve_pipe(*apid) else { continue };
                        // De tremmed rang heeft zelf geen tweede laag; zijn
                        // hoofdlaag ís de tremulant-opname.
                        extra.tremulant_releases = aextra.releases;
                        extra.tremulant_attack = Some(AttackDef {
                            path: apath,
                            ..Default::default()
                        });
                        gevuld += 1;
                    }
                    if gevuld > 0 {
                        info!(
                            "Hauptwerk stop '{}': {} tremulant-pijpen uit rang {} ({})",
                            name,
                            gevuld,
                            alt_id,
                            rank_name.get(&alt_id).map(|s| s.as_str()).unwrap_or("naamloos")
                        );
                    }
                }
            }

            if own.is_empty() {
                continue;
            }
            if first_rank_label.is_none() {
                first_rank_label = Some(sr_name.clone());
            }
            let overlaps = own.keys().any(|n| by_note.contains_key(n));
            if !overlaps {
                // Splice / eerste rank: in laag 0.
                for (n, v) in own {
                    by_note.entry(n).or_insert(v);
                }
            } else if extra_layers.len() < 15 {
                extra_layers.push((sr_name, own));
            } else {
                warn!(
                    "Hauptwerk stop '{}' (id {}): StopRank '{}' genegeerd — meer dan 15 extra lagen",
                    name, stop_id, sr_name
                );
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

        // Dense, keyboard-aligned pipe list from the stop's lowest to highest note
        // over ALLE lagen; gaps within the compass become Empty so key→pipe
        // alignment is preserved. Noten die alleen in een extra laag zitten
        // zijn in laag 0 Empty (stil) en klinken via die laag als hij aanstaat.
        let min_note = by_note
            .keys()
            .chain(extra_layers.iter().flat_map(|(_, m)| m.keys()))
            .copied()
            .min()
            .unwrap();
        let max_note = by_note
            .keys()
            .chain(extra_layers.iter().flat_map(|(_, m)| m.keys()))
            .copied()
            .max()
            .unwrap();
        let mut pipe_list: Vec<PipeDef> = Vec::with_capacity((max_note - min_note + 1) as usize);
        for note in min_note..=max_note {
            match by_note.remove(&note) {
                Some((_, path, extra)) => pipe_list.push(PipeDef::Sample { path, extra }),
                None => pipe_list.push(PipeDef::Empty),
            }
        }
        let mut layers: Vec<PipeLayer> = Vec::with_capacity(extra_layers.len());
        for (i, (lname, mut m)) in extra_layers.into_iter().enumerate() {
            let mut lp: Vec<PipeDef> = Vec::with_capacity((max_note - min_note + 1) as usize);
            for note in min_note..=max_note {
                match m.remove(&note) {
                    Some((_, path, extra)) => lp.push(PipeDef::Sample { path, extra }),
                    None => lp.push(PipeDef::Empty),
                }
            }
            let perspective = crate::perspective::detect_perspective(&lname);
            layers.push(PipeLayer { index: (i + 1) as u8, name: lname, perspective, pipes: lp });
        }
        let has_perspectives = layers.iter().any(|l| l.perspective.is_some());
        let primary_perspective = if has_perspectives {
            first_rank_label.as_deref().and_then(crate::perspective::detect_perspective)
        } else {
            None
        };
        // Perspectief-suffix alleen van de stopnaam strippen wanneer ≥1 laag
        // een perspectief heeft; zonder perspectieven blijft de naam intact.
        let name = if has_perspectives {
            crate::perspective::strip_perspective(&name)
        } else {
            name
        };
        if !layers.is_empty() {
            info!(
                "Hauptwerk stop '{}': {} extra laag/lagen: {}",
                name,
                layers.len(),
                layers
                    .iter()
                    .map(|l| match &l.perspective {
                        Some(p) => format!("{} [{}]", l.name, p),
                        None => l.name.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        let division_id = st.get_u32("DivisionID").unwrap_or(0);
        stop_entries.push((division_id, StopDef {
            id: stop_id,
            name,
            harmonic_number: if harmonic > 0 { harmonic } else { 8 },
            sounding_harmonic: if klinkend > 0 { klinkend } else if harmonic > 0 { harmonic } else { 8 },
            pitch_correction: 0.0,
            pitch_tuning_cents: 0.0,
            gain_db: 0.0,
            number_of_pipes: pipe_list.len() as u32,
            first_accessible_pipe_logical_key: 1,
            windchest_group,
            amplitude_level: 100.0,
            percussive: false,
            accepts_retuning: true,
            first_midi_note: Some(min_note),
            pipes: pipe_list,
            layers,
            primary_perspective,
        }));
    }

    // --- Build manuals (one per division); pedal => number 0 ---
    // Het manuaalnummer komt bij voorkeur uit de KEYBOARDS (0.9.29): de
    // lijstvolgorde van de divisies klopt niet altijd met de fysieke manualen
    // (compacte SP-demo's zetten ze door elkaar). Lukt dat niet volledig, dan
    // de oude, volgorde-gebaseerde nummering (geen regressie).
    let mut manuals: Vec<ManualDef> = Vec::new();
    let mut manual_counter = 1u32;
    let mut div_to_manual_number: HashMap<u32, u32> = HashMap::new();
    let manual_uit_keyboards = manual_numbers_from_keyboards(&lees("Keyboard"), &divisions);
    if let Some(m) = &manual_uit_keyboards {
        info!("Hauptwerk: manuaalvolgorde uit de keyboards afgeleid ({} manualen)", m.len());
    }
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
        let number = if is_pedal {
            0
        } else if let Some(&n) = manual_uit_keyboards.as_ref().and_then(|m| m.get(&division_id)) {
            n
        } else {
            let n = manual_counter;
            manual_counter += 1;
            n
        };

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
    let key_actions = lees("KeyAction");
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
            melodie_of_bas: false,
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
            amplitude_level: 100.0,
            gain_db: 0.0,
            pitch_tuning_cents: 0.0,
            pitch_correction_cents: 0.0,
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
        pitch_tuning_cents: 0.0,
        pitch_correction_cents: 0.0,
        // AudioEngine_EnablePlayingAtOriginalOrganPitch = N: deze set levert
        // geen originele orgeltoonhoogte en moet op de getempereerde doeltoon
        // spelen, ook in "Origineel". Afwezig of Y = gewoon gedrag.
        geen_originele_toonhoogte: getg("AudioEngine_EnablePlayingAtOriginalOrganPitch")
            .trim()
            .eq_ignore_ascii_case("N"),
    };

    let stop_defs: Vec<StopDef> = stop_entries.into_iter().map(|(_, s)| s).collect();
    let total_pipes: usize = stop_defs.iter().map(|s| s.pipes.len()).sum();
    info!(
        "Hauptwerk import: '{}' — {} stops in {} divisions, {} pipes",
        organ.church_name, stop_defs.len(), manuals.len(), total_pipes
    );

    // Een orgel zonder registers is geen orgel. Tot 0.7.52 kwam zo'n import
    // als "gelukt" door en kreeg de gebruiker een leeg scherm zonder uitleg;
    // dat gebeurde bij elke gecomprimeerde orgeldefinitie.
    if stop_defs.is_empty() {
        return Err(OdfError::ParseError {
            line: 0,
            message: "deze Hauptwerk-orgeldefinitie leverde geen enkel register op: het bestand is leesbaar, maar de indeling wordt niet herkend".into(),
        });
    }

    // Klavier-afkortingen (0.8.4): Keyboard.ShortName → divisienaam, voor de
    // koppeling van toetsmechaniek-ranks ("PO Key action noises …") aan het
    // klavier wanneer de stopnamen die prefix niet dragen.
    let klavier_afkortingen: HashMap<String, String> = {
        let div_namen: HashMap<u32, String> = lees("Division").iter()
            .filter_map(|d| Some((d.get_u32("DivisionID")?, clean_division_name(d.get("Name")?))))
            .collect();
        let mut m = HashMap::new();
        for k in lees("Keyboard") {
            let (Some(kort), Some(did)) = (k.get("ShortName"), k.get_u32("Hint_PrimaryAssociatedDivisionID")) else { continue };
            let kort = kort.trim();
            if kort.is_empty() { continue; }
            if let Some(naam) = div_namen.get(&did) {
                m.entry(kort.to_uppercase()).or_insert_with(|| naam.clone());
            }
        }
        m
    };

    // Tremulantruis per divisie (0.8.5): welke divisie hoort bij welke
    // tremulant, zodat het mechanische geluid van de tremulant alleen
    // meeloopt met de tremulant van zijn eigen werk.
    let trem_koppeling = tremulant_divisies(&lees("Tremulant"), &divisions, &manuals, &stop_defs);

    Ok(OrganDefinition {
        base_path: root,
        organ,
        manuals,
        stops: stop_defs,
        windchests: windchest_defs,
        enclosures: enclosure_defs,
        tremulants: Vec::new(),
        couplers: coupler_defs,
        extra_bijgeluiden: verzamel_hw_bijgeluiden(
            &pipes, &pipes_by_rank, &sample_file, &pkg_dirs,
            // `lees` i.p.v. extract_objects: bij een gecomprimeerde definitie
            // staan de switches in de korte <o>-vorm. (De compacte lezer kent
            // van Switch alleen het ID; naam en standaardstand ontbreken daar
            // nog, dus bijgeluiden op compacte sets volgen later.)
            &layers_of_pipe, &sample_of_layer, &lees("Switch"), &lees("SwitchLinkage"), &rank_name,
            &trem_koppeling,
        ),
        stop_action_clicks: verzamel_hw_stop_action_clicks(
            &pipes, &sample_file, &pkg_dirs,
            &layers_of_pipe, &sample_of_layer, &stops, &rank_name,
        ),
        keyboard_clicks: verzamel_hw_keyboard_clicks(
            &pipes, &sample_file, &pkg_dirs,
            &layers_of_pipe, &sample_of_layer, &ranks_xml,
        ),
        klavier_afkortingen,
    })
}

/// Terugval voor definities zónder StopRank-objecten (0.8.3). De nieuwere
/// sets van Grabowski (Nowa Ruda) bouwen de verbinding register → pijp als
/// schakelnetwerk: elke pijp hangt aan een eigen pallet-schakelaar, die via
/// SwitchLinkages wordt bediend — bron = de toets-schakelaar uit
/// `DivisionInput`, voorwaarde = de register-schakelaar uit
/// `Stop.ControllingSwitchID`, meestal met een of twee tussenliggende
/// routing-schakelaars ("__RoutingSwPostStop", "__DelayStopPostPD…"). Deze
/// functie loopt per pijp het netwerk achterstevoren af (breedte-eerst,
/// hoogstens acht stappen), vindt het dichtstbijzijnde register en de toets
/// uit dezelfde divisie, en synthetiseert per (register, rank) een StopRank-
/// FieldMap met precies de velden die de importer verderop leest. Pijpen in
/// ruis-/mechaniekranks ("noise") blijven buiten: die hangen weliswaar aan
/// register- of toetsschakelaars, maar zijn geen klinkende lagen.
fn synth_stopranks_via_switches(
    stops: &[FieldMap],
    pipes: &[FieldMap],
    linkages: &[FieldMap],
    division_inputs: &[FieldMap],
    ranks_xml: &[FieldMap],
) -> Vec<FieldMap> {
    use std::collections::{HashSet, VecDeque};
    // register-schakelaar → (stop_id, division_id)
    let mut stop_by_switch: HashMap<u32, (u32, u32)> = HashMap::new();
    for s in stops {
        if let (Some(id), Some(sw)) = (s.get_u32("StopID"), s.get_u32("ControllingSwitchID")) {
            stop_by_switch.insert(sw, (id, s.get_u32("DivisionID").unwrap_or(0)));
        }
    }
    // toets-schakelaar → (division_id, noot)
    let mut key_by_switch: HashMap<u32, (u32, u32)> = HashMap::new();
    for d in division_inputs {
        if let (Some(sw), Some(n)) = (d.get_u32("SwitchID"), d.get_u32("NormalMIDINoteNumber")) {
            key_by_switch.insert(sw, (d.get_u32("DivisionID").unwrap_or(0), n));
        }
    }
    // doel → [(bron, voorwaarde)]
    let mut by_dest: HashMap<u32, Vec<(u32, Option<u32>)>> = HashMap::new();
    for l in linkages {
        if let (Some(src), Some(dst)) = (l.get_u32("SourceSwitchID"), l.get_u32("DestSwitchID")) {
            by_dest.entry(dst).or_default().push((src, l.get_u32("ConditionSwitchID")));
        }
    }
    let rank_name: HashMap<u32, String> = ranks_xml.iter()
        .filter_map(|r| Some((r.get_u32("RankID")?, r.get("Name")?.clone())))
        .collect();
    // per (register, rank): (divisienoot, pijpnoot)
    let mut groepen: HashMap<(u32, u32), Vec<(u32, u32)>> = HashMap::new();
    let (mut zonder, mut dubbel) = (0usize, 0usize);
    for p in pipes {
        let (Some(pal), Some(rid), Some(note)) = (
            p.get_u32("ControllingPalletSwitchID"), p.get_u32("RankID"), p.get_u32("NormalMIDINoteNumber"),
        ) else { continue };
        if pal == 0 { continue; }
        if rank_name.get(&rid).map(|n| n.to_lowercase().contains("noise")).unwrap_or(false) {
            continue;
        }
        let mut stop_hits: Vec<(u32, u32, usize)> = Vec::new(); // (stop, divisie, diepte)
        let mut key_hits: Vec<(u32, u32, usize)> = Vec::new();  // (divisie, noot, diepte)
        let mut seen: HashSet<u32> = HashSet::new();
        let mut queue: VecDeque<(u32, usize)> = VecDeque::new();
        queue.push_back((pal, 0));
        seen.insert(pal);
        while let Some((s, d)) = queue.pop_front() {
            if let Some(&(sid, div)) = stop_by_switch.get(&s) { stop_hits.push((sid, div, d)); }
            if let Some(&(div, n)) = key_by_switch.get(&s) { key_hits.push((div, n, d)); }
            if d >= 8 { continue; }
            if let Some(links) = by_dest.get(&s) {
                for &(src, cond) in links {
                    for nxt in std::iter::once(src).chain(cond) {
                        if seen.insert(nxt) { queue.push_back((nxt, d + 1)); }
                    }
                }
            }
        }
        stop_hits.sort_by_key(|h| h.2);
        let Some(&(stop_id, stop_div, diepte)) = stop_hits.first() else { zonder += 1; continue };
        if stop_hits.iter().any(|h| h.0 != stop_id && h.2 == diepte) { dubbel += 1; }
        key_hits.sort_by_key(|h| h.2);
        let div_note = key_hits.iter().find(|h| h.0 == stop_div).or(key_hits.first())
            .map(|k| k.1).unwrap_or(note);
        groepen.entry((stop_id, rid)).or_default().push((div_note, note));
    }
    let mut uit: Vec<FieldMap> = Vec::new();
    for ((stop_id, rid), mut lijst) in groepen {
        lijst.sort();
        let first = lijst[0].0;
        let last = lijst[lijst.len() - 1].0;
        // Verschuiving divisienoot → pijpnoot: de meest voorkomende waarde
        // (een 4'-register op een 8'-rang geeft +12).
        let mut tel: HashMap<i32, usize> = HashMap::new();
        for &(dn, pn) in &lijst { *tel.entry(pn as i32 - dn as i32).or_insert(0) += 1; }
        let inc = tel.into_iter().max_by_key(|(k, v)| (*v, -*k)).map(|(k, _)| k).unwrap_or(0);
        let mut m = FieldMap::new();
        m.insert("StopID".into(), stop_id.to_string());
        m.insert("RankID".into(), rid.to_string());
        if let Some(n) = rank_name.get(&rid) { m.insert("Name".into(), n.clone()); }
        m.insert("MIDINoteNumOfFirstMappedDivisionInputNode".into(), first.to_string());
        m.insert("NumberOfMappedDivisionInputNodes".into(), (last - first + 1).to_string());
        m.insert("MIDINoteNumIncrementFromDivisionToRank".into(), inc.to_string());
        uit.push(m);
    }
    // Volgorde: per register de rangen oplopend — de eerste (laagste rank-id,
    // bij Grabowski "close") wordt het primaire perspectief.
    uit.sort_by_key(|m| (m.get_u32("StopID"), m.get_u32("RankID")));
    // Tremmed rangen (0.8.4): een set met echte tremulant-opnamen heeft per
    // positie een tweede rang "… (close, tremmed)" waarvan de pallets via
    // "Tremulant N Trem" lopen; de droge rang loopt via "… NonTrem". De
    // zoektocht hierboven vindt voor allebei hetzelfde register, en zonder
    // koppeling werd de tremmed rang een tweede laag die altijd meeklonk —
    // dubbel geluid zonder tremulant, en geen tremulant mét. Hier wordt hij
    // de AlternateRankID van zijn droge tegenhanger (zelfde register, zelfde
    // naam zonder "tremmed"), precies zoals een StopRank-object dat zou doen.
    let sleutel = |naam: &str| -> String {
        let l = naam.to_lowercase();
        let l = l.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace());
        l.replace("tremmed", "").chars().filter(|c| c.is_alphanumeric()).collect()
    };
    let is_tremmed = |m: &FieldMap| m.get("Name").map(|n| n.to_lowercase().contains("tremmed")).unwrap_or(false);
    let mut gekoppeld = 0usize;
    let mut i = 0;
    while i < uit.len() {
        if !is_tremmed(&uit[i]) { i += 1; continue; }
        let stop = uit[i].get_u32("StopID");
        let rid = uit[i].get_u32("RankID").unwrap_or(0);
        let k = uit[i].get("Name").map(|n| sleutel(n)).unwrap_or_default();
        let droog = uit.iter().position(|m| {
            m.get_u32("StopID") == stop
                && m.get_u32("RankID") != Some(rid)
                && !is_tremmed(m)
                && m.get("Name").map(|n| sleutel(n) == k).unwrap_or(false)
        });
        match droog {
            Some(j) => {
                uit[j].insert("AlternateRankID".into(), rid.to_string());
                uit.remove(i);
                gekoppeld += 1;
            }
            None => { i += 1; }
        }
    }
    info!(
        "Hauptwerk: geen StopRank-objecten — {} register/rank-koppelingen afgeleid uit het schakelnetwerk ({} pijpen zonder register, {} met meer dan één, {} tremmed rangen als tremulantlaag gekoppeld)",
        uit.len(), zonder, dubbel, gekoppeld
    );
    uit
}

/// Toetsmechaniek-geluiden (0.7.102): voor Hauptwerk-sets de ranks "{prefix}
/// Key action noises Attack" met per toets één pijp. De prefix (bv. "P",
/// "RP", "HW") komt uit de rank-naam en wordt in de koppel-stap gematcht
/// aan de divisienaam. Alleen de PRIMAIRE (eerste perspectief) laag wordt
/// bewaard: toetskliks hebben zelf geen perspectief nodig.
fn verzamel_hw_keyboard_clicks(
    pipes: &[FieldMap],
    sample_file: &std::collections::HashMap<u32, (u32, String)>,
    pkg_dirs: &std::collections::HashMap<u32, std::path::PathBuf>,
    layers_of_pipe: &std::collections::HashMap<u32, Vec<(u32, u32)>>,
    sample_of_layer: &std::collections::HashMap<u32, u32>,
    ranks_xml: &[FieldMap],
) -> std::collections::HashMap<(String, u8), crate::grandorgue::KeyboardClickPaths> {
    // Rank-ID → (prefix, is_attack). "{prefix} Key action noises Attack"
    // levert attack per toets, "… Release" levert loslaat (0.7.103).
    let mut rank_prefix: std::collections::HashMap<u32, (String, bool)> = std::collections::HashMap::new();
    for r in ranks_xml {
        let Some(rid) = r.get_u32("RankID") else { continue };
        let Some(name) = r.get("Name") else { continue };
        let lower = name.to_lowercase();
        if !lower.contains("key action") { continue; }
        let is_attack = lower.contains("attack");
        let is_release = lower.contains("release");
        if !is_attack && !is_release { continue; }
        // Prefix = alles vóór "Key action". Spaties en volgnummer eraf; de
        // Hauptwerk-conventie "01. P  Key action …" wordt zo "P".
        let kop = lower.split("key action").next().unwrap_or("");
        let kop = kop.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace());
        let kop = kop.trim();
        if kop.is_empty() { continue; }
        rank_prefix.insert(rid, (kop.to_uppercase(), is_attack));
    }
    let resolve_sample = |sample_id: u32| -> Option<std::path::PathBuf> {
        let (pkg, fname) = sample_file.get(&sample_id)?;
        let dir = pkg_dirs.get(pkg)?;
        Some(dir.join(fname.replace('\\', "/")))
    };
    use crate::grandorgue::KeyboardClickPaths;
    let mut uit: std::collections::HashMap<(String, u8), KeyboardClickPaths>
        = std::collections::HashMap::new();
    for p in pipes {
        let Some(rid) = p.get_u32("RankID") else { continue };
        let Some((prefix, is_attack)) = rank_prefix.get(&rid) else { continue };
        let Some(note) = p.get_u32("NormalMIDINoteNumber") else { continue };
        if note > 127 { continue; }
        let Some(pid) = p.get_u32("PipeID") else { continue };
        let Some(layers) = layers_of_pipe.get(&pid) else { continue };
        let Some((_, lid)) = layers.first() else { continue };
        let Some(sid) = sample_of_layer.get(lid) else { continue };
        let Some(path) = resolve_sample(*sid) else { continue };
        // Dedup: close/front/rear-perspectieven van dezelfde toets geven
        // dezelfde ControllingPalletSwitchID; we pakken één exemplaar.
        let e = uit.entry((prefix.clone(), note as u8)).or_default();
        if *is_attack && e.attack.is_none() { e.attack = Some(path); }
        else if !*is_attack && e.release.is_none() { e.release = Some(path); }
    }
    if !uit.is_empty() {
        let mut prefixen: Vec<&String> = uit.keys().map(|(p, _)| p).collect();
        prefixen.sort(); prefixen.dedup();
        let a: usize = uit.values().filter(|v| v.attack.is_some()).count();
        let r: usize = uit.values().filter(|v| v.release.is_some()).count();
        info!("Hauptwerk toetsmechaniek: {} toetsen (attack: {}, release: {}) op klavier-prefixen {:?}",
              uit.len(), a, r, prefixen);
    }
    uit
}

/// Register-klikgeluiden uit een HW-definitie (0.7.99). Nitra heeft de ranks
/// "Stop action noises Attack" (rank 81 etc.) en "Stop action noises
/// Release" met per registerschakelaar één pijp. Deze helper verzamelt per
/// Stop-id het attack- en release-pad.
fn verzamel_hw_stop_action_clicks(
    pipes: &[FieldMap],
    sample_file: &std::collections::HashMap<u32, (u32, String)>,
    pkg_dirs: &std::collections::HashMap<u32, std::path::PathBuf>,
    layers_of_pipe: &std::collections::HashMap<u32, Vec<(u32, u32)>>,
    sample_of_layer: &std::collections::HashMap<u32, u32>,
    stops: &[FieldMap],
    rank_name: &std::collections::HashMap<u32, String>,
) -> std::collections::HashMap<u32, crate::grandorgue::StopActionClickDef> {
    use crate::grandorgue::StopActionClickDef;
    let resolve_sample = |sample_id: u32| -> Option<std::path::PathBuf> {
        let (pkg, fname) = sample_file.get(&sample_id)?;
        let dir = pkg_dirs.get(pkg)?;
        Some(dir.join(fname.replace('\\', "/")))
    };
    // ControllingSwitchID → stop_id (voor de koppeling vanuit de pijp op de
    // registerschakelaar terug naar de stop die we gaan tonen).
    let mut switch_to_stop: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for s in stops {
        if let (Some(stop_id), Some(sw_id)) = (
            s.get_u32("StopID"), s.get_u32("ControllingSwitchID")
        ) {
            switch_to_stop.insert(sw_id, stop_id);
        }
    }
    let mut uit: std::collections::HashMap<u32, StopActionClickDef> = std::collections::HashMap::new();
    for p in pipes {
        let Some(sw_id) = p.get_u32("ControllingPalletSwitchID") else { continue };
        let Some(stop_id) = switch_to_stop.get(&sw_id) else { continue };
        let rid = p.get_u32("RankID").unwrap_or(0);
        let rname = rank_name.get(&rid).map(|n| n.to_lowercase()).unwrap_or_default();
        // De rank-naam zegt of het attack of release is.
        let is_attack = rname.contains("stop action") && rname.contains("attack");
        let is_release = rname.contains("stop action") && rname.contains("release");
        if !is_attack && !is_release { continue; }
        let Some(pid) = p.get_u32("PipeID") else { continue };
        let Some(layers) = layers_of_pipe.get(&pid) else { continue };
        let Some((_, lid)) = layers.first() else { continue };
        let Some(sid) = sample_of_layer.get(lid) else { continue };
        let Some(path) = resolve_sample(*sid) else { continue };
        let e = uit.entry(*stop_id).or_insert_with(|| StopActionClickDef { attack: None, release: None });
        if is_attack && e.attack.is_none() { e.attack = Some(path); }
        else if is_release && e.release.is_none() { e.release = Some(path); }
    }
    if !uit.is_empty() {
        info!("Hauptwerk register-klikken: {} stops met attack/release-mechaniek", uit.len());
    }
    uit
}

/// Pijpen op switches die geen klaviertoets zijn (0.7.97): dat zijn de
/// NoiseSwitch (ambient), Blower/Motor-switch (blower), speelgoedregisters
/// (Cymbelstern, Vogelsang — later) en soortgelijke. Zelfs sets die er
/// een `Rank` "Noises" voor hebben, zetten die pijpen via Switch i.p.v.
/// via een Stop. Deze helper haalt alleen blower en ambient eruit.
/// Welke divisie bij welke tremulant hoort (0.8.5), afgeleid uit de
/// Tremulant-objecten van de definitie. Hauptwerk legt die band niet vast;
/// de naam doet het werk: "1. Tremulant II" → "Manual II", "Trémolo Récit
/// expressif" → "Récit expressif". Heeft maar één werk tremulant-opnamen,
/// dan is dat de terugval (`enige`).
#[derive(Debug, Default, Clone)]
pub struct TremulantKoppeling {
    /// Bedienende schakelaar van de tremulant → divisienaam.
    pub per_switch: HashMap<u32, String>,
    /// TremulantID → divisienaam.
    pub per_nummer: HashMap<u32, String>,
    /// Het enige werk met tremulant-opnamen, als er precies één is.
    pub enige: Option<String>,
    /// Alle (opgeschoonde) divisienamen, voor naam-matching.
    pub divisies: Vec<String>,
}

/// Lettertekens gelijkschakelen voor naam-matching: kleine letters, accenten
/// weg ("Récit" ↔ "Recit"), ß → ss.
fn naam_sleutel(s: &str) -> String {
    s.to_lowercase().chars().map(|c| match c {
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'á' | 'à' | 'â' | 'ä' | 'å' => 'a',
        'ó' | 'ò' | 'ô' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ç' => 'c',
        'ß' => 's',
        other => other,
    }).collect()
}

/// Divisie waarvan de naam in `naam` voorkomt (hele naam, of het laatste
/// woord — "II" in "Tremulant II"). Eerste treffer in orgelvolgorde.
pub fn match_divisie(naam: &str, divisies: &[String]) -> Option<String> {
    let n = naam_sleutel(naam);
    let woorden: Vec<&str> = n.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    for d in divisies {
        let ds = naam_sleutel(d);
        if !ds.is_empty() && n.contains(&ds) { return Some(d.clone()); }
    }
    for d in divisies {
        let ds = naam_sleutel(d);
        let Some(laatste) = ds.split_whitespace().last() else { continue };
        // Alleen een onderscheidend laatste woord (Romeins cijfer of getal),
        // anders matcht "Noise" op een werk dat toevallig zo eindigt.
        let onderscheidend = laatste.chars().all(|c| matches!(c, 'i' | 'v' | 'x')) || laatste.chars().all(|c| c.is_ascii_digit());
        if onderscheidend && woorden.iter().any(|w| *w == laatste) { return Some(d.clone()); }
    }
    None
}

/// Eerste getal in een schakelaarnaam ("Tremulant 1 Noise" → 1).
fn nummer_in_naam(naam: &str) -> Option<u32> {
    naam.split(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty()).and_then(|s| s.parse().ok())
}

pub fn tremulant_divisies(
    tremulants: &[FieldMap],
    divisions: &[FieldMap],
    manuals: &[ManualDef],
    stops: &[StopDef],
) -> TremulantKoppeling {
    let divisies: Vec<String> = divisions.iter()
        .filter_map(|d| d.get("Name").map(|n| clean_division_name(n)))
        .filter(|n| !n.is_empty())
        .collect();
    // Werken met tremulant-opnamen (een stop met een trem-laag).
    let trem_stops: std::collections::HashSet<u32> = stops.iter()
        .filter(|s| s.pipes.iter().any(|p| matches!(p, PipeDef::Sample { extra, .. } if extra.tremulant_attack.is_some())))
        .map(|s| s.id)
        .collect();
    let trem_divs: Vec<String> = manuals.iter()
        .filter(|m| m.stop_ids.iter().any(|id| trem_stops.contains(id)))
        .map(|m| m.name.clone())
        .collect();
    let enige = if trem_divs.len() == 1 { Some(trem_divs[0].clone()) } else { None };
    let mut k = TremulantKoppeling { divisies, enige, ..Default::default() };
    for tr in tremulants {
        let naam = tr.get("Name").cloned().unwrap_or_default();
        let div = match_divisie(&naam, &k.divisies).or_else(|| k.enige.clone());
        let Some(div) = div else { continue };
        if let Some(id) = tr.get_u32("TremulantID") { k.per_nummer.insert(id, div.clone()); }
        if let Some(sw) = tr.get_u32("ControllingSwitchID") { k.per_switch.insert(sw, div); }
    }
    k
}

fn verzamel_hw_bijgeluiden(
    pipes: &[FieldMap],
    _pipes_by_rank: &std::collections::HashMap<u32, std::collections::HashMap<u32, u32>>,
    sample_file: &std::collections::HashMap<u32, (u32, String)>,
    pkg_dirs: &std::collections::HashMap<u32, std::path::PathBuf>,
    layers_of_pipe: &std::collections::HashMap<u32, Vec<(u32, u32)>>,
    sample_of_layer: &std::collections::HashMap<u32, u32>,
    switches: &[FieldMap],
    linkages: &[FieldMap],
    rank_name: &std::collections::HashMap<u32, String>,
    trem: &TremulantKoppeling,
) -> Vec<crate::grandorgue::BackgroundSoundDef> {
    use crate::grandorgue::{BackgroundSoundDef, BackgroundSoundKind};
    // doel-schakelaar → bron-schakelaars (0.8.4): een ruispijp hangt vaak aan
    // een niet-klikbare hulpschakelaar ("Zimbelstern Noise") die via één
    // linkage aan de echte knop ("26. Zimbelstern") zit.
    let mut bron_van: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for l in linkages {
        if let (Some(src), Some(dst)) = (l.get_u32("SourceSwitchID"), l.get_u32("DestSwitchID")) {
            bron_van.entry(dst).or_default().push(src);
        }
    }
    // Switch-naam, of hij standaard aan staat én of hij klikbaar is.
    // Blower/ambient: default_on=Y en meestal niet klikbaar.
    // Speelgoedregisters: default_on=N én klikbaar=Y (de organist drukt ze aan).
    let switch_info: std::collections::HashMap<u32, (String, bool, bool)> = switches.iter()
        .filter_map(|s| {
            let id = s.get_u32("SwitchID")?;
            let name = s.get("Name").cloned().unwrap_or_default();
            let default_on = s.get("DefaultToEngaged").map(|v| v == "Y").unwrap_or(false);
            let clickable = s.get("Clickable").map(|v| v == "Y").unwrap_or(false);
            Some((id, (name, default_on, clickable)))
        }).collect();
    let resolve_sample = |sample_id: u32| -> Option<std::path::PathBuf> {
        let (pkg, fname) = sample_file.get(&sample_id)?;
        let dir = pkg_dirs.get(pkg)?;
        Some(dir.join(fname.replace('\\', "/")))
    };
    let mut uit = Vec::new();
    // Deduplicatie per (kind, switch_id): sets hebben vaak dezelfde pijp
    // in 3 posities (close/front/rear) via 3 ranks. We pakken één exemplaar.
    // Dedup op (kind, naam) i.p.v. (kind, switch_id): elke perspectief-rang
    // ("Noises (close)", "Noises (front)", "Noises (rear)") heeft eigen
    // __Duplicated-switches voor dezelfde speelgoedknop, dus zonder deze
    // normalisatie verschijnt bv. "Vogelsang" bij Nitra drie keer (0.8.75).
    let mut gezien: std::collections::HashSet<(BackgroundSoundKind, String)> = std::collections::HashSet::new();
    for p in pipes {
        let Some(pid) = p.get_u32("PipeID") else { continue };
        let Some(sw_id) = p.get_u32("ControllingPalletSwitchID") else { continue };
        if sw_id == 0 { continue; }
        let Some((sw_name, default_on, clickable)) = switch_info.get(&sw_id) else { continue };
        let sw_lower = sw_name.to_lowercase();
        // Alleen pijpen in de kale "Noises"-rank zijn kandidaat. De ranks
        // "Stop action noises …" en "Key action noises …" bevatten óók
        // "noise" maar zijn register- en toetsmechaniek (0.7.99/0.7.102);
        // die hier meenemen leverde in de rooktest 180 "speelgoedregisters"
        // op — elke toets en elk register één (0.8.1).
        let rid = p.get_u32("RankID").unwrap_or(0);
        let rname_lower = rank_name.get(&rid).map(|n| n.to_lowercase()).unwrap_or_default();
        if !rname_lower.contains("noise") || rname_lower.contains("action") { continue; }
        // De blower-schakelaar zelf staat vaak standaard UIT en wordt bij het
        // laden via een linkage ("Start blower on organ load") aangezet; de
        // naam is hier dus leidend, niet DefaultToEngaged.
        // Naam zonder Hauptwerk-volgnummer ("96. Blower" → "Blower").
        let mut naam = sw_name
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace())
            .trim()
            .to_string();
        if naam.is_empty() { naam = sw_name.clone(); }
        let kind = if sw_lower.contains("blower") || sw_lower.contains("motor")
            || sw_lower.contains("soufflerie") {
            BackgroundSoundKind::Blower
        } else if *default_on && (sw_lower.contains("noise") || sw_lower.contains("ambient")
            || sw_lower.contains("rausch") || sw_lower.contains("geräusch")) {
            BackgroundSoundKind::Ambient
        } else if sw_lower.contains("trem") {
            // "Tremulant Noise" / "Tremolo Recit Noise" (0.8.4): het
            // mechanische geluid van de tremulant; volgt de tremulantknop.
            BackgroundSoundKind::TremulantNoise
        } else if *clickable && !*default_on && crate::grandorgue::is_toy_naam(&sw_lower) {
            // Speelgoedregister (0.8.71): klikbaar, standaard uit én de naam
            // bevat een echte toy-marker (cymbelstern, nachtegaal, koekoek,
            // …). Zonder die laatste eis gingen combinatie-vastleg-knoppen,
            // pistons en memory-advance ook door als Toy — op compacte sets
            // soms tientallen tegelijk.
            BackgroundSoundKind::Toy
        } else if !*default_on {
            // Niet-klikbare hulpschakelaar: alleen een bedienende knop
            // waarvan de naam een echte toy-marker bevat telt. ("Zimbelstern
            // Noise" ← "26. Zimbelstern" blijft werken.)
            let knop_hit = bron_van.get(&sw_id).and_then(|srcs| srcs.iter()
                .filter_map(|s| switch_info.get(s))
                .find(|(n, _, c)| *c && crate::grandorgue::is_toy_naam(&n.to_lowercase())));
            match knop_hit {
                Some((knop_naam, _, _)) => {
                    naam = knop_naam
                        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace())
                        .trim()
                        .to_string();
                    BackgroundSoundKind::Toy
                }
                None => continue,
            }
        } else {
            continue;
        };
        if !gezien.insert((kind, naam.to_lowercase())) { continue; }
        // Loop de hoofdlaag van deze pijp op naar het WAV-pad.
        let Some(layers) = layers_of_pipe.get(&pid) else { continue };
        let Some((_num, lid)) = layers.first() else { continue };
        let Some(sid) = sample_of_layer.get(lid) else { continue };
        let Some(path) = resolve_sample(*sid) else { continue };
        // Tremulantruis (0.8.5): de divisie van de tremulant die deze ruis
        // bedient — via de schakelaar die de ruisschakelaar aanstuurt, het
        // nummer in de naam ("Tremulant 1 Noise"), de divisienaam in de
        // naam ("Tremolo Recit Noise") of, als maar één werk tremulant-
        // opnamen heeft, dat werk. Zonder match volgt de ruis elke tremulant.
        let division = if matches!(kind, BackgroundSoundKind::TremulantNoise) {
            bron_van.get(&sw_id)
                .and_then(|srcs| srcs.iter().find_map(|s| trem.per_switch.get(s).cloned()))
                .or_else(|| nummer_in_naam(&sw_lower).and_then(|n| trem.per_nummer.get(&n).cloned()))
                .or_else(|| match_divisie(sw_name, &trem.divisies))
                .or_else(|| trem.enige.clone())
        } else {
            None
        };
        info!("Hauptwerk-bijgeluid gevonden: {:?} '{}' op switch '{}' (divisie {:?}) → {:?}", kind, naam, sw_name, division, path);
        uit.push(BackgroundSoundDef {
            kind,
            name: naam,
            path,
            division,
        });
    }
    uit
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
    /// Float (Hauptwerk schrijft ook exponentnotatie zoals "1e+2").
    fn get_f32(&self, key: &str) -> Option<f32>;
}
impl FieldMapExt for FieldMap {
    fn get_u32(&self, key: &str) -> Option<u32> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }
    fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }
    fn get_f32(&self, key: &str) -> Option<f32> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }
}

/// Absolute toonhoogte in cents (6900 = a' 440 Hz) uit een frequentie.
fn hz_to_cents(hz: f32) -> f32 {
    6900.0 + 1200.0 * (hz / 440.0).log2()
}

/// Toonhoogte-metadata per Hauptwerk-pijp (Pipe_SoundEngine01), voor het
/// hertemperen vanaf de gemeten pijptoonhoogte.
#[derive(Debug, Clone, Copy)]
struct HwPipePitch {
    /// NormalMIDINoteNumber: MIDI-toets van de pijp.
    key: u32,
    /// Pitch_Tempered_RankBasePitch64ftHarmonicNum (0/leeg → None → 8).
    harmonic: Option<u32>,
    /// Pitch_OriginalOrgan_PitchHz (elke methodecode, mits gevuld).
    original_cents: Option<f32>,
    /// Percentage van de originele afwijking dat bij hertemperen blijft:
    /// code 4 = Deviation, code 1/3 = 0 (expliciet op het temperament).
    keep_pct: Option<f32>,
    /// Code 3: bewuste doelafwijking in cents boven het temperament.
    tuning_offset_cents: Option<f32>,
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


/// Strip een vooraan geplakte 1-4-cijfer-interne-ID ("2001_", "123_", enz.)
/// uit een registernaam. Geen enkele registernaam begint met een reeks
/// cijfers gevolgd door een underscore; wanneer wel, is dat de interne
/// identifier die de maker per abuis in de Name heeft gezet. De strip
/// werkt alleen als er na de prefix nog iets staat dat niet alleen cijfers
/// is — anders laten we de naam intact zodat we een echte "0", "12" of een
/// toekomstig numeriek register nooit per ongeluk wegvagen.
pub fn strip_numeric_prefix(naam: &str) -> &str {
    let bytes = naam.as_bytes();
    let mut einde_cijfers = 0;
    while einde_cijfers < bytes.len() && einde_cijfers < 4 && bytes[einde_cijfers].is_ascii_digit() {
        einde_cijfers += 1;
    }
    if einde_cijfers == 0 { return naam; }
    if einde_cijfers >= bytes.len() { return naam; }
    if bytes[einde_cijfers] != b'_' { return naam; }
    let rest = &naam[einde_cijfers + 1..];
    if rest.chars().all(|c| c.is_ascii_digit()) || rest.trim().is_empty() {
        return naam;
    }
    rest
}

#[cfg(test)]
mod numeric_prefix_tests {
    use super::strip_numeric_prefix;

    #[test]
    fn standaard_augustine_patronen() {
        assert_eq!(strip_numeric_prefix("2001_Subbas16"), "Subbas16");
        assert_eq!(strip_numeric_prefix("2119_TrompetDisc"), "TrompetDisc");
        assert_eq!(strip_numeric_prefix("1_Bourdon 8"), "Bourdon 8");
    }

    #[test]
    fn naam_zonder_prefix_blijft_ongewijzigd() {
        assert_eq!(strip_numeric_prefix("Principal 8"), "Principal 8");
        assert_eq!(strip_numeric_prefix("Trompet"), "Trompet");
        assert_eq!(strip_numeric_prefix("_Mystery"), "_Mystery");
    }

    #[test]
    fn alleen_cijfers_na_underscore_blijft_staan() {
        // Een onwaarschijnlijk register "12_34" — we laten hem met rust.
        assert_eq!(strip_numeric_prefix("12_34"), "12_34");
    }

    #[test]
    fn te_veel_cijfers_niet_als_prefix_zien() {
        // 5+ cijfers lijkt eerder op een jaartal of code; laat ongemoeid.
        assert_eq!(strip_numeric_prefix("12345_Montre"), "12345_Montre");
    }
}

/// Clean a stop name: strip a leading division prefix like "P  ", "M  ",
/// "PED  ", "GO  ", or "HW " / "SW " (known abbreviation + 1+ spaces; any other
/// 1-4 letter token only when followed by 2+ spaces, to avoid false positives).
/// Shared with the GrandOrgue route (German sets prefix register names too).
pub fn clean_stop_name(raw: &str) -> String {
    let s = raw.trim();
    let bytes = s.as_bytes();
    // Count leading ASCII letters.
    let letters = bytes.iter().take_while(|b| b.is_ascii_alphabetic()).count();
    // "Pos: Prestant 8" — een dubbele punt achter een kort woord is altijd een
    // divisieaanduiding. Die staat al boven de kolom, dus weg ermee.
    if (1..=6).contains(&letters) && bytes.get(letters) == Some(&b':') {
        let rest = s[letters + 1..].trim();
        if !rest.is_empty() {
            return rest.to_string();
        }
    }
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

/// De harmonische van wat je hoort: die van de rang, verschoven met de sprong
/// die het register naar die rang maakt.
///
/// Een octaaf hoger is een twee keer zo hoge harmonische (16' → 8'), een
/// kwint hoger anderhalf keer (8' → 5 1/3'). Zonder sprong verandert er
/// niets.
fn klinkende_harmonische(rang: u32, inc: i32) -> u32 {
    if rang == 0 || inc == 0 {
        return rang;
    }
    ((rang as f64 * 2f64.powf(inc as f64 / 12.0)).round() as u32).max(1)
}

/// Het voorvoegsel dat vóór élke registernaam van een divisie staat.
///
/// Sets zetten er van alles voor: een klaviernummer ("4 Gedekt 8"), een
/// afkorting ("CB Clarin 2", "P Octaaf 8") of allebei door elkaar. Het staat
/// al boven de kolom, dus het mag eraf — maar aan één naam alleen zie je niet
/// óf het een aanduiding is. "4" kan een voetmaat zijn, "Sub" kan het begin
/// van de naam zijn.
///
/// Pas als de hele divisie hetzelfde voorvoegsel draagt is het er een. Vandaar:
/// minstens twee registers, elk voorvoegsel dat voorkomt is hetzelfde, de
/// meerderheid draagt het, en wat erachter overblijft begint met een letter.
/// Die laatste eis houdt "1 1/3 Quint" heel en redt een divisie van "Sub 16"
/// en "Sub 8".
pub fn gedeeld_voorvoegsel(namen: &[&str]) -> Option<String> {
    if namen.len() < 2 {
        return None;
    }
    let mut gevonden: Option<String> = None;
    let mut met = 0usize;
    for naam in namen {
        match voorvoegsel_van(naam) {
            None => continue,
            Some(v) => {
                match &gevonden {
                    None => gevonden = Some(v),
                    Some(eerder) if !eerder.eq_ignore_ascii_case(&v) => return None,
                    _ => {}
                }
                met += 1;
            }
        }
    }
    gevonden.filter(|_| met * 2 > namen.len())
}

/// Het voorvoegsel vooraan één naam, als het er een kan zijn: één cijfer of
/// hoogstens vier letters, eventueel met een punt of dubbele punt erachter,
/// en dan witruimte met een echte naam erna.
fn voorvoegsel_van(naam: &str) -> Option<String> {
    let n = naam.trim_start();
    let kop: String = n.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    let lengte = kop.len();
    let is_cijfer = lengte == 1 && kop.chars().all(|c| c.is_ascii_digit());
    let is_woord = (1..=4).contains(&lengte) && kop.chars().all(|c| c.is_ascii_alphabetic());
    if !(is_cijfer || is_woord) {
        return None;
    }
    let na = &n[lengte..];
    let na = na.strip_prefix([':', '.']).unwrap_or(na);
    let rest = na.strip_prefix(|c: char| c.is_whitespace())?.trim_start();
    // Er moet een náám overblijven, geen breuk of voetmaat.
    rest.chars().next().filter(|c| c.is_alphabetic())?;
    Some(n[..n.len() - na.len()].to_string())
}

/// De registernaam zonder dat gedeelde voorvoegsel.
pub fn strip_voorvoegsel(naam: &str, voorvoegsel: &str) -> String {
    match voorvoegsel_van(naam) {
        Some(v) if v.eq_ignore_ascii_case(voorvoegsel) => {
            let n = naam.trim_start();
            n[v.len()..].trim_start().to_string()
        }
        _ => naam.to_string(),
    }
}

/// Registernaam zonder het voettal dat er al achter staat.
///
/// Hauptwerk-sets schrijven het voettal meestal kaal achter de naam
/// ("Gedekt 8", "Sifflet 1 1/3") of met een voetwoord ("Subbaß 16 Fuß"), niet
/// met een voetteken. Daardoor bleef het staan en kwam het er onder de knop
/// nóg een keer bij.
///
/// We strippen alleen wanneer het achter de naam hetzelfde voettal is als het
/// register werkelijk heeft. "Mixtuur 4" op een 2'-rang houdt dus zijn 4: dat
/// zijn koren en geen voeten.
pub fn strip_pitch_from_name(name: &str, pitch: &str) -> String {
    let doel = normaliseer_voettal(pitch);
    if doel.is_empty() {
        return name.to_string();
    }
    let woorden: Vec<&str> = name.split_whitespace().collect();
    // Een voettal beslaat hooguit drie woorden: "2 2/3 Fuß".
    for aantal in [3usize, 2, 1] {
        if woorden.len() <= aantal {
            continue; // er moet een naam overblijven
        }
        let staart = woorden[woorden.len() - aantal..].join(" ");
        if normaliseer_voettal(&staart) == doel {
            return woorden[..woorden.len() - aantal].join(" ");
        }
    }
    name.to_string()
}

/// Registernaam zonder een voettal dat direct aan het laatste woord vastzit
/// ("Principal8", "Bourdon16'"): de voetmaat staat al onder de knop. Alleen
/// echte voettallen en minstens drie letters ervoor, zodat "Cornet5" of
/// "Quint3" blijven staan.
pub fn zonder_vastgeplakte_voetmaat(name: &str) -> String {
    let mut woorden: Vec<&str> = name.split_whitespace().collect();
    let Some(laatste) = woorden.pop() else { return name.to_string() };
    let kaal = laatste.trim_end_matches(['\'', '\u{2032}', '\u{2019}']);
    let cijfers = kaal.chars().rev().take_while(|c| c.is_ascii_digit()).count();
    let (kop, getal) = kaal.split_at(kaal.len() - cijfers);
    if matches!(getal, "1" | "2" | "4" | "8" | "16" | "32" | "64")
        && kop.chars().count() >= 3 && kop.chars().all(|c| c.is_alphabetic())
    {
        woorden.push(kop);
        return woorden.join(" ");
    }
    name.to_string()
}

/// Voettal in één vorm: zonder voetteken of voetwoord, breuken uitgeschreven,
/// zodat "8'", "8", "8 Fuß" en "2⅔'" met "2 2/3'" te vergelijken zijn.
fn normaliseer_voettal(s: &str) -> String {
    let mut uit = String::new();
    for c in s.chars() {
        match c {
            '\'' | '\u{2032}' | '\u{2019}' | '"' => {}
            '\u{00BC}' => uit.push_str(" 1/4"),
            '\u{00BD}' => uit.push_str(" 1/2"),
            '\u{00BE}' => uit.push_str(" 3/4"),
            '\u{2153}' => uit.push_str(" 1/3"),
            '\u{2154}' => uit.push_str(" 2/3"),
            '\u{2155}' => uit.push_str(" 1/5"),
            '\u{2156}' => uit.push_str(" 2/5"),
            '\u{2157}' => uit.push_str(" 3/5"),
            '\u{2158}' => uit.push_str(" 4/5"),
            '\u{215B}' => uit.push_str(" 1/8"),
            '\u{215C}' => uit.push_str(" 3/8"),
            '\u{215D}' => uit.push_str(" 5/8"),
            '\u{215E}' => uit.push_str(" 7/8"),
            _ => uit.push(c),
        }
    }
    let mut woorden: Vec<&str> = uit.split_whitespace().collect();
    // Voetwoord achteraan weghalen: Fuß, voet, ft, pieds, feet.
    while let Some(laatste) = woorden.last() {
        let l = laatste.trim_end_matches('.').to_lowercase();
        if matches!(l.as_str(), "fuss" | "fuß" | "fus" | "voet" | "voeten" | "ft" | "feet" | "foot" | "pied" | "pieds") {
            woorden.pop();
        } else {
            break;
        }
    }
    woorden.join(" ")
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
/// Publiek omdat de bibliotheek hem gebruikt om de pakketmappen (met de
/// console-/orgelfoto) te vinden voor de kaartafbeelding.
pub fn find_package_root(odf_path: &Path) -> Option<PathBuf> {
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
pub fn scan_package_dirs(root: &Path) -> HashMap<u32, PathBuf> {
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

    /// De divisieaanduiding staat al boven de kolom. "Pos:" en "RW " horen
    /// dus niet nóg een keer voor elke registernaam te staan.
    /// Niet elke set zet de tremulant-opname als tweede laag op de pijp.
    /// Rotterdam en Utrecht gebruiken een aparte "tremmed" rang waar een
    /// schakelaar het register naartoe zet. Die komt op dezelfde plek terecht
    /// als de tweede laag, zodat de rest van de keten er niets van merkt.
    #[test]
    fn een_tremmed_rang_levert_dezelfde_tremulant_opname() {
        let def = mini_definition();
        let montre = def.stops.iter().find(|s| s.name == "Montre 8").unwrap();
        let PipeDef::Sample { path, extra } = &montre.pipes[0] else {
            panic!("geen sample-pijp");
        };
        // De hoofdopname blijft droog.
        assert!(path.ends_with("Montre/036-c.wav"), "droge opname: {path:?}");
        // En de tremulant-opname komt uit de tremmed rang, met zijn release.
        let trem = extra.tremulant_attack.as_ref().expect("tremulant-opname");
        assert!(
            trem.path.ends_with("Montre_trem/036-c.wav"),
            "tremulant-opname: {:?}",
            trem.path
        );
        assert_eq!(extra.tremulant_releases.len(), 1);
        assert!(extra.tremulant_releases[0]
            .path
            .ends_with("Montre_trem/R0/036-c.wav"));
    }

    #[test]
    fn de_divisieaanduiding_gaat_van_de_registernaam_af() {
        // Dubbele punt: altijd een aanduiding, ook zonder divisienaam erbij.
        assert_eq!(clean_stop_name("Pos: Prestant 8"), "Prestant 8");
        assert_eq!(clean_stop_name("HW:Trompet 8"), "Trompet 8");
        // De bestaande vormen blijven werken.
        assert_eq!(clean_stop_name("PED  Soubasse 16"), "Soubasse 16");
        assert_eq!(clean_stop_name("P  Subbaß 16 Fuß"), "Subbaß 16 Fuß");

        // Een korte aanduiding zonder dubbele punt en met één spatie is op
        // zichzelf niet genoeg — die gaat pas weg als de hele divisie hem
        // draagt (zie `gedeeld_voorvoegsel`). Zo blijft "V Cornet" heel.
        assert_eq!(clean_stop_name("Pd Subbas 16"), "Pd Subbas 16");
        assert_eq!(clean_stop_name("V Cornet"), "V Cornet");
    }

    /// Een register kan een rang een octaaf hoger aanspreken. Dan is de
    /// voetmaat die je hoort niet die van de rang: een pedaal-Bourdon 8 op
    /// een 16'-rang klinkt als 8', en zo hoort hij ook op de knop te staan.
    #[test]
    fn de_voetmaat_volgt_de_octaafsprong_van_het_register() {
        assert_eq!(klinkende_harmonische(4, 0), 4);    // 16' blijft 16'
        assert_eq!(klinkende_harmonische(4, 12), 8);   // 16'-rang → 8'
        assert_eq!(klinkende_harmonische(8, 12), 16);  // 8'-rang  → 4'
        assert_eq!(klinkende_harmonische(8, 24), 32);  // 8'-rang  → 2'
        assert_eq!(klinkende_harmonische(8, -12), 4);  // 8'-rang  → 16'
        assert_eq!(klinkende_harmonische(8, 7), 12);   // kwint    → 5 1/3'
        assert_eq!(klinkende_harmonische(0, 12), 0);   // onbekend blijft onbekend
    }

    /// Sommige sets zetten een klaviernummer of een afkorting vóór elke
    /// registernaam ("4 Gedekt 8", "CB Clarin 2"). Dat staat al boven de
    /// kolom. Maar je mag het pas wegstrepen als de hele divisie hetzelfde
    /// draagt, anders haal je een voetmaat of het begin van een naam weg.
    #[test]
    fn het_voorvoegsel_gaat_eraf_als_de_hele_divisie_het_draagt() {
        // Klaviernummer (Rotterdam, Borstwerk).
        let bw = ["4 Gedekt 8", "4 Praestant 4", "4 Octaaf 2", "4 Tertiaan"];
        assert_eq!(gedeeld_voorvoegsel(&bw).as_deref(), Some("4"));
        assert_eq!(strip_voorvoegsel("4 Gedekt 8", "4"), "Gedekt 8");

        // Afkorting (Rotterdam, Chamade en Pedaal).
        let cham = ["CB Clarin 2", "CB Orlos 8", "CB Trompeta batalla 8"];
        assert_eq!(gedeeld_voorvoegsel(&cham).as_deref(), Some("CB"));
        assert_eq!(strip_voorvoegsel("CB Clarin 2", "CB"), "Clarin 2");
        let ped = ["P Praestant 32", "P Octaaf 16", "P Cornet"];
        assert_eq!(gedeeld_voorvoegsel(&ped).as_deref(), Some("P"));

        // Met dubbele punt, en één register zonder voorvoegsel mag.
        let pos = ["Pos: Prestant 8", "Pos: Fluit 4", "Cornet"];
        assert_eq!(gedeeld_voorvoegsel(&pos).as_deref(), Some("Pos:"));
        assert_eq!(strip_voorvoegsel("Cornet", "Pos:"), "Cornet");

        // Twee verschillende voorvoegsels: dan is het er geen.
        assert_eq!(gedeeld_voorvoegsel(&["1 Prestant 8", "2 Fluit 4"]), None);
        assert_eq!(gedeeld_voorvoegsel(&["HW Prestant 8", "RW Fluit 4"]), None);
        // Voetmaten en breuken vooraan blijven staan.
        assert_eq!(gedeeld_voorvoegsel(&["1 1/3 Quint", "1 3/5 Terts"]), None);
        assert_eq!(gedeeld_voorvoegsel(&["16 Bourdon", "16 Prestant"]), None);
        // En een divisie van "Sub 16" en "Sub 8" houdt zijn namen: er moet
        // een naam overblijven, geen kaal getal.
        assert_eq!(gedeeld_voorvoegsel(&["Sub 16", "Sub 8"]), None);
        // Zonder voorvoegsel verandert er niets, en één register zegt niets.
        assert_eq!(gedeeld_voorvoegsel(&["Prestant 8", "Octaaf 4"]), None);
        assert_eq!(gedeeld_voorvoegsel(&["4 Gedekt 8"]), None);
    }

    /// Het voettal staat al onder de knop. Staat het kaal achter de naam
    /// ("Gedekt 8") of met een voetwoord ("Subbaß 16 Fuß"), dan moet het daar
    /// weg — maar alleen als het hetzelfde voettal is.
    #[test]
    fn het_voettal_komt_niet_twee_keer_voor() {
        assert_eq!(strip_pitch_from_name("Gedekt 8", "8'"), "Gedekt");
        assert_eq!(strip_pitch_from_name("Praestant 4", "4'"), "Praestant");
        assert_eq!(strip_pitch_from_name("Prestant 8'", "8'"), "Prestant");
        assert_eq!(strip_pitch_from_name("Sifflet 1 1/3", "1 1/3'"), "Sifflet");
        assert_eq!(strip_pitch_from_name("Roerquint 2 2/3", "2 2/3'"), "Roerquint");
        assert_eq!(strip_pitch_from_name("Subbaß 16 Fuß", "16'"), "Subbaß");
        assert_eq!(strip_pitch_from_name("Nasard 2\u{2154}'", "2 2/3'"), "Nasard");

        // Een getal dat géén voettal is blijft staan: "Mixtuur 4" op een
        // 2'-rang telt koren.
        assert_eq!(strip_pitch_from_name("Mixtuur 4", "2'"), "Mixtuur 4");
        // Zonder voettal in de naam verandert er niets.
        assert_eq!(strip_pitch_from_name("Carillion", "4'"), "Carillion");
        assert_eq!(strip_pitch_from_name("Cornet", ""), "Cornet");
        // En er moet altijd een naam overblijven.
        assert_eq!(strip_pitch_from_name("8", "8'"), "8");
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
    /// met R0/R1-releases en een tremulant-laag, plus een tweede StopRank
    /// "(rear)" over dezelfde noten → perspectief-laag) en "Montre 8" (1 pijp,
    /// niet omkast, release wijst naar hetzelfde bestand als de attack → geskipt).
    fn mini_xml() -> String {
        r#"<Hauptwerk FileFormat="Organ" FileFormatVersion="4.0">
<ObjectList ObjectType="_General"><_General><Identification_Name>Testorgel</Identification_Name></_General></ObjectList>
<ObjectList ObjectType="Division"><Division><DivisionID>1</DivisionID><Name>Grand Orgue</Name></Division></ObjectList>
<ObjectList ObjectType="Stop"><Stop><StopID>1</StopID><Name>Bourdon 8</Name><DivisionID>1</DivisionID></Stop><Stop><StopID>2</StopID><Name>Montre 8</Name><DivisionID>1</DivisionID></Stop></ObjectList>
<ObjectList ObjectType="StopRank"><StopRank><StopID>1</StopID><RankID>1</RankID><Name>GO  Bourdon 8 (front)</Name><MIDINoteNumOfFirstMappedDivisionInputNode>36</MIDINoteNumOfFirstMappedDivisionInputNode><NumberOfMappedDivisionInputNodes>2</NumberOfMappedDivisionInputNodes><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank></StopRank><StopRank><StopID>1</StopID><RankID>1001</RankID><Name>GO  Bourdon 8 (rear)</Name><MIDINoteNumOfFirstMappedDivisionInputNode>36</MIDINoteNumOfFirstMappedDivisionInputNode><NumberOfMappedDivisionInputNodes>2</NumberOfMappedDivisionInputNodes><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank></StopRank><StopRank><StopID>2</StopID><RankID>2</RankID><AlternateRankID>3</AlternateRankID><SwitchIDToSwitchToAlternateRank>77</SwitchIDToSwitchToAlternateRank><MIDINoteNumOfFirstMappedDivisionInputNode>36</MIDINoteNumOfFirstMappedDivisionInputNode><NumberOfMappedDivisionInputNodes>1</NumberOfMappedDivisionInputNodes><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank></StopRank></ObjectList>
<ObjectList ObjectType="Rank"><Rank><RankID>1</RankID><Name>01. GO  Bourdon 8 (front)</Name></Rank><Rank><RankID>1001</RankID><Name>01. GO  Bourdon 8 (rear)</Name></Rank><Rank><RankID>2</RankID><Name>02. GO  Montre 8</Name></Rank><Rank><RankID>3</RankID><Name>02. GO  Montre 8 (tremmed)</Name></Rank></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01"><Pipe_SoundEngine01><PipeID>11</PipeID><RankID>1</RankID><NormalMIDINoteNumber>36</NormalMIDINoteNumber><Pitch_Tempered_RankBasePitch64ftHarmonicNum>8</Pitch_Tempered_RankBasePitch64ftHarmonicNum><Pitch_OriginalOrgan_SpecificationMethodCode>2</Pitch_OriginalOrgan_SpecificationMethodCode><Pitch_OriginalOrgan_PitchHz>65.8</Pitch_OriginalOrgan_PitchHz></Pipe_SoundEngine01><Pipe_SoundEngine01><PipeID>12</PipeID><RankID>1</RankID><NormalMIDINoteNumber>37</NormalMIDINoteNumber></Pipe_SoundEngine01><Pipe_SoundEngine01><PipeID>1011</PipeID><RankID>1001</RankID><NormalMIDINoteNumber>36</NormalMIDINoteNumber></Pipe_SoundEngine01><Pipe_SoundEngine01><PipeID>1012</PipeID><RankID>1001</RankID><NormalMIDINoteNumber>37</NormalMIDINoteNumber></Pipe_SoundEngine01><Pipe_SoundEngine01><PipeID>31</PipeID><RankID>3</RankID><NormalMIDINoteNumber>36</NormalMIDINoteNumber></Pipe_SoundEngine01><Pipe_SoundEngine01><PipeID>21</PipeID><RankID>2</RankID><NormalMIDINoteNumber>36</NormalMIDINoteNumber><Pitch_Tempered_BaseTuningSchemeCode>4</Pitch_Tempered_BaseTuningSchemeCode><Pitch_Tempered_BaseTuningDeviation>1e+2</Pitch_Tempered_BaseTuningDeviation></Pipe_SoundEngine01></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_Layer"><Pipe_SoundEngine01_Layer><LayerID>111</LayerID><PipeID>11</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>511</LayerID><PipeID>11</PipeID><PipeLayerNumber>2</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>112</LayerID><PipeID>12</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>1111</LayerID><PipeID>1011</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>1112</LayerID><PipeID>1012</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>121</LayerID><PipeID>21</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer><Pipe_SoundEngine01_Layer><LayerID>131</LayerID><PipeID>31</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_AttackSample"><Pipe_SoundEngine01_AttackSample><UniqueID>1</UniqueID><LayerID>111</LayerID><SampleID>1</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>2</UniqueID><LayerID>511</LayerID><SampleID>2</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>3</UniqueID><LayerID>112</LayerID><SampleID>3</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>4</UniqueID><LayerID>121</LayerID><SampleID>4</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>7</UniqueID><LayerID>131</LayerID><SampleID>20</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>5</UniqueID><LayerID>1111</LayerID><SampleID>7</SampleID></Pipe_SoundEngine01_AttackSample><Pipe_SoundEngine01_AttackSample><UniqueID>6</UniqueID><LayerID>1112</LayerID><SampleID>8</SampleID></Pipe_SoundEngine01_AttackSample></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_ReleaseSample"><Pipe_SoundEngine01_ReleaseSample><UniqueID>10</UniqueID><LayerID>111</LayerID><SampleID>5</SampleID><ReleaseSelCriteria_LatestKeyReleaseTimeMs>99999</ReleaseSelCriteria_LatestKeyReleaseTimeMs></Pipe_SoundEngine01_ReleaseSample><Pipe_SoundEngine01_ReleaseSample><UniqueID>11</UniqueID><LayerID>111</LayerID><SampleID>6</SampleID><ReleaseSelCriteria_LatestKeyReleaseTimeMs>750</ReleaseSelCriteria_LatestKeyReleaseTimeMs></Pipe_SoundEngine01_ReleaseSample><Pipe_SoundEngine01_ReleaseSample><UniqueID>12</UniqueID><LayerID>121</LayerID><SampleID>4</SampleID><ReleaseSelCriteria_LatestKeyReleaseTimeMs>99999</ReleaseSelCriteria_LatestKeyReleaseTimeMs></Pipe_SoundEngine01_ReleaseSample><Pipe_SoundEngine01_ReleaseSample><UniqueID>14</UniqueID><LayerID>131</LayerID><SampleID>21</SampleID><ReleaseSelCriteria_LatestKeyReleaseTimeMs>99999</ReleaseSelCriteria_LatestKeyReleaseTimeMs></Pipe_SoundEngine01_ReleaseSample><Pipe_SoundEngine01_ReleaseSample><UniqueID>13</UniqueID><LayerID>511</LayerID><SampleID>9</SampleID><ReleaseSelCriteria_LatestKeyReleaseTimeMs>99999</ReleaseSelCriteria_LatestKeyReleaseTimeMs></Pipe_SoundEngine01_ReleaseSample></ObjectList>
<ObjectList ObjectType="Sample"><Sample><SampleID>1</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/A0/036-c.wav</SampleFilename><Pitch_SpecificationMethodCode>4</Pitch_SpecificationMethodCode><Pitch_ExactSamplePitch>66</Pitch_ExactSamplePitch></Sample><Sample><SampleID>2</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/AT0/036-c.wav</SampleFilename></Sample><Sample><SampleID>3</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/A0/037-c#.wav</SampleFilename></Sample><Sample><SampleID>4</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Montre/036-c.wav</SampleFilename></Sample><Sample><SampleID>5</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/R0/036-c.wav</SampleFilename></Sample><Sample><SampleID>6</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/R1/036-c.wav</SampleFilename></Sample><Sample><SampleID>7</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon_rear/A0/036-c.wav</SampleFilename></Sample><Sample><SampleID>8</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon_rear/A0/037-c#.wav</SampleFilename></Sample><Sample><SampleID>9</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/RT0/036-c.wav</SampleFilename></Sample><Sample><SampleID>20</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Montre_trem/036-c.wav</SampleFilename></Sample><Sample><SampleID>21</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Montre_trem/R0/036-c.wav</SampleFilename></Sample></ObjectList>
<ObjectList ObjectType="Enclosure"><Enclosure><EnclosureID>7</EnclosureID><Name>Enclosure Grand Orgue</Name></Enclosure></ObjectList>
<ObjectList ObjectType="EnclosurePipe"><EnclosurePipe><PipeID>11</PipeID><EnclosureID>7</EnclosureID></EnclosurePipe><EnclosurePipe><PipeID>12</PipeID><EnclosureID>7</EnclosureID></EnclosurePipe></ObjectList>
</Hauptwerk>"#.to_string()
    }

    fn mini_definition() -> OrganDefinition {
        let mut pkg_dirs = HashMap::new();
        pkg_dirs.insert(1u32, PathBuf::from("/pkg"));
        build_definition(&mini_xml(), PathBuf::from("/root"), &pkg_dirs).expect("build")
    }

    /// AudioEngine_EnablePlayingAtOriginalOrganPitch = N (0.9.23): de set heeft
    /// geen originele orgeltoonhoogte en moet altijd op de getempereerde
    /// doeltoon spelen (Cracow). Afwezig of Y = gewoon gedrag (false), zodat
    /// GrandOrgue en gewone Hauptwerk-sets in "Origineel" op opnametoon blijven.
    #[test]
    fn enable_playing_at_original_pitch_vlag() {
        let mut pkg = HashMap::new();
        pkg.insert(1u32, PathBuf::from("/pkg"));
        let bouw = |xml: &str| build_definition(xml, PathBuf::from("/root"), &pkg).expect("build");
        let met = |waarde: &str| {
            mini_xml().replace(
                "<Identification_Name>Testorgel</Identification_Name>",
                &format!("<Identification_Name>Testorgel</Identification_Name><AudioEngine_EnablePlayingAtOriginalOrganPitch>{waarde}</AudioEngine_EnablePlayingAtOriginalOrganPitch>"),
            )
        };
        // Afwezig → false (Hauptwerk-standaard: speelt in Origineel op opnametoon).
        assert!(!bouw(&mini_xml()).organ.geen_originele_toonhoogte);
        // = N → true (geen originele toonhoogte, altijd getempereerd).
        assert!(bouw(&met("N")).organ.geen_originele_toonhoogte);
        // = Y → false.
        assert!(!bouw(&met("Y")).organ.geen_originele_toonhoogte);
    }

    /// Hauptwerks eigen meegeleverde sets staan in het gesloten .hbw-formaat.
    /// De orgeldefinitie is dan gewoon leesbaar, dus zonder deze controle komt
    /// er een volledig orgel binnen waar geen noot uit komt.
    #[test]
    fn een_set_met_hbw_opnamen_wordt_geweigerd() {
        let mut pkg_dirs = HashMap::new();
        pkg_dirs.insert(1u32, PathBuf::from("/pkg"));
        let err = build_definition(
            &mini_xml().replace(".wav", ".hbw"),
            PathBuf::from("/root"),
            &pkg_dirs,
        )
        .expect_err("een set zonder afspeelbare opnamen hoort geweigerd te worden");
        let melding = err.to_string();
        assert!(melding.contains(".hbw"), "onverwachte fout: {melding}");
        // De code waarmee de app deze uitleg vertaalt (App.svelte: vertaalFout).
        // Verdwijnt hij, dan krijgt iedereen de Nederlandse zin te zien.
        assert!(melding.contains("[gesloten-formaat "), "vertaalcode ontbreekt: {melding}");

        // Dezelfde set met gewone WAV-opnamen komt gewoon binnen.
        assert!(!mini_definition().stops.is_empty());
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
        // Tremulant-laag (PipeLayerNumber 2) → tremulant_attack, hoofdlaag droog.
        assert!(extra
            .tremulant_attack
            .as_ref()
            .unwrap()
            .path
            .ends_with("Bourdon/AT0/036-c.wav"));
        // …inclusief de release-samples van díe laag (RT0), zodat loslaten met
        // tremulant aan de tremulant-release geeft en niet de droge.
        assert_eq!(extra.tremulant_releases.len(), 1);
        assert!(extra.tremulant_releases[0].path.ends_with("Bourdon/RT0/036-c.wav"));
        assert_eq!(extra.tremulant_releases[0].max_key_press_time_ms, None);

        // Bourdon-pijp 37 heeft geen releases en geen tremulant-laag.
        let PipeDef::Sample { extra: e37, .. } = &bourdon.pipes[1] else {
            panic!("pijp 37 hoort een sample te zijn");
        };
        assert!(e37.releases.is_empty());
        assert!(e37.tremulant_attack.is_none());
        assert!(e37.tremulant_releases.is_empty());

        // Montre: release wijst naar hetzelfde bestand als de attack → geskipt.
        let PipeDef::Sample { extra: em, .. } = &montre.pipes[0] else {
            panic!("montre-pijp hoort een sample te zijn");
        };
        assert!(em.releases.is_empty());
        // Montre heeft geen tremulant-láág; zijn tremulant-opname komt uit de
        // tremmed rang (zie `een_tremmed_rang_levert_dezelfde_tremulant_opname`).
        assert!(em
            .tremulant_attack
            .as_ref()
            .is_some_and(|a| a.path.ends_with("Montre_trem/036-c.wav")));
    }

    #[test]
    fn test_pitch_metadata() {
        // Hertemperen: Sample-pitch (method 4, Hz), originele pijptoon (method
        // 2, Hz), toets/harmonisch per pijp en het behoud-percentage.
        let def = mini_definition();
        let bourdon = def.stops.iter().find(|s| s.name == "Bourdon 8").unwrap();
        let montre = def.stops.iter().find(|s| s.name == "Montre 8").unwrap();
        let PipeDef::Sample { extra, .. } = &bourdon.pipes[0] else { panic!("sample") };
        // 66 Hz → 6900 + 1200·log2(66/440) = 3615.64 ct
        assert!((extra.sample_pitch_cents.unwrap() - 3615.64).abs() < 0.05, "{:?}", extra.sample_pitch_cents);
        // 65.8 Hz → 3610.39 ct
        assert!((extra.original_pitch_cents.unwrap() - 3610.39).abs() < 0.05, "{:?}", extra.original_pitch_cents);
        assert_eq!(extra.key_midi_note, Some(36));
        assert_eq!(extra.harmonic_number, Some(8));
        assert_eq!(extra.retune_keep_pct, None);
        // Pijp 37: geen pitch-velden → alleen de toets.
        let PipeDef::Sample { extra: e37, .. } = &bourdon.pipes[1] else { panic!("sample") };
        assert_eq!(e37.key_midi_note, Some(37));
        assert_eq!(e37.sample_pitch_cents, None);
        assert_eq!(e37.original_pitch_cents, None);
        // Montre: BaseTuningSchemeCode 4 + "1e+2" → keep 100 %.
        let PipeDef::Sample { extra: em, .. } = &montre.pipes[0] else { panic!("sample") };
        assert_eq!(em.retune_keep_pct, Some(100.0));
        assert_eq!(em.harmonic_number, None);
    }

    /// 0.9.24: de tremulant-opname draagt haar EIGEN ODF-samplepitch, niet die
    /// van de droge laag en niet de WAV-koptekst. Zonder dit viel het
    /// hertemperen van de tremulantvariant terug op de smpl-chunk van de WAV,
    /// die van de ODF-opgave kan afwijken (Nitra: ODF 709 Hz, WAV 712 Hz).
    #[test]
    fn tremulant_attack_draagt_eigen_odf_samplepitch() {
        // Geef de tremulant-opname van Bourdon (Sample 2, Bourdon/AT0) een eigen
        // Pitch_ExactSamplePitch (70 Hz) die afwijkt van de droge (Sample 1 = 66 Hz).
        let xml = mini_xml().replace(
            "<Sample><SampleID>2</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/AT0/036-c.wav</SampleFilename></Sample>",
            "<Sample><SampleID>2</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/AT0/036-c.wav</SampleFilename><Pitch_SpecificationMethodCode>4</Pitch_SpecificationMethodCode><Pitch_ExactSamplePitch>70</Pitch_ExactSamplePitch></Sample>",
        );
        assert!(xml.contains("<Pitch_ExactSamplePitch>70</Pitch_ExactSamplePitch>"), "testdata niet vervangen");
        let mut pkg = HashMap::new();
        pkg.insert(1u32, PathBuf::from("/pkg"));
        let def = build_definition(&xml, PathBuf::from("/root"), &pkg).expect("build");
        let bourdon = def.stops.iter().find(|s| s.name == "Bourdon 8").unwrap();
        let PipeDef::Sample { extra, .. } = &bourdon.pipes[0] else { panic!("sample") };
        let trem = extra.tremulant_attack.as_ref().expect("trem-attack ontbreekt");
        let dry = extra.sample_pitch_cents.expect("droge samplepitch");
        let tp = trem.sample_pitch_cents.expect("trem-samplepitch moet nu gevuld zijn");
        // 66 Hz → 3615.64 ct (droog), 70 Hz → 3717.26 ct (trem): ~102 ct apart.
        assert!((dry - hz_to_cents(66.0)).abs() < 0.05, "droog {dry}");
        assert!((tp - hz_to_cents(70.0)).abs() < 0.05, "trem {tp}");
        assert!((tp - dry).abs() > 1.0, "trem moet van droog verschillen: {tp} vs {dry}");
    }

    /// Piotr Grabowski's sets (Nitra, Nowa Ruda, en vele andere) zetten de
    /// originele pijptoon in Pitch_OriginalOrgan_PitchHz met
    /// SpecificationMethodCode = 1 (niet 2). Tot 0.7.93 negeerden we die Hz-
    /// waarde omdat we alleen op code 2 lazen; daardoor klonken vervangen
    /// pijpen (bv. Copula maior g'' die op bestand `077-f.wav` wijst) op de
    /// toon van het buurbestand. Vanaf 0.7.94 telt elke gevulde Hz-waarde.
    #[test]
    fn pitchhz_bij_methodecode_1_wordt_opgepikt() {
        // Fixture: pijp 11 krijgt code 1 (i.p.v. code 2) met dezelfde Hz-waarde.
        let xml = mini_xml().replace(
            "<Pitch_OriginalOrgan_SpecificationMethodCode>2</Pitch_OriginalOrgan_SpecificationMethodCode>",
            "<Pitch_OriginalOrgan_SpecificationMethodCode>1</Pitch_OriginalOrgan_SpecificationMethodCode>",
        );
        let mut pkg_dirs = HashMap::new();
        pkg_dirs.insert(1u32, PathBuf::from("/pkg"));
        let def = build_definition(&xml, PathBuf::from("/root"), &pkg_dirs).expect("build");
        let bourdon = def.stops.iter().find(|s| s.name == "Bourdon 8").unwrap();
        let PipeDef::Sample { extra, .. } = &bourdon.pipes[0] else { panic!("sample") };
        // 65.8 Hz → 3610.39 ct, ook bij code 1.
        assert!(
            extra.original_pitch_cents.map(|c| (c - 3610.39).abs() < 0.05).unwrap_or(false),
            "originele pijptoon bij methodecode 1 niet gelezen: {:?}",
            extra.original_pitch_cents
        );
    }

    /// BaseTuningSchemeCode: code 3 = temperament + Deviation in cents (bewuste
    /// zweving, bv. Unda maris +13), code 1 = precies op het temperament. Beide
    /// zijn een opgave van de definitie (keep 0), zodat een registernaam ze
    /// niet meer naar "opname-afwijking behouden" kan omzetten. Code 4 blijft
    /// een percentage, en een pijp zonder code blijft onbekend.
    #[test]
    fn stemmingscodes_1_3_4_hebben_elk_hun_eigen_betekenis() {
        let xml = mini_xml().replace(
            "<Pitch_OriginalOrgan_SpecificationMethodCode>2</Pitch_OriginalOrgan_SpecificationMethodCode>",
            "<Pitch_Tempered_BaseTuningSchemeCode>3</Pitch_Tempered_BaseTuningSchemeCode>\
             <Pitch_Tempered_BaseTuningDeviation>13</Pitch_Tempered_BaseTuningDeviation>\
             <Pitch_OriginalOrgan_SpecificationMethodCode>2</Pitch_OriginalOrgan_SpecificationMethodCode>",
        ).replace(
            "<PipeID>12</PipeID><RankID>1</RankID><NormalMIDINoteNumber>37</NormalMIDINoteNumber>",
            "<PipeID>12</PipeID><RankID>1</RankID><NormalMIDINoteNumber>37</NormalMIDINoteNumber>\
             <Pitch_Tempered_BaseTuningSchemeCode>1</Pitch_Tempered_BaseTuningSchemeCode>",
        );
        let mut pkg_dirs = HashMap::new();
        pkg_dirs.insert(1u32, PathBuf::from("/pkg"));
        let def = build_definition(&xml, PathBuf::from("/root"), &pkg_dirs).expect("build");
        let bourdon = def.stops.iter().find(|s| s.name == "Bourdon 8").unwrap();
        let PipeDef::Sample { extra: e3, .. } = &bourdon.pipes[0] else { panic!("sample") };
        assert_eq!(e3.retune_keep_pct, Some(0.0));
        assert_eq!(e3.pitch_correction_cents, Some(13.0));
        // De vervangen-opname-gegevens blijven los daarvan bestaan.
        assert!(e3.original_pitch_cents.is_some() && e3.sample_pitch_cents.is_some());
        let PipeDef::Sample { extra: e1, .. } = &bourdon.pipes[1] else { panic!("sample") };
        assert_eq!(e1.retune_keep_pct, Some(0.0));
        assert_eq!(e1.pitch_correction_cents, None);
        let montre = def.stops.iter().find(|s| s.name == "Montre 8").unwrap();
        let PipeDef::Sample { extra: e4, .. } = &montre.pipes[0] else { panic!("sample") };
        assert_eq!(e4.retune_keep_pct, Some(100.0));
        assert_eq!(e4.pitch_correction_cents, None);
    }

    /// Een maker die het eigen StopID vóór de naam zet ("1_Bourdon 8"): dat
    /// voorvoegsel gaat eraf; een ander getal blijft staan.
    #[test]
    fn eigen_stopid_voor_de_naam_verdwijnt() {
        // 0.9.20: elk 1-4-cijferig voorvoegsel met underscore gaat eraf,
        // ook als het niet overeenkomt met het StopID. Bij Augustine's sets
        // komen de prefixen uit een naburig register: "2119_TrompetDisc"
        // bij StopID 2121. De test checkt beide varianten.
        let xml = mini_xml()
            .replace("<StopID>1</StopID><Name>Bourdon 8</Name>", "<StopID>1</StopID><Name>1_Bourdon 8</Name>")
            .replace("<StopID>2</StopID><Name>Montre 8</Name>", "<StopID>2</StopID><Name>7_Montre 8</Name>");
        let mut pkg_dirs = HashMap::new();
        pkg_dirs.insert(1u32, PathBuf::from("/pkg"));
        let def = build_definition(&xml, PathBuf::from("/root"), &pkg_dirs).expect("build");
        let namen: Vec<_> = def.stops.iter().map(|s| s.name.clone()).collect();
        assert!(namen.iter().any(|s| s == "Bourdon 8"), "{:?}", namen);
        assert!(namen.iter().any(|s| s == "Montre 8"), "{:?}", namen);
    }

    /// Schakelnetwerk-terugval (0.8.3): een definitie zonder StopRank-objecten
    /// waarin toets-schakelaar + register-schakelaar via een routing-
    /// schakelaar de pallet van elke pijp bedienen (Nowa Ruda). Verwacht één
    /// gesynthetiseerde StopRank per (register, rank); ruisranks blijven
    /// buiten; een 4'-register op een 8'-rang krijgt verschuiving +12.
    #[test]
    fn stopranks_uit_schakelnetwerk() {
        let fm = |kv: &[(&str, &str)]| -> FieldMap {
            kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let stops = vec![
            fm(&[("StopID", "1"), ("Name", "Gambe 16"), ("DivisionID", "1"), ("ControllingSwitchID", "23001")]),
            fm(&[("StopID", "2"), ("Name", "Octave 4"), ("DivisionID", "1"), ("ControllingSwitchID", "23002")]),
        ];
        let division_inputs = vec![
            fm(&[("DivisionID", "1"), ("SwitchID", "9001"), ("NormalMIDINoteNumber", "36")]),
            fm(&[("DivisionID", "1"), ("SwitchID", "9002"), ("NormalMIDINoteNumber", "37")]),
        ];
        let ranks = vec![
            fm(&[("RankID", "4"), ("Name", "001. P  Gambe 16 (close)")]),
            fm(&[("RankID", "81"), ("Name", "019. Stop action noises Attack (close)")]),
        ];
        // Gambe: pijpen 36/37 op pallets 5001/5002 via routing 7001/7002.
        // Octave 4: toets 36 stuurt pijp 48 van dezelfde rang (verschuiving 12).
        // Ruisrank 81: pijp op de registerschakelaar zelf (registerklik).
        let pipes = vec![
            fm(&[("PipeID", "401"), ("RankID", "4"), ("NormalMIDINoteNumber", "36"), ("ControllingPalletSwitchID", "5001")]),
            fm(&[("PipeID", "402"), ("RankID", "4"), ("NormalMIDINoteNumber", "37"), ("ControllingPalletSwitchID", "5002")]),
            fm(&[("PipeID", "412"), ("RankID", "4"), ("NormalMIDINoteNumber", "48"), ("ControllingPalletSwitchID", "5012")]),
            fm(&[("PipeID", "8101"), ("RankID", "81"), ("NormalMIDINoteNumber", "1"), ("ControllingPalletSwitchID", "23001")]),
        ];
        let linkages = vec![
            fm(&[("SourceSwitchID", "9001"), ("DestSwitchID", "7001"), ("ConditionSwitchID", "23001")]),
            fm(&[("SourceSwitchID", "7001"), ("DestSwitchID", "5001"), ("ConditionSwitchID", "120")]),
            fm(&[("SourceSwitchID", "9002"), ("DestSwitchID", "7002"), ("ConditionSwitchID", "23001")]),
            fm(&[("SourceSwitchID", "7002"), ("DestSwitchID", "5002"), ("ConditionSwitchID", "120")]),
            fm(&[("SourceSwitchID", "9001"), ("DestSwitchID", "7012"), ("ConditionSwitchID", "23002")]),
            fm(&[("SourceSwitchID", "7012"), ("DestSwitchID", "5012")]),
        ];
        let srs = synth_stopranks_via_switches(&stops, &pipes, &linkages, &division_inputs, &ranks);
        assert_eq!(srs.len(), 2, "{srs:?}");
        let g = &srs[0];
        assert_eq!(g.get_u32("StopID"), Some(1));
        assert_eq!(g.get_u32("RankID"), Some(4));
        assert_eq!(g.get("Name").map(|s| s.as_str()), Some("001. P  Gambe 16 (close)"));
        assert_eq!(g.get_u32("MIDINoteNumOfFirstMappedDivisionInputNode"), Some(36));
        assert_eq!(g.get_u32("NumberOfMappedDivisionInputNodes"), Some(2));
        assert_eq!(g.get_i32("MIDINoteNumIncrementFromDivisionToRank"), Some(0));
        let o = &srs[1];
        assert_eq!(o.get_u32("StopID"), Some(2));
        assert_eq!(o.get_u32("MIDINoteNumOfFirstMappedDivisionInputNode"), Some(36));
        assert_eq!(o.get_u32("NumberOfMappedDivisionInputNodes"), Some(1));
        assert_eq!(o.get_i32("MIDINoteNumIncrementFromDivisionToRank"), Some(12));
    }

    /// Tremulantruis per divisie (0.8.5): de divisie volgt uit de naam van
    /// de tremulant, met het enige werk met trem-opnamen als terugval.
    #[test]
    fn tremulant_divisie_uit_naam() {
        let divs = vec!["Pedal".to_string(), "Manual I".to_string(), "Manual II".to_string(), "Récit expressif".to_string()];
        assert_eq!(match_divisie("1. Tremulant II", &divs).as_deref(), Some("Manual II"));
        assert_eq!(match_divisie("2. Tremulant II Echo", &divs).as_deref(), Some("Manual II"));
        assert_eq!(match_divisie("Trémolo Récit expressif", &divs).as_deref(), Some("Récit expressif"));
        assert_eq!(match_divisie("Tremolo Recit Noise", &divs), None, "alleen een heel werk of een cijfer telt");
        assert_eq!(match_divisie("Tremulant Noise", &divs), None);
        assert_eq!(nummer_in_naam("Tremulant 3 Noise"), Some(3));
        assert_eq!(nummer_in_naam("Tremulant Noise"), None);

        let fm = |kv: &[(&str, &str)]| -> FieldMap {
            kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let tremulants = vec![
            fm(&[("TremulantID", "1"), ("Name", "1. Tremulant II"), ("ControllingSwitchID", "701")]),
            fm(&[("TremulantID", "3"), ("Name", "3. Tremulant III"), ("ControllingSwitchID", "703")]),
        ];
        let divisions = vec![
            fm(&[("DivisionID", "1"), ("Name", "1. Pedal")]),
            fm(&[("DivisionID", "3"), ("Name", "3. Manual II")]),
            fm(&[("DivisionID", "4"), ("Name", "4. Manual III")]),
        ];
        let k = tremulant_divisies(&tremulants, &divisions, &[], &[]);
        assert_eq!(k.per_switch.get(&701).map(|s| s.as_str()), Some("Manual II"));
        assert_eq!(k.per_nummer.get(&3).map(|s| s.as_str()), Some("Manual III"));
        assert_eq!(k.enige, None);
    }

    /// Manuaalnummers volgen de keyboards, niet de lijstvolgorde van de divisies
    /// (0.9.29). Reproductie van de Stralsund-demo: divisies staan door elkaar,
    /// de keyboards dragen "n. manual", en keyboard-ID == divisie-ID.
    #[test]
    fn manuaalnummers_volgen_de_keyboards_niet_de_divisielijst() {
        let fm = |kv: &[(&str, &str)]| -> FieldMap {
            kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let divisions = vec![
            fm(&[("DivisionID", "4"), ("Name", "Recit")]),
            fm(&[("DivisionID", "1"), ("Name", "Pedale")]),
            fm(&[("DivisionID", "3"), ("Name", "Positif")]),
            fm(&[("DivisionID", "2"), ("Name", "GO")]),
        ];
        let keyboards = vec![
            fm(&[("Sys_ObjectID", "4"), ("Name", "3. manual"), ("Hint_PrimaryAssociatedDivisionID", "4")]),
            fm(&[("Sys_ObjectID", "2"), ("Name", "1. manual [lowest]"), ("Hint_PrimaryAssociatedDivisionID", "2")]),
            fm(&[("Sys_ObjectID", "1"), ("Name", "Pedal"), ("Hint_PrimaryAssociatedDivisionID", "1")]),
            fm(&[("Sys_ObjectID", "3"), ("Name", "2. manual"), ("Hint_PrimaryAssociatedDivisionID", "3")]),
        ];
        let m = manual_numbers_from_keyboards(&keyboards, &divisions).expect("afleiding uit keyboards");
        assert_eq!(m.get(&2), Some(&1), "divisie 2 (1. manual) hoort manuaal I");
        assert_eq!(m.get(&3), Some(&2), "divisie 3 (2. manual) hoort manuaal II");
        assert_eq!(m.get(&4), Some(&3), "divisie 4 (3. manual) hoort manuaal III");
        assert!(!m.contains_key(&1), "pedaal krijgt geen manuaalnummer");
    }

    /// Zonder bruikbare keyboards (geen namen, geen koppeling) valt de
    /// nummering terug op de oude volgorde-gebaseerde telling.
    #[test]
    fn manuaalnummering_valt_terug_zonder_keyboards() {
        let fm = |kv: &[(&str, &str)]| -> FieldMap {
            kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let divisions = vec![
            fm(&[("DivisionID", "2"), ("Name", "GO")]),
            fm(&[("DivisionID", "3"), ("Name", "Positif")]),
        ];
        assert!(manual_numbers_from_keyboards(&[], &divisions).is_none(), "zonder keyboards geen afleiding");
    }

    /// Koppeling via gelijke ID's wanneer Hint_PrimaryAssociatedDivisionID
    /// ontbreekt, en nummering uit de ID-volgorde als de namen geen nummer dragen.
    #[test]
    fn manuaalnummers_via_gelijke_ids_en_idvolgorde() {
        let fm = |kv: &[(&str, &str)]| -> FieldMap {
            kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let divisions = vec![
            fm(&[("DivisionID", "2"), ("Name", "Hauptwerk")]),
            fm(&[("DivisionID", "3"), ("Name", "Schwellwerk")]),
            fm(&[("DivisionID", "1"), ("Name", "Pedal")]),
        ];
        let keyboards = vec![
            fm(&[("Sys_ObjectID", "3"), ("Name", "Great")]),
            fm(&[("Sys_ObjectID", "2"), ("Name", "Swell")]),
            fm(&[("Sys_ObjectID", "1"), ("Name", "Pedal")]),
        ];
        let m = manual_numbers_from_keyboards(&keyboards, &divisions).expect("afleiding via ID's");
        assert_eq!(m.get(&2), Some(&1), "laagste klavier-ID = manuaal I");
        assert_eq!(m.get(&3), Some(&2));
        assert!(!m.contains_key(&1));
    }

    /// Tremmed rang in een schakelnetwerk-set (0.8.4): dezelfde toets en
    /// hetzelfde register sturen de droge pallet via "NonTrem" en de tremmed
    /// pallet via "Trem". De tremmed rang mag geen eigen laag worden maar
    /// hoort als AlternateRankID aan de droge rang.
    #[test]
    fn tremmed_rang_wordt_tremulantlaag_in_schakelnetwerk() {
        let fm = |kv: &[(&str, &str)]| -> FieldMap {
            kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let stops = vec![
            fm(&[("StopID", "42"), ("Name", "Konzertflöte 8"), ("DivisionID", "3"), ("ControllingSwitchID", "22042")]),
        ];
        let division_inputs = vec![
            fm(&[("DivisionID", "3"), ("SwitchID", "9225"), ("NormalMIDINoteNumber", "60")]),
        ];
        let ranks = vec![
            fm(&[("RankID", "42"), ("Name", "038. II  Konzertflöte 8 (close)")]),
            fm(&[("RankID", "542"), ("Name", "053. II  Konzertflöte 8 (close, tremmed)")]),
            fm(&[("RankID", "1042"), ("Name", "153. II  Konzertflöte 8 (front)")]),
        ];
        let pipes = vec![
            fm(&[("PipeID", "4225"), ("RankID", "42"), ("NormalMIDINoteNumber", "60"), ("ControllingPalletSwitchID", "569920")]),
            fm(&[("PipeID", "54225"), ("RankID", "542"), ("NormalMIDINoteNumber", "60"), ("ControllingPalletSwitchID", "569921")]),
            fm(&[("PipeID", "104225"), ("RankID", "1042"), ("NormalMIDINoteNumber", "60"), ("ControllingPalletSwitchID", "569930")]),
        ];
        let linkages = vec![
            fm(&[("SourceSwitchID", "9225"), ("DestSwitchID", "569919"), ("ConditionSwitchID", "22042")]),
            fm(&[("SourceSwitchID", "569919"), ("DestSwitchID", "569920"), ("ConditionSwitchID", "711")]),
            fm(&[("SourceSwitchID", "569919"), ("DestSwitchID", "569921"), ("ConditionSwitchID", "721")]),
            fm(&[("SourceSwitchID", "569919"), ("DestSwitchID", "569930")]),
        ];
        let srs = synth_stopranks_via_switches(&stops, &pipes, &linkages, &division_inputs, &ranks);
        // Twee lagen (close, front); de tremmed rang is geen eigen laag meer.
        assert_eq!(srs.len(), 2, "{srs:?}");
        let close = srs.iter().find(|m| m.get_u32("RankID") == Some(42)).expect("close-rang");
        assert_eq!(close.get_u32("AlternateRankID"), Some(542));
        let front = srs.iter().find(|m| m.get_u32("RankID") == Some(1042)).expect("front-rang");
        assert_eq!(front.get_u32("AlternateRankID"), None);
        assert!(srs.iter().all(|m| m.get_u32("RankID") != Some(542)));
    }

    #[test]
    fn test_perspectieven_front_rear() {
        let def = mini_definition();
        let bourdon = def.stops.iter().find(|s| s.name == "Bourdon 8").unwrap();
        let montre = def.stops.iter().find(|s| s.name == "Montre 8").unwrap();

        // Tweede StopRank "(rear)" over dezelfde noten → laag 1 met perspectief
        // 'rear'; laag 0 blijft de front-opname en krijgt 'front' als primair.
        assert_eq!(bourdon.layers.len(), 1);
        assert_eq!(bourdon.layers[0].index, 1);
        assert_eq!(bourdon.layers[0].perspective.as_deref(), Some("rear"));
        assert_eq!(bourdon.layers[0].pipes.len(), bourdon.pipes.len());
        let PipeDef::Sample { path, .. } = &bourdon.layers[0].pipes[0] else { panic!("rear-pijp 36 moet Sample zijn") };
        assert!(path.ends_with("Bourdon_rear/A0/036-c.wav"), "{path:?}");
        let PipeDef::Sample { path: p37, .. } = &bourdon.layers[0].pipes[1] else { panic!("rear-pijp 37 moet Sample zijn") };
        assert!(p37.ends_with("Bourdon_rear/A0/037-c#.wav"), "{p37:?}");
        assert_eq!(bourdon.primary_perspective.as_deref(), Some("front"));
        let PipeDef::Sample { path: front, .. } = &bourdon.pipes[0] else { panic!("front-pijp 36 moet Sample zijn") };
        assert!(front.ends_with("Bourdon/A0/036-c.wav"), "laag 0 blijft front: {front:?}");
        // Stopnaam blijft schoon (geen "(front)").
        assert_eq!(bourdon.name, "Bourdon 8");

        // Montre: één StopRank → geen lagen, geen perspectief.
        assert!(montre.layers.is_empty());
        assert_eq!(montre.primary_perspective, None);
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
                    n_trem += extra.tremulant_attack.is_some() as usize;
                }
            }
            println!("  [{}] {} — {} pipes, first_midi={}, harmonic={}, releases={}, trem={}, primair={:?}, lagen={:?}",
                s.windchest_group, s.name, n_real, first, s.harmonic_number, n_rel, n_trem,
                s.primary_perspective,
                s.layers.iter().map(|l| (l.index, l.perspective.clone(),
                    l.pipes.iter().filter(|p| matches!(p, PipeDef::Sample { .. })).count())).collect::<Vec<_>>());
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
                    if let Some(t) = &extra.tremulant_attack {
                        if t.path.exists() { sample_ok += 1; } else {
                            sample_missing += 1;
                            if sample_missing <= 3 { println!("    MISSING trem: {:?}", t.path); }
                        }
                    }
                    for r in &extra.tremulant_releases {
                        if r.path.exists() { sample_ok += 1; } else {
                            sample_missing += 1;
                            if sample_missing <= 3 { println!("    MISSING trem-rel: {:?}", r.path); }
                        }
                    }
                }
            }
        }
        // 0.9.24: tremulant-opnamen die hun EIGEN ODF-samplepitch dragen (niet
        // meer de smpl-chunk van de WAV of de toon van de droge laag).
        let mut trem_eigen = 0usize;
        let mut trem_voorbeelden = 0usize;
        for s in &def.stops {
            for p in &s.pipes {
                if let PipeDef::Sample { extra, .. } = p {
                    if let Some(t) = &extra.tremulant_attack {
                        if let Some(tc) = t.sample_pitch_cents {
                            trem_eigen += 1;
                            if trem_voorbeelden < 8 {
                                trem_voorbeelden += 1;
                                println!("    TREM-pitch: {} droog={:?} trem={:.2} ct  {:?}",
                                    s.name, extra.sample_pitch_cents, tc, t.path.file_name());
                            }
                        }
                    }
                }
            }
        }
        println!("Tremulant-opnamen met eigen ODF-samplepitch: {}", trem_eigen);
        println!("Samples on disk: ok={} missing={}", sample_ok, sample_missing);
        assert!(sample_missing == 0, "all resolved samples should exist on disk");
        assert!(!def.stops.is_empty());
        // Saint-Jean-de-Luz (choeur): 16 stops, elk front (primair) + rear + dry.
        if path.contains("SaintJeanDeLuz") {
            assert_eq!(def.stops.len(), 16);
            for s in &def.stops {
                assert_eq!(s.primary_perspective.as_deref(), Some("front"), "{}", s.name);
                let persp: Vec<&str> = s.layers.iter().filter_map(|l| l.perspective.as_deref()).collect();
                assert_eq!(persp, vec!["rear", "dry"], "{}", s.name);
                for l in &s.layers {
                    assert!(l.pipes.len() <= s.pipes.len());
                    assert!(l.pipes.iter().all(|p| !matches!(p, PipeDef::Sample { path, .. } if !path.exists())));
                }
            }
        }
    }
}
