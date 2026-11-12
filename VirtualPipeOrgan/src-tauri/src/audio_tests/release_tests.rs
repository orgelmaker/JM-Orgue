use super::*;

/// Constante sample: `read_voice` levert dan exact de envelope terug, zodat
/// een test het NIVEAUVERLOOP meet en niet de golfvorm.
fn vlakke_sample(frames: usize) -> SampleRef {
    std::sync::Arc::new(SampleData {
        data: vec![1.0; frames].into(),
        right: None,
        sample_rate: 48_000,
        channels: 1,
        loop_start: None,
        loop_end: None,
    })
}

/// Natuurlijke release-staart: speelt op zijn startniveau door (de afname
/// zit in de sampledata), dus envelope == target_envelope.
fn staart(age_samples: u64) -> PlayingVoice {
    let mut v = PlayingVoice::new_from_sample(
        vlakke_sample(48_000), 1, 100 | RELEASE_PIPE_FLAG, 60, 1.0,
    );
    v.one_shot = true;
    v.envelope = 1.0;
    v.target_envelope = 1.0;
    v.age_samples = age_samples;
    v
}

/// Het staartbudget mag NOOIT de zojuist gespawnde release stelen (die
/// staat nog op envelope 0 door de crossfade-infade); het slachtoffer is de
/// oudste natuurlijke staart.
#[test]
fn staartbudget_spaart_verse_crossfade_en_kiest_de_oudste() {
    let cap = 1024usize;
    let budget = (cap / 3).max(MAX_RELEASE_VOICES);
    let mut voices: Vec<PlayingVoice> =
        (0..budget).map(|i| staart(48_000 + i as u64)).collect();
    let mut vers = staart(0);
    crossfade_in(&mut vers, 1.0, release_fade_ms(36), 48_000);
    voices.push(vers);

    budget_release_voices(&mut voices, 48_000, cap);

    assert!(!voices[budget].releasing, "de verse crossfade-release werd gestolen");
    assert!(voices[budget].target_envelope > 0.0, "verse release kreeg doel 0");
    let slachtoffers: Vec<usize> = voices.iter().enumerate()
        .filter(|(_, v)| v.releasing).map(|(i, _)| i).collect();
    assert_eq!(slachtoffers.len(), 1, "verwacht precies één slachtoffer, kreeg {:?}", slachtoffers);
    assert_eq!(slachtoffers[0], budget - 1, "niet de OUDSTE staart gekozen");
}

/// Het staartbudget staat LOS van de kap (0.9.28): ook met de kap op het
/// maximum (32768) blijven release-staarten begrensd op
/// MAX_RELEASE_VOICE_BUDGET. Vóór de fix was het budget `cap/3` (~10900),
/// dus bleef er geen begrenzing over en stapelden natte release-staarten
/// ongeremd op tot gekraak. Played-stemmen (de kap zelf) blijven wél vrij.
#[test]
fn staartbudget_blijft_begrensd_bij_de_maximale_kap() {
    let cap = MAX_LIVE_VOICES; // 32768
    // Precies op de vaste bovengrens: onder de oude cap/3-formule (10922)
    // zou dit niets stelen; onder de afgetopte grens moet de oudste wijken.
    assert_eq!((cap / 3).max(MAX_RELEASE_VOICES).min(MAX_RELEASE_VOICE_BUDGET),
               MAX_RELEASE_VOICE_BUDGET, "budget niet afgetopt op de vaste grens");
    let mut voices: Vec<PlayingVoice> =
        (0..MAX_RELEASE_VOICE_BUDGET).map(|i| staart(48_000 + i as u64)).collect();

    budget_release_voices(&mut voices, 48_000, cap);

    let slachtoffers: Vec<usize> = voices.iter().enumerate()
        .filter(|(_, v)| v.releasing).map(|(i, _)| i).collect();
    assert_eq!(slachtoffers.len(), 1,
               "verwacht precies één onhoorbaar slachtoffer bij een volle staart-grens, kreeg {:?}", slachtoffers);
    assert_eq!(slachtoffers[0], MAX_RELEASE_VOICE_BUDGET - 1, "niet de OUDSTE staart gekozen");
}

