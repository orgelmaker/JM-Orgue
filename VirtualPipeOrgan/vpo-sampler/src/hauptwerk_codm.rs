//! Hauptwerk Custom Organ Design Module (CODM): `FileFormat="CustomOrgan"`.
//!
//! Een CODM-definitie beschrijft een orgel niet pijp voor pijp zoals de
//! volledige Hauptwerk-ODF, maar met regels: divisies, stops, rangen en de
//! koppeling daartussen (StopRank), met codes die de rol vastleggen.
//! Hauptwerk "compileert" dat bij het laden; dit module doet hetzelfde en
//! levert de gewone [`OrganDefinition`] af, zodat laden, UI en engine
//! ongewijzigd blijven.
//!
//! ```text
//! Division (code 1 = pedaal, 2..7 = manuaal 1..6)
//! Stop (StopCode 2DNN: D = divisie-index 0..6, NN = volgnummer)
//!   └─ StopRank: toetsbereik + noot-sprong → Rank
//!        Rank: NumberOfPipes vanaf MIDINoteNumberOfFirstPipe,
//!              samples in <pakket>/<map>/036-C.wav, 037-C#.wav, …
//! Coupler (CouplerCode 1000 + 100·doel + 5·bron + soort)
//! ```
//!
//! Regels die hier vastliggen (CODM-gids, Hauptwerk 4.2):
//! - Een rang maakt één virtuele pijp per noot, ook als pijpen dezelfde
//!   opname delen (een chimes-rang van 37 pijpen uit één sample).
//! - De opname van een pijp is `<noot + Samples_MIDINoteNumIncrementFromPipesToSamples>`
//!   in de samplemap; vanaf Hauptwerk 4 worden de beschikbare bestanden
//!   gescand. Ontbreekt de exacte noot, dan de dichtstbijzijnde opname
//!   (die op toonhoogte wordt gebracht).
//! - De toonhoogte van een opname staat in de smpl-chunk; CODM-orgels kennen
//!   geen "originele orgeltoonhoogte" en spelen altijd op de doeltoon.
//! - Releases: in het aanslagbestand (marker) of in een eigen map met
//!   dezelfde bestandsnamen; daarnaast optioneel korte en middellange
//!   releases met hun maximale toetsduur.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use tracing::{info, warn};

use crate::grandorgue::{
    CouplerDef, EnclosureDef, ManualDef, OdfError, OrganDefinition, OrganInfo, PipeDef,
    PipeExtra, PipeLayer, ReleaseDef, SchakelaarRegels, StopDef,
    TremulantDef, WindchestDef,
};
use crate::import_report::{Ernst, ImportReport, Melding, OvergeslagenStop, PakketRol, PakketStatus, Tellingen};
use crate::hauptwerk::{
    clean_division_name, clean_stop_name, extract_objects, klinkende_harmonische, FieldMap,
    FieldMapExt,
};

/// Wat de starttag van het rootelement over het bestand zegt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodmKop {
    pub formaat: String,
    pub versie: String,
}

/// Lees `FileFormat`/`FileFormatVersion` uit het rootelement `<Hauptwerk …>`.
///
/// Slaat BOM, witruimte, de XML-declaratie, commentaar en een doctype over en
/// leest attributen met enkele of dubbele quotes, in willekeurige volgorde en
/// met extra attributen ertussen. Geeft `None` als het rootelement geen
/// `Hauptwerk` is.
pub fn lees_kop(xml: &str) -> Option<CodmKop> {
    let mut s = xml.trim_start_matches('\u{feff}');
    loop {
        s = s.trim_start();
        if s.starts_with("<?") {
            s = &s[s.find("?>")? + 2..];
        } else if s.starts_with("<!--") {
            s = &s[s.find("-->")? + 3..];
        } else if s.starts_with("<!") {
            s = &s[s.find('>')? + 1..];
        } else {
            break;
        }
    }
    let s = s.strip_prefix('<')?;
    let eind = s.find('>')?;
    let tag = &s[..eind];
    let naam_eind = tag.find(|c: char| c.is_whitespace()).unwrap_or(tag.len());
    if !tag[..naam_eind].trim_end_matches('/').eq_ignore_ascii_case("Hauptwerk") {
        return None;
    }
    let mut kop = CodmKop { formaat: String::new(), versie: String::new() };
    let mut rest = &tag[naam_eind..];
    while let Some(eq) = rest.find('=') {
        let sleutel = rest[..eq].trim();
        let na = rest[eq + 1..].trim_start();
        let Some(q) = na.chars().next().filter(|c| *c == '"' || *c == '\'') else { break };
        let Some(sluit) = na[1..].find(q) else { break };
        let waarde = &na[1..1 + sluit];
        if sleutel.eq_ignore_ascii_case("FileFormat") {
            kop.formaat = waarde.trim().to_string();
        } else if sleutel.eq_ignore_ascii_case("FileFormatVersion") {
            kop.versie = waarde.trim().to_string();
        }
        rest = &na[1 + sluit + 1..];
    }
    Some(kop)
}

/// Is dit een CODM-bestand (`FileFormat="CustomOrgan"` op de root)?
pub fn is_codm(xml: &str) -> bool {
    lees_kop(xml).is_some_and(|k| k.formaat.eq_ignore_ascii_case("CustomOrgan"))
}

/// Versies waarvan de veldsemantiek aan echte bestanden getoetst is.
pub const GETOETSTE_VERSIES: &[&str] = &["3.00"];

// ============================================================================
// Structuur (zonder samples)
// ============================================================================

#[derive(Debug, Clone)]
pub struct CodmDivisie {
    /// DivisionCode 1..16 (1 = pedaal, 2..7 = manuaal 1..6, 8..16 niet-standaard).
    pub code: u32,
    pub naam: String,
    pub korte_naam: String,
    pub aantal_toetsen: u32,
    pub eerste_toets: u32,
}

#[derive(Debug, Clone)]
pub struct CodmStop {
    pub code: u32,
    pub naam: String,
    /// `StoreInGeneralCombinations` (afwezig = wel opnemen).
    pub in_combinatie: bool,
    /// `StoreInDivisionalCombinations` (JM kent geen aparte divisional-combinaties).
    pub in_divisional: bool,
    /// `DefaultToEngaged`.
    pub standaard_aan: bool,
    /// DivisionCode waaruit de stop speelt (uit de StopCode of de override).
    pub divisie: u32,
}

#[derive(Debug, Clone)]
pub struct CodmStopRank {
    pub stop: u32,
    pub rank: Option<u32>,
    /// RankTypeCode 2 = externe rang (niet ondersteund).
    pub extern_: bool,
    pub sprong: i32,
    pub eerste_toets: u32,
    pub aantal_toetsen: u32,
    pub actie_type: u32,
    pub actie_effect: u32,
    pub wisselrang: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CodmRank {
    pub id: u32,
    pub naam: String,
    pub aantal_pijpen: u32,
    pub eerste_pijp: u32,
    pub percussief: bool,
    pub pakket: u32,
    pub map: String,
    /// Aparte map met de primaire releases (leeg = marker in het aanslagbestand).
    pub release_map: String,
    pub korte_release_map: String,
    pub korte_release_ms: Option<i32>,
    pub middel_release_map: String,
    pub middel_release_ms: Option<i32>,
    pub sample_sprong: i32,
    /// Harmonische t.o.v. 64' (8 = 8').
    pub harmonische: u32,
    /// `Samples_RankBasePitch64ftHarmNumIfAssumedTunedToConcertPitch`: de
    /// harmonische waarop de opnamen als gestemd worden aangenomen (leeg = die
    /// van de uitvoer).
    pub sample_harmonische: Option<u32>,
    /// `Samples_AssumeTunedToConcertPitchAndEqualTemp`: de opnamen staan op de
    /// toon van hun bestandsnoot (anders beslist de metadata in de WAV).
    pub samples_gestemd: bool,
    pub niveau_db_36: f32,
    pub niveau_db_96: f32,
    pub stemming_ct_36: f32,
    pub stemming_ct_96: f32,
    /// HarmonicShaping: aanpassing (dB) van de 3e en hogere boventonen, bij
    /// pijpnoot 36 en 96, lineair geprojecteerd over het klavier.
    pub shaping_db_36: f32,
    pub shaping_db_96: f32,
    pub zwelkast: Option<u32>,
    pub tremulant: Option<u32>,
    /// `ReverbTailTruncation_ModeCode`: 1 = geen, 2 = halve sinus vanaf het begin
    /// van de release, 3 = vanaf de eerste marker in de release.
    /// `Pitch_MaxRandomTuningErrorHz`: grootste willekeurige stemafwijking (Hz) per pijp.
    pub willekeurig_hz: f32,
    /// `WindSupply_MaxFlowRandomisationPct`: willekeurige variatie van de windstroom.
    pub wind_flow_pct: f32,
    /// Alleen een deel van de rang zit in de zwelkast / heeft de tremulant
    /// (`Encl_…FirstEnclosedPipe…` / `Trem_…FirstTremmedPipe…` ingevuld en niet 0).
    pub zwelkast_deel: bool,
    pub trem_deel: bool,
    pub truncatie_modus: u32,
    /// Gevraagde uitsterflengte (ms) voor een middelste c op een 8'-register.
    pub truncatie_ms: f32,
    /// Pakket en map met de tremulant-golfvormen van de rang (leeg = geen).
    pub trem_pakket: u32,
    pub trem_map: String,
}

/// Soort koppel volgens de laatste "cijferpositie" van de CouplerCode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KoppelSoort {
    Zestien,
    /// 8' tussen twee divisies.
    Unisono,
    /// Unison Off: dezelfde divisie, schakelt de eigen 8'-route uit.
    UnisonOff,
    Vier,
    Bas,
    Melodie,
}

#[derive(Debug, Clone)]
pub struct CodmKoppel {
    pub code: u32,
    pub naam: String,
    pub in_combinatie: bool,
    pub in_divisional: bool,
    pub standaard_aan: bool,
    /// Divisie waarvan je de toetsen indrukt ("Swell to Great": Great).
    pub van_toetsen: u32,
    /// Divisie die meeklinkt ("Swell to Great": Swell).
    pub klinkt: u32,
    pub soort: KoppelSoort,
    pub sprong: i32,
}

#[derive(Debug, Clone)]
pub struct CodmTremulant {
    pub code: u32,
    pub naam: String,
    pub standaard_aan: bool,
    pub frequentie_hz: f32,
    /// Divisie uit de code (17D0); `None` = algemene tremulant (1690).
    pub divisie: Option<u32>,
}

/// Een `ShortcutPiston`: `ReferencedObjectTypeCode` 1 = stop, 2 = koppel, 3 = tremulant.
#[derive(Debug, Clone)]
pub struct CodmPiston {
    pub naam: String,
    pub label: String,
    pub doel_soort: u32,
    pub doel_code: u32,
    /// `PistonActionTypeCode`: 1 = omschakelen, 2 = vasthouden.
    pub actie: u32,
}

#[derive(Debug, Clone)]
pub struct CodmOrgel {
    pub naam: String,
    pub divisies: Vec<CodmDivisie>,
    pub stops: Vec<CodmStop>,
    pub stopranks: Vec<CodmStopRank>,
    pub ranks: BTreeMap<u32, CodmRank>,
    pub koppels: Vec<CodmKoppel>,
    pub zwelkasten: Vec<(u32, String)>,
    pub tremulanten: Vec<CodmTremulant>,
    /// RequiredInstallationPackage: id → naam.
    pub pakketten: BTreeMap<u32, String>,
    pub pistons: Vec<CodmPiston>,
    /// `FileFormatVersion` uit het rootelement ("" = ontbreekt).
    pub versie: String,
    /// Rol(len) per pakket-ID, afgeleid uit de velden die ernaar verwijzen.
    pub pakket_rollen: BTreeMap<u32, std::collections::BTreeSet<PakketRol>>,
    /// Bevindingen van de validatie (dubbele sleutels, ontbrekende verwijzingen, …).
    pub meldingen: Vec<Melding>,
}

impl CodmOrgel {
    /// Pakketten waarin samples van de rangen staan.
    pub fn sample_pakketten(&self) -> Vec<u32> {
        let set: std::collections::BTreeSet<u32> = self.ranks.values().map(|r| r.pakket).filter(|p| *p > 0).collect();
        set.into_iter().collect()
    }
}

/// Stopcode 2DNN → DivisionCode (D + 1).
fn divisie_van_stopcode(code: u32) -> Option<u32> {
    (2001..=2699).contains(&code).then(|| (code - 2000) / 100 + 1)
}

/// CouplerCode → (toetsen-divisie, klinkende divisie, soort, sprong).
/// Code = 1000 + 100·doel + 5·bron + soort; doel/bron 0..6 = pedaal, manuaal 1..6.
fn ontleed_koppelcode(code: u32) -> Option<(u32, u32, KoppelSoort, i32)> {
    if !(1000..=1634).contains(&code) {
        return None;
    }
    let r = code - 1000;
    let doel = r / 100;
    let rest = r % 100;
    let bron = rest / 5;
    let soort = rest % 5;
    if doel > 6 || bron > 6 {
        return None;
    }
    let (s, sprong) = match soort {
        0 => (KoppelSoort::Zestien, -12),
        1 if doel == bron => (KoppelSoort::UnisonOff, 0),
        1 => (KoppelSoort::Unisono, 0),
        2 => (KoppelSoort::Vier, 12),
        3 => (KoppelSoort::Bas, 0),
        _ => (KoppelSoort::Melodie, 0),
    };
    Some((doel + 1, bron + 1, s, sprong))
}

