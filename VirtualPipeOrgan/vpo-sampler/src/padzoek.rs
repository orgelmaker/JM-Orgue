//! Samplepaden hoofdletterongevoelig oplossen (E7).
//!
//! Sets worden op Windows gemaakt, waar hoofdletters in map- en bestandsnamen
//! niet uitmaken. De definitie noemt `Bourdon/R1/036-c.wav` terwijl de map op
//! schijf `bourdon/r1` heet: op Windows (en op de standaard-macOS-schijf)
//! werkt dat, op Linux laadt zo'n set geen enkele sample.
//!
//! Een [`PadZoeker`] lost een pad uit de definitie onderdeel voor onderdeel op
//! tegen wat er werkelijk op schijf staat. Van elke map wordt de inhoud één keer
//! gelezen en onthouden, omdat een set duizenden bestanden uit dezelfde paar
//! mappen vraagt. Eerst telt een exacte treffer; anders het ene onderdeel dat
//! op hoofdletters na gelijk is (bij meer dan één het eerste in alfabetische
//! volgorde, zodat de keuze niet van de volgorde van het bestandssysteem
//! afhangt). Is er niets, dan blijft het onderdeel zoals de definitie het
//! noemt: het pad bestaat dan niet, precies zoals zonder deze stap.
//!
//! Op Windows doet de zoeker niets: daar is het bestandssysteem al
//! hoofdletterongevoelig, en het resultaat is dan dezelfde samenvoeging als
//! voorheen.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

/// De inhoud van één map: de exacte namen en een opzoektabel op kleine letters.
struct MapLijst {
    exact: HashSet<OsString>,
    laag: HashMap<String, OsString>,
}

impl MapLijst {
    fn lees(map: &Path) -> Option<MapLijst> {
        let mut namen: Vec<OsString> = std::fs::read_dir(map)
            .ok()?
            .flatten()
            .map(|e| e.file_name())
            .collect();
        // Alfabetisch, zodat bij twee namen die op hoofdletters na gelijk zijn
        // altijd dezelfde wint.
        namen.sort();
        let mut laag: HashMap<String, OsString> = HashMap::with_capacity(namen.len());
        for n in &namen {
            laag.entry(n.to_string_lossy().to_lowercase()).or_insert_with(|| n.clone());
        }
        Some(MapLijst { exact: namen.into_iter().collect(), laag })
    }

    /// De echte naam voor `deel`: exact als die bestaat, anders de naam die op
    /// hoofdletters na gelijk is.
    fn zoek(&self, deel: &str) -> Option<OsString> {
        let os = OsString::from(deel);
        if self.exact.contains(&os) {
            return Some(os);
        }
        self.laag.get(&deel.to_lowercase()).cloned()
    }
}

/// Lost paden uit een definitie op tegen het bestandssysteem, met een geheugen
/// voor de maplijsten. Een zoeker hoort bij één importronde: bestanden die
/// daarna bijkomen, zou hij niet zien.
#[derive(Default)]
pub struct PadZoeker {
    lijsten: Mutex<HashMap<PathBuf, Option<Arc<MapLijst>>>>,
}

impl PadZoeker {
    pub fn nieuw() -> Self {
        Self::default()
    }

    /// `basis` + `relatief` (met `\` of `/` als scheiding), op Windows gewoon
    /// samengevoegd en elders hoofdletterongevoelig opgelost.
    pub fn los_op(&self, basis: &Path, relatief: &str) -> PathBuf {
        let genormaliseerd = relatief.replace('\\', "/");
        if cfg!(windows) {
            return basis.join(genormaliseerd);
        }
        self.los_op_op_schijf(basis, &genormaliseerd)
    }

