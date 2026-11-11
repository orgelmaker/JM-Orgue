//! Bewijst dat het verdelen van de stemmen over stukken hetzelfde geluid
//! oplevert. Dit is de test die fase 3 (echte werkerthreads) afdekt: die
//! verandert alleen WAAR een stuk gemengd wordt, niet WAT er gebeurt.
use super::*;
use std::sync::Arc;
use vpo_sampler::PreloadBuffer;

/// Een preload-buffer met een herkenbare golfvorm, zodat elke stem iets
/// anders bijdraagt en een verwisseling meteen opvalt.
fn buffer(zaad: u32, lengte: usize) -> Arc<PreloadBuffer> {
    let data: Vec<f32> = (0..lengte)
        .map(|i| ((i as f32 * 0.01 + zaad as f32) .sin()) * 0.5)
        .collect();
    Arc::new(PreloadBuffer {
        attack_data: data,
        attack_right: None,
        sample_rate: 48000,
        total_samples: lengte,
        loop_start: Some(0),
        loop_end: Some(lengte as u64 - 1),
        source_path: std::path::PathBuf::from("test.wav"),
        channels: 1,
        bits_per_sample: 16,
        data_offset: 44,
        align_table: None,
        trim_start: 0,
        smpl_unity_note: None,
        smpl_pitch_fraction_cents: None,            harm_shelf: None,
    })
}

fn stemmen(n: usize) -> Vec<PlayingVoice> {
    (0..n).map(|i| {
        let mut v = PlayingVoice::new_from_preload(
            buffer(i as u32, 4096), i as u32, (i as u32) + 1,
            36 + (i % 48) as u8, 1.0, 48000);
        // De velden die de mengloop leest, gevarieerd invullen.
        v.c_div = i % 4;
        v.c_pitch_mul = 1.0 + (i % 7) as f64 * 0.001;
        v.c_use_lfo_trem = i % 3 == 0;
        v.c_voicing_gain = 0.8 + (i % 5) as f32 * 0.05;
        // Windgevoeligheid per stem: fluit, prestant, strijker, tongwerk.
        v.c_kp = [0.064, 0.046, 0.028, 0.0][i % 4];
        v.c_kg = if i % 4 == 3 { 3.6 } else { 2.0 };
        v.c_dev_a = 0.01 + (i % 3) as f32 * 0.02;
        v
    }).collect()
}

fn blok_context<'a>(
    wind_dev: &'a [[f32; 32]], trem_rate: &'a [[f64; 32]],
    trem_amp: &'a [[f32; 32]],
    pan_cos: &'a [f32; 32], pan_sin: &'a [f32; 32], pans: &'a [f32],
    ccis_en: &'a [bool], bn: usize, ccis: bool,
) -> MengBlok<'a> {
    MengBlok {
        bn, wind_dev, trem_rate, trem_amp,
        pan_cos, pan_sin, div_pans: pans, ccis_on: ccis, ccis_en,
        ccis_strength: 0.7, ccis_falloff: 0.6, ccis_swap: false,
        pitch_offset_mul: 1.0, licht: false,
    }
}

/// Mengt `n_stemmen` in `stukken` stukken en geeft de opgetelde tabellen.
fn meng(n_stemmen: usize, stukken: usize, bn: usize, ccis: bool)
    -> (Vec<[f32; 32]>, Vec<[f32; 32]>)
{
    let wind_dev = vec![[-0.03f32; 32]; bn];
    let trem_rate = vec![[0.999f64; 32]; bn];
    let trem_amp = vec![[1.03f32; 32]; bn];
    let mut pan_cos = [0.0f32; 32];
    let mut pan_sin = [0.0f32; 32];
    for d in 0..32 {
        let angle = (d as f32 / 31.0 + 1.0) * 0.25 * std::f32::consts::PI;
        pan_cos[d] = angle.cos();
        pan_sin[d] = angle.sin();
    }
    let pans: Vec<f32> = (0..32).map(|d| (d as f32 / 31.0) * 2.0 - 1.0).collect();
    let ccis_en: Vec<bool> = (0..32).map(|d| d % 2 == 0).collect();
    let blok = blok_context(&wind_dev, &trem_rate, &trem_amp,
                            &pan_cos, &pan_sin, &pans, &ccis_en, bn, ccis);

    let mut voices = stemmen(n_stemmen);
    let mut hoofd_l = vec![[0.0f32; 32]; bn];
    let mut hoofd_r = vec![[0.0f32; 32]; bn];
    let mut deel_l: Vec<Vec<[f32; 32]>> =
        (1..stukken.max(1)).map(|_| vec![[0.0f32; 32]; bn]).collect();
    let mut deel_r: Vec<Vec<[f32; 32]>> =
        (1..stukken.max(1)).map(|_| vec![[0.0f32; 32]; bn]).collect();

    let per_stuk = voices.len().div_ceil(stukken.max(1)).max(1);
    for (stuk, deel) in voices.chunks_mut(per_stuk).enumerate() {
        let (dl, dr): (&mut [[f32; 32]], &mut [[f32; 32]]) = if stuk == 0 {
            (&mut hoofd_l, &mut hoofd_r)
        } else {
            (&mut deel_l[stuk - 1], &mut deel_r[stuk - 1])
        };
        meng_stemmen_blok(deel, dl, dr, &blok);
    }
    reduceer_deeltabellen(&mut hoofd_l, &mut hoofd_r, &deel_l, &deel_r, stukken, 4, bn);
    (hoofd_l, hoofd_r)
}

