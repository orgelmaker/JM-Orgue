use super::*;
use crate::commands::{DivisionDto, StopDto};

fn stop(id: &str, n: u32) -> StopDto {
    StopDto {
        id: id.to_string(), name: id.to_string(), pitch: "8".to_string(), drawn: false,
        groep: "grond", familie: "principaal", has_tremulant: false, midi_action_code: 0, internal_stop_id: n,
        first_midi_note: 36, last_midi_note: 96, is_reed: false, koren: None, mengwerk: false, wind_voet: 8.0, percussief: false,
        doorloop_onder: 0, doorloop_boven: 127,
    }
}

fn organ() -> OrganInfoDto {
    OrganInfoDto {
        id: "test".into(), name: "Test".into(), builder: String::new(), location: String::new(),
        year: None, stop_count: 3,
        divisions: vec![DivisionDto {
            name: "Hoofdwerk".into(), display_name: "Hoofdwerk".into(),
            stops: vec![stop("a", 1), stop("b", 2), stop("c", 3)],
            has_tremulant: false, tremulant_kind: None, has_swell: false, is_pedal: false,
        }],
        couplers: Some(vec![CouplerDto {
            id: "real_coupler_1".into(), name: "II/I".into(), source_division: "Hoofdwerk".into(),
            destination_division: "Hoofdwerk".into(), active: false, display_in_division: "Hoofdwerk".into(),
            midi_action_code: 0, coupler_type: "unison".into(), pitch_offset: 0, speciaal: false, cascade_toegestaan: true,
        }]),
        retune_pipes: 0, retune_total: 0, perspectives: Vec::new(), layered_stops: 0,
        release_pipes: 0,
        background_sounds: Vec::new(),
        key_click_count: 0,
        stop_click_count: 0,
        schakelaars: None,
        acoustics: None,
    }
}

