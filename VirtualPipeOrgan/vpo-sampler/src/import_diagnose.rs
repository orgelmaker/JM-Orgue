//! Diagnose bij het laden van een Hauptwerk-definitie (taak E8).
//!
//! Twee dingen die het gewone importverslag niet kan zeggen:
//!
//! 1. **Ongelezen velden.** Per `Tabel.Veld` hoeveel rijen een waarde hebben
//!    die niets heeft gelezen, met een paar voorbeeldwaarden. Dat maakt
//!    zichtbaar wat de importer op echte sets mist. De registratie zit in
//!    [`crate::hauptwerk::FieldMap`]: elke rij weet bij welke tabel ze hoort en
//!    meldt elke opgevraagde, niet-lege waarde hier.
//! 2. **Controlelijsten**: verwijzingen naar ontbrekende ID's, registers
//!    zonder rangen, ontbrekende samplebestanden, zwelkasten zonder pijpen,
//!    tremulanten met diepte nul en toonhoogtes buiten bereik; elk als eigen
//!    lijst (zie [`controles`]).
//!
//! De schakelaar staat standaard uit: bijhouden kost laadtijd en geheugen en
//! hoort niet in het gewone laadpad. Aanzetten kan met de omgevingsvariabele
//! `JM_ORGUE_DIAGNOSE=1` of met [`zet_aan`]. Onbekende tabellen leveren een
//! melding in het rapport op, nooit een harde fout.
//!
//! Niets hiervan draait in de audio-callback.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::grandorgue::{OrganDefinition, PipeDef, PipeExtra};
use crate::hauptwerk::{extract_objects, FieldMap, FieldMapExt};
use crate::hauptwerk_codm::extract_objects_als;
use crate::hauptwerk_compact::GecomprimeerdeOdf;
use crate::import_report::{Diagnose, DiagnoseLijst, OngelezenVeld, OnbekendeTabel, MAX_VERSCHILLEND};

/// Voorbeeldwaarden per veld die in het rapport komen.
const MAX_VOORBEELDEN: usize = 3;

/// Door [`zet_aan`] gezette schakelaar; de omgevingsvariabele werkt daarnaast.
static SCHAKELAAR: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// De lopende registratiesessie van déze thread. De importer leest een
    /// definitie op één thread; een sessie per thread houdt gelijktijdige
    /// laadacties (en tests) van elkaar gescheiden en kost geen lock.
    /// Zou het lezen ooit over meer threads verdeeld worden, dan blijven de
    /// leesacties op de andere threads onzichtbaar en lijkt alles ongelezen.
    static SESSIE: RefCell<Option<Sessie>> = const { RefCell::new(None) };
}

/// Zet de diagnose aan of uit voor de volgende laadacties (voor tests en
/// het gereedschap dat alle sets doorloopt).
pub fn zet_aan(aan: bool) {
    SCHAKELAAR.store(aan, Ordering::Relaxed);
}

/// Is de diagnoseschakelaar aan: via [`zet_aan`] of `JM_ORGUE_DIAGNOSE=1`?
pub fn schakelaar_aan() -> bool {
    SCHAKELAAR.load(Ordering::Relaxed)
        || std::env::var("JM_ORGUE_DIAGNOSE").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
}


#[derive(Default)]
struct VeldStat {
    met_waarde: usize,
    verschillend: HashSet<String>,
    voorbeelden: Vec<String>,
    /// Rijnummers waarvan dit veld gelezen is.
    gelezen: HashSet<u32>,
}

#[derive(Default)]
struct TabelStat {
    naam: String,
    velden: HashMap<String, VeldStat>,
}

#[derive(Default)]
struct Sessie {
    tabellen: Vec<TabelStat>,
    /// Tabelnaam in kleine letters → index in `tabellen`.
    index: HashMap<String, u32>,
}

/// Resultaat van een sessie.
#[derive(Debug, Default)]
pub struct Opbrengst {
    pub ongelezen_velden: Vec<OngelezenVeld>,
    /// Tabelnamen (kleine letters) die de importer heeft opgevraagd.
    pub gelezen_tabellen: HashSet<String>,
}

