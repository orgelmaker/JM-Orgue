use super::*;
use std::time::{Duration, Instant};

fn kiezer() -> TredeKiezer { TredeKiezer::new(Duration::from_millis(1500)) }

#[test]
fn ruisende_tweede_trede_wint_niet_meer() {
    let t0 = Instant::now();
    let mut k = kiezer();
    // Trede A (kanaal 1 CC 11) zakt van 127 naar 20 in 1 s.
    for i in 0..=10u8 { k.zie(0, 11, 127 - i * 10, t0 + Duration::from_millis(i as u64 * 100)); }
    // Trede B (kanaal 2 CC 11) ruist ±1 en is het laatst gezien.
    for i in 0..40u32 { k.zie(1, 11, 64 + (i % 2) as u8, t0 + Duration::from_millis(1000 + i as u64 * 60)); }
    let nu = t0 + Duration::from_millis(1000 + 40 * 60 + 1600);
    assert_eq!(k.keuze(nu), Some((0, 11, 27)));
}

#[test]
fn speeltafel_die_alles_stuurt_kiest_de_bewogen_trede() {
    let t0 = Instant::now();
    let mut k = kiezer();
    // Bij elke beweging komen alle drie waarden binnen; alleen trede 2 verandert.
    for i in 0..=12u8 {
        let t = t0 + Duration::from_millis(i as u64 * 80);
        k.zie(0, 11, 100, t);          // trede 1 staat stil
        k.zie(1, 11, 120 - i * 9, t);  // trede 2 beweegt
        k.zie(2, 11, 5, t);            // trede 3 staat stil, komt als laatste
    }
    let nu = t0 + Duration::from_millis(12 * 80 + 1600);
    assert_eq!(k.keuze(nu), Some((1, 11, 120 - 12 * 9)));
}

#[test]
fn lsb_van_14_bits_paar_telt_niet() {
    let t0 = Instant::now();
    let mut k = kiezer();
    for i in 0..=10u8 {
        let t = t0 + Duration::from_millis(i as u64 * 100);
        k.zie(0, 7, i * 12, t);
        k.zie(0, 39, ((i as u16 * 37) % 128) as u8, t + Duration::from_millis(1));
    }
    let nu = t0 + Duration::from_millis(1000 + 1600);
    assert_eq!(k.keuze(nu), Some((0, 7, 120)));
}

#[test]
fn niet_gesetteld_geeft_nog_geen_keuze() {
    let t0 = Instant::now();
    let mut k = kiezer();
    for i in 0..=10u8 { k.zie(0, 4, i * 10, t0 + Duration::from_millis(i as u64 * 100)); }
    assert_eq!(k.keuze(t0 + Duration::from_millis(1200)), None);
    assert_eq!(k.keuze(t0 + Duration::from_millis(1000 + 1500)), Some((0, 4, 100)));
}

#[test]
fn noodkeuze_kleine_slag_en_zonder_beweging() {
    let t0 = Instant::now();
    let mut k = kiezer();
    k.zie(0, 4, 3, t0);
    k.zie(0, 4, 5, t0 + Duration::from_millis(200));
    k.zie(3, 1, 64, t0 + Duration::from_millis(300)); // laatst gezien, geen slag
    assert_eq!(k.keuze(t0 + Duration::from_secs(5)), None, "slag 2 is geen trede");
    assert_eq!(k.noodkeuze(Some(t0 + Duration::from_secs(5))), Some((0, 4, 5)));
    let mut stil = kiezer();
    stil.zie(5, 64, 127, t0);
    assert_eq!(stil.noodkeuze(Some(t0 + Duration::from_secs(5))), None, "zonder slag alleen bij time-out");
    assert_eq!(stil.noodkeuze(None), Some((5, 64, 127)));
}

#[test]
fn herhaalde_gelijke_waarden_zetten_de_rustklok_niet_terug() {
    let t0 = Instant::now();
    let mut k = kiezer();
    for i in 0..=10u8 { k.zie(0, 11, i * 12, t0 + Duration::from_millis(i as u64 * 50)); }
    // Speeltafel blijft dezelfde eindwaarde zenden.
    for i in 0..100u64 { k.zie(0, 11, 120, t0 + Duration::from_millis(500 + i * 20)); }
    assert_eq!(k.keuze(t0 + Duration::from_millis(500 + 1600)), Some((0, 11, 120)));
}
