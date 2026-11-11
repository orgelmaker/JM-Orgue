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

#[test]
fn uit_verandert_niets() {
    let m = klavier(false, Some(36), 0);
    for n in 36..=60u8 { assert_eq!(m.speelnoot(n), n); }
}

#[test]
fn onderste_octaaf_klinkt_zoals_op_een_historisch_orgel() {
    // Laagste toets C3 (36) ziet eruit als E maar klinkt als C.
    let m = klavier(true, Some(36), 0);
    // toets:      E   F   F#  G   G#  A   Bes B   C
    // klinkt:     C   F   D   G   E   A   Bes B   C(+1 oct)
    let verwacht = [36u8, 41, 38, 43, 40, 45, 46, 47, 48];
    for (i, v) in verwacht.iter().enumerate() {
        assert_eq!(m.speelnoot(36 + i as u8), *v, "toets {}", i);
    }
}

#[test]
fn boven_het_korte_octaaf_loopt_alles_weer_gelijk() {
    let m = klavier(true, Some(36), 0);
    // Vanaf de negende toets: vier halve tonen hoger dan de toets suggereert,
    // dus een doorlopende reeks zonder gaten.
    for i in 8..30u8 {
        assert_eq!(m.speelnoot(36 + i), 36 + i + 4, "toets {}", i);
    }
}

#[test]
fn zonder_ingeleerd_bereik_doet_de_vlag_niets() {
    // We weten dan niet waar de onderste octaaf begint; liever niets dan de
    // verkeerde noot.
    let m = klavier(true, None, 0);
    for n in 36..=60u8 { assert_eq!(m.speelnoot(n), n); }
}

#[test]
fn transponeren_komt_er_bovenop() {
    let m = klavier(true, Some(36), 12);
    assert_eq!(m.speelnoot(36), 48);        // C, een octaaf hoger
    assert_eq!(m.speelnoot(38), 50);        // F# → D, een octaaf hoger
}

#[test]
fn noten_onder_de_laagste_toets_blijven_ongemoeid() {
    let m = klavier(true, Some(36), 0);
    assert_eq!(m.speelnoot(30), 30);
}
