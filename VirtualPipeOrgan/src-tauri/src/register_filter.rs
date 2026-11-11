//! Registers niet laden (F2): de gebruiker kan registers die hij niet gebruikt overslaan bij het laden,
//! per orgel. Dat scheelt geheugen en laadtijd.
//!
//! De keuze wordt VOOR alles wat uit de definitie volgt uit die definitie gehaald (de registers verdwijnen uit de
//! manualen en uit de lijst van registers), dus de schatting van het geheugen, de laadroute, de knoppen en de
//! rest zien gewoon een kleiner orgel. Een register wordt aangeduid met dezelfde id als op het scherm:
//! `"{manuaal}_{register}"`. Wat niet (meer) bestaat in de definitie, wordt genegeerd.
//!
//! Het geheugen per register wordt geteld met dezelfde regel als de schatting vooraf van het laden
//! (unieke bestanden en segmenten per perspectieflabel, zie `commands::voeg_pijp_buffers_toe`).

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use vpo_sampler::grandorgue::OrganDefinition;

use crate::commands::{voeg_pijp_buffers_toe, BufferSleutel, GEEN_PERSPECTIEF};

/// Eén register van het volledige orgel, ook als het niet geladen wordt.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RegisterGeheugen {
    /// Id zoals op het scherm: `"{manuaal}_{register}"`.
    pub id: String,
    pub naam: String,
    /// Naam van de divisie (het manuaal) waar het register staat.
    pub divisie: String,
    /// `true` als dit register bij de laatste lading is weggelaten.
    pub uitgesloten: bool,
    /// Aantal preload-buffers per perspectieflabel (zonder label onder `GEEN_PERSPECTIEF`).
    pub buffers_per_label: HashMap<String, u64>,
}

impl RegisterGeheugen {
    /// Het aantal buffers dat dit register vraagt voor de posities die meetellen: lagen zonder label altijd, de
    /// rest alleen als `label_telt` het label goedkeurt.
    pub fn buffers(&self, label_telt: &dyn Fn(&str) -> bool) -> u64 {
        self.buffers_per_label
            .iter()
            .filter(|(label, _)| label.as_str() == GEEN_PERSPECTIEF || label_telt(label))
            .map(|(_, n)| *n)
            .sum()
    }
}

/// Wat de interface van een register moet weten.
#[derive(Debug, Clone, Serialize)]
pub struct RegisterGeheugenDto {
    pub id: String,
    pub naam: String,
    pub divisie: String,
    pub uitgesloten: bool,
    /// Geheugen (bytes) voor het begin van de opnamen, voor de posities die nu aan staan.
    pub bytes: u64,
}

/// Haal de gekozen registers uit de definitie en geef de lijst van alle registers terug (voor de interface),
/// met het aantal verwijderde registers. Mechaniek- en ruisregisters doen niet mee: dat zijn geen knoppen.
pub fn pas_toe(definition: &mut OrganDefinition, weg: &[String]) -> (Vec<RegisterGeheugen>, usize) {
    let weg: HashSet<&str> = weg.iter().map(String::as_str).collect();
    let mut tabel: Vec<RegisterGeheugen> = Vec::new();
    // (manuaal, register)-paren die weg moeten.
    let mut weg_paren: HashSet<(u32, u32)> = HashSet::new();
    for manual in &definition.manuals {
        for sid in &manual.stop_ids {
            let Some(stop) = definition.stops.iter().find(|s| s.id == *sid) else { continue };
            if stop.is_noise_or_mechanical() {
                continue;
            }
            let id = format!("{}_{}", manual.number, stop.id);
            let uitgesloten = weg.contains(id.as_str());
            let mut per_label: HashMap<String, (HashSet<BufferSleutel>, usize)> = HashMap::new();
            for (_idx, persp, pipes) in stop.all_layers() {
                let label = persp.map(|l| l.to_string()).unwrap_or_else(|| GEEN_PERSPECTIEF.to_string());
                let (unieke, referenties) = per_label.entry(label).or_default();
                for p in pipes {
                    voeg_pijp_buffers_toe(p, unieke, referenties);
                }
            }
            tabel.push(RegisterGeheugen {
                id,
                naam: stop.name.clone(),
                divisie: manual.name.clone(),
                uitgesloten,
                buffers_per_label: per_label.into_iter().map(|(l, (u, r))| (l, (u.len() + r) as u64)).collect(),
            });
            if uitgesloten {
                weg_paren.insert((manual.number, stop.id));
            }
        }
    }
    if weg_paren.is_empty() {
        return (tabel, 0);
    }
    for manual in &mut definition.manuals {
        let nummer = manual.number;
        manual.stop_ids.retain(|sid| !weg_paren.contains(&(nummer, *sid)));
    }
    // Het register zelf gaat pas weg als geen enkel manuaal het nog gebruikt.
    let nog_in_gebruik: HashSet<u32> = definition.manuals.iter().flat_map(|m| m.stop_ids.iter().copied()).collect();
    let weg_ids: HashSet<u32> = weg_paren.iter().map(|(_, s)| *s).collect();
    definition.stops.retain(|s| !weg_ids.contains(&s.id) || nog_in_gebruik.contains(&s.id));
    (tabel, weg_paren.len())
}