fn ids(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

struct Rig {
    stages: Arc<RwLock<Vec<Vec<String>>>>,
    stage: Arc<RwLock<u8>>,
    claims: Arc<RwLock<Vec<String>>>,
    drawn: Arc<RwLock<Vec<String>>>,
    couplers: Arc<RwLock<Vec<String>>>,
    player: Arc<RwLock<Option<AudioPlayer>>>,
    organ: Arc<RwLock<Option<OrganInfoDto>>>,
    mappings: Arc<RwLock<Vec<MidiChannelMapping>>>,
    held: Arc<RwLock<Vec<(u8, u8, u8)>>>,
}

impl Rig {
    fn new(stages: Vec<Vec<String>>, drawn: Vec<String>) -> Self {
        Rig {
            stages: Arc::new(RwLock::new(stages)),
            stage: Arc::new(RwLock::new(0)),
            claims: Arc::new(RwLock::new(Vec::new())),
            drawn: Arc::new(RwLock::new(drawn)),
            couplers: Arc::new(RwLock::new(Vec::new())),
            player: Arc::new(RwLock::new(None)),
            organ: Arc::new(RwLock::new(Some(organ()))),
            mappings: Arc::new(RwLock::new(Vec::new())),
            held: Arc::new(RwLock::new(Vec::new())),
        }
    }
    fn apply(&self, s: u8) -> (Vec<String>, Vec<String>) {
        AppState::apply_crescendo_stage_inner(
            s, &self.stages, &self.stage, &self.claims, &self.drawn, &self.couplers,
            &self.player, &self.organ, &self.mappings, &self.held,
        )
    }
    fn sorted(v: &Arc<RwLock<Vec<String>>>) -> Vec<String> {
        let mut x = v.read().clone(); x.sort(); x
    }
}

fn organ_met_koppel() -> OrganInfoDto {
    let mut o = organ();
    let mut r = stop("r", 20);
    r.drawn = true; // de DTO-vlag staat al op 'getrokken' als de sync draait
    o.divisions = vec![
        DivisionDto {
            name: "Pedaal".into(), display_name: "Pedaal".into(), stops: vec![stop("p", 10)],
            has_tremulant: false, tremulant_kind: None, has_swell: false, is_pedal: true,
        },
        DivisionDto {
            name: "Hoofdwerk".into(), display_name: "Hoofdwerk".into(), stops: vec![r],
            has_tremulant: false, tremulant_kind: None, has_swell: false, is_pedal: false,
        },
    ];
    o.couplers = Some(vec![CouplerDto {
        id: "k".into(), name: "I/P".into(), source_division: "Pedaal".into(),
        destination_division: "Hoofdwerk".into(), active: false, display_in_division: "Pedaal".into(),
        midi_action_code: 0, coupler_type: "unison".into(), pitch_offset: 0, speciaal: false, cascade_toegestaan: true,
    }]);
    o
}

/// 0.7.74: een trap die register r én koppel Pedaal→Hoofdwerk tegelijk
/// bijtrekt, start de pijp van r precies één keer. Met de NIEUWE koppellijst
/// in sync_stop_voices_inner (de fout tot 0.7.73) kwam hij twee keer:
/// refcount 2, één NoteOff bij loslaten → hanger zonder enige drop.
#[test]
fn trap_met_register_en_koppel_tegelijk_start_pijp_een_keer() {
    let o = organ_met_koppel();
    let (tx, rx) = bounded::<AudioCommand>(64);
    let mappings = vec![
        MidiChannelMapping { division: "Pedaal".into(), channel: Some(0), ..Default::default() },
        MidiChannelMapping { division: "Hoofdwerk".into(), channel: Some(1), ..Default::default() },
    ];
    let held = vec![(0u8, 48u8, 100u8)];
    let voor: Vec<String> = Vec::new();
    let na = ids(&["k"]);
    let tel = |rx: &Receiver<AudioCommand>| rx.try_iter().filter(|c| matches!(c, AudioCommand::NoteOn { stop_id: 20, .. })).count();
    // Zoals apply_crescendo_stage_inner sinds 0.7.74: register met de OUDE lijst, dan de koppelwissel.
    AppState::sync_stop_voices_inner(&o, &tx, &mappings, &voor, &na, &held, "r", true);
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &held, &voor, &na);
    assert_eq!(tel(&rx), 1);
    // Ter documentatie: met de nieuwe lijst als routelijst is het er twee.
    AppState::sync_stop_voices_inner(&o, &tx, &mappings, &na, &na, &held, "r", true);
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &held, &voor, &na);
    assert_eq!(tel(&rx), 2);
    // Alleen het register bijtrekken terwijl de koppel al stond: ook één.
    AppState::sync_stop_voices_inner(&o, &tx, &mappings, &na, &na, &held, "r", true);
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &held, &na, &na);
    assert_eq!(tel(&rx), 1);
}

/// T.A. (Tongwerken Af) komt uit de NIEUWE lijst: een tongwerk dat samen
/// met T.A. bijkomt start niet; valt T.A. in dezelfde trap weg, dan wél.
#[test]
fn tongwerk_volgt_de_nieuwe_ta_stand() {
    let mut o = organ_met_koppel();
    o.divisions[1].stops[0].is_reed = true;
    o.couplers.as_mut().unwrap().push(CouplerDto {
        id: "ta1".into(), name: "T.A.".into(), source_division: "Hoofdwerk".into(),
        destination_division: "Hoofdwerk".into(), active: false, display_in_division: "Hoofdwerk".into(),
        midi_action_code: 0, coupler_type: "ta".into(), pitch_offset: 0, speciaal: false, cascade_toegestaan: true,
    });
    let (tx, rx) = bounded::<AudioCommand>(64);
    let mappings = vec![MidiChannelMapping { division: "Hoofdwerk".into(), channel: Some(1), ..Default::default() }];
    let held = vec![(1u8, 60u8, 100u8)];
    let zonder: Vec<String> = Vec::new();
    let met = ids(&["ta1"]);
    let tel = |rx: &Receiver<AudioCommand>| rx.try_iter().filter(|c| matches!(c, AudioCommand::NoteOn { stop_id: 20, .. })).count();
    // Tongwerk én T.A. tegelijk bij: niet starten.
    AppState::sync_stop_voices_inner(&o, &tx, &mappings, &zonder, &met, &held, "r", true);
    assert_eq!(tel(&rx), 0);
    // Tongwerk bij terwijl T.A. wegvalt: wél starten.
    AppState::sync_stop_voices_inner(&o, &tx, &mappings, &met, &zonder, &held, "r", true);
    assert_eq!(tel(&rx), 1);
}

