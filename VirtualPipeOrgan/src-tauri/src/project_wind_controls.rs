//! Validation and restoration of source-local, standalone valve targets.
use std::collections::HashMap;
use crate::windwerk_set::ProjectWindPlan;
use vpo_sampler::windwerk::ValveControlKind;

pub fn normalise(kind: ValveControlKind, value: f32) -> Result<f32, String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("Een klepstand moet een eindig getal tussen 0 en 1 zijn".into());
    }
    Ok(match kind { ValveControlKind::Switch => if value >= 0.5 { 1.0 } else { 0.0 },
        ValveControlKind::Continuous => value })
}

pub fn restore(plan: &ProjectWindPlan, saved: &HashMap<String, f32>) -> HashMap<String, f32> {
    plan.controls.iter().filter(|control| !control.automatic && control.project_stop_ids.is_empty()).map(|control| {
        let value = saved.get(&control.id).and_then(|value| normalise(control.kind, *value).ok())
            .unwrap_or(control.default_value);
        (control.id.clone(), value)
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windwerk_set::ProjectWindControl;

    #[test]
    fn invalid_values_cannot_reach_audio_or_saved_settings() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
            assert!(normalise(ValveControlKind::Continuous, value).is_err());
        }
        assert_eq!(normalise(ValveControlKind::Switch, 0.49).unwrap(), 0.0);
        assert_eq!(normalise(ValveControlKind::Switch, 0.5).unwrap(), 1.0);
    }

    #[test]
    fn restore_isolates_sources_drops_stale_and_linked_values_and_uses_defaults() {
        let control = |id: &str, linked: bool| ProjectWindControl {
            id: id.into(), source_id: id[..1].into(), source_control_id: 9,
            kind: ValveControlKind::Continuous, name: id.into(), default_value: 0.75,
            project_stop_ids: if linked { vec![3] } else { vec![] }, travel_seconds: 0.01, automatic: false,
        };
        let mut plan = crate::windwerk_set::compile_project_wind(&[], &[]).unwrap();
        let mut automatic = control("A:continuous:10", false);
        automatic.automatic = true;
        plan.controls = vec![control("A:continuous:9", false), control("B:continuous:9", false), control("A:switch:1", true), automatic];
        let saved = HashMap::from([("A:continuous:9".into(), 0.2), ("B:continuous:9".into(), f32::NAN),
            ("A:switch:1".into(), 1.0), ("A:continuous:10".into(), 0.5), ("removed:continuous:9".into(), 1.0)]);
        assert_eq!(restore(&plan, &saved), HashMap::from([("A:continuous:9".into(), 0.2), ("B:continuous:9".into(), 0.75)]));
    }
}