/// De lijst voor de interface, met het geheugen bij `per_buffer` bytes per buffer.
pub fn voor_interface(tabel: &[RegisterGeheugen], label_telt: &dyn Fn(&str) -> bool, per_buffer: u64) -> Vec<RegisterGeheugenDto> {
    tabel
        .iter()
        .map(|r| RegisterGeheugenDto {
            id: r.id.clone(),
            naam: r.naam.clone(),
            divisie: r.divisie.clone(),
            uitgesloten: r.uitgesloten,
            bytes: r.buffers(label_telt) * per_buffer,
        })
        .collect()
}

/// Normaliseer een lijst gekozen registers voor opslag: zonder dubbelen en zonder lege regels, op volgorde.
pub fn schoon(ids: Vec<String>) -> Vec<String> {
    let mut v: Vec<String> = ids.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    v.sort();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use vpo_sampler::grandorgue::{ManualDef, OrganInfo, PipeDef, PipeExtra, PipeLayer, ReleaseDef, StopDef};

    fn stop(id: u32, naam: &str, pijpen: Vec<PipeDef>, lagen: Vec<PipeLayer>, label: Option<&str>) -> StopDef {
        StopDef {
            id,
            name: naam.to_string(),
            harmonic_number: 8,
            sounding_harmonic: 8,
            pitch_correction: 0.0,
            pitch_tuning_cents: 0.0,
            gain_db: 0.0,
            number_of_pipes: pijpen.len() as u32,
            first_accessible_pipe_logical_key: 1,
            windchest_group: 1,
            amplitude_level: 100.0,
            percussive: false,
            accepts_retuning: true,
            first_midi_note: Some(36),
            pipes: pijpen,
            layers: lagen,
            primary_perspective: label.map(String::from),
        }
    }

    fn pijp(pad: &str) -> PipeDef {
        PipeDef::Sample { path: PathBuf::from(pad), extra: PipeExtra::default() }
    }

    fn pijp_met_release(pad: &str, release: &str) -> PipeDef {
        PipeDef::Sample {
            path: PathBuf::from(pad),
            extra: PipeExtra {
                releases: vec![ReleaseDef { path: PathBuf::from(release), max_key_press_time_ms: None, cue_point: None, release_end: None, crossfade_ms: None }],
                ..Default::default()
            },
        }
    }

    fn manuaal(nummer: u32, naam: &str, stops: &[u32]) -> ManualDef {
        ManualDef {
            number: nummer,
            name: naam.to_string(),
            midi_input_number: nummer,
            number_of_logical_keys: 61,
            number_of_accessible_keys: 61,
            first_accessible_key_midi_note: 36,
            stop_ids: stops.to_vec(),
            coupler_ids: vec![],
            tremulant_ids: vec![],
        }
    }

    fn definitie() -> OrganDefinition {
        // Register 1 en 2 staan op manuaal 1; register 3 op manuaal 2; 4 is mechaniek.
        // Register 2 deelt zijn attack-bestand met register 1 ("Bourdon.wav").
        OrganDefinition {
            base_path: PathBuf::from("/set"),
            organ: OrganInfo::default(),
            manuals: vec![manuaal(1, "Hoofdwerk", &[1, 2, 4]), manuaal(2, "Zwelwerk", &[3])],
            stops: vec![
                stop(1, "Bourdon 8", vec![pijp("Bourdon.wav"), pijp_met_release("A.wav", "A-r.wav")], vec![], None),
                stop(2, "Gedekt 8", vec![pijp("Bourdon.wav")], vec![], None),
                stop(3, "Fluit 8", vec![pijp("F.wav")], vec![PipeLayer { index: 1, name: "rear".into(), perspective: Some("rear".into()), pipes: vec![pijp("F-rear.wav")] }], Some("front")),
                stop(4, "Stop attack noise", vec![pijp("klik.wav")], vec![], None),
            ],
            windchests: vec![],
            enclosures: vec![],
            tremulants: vec![],
            couplers: vec![],
            extra_bijgeluiden: vec![],
            stop_action_clicks: Default::default(),
            keyboard_clicks: Default::default(),
            klavier_afkortingen: Default::default(),
            schakelaars: Default::default(),
        }
    }

    #[test]
    fn zonder_keuze_blijft_de_definitie_gelijk_en_komt_elk_echt_register_in_de_lijst() {
        let mut d = definitie();
        let (tabel, weg) = pas_toe(&mut d, &[]);
        assert_eq!(weg, 0);
        assert_eq!(d.stops.len(), 4);
        // De mechaniek (4) doet niet mee in de lijst.
        let ids: Vec<&str> = tabel.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["1_1", "1_2", "2_3"]);
        assert_eq!(tabel[0].divisie, "Hoofdwerk");
        assert_eq!(tabel[2].naam, "Fluit 8");
        assert!(tabel.iter().all(|r| !r.uitgesloten));
    }

    #[test]
    fn een_gekozen_register_verdwijnt_uit_manuaal_en_lijst_van_registers() {
        let mut d = definitie();
        let (tabel, weg) = pas_toe(&mut d, &["1_2".to_string()]);
        assert_eq!(weg, 1);
        assert_eq!(d.manuals[0].stop_ids, vec![1, 4], "register 2 is weg, de mechaniek blijft staan");
        assert_eq!(d.manuals[1].stop_ids, vec![3]);
        assert!(d.stops.iter().all(|s| s.id != 2));
        assert_eq!(d.stops.len(), 3);
        // Het weggelaten register staat nog wel in de lijst, gemarkeerd.
        let r2 = tabel.iter().find(|r| r.id == "1_2").unwrap();
        assert!(r2.uitgesloten);
        assert!(tabel.iter().filter(|r| r.id != "1_2").all(|r| !r.uitgesloten));
    }

    #[test]
    fn onbekende_en_dubbele_ids_worden_genegeerd() {
        let mut d = definitie();
        let (_t, weg) = pas_toe(&mut d, &["9_9".to_string(), "1_99".to_string(), "x".to_string(), "1_2".to_string(), "1_2".to_string()]);
        assert_eq!(weg, 1);
        assert_eq!(d.stops.len(), 3);
        // Mechaniek kan niet worden weggelaten via een id.
        let mut d2 = definitie();
        let (_t, weg2) = pas_toe(&mut d2, &["1_4".to_string()]);
        assert_eq!(weg2, 0);
        assert_eq!(d2.stops.len(), 4);
    }

    #[test]
    fn het_geheugen_per_register_telt_unieke_bestanden_per_label() {
        let mut d = definitie();
        let (tabel, _) = pas_toe(&mut d, &[]);
        let alles = |_: &str| true;
        // Register 1: Bourdon.wav + A.wav + A-r.wav = 3; register 2: Bourdon.wav = 1; register 3: front 1, rear 1.
        assert_eq!(tabel[0].buffers(&alles), 3);
        assert_eq!(tabel[1].buffers(&alles), 1);
        assert_eq!(tabel[2].buffers(&alles), 2);
        // Alleen de positie "front": het register met twee posities telt de achterste niet.
        let alleen_front = |l: &str| l == "front";
        assert_eq!(tabel[2].buffers(&alleen_front), 1);
        // Lagen zonder label tellen altijd mee, ook als geen enkele positie aan staat.
        let geen = |_: &str| false;
        assert_eq!(tabel[0].buffers(&geen), 3);
        assert_eq!(tabel[2].buffers(&geen), 0);
    }

    #[test]
    fn de_lijst_voor_de_interface_rekent_met_de_grootte_per_buffer() {
        let mut d = definitie();
        let (tabel, _) = pas_toe(&mut d, &["1_1".to_string()]);
        let alles = |_: &str| true;
        let lijst = voor_interface(&tabel, &alles, 1_000);
        assert_eq!(lijst.iter().map(|r| r.bytes).collect::<Vec<_>>(), vec![3_000, 1_000, 2_000]);
        assert!(lijst[0].uitgesloten, "ook het weggelaten register toont wat het zou kosten");
    }

    #[test]
    fn schoon_ordent_en_ontdubbelt() {
        assert_eq!(schoon(vec![" 2_1".into(), "1_5".into(), "".into(), "2_1".into()]), vec!["1_5".to_string(), "2_1".to_string()]);
        assert!(schoon(vec![]).is_empty());
    }
}
