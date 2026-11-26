//! Source-local compartment graphs for composite organs. The graph is retained
//! instead of collapsing two different supplies into the target manual's wind
//! group. This is JM-Orgue's bounded pneumatic approximation of the imported
//! parameters, not a reimplementation of Hauptwerk's undisclosed wind solver.
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use vpo_sampler::windwerk::{ValveControlKind, ValveControlRef, WindwerkDef, PA_PER_INCH};
#[path = "project_automation.rs"]
mod automation;
use automation::{compile_source_automation, AutomationRuntime, ProjectAutomationPlan};

pub const MAX_PROJECT_WIND_NODES: usize = 256;
pub const MAX_PROJECT_WIND_CONTROLS: usize = 4096;
pub const MAX_PROJECT_WIND_LINKS: usize = 4096;
/// Imported ODF valve travel time is not available. JM-Orgue bounds a full
/// traversal to 10 ms; this is a click-free control approximation, not an
/// assertion about the original organ's valve mechanics.
const CONTROL_TRAVEL_SECONDS: f64 = 0.010;
const AIR_DENSITY: f64 = 1.204;
const AIR_BULK_PA: f64 = 1.4 * 101_325.0;

#[derive(Debug, Clone)]
pub struct ProjectWindSource {
    pub source_id: String,
    pub windwerk: WindwerkDef,
    /// Raw source switch ID -> selected, stable project stop IDs.
    pub stop_controls: HashMap<u32, Vec<u32>>,
}
#[derive(Debug, Clone)]
pub struct ProjectWindPipe {
    pub key: (u32, u32),
    pub source_id: String,
    pub lade: u32,
    pub kg_s: f32,
    pub ref_inch: Option<f32>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ProjectWindNode {
    pub source_id: String,
    pub compartment_id: u32,
    pub name: String,
    pub nominal_pa: f64,
    pub infinite: bool,
    pub volume_m3: f64,
    pub bellows: bool,
    pub compliance_kg_per_pa: f64,
    pub bellows_damping_ratio: f64,
    pub mechanical: Option<ProjectBellows>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ProjectBellows {
    pub volume_area_m2: f64,
    pub maximum_extension_m: f64,
    pub effective_mass_kg: f64,
    pub damping_n_s_per_m: f64,
    pub initial_extension_fraction: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ProjectWindLink {
    pub from: usize,
    pub to: usize,
    pub kg_s_per_sqrt_pa: f64,
    pub reference_pa: f64,
    pub controlled_valve: bool,
    pub control: Option<usize>,
    pub open_if_engaged: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct ProjectWindControl {
    pub id: String,
    pub source_id: String,
    pub source_control_id: u32,
    pub kind: ValveControlKind,
    pub name: String,
    pub default_value: f32,
    pub project_stop_ids: Vec<u32>,
    pub travel_seconds: f32,
    pub automatic: bool,
}
#[derive(Debug, Clone, Copy)]
pub struct ProjectPipeWind {
    pub node: usize,
    pub kg_s: f32,
}
#[derive(Debug, Clone)]
pub struct ProjectWindPlan {
    pub nodes: Vec<ProjectWindNode>,
    pub links: Vec<ProjectWindLink>,
    pub controls: Vec<ProjectWindControl>,
    pub automation: ProjectAutomationPlan,
    pub pipes: HashMap<(u32, u32), ProjectPipeWind>,
    pub warnings: Vec<String>,
}

fn nonnegative(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}
fn normalise_control(kind: ValveControlKind, value: f32) -> f32 {
    match kind {
        ValveControlKind::Switch => {
            if value >= 0.5 {
                1.0
            } else {
                0.0
            }
        }
        ValveControlKind::Continuous => value.clamp(0.0, 1.0),
    }
}

pub fn compile_project_wind(
    sources: &[ProjectWindSource],
    pipes: &[ProjectWindPipe],
) -> Result<ProjectWindPlan, String> {
    let mut plan = ProjectWindPlan {
        nodes: Vec::new(),
        links: Vec::new(),
        controls: Vec::new(),
        automation: ProjectAutomationPlan::default(),
        pipes: HashMap::new(),
        warnings: Vec::new(),
    };
    let mut source_ids = HashSet::new();
    let mut slots = HashMap::new();
    for source in sources {
        if source.source_id.is_empty() || !source_ids.insert(source.source_id.clone()) {
            return Err("Windnetwerken hebben unieke bron-ID's nodig".into());
        }
        if source.windwerk.is_leeg() {
            continue;
        }
        let mut control_slots = HashMap::new();
        for control in &source.windwerk.controls {
            let kind = match control.kind {
                ValveControlKind::Switch => "switch",
                ValveControlKind::Continuous => "continuous",
            };
            if control.id == 0
                || !control.default_value.is_finite()
                || !(0.0..=1.0).contains(&control.default_value)
                || !control.issues.is_empty()
            {
                return Err(format!(
                    "Bron {}: onbruikbare windklepbesturing {} {}: {}",
                    source.source_id,
                    kind,
                    control.id,
                    control.issues.join("; ")
                ));
            }
            let key = (kind, control.id);
            if control_slots.insert(key, plan.controls.len()).is_some() {
                return Err(format!(
                    "Dubbele windklepbesturing {} {} in bron {}",
                    kind, control.id, source.source_id
                ));
            }
            if control.automatic
                && control.kind == ValveControlKind::Switch
                && source
                    .stop_controls
                    .get(&control.id)
                    .is_some_and(|stops| !stops.is_empty())
            {
                return Err(format!("Bron {}: schakelaar {} met automatische bronbesturing wordt ook door een geselecteerd register bediend; een gedeeld overschrijfbeleid ontbreekt", source.source_id, control.id));
            }
            let mut project_stop_ids =
                if control.kind == ValveControlKind::Switch && !control.automatic {
                    source
                        .stop_controls
                        .get(&control.id)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
            project_stop_ids.sort_unstable();
            project_stop_ids.dedup();
            plan.controls.push(ProjectWindControl {
                id: format!("{}:{}:{}", source.source_id, kind, control.id),
                source_id: source.source_id.clone(),
                source_control_id: control.id,
                kind: control.kind,
                name: control.name.clone(),
                default_value: normalise_control(control.kind, control.default_value),
                project_stop_ids,
                travel_seconds: CONTROL_TRAVEL_SECONDS as f32,
                automatic: control.automatic,
            });
        }
        // Unconnected, unused source compartments are not part of the
        // compiled physical network. Keep every linkage endpoint (including
        // constant-open networks), every selected pipe supply and every
        // physical sensor. Never invent a reference pressure for an unused
        // isolated placeholder. Linkage order stays intact for flow sensors.
        let mut retained: HashSet<u32> = source
            .windwerk
            .verbindingen
            .iter()
            .flat_map(|link| [link.van, link.naar])
            .collect();
        retained.extend(
            pipes
                .iter()
                .filter(|pipe| pipe.source_id == source.source_id)
                .map(|pipe| pipe.lade),
        );
        retained.extend(
            source
                .windwerk
                .automation
                .outputs
                .iter()
                .filter_map(|output| match output.kind {
                    vpo_sampler::windwerk::WindControlOutputKind::BellowsExtension {
                        compartment_id,
                    }
                    | vpo_sampler::windwerk::WindControlOutputKind::Pressure { compartment_id } => {
                        Some(compartment_id)
                    }
                    vpo_sampler::windwerk::WindControlOutputKind::Flow { .. } => None,
                }),
        );
        let mut local_ids = HashSet::new();
        let mut pressures = HashMap::new();
        for compartment in &source.windwerk.laden {
            if !local_ids.insert(compartment.id) {
                return Err(format!(
                    "Dubbele windlade {} in bron {}",
                    compartment.id, source.source_id
                ));
            }
            if !retained.contains(&compartment.id) {
                continue;
            }
            if !nonnegative(compartment.volume_m3)
                || compartment
                    .druk_inch
                    .is_some_and(|p| !p.is_finite() || p <= 0.0)
            {
                return Err(format!(
                    "Ongeldige druk of inhoud van windlade '{}'",
                    compartment.naam
                ));
            }
            if let Some(b) = &compartment.balg {
                if ![b.breedte_m, b.lengte_m, b.slag_m, b.massa_kg, b.demping]
                    .into_iter()
                    .all(nonnegative)
                {
                    return Err(format!(
                        "Ongeldige balggegevens voor '{}'",
                        compartment.naam
                    ));
                }
            }
            let given = compartment.druk_inch.or_else(|| {
                compartment.balg.as_ref().and_then(|b| {
                    if b.shape_code == Some(2) && b.movement_direction_code == Some(1) {
                        let center = b.gravity_center_distance_m?;
                        let area = b.oppervlak_m2();
                        (b.lengte_m > 0.0 && center > 0.0 && area > 0.0 && b.massa_kg > 0.0).then(
                            || {
                                b.massa_kg * 9.81 * (center / b.lengte_m)
                                    / (area * 0.5)
                                    / PA_PER_INCH
                            },
                        )
                    } else {
                        b.druk_inch()
                    }
                })
            });
            if let Some(pressure) = given {
                pressures.insert(compartment.id, f64::from(pressure * PA_PER_INCH));
            } else if compartment.oneindig {
                pressures.insert(compartment.id, 0.0);
            }
        }
        for link in &source.windwerk.verbindingen {
            if !local_ids.contains(&link.van)
                || !local_ids.contains(&link.naar)
                || link.van == link.naar
            {
                return Err(format!(
                    "Windverbinding '{}' heeft een ontbrekende of gelijke begin-/eindlade",
                    link.naam
                ));
            }
            if !nonnegative(link.stroom_kg_s)
                || !link.ref_inch.is_finite()
                || (link.stroom_kg_s > 0.0 && link.ref_inch <= 0.0)
            {
                return Err(format!(
                    "Ongeldige doorstroming van windverbinding '{}'",
                    link.naam
                ));
            }
        }
        // A downstream chest without its own pressure inherits the source
        // reference. No cross-source fallback or arbitrary A440-like default.
        for _ in 0..source.windwerk.laden.len() {
            let mut changed = false;
            for compartment in &source.windwerk.laden {
                if !retained.contains(&compartment.id) {
                    continue;
                }
                if pressures.contains_key(&compartment.id) {
                    continue;
                }
                let pressure = source
                    .windwerk
                    .verbindingen
                    .iter()
                    .filter(|link| link.naar == compartment.id)
                    .filter_map(|link| pressures.get(&link.van).copied())
                    .filter(|p| *p > 0.0)
                    .reduce(f64::max);
                if let Some(pressure) = pressure {
                    pressures.insert(compartment.id, pressure);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for compartment in &source.windwerk.laden {
            if !retained.contains(&compartment.id) {
                continue;
            }
            let nominal = pressures.get(&compartment.id).copied().ok_or_else(|| {
                format!(
                    "Windlade '{}' heeft geen bruikbare of afleidbare referentiedruk",
                    compartment.naam
                )
            })?;
            let area_stroke = compartment
                .balg
                .as_ref()
                .map(|b| f64::from(b.oppervlak_m2() * b.slag_m))
                .unwrap_or(0.0);
            let explicit_feedback =
                source
                    .windwerk
                    .automation
                    .outputs
                    .iter()
                    .any(|output| match output.kind {
                        vpo_sampler::windwerk::WindControlOutputKind::BellowsExtension {
                            compartment_id,
                        }
                        | vpo_sampler::windwerk::WindControlOutputKind::Pressure {
                            compartment_id,
                        } => compartment_id == compartment.id,
                        _ => false,
                    });
            if explicit_feedback {
                if let Some(b) = &compartment.balg {
                    if !matches!(b.shape_code, None | Some(1 | 2))
                        || !matches!(b.movement_direction_code, None | Some(1))
                        || b.shape_code == Some(2)
                            && (b.lengte_m <= 0.0
                                || b.gravity_center_distance_m.is_none_or(|center| {
                                    !center.is_finite() || center <= 0.0 || center > b.lengte_m
                                }))
                    {
                        return Err(format!(
                            "Niet ondersteunde balggeometrie voor '{}' (vorm {:?}, beweging {:?})",
                            compartment.naam, b.shape_code, b.movement_direction_code
                        ));
                    }
                }
            }
            let mechanical = if explicit_feedback {
                compartment.balg.as_ref().map(|b| {
                let hinged = b.shape_code == Some(2) && b.movement_direction_code == Some(1);
                let lever = if hinged { f64::from(b.gravity_center_distance_m.unwrap_or(b.lengte_m * 0.5) / b.lengte_m) } else { 1.0 };
                let initial_extension_fraction = source.windwerk.automation.outputs.iter().find_map(|output| {
                    if matches!(output.kind, vpo_sampler::windwerk::WindControlOutputKind::BellowsExtension { compartment_id } if compartment_id == compartment.id) {
                        source.windwerk.controls.iter().find(|c| c.kind == ValveControlKind::Continuous && c.id == output.control_id).map(|c| f64::from(c.default_value))
                    } else { None }
                }).unwrap_or(0.5);
                ProjectBellows { volume_area_m2: f64::from(b.oppervlak_m2()) * if hinged { 0.5 } else { 1.0 },
                    maximum_extension_m: f64::from(b.slag_m), effective_mass_kg: f64::from(b.moving_inertia_mass_kg.unwrap_or(b.massa_kg)) * lever * lever,
                    damping_n_s_per_m: f64::from(b.demping), initial_extension_fraction }
            })
            } else {
                None
            };
            if mechanical.as_ref().is_some_and(|b| {
                !b.volume_area_m2.is_finite()
                    || b.volume_area_m2 <= 0.0
                    || !b.maximum_extension_m.is_finite()
                    || b.maximum_extension_m <= 0.0
                    || !b.effective_mass_kg.is_finite()
                    || b.effective_mass_kg <= 0.0
                    || !b.damping_n_s_per_m.is_finite()
                    || b.damping_n_s_per_m < 0.0
                    || !b.initial_extension_fraction.is_finite()
                    || !(0.0..=1.0).contains(&b.initial_extension_fraction)
                    || compartment.oneindig
            }) {
                return Err(format!(
                    "Onbruikbare mechanische balggegevens voor '{}'",
                    compartment.naam
                ));
            }
            let volume = if mechanical.is_some() {
                f64::from(compartment.volume_m3).max(1e-6)
            } else {
                f64::from(compartment.volume_m3).max(area_stroke * 0.5)
            };
            if !compartment.oneindig && (nominal <= 0.0 || volume <= 0.0) {
                return Err(format!(
                    "Windlade '{}' heeft geen positieve inhoud/referentiedruk",
                    compartment.naam
                ));
            }
            let air_compliance = AIR_DENSITY * volume / AIR_BULK_PA;
            // A loaded moving bellows stores air through board travel as well
            // as compression. Its stroke is spread over a 5% working band.
            let bellows_compliance = if nominal > 0.0 && mechanical.is_none() {
                AIR_DENSITY * area_stroke / (nominal * 0.05)
            } else {
                0.0
            };
            let bellows_damping_ratio = compartment
                .balg
                .as_ref()
                .and_then(|b| {
                    b.eigenfrequentie_hz(volume as f32).and_then(|frequency| {
                        (b.massa_kg > 0.0).then(|| {
                            f64::from(b.demping)
                                / (2.0
                                    * f64::from(b.massa_kg)
                                    * 2.0
                                    * std::f64::consts::PI
                                    * f64::from(frequency))
                        })
                    })
                })
                .unwrap_or(0.0);
            if !nominal.is_finite()
                || !volume.is_finite()
                || !(air_compliance + bellows_compliance).is_finite()
                || !bellows_damping_ratio.is_finite()
            {
                return Err(format!(
                    "Windlade '{}' heeft onbegrensde afgeleide druk/inhoud",
                    compartment.naam
                ));
            }
            let slot = plan.nodes.len();
            slots.insert((source.source_id.clone(), compartment.id), slot);
            plan.nodes.push(ProjectWindNode {
                source_id: source.source_id.clone(),
                compartment_id: compartment.id,
                name: compartment.naam.clone(),
                nominal_pa: nominal,
                infinite: compartment.oneindig,
                volume_m3: volume,
                bellows: compartment.balg.is_some(),
                compliance_kg_per_pa: (air_compliance + bellows_compliance).max(1e-10),
                bellows_damping_ratio: bellows_damping_ratio.clamp(0.0, 20.0),
                mechanical,
            });
        }
        let first_link = plan.links.len();
        for link in &source.windwerk.verbindingen {
            let reference_pa = f64::from(link.ref_inch.max(0.0001) * PA_PER_INCH);
            if !reference_pa.is_finite() {
                return Err(format!(
                    "Windverbinding '{}' heeft onbegrensde referentiedruk",
                    link.naam
                ));
            }
            let (control, open_if_engaged) =
                match &link.valve_control {
                    Some(ValveControlRef::Switch {
                        control_id,
                        open_if_engaged,
                    }) => (
                        Some(*control_slots.get(&("switch", *control_id)).ok_or_else(|| {
                            format!(
                                "Windverbinding '{}' mist schakelaar {} in bron {}",
                                link.naam, control_id, source.source_id
                            )
                        })?),
                        *open_if_engaged,
                    ),
                    Some(ValveControlRef::Continuous { control_id }) => (
                        Some(*control_slots.get(&("continuous", *control_id)).ok_or_else(
                            || {
                                format!(
                                    "Windverbinding '{}' mist continue besturing {} in bron {}",
                                    link.naam, control_id, source.source_id
                                )
                            },
                        )?),
                        true,
                    ),
                    Some(ValveControlRef::Unsupported { type_code, reason }) => {
                        return Err(format!(
                            "Windverbinding '{}' heeft niet ondersteunde klepbesturing {}: {}",
                            link.naam, type_code, reason
                        ))
                    }
                    None if link.klep => {
                        return Err(format!(
                            "Windverbinding '{}' in bron {} mist een bruikbare klepbesturings-ID",
                            link.naam, source.source_id
                        ))
                    }
                    None => (None, true),
                };
            plan.links.push(ProjectWindLink {
                from: slots[&(source.source_id.clone(), link.van)],
                to: slots[&(source.source_id.clone(), link.naar)],
                kg_s_per_sqrt_pa: f64::from(link.stroom_kg_s) / reference_pa.sqrt(),
                reference_pa,
                controlled_valve: link.klep || control.is_some(),
                control,
                open_if_engaged,
            });
        }
        compile_source_automation(&mut plan, source, &control_slots, &slots, first_link)?;
    }
    plan.automation.prepare(&plan.controls)?;
    if plan.nodes.len() > MAX_PROJECT_WIND_NODES
        || plan.links.len() > MAX_PROJECT_WIND_LINKS
        || plan.controls.len() > MAX_PROJECT_WIND_CONTROLS
    {
        return Err(format!("Projectwindnetwerk is te groot: maximaal {MAX_PROJECT_WIND_NODES} laden, {MAX_PROJECT_WIND_LINKS} verbindingen en {MAX_PROJECT_WIND_CONTROLS} besturingen"));
    }
    for pipe in pipes {
        let node = slots
            .get(&(pipe.source_id.clone(), pipe.lade))
            .copied()
            .ok_or_else(|| {
                format!(
                    "Pijp {:?} verwijst naar ontbrekende bronwindlade {}",
                    pipe.key, pipe.lade
                )
            })?;
        if !pipe.kg_s.is_finite()
            || pipe.kg_s <= 0.0
            || pipe.ref_inch.is_some_and(|p| !p.is_finite() || p <= 0.0)
        {
            return Err(format!(
                "Pijp {:?} heeft geen bruikbaar windverbruik/referentiedruk",
                pipe.key
            ));
        }
        let nominal = plan.nodes[node].nominal_pa;
        if nominal <= 0.0 {
            return Err(format!(
                "Pijp {:?} wordt gevoed uit een drukloze buitenluchtlade",
                pipe.key
            ));
        }
        let ref_pa = pipe
            .ref_inch
            .map(|p| f64::from(p * PA_PER_INCH))
            .unwrap_or(nominal);
        let kg_s = f64::from(pipe.kg_s) * (nominal / ref_pa).sqrt();
        if !kg_s.is_finite() || kg_s > 100.0 {
            return Err(format!(
                "Pijp {:?} heeft onrealistisch windverbruik",
                pipe.key
            ));
        }
        if plan
            .pipes
            .insert(
                pipe.key,
                ProjectPipeWind {
                    node,
                    kg_s: kg_s as f32,
                },
            )
            .is_some()
        {
            return Err(format!(
                "Dubbele pijpsleutel {:?} in projectwindnetwerk",
                pipe.key
            ));
        }
        if pipe.ref_inch.is_none()
            && !plan
                .warnings
                .iter()
                .any(|w| w.contains("ontbrekende pijpreferentiedruk"))
        {
            plan.warnings.push("Bij ontbrekende pijpreferentiedruk geldt het opgegeven kg/s-verbruik bij de nominale bronlagedruk.".into());
        }
    }
    Ok(plan)
}

/// Prepared outside the callback. All integration buffers are fixed-size and
/// per-frame processing performs neither allocation nor a graph lookup.
#[derive(Debug, Clone)]
pub struct ProjectWindRuntime {
    pub plan: std::sync::Arc<ProjectWindPlan>,
    pressure: Vec<f64>,
    demand: [f64; MAX_PROJECT_WIND_NODES],
    net: [f64; MAX_PROJECT_WIND_NODES],
    conductance: [f64; MAX_PROJECT_WIND_NODES],
    smooth: [f32; MAX_PROJECT_WIND_NODES],
    target: [f32; MAX_PROJECT_WIND_NODES],
    control_values: Vec<f32>,
    control_targets: Vec<f32>,
    stop_slots: HashMap<u32, usize>,
    stop_states: Vec<bool>,
    control_stop_slots: Vec<Vec<usize>>,
    link_flows: Vec<f64>,
    bellows_positions: Vec<f64>,
    bellows_velocities: Vec<f64>,
    bellows_extensions: Vec<f64>,
    automation: AutomationRuntime,
    ticks: u64,
    interval: usize,
    phase: usize,
    dt: f64,
    alpha: f32,
    enabled: bool,
}
impl ProjectWindRuntime {
    pub fn new(plan: &ProjectWindPlan, sample_rate: u32) -> Result<Self, String> {
        if !(8_000..=384_000).contains(&sample_rate)
            || plan.nodes.len() > MAX_PROJECT_WIND_NODES
            || plan.links.len() > MAX_PROJECT_WIND_LINKS
            || plan.controls.len() > MAX_PROJECT_WIND_CONTROLS
        {
            return Err("Ongeldige samplerate of grootte voor projectwindnetwerk".into());
        }
        if plan.nodes.iter().any(|n| {
            !n.nominal_pa.is_finite()
                || !n.compliance_kg_per_pa.is_finite()
                || n.compliance_kg_per_pa <= 0.0
                || !n.bellows_damping_ratio.is_finite()
        }) || plan.links.iter().any(|link| {
            link.from >= plan.nodes.len()
                || link.to >= plan.nodes.len()
                || !link.kg_s_per_sqrt_pa.is_finite()
                || link.kg_s_per_sqrt_pa < 0.0
                || !link.reference_pa.is_finite()
                || link.reference_pa <= 0.0
                || link.control.is_some_and(|i| i >= plan.controls.len())
                || (link.controlled_valve && link.control.is_none())
        }) || plan.controls.iter().any(|control| {
            !control.default_value.is_finite()
                || !(0.0..=1.0).contains(&control.default_value)
                || !control.travel_seconds.is_finite()
                || control.travel_seconds <= 0.0
        }) || plan
            .pipes
            .values()
            .any(|pipe| pipe.node >= plan.nodes.len() || !pipe.kg_s.is_finite() || pipe.kg_s <= 0.0)
        {
            return Err("Ongeldig voorbereid projectwindnetwerk".into());
        }
        let interval = (sample_rate as usize / 1000).max(1);
        let dt = interval as f64 / sample_rate as f64;
        let mut stop_slots = HashMap::new();
        let mut stop_states = Vec::new();
        let mut control_stop_slots = Vec::with_capacity(plan.controls.len());
        for control in &plan.controls {
            let mut slots = Vec::with_capacity(control.project_stop_ids.len());
            for stop in &control.project_stop_ids {
                let slot = *stop_slots.entry(*stop).or_insert_with(|| {
                    let index = stop_states.len();
                    stop_states.push(false);
                    index
                });
                if control.default_value >= 0.5 {
                    stop_states[slot] = true;
                }
                slots.push(slot);
            }
            control_stop_slots.push(slots);
        }
        Ok(Self {
            plan: std::sync::Arc::new(plan.clone()),
            pressure: plan.nodes.iter().map(|n| n.nominal_pa).collect(),
            demand: [0.0; MAX_PROJECT_WIND_NODES],
            net: [0.0; MAX_PROJECT_WIND_NODES],
            conductance: [0.0; MAX_PROJECT_WIND_NODES],
            smooth: [0.0; MAX_PROJECT_WIND_NODES],
            target: [0.0; MAX_PROJECT_WIND_NODES],
            control_values: plan
                .controls
                .iter()
                .map(|c| normalise_control(c.kind, c.default_value))
                .collect(),
            control_targets: plan
                .controls
                .iter()
                .map(|c| normalise_control(c.kind, c.default_value))
                .collect(),
            stop_slots,
            stop_states,
            control_stop_slots,
            link_flows: vec![0.0; plan.links.len()],
            bellows_positions: plan
                .nodes
                .iter()
                .map(|n| {
                    n.mechanical
                        .as_ref()
                        .map(|b| b.initial_extension_fraction * b.maximum_extension_m)
                        .unwrap_or(0.0)
                })
                .collect(),
            bellows_velocities: vec![0.0; plan.nodes.len()],
            bellows_extensions: plan
                .nodes
                .iter()
                .map(|n| {
                    n.mechanical
                        .as_ref()
                        .map(|b| b.initial_extension_fraction)
                        .unwrap_or(0.0)
                })
                .collect(),
            automation: AutomationRuntime::new(plan, dt),
            ticks: 0,
            interval,
            phase: 0,
            dt,
            alpha: (1.0 - (-1.0 / (0.004 * f64::from(sample_rate))).exp()) as f32,
            enabled: true,
        })
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            for (pressure, node) in self.pressure.iter_mut().zip(&self.plan.nodes) {
                *pressure = node.nominal_pa;
            }
            self.smooth.fill(0.0);
            self.target.fill(0.0);
            self.demand.fill(0.0);
            self.link_flows.fill(0.0);
            self.bellows_velocities.fill(0.0);
            self.phase = 0;
        }
    }
    /// The index is resolved before dispatch. Invalid indices/non-finite data
    /// never alter a previous controller, and the callback allocates nothing.
    pub fn set_control(&mut self, index: usize, value: f32) -> bool {
        let Some(control) = self.plan.controls.get(index) else {
            return false;
        };
        if !value.is_finite() || control.automatic {
            return false;
        }
        self.control_targets[index] = normalise_control(control.kind, value);
        true
    }
    /// A source switch shared by selected project stops is engaged while any
    /// of those stops is drawn. Storage and bindings were prepared at load.
    pub fn set_stop_drawn(&mut self, stop_id: u32, drawn: bool) -> bool {
        let Some(&slot) = self.stop_slots.get(&stop_id) else {
            return false;
        };
        self.stop_states[slot] = drawn;
        for (index, slots) in self.control_stop_slots.iter().enumerate() {
            if !self.plan.controls[index].automatic && slots.contains(&slot) {
                self.control_targets[index] = if slots.iter().any(|i| self.stop_states[*i]) {
                    1.0
                } else {
                    0.0
                };
            }
        }
        true
    }
    pub fn control_value(&self, index: usize) -> f32 {
        self.control_values.get(index).copied().unwrap_or(0.0)
    }
    pub fn valve_opening(&self, link_index: usize) -> f32 {
        let Some(link) = self.plan.links.get(link_index) else {
            return 0.0;
        };
        match link.control {
            Some(index) if !link.open_if_engaged => 1.0 - self.control_values[index],
            Some(index) => self.control_values[index],
            None => 1.0,
        }
    }
    pub fn link_flow_kg_s(&self, index: usize) -> f32 {
        self.link_flows.get(index).copied().unwrap_or(0.0) as f32
    }
    pub fn bellows_extension(&self, node: usize) -> f32 {
        self.bellows_extensions.get(node).copied().unwrap_or(0.0) as f32
    }
    pub fn bellows_position(&self, node: usize) -> f32 {
        self.bellows_positions.get(node).copied().unwrap_or(0.0) as f32
    }
    pub fn bellows_velocity(&self, node: usize) -> f32 {
        self.bellows_velocities.get(node).copied().unwrap_or(0.0) as f32
    }
    pub fn model_time_seconds(&self) -> f64 {
        self.ticks as f64 * self.dt
    }
    pub fn control_tick_count(&self) -> u64 {
        self.ticks
    }
    fn advance_controls(&mut self) {
        for (i, control) in self.plan.controls.iter().enumerate() {
            if control.automatic {
                continue;
            }
            let difference = self.control_targets[i] - self.control_values[i];
            let step = (self.dt / f64::from(control.travel_seconds)).min(1.0) as f32;
            if difference.abs() <= step {
                self.control_values[i] = self.control_targets[i];
            } else {
                self.control_values[i] += step.copysign(difference);
            }
        }
        self.automation.tick(
            &self.plan,
            &mut self.control_values,
            &self.bellows_extensions,
            &self.pressure,
            &self.link_flows,
        );
    }
    pub fn set_demands(&mut self, demands: &[f64; MAX_PROJECT_WIND_NODES]) {
        self.demand.copy_from_slice(demands);
    }
    pub fn pipe(&self, stop: u32, layer_pipe: u32) -> Option<ProjectPipeWind> {
        self.plan.pipes.get(&(stop, layer_pipe)).copied()
    }
    pub fn pressure_ratio(&self, node: usize) -> f32 {
        let nominal = self
            .plan
            .nodes
            .get(node)
            .map(|n| n.nominal_pa)
            .unwrap_or(0.0);
        if nominal > 0.0 {
            (self.pressure[node] / nominal) as f32
        } else {
            1.0
        }
    }
    fn integrate(&mut self) {
        for (i, node) in self.plan.nodes.iter().enumerate() {
            let ratio = if node.nominal_pa > 0.0 {
                (self.pressure[i] / node.nominal_pa).max(0.0)
            } else {
                0.0
            };
            self.net[i] = -self.demand[i] * ratio.sqrt();
            self.conductance[i] = if node.nominal_pa > 0.0 {
                self.demand[i] / (2.0 * node.nominal_pa)
            } else {
                0.0
            };
        }
        for (index, link) in self.plan.links.iter().enumerate() {
            let opening = f64::from(self.valve_opening(index));
            if opening == 0.0 {
                // A shut valve exchanges no mass, including no numerical
                // conductance or artificial recovery path.
                self.link_flows[index] = 0.0;
                continue;
            }
            let difference = self.pressure[link.from] - self.pressure[link.to];
            let mut flow =
                opening * link.kg_s_per_sqrt_pa * difference.abs().sqrt() * difference.signum();
            // Bellows inlet regulation: the target pressure closes the inlet;
            // a falling board reopens it. The compartment and link stay distinct.
            let destination = &self.plan.nodes[link.to];
            let gate = if flow > 0.0
                && destination.bellows
                && destination.mechanical.is_none()
                && !destination.infinite
            {
                ((destination.nominal_pa - self.pressure[link.to])
                    / (destination.nominal_pa * 0.005))
                    .clamp(0.0, 1.0)
            } else {
                1.0
            };
            flow *= gate;
            // A square-root flow has an infinite derivative at equal pressure.
            // With a finite control step its unconstrained mass transfer could
            // overshoot equilibrium and keep an unloaded chest oscillating.
            // Do not move more air than would equalise this pair's pressures
            // in this step. Fixed-pressure sources have infinite capacity.
            let inverse_from = if self.plan.nodes[link.from].infinite {
                0.0
            } else {
                1.0 / self.plan.nodes[link.from].compliance_kg_per_pa
            };
            let inverse_to = if destination.infinite {
                0.0
            } else {
                1.0 / destination.compliance_kg_per_pa
            };
            let inverse_capacity = inverse_from + inverse_to;
            if inverse_capacity > 0.0 {
                let equalising_flow = difference.abs() / (self.dt * inverse_capacity);
                flow = flow.clamp(-equalising_flow, equalising_flow);
            }
            self.link_flows[index] = flow;
            let slope = opening * gate * link.kg_s_per_sqrt_pa
                / (2.0 * difference.abs().max(link.reference_pa * 0.02).sqrt());
            self.net[link.from] -= flow;
            self.net[link.to] += flow;
            self.conductance[link.from] += slope;
            self.conductance[link.to] += slope;
        }
        for (i, node) in self.plan.nodes.iter().enumerate() {
            if node.infinite {
                self.pressure[i] = node.nominal_pa;
                continue;
            }
            if let Some(bellows) = &node.mechanical {
                // Backward Euler couples the board's force and air mass in
                // one solve. Calibrate static load to the imported nominal
                // pressure; shape/lever geometry is compiled before playback.
                let position = self.bellows_positions[i];
                let volume = node.volume_m3 + bellows.volume_area_m2 * position;
                let compliance = (AIR_DENSITY * volume / AIR_BULK_PA).max(1e-12);
                let pressure_step = self.dt / (compliance + self.dt * self.conductance[i]);
                let area = bellows.volume_area_m2;
                let denominator = bellows.effective_mass_kg
                    + self.dt * bellows.damping_n_s_per_m
                    + self.dt * area * pressure_step * AIR_DENSITY * area;
                let velocity = (bellows.effective_mass_kg * self.bellows_velocities[i]
                    + self.dt
                        * area
                        * (self.pressure[i] + pressure_step * self.net[i] - node.nominal_pa))
                    / denominator;
                let next = (position + self.dt * velocity).clamp(0.0, bellows.maximum_extension_m);
                let actual_velocity = (next - position) / self.dt;
                self.pressure[i] = (self.pressure[i]
                    + pressure_step * (self.net[i] - AIR_DENSITY * area * actual_velocity))
                    .clamp(0.0, node.nominal_pa * 1.15);
                self.bellows_positions[i] = next;
                self.bellows_velocities[i] = if next == 0.0 || next == bellows.maximum_extension_m {
                    0.0
                } else {
                    actual_velocity
                };
                self.bellows_extensions[i] = next / bellows.maximum_extension_m;
                continue;
            }
            // Diagonally implicit, under-relaxed pressure update. The positive
            // denominator bounds stiff tiny air chests without timestep loops.
            let change = 0.5 * self.dt * self.net[i]
                / ((node.compliance_kg_per_pa + self.dt * self.conductance[i])
                    * (1.0 + node.bellows_damping_ratio));
            let limit = node.nominal_pa * 0.1;
            self.pressure[i] =
                (self.pressure[i] + change.clamp(-limit, limit)).clamp(0.0, node.nominal_pa * 1.15);
        }
        for (i, node) in self.plan.nodes.iter().enumerate() {
            self.target[i] = if node.nominal_pa > 0.0 {
                (self.pressure[i] / node.nominal_pa - 1.0).clamp(-1.0, 0.1) as f32
            } else {
                0.0
            };
        }
    }
    pub fn process_frame(&mut self, output: &mut [f32; MAX_PROJECT_WIND_NODES]) {
        if self.phase == 0 {
            self.advance_controls();
            if self.enabled {
                self.integrate();
            }
            self.ticks = self.ticks.saturating_add(1);
        }
        self.phase += 1;
        if self.phase == self.interval {
            self.phase = 0;
        }
        if !self.enabled {
            output.fill(0.0);
            return;
        }
        for i in 0..self.plan.nodes.len() {
            self.smooth[i] += self.alpha * (self.target[i] - self.smooth[i]);
            output[i] = self.smooth[i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vpo_sampler::windwerk::{ValveControlDef, WindLadeDef, WindVerbindingDef};
    fn source(id: &str) -> ProjectWindSource {
        ProjectWindSource {
            source_id: id.into(),
            windwerk: WindwerkDef {
                laden: vec![
                    WindLadeDef {
                        id: 1,
                        naam: "Ventilator".into(),
                        oneindig: true,
                        druk_inch: Some(4.0),
                        ..Default::default()
                    },
                    WindLadeDef {
                        id: 2,
                        naam: "Lade".into(),
                        volume_m3: 0.08,
                        druk_inch: Some(4.0),
                        ..Default::default()
                    },
                ],
                verbindingen: vec![WindVerbindingDef {
                    van: 1,
                    naar: 2,
                    stroom_kg_s: 0.02,
                    ref_inch: 1.0,
                    ..Default::default()
                }],
                ..Default::default()
            },
            stop_controls: HashMap::new(),
        }
    }
    fn pipe(stop: u32, source: &str) -> ProjectWindPipe {
        ProjectWindPipe {
            key: (stop, 1),
            source_id: source.into(),
            lade: 2,
            kg_s: 0.008,
            ref_inch: Some(4.0),
        }
    }
    #[test]
    fn duplicate_source_compartment_numbers_remain_independent_under_load() {
        let plan =
            compile_project_wind(&[source("A"), source("B")], &[pipe(10, "A"), pipe(20, "B")])
                .unwrap();
        assert_eq!(plan.nodes.len(), 4);
        assert_eq!(plan.links.len(), 2);
        let a = plan.pipes[&(10, 1)];
        let b = plan.pipes[&(20, 1)];
        assert_ne!(a.node, b.node);
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        let mut demands = [0.0; MAX_PROJECT_WIND_NODES];
        demands[a.node] = f64::from(a.kg_s);
        model.set_demands(&demands);
        let mut output = [0.0; MAX_PROJECT_WIND_NODES];
        for _ in 0..48_000 {
            model.process_frame(&mut output);
        }
        assert!(model.pressure_ratio(a.node) < 0.995);
        assert_eq!(model.pressure_ratio(b.node), 1.0);
        assert!(output[a.node] < 0.0);
        assert_eq!(output[b.node], 0.0);
    }
    #[test]
    fn connected_compartments_share_load_and_recover_without_cross_source_links() {
        let mut a = source("A");
        a.windwerk.laden.push(WindLadeDef {
            id: 3,
            naam: "Tweede lade".into(),
            volume_m3: 0.05,
            druk_inch: Some(4.0),
            ..Default::default()
        });
        a.windwerk.verbindingen.push(WindVerbindingDef {
            van: 2,
            naar: 3,
            stroom_kg_s: 0.04,
            ref_inch: 1.0,
            ..Default::default()
        });
        let mut p = pipe(10, "A");
        p.lade = 3;
        let plan = compile_project_wind(&[a], &[p]).unwrap();
        let node = plan.pipes[&(10, 1)].node;
        let mut model = ProjectWindRuntime::new(&plan, 44100).unwrap();
        let mut demands = [0.0; MAX_PROJECT_WIND_NODES];
        demands[node] = 0.008;
        model.set_demands(&demands);
        let mut output = [0.0; MAX_PROJECT_WIND_NODES];
        for _ in 0..44_100 {
            model.process_frame(&mut output);
        }
        let loaded = model.pressure_ratio(node);
        assert!(loaded < 0.99);
        assert!(model.pressure_ratio(1) < 0.999);
        model.set_demands(&[0.0; MAX_PROJECT_WIND_NODES]);
        for _ in 0..44_100 {
            model.process_frame(&mut output);
        }
        assert!(model.pressure_ratio(node) > loaded);
        model.set_enabled(false);
        model.process_frame(&mut output);
        assert!(output.iter().all(|v| *v == 0.0));
        assert_eq!(model.pressure_ratio(node), 1.0);
    }
    #[test]
    fn invalid_or_missing_supplies_never_fall_back_to_another_source() {
        assert!(compile_project_wind(&[source("A"), source("B")], &[pipe(1, "C")]).is_err());
        let mut p = pipe(1, "A");
        p.lade = 99;
        assert!(compile_project_wind(&[source("A")], &[p]).is_err());
        let mut a = source("A");
        a.windwerk.verbindingen[0].naar = 88;
        assert!(compile_project_wind(&[a], &[]).is_err());
        let mut p = pipe(1, "A");
        p.kg_s = f32::NAN;
        assert!(compile_project_wind(&[source("A")], &[p]).is_err());
    }
    #[test]
    fn unused_isolated_compartments_are_pruned_but_selected_supplies_and_sensors_are_required() {
        use vpo_sampler::windwerk::{WindControlOutputDef, WindControlOutputKind};
        let mut source = source("A");
        source.windwerk.laden.push(WindLadeDef {
            id: 99,
            naam: "Unused placeholder".into(),
            volume_m3: 0.08,
            ..Default::default()
        });
        let plan = compile_project_wind(&[source.clone()], &[]).unwrap();
        assert_eq!(plan.nodes.len(), 2);
        assert_eq!(plan.links.len(), 1);
        assert!(plan.nodes.iter().all(|node| node.compartment_id != 99));
        let mut selected = pipe(1, "A");
        selected.lade = 99;
        assert!(compile_project_wind(&[source.clone()], &[selected.clone()])
            .unwrap_err()
            .contains("referentiedruk"));
        source.windwerk.laden.last_mut().unwrap().druk_inch = Some(4.0);
        let plan = compile_project_wind(&[source.clone()], &[selected]).unwrap();
        assert_eq!(plan.nodes.len(), 3);
        assert_eq!(plan.nodes[plan.pipes[&(1, 1)].node].compartment_id, 99);
        add_control(&mut source, 8, ValveControlKind::Continuous, 0.0, true);
        source
            .windwerk
            .automation
            .outputs
            .push(WindControlOutputDef {
                control_id: 8,
                kind: WindControlOutputKind::Pressure { compartment_id: 99 },
            });
        let plan = compile_project_wind(&[source.clone()], &[]).unwrap();
        assert_eq!(
            plan.nodes.len(),
            3,
            "a physical sensor retains its isolated compartment"
        );
        source.windwerk.laden.last_mut().unwrap().druk_inch = None;
        assert!(compile_project_wind(&[source], &[])
            .unwrap_err()
            .contains("referentiedruk"));
    }
    #[test]
    fn pruning_isolated_nodes_preserves_link_ordinals_flow_sensors_and_constant_open_networks() {
        use vpo_sampler::windwerk::{WindControlOutputDef, WindControlOutputKind};
        let a = source("A");
        let mut b = source("B");
        b.windwerk.laden.insert(
            0,
            WindLadeDef {
                id: 99,
                naam: "Unused".into(),
                ..Default::default()
            },
        );
        b.windwerk.laden.extend([
            WindLadeDef {
                id: 3,
                naam: "Independent fan".into(),
                oneindig: true,
                druk_inch: Some(4.0),
                ..Default::default()
            },
            WindLadeDef {
                id: 4,
                naam: "Independent chest".into(),
                volume_m3: 0.08,
                druk_inch: Some(4.0),
                ..Default::default()
            },
        ]);
        b.windwerk.verbindingen.push(WindVerbindingDef {
            van: 3,
            naar: 4,
            stroom_kg_s: 0.02,
            ref_inch: 1.0,
            ..Default::default()
        });
        add_control(&mut b, 17, ValveControlKind::Continuous, 0.0, true);
        b.windwerk.automation.outputs.push(WindControlOutputDef {
            control_id: 17,
            kind: WindControlOutputKind::Flow {
                link_index: 1,
                max_flow_kg_s: 0.02,
            },
        });
        let mut selected = pipe(1, "B");
        selected.lade = 4;
        let plan = compile_project_wind(&[a, b], &[selected]).unwrap();
        assert_eq!(plan.nodes.len(), 6);
        assert_eq!(plan.links.len(), 3);
        assert!(plan.nodes.iter().all(|node| node.compartment_id != 99));
        let node = plan.pipes[&(1, 1)].node;
        assert_eq!(plan.links[2].to, node);
        assert_eq!(plan.nodes[plan.links[2].from].compartment_id, 3);
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        let mut demand = [0.0; MAX_PROJECT_WIND_NODES];
        demand[node] = 0.008;
        model.set_demands(&demand);
        render(&mut model, 48000);
        assert_eq!(model.link_flow_kg_s(0), 0.0);
        assert_eq!(model.link_flow_kg_s(1), 0.0);
        assert!(model.link_flow_kg_s(2) > 0.007);
        assert!((model.control_value(0) * 0.02 - model.link_flow_kg_s(2)).abs() < 1e-6);
    }
    #[test]
    fn active_compartment_budget_is_enforced_after_unused_placeholders_are_pruned() {
        let mut source = source("A");
        source
            .windwerk
            .laden
            .extend((1000..1500).map(|id| WindLadeDef {
                id,
                ..Default::default()
            }));
        assert_eq!(
            compile_project_wind(&[source.clone()], &[])
                .unwrap()
                .nodes
                .len(),
            2
        );
        for id in 3..=MAX_PROJECT_WIND_NODES as u32 {
            source.windwerk.laden.push(WindLadeDef {
                id,
                volume_m3: 0.01,
                druk_inch: Some(4.0),
                ..Default::default()
            });
            source.windwerk.verbindingen.push(WindVerbindingDef {
                van: id - 1,
                naar: id,
                stroom_kg_s: 0.02,
                ref_inch: 1.0,
                ..Default::default()
            });
        }
        let plan = compile_project_wind(&[source.clone()], &[]).unwrap();
        assert_eq!(plan.nodes.len(), MAX_PROJECT_WIND_NODES);
        assert!(ProjectWindRuntime::new(&plan, 48000).is_ok());
        let mut oversized_plan = plan.clone();
        let mut extra = plan.nodes.last().unwrap().clone();
        extra.compartment_id += 1;
        oversized_plan.nodes.push(extra);
        let mut link = plan.links.last().unwrap().clone();
        link.from += 1;
        link.to += 1;
        oversized_plan.links.push(link);
        assert!(ProjectWindRuntime::new(&oversized_plan, 48000).is_err());
        let id = MAX_PROJECT_WIND_NODES as u32 + 1;
        source.windwerk.laden.push(WindLadeDef {
            id,
            volume_m3: 0.01,
            druk_inch: Some(4.0),
            ..Default::default()
        });
        source.windwerk.verbindingen.push(WindVerbindingDef {
            van: id - 1,
            naar: id,
            stroom_kg_s: 0.02,
            ref_inch: 1.0,
            ..Default::default()
        });
        assert!(compile_project_wind(&[source], &[])
            .unwrap_err()
            .contains("te groot"));
    }

    #[test]
    fn selected_register_cannot_silently_share_an_automatic_source_switch() {
        let mut source = controlled_source("A", ValveControlKind::Switch, 1.0);
        source.windwerk.controls[0].automatic = true;
        source.stop_controls.insert(7, vec![101]);
        let error = compile_project_wind(&[source], &[]).unwrap_err();
        assert!(
            error.contains("met automatische bronbesturing")
                && error.contains("geselecteerd register")
        );
    }
    #[test]
    fn pipe_reference_pressure_is_applied_in_physical_units() {
        let mut p = pipe(1, "A");
        p.ref_inch = Some(1.0);
        let plan = compile_project_wind(&[source("A")], &[p]).unwrap();
        assert!((plan.pipes[&(1, 1)].kg_s - 0.016).abs() < 1e-7);
    }

    #[test]
    fn released_chest_recovers_monotonically_without_idle_limit_cycle() {
        let plan = compile_project_wind(&[source("A"), source("B")], &[pipe(1, "A"), pipe(2, "B")])
            .unwrap();
        let a = plan.pipes[&(1, 1)];
        let b = plan.pipes[&(2, 1)];
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        let mut demands = [0.0; MAX_PROJECT_WIND_NODES];
        demands[a.node] = f64::from(a.kg_s);
        model.set_demands(&demands);
        let mut output = [0.0; MAX_PROJECT_WIND_NODES];
        for _ in 0..48000 {
            model.process_frame(&mut output);
        }
        // Q_pipe scales as sqrt(P), Q_link as sqrt(P_source-P). The exact
        // equilibrium for this fixture is 1 / (1 + .04) = 0.96153846.
        assert!((model.pressure_ratio(a.node) - 1.0 / 1.04).abs() < 1e-5);
        model.set_demands(&[0.0; MAX_PROJECT_WIND_NODES]);
        let mut previous = model.pressure_ratio(a.node);
        for _ in 0..48000 {
            model.process_frame(&mut output);
            let now = model.pressure_ratio(a.node);
            assert!(
                now >= previous && now <= 1.0,
                "unloaded pressure oscillates: {previous} -> {now}"
            );
            assert_eq!(model.pressure_ratio(b.node), 1.0);
            previous = now;
        }
        assert!(
            (previous - 1.0).abs() < 1e-7,
            "pressure failed to settle: {previous}"
        );
    }

    fn controlled_source(
        id: &str,
        kind: ValveControlKind,
        default_value: f32,
    ) -> ProjectWindSource {
        let mut source = source(id);
        source.windwerk.controls.push(ValveControlDef {
            id: 7,
            kind,
            name: "Toevoer".into(),
            default_value,
            source_stop_ids: Vec::new(),
            issues: Vec::new(),
            linkages: Vec::new(),
            automatic: false,
        });
        let link = &mut source.windwerk.verbindingen[0];
        link.klep = true;
        link.valve_control = Some(match kind {
            ValveControlKind::Switch => ValveControlRef::Switch {
                control_id: 7,
                open_if_engaged: true,
            },
            ValveControlKind::Continuous => ValveControlRef::Continuous { control_id: 7 },
        });
        source
    }
    fn render(model: &mut ProjectWindRuntime, frames: usize) {
        let mut output = [0.0; MAX_PROJECT_WIND_NODES];
        for _ in 0..frames {
            model.process_frame(&mut output);
        }
    }

    #[test]
    fn shut_valve_transports_no_air_and_cannot_refill_a_drained_chest() {
        let source = controlled_source("A", ValveControlKind::Switch, 0.0);
        let plan = compile_project_wind(&[source], &[pipe(1, "A")]).unwrap();
        let node = plan.pipes[&(1, 1)].node;
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        let mut demand = [0.0; MAX_PROJECT_WIND_NODES];
        demand[node] = 0.008;
        model.set_demands(&demand);
        render(&mut model, 48000);
        assert_eq!(model.valve_opening(0), 0.0);
        assert_eq!(model.link_flow_kg_s(0), 0.0);
        assert!(model.pressure_ratio(node) < 0.01);
        model.set_demands(&[0.0; MAX_PROJECT_WIND_NODES]);
        let drained = model.pressure_ratio(node);
        render(&mut model, 48000);
        assert_eq!(
            model.pressure_ratio(node),
            drained,
            "closed valve leaked a refill path"
        );
        assert!(model.set_control(0, 1.0));
        render(&mut model, 48000);
        assert!(model.pressure_ratio(node) > 0.99999);
        assert_eq!(model.valve_opening(0), 1.0);
    }

    #[test]
    fn identical_switch_ids_in_different_sources_are_independent() {
        let a = controlled_source("A", ValveControlKind::Switch, 1.0);
        let b = controlled_source("B", ValveControlKind::Switch, 1.0);
        let plan = compile_project_wind(&[a, b], &[pipe(1, "A"), pipe(2, "B")]).unwrap();
        assert_eq!(plan.controls[0].id, "A:switch:7");
        assert_eq!(plan.controls[1].id, "B:switch:7");
        let a = plan.pipes[&(1, 1)].node;
        let b = plan.pipes[&(2, 1)].node;
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        let mut demand = [0.0; MAX_PROJECT_WIND_NODES];
        demand[a] = 0.008;
        demand[b] = 0.008;
        model.set_demands(&demand);
        model.set_control(0, 0.0);
        render(&mut model, 48000);
        assert_eq!(model.valve_opening(0), 0.0);
        assert_eq!(model.valve_opening(1), 1.0);
        assert!(model.pressure_ratio(a) < 0.01);
        assert!((model.pressure_ratio(b) - 1.0 / 1.04).abs() < 1e-5);
        assert_eq!(model.link_flow_kg_s(0), 0.0);
        assert!(model.link_flow_kg_s(1) > 0.0);
    }

    #[test]
    fn switch_and_continuous_control_with_same_source_number_have_distinct_bindings() {
        let mut source = controlled_source("A", ValveControlKind::Switch, 0.0);
        source.windwerk.controls.push(ValveControlDef {
            id: 7,
            kind: ValveControlKind::Continuous,
            name: "Regelaar".into(),
            default_value: 0.25,
            source_stop_ids: Vec::new(),
            issues: Vec::new(),
            linkages: Vec::new(),
            automatic: false,
        });
        let mut continuous = source.windwerk.verbindingen[0].clone();
        continuous.valve_control = Some(ValveControlRef::Continuous { control_id: 7 });
        source.windwerk.verbindingen.push(continuous);
        let plan = compile_project_wind(&[source], &[]).unwrap();
        assert_eq!(plan.controls[0].id, "A:switch:7");
        assert_eq!(plan.controls[1].id, "A:continuous:7");
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        model.set_control(0, 1.0);
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 1.0);
        assert_eq!(model.valve_opening(1), 0.25);
        model.set_control(1, 0.5);
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 1.0);
        assert_eq!(model.valve_opening(1), 0.5);
    }

    #[test]
    fn one_switch_drives_multiple_valves_with_per_link_polarity() {
        let mut source = controlled_source("A", ValveControlKind::Switch, 0.0);
        let mut inverse = source.windwerk.verbindingen[0].clone();
        inverse.valve_control = Some(ValveControlRef::Switch {
            control_id: 7,
            open_if_engaged: false,
        });
        source.windwerk.verbindingen.push(inverse);
        let plan = compile_project_wind(&[source], &[]).unwrap();
        assert_eq!(plan.controls.len(), 1);
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        assert_eq!(model.valve_opening(0), 0.0);
        assert_eq!(model.valve_opening(1), 1.0);
        model.set_control(0, 0.51);
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 1.0);
        assert_eq!(model.valve_opening(1), 0.0);
        model.set_control(0, 0.49);
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 0.0);
        assert_eq!(model.valve_opening(1), 1.0);
    }

    #[test]
    fn continuous_valve_changes_loaded_pressure_and_reaches_exact_endpoints() {
        let source = controlled_source("A", ValveControlKind::Continuous, 1.0);
        let plan = compile_project_wind(&[source], &[pipe(1, "A")]).unwrap();
        let node = plan.pipes[&(1, 1)].node;
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        let mut demand = [0.0; MAX_PROJECT_WIND_NODES];
        demand[node] = 0.008;
        model.set_demands(&demand);
        model.set_control(0, 0.5);
        render(&mut model, 48000);
        assert_eq!(model.valve_opening(0), 0.5);
        assert!((model.pressure_ratio(node) - 1.0 / 1.16).abs() < 1e-5);
        model.set_control(0, -3.0);
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 0.0);
        assert_eq!(model.link_flow_kg_s(0), 0.0);
        model.set_control(0, 3.0);
        render(&mut model, 1);
        assert!(model.valve_opening(0) > 0.0 && model.valve_opening(0) < 1.0);
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 1.0);
        assert!(!model.set_control(0, f32::NAN));
        assert!(!model.set_control(99, 0.0));
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 1.0);
    }

    #[test]
    fn linked_project_stops_use_or_and_duplicate_calls_are_idempotent() {
        let mut source = controlled_source("A", ValveControlKind::Switch, 0.0);
        source.stop_controls.insert(7, vec![102, 101, 101]);
        let plan = compile_project_wind(&[source], &[]).unwrap();
        assert_eq!(plan.controls[0].project_stop_ids, vec![101, 102]);
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        assert!(!model.set_stop_drawn(999, true));
        assert!(model.set_stop_drawn(101, true));
        assert!(model.set_stop_drawn(101, true));
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 1.0);
        model.set_stop_drawn(102, true);
        model.set_stop_drawn(101, false);
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 1.0);
        model.set_stop_drawn(102, false);
        render(&mut model, 960);
        assert_eq!(model.valve_opening(0), 0.0);
    }

    #[test]
    fn replacing_runtime_starts_at_source_defaults_and_disabled_wind_keeps_controls_current() {
        let source = controlled_source("A", ValveControlKind::Switch, 1.0);
        let plan = compile_project_wind(&[source], &[]).unwrap();
        let mut old = ProjectWindRuntime::new(&plan, 48000).unwrap();
        old.set_enabled(false);
        old.set_control(0, 0.0);
        render(&mut old, 960);
        assert_eq!(old.valve_opening(0), 0.0);
        assert_eq!(old.link_flow_kg_s(0), 0.0);
        let replacement = ProjectWindRuntime::new(&plan, 48000).unwrap();
        assert_eq!(replacement.valve_opening(0), 1.0);
        assert_eq!(replacement.pressure_ratio(1), 1.0);
        let ordinary = compile_project_wind(&[], &[]).unwrap();
        let mut ordinary = ProjectWindRuntime::new(&ordinary, 48000).unwrap();
        assert!(ordinary.plan.controls.is_empty());
        assert!(ordinary.plan.links.is_empty());
        assert!(!ordinary.set_control(0, 1.0));
        assert!(!ordinary.set_stop_drawn(101, true));
    }

    #[test]
    fn unknown_automation_and_missing_controller_metadata_cannot_be_silently_opened() {
        let mut source = controlled_source("A", ValveControlKind::Switch, 1.0);
        source.windwerk.verbindingen[0].valve_control = None;
        assert!(compile_project_wind(&[source], &[])
            .unwrap_err()
            .contains("klepbesturings-ID"));
        let mut source = controlled_source("A", ValveControlKind::Switch, 1.0);
        source.windwerk.controls[0]
            .issues
            .push("Onbekende bronautomatisering".into());
        assert!(compile_project_wind(&[source], &[])
            .unwrap_err()
            .contains("Onbekende bronautomatisering"));
        let mut source = controlled_source("A", ValveControlKind::Continuous, 1.0);
        source.windwerk.verbindingen[0].valve_control = Some(ValveControlRef::Unsupported {
            type_code: 9,
            reason: "Onbekend".into(),
        });
        assert!(compile_project_wind(&[source], &[])
            .unwrap_err()
            .contains("niet ondersteunde"));
    }

    fn add_control(
        source: &mut ProjectWindSource,
        id: u32,
        kind: ValveControlKind,
        value: f32,
        automatic: bool,
    ) {
        source.windwerk.controls.push(ValveControlDef {
            id,
            kind,
            name: format!("Control {id}"),
            default_value: value,
            automatic,
            source_stop_ids: Vec::new(),
            issues: Vec::new(),
            linkages: Vec::new(),
        });
    }
    fn cc_link(
        source: u32,
        dest: u32,
        model: vpo_sampler::windwerk::WindContinuousLinkModel,
    ) -> vpo_sampler::windwerk::WindContinuousLinkDef {
        vpo_sampler::windwerk::WindContinuousLinkDef {
            source,
            dest,
            name: format!("{source}->{dest}"),
            condition: None,
            invert: false,
            increment: 0.0,
            coefficient: 1.0,
            index: 1.0,
            model,
        }
    }
    fn timed_source(model: vpo_sampler::windwerk::WindContinuousLinkModel) -> ProjectWindSource {
        let mut source = controlled_source("A", ValveControlKind::Continuous, 0.0);
        source.windwerk.controls[0].automatic = true;
        add_control(&mut source, 8, ValveControlKind::Continuous, 0.0, false);
        source
            .windwerk
            .automation
            .cc_links
            .push(cc_link(8, 7, model));
        source
    }
    #[test]
    fn willis_response_has_explicit_seconds_and_reaches_stage_endpoints() {
        use vpo_sampler::windwerk::{WindContinuousLinkModel, WindStageSwitchDef};
        let mut source = timed_source(WindContinuousLinkModel::Willis {
            milliseconds_per_step: 100.0,
        });
        add_control(&mut source, 9, ValveControlKind::Switch, 0.0, true);
        source.windwerk.automation.stages.push(WindStageSwitchDef {
            control: 7,
            switch_id: 9,
            threshold: 1.0,
            engage_increasing: true,
            engage_decreasing: false,
            disengage_increasing: false,
            disengage_decreasing: false,
        });
        source.windwerk.automation.stages.push(WindStageSwitchDef {
            control: 7,
            switch_id: 9,
            threshold: 0.0,
            engage_increasing: false,
            engage_decreasing: false,
            disengage_increasing: false,
            disengage_decreasing: true,
        });
        let plan = compile_project_wind(&[source], &[]).unwrap();
        assert!(plan.controls[2].project_stop_ids.is_empty());
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        assert!(!model.set_control(0, 1.0)); // Derived outputs cannot be written manually.
        model.set_control(1, 1.0);
        render(&mut model, 4800);
        assert!((model.control_value(0) - 0.632).abs() < 0.04);
        render(&mut model, 48000);
        assert_eq!(model.control_value(0), 1.0);
        assert_eq!(model.control_value(2), 1.0);
        model.set_control(1, 0.0);
        render(&mut model, 48000);
        assert_eq!(model.control_value(0), 0.0);
        assert_eq!(model.control_value(2), 0.0);
        assert_eq!(model.control_tick_count(), 2100);
        assert!((model.model_time_seconds() - 2.1).abs() < 1e-12);
    }
    #[test]
    fn inertia_is_dynamic_bounded_and_stable_for_under_critical_and_overdamped_cases() {
        use vpo_sampler::windwerk::WindContinuousLinkModel;
        for (acceleration, damping) in [
            (1000.0, 1.0),
            (100.0, 20.0),
            (100.0, 50.0),
            (f32::MAX, f32::MAX),
        ] {
            let plan = compile_project_wind(
                &[timed_source(WindContinuousLinkModel::Inertia {
                    acceleration,
                    damping,
                })],
                &[],
            )
            .unwrap();
            let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
            model.set_control(1, 1.0);
            render(&mut model, 48);
            assert!(model.control_value(0) < 1.0);
            for _ in 0..2000 {
                render(&mut model, 48);
                assert!((0.0..=1.0).contains(&model.control_value(0)));
            }
            assert!(model.control_value(0).is_finite());
            if acceleration != f32::MAX {
                assert!(model.control_value(0) > 0.98);
            }
        }
    }
    #[test]
    fn source_transfer_order_and_complementary_condition_selection_are_explicit() {
        use vpo_sampler::windwerk::{WindContinuousLinkModel, WindControlConditionDef};
        let mut source = timed_source(WindContinuousLinkModel::Direct);
        let link = &mut source.windwerk.automation.cc_links[0];
        link.invert = true;
        link.increment = -27.0;
        link.coefficient = 2.54;
        let plan = compile_project_wind(&[source.clone()], &[]).unwrap();
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        model.set_control(1, 0.5);
        render(&mut model, 960);
        assert!((model.control_value(0) - ((127.0 - 63.5 - 27.0) * 2.54 / 127.0)).abs() < 1e-6);
        add_control(&mut source, 10, ValveControlKind::Switch, 0.0, false);
        source.windwerk.automation.cc_links[0].condition = Some(WindControlConditionDef {
            switch_id: 10,
            engaged: true,
            reevaluate: true,
        });
        let mut other = cc_link(8, 7, WindContinuousLinkModel::Direct);
        other.condition = Some(WindControlConditionDef {
            switch_id: 10,
            engaged: false,
            reevaluate: true,
        });
        source.windwerk.automation.cc_links.push(other);
        let plan = compile_project_wind(&[source.clone()], &[]).unwrap();
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        model.set_control(1, 0.5);
        render(&mut model, 960);
        assert_eq!(model.control_value(0), 0.5);
        model.set_control(2, 1.0);
        render(&mut model, 960);
        assert!((model.control_value(0) - 0.73).abs() < 0.01);
        source.windwerk.automation.cc_links[1].condition = None;
        assert!(compile_project_wind(&[source], &[])
            .unwrap_err()
            .contains("Conflicterende"));
    }
    #[test]
    fn algebraic_cc_cycles_are_rejected_but_stateful_relay_feedback_is_bounded() {
        use vpo_sampler::windwerk::{WindContinuousLinkModel, WindSwitchAction, WindSwitchLinkDef};
        let mut cyclic = timed_source(WindContinuousLinkModel::Direct);
        cyclic
            .windwerk
            .automation
            .cc_links
            .push(cc_link(7, 8, WindContinuousLinkModel::Direct));
        assert!(compile_project_wind(&[cyclic], &[])
            .unwrap_err()
            .contains("cyclische"));
        let mut source = source("A");
        add_control(&mut source, 1, ValveControlKind::Switch, 1.0, true);
        add_control(&mut source, 2, ValveControlKind::Switch, 0.0, true);
        for (from, dest) in [(1, 2), (2, 1)] {
            source
                .windwerk
                .automation
                .switch_links
                .push(WindSwitchLinkDef {
                    source: from,
                    dest,
                    condition: None,
                    source_engaged: true,
                    engage_action: WindSwitchAction::Engage,
                    disengage_action: WindSwitchAction::Disengage,
                });
        }
        let plan = compile_project_wind(&[source], &[]).unwrap();
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        for _ in 0..100 {
            render(&mut model, 48);
            assert!([0.0, 1.0].contains(&model.control_value(0)));
            assert!([0.0, 1.0].contains(&model.control_value(1)));
        }
    }

    #[test]
    fn conditional_cc_without_reevaluation_waits_for_a_new_source_event() {
        use vpo_sampler::windwerk::{WindContinuousLinkModel, WindControlConditionDef};
        let mut source = timed_source(WindContinuousLinkModel::Willis {
            milliseconds_per_step: 50.0,
        });
        add_control(&mut source, 10, ValveControlKind::Switch, 0.0, false);
        source.windwerk.automation.cc_links[0].condition = Some(WindControlConditionDef {
            switch_id: 10,
            engaged: true,
            reevaluate: false,
        });
        let plan = compile_project_wind(&[source], &[]).unwrap();
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        model.set_control(1, 0.5);
        render(&mut model, 4800); // Change while disabled.
        model.set_control(2, 1.0);
        render(&mut model, 9600);
        assert_eq!(
            model.control_value(0),
            0.0,
            "reevaluate=N transferred an old input on activation"
        );
        model.set_control(1, 0.8);
        render(&mut model, 4800);
        let progressing = model.control_value(0);
        assert!(progressing > 0.5 && progressing < 0.8);
        render(&mut model, 48000);
        assert_eq!(model.control_value(0), 0.8);
        model.set_control(2, 0.0);
        render(&mut model, 960);
        model.set_control(1, 0.2);
        render(&mut model, 960);
        model.set_control(2, 1.0);
        render(&mut model, 4800);
        assert_eq!(
            model.control_value(0),
            0.8,
            "reactivation ignored reevaluate=N latch"
        );
    }

    fn feedback_source(id: &str) -> ProjectWindSource {
        use vpo_sampler::windwerk::{
            BalgDef, WindContinuousLinkModel, WindControlOutputDef, WindControlOutputKind,
        };
        let mut source = controlled_source(id, ValveControlKind::Continuous, 1.0);
        source.windwerk.controls[0].automatic = true;
        add_control(&mut source, 8, ValveControlKind::Continuous, 0.5, true);
        let chest = &mut source.windwerk.laden[1];
        chest.druk_inch = Some(3.0);
        chest.balg = Some(BalgDef {
            breedte_m: 0.25,
            lengte_m: 0.4,
            slag_m: 0.05,
            massa_kg: 0.2,
            demping: 2.0,
            ..Default::default()
        });
        source
            .windwerk
            .automation
            .outputs
            .push(WindControlOutputDef {
                control_id: 8,
                kind: WindControlOutputKind::BellowsExtension { compartment_id: 2 },
            });
        let mut link = cc_link(8, 7, WindContinuousLinkModel::Direct);
        link.invert = true;
        link.increment = -40.0;
        link.coefficient = 3.0;
        source.windwerk.automation.cc_links.push(link);
        source
    }
    #[test]
    fn physical_bellows_feedback_moves_under_load_regulates_and_recovers_independently() {
        let plan = compile_project_wind(
            &[feedback_source("A"), feedback_source("B")],
            &[pipe(1, "A"), pipe(2, "B")],
        )
        .unwrap();
        let a = plan.pipes[&(1, 1)].node;
        let b = plan.pipes[&(2, 1)].node;
        assert!(plan.nodes[a].mechanical.is_some());
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        render(&mut model, 240000);
        let initial = model.bellows_extension(a);
        let other = model.bellows_extension(b);
        let mut demand = [0.0; MAX_PROJECT_WIND_NODES];
        demand[a] = 0.008;
        model.set_demands(&demand);
        render(&mut model, 240000);
        let loaded = model.bellows_extension(a);
        assert!(
            loaded < initial - 0.02,
            "bellows failed to lower: {initial}->{loaded}"
        );
        assert!((model.pressure_ratio(a) - 1.0).abs() < 0.08);
        assert!((model.bellows_extension(b) - other).abs() < 0.002);
        assert!(model.link_flow_kg_s(0) > 0.005);
        model.set_demands(&[0.0; MAX_PROJECT_WIND_NODES]);
        render(&mut model, 240000);
        assert!(model.bellows_extension(a) > loaded + 0.02);
        assert!(
            (0.0..=1.0).contains(&model.bellows_extension(a))
                && model.bellows_velocity(a).is_finite()
        );
        let replacement = ProjectWindRuntime::new(&plan, 48000).unwrap();
        assert_eq!(replacement.control_tick_count(), 0);
        assert_eq!(replacement.bellows_extension(a), 0.5);
    }

    /// Opt in to the installed, readable Nitra definition. This exercises only
    /// its imported wind/control closure, not full-organ or tremulant support.
    /// No samples are played and no source/settings files are written.
    #[test]
    #[ignore = "requires JM_NITRA_DEFINITION pointing to the installed readable Nitra ODF"]
    fn installed_nitra_automatic_wind_closure_runs_ten_seconds() {
        let definition = std::path::PathBuf::from(
            std::env::var_os("JM_NITRA_DEFINITION")
                .expect("set JM_NITRA_DEFINITION to the installed readable Nitra definition"),
        );
        assert!(
            definition.is_file(),
            "JM_NITRA_DEFINITION must be an existing file"
        );
        let (_organ_result, report) = vpo_sampler::load_hauptwerk_met_verslag(&definition, None);
        // The report remains useful when separate organ metadata is outside
        // current full-organ limits. Do not infer full-organ compatibility.
        let windwerk = report
            .windwerk
            .expect("native Nitra import must expose its wind model in the report");
        assert!(
            windwerk.automation.issues.is_empty(),
            "native automation issues: {:?}",
            windwerk.automation.issues
        );
        let source = ProjectWindSource {
            source_id: "native-nitra".into(),
            windwerk,
            stop_controls: HashMap::new(),
        };
        let plan = compile_project_wind(&[source], &[])
            .expect("compile the complete native wind/control closure");
        assert_eq!(
            plan.controls.len(),
            18,
            "the installed Nitra closure has eleven continuous controls and seven switches"
        );
        let slot = |kind, raw_id| {
            plan.controls
                .iter()
                .position(|c| c.kind == kind && c.source_control_id == raw_id)
                .unwrap_or_else(|| panic!("missing native Nitra controller {kind:?} {raw_id}"))
        };
        let master = slot(ValveControlKind::Continuous, 9001);
        let willis_100 = slot(ValveControlKind::Continuous, 9002);
        let willis_50 = slot(ValveControlKind::Continuous, 9003);
        let stage_switch = slot(ValveControlKind::Switch, 109);
        let extension_slots: Vec<_> = plan
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.mechanical.is_some())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            extension_slots.len(),
            2,
            "both native bellows must have explicit mechanical feedback"
        );
        let mut model = ProjectWindRuntime::new(&plan, 48000).unwrap();
        let initial_master = model.control_value(master);
        let initial_positions: Vec<_> = extension_slots
            .iter()
            .map(|i| model.bellows_position(*i))
            .collect();
        let mut moved = false;
        for _ in 0..10_000 {
            render(&mut model, 48); // One physical/control millisecond.
            for (i, node) in plan.nodes.iter().enumerate() {
                assert!(model.pressure[i].is_finite() && model.pressure[i] >= 0.0);
                assert!(model.pressure_ratio(i).is_finite());
                if let Some(bellows) = &node.mechanical {
                    let position = f64::from(model.bellows_position(i));
                    assert!(position >= 0.0 && position <= bellows.maximum_extension_m + 1e-6);
                    assert!((0.0..=1.0).contains(&model.bellows_extension(i)));
                    assert!(model.bellows_velocity(i).is_finite());
                }
            }
            for i in 0..plan.controls.len() {
                assert!((0.0..=1.0).contains(&model.control_value(i)));
            }
            for i in 0..plan.links.len() {
                assert!(model.link_flow_kg_s(i).is_finite());
                assert!((0.0..=1.0).contains(&model.valve_opening(i)));
            }
            moved |= extension_slots
                .iter()
                .zip(&initial_positions)
                .any(|(i, initial)| (model.bellows_position(*i) - initial).abs() > 1e-6);
        }
        assert_eq!(model.control_tick_count(), 10_000);
        assert!((model.model_time_seconds() - 10.0).abs() < 1e-12);
        assert!(
            model.control_value(willis_100) > 0.99 && model.control_value(willis_50) > 0.99,
            "the native 100/50 ms Willis startup branches must reach their upper stage"
        );
        assert_eq!(
            model.control_value(stage_switch),
            1.0,
            "native upper stage must engage switch 109"
        );
        assert!(
            model.control_value(master) > initial_master + 0.5,
            "the native stage/relay chain must start the master supply"
        );
        assert!(
            moved,
            "native bellows outputs must follow physical board travel"
        );
        eprintln!("native Nitra wind-only probe: {} nodes, {} controls, {} links, {:.3}s, master={:.4}, Willis100={:.4}, Willis50={:.4}, switch109={}, extensions={:?}",
            plan.nodes.len(), plan.controls.len(), plan.links.len(), model.model_time_seconds(), model.control_value(master),
            model.control_value(willis_100), model.control_value(willis_50), model.control_value(stage_switch),
            extension_slots.iter().map(|i| model.bellows_extension(*i)).collect::<Vec<_>>());
    }
}
