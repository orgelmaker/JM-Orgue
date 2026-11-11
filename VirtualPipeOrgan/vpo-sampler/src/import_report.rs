//! Importverslag: wat een orgeldefinitie beloofde, wat er werkelijk geladen is
//! en waarom het verschil er is.
//!
//! Een logregel is geen verslag: de gebruiker moet vóór het vervangen van het
//! geladen orgel kunnen zien dat een essentiële klankafhankelijkheid ontbreekt,
//! en achteraf het verslag opnieuw kunnen openen. De UI toont dit type direct.

use serde::{Deserialize, Serialize};

/// Ernst van een melding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ernst {
    /// Iets dat de klank of bediening aantoonbaar vermindert of verandert.
    Fout,
    /// Mogelijk onverwacht, maar het orgel blijft bruikbaar.
    Waarschuwing,
    /// Alleen ter informatie (bijvoorbeeld een bewust niet ondersteund veld).
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Melding {
    pub ernst: Ernst,
    /// Object waar de melding over gaat ("rank 33", "stop 2014", "pakket 758").
    pub object: String,
    pub tekst: String,
}

impl Melding {
    pub fn nieuw(ernst: Ernst, object: impl Into<String>, tekst: impl Into<String>) -> Self {
        Melding { ernst, object: object.into(), tekst: tekst.into() }
    }
}

/// Rol van een installatiepakket. Een ontbrekend beeldpakket is geen ontbrekende klank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PakketRol {
    /// Hoofdsamples van rangen (aanslag, eigen release-mappen).
    Samples,
    /// Tremulant-golfvormen.
    Tremulant,
    /// Afbeeldingen van de grafische console.
    Beeld,
    /// Documentatie / orgelinformatie.
    Info,
    /// Standaardcomponenten of onbekend.
    Overig,
}

impl PakketRol {
    /// Essentieel voor de klank: zonder dit pakket klinkt (een deel van) het orgel niet.
    /// Tremulant-golfvormen horen daar bewust niet bij: zonder die bestanden klinkt het
    /// orgel nog wel (met een eenvoudige tremulant), dat is verminderde dekking en
    /// wordt als waarschuwing gemeld.
    pub fn is_klank(self) -> bool {
        matches!(self, PakketRol::Samples)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PakketStatus {
    pub id: u32,
    pub naam: String,
    /// Alle rollen waarin het pakket gebruikt wordt (een pakket kan beide zijn).
    pub rollen: Vec<PakketRol>,
    pub gevonden: bool,
    pub pad: Option<String>,
}

impl PakketStatus {
    /// Is dit pakket nodig voor de klank (niet alleen voor beeld/info)?
    pub fn klank_nodig(&self) -> bool {
        self.rollen.iter().any(|r| r.is_klank())
    }
}

/// Aantallen volgens de definitie, naast wat er werkelijk geladen is.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tellingen {
    pub divisies: usize,
    pub stops: usize,
    pub stopranks: usize,
    pub ranks: usize,
    pub koppels: usize,
    pub tremulanten: usize,
    pub zwelkasten: usize,
    pub pijpen: usize,
    pub releases: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvergeslagenStop {
    pub id: u32,
    pub naam: String,
    pub reden: String,
}

/// Een veld (`Tabel.Veld`) waarvan rijen een waarde hebben die de importer
/// niet gelezen heeft. Zo wordt zichtbaar wat een importer op echte sets mist.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OngelezenVeld {
    pub tabel: String,
    pub veld: String,
    /// Aantal rijen van de tabel waar dit veld een niet-lege waarde heeft.
    pub rijen: usize,
    /// Daarvan rijen waarin niets de waarde opvroeg.
    pub ongelezen: usize,
    /// Aantal verschillende waarden (afgekapt op [`MAX_VERSCHILLEND`]).
    pub verschillend: usize,
    /// Een paar verschillende voorbeeldwaarden.
    pub voorbeelden: Vec<String>,
}

/// Tabel in de definitie die de importer helemaal niet gelezen heeft.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnbekendeTabel {
    pub tabel: String,
    pub rijen: usize,
}

/// Bovengrens voor het bijhouden van verschillende waarden per veld.
pub const MAX_VERSCHILLEND: usize = 64;

