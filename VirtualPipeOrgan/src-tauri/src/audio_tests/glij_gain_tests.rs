use super::*;

#[test]
fn drain_mag_door_vloer_en_tijdbudget() {
    let periode = 2_666_000u64; // 128 frames @ 48 kHz
    let pct = |p: u64| periode / 100 * p;
    // Onder de vloer altijd door, ook zonder tijd.
    assert!(drain_mag_door(periode, periode, periode, 7, 8));
    // Op de vloer (8 eenheden ≈ 1,6 % mengkost): vorige mengtijd 82 % laat
    // nog net toe, 4 % verstreken erbij niet, 86 % niets.
    assert!(drain_mag_door(0, periode, pct(82), 8, 8));
    assert!(!drain_mag_door(pct(4), periode, pct(82), 8, 8));
    assert!(!drain_mag_door(0, periode, pct(86), 8, 8));
    // Vorige mengtijd 20 %, 40 % verstreken: 64 nieuwe eenheden (12,5 %)
    // passen nog, 160 (31 %) niet — de nieuwe stemmen tellen mee.
    assert!(drain_mag_door(pct(40), periode, pct(20), 64, 8));
    assert!(!drain_mag_door(pct(40), periode, pct(20), 160, 8));
    // Vanuit stilte: hooguit ~340 eenheden per callback.
    assert!(drain_mag_door(pct(10), periode, 0, 330, 8));
    assert!(!drain_mag_door(pct(10), periode, 0, 400, 8));
    // De afvoer zelf nooit meer dan de helft van de periode.
    assert!(!drain_mag_door(periode / 2, periode, 0, 8, 8));
    assert!(drain_mag_door(periode / 2 - 1, periode, 0, 8, 8));
    // Onbekende periode: alleen de vloer.
    assert!(!drain_mag_door(0, 0, 0, 8, 8));
}


fn k_bij(sr: f32) -> f32 { 1.0 - (-1.0 / (0.015 * sr)).exp() }

#[test]
fn stilstaand_volume_blijft_exact_gelijk() {
    let mut nu = 0.5f32;
    for _ in 0..10_000 {
        assert_eq!(glij_gain(&mut nu, 0.5, k_bij(48_000.0)), 0.5);
    }
}

#[test]
fn sprong_loopt_glad_en_zonder_doorschieten() {
    // -6 dB naar +6 dB in één klik op de baan.
    let van = 10f32.powf(-6.0 / 20.0);
    let naar = 10f32.powf(6.0 / 20.0);
    let k = k_bij(48_000.0);
    let mut nu = van;
    let mut vorige = van;
    let mut grootste_stap = 0.0f32;
    let mut n = 0;
    while nu != naar {
        let g = glij_gain(&mut nu, naar, k);
        assert!(g <= naar, "schiet door: {g}");
        assert!(g >= vorige, "loopt terug");
        grootste_stap = grootste_stap.max(g - vorige);
        vorige = g;
        n += 1;
        assert!(n < 48_000, "komt niet aan");
    }
    // Grootste stap per sample: een fractie van de sprong (geen tik).
    assert!(grootste_stap < (naar - van) * 0.002, "stap {grootste_stap}");
    // Na 5 tijdconstanten (75 ms) vrijwel op het doel; helemaal binnen 1 s.
    let mut nu = van;
    for _ in 0..(0.075 * 48_000.0) as usize { glij_gain(&mut nu, naar, k); }
    assert!((naar - nu) / (naar - van) < 0.01);
}

#[test]
fn omlaag_naar_stilte_komt_op_nul() {
    let k = k_bij(44_100.0);
    let mut nu = 1.0f32;
    for _ in 0..44_100 { glij_gain(&mut nu, 0.0, k); }
    assert_eq!(nu, 0.0);
}
