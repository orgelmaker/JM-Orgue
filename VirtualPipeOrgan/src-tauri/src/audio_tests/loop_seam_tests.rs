use super::{read_voice, read_voice_lr, xfade_weights, interp, linear2, hermite4};

/// Lichte interpolatie (0.9.30): lineair tussen de buren, gelijk aan cubisch
/// op een geheel punt, en `interp` kiest de juiste tak.
#[test]
fn lineaire_interpolatie_klopt_en_interp_kiest() {
    let d = [0.0f32, 1.0, 0.0, -1.0];
    // Op een geheel punt (frac 0) geven beide exact de sample.
    assert_eq!(linear2(&d, 1, 0.0), 1.0);
    assert_eq!(interp(&d, 1, 0.0, true), 1.0);
    assert_eq!(interp(&d, 1, 0.0, false), 1.0);
    // Halverwege 1→2 (1.0→0.0) is lineair precies 0,5.
    assert!((linear2(&d, 1, 0.5) - 0.5).abs() < 1e-6);
    // interp(licht=true) == linear2, interp(licht=false) == hermite4.
    assert_eq!(interp(&d, 1, 0.5, true), linear2(&d, 1, 0.5));
    assert_eq!(interp(&d, 1, 0.5, false), hermite4(&d, 1, 0.5));
    // Lineair blijft tussen de twee buren (geen overshoot).
    let v = linear2(&d, 1, 0.5);
    assert!(v <= 1.0 && v >= 0.0);
}

/// De lichte tak van read_voice_lr geeft op een geheel punt hetzelfde als de
/// cubische — het overschakelen tussen callbacks kan dus geen sprong geven.
#[test]
fn read_voice_licht_en_cubisch_gelijk_op_geheel_punt() {
    let data: Vec<f32> = (0..64).map(|i| ((i as f32) * 0.1).sin()).collect();
    let (ls, le, xf) = (8usize, 56usize, 8usize);
    let mut env = 1.0f32;
    let mut p1 = 20.0f64;
    let a = read_voice(&data, ls, le, xf, &mut p1, false, &mut env, false);
    let mut p2 = 20.0f64;
    let b = read_voice(&data, ls, le, xf, &mut p2, false, &mut env, true);
    assert!((a - b).abs() < 1e-6, "cubisch {a} vs licht {b} op geheel punt");
}

/// Sinustabel ≈ sin/cos en blijft equal-power over de hele fade.
#[test]
fn xfade_lut_matches_sin_cos() {
    for k in 0..=2000 {
        let fade = k as f32 / 2000.0;
        let (wp, wb) = xfade_weights(fade);
        let theta = fade * std::f32::consts::FRAC_PI_2;
        assert!((wp - theta.sin()).abs() < 2e-6, "sin bij {}", fade);
        assert!((wb - theta.cos()).abs() < 2e-6, "cos bij {}", fade);
        assert!((wp * wp + wb * wb - 1.0).abs() < 5e-6, "power bij {}", fade);
    }
}

/// Stereo-lezer: rechtervlak volgt exact dezelfde positie/loop/crossfade als links.
#[test]
fn read_voice_lr_reads_both_planes_at_same_index() {
    let n = 4000usize;
    let l: Vec<f32> = (0..n).map(|i| (i as f32 * 0.05).sin()).collect();
    let r: Vec<f32> = l.iter().map(|v| -v * 0.5).collect();
    let (ls, le, xfade) = (1000usize, 3000usize, 500usize);
    let mut pos = 0.0f64;
    let mut env = 1.0f32;
    let mut pos_m = 0.0f64;
    let mut env_m = 1.0f32;
    for _ in 0..6000 {
        let (a, b) = read_voice_lr(&l, Some(&r), ls, le, xfade, &mut pos, false, &mut env, false);
        let m = read_voice(&l, ls, le, xfade, &mut pos_m, false, &mut env_m, false);
        assert_eq!(a, m);
        assert!((b + 0.5 * a).abs() < 1e-6);
        pos += 1.0;
        pos_m += 1.0;
    }
}

// Build a sine whose value at `ls` differs from `le` so a naive hard wrap
// would jump (click). The crossfade must keep the output continuous.
fn sine(n: usize) -> Vec<f32> {
    (0..n).map(|i| (i as f32 * 0.05).sin()).collect()
}

#[test]
fn loop_seam_is_click_free() {
    let data = sine(6000);
    let (ls, le) = (1000usize, 3000usize);
    let xfade = super::loop_xfade(le - ls, ls);
    let mut pos = 0.0f64;
    let mut env = 1.0f32;
    let mut prev: Option<f32> = None;
    let mut max_jump = 0.0f32;
    // Long enough to cross the seam several times.
    for _ in 0..20_000 {
        let v = read_voice(&data, ls, le, xfade, &mut pos, false, &mut env, false);
        if let Some(p) = prev {
            max_jump = max_jump.max((v - p).abs());
        }
        prev = Some(v);
        pos += 1.0;
    }
    // Per-sample slope of the sine is ~0.05; a hard-wrap click would be ~|sin(le)-sin(ls)| (up to ~2).
    assert!(max_jump < 0.15, "loop seam click detected: max consecutive jump = {}", max_jump);
}

#[test]
fn loop_end_at_buffer_end_has_no_zero_sample_tick() {
    // le == len, zoals bij preload-buffers en fallback-loops. De oude code
    // liet de positie in [len-1, len) belanden vóór de wrap-check en gaf
    // dan één sample 0.0 terug → een periodieke tik bij elke loop-omloop.
    let data = sine(6000);
    let (ls, le) = (1000usize, 6000usize); // le == data.len()
    let xfade = super::loop_xfade(le - ls, ls);
    let mut pos = 0.0f64;
    let mut env = 1.0f32;
    let mut prev: Option<f32> = None;
    let mut max_jump = 0.0f32;
    for _ in 0..30_000 {
        let v = read_voice(&data, ls, le, xfade, &mut pos, false, &mut env, false);
        if let Some(p) = prev {
            max_jump = max_jump.max((v - p).abs());
        }
        prev = Some(v);
        pos += 1.0;
    }
    assert!(max_jump < 0.15, "tik bij loop-einde == bufferlengte: max jump = {}", max_jump);
}

#[test]
fn hard_wrap_without_crossfade_does_click() {
    // Sanity check: with the crossfade disabled (xfade = 0) the SAME loop jumps,
    // proving the test sample really has a discontinuous seam.
    let data = sine(6000);
    let (ls, le) = (1000usize, 3000usize);
    let mut pos = 0.0f64;
    let mut env = 1.0f32;
    let mut prev: Option<f32> = None;
    let mut max_jump = 0.0f32;
    for _ in 0..20_000 {
        let v = read_voice(&data, ls, le, 0, &mut pos, false, &mut env, false);
        if let Some(p) = prev {
            max_jump = max_jump.max((v - p).abs());
        }
        prev = Some(v);
        pos += 1.0;
    }
    assert!(max_jump > 0.3, "expected an audible jump without crossfade, got {}", max_jump);
}
