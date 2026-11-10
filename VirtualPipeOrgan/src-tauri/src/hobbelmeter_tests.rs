//! Hobbelmeter voor het loslaten (taak F4), op een gesynthetiseerde toon.
//!
//! Wat hier gemeten wordt, met de echte onderdelen van de engine (dezelfde
//! stemmen, dezelfde `crossfade_in`, dezelfde `ReleaseAlignTable`, dezelfde
//! `release_fade_ms`), maar zonder bestanden of audioapparaat:
//!
//! - **de hobbel**: een aangehouden toon gaat bij het loslaten over in zijn
//!   release-sample. Is de startpositie van de release goed gekozen, dan telt
//!   het uitfadende deel en het infadende deel coherent op en blijft het niveau
//!   vlak. Is ze fout, dan telt het deels uit fase op en zakt het niveau in het
//!   midden van de crossfade. De meter loopt over honderden verschillende
//!   loslaatmomenten (dus verschillende fasen van de golf) en meldt de grootste
//!   dip en de grootste bult ten opzichte van het ideale, coherente verloop;
//! - het startpunt van de release (sluit de eerste release-sample aan op de
//!   klinkende golf?);
//! - de GrandOrgue-technieken: een zachtere release na loslaten tijdens de
//!   aanzet, en de staart die na een staccatonoot uitfadet;
//! - "oneffenheid": met het verschil per pijp op 0 klinken twee pijpen met
//!   verschillende identiteit bit-identiek.
//!
//! Een meetproef met een echte set komt in C3; dit is de goedkope eerste stap
//! die de fase-uitlijning tegen terugval beschermt. Elke meting heeft een
//! negatieve controle: zonder uitlijning moet de meter wél een dip zien.

use super::*;
use vpo_audio::{PijpFamilie, WIND_RATE_PER_EENHEID};
use vpo_sampler::ReleaseAlignTable;

const SR: u32 = 48_000;
/// Uitklinktijd van de gesynthetiseerde release (seconden).
const TAU_S: f64 = 0.8;

fn frequentie(note: u8) -> f64 {
    440.0 * 2f64.powf((note as f64 - 69.0) / 12.0)
}

/// Pijpachtige toon: grondtoon plus drie boventonen met vaste fasen, zodat de
/// golfvorm niet symmetrisch is en de toestand (waarde, helling) de fase
/// eenduidig vastlegt, zoals bij een echte pijp.
fn toon(f0: f64, frame: f64) -> f32 {
    let w = 2.0 * std::f64::consts::PI * f0 * frame / SR as f64;
    (0.4 * ((w + 0.3).sin()
        + 0.5 * (2.0 * w + 1.1).sin()
        + 0.3 * (3.0 * w + 2.0).sin()
        + 0.2 * (4.0 * w + 0.7).sin())) as f32
}

fn sample_van(data: Vec<f32>) -> SampleRef {
    Arc::new(SampleData {
        data,
        right: None,
        sample_rate: SR,
        channels: 1,
        loop_start: None,
        loop_end: None,
    })
}

/// De aangehouden toon (constante sterkte), lang genoeg om niet te loopen.
fn aangehouden(f0: f64, frames: usize) -> SampleRef {
    sample_van((0..frames).map(|i| toon(f0, i as f64)).collect())
}

/// De release-opname: dezelfde toon vanaf fase 0, exponentieel uitklinkend.
fn uitklinkend(f0: f64, frames: usize) -> SampleRef {
    sample_van(
        (0..frames)
            .map(|i| toon(f0, i as f64) * (-(i as f64) / (TAU_S * SR as f64)).exp() as f32)
            .collect(),
    )
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

/// Eenvoudige, deterministische getallenreeks: dezelfde loslaatmomenten bij
/// elke run, zonder een extra afhankelijkheid.
struct Reeks(u64);
impl Reeks {
    fn volgende(&mut self, ondergrens: u64, bovengrens: u64) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ondergrens + (self.0 >> 33) % (bovengrens - ondergrens)
    }
}