/// 0.9.39: Unison Off zet de directe route uit, maakt zelf geen route en
/// schakelt live: aan laat de direct klinkende pijp los, uit start hem weer.
#[test]
fn unison_off_zet_de_directe_route_uit() {
    let mut o = organ_met_koppel(); // "r" (stop_id 20) op Hoofdwerk, getrokken
    o.couplers.as_mut().unwrap().push(CouplerDto {
        id: "uo".into(), name: "Unison Off".into(), source_division: "Hoofdwerk".into(),
        destination_division: "Hoofdwerk".into(), active: false, display_in_division: "Hoofdwerk".into(),
        midi_action_code: 0, coupler_type: "unison_off".into(), pitch_offset: 0, speciaal: true, cascade_toegestaan: true,
    });
    let cl = o.couplers.clone().unwrap();
    let zonder: Vec<String> = Vec::new();
    let met = ids(&["uo"]);
    // Geen route, ook niet transitief.
    assert!(AppState::expand_coupler_routes(&cl, &met).is_empty());
    assert!(unison_off_actief(Some(&cl), &met, "Hoofdwerk"));
    assert!(!unison_off_actief(Some(&cl), &met, "Pedaal"));
    assert!(!unison_off_actief(Some(&cl), &zonder, "Hoofdwerk"));

    let (tx, rx) = bounded::<AudioCommand>(64);
    let mappings = vec![MidiChannelMapping { division: "Hoofdwerk".into(), channel: Some(1), ..Default::default() }];
    let held = vec![(1u8, 60u8, 100u8)];
    let noot_aan = |rx: &Receiver<AudioCommand>| rx.try_iter().filter(|c| matches!(c, AudioCommand::NoteOn { stop_id: 20, .. })).count();
    // Register trekken onder Unison Off: de directe route blijft stil.
    AppState::sync_stop_voices_inner(&o, &tx, &mappings, &met, &met, &held, "r", true);
    assert_eq!(noot_aan(&rx), 0);
    AppState::sync_stop_voices_inner(&o, &tx, &mappings, &zonder, &zonder, &held, "r", true);
    assert_eq!(noot_aan(&rx), 1);
    // Live wisselen: aan → NoteOff per pijp (geen ReleaseStop), uit → NoteOn.
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &held, &zonder, &met);
    let cmds: Vec<AudioCommand> = rx.try_iter().collect();
    assert_eq!(cmds.iter().filter(|c| matches!(c, AudioCommand::NoteOff { stop_id: 20, .. })).count(), 1, "aan: directe pijp los");
    assert!(!cmds.iter().any(|c| matches!(c, AudioCommand::ReleaseStop { .. })));
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &held, &met, &zonder);
    assert_eq!(noot_aan(&rx), 1, "uit: directe pijp weer aan");
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &[], &zonder, &met);
    assert_eq!(rx.try_iter().count(), 0);
}

