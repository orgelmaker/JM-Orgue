//! Het windwerk van een Hauptwerk-set (D4, fase 0): windladen met hun balg, de verbindingen ertussen, en per pijp de lade
//! en het verbruik. Alleen lezen en beschrijven; de klank verandert hier niet.
//!
//! Begrippen (zoals de set ze geeft):
//! - **Lade** (`WindCompartment`): een ruimte met wind. Volume in m³, of oneindig (buitenlucht, de ventilator). Heeft hij een
//!   balg (`Bellows_HasBellows`), dan het oppervlak van het bovenblad (breedte × lengte), de maximale slag, de massa die de
//!   druk geeft, en een dempingscoëfficiënt.
//! - **Verbinding** (`WindCompartmentLinkage`): van de eerste naar de tweede lade, met de doorstroming (kg/s) bij een
//!   referentiedrukverschil (inch water), soms met een klep die een schakelaar bedient.
//! - **Pijp**: haalt haar wind uit een lade (`WindSupply_SourceWindCompartmentID`) en verbruikt zoveel kg/s bij een
//!   referentiedruk.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::hauptwerk::FieldMap;
use crate::hauptwerk::FieldMapExt as _;

/// Valversnelling (m/s²), luchtdruk op zeeniveau (Pa), adiabatische exponent van lucht, en 1 inch water in Pa.
const G: f32 = 9.81;
const P_ATM: f32 = 101_325.0;
const GAMMA: f32 = 1.4;
pub const PA_PER_INCH: f32 = 249.089;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BalgDef {
    pub breedte_m: f32,
    pub lengte_m: f32,
    pub slag_m: f32,
    pub massa_kg: f32,
    pub demping: f32,
}

impl BalgDef {
    pub fn oppervlak_m2(&self) -> f32 {
        self.breedte_m * self.lengte_m
    }

    /// Druk die het gewicht op de balg geeft (inch water), of None als oppervlak of massa ontbreken.
    pub fn druk_inch(&self) -> Option<f32> {
        let a = self.oppervlak_m2();
        (a > 0.0 && self.massa_kg > 0.0).then(|| self.massa_kg * G / a / PA_PER_INCH)
    }