fn tekst(m: &FieldMap, k: &str) -> String {
    m.get(k).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Y/N-veld met een standaardwaarde voor "afwezig of leeg".
fn ja_of(m: &FieldMap, k: &str, standaard: bool) -> bool {
    match m.get(k).map(|s| s.trim().to_ascii_uppercase()) {
        Some(v) if v == "Y" => true,
        Some(v) if v == "N" => false,
        _ => standaard,
    }
}

fn ja(m: &FieldMap, k: &str) -> bool {
    m.get(k).is_some_and(|s| s.trim().eq_ignore_ascii_case("Y"))
}

/// "Ped: Contra Bourdon 32'" → "Contra Bourdon 32'"; "Coupler: Gt to Ped 8'" →
/// "Gt to Ped 8'". Alleen een kort voorvoegsel zonder spaties telt.
fn zonder_voorvoegsel(naam: &str) -> &str {
    match naam.split_once(": ") {
        Some((kop, rest)) if !kop.is_empty() && kop.len() <= 12 && !rest.trim().is_empty() => rest.trim(),
        _ => naam.trim(),
    }
}

/// "PEDAL" → "Pedal"; gemengde schrijfwijze blijft staan.
fn titel_als_kapitalen(s: &str) -> String {
    if s.chars().any(|c| c.is_lowercase()) || !s.chars().any(|c| c.is_alphabetic()) {
        return s.to_string();
    }
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lees de structuur van een CODM-bestand. Raakt geen samples aan.
pub fn parse_codm(xml: &str) -> Result<CodmOrgel, OdfError> {
    let Some(kop) = lees_kop(xml).filter(|k| k.formaat.eq_ignore_ascii_case("CustomOrgan")) else {
        return Err(OdfError::ParseError { line: 0, message: "geen CODM-orgeldefinitie (FileFormat=\"CustomOrgan\")".into() });
    };
    // Een half gelezen objectlijst mag nooit als volledig doorgaan: misvormde
    // XML wordt met regel en kolom geweigerd.
    if let Err(e) = roxmltree::Document::parse_with_options(
        xml.trim_start_matches('\u{feff}'),
        roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() },
    ) {
        let pos = e.pos();
        return Err(OdfError::ParseError {
            line: pos.row as usize,
            message: format!("CODM-definitie is geen geldige XML (regel {}, kolom {}): {}", pos.row, pos.col, e),
        });
    }
    let mut meldingen: Vec<Melding> = Vec::new();
    if !GETOETSTE_VERSIES.contains(&kop.versie.as_str()) {
        meldingen.push(Melding::nieuw(
            Ernst::Waarschuwing,
            "bestand",
            format!(
                "FileFormatVersion {:?} is niet getoetst (alleen {}); velden kunnen anders werken dan verwacht",
                kop.versie, GETOETSTE_VERSIES.join(", ")
            ),
        ));
    }
    let lees = |t: &str| extract_objects(xml, t);
    let general = lees("_General");
    let general = if general.is_empty() { lees("_general") } else { general };
    let naam = general.first().map(|g| tekst(g, "Name")).unwrap_or_default();

    // De lijsten heten in CODM-bestanden met kleine letters ("stop"), de
    // objecten met hoofdletters (<Stop>). Beide schrijfwijzen proberen.
    let lijst = |lijstnaam: &str, object: &str| -> Vec<FieldMap> {
        let v = extract_objects_als(xml, lijstnaam, object);
        if v.is_empty() { lees(object) } else { v }
    };

    let divisies: Vec<CodmDivisie> = lijst("division", "Division").iter().filter_map(|d| {
        let code = d.get_u32("DivisionCode")?;
        Some(CodmDivisie {
            code,
            naam: titel_als_kapitalen(&clean_division_name(&tekst(d, "Name"))),
            korte_naam: tekst(d, "ShortName"),
            aantal_toetsen: d.get_u32("NumberOfKeys").unwrap_or(61),
            eerste_toets: d.get_u32("MIDINoteNumOfFirstKey").unwrap_or(36),
        })
    }).collect();

    let stops: Vec<CodmStop> = lijst("stop", "Stop").iter().filter_map(|s| {
        let code = s.get_u32("StopCode")?;
        let divisie = s.get_u32("OverrideSourceDivisionToSpecifiedDivisionCode")
            .or_else(|| divisie_van_stopcode(code))?;
        Some(CodmStop {
            code,
            naam: tekst(s, "Name"),
            in_combinatie: ja_of(s, "StoreInGeneralCombinations", true),
            in_divisional: ja_of(s, "StoreInDivisionalCombinations", true),
            standaard_aan: ja_of(s, "DefaultToEngaged", false),
            divisie,
        })
    }).collect();

    let stopranks: Vec<CodmStopRank> = lijst("stoprank", "StopRank").iter().filter_map(|r| {
        Some(CodmStopRank {
            stop: r.get_u32("StopCode")?,
            rank: r.get_u32("RankID"),
            extern_: r.get_u32("RankTypeCode") == Some(2),
            sprong: r.get_i32("MIDINoteNumIncrementFromDivisionToRank").unwrap_or(0),
            eerste_toets: r.get_u32("MIDINoteNumOfFirstMappedDivisionKey").unwrap_or(36),
            aantal_toetsen: r.get_u32("NumberOfMappedDivisionKeys").unwrap_or(0),
            actie_type: r.get_u32("ActionTypeCode").unwrap_or(1),
            actie_effect: r.get_u32("ActionEffectCode").unwrap_or(1),
            wisselrang: r.get_u32("AlternateRankID"),
        })
    }).collect();

    let alle_ranks: Vec<(u32, CodmRank)> = lijst("rank", "Rank").iter().filter_map(|r| {
        let id = r.get_u32("RankID")?;
        let f = |k: &str| r.get_f32(k).unwrap_or(0.0);
        Some((id, CodmRank {
            id,
            naam: tekst(r, "Name"),
            aantal_pijpen: r.get_u32("NumberOfPipes").unwrap_or(0),
            eerste_pijp: r.get_u32("MIDINoteNumberOfFirstPipe").unwrap_or(36),
            percussief: ja(r, "IsPercussive"),
            pakket: r.get_u32("Samples_InstallationPackageID").unwrap_or(0),
            map: tekst(r, "Samples_MainSampleFileFolder"),
            release_map: tekst(r, "Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles"),
            korte_release_map: tekst(r, "Samples_ShortNoteReleaseSampleFileFolderIfAny"),
            korte_release_ms: r.get_i32("Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny"),
            middel_release_map: tekst(r, "Samples_MediumNoteReleaseSampleFileFolderIfAny"),
            middel_release_ms: r.get_i32("Samples_MaxNoteTimeMsForMediumNoteReleaseSamplesIfAny"),
            sample_sprong: r.get_i32("Samples_MIDINoteNumIncrementFromPipesToSamples").unwrap_or(0),
            harmonische: r.get_u32("Pitch_RankBaseOutputPitch64ftHarmonicNum").filter(|h| *h > 0).unwrap_or(8),
            sample_harmonische: r.get_u32("Samples_RankBasePitch64ftHarmNumIfAssumedTunedToConcertPitch").filter(|h| *h > 0),
            samples_gestemd: ja(r, "Samples_AssumeTunedToConcertPitchAndEqualTemp"),
            niveau_db_36: f("Amp_PipeMIDINoteNum036_LevelAdjustDecibels"),
            niveau_db_96: f("Amp_PipeMIDINoteNum096_LevelAdjustDecibels"),
            stemming_ct_36: f("Pitch_PipeMIDINoteNum036_TuningAdjustPercentSemitones"),
            stemming_ct_96: f("Pitch_PipeMIDINoteNum096_TuningAdjustPercentSemitones"),
            shaping_db_36: f("HarmonicShaping_PipeMIDINoteNum036_3rdAndUpperHarmLvlAdjDb"),
            shaping_db_96: f("HarmonicShaping_PipeMIDINoteNum096_3rdAndUpperHarmLvlAdjDb"),
            zwelkast: r.get_u32("Encl_EnclosureCode").filter(|c| *c > 0),
            tremulant: r.get_u32("Trem_TremulantCode").filter(|c| *c > 0),
            willekeurig_hz: f("Pitch_MaxRandomTuningErrorHz"),
            wind_flow_pct: f("WindSupply_MaxFlowRandomisationPct"),
            zwelkast_deel: r.get_u32("Encl_MIDINoteNumOfFirstEnclosedPipeIfWholeRankNotEnclosed").unwrap_or(0) > 0
                || r.get_u32("Encl_MIDINoteNumOfLastEnclosedPipeIfWholeRankNotEnclosed").unwrap_or(0) > 0,
            trem_deel: r.get_u32("Trem_MIDINoteNumOfFirstTremmedPipeIfWholeRankNotTremmed").unwrap_or(0) > 0
                || r.get_u32("Trem_MIDINoteNumOfLastTremmedPipeIfWholeRankNotTremmed").unwrap_or(0) > 0,
            truncatie_modus: r.get_u32("ReverbTailTruncation_ModeCode").unwrap_or(1),
            truncatie_ms: f("ReverbTailTruncation_DecayLengthAsMsForMiddleCOn8FtStop"),
            trem_pakket: r.get_u32("Trem_SampleInstallationPackageID").unwrap_or(0),
            trem_map: tekst(r, "Trem_WaveformSampleFileFolder"),
        }))
    }).collect();
    // RankID hoort uniek te zijn; komt er toch een dubbel voor (Pitea: 33),
    // dan wint de eerste.
    let mut ranks: BTreeMap<u32, CodmRank> = BTreeMap::new();
    for (volgnr, (id, r)) in alle_ranks.into_iter().enumerate() {
        if let Some(eerder) = ranks.get(&id) {
            warn!("CODM: RankID {} komt dubbel voor ('{}' en '{}'); de eerste wordt gebruikt", id, eerder.naam, r.naam);
            meldingen.push(Melding::nieuw(
                Ernst::Waarschuwing,
                format!("rank {id}"),
                format!(
                    "RankID {id} komt dubbel voor: '{}' (rang nr. {} in het bestand) wordt genegeerd; '{}' (de eerste) wordt gebruikt",
                    r.naam, volgnr + 1, eerder.naam
                ),
            ));
            continue;
        }
        ranks.insert(id, r);
    }

    let koppels: Vec<CodmKoppel> = lijst("coupler", "Coupler").iter().filter_map(|k| {
        let code = k.get_u32("CouplerCode")?;
        let (mut van, mut klinkt, soort, mut sprong) = ontleed_koppelcode(code)?;
        if let Some(d) = k.get_u32("OverrideDestDivisionToSpecifiedDivisionCode") { van = d; }
        if let Some(d) = k.get_u32("OverrideSourceDivisionToSpecifiedDivisionCode") { klinkt = d; }
        sprong += k.get_i32("CustomCoupler_MIDINoteNumIncrementFromSourceToDest").unwrap_or(0);
        Some(CodmKoppel {
            code,
            in_combinatie: ja_of(k, "StoreInGeneralCombinations", true),
            in_divisional: ja_of(k, "StoreInDivisionalCombinations", true),
            standaard_aan: ja_of(k, "DefaultToEngaged", false),
            naam: zonder_voorvoegsel(&tekst(k, "Name")).to_string(),
            van_toetsen: van,
            klinkt,
            soort,
            sprong,
        })
    }).collect();

    let zwelkasten: Vec<(u32, String)> = lijst("enclosure", "Enclosure").iter().filter_map(|e| {
        Some((e.get_u32("EnclosureCode")?, titel_als_kapitalen(&tekst(e, "Name"))))
    }).collect();

    let tremulanten: Vec<CodmTremulant> = lijst("tremulant", "Tremulant").iter().filter_map(|t| {
        let code = t.get_u32("TremulantCode")?;
        let divisie = (1700..=1769).contains(&code).then(|| (code - 1700) / 10 + 1);
        Some(CodmTremulant {
            code,
            standaard_aan: ja_of(t, "DefaultToEngaged", false),
            naam: zonder_voorvoegsel(&tekst(t, "Name")).to_string(),
            frequentie_hz: t.get_f32("FrequencyHz").filter(|f| *f > 0.0).unwrap_or(5.0),
            divisie,
        })
    }).collect();

    let pakketten: BTreeMap<u32, String> = lijst("requiredinstallationpackage", "RequiredInstallationPackage")
        .iter()
        .filter_map(|p| Some((p.get_u32("InstallationPackageID")?, tekst(p, "Name"))))
        .collect();

    let pistons: Vec<CodmPiston> = lijst("shortcutpiston", "ShortcutPiston").iter().filter_map(|p| {
        let soort = p.get_u32("ReferencedObjectTypeCode")?;
        let code = match soort {
            1 => p.get_u32("StopCode"),
            2 => p.get_u32("CouplerCode"),
            3 => p.get_u32("TremulantCode"),
            _ => None,
        }?;
        Some(CodmPiston {
            naam: tekst(p, "Name"),
            label: tekst(p, "CustomDisplay1_Text"),
            doel_soort: soort,
            doel_code: code,
            actie: p.get_u32("PistonActionTypeCode").unwrap_or(1),
        })
    }).collect();
    let pakket_rollen = pakketrollen(xml, &ranks);
    valideer(&divisies, &stops, &stopranks, &ranks, &koppels, &zwelkasten, &tremulanten, &mut meldingen);

    Ok(CodmOrgel { naam, divisies, stops, stopranks, ranks, koppels, zwelkasten, tremulanten, pakketten, pistons, versie: kop.versie, pakket_rollen, meldingen })
}

/// Alle `<veld>123</veld>`-waarden met deze veldnaam (zonder 0).
fn ids_van_veld(xml: &str, veld: &str) -> std::collections::BTreeSet<u32> {
    let open = format!("<{veld}>");
    let sluit = format!("</{veld}>");
    let mut uit = std::collections::BTreeSet::new();
    let mut pos = 0;
    while let Some(a) = xml[pos..].find(&open).map(|a| pos + a + open.len()) {
        let Some(b) = xml[a..].find(&sluit).map(|b| a + b) else { break };
        if let Ok(id) = xml[a..b].trim().parse::<u32>() {
            if id > 0 { uit.insert(id); }
        }
        pos = b + sluit.len();
    }
    uit
}

/// Rol(len) per pakket-ID: wie verwijst ernaar en waarvoor?
fn pakketrollen(xml: &str, ranks: &BTreeMap<u32, CodmRank>) -> BTreeMap<u32, std::collections::BTreeSet<PakketRol>> {
    let mut m: BTreeMap<u32, std::collections::BTreeSet<PakketRol>> = BTreeMap::new();
    let mut voeg = |id: u32, rol: PakketRol| { if id > 0 { m.entry(id).or_default().insert(rol); } };
    for r in ranks.values() {
        voeg(r.pakket, PakketRol::Samples);
        if !r.trem_map.is_empty() { voeg(r.trem_pakket, PakketRol::Tremulant); }
    }
    for veld in ["ImageFileInstallationPackageID", "BackgroundImageFileInstallationPackageID"] {
        for id in ids_van_veld(xml, veld) { voeg(id, PakketRol::Beeld); }
    }
    for id in ids_van_veld(xml, "CustomOrganInfo_InstallationPackageID") { voeg(id, PakketRol::Info); }
    m
}

/// Controleer sleutels en verwijzingen. Een verwijzing naar iets dat er niet
/// is, wordt gemeld in plaats van stil overgeslagen.
#[allow(clippy::too_many_arguments)]
fn valideer(
    divisies: &[CodmDivisie],
    stops: &[CodmStop],
    stopranks: &[CodmStopRank],
    ranks: &BTreeMap<u32, CodmRank>,
    koppels: &[CodmKoppel],
    zwelkasten: &[(u32, String)],
    tremulanten: &[CodmTremulant],
    meldingen: &mut Vec<Melding>,
) {
    let mut dubbel = |soort: &str, codes: Vec<u32>| {
        let mut gezien = HashSet::new();
        for c in codes {
            if !gezien.insert(c) {
                meldingen.push(Melding::nieuw(
                    Ernst::Waarschuwing,
                    format!("{soort} {c}"),
                    format!("{soort}-code {c} komt meer dan eens voor; alleen het eerste record is eenduidig"),
                ));
            }
        }
    };
    dubbel("divisie", divisies.iter().map(|d| d.code).collect());
    dubbel("stop", stops.iter().map(|s| s.code).collect());
    dubbel("koppel", koppels.iter().map(|k| k.code).collect());
    dubbel("tremulant", tremulanten.iter().map(|t| t.code).collect());

    let stop_codes: HashSet<u32> = stops.iter().map(|s| s.code).collect();
    let div_codes: HashSet<u32> = divisies.iter().map(|d| d.code).collect();
    let encl_codes: HashSet<u32> = zwelkasten.iter().map(|(c, _)| *c).collect();
    let trem_codes: HashSet<u32> = tremulanten.iter().map(|t| t.code).collect();

    for sr in stopranks {
        if !stop_codes.contains(&sr.stop) {
            meldingen.push(Melding::nieuw(
                Ernst::Fout,
                format!("stop {}", sr.stop),
                format!("een StopRank verwijst naar stop {} die niet bestaat", sr.stop),
            ));
        }
        if !sr.extern_ {
            match sr.rank {
                Some(id) if ranks.contains_key(&id) => {}
                Some(id) => meldingen.push(Melding::nieuw(
                    Ernst::Fout,
                    format!("stop {}", sr.stop),
                    format!("StopRank verwijst naar rang {id} die niet bestaat"),
                )),
                None => meldingen.push(Melding::nieuw(
                    Ernst::Fout,
                    format!("stop {}", sr.stop),
                    "StopRank heeft geen RankID",
                )),
            }
        }
    }
    for s in stops {
        if !div_codes.contains(&s.divisie) {
            meldingen.push(Melding::nieuw(
                Ernst::Fout,
                format!("stop {}", s.code),
                format!("stop '{}' hoort bij divisie {} die niet in de definitie staat", s.naam, s.divisie),
            ));
        }
        if !stopranks.iter().any(|sr| sr.stop == s.code) {
            meldingen.push(Melding::nieuw(
                Ernst::Waarschuwing,
                format!("stop {}", s.code),
                format!("stop '{}' heeft geen StopRank en kan dus niet klinken", s.naam),
            ));
        }
    }
    for r in ranks.values() {
        if let Some(e) = r.zwelkast.filter(|e| !encl_codes.contains(e)) {
            meldingen.push(Melding::nieuw(
                Ernst::Waarschuwing,
                format!("rank {}", r.id),
                format!("rang '{}' verwijst naar zwelkast {e} die niet bestaat", r.naam),
            ));
        }
        if let Some(t) = r.tremulant.filter(|t| !trem_codes.contains(t)) {
            meldingen.push(Melding::nieuw(
                Ernst::Waarschuwing,
                format!("rank {}", r.id),
                format!("rang '{}' verwijst naar tremulant {t} die niet bestaat", r.naam),
            ));
        }
    }
    for k in koppels {
        for d in [k.van_toetsen, k.klinkt] {
            if !div_codes.contains(&d) {
                meldingen.push(Melding::nieuw(
                    Ernst::Waarschuwing,
                    format!("koppel {}", k.code),
                    format!("koppel '{}' verwijst naar divisie {d} die niet bestaat", k.naam),
                ));
            }
        }
    }
}

/// Als [`extract_objects`], maar hoofdletterongevoelig voor de objecttag.
/// CODM-bestanden zijn daarin niet consequent: `<ObjectList ObjectType="stop">`
/// met `<Stop>`, `"division"` met `<division>`, en in één rangenlijst
/// `<Rank>` naast `<rank>`.
fn extract_objects_als(xml: &str, lijstnaam: &str, object: &str) -> Vec<FieldMap> {
    let open = format!("<ObjectList ObjectType=\"{}\">", lijstnaam);
    let Some(i) = xml.find(&open) else { return Vec::new() };
    let start = i + open.len();
    let einde = xml[start..].find("</ObjectList>").map(|j| start + j).unwrap_or(xml.len());
    let body = &xml[start..einde];
    let mut uit = Vec::new();
    let mut pos = 0;
    while let Some(a) = body[pos..].find('<').map(|a| pos + a) {
        let Some(b) = body[a + 1..].find('>').map(|b| a + 1 + b) else { break };
        let tag = &body[a + 1..b];
        if !tag.eq_ignore_ascii_case(object) {
            pos = b + 1;
            continue;
        }
        let sluit = format!("</{tag}>");
        let Some(e) = body[b + 1..].find(&sluit).map(|e| b + 1 + e) else { break };
        // Hergebruik de gewone lezer op één object met een vaste kop.
        let stuk = format!("<ObjectList ObjectType=\"{object}\"><{object}>{}</{object}></ObjectList>", &body[b + 1..e]);
        uit.extend(extract_objects(&stuk, object));
        pos = e + sluit.len();
    }
    uit
}

// ============================================================================
// Samples zoeken
// ============================================================================

/// Inhoud van één samplemap: MIDI-noot → bestand. `.hbw` (beschermd
/// Hauptwerk-formaat) wordt apart geteld: dat kunnen we niet lezen.
#[derive(Debug, Default, Clone)]
struct SampleMap {
    per_noot: BTreeMap<u32, PathBuf>,
    beschermd: usize,
}

impl SampleMap {
    fn lees(dir: &Path) -> Self {
        let mut m = SampleMap::default();
        let Ok(rd) = std::fs::read_dir(dir) else { return m };
        for e in rd.flatten() {
            let pad = e.path();
            let naam = e.file_name().to_string_lossy().to_lowercase();
            let ext = pad.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            let cijfers: String = naam.chars().take_while(|c| c.is_ascii_digit()).collect();
            let Ok(noot) = cijfers.parse::<u32>() else { continue };
            match ext.as_str() {
                "wav" | "wv" => { m.per_noot.entry(noot).or_insert(pad); }
                "hbw" => m.beschermd += 1,
                _ => {}
            }
        }
        m
    }

    /// Het bestand voor `noot`; anders de dichtstbijzijnde opname (bij
    /// gelijke afstand de lagere).
    fn kies(&self, noot: u32) -> Option<&PathBuf> {
        if let Some(p) = self.per_noot.get(&noot) {
            return Some(p);
        }
        let onder = self.per_noot.range(..noot).next_back();
        let boven = self.per_noot.range(noot..).next();
        match (onder, boven) {
            (Some((a, pa)), Some((b, pb))) => Some(if noot - a <= b - noot { pa } else { pb }),
            (Some((_, p)), None) | (None, Some((_, p))) => Some(p),
            (None, None) => None,
        }
    }

    /// Precies dit bestand (voor releases: dezelfde opname als de aanslag).
    fn exact(&self, noot: u32) -> Option<&PathBuf> {
        self.per_noot.get(&noot)
    }
}

struct SampleZoeker<'a> {
    pakketten: &'a HashMap<u32, PathBuf>,
    mappen: HashMap<(u32, String), SampleMap>,
    /// Mapnamen die uit het pakket zouden ontsnappen en daarom geweigerd zijn.
    geweigerd: std::collections::BTreeSet<(u32, String)>,
    /// Markers (loops + cue) per hoofdbestand, eenmaal gelezen.
    markers: HashMap<PathBuf, Option<crate::loader::WavMarkers>>,
    /// Markers werkelijk lezen (niet in de snelle voorcontrole).
    markers_lezen: bool,
    /// Rangen met pijpen die geen release kunnen krijgen: rang-id → (naam, aantal).
    zonder_release: BTreeMap<u32, (String, usize)>,
}

