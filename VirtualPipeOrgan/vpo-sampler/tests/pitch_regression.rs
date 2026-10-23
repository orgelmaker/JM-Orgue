//! Stemming met echte samplesets: echte importer, echte WAV-markerlezer en de
//! hertemper-kern + koppelfuncties uit de app (src-tauri/src/retune.rs).
//!
//! Sets worden gezocht in `PITCH_TEST_SETS` (standaard `D:\Samplesets`). Een
//! ontbrekende set wordt gemeld als OVERGESLAGEN; de test faalt dan niet.
//! Registernamen in deze tests zijn uitsluitend fixture-selectie, geen
//! stemmingsregel.

#[path = "../../src-tauri/src/retune.rs"]
#[allow(dead_code)]
mod retune;

use std::path::{Path, PathBuf};
use vpo_sampler::{load_hauptwerk, read_wav_markers, OrganDefinition, PipeDef, PipeExtra, StopDef};

fn sets_dir() -> PathBuf {
    std::env::var_os("PITCH_TEST_SETS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"D:\Samplesets"))
}

fn first_file(dir: &Path, ext: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.extension().and_then(|e| e.to_str()).map_or(false, |e| e.eq_ignore_ascii_case(ext))
    })
}

fn hw_odf(set: &str) -> Option<PathBuf> {
    first_file(&sets_dir().join(set).join("OrganDefinitions"), "Organ_Hauptwerk_xml")
}

fn go_odf(set: &str) -> Option<PathBuf> {
    first_file(&sets_dir().join(set), "organ")
}

fn laad(odf: Option<PathBuf>, naam: &str) -> Option<OrganDefinition> {
    let Some(p) = odf else {
        eprintln!("OVERGESLAGEN: set '{}' niet gevonden onder {:?}", naam, sets_dir());
        return None;
    };
    let def = if p.extension().map_or(false, |e| e == "organ") {
        OrganDefinition::load(&p)
    } else {
        load_hauptwerk(&p)
    };
    Some(def.unwrap_or_else(|e| panic!("{}: laden mislukt: {:?}", naam, e)))
}

/// Eén speelbare route (pijp × laag) zoals de app hem opbouwt.
#[derive(Debug)]
struct Route {
    stop: String,
    layer: u8,
    key: u32,
    extra: PipeExtra,
    /// Samplepitch volgens de gekozen methode (ODF, anders WAV-smpl).
    sample: Option<f32>,
    /// Origineel-route: totale correctie (PitchTuning + vervangen opname).
    origineel: f32,
    /// Hertemper-route: correctie, of None (dan PitchTuning).
    retune: Option<f32>,
    input: retune::PipePitchInput,
}

impl Route {
    fn target(&self) -> f32 {
        retune::target_cents(self.input.key_midi, self.input.harmonic)
    }
    /// Klinkende toon in cents bij hertemperen (gelijkzwevend).
    fn speelt_retune(&self) -> f32 {
        self.sample.expect("samplepitch") + self.retune.unwrap_or(self.origineel)
    }
    fn speelt_origineel(&self) -> f32 {
        self.sample.expect("samplepitch") + self.origineel
    }
}

fn stop_first_midi(def: &OrganDefinition, stop: &StopDef) -> u32 {
    if let Some(n) = stop.first_midi_note {
        return n;
    }
    let manual_first = def.manuals.iter().find(|m| m.stop_ids.contains(&stop.id))
        .map(|m| m.first_accessible_key_midi_note).unwrap_or(36);
    manual_first + stop.first_accessible_pipe_logical_key.saturating_sub(1)
}

