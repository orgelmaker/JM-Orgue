//! JM's explicitly defined, bounded interpretation of the source controller
//! objects. Compile identities, dependency ordering and response coefficients
//! before entering the callback; only indexed arithmetic runs in real time.
use super::*;
use vpo_sampler::windwerk::{WindContinuousLinkModel, WindControlOutputKind, WindSwitchAction};

#[derive(Debug, Clone, Default)]
pub struct ProjectAutomationPlan {
    operations: Vec<Operation>,
    order: Vec<usize>,
    sensors: Vec<Sensor>,
}
#[derive(Debug, Clone)]
struct Condition {
    switch: usize,
    engaged: bool,
    reevaluate: bool,
}
#[derive(Debug, Clone)]
enum Response {
    Direct,
    Willis(f64),
    Inertia { acceleration: f64, damping: f64 },
}
#[derive(Debug, Clone)]
enum Operation {
    Continuous {
        source: usize,
        dest: usize,
        condition: Option<Condition>,
        invert: bool,
        increment: f64,
        coefficient: f64,
        index: f64,
        response: Response,
    },
    Switch {
        source: usize,
        dest: usize,
        condition: Option<Condition>,
        source_engaged: bool,
        engage: WindSwitchAction,
        disengage: WindSwitchAction,
    },
    Stage {
        source: usize,
        dest: usize,
        threshold: f64,
        engage_up: bool,
        engage_down: bool,
        disengage_up: bool,
        disengage_down: bool,
    },
}
impl Operation {
    fn dest(&self) -> usize {
        match self {
            Self::Continuous { dest, .. }
            | Self::Switch { dest, .. }
            | Self::Stage { dest, .. } => *dest,
        }
    }
    fn source(&self) -> usize {
        match self {
            Self::Continuous { source, .. }
            | Self::Switch { source, .. }
            | Self::Stage { source, .. } => *source,
        }
    }
    fn condition(&self) -> Option<&Condition> {
        match self {
            Self::Continuous { condition, .. } | Self::Switch { condition, .. } => {
                condition.as_ref()
            }
            _ => None,
        }
    }
    fn instant_continuous(&self) -> bool {
        matches!(
            self,
            Self::Continuous {
                response: Response::Direct,
                ..
            }
        )
    }
}
#[derive(Debug, Clone)]
enum Sensor {
    Extension {
        node: usize,
        control: usize,
    },
    Pressure {
        node: usize,
        control: usize,
    },
    Flow {
        link: usize,
        control: usize,
        maximum: f64,
    },
}

