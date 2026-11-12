//! Tremulant-golfvorm van de set naar de engine (C5, stap 2), per orgel met de vlag `tremulant_golf_van_set`.
//!
//! Per divisie de golfvorm die de meeste van haar pijpen noemen: die cyclus wordt de vorm van de tremulant-LFO, op de
//! snelheid van het tremulant van de set (`FrequencyWhenEngagedHz`). De diepte van de divisie is de mediaan van haar
//! pijpen (sterkte 10^(dB/20), toonhoogte procent/100); een pijp die daarvan afwijkt krijgt in de mengkern zijn eigen
//! factor. Zie `C5-golfvormen-onderzoek.md` voor wat de bestanden bevatten.
//!
//! Uit, of geen bruikbare golfvorm: er wordt niets gestuurd; de engine heeft bij elk nieuw orgel de gewone sinus.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use tracing::{info, warn};
use vpo_sampler::{OrganDefinition, PipeDef};

use crate::audio::AudioCommand;
use crate::commands::DivisionDto;
use crate::state::AppState;

/// Snelheid als de set er geen geeft (Hz).
const STANDAARD_HZ: f32 = 6.0;

fn mediaan(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[v.len() / 2])
}

/// Eén pijp: engine-sleutel, divisie, golfvorm en de diepten uit de set.
struct PijpTrem {
    sleutel: (u32, u32),
    divisie: usize,
    golf: u32,
    ka: Option<f32>,
    kp: Option<f32>,
}

fn pijpen(definition: &OrganDefinition, divisions: &[DivisionDto]) -> Vec<PijpTrem> {
    let mut uit = Vec::new();
    for (d, div) in divisions.iter().enumerate() {
        for s in &div.stops {
            let Some(stop) = definition.stops.iter().find(|x| x.id == s.internal_stop_id) else { continue };
            for (i, pd) in stop.pipes.iter().enumerate() {
                let Some(PipeDef::Sample { extra, .. }) = definition.resolve_reference(pd) else { continue };
                let Some(golf) = extra.trem_golf else { continue };
                uit.push(PijpTrem {
                    sleutel: (s.internal_stop_id, i as u32 + 1),
                    divisie: d,
                    golf,
                    ka: extra.trem_amp_db.map(|db| 10f32.powf(db / 20.0)),
                    kp: extra.trem_toon_pct.map(|p| p / 100.0),
                });
            }
        }
    }
    uit
}

/// Wat er naar de engine gaat, plus regels voor het logboek en het verslag.
pub struct TremGolfSet {
    pub golven: Vec<Option<vpo_audio::TremGolf>>,
    pub diepte: HashMap<(u32, u32), (f32, f32)>,
    pub regels: Vec<String>,
}

/// Afleiden voor het geladen orgel. `lees` leest een golfvormbestand (los te testen); None = niets te doen.
pub fn afleiden(
    definition: &OrganDefinition,
    divisions: &[DivisionDto],
    golfvormen: &[vpo_sampler::import_report::TremulantGolfvorm],
    lees: &dyn Fn(&std::path::Path) -> Result<vpo_sampler::tremulant_golf::GolfTabel, String>,
) -> Option<TremGolfSet> {
    if golfvormen.is_empty() || divisions.is_empty() || divisions.len() > 32 {
        return None;
    }
    let ps = pijpen(definition, divisions);
    if ps.is_empty() {
        return None;
    }
    // Per divisie de golfvorm van de meeste pijpen (bij gelijkspel de laagste id).
    let mut telling: Vec<BTreeMap<u32, usize>> = vec![BTreeMap::new(); divisions.len()];
    for p in &ps {
        *telling[p.divisie].entry(p.golf).or_default() += 1;
    }
    let mut tabellen: HashMap<u32, Option<Arc<Vec<[f32; 2]>>>> = HashMap::new();
    let mut golven: Vec<Option<vpo_audio::TremGolf>> = vec![None; 32];
    let mut mediaan_van: Vec<Option<(f32, f32)>> = vec![None; divisions.len()];
    let mut regels = Vec::new();
    for (d, t) in telling.iter().enumerate() {
        let Some((&golf, _)) = t.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))) else { continue };
        let Some(gv) = golfvormen.iter().find(|g| g.golfvorm_id == golf) else { continue };
        let tabel = tabellen.entry(golf).or_insert_with(|| {
            let pad = gv.grondtoon_pad.as_ref()?;
            match lees(pad) {
                Ok(t) => Some(Arc::new(t.punten)),
                Err(e) => {
                    warn!("Tremulant-golfvorm '{}' niet te lezen: {}", gv.tremulant_naam, e);
                    None
                }
            }
        });
        let Some(tabel) = tabel.clone() else { continue };
        let eigen: Vec<&PijpTrem> = ps.iter().filter(|p| p.divisie == d && p.golf == golf).collect();
        let ka = mediaan(eigen.iter().filter_map(|p| p.ka).collect()).unwrap_or(1.0);
        let kp = mediaan(eigen.iter().filter_map(|p| p.kp).collect()).unwrap_or(1.0);
        let hz = gv.frequentie_hz.filter(|h| *h > 0.5 && *h < 20.0).unwrap_or(STANDAARD_HZ);
        mediaan_van[d] = Some((ka, kp));
        golven[d] = Some(vpo_audio::TremGolf { tabel, hz, ka, kp });
        regels.push(format!(
            "divisie '{}': golfvorm van '{}', {:.1} Hz, sterkte ×{:.3}, toonhoogte ×{:.3} ({} pijpen)",
            divisions[d].name, gv.tremulant_naam, hz, ka, kp, eigen.len()
        ));
    }
    if golven.iter().all(|g| g.is_none()) {
        return None;
    }
    // Per pijp de verhouding tot de diepte van haar divisie; alleen als die merkbaar afwijkt.
    let mut diepte = HashMap::new();
    for p in &ps {
        let Some((ka_d, kp_d)) = mediaan_van[p.divisie] else { continue };
        let rel = |eigen: Option<f32>, d: f32| match eigen {
            Some(e) if d.abs() > 1e-6 => e / d,
            Some(_) => 0.0,
            None => 1.0,
        };
        let (a, k) = (rel(p.ka, ka_d), rel(p.kp, kp_d));
        if (a - 1.0).abs() > 1e-4 || (k - 1.0).abs() > 1e-4 {
            diepte.insert(p.sleutel, (a, k));
        }
    }
    Some(TremGolfSet { golven, diepte, regels })
}

/// Naar de engine, het logboek en het verslag.
pub fn toepassen(state: &AppState, set: TremGolfSet) {
    for r in &set.regels {
        info!("Tremulant volgt de set: {}", r);
    }
    if let Some(v) = state.laatste_importverslag.write().as_mut() {
        for r in &set.regels {
            v.melding(vpo_sampler::Ernst::Info, "tremulantgolfvorm", format!("Tremulant volgt de golfvorm van de set: {}", r));
        }
    }
    state.send_audio_command(AudioCommand::RegisterTremGolf { golven: Arc::new(set.golven), diepte: Arc::new(set.diepte) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mediaan_kiest_het_midden() {
        assert_eq!(mediaan(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(mediaan(vec![]), None);
    }
}