#[derive(Debug, Clone, Copy)]
struct Meting {
    /// Grootste zakking van het niveau onder het ideale verloop (dB, ≥ 0).
    dip_db: f64,
    /// Mediaan en 95e percentiel over de loslaatmomenten van de zakking per
    /// moment: een terugval in de uitlijning schuift deze op, ook als de
    /// slechtste uitschieter gelijk blijft.
    dip_mediaan_db: f64,
    dip_p95_db: f64,
    /// Grootste overschrijding van het ideale verloop (dB, ≥ 0).
    bult_db: f64,
    /// Grootste verschil tussen de eerste release-sample en de volgende sample
    /// van de klinkende golf, als deel van de piek van de toon.
    sprong: f64,
    /// Aantal loslaatmomenten en vensters waarover is gemeten.
    momenten: usize,
    vensters: usize,
}

/// Laat de aangehouden toon `momenten` keer op een ander moment los en meet de
/// overgang. Het ideale verloop is de toon die coherent van niveau 1 naar het
/// niveau van de release-opname gaat; het venster is één periode, zodat de
/// RMS niet van de fase afhangt.
fn meet_overgang(note: u8, uitlijnen: bool, momenten: usize) -> Meting {
    let f0 = frequentie(note);
    let periode = ((SR as f64 / f0).round() as usize).max(2);
    let fade_ms = release_fade_ms(note);
    // Zelfde frameaantal als `crossfade_in` en `release_ms`.
    let frames = ((fade_ms.max(1.0) * 0.001 * SR as f32).max(8.0)) as usize;
    let aangehouden = aangehouden(f0, 5 * SR as usize);
    let release = uitklinkend(f0, 8 * SR as usize);
    let tabel = ReleaseAlignTable::compute(&release.data, SR).expect("de tabel hoort zich te laten bouwen");
    let s_aangehouden = rms(&aangehouden.data[..SR as usize]);
    let piek = aangehouden.data.iter().fold(0.0f32, |m, v| m.max(v.abs())) as f64;

    let mut reeks = Reeks(0x2545_F491_4F6C_DD1D ^ note as u64);
    let mut m = Meting {
        dip_db: 0.0, dip_mediaan_db: 0.0, dip_p95_db: 0.0, bult_db: 0.0, sprong: 0.0, momenten, vensters: 0,
    };
    let mut dip_per_moment: Vec<f64> = Vec::with_capacity(momenten);
    for _ in 0..momenten {
        let n = reeks.volgende(12_000, 72_000);
        let mut speel = PlayingVoice::new_from_sample(aangehouden.clone(), 1, 1, note, 1.0);
        speel.rate = 1.0;
        speel.envelope = 1.0;
        speel.target_envelope = 1.0;
        speel.position = n as f64;
        // Twee samples spelen: de stem houdt daarbij zelf zijn historie bij,
        // precies wat de callback bij een NoteOff uitleest.
        speel.next_sample(false);
        speel.next_sample(false);
        let start = if uitlijnen {
            tabel.position_for(speel.hist_last, speel.hist_prev) as u64
        } else {
            0
        };

        let volgende = toon(f0, (n + 2) as f64);
        m.sprong = m.sprong.max(((release.data[start as usize] - volgende).abs() as f64) / piek);

        let mut rel = PlayingVoice::new_from_sample(release.clone(), 1, 1 | RELEASE_PIPE_FLAG, note, 1.0);
        rel.position = start as f64;
        rel.one_shot = true;
        crossfade_in(&mut rel, 1.0, fade_ms, SR);
        speel.release_ms(fade_ms, SR);

        let mut y = vec![0.0f32; frames];
        for v in y.iter_mut() {
            *v = speel.next_sample(false).0 + rel.next_sample(false).0;
        }
        // Venstergemiddelde van de kwadraten via een lopende som.
        let mut som = vec![0.0f64; frames + 1];
        for (i, v) in y.iter().enumerate() {
            som[i + 1] = som[i] + (*v as f64) * (*v as f64);
        }
        let stap = (periode / 4).max(1);
        let mut dip_dit_moment = 0.0f64;
        let mut midden = periode;
        while midden + periode <= frames {
            let (a, b) = (midden - periode / 2, midden - periode / 2 + periode);
            let rms_y = ((som[b] - som[a]) / periode as f64).sqrt();
            // Ideaal: uitfadende speelnoot (niveau 1 - x) plus infadende
            // release (x, met zijn eigen uitklinking), coherent opgeteld.
            let x = (midden as f64 + 1.0) / frames as f64;
            let rho = (-((start as f64 + midden as f64) / (TAU_S * SR as f64))).exp();
            let ideaal = s_aangehouden * ((1.0 - x) + x * rho);
            let db = 20.0 * (rms_y / ideaal).log10();
            dip_dit_moment = dip_dit_moment.max(-db);
            m.bult_db = m.bult_db.max(db);
            m.vensters += 1;
            midden += stap;
        }
        m.dip_db = m.dip_db.max(dip_dit_moment);
        dip_per_moment.push(dip_dit_moment);
    }
    dip_per_moment.sort_by(|a, b| a.partial_cmp(b).unwrap());
    m.dip_mediaan_db = dip_per_moment[dip_per_moment.len() / 2];
    m.dip_p95_db = dip_per_moment[(dip_per_moment.len() * 95 / 100).min(dip_per_moment.len() - 1)];
    m
}