// ── Equivalentie snelle mengkern (0.9.32) ────────────────────────────

fn volledig(len: usize, stereo: bool, lus: Option<(u64, u64)>, zaad: f32) -> SampleRef {
    Arc::new(vpo_sampler::SampleData {
        data: (0..len).map(|i| ((i as f32) * 0.017 + zaad).sin() * 0.6).collect(),
        right: if stereo { Some((0..len).map(|i| ((i as f32) * 0.021 + zaad).cos() * 0.5).collect()) } else { None },
        sample_rate: 48000,
        channels: if stereo { 2 } else { 1 },
        loop_start: lus.map(|l| l.0),
        loop_end: lus.map(|l| l.1),
    })
}

fn preload(len: usize, totaal: usize, stereo: bool, lus: (Option<u64>, Option<u64>), zaad: f32) -> Arc<PreloadBuffer> {
    Arc::new(PreloadBuffer {
        attack_data: (0..len).map(|i| ((i as f32) * 0.019 + zaad).sin() * 0.6).collect(),
        attack_right: if stereo { Some((0..len).map(|i| ((i as f32) * 0.023 + zaad).cos() * 0.5).collect()) } else { None },
        sample_rate: 48000, total_samples: totaal, loop_start: lus.0, loop_end: lus.1,
        source_path: std::path::PathBuf::from("eq.wav"), channels: if stereo { 2 } else { 1 },
        bits_per_sample: 24, data_offset: 44, align_table: None, trim_start: 0,
        smpl_unity_note: None, smpl_pitch_fraction_cents: None, harm_shelf: None,
    })
}

/// Alle scenario's van de leesroute, elk als bouwfunctie (de stem wordt
/// twee keer identiek gebouwd: één voor de oude, één voor de nieuwe route).
fn scenario(nr: usize) -> PlayingVoice {
    let mut v = match nr {
        // Volledig stereo, lange naad-crossfade, start vlak voor de naad.
        0 => { let mut v = PlayingVoice::new_from_sample(volledig(20_000, true, Some((6_000, 18_000)), 0.3), 1, 1, 60, 1.0); v.position = 17_200.5; v }
        // Volledig mono zonder looppunten (terugval ¼..einde).
        1 => { let mut v = PlayingVoice::new_from_sample(volledig(9_000, false, None, 1.1), 1, 2, 61, 1.0); v.position = 8_700.25; v }
        // Releasing: uitspelen tot het einde van de opname.
        2 => { let mut v = PlayingVoice::new_from_sample(volledig(6_000, true, Some((1_000, 5_000)), 0.7), 1, 3, 62, 1.0);
               v.position = 5_500.0; v.releasing = true; v.target_envelope = 0.0; v.envelope_speed = 0.0007; v }
        // One-shot (release-staart) die halverwege het blok zijn einde haalt.
        3 => { let mut v = PlayingVoice::new_from_sample(volledig(4_000, true, None, 2.2), 1, 4, 63, 1.0);
               v.one_shot = true; v.position = 3_700.0; v }
        // Preload met volledige loop binnen de buffer.
        4 => { let mut v = PlayingVoice::new_from_preload(preload(12_000, 60_000, true, (Some(3_000), Some(11_000)), 0.4), 1, 5, 64, 1.0, 48000);
               v.position = 10_800.0; v }
        // Preload: loop begint binnen, eindigt erbuiten → startbuffer-tekort.
        5 => { let mut v = PlayingVoice::new_from_preload(preload(10_000, 80_000, false, (Some(2_000), Some(70_000)), 0.9), 1, 6, 65, 1.0, 48000);
               v.position = 9_600.0; v }
        // Preload zonder looppunten (terugval).
        6 => { let mut v = PlayingVoice::new_from_preload(preload(8_000, 50_000, true, (None, None), 1.7), 1, 7, 66, 1.0, 48000);
               v.position = 7_800.0; v }
        // Infaden (crossfade_in) op een looping stem.
        _ => { let mut v = PlayingVoice::new_from_sample(volledig(15_000, true, Some((4_000, 14_000)), 0.1), 1, 8, 67, 1.0);
               crossfade_in(&mut v, 0.8, 30.0, 48000); v.position = 13_500.0; v }
    };
    v.rate = 1.0;
    v
}

