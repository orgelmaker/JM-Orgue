use super::*;

fn flat_voice(node: Option<usize>) -> PlayingVoice {
    let sample = Arc::new(SampleData { data: vec![1.0f32; 512].into(), right: None,
        sample_rate: 48_000, channels: 1, loop_start: Some(0), loop_end: Some(512) });
    let mut voice = PlayingVoice::new_from_sample(sample, 1, 1, 60, 1.0);
    voice.c_div = 0; voice.c_bus = 0; voice.c_project_wind_node = node;
    voice.c_kp = 0.0; voice.c_kg = 1.0; voice.c_dev_a = 1.0;
    voice.envelope = 1.0; voice.target_envelope = 1.0;
    voice.c_use_lfo_trem = false;
    voice
}

#[test]
fn independent_source_pressure_reaches_voices_on_the_same_target_manual() {
    let legacy = [[-0.3f32; 32]; 8];
    let mut project = [[0.0f32; MAX_PROJECT_WIND_NODES]; 8];
    for frame in &mut project { frame[0] = -0.1; frame[1] = 0.0; }
    let neutral_rate = [[1.0f64; 32]; 8]; let neutral_amp = [[1.0f32; 32]; 8];
    let pan_l = [1.0; 32]; let pan_r = [0.0; 32]; let pans = [0.0; 32]; let enabled = [false; 32];
    let context = MengBlok { bn: 8, wind_dev: &legacy, project_wind_dev: Some(&project),
        trem_rate: &neutral_rate, trem_amp: &neutral_amp, pan_cos: &pan_l, pan_sin: &pan_r,
        div_pans: &pans, ccis_on: false, ccis_en: &enabled, ccis_strength: 0.0,
        ccis_falloff: 0.0, ccis_swap: false, pitch_offset_mul: 1.0, licht: false };
    let mut a = [flat_voice(Some(0))]; let mut b = [flat_voice(Some(1))];
    let mut left_a = [[0.0; 32]; 8]; let mut right_a = [[0.0; 32]; 8];
    let mut left_b = [[0.0; 32]; 8]; let mut right_b = [[0.0; 32]; 8];
    meng_stemmen_blok(&mut a, &mut left_a, &mut right_a, &context);
    meng_stemmen_blok(&mut b, &mut left_b, &mut right_b, &context);
    assert!((left_a[7][0] - 0.9).abs() < 1e-5);
    assert!((left_b[7][0] - 1.0).abs() < 1e-5);
    assert_eq!(a[0].c_div, b[0].c_div);
}

#[test]
fn normal_voices_use_only_the_original_division_pressure() {
    let legacy = [[-0.03f32; 32]; 1]; let project = [[-0.2f32; MAX_PROJECT_WIND_NODES]; 1];
    let tr = [[1.0f64; 32]; 1]; let ta = [[1.0f32; 32]; 1];
    let p = [1.0; 32]; let r = [0.0; 32]; let enabled = [false; 32];
    let context = MengBlok { bn: 1, wind_dev: &legacy, project_wind_dev: Some(&project), trem_rate: &tr,
        trem_amp: &ta, pan_cos: &p, pan_sin: &r, div_pans: &r, ccis_on: false, ccis_en: &enabled,
        ccis_strength: 0.0, ccis_falloff: 0.0, ccis_swap: false, pitch_offset_mul: 1.0, licht: false };
    assert_eq!(voice_wind_deviation(None, &context, 0, 0), -0.03);
    let ordinary = MengBlok { project_wind_dev: None, ..context };
    assert_eq!(voice_wind_deviation(Some(0), &ordinary, 0, 0), -0.03);
}

#[test]
fn release_inherits_source_pressure_but_draws_no_more_wind() {
    let mut attack = flat_voice(Some(7)); attack.c_dev_cur = -0.025; attack.c_wind_flow = 3.0;
    let mut release = flat_voice(None); release.erf_wind(&attack, true);
    assert_eq!(release.c_project_wind_node, Some(7)); assert_eq!(release.c_wind_flow, 0.0);
    assert_eq!(release.c_dev_cur, -0.025); assert_eq!(release.c_dev_a, 0.0);
}

#[test]
fn fully_drained_source_is_silent_without_muting_other_source_or_ordinary_voice() {
    let legacy = [[-0.05f32; 32]; 8];
    let mut project = [[0.0f32; MAX_PROJECT_WIND_NODES]; 8];
    for frame in &mut project { frame[0] = -1.0; }
    let tr = [[1.0f64; 32]; 8]; let ta = [[1.0f32; 32]; 8];
    let p = [1.0; 32]; let r = [0.0; 32]; let enabled = [false; 32];
    let context = MengBlok { bn: 8, wind_dev: &legacy, project_wind_dev: Some(&project),
        trem_rate: &tr, trem_amp: &ta, pan_cos: &p, pan_sin: &r, div_pans: &r,
        ccis_on: false, ccis_en: &enabled, ccis_strength: 0.0, ccis_falloff: 0.0,
        ccis_swap: false, pitch_offset_mul: 1.0, licht: false };
    let mut drained = [flat_voice(Some(0))]; drained[0].c_kg = 0.0;
    let mut other = [flat_voice(Some(1))];
    let mut ordinary = [flat_voice(None)];
    let mut out_drained = [[0.0; 32]; 8]; let mut out_other = [[0.0; 32]; 8];
    let mut out_ordinary = [[0.0; 32]; 8]; let mut right = [[0.0; 32]; 8];
    meng_stemmen_blok(&mut drained, &mut out_drained, &mut right, &context);
    meng_stemmen_blok(&mut other, &mut out_other, &mut right, &context);
    meng_stemmen_blok(&mut ordinary, &mut out_ordinary, &mut right, &context);
    assert!(out_drained.iter().all(|frame| frame[0] == 0.0));
    assert!((out_other[7][0] - 1.0).abs() < 1e-5);
    assert!((out_ordinary[7][0] - 0.95).abs() < 1e-5);
    assert_eq!(voice_wind_gain(-1.0, 1.0, false), 0.25);
    assert_eq!(voice_wind_gain(-1.0, 1.0, true), 0.0);
}

#[test]
fn source_low_pressure_fade_is_continuous_and_preserves_normal_pressure_voicing() {
    assert!((voice_wind_gain(-0.4, 0.2, true) - voice_wind_gain(-0.4, 0.2, false)).abs() < 1e-6);
    let mut previous = 0.0;
    for step in 0..=1000 {
        let deviation = -1.0 + step as f32 / 1000.0;
        let gain = voice_wind_gain(deviation, 0.2, true);
        assert!(gain.is_finite() && gain >= previous && gain <= 1.0);
        assert!((gain - previous) < 0.003);
        previous = gain;
    }
}