    /// Eigenfrequentie (Hz) van de balg als massa op een luchtveer: k = γ·p·A²/V, f0 = √(k/m)/2π. `volume_m3` is het
    /// volume onder de balg (de lade zelf).
    pub fn eigenfrequentie_hz(&self, volume_m3: f32) -> Option<f32> {
        let a = self.oppervlak_m2();
        if a <= 0.0 || self.massa_kg <= 0.0 || volume_m3 <= 0.0 {
            return None;
        }
        let p = P_ATM + self.massa_kg * G / a;
        let k = GAMMA * p * a * a / volume_m3;
        Some((k / self.massa_kg).sqrt() / (2.0 * std::f32::consts::PI))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WindLadeDef {
    pub id: u32,
    pub naam: String,
    pub oneindig: bool,
    pub volume_m3: f32,
    pub druk_inch: Option<f32>,
    pub balg: Option<BalgDef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WindVerbindingDef {
    pub van: u32,
    pub naar: u32,
    pub naam: String,
    pub stroom_kg_s: f32,
    pub ref_inch: f32,
    /// Heeft een klep die een schakelaar of regelaar bedient (motor, balg leeg laten lopen).
    pub klep: bool,
}

/// Het windwerk van een set. Leeg (`laden` leeg) als de set niets geeft of niet leesbaar is.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WindwerkDef {
    pub laden: Vec<WindLadeDef>,
    pub verbindingen: Vec<WindVerbindingDef>,
}

fn getal(o: &FieldMap, veld: &str) -> Option<f32> {
    o.get(veld).and_then(|v| v.trim().replace(',', ".").parse::<f32>().ok())
}

fn ja(o: &FieldMap, veld: &str) -> bool {
    o.get(veld).is_some_and(|v| v.trim().eq_ignore_ascii_case("Y"))
}

impl WindwerkDef {
    /// Uit de tabellen `WindCompartment` en `WindCompartmentLinkage` van een leesbare definitie.
    pub(crate) fn uit_tabellen(laden: &[FieldMap], verbindingen: &[FieldMap]) -> WindwerkDef {
        let laden = laden.iter().filter_map(|o| {
            let id = o.get_u32("WindCompartmentID")?;
            let balg = ja(o, "Bellows_HasBellows").then(|| BalgDef {
                breedte_m: getal(o, "Bellows_FrameBaseWidthMetres").unwrap_or(0.0),
                lengte_m: getal(o, "Bellows_FrameBaseLengthMetres").unwrap_or(0.0),
                slag_m: getal(o, "Bellows_MaximumExtensionMetres").unwrap_or(0.0),
                massa_kg: getal(o, "Bellows_MassOfMovingBoardActedOnByGravityKg").unwrap_or(0.0),
                demping: getal(o, "Bellows_PositiveDampingCoefficient").unwrap_or(0.0),
            });
            Some(WindLadeDef {
                id,
                naam: o.get("Name").cloned().unwrap_or_default(),
                oneindig: ja(o, "InfiniteVolume"),
                volume_m3: getal(o, "StandardVolumeMetresCubed").unwrap_or(0.0),
                druk_inch: getal(o, "DefaultAirPressureInches").filter(|d| *d > 0.0),
                balg,
            })
        }).collect();
        let verbindingen = verbindingen.iter().filter_map(|o| {
            Some(WindVerbindingDef {
                van: o.get_u32("FirstWindCompartmentID")?,
                naar: o.get_u32("SecondWindCompartmentID")?,
                naam: o.get("Name").cloned().unwrap_or_default(),
                stroom_kg_s: getal(o, "MassFlowRateKilogramsPerSecAtReferencePressureDiff").unwrap_or(0.0),
                ref_inch: getal(o, "ReferencePressureDifferenceInches").unwrap_or(1.0),
                // Code 1 = altijd open; 2 = schakelaar; 3 = regelaar.
                klep: o.get("ValveControlTypeCode").is_some_and(|c| matches!(c.trim(), "2" | "3")),
            })
        }).collect();
        WindwerkDef { laden, verbindingen }
    }

    pub fn is_leeg(&self) -> bool {
        self.laden.is_empty()
    }

    pub fn lade(&self, id: u32) -> Option<&WindLadeDef> {
        self.laden.iter().find(|l| l.id == id)
    }

    /// De balg die een lade voedt: de lade zelf als die een balg heeft, anders de dichtstbijzijnde lade met een balg
    /// stroomopwaarts, langs verbindingen zonder klep. None als er geen is (dan voedt de ventilator direct).
    pub fn balg_van_lade(&self, lade: u32) -> Option<u32> {
        let mut gezien = HashSet::new();
        let mut q = VecDeque::from([lade]);
        while let Some(x) = q.pop_front() {
            if !gezien.insert(x) {
                continue;
            }
            if self.lade(x).is_some_and(|l| l.balg.is_some()) {
                return Some(x);
            }
            for v in self.verbindingen.iter().filter(|v| v.naar == x && !v.klep) {
                q.push_back(v.van);
            }
        }
        None
    }

    /// Alle balgen, met hun eigenfrequentie en druk.
    pub fn balgen(&self) -> Vec<(&WindLadeDef, &BalgDef)> {
        self.laden.iter().filter_map(|l| l.balg.as_ref().map(|b| (l, b))).collect()
    }

    /// Hoeveel kg/s de verbindingen zonder klep een balg kunnen toevoeren bij hun referentiedruk (de som van de
    /// inkomende verbindingen).
    pub fn toevoer_kg_s(&self, balg: u32) -> f32 {
        self.verbindingen.iter().filter(|v| v.naar == balg).map(|v| v.stroom_kg_s).sum()
    }

    /// Eén regel voor het importverslag.
    pub fn samenvatting(&self, pijpen_met_verbruik: usize, divisies_per_balg: &HashMap<u32, Vec<String>>) -> String {
        let balgen = self.balgen();
        let mut delen: Vec<String> = Vec::new();
        for (l, b) in &balgen {
            let f0 = b.eigenfrequentie_hz(l.volume_m3).map(|f| format!("{:.1} Hz", f)).unwrap_or_else(|| "?".into());
            let druk = b.druk_inch().map(|d| format!("{:.1}\"", d)).unwrap_or_else(|| "?".into());
            let divs = divisies_per_balg.get(&l.id).map(|d| d.join(", ")).unwrap_or_default();
            delen.push(format!("{} ({}, {}{})", l.naam.trim(), f0, druk, if divs.is_empty() { String::new() } else { format!("; {}", divs) }));
        }
        format!(
            "Windwerk van de set: {} balg(en), {} laden, {} verbindingen, {} pijpen met verbruik. Balgen: {}.",
            balgen.len(),
            self.laden.iter().filter(|l| !l.oneindig).count(),
            self.verbindingen.len(),
            pijpen_met_verbruik,
            if delen.is_empty() { "geen".to_string() } else { delen.join("; ") }
        )
    }
}

/// Fase 1 (D4): hoeveel windgroepen het windwerk van de set mag krijgen. Het scherm kent er 8; heeft een set meer
/// balgen (plus werken zonder balg), dan blijft de gewone indeling.
pub const MAX_SET_GROEPEN: usize = 8;
/// De standaardstand van de knoppen balggrootte en maximale daling (`WindInstelling::default`): de middenstand die
/// voortaan de waarde van de set betekent.
pub const STANDAARD_BALG: f32 = 0.5;
pub const STANDAARD_DALING: f32 = 0.05;
/// Eigenfrequenties buiten deze band (Hz) zijn geen echte balg (een set met 62 Hz heeft kennelijk een paar kilo op een
/// paar dm²): dan blijven balggrootte en daling op de standaard.
const F0_REDELIJK: (f32, f32) = (1.0, 20.0);
/// Grenzen van de factor op de maximale daling: de set mag de knop dempen of versterken, niet uitzetten of
/// onbegrensd maken.
const DALING_FACTOR: (f32, f32) = (0.2, 3.0);

/// Eén pijp voor de afleiding: haar sleutel in de engine (stop, pijp), de divisie waarin ze speelt, de lade en het
/// verbruik uit de set, en de schatting van JM-Orgue (8'-C-eenheden, `verbruik_van_pijp`).
#[derive(Debug, Clone, PartialEq)]
pub struct PijpWindIn {
    pub sleutel: (u32, u32),
    pub divisie: usize,
    pub lade: Option<u32>,
    pub kg_s: Option<f32>,
    pub schatting: f32,
}

/// Wat de set voor één windgroep geeft.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroepAfleiding {
    /// De balg (lade-id) van deze groep; None = een werk zonder balg (houdt een eigen groep).
    pub balg: Option<u32>,
    pub naam: String,
    pub divisies: Vec<usize>,
    /// Eigenfrequentie van de balg (Hz), en of die aannemelijk is.
    pub f0_hz: Option<f32>,
    pub redelijk: bool,
    /// Factor op de knop balggrootte: de middenstand (0,5) wordt 3/f0 (begrensd 0,1–2,0).
    pub balg_factor: f32,
    /// Daling van de set als alle pijpen van de balg tegelijk spreken (fractie van de druk), en de factor op de knop
    /// maximale daling (de middenstand 5 % wordt die daling, begrensd).
    pub daling_set: Option<f32>,
    pub daling_factor: f32,
    /// kg/s → 8'-C-eenheden voor de pijpen van deze groep.
    pub schaal: f32,
}

/// Het resultaat van fase 1: per divisie de windgroep, per groep de afleiding, en per pijp het verbruik.
#[derive(Debug, Clone, PartialEq)]
pub struct WindwerkAfleiding {
    pub groep_van_divisie: Vec<u8>,
    pub groepen: Vec<GroepAfleiding>,
    pub verbruik: HashMap<(u32, u32), f32>,
}

impl WindwerkAfleiding {
    /// Eén regel per groep voor het verslag en het telprogramma.
    pub fn regels(&self, divisienamen: &[String]) -> Vec<String> {
        self.groepen.iter().enumerate().map(|(i, g)| {
            let divs: Vec<&str> = g.divisies.iter().filter_map(|d| divisienamen.get(*d).map(|s| s.as_str())).collect();
            let f0 = g.f0_hz.map(|f| format!("{:.1} Hz{}", f, if g.redelijk { "" } else { " (onaannemelijk: standaard)" }))
                .unwrap_or_else(|| "geen balg".into());
            let daling = g.daling_set.map(|d| format!("{:.1} %", d * 100.0)).unwrap_or_else(|| "?".into());
            format!(
                "windgroep {} '{}' ({}): f0 {}, balggrootte x{:.2}; daling bij alle pijpen {} -> maximale daling x{:.2}",
                i + 1, g.naam, divs.join(", "), f0, g.balg_factor, daling, g.daling_factor
            )
        }).collect()
    }
}

impl WindwerkDef {
    /// Referentiedruk (inch) van de toevoer naar een balg: het stroomgewogen gemiddelde van de inkomende verbindingen.
    fn toevoer_ref_inch(&self, balg: u32) -> Option<f32> {
        let (mut q, mut qr) = (0.0f32, 0.0f32);
        for v in self.verbindingen.iter().filter(|v| v.naar == balg && v.stroom_kg_s > 0.0) {
            q += v.stroom_kg_s;
            qr += v.stroom_kg_s * v.ref_inch;
        }
        (q > 0.0 && qr > 0.0).then(|| qr / q)
    }

