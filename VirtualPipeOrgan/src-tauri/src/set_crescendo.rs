//! Crescendo van de set (E4): de trappen die een leesbare Hauptwerk-definitie zelf
//! meebrengt, als extra voorstel naast dat van JM-Orgue (`crescendo_voorstel.rs`).
//!
//! De importer (`vpo-sampler`) levert per trap de register-id's van de definitie. Hier worden die
//! vertaald naar de register-id's van het geladen orgel (`"{manuaal}_{register}"`), beperkt tot
//! wat er echt als knop staat (een demo laadt maar een deel van de registers van de set), en
//! verdeeld over het gekozen aantal trappen van het crescendoscherm.
//!
//! Alleen registers: koppels en tremulanten van de set worden niet overgenomen. Het is een knop;
//! niets verandert zonder dat de gebruiker erom vraagt.

use std::collections::HashMap;

use serde::Serialize;
use vpo_sampler::import_report::SetCrescendo;

/// Wat de interface van de crescendo van de set moet weten.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetCrescendoInfo {
    /// Naam van de regelaar in de set.
    pub naam: String,
    /// Aantal trappen in de set zelf.
    pub trappen_in_set: usize,
    /// Aantal verschillende trappen dat overblijft voor dit orgel.
    pub trappen_voor_orgel: usize,
    /// Hoeveel registers de volste trap voor dit orgel aanzet.
    pub registers_volste_trap: usize,
    /// Schakelaars in de trappen die niet zijn overgenomen (koppels, tremulanten, ...).
    pub niet_gelezen: usize,
}

/// De register-id's van het geladen orgel per register-id van de definitie. Een id van de
/// definitie kan in principe in meer dan één manuaal voorkomen.
pub fn id_kaart<'a>(ids: impl Iterator<Item = &'a str>) -> HashMap<u32, Vec<String>> {
    let mut kaart: HashMap<u32, Vec<String>> = HashMap::new();
    for id in ids {
        // "{manuaal}_{register}": het register-deel is het id van de definitie.
        if let Some((_, rest)) = id.split_once('_') {
            if let Ok(stop) = rest.parse::<u32>() {
                kaart.entry(stop).or_default().push(id.to_string());
            }
        }
    }
    kaart
}

/// De trappen van de set voor dit orgel: alleen registers die er zijn, zonder lege trappen en
/// zonder trappen die na het weglaten van onbekende registers gelijk zijn aan de vorige.
/// Elke trap bevat alles van de vorige (zoals de crescendotrappen van de app zelf).
pub fn trappen_voor_orgel(c: &SetCrescendo, kaart: &HashMap<u32, Vec<String>>) -> Vec<Vec<String>> {
    let mut uit: Vec<Vec<String>> = Vec::new();
    for t in &c.trappen {
        let mut ids: Vec<String> = t.stops.iter().filter_map(|s| kaart.get(s)).flatten().cloned().collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() || uit.last() == Some(&ids) {
            continue;
        }
        uit.push(ids);
    }
    uit
}

/// Verdeel de trappen van de set over `n` trappen: de eerste en de laatste blijven, de rest
/// gelijkmatig. Heeft de set minder trappen dan er gevraagd zijn, dan herhaalt een trap zich.
pub fn verdeel_over(trappen: &[Vec<String>], n: usize) -> Vec<Vec<String>> {
    let m = trappen.len();
    if m == 0 || n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![trappen[m - 1].clone()];
    }
    (0..n)
        .map(|k| {
            let i = (k * (m - 1) + (n - 1) / 2) / (n - 1);
            trappen[i].clone()
        })
        .collect()
}