/// Begin een registratiesessie op deze thread. Een eerdere, niet afgesloten sessie vervalt.
pub fn start() {
    SESSIE.with(|s| *s.borrow_mut() = Some(Sessie::default()));
}

/// Sluit de sessie af en lever wat er gelezen is. Zonder sessie een lege opbrengst.
pub fn stop() -> Opbrengst {
    let Some(s) = SESSIE.with(|s| s.borrow_mut().take()) else { return Opbrengst::default() };
    let mut uit = Opbrengst::default();
    for t in &s.tabellen {
        uit.gelezen_tabellen.insert(t.naam.to_lowercase());
        for (veld, st) in &t.velden {
            let ongelezen = st.met_waarde.saturating_sub(st.gelezen.len());
            if ongelezen == 0 {
                continue;
            }
            let mut voorbeelden = st.voorbeelden.clone();
            voorbeelden.sort();
            uit.ongelezen_velden.push(OngelezenVeld {
                tabel: t.naam.clone(),
                veld: veld.clone(),
                rijen: st.met_waarde,
                ongelezen,
                verschillend: st.verschillend.len(),
                voorbeelden,
            });
        }
    }
    // Eerst de velden met de meeste ongelezen rijen; gelijke aantallen op naam.
    uit.ongelezen_velden
        .sort_by(|a, b| b.ongelezen.cmp(&a.ongelezen).then_with(|| a.tabel.cmp(&b.tabel)).then_with(|| a.veld.cmp(&b.veld)));
    uit
}

/// Meld een geëxtraheerde tabel aan en geef haar nummer terug. Dezelfde
/// tabel twee keer aanmelden (de importer leest sommige tabellen twee keer)
/// telt één keer: de tweede extractie hergebruikt het nummer en dezelfde rijnummers.
pub(crate) fn registreer(tabel: &str, rijen: &[HashMap<String, String>]) -> Option<u32> {
    SESSIE.with(|cel| {
        let mut guard = cel.borrow_mut();
        let s = guard.as_mut()?;
        let sleutel = tabel.to_lowercase();
        if let Some(&id) = s.index.get(&sleutel) {
            return Some(id);
        }
        let id = s.tabellen.len() as u32;
        let mut stat = TabelStat { naam: tabel.to_string(), velden: HashMap::new() };
        for rij in rijen {
            for (veld, waarde) in rij {
                if waarde.trim().is_empty() {
                    continue;
                }
                let v = stat.velden.entry(veld.clone()).or_default();
                v.met_waarde += 1;
                if v.verschillend.len() < MAX_VERSCHILLEND {
                    v.verschillend.insert(waarde.clone());
                }
                if v.voorbeelden.len() < MAX_VOORBEELDEN && !v.voorbeelden.contains(waarde) {
                    v.voorbeelden.push(afkappen(waarde));
                }
            }
        }
        s.tabellen.push(stat);
        s.index.insert(sleutel, id);
        Some(id)
    })
}

/// Een niet-lege waarde van `veld` in rij `rij` van tabel `tabel` is opgevraagd.
pub(crate) fn gelezen(tabel: u32, rij: u32, veld: &str) {
    SESSIE.with(|cel| {
        let mut guard = cel.borrow_mut();
        let Some(s) = guard.as_mut() else { return };
        if let Some(v) = s.tabellen.get_mut(tabel as usize).and_then(|t| t.velden.get_mut(veld)) {
            v.gelezen.insert(rij);
        }
    });
}

fn afkappen(w: &str) -> String {
    let w = w.trim();
    if w.chars().count() <= 60 {
        w.to_string()
    } else {
        let mut k: String = w.chars().take(57).collect();
        k.push_str("...");
        k
    }
}

/// Tabellen van de getekende console van de set (beeldsets, schermpagina's,
/// teksten). JM-Orgue tekent een eigen speeltafel; dat is een bewuste keuze,
/// dus deze tabellen horen niet in de lijst van wat de importer mist.
const GRAFISCHE_TABELLEN: &[&str] = &[
    "displaypage",
    "textstyle",
    "textinstance",
    "imageset",
    "imagesetelement",
    "imagesetinstance",
    "keyimageset",
    "continuouscontrolimagesetstage",
    "threepositionswitchimage",
];