/// 0.9.12: T.A. wisselen terwijl een toets is ingedrukt werkt meteen —
/// aan laat het klinkende tongwerk los, uit start het weer.
#[test]
fn ta_wissel_met_ingedrukte_toets_schakelt_tongwerk_live() {
    let mut o = organ_met_koppel();
    o.divisions[1].stops[0].is_reed = true; // "r" (stop_id 20) op Hoofdwerk, getrokken
    o.couplers.as_mut().unwrap().push(CouplerDto {
        id: "ta1".into(), name: "T.A.".into(), source_division: "Hoofdwerk".into(),
        destination_division: "Hoofdwerk".into(), active: false, display_in_division: "Hoofdwerk".into(),
        midi_action_code: 0, coupler_type: "ta".into(), pitch_offset: 0, speciaal: false, cascade_toegestaan: true,
    });
    let (tx, rx) = bounded::<AudioCommand>(64);
    let mappings = vec![MidiChannelMapping { division: "Hoofdwerk".into(), channel: Some(1), ..Default::default() }];
    let held = vec![(1u8, 60u8, 100u8)];
    let zonder: Vec<String> = Vec::new();
    let met = ids(&["ta1"]);
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &held, &zonder, &met);
    assert_eq!(rx.try_iter().filter(|c| matches!(c, AudioCommand::ReleaseStop { stop_id: 20 })).count(), 1, "T.A. aan: tongwerk los");
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &held, &met, &zonder);
    assert_eq!(rx.try_iter().filter(|c| matches!(c, AudioCommand::NoteOn { stop_id: 20, .. })).count(), 1, "T.A. uit: tongwerk weer aan");
    // Zonder ingedrukte toetsen gebeurt er niets.
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &[], &zonder, &met);
    assert_eq!(rx.try_iter().count(), 0);
}

/// 0.9.12: een tongwerk dat onder T.A. wordt getrokken klinkt via géén
/// route — ook niet via het pedaalkoppel. Bij T.A.-uit start het precies
/// één keer (direct + koppel), zodat het pedaal-loslaten het weer uitzet.
#[test]
fn tongwerk_onder_ta_klinkt_ook_niet_via_koppel() {
    let mut o = organ_met_koppel();
    o.divisions[1].stops[0].is_reed = true;
    o.couplers.as_mut().unwrap().push(CouplerDto {
        id: "ta1".into(), name: "T.A.".into(), source_division: "Hoofdwerk".into(),
        destination_division: "Hoofdwerk".into(), active: false, display_in_division: "Hoofdwerk".into(),
        midi_action_code: 0, coupler_type: "ta".into(), pitch_offset: 0, speciaal: false, cascade_toegestaan: true,
    });
    let (tx, rx) = bounded::<AudioCommand>(64);
    let mappings = vec![
        MidiChannelMapping { division: "Pedaal".into(), channel: Some(0), ..Default::default() },
        MidiChannelMapping { division: "Hoofdwerk".into(), channel: Some(1), ..Default::default() },
    ];
    let held = vec![(0u8, 48u8, 100u8)]; // alleen een pedaaltoets
    let beide = ids(&["k", "ta1"]);
    let alleen_k = ids(&["k"]);
    let tel = |rx: &Receiver<AudioCommand>| rx.try_iter().filter(|c| matches!(c, AudioCommand::NoteOn { stop_id: 20, .. })).count();
    AppState::sync_stop_voices_inner(&o, &tx, &mappings, &beide, &beide, &held, "r", true);
    assert_eq!(tel(&rx), 0, "onder T.A. ook niet via het koppel");
    AppState::sync_coupler_voices_inner(&o, &tx, &mappings, &held, &beide, &alleen_k);
    assert_eq!(tel(&rx), 1, "T.A. uit: precies één keer via het koppel");
}

/// 0.9.12: bronnoten per route — solo-koppel alleen het uiterste en per
/// noot één keer (twee kanalen op één klavier met dezelfde toets).
#[test]
fn bron_noten_solo_een_per_noot() {
    let m = MidiChannelMapping { division: "Hoofdwerk".into(), channel: None, ..Default::default() };
    let held = vec![(1u8, 60u8, 100u8), (2u8, 60u8, 90u8), (1u8, 67u8, 80u8)];
    assert_eq!(bron_noten_voor_route("melody", &held, Some(&m)), vec![(67, 80)]);
    assert_eq!(bron_noten_voor_route("bass", &held, Some(&m)), vec![(60, 100)]);
    assert_eq!(bron_noten_voor_route("unison", &held, Some(&m)).len(), 3);
    assert!(bron_noten_voor_route("melody", &held, None).is_empty());
}