/// Bouwt alle routes van de stops waarvoor `kies(stop)` waar is, met dezelfde
/// koppeling als `load_grandorgue_organ` in commands.rs.
fn routes(def: &OrganDefinition, kies: impl Fn(&StopDef) -> bool) -> Vec<Route> {
    let wc = |g: u32| def.windchests.iter().find(|w| w.number == g)
        .map(|w| (w.pitch_tuning_cents, w.pitch_correction_cents)).unwrap_or((0.0, 0.0));
    let mut out = Vec::new();
    for stop in def.stops.iter().filter(|s| !s.is_noise_or_mechanical() && kies(s)) {
        for (layer, _persp, pipes) in stop.all_layers() {
            for (idx, pipe) in pipes.iter().enumerate() {
                let Some(PipeDef::Sample { path, extra }) = def.resolve_reference(pipe) else { continue };
                let Some((owner, owner_idx, _)) = def.resolve_reference_owner(stop, idx, pipe) else { continue };
                let odf = retune::odf_measured_cents(extra);
                let wav = || read_wav_markers(path)
                    .and_then(|m| retune::measured_from_smpl(m.unity_note, m.pitch_fraction_cents));
                let sample = odf.or_else(wav);
                let delta = match (extra.original_pitch_cents, sample) {
                    (Some(o), Some(s)) => retune::origineel_delta(o, s).unwrap_or(0.0),
                    _ => 0.0,
                };
                let origineel = def.organ.pitch_tuning_cents + wc(stop.windchest_group).0
                    + stop.pitch_tuning_cents + extra.pitch_tuning_cents.unwrap_or(0.0) + delta;
                let key = extra.key_midi_note.unwrap_or(stop_first_midi(def, owner) + owner_idx as u32);
                let zwevend = retune::is_zwevend_naam(&stop.name);
                let hierarchie = def.organ.pitch_correction_cents + wc(owner.windchest_group).1
                    + owner.pitch_correction;
                let mut input = retune::pipe_pitch_input(
                    extra, key, owner.harmonic_number, hierarchie, zwevend, odf, origineel);
                if input.measured_cents.is_none() {
                    let target = retune::target_cents(input.key_midi, input.harmonic);
                    input.measured_cents = read_wav_markers(path)
                        .and_then(|m| retune::measured_from_smpl_for_target(m.unity_note, m.pitch_fraction_cents, target))
                        .or(extra.original_pitch_cents)
                        .or(Some(target));
                }
                let retune = retune::retune_cents(&input);
                out.push(Route { stop: stop.name.clone(), layer, key, extra: extra.clone(), sample, origineel, retune, input });
            }
        }
    }
    out
}

fn bevat(naam: &str, deel: &str) -> bool {
    naam.to_lowercase().contains(deel)
}

// ── Nitra: vervangen opname Copula maior 8', toets 79 → 077-f.wav ──────────

#[test]
fn nitra_copula_g79_blijft_gerepareerd_in_alle_perspectieven() {
    let Some(def) = laad(hw_odf("Nitra_Demo_1_01.CompPkg.Hauptwerk"), "Nitra") else { return };
    let r: Vec<_> = routes(&def, |s| bevat(&s.name, "copula maior"))
        .into_iter().filter(|r| r.key == 79).collect();
    assert_eq!(r.len(), 3, "verwacht drie perspectieven: {:?}", r.iter().map(|r| (&r.stop, r.layer)).collect::<Vec<_>>());
    for x in &r {
        // ODF-methode 4 (709,11 Hz) gaat vóór de smpl-chunk (712,02 Hz).
        let hz = 440.0 * 2f32.powf((x.sample.unwrap() - 6900.0) / 1200.0);
        assert!((hz - 709.11).abs() < 0.01, "{} samplepitch {hz}", x.stop);
        assert!((x.origineel - 200.731).abs() < 0.05, "{} origineel {}", x.stop, x.origineel);
        let r = x.retune.expect("retune");
        assert!((r - 173.793).abs() < 0.05, "{} gelijkzwevend {r}", x.stop);
    }
}