/// Tabellen die in de XML staan maar die de importer nooit heeft opgevraagd.
/// `gelezen` = de tabelnamen (kleine letters) uit [`Opbrengst`]. Lege lijsten
/// tellen niet mee: daar valt niets te missen. De grafische consoletabellen
/// ([`GRAFISCHE_TABELLEN`]) blijven buiten de lijst.
pub fn onbekende_tabellen(xml: &str, gelezen: &HashSet<String>) -> Vec<OnbekendeTabel> {
    const OPEN: &str = "<ObjectList ObjectType=\"";
    let mut uit: Vec<OnbekendeTabel> = Vec::new();
    let mut pos = 0;
    while let Some(i) = xml[pos..].find(OPEN).map(|i| pos + i) {
        let naam_start = i + OPEN.len();
        let Some(naam_eind) = xml[naam_start..].find('"').map(|j| naam_start + j) else { break };
        let naam = &xml[naam_start..naam_eind];
        let body_start = xml[naam_eind..].find('>').map(|j| naam_eind + j + 1).unwrap_or(naam_eind);
        let body_eind = xml[body_start..].find("</ObjectList>").map(|j| body_start + j).unwrap_or(xml.len());
        pos = body_eind.max(naam_eind);
        let klein = naam.to_lowercase();
        if gelezen.contains(&klein) || GRAFISCHE_TABELLEN.contains(&klein.as_str()) {
            continue;
        }
        let rijen = tel_rijen(&xml[body_start..body_eind]);
        if rijen == 0 {
            continue;
        }
        // Dezelfde tabelnaam in twee schrijfwijzen of lijsten: samenvoegen.
        match uit.iter_mut().find(|t| t.tabel.eq_ignore_ascii_case(naam)) {
            Some(t) => t.rijen += rijen,
            None => uit.push(OnbekendeTabel { tabel: naam.to_string(), rijen }),
        }
    }
    uit.sort_by(|a, b| b.rijen.cmp(&a.rijen).then_with(|| a.tabel.cmp(&b.tabel)));
    uit
}

/// Aantal objecten in een lijstinhoud: het aantal sluittags van het eerste
/// kindelement (`<Stop>…</Stop>` of de compacte `<o>…</o>`).
fn tel_rijen(body: &str) -> usize {
    let Some(a) = body.find('<') else { return 0 };
    let Some(b) = body[a + 1..].find('>').map(|b| a + 1 + b) else { return 0 };
    let tag = body[a + 1..b].trim_end_matches('/');
    if tag.is_empty() || tag.starts_with('/') {
        return 0;
    }
    if body[a + 1..b].ends_with('/') {
        // Zelfsluitend kind: tel de gelijke tags.
        return body.matches(&format!("<{tag}/>")).count();
    }
    body.matches(&format!("</{tag}>")).count()
}

// ============================================================================
// Controlelijsten
// ============================================================================

/// Tabellen van de definitie, elk één keer gelezen en onthouden. Alleen voor
/// de controles: ze lezen onafhankelijk van de importer, na afloop van de
/// registratiesessie, en tellen dus niet mee als "gelezen".
struct Tabellen<'a> {
    xml: &'a str,
    compact: Option<GecomprimeerdeOdf>,
    cache: HashMap<String, Vec<FieldMap>>,
}

impl<'a> Tabellen<'a> {
    fn nieuw(xml: &'a str) -> Self {
        Tabellen { xml, compact: GecomprimeerdeOdf::detecteer(xml), cache: HashMap::new() }
    }

    /// De rijen van een tabel; leeg als de tabel ontbreekt. Een CODM schrijft
    /// de lijstnaam in kleine letters, vandaar de tweede poging.
    fn rijen(&mut self, naam: &str) -> &[FieldMap] {
        if !self.cache.contains_key(naam) {
            let rijen = match &self.compact {
                Some(c) => FieldMap::rijen_van_tabel(naam, c.objecten(self.xml, naam)),
                None => {
                    let v = extract_objects(self.xml, naam);
                    if v.is_empty() { extract_objects_als(self.xml, &naam.to_lowercase(), naam) } else { v }
                }
            };
            self.cache.insert(naam.to_string(), rijen);
        }
        &self.cache[naam]
    }

