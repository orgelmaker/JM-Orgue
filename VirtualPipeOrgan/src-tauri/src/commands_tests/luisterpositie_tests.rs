//! Testen voor de luisterpositie-schuif (0.7.95): canonieke volgorde van
//! opnameposities op de as, equal-power crossfade, en de merkstreep-regel.
use super::*;
use crate::state::PerspectiveRuntime;

fn p(name: &str, slot: u8, enabled: bool, loaded: bool) -> PerspectiveRuntime {
    PerspectiveRuntime {
        name: name.into(),
        enabled, loaded, slot, gain_db: 0.0, pipe_count: 0,
    }
}

#[test]
fn as_rang_volgt_route_van_dichtbij_naar_ver() {
    // De as loopt van bij het orgel (dry/close) via front naar achter (rear/
    // diffuse). Afwijkende labels (`micN`) komen daarna, onbekende achter
    // alles.
    assert!(luister_as_rang("dry") < luister_as_rang("close"));
    assert!(luister_as_rang("close") < luister_as_rang("front"));
    assert!(luister_as_rang("front") < luister_as_rang("middle"));
    assert!(luister_as_rang("middle") < luister_as_rang("rear"));
    assert!(luister_as_rang("rear") < luister_as_rang("diffuse"));
    assert!(luister_as_rang("diffuse") < luister_as_rang("mic1"));
    assert!(luister_as_rang("mic1") < luister_as_rang("onbekend_label"));
    // Hoofdletters tellen niet mee.
    assert_eq!(luister_as_rang("Front"), luister_as_rang("front"));
}

#[test]
fn indices_sorteren_canoniek_en_slaan_uitgezette_positie_over() {
    // Documentvolgorde close, rear, front — moet close, front, rear worden.
    // Een uitgezette positie telt niet mee op de as.
    let v = vec![
        p("close", 1, true,  true),
        p("rear",  2, false, true),  // uit → slaat de schuif over
        p("front", 3, true,  true),
        p("niet-geladen", 4, true, false), // niet geladen → telt niet
    ];
    let idx = luisteras_indices(&v);
    let namen: Vec<&str> = idx.iter().map(|&i| v[i].name.as_str()).collect();
    assert_eq!(namen, vec!["close", "front"]);
}

#[test]
fn merkstreep_speelt_een_positie_vol() {
    // 3 posities, pos = 0.5 = de middelste streep: alleen die klinkt (1.0),
    // de andere twee staan op lineaire gain 0.
    let g0 = luister_gain_lineair(0.5, 0, 3);
    let g1 = luister_gain_lineair(0.5, 1, 3);
    let g2 = luister_gain_lineair(0.5, 2, 3);
    assert!(g0.abs() < 1e-6, "{}", g0);
    assert!((g1 - 1.0).abs() < 1e-6, "{}", g1);
    assert!(g2.abs() < 1e-6, "{}", g2);
}

#[test]
fn eindpunten_zijn_identiek_aan_een_enkele_positie() {
    // 2 posities, pos = 0.0 = alléén eerste; pos = 1.0 = alléén tweede.
    assert!((luister_gain_lineair(0.0, 0, 2) - 1.0).abs() < 1e-6);
    assert!(luister_gain_lineair(0.0, 1, 2).abs() < 1e-6);
    assert!(luister_gain_lineair(1.0, 0, 2).abs() < 1e-6);
    assert!((luister_gain_lineair(1.0, 1, 2) - 1.0).abs() < 1e-6);
}

#[test]
fn equal_power_in_het_midden_blijft_op_vol_vermogen() {
    // Bij een equal-power crossfade geldt g1² + g2² = 1.
    for n in 2..=5 {
        for k in 0..n-1 {
            // Midden van segment [k, k+1] in de as [0, n-1].
            let pos = (k as f32 + 0.5) / (n - 1) as f32;
            let g_links = luister_gain_lineair(pos, k, n);
            let g_rechts = luister_gain_lineair(pos, k + 1, n);
            let som = g_links * g_links + g_rechts * g_rechts;
            assert!((som - 1.0).abs() < 1e-5, "n={} k={} som={}", n, k, som);
            // Alle andere posities op gain 0.
            for i in 0..n {
                if i != k && i != k + 1 {
                    assert!(luister_gain_lineair(pos, i, n).abs() < 1e-6,
                        "n={} k={} i={} nog gain", n, k, i);
                }
            }
        }
    }
}