    /// Het eigenlijke zoeken, los van het platform zodat het overal te testen is.
    fn los_op_op_schijf(&self, basis: &Path, relatief: &str) -> PathBuf {
        let mut huidig = basis.to_path_buf();
        let mut delen = relatief.split('/').filter(|d| !d.is_empty() && *d != ".");
        while let Some(deel) = delen.next() {
            if deel == ".." {
                huidig.push(deel);
                continue;
            }
            let echte = self.lijst(&huidig).and_then(|l| l.zoek(deel));
            match echte {
                Some(naam) => huidig.push(naam),
                None => {
                    // Niets gevonden: dit onderdeel en de rest blijven zoals
                    // in de definitie, het pad bestaat dan gewoon niet.
                    huidig.push(deel);
                    for rest in delen {
                        huidig.push(rest);
                    }
                    return huidig;
                }
            }
        }
        huidig
    }

    fn lijst(&self, map: &Path) -> Option<Arc<MapLijst>> {
        let mut lijsten = self.lijsten.lock();
        if let Some(l) = lijsten.get(map) {
            return l.clone();
        }
        let l = MapLijst::lees(map).map(Arc::new);
        lijsten.insert(map.to_path_buf(), l.clone());
        l
    }
}

/// De map `OrganInstallationPackages` in `root`, in de schrijfwijze die op
/// schijf staat (op Windows gewoon de samenvoeging).
pub fn pakketten_map(root: &Path) -> PathBuf {
    PadZoeker::nieuw().los_op(root, "OrganInstallationPackages")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Een tijdelijke map met `Bourdon/R1/036-c.wav` en `Montre/036-C.WAV`.
    fn proefmap(naam: &str) -> PathBuf {
        let basis = std::env::temp_dir().join(format!("jm_padzoek_{}_{}", naam, std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        std::fs::create_dir_all(basis.join("Bourdon").join("R1")).unwrap();
        std::fs::write(basis.join("Bourdon").join("R1").join("036-c.wav"), b"x").unwrap();
        std::fs::create_dir_all(basis.join("Montre")).unwrap();
        std::fs::write(basis.join("Montre").join("036-C.WAV"), b"x").unwrap();
        basis
    }

    fn als_tekst(p: &Path, basis: &Path) -> String {
        p.strip_prefix(basis).unwrap().to_string_lossy().replace('\\', "/")
    }

    #[test]
    fn andere_schrijfwijze_van_mappen_en_bestand_wordt_gevonden() {
        let basis = proefmap("schrijfwijze");
        let z = PadZoeker::nieuw();
        // Definitie noemt kleine letters, op schijf staan hoofdletters en omgekeerd.
        let p = z.los_op_op_schijf(&basis, "bourdon/r1/036-C.WAV");
        assert_eq!(als_tekst(&p, &basis), "Bourdon/R1/036-c.wav");
        let q = z.los_op_op_schijf(&basis, "MONTRE/036-c.wav");
        assert_eq!(als_tekst(&q, &basis), "Montre/036-C.WAV");
        assert!(p.exists() && q.exists());
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn pakkettenmap_in_andere_schrijfwijze_wordt_gevonden() {
        let basis = std::env::temp_dir().join(format!("jm_padzoek_pakketten_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        std::fs::create_dir_all(basis.join("organinstallationpackages").join("000690")).unwrap();
        let z = PadZoeker::nieuw();
        let p = z.los_op_op_schijf(&basis, "OrganInstallationPackages");
        assert_eq!(als_tekst(&p, &basis), "organinstallationpackages");
        assert!(p.is_dir());
        // Het hulpfunctie geeft op elk platform een map die bestaat.
        assert!(pakketten_map(&basis).is_dir());
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn exacte_schrijfwijze_blijft_zoals_ze_is() {
        let basis = proefmap("exact");
        let z = PadZoeker::nieuw();
        let p = z.los_op_op_schijf(&basis, "Bourdon/R1/036-c.wav");
        assert_eq!(als_tekst(&p, &basis), "Bourdon/R1/036-c.wav");
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn ontbrekend_onderdeel_laat_de_rest_zoals_in_de_definitie() {
        let basis = proefmap("ontbreekt");
        let z = PadZoeker::nieuw();
        // "Flute" bestaat niet: dat onderdeel en de rest blijven staan zoals ze zijn.
        let p = z.los_op_op_schijf(&basis, "bourdon/Flute/Sub/036-c.wav");
        assert_eq!(als_tekst(&p, &basis), "Bourdon/Flute/Sub/036-c.wav");
        assert!(!p.exists());
        // Bestaande map, ontbrekend bestand: map wordt wel rechtgezet.
        let q = z.los_op_op_schijf(&basis, "bourdon/r1/037-c.wav");
        assert_eq!(als_tekst(&q, &basis), "Bourdon/R1/037-c.wav");
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn niet_bestaande_basis_geeft_gewoon_de_samenvoeging() {
        let basis = std::env::temp_dir().join("jm_padzoek_bestaat_niet_xyz");
        let z = PadZoeker::nieuw();
        let p = z.los_op_op_schijf(&basis, "A/b/C.wav");
        assert_eq!(p, basis.join("A").join("b").join("C.wav"));
    }

    #[test]
    fn backslashes_punten_en_lege_delen_worden_genormaliseerd() {
        let basis = proefmap("normaal");
        let z = PadZoeker::nieuw();
        let p = z.los_op(&basis, ".\\bourdon\\\\r1\\.\\036-C.wav");
        // Op Windows telt de gewone samenvoeging; elders de opgeloste. Beide
        // wijzen naar hetzelfde bestand.
        assert!(p.exists(), "{p:?}");
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn twee_namen_die_op_hoofdletters_na_gelijk_zijn_kiezen_deterministisch() {
        // Op een hoofdletterongevoelig bestandssysteem kunnen we ze niet allebei
        // aanmaken; test de opzoektabel zelf.
        let namen = ["abc.wav", "ABC.wav", "Abc.wav"];
        let mut gesorteerd: Vec<OsString> = namen.iter().map(OsString::from).collect();
        gesorteerd.sort();
        let mut laag: HashMap<String, OsString> = HashMap::new();
        for n in &gesorteerd {
            laag.entry(n.to_string_lossy().to_lowercase()).or_insert_with(|| n.clone());
        }
        let lijst = MapLijst { exact: gesorteerd.iter().cloned().collect(), laag };
        // Exacte treffer wint altijd.
        assert_eq!(lijst.zoek("Abc.wav"), Some(OsString::from("Abc.wav")));
        // Anders de alfabetisch eerste ("ABC.wav" sorteert vóór "Abc.wav" en "abc.wav").
        assert_eq!(lijst.zoek("aBC.WAV"), Some(OsString::from("ABC.wav")));
        assert_eq!(lijst.zoek("onbekend.wav"), None);
    }

    #[test]
    fn de_maplijst_wordt_maar_een_keer_gelezen() {
        let basis = proefmap("geheugen");
        let z = PadZoeker::nieuw();
        let _ = z.los_op_op_schijf(&basis, "bourdon/r1/036-c.wav");
        // Een bestand dat erbij komt nadat de lijst is gelezen, ziet deze zoeker niet.
        std::fs::write(basis.join("Bourdon").join("R1").join("038-c.wav"), b"x").unwrap();
        let p = z.los_op_op_schijf(&basis, "bourdon/r1/038-C.wav");
        assert_eq!(als_tekst(&p, &basis), "Bourdon/R1/038-C.wav", "lijst uit het geheugen, dus niet gevonden");
        // Een nieuwe zoeker leest opnieuw en vindt het wel.
        let nieuw = PadZoeker::nieuw();
        let q = nieuw.los_op_op_schijf(&basis, "bourdon/r1/038-C.wav");
        assert_eq!(als_tekst(&q, &basis), "Bourdon/R1/038-c.wav");
        let _ = std::fs::remove_dir_all(&basis);
    }
}