#[test]
fn snelle_leesroute_is_bitidentiek() {
    for nr in 0..8 {
        for licht in [false, true] {
            let mut oud = scenario(nr);
            let mut nieuw = scenario(nr);
            let geom = nieuw.lees_geom().expect("dit scenario hoort de snelle route te nemen");
            let bn = 256usize;
            for blok in 0..3 {
                let rates: Vec<f64> = (0..bn).map(|f| 1.0 + 0.013 * (((blok * bn + f) as f64) * 0.07).sin()).collect();
                let mut ul = vec![0.0f32; bn];
                let mut ur = vec![0.0f32; bn];
                nieuw.lees_blok(&geom, &rates, &mut ul, &mut ur, licht);
                for f in 0..bn {
                    let basis = oud.rate;
                    oud.rate = rates[f];
                    let (a, b) = oud.next_sample(licht);
                    oud.rate = basis;
                    assert_eq!((a.to_bits(), b.to_bits()), (ul[f].to_bits(), ur[f].to_bits()),
                        "scenario {} licht {} blok {} frame {}: oud ({}, {}) nieuw ({}, {})", nr, licht, blok, f, a, b, ul[f], ur[f]);
                }
            }
            assert_eq!(oud.position.to_bits(), nieuw.position.to_bits(), "positie, scenario {}", nr);
            assert_eq!(oud.envelope.to_bits(), nieuw.envelope.to_bits(), "envelope, scenario {}", nr);
            assert_eq!(oud.target_envelope.to_bits(), nieuw.target_envelope.to_bits(), "doel, scenario {}", nr);
            assert_eq!(oud.releasing, nieuw.releasing, "releasing, scenario {}", nr);
            assert_eq!(oud.preload_wrapped, nieuw.preload_wrapped, "preload_wrapped, scenario {}", nr);
            assert_eq!((oud.hist_last.to_bits(), oud.hist_prev.to_bits()), (nieuw.hist_last.to_bits(), nieuw.hist_prev.to_bits()), "historie, scenario {}", nr);
            assert_eq!(oud.age_samples, nieuw.age_samples, "leeftijd, scenario {}", nr);
        }
    }
}