/// Ook een release die nog aan zijn uitgestelde staart-decay moet beginnen
/// is beschermd — anders komt de "kettingmoord" via de achterdeur terug.
#[test]
fn staartbudget_spaart_ook_een_release_met_uitgestelde_decay() {
    let cap = 1024usize;
    let budget = (cap / 3).max(MAX_RELEASE_VOICES);
    let mut voices: Vec<PlayingVoice> =
        (0..budget).map(|i| staart(48_000 + i as u64)).collect();
    let mut vers = staart(0);
    crossfade_in_with_decay(&mut vers, 1.0, release_fade_ms(36), 1200.0, 48_000);
    // Doe alsof de infade al klaar is: zonder de pending-decay-guard zou
    // deze verse staart nu kandidaat zijn (age 0 beschermt hem niet meer).
    vers.envelope = vers.target_envelope;
    vers.age_samples = 48_000 * 2;
    voices.push(vers);

    budget_release_voices(&mut voices, 48_000, cap);

    assert!(!voices[budget].releasing, "release met uitgestelde decay werd gestolen");
}

/// Een tremulantwissel op vol werk: elke klinkende pijp krijgt er een stem
/// bij terwijl de oude uitfadet. Die burst moet langs het STAARTBUDGET —
/// vóór elke push één keer — anders groeit het stemmenaantal ongeremd
/// terwijl de staarten al op hun grens staan. De teruggegeven telling is
/// tegelijk het werk dat de callback op zijn spawn-budget boekt.
#[test]
fn tremulantburst_gaat_langs_het_staartbudget_en_telt_het_werk() {
    let cap = 1024usize;
    let budget = (cap / 3).max(MAX_RELEASE_VOICES);
    // Staarten precies op het budget: elke extra stem moet er nu één
    // onhoorbaar laten plaatsmaken.
    let mut voices: Vec<PlayingVoice> =
        (0..budget).map(|i| staart(48_000 + i as u64)).collect();

    // Vier nieuwe crossfade-stemmen zoals SetTremulant ze maakt (klinkende
    // pijpen, dus géén one-shot staarten).
    let mut nieuw: Vec<PlayingVoice> = (0..4)
        .map(|_| {
            let mut v = PlayingVoice::new_from_sample(vlakke_sample(48_000), 1, 100, 60, 1.0);
            v.envelope = 1.0;
            v.target_envelope = 1.0;
            v
        })
        .collect();

    let (gc_tx, _gc_rx) = bounded::<GcItem>(8);
    let mut gc = GcSink::new(gc_tx);
    let gespawnd = push_voices_budgeted(&mut voices, &mut nieuw, 48_000, cap, &mut gc);

    assert_eq!(gespawnd, 4, "werkeenheden niet teruggegeven");
    assert_eq!(voices.len(), budget + 4, "stemmen niet toegevoegd");
    let uitfadend = voices.iter().filter(|v| v.one_shot && v.releasing).count();
    assert_eq!(uitfadend, 4, "het staartbudget liep niet mee met elke push (kreeg {})", uitfadend);
    assert!(
        voices[budget..].iter().all(|v| !v.releasing),
        "een zojuist toegevoegde crossfade-stem werd meteen weer losgelaten"
    );
}

/// Zijn álle staarten net gespawnd (< 20 ms), dan wordt er niets gestolen:
/// liever even boven het budget dan een akkoord zijn hele nagalm afnemen.
#[test]
fn staartbudget_laat_alles_met_rust_als_alle_staarten_vers_zijn() {
    let cap = 1024usize;
    let budget = (cap / 3).max(MAX_RELEASE_VOICES);
    let mut voices: Vec<PlayingVoice> = (0..=budget).map(|_| {
        let mut v = staart(0);
        crossfade_in(&mut v, 1.0, release_fade_ms(36), 48_000);
        v
    }).collect();

    budget_release_voices(&mut voices, 48_000, cap);

    assert!(voices.iter().all(|v| !v.releasing), "er werd toch een verse staart gestolen");
}

/// Korte noot: de som van de uitfadende speelnoot en de inkomende release
/// blijft ≈ constant (geen niveaubult), en de staart dooft daarna netjes
/// uit op 0.
#[test]
fn korte_noot_houdt_niveau_constant_en_dooft_uit() {
    let sr = 48_000u32;
    let midi = 36u8;
    let fade = release_fade_ms(midi); // 184 ms in de bas
    let decay_ms = 1200.0f32;

    let mut speel = PlayingVoice::new_from_sample(vlakke_sample(480_000), 1, 100, midi, 1.0);
    speel.envelope = 1.0;
    speel.target_envelope = 1.0;
    speel.release_ms(fade, sr);

    let mut rel = PlayingVoice::new_from_sample(
        vlakke_sample(480_000), 1, 100 | RELEASE_PIPE_FLAG, midi, 1.0,
    );
    rel.one_shot = true;
    crossfade_in_with_decay(&mut rel, 1.0, fade, decay_ms, sr);

    let fade_frames = (fade * 0.001 * sr as f32) as usize;
    let mut max_som = 0.0f32;
    for _ in 0..fade_frames {
        let (a, _) = speel.next_sample(false);
        let (b, _) = rel.next_sample(false);
        max_som = max_som.max(a + b);
    }
    assert!(max_som <= 1.02,
            "niveaubult bij het loslaten: {:.3} (instant-start gaf ~2,0)", max_som);
    assert!(rel.pending_decay_frames.is_none(), "de uitgestelde decay is niet gestart");
    assert!(rel.releasing, "de staart is na de infade niet gaan uitsterven");

    let rest = (decay_ms * 0.001 * sr as f32) as usize + fade_frames;
    for _ in 0..rest {
        if rel.is_finished() { break; }
        rel.next_sample(false);
    }
    assert!(rel.is_finished(), "staart dooft niet uit (envelope {:.5})", rel.envelope);
}

