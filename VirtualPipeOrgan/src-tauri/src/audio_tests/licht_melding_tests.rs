use super::{licht_recent, LICHT_MELDING_MS};

#[test]
fn nooit_actief_geweest_meldt_niets() {
    assert!(!licht_recent(0, 0));
    assert!(!licht_recent(0, 10 * LICHT_MELDING_MS));
}

#[test]
fn melding_blijft_even_staan_en_gaat_dan_uit() {
    let laatste = 5_000;
    assert!(licht_recent(laatste, laatste));
    assert!(licht_recent(laatste, laatste + LICHT_MELDING_MS - 1));
    assert!(!licht_recent(laatste, laatste + LICHT_MELDING_MS));
    assert!(!licht_recent(laatste, laatste + 60_000));
}

#[test]
fn klok_die_even_terugloopt_geeft_geen_overloop() {
    // nu < laatste kan alleen bij ongelukkige volgorde van lezen; mag niet
    // paniekeren en telt als "net gebeurd".
    assert!(licht_recent(5_000, 4_999));
}