#[test]
fn nitra_alle_copula_routes_klinken_op_hun_doel() {
    let Some(def) = laad(hw_odf("Nitra_Demo_1_01.CompPkg.Hauptwerk"), "Nitra") else { return };
    let r = routes(&def, |s| bevat(&s.name, "copula maior"));
    assert_eq!(r.len(), 168, "56 toetsen × 3 perspectieven");
    for x in &r {
        let orig = x.extra.original_pitch_cents.expect("originele pijptoon");
        assert!((x.speelt_origineel() - orig).abs() < 0.05, "{} toets {} origineel", x.stop, x.key);
        let doel = x.target() + x.input.keep_pct / 100.0 * (orig - x.target());
        assert!((x.speelt_retune() - doel).abs() < 0.05, "{} toets {} retune {} vs {}", x.stop, x.key, x.speelt_retune(), doel);
    }
}

// ── Hauptwerk BaseTuningSchemeCode 3 (bewuste zweving in cents) ────────────

/// `toetsen` en `lo..=hi` = toetsbereik en Deviation-bereik van de code-3-
/// pijpen volgens de ruwe XML van deze set (fixture-gegevens, met een script
/// gelezen), zodat de test niet op de parser leunt die hij controleert.
fn code3_klinkt_op_temperament_plus_afwijking(set: &str, register: &str, toetsen: std::ops::RangeInclusive<u32>, verwacht: usize, lo: f32, hi: f32) {
    let Some(def) = laad(hw_odf(set), set) else { return };
    let r: Vec<_> = routes(&def, |s| bevat(&s.name, register))
        .into_iter().filter(|x| x.sample.is_some() && toetsen.contains(&x.key)).collect();
    assert_eq!(r.len(), verwacht, "{set}: aantal routes voor '{register}'");
    let (mut min, mut max) = (f32::MAX, f32::MIN);
    for x in &r {
        let afw = x.speelt_retune() - x.target();
        min = min.min(afw);
        max = max.max(afw);
        assert!(afw >= lo - 0.05 && afw <= hi + 0.05,
            "{} toets {}: klinkt {afw:+.3} ct boven temperament, definitie geeft {lo}..{hi}", x.stop, x.key);
        let dev = x.extra.pitch_correction_cents.expect("code-3-afwijking");
        assert!((afw - dev).abs() < 0.05, "{} toets {}: {afw:+.3} vs pijpopgave {dev:+.3}", x.stop, x.key);
        assert_eq!(x.input.keep_pct, 0.0);
    }
    eprintln!("{set} '{register}': {} routes, {min:+.3}..{max:+.3} ct boven temperament", r.len());
}

#[test]
fn nancy_voix_celeste_code3() {
    code3_klinkt_op_temperament_plus_afwijking("Nancy_Demo_1_02.CompPkg.Hauptwerk", "voix c", 48..=96, 196, 14.0, 20.0);
}

#[test]
fn giubiasco_hw_voce_umana_code3() {
    code3_klinkt_op_temperament_plus_afwijking("Giubiasco.CompPkg.Hauptwerk", "voce umana", 55..=93, 39, 14.0652, 15.8202);
}

// ── GrandOrgue: PitchCorrection is de doelafwijking bij hertemperen ────────

#[test]
fn giubiasco_go_voce_umana_houdt_opgegeven_15_cent() {
    let Some(def) = laad(go_odf("Giubiasco_GrandOrgue"), "Giubiasco GO") else { return };
    let r = routes(&def, |s| bevat(&s.name, "voce umana"));
    assert!(!r.is_empty());
    for x in r.iter().filter(|x| x.sample.is_some()) {
        let afw = x.speelt_retune() - x.target();
        assert!((afw - 15.0).abs() < 0.05, "{} toets {}: {afw:+.3} ct i.p.v. +15", x.stop, x.key);
        assert_eq!(x.input.keep_pct, 0.0, "{} toets {}", x.stop, x.key);
    }
}

