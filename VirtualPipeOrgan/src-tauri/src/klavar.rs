//! Klavar (0.7.70): het model dat de klavar-weergave tekent. Leest het
//! gedeelde gekwantiseerde model uit notation.rs en voegt toe wat klavar
//! nodig heeft: de hand per noot (stokrichting), de pedaalbalk, het
//! stopteken en de doorklinkstippen. Het tekenen zelf gebeurt in de frontend
//! (KlavarSheet.svelte, 0.7.71); hier staan alleen de regels, zodat ze
//! getest zijn en notenschrift en klavar dezelfde maten tonen.
//!
//! Regels (zie PLAN_klavar_notatie.md, paragraaf 2):
//! - duur is afstand: geen notenwaarden, geen rusten, geen overbindingen;
//! - een noot klinkt tot de volgende noot van dezelfde (balk, hand), tenzij
//!   een stopteken ("v") of doorklinkstip anders zegt;
//! - stopteken alleen als er een gat volgt dat groter is dan de speling
//!   (max(1, q/4) rastereenheden), of als er niets meer volgt;
//! - legato-overlap (een noot die tot kort ná een volgende inzet op dezelfde
//!   balk doorklinkt) wordt voor stop- en stipbepaling op die inzet afgekapt;
//!   dezelfde toets wordt altijd op haar eerstvolgende inzet afgekapt (één
//!   hand kan één toets niet twee keer laten klinken);
//! - de speling is max(1, q/4) rastereenheden met een tijdsvloer van 150 ms
//!   (organistenlegato); korte rusten van die lengte krijgen dus geen "v";
//! - doorklinkstip op elke latere inzet van dezelfde (balk, hand) zolang de
//!   noot klinkt; pedaal en linkerhand zijn aparte groepen;
//! - maatstrepen die een noot kruist worden apart geleverd: de renderer zet
//!   daar een stip onder de bovenste maatstreep van een systeem.

use crate::notation::{options_from_score, quantize_score, score_to_staves, KlavarHand, NotationOptions, QuantizedScore, Score, Staff};
use serde::Serialize;

/// Eén regel in de legenda boven de balk: laagnaam en hand (ook lege lagen,
/// zodat de toewijzing niet verspringt als een laag haar eerste noot krijgt).
#[derive(Debug, Clone, Serialize)]
pub struct KlavarLegend {
    pub layer_id: Option<u32>,
    pub name: String,
    pub hand: KlavarHand,
    /// Splitspunt bij R+L (MIDI-nummer): eronder links, erop en erboven rechts.
    /// Codering: hand Right + split_midi Some = "R+L"; een manuaalbalk met
    /// splitspunt krijgt in de legenda altijd hand Right.
    pub split_midi: Option<u8>,
}

/// Eén getekende noot.
#[derive(Debug, Clone, Serialize)]
pub struct KlavarNote {
    pub id: Option<u64>,
    pub layer_id: Option<u32>,
    pub midi: u8,
    /// Rastereenheden, einde exclusief en ONGEKORT (klavar is polyfoon).
    pub start: u64,
    pub end: u64,
    /// Het einde waarop stopteken en stippen zijn bepaald: `end`, of de
    /// inzet waarop de noot legato is afgekapt. De renderer zet de "v" hier.
    pub end_cut: u64,
    /// Rechts, links of pedaal; de renderer leidt de stokrichting eraf.
    pub hand: KlavarHand,
    /// Stopteken "v" op het einde.
    pub stop: bool,
    /// Rastermomenten van latere inzetten van dezelfde (balk, hand) waarop
    /// deze noot nog klinkt: daar komt een doorklinkstip.
    pub dots: Vec<u64>,
    /// Maatstrepen (rastermomenten) die deze noot kruist, voor de stip onder
    /// de bovenste maatstreep van een systeem.
    pub bar_crossings: Vec<u64>,
    /// Balkgroep per tel (0.7.72); nu altijd None.
    pub beam: Option<u32>,
    /// Manuaallabel bij een wissel van laag (0.7.72); nu altijd None.
    pub label: Option<String>,
}

