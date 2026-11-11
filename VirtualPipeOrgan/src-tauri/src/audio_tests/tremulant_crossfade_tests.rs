use super::*;

/// Kale preload-buffer: vlakke data (0,25) zodat een test posities meet en
/// niet de golfvorm.
fn preload(frames: usize, rate: u32, trim: usize, lp: Option<(u64, u64)>) -> Arc<PreloadBuffer> {
    Arc::new(PreloadBuffer {
        attack_data: vec![0.25; frames],
        attack_right: None,
        sample_rate: rate,
        total_samples: frames + trim,
        loop_start: lp.map(|(a, _)| a),
        loop_end: lp.map(|(_, b)| b),
        source_path: std::path::PathBuf::from("trem.wav"),
        channels: 1,
        bits_per_sample: 16,
        data_offset: 0,
        align_table: None,
        trim_start: trim,
        smpl_unity_note: None,
        smpl_pitch_fraction_cents: None,            harm_shelf: None,
    })
}

/// Klinkende stem die nog op zijn preload draait (bronrate ≠ uitgangsrate).
fn stem_op_preload(pos: f64, frames: usize, rate: u32, trim: usize, out_rate: u32) -> PlayingVoice {
    let mut v = PlayingVoice::new_from_preload(
        preload(frames, rate, trim, Some((frames as u64 / 4, frames as u64))),
        7, 60, 60, 1.0, out_rate,
    );
    v.position = pos;
    v
}

/// Klinkende stem op de volledige sample: die staat al op uitgangsrate
/// (rate = 1,0) en is NIET getrimd — positie = frames sinds bestandsbegin.
fn stem_op_volledige_sample(pos: f64, frames: usize, out_rate: u32) -> PlayingVoice {
    let sample: SampleRef = Arc::new(SampleData {
        data: vec![0.25; frames],
        right: None,
        sample_rate: out_rate,
        channels: 1,
        loop_start: None,
        loop_end: None,
    });
    let mut v = PlayingVoice::new_from_sample(sample, 7, 60, 60, 1.0);
    v.position = pos;
    v
}

/// Gelijke samplerate, verschillende trim: de nieuwe stem landt op exact
/// hetzelfde TIJDSTIP in de doelopname (trimverschil netjes verrekend).
#[test]
fn crossfade_rekent_trimverschil_om() {
    let bron = stem_op_preload(40_000.0, 200_000, 44_100, 1_000, 48_000);
    let doel = preload(200_000, 44_100, 500, Some((50_000, 190_000)));

    let pos = bron.crossfade_position_in(&doel, 48_000);

    // (40 000 + 1 000)/44 100 s → ×44 100 − 500 = 40 500 doelframes.
    assert!((pos - 40_500.0).abs() < 1.0, "verwachtte ~40500, kreeg {pos}");
    assert!(pos <= (doel.attack_data.len() - 2) as f64);
}

/// 44,1 kHz-set op een 48 kHz-uitgang, lang vastgehouden noot op de
/// VOLLEDIGE sample: de ruwe positie (480 000 output-frames = 10 s) ligt
/// ver voorbij het einde van de trem-preload (2 s). De omrekening moet hem
/// in de loop van de doelbuffer terugvouwen in plaats van in stilte.
#[test]
fn crossfade_vouwt_lange_noot_terug_in_de_loop() {
    let bron = stem_op_volledige_sample(480_000.0, 960_000, 48_000);
    let (ls, le) = (22_050u64, 80_000u64);
    let doel = preload(88_200, 44_100, 0, Some((ls, le)));

    // Zonder omrekening (het oude gedrag) lag de startpositie buiten de
    // buffer — bewijs dat deze test de regressie zou zien.
    assert!(bron.position > doel.attack_data.len() as f64);

    let pos = bron.crossfade_position_in(&doel, 48_000);

    assert!(pos >= ls as f64 && pos < le as f64,
            "landde buiten de loop van de doelbuffer: {pos}");
    // Exacte terugvouwing: 10 s = 441 000 doelframes → ls + (441000−ls) mod (le−ls).
    let verwacht = ls as f64 + (441_000.0 - ls as f64) % (le - ls) as f64;
    assert!((pos - verwacht).abs() < 1.0, "verwachtte ~{verwacht}, kreeg {pos}");
}

/// Doelbuffer zonder bruikbare looppunten: veilig midden in de buffer —
/// nooit opnieuw door de attack en nooit voorbij het einde.
#[test]
fn crossfade_zonder_looppunten_landt_midden_in_de_buffer() {
    let bron = stem_op_preload(300_000.0, 400_000, 48_000, 0, 48_000);
    let doel = preload(40_000, 44_100, 0, None);

    let pos = bron.crossfade_position_in(&doel, 48_000);

    assert!((pos - 20_000.0).abs() < 1.0, "verwachtte het midden, kreeg {pos}");
    assert!(pos <= (doel.attack_data.len() - 2) as f64);
}

/// Alle combinaties van bron-/doelrate, trim en lengte: de uitkomst ligt
/// ALTIJD binnen de doelbuffer, en de gemaakte stem klinkt ook echt
/// (leest geldige data i.p.v. stilte).
#[test]
fn crossfade_blijft_bij_elke_ratio_binnen_de_doelbuffer() {
    let rates = [44_100u32, 48_000, 96_000];
    let out_rates = [44_100u32, 48_000];
    for &out in &out_rates {
        for &br in &rates {
            for &dr in &rates {
                for &trim in &[0usize, 5_000] {
                    for &pos in &[0.0f64, 1_000.0, 250_000.0, 5_000_000.0] {
                        let bron = stem_op_preload(pos, 300_000, br, trim, out);
                        let doel = preload(60_000, dr, trim / 2, Some((15_000, 58_000)));
                        let p = bron.crossfade_position_in(&doel, out);
                        assert!(p.is_finite() && p >= 0.0 && p <= 59_998.0,
                                "buiten de doelbuffer: {p} (bron {br} Hz, doel {dr} Hz, uit {out} Hz, pos {pos})");

                        // De stem die hiermee ontstaat moet klinken: na de
                        // infade komt er signaal uit, en de leespositie
                        // blijft binnen de data.
                        let mut nv = PlayingVoice::new_crossfade(
                            doel.clone(), 7, 60 | TREM_FLAG, 60, 1.0, p, out,
                        );
                        let mut laatste = 0.0f32;
                        for _ in 0..2_000 {
                            laatste = nv.next_sample(false).0;
                        }
                        assert!(laatste > 0.0,
                                "crossfade-stem bleef stil (bron {br} Hz, doel {dr} Hz, pos {pos})");
                        assert!(nv.position >= 0.0 && nv.position < 60_000.0,
                                "leespositie liep de buffer uit: {}", nv.position);
                    }
                }
            }
        }
    }
}