#[test]
fn trapformule_en_grenzen() {
    assert_eq!(crescendo_stage_for(0, 7), 0);
    assert_eq!(crescendo_stage_for(7, 7), 0);
    assert_eq!(crescendo_stage_for(8, 7), 1);
    assert_eq!(crescendo_stage_for(127, 7), 7);
    assert_eq!(crescendo_stage_for(64, 0), 0);
    assert_eq!(crescendo_stage_bounds(0, 7), (0, 7));
    assert_eq!(crescendo_stage_bounds(1, 7).0, 8);
    assert_eq!(crescendo_stage_bounds(7, 7).1, 127);
    // Trap 3 begint bij 42 (dode zone 8: (m-8)*7/119+1; zie meetplan-model).
    assert_eq!(crescendo_stage_for(41, 7), 2);
    assert_eq!(crescendo_stage_for(42, 7), 3);
    assert_eq!(crescendo_stage_bounds(3, 7).0, 42);
    assert_eq!(crescendo_stage_bounds(2, 7).1, 41);
}

#[test]
fn hysterese_klappert_niet_op_een_trapgrens() {
    let n = 7;
    // Omhoog pas H voorbij de bovengrens van trap 2 (41): 42 en 43 blijven staan.
    assert_eq!(crescendo_next_stage(42, 2, n), None);
    assert_eq!(crescendo_next_stage(43, 2, n), None);
    assert_eq!(crescendo_next_stage(44, 2, n), Some(3));
    // Omlaag pas H onder de ondergrens van trap 3 (42): 41 en 40 blijven staan.
    assert_eq!(crescendo_next_stage(41, 3, n), None);
    assert_eq!(crescendo_next_stage(40, 3, n), None);
    assert_eq!(crescendo_next_stage(39, 3, n), Some(2));
    // Ruis 41<->42 rond de grens: geen enkele wissel vanaf trap 2 of 3.
    for _ in 0..20 {
        assert_eq!(crescendo_next_stage(42, 2, n), None);
        assert_eq!(crescendo_next_stage(41, 2, n), None);
        assert_eq!(crescendo_next_stage(41, 3, n), None);
        assert_eq!(crescendo_next_stage(42, 3, n), None);
    }
    // Dode zone 8 + hysterese H=2 vanaf trap 0 (bovengrens 7): 9 blijft 0, 10 wordt 1.
    assert_eq!(crescendo_next_stage(7, 0, n), None);
    assert_eq!(crescendo_next_stage(9, 0, n), None);
    assert_eq!(crescendo_next_stage(10, 0, n), Some(1));
    // Volle pedaalweg en dode zone gelden altijd; sprongen over trappen mogen.
    assert_eq!(crescendo_next_stage(127, 1, n), Some(7));
    assert_eq!(crescendo_next_stage(3, 5, n), Some(0));
    assert_eq!(crescendo_next_stage(0, 1, n), Some(0));
    assert_eq!(crescendo_next_stage(4, 0, n), None);
    assert_eq!(crescendo_next_stage(6, 0, n), None); // dode zone 8
    assert_eq!(crescendo_next_stage(64, crescendo_stage_for(64, n), n), None);
    // Gekrompen matrix: huidige trap > N -> direct naar de doeltrap.
    assert_eq!(crescendo_next_stage(64, 12, n), Some(crescendo_stage_for(64, n)));
    // Gespiegeld/bereik: 110 in bereik 20..110 = vol, 20 = 0.
    assert_eq!(crescendo_mapped_value(110, 20, 110, false), 127);
    assert_eq!(crescendo_mapped_value(20, 20, 110, false), 0);
    assert_eq!(crescendo_mapped_value(127, 110, 20, true), 0);
}

#[test]
fn lege_trap_erft_lagere_gevulde_trap() {
    let stages = vec![ids(&["a"]), ids(&["a", "b"]), Vec::new(), Vec::new()];
    assert_eq!(crescendo_effective_stops(&stages, 0), Vec::<String>::new());
    assert_eq!(crescendo_effective_stops(&stages, 1), ids(&["a"]));
    assert_eq!(crescendo_effective_stops(&stages, 3), ids(&["a", "b"]));
    assert_eq!(crescendo_effective_stops(&stages, 4), ids(&["a", "b"]));
    assert_eq!(crescendo_effective_stops(&stages, 9), ids(&["a", "b"]));
}