/// De hele mengkern: nieuw (blok + geometrie) tegen oud (per sample), met
/// wind, tremulant, C/Cis-spreiding, gemengde bronnen en licht/cubisch.
#[test]
fn mengkern_nieuw_gelijk_aan_oud() {
    let bn = 300usize; // > MAX_BLOK_FRAMES: ook het opknippen wordt getest
    let wind_dev: Vec<[f32; 32]> = (0..bn).map(|f| [((f as f32) * 0.05).sin() * 0.04; 32]).collect();
    let trem_rate: Vec<[f64; 32]> = (0..bn).map(|f| [1.0 + ((f as f64) * 0.11).sin() * 0.004; 32]).collect();
    let trem_amp: Vec<[f32; 32]> = (0..bn).map(|f| [1.0 + ((f as f32) * 0.09).cos() * 0.05; 32]).collect();
    let mut pan_cos = [0.0f32; 32];
    let mut pan_sin = [0.0f32; 32];
    for d in 0..32 {
        let angle = (d as f32 / 31.0 + 1.0) * 0.25 * std::f32::consts::PI;
        pan_cos[d] = angle.cos();
        pan_sin[d] = angle.sin();
    }
    let pans: Vec<f32> = (0..32).map(|d| (d as f32 / 31.0) * 2.0 - 1.0).collect();
    let ccis_en: Vec<bool> = (0..32).map(|d| d % 2 == 0).collect();
    for licht in [false, true] {
        let mut blok = blok_context(&wind_dev, &trem_rate, &trem_amp, &pan_cos, &pan_sin, &pans, &ccis_en, bn, true);
        blok.licht = licht;
        let bouw = || -> Vec<PlayingVoice> {
            let mut vs: Vec<PlayingVoice> = stemmen(120);
            for nr in 0..8 { vs.push(scenario(nr)); }
            // Eén stem met een uitgestelde decay: die gaat via de oude route.
            let mut uitgesteld = scenario(0);
            crossfade_in_with_decay(&mut uitgesteld, 1.0, 20.0, 800.0, 48000);
            vs.push(uitgesteld);
            for (i, v) in vs.iter_mut().enumerate() {
                v.c_div = i % 4;
                v.c_kp = [0.064, 0.046, 0.028, 0.0][i % 4];
                v.c_kg = if i % 4 == 3 { 3.6 } else { 2.0 };
                v.c_dev_a = 0.01 + (i % 3) as f32 * 0.02;
                v.c_use_lfo_trem = i % 3 == 0;
                v.c_voicing_gain = 0.8 + (i % 5) as f32 * 0.05;
                v.c_pitch_mul = 1.0 + (i % 7) as f64 * 0.001;
            }
            vs
        };
        let mut oud = bouw();
        let mut nieuw = bouw();
        let (mut ol, mut or) = (vec![[0.0f32; 32]; bn], vec![[0.0f32; 32]; bn]);
        let (mut nl, mut nr) = (vec![[0.0f32; 32]; bn], vec![[0.0f32; 32]; bn]);
        for _ in 0..3 {
            meng_stemmen_blok_oud(&mut oud, &mut ol, &mut or, &blok);
            meng_stemmen_blok(&mut nieuw, &mut nl, &mut nr, &blok);
        }
        for f in 0..bn {
            for d in 0..32 {
                assert_eq!(ol[f][d].to_bits(), nl[f][d].to_bits(), "L frame {} bus {} licht {}", f, d, licht);
                assert_eq!(or[f][d].to_bits(), nr[f][d].to_bits(), "R frame {} bus {} licht {}", f, d, licht);
            }
        }
        for (a, b) in oud.iter().zip(nieuw.iter()) {
            assert_eq!(a.position.to_bits(), b.position.to_bits());
            assert_eq!(a.envelope.to_bits(), b.envelope.to_bits());
            assert_eq!(a.c_dev_cur.to_bits(), b.c_dev_cur.to_bits());
            assert_eq!(a.rate.to_bits(), b.rate.to_bits());
        }
    }
}