/// De samenvatting voor de interface.
pub fn info_voor(c: &SetCrescendo, kaart: &HashMap<u32, Vec<String>>) -> SetCrescendoInfo {
    let trappen = trappen_voor_orgel(c, kaart);
    SetCrescendoInfo {
        naam: c.naam.clone(),
        trappen_in_set: c.trappen.len(),
        trappen_voor_orgel: trappen.len(),
        registers_volste_trap: trappen.last().map(|t| t.len()).unwrap_or(0),
        niet_gelezen: c.niet_gelezen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vpo_sampler::import_report::SetTrap;

    fn trap(waarde: u32, stops: &[u32]) -> SetTrap {
        SetTrap { naam: format!("trap {waarde}"), waarde, stops: stops.to_vec() }
    }

    fn set() -> SetCrescendo {
        SetCrescendo {
            naam: "Walze".into(),
            niet_gelezen: 2,
            trappen: vec![
                trap(0, &[]),
                trap(10, &[7]),
                trap(20, &[7, 8]),
                trap(30, &[7, 8, 99]),
                trap(40, &[7, 8, 9, 99]),
            ],
        }
    }

    fn kaart() -> HashMap<u32, Vec<String>> {
        // Register 99 bestaat in het geladen orgel niet (een demo laadt maar een deel).
        id_kaart(["1_7", "1_8", "0_9"].into_iter())
    }

    #[test]
    fn id_kaart_leest_het_registerdeel_van_de_id() {
        let k = id_kaart(["1_7", "0_9", "2_7", "rommel", "1_x"].into_iter());
        assert_eq!(k.get(&7), Some(&vec!["1_7".to_string(), "2_7".to_string()]));
        assert_eq!(k.get(&9), Some(&vec!["0_9".to_string()]));
        assert_eq!(k.len(), 2);
    }

    #[test]
    fn trappen_voor_orgel_laat_onbekende_registers_en_dubbele_of_lege_trappen_weg() {
        let t = trappen_voor_orgel(&set(), &kaart());
        // De lege nulstand valt weg; trap 30 is na het weglaten van register 99 gelijk aan trap 20.
        assert_eq!(t, vec![
            vec!["1_7".to_string()],
            vec!["1_7".to_string(), "1_8".to_string()],
            vec!["0_9".to_string(), "1_7".to_string(), "1_8".to_string()],
        ]);
        // Elke trap bevat de vorige.
        for w in t.windows(2) {
            assert!(w[0].iter().all(|id| w[1].contains(id)));
        }
    }

    #[test]
    fn verdeel_over_houdt_eerste_en_laatste_en_herhaalt_bij_te_weinig_trappen() {
        let t = trappen_voor_orgel(&set(), &kaart());
        assert_eq!(verdeel_over(&t, 3), t);
        let vijf = verdeel_over(&t, 5);
        assert_eq!(vijf.len(), 5);
        assert_eq!(vijf[0], t[0]);
        assert_eq!(vijf[4], t[2]);
        // Eén trap: de volste.
        assert_eq!(verdeel_over(&t, 1), vec![t[2].clone()]);
        // Twee trappen: eerste en laatste.
        assert_eq!(verdeel_over(&t, 2), vec![t[0].clone(), t[2].clone()]);
        // Veel trappen verdeeld over minder: eerste en laatste blijven, volgorde blijft oplopend.
        let lang: Vec<Vec<String>> = (1..=44).map(|n| (1..=n).map(|i| format!("1_{i}")).collect()).collect();
        let acht = verdeel_over(&lang, 8);
        assert_eq!(acht.len(), 8);
        assert_eq!(acht[0], lang[0]);
        assert_eq!(acht[7], lang[43]);
        assert!(acht.windows(2).all(|w| w[0].len() < w[1].len()), "strikt oplopend");
        assert!(verdeel_over(&[], 5).is_empty());
        assert!(verdeel_over(&t, 0).is_empty());
    }

    #[test]
    fn info_telt_wat_er_voor_het_orgel_overblijft() {
        let i = info_voor(&set(), &kaart());
        assert_eq!(i.naam, "Walze");
        assert_eq!(i.trappen_in_set, 5);
        assert_eq!(i.trappen_voor_orgel, 3);
        assert_eq!(i.registers_volste_trap, 3);
        assert_eq!(i.niet_gelezen, 2);
        // Een orgel waar geen enkel register van de set bestaat: niets te kiezen.
        let leeg = info_voor(&set(), &HashMap::new());
        assert_eq!(leeg.trappen_voor_orgel, 0);
        assert_eq!(leeg.registers_volste_trap, 0);
    }
}
