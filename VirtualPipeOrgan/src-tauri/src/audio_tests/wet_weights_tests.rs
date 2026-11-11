use super::wet_channel_weights;

/// Compacte weergave: alleen kanalen met gewicht, als (kanaal, wl, wr).
fn actief(wl: &[f32], wr: &[f32], n: usize) -> Vec<(usize, f32, f32)> {
    (0..n).filter(|&c| wl[c] > 1e-6 || wr[c] > 1e-6)
        .map(|c| (c, (wl[c] * 1000.0).round() / 1000.0, (wr[c] * 1000.0).round() / 1000.0))
        .collect()
}

#[test]
fn standaard_stereo_naar_voorste_paar() {
    // Geen routing geconfigureerd: volle galm op (0,1), zoals vóór 0.7.33.
    let chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
    let (wl, wr, n) = wet_channel_weights(&chans, 3, 2);
    assert_eq!(actief(&wl, &wr, n), vec![(0, 1.0, 0.0), (1, 0.0, 1.0)]);
}

#[test]
fn hoofdtelefoonprofiel_volgt_override() {
    // Testorgel-klacht: alle divisies via de override op [0,1] van een
    // 8-kanaals apparaat → volle galm op (0,1), niets op 2..7.
    let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
    for i in 0..3 { chans[i] = vec![0, 1]; }
    let (wl, wr, n) = wet_channel_weights(&chans, 3, 8);
    assert_eq!(actief(&wl, &wr, n), vec![(0, 1.0, 0.0), (1, 0.0, 1.0)]);
}

#[test]
fn speakerprofiel_alle_divisies_op_alle_paren() {
    // Elke divisie speelt droog op alle drie de paren → elk paar krijgt ook
    // de volle galm; de droog/galm-balans per luidspreker blijft gelijk.
    let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
    for i in 0..3 { chans[i] = vec![2, 3, 4, 5, 6, 7]; }
    let (wl, wr, n) = wet_channel_weights(&chans, 3, 8);
    assert_eq!(actief(&wl, &wr, n), vec![
        (2, 1.0, 0.0), (3, 0.0, 1.0), (4, 1.0, 0.0), (5, 0.0, 1.0), (6, 1.0, 0.0), (7, 0.0, 1.0)]);
}

#[test]
fn divisie_per_paar_telt_op_tot_een_keer_galm() {
    // Klassieke multikanaals-opstelling: elk werk zijn eigen paar. Elk paar
    // krijgt 1/3 galm — samen precies één keer, niet drie keer (anders
    // wordt alles fors natter zodra je de werken uit elkaar trekt).
    let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
    chans[0] = vec![0, 1]; chans[1] = vec![2, 3]; chans[2] = vec![4, 5];
    let (wl, wr, n) = wet_channel_weights(&chans, 3, 8);
    let a = actief(&wl, &wr, n);
    assert_eq!(a, vec![(0, 0.333, 0.0), (1, 0.0, 0.333), (2, 0.333, 0.0),
                       (3, 0.0, 0.333), (4, 0.333, 0.0), (5, 0.0, 0.333)]);
    let totaal_l: f32 = (0..n).map(|c| wl[c]).sum();
    assert!((totaal_l - 1.0).abs() < 0.01, "totale galm moet 1x zijn, is {}", totaal_l);
}

#[test]
fn gedeeld_kanaal_krijgt_galm_niet_dubbel() {
    // Overlappende routings: kanaal 0 wordt door beide divisies gebruikt en
    // krijgt daarom precies hun sommatie (1.0), niet twee volle kopieën.
    let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
    chans[0] = vec![0, 1]; chans[1] = vec![0, 2];
    let (wl, wr, n) = wet_channel_weights(&chans, 2, 8);
    assert_eq!(actief(&wl, &wr, n), vec![(0, 1.0, 0.0), (1, 0.0, 0.5), (2, 0.0, 0.5)]);
}

#[test]
fn los_laatste_kanaal_wordt_mono() {
    let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
    chans[0] = vec![4, 5, 6];
    let (wl, wr, n) = wet_channel_weights(&chans, 1, 8);
    assert_eq!(actief(&wl, &wr, n), vec![(4, 1.0, 0.0), (5, 0.0, 1.0), (6, 0.5, 0.5)]);
}

#[test]
fn routing_buiten_apparaat_valt_terug() {
    // 8-kanaals routing op een stereo-apparaat (na profielwissel): terugval
    // op het voorste paar, net als het droge signaal (auditbevinding 37).
    let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
    chans[0] = vec![2, 3, 4, 5];
    let (wl, wr, n) = wet_channel_weights(&chans, 1, 2);
    assert_eq!(actief(&wl, &wr, n), vec![(0, 1.0, 0.0), (1, 0.0, 1.0)]);
}

#[test]
fn mono_apparaat_somt_naar_kanaal_nul() {
    let chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
    let (wl, wr, n) = wet_channel_weights(&chans, 2, 1);
    assert_eq!(actief(&wl, &wr, n), vec![(0, 0.5, 0.5)]);
}