const NOTEN: [u8; 6] = [36, 48, 60, 72, 84, 96];

const MOMENTEN: usize = 120;

/// De hobbelmeter zelf. Gemeten basislijn (120 loslaatmomenten per noot, noten
/// 36 t/m 96): met de uitlijning is de mediaan van de grootste zakking per
/// moment 0,01-0,04 dB (de overgang is dan vlak), maar een staart van momenten
/// haalt tot ~3,2 dB: de tabel heeft 16 x 16 vakjes, dus een deel van de fasen
/// landt tot een kwart periode ernaast. De drempels staan ruim boven die
/// basislijn en ruim onder het gedrag zonder uitlijning (zie de controle
/// hieronder); ze vangen een terugval, ze beloven geen betere uitlijning.
#[test]
fn hobbel_blijft_klein_bij_elk_loslaatmoment() {
    for &note in &NOTEN {
        let m = meet_overgang(note, true, MOMENTEN);
        eprintln!(
            "noot {note}: dip max {:.2} p95 {:.2} mediaan {:.2} dB, bult {:.2} dB ({} momenten, {} vensters)",
            m.dip_db, m.dip_p95_db, m.dip_mediaan_db, m.bult_db, m.momenten, m.vensters
        );
        assert!(m.dip_mediaan_db < 0.2, "noot {note}: mediaan van de dip {:.2} dB, de overgang is niet meer vlak", m.dip_mediaan_db);
        assert!(m.dip_p95_db < 3.2, "noot {note}: p95 van de dip {:.2} dB", m.dip_p95_db);
        assert!(m.dip_db < 3.8, "noot {note}: grootste dip {:.2} dB", m.dip_db);
        assert!(m.bult_db < 0.3, "noot {note}: bult van {:.2} dB boven het coherente verloop", m.bult_db);
    }
}

/// Negatieve controle: start de release op frame 0 (zoals vóór de uitlijning)
/// en de meter MOET de dip zien, anders meet hij niets.
#[test]
fn hobbelmeter_ziet_een_release_zonder_uitlijning() {
    for &note in &NOTEN {
        let m = meet_overgang(note, false, MOMENTEN);
        eprintln!(
            "noot {note} zonder uitlijning: dip max {:.2} p95 {:.2} mediaan {:.2} dB",
            m.dip_db, m.dip_p95_db, m.dip_mediaan_db
        );
        assert!(m.dip_mediaan_db > 2.5, "noot {note}: de meter ziet de mediane dip niet ({:.2} dB)", m.dip_mediaan_db);
        assert!(m.dip_db > 5.5, "noot {note}: de meter ziet de grootste dip niet ({:.2} dB)", m.dip_db);
    }
}

