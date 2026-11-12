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
}