    /// Fase 1 (D4): het bestaande windmodel sturen met de set.
    ///
    /// - Elke divisie krijgt de balg van de meeste van haar pijpen; elke gebruikte balg is één windgroep (in de
    ///   volgorde van de divisies), een divisie zonder balg krijgt een eigen groep. Geen enkele pijp aan een balg, of
    ///   meer dan [`MAX_SET_GROEPEN`] groepen: None (de gewone indeling blijft).
    /// - Verbruik per pijp uit de set, per groep zo geschaald dat de som over de pijpen van de groep gelijk is aan de
    ///   som van de schattingen: de verdeling komt uit de set, de diepte blijft bij de knoppen.
    /// - Per groep factoren op balggrootte (uit de eigenfrequentie) en maximale daling (uit verbruik tegenover toevoer).
    pub fn afleiden(&self, pijpen: &[PijpWindIn], n_divisies: usize) -> Option<WindwerkAfleiding> {
        let mut telling: Vec<HashMap<u32, usize>> = vec![HashMap::new(); n_divisies];
        let mut q_balg: HashMap<u32, f32> = HashMap::new();
        let mut balg_cache: HashMap<u32, Option<u32>> = HashMap::new();
        for p in pijpen {
            let Some(lade) = p.lade else { continue };
            let Some(b) = *balg_cache.entry(lade).or_insert_with(|| self.balg_van_lade(lade)) else { continue };
            if let Some(t) = telling.get_mut(p.divisie) {
                *t.entry(b).or_default() += 1;
            }
            if let Some(q) = p.kg_s {
                *q_balg.entry(b).or_default() += q;
            }
        }
        // Meeste pijpen; bij gelijkspel de laagste id (vast resultaat).
        let balg_van_div: Vec<Option<u32>> = telling.iter()
            .map(|t| t.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(b, _)| *b))
            .collect();
        if balg_van_div.iter().all(|b| b.is_none()) {
            return None;
        }
        let mut groepen: Vec<GroepAfleiding> = Vec::new();
        let mut groep_van_divisie = vec![0u8; n_divisies];
        for (d, b) in balg_van_div.iter().enumerate() {
            if let Some(i) = b.and_then(|b| groepen.iter().position(|g| g.balg == Some(b))) {
                groepen[i].divisies.push(d);
                groep_van_divisie[d] = i as u8;
                continue;
            }
            groep_van_divisie[d] = groepen.len().min(255) as u8;
            groepen.push(self.groep_voor(*b, d, b.and_then(|b| q_balg.get(&b).copied())));
        }
        if groepen.len() > MAX_SET_GROEPEN {
            return None;
        }
        // Schaal per groep: som van de schattingen / som van de setwaarden, over de pijpen met een setwaarde.
        let mut som = vec![(0.0f32, 0.0f32); groepen.len()];
        for p in pijpen {
            let (Some(q), Some(&g)) = (p.kg_s, groep_van_divisie.get(p.divisie)) else { continue };
            som[g as usize].0 += p.schatting;
            som[g as usize].1 += q;
        }
        for (g, (s_est, s_set)) in groepen.iter_mut().zip(&som) {
            g.schaal = if *s_set > 0.0 && *s_est > 0.0 { s_est / s_set } else { 0.0 };
        }
        let mut verbruik = HashMap::new();
        for p in pijpen {
            let (Some(q), Some(&g)) = (p.kg_s, groep_van_divisie.get(p.divisie)) else { continue };
            let s = groepen[g as usize].schaal;
            if s > 0.0 {
                verbruik.insert(p.sleutel, q * s);
            }
        }
        Some(WindwerkAfleiding { groep_van_divisie, groepen, verbruik })
    }