/// Het startpunt: de eerste sample van de release sluit aan op de volgende
/// sample van de klinkende golf. Op frame 0 kan het verschil bijna de hele
/// piek zijn (een tik); met de tabel blijft het een klein deel ervan.
#[test]
fn startpunt_van_de_release_sluit_aan_op_de_golf() {
    for &note in &NOTEN {
        let met = meet_overgang(note, true, MOMENTEN);
        let zonder = meet_overgang(note, false, MOMENTEN);
        eprintln!("noot {note}: sprong met {:.3}, zonder {:.3} (deel van de piek)", met.sprong, zonder.sprong);
        assert!(met.sprong < 0.5, "noot {note}: de release start {:.0} % van de piek ernaast", met.sprong * 100.0);
        assert!(zonder.sprong > 1.0, "noot {note}: de controle zonder uitlijning laat geen sprong zien ({:.3})", zonder.sprong);
    }
}

/// Zonder bruikbare data bouwt de tabel zich niet (de release start dan op
/// frame 0, zoals de callback het doet bij `None`).
#[test]
fn uitlijntabel_bouwt_zich_niet_op_te_korte_of_stille_data() {
    assert!(ReleaseAlignTable::compute(&vec![0.5f32; 100], SR).is_none(), "te kort");
    assert!(ReleaseAlignTable::compute(&vec![0.0f32; 5_000], SR).is_none(), "stil");
    let f0 = frequentie(60);
    let data: Vec<f32> = (0..5_000).map(|i| toon(f0, i as f64)).collect();
    assert!(ReleaseAlignTable::compute(&data, SR).is_some(), "een gewone toon hoort een tabel te geven");
}

// ── GrandOrgue-technieken: geschaalde release ────────────────────────────

/// Aanslagduur-schatting zoals `scaled_release_params` ze hanteert, voor de
/// verwachtingen hieronder: 500 ms onder MIDI 24, 50 ms vanaf 96, lineair
/// ertussen.
fn aanslag_ms(note: u8) -> f32 {
    if note >= 96 { 50.0 } else if note < 24 { 500.0 } else { 500.0 + (24.0 - note as f32) * 6.25 }
}

/// Een noot die tijdens de aanzet wordt losgelaten krijgt een zachtere release
/// (0,2 tot 1,0), die met de nootduur oploopt en vanaf het einde van de aanzet
/// vol is; een lage pijp heeft een langere aanzet dan een hoge.
#[test]
fn release_na_loslaten_in_de_aanzet_is_zachter() {
    for &note in &[12u8, 24, 36, 60, 84, 96, 108] {
        let aanslag = aanslag_ms(note);
        assert_eq!(scaled_release_params(0.0, note, 3.0).0, 0.2, "noot {note}: loslaten op t=0");
        let mut vorige = 0.0f32;
        let mut t = 0.0f32;
        while t <= aanslag + 100.0 {
            let (g, _) = scaled_release_params(t, note, 3.0);
            assert!((0.2..=1.0).contains(&g), "noot {note}, {t} ms: gain {g} buiten 0,2..1,0");
            assert!(g + 1e-6 >= vorige, "noot {note}, {t} ms: gain daalt ({vorige} -> {g})");
            vorige = g;
            t += 1.0;
        }
        assert_eq!(scaled_release_params(aanslag, note, 3.0).0, 1.0, "noot {note}: vol vanaf het einde van de aanzet");
        assert_eq!(scaled_release_params(aanslag + 500.0, note, 3.0).0, 1.0, "noot {note}: lang aangehouden");
    }
    // Halverwege de aanzet: 0,2 + 0,8 * (2 * 0,5 - 0,25) = 0,8.
    let halverwege = aanslag_ms(60) * 0.5;
    assert!((scaled_release_params(halverwege, 60, 3.0).0 - 0.8).abs() < 1e-5);
    // Dezelfde 50 ms: een hoge pijp is al uitgesproken, een lage nog niet.
    let (laag, midden, hoog) = (
        scaled_release_params(50.0, 20, 3.0).0,
        scaled_release_params(50.0, 60, 3.0).0,
        scaled_release_params(50.0, 100, 3.0).0,
    );
    assert!(laag < midden && midden < hoog, "gain per toonhoogte: {laag} {midden} {hoog}");
    // Onbekende noot (0, of boven 133) telt als een middenpijp (60).
    assert_eq!(scaled_release_params(100.0, 0, 3.0), scaled_release_params(100.0, 60, 3.0));
    assert_eq!(scaled_release_params(100.0, 200, 3.0), scaled_release_params(100.0, 60, 3.0));
}