/// Eén balk (manuaal of pedaal) met zijn bereik en noten.
#[derive(Debug, Clone, Serialize)]
pub struct KlavarStaff {
    /// Laagste getekende toets (altijd een C) en hoogste (een B, of 127 als
    /// het bovenste octaaf niet past; de renderer mag daar niet op "% 12 == 11"
    /// vertrouwen).
    pub midi_min: u8,
    pub midi_max: u8,
    pub notes: Vec<KlavarNote>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KlavarModel {
    /// Score.generation in de live-flow; None in de bestandsmodus.
    pub generation: Option<u64>,
    pub title: String,
    pub bpm: f64,
    /// Rastereenheden per kwartnoot.
    pub q: u8,
    pub beats_per_bar: u8,
    pub measure_len: u64,
    /// Uit het langste ongekorte einde, minimaal 1.
    pub num_measures: u64,
    /// Alle lagen, ook lege.
    pub legend: Vec<KlavarLegend>,
    /// Eén manuaalbalk voor alle manuaallagen (hand via de stok).
    pub manual: KlavarStaff,
    /// Smallere pedaalbalk links; None als er geen pedaallaag is.
    pub pedal: Option<KlavarStaff>,
}

/// Standaard-handtoewijzing per balk, over ALLE balken (ook lege): een
/// expliciete hand wint; anders pedaal → pedaalbalk; precies één manuaalbalk
/// → rechts met splitspunt c' (60), dus R+L; meer manuaalbalken: de eerste
/// rechts, de rest links. Een balk met een splitspunt verdeelt per noot:
/// eronder links, erop en erboven rechts.
pub fn assign_hands(staves: &[Staff]) -> Vec<(KlavarHand, Option<u8>)> {
    let is_manuaal = |s: &Staff| !s.pedal && s.hand != Some(KlavarHand::Pedal);
    let een_manuaal = staves.iter().filter(|s| is_manuaal(s)).count() == 1;
    // Rechts is maar één keer te vergeven: een expliciete rechterhand (of
    // een balk met splitspunt, dat is R+L) telt mee, zodat [HW = links,
    // ZW = automatisch] het Zwelwerk rechts geeft.
    let mut rechts_vergeven = staves.iter().any(|s| is_manuaal(s)
        && (s.hand == Some(KlavarHand::Right) || (s.hand.is_some() && s.split_midi.is_some())));
    staves.iter().map(|s| {
        if let Some(h) = s.hand {
            if h == KlavarHand::Pedal { return (KlavarHand::Pedal, None); }
            // Een splitspunt betekent R+L, wat de hand ook zegt.
            if s.split_midi.is_some() { return (KlavarHand::Right, s.split_midi); }
            return (h, None);
        }
        if s.pedal { return (KlavarHand::Pedal, None); }
        if een_manuaal { return (KlavarHand::Right, Some(s.split_midi.unwrap_or(60))); }
        if s.split_midi.is_some() { return (KlavarHand::Right, s.split_midi); }
        if !rechts_vergeven { rechts_vergeven = true; (KlavarHand::Right, None) } else { (KlavarHand::Left, None) }
    }).collect()
}

/// Hand van één noot: op de pedaalbalk altijd pedaal; anders de per-noot-hand
/// (alleen links of rechts: een per-noot-hand verplaatst een noot nooit naar
/// een andere balk), dan het splitspunt van de balk, dan de hand van de balk.
fn hand_van_noot(balk: (KlavarHand, Option<u8>), noot: Option<KlavarHand>, midi: u8) -> KlavarHand {
    if balk.0 == KlavarHand::Pedal { return KlavarHand::Pedal; }
    if let Some(h) = noot.filter(|h| *h != KlavarHand::Pedal) { return h; }
    match balk.1 {
        Some(split) => if midi < split { KlavarHand::Left } else { KlavarHand::Right },
        None => balk.0,
    }
}

/// Afronden op de C eronder en de B erboven.
fn octaaf_onder(midi: u8) -> u8 { midi - midi % 12 }
fn octaaf_boven(midi: u8) -> u8 { (midi / 12) * 12 + 11 }

/// Bereik van de manuaalbalk: hele octaven, altijd met c'–b' (de enige
/// oriëntatie: de stippellijnen bij cis'/dis'), minimaal c–b'' (48-83).
fn bereik_manuaal(notes: &[KlavarNote]) -> (u8, u8) {
    let mut lo = 48u8;
    let mut hi = 83u8;
    for n in notes {
        lo = lo.min(octaaf_onder(n.midi));
        hi = hi.max(octaaf_boven(n.midi.min(127)));
    }
    (lo.min(60), hi.max(71).min(127))
}

/// Bereik van de pedaalbalk: ondergrens C (36), zodat de linkerrand de
/// oriëntatie is, maar lager als er lagere noten zijn (een pedaal dat ruwe
/// nootnummers vanaf 24 zendt, of een MIDI-bestand in 16'-notatie); bovengrens
/// automatisch, minimaal b (59).
fn bereik_pedaal(notes: &[KlavarNote]) -> (u8, u8) {
    let mut lo = 36u8;
    let mut hi = 59u8;
    for n in notes {
        lo = lo.min(octaaf_onder(n.midi));
        hi = hi.max(octaaf_boven(n.midi.min(127)));
    }
    (lo, hi.min(127))
}

/// Stoptekens, doorklinkstippen en maatstreep-kruisingen voor één groep
/// (balk, hand). `groep` zijn indices in `alle`; de legato-afkapping kijkt
/// naar álle noten van de balk (`alle`: één klavier, één speler), het
/// stopteken en de stippen alleen naar de eigen groep.
fn stop_en_stippen(alle: &mut [KlavarNote], groep: &[usize], speling: u64, measure_len: u64) {
    // Inzetten van de groep, oplopend en uniek.
    let mut inzetten: Vec<u64> = groep.iter().map(|&i| alle[i].start).collect();
    inzetten.sort_unstable();
    inzetten.dedup();
    for &i in groep {
        let (start, end, midi) = (alle[i].start, alle[i].end, alle[i].midi);
        // Dubbelslag of twee lagen op dezelfde toets en inzet: de kortste is
        // gedekt door de langste en krijgt zelf geen tekens.
        let gedekt = alle.iter().enumerate().any(|(j, m)| j != i && m.midi == midi && m.start == start
            && (m.end > end || (m.end == end && j < i)));
        if gedekt {
            alle[i].stop = false;
            alle[i].dots = Vec::new();
            alle[i].bar_crossings = Vec::new();
            continue;
        }
        // Legato-afkapping: klinkt de noot tot kort ná een latere inzet op de
        // balk door, dan telt die inzet als het einde. Dezelfde toets wordt
        // altijd op haar volgende inzet afgekapt (die kan niet twee keer
        // tegelijk klinken).
        let mut eind = end;
        for (j, m) in alle.iter().enumerate() {
            if j == i { continue; }
            if m.start > start && m.start < eind && (m.midi == midi || eind <= m.start + speling) {
                eind = m.start;
            }
        }
        alle[i].end_cut = eind;
        // Stopteken: volgt er na het (afgekapte) einde binnen de speling een
        // inzet van dezelfde groep? Inzetten tíjdens de noot tellen niet mee:
        // die zijn de doorklinkstippen.
        let volgende = inzetten.iter().copied().find(|&t| t >= eind);
        alle[i].stop = match volgende {
            None => true,
            Some(v) => v > eind + speling,
        };
        alle[i].dots = inzetten.iter().copied().filter(|&t| t > start && t < eind).collect();
        let mut kruisingen = Vec::new();
        if measure_len > 0 {
            let mut m = (start / measure_len + 1) * measure_len;
            while m < eind { kruisingen.push(m); m += measure_len; }
        }
        alle[i].bar_crossings = kruisingen;
    }
}

/// Het klavar-model uit het gedeelde gekwantiseerde model. `handen` is de
/// toewijzing per balkindex (uit `assign_hands` over ALLE balken), `legend`
/// de legenda over alle lagen.
pub fn klavar_model(qs: &QuantizedScore, handen: &[(KlavarHand, Option<u8>)], legend: Vec<KlavarLegend>, title: &str, generation: Option<u64>) -> KlavarModel {
    let mut manual: Vec<KlavarNote> = Vec::new();
    let mut pedal: Vec<KlavarNote> = Vec::new();
    for st in &qs.staves {
        let balk = handen.get(st.staff_index).copied().unwrap_or((KlavarHand::Right, None));
        for n in &st.notes {
            let hand = hand_van_noot(balk, n.hand, n.midi);
            let noot = KlavarNote {
                id: n.id, layer_id: st.layer_id, midi: n.midi, start: n.start, end: n.end, end_cut: n.end, hand,
                stop: false, dots: Vec::new(), bar_crossings: Vec::new(), beam: None, label: None,
            };
            if hand == KlavarHand::Pedal { pedal.push(noot); } else { manual.push(noot); }
        }
    }
    let sorteer = |v: &mut Vec<KlavarNote>| v.sort_by_key(|n| (n.start, n.midi));
    sorteer(&mut manual);
    sorteer(&mut pedal);

    // Speling: een zestiende (q/4, minimaal één eenheid), met een vloer van
    // 150 ms organistenlegato, zodat ook bij 120 bpm en q=8 een legato-keten
    // geen valse stippen en stoptekens krijgt.
    let vloer = (0.15 * qs.bpm / 60.0 * qs.q as f64).ceil() as u64;
    let speling = (qs.q / 4).max(1).max(vloer);
    let rechts: Vec<usize> = manual.iter().enumerate().filter(|(_, n)| n.hand == KlavarHand::Right).map(|(i, _)| i).collect();
    let links: Vec<usize> = manual.iter().enumerate().filter(|(_, n)| n.hand == KlavarHand::Left).map(|(i, _)| i).collect();
    stop_en_stippen(&mut manual, &rechts, speling, qs.measure_len);
    stop_en_stippen(&mut manual, &links, speling, qs.measure_len);
    let alle_pedaal: Vec<usize> = (0..pedal.len()).collect();
    stop_en_stippen(&mut pedal, &alle_pedaal, speling, qs.measure_len);

    let langste = manual.iter().chain(pedal.iter()).map(|n| n.end).max().unwrap_or(0);
    let num_measures = if qs.measure_len == 0 { 1 } else { ((langste + qs.measure_len - 1) / qs.measure_len).max(1) };

    let heeft_pedaal = legend.iter().any(|l| l.hand == KlavarHand::Pedal) || !pedal.is_empty();
    let (mmin, mmax) = bereik_manuaal(&manual);
    let (pmin, pmax) = bereik_pedaal(&pedal);
    KlavarModel {
        generation,
        title: title.to_string(),
        bpm: qs.bpm,
        q: qs.q as u8,
        beats_per_bar: qs.beats_per_bar as u8,
        measure_len: qs.measure_len,
        num_measures,
        legend,
        manual: KlavarStaff { midi_min: mmin, midi_max: mmax, notes: manual },
        pedal: if heeft_pedaal { Some(KlavarStaff { midi_min: pmin, midi_max: pmax, notes: pedal }) } else { None },
    }
}

/// Legenda over alle balken, met de toegewezen hand.
fn legenda(staves: &[Staff], handen: &[(KlavarHand, Option<u8>)]) -> Vec<KlavarLegend> {
    staves.iter().zip(handen.iter()).map(|(s, &(hand, split))| KlavarLegend {
        layer_id: s.layer_id, name: s.name.clone(), hand, split_midi: split,
    }).collect()
}

/// Klavar-model uit balken (bestandsmodus en live). Een partituur zonder
/// noten geeft een leeg model (één maat), geen fout: het venster toont dan
/// de lege balk waarop de noten tijdens het opnemen verschijnen.
pub fn klavar_model_from_staves(staves: &[Staff], opts: &NotationOptions, generation: Option<u64>) -> KlavarModel {
    let handen = assign_hands(staves);
    let legend = legenda(staves, &handen);
    let qs = quantize_score(staves, opts);
    let title = opts.title.clone().unwrap_or_default();
    klavar_model(&qs, &handen, legend, &title, generation)
}

/// Klavar-model uit een live Score.
pub fn klavar_model_from_score(score: &Score) -> KlavarModel {
    let staves = score_to_staves(score);
    klavar_model_from_staves(&staves, &options_from_score(score), Some(score.generation))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::{LayerEv, NoteEv};

    fn opts(beats: u8, q: u8) -> NotationOptions {
        NotationOptions { bpm: 60.0, beats_per_bar: beats, quantize: q, key_fifths: 0, title: Some("T".into()), staves: None, tolerance_pct: Some(100) }
    }

    /// Balk met noten (midi, start s, eind s); bij 60 bpm en q=4 is één
    /// rastereenheid 0,25 s.
    fn balk(naam: &str, pedal: bool, hand: Option<KlavarHand>, noten: &[(u8, f64, f64)]) -> Staff {
        Staff {
            name: naam.into(), pedal, hand,
            notes: noten.iter().enumerate().map(|(i, &(m, s, e))| NoteEv { midi: m, start_sec: s, end_sec: e, id: Some(i as u64 + 1), hand: None }).collect(),
            ..Default::default()
        }
    }

    fn model(staves: &[Staff], beats: u8, q: u8) -> KlavarModel {
        klavar_model_from_staves(staves, &opts(beats, q), None)
    }

    fn noot<'a>(st: &'a KlavarStaff, midi: u8, start: u64) -> &'a KlavarNote {
        st.notes.iter().find(|n| n.midi == midi && n.start == start).expect("noot")
    }

    #[test]
    fn assign_hands_standaard() {
        let staves = vec![balk("Hoofdwerk", false, None, &[]), balk("Zwelwerk", false, None, &[]), balk("Pedaal", true, None, &[])];
        let h = assign_hands(&staves);
        assert_eq!(h, vec![(KlavarHand::Right, None), (KlavarHand::Left, None), (KlavarHand::Pedal, None)]);
    }

    #[test]
    fn een_manuaallaag_krijgt_split() {
        let staves = vec![balk("Balk 1", false, None, &[]), balk("Pedaal", true, None, &[])];
        assert_eq!(assign_hands(&staves)[0], (KlavarHand::Right, Some(60)));
        let alleen = vec![balk("Balk 1", false, None, &[])];
        assert_eq!(assign_hands(&alleen)[0], (KlavarHand::Right, Some(60)));
    }

    #[test]
    fn assign_hands_override() {
        let mut staves = vec![balk("Hoofdwerk", false, Some(KlavarHand::Left), &[]), balk("Zwelwerk", false, None, &[])];
        staves[0].split_midi = Some(55);
        let h = assign_hands(&staves);
        // Een splitspunt betekent R+L, wat de hand ook zegt.
        assert_eq!(h[0], (KlavarHand::Right, Some(55)));
        // Rechts is al vergeven (R+L), dus de tweede manuaalbalk wordt links.
        assert_eq!(h[1], (KlavarHand::Left, None));
        // [HW = links, ZW = automatisch]: het Zwelwerk krijgt rechts.
        let staves = vec![balk("Hoofdwerk", false, Some(KlavarHand::Left), &[]), balk("Zwelwerk", false, None, &[])];
        assert_eq!(assign_hands(&staves), vec![(KlavarHand::Left, None), (KlavarHand::Right, None)]);
        // [HW = automatisch, ZW = rechts]: het Hoofdwerk wordt links.
        let staves = vec![balk("Hoofdwerk", false, None, &[]), balk("Zwelwerk", false, Some(KlavarHand::Right), &[])];
        assert_eq!(assign_hands(&staves), vec![(KlavarHand::Left, None), (KlavarHand::Right, None)]);
        // Een pedaallaag met een expliciete manuaalhand blijft op de pedaalbalk.
        let staves = vec![balk("Pedaal", true, Some(KlavarHand::Pedal), &[]), balk("HW", false, None, &[])];
        assert_eq!(assign_hands(&staves), vec![(KlavarHand::Pedal, None), (KlavarHand::Right, Some(60))]);
    }

    #[test]
    fn per_noot_hand_verandert_de_balk_niet() {
        let mut hw = balk("HW", false, None, &[(48, 0.0, 1.0)]);
        hw.notes[0].hand = Some(KlavarHand::Pedal);
        let mut ped = balk("Pedaal", true, None, &[(36, 0.0, 1.0)]);
        ped.notes[0].hand = Some(KlavarHand::Right);
        let m = model(&[hw, ped], 4, 4);
        assert_eq!(noot(&m.manual, 48, 0).hand, KlavarHand::Left, "pedaal op een manuaalnoot: terug naar het splitspunt");
        assert_eq!(noot(m.pedal.as_ref().unwrap(), 36, 0).hand, KlavarHand::Pedal);
    }

    #[test]
    fn herhaalde_toon_geeft_geen_stip_en_geen_vals_stopteken() {
        // Twee lagen met dezelfde hand: de c' wordt overgenomen en overlapt
        // door de kwantisatie één eenheid (0-1,15 s → einde 5, inzet 4).
        let a = balk("HW", false, Some(KlavarHand::Right), &[(60, 0.0, 1.15)]);
        let b = balk("ZW", false, Some(KlavarHand::Right), &[(60, 1.0, 2.0)]);
        let m = model(&[a, b], 4, 4);
        let eerste = noot(&m.manual, 60, 0);
        assert!(eerste.dots.is_empty());
        assert!(!eerste.stop);
        assert!(noot(&m.manual, 60, 4).stop);
    }

    #[test]
    fn dubbelslag_is_gedekt() {
        // Contactdender: een piepkorte noot op dezelfde inzet als de echte.
        let m = model(&[balk("HW", false, Some(KlavarHand::Right), &[(60, 1.0, 1.02), (60, 1.0, 2.0)])], 4, 4);
        let kort: Vec<&KlavarNote> = m.manual.notes.iter().filter(|n| n.midi == 60 && n.end == 5).collect();
        assert_eq!(kort.len(), 1);
        assert!(!kort[0].stop && kort[0].dots.is_empty(), "de korte is gedekt");
        assert!(noot(&m.manual, 60, 4).end == 8 || kort[0].end == 5);
    }

    #[test]
    fn legato_afkapping_over_de_splitgrens() {
        // Tenor a (links) gaat legato over in c' (rechts): het stopteken van
        // de a komt op de inzet van de c', niet een zestiende later.
        let m = model(&[balk("Balk 1", false, None, &[(57, 0.0, 1.15), (60, 1.0, 2.0)])], 4, 4);
        let a = noot(&m.manual, 57, 0);
        assert_eq!(a.hand, KlavarHand::Left);
        assert!(a.stop, "na de a volgt in de linkerhand niets meer");
        assert!(a.dots.is_empty());
    }

    #[test]
    fn speling_heeft_tijdsvloer() {
        // 120 bpm, q=8: een eenheid is 62,5 ms; legato-overlap van 160 ms mag
        // geen stip en geen stopteken geven.
        let o = NotationOptions { bpm: 120.0, beats_per_bar: 4, quantize: 8, key_fifths: 0, title: None, staves: None, tolerance_pct: Some(100) };
        let m = klavar_model_from_staves(&[balk("HW", false, Some(KlavarHand::Right), &[(60, 0.0, 1.16), (62, 1.0, 2.0)])], &o, None);
        let n = noot(&m.manual, 60, 0);
        assert!(n.dots.is_empty());
        assert!(!n.stop);
    }

    #[test]
    fn pedaalbalk_zakt_mee_onder_c() {
        let m = model(&[balk("Pedaal", true, None, &[(24, 0.0, 1.0)])], 4, 4);
        let p = m.pedal.expect("pedaal");
        assert_eq!((p.midi_min, p.midi_max), (24, 59));
    }

    #[test]
    fn manuaal_in_bassleutel_is_geen_pedaal() {
        let mut st = balk("Hoofdwerk", false, None, &[(48, 0.0, 1.0)]);
        st.bass_clef = true;
        let m = model(&[st], 4, 4);
        assert_eq!(m.manual.notes.len(), 1);
        assert!(m.pedal.is_none());
    }

    #[test]
    fn hand_per_noot_wint_van_splitspunt() {
        let mut st = balk("Balk 1", false, None, &[(55, 0.0, 1.0), (67, 0.0, 1.0), (55, 2.0, 3.0)]);
        st.notes[2].hand = Some(KlavarHand::Right);
        let m = model(&[st], 4, 4);
        assert_eq!(noot(&m.manual, 55, 0).hand, KlavarHand::Left);
        assert_eq!(noot(&m.manual, 67, 0).hand, KlavarHand::Right);
        assert_eq!(noot(&m.manual, 55, 8).hand, KlavarHand::Right);
        assert_eq!(m.legend[0].hand, KlavarHand::Right);
        assert_eq!(m.legend[0].split_midi, Some(60));
    }

    #[test]
    fn legenda_bevat_lege_laag() {
        let m = model(&[balk("Hoofdwerk", false, None, &[(60, 0.0, 1.0)]), balk("Zwelwerk", false, None, &[]), balk("Pedaal", true, None, &[])], 4, 4);
        assert_eq!(m.legend.len(), 3);
        assert_eq!(m.legend[1].name, "Zwelwerk");
        assert_eq!(m.legend[1].hand, KlavarHand::Left);
        assert!(m.pedal.is_some(), "pedaallaag zonder noten geeft toch een pedaalbalk");
    }

    #[test]
    fn stopteken_alleen_bij_gat() {
        let m = model(&[balk("HW", false, None, &[(60, 0.0, 1.0), (62, 1.0, 2.0), (64, 3.0, 4.0)])], 4, 4);
        assert!(!noot(&m.manual, 60, 0).stop, "legato: geen stopteken");
        assert!(noot(&m.manual, 62, 4).stop, "gat van een tel: stopteken");
        assert!(noot(&m.manual, 64, 12).stop, "laatste noot: stopteken");
    }

    #[test]
    fn stopteken_speling() {
        // Iets te vroeg losgelaten (gat van één zestiende): geen stopteken.
        let m = model(&[balk("HW", false, None, &[(60, 0.0, 0.8), (62, 1.0, 2.0)])], 4, 4);
        assert!(!noot(&m.manual, 60, 0).stop);
        // Gat van twee zestienden: wel.
        let m = model(&[balk("HW", false, None, &[(60, 0.0, 0.6), (62, 1.0, 2.0)])], 4, 4);
        assert!(noot(&m.manual, 60, 0).stop);
    }

    #[test]
    fn stopteken_speling_bij_q1() {
        // q=1: één eenheid is een kwartnoot; de speling is dan ook één eenheid.
        let m = model(&[balk("HW", false, None, &[(60, 0.0, 2.0), (62, 3.0, 4.0), (64, 6.0, 7.0)])], 4, 1);
        assert!(!noot(&m.manual, 60, 0).stop, "gat van één eenheid valt in de speling");
        assert!(noot(&m.manual, 62, 3).stop, "gat van twee eenheden");
    }

    #[test]
    fn stopteken_per_noot_in_akkoord() {
        let m = model(&[balk("HW", false, None, &[(60, 0.0, 1.0), (64, 0.0, 2.0), (67, 2.0, 3.0)])], 4, 4);
        assert!(noot(&m.manual, 60, 0).stop, "de korte akkoordnoot krijgt een stopteken");
        assert!(!noot(&m.manual, 64, 0).stop, "de lange loopt door tot de volgende inzet");
    }

    #[test]
    fn geen_doorklinkstip_bij_legato_overlap() {
        let m = model(&[balk("HW", false, None, &[(60, 0.0, 1.2), (62, 1.0, 2.0)])], 4, 4);
        let n = noot(&m.manual, 60, 0);
        assert!(n.dots.is_empty(), "overlap van één zestiende is legato, geen stip");
        assert!(!n.stop);
        assert_eq!((n.end, n.end_cut), (5, 4), "het afgekapte einde gaat mee naar de renderer");
    }

    #[test]
    fn wel_doorklinkstip_bij_echte_overlap() {
        let m = model(&[balk("HW", false, Some(KlavarHand::Right), &[(48, 0.0, 4.0), (60, 0.0, 1.0), (62, 1.0, 2.0), (64, 2.0, 3.0)])], 4, 4);
        assert_eq!(noot(&m.manual, 48, 0).dots, vec![4, 8]);
        assert!(noot(&m.manual, 48, 0).stop, "laatste in de groep");
    }

    #[test]
    fn pedaal_stopteken_onafhankelijk_van_linkerhand() {
        let m = model(&[
            balk("HW", false, Some(KlavarHand::Left), &[(48, 0.0, 1.0), (50, 0.25, 0.5)]),
            balk("Pedaal", true, None, &[(36, 0.0, 0.5)]),
        ], 4, 4);
        let p = m.pedal.as_ref().expect("pedaalbalk");
        let n = noot(p, 36, 0);
        assert!(n.stop, "de pedaalnoot is de laatste van zijn groep, de linkerhand doet er niet toe");
        assert!(n.dots.is_empty(), "een linkerhandinzet zet geen stip in de pedaalkolom");
        assert_eq!(n.hand, KlavarHand::Pedal);
    }

    #[test]
    fn maatstreep_kruisingen() {
        let m = model(&[balk("HW", false, None, &[(60, 0.0, 2.5)])], 1, 4);
        assert_eq!(noot(&m.manual, 60, 0).bar_crossings, vec![4, 8]);
    }

    #[test]
    fn num_measures_uit_langste_noot() {
        // De lange noot vóór een latere inzet telt ongekort mee (notenschrift
        // zou hem inkorten en op minder maten uitkomen).
        let m = model(&[balk("HW", false, None, &[(60, 0.0, 3.0), (62, 1.0, 1.5)])], 1, 4);
        assert_eq!(m.num_measures, 3);
    }

    #[test]
    fn pedaallaag_op_eigen_balk_manualen_samen() {
        let m = model(&[
            balk("Hoofdwerk", false, None, &[(60, 0.0, 1.0)]),
            balk("Zwelwerk", false, None, &[(72, 0.0, 1.0)]),
            balk("Pedaal", true, None, &[(36, 0.0, 1.0)]),
        ], 4, 4);
        assert_eq!(m.manual.notes.len(), 2);
        assert_eq!(noot(&m.manual, 60, 0).hand, KlavarHand::Right);
        assert_eq!(noot(&m.manual, 72, 0).hand, KlavarHand::Left);
        let p = m.pedal.expect("pedaal");
        assert_eq!(p.notes.len(), 1);
        assert_eq!(p.notes[0].hand, KlavarHand::Pedal);
    }

    #[test]
    fn bereik_bevat_centrale_c_en_pedaal_begint_bij_c() {
        let m = model(&[balk("HW", false, None, &[(84, 0.0, 1.0)]), balk("Pedaal", true, None, &[(48, 0.0, 1.0)])], 4, 4);
        assert_eq!((m.manual.midi_min, m.manual.midi_max), (48, 95));
        let p = m.pedal.expect("pedaal");
        assert_eq!((p.midi_min, p.midi_max), (36, 59));
        let leeg = model(&[balk("HW", false, None, &[])], 4, 4);
        assert_eq!((leeg.manual.midi_min, leeg.manual.midi_max), (48, 83));
    }

    #[test]
    fn lege_score_geeft_leeg_model() {
        let mut sc = Score::new(1);
        sc.add_layer("Hoofdwerk".into(), None);
        let m = klavar_model_from_score(&sc);
        assert!(m.manual.notes.is_empty());
        assert_eq!(m.num_measures, 1);
        assert_eq!(m.legend.len(), 1);
        assert_eq!(m.generation, Some(sc.generation));
        assert!(m.pedal.is_none());
    }

    #[test]
    fn klavar_model_json_snake_case() {
        let mut sc = Score::new(1);
        sc.add_layer("Hoofdwerk".into(), None);
        sc.add_layer("Pedaal".into(), None);
        let id = sc.new_event_id();
        sc.layers[0].takes[0].events.push(LayerEv { id, midi: 60, start_us: 0, end_us: 500_000, channel: 0, locked: false, hand: None });
        let v = serde_json::to_value(klavar_model_from_score(&sc)).expect("json");
        assert!(v.get("measure_len").is_some());
        assert_eq!(v["manual"]["notes"][0]["id"], serde_json::json!(id));
        assert_eq!(v["manual"]["notes"][0]["hand"], serde_json::json!("right"));
        assert_eq!(v["legend"][1]["hand"], serde_json::json!("pedal"));
        assert_eq!(v["manual"]["notes"][0]["layer_id"], serde_json::json!(sc.layers[0].id));
    }
}