/// Benchmark (handmatig, release): kosten van de mengkern per stem per
/// sample bij 1000 stemmen met elk een eigen stereo-buffer, tegenover het
/// kale minimum (alleen de stereo-interpolatie). `cargo test --release -p
/// vpo-app --bins bench_mengkern -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_mengkern() {
    let n = 1000usize;
    let len = 48_000usize;
    let maak = |zaad: usize| -> Arc<PreloadBuffer> {
        let l: Vec<f32> = (0..len).map(|i| ((i as f32 * 0.013 + zaad as f32).sin()) * 0.5).collect();
        let r: Vec<f32> = (0..len).map(|i| ((i as f32 * 0.011 + zaad as f32).cos()) * 0.5).collect();
        Arc::new(PreloadBuffer {
            attack_data: l, attack_right: Some(r), sample_rate: 48000, total_samples: len,
            loop_start: Some(12_000), loop_end: Some(44_000),
            source_path: std::path::PathBuf::from("bench.wav"), channels: 2, bits_per_sample: 24,
            data_offset: 44, align_table: None, trim_start: 0, smpl_unity_note: None,
            smpl_pitch_fraction_cents: None,                harm_shelf: None,
        })
    };
    let bufs: Vec<_> = (0..n).map(maak).collect();
    let bouw = |bufs: &Vec<Arc<PreloadBuffer>>| -> Vec<PlayingVoice> { (0..n).map(|i| {
        let mut v = PlayingVoice::new_from_preload(bufs[i].clone(), i as u32, i as u32 + 1, 36 + (i % 48) as u8, 1.0, 48000);
        v.c_div = i % 4;
        v.c_pitch_mul = 1.0 + (i % 7) as f64 * 0.0013;
        v.c_use_lfo_trem = i % 3 == 0;
        v.c_voicing_gain = 0.8;
        v.c_kp = [0.064, 0.046, 0.028, 0.0][i % 4];
        v.c_kg = 2.0;
        v.c_dev_a = 0.02;
        v.position = (i * 37 % 20_000) as f64;
        v
    }).collect() };
    let mut voices = bouw(&bufs);
    let mut voices_oud = bouw(&bufs);
    let bn = 480usize;
    let wind_dev = vec![[-0.01f32; 32]; bn];
    let trem_rate = vec![[1.0f64; 32]; bn];
    let trem_amp = vec![[1.0f32; 32]; bn];
    let pan_cos = [std::f32::consts::FRAC_1_SQRT_2; 32];
    let pan_sin = [std::f32::consts::FRAC_1_SQRT_2; 32];
    let pans = vec![0.0f32; 32];
    let ccis_en = vec![false; 32];
    let blok = blok_context(&wind_dev, &trem_rate, &trem_amp, &pan_cos, &pan_sin, &pans, &ccis_en, bn, false);
    let mut dl = vec![[0.0f32; 32]; bn];
    let mut dr = vec![[0.0f32; 32]; bn];
    let rondes = 40;
    // Opwarmen.
    meng_stemmen_blok(&mut voices, &mut dl, &mut dr, &blok);
    let t = std::time::Instant::now();
    for _ in 0..rondes { meng_stemmen_blok(&mut voices, &mut dl, &mut dr, &blok); }
    let ns_meng = t.elapsed().as_nanos() as f64 / (rondes * n * bn) as f64;

    // Oude kern (per sample next_sample) ter vergelijking.
    meng_stemmen_blok_oud(&mut voices_oud, &mut dl, &mut dr, &blok);
    let t = std::time::Instant::now();
    for _ in 0..rondes { meng_stemmen_blok_oud(&mut voices_oud, &mut dl, &mut dr, &blok); }
    let ns_oud = t.elapsed().as_nanos() as f64 / (rondes * n * bn) as f64;
    println!("OUDE kern: {:.1} ns per stem-sample  →  nieuwe kern {:.2}x zo snel", ns_oud, ns_oud / ns_meng);

    // Kaal minimum: alleen stereo-hermite op dezelfde buffers.
    let mut pos: Vec<f64> = (0..n).map(|i| (i * 37 % 20_000) as f64).collect();
    let mut som = 0.0f32;
    let t = std::time::Instant::now();
    for _ in 0..rondes {
        for (i, b) in bufs.iter().enumerate() {
            let r = b.attack_right.as_ref().unwrap();
            let mut p = pos[i];
            for _ in 0..bn {
                let k = p as usize;
                let fr = (p - k as f64) as f32;
                som += hermite4(&b.attack_data, k, fr) + hermite4(r, k, fr);
                p += 1.0013;
                if p >= 44_000.0 { p -= 32_000.0; }
            }
            pos[i] = p;
        }
    }
    let ns_kaal = t.elapsed().as_nanos() as f64 / (rondes * n * bn) as f64;
    println!("mengkern: {:.1} ns per stem-sample | kaal minimum (stereo-hermite): {:.1} ns | overhead {:.1}x  (som {})",
        ns_meng, ns_kaal, ns_meng / ns_kaal, som);
    println!("→ 1000 stemmen × 480 frames op 1 kern: {:.1} ms per blok (buffer = 10 ms)", ns_meng * (n * bn) as f64 / 1e6);
}

/// Grootste absolute verschil tussen twee tabellen, en het grootste
/// voorkomende niveau (om het verschil relatief te kunnen wegen).
fn verschil(a: &[[f32; 32]], b: &[[f32; 32]]) -> (f32, f32) {
    let mut max_d = 0.0f32;
    let mut max_v = 0.0f32;
    for (ra, rb) in a.iter().zip(b.iter()) {
        for d in 0..32 {
            max_d = max_d.max((ra[d] - rb[d]).abs());
            max_v = max_v.max(ra[d].abs());
        }
    }
    (max_d, max_v)
}