/// Een korte noot krijgt een actief afgebouwde staart, evenredig met de
/// nootduur; een lange noot houdt de natuurlijke release (geen decay).
#[test]
fn korte_noot_krijgt_een_afbouwende_staart_en_lange_noot_niet() {
    for &(rel_secs, ttfr) in &[(0.5f32, 100.0f32), (2.0, 160.0), (3.0, 220.0), (10.0, 350.0)] {
        let mut vorige = 0.0f32;
        let mut t = 0.0f32;
        while t < ttfr {
            let (_, d) = scaled_release_params(t, 60, rel_secs);
            let d = d.unwrap_or_else(|| panic!("release {rel_secs} s, {t} ms: geen decay terwijl de noot korter is dan {ttfr} ms"));
            assert!(d >= ttfr - 1e-3 && d <= ttfr + 6000.0 + 1e-3, "decay {d} ms buiten {ttfr}..{}", ttfr + 6000.0);
            assert!(d + 1e-3 >= vorige, "decay daalt bij een langere noot ({vorige} -> {d})");
            vorige = d;
            t += 1.0;
        }
        assert!(vorige > ttfr + 5000.0, "vlak onder de grens hoort de decay bijna maximaal te zijn: {vorige}");
        assert_eq!(scaled_release_params(ttfr, 60, rel_secs).1, None, "release {rel_secs} s: op de grens is de noot niet meer kort");
        assert_eq!(scaled_release_params(ttfr + 400.0, 60, rel_secs).1, None);
    }
}

/// Venstergemiddelde van de RMS (één periode per venster, halve stap).
fn rms_vensters(y: &[f32], periode: usize) -> Vec<f64> {
    let mut som = vec![0.0f64; y.len() + 1];
    for (i, v) in y.iter().enumerate() {
        som[i + 1] = som[i] + (*v as f64) * (*v as f64);
    }
    let stap = (periode / 2).max(1);
    let mut uit = Vec::new();
    let mut a = 0;
    while a + periode <= y.len() {
        uit.push(((som[a + periode] - som[a]) / periode as f64).sqrt());
        a += stap;
    }
    uit
}