/// Eén controlelijst van de diagnose (bijvoorbeeld "register zonder rangen").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiagnoseLijst {
    /// Korte vaste naam van de controle ("verwijzing-ontbreekt", "register-zonder-rangen", …).
    pub soort: String,
    /// Totaal aantal bevindingen; `items` kan korter zijn.
    pub totaal: usize,
    /// Aantal rijen of bestanden dat de controle werkelijk bekeken heeft. Nul
    /// bevindingen bij nul gecontroleerd betekent "niet te beoordelen", niet "in orde".
    #[serde(default)]
    pub gecontroleerd: usize,
    /// De eerste bevindingen als leesbare regels (afgekapt op [`MAX_ITEMS`]).
    pub items: Vec<String>,
}

/// Bovengrens voor het aantal regels per controlelijst.
pub const MAX_ITEMS: usize = 50;

impl DiagnoseLijst {
    pub fn nieuw(soort: impl Into<String>) -> Self {
        DiagnoseLijst { soort: soort.into(), ..Default::default() }
    }

    pub fn voeg_toe(&mut self, regel: impl Into<String>) {
        self.totaal += 1;
        if self.items.len() < MAX_ITEMS {
            self.items.push(regel.into());
        }
    }
}

/// Uitgebreide diagnose bij het laden. Alleen gevuld als de schakelaar
/// ([`crate::import_diagnose`]) aanstaat: dat kost laadtijd en geheugen.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Diagnose {
    pub ongelezen_velden: Vec<OngelezenVeld>,
    pub onbekende_tabellen: Vec<OnbekendeTabel>,
    pub lijsten: Vec<DiagnoseLijst>,
}

/// Een opgenomen tremulant-golfvorm van de set (C5, stap 1): alleen wat de definitie zegt.
/// JM-Orgue past dit nog niet toe; het geeft de tremulant een eenvoudige modulatie.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TremulantGolfvorm {
    pub tremulant_id: u32,
    pub tremulant_naam: String,
    /// Bestand met de golfvorm voor toonhoogte en grondtoon (één periode van de modulatie).
    pub grondtoon_bestand: String,
    /// Bestand met de golfvorm voor de derde harmonische.
    pub harmonische_bestand: String,
    /// Aantal pijpen met gegevens voor deze golfvorm.
    pub pijpen: usize,
    /// Mediaan over die pijpen: amplitudediepte (dB), toonhoogtediepte (procent) en de filterdiepte
    /// bij de derde harmonische (dB). `None` als geen enkele pijp de waarde noemt.
    pub amplitude_db: Option<f32>,
    pub toonhoogte_procent: Option<f32>,
    pub harmonisch_filter_db: Option<f32>,
}

/// Eén trap van de crescendo die de set zelf meebrengt (E4).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetTrap {
    pub naam: String,
    /// Stand van de regelaar (0 tot 127) waarbij deze trap begint.
    pub waarde: u32,
    /// De registers (stop-id's van de definitie) die op deze trap aan staan, oplopend.
    pub stops: Vec<u32>,
}

/// De crescendo die de set zelf meebrengt (E4): trappen van zacht naar vol. Alleen
/// registers; koppels en tremulanten van een trap worden niet overgenomen en geteld
/// in `niet_gelezen`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetCrescendo {
    pub naam: String,
    pub trappen: Vec<SetTrap>,
    /// Aantal verschillende schakelaars in de trappen die niet naar een register leiden
    /// (koppels, tremulanten, ...) of waarvan de beginstand omgekeerd is opgegeven.
    pub niet_gelezen: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportReport {
    /// De opgenomen tremulant-golfvormen van de set (C5, stap 1); leeg als de set er geen heeft of ze
    /// niet te lezen zijn.
    #[serde(default)]
    pub tremulant_golfvormen: Vec<TremulantGolfvorm>,
    /// De crescendo van de set, als de definitie er een heeft en we hem kunnen lezen.
    #[serde(default)]
    pub set_crescendo: Option<SetCrescendo>,
    /// "codm" | "hauptwerk" | "grandorgue" | "map"
    pub formaat: String,
    pub versie: String,
    pub bron: String,
    pub naam: String,
    /// Pakketroot die gebruikt is (of die handmatig gekozen werd).
    pub pakketroot: Option<String>,
    pub pakketroot_handmatig: bool,
    pub pakketten: Vec<PakketStatus>,
    pub verwacht: Tellingen,
    pub geladen: Tellingen,
    pub overgeslagen: Vec<OvergeslagenStop>,
    /// Samples in het beveiligde `.hbw`-formaat: herkend, niet te lezen.
    pub beschermd: usize,
    pub meldingen: Vec<Melding>,
    /// Een essentiële klankafhankelijkheid ontbreekt of een materieel deel van de stops laadt niet.
    pub onvolledig: bool,
    /// Geen enkele stop is speelbaar.
    pub niet_speelbaar: bool,
    /// Ongelezen velden, onbekende tabellen en controlelijsten; alleen met de
    /// diagnoseschakelaar aan (`JM_ORGUE_DIAGNOSE=1`).
    #[serde(default)]
    pub diagnose: Option<Diagnose>,
}

