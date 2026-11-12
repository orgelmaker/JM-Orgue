use super::*;
use std::sync::Weak;

/// Sample zoals de laadpijplijn hem aflevert (1 s mono op uitgangsrate).
fn sample() -> SampleRef {
    Arc::new(SampleData {
        data: vec![0.5; 48_000].into(),
        right: None,
        sample_rate: 48_000,
        channels: 1,
        loop_start: None,
        loop_end: None,
    })
}

/// Uitgeklonken stem op `sample`: losgelaten en onder de stiltedrempel,
/// dus `is_finished()` — precies wat de verwijderstap opruimt.
fn uitgeklonken(sample: SampleRef, pipe: u32) -> PlayingVoice {
    let mut v = PlayingVoice::new_from_sample(sample, 1, pipe, 60, 1.0);
    v.releasing = true;
    v.envelope = 0.0;
    v.age_samples = 48_000;
    v
}

/// Klinkende stem (niet losgelaten, vol niveau, ouder dan 20 ms).
fn klinkend(sample: SampleRef, pipe: u32) -> PlayingVoice {
    let mut v = PlayingVoice::new_from_sample(sample, 1, pipe, 60, 1.0);
    v.envelope = 1.0;
    v.age_samples = 48_000;
    v
}

/// Afvoerluik met een eigen kanaal, zodat de test kan zien wat de janitor
/// zou krijgen.
fn luik(kanaal: usize) -> (GcSink, Receiver<GcItem>) {
    let (tx, rx) = bounded::<GcItem>(kanaal);
    (GcSink::new(tx), rx)
}

/// De stem is de enige houder (de map heeft de sample geëvict; de
/// decode_cache heeft alleen een Weak): de Arc moet de verwijderstap
/// OVERLEVEN en via het gc-kanaal aankomen; pas het droppen van dat item
/// — het werk van de janitor-thread — geeft de sample vrij.
#[test]
fn laatste_houder_gaat_via_het_gc_kanaal_niet_via_de_verwijderstap() {
    let s = sample();
    let zwak: Weak<SampleData> = Arc::downgrade(&s);
    let mut voices = vec![uitgeklonken(s, 100)];
    assert_eq!(zwak.strong_count(), 1, "opzet: de stem hoort de enige houder te zijn");
    let (mut gc, rx) = luik(8);

    remove_finished_voices(&mut voices, &mut gc);

    assert!(voices.is_empty(), "uitgeklonken stem niet verwijderd");
    assert_eq!(zwak.strong_count(), 1, "de sample werd ín de verwijderstap gedropt");
    assert!(gc.backlog.is_empty(), "kanaal had ruimte; niets hoort in de wachtrij");
    let item = rx.try_recv().expect("geen item op het gc-kanaal");
    match &item {
        GcItem::Sample(ontvangen) => {
            let nog = zwak.upgrade().expect("sample al weg");
            assert!(Arc::ptr_eq(&nog, ontvangen), "ander object op het kanaal");
        }
        _ => panic!("verkeerd item-type op het gc-kanaal (verwacht GcItem::Sample)"),
    }
    assert!(rx.try_recv().is_err(), "meer dan één item verstuurd");
    drop(item); // wat de janitor-thread doet
    assert_eq!(zwak.strong_count(), 0, "na de janitor-drop hoort de sample vrij te zijn");
}

/// Gedeelde bron — nog in de samples-map én bij een andere (klinkende)
/// stem: de eindigende stem laat alleen zijn teller los, er gaat niets
/// over het kanaal, en de overblijvende stemmen houden hun volgorde.
#[test]
fn gedeelde_bron_wordt_gewoon_gedropt_en_volgorde_blijft() {
    let s = sample();
    let mut map: HashMap<(u32, u32), SampleRef> = HashMap::new();
    map.insert((1, 100), s.clone());
    let mut voices = vec![
        uitgeklonken(s.clone(), 100),
        klinkend(s.clone(), 101),
        uitgeklonken(s.clone(), 100),
        klinkend(s.clone(), 102),
    ];
    assert_eq!(Arc::strong_count(&s), 6); // s + map + 4 stemmen
    let (mut gc, rx) = luik(8);

    remove_finished_voices(&mut voices, &mut gc);

    assert_eq!(Arc::strong_count(&s), 4, "tellers niet losgelaten: s + map + 2 klinkende");
    assert!(rx.try_recv().is_err(), "gedeelde bron hoort NIET naar de janitor");
    assert!(gc.backlog.is_empty());
    let over: Vec<u32> = voices.iter().map(|v| v.pipe_num).collect();
    assert_eq!(over, vec![101, 102], "klinkende stemmen weg of van volgorde veranderd");

    // Ook wanneer alleen een ANDERE stem hem nog vasthoudt (geen map):
    // koppel + directe aanslag op dezelfde pijp, één ervan klinkt uit.
    let t = sample();
    let zwak_t = Arc::downgrade(&t);
    voices.push(uitgeklonken(t.clone(), 104));
    voices.push(klinkend(t, 103));
    assert_eq!(zwak_t.strong_count(), 2);
    remove_finished_voices(&mut voices, &mut gc);
    assert!(rx.try_recv().is_err(), "bron die een andere stem nog vasthoudt ging naar de janitor");
    assert_eq!(zwak_t.strong_count(), 1, "teller van de uitgeklonken stem niet losgelaten");
    let over: Vec<u32> = voices.iter().map(|v| v.pipe_num).collect();
    assert_eq!(over, vec![101, 102, 103]);
}