#[test]
fn een_positie_blijft_altijd_op_vol_vermogen() {
    // Zonder keuze (één positie): elke schuifwaarde geeft volle gain.
    for pos in [0.0, 0.25, 0.5, 0.75, 1.0] {
        assert!((luister_gain_lineair(pos, 0, 1) - 1.0).abs() < 1e-6);
    }
}

/// Actieve posities en hun buren (0.8.1): op een merkstreep is één
/// positie actief en zijn de twee buren "stil-met-stem"; ertussen zijn
/// er twee actieve en hoogstens twee buren. De rest krijgt geen stem.
#[test]
fn actief_en_buren_begrenzen_de_stemmen_op_vier_posities() {
    // 5 posities, op merkstreep 2 (pos 0.5): actief [2], buren [1, 3].
    let (a, b) = luister_actief_en_buren(0.5, 5);
    assert_eq!(a, vec![2]);
    assert_eq!(b, vec![1, 3]);
    // Tussen 1 en 2 (pos 0.375): actief [1, 2], buren [0, 3].
    let (a, b) = luister_actief_en_buren(0.375, 5);
    assert_eq!(a, vec![1, 2]);
    assert_eq!(b, vec![0, 3]);
    // Eindpunt: actief [0], buur [1].
    let (a, b) = luister_actief_en_buren(0.0, 5);
    assert_eq!(a, vec![0]);
    assert_eq!(b, vec![1]);
    // Eén of geen positie.
    assert_eq!(luister_actief_en_buren(0.7, 1), (vec![0], vec![]));
    assert_eq!(luister_actief_en_buren(0.7, 0), (vec![], vec![]));
}

/// RAM-rem (0.8.1): zonder opslag en met een te grote schatting blijft
/// alleen de dichtstbijzijnde positie aan; met opslag grijpt hij niet in.
#[test]
fn ram_rem_houdt_alleen_dichtstbij_over() {
    let mk = |name: &str, slot: u8, pipes: usize| PerspectiveRuntime {
        name: name.into(), enabled: true, gain_db: 0.0, loaded: false, slot, pipe_count: pipes,
    };
    let geen = std::collections::HashMap::new();
    // Onrealistisch veel pijpen zodat de rem op élke pc afgaat.
    let mut plan = vec![mk("rear", 1, 50_000_000), mk("close", 2, 50_000_000), mk("front", 3, 50_000_000)];
    let ingegrepen = rem_op_geheugen(&mut plan, false, &geen);
    if fysiek_geheugen_bytes().is_some() {
        assert!(ingegrepen);
        assert_eq!(plan.iter().map(|p| (p.name.as_str(), p.enabled)).collect::<Vec<_>>(),
            vec![("rear", false), ("close", true), ("front", false)]);
    }
    // Met opgeslagen voorkeur: niets veranderen.
    let mut plan2 = vec![mk("rear", 1, 50_000_000), mk("close", 2, 50_000_000)];
    assert!(!rem_op_geheugen(&mut plan2, true, &geen));
    assert!(plan2.iter().all(|p| p.enabled));
    // Kleine set: niets veranderen.
    let mut plan3 = vec![mk("rear", 1, 500), mk("close", 2, 500)];
    assert!(!rem_op_geheugen(&mut plan3, false, &geen));
    assert!(plan3.iter().all(|p| p.enabled));
    // De buffertelling wint van het pijpenaantal: 500 pijpen met elk
    // 100 000 buffers is wél te groot.
    let mut plan4 = vec![mk("rear", 1, 500), mk("close", 2, 500)];
    let buffers: std::collections::HashMap<String, usize> =
        [("rear".to_string(), 50_000_000usize), ("close".to_string(), 50_000_000usize)].into_iter().collect();
    if fysiek_geheugen_bytes().is_some() {
        assert!(rem_op_geheugen(&mut plan4, false, &buffers));
        assert_eq!(plan4.iter().filter(|p| p.enabled).count(), 1);
    }
}