impl ImportReport {
    pub fn melding(&mut self, ernst: Ernst, object: impl Into<String>, tekst: impl Into<String>) {
        self.meldingen.push(Melding::nieuw(ernst, object, tekst));
    }

    pub fn ontbrekende_klankpakketten(&self) -> Vec<&PakketStatus> {
        self.pakketten.iter().filter(|p| !p.gevonden && p.klank_nodig()).collect()
    }

    pub fn aantal(&self, ernst: Ernst) -> usize {
        self.meldingen.iter().filter(|m| m.ernst == ernst).count()
    }

    /// Bepaal `onvolledig` / `niet_speelbaar` uit de rest van het verslag.
    ///
    /// Onvolledig = er ontbreekt een klankpakket, of er is een stop die door de
    /// definitie beloofd werd maar niet geladen kan worden. Een ontbrekend
    /// beeldpakket telt niet mee.
    pub fn bepaal_volledigheid(&mut self) {
        self.niet_speelbaar = self.geladen.stops == 0;
        self.onvolledig = !self.ontbrekende_klankpakketten().is_empty()
            || self.geladen.stops < self.verwacht.stops
            || self.beschermd > 0;
    }

    /// Eenvoudig verslag voor formaten die hun eigen volledigheid niet apart
    /// rapporteren (GrandOrgue, native Hauptwerk, map): alles wat de definitie
    /// bevat is geladen.
    pub fn uit_definitie(formaat: &str, versie: &str, bron: &str, naam: &str, stops: usize, divisies: usize, koppels: usize) -> Self {
        let t = Tellingen { divisies, stops, koppels, ..Tellingen::default() };
        ImportReport {
            formaat: formaat.into(),
            versie: versie.into(),
            bron: bron.into(),
            naam: naam.into(),
            verwacht: t.clone(),
            geladen: t,
            ..ImportReport::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pakket(id: u32, rollen: &[PakketRol], gevonden: bool) -> PakketStatus {
        PakketStatus { id, naam: format!("p{id}"), rollen: rollen.to_vec(), gevonden, pad: None }
    }

    #[test]
    fn ontbrekend_beeldpakket_is_geen_onvolledig_orgel() {
        let mut r = ImportReport::uit_definitie("codm", "3.00", "x", "x", 10, 2, 0);
        r.pakketten.push(pakket(1, &[PakketRol::Beeld], false));
        r.bepaal_volledigheid();
        assert!(!r.onvolledig);
        assert!(!r.niet_speelbaar);
    }

    #[test]
    fn ontbrekend_trempakket_blokkeert_niet() {
        let mut r = ImportReport::uit_definitie("codm", "3.00", "x", "x", 10, 2, 0);
        r.pakketten.push(pakket(2, &[PakketRol::Tremulant], false));
        r.bepaal_volledigheid();
        assert!(!r.onvolledig);
    }

    #[test]
    fn ontbrekend_klankpakket_is_onvolledig() {
        let mut r = ImportReport::uit_definitie("codm", "3.00", "x", "x", 10, 2, 0);
        r.pakketten.push(pakket(758, &[PakketRol::Samples], false));
        r.bepaal_volledigheid();
        assert!(r.onvolledig);
    }

    #[test]
    fn minder_stops_geladen_dan_beloofd_is_onvolledig() {
        let mut r = ImportReport::uit_definitie("codm", "3.00", "x", "x", 59, 4, 25);
        r.geladen.stops = 1;
        r.bepaal_volledigheid();
        assert!(r.onvolledig);
        assert!(!r.niet_speelbaar);
    }

    #[test]
    fn niets_geladen_is_niet_speelbaar() {
        let mut r = ImportReport::uit_definitie("codm", "3.00", "x", "x", 59, 4, 25);
        r.geladen.stops = 0;
        r.bepaal_volledigheid();
        assert!(r.niet_speelbaar && r.onvolledig);
    }
}