pub(super) fn compile_source_automation(
    plan: &mut ProjectWindPlan,
    source: &ProjectWindSource,
    control_slots: &HashMap<(&str, u32), usize>,
    node_slots: &HashMap<(String, u32), usize>,
    first_link: usize,
) -> Result<(), String> {
    let graph = &source.windwerk.automation;
    if !graph.issues.is_empty() {
        return Err(format!(
            "Bron {}: {}",
            source.source_id,
            graph.issues.join("; ")
        ));
    }
    let slot = |kind: &str, id: u32| {
        control_slots.get(&(kind, id)).copied().ok_or_else(|| {
            format!(
                "Bron {} mist {} {} in de windbesturingsketen",
                source.source_id, kind, id
            )
        })
    };
    let condition = |value: &Option<vpo_sampler::windwerk::WindControlConditionDef>| -> Result<Option<Condition>, String> {
        value.as_ref().map(|c| Ok(Condition { switch: slot("switch", c.switch_id)?, engaged: c.engaged, reevaluate: c.reevaluate })).transpose()
    };
    for link in &graph.cc_links {
        if ![link.increment, link.coefficient, link.index]
            .into_iter()
            .all(f32::is_finite)
            || link.coefficient < 0.0
            || link.index <= 0.0
        {
            return Err(format!("Ongeldige overdrachtsfunctie voor '{}'", link.name));
        }
        let response = match link.model {
            WindContinuousLinkModel::Direct => Response::Direct,
            WindContinuousLinkModel::Willis {
                milliseconds_per_step,
            } if milliseconds_per_step.is_finite() && milliseconds_per_step > 0.0 => {
                Response::Willis(f64::from(milliseconds_per_step) / 1000.0)
            }
            WindContinuousLinkModel::Inertia {
                acceleration,
                damping,
            } if acceleration.is_finite()
                && acceleration > 0.0
                && damping.is_finite()
                && damping >= 0.0 =>
            {
                Response::Inertia {
                    acceleration: f64::from(acceleration),
                    damping: f64::from(damping),
                }
            }
            _ => {
                return Err(format!(
                    "Ongeldige tijd-/inertieparameters voor '{}'",
                    link.name
                ))
            }
        };
        plan.automation.operations.push(Operation::Continuous {
            source: slot("continuous", link.source)?,
            dest: slot("continuous", link.dest)?,
            condition: condition(&link.condition)?,
            invert: link.invert,
            increment: f64::from(link.increment),
            coefficient: f64::from(link.coefficient),
            index: f64::from(link.index),
            response,
        });
    }
    for link in &graph.switch_links {
        plan.automation.operations.push(Operation::Switch {
            source: slot("switch", link.source)?,
            dest: slot("switch", link.dest)?,
            condition: condition(&link.condition)?,
            source_engaged: link.source_engaged,
            engage: link.engage_action,
            disengage: link.disengage_action,
        });
    }
    for stage in &graph.stages {
        if !stage.threshold.is_finite() || !(0.0..=1.0).contains(&stage.threshold) {
            return Err("Ongeldige schakeltrap in bronwindbesturing".into());
        }
        plan.automation.operations.push(Operation::Stage {
            source: slot("continuous", stage.control)?,
            dest: slot("switch", stage.switch_id)?,
            threshold: f64::from(stage.threshold) * 127.0,
            engage_up: stage.engage_increasing,
            engage_down: stage.engage_decreasing,
            disengage_up: stage.disengage_increasing,
            disengage_down: stage.disengage_decreasing,
        });
    }
    for output in &graph.outputs {
        let control = slot("continuous", output.control_id)?;
        let node = |id| {
            node_slots
                .get(&(source.source_id.clone(), id))
                .copied()
                .ok_or_else(|| format!("Ontbrekende windlade {} voor bronuitgang", id))
        };
        plan.automation.sensors.push(match output.kind {
            WindControlOutputKind::BellowsExtension { compartment_id } => {
                let node = node(compartment_id)?;
                if plan.nodes[node].mechanical.is_none() {
                    return Err("Slaguitgang verwijst naar een lade zonder bruikbare balg".into());
                }
                Sensor::Extension { node, control }
            }
            WindControlOutputKind::Pressure { compartment_id } => Sensor::Pressure {
                node: node(compartment_id)?,
                control,
            },
            WindControlOutputKind::Flow {
                link_index,
                max_flow_kg_s,
            } => {
                if link_index >= source.windwerk.verbindingen.len()
                    || !max_flow_kg_s.is_finite()
                    || max_flow_kg_s <= 0.0
                {
                    return Err("Ongeldige windstroomuitgang".into());
                }
                Sensor::Flow {
                    link: first_link + link_index,
                    control,
                    maximum: f64::from(max_flow_kg_s),
                }
            }
        });
    }
    Ok(())
}

