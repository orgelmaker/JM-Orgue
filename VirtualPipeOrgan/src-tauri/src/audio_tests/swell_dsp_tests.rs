use super::swell_dsp::*;

#[test]
fn alpha_in_bereik_en_groter_bij_groter_blok() {
    let a256 = block_alpha(256, 48000, TAU_S);
    let a64 = block_alpha(64, 48000, TAU_S);
    assert!(a256 > 0.0 && a256 < 1.0);
    assert!(a64 > 0.0 && a64 < a256);
    // Kapotte invoer valt veilig terug.
    assert!(block_alpha(0, 0, 0.0) <= 1.0);
}

#[test]
fn smoothing_loopt_monotoon_en_convergeert_binnen_5_tau() {
    let sr = 48000u32;
    let alpha = block_alpha(256, sr, TAU_S);
    let mut g = 1.0f32;
    let mut prev = g;
    let mut blocks = 0;
    while g != 0.0 && blocks < 10_000 {
        g = smooth_step(g, 0.0, alpha);
        assert!(g <= prev, "niet monotoon dalend");
        prev = g;
        blocks += 1;
    }
    // Binnen ~5τ (100 ms = 4800 frames ≈ 19 blokken van 256) op het doel.
    assert!(blocks <= 40, "convergeert te traag: {} blokken", blocks);
    assert!(blocks >= 2, "te snel (geen smoothing): {} blokken", blocks);
    // Na 1τ ≈ 63 % van de weg afgelegd.
    let mut g2 = 1.0f32;
    let per_tau = (TAU_S * sr as f32 / 256.0).round() as usize;
    for _ in 0..per_tau { g2 = smooth_step(g2, 0.0, alpha); }
    assert!((g2 - (-1.0f32).exp()).abs() < 0.08, "na 1 tau: {}", g2);
}

#[test]
fn volume_en_cutoff_mapping() {
    assert!((volume(-20.0, 0.0) - 0.1).abs() < 1e-5);
    assert!((volume(-20.0, 1.0) - 1.0).abs() < 1e-6);
    assert!((volume(-30.0, 0.5) - 10f32.powf(-15.0 / 20.0)).abs() < 1e-5);
    assert!((cutoff_hz(800.0, 0.0) - 800.0).abs() < 0.01);
    assert!((cutoff_hz(800.0, 1.0) - 20000.0).abs() < 0.5);
    assert!((cutoff_hz(800.0, 0.5) - 4000.0).abs() < 1.0); // meetkundig gemiddelde
    assert!(cutoff_hz(800.0, 0.25) < cutoff_hz(800.0, 0.75));
}