/// Controle op dezelfde opstelling: het OUDE gedrag (release instant op vol
/// niveau) geeft wél een bult — bewijst dat de test hem zou zien.
#[test]
fn instant_start_release_geeft_wel_een_bult() {
    let sr = 48_000u32;
    let midi = 36u8;
    let fade = release_fade_ms(midi);

    let mut speel = PlayingVoice::new_from_sample(vlakke_sample(480_000), 1, 100, midi, 1.0);
    speel.envelope = 1.0;
    speel.target_envelope = 1.0;
    speel.release_ms(fade, sr);

    let mut rel = PlayingVoice::new_from_sample(
        vlakke_sample(480_000), 1, 100 | RELEASE_PIPE_FLAG, midi, 1.0,
    );
    rel.one_shot = true;
    rel.envelope = 1.0;
    rel.target_envelope = 1.0;
    rel.release_ms(1200.0, sr);

    let mut max_som = 0.0f32;
    for _ in 0..(fade * 0.001 * sr as f32) as usize {
        let (a, _) = speel.next_sample(false);
        let (b, _) = rel.next_sample(false);
        max_som = max_som.max(a + b);
    }
    assert!(max_som > 1.5, "verwachtte een niveaubult, kreeg {:.3}", max_som);
}

/// Landt de volledige sample halverwege de decay, dan mag de staart niet
/// blijven hangen op de (tot de preload-rand ingekorte) snelheid: de
/// resterende decay wordt herberekend en de staart dooft uit op tijd.
#[test]
fn upgrade_to_full_herberekent_de_resterende_decay() {
    let _sr = 48_000u32;
    let mut rel = PlayingVoice::new_from_sample(
        vlakke_sample(480_000), 1, 100 | RELEASE_PIPE_FLAG, 48, 1.0,
    );
    rel.one_shot = true;
    rel.envelope = 1.0;
    rel.target_envelope = 1.0;
    rel.releasing = true;
    rel.age_samples = 0;
    rel.decay_end_age = Some(240_000); // 5 s decay
    // Doe alsof de fade was ingekort tot de preload-rand (~1,8 s).
    rel.envelope_speed = 1.0 / 86_400.0;

    rel.upgrade_to_full(vlakke_sample(480_000));

    let verwacht = 1.0f32 / 240_000.0;
    assert!((rel.envelope_speed - verwacht).abs() < verwacht * 0.05,
            "fadesnelheid niet hersteld: {} (verwacht ~{})", rel.envelope_speed, verwacht);
}

// ---------------------------------------------------------------------------
// Stemroof bij een volle kap (D7): de volgorde van `pick_victim` vastgelegd.
// ---------------------------------------------------------------------------

/// Klinkende of uitklinkende (losgelaten) pijp, geen staart.
fn pijp_voor_stemroof(pipe: u32, age_samples: u64, releasing: bool, envelope: f32) -> PlayingVoice {
    let mut v = PlayingVoice::new_from_sample(vlakke_sample(48_000), 1, pipe, 60, 1.0);
    v.age_samples = age_samples;
    v.releasing = releasing;
    v.envelope = envelope;
    v.target_envelope = if releasing { 0.0 } else { 1.0 };
    v
}

/// Staart (one-shot) in een gekozen toestand.
fn staart_voor_stemroof(pipe: u32, age_samples: u64, releasing: bool, envelope: f32) -> PlayingVoice {
    let mut v = staart(age_samples);
    v.pipe_num = pipe | RELEASE_PIPE_FLAG;
    v.releasing = releasing;
    v.envelope = envelope;
    v.target_envelope = if releasing { 0.0 } else { 1.0 };
    v
}