impl ProjectAutomationPlan {
    pub(super) fn prepare(&mut self, controls: &[ProjectWindControl]) -> Result<(), String> {
        if self.operations.len() + self.sensors.len() > MAX_PROJECT_WIND_LINKS {
            return Err("Te veel bronbesturingsverbindingen".into());
        }
        // CC fan-in is unambiguous only for complementary switch conditions.
        // Stage/relay actions are stateful events and may share an output.
        let mut cc_writers: HashMap<usize, Vec<Option<&Condition>>> = HashMap::new();
        let mut sensor_destinations = HashSet::new();
        for sensor in &self.sensors {
            let dest = match sensor {
                Sensor::Extension { control, .. }
                | Sensor::Pressure { control, .. }
                | Sensor::Flow { control, .. } => *control,
            };
            if !sensor_destinations.insert(dest) {
                return Err("Meerdere bronuitgangen schrijven dezelfde regelaar".into());
            }
        }
        for operation in &self.operations {
            if let Operation::Continuous {
                dest, condition, ..
            } = operation
            {
                cc_writers
                    .entry(*dest)
                    .or_default()
                    .push(condition.as_ref());
            }
        }
        for (dest, writers) in &cc_writers {
            if sensor_destinations.contains(dest) {
                return Err(
                    "Een regelaar heeft zowel een sensor- als een koppelingsuitgang".into(),
                );
            }
            if writers.len() > 1
                && !(writers.len() == 2
                    && matches!((writers[0], writers[1]), (Some(a), Some(b)) if a.switch == b.switch && a.engaged != b.engaged))
            {
                return Err("Conflicterende gelijktijdige schrijvers in bronbesturing".into());
            }
        }
        // Kahn order over direct CC dependencies rejects algebraic CC loops.
        // Stateful response/relay edges read a tick snapshot and break cycles.
        let count = controls.len();
        let mut successors = vec![Vec::new(); count];
        let mut degree = vec![0usize; count];
        for operation in &self.operations {
            if operation.instant_continuous() {
                successors[operation.source()].push(operation.dest());
                degree[operation.dest()] += 1;
            }
        }
        let mut queue: Vec<usize> = (0..count).filter(|i| degree[*i] == 0).collect();
        let mut cursor = 0;
        while cursor < queue.len() {
            let node = queue[cursor];
            cursor += 1;
            for &dest in &successors[node] {
                degree[dest] -= 1;
                if degree[dest] == 0 {
                    queue.push(dest);
                }
            }
        }
        if queue.len() != count {
            return Err(
                "Directe cyclische bronbesturing heeft geen tijd- of fysieke toestand".into(),
            );
        }
        let mut rank = vec![0usize; count];
        for (i, node) in queue.into_iter().enumerate() {
            rank[node] = i;
        }
        self.order = (0..self.operations.len()).collect();
        self.order
            .sort_by_key(|i| (rank[self.operations[*i].dest()], *i));
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct ResponseStep {
    m00: f64,
    m01: f64,
    m10: f64,
    m11: f64,
    first_order: bool,
}
impl ResponseStep {
    fn new(response: &Response, dt: f64) -> Self {
        match *response {
            Response::Direct => Self {
                m00: 0.0,
                m01: 0.0,
                m10: 0.0,
                m11: 0.0,
                first_order: true,
            },
            Response::Willis(tau) => Self {
                m00: (-dt / tau).exp(),
                m01: 0.0,
                m10: 0.0,
                m11: 0.0,
                first_order: true,
            },
            Response::Inertia {
                acceleration: a,
                damping: d,
            } => {
                let h = d / 2.0;
                let discriminant = h * h - a;
                let (s, c) = if discriminant.abs() < 1e-10 * a.max(1.0) {
                    let e = (-h * dt).exp();
                    (dt * e, e)
                } else if discriminant < 0.0 {
                    let w = (-discriminant).sqrt();
                    let e = (-h * dt).exp();
                    ((w * dt).sin() / w * e, (w * dt).cos() * e)
                } else {
                    let w = discriminant.sqrt();
                    let first = ((-h + w) * dt).exp();
                    let second = ((-h - w) * dt).exp();
                    ((first - second) / (2.0 * w), (first + second) / 2.0)
                };
                Self {
                    m00: c + h * s,
                    m01: s,
                    m10: -a * s,
                    m11: c - h * s,
                    first_order: false,
                }
            }
        }
    }
}
#[derive(Debug, Clone)]
pub(super) struct AutomationRuntime {
    previous: Vec<f32>,
    previous_inputs: Vec<f64>,
    previous_conditions: Vec<bool>,
    active: Vec<bool>,
    latched_targets: Vec<f64>,
    velocities: Vec<f64>,
    response_steps: Vec<ResponseStep>,
}
impl AutomationRuntime {
    pub(super) fn new(plan: &ProjectWindPlan, dt: f64) -> Self {
        let values: Vec<_> = plan.controls.iter().map(|c| c.default_value).collect();
        Self {
            previous: values.clone(),
            previous_inputs: plan
                .automation
                .operations
                .iter()
                .map(|op| f64::from(values[op.source()]) * 127.0)
                .collect(),
            previous_conditions: vec![false; plan.automation.operations.len()],
            active: vec![false; plan.automation.operations.len()],
            latched_targets: vec![0.0; plan.automation.operations.len()],
            velocities: vec![0.0; plan.automation.operations.len()],
            response_steps: plan
                .automation
                .operations
                .iter()
                .map(|op| {
                    ResponseStep::new(
                        match op {
                            Operation::Continuous { response, .. } => response,
                            _ => &Response::Direct,
                        },
                        dt,
                    )
                })
                .collect(),
        }
    }
    pub(super) fn tick(
        &mut self,
        plan: &ProjectWindPlan,
        values: &mut [f32],
        extensions: &[f64],
        pressure: &[f64],
        flows: &[f64],
    ) {
        self.previous.copy_from_slice(values);
        for sensor in &plan.automation.sensors {
            let (control, value) = match *sensor {
                Sensor::Extension { node, control } => (control, extensions[node]),
                Sensor::Pressure { node, control } => (
                    control,
                    if plan.nodes[node].nominal_pa > 0.0 {
                        pressure[node] / plan.nodes[node].nominal_pa
                    } else {
                        0.0
                    },
                ),
                Sensor::Flow {
                    link,
                    control,
                    maximum,
                } => (control, flows[link] / maximum),
            };
            values[control] = value.clamp(0.0, 1.0) as f32;
        }
        for &i in &plan.automation.order {
            let operation = &plan.automation.operations[i];
            let condition = operation.condition();
            let enabled = condition.is_none_or(|c| (self.previous[c.switch] >= 0.5) == c.engaged);
            match operation {
                Operation::Continuous {
                    source,
                    dest,
                    invert,
                    increment,
                    coefficient,
                    index,
                    response,
                    ..
                } => {
                    let source_raw = f64::from(if matches!(response, Response::Direct) {
                        values[*source]
                    } else {
                        self.previous[*source]
                    }) * 127.0;
                    let input_changed = (source_raw - self.previous_inputs[i]).abs() > 1e-9;
                    if enabled
                        && (condition.is_none()
                            || input_changed
                            || !self.previous_conditions[i]
                                && condition.is_some_and(|c| c.reevaluate))
                    {
                        let raw = source_raw;
                        let raw = if *invert { 127.0 - raw } else { raw };
                        self.latched_targets[i] = (((raw + increment).max(0.0) * coefficient)
                            .powf(*index))
                        .clamp(0.0, 127.0);
                        self.active[i] = true;
                    }
                    self.previous_inputs[i] = source_raw;
                    if enabled && self.active[i] {
                        let target = self.latched_targets[i];
                        let current = f64::from(values[*dest]) * 127.0;
                        let step = self.response_steps[i];
                        let error = current - target;
                        let mut next = target + step.m00 * error + step.m01 * self.velocities[i];
                        let mut velocity = step.m10 * error + step.m11 * self.velocities[i];
                        if next <= 0.0 || next >= 127.0 {
                            next = next.clamp(0.0, 127.0);
                            velocity = 0.0;
                        }
                        if step.first_order && (next - target).abs() <= 0.5 {
                            next = target;
                            velocity = 0.0;
                        }
                        values[*dest] = (next / 127.0) as f32;
                        self.velocities[i] = velocity;
                    } else {
                        self.velocities[i] = 0.0;
                        if !enabled {
                            self.active[i] = false;
                        }
                    }
                }
                Operation::Switch {
                    source,
                    dest,
                    source_engaged,
                    engage,
                    disengage,
                    ..
                } => {
                    let engaged = (self.previous[*source] >= 0.5) == *source_engaged;
                    let was_engaged = (self.previous_inputs[i] >= 63.5) == *source_engaged;
                    if enabled
                        && (engaged != was_engaged
                            || !self.previous_conditions[i]
                                && condition.is_none_or(|c| c.reevaluate))
                    {
                        match if engaged { engage } else { disengage } {
                            WindSwitchAction::Engage => values[*dest] = 1.0,
                            WindSwitchAction::Disengage => values[*dest] = 0.0,
                            WindSwitchAction::NoAction => {}
                        }
                    }
                    self.previous_inputs[i] = f64::from(self.previous[*source]) * 127.0;
                }
                Operation::Stage {
                    source,
                    dest,
                    threshold,
                    engage_up,
                    engage_down,
                    disengage_up,
                    disengage_down,
                } => {
                    let current = (f64::from(values[*source]) * 127.0).round();
                    let previous = self.previous_inputs[i].round();
                    let threshold = threshold.round();
                    let up = current > previous && previous <= threshold && current >= threshold;
                    let down = current < previous && previous >= threshold && current <= threshold;
                    if up && *engage_up || down && *engage_down {
                        values[*dest] = 1.0;
                    }
                    if up && *disengage_up || down && *disengage_down {
                        values[*dest] = 0.0;
                    }
                    self.previous_inputs[i] = current;
                }
            }
            self.previous_conditions[i] = enabled;
        }
    }
}