    /// Alle waarden van een numeriek veld, zonder dubbelen.
    fn ids(&mut self, tabel: &str, veld: &str) -> HashSet<u32> {
        self.rijen(tabel).iter().filter_map(|r| r.get_u32(veld)).collect()
    }

    /// Heeft de tabel rijen, en dragen die dit veld?
    fn heeft_veld(&mut self, tabel: &str, veld: &str) -> bool {
        self.rijen(tabel).iter().any(|r| r.get(veld).is_some())
    }
}

/// (tabel, veld) → (doeltabel, doelveld): de waarde moet als doelveld in de doeltabel staan.
const VERWIJZINGEN: &[(&str, &str, &str, &str)] = &[
    ("Stop", "DivisionID", "Division", "DivisionID"),
    ("StopRank", "StopID", "Stop", "StopID"),
    ("StopRank", "StopCode", "Stop", "StopCode"),
    ("StopRank", "RankID", "Rank", "RankID"),
    ("Pipe_SoundEngine01", "RankID", "Rank", "RankID"),
    ("Pipe_SoundEngine01_Layer", "PipeID", "Pipe_SoundEngine01", "PipeID"),
    ("Pipe_SoundEngine01_AttackSample", "LayerID", "Pipe_SoundEngine01_Layer", "LayerID"),
    ("Pipe_SoundEngine01_AttackSample", "SampleID", "Sample", "SampleID"),
    ("Pipe_SoundEngine01_ReleaseSample", "LayerID", "Pipe_SoundEngine01_Layer", "LayerID"),
    ("Pipe_SoundEngine01_ReleaseSample", "SampleID", "Sample", "SampleID"),
    ("EnclosurePipe", "EnclosureID", "Enclosure", "EnclosureID"),
    ("EnclosurePipe", "PipeID", "Pipe_SoundEngine01", "PipeID"),
];

/// Toonhoogtes (Hz) buiten dit bereik zijn geen orgeltoon: MIDI 0 is 8,2 Hz,
/// MIDI 127 is 12,5 kHz.
const HZ_MIN: f32 = 8.0;
const HZ_MAX: f32 = 14_000.0;

/// Verwijzingen naar ID's die niet bestaan, per verwijzing de ontbrekende waarden.
fn lijst_verwijzingen(t: &mut Tabellen) -> DiagnoseLijst {
    let mut lijst = DiagnoseLijst::nieuw("verwijzing-ontbreekt");
    for &(van, veld, naar, naar_veld) in VERWIJZINGEN {
        if !t.heeft_veld(van, veld) || !t.heeft_veld(naar, naar_veld) {
            continue;
        }
        let doel = t.ids(naar, naar_veld);
        let mut ontbrekend: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
        lijst.gecontroleerd += t.rijen(van).len();
        for r in t.rijen(van) {
            if let Some(id) = r.get_u32(veld).filter(|id| *id != 0) {
                if !doel.contains(&id) {
                    *ontbrekend.entry(id).or_insert(0) += 1;
                }
            }
        }
        for (id, aantal) in ontbrekend {
            lijst.voeg_toe(format!("{van}.{veld} = {id} bestaat niet in {naar}.{naar_veld} ({aantal} rij(en))"));
        }
    }
    lijst
}

/// Registers (Stop) zonder één rang (StopRank). Alleen te beoordelen als de
/// definitie StopRank-objecten heeft; anders komt de koppeling uit het
/// schakelnetwerk en klopt de vraag niet.
fn lijst_registers_zonder_rangen(t: &mut Tabellen) -> DiagnoseLijst {
    let mut lijst = DiagnoseLijst::nieuw("register-zonder-rangen");
    for sleutel in ["StopID", "StopCode"] {
        if !t.heeft_veld("Stop", sleutel) || !t.heeft_veld("StopRank", sleutel) {
            continue;
        }
        let met_rang = t.ids("StopRank", sleutel);
        lijst.gecontroleerd += t.rijen("Stop").len();
        for r in t.rijen("Stop") {
            if let Some(id) = r.get_u32(sleutel) {
                if !met_rang.contains(&id) {
                    let naam = r.get("Name").map(|n| n.as_str()).unwrap_or("");
                    lijst.voeg_toe(format!("Stop {id} '{naam}' heeft geen StopRank"));
                }
            }
        }
        break;
    }
    lijst
}