/// Origineel-route van GrandOrgue = samplepitch + PitchTuning; PitchCorrection
/// hoort daar niet bij.
#[test]
fn giubiasco_go_origineel_gebruikt_alleen_pitchtuning() {
    let Some(def) = laad(go_odf("Giubiasco_GrandOrgue"), "Giubiasco GO") else { return };
    for x in routes(&def, |s| bevat(&s.name, "voce umana")) {
        let pt = def.organ.pitch_tuning_cents + x.extra.pitch_tuning_cents.unwrap_or(0.0);
        assert!((x.origineel - pt).abs() < 1e-3, "{} toets {}", x.stop, x.key);
    }
}

// ── Cracow (uit de overdracht; alleen als de sets aanwezig zijn) ───────────

fn cracow_midi48(def: &OrganDefinition, register: &str) -> Vec<f32> {
    routes(def, |s| bevat(&s.name, register))
        .into_iter()
        .filter(|r| r.key == 48 && r.sample.is_some())
        .map(|r| r.speelt_retune() - r.target())
        .collect()
}

#[test]
fn regression_cracow_hw_celestes_must_reach_explicit_target() {
    let Some(def) = laad(hw_odf("CracowStJohnCantius_1_03.CompPkg.Hauptwerk"), "Cracow HW") else { return };
    for (reg, dev) in [("unda", 13.0), ("leste", 11.0)] {
        let afw = cracow_midi48(&def, reg);
        assert!(!afw.is_empty(), "{reg}: geen route op MIDI 48");
        for a in afw {
            assert!((a - dev).abs() < 0.05, "Cracow HW {reg} MIDI 48: {a:+.4} ct, verwacht {dev:+}");
        }
    }
}

#[test]
fn regression_cracow_go_retune_must_preserve_declared_15_cents() {
    let Some(def) = laad(go_odf("CracowStJohnCantius_GrandOrgue_1_03"), "Cracow GO") else { return };
    for reg in ["unda", "leste"] {
        let afw = cracow_midi48(&def, reg);
        assert!(!afw.is_empty(), "{reg}: geen route op MIDI 48");
        for a in afw {
            assert!((a - 15.0).abs() < 0.05, "Cracow GO {reg} MIDI 48: {a:+.4} ct, verwacht +15");
        }
    }
}

/// Diagnose: `PITCH_SHOW="<set>|<register>|<toets>"` drukt per route de
/// samplepitch en de verwachte klinkende frequentie in beide routes af.
#[test]
#[ignore]
fn toon_routes() {
    let Ok(v) = std::env::var("PITCH_SHOW") else { return };
    let d: Vec<&str> = v.split('|').collect();
    let odf = hw_odf(d[0]).or_else(|| go_odf(d[0]));
    let Some(def) = laad(odf, d[0]) else { return };
    let key: u32 = d[2].parse().unwrap();
    let hz = |c: f32| 440.0 * 2f32.powf((c - 6900.0) / 1200.0);
    for x in routes(&def, |s| bevat(&s.name, d[1])).into_iter().filter(|x| x.key == key) {
        let Some(s) = x.sample else { continue };
        println!("{} laag {} toets {}: sample {:.3} Hz, origineel {:.3} Hz, hertemperd {:.3} Hz (keep {}, pc {:?})",
            x.stop, x.layer, x.key, hz(s), hz(x.speelt_origineel()), hz(x.speelt_retune()),
            x.input.keep_pct, x.extra.pitch_correction_cents);
    }
}

/// Diagnose: `PITCH_KEYS="<pad naar odf>"` drukt per register het
/// toetsbereik (laagste/hoogste getoonde toets) en het aantal pijpen af.
#[test]
#[ignore]
fn toon_toetsen() {
    let Ok(pad) = std::env::var("PITCH_KEYS") else { return };
    let def = load_hauptwerk(Path::new(&pad)).expect("laden");
    for s in &def.stops {
        let n = s.pipes.iter().filter(|p| !matches!(p, PipeDef::Empty)).count();
        println!("{:24} first_midi {:?} pijpen {} (slots {})", s.name, s.first_midi_note, n, s.pipes.len());
    }
}