    fn groep_voor(&self, balg: Option<u32>, divisie: usize, q_alle: Option<f32>) -> GroepAfleiding {
        let mut g = GroepAfleiding {
            balg, naam: String::new(), divisies: vec![divisie], f0_hz: None, redelijk: false,
            balg_factor: 1.0, daling_set: None, daling_factor: 1.0, schaal: 0.0,
        };
        let Some((lade, b)) = balg.and_then(|id| self.lade(id)).and_then(|l| l.balg.as_ref().map(|b| (l, b))) else {
            return g;
        };
        g.naam = lade.naam.trim().to_string();
        g.f0_hz = b.eigenfrequentie_hz(lade.volume_m3);
        g.redelijk = g.f0_hz.is_some_and(|f| (F0_REDELIJK.0..=F0_REDELIJK.1).contains(&f));
        if !g.redelijk {
            return g;
        }
        // Het model rekent f0 = 3/balggrootte.
        let f0 = g.f0_hz.unwrap_or(6.0);
        g.balg_factor = (3.0 / f0).clamp(0.1, 2.0) / STANDAARD_BALG;
        // Drukval over de toevoer ∝ (Q/Q_ref)²: bij alle pijpen tegelijk ref·(Q/toevoer)², als deel van de balgdruk.
        let toevoer = self.toevoer_kg_s(lade.id);
        let druk = b.druk_inch().or(lade.druk_inch);
        if let (Some(q), Some(r), Some(p)) = (q_alle, self.toevoer_ref_inch(lade.id), druk) {
            if toevoer > 0.0 && p > 0.0 && q > 0.0 {
                let daling = r * (q / toevoer).powi(2) / p;
                g.daling_set = Some(daling);
                g.daling_factor = (daling / STANDAARD_DALING).clamp(DALING_FACTOR.0, DALING_FACTOR.1);
            }
        }
        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fm(kv: &[(&str, &str)]) -> FieldMap {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn voorbeeld() -> WindwerkDef {
        // Ventilator (oneindig) → magazijn met balg → kanaal → lade C; een lek naar buiten; een klep met schakelaar.
        let laden = vec![
            fm(&[("WindCompartmentID", "1"), ("Name", "Open air"), ("InfiniteVolume", "Y")]),
            fm(&[("WindCompartmentID", "2"), ("Name", "Ventilator"), ("InfiniteVolume", "Y"), ("DefaultAirPressureInches", "4")]),
            fm(&[("WindCompartmentID", "101"), ("Name", "M P"), ("StandardVolumeMetresCubed", "0.8"), ("Bellows_HasBellows", "Y"),
                 ("Bellows_FrameBaseWidthMetres", "1.3"), ("Bellows_FrameBaseLengthMetres", "1.4"), ("Bellows_MaximumExtensionMetres", "0.4"),
                 ("Bellows_MassOfMovingBoardActedOnByGravityKg", "280"), ("Bellows_PositiveDampingCoefficient", "11")]),
            fm(&[("WindCompartmentID", "202"), ("Name", "Kanaal"), ("StandardVolumeMetresCubed", "1.3")]),
            fm(&[("WindCompartmentID", "201"), ("Name", "Lade C"), ("StandardVolumeMetresCubed", "1.2")]),
        ];
        let verb = vec![
            fm(&[("FirstWindCompartmentID", "2"), ("SecondWindCompartmentID", "101"), ("MassFlowRateKilogramsPerSecAtReferencePressureDiff", "2"), ("ValveControlTypeCode", "2")]),
            fm(&[("FirstWindCompartmentID", "101"), ("SecondWindCompartmentID", "202"), ("MassFlowRateKilogramsPerSecAtReferencePressureDiff", "1"), ("ValveControlTypeCode", "1")]),
            fm(&[("FirstWindCompartmentID", "202"), ("SecondWindCompartmentID", "201"), ("MassFlowRateKilogramsPerSecAtReferencePressureDiff", "1"), ("ValveControlTypeCode", "1")]),
            fm(&[("FirstWindCompartmentID", "201"), ("SecondWindCompartmentID", "1"), ("MassFlowRateKilogramsPerSecAtReferencePressureDiff", "0.001"), ("ValveControlTypeCode", "1")]),
        ];
        WindwerkDef::uit_tabellen(&laden, &verb)
    }

    #[test]
    fn leest_laden_balgen_en_verbindingen() {
        let w = voorbeeld();
        assert_eq!(w.laden.len(), 5);
        assert_eq!(w.balgen().len(), 1);
        let b = w.lade(101).and_then(|l| l.balg.clone()).expect("balg");
        assert!((b.oppervlak_m2() - 1.82).abs() < 1e-4);
        assert_eq!(w.verbindingen.iter().filter(|v| v.klep).count(), 1);
        assert_eq!(w.lade(2).and_then(|l| l.druk_inch), Some(4.0));
    }

    #[test]
    fn lade_vindt_haar_balg_stroomopwaarts_en_niet_door_een_klep() {
        let w = voorbeeld();
        assert_eq!(w.balg_van_lade(201), Some(101), "lade ← kanaal ← magazijn");
        assert_eq!(w.balg_van_lade(101), Some(101), "een balg voedt zichzelf");
        assert_eq!(w.balg_van_lade(2), None, "de ventilator heeft geen balg");
        assert_eq!(w.toevoer_kg_s(101), 2.0);
    }

    #[test]
    fn eigenfrequentie_en_druk_zijn_aannemelijk() {
        let w = voorbeeld();
        let l = w.lade(101).unwrap();
        let b = l.balg.as_ref().unwrap();
        let f0 = b.eigenfrequentie_hz(l.volume_m3).unwrap();
        assert!((6.0..9.0).contains(&f0), "f0 = {}", f0);
        let p = b.druk_inch().unwrap();
        assert!((5.0..7.0).contains(&p), "druk = {}", p);
        // Groter volume onder dezelfde balg = zachtere veer = lagere frequentie.
        assert!(b.eigenfrequentie_hz(3.0).unwrap() < f0);
    }

    fn pijp(stop: u32, nr: u32, divisie: usize, lade: Option<u32>, kg_s: Option<f32>, schatting: f32) -> PijpWindIn {
        PijpWindIn { sleutel: (stop, nr), divisie, lade, kg_s, schatting }
    }

    /// Twee balgen (pedaal en manuaal) en een derde divisie zonder windgegevens.
    fn twee_balgen() -> WindwerkDef {
        let mut w = voorbeeld();
        let extra = WindwerkDef::uit_tabellen(
            &[fm(&[("WindCompartmentID", "102"), ("Name", "M HW"), ("StandardVolumeMetresCubed", "0.6"), ("Bellows_HasBellows", "Y"),
                   ("Bellows_FrameBaseWidthMetres", "1.0"), ("Bellows_FrameBaseLengthMetres", "1.2"),
                   ("Bellows_MassOfMovingBoardActedOnByGravityKg", "150")])],
            &[fm(&[("FirstWindCompartmentID", "2"), ("SecondWindCompartmentID", "102"),
                   ("MassFlowRateKilogramsPerSecAtReferencePressureDiff", "0.1"), ("ReferencePressureDifferenceInches", "1")])],
        );
        w.laden.extend(extra.laden);
        w.verbindingen.extend(extra.verbindingen);
        w
    }

    #[test]
    fn een_groep_per_balg_en_een_eigen_groep_zonder_balg() {
        let w = twee_balgen();
        let pijpen = vec![
            pijp(1, 1, 0, Some(201), Some(0.004), 4.0), // pedaal via kanaal aan M P
            pijp(1, 2, 0, Some(201), Some(0.002), 2.0),
            pijp(2, 1, 1, Some(102), Some(0.001), 1.0), // manuaal aan M HW
            pijp(2, 2, 1, Some(101), Some(0.001), 1.0), // één uitschieter naar M P: de meerderheid wint
            pijp(2, 3, 1, Some(102), Some(0.003), 0.5),
            pijp(3, 1, 2, None, None, 1.0),            // werk zonder windgegevens
        ];
        let a = w.afleiden(&pijpen, 3).expect("afleiding");
        assert_eq!(a.groep_van_divisie, vec![0, 1, 2]);
        assert_eq!(a.groepen[0].balg, Some(101));
        assert_eq!(a.groepen[1].balg, Some(102));
        assert_eq!(a.groepen[2].balg, None);
        assert_eq!(a.groepen[2].balg_factor, 1.0);
        // Per groep: de som van het setverbruik = de som van de schattingen; de verdeling volgt de set.
        let som = |div: usize| pijpen.iter().filter(|p| p.divisie == div).filter_map(|p| a.verbruik.get(&p.sleutel)).sum::<f32>();
        assert!((som(0) - 6.0).abs() < 1e-4, "{}", som(0));
        assert!((som(1) - 2.5).abs() < 1e-4, "{}", som(1));
        assert!((a.verbruik[&(2, 3)] / a.verbruik[&(2, 1)] - 3.0).abs() < 1e-4);
        assert!(!a.verbruik.contains_key(&(3, 1)), "zonder setwaarde blijft de schatting");
    }

    #[test]
    fn twee_divisies_op_een_balg_delen_een_groep() {
        let w = twee_balgen();
        let pijpen = vec![pijp(1, 1, 0, Some(201), Some(0.004), 4.0), pijp(2, 1, 1, Some(202), Some(0.001), 1.0)];
        let a = w.afleiden(&pijpen, 2).unwrap();
        assert_eq!(a.groep_van_divisie, vec![0, 0]);
        assert_eq!(a.groepen.len(), 1);
        assert_eq!(a.groepen[0].divisies, vec![0, 1]);
    }

    #[test]
    fn zonder_balgen_of_met_te_veel_groepen_de_gewone_indeling() {
        let w = twee_balgen();
        assert!(w.afleiden(&[pijp(1, 1, 0, Some(2), Some(0.01), 1.0)], 1).is_none(), "alleen de ventilator: geen balg");
        // 1 balg + 8 werken zonder balg = 9 groepen.
        let mut pijpen = vec![pijp(1, 1, 0, Some(201), Some(0.004), 4.0)];
        for d in 1..9 { pijpen.push(pijp(9, d as u32, d, None, None, 1.0)); }
        assert!(w.afleiden(&pijpen, 9).is_none());
    }

    #[test]
    fn factoren_volgen_de_balg_en_de_toevoer() {
        let w = twee_balgen();
        // M P: f0 ~7 Hz → kleinere balg dan de middenstand (3/7 = 0,43 < 0,5).
        let a = w.afleiden(&[pijp(1, 1, 0, Some(201), Some(1.0), 1.0)], 1).unwrap();
        let g = &a.groepen[0];
        assert!(g.redelijk);
        assert!((g.balg_factor - (3.0 / g.f0_hz.unwrap()) / STANDAARD_BALG).abs() < 1e-4);
        // Toevoer 2 kg/s (ref 1"), verbruik 1 kg/s, druk ~6": daling = 1·0,25/6 ≈ 4 % → factor ≈ 0,8.
        let d = g.daling_set.unwrap();
        assert!((0.03..0.05).contains(&d), "daling {}", d);
        assert!((g.daling_factor - d / STANDAARD_DALING).abs() < 1e-4);
        // Veel meer verbruik dan toevoer: begrensd op 3×.
        let a = w.afleiden(&[pijp(1, 1, 0, Some(201), Some(20.0), 1.0)], 1).unwrap();
        assert_eq!(a.groepen[0].daling_factor, 3.0);
    }

    #[test]
    fn onaannemelijke_balg_houdt_de_standaard() {
        // Lichte, kleine balg op een klein volume: tientallen Hz.
        let w = WindwerkDef::uit_tabellen(
            &[fm(&[("WindCompartmentID", "5"), ("Name", "Miech"), ("StandardVolumeMetresCubed", "0.01"), ("Bellows_HasBellows", "Y"),
                   ("Bellows_FrameBaseWidthMetres", "0.5"), ("Bellows_FrameBaseLengthMetres", "0.5"),
                   ("Bellows_MassOfMovingBoardActedOnByGravityKg", "5")])],
            &[],
        );
        let a = w.afleiden(&[pijp(1, 1, 0, Some(5), Some(0.01), 1.0)], 1).unwrap();
        let g = &a.groepen[0];
        assert!(g.f0_hz.unwrap() > 20.0);
        assert!(!g.redelijk);
        assert_eq!((g.balg_factor, g.daling_factor), (1.0, 1.0));
    }
}