/// Zwelkasten waar geen pijp of rang aan hangt. Bij Hauptwerk via
/// `EnclosurePipe`, bij een CODM via `Encl_EnclosureCode` op de rang.
fn lijst_zwelkasten_zonder_kleppen(t: &mut Tabellen) -> DiagnoseLijst {
    let mut lijst = DiagnoseLijst::nieuw("zwelkast-zonder-pijpen");
    for (sleutel, bron_tabel, bron_veld) in [
        ("EnclosureID", "EnclosurePipe", "EnclosureID"),
        ("EnclosureCode", "Rank", "Encl_EnclosureCode"),
    ] {
        if !t.heeft_veld("Enclosure", sleutel) {
            continue;
        }
        let in_gebruik = t.ids(bron_tabel, bron_veld);
        lijst.gecontroleerd += t.rijen("Enclosure").len();
        for r in t.rijen("Enclosure") {
            if let Some(id) = r.get_u32(sleutel) {
                if !in_gebruik.contains(&id) {
                    let naam = r.get("Name").map(|n| n.as_str()).unwrap_or("");
                    lijst.voeg_toe(format!("Enclosure {id} '{naam}': niets in {bron_tabel}.{bron_veld} verwijst ernaar"));
                }
            }
        }
        break;
    }
    lijst
}

/// Een modulatie van de amplitude die zoveel dB (of meer) is gedempt, hoor je niet meer.
const STIL_DB: f32 = -60.0;

/// Tremulanten zonder modulatiediepte. De diepte staat niet in de tabel
/// `Tremulant` zelf maar per pijp in `TremulantWaveformPipe`
/// (`PitchModDepthAdjustPercent`, `AmplitudeModDepthAdjustDecibels`), met de
/// tabel `TremulantWaveform` als schakel naar de tremulant. Een tremulant telt
/// als diepte nul als al zijn pijpen een toonhoogtediepte van nul hebben én
/// een amplitudemodulatie van [`STIL_DB`] of stiller.
///
/// Een definitie waarvan die velden niet herkenbaar zijn (een gecomprimeerde
/// ODF noemt ze met letters, of de tabel ontbreekt) kan niet beoordeeld
/// worden: dat staat in een eigen lijst (`tremulant-diepte-niet-te-controleren`),
/// niet tussen de bevindingen.
fn lijst_tremulanten_diepte_nul(t: &mut Tabellen) -> (DiagnoseLijst, DiagnoseLijst) {
    let mut lijst = DiagnoseLijst::nieuw("tremulant-diepte-nul");
    let mut onbeoordeeld = DiagnoseLijst::nieuw("tremulant-diepte-niet-te-controleren");
    let aantal_rijen = t.rijen("Tremulant").len();
    let tremulanten: Vec<(u32, String)> = t
        .rijen("Tremulant")
        .iter()
        .filter_map(|r| {
            let id = r.get_u32("TremulantID")?;
            Some((id, r.get("Name").cloned().unwrap_or_default()))
        })
        .collect();
    if tremulanten.is_empty() {
        // Rijen die we niet kunnen identificeren (een gecomprimeerde ODF noemt
        // ook TremulantID met een letter) zijn niet te beoordelen.
        if aantal_rijen > 0 {
            onbeoordeeld.gecontroleerd = aantal_rijen;
            onbeoordeeld.voeg_toe(format!("{aantal_rijen} tremulant(en): velden van de tabel Tremulant niet herkend"));
        }
        return (lijst, onbeoordeeld);
    }
    let diepte_leesbaar = t.heeft_veld("TremulantWaveformPipe", "PitchModDepthAdjustPercent")
        || t.heeft_veld("TremulantWaveformPipe", "AmplitudeModDepthAdjustDecibels");
    if !diepte_leesbaar {
        onbeoordeeld.gecontroleerd = tremulanten.len();
        onbeoordeeld.voeg_toe(format!(
            "{} tremulant(en): geen modulatiediepte per pijp (TremulantWaveformPipe) herkend",
            tremulanten.len()
        ));
        return (lijst, onbeoordeeld);
    }
    let golf_naar_tremulant: HashMap<u32, u32> = t
        .rijen("TremulantWaveform")
        .iter()
        .filter_map(|r| Some((r.get_u32("TremulantWaveformID")?, r.get_u32("TremulantID")?)))
        .collect();
    // Per tremulant: (pijpen met een dieptewaarde, daarvan zonder diepte).
    let mut per_tremulant: HashMap<u32, (usize, usize)> = HashMap::new();
    for r in t.rijen("TremulantWaveformPipe") {
        let Some(trem) = r.get_u32("TremulantWaveformID").and_then(|w| golf_naar_tremulant.get(&w)) else { continue };
        let toonhoogte = r.get_f32("PitchModDepthAdjustPercent");
        let amplitude = r.get_f32("AmplitudeModDepthAdjustDecibels");
        if toonhoogte.is_none() && amplitude.is_none() {
            continue;
        }
        let e = per_tremulant.entry(*trem).or_default();
        e.0 += 1;
        if toonhoogte.unwrap_or(0.0) == 0.0 && amplitude.is_some_and(|a| a <= STIL_DB) {
            e.1 += 1;
        }
    }
    lijst.gecontroleerd = per_tremulant.values().map(|e| e.0).sum();
    for (id, naam) in tremulanten {
        if let Some(&(aantal, zonder)) = per_tremulant.get(&id) {
            if aantal > 0 && zonder == aantal {
                lijst.voeg_toe(format!("Tremulant {id} '{naam}': diepte nul bij al zijn {aantal} pijpen"));
            }
        }
    }
    (lijst, onbeoordeeld)
}