/// De slachtoffers komen in oplopende hoorbaarheid: (a) uitfadende staarten,
/// stilste eerst; (b) overige staarten, oudste eerst; (c) uitklinkende pijpen,
/// stilste eerst; (d) klinkende pijpen, oudste eerst. Door telkens het
/// slachtoffer te verwijderen en opnieuw te kiezen volgt de hele rij.
#[test]
fn stemroof_volgorde_staarten_dan_uitklinkende_pijpen_dan_oudste_klinkende() {
    let oud = 96_000u64;
    let mut voices = vec![
        pijp_voor_stemroof(1, oud * 3, false, 1.0),            // d: oudste klinkende
        pijp_voor_stemroof(2, oud, false, 1.0),                // d: jongere klinkende
        pijp_voor_stemroof(3, oud * 2, true, 0.50),            // c: uitklinkend, luider
        pijp_voor_stemroof(4, oud * 2, true, 0.20),            // c: uitklinkend, stiller
        staart_voor_stemroof(5, oud, false, 1.0),              // b: staart, jonger
        staart_voor_stemroof(6, oud * 2, false, 1.0),          // b: staart, ouder
        staart_voor_stemroof(7, oud, true, 0.60),              // a: uitfadend, luider
        staart_voor_stemroof(8, oud, true, 0.05),              // a: uitfadend, stilste
    ];
    let min_age = 48_000u64 / 50;
    let verwacht = [
        (8u32 | RELEASE_PIPE_FLAG), (7 | RELEASE_PIPE_FLAG),   // a
        (6 | RELEASE_PIPE_FLAG), (5 | RELEASE_PIPE_FLAG),      // b
        4, 3,                                                  // c
        1, 2,                                                  // d
    ];
    for (stap, want) in verwacht.iter().enumerate() {
        let i = pick_victim(&voices, min_age).expect("er is nog een slachtoffer");
        assert_eq!(voices[i].pipe_num, *want, "stap {stap}: verkeerd slachtoffer");
        voices.swap_remove(i);
    }
    assert!(pick_victim(&voices, min_age).is_none());
}

/// Een net gestarte stem (< 20 ms) is nooit slachtoffer, ook niet als hij een
/// staart is die verder vooraan in de volgorde staat. Zijn álle stemmen zo
/// jong, dan kiest de aanroeper (`push_voice_capped`) met leeftijd 0 alsnog,
/// omdat de kap hard blijft.
#[test]
fn stemroof_spaart_net_gestarte_stemmen_tenzij_alles_jong_is() {
    let min_age = 48_000u64 / 50; // 960 samples = 20 ms
    let voices = vec![
        staart_voor_stemroof(1, min_age - 1, false, 1.0),  // verse staart: klasse b maar te jong
        pijp_voor_stemroof(2, 48_000, false, 1.0),         // oude klinkende pijp: klasse d
    ];
    let i = pick_victim(&voices, min_age).unwrap();
    assert_eq!(voices[i].pipe_num, 2, "de verse staart mocht niet gekozen worden");

    let alleen_jong = vec![
        pijp_voor_stemroof(3, 10, false, 1.0),
        pijp_voor_stemroof(4, 500, false, 1.0),
    ];
    assert!(pick_victim(&alleen_jong, min_age).is_none(), "met de jongerengrens is niemand te stelen");
    let i = pick_victim(&alleen_jong, 0).unwrap();
    assert_eq!(alleen_jong[i].pipe_num, 4, "zonder grens de oudste");
}

/// Bij een volle kap krijgt een hoorbaar slachtoffer een korte fade (20 ms voor
/// een pijp, 30 ms voor een staart) in plaats van een harde knip, en de nieuwe
/// stem komt er gewoon bij.
#[test]
fn stemroof_bij_volle_kap_fadet_het_slachtoffer_en_voegt_de_nieuwe_toe() {
    let cap = MIN_LIVE_VOICES;
    let mut voices: Vec<PlayingVoice> = (0..cap)
        .map(|i| pijp_voor_stemroof(10 + i as u32, 48_000 + i as u64, false, 1.0))
        .collect();
    let (tx, _rx) = bounded::<GcItem>(8);
    let mut gc = GcSink::new(tx);
    let nieuw = pijp_voor_stemroof(9_999, 0, false, 0.0);
    push_voice_capped(&mut voices, nieuw, 48_000, cap, &mut gc);
    assert_eq!(voices.len(), cap + 1, "de nieuwe stem hoort erbij te komen");
    let fadend: Vec<u32> = voices.iter().filter(|v| v.releasing).map(|v| v.pipe_num).collect();
    assert_eq!(fadend, vec![10 + (cap as u32 - 1)], "alleen de oudste klinkende pijp fadet weg");
    assert!(voices.iter().any(|v| v.pipe_num == 9_999 && !v.releasing), "de nieuwe stem mag niet zelf weg");
}