/// Voice-stealing bij een volle kap: het direct verwijderde slachtoffer
/// (bijna stille staart, enige houder van zijn geëvicte sample) gaat óók
/// langs de janitor in plaats van in `swap_remove` te vallen.
#[test]
fn gestolen_stem_bij_volle_kap_gaat_ook_langs_de_janitor() {
    let cap = MIN_LIVE_VOICES;
    let gedeeld = sample();
    let mut voices: Vec<PlayingVoice> =
        (0..cap - 1).map(|i| klinkend(gedeeld.clone(), i as u32)).collect();
    let eigen = sample();
    let zwak = Arc::downgrade(&eigen);
    let mut staart = PlayingVoice::new_from_sample(eigen, 1, 999, 60, 1.0);
    staart.releasing = true;
    staart.envelope = 0.005; // < 0,01: mag hard weg
    staart.age_samples = 48_000;
    voices.push(staart);
    assert_eq!(voices.len(), cap);
    let (mut gc, rx) = luik(8);

    push_voice_capped(&mut voices, klinkend(gedeeld.clone(), 5000), 48_000, cap, &mut gc);

    assert_eq!(voices.len(), cap, "slachtoffer niet verwijderd of nieuwe stem niet toegevoegd");
    assert!(voices.iter().all(|v| v.pipe_num != 999), "niet de stille staart gestolen");
    assert!(voices.iter().any(|v| v.pipe_num == 5000), "nieuwe stem ontbreekt");
    assert_eq!(zwak.strong_count(), 1, "sample van de gestolen stem viel in de callback");
    let item = rx.try_recv();
    assert!(matches!(item, Ok(GcItem::Sample(_))), "gestolen sample niet op het gc-kanaal");
    drop(item);
    assert_eq!(zwak.strong_count(), 0);
}

/// Vol kanaal: het tweede item wordt in de wachtrij geparkeerd — nooit
/// inline gedropt — en schuift bij de volgende callback alsnog door.
#[test]
fn vol_kanaal_parkeert_in_de_wachtrij_en_dropt_niets_inline() {
    let (mut gc, rx) = luik(1);
    let (a, b) = (sample(), sample());
    let (za, zb) = (Arc::downgrade(&a), Arc::downgrade(&b));
    let mut voices = vec![uitgeklonken(a, 100), uitgeklonken(b, 101)];

    remove_finished_voices(&mut voices, &mut gc);

    assert!(voices.is_empty());
    assert_eq!(za.strong_count() + zb.strong_count(), 2, "een sample viel inline");
    assert_eq!(gc.backlog.len(), 1, "tweede item niet in de wachtrij");
    // De janitor haalt het kanaal leeg; de volgende callback schuift de
    // wachtrij door.
    drop(rx.try_recv().expect("kanaal leeg"));
    gc.flush_backlog();
    assert!(gc.backlog.is_empty(), "wachtrij niet doorgeschoven");
    drop(rx.try_recv().expect("doorgeschoven item niet op het kanaal"));
    assert_eq!(za.strong_count() + zb.strong_count(), 0, "niet alles vrijgegeven");
}

/// Een stem op zijn preload-buffer: zelfde regel (laatste houder → janitor,
/// als `GcItem::Preload`).
#[test]
fn preload_bron_volgt_dezelfde_regel() {
    let pb = Arc::new(PreloadBuffer {
        attack_data: vec![0.25; 4_800].into(),
        attack_right: None,
        sample_rate: 48_000,
        total_samples: 4_800,
        loop_start: None,
        loop_end: None,
        source_path: std::sync::Arc::from(std::path::Path::new("pijp.wav")),
        channels: 1,
        bits_per_sample: 16,
        data_offset: 0,
        align_table: None,
        trim_start: 0,
        smpl_unity_note: None,
        smpl_pitch_fraction_cents: None,            harm_shelf: None,
    });
    let zwak = Arc::downgrade(&pb);
    let mut v = PlayingVoice::new_from_preload(pb, 1, 100, 60, 1.0, 48_000);
    v.releasing = true;
    v.envelope = 0.0;
    let mut voices = vec![v];
    let (mut gc, rx) = luik(8);

    remove_finished_voices(&mut voices, &mut gc);

    assert!(voices.is_empty());
    assert_eq!(zwak.strong_count(), 1, "preload viel in de verwijderstap");
    let item = rx.try_recv();
    assert!(matches!(item, Ok(GcItem::Preload(_))), "preload niet als GcItem::Preload verstuurd");
    drop(item);
    assert_eq!(zwak.strong_count(), 0);
}
