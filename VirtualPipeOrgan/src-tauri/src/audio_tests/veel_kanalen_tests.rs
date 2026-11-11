use super::{wet_channel_weights, MAX_OUT_CH};

#[test]
fn het_kanaalplafond_is_ruim_genoeg_voor_grote_interfaces() {
    assert!(MAX_OUT_CH >= 1024, "plafond te laag: {}", MAX_OUT_CH);
}

#[test]
fn honderden_kanalen_wegen_zonder_paniek() {
    // Drie divisies, elk op een eigen paar hoog in een interface met 256
    // uitgangen. Vóór 0.7.48 viel alles boven kanaal 63 buiten de tabel.
    // Kanalen ver boven de oude u8-grens van 255, in een interface met 600
    // uitgangen: pas sinds 0.7.48 kán een divisie die aanwijzen.
    let chans = vec![vec![500u16, 501], vec![502, 503], vec![504, 505]];
    let (wl, wr, n) = wet_channel_weights(&chans, 3, 600);
    assert_eq!(n, 506, "hoogste gebruikte kanaal + 1");
    for (l, r) in [(500usize, 501usize), (502, 503), (504, 505)] {
        assert!((wl[l] - 1.0 / 3.0).abs() < 1e-6, "kanaal {} links: {}", l, wl[l]);
        assert!((wr[r] - 1.0 / 3.0).abs() < 1e-6, "kanaal {} rechts: {}", r, wr[r]);
    }
    // Ongebruikte kanalen krijgen niets.
    assert_eq!(wl[100], 0.0);
    assert_eq!(wr[255], 0.0);
    assert_eq!(wl[599], 0.0);
}

#[test]
fn kanalen_boven_het_plafond_worden_geklemd() {
    // Een apparaat dat meer kanalen meldt dan de tabel aankan mag geen
    // index-paniek geven.
    let chans = vec![vec![0u16, 1]];
    let (_, _, n) = wet_channel_weights(&chans, 1, MAX_OUT_CH + 500);
    assert!(n <= MAX_OUT_CH);
}
