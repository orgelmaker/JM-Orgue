//! Zwelkast van de set (E5, stap 3): per divisie een voorstel voor de diepte en het
//! kantelpunt van de zwelkast, afgeleid van de filterwaarden die de set per kast opgeeft.
//!
//! De vertaling (door de gebruiker gekozen):
//!
//! - **diepte** = de demping die de set bij een dichte kast opgeeft, als negatieve dB
//!   (dezelfde as als "min dB" bij Zwelkast-instellingen);
//! - **kantelpunt** = het meetkundig gemiddelde van de laagste en de hoogste
//!   filterfrequentie bij een dichte kast.
//!
//! Beide worden net zo begrensd als de instelling zelf (diepte −24 tot −5 dB, kantelpunt
//! 200 tot 5000 Hz), dus een set die maar 3,7 dB opgeeft krijgt −5 dB.
//!
//! Dit is alleen een voorstel. Of het gebruikt wordt bepaalt het vinkje "Zwelkast van de
//! set gebruiken" per orgel, dat standaard uit staat, en een divisie waarvoor de
//! gebruiker zelf een waarde koos houdt die altijd.

use serde::Serialize;
use vpo_sampler::grandorgue::EnclosureFilter;

/// Voorstel voor één divisie.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZwelkastVoorstel {
    /// Naam van de divisie zoals de rest van de app die gebruikt.
    pub division: String,
    /// Naam van de zwelkast in de definitie.
    pub kast: String,
    /// Voorgestelde diepte (dB, negatief), begrensd op −24…−5.
    pub min_db: f32,
    /// Voorgesteld kantelpunt (Hz), begrensd op 200…5000.
    pub filter_cutoff: f32,
    /// Wat de set zelf opgeeft, onbegrensd: demping bij dicht (dB) en het kantelpunt (Hz).
    pub set_demping_db: f32,
    pub set_kantelpunt_hz: f32,
}

/// Een divisie met de zwelkasten van haar registers, voor [`voorstel_uit`].
pub struct DivisieKasten {
    pub naam: String,
    /// Per echt register (geen ruis/mechaniek) de zwelkast-id's van zijn windlade.
    pub registers: Vec<Vec<u32>>,
}

/// De zwelkast van een divisie: die waarin de meeste van haar registers zitten, mits
/// minstens de helft van de registers in een zwelkast zit (zelfde regel als de
/// herkenning van een ingesloten divisie). Bij een gelijk aantal de laagste id.
fn kies_kast(d: &DivisieKasten) -> Option<u32> {
    if d.registers.is_empty() {
        return None;
    }
    let mut tel: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    let mut ingesloten = 0usize;
    for kasten in &d.registers {
        if !kasten.is_empty() {
            ingesloten += 1;
        }
        for k in kasten {
            *tel.entry(*k).or_insert(0) += 1;
        }
    }
    if ingesloten * 2 < d.registers.len() {
        return None;
    }
    tel.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|(k, _)| k)
}

/// Het voorstel per divisie. `kasten` geeft per zwelkast-id de naam en de filterwaarden
/// van de set (None = de set geeft er geen). Een divisie zonder zwelkast met
/// filterwaarden komt er niet in.
pub fn voorstel_uit(
    divisies: &[DivisieKasten],
    kasten: &dyn Fn(u32) -> Option<(String, Option<EnclosureFilter>)>,
) -> Vec<ZwelkastVoorstel> {
    divisies
        .iter()
        .filter_map(|d| {
            let id = kies_kast(d)?;
            let (kast, filter) = kasten(id)?;
            let f = filter?;
            let demping = f.dicht_demping_db;
            let kantelpunt = f.kantelpunt_hz();
            Some(ZwelkastVoorstel {
                division: d.naam.clone(),
                kast,
                min_db: f.diepte_db().clamp(-24.0, -5.0),
                filter_cutoff: kantelpunt.clamp(200.0, 5000.0),
                set_demping_db: demping,
                set_kantelpunt_hz: kantelpunt,
            })
        })
        .collect()
}

