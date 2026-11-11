use super::pedaalbindingen_voor_herstel;
use crate::library::{CrescendoBindingSaved, OrganSettings, SwellBindingSaved};

fn zwel(div: &str, ch: u8, cc: u8) -> SwellBindingSaved {
    SwellBindingSaved {
        division_name: div.to_string(), division_index: 0, channel: ch, cc_num: cc,
        min_val: 0, max_val: 127, invert: false, last_value: None,
    }
}

fn cresc(ch: u8, cc: u8) -> CrescendoBindingSaved {
    CrescendoBindingSaved { channel: ch, cc_num: cc, min_val: 0, max_val: 127, invert: false }
}

fn settings(zwellen: Vec<SwellBindingSaved>, crescendo: Option<CrescendoBindingSaved>) -> OrganSettings {
    OrganSettings { swell_bindings: zwellen, crescendo_binding: crescendo, ..Default::default() }
}

#[test]
fn zwel_en_crescendo_op_dezelfde_trede_alleen_crescendo() {
    // 0.7.38-bestand: beide op (kanaal 0, CC7). Voorheen werden ze allebei
    // hersteld en was de zwelkast stil dood (cc_claimed_by_crescendo wint).
    let s = settings(vec![zwel("Nevenwerk", 0, 7)], Some(cresc(0, 7)));
    let (zwellen, crescendo) = pedaalbindingen_voor_herstel(&s);
    assert!(zwellen.is_empty());
    let c = crescendo.expect("crescendobinding blijft");
    assert_eq!((c.channel, c.cc_num), (0, 7));
}

#[test]
fn zwel_op_ander_kanaal_blijft_naast_crescendo() {
    let s = settings(vec![zwel("Nevenwerk", 1, 7)], Some(cresc(0, 7)));
    let (zwellen, crescendo) = pedaalbindingen_voor_herstel(&s);
    assert_eq!(zwellen.len(), 1);
    assert_eq!((zwellen[0].channel, zwellen[0].cc_num), (1, 7));
    assert!(crescendo.is_some());
}

#[test]
fn alleen_de_botsende_zwelbindingen_vervallen() {
    let s = settings(
        vec![zwel("Nevenwerk", 0, 7), zwel("Bovenwerk", 0, 7), zwel("Rugwerk", 0, 11)],
        Some(cresc(0, 7)),
    );
    let (zwellen, _) = pedaalbindingen_voor_herstel(&s);
    let namen: Vec<&str> = zwellen.iter().map(|b| b.division_name.as_str()).collect();
    assert_eq!(namen, vec!["Rugwerk"]);
    // Zonder crescendobinding blijft alles staan.
    let s2 = settings(vec![zwel("Nevenwerk", 0, 7)], None);
    let (z2, c2) = pedaalbindingen_voor_herstel(&s2);
    assert_eq!(z2.len(), 1);
    assert!(c2.is_none());
}