/// Na een staccatonoot dooft de staart uit zonder bult: nergens luider dan de
/// aangehouden toon, na de crossfade nergens weer luider, en aan het eind
/// stil. Meting op de gesynthetiseerde toon met de echte uitlijning, infade
/// en decay.
#[test]
fn staart_na_een_staccatonoot_dooft_uit_zonder_bult() {
    for &note in &[36u8, 60, 84] {
        let f0 = frequentie(note);
        let periode = ((SR as f64 / f0).round() as usize).max(2);
        let fade_ms = release_fade_ms(note);
        let fade_frames = ((fade_ms.max(1.0) * 0.001 * SR as f32).max(8.0)) as usize;
        let aangehouden = aangehouden(f0, 5 * SR as usize);
        let release = uitklinkend(f0, 8 * SR as usize);
        let rel_secs = release.data.len() as f32 / SR as f32;
        let tabel = ReleaseAlignTable::compute(&release.data, SR).unwrap();
        let s = rms(&aangehouden.data[..SR as usize]);

        for &note_ms in &[10.0f32, 40.0, 100.0, 200.0, 340.0] {
            let (gain, decay) = scaled_release_params(note_ms, note, rel_secs);
            let decay_ms = decay.expect("een korte noot hoort een decay te krijgen");
            let n = (note_ms * 0.001 * SR as f32) as u64;
            let mut speel = PlayingVoice::new_from_sample(aangehouden.clone(), 1, 1, note, 1.0);
            speel.rate = 1.0;
            speel.envelope = 1.0;
            speel.target_envelope = 1.0;
            speel.position = n as f64;
            speel.age_samples = n;
            speel.next_sample(false);
            speel.next_sample(false);
            let start = tabel.position_for(speel.hist_last, speel.hist_prev) as f64;
            let niveau = (speel.envelope * gain).clamp(0.05, 1.0);
            let mut rel = PlayingVoice::new_from_sample(release.clone(), 1, 1 | RELEASE_PIPE_FLAG, note, 1.0);
            rel.position = start;
            rel.one_shot = true;
            crossfade_in_with_decay(&mut rel, niveau, fade_ms, decay_ms, SR);
            speel.release_ms(fade_ms, SR);

            let max_frames = (decay_ms * 0.001 * SR as f32) as usize + 2 * fade_frames + SR as usize;
            let mut y: Vec<f32> = Vec::new();
            for _ in 0..max_frames {
                y.push(speel.next_sample(false).0 + rel.next_sample(false).0);
                if rel.is_finished() && speel.is_finished() { break; }
            }
            let ctx = format!("noot {note}, {note_ms} ms, gain {gain:.2}, decay {decay_ms:.0} ms");
            assert!(rel.is_finished(), "{ctx}: de staart dooft niet uit (envelope {:.5})", rel.envelope);
            let v = rms_vensters(&y, periode);
            let piek = v.iter().cloned().fold(0.0f64, f64::max);
            assert!(piek <= s * 1.03, "{ctx}: bult, piek {:.3} tegen aangehouden {:.3}", piek, s);
            // Na de crossfade (en een venster erbij) mag het niveau niet meer stijgen.
            let na = (fade_frames + periode) / (periode / 2).max(1) + 1;
            for w in v[na.min(v.len().saturating_sub(1))..].windows(2) {
                assert!(w[1] <= w[0] * 1.05 + 1e-6, "{ctx}: het niveau stijgt weer na de crossfade ({:.4} -> {:.4})", w[0], w[1]);
            }
            let slot = v.last().copied().unwrap_or(0.0);
            assert!(slot < s * 1e-2, "{ctx}: de staart is aan het eind niet stil ({:.5})", slot);
        }
    }
}

// ── Oneffenheid: het verschil per pijp ───────────────────────────────────

/// Mengt één stem door de echte mengkern met een rimpelende winddruk en
/// geeft links en rechts terug.
fn meng_een_stem(pw: PijpWind, stop: u32, pipe: u32) -> (Vec<f32>, Vec<f32>) {
    const BN: usize = 1024;
    let mut v = PlayingVoice::new_from_sample(aangehouden(frequentie(72), SR as usize), stop, pipe, 72, 1.0);
    v.rate = 1.0;
    v.envelope = 1.0;
    v.target_envelope = 1.0;
    v.zet_wind(&pw, 0, true);
    v.c_use_lfo_trem = false;
    let wind_dev: Vec<[f32; 32]> = (0..BN).map(|f| [0.05 * (f as f32 * 0.01).sin(); 32]).collect();
    let trem_rate = vec![[1.0f64; 32]; BN];
    let trem_amp = vec![[1.0f32; 32]; BN];
    let pan = [std::f32::consts::FRAC_1_SQRT_2; 32];
    let div_pans = vec![0.0f32; 32];
    let ccis_en = vec![false; 32];
    let blok = MengBlok {
        bn: BN, wind_dev: &wind_dev, trem_rate: &trem_rate, trem_amp: &trem_amp,
        pan_cos: &pan, pan_sin: &pan, div_pans: &div_pans,
        ccis_on: false, ccis_en: &ccis_en, ccis_strength: 0.7, ccis_falloff: 0.6, ccis_swap: false,
        pitch_offset_mul: 1.0, licht: false,
    };
    let mut dl = vec![[0.0f32; 32]; BN];
    let mut dr = vec![[0.0f32; 32]; BN];
    meng_stemmen_blok(std::slice::from_mut(&mut v), &mut dl, &mut dr, &blok);
    (dl.iter().map(|f| f[0]).collect(), dr.iter().map(|f| f[0]).collect())
}