/// Het voorstel voor een geladen definitie.
pub fn voorstel_uit_definitie(def: &vpo_sampler::OrganDefinition) -> Vec<ZwelkastVoorstel> {
    let divisies: Vec<DivisieKasten> = def
        .manuals
        .iter()
        .map(|m| DivisieKasten {
            naam: m.name.clone(),
            registers: m
                .stop_ids
                .iter()
                .filter_map(|sid| def.stops.iter().find(|s| s.id == *sid))
                .filter(|s| !s.is_noise_or_mechanical())
                .map(|s| {
                    def.windchests
                        .iter()
                        .find(|w| w.number == s.windchest_group)
                        .map(|w| w.enclosure_ids.clone())
                        .unwrap_or_default()
                })
                .collect(),
        })
        .collect();
    voorstel_uit(&divisies, &|id| {
        def.enclosures.iter().find(|e| e.number == id).map(|e| (e.name.clone(), e.filter.clone()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(demping: f32, dmin: f32, dmax: f32) -> EnclosureFilter {
        EnclosureFilter::uit_waarden(demping, None, (dmin, dmax), (dmin * 4.0, dmax * 2.0), "test", 10).unwrap()
    }

    fn divisie(naam: &str, registers: Vec<Vec<u32>>) -> DivisieKasten {
        DivisieKasten { naam: naam.to_string(), registers }
    }

    fn opzoek(id: u32) -> Option<(String, Option<EnclosureFilter>)> {
        match id {
            1 => Some(("Zwel".to_string(), Some(filter(7.0, 500.0, 3500.0)))),
            2 => Some(("Echo".to_string(), Some(filter(10.0, 100.0, 400.0)))),
            3 => Some(("Zonder".to_string(), None)),
            _ => None,
        }
    }

    #[test]
    fn diepte_is_de_demping_en_kantelpunt_het_meetkundig_gemiddelde() {
        let v = voorstel_uit(&[divisie("Zwelwerk", vec![vec![1], vec![1], vec![1]])], &opzoek);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].division, "Zwelwerk");
        assert_eq!(v[0].kast, "Zwel");
        assert_eq!(v[0].min_db, -7.0);
        let verwacht = (500.0f32 * 3500.0).sqrt();
        assert!((v[0].filter_cutoff - verwacht).abs() < 1e-2, "{} tegen {}", v[0].filter_cutoff, verwacht);
        assert_eq!((v[0].set_demping_db, v[0].set_kantelpunt_hz), (7.0, v[0].filter_cutoff));
    }

    #[test]
    fn de_waarden_worden_net_zo_begrensd_als_de_instelling() {
        // 3 dB demping geeft −5 (ondergrens van het bereik), 30 dB geeft −24; kantelpunt 100 Hz geeft 200.
        let diep = |db| EnclosureFilter::uit_waarden(db, None, (50.0, 200.0), (100.0, 400.0), "test", 1).unwrap();
        let opzoek2 = |id: u32| -> Option<(String, Option<EnclosureFilter>)> {
            Some(("K".to_string(), Some(match id { 1 => diep(3.0), _ => diep(30.0) })))
        };
        let v = voorstel_uit(&[divisie("A", vec![vec![1]]), divisie("B", vec![vec![2]])], &opzoek2);
        assert_eq!((v[0].min_db, v[0].filter_cutoff), (-5.0, 200.0));
        assert_eq!((v[0].set_demping_db, v[0].set_kantelpunt_hz.round()), (3.0, 100.0), "het onbegrensde getal blijft zichtbaar");
        assert_eq!(v[1].min_db, -24.0);
        let hoog = EnclosureFilter::uit_waarden(8.0, None, (5000.0, 20_000.0), (6000.0, 22_000.0), "test", 1).unwrap();
        let v = voorstel_uit(&[divisie("C", vec![vec![1]])], &|_| Some(("K".to_string(), Some(hoog.clone()))));
        assert_eq!(v[0].filter_cutoff, 5000.0);
    }

    #[test]
    fn een_divisie_krijgt_de_kast_waarin_de_meeste_registers_zitten() {
        // Vier registers: twee in kast 1, één in kast 2, één zonder kast: kast 1 wint en de helft zit in een kast.
        let v = voorstel_uit(&[divisie("Man", vec![vec![1], vec![1], vec![2], vec![]])], &opzoek);
        assert_eq!(v[0].kast, "Zwel");
        // Gelijk aantal: de laagste id.
        let v = voorstel_uit(&[divisie("Man", vec![vec![2], vec![1]])], &opzoek);
        assert_eq!(v[0].kast, "Zwel");
    }

    #[test]
    fn een_divisie_waarvan_minder_dan_de_helft_in_een_kast_zit_krijgt_niets() {
        let v = voorstel_uit(&[divisie("Man", vec![vec![1], vec![], vec![], vec![]])], &opzoek);
        assert!(v.is_empty());
        assert!(voorstel_uit(&[divisie("Leeg", vec![])], &opzoek).is_empty());
    }

    #[test]
    fn een_kast_zonder_filterwaarden_of_onbekend_geeft_geen_voorstel() {
        assert!(voorstel_uit(&[divisie("Man", vec![vec![3], vec![3]])], &opzoek).is_empty());
        assert!(voorstel_uit(&[divisie("Man", vec![vec![99]])], &opzoek).is_empty());
    }

    #[test]
    fn meerdere_divisies_krijgen_elk_hun_eigen_voorstel() {
        let v = voorstel_uit(
            &[divisie("Hoofdwerk", vec![vec![]]), divisie("Zwelwerk", vec![vec![1]]), divisie("Echo", vec![vec![2]])],
            &opzoek,
        );
        assert_eq!(v.iter().map(|x| x.division.as_str()).collect::<Vec<_>>(), vec!["Zwelwerk", "Echo"]);
        assert_eq!((v[0].min_db, v[1].min_db), (-7.0, -10.0));
    }
}
