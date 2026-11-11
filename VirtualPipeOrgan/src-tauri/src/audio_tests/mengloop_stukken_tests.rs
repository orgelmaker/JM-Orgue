use super::{meng_stukken, reduceer_deeltabellen, set_meng_stukken,
            MAX_MENG_STUKKEN, MENG_DREMPEL_STEMMEN};

/// Bouwt een deeltabel waarin elke waarde uniek is, zodat een
/// vergeten stuk of divisie meteen opvalt.
fn deel(k: usize, bn: usize) -> Vec<[f32; 32]> {
    (0..bn).map(|f| {
        let mut rij = [0.0f32; 32];
        for d in 0..32 { rij[d] = (k * 1000 + f * 10 + d) as f32; }
        rij
    }).collect()
}

#[test]
fn een_stuk_verandert_niets() {
    let bn = 8;
    let mut l = deel(9, bn);
    let mut r = deel(9, bn);
    let voor_l = l.clone();
    let delen: Vec<Vec<[f32; 32]>> = Vec::new();
    reduceer_deeltabellen(&mut l, &mut r, &delen, &delen, 1, 4, bn);
    assert_eq!(l, voor_l, "bij één stuk hoort er niets opgeteld te worden");
}

#[test]
fn alle_stukken_en_divisies_komen_erbij() {
    let bn = 4;
    let n_div = 6;
    let mut l = vec![[0.0f32; 32]; bn];
    let mut r = vec![[0.0f32; 32]; bn];
    let dl: Vec<Vec<[f32; 32]>> = (0..3).map(|k| deel(k + 1, bn)).collect();
    let dr: Vec<Vec<[f32; 32]>> = (0..3).map(|k| deel(k + 1, bn)).collect();
    reduceer_deeltabellen(&mut l, &mut r, &dl, &dr, 4, n_div, bn);
    for f in 0..bn {
        for d in 0..n_div {
            let verwacht: f32 = (0..3).map(|k| dl[k][f][d]).sum();
            assert_eq!(l[f][d], verwacht, "frame {} divisie {}", f, d);
            assert_eq!(r[f][d], verwacht, "frame {} divisie {} (rechts)", f, d);
        }
    }
}

#[test]
fn divisies_boven_n_div_blijven_onaangeroerd() {
    // De tabellen zijn altijd 32 breed; alles boven het aantal divisies van
    // dit orgel is nul en hoeft niet aangeraakt — dat scheelt geheugenverkeer.
    let bn = 2;
    let mut l = vec![[0.0f32; 32]; bn];
    let mut r = vec![[0.0f32; 32]; bn];
    let dl: Vec<Vec<[f32; 32]>> = vec![deel(1, bn)];
    reduceer_deeltabellen(&mut l, &mut r, &dl, &dl, 2, 3, bn);
    for f in 0..bn {
        assert_ne!(l[f][2], 0.0, "divisie 2 hoort wel opgeteld te zijn");
        for d in 3..32 {
            assert_eq!(l[f][d], 0.0, "divisie {} hoort nul te blijven", d);
        }
    }
}

#[test]
fn de_optelvolgorde_ligt_vast() {
    // Reproduceerbaarheid is de eis: twee keer dezelfde invoer moet
    // bit-identiek hetzelfde geven, ook bij waarden waar de volgorde
    // aantoonbaar uitmaakt (groot + klein + klein).
    let bn = 1;
    let maak = |v: f32| -> Vec<[f32; 32]> {
        let mut rij = [0.0f32; 32];
        rij[0] = v;
        vec![rij; bn]
    };
    let dl = vec![maak(1e-8), maak(1e-8), maak(1.0)];
    let draai = |dl: &Vec<Vec<[f32; 32]>>| {
        let mut l = vec![[0.0f32; 32]; bn];
        let mut r = vec![[0.0f32; 32]; bn];
        reduceer_deeltabellen(&mut l, &mut r, dl, dl, 4, 1, bn);
        l[0][0]
    };
    assert_eq!(draai(&dl), draai(&dl));
}

#[test]
fn de_instelling_wordt_geklemd() {
    set_meng_stukken(0);
    assert_eq!(meng_stukken(), 1, "0 stukken bestaat niet");
    set_meng_stukken(9999);
    assert_eq!(meng_stukken(), MAX_MENG_STUKKEN);
    set_meng_stukken(1);
    assert_eq!(meng_stukken(), 1);
}

#[test]
fn de_drempel_is_zinnig() {
    // Onder de drempel loont verdelen niet; hij moet wel ruim onder een
    // gewoon akkoord met een paar registers liggen.
    assert!(MENG_DREMPEL_STEMMEN >= 16 && MENG_DREMPEL_STEMMEN <= 256);
}