/// Toonhoogtes buiten bereik: MIDI-noten buiten 0..=127 en frequenties buiten [`HZ_MIN`, `HZ_MAX`].
fn lijst_toonhoogte_buiten_bereik(t: &mut Tabellen) -> DiagnoseLijst {
    let mut lijst = DiagnoseLijst::nieuw("toonhoogte-buiten-bereik");
    // Pijpen: toets en gemeten orgeltoon.
    let mut noot_fout: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut hz_fout = 0usize;
    let mut hz_voorbeeld: Option<(String, f32)> = None;
    lijst.gecontroleerd += t.rijen("Pipe_SoundEngine01").len();
    for r in t.rijen("Pipe_SoundEngine01") {
        if let Some(n) = r.get_u32("NormalMIDINoteNumber").filter(|n| *n > 127) {
            *noot_fout.entry(n).or_insert(0) += 1;
        }
        if let Some(hz) = r.get_f32("Pitch_OriginalOrgan_PitchHz").filter(|hz| *hz != 0.0 && !(HZ_MIN..=HZ_MAX).contains(hz)) {
            hz_fout += 1;
            hz_voorbeeld.get_or_insert((r.get("PipeID").cloned().unwrap_or_default(), hz));
        }
    }
    for (n, aantal) in noot_fout {
        lijst.voeg_toe(format!("Pipe_SoundEngine01.NormalMIDINoteNumber = {n} ligt buiten 0..127 ({aantal} pijp(en))"));
    }
    if let Some((pijp, hz)) = hz_voorbeeld {
        lijst.voeg_toe(format!("Pipe_SoundEngine01.Pitch_OriginalOrgan_PitchHz buiten {HZ_MIN}..{HZ_MAX} Hz bij {hz_fout} pijp(en), bv. pijp {pijp}: {hz} Hz"));
    }
    // Opnamen met een opgegeven toonhoogte (methode 4 = exact, in Hz).
    let mut sample_fout = 0usize;
    let mut sample_voorbeeld: Option<(String, f32)> = None;
    for r in t.rijen("Sample") {
        if r.get_u32("Pitch_SpecificationMethodCode") != Some(4) {
            continue;
        }
        lijst.gecontroleerd += 1;
        if let Some(hz) = r.get_f32("Pitch_ExactSamplePitch").filter(|hz| !(HZ_MIN..=HZ_MAX).contains(hz)) {
            sample_fout += 1;
            sample_voorbeeld.get_or_insert((r.get("SampleID").cloned().unwrap_or_default(), hz));
        }
    }
    if let Some((id, hz)) = sample_voorbeeld {
        lijst.voeg_toe(format!("Sample.Pitch_ExactSamplePitch buiten {HZ_MIN}..{HZ_MAX} Hz bij {sample_fout} opname(n), bv. sample {id}: {hz} Hz"));
    }
    // CODM: een rang loopt van de eerste pijp tot NumberOfPipes noten verder.
    for r in t.rijen("Rank") {
        if let (Some(eerste), Some(aantal)) = (r.get_u32("MIDINoteNumberOfFirstPipe"), r.get_u32("NumberOfPipes")) {
            lijst.gecontroleerd += 1;
            if aantal > 0 && eerste + aantal - 1 > 127 {
                let id = r.get("RankID").map(|s| s.as_str()).unwrap_or("?");
                lijst.voeg_toe(format!("Rank {id}: {aantal} pijpen vanaf MIDI-noot {eerste} lopen voorbij noot 127"));
            }
        }
    }
    lijst
}