#[test]
fn verdeeld_mengen_geeft_hetzelfde_geluid() {
    // De kern van fase 3: 200 stemmen, in 1 / 2 / 4 / 8 stukken. Het
    // verschil mag alleen de afrondingsfout van een andere optelvolgorde
    // zijn -- ruim onder een miljoenste van het niveau.
    let (ref_l, ref_r) = meng(200, 1, 64, true);
    for stukken in [2usize, 3, 4, 8] {
        let (l, r) = meng(200, stukken, 64, true);
        let (dl, niveau) = verschil(&ref_l, &l);
        let (dr, _) = verschil(&ref_r, &r);
        assert!(niveau > 0.05, "de test zelf produceert te weinig geluid ({})", niveau);
        let grens = niveau * 1e-5;
        assert!(dl <= grens, "{} stukken: links {} verschil op niveau {}", stukken, dl, niveau);
        assert!(dr <= grens, "{} stukken: rechts {} verschil op niveau {}", stukken, dr, niveau);
    }
}

/// Een losgelaten pijp en zijn staart houden de afwijking van het moment
/// van loslaten vast; een klinkende stem volgt de wind wel.
#[test]
fn een_staart_houdt_de_toonhoogte_van_het_moment_van_loslaten() {
    let bn = 64;
    let mut voices = stemmen(1);
    voices[0].c_dev_a = 0.05;
    voices[0].c_dev_cur = -0.02;
    let mut staart = PlayingVoice::new_from_preload(buffer(9, 4096), 1, 2, 60, 1.0, 48000);
    staart.erf_wind(&voices[0], true);
    staart.one_shot = true;
    assert_eq!(staart.c_dev_cur, -0.02);
    assert_eq!(staart.c_dev_a, 0.0);
    assert_eq!(staart.c_wind_flow, 0.0);
    voices.push(staart);
    let wind_dev = vec![[0.03f32; 32]; bn];
    let trem_rate = vec![[1.0f64; 32]; bn];
    let trem_amp = vec![[1.0f32; 32]; bn];
    let pan_cos = [0.7f32; 32];
    let pan_sin = [0.7f32; 32];
    let pans = vec![0.0f32; 32];
    let ccis_en = vec![false; 32];
    let blok = blok_context(&wind_dev, &trem_rate, &trem_amp, &pan_cos, &pan_sin, &pans, &ccis_en, bn, false);
    let mut l = vec![[0.0f32; 32]; bn];
    let mut r = vec![[0.0f32; 32]; bn];
    meng_stemmen_blok(&mut voices, &mut l, &mut r, &blok);
    assert!(voices[0].c_dev_cur > -0.02, "de klinkende stem hoort de wind te volgen: {}", voices[0].c_dev_cur);
    assert_eq!(voices[1].c_dev_cur, -0.02, "de staart hoort bevroren te blijven");
}

#[test]
fn ook_zonder_ccis_spreiding() {
    let (ref_l, _) = meng(120, 1, 32, false);
    let (l, _) = meng(120, 4, 32, false);
    let (d, niveau) = verschil(&ref_l, &l);
    assert!(d <= niveau * 1e-5, "verschil {} op niveau {}", d, niveau);
}

#[test]
fn meer_stukken_dan_stemmen_gaat_goed() {
    // chunks_mut mag nooit meer stukken opleveren dan er deeltabellen zijn.
    let (ref_l, _) = meng(3, 1, 16, true);
    let (l, _) = meng(3, 8, 16, true);
    let (d, niveau) = verschil(&ref_l, &l);
    assert!(niveau > 0.0);
    assert!(d <= niveau * 1e-5, "verschil {} op niveau {}", d, niveau);
}

#[test]
fn geen_enkele_stem_valt_bij_een_stukgrens_weg() {
    // Één stem minder MOET een ander resultaat geven. Zou een stem op een
    // stukgrens stilletjes wegvallen, dan zou het verschil nul zijn.
    // (Luider worden is geen goede maatstaf: stemmen met verschillende
    // fase kunnen elkaar ook uitdoven.)
    let (met, _) = meng(200, 4, 32, false);
    let (zonder, _) = meng(199, 4, 32, false);
    let (d, niveau) = verschil(&met, &zonder);
    assert!(d > niveau * 1e-4,
        "199 en 200 stemmen geven bijna hetzelfde ({} op niveau {}) — er valt er een weg",
        d, niveau);
}