#[test]
fn apply_diff_claimt_alleen_wat_de_trede_bijtrekt() {
    let stages = vec![ids(&["a"]), ids(&["a", "b", "real_coupler_1", "onbekend"]), ids(&["a", "b", "c", "real_coupler_1"])];
    let rig = Rig::new(stages, ids(&["c"])); // c handmatig getrokken
    let (added, removed) = rig.apply(2);
    assert_eq!(*rig.stage.read(), 2);
    assert!(removed.is_empty());
    let mut a = added.clone(); a.sort();
    assert_eq!(a, ids(&["a", "b", "real_coupler_1"]));
    assert_eq!(Rig::sorted(&rig.drawn), ids(&["a", "b", "c"]));
    assert_eq!(Rig::sorted(&rig.couplers), ids(&["real_coupler_1"]), "echte ODF-koppel gaat via lidmaatschap aan");
    assert_eq!(Rig::sorted(&rig.claims), ids(&["a", "b", "real_coupler_1"]));
    assert!(!rig.claims.read().iter().any(|s| s == "onbekend"), "onbekende IDs worden genegeerd");
    // DTO-vlaggen volgen.
    let o = rig.organ.read().clone().unwrap();
    assert!(o.divisions[0].stops.iter().find(|s| s.id == "a").unwrap().drawn);
    assert!(o.couplers.as_ref().unwrap()[0].active);

    // Trap 3: c stond al handmatig -> niet geclaimd.
    rig.apply(3);
    assert_eq!(Rig::sorted(&rig.claims), ids(&["a", "b", "real_coupler_1"]));

    // Terug naar 0: handregistratie c blijft, trede-registers en koppel weg.
    let (_, removed) = rig.apply(0);
    assert_eq!(removed.len(), 3);
    assert_eq!(Rig::sorted(&rig.drawn), ids(&["c"]));
    assert!(rig.couplers.read().is_empty());
    assert!(rig.claims.read().is_empty());
    assert_eq!(*rig.stage.read(), 0);

    // Clamp: trap 9 op een matrix van 3 = trap 3.
    rig.apply(9);
    assert_eq!(*rig.stage.read(), 3);
}

#[test]
fn handmatig_terugduwen_laat_claim_vallen() {
    let stages = vec![ids(&["a"]), ids(&["a", "b"])];
    let rig = Rig::new(stages, Vec::new());
    rig.apply(2);
    assert_eq!(Rig::sorted(&rig.claims), ids(&["a", "b"]));
    // Organist duwt a terug: claim weg (drawn zelf wordt door toggle_stop bijgewerkt).
    rig.drawn.write().retain(|s| s != "a");
    AppState::note_manual_change_inner(&rig.claims, "a");
    assert_eq!(Rig::sorted(&rig.claims), ids(&["b"]));
    // Bij opnieuw toepassen van trap 2 komt a als trede-register terug.
    let (added, _) = rig.apply(2);
    assert_eq!(added, ids(&["a"]));
    assert_eq!(Rig::sorted(&rig.claims), ids(&["a", "b"]));
    // Handmatig aanzetten van een geclaimd register -> geen claim meer, blijft staan bij trap 0.
    AppState::note_manual_change_inner(&rig.claims, "b");
    rig.apply(0);
    assert_eq!(Rig::sorted(&rig.drawn), ids(&["b"]));
}

