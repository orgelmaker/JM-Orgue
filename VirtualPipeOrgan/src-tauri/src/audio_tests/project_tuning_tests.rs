use super::*;

fn pitch(normalize: bool, reference: f64) -> ProjectSourcePitch {
    ProjectSourcePitch::new(normalize, reference, 440.0, 0.0, false).unwrap()
}

fn rate(p: &ProjectSourcePitch, global_retune: bool, retuned: Option<f32>) -> f64 {
    voice_pitch_ratio(Some(p), global_retune, 0.0, retuned, 0.0, 1.0, 1.0)
}

#[test]
fn twee_bronnen_normaliseren_naar_dezelfde_referentie() {
    for reference in [415.0, 440.0, 442.0] {
        let p = pitch(true, reference);
        assert!((reference * rate(&p, true, Some(88.0)) - 440.0).abs() < 1e-9);
    }
}

#[test]
fn origineel_negeert_globale_auto_retune_en_temperament() {
    for reference in [415.0, 440.0] {
        let p = pitch(false, reference);
        let multiplier = voice_pitch_ratio(Some(&p), true, 0.0, Some(101.27), 0.0, 1.04, 1.0);
        assert_eq!(reference * multiplier, reference);
    }
}

#[test]
fn bron_normaliseren_behoudt_celeste_en_interne_intervallen() {
    let p = pitch(true, 415.0);
    for source_interval in [0.0, 10.0, -13.7, 702.0] {
        let interval_ratio = 2.0_f64.powf(source_interval / 1200.0);
        let actual = 415.0 * interval_ratio * rate(&p, true, Some(0.0));
        assert!((actual - 440.0 * interval_ratio).abs() < 1e-9);
    }
}

#[test]
fn broncorrectie_stapelt_met_odf_en_gebruikersintonatie() {
    let p = ProjectSourcePitch::new(true, 415.0, 440.0, 5.0, false).unwrap();
    let multiplier = voice_pitch_ratio(Some(&p), true, -7.0, Some(200.0), 3.0, 1.03, 2.0);
    let expected = 440.0 / 415.0 * 2.0_f64.powf(1.0 / 1200.0) * 2.0;
    assert!((multiplier - expected).abs() < 1e-12);
}

#[test]
fn gewone_orgels_behouden_het_bestaande_stemgedrag() {
    for (retune, expected_cents) in [(false, 11.0_f32), (true, -5.0)] {
        let actual = voice_pitch_ratio(None, retune, 8.0, Some(-8.0), 3.0, 1.02, 0.5);
        let expected = 1.02 * 2.0_f64.powf(expected_cents as f64 / 1200.0) * 0.5;
        assert!((actual - expected).abs() < 1e-12);
    }
    // Geen plausibele hertempercorrectie: ODF-PitchTuning blijft gelden.
    assert_eq!(
        voice_pitch_ratio(None, true, 8.0, None, 3.0, 1.02, 0.5),
        voice_pitch_ratio(None, false, 8.0, None, 3.0, 1.02, 0.5),
    );
}

#[test]
fn bronbeleid_blijft_onafhankelijk_van_verplicht_getempereerde_buur() {
    let original = pitch(false, 415.0);
    let forced = ProjectSourcePitch::new(true, 415.0, 442.0, 0.0, true).unwrap();
    assert_eq!(rate(&original, true, Some(101.27)), 1.0);
    // Alleen deze bron heeft een A440-retunebaseline, onafhankelijk van de
    // opgegeven oorspronkelijke referentie (die voor deze bron niet bestaat).
    let actual = voice_pitch_ratio(Some(&forced), false, -9.0, Some(0.0), 0.0, 1.05, 1.0);
    assert!((440.0 * actual - 442.0).abs() < 1e-9);
}

