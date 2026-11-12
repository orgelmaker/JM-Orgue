//! Windwerk van de set naar het windmodel (D4, fase 1), per orgel met de vlag `windwerk_van_set`.
//!
//! De afleiding zelf staat in `vpo_sampler::windwerk` (zuiver en getest); hier komen de pijpen van het geladen orgel
//! erin (sleutel zoals de engine hem kent, divisie, lade en verbruik uit de set, en de schatting van JM-Orgue), en gaat
//! het resultaat naar de engine en de windgroepen:
//!
//! - elke balg wordt één windgroep (via de bestaande indeling divisie → windgroep);
//! - per pijp het verbruik van de set, geschaald zodat elke groep in totaal de schatting houdt;
//! - per groep een factor op de knoppen balggrootte en maximale daling: hun middenstand is de waarde van de set.
//!
//! Uit (of geen bruikbaar windwerk): er wordt niets gestuurd en alles blijft zoals het was.

use std::collections::HashMap;

use tracing::info;
use vpo_sampler::windwerk::{PijpWindIn, WindwerkAfleiding, WindwerkDef};
use vpo_sampler::{OrganDefinition, PipeDef};

use crate::audio::AudioCommand;
use crate::commands::DivisionDto;
use crate::state::AppState;

/// De pijpen van het geladen orgel voor de afleiding.
pub fn pijpen(
    definition: &OrganDefinition,
    divisions: &[DivisionDto],
    profielen: &HashMap<u32, vpo_audio::StopWindProfiel>,
) -> Vec<PijpWindIn> {
    let mut uit = Vec::new();
    for (d, div) in divisions.iter().enumerate() {
        for s in &div.stops {
            let Some(stop) = definition.stops.iter().find(|x| x.id == s.internal_stop_id) else { continue };
            let profiel = profielen.get(&s.internal_stop_id);
            for (i, pd) in stop.pipes.iter().enumerate() {
                let Some(PipeDef::Sample { extra, .. }) = definition.resolve_reference(pd) else { continue };
                let noot = (s.first_midi_note as usize + i).min(127) as u8;
                uit.push(PijpWindIn {
                    // Engine-sleutel: pijpindex + 1, zonder laagbits (zoals de NoteOn hem draagt).
                    sleutel: (s.internal_stop_id, i as u32 + 1),
                    divisie: d,
                    lade: extra.wind_lade,
                    kg_s: extra.wind_verbruik_kg_s,
                    schatting: profiel.map(|p| vpo_audio::verbruik_van_pijp(p, noot)).unwrap_or(0.0),
                });
            }
        }
    }
    uit
}

/// Afleiden voor het geladen orgel; None = de gewone indeling.
pub fn afleiden(
    definition: &OrganDefinition,
    divisions: &[DivisionDto],
    profielen: &HashMap<u32, vpo_audio::StopWindProfiel>,
    windwerk: &WindwerkDef,
) -> Option<WindwerkAfleiding> {
    if windwerk.is_leeg() || divisions.is_empty() || divisions.len() > 32 {
        return None;
    }
    windwerk.afleiden(&pijpen(definition, divisions, profielen), divisions.len())
}

/// Naar de engine en de windgroepen. Na `RegisterStopWindProfiles` en `reset_division_settings` aanroepen.
pub fn toepassen(state: &AppState, a: &WindwerkAfleiding, divisienamen: &[String]) {
    let mut balg = [1.0f32; 32];
    let mut daling = [1.0f32; 32];
    for (g, ga) in a.groepen.iter().enumerate().take(32) {
        balg[g] = ga.balg_factor;
        daling[g] = ga.daling_factor;
    }
    state.send_audio_command(AudioCommand::RegisterWindwerkSet {
        verbruik: std::sync::Arc::new(a.verbruik.clone()),
        balg,
        daling,
    });
    zet_groepen(state, &a.groep_van_divisie);
    *state.windwerk_set_groepen.write() = Some(a.groep_van_divisie.clone());
    for r in a.regels(divisienamen) {
        info!("Windwerk van de set: {}", r);
    }
    if let Some(v) = state.laatste_importverslag.write().as_mut() {
        for r in a.regels(divisienamen) {
            v.melding(vpo_sampler::Ernst::Info, "windwerk", format!("Windmodel volgt de set: {}", r));
        }
    }
}

/// De indeling divisie → windgroep van de set in de staat en de engine zetten, met genoeg groepen in het scherm.
pub fn zet_groepen(state: &AppState, groepen: &[u8]) {
    let nodig = groepen.iter().copied().max().map(|m| m + 1).unwrap_or(1);
    {
        let mut n = state.num_wind_groups.write();
        if *n < nodig {
            *n = nodig.min(8);
        }
    }
    {
        let mut wg = state.division_wind_groups.write();
        for (d, &g) in groepen.iter().enumerate() {
            if d < wg.len() {
                wg[d] = g;
            }
        }
    }
    for (d, &g) in groepen.iter().enumerate() {
        state.send_audio_command(AudioCommand::SetDivisionWindGroup { division_index: d as u8, group: g });
    }
}