/// F1: lagen zonder perspectief-label tellen mee in de schatting voor het
/// vrije geheugen; de bestaande telling per positie blijft ongewijzigd.
#[test]
fn preload_schatting_telt_lagen_zonder_label_mee() {
    let mk = |enabled: bool| PerspectiveRuntime {
        name: "front".into(), enabled, gain_db: 0.0, loaded: false, slot: 1, pipe_count: 900,
    };
    let mut buffers = std::collections::HashMap::new();
    buffers.insert("front".to_string(), 1000usize);
    buffers.insert(GEEN_PERSPECTIEF.to_string(), 400usize);
    let per_buffer = preload_bytes_per_buffer();
    // De telling per positie ziet de lagen zonder label niet.
    assert_eq!(schatting_preload_bytes(&[mk(true)], &buffers), 1000 * per_buffer);
    assert_eq!(vaste_preload_bytes(&buffers), 400 * per_buffer);
    // Een set zonder posities in het plan: alleen de vaste lagen tellen.
    assert_eq!(schatting_preload_bytes(&[], &buffers), 0);
    assert_eq!(
        schatting_preload_bytes_voor(&[], &buffers, true) + vaste_preload_bytes(&buffers),
        400 * per_buffer);
    // Zonder die sleutel: niets.
    assert_eq!(vaste_preload_bytes(&std::collections::HashMap::new()), 0);
    // Een uitgeschakelde positie telt alleen mee bij "alles laden".
    assert_eq!(schatting_preload_bytes_voor(&[mk(false)], &buffers, false), 0);
    assert_eq!(schatting_preload_bytes_voor(&[mk(false)], &buffers, true), 1000 * per_buffer);
}

#[test]
fn mic_posities_sorteren_op_nummer() {
    let v = vec![
        p("mic10", 1, true, true),
        p("mic2",  2, true, true),
        p("mic1",  3, true, true),
    ];
    let idx = luisteras_indices(&v);
    let namen: Vec<&str> = idx.iter().map(|&i| v[i].name.as_str()).collect();
    assert_eq!(namen, vec!["mic1", "mic2", "mic10"]);
}

/// F2: de schatting telt unieke (bestand, segment)-paren, zoals de laadroute ze één keer laadt;
/// pijpen die een bestand delen tellen niet dubbel, releases met een ander cue-punt wel.
#[test]
fn preload_schatting_telt_unieke_bestanden_en_geen_pijpen() {
    use std::collections::HashSet;
    use vpo_sampler::grandorgue::{PipeExtra, ReleaseDef};
    use vpo_sampler::PipeDef;
    let release = |pad: &str, cue: Option<u32>| ReleaseDef {
        path: pad.into(), max_key_press_time_ms: None, cue_point: cue, release_end: None, crossfade_ms: None,
    };
    let pijp = |pad: &str, releases: Vec<ReleaseDef>| PipeDef::Sample {
        path: pad.into(),
        extra: PipeExtra { releases, ..Default::default() },
    };
    let mut unieke = HashSet::new();
    let mut referenties = 0usize;
    // Drie pijpen delen hetzelfde attack-bestand en dezelfde release (twee registers op één rank).
    for _ in 0..3 {
        voeg_pijp_buffers_toe(&pijp("a.wav", vec![release("r.wav", None)]), &mut unieke, &mut referenties);
    }
    assert_eq!(unieke.len(), 2, "attack + release, niet 3 x 2");
    // Dezelfde release met een ander cue-punt is een ander segment.
    voeg_pijp_buffers_toe(&pijp("a.wav", vec![release("r.wav", Some(7))]), &mut unieke, &mut referenties);
    assert_eq!(unieke.len(), 3);
    // Een tremulantlaag telt mee, ook met gedeelde bestanden maar één keer.
    let mut trem = pijp("b.wav", vec![]);
    if let PipeDef::Sample { extra, .. } = &mut trem {
        extra.tremulant_attack = Some(Default::default());
        if let Some(t) = &mut extra.tremulant_attack { t.path = "t.wav".into(); }
        extra.tremulant_releases = vec![release("tr.wav", None)];
    }
    voeg_pijp_buffers_toe(&trem, &mut unieke, &mut referenties);
    voeg_pijp_buffers_toe(&trem, &mut unieke, &mut referenties);
    assert_eq!(unieke.len(), 3 + 3, "b.wav, t.wav, tr.wav erbij, de tweede keer niets");
    // Een verwijzing blijft één buffer, een lege pijp niets.
    voeg_pijp_buffers_toe(&PipeDef::Reference { manual: 1, stop: 1, pipe: 1 }, &mut unieke, &mut referenties);
    voeg_pijp_buffers_toe(&PipeDef::Empty, &mut unieke, &mut referenties);
    assert_eq!(referenties, 1);
}