/// Registers zonder één pijp en samplebestanden die op schijf ontbreken, uit
/// de gebouwde definitie.
fn lijsten_uit_definitie(def: &OrganDefinition) -> (DiagnoseLijst, DiagnoseLijst) {
    let mut zonder_pijpen = DiagnoseLijst::nieuw("register-zonder-pijpen");
    let mut ontbrekend = DiagnoseLijst::nieuw("samplebestand-ontbreekt");
    zonder_pijpen.gecontroleerd = def.stops.len();
    for s in &def.stops {
        let pijpen = s.pipes.iter().chain(s.layers.iter().flat_map(|l| l.pipes.iter()));
        let (mut aantal, mut weg) = (0usize, 0usize);
        let mut voorbeeld: Option<String> = None;
        for p in pijpen {
            if let PipeDef::Sample { path, extra } = p {
                aantal += 1;
                for pad in paden_van_pijp(path, extra) {
                    ontbrekend.gecontroleerd += 1;
                    if !pad.exists() {
                        weg += 1;
                        voorbeeld.get_or_insert_with(|| pad.to_string_lossy().to_string());
                    }
                }
            }
        }
        if aantal == 0 {
            zonder_pijpen.voeg_toe(format!("Register {} '{}' heeft geen enkele pijp met een opname", s.id, s.name));
        }
        if let Some(v) = voorbeeld {
            ontbrekend.voeg_toe(format!("Register {} '{}': {weg} bestand(en) ontbreken, bv. {v}", s.id, s.name));
        }
    }
    (zonder_pijpen, ontbrekend)
}

/// Alle bestanden die één pijp nodig heeft: aanslag, releases en de
/// tremulant- en droge varianten.
fn paden_van_pijp<'a>(path: &'a std::path::Path, extra: &'a PipeExtra) -> Vec<&'a std::path::Path> {
    let mut v: Vec<&std::path::Path> = vec![path];
    v.extend(extra.releases.iter().map(|r| r.path.as_path()));
    v.extend(extra.tremulant_attack.iter().map(|a| a.path.as_path()));
    v.extend(extra.tremulant_releases.iter().map(|r| r.path.as_path()));
    v.extend(extra.dry_attack.iter().map(|a| a.path.as_path()));
    v
}

/// Alle controlelijsten. Lege lijsten blijven in het rapport staan: "leeg"
/// is hier een resultaat. `def` is `None` als het laden mislukte.
pub fn controles(xml: &str, def: Option<&OrganDefinition>) -> Vec<DiagnoseLijst> {
    let mut t = Tabellen::nieuw(xml);
    let (trem_nul, trem_onbeoordeeld) = lijst_tremulanten_diepte_nul(&mut t);
    let mut uit = vec![
        lijst_verwijzingen(&mut t),
        lijst_registers_zonder_rangen(&mut t),
        lijst_zwelkasten_zonder_kleppen(&mut t),
        trem_nul,
        trem_onbeoordeeld,
        lijst_toonhoogte_buiten_bereik(&mut t),
    ];
    if let Some(def) = def {
        let (zonder_pijpen, ontbrekend) = lijsten_uit_definitie(def);
        uit.push(zonder_pijpen);
        uit.push(ontbrekend);
    }
    uit
}

