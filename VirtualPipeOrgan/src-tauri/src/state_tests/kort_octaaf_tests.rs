use super::*;

fn klavier(kort: bool, laagste: Option<u8>, transpose: i8) -> MidiChannelMapping {
    MidiChannelMapping {
        division: "Hoofdwerk".into(),
        channel: Some(0),
        transpose,
        first_midi_note: laagste,
        last_midi_note: Some(96),
        short_octave: kort,
    }
}

// MIDI-noten van de onderste octaaf van een klavier dat op C (36) begint.
const C: u8 = 36;
const CIS: u8 = 37;
const D: u8 = 38;
const DIS: u8 = 39;
const E: u8 = 40;
const F: u8 = 41;
const FIS: u8 = 42;
const G: u8 = 43;
const GIS: u8 = 44;
const A: u8 = 45;
const BES: u8 = 46;
const B: u8 = 47;

#[test]
fn uit_verandert_niets() {
    let m = klavier(false, Some(C), 0);
    for n in 36..=60u8 {
        assert_eq!(m.speelnoot(n), n);
        assert!(m.accepts(0, n));
    }
}

/// De melding van een gebruiker (november 2026): op een gewoon klavier hoort de
/// E-toets als C te klinken, Fis als D en Gis als E, en de rest gewoon.
#[test]
fn gewoon_klavier_e_fis_gis_klinken_als_c_d_e() {
    let m = klavier(true, Some(C), 0);
    assert_eq!(m.speelnoot(E), C, "E-toets klinkt als C");
    assert_eq!(m.speelnoot(FIS), D, "Fis-toets klinkt als D");
    assert_eq!(m.speelnoot(GIS), E, "Gis-toets klinkt als E");
    for n in [F, G, A, BES, B] {
        assert_eq!(m.speelnoot(n), n, "toets {} klinkt gewoon", n);
    }
    // De toonladder van C op de toetsen van het echte instrument: E Fis Gis F G A B c.
    let toetsen = [E, FIS, GIS, F, G, A, B, 48];
    let klinkt: Vec<u8> = toetsen.iter().map(|t| m.speelnoot(*t)).collect();
    assert_eq!(klinkt, [36, 38, 40, 41, 43, 45, 47, 48]);
}

#[test]
fn boven_het_onderste_octaaf_blijft_alles_zoals_het_is() {
    let m = klavier(true, Some(C), 0);
    for n in 48..=96u8 {
        assert_eq!(m.speelnoot(n), n, "toets {}", n);
        assert!(m.accepts(0, n), "toets {} speelt", n);
    }
    // Ook E, Fis en Gis een octaaf hoger: alleen de onderste octaaf is kort.
    assert_eq!(m.speelnoot(E + 12), E + 12);
    assert_eq!(m.speelnoot(FIS + 12), FIS + 12);
}

#[test]
fn c_cis_d_en_dis_van_het_onderste_octaaf_doen_niets() {
    let m = klavier(true, Some(C), 0);
    for n in [C, CIS, D, DIS] {
        assert!(m.kort_octaaf_stil(n), "toets {} bestaat niet", n);
        assert!(!m.accepts(0, n), "toets {} wordt niet aangenomen", n);
    }
    for n in [E, F, FIS, G, GIS, A, BES, B, 48, 49, 50, 51] {
        assert!(!m.kort_octaaf_stil(n), "toets {} bestaat wel", n);
        assert!(m.accepts(0, n), "toets {} speelt", n);
    }
    // Uit: dezelfde toetsen spelen gewoon.
    let uit = klavier(false, Some(C), 0);
    for n in [C, CIS, D, DIS] {
        assert!(uit.accepts(0, n));
    }
}

/// Een echt kort-octaafklavier begint op de E-toets (stuurt 40): dezelfde omzetting.
#[test]
fn klavier_dat_op_e_begint() {
    let m = klavier(true, Some(E), 0);
    assert_eq!(m.speelnoot(E), C);
    assert_eq!(m.speelnoot(FIS), D);
    assert_eq!(m.speelnoot(GIS), E);
    assert_eq!(m.speelnoot(F), F);
    assert_eq!(m.speelnoot(48), 48);
    assert!(m.accepts(0, E));
    // Onder de laagste toets nam het klavier al niets aan.
    assert!(!m.accepts(0, DIS));
}

/// Een klavier dat een octaaf lager begint (C1 = 24): de onderste octaaf van dát klavier is kort.
#[test]
fn het_onderste_octaaf_volgt_de_laagste_toets() {
    let m = klavier(true, Some(24), 0);
    assert_eq!(m.speelnoot(28), 24);
    assert_eq!(m.speelnoot(30), 26);
    assert_eq!(m.speelnoot(32), 28);
    assert!(!m.accepts(0, 25));
    // De octaaf erboven (C2) is gewoon.
    for n in 36..48u8 {
        assert_eq!(m.speelnoot(n), n);
        assert!(m.accepts(0, n));
    }
}

#[test]
fn zonder_ingeleerd_bereik_doet_de_vlag_niets() {
    // We weten dan niet waar de onderste octaaf begint; liever niets dan de
    // verkeerde noot.
    let m = klavier(true, None, 0);
    for n in 36..=60u8 {
        assert_eq!(m.speelnoot(n), n);
        assert!(m.accepts(0, n));
    }
}

#[test]
fn transponeren_komt_er_bovenop() {
    let m = klavier(true, Some(C), 12);
    assert_eq!(m.speelnoot(E), C + 12); // E → C, een octaaf hoger
    assert_eq!(m.speelnoot(FIS), D + 12); // Fis → D, een octaaf hoger
    assert_eq!(m.speelnoot(F), F + 12);
}

/// Aanslaan en loslaten rekenen dezelfde noot uit, en een stille toets wordt bij
/// allebei geweigerd: anders blijft er een noot hangen.
#[test]
fn aanslag_en_loslaten_zijn_gelijk() {
    let m = klavier(true, Some(C), -2);
    for n in 0..=127u8 {
        assert_eq!(m.speelnoot(n), m.speelnoot(n));
        assert_eq!(m.accepts(0, n), m.accepts(0, n));
        if m.kort_octaaf_stil(n) {
            assert!(!m.accepts(0, n));
        }
    }
}

#[test]
fn randen_van_het_midi_bereik_lopen_niet_over() {
    // Laagste toets helemaal bovenaan: er past geen E meer boven 127.
    let m = klavier(true, Some(126), 0);
    for n in 120..=127u8 {
        assert_eq!(m.speelnoot(n), n);
    }
    // Laagste toets 0 (C-1): E = 4.
    let m = MidiChannelMapping { last_midi_note: Some(127), ..klavier(true, Some(0), 0) };
    assert_eq!(m.speelnoot(4), 0);
    assert!(!m.accepts(0, 0));
    assert!(m.accepts(0, 4));
}