impl SampleZoeker<'_> {
    fn markers(&mut self, pad: &Path) -> Option<&crate::loader::WavMarkers> {
        // Alleen gewone WAV: een WavPack-bestand zou hiervoor volledig gedecodeerd worden.
        if !self.markers_lezen || !pad.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
            return None;
        }
        self.markers
            .entry(pad.to_path_buf())
            .or_insert_with(|| crate::loader::read_wav_markers(pad))
            .as_ref()
    }
}

/// Blijft deze mapnaam binnen het pakket? Absolute paden, stationsletters en
/// `..` zouden bestanden buiten de gekozen pakketroot kunnen openen.
fn map_blijft_in_pakket(map: &str) -> bool {
    let m = map.replace('\\', "/");
    !(m.starts_with('/') || m.contains(':') || m.split('/').any(|d| d == ".."))
}

impl<'a> SampleZoeker<'a> {
    fn map(&mut self, pakket: u32, map: &str) -> Option<&SampleMap> {
        let dir = self.pakketten.get(&pakket)?;
        if !map_blijft_in_pakket(map) {
            self.geweigerd.insert((pakket, map.to_string()));
            return None;
        }
        let sleutel = (pakket, map.to_string());
        if !self.mappen.contains_key(&sleutel) {
            let pad = if map.is_empty() { dir.clone() } else { dir.join(map.replace('\\', "/")) };
            self.mappen.insert(sleutel.clone(), SampleMap::lees(&pad));
        }
        self.mappen.get(&sleutel)
    }
}

fn lineair(a36: f32, a96: f32, noot: u32) -> f32 {
    a36 + (a96 - a36) * (noot as f32 - 36.0) / 60.0
}

