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

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

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
    #[serde(default)]
    pub moving_inertia_mass_kg: Option<f32>,
    #[serde(default)]
    pub shape_code: Option<u32>,
    #[serde(default)]
    pub movement_direction_code: Option<u32>,
    #[serde(default)]
    pub gravity_center_distance_m: Option<f32>,
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

/// Source-local Hauptwerk controller identity. Switch and continuous IDs have
/// separate namespaces, even when their numeric values happen to be equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValveControlKind {
    Switch,
    Continuous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValveControlRef {
    Switch { control_id: u32, open_if_engaged: bool },
    Continuous { control_id: u32 },
    Unsupported { type_code: u32, reason: String },
}

/// Exact source automation fields are retained for diagnosis. They are not
/// silently replaced by an always-open valve when their semantics are unknown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValveControlLinkDef {
    pub object_type: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValveControlDef {
    pub id: u32,
    pub kind: ValveControlKind,
    pub name: String,
    /// Explicit source initial value normalized from Hauptwerk's 0..127 range.
    /// If no valid initial value exists, issues explains why loading is refused.
    pub default_value: f32,
    /// Driven by source automation/physical feedback; not a manual target.
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub source_stop_ids: Vec<u32>,
    #[serde(default)]
    pub linkages: Vec<ValveControlLinkDef>,
    #[serde(default)]
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindControlConditionDef {
    pub switch_id: u32,
    pub engaged: bool,
    pub reevaluate: bool,
}

/// These are source parameters interpreted by the documented JM control
/// model; no assertion of identical Hauptwerk timing or operation ordering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WindContinuousLinkModel {
    Direct,
    Willis { milliseconds_per_step: f32 },
    Inertia { acceleration: f32, damping: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindContinuousLinkDef {
    pub source: u32,
    pub dest: u32,
    pub name: String,
    pub condition: Option<WindControlConditionDef>,
    pub invert: bool,
    /// Increment and power apply to the source's raw 0..127 value.
    pub increment: f32,
    pub coefficient: f32,
    pub index: f32,
    pub model: WindContinuousLinkModel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindSwitchAction {
    Engage,
    Disengage,
    /// Source code 7 uses JM's explicit latch/no-release-action interpretation.
    NoAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindSwitchLinkDef {
    pub source: u32,
    pub dest: u32,
    pub condition: Option<WindControlConditionDef>,
    pub source_engaged: bool,
    pub engage_action: WindSwitchAction,
    pub disengage_action: WindSwitchAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindStageSwitchDef {
    pub control: u32,
    pub switch_id: u32,
    /// Normalized source threshold, 0..1.
    pub threshold: f32,
    pub engage_increasing: bool,
    pub engage_decreasing: bool,
    pub disengage_increasing: bool,
    pub disengage_decreasing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WindControlOutputKind {
    BellowsExtension { compartment_id: u32 },
    Pressure { compartment_id: u32 },
    Flow { link_index: usize, max_flow_kg_s: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindControlOutputDef {
    pub control_id: u32,
    pub kind: WindControlOutputKind,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindAutomationDef {
    #[serde(default)]
    pub cc_links: Vec<WindContinuousLinkDef>,
    #[serde(default)]
    pub switch_links: Vec<WindSwitchLinkDef>,
    #[serde(default)]
    pub stages: Vec<WindStageSwitchDef>,
    #[serde(default)]
    pub outputs: Vec<WindControlOutputDef>,
    #[serde(default)]
    pub issues: Vec<String>,
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
    #[serde(default)]
    pub valve_control_type_code: Option<u32>,
    #[serde(default)]
    pub valve_control: Option<ValveControlRef>,
}

/// Het windwerk van een set. Explicit links without compartments remain present
/// so project preflight can reject their missing endpoints instead of losing valves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WindwerkDef {
    pub laden: Vec<WindLadeDef>,
    pub verbindingen: Vec<WindVerbindingDef>,
    #[serde(default)]
    pub controls: Vec<ValveControlDef>,
    #[serde(default)]
    pub automation: WindAutomationDef,
}

fn getal(o: &FieldMap, veld: &str) -> Option<f32> {
    o.get(veld).and_then(|v| v.trim().replace(',', ".").parse::<f32>().ok())
}

fn ja(o: &FieldMap, veld: &str) -> bool {
    o.get(veld).is_some_and(|v| v.trim().eq_ignore_ascii_case("Y"))
}

fn control_id(row: &FieldMap, field: &str) -> Result<u32, String> {
    row.get_u32(field).filter(|id| *id > 0).ok_or_else(|| format!("{field} mist een geldig positief ID"))
}

fn control_bool(row: &FieldMap, field: &str, default: bool) -> Result<bool, String> {
    match row.get(field).map(|value| value.trim()).filter(|value| !value.is_empty()) {
        None => Ok(default), Some("Y") => Ok(true), Some("N") => Ok(false),
        _ => Err(format!("{field} moet Y of N zijn")),
    }
}

fn control_number(row: &FieldMap, field: &str, default: f32) -> Result<f32, String> {
    if row.get(field).is_none_or(|value| value.trim().is_empty()) { return Ok(default); }
    getal(row, field).filter(|value| value.is_finite())
        .ok_or_else(|| format!("{field} moet een eindig getal zijn"))
}

fn control_condition(row: &FieldMap) -> Result<Option<WindControlConditionDef>, String> {
    if row.get("ConditionSwitchID").is_none_or(|value| value.trim().is_empty() || value.trim() == "0") {
        return Ok(None);
    }
    Ok(Some(WindControlConditionDef {
        switch_id: control_id(row, "ConditionSwitchID")?,
        engaged: control_bool(row, "ConditionSwitchLinkIfEngaged", true)?,
        reevaluate: control_bool(row, "ReevaluateIfCondSwitchChangesState", true)?,
    }))
}

fn control_action(row: &FieldMap, field: &str) -> Result<WindSwitchAction, String> {
    match row.get_u32(field) {
        Some(1) => Ok(WindSwitchAction::Engage),
        Some(2) => Ok(WindSwitchAction::Disengage),
        Some(7) => Ok(WindSwitchAction::NoAction),
        _ => Err(format!("{field} gebruikt een niet ondersteunde actiecode")),
    }
}

struct ControlProducer<'a> {
    kind: ValveControlKind,
    id: u32,
    object_type: &'a str,
    output_field: &'a str,
    row: &'a FieldMap,
    row_index: usize,
}

impl ControlProducer<'_> {
    fn dependencies(&self) -> Vec<(ValveControlKind, u32)> {
        use ValveControlKind::{Continuous, Switch};
        if self.object_type == "WindCompartmentLinkage" {
            let field = match self.row.get_u32("ValveControlTypeCode") {
                Some(2) => Some((Switch, "ValveControllingSwitchID")),
                Some(3) => Some((Continuous, "ValveControllingContinuousControlID")),
                _ => None,
            };
            return field.and_then(|(kind, field)| self.row.get_u32(field).filter(|id| *id > 0).map(|id| (kind, id)))
                .into_iter().collect();
        }
        let fields: &[(ValveControlKind, &str)] = match self.object_type {
            "ContinuousControlLinkage" => &[(Continuous, "SourceControlID"), (Switch, "ConditionSwitchID")],
            "ContinuousControlDoubleLinkage" => &[(Continuous, "FirstSourceControl_ID"), (Continuous, "SecondSourceControl_ID")],
            "SwitchLinkage" => &[(Switch, "SourceSwitchID"), (Switch, "ConditionSwitchID")],
            "ContinuousControlStageSwitch" => &[(Continuous, "ContinuousControlID")],
            "WindCompartment" => &[(Continuous, "Bellows_ExtensionSettingContinuousControlID")],
            _ => &[],
        };
        fields.iter().filter_map(|(kind, field)| self.row.get_u32(field).filter(|id| *id > 0).map(|id| (*kind, id))).collect()
    }
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
                moving_inertia_mass_kg: getal(o, "Bellows_MassOfMovingBoardGivingRiseToInertiaKg"),
                shape_code: o.get_u32("Bellows_ShapeCode"),
                movement_direction_code: o.get_u32("Bellows_MovementDirectionCode"),
                gravity_center_distance_m: getal(o, "Bellows_DistOfCenOfMassOfMvgBrdFromAxisIfDiagTypMetres"),
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
            let type_code = o.get_u32("ValveControlTypeCode");
            let unsupported = |reason: &str| Some(ValveControlRef::Unsupported {
                type_code: type_code.unwrap_or(0), reason: reason.into(),
            });
            let valve_control = match type_code {
                None if o.get("ValveControlTypeCode").is_some_and(|v| !v.trim().is_empty()) =>
                    unsupported("ValveControlTypeCode is ongeldig"),
                None | Some(1) => None,
                Some(2) => match (o.get_u32("ValveControllingSwitchID").filter(|id| *id > 0),
                    o.get("ValveOpenIfControllingSwitchEngaged").map(|v| v.trim())) {
                    (Some(control_id), Some(polarity)) if matches!(polarity, "Y" | "N") =>
                        Some(ValveControlRef::Switch { control_id, open_if_engaged: polarity == "Y" }),
                    _ => unsupported("Schakelklep mist een geldige ValveControllingSwitchID of ValveOpenIfControllingSwitchEngaged"),
                },
                Some(3) => match o.get_u32("ValveControllingContinuousControlID").filter(|id| *id > 0) {
                    Some(control_id) => Some(ValveControlRef::Continuous { control_id }),
                    None => unsupported("Regelklep mist een geldige ValveControllingContinuousControlID"),
                },
                Some(_) => unsupported("Dit ValveControlTypeCode wordt niet ondersteund"),
            };
            Some(WindVerbindingDef {
                // Retain malformed links for preflight rejection instead of
                // silently dropping their valves or shifting flow-output IDs.
                van: o.get_u32("FirstWindCompartmentID").unwrap_or(0),
                naar: o.get_u32("SecondWindCompartmentID").unwrap_or(0),
                naam: o.get("Name").cloned().unwrap_or_default(),
                stroom_kg_s: getal(o, "MassFlowRateKilogramsPerSecAtReferencePressureDiff").unwrap_or(0.0),
                ref_inch: getal(o, "ReferencePressureDifferenceInches").unwrap_or(1.0),
                // Code 1 = altijd open; 2 = schakelaar; 3 = regelaar.
                klep: valve_control.is_some(),
                valve_control_type_code: type_code,
                valve_control,
            })
        }).collect();
        WindwerkDef { laden, verbindingen, controls: Vec::new(), automation: WindAutomationDef::default() }
    }

    /// Keep the complete backwards dependency closure of every wind valve.
    /// Unrelated organ relay/audio logic is deliberately outside this graph.
    pub(crate) fn voeg_besturing_toe(
        &mut self,
        switches: &[FieldMap],
        continuous: &[FieldMap],
        stops: &[FieldMap],
        automation: &[(&str, &[FieldMap])],
    ) {
        self.automation = WindAutomationDef::default();
        let mut identities: std::collections::BTreeSet<_> = self.verbindingen.iter()
            .filter_map(|link| match &link.valve_control {
                Some(ValveControlRef::Switch { control_id, .. }) => Some((ValveControlKind::Switch, *control_id)),
                Some(ValveControlRef::Continuous { control_id }) => Some((ValveControlKind::Continuous, *control_id)),
                _ => None,
            }).collect();
        let mut producers = Vec::new();
        for (object_type, rows) in automation {
            for (row_index, row) in rows.iter().enumerate() {
                let outputs: &[(ValveControlKind, &str)] = match *object_type {
                    "SwitchLinkage" => &[(ValveControlKind::Switch, "DestSwitchID")],
                    "ContinuousControlStageSwitch" => &[(ValveControlKind::Switch, "ControlledSwitchID")],
                    "ContinuousControlLinkage" => &[(ValveControlKind::Continuous, "DestControlID")],
                    "ContinuousControlDoubleLinkage" => &[(ValveControlKind::Continuous, "DestControl_ID")],
                    "WindCompartment" => &[(ValveControlKind::Continuous, "Bellows_ExtensionOutputContinuousControlID"),
                        (ValveControlKind::Continuous, "PressureOutputContinuousControlID")],
                    "WindCompartmentLinkage" => &[(ValveControlKind::Continuous, "FlowRateOutputCtsCtrl_ContinuousControlID")],
                    _ => &[],
                };
                for (kind, output_field) in outputs {
                    if let Some(id) = row.get_u32(output_field).filter(|id| *id > 0) {
                        producers.push(ControlProducer { kind: *kind, id, object_type, output_field, row, row_index });
                    }
                }
            }
        }
        loop {
            let old_len = identities.len();
            for producer in &producers {
                if identities.contains(&(producer.kind, producer.id)) {
                    identities.extend(producer.dependencies());
                }
            }
            if identities.len() > 4096 {
                self.automation.issues.push("Het bronwindnetwerk gebruikt meer dan 4.096 controllers".into());
                self.controls.clear();
                return;
            }
            if identities.len() == old_len { break; }
        }
        let relevant: Vec<_> = producers.iter().filter(|producer| identities.contains(&(producer.kind, producer.id))).collect();
        if relevant.len() > 4096 {
            self.automation.issues.push("Het bronwindnetwerk gebruikt meer dan 4.096 besturingsverbindingen".into());
            self.controls.clear();
            return;
        }
        self.controls = identities.into_iter().map(|(kind, id)| {
            let (rows, id_field) = match kind {
                ValveControlKind::Switch => (switches, "SwitchID"),
                ValveControlKind::Continuous => (continuous, "ControlID"),
            };
            let matches: Vec<_> = rows.iter().filter(|row| row.get_u32(id_field) == Some(id)).collect();
            let row = matches.first().copied();
            let mut issues = Vec::new();
            if matches.len() != 1 {
                issues.push(format!("{kind:?} {id}: verwacht één broncontroller, gevonden {}", matches.len()));
            }
            let initial = row.and_then(|row| match kind {
                ValveControlKind::Switch => row.get("DefaultToEngaged").and_then(|v| match v.trim() {
                    "Y" => Some(1.0), "N" => Some(0.0), _ => None,
                }),
                ValveControlKind::Continuous => getal(row, "DefaultValue")
                    .filter(|value| value.is_finite() && (0.0..=127.0).contains(value))
                    .map(|value| value / 127.0),
            });
            let driven = relevant.iter().any(|producer| (producer.kind, producer.id) == (kind, id));
            let mut source_stop_ids: Vec<_> = if kind == ValveControlKind::Switch {
                stops.iter().filter(|stop| stop.get_u32("ControllingSwitchID") == Some(id))
                    .filter_map(|stop| stop.get_u32("StopID")).collect()
            } else { Vec::new() };
            source_stop_ids.sort_unstable();
            source_stop_ids.dedup();
            let hidden_constant = source_stop_ids.is_empty() && row.is_some_and(|row|
                row.get("AccessibleForInput").is_some_and(|value| value.trim() == "N")
                    && row.get("Clickable").is_some_and(|value| value.trim() == "N"));
            let automatic = driven || hidden_constant;
            let default_field = if kind == ValveControlKind::Switch { "DefaultToEngaged" } else { "DefaultValue" };
            let has_explicit_default = row.and_then(|row| row.get(default_field)).is_some_and(|value| !value.trim().is_empty());
            if initial.is_none() && (has_explicit_default || !driven) {
                issues.push(format!("{kind:?} {id}: expliciete geldige beginstand ontbreekt (DefaultToEngaged/DefaultValue)"));
            }
            let linkages = relevant.iter().filter(|producer| (producer.kind, producer.id) == (kind, id))
                .map(|producer| ValveControlLinkDef { object_type: producer.object_type.into(),
                    fields: producer.row.iter().map(|(key, value)| (key.clone(), value.clone())).collect() }).collect();
            ValveControlDef {
                id, kind,
                name: row.and_then(|row| row.get("Name")).cloned().unwrap_or_else(|| format!("{kind:?} {id}")),
                default_value: initial.unwrap_or(0.0),
                automatic,
                source_stop_ids,
                linkages, issues,
            }
        }).collect();
        for producer in relevant {
            if let Err(error) = self.parse_control_producer(producer) {
                self.automation.issues.push(format!("{} → {:?} {}: {error}", producer.object_type, producer.kind, producer.id));
            }
        }
    }

    fn parse_control_producer(&mut self, producer: &ControlProducer<'_>) -> Result<(), String> {
        let row = producer.row;
        let known_fields: &[&str] = match producer.object_type {
            "ContinuousControlLinkage" => &["Name", "SourceControlID", "DestControlID", "LinkTypeCode",
                "WillisTypeIncSpeedInMillisecondsPerStepWithOneStepDiff", "InertiaModelTypePositiveAcceleratingCoeff",
                "InertiaModelTypePositiveDampingCoeff", "InertialModelTypePositiveAcceleratingCoeff",
                "InertialModelTypePositiveDampingCoeff", "ConditionSwitchID", "ConditionSwitchLinkIfEngaged",
                "ReevaluateIfCondSwitchChangesState", "InvertSourceControlValue", "SourceControlValueIncrement",
                "SourceControlValueCoefficient", "SourceControlValueIndex"],
            "SwitchLinkage" => &["Name", "SourceSwitchID", "DestSwitchID", "ConditionSwitchID",
                "ConditionSwitchLinkIfEngaged", "SourceSwitchLinkIfEngaged", "ReevaluateIfCondSwitchChangesState",
                "EngageLinkActionCode", "DisengageLinkActionCode"],
            "ContinuousControlStageSwitch" => &["Name", "ContinuousControlID", "ContinuousControlValue", "ControlledSwitchID",
                "EngageWhenValueIncreasing", "EngageWhenValueDecreasing", "DisengageWhenValueIncreasing", "DisengageWhenValueDecreasing"],
            _ => &[],
        };
        if !known_fields.is_empty() {
            let unknown: Vec<_> = row.iter().filter(|(key, value)| !value.trim().is_empty() && !known_fields.contains(&key.as_str()))
                .map(|(key, _)| key.as_str()).collect();
            if !unknown.is_empty() { return Err(format!("Onbekende besturingsvelden: {}", unknown.join(", "))); }
        }
        match producer.object_type {
            "ContinuousControlLinkage" => {
                let model = match row.get_u32("LinkTypeCode") {
                    Some(1) => WindContinuousLinkModel::Direct,
                    Some(2) => {
                        let ms = control_number(row, "WillisTypeIncSpeedInMillisecondsPerStepWithOneStepDiff", f32::NAN)?;
                        if !(0.0..=3_600_000.0).contains(&ms) || ms == 0.0 { return Err("Ongeldige Willis-tijdconstante".into()); }
                        WindContinuousLinkModel::Willis { milliseconds_per_step: ms }
                    }
                    Some(3) => {
                        let alias = |field: &str, alternate: &str| -> Result<f32, String> {
                            let value = control_number(row, field, f32::NAN)?;
                            let other = control_number(row, alternate, f32::NAN)?;
                            if value.is_finite() && other.is_finite() && value != other { return Err(format!("Tegenstrijdige {field}/{alternate}")); }
                            Ok(if value.is_finite() { value } else { other })
                        };
                        let acceleration = alias("InertiaModelTypePositiveAcceleratingCoeff", "InertialModelTypePositiveAcceleratingCoeff")?;
                        let damping = alias("InertiaModelTypePositiveDampingCoeff", "InertialModelTypePositiveDampingCoeff")?;
                        if !acceleration.is_finite() || !(0.0..=1e9).contains(&acceleration) || acceleration == 0.0
                            || !damping.is_finite() || !(0.0..=1e9).contains(&damping) { return Err("Ongeldige inertiecoëfficiënten".into()); }
                        WindContinuousLinkModel::Inertia { acceleration, damping }
                    }
                    _ => return Err("LinkTypeCode wordt niet ondersteund".into()),
                };
                let increment = control_number(row, "SourceControlValueIncrement", 0.0)?;
                let coefficient = control_number(row, "SourceControlValueCoefficient", 1.0)?;
                let index = control_number(row, "SourceControlValueIndex", 1.0)?;
                if increment.abs() > 1e6 || coefficient.abs() > 1e6 || !(0.0..=16.0).contains(&index) || index == 0.0 {
                    return Err("Onbegrensde continue overdracht".into());
                }
                self.automation.cc_links.push(WindContinuousLinkDef {
                    source: control_id(row, "SourceControlID")?, dest: producer.id,
                    name: row.get("Name").cloned().unwrap_or_default(), condition: control_condition(row)?,
                    invert: control_bool(row, "InvertSourceControlValue", false)?, increment, coefficient, index, model,
                });
            }
            "SwitchLinkage" => self.automation.switch_links.push(WindSwitchLinkDef {
                source: control_id(row, "SourceSwitchID")?, dest: producer.id,
                condition: control_condition(row)?, source_engaged: control_bool(row, "SourceSwitchLinkIfEngaged", true)?,
                engage_action: control_action(row, "EngageLinkActionCode")?,
                disengage_action: control_action(row, "DisengageLinkActionCode")?,
            }),
            "ContinuousControlStageSwitch" => {
                let threshold = control_number(row, "ContinuousControlValue", f32::NAN)?;
                if !threshold.is_finite() || !(0.0..=127.0).contains(&threshold) { return Err("Ongeldige trappenwaarde".into()); }
                self.automation.stages.push(WindStageSwitchDef {
                    control: control_id(row, "ContinuousControlID")?, switch_id: producer.id, threshold: threshold / 127.0,
                    engage_increasing: control_bool(row, "EngageWhenValueIncreasing", false)?,
                    engage_decreasing: control_bool(row, "EngageWhenValueDecreasing", false)?,
                    disengage_increasing: control_bool(row, "DisengageWhenValueIncreasing", false)?,
                    disengage_decreasing: control_bool(row, "DisengageWhenValueDecreasing", false)?,
                });
            }
            "WindCompartment" => {
                if row.get_u32("Bellows_ExtensionSettingContinuousControlID").is_some_and(|id| id > 0) {
                    return Err("Gedwongen balgstand via Bellows_ExtensionSettingContinuousControlID wordt niet ondersteund".into());
                }
                if control_bool(row, "Bellows_OpeningSprings_HasOpeningSprings", false)?
                    || control_bool(row, "Bellows_ClosingSprings_HasClosingSprings", false)? {
                    return Err("Balgveren worden niet ondersteund door dit JM-model".into());
                }
                if ja(row, "Bellows_HasBellows") {
                    for (field, allowed) in [("Bellows_ShapeCode", &[1, 2][..]), ("Bellows_MovementDirectionCode", &[1][..])] {
                        if row.get(field).is_some_and(|value| !value.trim().is_empty())
                            && !row.get_u32(field).is_some_and(|code| allowed.contains(&code)) {
                            return Err(format!("{field} wordt niet ondersteund door dit JM-model"));
                        }
                    }
                    if row.get_u32("Bellows_ShapeCode") == Some(2) {
                        let length = getal(row, "Bellows_FrameBaseLengthMetres").unwrap_or(0.0);
                        let center = getal(row, "Bellows_DistOfCenOfMassOfMvgBrdFromAxisIfDiagTypMetres").unwrap_or(0.0);
                        if !length.is_finite() || !center.is_finite() || length <= 0.0 || center <= 0.0 || center > length {
                            return Err("Diagonale balg mist een geldig zwaartepuntafstand/lengte".into());
                        }
                    }
                }
                let compartment_id = control_id(row, "WindCompartmentID")?;
                let kind = if producer.output_field == "Bellows_ExtensionOutputContinuousControlID" {
                    if !ja(row, "Bellows_HasBellows") { return Err("Balgextensie-uitgang verwijst naar een compartiment zonder balg".into()); }
                    WindControlOutputKind::BellowsExtension { compartment_id }
                } else { WindControlOutputKind::Pressure { compartment_id } };
                self.automation.outputs.push(WindControlOutputDef { control_id: producer.id, kind });
            }
            "WindCompartmentLinkage" => {
                let max_flow_kg_s = control_number(row, "FlowRateOutputCtsCtrl_MaxMassFlowRateKilogramsPerSec", f32::NAN)?;
                if !max_flow_kg_s.is_finite() || max_flow_kg_s <= 0.0 || producer.row_index >= self.verbindingen.len() {
                    return Err("Doorstroomuitgang mist een geldige referentiestroom/verbinding".into());
                }
                self.automation.outputs.push(WindControlOutputDef { control_id: producer.id,
                    kind: WindControlOutputKind::Flow { link_index: producer.row_index, max_flow_kg_s } });
            }
            _ => return Err("Deze bronautomatisering wordt nog niet ondersteund".into()),
        }
        Ok(())
    }

    pub fn is_leeg(&self) -> bool {
        self.laden.is_empty() && self.verbindingen.is_empty()
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

    fn controlled_links() -> WindwerkDef {
        WindwerkDef::uit_tabellen(&[], &[
            fm(&[("FirstWindCompartmentID", "1"), ("SecondWindCompartmentID", "2"),
                ("ValveControlTypeCode", "2"), ("ValveControllingSwitchID", "7"),
                ("ValveOpenIfControllingSwitchEngaged", "N")]),
            fm(&[("FirstWindCompartmentID", "2"), ("SecondWindCompartmentID", "3"),
                ("ValveControlTypeCode", "3"), ("ValveControllingContinuousControlID", "7")]),
        ])
    }

    #[test]
    fn reads_actual_hauptwerk_valve_ids_polarity_and_separate_control_kinds() {
        let mut wind = controlled_links();
        wind.voeg_besturing_toe(
            &[fm(&[("SwitchID", "7"), ("Name", "Windmotor"), ("DefaultToEngaged", "Y")])],
            &[fm(&[("ControlID", "7"), ("Name", "Windregelaar"), ("DefaultValue", "63.5")])],
            &[fm(&[("StopID", "42"), ("ControllingSwitchID", "7")])], &[],
        );
        assert_eq!(wind.verbindingen[0].valve_control_type_code, Some(2));
        assert_eq!(wind.verbindingen[0].valve_control,
            Some(ValveControlRef::Switch { control_id: 7, open_if_engaged: false }));
        assert_eq!(wind.verbindingen[1].valve_control,
            Some(ValveControlRef::Continuous { control_id: 7 }));
        assert_eq!(wind.controls.len(), 2);
        assert_eq!(wind.controls[0].kind, ValveControlKind::Switch);
        assert_eq!(wind.controls[0].default_value, 1.0);
        assert_eq!(wind.controls[0].source_stop_ids, vec![42]);
        assert_eq!(wind.controls[1].kind, ValveControlKind::Continuous);
        assert_eq!(wind.controls[1].default_value, 0.5);
        assert!(wind.controls.iter().all(|control| control.issues.is_empty()));
    }

    #[test]
    fn preserves_incoming_transfer_inversion_scale_and_timing_without_guessing() {
        let mut wind = controlled_links();
        let links = [fm(&[("SourceControlID", "300"), ("DestControlID", "7"),
            ("LinkTypeCode", "1"), ("InvertSourceControlValue", "Y"),
            ("SourceControlValueIncrement", "-27"), ("SourceControlValueCoefficient", "2.54")])];
        wind.voeg_besturing_toe(
            &[fm(&[("SwitchID", "7"), ("DefaultToEngaged", "N")])],
            &[fm(&[("ControlID", "7"), ("DefaultValue", "127")]), fm(&[("ControlID", "300"), ("DefaultValue", "0")])], &[],
            &[("ContinuousControlLinkage", &links)],
        );
        let control = &wind.controls[1];
        assert!(control.automatic);
        assert!(control.issues.is_empty());
        assert!(wind.automation.issues.is_empty());
        assert_eq!(wind.automation.cc_links.len(), 1);
        assert_eq!(wind.automation.cc_links[0].source, 300);
        assert!(wind.automation.cc_links[0].invert);
        assert_eq!(wind.automation.cc_links[0].increment, -27.0);
        assert_eq!(wind.automation.cc_links[0].coefficient, 2.54);
        assert_eq!(control.linkages[0].fields["InvertSourceControlValue"], "Y");
        assert_eq!(control.linkages[0].fields["SourceControlValueIncrement"], "-27");
        assert_eq!(control.linkages[0].fields["SourceControlValueCoefficient"], "2.54");
    }

    #[test]
    fn full_backward_closure_includes_conditional_inertia_and_stage_switches() {
        let mut wind = controlled_links();
        let cc = [fm(&[("ControlID", "7")]), fm(&[("ControlID", "21"), ("DefaultValue", "127")]),
            fm(&[("ControlID", "24"), ("DefaultValue", "0")])];
        let links = [fm(&[("SourceControlID", "21"), ("DestControlID", "24"), ("LinkTypeCode", "3"),
            ("InertialModelTypePositiveAcceleratingCoeff", "1000"), ("InertialModelTypePositiveDampingCoeff", "1")]),
            fm(&[("SourceControlID", "24"), ("DestControlID", "7"), ("LinkTypeCode", "2"),
                ("WillisTypeIncSpeedInMillisecondsPerStepWithOneStepDiff", "1000"), ("ConditionSwitchID", "100"),
                ("ConditionSwitchLinkIfEngaged", "Y")])];
        let stages = [fm(&[("ContinuousControlID", "24"), ("ContinuousControlValue", "127"),
            ("ControlledSwitchID", "100"), ("EngageWhenValueIncreasing", "Y")])];
        wind.voeg_besturing_toe(&[fm(&[("SwitchID", "7"), ("DefaultToEngaged", "N")]),
            fm(&[("SwitchID", "100"), ("DefaultToEngaged", "N")])], &cc, &[],
            &[("ContinuousControlLinkage", &links), ("ContinuousControlStageSwitch", &stages)]);
        assert!(wind.controls.iter().all(|control| control.issues.is_empty()));
        assert!(wind.automation.issues.is_empty(), "{:?}", wind.automation.issues);
        assert_eq!(wind.controls.len(), 5);
        assert_eq!(wind.automation.cc_links.len(), 2);
        assert_eq!(wind.automation.stages.len(), 1);
        assert!(matches!(wind.automation.cc_links[0].model, WindContinuousLinkModel::Inertia { acceleration: 1000.0, damping: 1.0 }));
        let target = wind.controls.iter().find(|control| control.kind == ValveControlKind::Continuous && control.id == 7).unwrap();
        assert!(target.automatic);
        assert_eq!(target.default_value, 0.0, "only a produced controller may derive its missing initial value");
    }

    #[test]
    fn physical_outputs_bring_their_source_parameters_and_feedback_chain() {
        let compartments = [fm(&[("WindCompartmentID", "1"), ("Bellows_HasBellows", "Y"),
            ("Bellows_FrameBaseWidthMetres", "2"), ("Bellows_FrameBaseLengthMetres", "5"),
            ("Bellows_MaximumExtensionMetres", "0.4"), ("Bellows_MassOfMovingBoardActedOnByGravityKg", "117"),
            ("Bellows_MassOfMovingBoardGivingRiseToInertiaKg", "118"), ("Bellows_ShapeCode", "2"),
            ("Bellows_MovementDirectionCode", "1"), ("Bellows_DistOfCenOfMassOfMvgBrdFromAxisIfDiagTypMetres", "2.4"),
            ("Bellows_ExtensionOutputContinuousControlID", "300")])];
        let mut wind = WindwerkDef::uit_tabellen(&compartments, &[
            fm(&[("FirstWindCompartmentID", "1"), ("SecondWindCompartmentID", "2"),
                ("ValveControlTypeCode", "3"), ("ValveControllingContinuousControlID", "301")])]);
        let links = [fm(&[("SourceControlID", "300"), ("DestControlID", "301"), ("LinkTypeCode", "1"),
            ("InvertSourceControlValue", "Y"), ("SourceControlValueIncrement", "-27"), ("SourceControlValueCoefficient", "2.54")])];
        wind.voeg_besturing_toe(&[], &[fm(&[("ControlID", "300"), ("DefaultValue", "0")]),
            fm(&[("ControlID", "301"), ("DefaultValue", "127")])], &[],
            &[("ContinuousControlLinkage", &links), ("WindCompartment", &compartments)]);
        assert!(wind.automation.issues.is_empty());
        assert_eq!(wind.automation.outputs[0].kind, WindControlOutputKind::BellowsExtension { compartment_id: 1 });
        assert!(wind.controls.iter().all(|control| control.automatic));
        let bellows = wind.laden[0].balg.as_ref().unwrap();
        assert_eq!(bellows.moving_inertia_mass_kg, Some(118.0));
        assert_eq!(bellows.shape_code, Some(2));
        assert_eq!(bellows.movement_direction_code, Some(1));
        assert_eq!(bellows.gravity_center_distance_m, Some(2.4));
    }

    #[test]
    fn unknown_automation_fields_codes_and_missing_inputs_are_not_silently_dropped() {
        for fields in [vec![("LinkTypeCode", "99")], vec![("LinkTypeCode", "1"), ("UnimplementedOperation", "1")]] {
            let mut wind = controlled_links();
            let mut values = vec![("SourceControlID", "300"), ("DestControlID", "7")];
            values.extend(fields);
            wind.voeg_besturing_toe(&[fm(&[("SwitchID", "7"), ("DefaultToEngaged", "N")])],
                &[fm(&[("ControlID", "7"), ("DefaultValue", "127")])], &[], &[("ContinuousControlLinkage", &[fm(&values)])]);
            assert!(!wind.automation.issues.is_empty());
            assert!(wind.controls.iter().any(|control| control.id == 300 && !control.issues.is_empty()));
        }
    }

    #[test]
    fn unsupported_feedback_springs_and_geometry_are_rejected_explicitly() {
        for (field, value) in [("Bellows_OpeningSprings_HasOpeningSprings", "Y"),
            ("Bellows_ClosingSprings_HasClosingSprings", "Y"), ("Bellows_ShapeCode", "99"),
            ("Bellows_MovementDirectionCode", "2"), ("Bellows_ShapeCode", "unknown")] {
            let mut values = vec![("WindCompartmentID", "1"), ("Bellows_HasBellows", "Y"),
                ("Bellows_ExtensionOutputContinuousControlID", "7")];
            values.push((field, value));
            let compartments = [fm(&values)];
            let mut wind = controlled_links();
            wind.voeg_besturing_toe(&[fm(&[("SwitchID", "7"), ("DefaultToEngaged", "N")])],
                &[fm(&[("ControlID", "7"), ("DefaultValue", "127")])], &[], &[("WindCompartment", &compartments)]);
            assert!(!wind.automation.issues.is_empty(), "{field}={value} was not diagnosed");
        }
    }

    #[test]
    #[ignore = "requires the optional locally installed Nitra demo definition"]
    fn installed_nitra_readable_definition_retains_complete_blower_and_bellows_closure() {
        let path = std::env::var_os("JM_NITRA_DEFINITION").map(std::path::PathBuf::from)
            .expect("Set JM_NITRA_DEFINITION to the optional readable Nitra demo definition before running this ignored test");
        let xml = std::fs::read_to_string(path).expect("optional Nitra demo definition");
        let tables: Vec<_> = ["WindCompartment", "WindCompartmentLinkage", "SwitchLinkage", "ContinuousControlLinkage",
            "ContinuousControlDoubleLinkage", "ContinuousControlStageSwitch"].into_iter()
            .map(|table| (table, crate::hauptwerk::extract_objects(&xml, table))).collect();
        let mut wind = WindwerkDef::uit_tabellen(&tables[0].1, &tables[1].1);
        let automation: Vec<_> = tables.iter().map(|(name, rows)| (*name, rows.as_slice())).collect();
        wind.voeg_besturing_toe(&crate::hauptwerk::extract_objects(&xml, "Switch"),
            &crate::hauptwerk::extract_objects(&xml, "ContinuousControl"),
            &crate::hauptwerk::extract_objects(&xml, "Stop"), &automation);
        assert_eq!(wind.controls.len(), 18);
        assert_eq!(wind.automation.cc_links.len(), 10);
        assert_eq!(wind.automation.switch_links.len(), 5);
        assert_eq!(wind.automation.stages.len(), 4);
        assert_eq!(wind.automation.outputs.len(), 2);
        assert!(wind.automation.issues.is_empty(), "{:?}", wind.automation.issues);
        assert!(wind.controls.iter().all(|control| control.issues.is_empty()));
    }

    #[test]
    fn malformed_valves_and_missing_controller_defaults_are_explicit() {
        let mut wind = controlled_links();
        wind.voeg_besturing_toe(&[], &[fm(&[("ControlID", "7"), ("DefaultValue", "NaN")])], &[], &[]);
        assert!(wind.controls.iter().all(|control| !control.issues.is_empty()));
        for code in ["2", "3", "4", "unknown"] {
            let invalid = WindwerkDef::uit_tabellen(&[], &[
                fm(&[("FirstWindCompartmentID", "1"), ("SecondWindCompartmentID", "2"), ("ValveControlTypeCode", code)])
            ]);
            assert!(matches!(invalid.verbindingen[0].valve_control, Some(ValveControlRef::Unsupported { .. })));
            assert!(invalid.verbindingen[0].klep);
        }
    }

    #[test]
    fn explicit_hidden_constants_are_read_only_but_unmarked_inputs_remain_manual() {
        let mut wind = controlled_links();
        wind.voeg_besturing_toe(&[fm(&[("SwitchID", "7"), ("DefaultToEngaged", "N")])],
            &[fm(&[("ControlID", "7"), ("DefaultValue", "127"), ("AccessibleForInput", "N"), ("Clickable", "N")])], &[], &[]);
        assert!(!wind.controls[0].automatic);
        assert!(wind.controls[1].automatic);
        assert!(wind.controls.iter().all(|control| control.issues.is_empty()));
        let mut uninitialized = controlled_links();
        uninitialized.voeg_besturing_toe(&[], &[fm(&[("ControlID", "7"), ("AccessibleForInput", "N"), ("Clickable", "N")])], &[], &[]);
        assert!(uninitialized.controls.iter().all(|control| !control.issues.is_empty()), "hidden is not a producer");
    }

    #[test]
    fn older_wind_metadata_deserializes_and_retains_legacy_controlled_flag() {
        let wind: WindwerkDef = serde_json::from_str(r#"{"laden":[],"verbindingen":[{"van":1,"naar":2,"naam":"legacy","stroom_kg_s":1.0,"ref_inch":1.0,"klep":true}]}"#).unwrap();
        assert!(wind.controls.is_empty());
        assert!(wind.verbindingen[0].klep);
        assert!(wind.verbindingen[0].valve_control.is_none());
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