/// Sluit de registratiesessie af en stel de diagnose samen voor het rapport.
pub fn rond_af(xml: &str, def: Option<&OrganDefinition>) -> Diagnose {
    let opbrengst = stop();
    let onbekende_tabellen = onbekende_tabellen(xml, &opbrengst.gelezen_tabellen);
    Diagnose { ongelezen_velden: opbrengst.ongelezen_velden, onbekende_tabellen, lijsten: controles(xml, def) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rij(kv: &[(&str, &str)]) -> HashMap<String, String> {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn ongelezen_waarde_komt_in_de_opbrengst_en_gelezen_niet() {
        start();
        let rijen = vec![rij(&[("A", "1"), ("B", "x")]), rij(&[("A", "2"), ("B", "x")]), rij(&[("A", ""), ("B", "y")])];
        let id = registreer("Stop", &rijen).expect("sessie actief");
        gelezen(id, 0, "A");
        gelezen(id, 1, "A");
        let o = stop();
        // A: twee rijen met waarde, beide gelezen → niet ongelezen. B: drie rijen, niets gelezen.
        assert!(o.ongelezen_velden.iter().all(|v| v.veld != "A"));
        let b = o.ongelezen_velden.iter().find(|v| v.veld == "B").expect("B ongelezen");
        assert_eq!((b.rijen, b.ongelezen, b.verschillend), (3, 3, 2));
        assert!(b.voorbeelden.contains(&"x".to_string()));
        assert!(o.gelezen_tabellen.contains("stop"));
    }

    #[test]
    fn deels_gelezen_telt_alleen_de_ongelezen_rijen() {
        start();
        let rijen = vec![rij(&[("A", "1")]), rij(&[("A", "2")]), rij(&[("A", "3")])];
        let id = registreer("Rank", &rijen).unwrap();
        gelezen(id, 1, "A");
        let o = stop();
        let a = o.ongelezen_velden.iter().find(|v| v.veld == "A").unwrap();
        assert_eq!((a.rijen, a.ongelezen), (3, 2));
    }

    #[test]
    fn dezelfde_tabel_twee_keer_aanmelden_telt_een_keer() {
        start();
        let rijen = vec![rij(&[("A", "1")]), rij(&[("A", "2")])];
        let a = registreer("Division", &rijen).unwrap();
        let b = registreer("division", &rijen).unwrap();
        assert_eq!(a, b);
        gelezen(b, 0, "A");
        gelezen(a, 1, "A");
        assert!(stop().ongelezen_velden.is_empty());
    }

    #[test]
    fn zonder_sessie_geen_registratie() {
        let _ = stop();
        assert!(registreer("Stop", &[rij(&[("A", "1")])]).is_none());
        gelezen(0, 0, "A"); // mag niet paniekeren
    }

    #[test]
    fn onbekende_tabellen_zijn_hoofdletterongevoelig_slaan_lege_en_beeldtabellen_over() {
        let xml = r#"<Hauptwerk>
<ObjectList ObjectType="Stop"><Stop><a>1</a></Stop><Stop><a>2</a></Stop></ObjectList>
<ObjectList ObjectType="TremulantWaveform"><TremulantWaveform><a>1</a></TremulantWaveform></ObjectList>
<ObjectList ObjectType="SwitchExclusiveSelectGroup"></ObjectList>
<ObjectList ObjectType="ImageSet"><o><a>1</a></o><o><a>2</a></o></ObjectList>
<ObjectList ObjectType="Pipe_X"><o><a>1</a></o><o><a>2</a></o><o><a>3</a></o></ObjectList>
</Hauptwerk>"#;
        let gelezen: HashSet<String> = ["stop".to_string()].into_iter().collect();
        let t = onbekende_tabellen(xml, &gelezen);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].tabel.as_str(), t[0].rijen), ("Pipe_X", 3));
        assert_eq!((t[1].tabel.as_str(), t[1].rijen), ("TremulantWaveform", 1));
    }
}