/// Vaste, willekeurige stemafwijking (cents) van één pijp, begrensd door
/// `max_hz` bij de doeltoon van de pijp. Hauptwerk past bij elke activering een
/// willekeurige afwijking toe "gewogen naar zuivere stemming"; hier is die
/// per pijp stabiel (afgeleid van rang en noot, niet van de laadvolgorde) en
/// kleiner dan het maximum met kans ~ u² (gemiddeld een derde van het maximum).
/// Benadering: de kansverdeling van Hauptwerk is niet gedocumenteerd.
pub fn willekeurige_stemming_cents(rank_id: u32, pijp_noot: u32, max_hz: f32, doel_cents: f32) -> f32 {
    // SplitMix64 over (rang, noot): geen afhankelijkheid van een RNG-crate.
    let mut z = ((rank_id as u64) << 32 | pijp_noot as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let u = ((z >> 11) as f64 / (1u64 << 53) as f64) as f32; // 0..1
    let teken = if z & 1 == 0 { 1.0 } else { -1.0 };
    let delta_hz = teken * max_hz * u * u;
    let f = 440.0 * 2f32.powf((doel_cents - 6900.0) / 1200.0);
    if f <= 0.0 { return 0.0; }
    1200.0 * ((f + delta_hz).max(0.01) / f).log2()
}

/// Doeltoon (absoluut, cents; 6000 = c') van de pijp met noot `pijp_noot` in
/// deze rang: de noot plus de oktaafverhouding van de uitvoerharmonische.
pub fn doeltoon_cents(r: &CodmRank, pijp_noot: u32) -> f32 {
    100.0 * pijp_noot as f32 + 1200.0 * (r.harmonische as f32 / 8.0).log2()
}

/// Nominale toon (absoluut, cents) van de opname met bestandsnoot `sample_noot`:
/// de noot plus de oktaafverhouding van haar basisharmonische (leeg = die van
/// de uitvoer, dus de opname staat dan op de toon van de pijp zelf).
pub fn nominale_sample_cents(r: &CodmRank, sample_noot: u32) -> f32 {
    let hs = r.sample_harmonische.filter(|_| r.samples_gestemd).unwrap_or(r.harmonische);
    100.0 * sample_noot as f32 + 1200.0 * (hs as f32 / 8.0).log2()
}

/// De pijp `pijp_noot` van een rang: aanslagbestand + releases + inschaling.
fn bouw_pijp(z: &mut SampleZoeker, r: &CodmRank, pijp_noot: u32) -> Option<PipeDef> {
    let sample_noot = (pijp_noot as i32 + r.sample_sprong).max(0) as u32;
    let hoofd = z.map(r.pakket, &r.map)?;
    let pad = hoofd.kies(sample_noot)?.clone();
    // Releases horen bij dezelfde opname als de aanslag.
    let gekozen_noot = hoofd.per_noot.iter().find(|(_, p)| **p == pad).map(|(n, _)| *n).unwrap_or(sample_noot);

    let mut extra = PipeExtra::default();
    extra.key_midi_note = Some(pijp_noot);
    extra.harmonic_number = Some(r.harmonische);
    let db = lineair(r.niveau_db_36, r.niveau_db_96, pijp_noot);
    if db != 0.0 { extra.gain_db = Some(db); }
    let ct = lineair(r.stemming_ct_36, r.stemming_ct_96, pijp_noot);
    // Samplekeuze-transpositie (los van de stemming): de gekozen opname klinkt
    // op haar eigen nominale toon, de pijp hoort op de doeltoon. Ze valt samen
    // bij een eigen opname per pijp, en is een halve toon of meer bij een
    // geleende opname of een opname die vele pijpen bestrijkt (chimes).
    let nominaal = nominale_sample_cents(r, gekozen_noot);
    let transpositie = doeltoon_cents(r, pijp_noot) - nominaal;
    extra.keuze_sample_cents = Some(nominaal);
    if r.willekeurig_hz > 0.0 {
        extra.random_tuning_cents = Some(willekeurige_stemming_cents(r.id, pijp_noot, r.willekeurig_hz, doeltoon_cents(r, pijp_noot)));
    }
    let totaal = ct + if transpositie.abs() > 0.01 { transpositie } else { 0.0 };
    if totaal != 0.0 { extra.pitch_tuning_cents = Some(totaal); }
    // HarmonicShaping: 3e+ boventonen bijstellen. De hoek ligt tussen de 2e en
    // 3e harmonische van de OPGENOMEN grondtoon (het filter werkt op de ruwe
    // opname, vóór hertemperen), dus relatief aan `gekozen_noot`.
    let shaping = lineair(r.shaping_db_36, r.shaping_db_96, pijp_noot);
    if shaping.abs() >= 0.1 {
        let f0 = 440.0 * 2f32.powf((gekozen_noot as f32 - 69.0) / 12.0);
        extra.harm_shelf = Some(crate::loader::HarmShelf { corner_hz: f0 * 2.449, gain_db: shaping });
    }
    if r.percussief {
        extra.percussive = Some(true);
    } else {
        for (map, max_ms) in [
            (&r.korte_release_map, r.korte_release_ms),
            (&r.middel_release_map, r.middel_release_ms),
            (&r.release_map, None),
        ] {
            if map.is_empty() { continue; }
            if let Some(rp) = z.map(r.pakket, map).and_then(|m| m.exact(gekozen_noot)).cloned() {
                extra.releases.push(ReleaseDef {
                    path: rp,
                    max_key_press_time_ms: max_ms.filter(|m| *m > 0),
                    // Een los release-bestand begint bij frame 0: een marker erin
                    // (natte opname) geeft alleen het begin van het uitsterven aan
                    // en is niet de start van de release.
                    cue_point: Some(0),
                    release_end: None,
                    crossfade_ms: None,
                });
            }
        }
        // Geen aparte primaire release: die zit in het hoofdbestand, vanaf de
        // EERSTE marker (Hauptwerk). Het GrandOrgue-gedrag (hoogste cue) zou bij
        // een tweede marker voor het uitsterven de release te laat laten starten.
        if r.release_map.is_empty() {
            let mk = z.markers(&pad).cloned();
            match mk {
                Some(m) => {
                    if let Some(eerste) = m.cue_frames.first() {
                        extra.cue_point = Some(*eerste as u32);
                    }
                    if m.cue_frames.is_empty() || m.loops.is_empty() {
                        let e = z.zonder_release.entry(r.id).or_insert_with(|| (r.naam.clone(), 0));
                        e.1 += 1;
                    }
                }
                None => {}
            }
        }
    }
    Some(PipeDef::Sample { path: pad, extra })
}

// ============================================================================
// Naar OrganDefinition
// ============================================================================

/// Laad een CODM-bestand: structuur lezen, samples in de pakketten zoeken.
pub fn laad_codm(odf_path: &Path, xml: &str) -> Result<OrganDefinition, OdfError> {
    laad_codm_met_verslag(odf_path, xml, None).0
}

/// Laad een CODM-bestand en lever daarbij een [`ImportReport`], ook als het laden mislukt.
///
/// `pakketroot` = een door de gebruiker gekozen map (die `OrganInstallationPackages`
/// bevat); `None` = zoek vanaf de definitie omhoog.
pub fn laad_codm_met_verslag(
    odf_path: &Path,
    xml: &str,
    pakketroot: Option<&Path>,
) -> (Result<OrganDefinition, OdfError>, ImportReport) {
    laad_codm_intern(odf_path, xml, pakketroot, true)
}

/// Snelle voorcontrole: dezelfde parse, pakketten en samplekeuze, maar zonder de
/// markers (loops/cue) van elk hoofdbestand te lezen. Bij een grote set scheelt dat
/// duizenden bestandsopeningen; de release-bevindingen komen dan pas bij het echte
/// laden in het verslag. Voor de beslissing "ontbreekt er iets" is niets van belang
/// voor de markers.
pub fn voorcontrole_codm(
    odf_path: &Path,
    xml: &str,
    pakketroot: Option<&Path>,
) -> (Result<OrganDefinition, OdfError>, ImportReport) {
    laad_codm_intern(odf_path, xml, pakketroot, false)
}

fn laad_codm_intern(
    odf_path: &Path,
    xml: &str,
    pakketroot: Option<&Path>,
    diep: bool,
) -> (Result<OrganDefinition, OdfError>, ImportReport) {
    let bron = odf_path.to_string_lossy().to_string();
    let orgel = match parse_codm(xml) {
        Ok(o) => o,
        Err(e) => {
            let mut v = ImportReport { formaat: "codm".into(), bron, niet_speelbaar: true, onvolledig: true, ..Default::default() };
            v.melding(Ernst::Fout, "bestand", e.to_string());
            return (Err(e), v);
        }
    };
    let handmatig = pakketroot.is_some();
    let root: Option<PathBuf> = match pakketroot {
        Some(r) => Some(r.to_path_buf()),
        None => crate::hauptwerk::find_package_root(odf_path),
    };
    let pakketten = root.as_deref().map(crate::hauptwerk::scan_package_dirs).unwrap_or_default();
    let base = odf_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut verslag = nieuw_verslag(&orgel, &pakketten, &bron);
    verslag.pakketroot = root.as_ref().map(|r| r.to_string_lossy().to_string());
    verslag.pakketroot_handmatig = handmatig;
    if root.is_none() {
        verslag.melding(
            Ernst::Fout,
            "OrganInstallationPackages",
            "geen map OrganInstallationPackages gevonden naast of boven de definitie; kies de map handmatig",
        );
    }
    let r = bouw_definitie(&orgel, &pakketten, base, &mut verslag, diep);
    if let Ok(def) = &r {
        vul_geladen(&mut verslag, def);
    }
    if let Err(e) = &r {
        verslag.melding(Ernst::Fout, "orgel", e.to_string());
    }
    verslag.bepaal_volledigheid();
    (r, verslag)
}

/// Vul de "geladen"-tellingen van het verslag uit de gebouwde definitie.
fn vul_geladen(verslag: &mut ImportReport, def: &OrganDefinition) {
    verslag.geladen.stops = def.stops.len();
    verslag.geladen.divisies = def.manuals.len();
    verslag.geladen.koppels = def.couplers.len();
    verslag.geladen.tremulanten = def.tremulants.len();
    verslag.geladen.pijpen = def.stops.iter().map(|s| {
        s.pipes.iter().chain(s.layers.iter().flat_map(|l| l.pipes.iter()))
            .filter(|p| matches!(p, PipeDef::Sample { .. })).count()
    }).sum();
    verslag.geladen.releases = def.stops.iter().map(|s| {
        s.pipes.iter().chain(s.layers.iter().flat_map(|l| l.pipes.iter()))
            .map(|p| match p { PipeDef::Sample { extra, .. } => extra.releases.len(), _ => 0 }).sum::<usize>()
    }).sum();
}

/// Het verslag zoals het er zonder samples uitziet: wat de definitie belooft
/// en welke pakketten daarvoor nodig zijn en gevonden zijn.
fn nieuw_verslag(orgel: &CodmOrgel, pakketten: &HashMap<u32, PathBuf>, bron: &str) -> ImportReport {
    let mut v = ImportReport {
        formaat: "codm".into(),
        versie: orgel.versie.clone(),
        bron: bron.to_string(),
        naam: orgel.naam.clone(),
        ..Default::default()
    };
    v.verwacht = Tellingen {
        divisies: orgel.divisies.len(),
        stops: orgel.stops.len(),
        stopranks: orgel.stopranks.len(),
        ranks: orgel.ranks.len(),
        koppels: orgel.koppels.len(),
        tremulanten: orgel.tremulanten.len(),
        zwelkasten: orgel.zwelkasten.len(),
        ..Default::default()
    };
    // Alle pakketten die ergens naar verwijzen, ook als ze niet in de lijst
    // RequiredInstallationPackage staan; rol per pakket.
    let mut ids: std::collections::BTreeSet<u32> = orgel.pakket_rollen.keys().copied().collect();
    ids.extend(orgel.pakketten.keys().copied());
    for id in ids {
        let rollen: Vec<PakketRol> = orgel.pakket_rollen.get(&id).map(|r| r.iter().copied().collect()).unwrap_or_default();
        let rollen = if rollen.is_empty() { vec![PakketRol::Overig] } else { rollen };
        let pad = pakketten.get(&id);
        v.pakketten.push(PakketStatus {
            id,
            naam: orgel.pakketten.get(&id).cloned().unwrap_or_default(),
            rollen,
            gevonden: pad.is_some(),
            pad: pad.map(|p| p.to_string_lossy().to_string()),
        });
    }
    v.meldingen.extend(orgel.meldingen.iter().cloned());
    // Golfvormtremulant: de golfvorm (`…TremulantPitchAndFundamentalAmplitudeWaveform.wav`)
    // staat in een Hauptwerk-eigen formaat dat niet openbaar gedocumenteerd is en
    // wordt niet gelezen. De tremulant klinkt als een eenvoudige LFO-modulatie.
    let trem_rangen: Vec<&CodmRank> = orgel.ranks.values().filter(|r| !r.trem_map.is_empty() && r.tremulant.is_some()).collect();
    if !trem_rangen.is_empty() {
        v.melding(
            Ernst::Waarschuwing,
            "tremulant",
            format!(
                "{} rangen gebruiken golfvormtremulant (Hauptwerk-eigen formaat, niet openbaar gedocumenteerd): de golfvorm wordt niet gelezen en de tremulant klinkt als een eenvoudige modulatie (benadering, niet gevalideerd)",
                trem_rangen.len()
            ),
        );
    }
    let deel_zwelkast = orgel.ranks.values().filter(|r| r.zwelkast_deel).count();
    if deel_zwelkast > 0 {
        v.melding(
            Ernst::Waarschuwing,
            "zwelkast",
            format!("{deel_zwelkast} rangen hebben maar een deel van hun pijpen in de zwelkast; de hele rang wordt als ingesloten behandeld"),
        );
    }
    let deel_trem = orgel.ranks.values().filter(|r| r.trem_deel).count();
    if deel_trem > 0 {
        v.melding(
            Ernst::Waarschuwing,
            "tremulant",
            format!("{deel_trem} rangen hebben maar een deel van hun pijpen onder de tremulant; de hele rang wordt als tremulant-gevoelig behandeld"),
        );
    }
    let wind_rangen = orgel.ranks.values().filter(|r| r.wind_flow_pct > 0.0).count();
    if wind_rangen > 0 {
        v.melding(
            Ernst::Info,
            "wind",
            format!("{wind_rangen} rangen vragen willekeurige variatie van de windstroom (WindSupply_MaxFlowRandomisationPct); niet toegepast, het JM-windmodel heeft een eigen pijpverschil"),
        );
    }
    let random_rangen = orgel.ranks.values().filter(|r| r.willekeurig_hz > 0.0).count();
    if random_rangen > 0 {
        v.melding(
            Ernst::Info,
            "stemming",
            format!("{random_rangen} rangen krijgen een vaste willekeurige stemafwijking per pijp (Pitch_MaxRandomTuningErrorHz, benadering van de Hauptwerk-kansverdeling); uit te zetten voor A/B-vergelijking"),
        );
    }
    // Reverb tail truncation (modus 2/3) wordt nog niet toegepast: de natte
    // release klinkt dan met de volle nagalm. Vermeld dat per rang, in plaats van
    // het stil te laten vallen.
    for r in orgel.ranks.values().filter(|r| !r.percussief && r.truncatie_modus >= 2) {
        v.melding(
            Ernst::Waarschuwing,
            format!("rank {}", r.id),
            format!(
                "rang '{}' vraagt reverb tail truncation (modus {}, {} ms): niet beschikbaar, de release klinkt met de volle nagalm",
                r.naam, r.truncatie_modus, r.truncatie_ms
            ),
        );
    }
    for p in v.pakketten.clone() {
        if p.gevonden { continue; }
        let naam = if p.naam.is_empty() { p.id.to_string() } else { format!("{} ({})", p.id, p.naam) };
        if p.rollen.contains(&PakketRol::Tremulant) && !p.klank_nodig() {
            v.melding(
                Ernst::Waarschuwing,
                format!("pakket {}", p.id),
                format!("tremulant-golfvormen in pakket {naam} ontbreken: verminderde klankdekking, de tremulant klinkt als eenvoudige modulatie"),
            );
        } else if p.rollen.iter().all(|r| matches!(r, PakketRol::Beeld | PakketRol::Info)) {
            v.melding(
                Ernst::Info,
                format!("pakket {}", p.id),
                format!("pakket {naam} bevat alleen afbeeldingen of documentatie; de klank wordt er niet door beïnvloed"),
            );
        }
    }
    v
}

/// Zet de CODM-structuur om naar een speelbare [`OrganDefinition`].
pub fn codm_naar_definitie(
    orgel: &CodmOrgel,
    pakketten: &HashMap<u32, PathBuf>,
    base_path: PathBuf,
) -> Result<OrganDefinition, OdfError> {
    let mut verslag = nieuw_verslag(orgel, pakketten, "");
    bouw_definitie(orgel, pakketten, base_path, &mut verslag, true)
}

fn bouw_definitie(
    orgel: &CodmOrgel,
    pakketten: &HashMap<u32, PathBuf>,
    base_path: PathBuf,
    verslag: &mut ImportReport,
    diep: bool,
) -> Result<OrganDefinition, OdfError> {
    let ontbrekend: Vec<u32> = orgel.sample_pakketten().into_iter().filter(|p| !pakketten.contains_key(p)).collect();
    if !ontbrekend.is_empty() {
        warn!("CODM '{}': samplepakket(ten) niet gevonden: {:?}", orgel.naam, ontbrekend);
    }

    let mut z = SampleZoeker { pakketten, mappen: HashMap::new(), geweigerd: Default::default(), markers: HashMap::new(), markers_lezen: diep, zonder_release: BTreeMap::new() };
    let divisie_bekend: HashSet<u32> = orgel.divisies.iter().map(|d| d.code).collect();

    // Windladen: één per combinatie (zwelkast, tremulant) die een stop gebruikt.
    let mut windladen: Vec<(Option<u32>, Option<u32>)> = Vec::new();
    let trem_nummer: HashMap<u32, u32> = orgel.tremulanten.iter().enumerate().map(|(i, t)| (t.code, i as u32 + 1)).collect();

    let mut stop_defs: Vec<(u32, StopDef)> = Vec::new();
    let mut effect_bijgeluiden: Vec<crate::grandorgue::BackgroundSoundDef> = Vec::new();
    let mut overgeslagen = 0usize;
    let mut beschermd = 0usize;
    // Effectregister-herkenning (0.9.49): een stop met één pijp (één WAV) is
    // een percussief effect zoals Cymbelstern of Vogelzang; die hoort in de
    // Effectregisters-divisie i.p.v. op het klavier. Grotere rangen blijven
    // altijd op het klavier, ook als er een momentary-piston op zit.
    for stop in &orgel.stops {
        if !divisie_bekend.contains(&stop.divisie) {
            warn!("CODM stop '{}' ({}): divisie {} staat niet in de lijst — overgeslagen", stop.naam, stop.code, stop.divisie);
            continue;
        }
        let delen: Vec<&CodmStopRank> = orgel.stopranks.iter().filter(|sr| sr.stop == stop.code).collect();
        // Per StopRank: toets → pijp. Overlappende StopRanks (mixtuurkoren)
        // gaan in een eigen laag.
        let mut lagen: Vec<(String, BTreeMap<u32, PipeDef>)> = Vec::new();
        let mut eerste_rang: Option<(&CodmRank, i32)> = None;
        for sr in &delen {
            if sr.extern_ || sr.actie_type != 1 || sr.actie_effect != 1 {
                warn!("CODM stop '{}': StopRank met externe rang/actie {}/{} wordt nog niet ondersteund", stop.naam, sr.actie_type, sr.actie_effect);
                verslag.melding(
                    Ernst::Fout,
                    format!("stop {}", stop.code),
                    format!(
                        "stop '{}': een StopRank met {} (actie {}/{}) wordt niet ondersteund en klinkt niet",
                        stop.naam,
                        if sr.extern_ { "externe rang" } else { "andere actiecode" },
                        sr.actie_type, sr.actie_effect
                    ),
                );
                continue;
            }
            let Some(r) = sr.rank.and_then(|id| orgel.ranks.get(&id)) else {
                warn!("CODM stop '{}': rang {:?} bestaat niet", stop.naam, sr.rank);
                continue;
            };
            if let Some(m) = z.map(r.pakket, &r.map) { beschermd += m.beschermd; }
            let mut deel: BTreeMap<u32, PipeDef> = BTreeMap::new();
            for toets in sr.eerste_toets..sr.eerste_toets + sr.aantal_toetsen {
                let pijp_noot = toets as i32 + sr.sprong;
                if pijp_noot < r.eerste_pijp as i32 || pijp_noot >= (r.eerste_pijp + r.aantal_pijpen) as i32 {
                    continue;
                }
                if let Some(p) = bouw_pijp(&mut z, r, pijp_noot as u32) {
                    deel.insert(toets, p);
                }
            }
            if deel.is_empty() { continue; }
            if eerste_rang.is_none() { eerste_rang = Some((r, sr.sprong)); }
            match lagen.iter_mut().find(|(_, l)| deel.keys().all(|k| !l.contains_key(k))) {
                Some((_, l)) => l.extend(deel),
                None => lagen.push((r.naam.clone(), deel)),
            }
        }
        let Some((rang, sprong)) = eerste_rang else {
            overgeslagen += 1;
            warn!("CODM stop '{}' ({}): geen samples gevonden — overgeslagen", stop.naam, stop.code);
            // Waarom? Eerst het ontbrekende pakket, dan beschermde samples, dan een lege map.
            let mut reden = String::from("geen samples gevonden in de samplemap van de rang");
            let ontbr: Vec<String> = delen.iter()
                .filter_map(|sr| sr.rank.and_then(|id| orgel.ranks.get(&id)))
                .map(|r| r.pakket)
                .filter(|p| *p > 0 && !pakketten.contains_key(p))
                .collect::<std::collections::BTreeSet<u32>>()
                .into_iter()
                .map(|p| match orgel.pakketten.get(&p) {
                    Some(n) if !n.is_empty() => format!("{p} ({n})"),
                    _ => p.to_string(),
                })
                .collect();
            if !ontbr.is_empty() {
                reden = format!("samplepakket {} ontbreekt", ontbr.join(", "));
            } else if delen.iter().filter_map(|sr| sr.rank.and_then(|id| orgel.ranks.get(&id)))
                .any(|r| z.map(r.pakket, &r.map).is_some_and(|m| m.beschermd > 0)) {
                reden = "de samples staan in het beveiligde .hbw-formaat en zijn niet te lezen".into();
            }
            verslag.overgeslagen.push(OvergeslagenStop { id: stop.code, naam: stop.naam.clone(), reden });
            continue;
        };
        if lagen.len() > 16 {
            warn!("CODM stop '{}': {} lagen, alleen de eerste 16 worden gebruikt", stop.naam, lagen.len());
            lagen.truncate(16);
        }
        let laag_noot = lagen.iter().filter_map(|(_, l)| l.keys().next().copied()).min().unwrap();
        let hoog_noot = lagen.iter().filter_map(|(_, l)| l.keys().next_back().copied()).max().unwrap();
        let mut lijsten: Vec<(String, Vec<PipeDef>)> = lagen.into_iter().map(|(naam, mut l)| {
            let v = (laag_noot..=hoog_noot).map(|n| l.remove(&n).unwrap_or(PipeDef::Empty)).collect();
            (naam, v)
        }).collect();
        let pipes = lijsten.remove(0).1;
        let layers: Vec<PipeLayer> = lijsten.into_iter().enumerate()
            .map(|(i, (name, pipes))| PipeLayer { index: (i + 1) as u8, name, perspective: None, pipes })
            .collect();

        let windlade = (rang.zwelkast, rang.tremulant.filter(|t| trem_nummer.contains_key(t)));
        {
            let andere: std::collections::BTreeSet<(Option<u32>, Option<u32>)> = delen.iter()
                .filter(|sr| !sr.extern_)
                .filter_map(|sr| sr.rank.and_then(|id| orgel.ranks.get(&id)))
                .map(|r| (r.zwelkast, r.tremulant.filter(|t| trem_nummer.contains_key(t))))
                .collect();
            if andere.len() > 1 {
                verslag.melding(
                    Ernst::Waarschuwing,
                    format!("stop {}", stop.code),
                    format!(
                        "stop '{}' heeft rangen met verschillende zwelkast/tremulant; alleen die van de eerste rang (zwelkast {:?}, tremulant {:?}) geldt voor het hele register",
                        stop.naam, windlade.0, windlade.1
                    ),
                );
            }
        }
        let windchest_group = if windlade == (None, None) {
            0
        } else {
            match windladen.iter().position(|w| *w == windlade) {
                Some(i) => i as u32 + 1,
                None => { windladen.push(windlade); windladen.len() as u32 }
            }
        };

        // Effectregister (0.9.49): een CODM-stop met één pijp (één WAV) is
        // een percussief effect zoals Cymbelstern of Vogelzang; die komt als
        // Toy in de Effectregisters-divisie en NIET op het klavier. Rangen
        // met meerdere pijpen blijven altijd op het klavier.
        if pipes.len() == 1 {
            let wav: Option<PathBuf> = pipes.iter().find_map(|p| match p {
                PipeDef::Sample { path, .. } => Some(path.clone()),
                _ => None,
            });
            if let Some(path) = wav {
                let loop_hint = if rang.percussief { " (percussief — speelt voorlopig nog als loop; one-shot volgt)" } else { "" };
                verslag.melding(
                    Ernst::Info,
                    format!("stop {}", stop.code),
                    format!(
                        "stop '{}' is een effectregister (één pijp) en komt in de Effectregisters-divisie{loop_hint}",
                        stop.naam
                    ),
                );
                effect_bijgeluiden.push(crate::grandorgue::BackgroundSoundDef {
                    kind: crate::grandorgue::BackgroundSoundKind::Toy,
                    name: clean_stop_name(zonder_voorvoegsel(&stop.naam)),
                    division: None,
                    path,
                });
                continue;
            }
            // Geen Sample-pad gevonden: laat als gewone stop staan — beter een
            // (misschien stille) klavierknop dan een onbereikbare Toy.
        }

        stop_defs.push((stop.divisie, StopDef {
            id: stop.code,
            name: clean_stop_name(zonder_voorvoegsel(&stop.naam)),
            harmonic_number: rang.harmonische,
            sounding_harmonic: klinkende_harmonische(rang.harmonische, sprong),
            pitch_correction: 0.0,
            pitch_tuning_cents: 0.0,
            gain_db: 0.0,
            number_of_pipes: pipes.len() as u32,
            first_accessible_pipe_logical_key: 1,
            windchest_group,
            amplitude_level: 100.0,
            percussive: rang.percussief,
            accepts_retuning: true,
            first_midi_note: Some(laag_noot),
            pipes,
            layers,
            primary_perspective: None,
        }));
    }

    verslag.beschermd = beschermd;
    // Golfvormbestanden van tremulantrangen: aanwezig in het pakket?
    for r in orgel.ranks.values().filter(|r| !r.trem_map.is_empty() && r.tremulant.is_some()) {
        let Some(dir) = pakketten.get(&r.trem_pakket) else { continue };
        if !map_blijft_in_pakket(&r.trem_map) { continue; }
        let pad = dir.join(r.trem_map.replace('\\', "/"));
        let aantal = std::fs::read_dir(&pad).map(|rd| rd.flatten().filter(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.contains("tremulant") && (n.ends_with(".wav") || n.ends_with(".hbw") || n.ends_with(".wv"))
        }).count()).unwrap_or(0);
        if aantal == 0 {
            verslag.melding(
                Ernst::Waarschuwing,
                format!("rank {}", r.id),
                format!("rang '{}': geen tremulantgolfbestanden gevonden in '{}'", r.naam, r.trem_map),
            );
        }
    }
    for (id, (naam, aantal)) in &z.zonder_release {
        verslag.melding(
            Ernst::Waarschuwing,
            format!("rank {id}"),
            format!(
                "rang '{naam}': {aantal} pijpen hebben geen release-marker met loop in het hoofdbestand en geen aparte release; die noten klinken abrupt af"
            ),
        );
    }
    for (pakket, map) in &z.geweigerd {
        verslag.melding(
            Ernst::Fout,
            format!("pakket {pakket}"),
            format!("samplemap '{map}' verwijst buiten het pakket en is niet gebruikt"),
        );
    }
    if stop_defs.is_empty() {
        let mut uitleg = String::from("deze Hauptwerk Custom Organ-definitie (CODM) is gelezen, maar er zijn geen samples gevonden");
        if !ontbrekend.is_empty() {
            let namen: Vec<String> = ontbrekend.iter().map(|id| match orgel.pakketten.get(id) {
                Some(n) if !n.is_empty() => format!("{} ({})", id, n),
                _ => id.to_string(),
            }).collect();
            // De code tussen haken zet de UI om in een melding in de gekozen taal.
            uitleg = format!(
                "[codm-pakketten {}] {}. Ontbrekende samplepakketten: {}. Zet die in de map OrganInstallationPackages naast (of boven) deze definitie",
                namen.join(", "), uitleg, namen.join(", ")
            );
        } else if beschermd > 0 {
            uitleg.push_str(&format!(": {} samples staan in het beveiligde Hauptwerk-formaat (.hbw) en zijn niet te lezen", beschermd));
        }
        return Err(OdfError::ParseError { line: 0, message: uitleg });
    }

    // Manualen: DivisionCode 1 = pedaal (0), 2..7 = manuaal 1..6, verder doorgenummerd.
    let mut manuals: Vec<ManualDef> = orgel.divisies.iter().filter_map(|d| {
        let stop_ids: Vec<u32> = stop_defs.iter().filter(|(div, _)| *div == d.code).map(|(_, s)| s.id).collect();
        if stop_ids.is_empty() { return None; }
        let nummer = d.code - 1;
        Some(ManualDef {
            number: nummer,
            name: d.naam.clone(),
            midi_input_number: nummer + 1,
            number_of_logical_keys: d.aantal_toetsen,
            number_of_accessible_keys: d.aantal_toetsen,
            first_accessible_key_midi_note: d.eerste_toets,
            stop_ids,
            coupler_ids: Vec::new(),
            tremulant_ids: Vec::new(),
        })
    }).collect();
    manuals.sort_by_key(|m| m.number);
    let met_stops: HashSet<u32> = manuals.iter().map(|m| m.number + 1).collect();
    let heeft_manual = |div: u32| met_stops.contains(&div);

    let mut couplers: Vec<CouplerDef> = Vec::new();
    let mut koppel_nummer: HashMap<u32, u32> = HashMap::new();
    for k in &orgel.koppels {
        if !heeft_manual(k.van_toetsen) || !heeft_manual(k.klinkt) {
            continue;
        }
        let nummer = couplers.len() as u32 + 1;
        koppel_nummer.insert(k.code, nummer);
        couplers.push(CouplerDef {
            number: nummer,
            // Niet door clean_stop_name: die haalt een divisie-afkorting
            // vooraan weg ("Sw to Gt 16'" → "to Gt 16'").
            name: k.naam.clone(),
            source_manual: k.van_toetsen - 1,
            destination_manual: k.klinkt - 1,
            destination_keyshift: k.sprong,
            unison_off: k.soort == KoppelSoort::UnisonOff,
            melodie_of_bas: matches!(k.soort, KoppelSoort::Bas | KoppelSoort::Melodie),
            melodie: k.soort == KoppelSoort::Melodie,
            // CODM: geen keten (gids: source keyboard → destination key action).
            cascade_toegestaan: false,
        });
        if let Some(m) = manuals.iter_mut().find(|m| m.number + 1 == k.van_toetsen) {
            m.coupler_ids.push(nummer);
        }
    }

    let tremulants: Vec<TremulantDef> = orgel.tremulanten.iter().map(|t| TremulantDef {
        number: trem_nummer[&t.code],
        name: t.naam.clone(),
        wave: false,
        period: 1000.0 / t.frequentie_hz,
        amp_mod_depth: 18.0,
        start_rate: 4.0,
        stop_rate: 4.0,
    }).collect();
    for t in &orgel.tremulanten {
        let n = trem_nummer[&t.code];
        match t.divisie {
            Some(div) => if let Some(m) = manuals.iter_mut().find(|m| m.number + 1 == div) { m.tremulant_ids.push(n) },
            None => if let Some(m) = manuals.iter_mut().find(|m| m.number > 0) { m.tremulant_ids.push(n) },
        }
    }

    let enclosures: Vec<EnclosureDef> = orgel.zwelkasten.iter().map(|(code, naam)| EnclosureDef {
        number: *code,
        name: naam.clone(),
        amp_minimum_level: 0.0,
        midi_input_number: 0,
    }).collect();
    let windchests: Vec<WindchestDef> = windladen.iter().enumerate().map(|(i, (kast, trem))| WindchestDef {
        number: i as u32 + 1,
        comment: kast.and_then(|k| orgel.zwelkasten.iter().find(|(c, _)| *c == k)).map(|(_, n)| n.clone()).unwrap_or_default(),
        enclosure_ids: kast.iter().copied().collect(),
        tremulant_ids: trem.and_then(|t| trem_nummer.get(&t)).copied().into_iter().collect(),
        amplitude_level: 100.0,
        gain_db: 0.0,
        pitch_tuning_cents: 0.0,
        pitch_correction_cents: 0.0,
    }).collect();

    let klavier_afkortingen: HashMap<String, String> = orgel.divisies.iter()
        .filter(|d| !d.korte_naam.is_empty())
        .map(|d| (d.korte_naam.to_uppercase(), d.naam.clone()))
        .collect();

    let organ = OrganInfo {
        church_name: orgel.naam.clone(),
        church_address: String::new(),
        organ_builder: String::new(),
        organ_build_date: String::new(),
        organ_comments: String::new(),
        recording_details: String::new(),
        number_of_manuals: manuals.iter().filter(|m| m.number > 0).count() as u32,
        has_pedals: manuals.iter().any(|m| m.number == 0),
        number_of_enclosures: enclosures.len() as u32,
        number_of_tremulants: tremulants.len() as u32,
        number_of_windchest_groups: windchests.len() as u32,
        amplitude_level: 100.0,
        gain_db: 0.0,
        pitch_tuning_cents: 0.0,
        pitch_correction_cents: 0.0,
        // CODM-rangen worden altijd naar de doeltoon gestemd; een
        // "originele orgeltoonhoogte" bestaat hier niet.
        geen_originele_toonhoogte: true,
    };

    let stops: Vec<StopDef> = stop_defs.into_iter().map(|(_, s)| s).collect();
    info!(
        "CODM import: '{}' — {} stops ({} zonder samples overgeslagen), {} manualen, {} koppels, {} zwelkasten, {} tremulanten",
        organ.church_name, stops.len(), overgeslagen, manuals.len(), couplers.len(), enclosures.len(), tremulants.len()
    );

    // Schakelaargedrag: wat in een combinatie hoort, wat standaard aan staat, pistons.
    let mut schakelaars = SchakelaarRegels::default();
    let gebouwd: HashSet<u32> = stops.iter().map(|s| s.id).collect();
    for s in orgel.stops.iter().filter(|s| gebouwd.contains(&s.code)) {
        if !s.in_combinatie { schakelaars.stops_buiten_combinatie.push(s.code); }
        if s.standaard_aan { schakelaars.stops_standaard_aan.push(s.code); }
    }
    for k in &orgel.koppels {
        let Some(&n) = koppel_nummer.get(&k.code) else { continue };
        if !k.in_combinatie { schakelaars.koppels_buiten_combinatie.push(n); }
        if k.standaard_aan { schakelaars.koppels_standaard_aan.push(n); }
    }
    // Pistons krijgen GEEN eigen UI-balk: elk doel (register, koppel,
    // tremulant) staat al bij zijn eigen divisie als knop, zo bedient de
    // organist dezelfde schakelaar via dat knop i.p.v. een duplicaat-balk.
    // Alleen in het verslag staat wat er met elke piston is gebeurd, zodat je
    // kunt zien dat de sneltoets uit de ODF niet genegeerd is zonder reden.
    let mut piston_gezien = 0usize;
    for p in &orgel.pistons {
        let gevonden = match p.doel_soort {
            1 => gebouwd.contains(&p.doel_code),
            2 => koppel_nummer.contains_key(&p.doel_code),
            3 => trem_nummer.contains_key(&p.doel_code),
            _ => false,
        };
        if !gevonden {
            verslag.melding(
                Ernst::Waarschuwing,
                format!("piston '{}'", p.naam),
                format!(
                    "shortcutpiston verwijst naar {} {} die niet geladen is; de piston doet niets",
                    ["", "register", "koppel", "tremulant"].get(p.doel_soort as usize).copied().unwrap_or("object"),
                    p.doel_code,
                ),
            );
        } else {
            piston_gezien += 1;
        }
    }
    if piston_gezien > 0 {
        verslag.melding(
            Ernst::Info,
            "pistons",
            format!(
                "{piston_gezien} shortcutpistons zijn ingelezen; hun doel staat al als knop bij de juiste divisie (register, koppel of tremulant), dus er komt geen aparte pistonbalk",
            ),
        );
    }
    let buiten_divisional = orgel.stops.iter().filter(|s| !s.in_divisional).count()
        + orgel.koppels.iter().filter(|k| !k.in_divisional).count();
    if buiten_divisional > 0 {
        verslag.melding(
            Ernst::Info,
            "combinaties",
            format!("{buiten_divisional} schakelaars horen niet in de divisional-combinaties; JM-Orgue kent alleen algemene combinaties (waarin ze wel horen)"),
        );
    }
    let trem_aan = orgel.tremulanten.iter().filter(|t| t.standaard_aan).count();
    if trem_aan > 0 {
        verslag.melding(
            Ernst::Info,
            "tremulant",
            format!("{trem_aan} tremulanten staan volgens de definitie standaard aan; niet toegepast (tremulanten worden per klavier bediend)"),
        );
    }

    Ok(OrganDefinition {
        base_path,
        organ,
        manuals,
        stops,
        windchests,
        enclosures,
        tremulants,
        couplers,
        extra_bijgeluiden: effect_bijgeluiden,
        stop_action_clicks: HashMap::new(),
        keyboard_clicks: HashMap::new(),
        klavier_afkortingen,
        schakelaars,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koppelcodes_volgens_de_gids() {
        // "Coupler: Gt to Gt 4'": toetsen Great, klinkt Great, 4'.
        assert_eq!(ontleed_koppelcode(1107), Some((2, 2, KoppelSoort::Vier, 12)));
        // "Gt to Ped 8'": toetsen pedaal, klinkt Great.
        assert_eq!(ontleed_koppelcode(1006), Some((1, 2, KoppelSoort::Unisono, 0)));
        // Unison Off van Choir (manuaal 3) en Swell (manuaal 2).
        assert_eq!(ontleed_koppelcode(1316), Some((4, 4, KoppelSoort::UnisonOff, 0)));
        assert_eq!(ontleed_koppelcode(1211), Some((3, 3, KoppelSoort::UnisonOff, 0)));
        assert_eq!(ontleed_koppelcode(1000), Some((1, 1, KoppelSoort::Zestien, -12)));
        assert_eq!(ontleed_koppelcode(1009).map(|k| k.2), Some(KoppelSoort::Melodie));
        assert_eq!(ontleed_koppelcode(1123).map(|k| k.2), Some(KoppelSoort::Bas));
        assert_eq!(ontleed_koppelcode(1700), None);
    }

    #[test]
    fn stopcodes_naar_divisie() {
        assert_eq!(divisie_van_stopcode(2001), Some(1));
        assert_eq!(divisie_van_stopcode(2101), Some(2));
        assert_eq!(divisie_van_stopcode(2699), Some(7));
        assert_eq!(divisie_van_stopcode(1999), None);
    }

    #[test]
    fn namen() {
        assert_eq!(zonder_voorvoegsel("Ped: Contra Bourdon 32'"), "Contra Bourdon 32'");
        assert_eq!(zonder_voorvoegsel("Coupler: Gt to Ped 8'"), "Gt to Ped 8'");
        assert_eq!(zonder_voorvoegsel("Principal 8'"), "Principal 8'");
        assert_eq!(titel_als_kapitalen("PEDAL"), "Pedal");
        assert_eq!(titel_als_kapitalen("Great"), "Great");
    }

    /// Klein maar volledig CODM-voorbeeld in de vorm van de echte bestanden.
    pub(crate) const VOORBEELD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Hauptwerk FileFormat="CustomOrgan" FileFormatVersion="3.00">
<ObjectList ObjectType="_general"><_general><Name>Proef</Name></_general></ObjectList>
<ObjectList ObjectType="requiredinstallationpackage">
  <RequiredInstallationPackage><InstallationPackageID>758</InstallationPackageID><Name>Pitea samples</Name></RequiredInstallationPackage>
</ObjectList>
<ObjectList ObjectType="division">
  <Division><DivisionCode>01</DivisionCode><Name>PEDAL</Name><ShortName>Ped</ShortName><NumberOfKeys>32</NumberOfKeys><MIDINoteNumOfFirstKey>36</MIDINoteNumOfFirstKey></Division>
  <Division><DivisionCode>02</DivisionCode><Name>GREAT</Name><ShortName>Gt</ShortName><NumberOfKeys>61</NumberOfKeys><MIDINoteNumOfFirstKey>36</MIDINoteNumOfFirstKey></Division>
  <Division><DivisionCode>03</DivisionCode><Name>SWELL</Name><ShortName>Sw</ShortName><NumberOfKeys>61</NumberOfKeys><MIDINoteNumOfFirstKey>36</MIDINoteNumOfFirstKey></Division>
</ObjectList>
<ObjectList ObjectType="coupler">
  <Coupler><CouplerCode>1006</CouplerCode><Name>Coupler: Gt to Ped 8'</Name></Coupler>
  <Coupler><CouplerCode>1211</CouplerCode><Name>Coupler: Sw Unison Off</Name></Coupler>
</ObjectList>
<ObjectList ObjectType="enclosure"><Enclosure><EnclosureCode>220</EnclosureCode><Name>SWELL</Name></Enclosure></ObjectList>
<ObjectList ObjectType="tremulant"><Tremulant><TremulantCode>1720</TremulantCode><Name>Tremulant: Swell</Name><FrequencyHz>5.2</FrequencyHz></Tremulant></ObjectList>
<ObjectList ObjectType="rank">
  <Rank><RankID>1</RankID><Name>Bourdon16</Name><NumberOfPipes>32</NumberOfPipes><MIDINoteNumberOfFirstPipe>36</MIDINoteNumberOfFirstPipe><IsPercussive>N</IsPercussive><Samples_InstallationPackageID>758</Samples_InstallationPackageID><Samples_MainSampleFileFolder>Bourdon16</Samples_MainSampleFileFolder><Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles>Bourdon16\rel</Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles><Samples_ShortNoteReleaseSampleFileFolderIfAny>Bourdon16\kort</Samples_ShortNoteReleaseSampleFileFolderIfAny><Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny>300</Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny><Samples_MIDINoteNumIncrementFromPipesToSamples>0</Samples_MIDINoteNumIncrementFromPipesToSamples><Pitch_RankBaseOutputPitch64ftHarmonicNum>4</Pitch_RankBaseOutputPitch64ftHarmonicNum><HarmonicShaping_PipeMIDINoteNum036_3rdAndUpperHarmLvlAdjDb>8</HarmonicShaping_PipeMIDINoteNum036_3rdAndUpperHarmLvlAdjDb><HarmonicShaping_PipeMIDINoteNum096_3rdAndUpperHarmLvlAdjDb>8</HarmonicShaping_PipeMIDINoteNum096_3rdAndUpperHarmLvlAdjDb><Encl_EnclosureCode></Encl_EnclosureCode><Trem_TremulantCode></Trem_TremulantCode></Rank>
  <Rank><RankID>2</RankID><Name>Chimes</Name><NumberOfPipes>37</NumberOfPipes><MIDINoteNumberOfFirstPipe>48</MIDINoteNumberOfFirstPipe><IsPercussive>Y</IsPercussive><Samples_InstallationPackageID>758</Samples_InstallationPackageID><Samples_MainSampleFileFolder>Chimes</Samples_MainSampleFileFolder><Samples_MIDINoteNumIncrementFromPipesToSamples>0</Samples_MIDINoteNumIncrementFromPipesToSamples><Pitch_RankBaseOutputPitch64ftHarmonicNum>8</Pitch_RankBaseOutputPitch64ftHarmonicNum><Encl_EnclosureCode>220</Encl_EnclosureCode><Trem_TremulantCode>1720</Trem_TremulantCode></Rank>
</ObjectList>
<ObjectList ObjectType="stop">
  <Stop><StopCode>2001</StopCode><Name>Ped: Bourdon 16'</Name></Stop>
  <Stop><StopCode>2002</StopCode><Name>Ped: Bourdon 8'</Name></Stop>
  <Stop><StopCode>2201</StopCode><Name>Sw: Chimes</Name></Stop>
</ObjectList>
<ObjectList ObjectType="stoprank">
  <StopRank><Name>a</Name><StopCode>2001</StopCode><RankTypeCode>1</RankTypeCode><RankID>1</RankID><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank><MIDINoteNumOfFirstMappedDivisionKey>36</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>32</NumberOfMappedDivisionKeys><ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank>
  <StopRank><Name>b</Name><StopCode>2002</StopCode><RankTypeCode>1</RankTypeCode><RankID>1</RankID><MIDINoteNumIncrementFromDivisionToRank>12</MIDINoteNumIncrementFromDivisionToRank><MIDINoteNumOfFirstMappedDivisionKey>36</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>32</NumberOfMappedDivisionKeys><ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank>
  <StopRank><Name>c</Name><StopCode>2201</StopCode><RankTypeCode>1</RankTypeCode><RankID>2</RankID><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank><MIDINoteNumOfFirstMappedDivisionKey>48</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>37</NumberOfMappedDivisionKeys><ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank>
</ObjectList>
</Hauptwerk>"#;

    #[test]
    fn structuur_zonder_samples() {
        let o = parse_codm(VOORBEELD).unwrap();
        assert_eq!(o.naam, "Proef");
        assert_eq!(o.divisies.len(), 3);
        assert_eq!(o.divisies[0].naam, "Pedal");
        assert_eq!(o.stops.iter().map(|s| s.divisie).collect::<Vec<_>>(), vec![1, 1, 3]);
        assert_eq!(o.stopranks.len(), 3);
        assert_eq!(o.ranks[&1].release_map, r"Bourdon16\rel");
        assert_eq!((o.ranks[&1].shaping_db_36, o.ranks[&1].shaping_db_96), (8.0, 8.0));
        assert_eq!(o.ranks[&1].korte_release_ms, Some(300));
        assert_eq!(o.ranks[&2].zwelkast, Some(220));
        assert_eq!(o.koppels.len(), 2);
        assert_eq!(o.koppels[1].soort, KoppelSoort::UnisonOff);
        assert_eq!(o.tremulanten[0].divisie, Some(3));
        assert_eq!(o.sample_pakketten(), vec![758]);
    }

    #[test]
    fn zonder_pakketten_een_duidelijke_melding() {
        let o = parse_codm(VOORBEELD).unwrap();
        let fout = codm_naar_definitie(&o, &HashMap::new(), PathBuf::from(".")).unwrap_err().to_string();
        assert!(fout.contains("758 (Pitea samples)"), "{}", fout);
        assert!(fout.contains("OrganInstallationPackages"), "{}", fout);
    }

    fn maak_pakket(dir: &Path) {
        let leeg = b"RIFF\0\0\0\0WAVE";
        for (map, noten) in [
            ("Bourdon16", (36..=67).collect::<Vec<u32>>()),
            ("Bourdon16/rel", (36..=67).collect()),
            ("Bourdon16/kort", (36..=67).collect()),
            ("Chimes", vec![61]),
        ] {
            let d = dir.join(map);
            std::fs::create_dir_all(&d).unwrap();
            for n in noten {
                std::fs::write(d.join(format!("{:03}-x.wav", n)), leeg).unwrap();
            }
        }
    }

    #[test]
    fn met_samples_een_speelbaar_orgel() {
        let tmp = std::env::temp_dir().join(format!("vpo_codm_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        maak_pakket(&tmp);
        let pakketten: HashMap<u32, PathBuf> = [(758u32, tmp.clone())].into_iter().collect();
        let o = parse_codm(VOORBEELD).unwrap();
        let def = codm_naar_definitie(&o, &pakketten, tmp.clone()).unwrap();

        assert_eq!(def.manuals.iter().map(|m| m.number).collect::<Vec<_>>(), vec![0, 2]);
        let b16 = def.stops.iter().find(|s| s.id == 2001).unwrap();
        assert_eq!(b16.name, "Bourdon 16'");
        assert_eq!(b16.pipes.len(), 32);
        assert_eq!((b16.harmonic_number, b16.sounding_harmonic), (4, 4));
        // Releases: kort (300 ms) en de gewone.
        let PipeDef::Sample { extra, .. } = &b16.pipes[0] else { panic!() };
        assert_eq!(extra.releases.len(), 2);
        assert_eq!(extra.releases[0].max_key_press_time_ms, Some(300));
        assert_eq!(extra.releases[1].max_key_press_time_ms, None);
        // HarmonicShaping: +8 dB high-shelf op de 3e+ boventonen, hoek tussen de
        // 2e en 3e harmonische van de opnamegrondtoon (toets 36, C ≈ 65,4 Hz).
        let shelf = extra.harm_shelf.expect("shaping gezet");
        assert_eq!(shelf.gain_db, 8.0);
        assert!((shelf.corner_hz - 65.41 * 2.449).abs() < 1.0, "{}", shelf.corner_hz);

        // Bourdon 8' leent dezelfde rang een octaaf hoger: 20 pijpen (48..67).
        let b8 = def.stops.iter().find(|s| s.id == 2002).unwrap();
        assert_eq!(b8.sounding_harmonic, 8);
        assert_eq!(b8.pipes.len(), 20);
        let PipeDef::Sample { path, extra } = &b8.pipes[0] else { panic!() };
        assert!(path.ends_with("048-x.wav"), "{:?}", path);
        assert_eq!(extra.key_midi_note, Some(48));

        // Chimes: 37 virtuele pijpen uit één opname, in zwelkast + tremulant.
        let ch = def.stops.iter().find(|s| s.id == 2201).unwrap();
        assert_eq!(ch.pipes.len(), 37);
        assert!(ch.pipes.iter().all(|p| matches!(p, PipeDef::Sample { path, .. } if path.ends_with("061-x.wav"))));
        assert!(ch.percussive);
        let wl = def.windchests.iter().find(|w| w.number == ch.windchest_group).unwrap();
        assert_eq!(wl.enclosure_ids, vec![220]);
        assert_eq!(wl.tremulant_ids, vec![1]);

        // Koppels: Gt→Ped (toetsen pedaal, klinkt Great) valt weg want Great
        // heeft geen stops; Swell Unison Off blijft.
        assert_eq!(def.couplers.len(), 1);
        assert!(def.couplers[0].unison_off);
        assert_eq!((def.couplers[0].source_manual, def.couplers[0].destination_manual), (2, 2));
        assert!(def.organ.geen_originele_toonhoogte);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ---- Kopdetectie en wellgevormdheid (werkpakket B) ----

    #[test]
    fn kop_ondanks_quotes_witruimte_en_volgorde() {
        let varianten = [
            "<?xml version=\"1.0\"?>\n<Hauptwerk FileFormat=\"CustomOrgan\" FileFormatVersion=\"3.00\">",
            "\u{feff}  \n<Hauptwerk   FileFormatVersion='3.00'  FileFormat='CustomOrgan' >",
            "<!-- commentaar met FileFormat=\"Organ\" -->\n<Hauptwerk Extra=\"x\" FileFormat = \"CustomOrgan\" FileFormatVersion=\"3.00\">",
            "<?xml version=\"1.0\"?><!DOCTYPE x><hauptwerk fileformat=\"customorgan\" fileformatversion=\"3.00\">",
        ];
        for v in varianten {
            assert!(is_codm(v), "{v}");
            assert_eq!(lees_kop(v).unwrap().versie, "3.00", "{v}");
        }
    }

    #[test]
    fn kop_herkent_geen_andere_formaten() {
        assert!(!is_codm("<Hauptwerk FileFormat=\"Organ\" FileFormatVersion=\"4.0\">"));
        assert!(!is_codm("<Iets FileFormat=\"CustomOrgan\">"));
        assert!(!is_codm("geen xml"));
        // Een attribuut dat alleen in commentaar of verderop staat telt niet mee.
        assert!(!is_codm("<Hauptwerk FileFormat=\"Organ\"><!-- FileFormat=\"CustomOrgan\" -->"));
    }

    #[test]
    fn misvormde_xml_wordt_met_plaats_geweigerd() {
        let kapot = VOORBEELD.replace("</Hauptwerk>", "");
        let fout = parse_codm(&kapot).unwrap_err().to_string();
        assert!(fout.contains("geen geldige XML") && fout.contains("regel"), "{fout}");
        // Afgekapt midden in een object: ook geweigerd, geen half orgel.
        let afgekapt = &VOORBEELD[..VOORBEELD.len() / 2];
        assert!(parse_codm(afgekapt).is_err());
    }

    #[test]
    fn andere_quotes_en_witruimte_geven_dezelfde_structuur() {
        let basis = parse_codm(VOORBEELD).unwrap();
        let anders = VOORBEELD
            .replace("FileFormat=\"CustomOrgan\" FileFormatVersion=\"3.00\"", "FileFormatVersion='3.00'   FileFormat='CustomOrgan'")
            .replace("\n", "\r\n");
        let o = parse_codm(&anders).unwrap();
        assert_eq!((o.divisies.len(), o.stops.len(), o.stopranks.len(), o.ranks.len(), o.koppels.len()),
                   (basis.divisies.len(), basis.stops.len(), basis.stopranks.len(), basis.ranks.len(), basis.koppels.len()));
    }

    #[test]
    fn onbekende_versie_wordt_gemeld() {
        let v5 = VOORBEELD.replace("FileFormatVersion=\"3.00\"", "FileFormatVersion=\"5.00\"");
        let o = parse_codm(&v5).unwrap();
        assert_eq!(o.versie, "5.00");
        assert!(o.meldingen.iter().any(|m| m.tekst.contains("niet getoetst")), "{:?}", o.meldingen);
        // 3.00 geeft die melding niet.
        assert!(!parse_codm(VOORBEELD).unwrap().meldingen.iter().any(|m| m.tekst.contains("niet getoetst")));
    }

    #[test]
    fn dubbele_rankid_wordt_expliciet_gemeld() {
        // Maak van rang 2 een tweede rang met RankID 1.
        let dubbel = VOORBEELD.replacen("<RankID>2</RankID>", "<RankID>1</RankID>", 1);
        let o = parse_codm(&dubbel).unwrap();
        let m = o.meldingen.iter().find(|m| m.object == "rank 1");
        assert!(m.is_some_and(|m| m.tekst.contains("de eerste") && m.tekst.contains("dubbel")), "{:?}", o.meldingen);
    }

    #[test]
    fn verwijzing_naar_onbestaande_rang_is_een_fout() {
        let stuk = VOORBEELD.replacen("<RankID>2</RankID>", "<RankID>77</RankID>", 1);
        // Alleen de StopRank-verwijzing moet breken: zoek een StopRank met RankID 2.
        if stuk != VOORBEELD {
            let o = parse_codm(&stuk).unwrap();
            assert!(o.meldingen.iter().any(|m| m.ernst == Ernst::Fout || m.ernst == Ernst::Waarschuwing));
        }
    }

    // ---- Echte bestanden en verslag (werkpakket A) ----

    fn fixtures() -> Option<PathBuf> {
        let map = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testen/codm");
        if map.join("Les-Pitea895-945.CustomOrgan_Hauptwerk_xml").is_file() {
            Some(map)
        } else {
            // De vier oorspronkelijke CODM-definities staan niet in de repository.
            eprintln!("OVERGESLAGEN: testen/codm ontbreekt (optionele corpusproef met de originele CODM-bestanden)");
            None
        }
    }

    /// Echte CODM-bestanden (testen/codm): structuur zoals in het vervolgonderzoek
    /// geteld. Optioneel: zonder de map wordt de proef met reden overgeslagen.
    #[test]
    fn codm_echte_bestanden() {
        let Some(map) = fixtures() else { return };
        let verwacht = [
            ("Les-Pitea895-945.CustomOrgan_Hauptwerk_xml", 4, 59, 79, 67, 25),
            ("Les-Litomysl2.CustomOrgan_Hauptwerk_xml", 4, 56, 74, 65, 19),
            ("Les-LitomyslWet2.CustomOrgan_Hauptwerk_xml", 4, 56, 74, 65, 19),
            ("Les-LitomyslWet24_2.CustomOrgan_Hauptwerk_xml", 4, 56, 74, 65, 19),
        ];
        for (f, div, stops, sr, ranks, koppels) in verwacht {
            let xml = std::fs::read_to_string(map.join(f)).unwrap();
            let o = parse_codm(&xml).unwrap();
            println!("{f}: {} divisies, {} stops, {} stopranks, {} ranks, {} koppels, pakketten {:?}",
                o.divisies.len(), o.stops.len(), o.stopranks.len(), o.ranks.len(), o.koppels.len(), o.sample_pakketten());
            assert_eq!((o.divisies.len(), o.stops.len(), o.stopranks.len(), o.ranks.len(), o.koppels.len()), (div, stops, sr, ranks, koppels), "{f}");
            assert_eq!(o.versie, "3.00", "{f}");
            // Elke StopRank verwijst naar een bestaande stop en rang.
            for s in &o.stopranks {
                assert!(o.stops.iter().any(|x| x.code == s.stop), "{f}: stop {}", s.stop);
                assert!(s.rank.is_some_and(|r| o.ranks.contains_key(&r)), "{f}: rang {:?}", s.rank);
            }
            let fouten: Vec<_> = o.meldingen.iter().filter(|m| m.ernst == Ernst::Fout).collect();
            assert!(fouten.is_empty(), "{f}: {fouten:?}");
            // Zonder pakketten: geen orgel, wél een verslag dat alles benoemt.
            let (r, v) = laad_codm_met_verslag(&map.join(f), &xml, Some(&map.join("leeg-bestaat-niet")));
            assert!(r.is_err(), "{f}");
            assert_eq!(v.verwacht.stops, stops, "{f}");
            assert_eq!(v.geladen.stops, 0, "{f}");
            assert!(v.niet_speelbaar && v.onvolledig, "{f}");
            let ontbr = v.ontbrekende_klankpakketten();
            assert!(!ontbr.is_empty(), "{f}");
            println!("  ontbrekende klankpakketten: {:?}", ontbr.iter().map(|p| p.id).collect::<Vec<_>>());
        }
    }

    /// Piteå met alleen pakket 537000 (één Chime-WAV): 59 verwacht, 1 speelbaar,
    /// 758/759 ontbreken met naam, en het is nadrukkelijk "onvolledig".
    #[test]
    fn pitea_met_alleen_het_chime_pakket_is_onvolledig() {
        let Some(map) = fixtures() else { return };
        let mini = map.join("pitea-minimal");
        let odf = mini.join("OrganDefinitions/Les-Pitea895-945.CustomOrgan_Hauptwerk_xml");
        if !odf.is_file() {
            eprintln!("OVERGESLAGEN: testen/codm/pitea-minimal ontbreekt");
            return;
        }
        let xml = std::fs::read_to_string(&odf).unwrap();
        let (r, v) = laad_codm_met_verslag(&odf, &xml, None);
        let def = r.expect("Chimes moet laden");
        assert_eq!(v.verwacht.stops, 59);
        assert_eq!(v.geladen.stops, 1, "{:?}", v.overgeslagen.iter().map(|o| &o.naam).collect::<Vec<_>>());
        assert_eq!(def.stops.len(), 1);
        assert!(v.onvolledig && !v.niet_speelbaar);
        let ontbr: Vec<u32> = v.ontbrekende_klankpakketten().iter().map(|p| p.id).collect();
        assert!(ontbr.contains(&758) && ontbr.contains(&759), "{ontbr:?}");
        assert!(!ontbr.contains(&537000));
        let p758 = v.pakketten.iter().find(|p| p.id == 758).unwrap();
        assert!(!p758.naam.is_empty(), "pakket 758 moet bij naam genoemd worden");
        assert_eq!(v.overgeslagen.len(), 58);
        assert!(v.overgeslagen.iter().any(|o| o.reden.contains("758")), "{:?}", v.overgeslagen.first());
    }

    // ---- Releases en articulatie (werkpakket D) ----

    /// Schrijf een stille 16-bit mono WAV met optionele loops (smpl) en cue-punten.
    fn schrijf_wav(pad: &Path, frames: u32, loops: &[(u32, u32)], cues: &[u32]) {
        let mut b: Vec<u8> = Vec::new();
        let put32 = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
        let put16 = |b: &mut Vec<u8>, v: u16| b.extend_from_slice(&v.to_le_bytes());
        b.extend_from_slice(b"RIFF"); put32(&mut b, 0); b.extend_from_slice(b"WAVE");
        b.extend_from_slice(b"fmt "); put32(&mut b, 16);
        put16(&mut b, 1); put16(&mut b, 1); put32(&mut b, 48000); put32(&mut b, 96000); put16(&mut b, 2); put16(&mut b, 16);
        if !loops.is_empty() {
            b.extend_from_slice(b"smpl"); put32(&mut b, 36 + 24 * loops.len() as u32);
            for v in [0u32, 0, 20833, 60, 0, 0, 0, loops.len() as u32, 0] { put32(&mut b, v); }
            for (i, (a, e)) in loops.iter().enumerate() {
                for v in [i as u32, 0, *a, *e, 0, 0] { put32(&mut b, v); }
            }
        }
        if !cues.is_empty() {
            b.extend_from_slice(b"cue "); put32(&mut b, 4 + 24 * cues.len() as u32); put32(&mut b, cues.len() as u32);
            for (i, c) in cues.iter().enumerate() {
                put32(&mut b, i as u32 + 1); put32(&mut b, *c); b.extend_from_slice(b"data"); put32(&mut b, 0); put32(&mut b, 0); put32(&mut b, *c);
            }
        }
        b.extend_from_slice(b"data"); put32(&mut b, frames * 2); b.extend(std::iter::repeat(0u8).take(frames as usize * 2));
        let totaal = b.len() as u32 - 8;
        b[4..8].copy_from_slice(&totaal.to_le_bytes());
        std::fs::write(pad, b).unwrap();
    }

    /// Eén sustaining rang van 2 pijpen (noot 60/61) met de gegeven extra rangvelden.
    fn mini_orgel(rank_velden: &str) -> String {
        format!(r#"<?xml version="1.0"?>
<Hauptwerk FileFormat="CustomOrgan" FileFormatVersion="3.00">
<ObjectList ObjectType="_general"><_general><Name>Mini</Name></_general></ObjectList>
<ObjectList ObjectType="division"><Division><DivisionCode>02</DivisionCode><Name>GREAT</Name><ShortName>Gt</ShortName><NumberOfKeys>61</NumberOfKeys><MIDINoteNumOfFirstKey>36</MIDINoteNumOfFirstKey></Division></ObjectList>
<ObjectList ObjectType="rank"><Rank><RankID>1</RankID><Name>Test</Name><NumberOfPipes>2</NumberOfPipes><MIDINoteNumberOfFirstPipe>60</MIDINoteNumberOfFirstPipe><IsPercussive>N</IsPercussive><Samples_InstallationPackageID>9</Samples_InstallationPackageID><Samples_MainSampleFileFolder>Main</Samples_MainSampleFileFolder><Samples_MIDINoteNumIncrementFromPipesToSamples>0</Samples_MIDINoteNumIncrementFromPipesToSamples><Pitch_RankBaseOutputPitch64ftHarmonicNum>8</Pitch_RankBaseOutputPitch64ftHarmonicNum>{rank_velden}</Rank></ObjectList>
<ObjectList ObjectType="stop"><Stop><StopCode>2101</StopCode><Name>Gt: Test 8'</Name></Stop></ObjectList>
<ObjectList ObjectType="stoprank"><StopRank><StopCode>2101</StopCode><RankTypeCode>1</RankTypeCode><RankID>1</RankID><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank><MIDINoteNumOfFirstMappedDivisionKey>60</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>2</NumberOfMappedDivisionKeys><ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank></ObjectList>
</Hauptwerk>"#)
    }

    fn bouw_mini(xml: &str, dir: &Path) -> (OrganDefinition, ImportReport) {
        let pakketten: HashMap<u32, PathBuf> = [(9u32, dir.to_path_buf())].into_iter().collect();
        let o = parse_codm(xml).unwrap();
        let mut v = nieuw_verslag(&o, &pakketten, "");
        let def = bouw_definitie(&o, &pakketten, dir.to_path_buf(), &mut v, true).unwrap();
        vul_geladen(&mut v, &def);
        v.bepaal_volledigheid();
        (def, v)
    }

    fn tijdmap(naam: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("vpo_codm_{}_{}", naam, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn eerste_extra(def: &OrganDefinition) -> &PipeExtra {
        match &def.stops[0].pipes[0] {
            PipeDef::Sample { extra, .. } => extra,
            _ => panic!("geen sample"),
        }
    }

    /// Hauptwerk: de release in het hoofdbestand begint bij de EERSTE marker; een
    /// tweede marker (begin van het uitsterven) mag de release niet te laat laten
    /// starten. GrandOrgue-regel (hoogste cue) blijft in de loader ongemoeid.
    #[test]
    fn hoofdbestand_met_meer_cues_start_op_de_eerste_marker() {
        let d = tijdmap("multicue");
        std::fs::create_dir_all(d.join("Main")).unwrap();
        for n in [60, 61] { schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[30000, 50000]); }
        let (def, v) = bouw_mini(&mini_orgel(""), &d);
        assert_eq!(eerste_extra(&def).cue_point, Some(30000));
        let m = crate::loader::read_wav_markers(&d.join("Main/060-x.wav")).unwrap();
        assert_eq!(m.cue_frames, vec![30000, 50000]);
        assert_eq!(m.cue_frame, Some(50000), "GrandOrgue-regel (hoogste cue) blijft");
        assert!(!v.meldingen.iter().any(|m| m.tekst.contains("release-marker")), "{:?}", v.meldingen);
    }

    /// Zonder marker/loop en zonder aparte release bestaat er geen release: dat
    /// staat in het verslag in plaats van stil te falen.
    #[test]
    fn hoofdbestand_zonder_markers_geeft_een_waarschuwing() {
        let d = tijdmap("geenmarker");
        std::fs::create_dir_all(d.join("Main")).unwrap();
        for n in [60, 61] { schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[], &[]); }
        let (def, v) = bouw_mini(&mini_orgel(""), &d);
        assert_eq!(eerste_extra(&def).cue_point, None);
        assert!(v.meldingen.iter().any(|m| m.object == "rank 1" && m.tekst.contains("2 pijpen") && m.tekst.contains("abrupt")), "{:?}", v.meldingen);
    }

    /// Korte en middellange release op de opgegeven grenzen plus de aparte
    /// primaire release; een marker in zo'n los bestand (natte opname) zet het
    /// begin van de release niet op die marker maar op frame 0.
    #[test]
    fn losse_releases_kort_middel_lang_beginnen_op_nul() {
        let d = tijdmap("losserel");
        for map in ["Main", "Main/rel150", "Main/rel300", "Main/rel900"] { std::fs::create_dir_all(d.join(map)).unwrap(); }
        for n in [60, 61] {
            schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[]);
            for map in ["Main/rel150", "Main/rel300", "Main/rel900"] {
                schrijf_wav(&d.join(map).join(format!("{n:03}-x.wav")), 20000, &[], &[8000]);
            }
        }
        let velden = r"<Samples_ShortNoteReleaseSampleFileFolderIfAny>Main\rel150</Samples_ShortNoteReleaseSampleFileFolderIfAny><Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny>150</Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny><Samples_MediumNoteReleaseSampleFileFolderIfAny>Main\rel300</Samples_MediumNoteReleaseSampleFileFolderIfAny><Samples_MaxNoteTimeMsForMediumNoteReleaseSamplesIfAny>300</Samples_MaxNoteTimeMsForMediumNoteReleaseSamplesIfAny><Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles>Main\rel900</Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles>";
        let (def, v) = bouw_mini(&mini_orgel(velden), &d);
        let rel = &eerste_extra(&def).releases;
        assert_eq!(rel.iter().map(|r| r.max_key_press_time_ms).collect::<Vec<_>>(), vec![Some(150), Some(300), None]);
        assert!(rel.iter().all(|r| r.cue_point == Some(0)), "{:?}", rel.iter().map(|r| r.cue_point).collect::<Vec<_>>());
        assert!(rel[0].path.to_string_lossy().contains("rel150") && rel[2].path.to_string_lossy().contains("rel900"));
        // Met aparte primaire release hoeft het hoofdbestand geen marker te hebben.
        assert!(!v.meldingen.iter().any(|m| m.tekst.contains("abrupt")), "{:?}", v.meldingen);
    }

    /// Alleen korte/middellange releases (geen aparte primaire): de primaire
    /// release zit dan in het hoofdbestand (marker + loop).
    #[test]
    fn alleen_getimede_releases_laten_de_primaire_in_het_hoofdbestand() {
        let d = tijdmap("getimed");
        for map in ["Main", "Main/rel150"] { std::fs::create_dir_all(d.join(map)).unwrap(); }
        for n in [60, 61] {
            schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[40000]);
            schrijf_wav(&d.join("Main/rel150").join(format!("{n:03}-x.wav")), 8000, &[], &[]);
        }
        let velden = r"<Samples_ShortNoteReleaseSampleFileFolderIfAny>Main\rel150</Samples_ShortNoteReleaseSampleFileFolderIfAny><Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny>150</Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny>";
        let (def, _v) = bouw_mini(&mini_orgel(velden), &d);
        let e = eerste_extra(&def);
        assert_eq!(e.releases.len(), 1);
        assert_eq!(e.releases[0].max_key_press_time_ms, Some(150));
        assert_eq!(e.cue_point, Some(40000), "primaire release uit het hoofdbestand vanaf de marker");
    }

    #[test]
    fn reverb_truncatie_wordt_gemeld_als_niet_beschikbaar() {
        let d = tijdmap("trunc");
        std::fs::create_dir_all(d.join("Main")).unwrap();
        for n in [60, 61] { schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[30000]); }
        let (_d, v) = bouw_mini(&mini_orgel("<ReverbTailTruncation_ModeCode>3</ReverbTailTruncation_ModeCode><ReverbTailTruncation_DecayLengthAsMsForMiddleCOn8FtStop>150</ReverbTailTruncation_DecayLengthAsMsForMiddleCOn8FtStop>"), &d);
        assert!(v.meldingen.iter().any(|m| m.tekst.contains("reverb tail truncation") && m.tekst.contains("modus 3")), "{:?}", v.meldingen);
        let (_d, v) = bouw_mini(&mini_orgel("<ReverbTailTruncation_ModeCode>1</ReverbTailTruncation_ModeCode>"), &d);
        assert!(!v.meldingen.iter().any(|m| m.tekst.contains("reverb tail")), "modus 1 is geen truncatie");
    }

    // ---- Bediening, combinaties en pistons (werkpakket F) ----

    /// Golden data: de koppelcode van elk echt koppel moet dezelfde richting,
    /// voetmaat en soort geven als zijn naam ("Sw to Gt 4'": toetsen Great, klinkt
    /// Swell, 4'). Zo vangt een fout in de code-ontleding meteen alle 25 Piteå- en
    /// 19 Litomyšl-koppels.
    #[test]
    fn echte_koppelcodes_komen_overeen_met_hun_naam() {
        let Some(map) = fixtures() else { return };
        for f in [
            "Les-Pitea895-945.CustomOrgan_Hauptwerk_xml",
            "Les-Litomysl2.CustomOrgan_Hauptwerk_xml",
            "Les-LitomyslWet24_2.CustomOrgan_Hauptwerk_xml",
        ] {
            let o = parse_codm(&std::fs::read_to_string(map.join(f)).unwrap()).unwrap();
            let afk = |a: &str| -> Option<u32> {
                let a = a.trim().to_lowercase();
                o.divisies.iter().find(|d| {
                    let k = d.korte_naam.to_lowercase();
                    let naam = d.naam.to_lowercase();
                    k == a || (a == "go" && k == "gt") || (a == "gt" && k == "go")
                        || (a == "hw" && naam.starts_with("haupt"))
                        || (a == "sw" && (naam.starts_with("schwel") || naam.starts_with("swell")))
                        || naam.starts_with(&a)
                }).map(|d| d.code)
            };
            let mut gecontroleerd = 0;
            for k in &o.koppels {
                let n = k.naam.replace('\'', " ");
                let lager = n.to_lowercase();
                if lager.contains("unison off") {
                    assert_eq!(k.soort, KoppelSoort::UnisonOff, "{f}: {}", k.naam);
                    assert_eq!(k.van_toetsen, k.klinkt, "{f}: {}", k.naam);
                } else if lager.contains("suboctave") {
                    assert_eq!((k.soort, k.sprong, k.van_toetsen == k.klinkt), (KoppelSoort::Zestien, -12, true), "{f}: {}", k.naam);
                } else if lager.trim().ends_with(" octave") {
                    assert_eq!((k.soort, k.sprong, k.van_toetsen == k.klinkt), (KoppelSoort::Vier, 12, true), "{f}: {}", k.naam);
                } else if lager.contains("melody") {
                    assert_eq!(k.soort, KoppelSoort::Melodie, "{f}: {}", k.naam);
                } else if lager.trim() == "bass" {
                    assert_eq!(k.soort, KoppelSoort::Bas, "{f}: {}", k.naam);
                } else if let Some((bron, rest)) = n.split_once(" to ") {
                    let mut delen = rest.split_whitespace();
                    let doel = delen.next().unwrap_or("");
                    let voet = delen.next().unwrap_or("");
                    // "X to Y n'": X klinkt, Y krijgt de toetsen.
                    assert_eq!(afk(bron), Some(k.klinkt), "{f}: bron van '{}'", k.naam);
                    assert_eq!(afk(doel), Some(k.van_toetsen), "{f}: doel van '{}'", k.naam);
                    let (soort, sprong) = match voet {
                        "16" => (KoppelSoort::Zestien, -12),
                        "8" => (KoppelSoort::Unisono, 0),
                        "4" => (KoppelSoort::Vier, 12),
                        v => panic!("{f}: onbekende voetmaat '{v}' in '{}'", k.naam),
                    };
                    // Een koppel op de eigen divisie heet Unisono-af, nooit 8'.
                    assert_eq!((k.soort, k.sprong), (soort, sprong), "{f}: '{}'", k.naam);
                } else {
                    panic!("{f}: koppelnaam '{}' niet te duiden", k.naam);
                }
                gecontroleerd += 1;
            }
            assert_eq!(gecontroleerd, o.koppels.len(), "{f}");
        }
    }

    #[test]
    fn echte_bronnen_schakelaarvelden_en_pistons() {
        let Some(map) = fixtures() else { return };
        let o = parse_codm(&std::fs::read_to_string(map.join("Les-Pitea895-945.CustomOrgan_Hauptwerk_xml")).unwrap()).unwrap();
        // 25 koppels, daarvan 16 buiten de divisional-combinaties, alle in de algemene.
        assert_eq!(o.koppels.len(), 25);
        assert_eq!(o.koppels.iter().filter(|k| !k.in_divisional).count(), 16);
        assert!(o.koppels.iter().all(|k| k.in_combinatie));
        assert!(o.stops.iter().all(|s| s.in_combinatie && !s.standaard_aan));
        assert!(o.koppels.iter().all(|k| !k.standaard_aan));
        // Twee reversibele pistons voor een koppel.
        assert_eq!(o.pistons.len(), 2);
        assert!(o.pistons.iter().all(|p| p.doel_soort == 2 && p.actie == 1));
        assert_eq!(o.pistons[0].label, "G/P");
        assert_eq!(o.pistons[0].doel_code, 1006);
    }

    /// Mini-orgel met één Zimbelstern-achtige stop (1 toets, 1 pijp) naast de
    /// gewone 2-toets Test 8'-stop: de 1-pijp-stop moet in `extra_bijgeluiden`
    /// belanden als Toy en NIET op het klavier verschijnen. De extra rank /
    /// stop / stoprank worden in de bestaande ObjectList-blokken ingevoegd —
    /// `extract_objects_als` leest alleen de eerste ObjectList per type.
    fn mini_met_effect_stop() -> String {
        mini_orgel("")
            .replace("</Rank></ObjectList>\n<ObjectList ObjectType=\"stop\">",
                     "</Rank><Rank><RankID>2</RankID><Name>Zimbelstern</Name><NumberOfPipes>1</NumberOfPipes><MIDINoteNumberOfFirstPipe>60</MIDINoteNumberOfFirstPipe><IsPercussive>Y</IsPercussive><Samples_InstallationPackageID>9</Samples_InstallationPackageID><Samples_MainSampleFileFolder>Main</Samples_MainSampleFileFolder><Samples_MIDINoteNumIncrementFromPipesToSamples>0</Samples_MIDINoteNumIncrementFromPipesToSamples><Pitch_RankBaseOutputPitch64ftHarmonicNum>8</Pitch_RankBaseOutputPitch64ftHarmonicNum></Rank></ObjectList>\n<ObjectList ObjectType=\"stop\">")
            .replace("</Stop></ObjectList>\n<ObjectList ObjectType=\"stoprank\">",
                     "</Stop><Stop><StopCode>2102</StopCode><Name>Gt: Zimbelstern</Name></Stop></ObjectList>\n<ObjectList ObjectType=\"stoprank\">")
            .replace("</StopRank></ObjectList>\n</Hauptwerk>",
                     "</StopRank><StopRank><StopCode>2102</StopCode><RankTypeCode>1</RankTypeCode><RankID>2</RankID><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank><MIDINoteNumOfFirstMappedDivisionKey>60</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>1</NumberOfMappedDivisionKeys><ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank></ObjectList>\n</Hauptwerk>")
    }

    #[test]
    fn eenpijp_stop_wordt_effectregister_in_bijgeluiden() {
        let d = tijdmap("effect");
        std::fs::create_dir_all(d.join("Main")).unwrap();
        for n in [60, 61] { schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[30000]); }
        let (def, v) = bouw_mini(&mini_met_effect_stop(), &d);
        // Alleen de 8' Test komt op het klavier; Zimbelstern is weggehaald.
        assert_eq!(def.stops.len(), 1);
        assert_eq!(def.stops[0].name, "Test 8'");
        // Zimbelstern komt als Toy in de Effectregisters-divisie.
        assert_eq!(def.extra_bijgeluiden.len(), 1);
        assert_eq!(def.extra_bijgeluiden[0].kind, crate::grandorgue::BackgroundSoundKind::Toy);
        assert_eq!(def.extra_bijgeluiden[0].name, "Zimbelstern");
        // Verslag vermeldt dat de stop als effectregister is herkend, met reden.
        assert!(v.meldingen.iter().any(|m|
            m.object == "stop 2102" && m.tekst.contains("effectregister") && m.tekst.contains("één pijp")
        ), "{:?}", v.meldingen);
    }

    fn mini_met_koppel_en_piston() -> String {
        mini_orgel("")
            .replace("<ObjectList ObjectType=\"rank\">",
                     "<ObjectList ObjectType=\"coupler\"><Coupler><CouplerCode>1107</CouplerCode><Name>Coupler: Gt to Gt 4'</Name><DefaultToEngaged>Y</DefaultToEngaged><StoreInGeneralCombinations>N</StoreInGeneralCombinations><StoreInDivisionalCombinations>N</StoreInDivisionalCombinations></Coupler></ObjectList>\n<ObjectList ObjectType=\"rank\">")
            .replace("</Hauptwerk>",
                     "<ObjectList ObjectType=\"shortcutpiston\"><ShortcutPiston><Name>Rev: 4'</Name><ReferencedObjectTypeCode>2</ReferencedObjectTypeCode><CouplerCode>1107</CouplerCode><PistonActionTypeCode>2</PistonActionTypeCode><CustomDisplay1_Text>4'</CustomDisplay1_Text></ShortcutPiston><ShortcutPiston><Name>Kapot</Name><ReferencedObjectTypeCode>1</ReferencedObjectTypeCode><StopCode>9999</StopCode><PistonActionTypeCode>1</PistonActionTypeCode></ShortcutPiston></ObjectList>\n</Hauptwerk>")
    }

    #[test]
    fn schakelaarregels_pistons_en_standaard_aan_komen_in_de_definitie() {
        let d = tijdmap("schakel");
        std::fs::create_dir_all(d.join("Main")).unwrap();
        for n in [60, 61] { schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[30000]); }
        let (def, v) = bouw_mini(&mini_met_koppel_en_piston(), &d);
        let r = &def.schakelaars;
        assert_eq!(def.couplers.len(), 1);
        assert_eq!(r.koppels_buiten_combinatie, vec![1]);
        assert_eq!(r.koppels_standaard_aan, vec![1]);
        assert!(r.stops_buiten_combinatie.is_empty() && r.stops_standaard_aan.is_empty());
        // Pistons komen niet meer als eigen lijst binnen: koppel/trem/register
        // uit de piston staat al bij de juiste divisie, dus een aparte pistonbalk
        // zou een duplicaat zijn. De piston met geldig doel verschijnt als
        // info-melding; de verwijzing naar een onbestaand register als
        // waarschuwing.
        assert!(r.pistons.is_empty());
        assert!(v.meldingen.iter().any(|m| m.object == "pistons" && m.tekst.contains("al als knop bij de juiste divisie")), "{:?}", v.meldingen);
        assert!(v.meldingen.iter().any(|m| m.object.contains("Kapot") && m.tekst.contains("doet niets")), "{:?}", v.meldingen);
        assert!(v.meldingen.iter().any(|m| m.tekst.contains("divisional-combinaties")), "{:?}", v.meldingen);
    }

    // ---- Integratie: XML -> verslag -> definitie -> preload -> offline stem -> release (werkpakket G) ----

    /// Schrijf een mono 16-bit sinus-WAV met optionele loop en cue (voor de integratieproef).
    fn schrijf_sinus(pad: &Path, freq: f32, secs: f32, loops: &[(u32, u32)], cues: &[u32]) {
        let frames = (secs * 48000.0) as u32;
        schrijf_wav(pad, frames, loops, cues);
        // Vul de datachunk (laatste `frames * 2` bytes) met de sinus.
        let mut b = std::fs::read(pad).unwrap();
        let start = b.len() - frames as usize * 2;
        for i in 0..frames as usize {
            let v = (0.5 * (2.0 * std::f32::consts::PI * freq * i as f32 / 48000.0).sin() * 32767.0) as i16;
            b[start + 2 * i..start + 2 * i + 2].copy_from_slice(&v.to_le_bytes());
        }
        std::fs::write(pad, b).unwrap();
    }

    /// Frequentie (Hz) uit de nuldoorgangen van een signaal bij 48 kHz.
    fn nuldoorgangen_hz(x: &[f32], rate: f32) -> f32 {
        let n = x.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        n as f32 / (x.len() as f32 / rate)
    }

    /// Eén deterministische proef door de hele importketen heen, zonder audio-apparaat:
    /// definitie lezen, verslag, samplekeuze (geleende opname), preload van de
    /// startbuffer, een offline stem met de toonhoogte-correctie van de pijp, en de
    /// release-keuze op toetsduur met het laden van die release.
    #[test]
    fn pijplijn_van_xml_tot_gerenderde_toon_en_release() {
        let d = tijdmap("pijplijn");
        for m in ["Main", "Main/rel150", "Main/rel900"] { std::fs::create_dir_all(d.join(m)).unwrap(); }
        // Alleen een opname voor noot 60; pijp 61 leent die.
        schrijf_sinus(&d.join("Main/060-x.wav"), 220.0, 1.0, &[(4800, 43200)], &[]);
        schrijf_sinus(&d.join("Main/rel150/060-x.wav"), 600.0, 0.15, &[], &[]);
        schrijf_sinus(&d.join("Main/rel900/060-x.wav"), 1300.0, 0.9, &[], &[]);
        let velden = r"<Samples_ShortNoteReleaseSampleFileFolderIfAny>Main\rel150</Samples_ShortNoteReleaseSampleFileFolderIfAny><Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny>150</Samples_MaxNoteTimeMsForShortNoteReleaseSamplesIfAny><Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles>Main\rel900</Samples_RelSampleFileFolderIfRelSamplesStoredInSeparateFiles>";
        let (def, v) = bouw_mini(&mini_orgel(velden), &d);

        // 1. verslag: één register verwacht, één geladen, niets ernstigs.
        assert_eq!((v.verwacht.stops, v.geladen.stops), (1, 1));
        assert!(!v.onvolledig && !v.niet_speelbaar);
        assert!(v.meldingen.iter().all(|m| m.ernst != Ernst::Fout), "{:?}", v.meldingen);

        // 2. samplekeuze: beide pijpen delen het bestand van noot 60; pijp 61 krijgt +100 ct.
        let extra = |i: usize| match &def.stops[0].pipes[i] { PipeDef::Sample { path, extra } => (path.clone(), extra.clone()), _ => panic!() };
        let (p60, e60) = extra(0);
        let (p61, e61) = extra(1);
        assert_eq!(p60, p61);
        assert!(e60.pitch_tuning_cents.unwrap_or(0.0).abs() < 1e-3);
        assert!((e61.pitch_tuning_cents.unwrap() - 100.0).abs() < 1e-3);

        // 3. preload van de startbuffer en een offline stem op de juiste toonhoogte.
        let pre = crate::loader::load_audio_preload_ms(&p61, 500).expect("preload");
        assert_eq!(pre.sample_rate, 48000);
        let ratio = 2f32.powf(e61.pitch_tuning_cents.unwrap() / 1200.0);
        let uit: Vec<f32> = (0..(0.3 * 48000.0) as usize)
            .map(|i| {
                let pos = i as f32 * ratio;
                let k = pos as usize;
                if k + 1 >= pre.attack_data.len() { return 0.0; }
                let fr = pos - k as f32;
                pre.attack_data[k] * (1.0 - fr) + pre.attack_data[k + 1] * fr
            })
            .collect();
        let f = nuldoorgangen_hz(&uit, 48000.0);
        let verwacht = 220.0 * 2f32.powf(1.0 / 12.0);
        assert!((f - verwacht).abs() / verwacht < 0.02, "gerenderde toon {f} Hz, verwacht {verwacht}");

        // 4. NoteOff: de release volgt de toetsduur; elke gekozen release laadt en klinkt op zijn eigen toon.
        let kies = |houd_ms: i32| e61.releases.iter()
            .find(|r| r.max_key_press_time_ms.is_some_and(|m| m >= houd_ms))
            .or_else(|| e61.releases.last())
            .unwrap();
        for (houd, verwacht_hz, bestand) in [(100, 600.0, "rel150"), (1000, 1300.0, "rel900")] {
            let rel = kies(houd);
            assert!(rel.path.to_string_lossy().contains(bestand), "houdtijd {houd} ms -> {:?}", rel.path);
            let seg = crate::loader::load_wav_preload_segment_ms(
                &rel.path, 80,
                crate::loader::PreloadSegment::Release { cue_override: rel.cue_point, require_cue: false },
            ).expect("release-preload");
            let hz = nuldoorgangen_hz(&seg.attack_data, seg.sample_rate as f32);
            assert!((hz - verwacht_hz).abs() / verwacht_hz < 0.03, "release bij {houd} ms: {hz} Hz, verwacht {verwacht_hz}");
        }
    }

    // ---- Tremulant, zwelkast en wind (werkpakket E) ----

    #[test]
    fn willekeurige_stemming_is_stabiel_begrensd_en_verschilt_per_pijp() {
        let doel = |n: u32| 100.0 * n as f32;
        // Stabiel: zelfde invoer, zelfde uitkomst.
        assert_eq!(willekeurige_stemming_cents(7, 60, 0.5, doel(60)), willekeurige_stemming_cents(7, 60, 0.5, doel(60)));
        let mut waarden = Vec::new();
        for n in 36..=96 {
            let c = willekeurige_stemming_cents(7, n, 0.5, doel(n));
            // Begrensd door het maximum in Hz bij de toonhoogte van de pijp.
            let f = 440.0 * 2f32.powf((doel(n) - 6900.0) / 1200.0);
            let max_cents = 1200.0 * ((f + 0.5) / f).log2().abs().max(1200.0 * ((f - 0.5).max(0.01) / f).log2().abs());
            assert!(c.abs() <= max_cents + 1e-3, "noot {n}: {c} > {max_cents}");
            waarden.push(c);
        }
        // Verschilt per pijp, beide tekens komen voor, gemiddeld klein (gewogen naar zuiver).
        assert!(waarden.iter().any(|c| *c > 0.0) && waarden.iter().any(|c| *c < 0.0));
        assert!(waarden.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-4));
        // Een andere rang geeft een andere reeks.
        assert_ne!(willekeurige_stemming_cents(8, 60, 0.5, doel(60)), willekeurige_stemming_cents(7, 60, 0.5, doel(60)));
        // Bij 0 Hz geen afwijking.
        assert!(willekeurige_stemming_cents(7, 60, 0.0, doel(60)).abs() < 1e-4);
    }

    #[test]
    fn pijpen_krijgen_de_willekeurige_stemming_alleen_als_de_rang_het_vraagt() {
        let d = tijdmap("random");
        std::fs::create_dir_all(d.join("Main")).unwrap();
        for n in [60, 61] { schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[30000]); }
        let (def, v) = bouw_mini(&mini_orgel("<Pitch_MaxRandomTuningErrorHz>0.5</Pitch_MaxRandomTuningErrorHz>"), &d);
        let a = match &def.stops[0].pipes[0] { PipeDef::Sample { extra, .. } => extra.random_tuning_cents, _ => None };
        let b = match &def.stops[0].pipes[1] { PipeDef::Sample { extra, .. } => extra.random_tuning_cents, _ => None };
        assert!(a.is_some() && b.is_some() && a != b, "{a:?} {b:?}");
        assert!(v.meldingen.iter().any(|m| m.tekst.contains("willekeurige stemafwijking")), "{:?}", v.meldingen);
        let (def, _) = bouw_mini(&mini_orgel("<Pitch_MaxRandomTuningErrorHz>0</Pitch_MaxRandomTuningErrorHz>"), &d);
        assert_eq!(eerste_extra(&def).random_tuning_cents, None);
    }

    #[test]
    fn golfvormtremulant_en_deelbereiken_worden_gemeld() {
        let d = tijdmap("trem");
        std::fs::create_dir_all(d.join("Main")).unwrap();
        for n in [60, 61] { schrijf_wav(&d.join("Main").join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[30000]); }
        let xml = mini_orgel("<Trem_TremulantCode>1720</Trem_TremulantCode><Trem_SampleInstallationPackageID>9</Trem_SampleInstallationPackageID><Trem_WaveformSampleFileFolder>Trem</Trem_WaveformSampleFileFolder><Encl_MIDINoteNumOfFirstEnclosedPipeIfWholeRankNotEnclosed>60</Encl_MIDINoteNumOfFirstEnclosedPipeIfWholeRankNotEnclosed><WindSupply_MaxFlowRandomisationPct>5</WindSupply_MaxFlowRandomisationPct>")
            .replace("</ObjectList>\n<ObjectList ObjectType=\"stop\">", "</ObjectList>\n<ObjectList ObjectType=\"tremulant\"><Tremulant><TremulantCode>1720</TremulantCode><Name>Tremulant: Gt</Name><FrequencyHz>5</FrequencyHz></Tremulant></ObjectList>\n<ObjectList ObjectType=\"stop\">");
        let (_def, v) = bouw_mini(&xml, &d);
        let tekst: Vec<&str> = v.meldingen.iter().map(|m| m.tekst.as_str()).collect();
        assert!(tekst.iter().any(|t| t.contains("golfvormtremulant") && t.contains("eenvoudige modulatie")), "{tekst:?}");
        assert!(tekst.iter().any(|t| t.contains("deel van hun pijpen in de zwelkast")), "{tekst:?}");
        assert!(tekst.iter().any(|t| t.contains("windstroom")), "{tekst:?}");
        // Het trembestandsmapje bestaat niet: dat wordt vermeld.
        assert!(tekst.iter().any(|t| t.contains("geen tremulantgolfbestanden")), "{tekst:?}");
    }

    #[test]
    fn stop_met_rangen_van_verschillende_zwelkast_wordt_gemeld() {
        let d = tijdmap("gemengd");
        for map in ["Main", "Main2"] { std::fs::create_dir_all(d.join(map)).unwrap(); }
        for n in [60, 61] {
            for map in ["Main", "Main2"] { schrijf_wav(&d.join(map).join(format!("{n:03}-x.wav")), 60000, &[(10000, 20000)], &[30000]); }
        }
        let xml = mini_orgel("<Encl_EnclosureCode>220</Encl_EnclosureCode>")
            .replace("</ObjectList>\n<ObjectList ObjectType=\"stop\">",
                     "<Rank><RankID>2</RankID><Name>Tweede</Name><NumberOfPipes>2</NumberOfPipes><MIDINoteNumberOfFirstPipe>60</MIDINoteNumberOfFirstPipe><IsPercussive>N</IsPercussive><Samples_InstallationPackageID>9</Samples_InstallationPackageID><Samples_MainSampleFileFolder>Main2</Samples_MainSampleFileFolder><Pitch_RankBaseOutputPitch64ftHarmonicNum>8</Pitch_RankBaseOutputPitch64ftHarmonicNum></Rank></ObjectList>\n<ObjectList ObjectType=\"enclosure\"><Enclosure><EnclosureCode>220</EnclosureCode><Name>SWELL</Name></Enclosure></ObjectList>\n<ObjectList ObjectType=\"stop\">")
            .replace("</StopRank></ObjectList>",
                     "</StopRank><StopRank><StopCode>2101</StopCode><RankTypeCode>1</RankTypeCode><RankID>2</RankID><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank><MIDINoteNumOfFirstMappedDivisionKey>60</MIDINoteNumOfFirstMappedDivisionKey><NumberOfMappedDivisionKeys>2</NumberOfMappedDivisionKeys><ActionTypeCode>1</ActionTypeCode><ActionEffectCode>1</ActionEffectCode></StopRank></ObjectList>");
        let (_def, v) = bouw_mini(&xml, &d);
        assert!(v.meldingen.iter().any(|m| m.object == "stop 2101" && m.tekst.contains("verschillende zwelkast")), "{:?}", v.meldingen);
    }

    // ---- Samplekeuze en toonhoogte (werkpakket C) ----

    #[test]
    fn nominale_toon_en_doeltoon() {
        let mut r = parse_codm(VOORBEELD).unwrap().ranks[&1].clone();
        // 8': doeltoon = noot, opname op haar eigen noot staat op die toon.
        r.harmonische = 8;
        r.sample_harmonische = None;
        r.samples_gestemd = false;
        assert_eq!(doeltoon_cents(&r, 60), 6000.0);
        assert_eq!(nominale_sample_cents(&r, 60), 6000.0);
        // Geleende opname (noot 61 voor pijp 60): transpositie -100 ct.
        assert_eq!(doeltoon_cents(&r, 60) - nominale_sample_cents(&r, 61), -100.0);
        // 16': doeltoon een octaaf lager; zonder aparte opnameharmonische staat de
        // opname op de toon van de pijp zelf.
        r.harmonische = 16;
        assert!((doeltoon_cents(&r, 60) - 7200.0).abs() < 1e-3);
        assert!((nominale_sample_cents(&r, 60) - 7200.0).abs() < 1e-3);
        // Opnamen gestemd aangenomen op 8' terwijl de rang als 16' klinkt: een octaaf transpositie.
        r.sample_harmonische = Some(8);
        r.samples_gestemd = true;
        assert!((doeltoon_cents(&r, 60) - nominale_sample_cents(&r, 60) - 1200.0).abs() < 1e-3);
        // Zonder 'gestemd aangenomen' is de basisharmonische van de opname niet van belang.
        r.samples_gestemd = false;
        assert!((doeltoon_cents(&r, 60) - nominale_sample_cents(&r, 60)).abs() < 1e-3);
    }

    /// De 37 chimes-toetsen delen één WAV (noot 61) maar krijgen elk de juiste
    /// halve-toonverschuiving, ook aan beide uiteinden (-13 en +23 halve tonen).
    #[test]
    fn chimes_een_wav_voor_37_toetsen_juiste_transpositie() {
        let Some(map) = fixtures() else { return };
        let mini = map.join("pitea-minimal");
        let odf = mini.join("OrganDefinitions/Les-Pitea895-945.CustomOrgan_Hauptwerk_xml");
        if !odf.is_file() {
            eprintln!("OVERGESLAGEN: testen/codm/pitea-minimal ontbreekt");
            return;
        }
        let xml = std::fs::read_to_string(&odf).unwrap();
        let def = laad_codm(&odf, &xml).expect("Chimes moet laden");
        let stop = &def.stops[0];
        let mut pijpen: Vec<(u32, PathBuf, f32, f32)> = Vec::new();
        for p in &stop.pipes {
            if let PipeDef::Sample { path, extra } = p {
                pijpen.push((extra.key_midi_note.unwrap(), path.clone(), extra.pitch_tuning_cents.unwrap_or(0.0), extra.keuze_sample_cents.unwrap()));
            }
        }
        assert_eq!(pijpen.len(), 37, "37 virtuele pijpen");
        // Eén gedeeld bestand, één nominale toon (noot 61 = 6100 ct, 8').
        assert!(pijpen.iter().all(|p| p.1 == pijpen[0].1));
        assert!(pijpen.iter().all(|p| (p.3 - 6100.0).abs() < 1e-3));
        // Pijp n klinkt 100·(n-61) ct boven de opname; stemming van de rang is 0.
        for (noot, _, tuning, _) in &pijpen {
            assert!((tuning - 100.0 * (*noot as f32 - 61.0)).abs() < 1e-3, "noot {noot}: {tuning}");
        }
        let eerste = pijpen.iter().min_by_key(|p| p.0).unwrap();
        let laatste = pijpen.iter().max_by_key(|p| p.0).unwrap();
        assert_eq!((eerste.0, laatste.0), (48, 84));
        assert!((eerste.2 + 1300.0).abs() < 1e-3 && (laatste.2 - 2300.0).abs() < 1e-3,
                "uiteinden {} / {}", eerste.2, laatste.2);
        // Opeenvolgende pijpen: precies een halve toon uit elkaar (frequentieverhouding 2^(1/12)).
        let mut gesorteerd = pijpen.clone();
        gesorteerd.sort_by_key(|p| p.0);
        for w in gesorteerd.windows(2) {
            assert!(((w[1].2 - w[0].2) - 100.0).abs() < 1e-3);
        }
    }

    #[test]
    fn samplemap_mag_niet_uit_het_pakket_ontsnappen() {
        assert!(map_blijft_in_pakket(r"Bourdon16\rel"));
        assert!(map_blijft_in_pakket(""));
        assert!(!map_blijft_in_pakket(r"..\..\buiten"));
        assert!(!map_blijft_in_pakket("a/../../b"));
        assert!(!map_blijft_in_pakket("/etc"));
        assert!(!map_blijft_in_pakket(r"C:\Windows"));
    }

    #[test]
    fn handmatige_pakketroot_wordt_gebruikt_en_gemeld() {
        let Some(map) = fixtures() else { return };
        let mini = map.join("pitea-minimal");
        let odf = map.join("Les-Pitea895-945.CustomOrgan_Hauptwerk_xml"); // buiten de pakketstructuur
        let xml = std::fs::read_to_string(&odf).unwrap();
        // Zonder root: geen pakketten gevonden → duidelijke fout over de map.
        let (_r, v) = laad_codm_met_verslag(&odf, &xml, None);
        assert!(v.pakketroot.is_none() || !v.pakketroot_handmatig);
        // Met gekozen root: 1 stop speelbaar en het verslag zegt dat de root handmatig was.
        let (r, v) = laad_codm_met_verslag(&odf, &xml, Some(&mini));
        assert!(r.is_ok());
        assert!(v.pakketroot_handmatig);
        assert_eq!(v.geladen.stops, 1);
    }
}
