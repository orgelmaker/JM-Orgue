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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportReport {
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