#[test]
fn attack_tremulant_en_release_gebruiken_dezelfde_bronfactor() {
    let source = pitch(true, 415.0);
    let keys = [60, 60 | TREM_FLAG, 60 | RELEASE_PIPE_FLAG, 60 | TREM_FLAG | RELEASE_PIPE_FLAG];
    for pipe in keys {
        // De oorspronkelijke ODF-correctie van elke opname wordt toegepast;
        // source-normalisatie voegt daarna voor alle voices dezelfde factor toe.
        let odf = if pipe & TREM_FLAG != 0 { -15.0 } else { 7.0 };
        let actual = voice_pitch_ratio(Some(&source), true, odf, Some(123.0), 0.0, 1.04, 1.0);
        let expected = 440.0 / 415.0 * 2.0_f64.powf(odf as f64 / 1200.0);
        assert!((actual - expected).abs() < 1e-12);
    }
    let retune = HashMap::from([((1, 60), 4.0), ((1, 60 | TREM_FLAG), -8.0)]);
    assert_eq!(retune_pitch_for_voice(1, 60, &retune), Some(4.0));
    assert_eq!(retune_pitch_for_voice(1, 60 | RELEASE_PIPE_FLAG, &retune), Some(4.0));
    assert_eq!(retune_pitch_for_voice(1, 60 | TREM_FLAG, &retune), Some(-8.0));
    assert_eq!(retune_pitch_for_voice(1, 60 | TREM_FLAG | RELEASE_PIPE_FLAG, &retune), Some(-8.0));
}

#[test]
fn ongeldige_broninstellingen_worden_voor_de_callback_geweigerd() {
    for hz in [0.0, -440.0, f64::NAN, f64::INFINITY] {
        assert!(ProjectSourcePitch::new(true, hz, 440.0, 0.0, false).is_err());
        assert!(ProjectSourcePitch::new(true, 440.0, hz, 0.0, false).is_err());
    }
    assert!(ProjectSourcePitch::new(true, 415.0, 440.0, f32::NAN, false).is_err());
    assert!(ProjectSourcePitch::new(true, 415.0, 440.0, 2500.0, false).is_err());
}

#[test]
fn oorspronkelijke_tremulantpitch_en_release_kiezen_eigen_odf_correctie() {
    let map = HashMap::from([((1, 60), (-3.0, 7.0)), ((1, 60 | TREM_FLAG), (-3.0, -15.0))]);
    for release in [0, RELEASE_PIPE_FLAG] {
        assert_eq!(odf_voicing_for_voice(1, 60 | release, true, &map), (-3.0, 7.0));
        assert_eq!(odf_voicing_for_voice(1, 60 | TREM_FLAG | release, true, &map), (-3.0, -15.0));
        // Gewone orgels houden het bestaande gedrag.
        assert_eq!(odf_voicing_for_voice(1, 60 | TREM_FLAG | release, false, &map), (-3.0, 7.0));
    }
    let dry_only = HashMap::from([((1, 60), (-3.0, 7.0))]);
    assert_eq!(odf_voicing_for_voice(1, 60 | TREM_FLAG, true, &dry_only), (-3.0, 7.0));
    assert_eq!(odf_voicing_for_voice(2, 60 | TREM_FLAG, true, &map), (0.0, 0.0));
}

#[test]
fn automatisch_gelijkstemmen_gebruikt_de_gemeten_pijpcorrectie_en_de_projectreferentie() {
    // Twee bronnen: de ene pijp is 20 cent te laag opgenomen, de andere 35 cent te hoog.
    // De hertemper-tabel levert per pijp de correctie; beide komen op dezelfde toon uit.
    let p = ProjectSourcePitch::new(true, 415.0, 442.0, 0.0, true).unwrap();
    let laag = 440.0 * 2.0_f64.powf(-20.0 / 1200.0) * voice_pitch_ratio(Some(&p), false, 0.0, Some(20.0), 0.0, 1.04, 1.0);
    let hoog = 440.0 * 2.0_f64.powf(35.0 / 1200.0) * voice_pitch_ratio(Some(&p), false, 0.0, Some(-35.0), 0.0, 1.04, 1.0);
    assert!((laag - 442.0).abs() < 1e-9, "{laag}");
    assert!((hoog - 442.0).abs() < 1e-9, "{hoog}");
}

#[test]
fn automatisch_gelijkstemmen_negeert_de_ingevulde_bronreferentie_en_stapelt_extra_cent() {
    for reference in [415.0, 440.0, 466.0] {
        let p = ProjectSourcePitch::new(true, reference, 440.0, 10.0, true).unwrap();
        let ratio = voice_pitch_ratio(Some(&p), true, -7.0, Some(0.0), 0.0, 1.0, 1.0);
        assert!((ratio - 2.0_f64.powf(10.0 / 1200.0)).abs() < 1e-12);
    }
}
