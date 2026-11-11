use super::*;
use std::time::{Duration, Instant};

/// Speelt een reeks (waarde, ms-sinds-vorige) af op één trede en geeft de
/// doorgegeven standen terug. Elke test een eigen (kanaal, cc): het filter
/// is een static en de tests draaien naast elkaar.
fn speel(channel: u8, cc: u8, reeks: &[(u8, u64)]) -> Vec<u8> {
    let mut t = Instant::now();
    reeks.iter().map(|(v, ms)| {
        t += Duration::from_millis(*ms);
        trede_filter_op(channel, cc, *v, t)
    }).collect()
}

#[test]
fn eerste_waarde_komt_ongewijzigd_door() {
    // Herstel na een orgelwissel zet de trede in één keer op zijn stand.
    assert_eq!(speel(90, 90, &[(73, 0)]), vec![73]);
}

#[test]
fn losse_uitschieter_haalt_de_zwelkast_niet() {
    // Het gemeten patroon: een rustige trede rond 60 met één meetfout van
    // 120 ertussen (overspraak van de buurtrede).
    let reeks: Vec<(u8, u64)> = vec![
        (60, 0), (61, 5), (59, 5), (60, 5), (120, 5), (61, 5), (60, 5), (59, 5),
    ];
    let uit = speel(91, 91, &reeks);
    assert!(uit.iter().all(|v| (55..=65).contains(v)),
        "uitschieter kwam door: {:?}", uit);
}

#[test]
fn rustwiebel_stuurt_geen_nieuwe_standen() {
    // Trede 3 van het testorgel pendelt in rust eindeloos 110/112/114.
    let mut reeks = vec![(112u8, 0u64)];
    for i in 0..40 { reeks.push(([110, 112, 114, 112][i % 4], 5)); }
    let uit = speel(92, 92, &reeks);
    let na_inloop = &uit[6..];
    assert!(na_inloop.iter().all(|v| *v == na_inloop[0]),
        "de kast trilt nog mee: {:?}", na_inloop);
}

#[test]
fn echte_beweging_komt_gewoon_aan() {
    // Zwelkast open in twee seconden: 1 eenheid per 15 ms.
    let reeks: Vec<(u8, u64)> = (0..=127u8).map(|v| (v, 15)).collect();
    let uit = speel(93, 93, &reeks);
    assert!(uit.last().unwrap() >= &120, "eindstand bleef steken op {:?}", uit.last());
    // Onderweg hooguit een paar eenheden achterstand.
    for (i, v) in uit.iter().enumerate().skip(6) {
        assert!((i as i16 - *v as i16).abs() <= 4, "te veel achterstand bij {}: {}", i, v);
    }
}

#[test]
fn snelle_voet_wordt_niet_geremd() {
    // Volle pedaalweg in 150 ms (8 eenheden per 10 ms) — sneller dan een
    // voet kan, en toch mag de begrenzing niet bijten.
    let reeks: Vec<(u8, u64)> = (0..16u8).map(|i| (i * 8, 10)).collect();
    let uit = speel(94, 94, &reeks);
    assert!(uit.last().unwrap() >= &100, "snelle beweging geremd: {:?}", uit);
}

#[test]
fn na_een_stilte_geldt_de_nieuwe_stand_meteen() {
    // Losse waarden met pauzes ertussen: een midibestand, de test-API, of
    // een trede die na een tijd stilstaan in één keer wordt ingetrapt.
    let uit = speel(95, 95, &[(0, 0), (127, 300), (0, 300), (64, 300)]);
    assert_eq!(uit, vec![0, 127, 0, 64]);
}

/// De kern van de klacht: een trede die in rust blijft wiebelen kwam voor
/// de inleerkiezer nooit "tot rust", zodat het inleren de volle time-out
/// uitzat en op de terugval uitkwam. Met het filter ertussen wél.
#[test]
fn wiebelende_trede_komt_bij_het_inleren_tot_rust() {
    let t0 = Instant::now();
    let settle = Duration::from_millis(1500);
    // Zoals gemeten: eerst de trede indrukken, daarna eindeloos 110/112/114.
    let mut reeks: Vec<(u8, u64)> = (0..100u8).map(|i| (10 + i, 5)).collect();
    for i in 0..600 { reeks.push(([110, 112, 114, 112][i % 4], 5)); }

    let met_filter = {
        let mut k = TredeKiezer::new(settle);
        let mut t = t0;
        for (i, (v, ms)) in reeks.iter().enumerate() {
            t += Duration::from_millis(*ms);
            let f = trede_filter_op(98, 98, *v, t0 + Duration::from_millis(i as u64 * 5));
            k.zie(98, 98, f, t);
        }
        k.keuze(t)
    };
    let zonder_filter = {
        let mut k = TredeKiezer::new(settle);
        let mut t = t0;
        for (v, ms) in reeks.iter() {
            t += Duration::from_millis(*ms);
            k.zie(99, 99, *v, t);
        }
        k.keuze(t)
    };
    assert!(zonder_filter.is_none(), "ongefilterd hoorde nooit tot rust te komen");
    let (ch, cc, stand) = met_filter.expect("gefilterd moet de trede kiezen");
    assert_eq!((ch, cc), (98, 98));
    assert!((108..=116).contains(&stand), "verkeerde eindstand ingeleerd: {}", stand);
}

#[test]
fn dichte_en_open_stand_komen_altijd_door() {
    // De rustband mag de uiterste standen niet inslikken: helemaal dicht
    // en helemaal open moeten exact gehaald worden.
    let mut reeks = vec![(120u8, 0u64)];
    for _ in 0..8 { reeks.push((127, 10)); }
    // 127 eenheden terug kost bij TREDE_MAX_PER_MS ruim 130 ms.
    for _ in 0..30 { reeks.push((0, 10)); }
    let uit = speel(96, 96, &reeks);
    assert!(uit.contains(&127), "kast ging niet helemaal open: {:?}", uit);
    assert_eq!(*uit.last().unwrap(), 0, "kast ging niet helemaal dicht: {:?}", uit);
}