fn bits(x: &[f32]) -> Vec<u32> {
    x.iter().map(|v| v.to_bits()).collect()
}

fn fluit() -> StopWindProfiel {
    StopWindProfiel { familie: PijpFamilie::Fluit, voet: 4.0, koren: 1 }
}

/// Met het verschil per pijp op 0 krijgen twee pijpen van verschillende
/// identiteit (ander register, andere pijp) dezelfde windconstanten en dus
/// een bit-identieke uitvoer; op 1 verschillen ze. De tweede helft is de
/// controle dat de test een verschil kan zien.
#[test]
fn oneffenheid_uit_geeft_bitidentieke_uitvoer() {
    let (p, noot) = (fluit(), 72u8);
    let a0 = pijp_constanten(&p, noot, 3, 7, SR, 0.0, true);
    let b0 = pijp_constanten(&p, noot, 9, 2, SR, 0.0, true);
    assert_eq!(a0.kp, WIND_RATE_PER_EENHEID, "uit = het gedrag van vóór de differentiatie");
    assert_eq!(a0.kg, 2.0);
    let (al, ar) = meng_een_stem(a0, 3, 7);
    let (bl, br) = meng_een_stem(b0, 9, 2);
    assert!(rms(&al) > 0.05, "de proef levert geen klank");
    assert_eq!(bits(&al), bits(&bl), "links verschilt met oneffenheid uit");
    assert_eq!(bits(&ar), bits(&br), "rechts verschilt met oneffenheid uit");

    // Controle: op volle oneffenheid klinken ze niet gelijk.
    let a1 = pijp_constanten(&p, noot, 3, 7, SR, 1.0, true);
    let b1 = pijp_constanten(&p, noot, 9, 2, SR, 1.0, true);
    let (a1l, _) = meng_een_stem(a1, 3, 7);
    let (b1l, _) = meng_een_stem(b1, 9, 2);
    assert_ne!(bits(&a1l), bits(&b1l), "de test ziet geen verschil tussen twee pijpen op volle oneffenheid");
    assert_ne!(bits(&al), bits(&a1l), "volle oneffenheid hoort van 'uit' te verschillen");
}

/// Een stem zonder windgevoeligheid (kp = kg = 0) laat de winddruk helemaal
/// langs zich heen gaan: bit-identiek aan dezelfde stem in windstilte.
#[test]
fn zonder_windgevoeligheid_beinvloedt_de_winddruk_de_uitvoer_niet() {
    let stil = PijpWind { verbruik: 0.0, kp: 0.0, kg: 0.0, dev_alpha: 0.05 };
    let (l, r) = meng_een_stem(stil, 1, 1);
    // Zelfde stem, andere traagheid: bij gevoeligheid 0 mag dat niets uitmaken.
    let (l2, r2) = meng_een_stem(PijpWind { dev_alpha: 0.2, ..stil }, 5, 5);
    assert_eq!(bits(&l), bits(&l2));
    assert_eq!(bits(&r), bits(&r2));
    // Controle: mét gevoeligheid verandert dezelfde winddruk de uitvoer wel.
    let (lw, _) = meng_een_stem(PijpWind { kp: 0.05, kg: 2.0, ..stil }, 1, 1);
    assert_ne!(bits(&l), bits(&lw), "de winddruk heeft geen effect met gevoeligheid; de test ziet niets");
}