#[test]
fn zwelstand_uit_cc() {
    assert_eq!(swell_gain_from_cc(0, 0, 127, false), 0.0);
    assert_eq!(swell_gain_from_cc(127, 0, 127, false), 1.0);
    assert!((swell_gain_from_cc(40, 0, 127, false) - 40.0 / 127.0).abs() < 1e-6);
    assert_eq!(swell_gain_from_cc(0, 0, 127, true), 1.0);
    assert_eq!(swell_gain_from_cc(19, 20, 110, false), 0.0);
    assert_eq!(swell_gain_from_cc(111, 20, 110, false), 1.0);
    assert!((swell_gain_from_cc(65, 20, 110, false) - 0.5).abs() < 1e-6);
    // min > max wordt geordend (vangnet), geen stapfunctie.
    assert!((swell_gain_from_cc(65, 110, 20, false) - 0.5).abs() < 1e-6);
}
// ---- Wederzijdse uitsluiting zwelkast <-> generaal crescendo ----
fn zwel(div: &str, ch: u8, cc: u8) -> SwellBinding {
    SwellBinding {
        division_name: div.to_string(), division_index: 0, channel: ch, cc_num: cc,
        min_val: 0, max_val: 127, invert: false, last_value: None,
    }
}

#[test]
fn zwelclaim_verdringt_crescendo_op_dezelfde_cc() {
    let cresc = Arc::new(RwLock::new(Some((0u8, 7u8, 0u8, 127u8, false))));
    let zwellen = Arc::new(RwLock::new(vec![zwel("Nevenwerk", 0, 11)]));
    // Andere CC: crescendo blijft staan.
    let v = claim_pedal_cc_inner(PedalCcKind::Swell, 0, 11, &cresc, &zwellen);
    assert!(v.is_empty());
    assert!(cresc.read().is_some());
    // Ander kanaal, zelfde CC-nummer: ook geen verdringing.
    let v = claim_pedal_cc_inner(PedalCcKind::Swell, 1, 7, &cresc, &zwellen);
    assert!(v.is_empty());
    assert!(cresc.read().is_some());
    // Zelfde (kanaal, CC): crescendo-koppeling vervalt en wordt gemeld.
    let v = claim_pedal_cc_inner(PedalCcKind::Swell, 0, 7, &cresc, &zwellen);
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].kind, "crescendo");
    assert_eq!((v[0].channel, v[0].cc), (0, 7));
    assert!(v[0].division.is_none());
    assert!(cresc.read().is_none());
    // Zwelbindingen blijven onaangeroerd.
    assert_eq!(zwellen.read().len(), 1);
}

#[test]
fn crescendoclaim_verdringt_alle_zwellen_op_dezelfde_cc() {
    let cresc: Arc<RwLock<Option<(u8, u8, u8, u8, bool)>>> = Arc::new(RwLock::new(None));
    let zwellen = Arc::new(RwLock::new(vec![
        zwel("Nevenwerk", 0, 7),
        zwel("Bovenwerk", 0, 7),   // zelfde trede voor twee kasten
        zwel("Rugwerk", 1, 7),     // ander kanaal
        zwel("Borstwerk", 0, 11),  // andere CC
    ]));
    let v = claim_pedal_cc_inner(PedalCcKind::Crescendo, 0, 7, &cresc, &zwellen);
    assert_eq!(v.len(), 2);
    assert!(v.iter().all(|d| d.kind == "swell" && d.channel == 0 && d.cc == 7));
    let overgebleven: Vec<String> = zwellen.read().iter().map(|b| b.division_name.clone()).collect();
    assert_eq!(overgebleven, vec!["Rugwerk".to_string(), "Borstwerk".to_string()]);
}

#[test]
fn beide_functies_op_verschillende_cc_blijven_naast_elkaar() {
    // Het normale geval uit de klacht: zwel op CC11, crescendo op CC7 - geen
    // van beide mag de ander verdringen, in welke volgorde ook.
    let cresc = Arc::new(RwLock::new(Some((0u8, 7u8, 0u8, 127u8, false))));
    let zwellen = Arc::new(RwLock::new(vec![zwel("Nevenwerk", 0, 11)]));
    assert!(claim_pedal_cc_inner(PedalCcKind::Crescendo, 0, 7, &cresc, &zwellen).is_empty());
    assert!(claim_pedal_cc_inner(PedalCcKind::Swell, 0, 11, &cresc, &zwellen).is_empty());
    assert!(cresc.read().is_some());
    assert_eq!(zwellen.read().len(), 1);
}
