//! MusicXML lezen (0.7.90): partwise MusicXML (en .mxl) uit Finale, MuseScore,
//! Dorico en andere programma's → `Score` voor het notatievenster. Eigen
//! lezer op roxmltree; lay-out (`<print>`, default-x/y, lettertypen) wordt
//! genegeerd. De sterkste test is de rondreis op de eigen fixtures: lezen →
//! `build_musicxml_from_score` → byte voor byte de fixture.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use roxmltree::{Document, Node};

use crate::notation::{
    is_pedal_name, klem_beat_unit, Articulation, BarAttr, BarStyle, LayerEv, Lyric, Placement, Score, Span, SpanKind,
    Spelling, Syllabic, TextKind, TextMark, KANAAL_ONBEKEND, MAX_MIN_MEASURES,
};

/// Resultaat van een import: het stuk en wat er onderweg is benaderd of
/// overgeslagen (de UI toont de waarschuwingen in één melding).
#[derive(Debug)]
pub struct ImportResult {
    pub score: Score,
    pub warnings: Vec<String>,
}

/// Leest een .musicxml/.xml of een gecomprimeerd .mxl-bestand.
pub fn read_musicxml_path(path: &Path) -> Result<ImportResult, String> {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    let tekst = if ext == "mxl" {
        lees_mxl(path)?
    } else {
        let bytes = std::fs::read(path).map_err(|e| format!("Kan {} niet lezen: {}", path.display(), e))?;
        bytes_naar_tekst(&bytes)
    };
    read_musicxml(&tekst)
}

/// UTF-16 met BOM (sommige exports) of UTF-8 (met of zonder BOM).
fn bytes_naar_tekst(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && ((bytes[0] == 0xFF && bytes[1] == 0xFE) || (bytes[0] == 0xFE && bytes[1] == 0xFF)) {
        let le = bytes[0] == 0xFF;
        let u16s: Vec<u16> = bytes[2..].chunks(2).filter(|c| c.len() == 2)
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        return String::from_utf16_lossy(&u16s);
    }
    let s = String::from_utf8_lossy(bytes).into_owned();
    s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s)
}

/// .mxl = zip met META-INF/container.xml die het hoofdbestand aanwijst;
/// zonder container het eerste .xml/.musicxml buiten META-INF.
fn lees_mxl(path: &Path) -> Result<String, String> {
    let f = std::fs::File::open(path).map_err(|e| format!("Kan {} niet openen: {}", path.display(), e))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("Geen geldig .mxl-bestand: {}", e))?;
    let mut root: Option<String> = None;
    if let Ok(mut c) = zip.by_name("META-INF/container.xml") {
        let mut s = String::new();
        if c.read_to_string(&mut s).is_ok() {
            if let Ok(doc) = parse_xml(&s) {
                root = doc.descendants().find(|n| n.has_tag_name("rootfile")).and_then(|n| n.attribute("full-path")).map(str::to_string);
            }
        }
    }
    let naam = match root {
        Some(r) => r,
        None => {
            let namen: Vec<String> = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string())).collect();
            namen.into_iter().find(|n| !n.starts_with("META-INF/") && (n.ends_with(".xml") || n.ends_with(".musicxml")))
                .ok_or_else(|| "Geen MusicXML-bestand in het .mxl-archief".to_string())?
        }
    };
    let mut f = zip.by_name(&naam).map_err(|e| format!("{} ontbreekt in het archief: {}", naam, e))?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).map_err(|e| format!("Lezen uit het archief mislukt: {}", e))?;
    Ok(bytes_naar_tekst(&bytes))
}

/// roxmltree weigert standaard een <!DOCTYPE> — en elke MusicXML-export
/// (Finale, MuseScore, Dorico, JM-Orgue zelf) begint ermee. De externe DTD
/// wordt nooit opgehaald; alleen interne entiteiten tellen mee.
fn parse_xml(text: &str) -> Result<Document<'_>, roxmltree::Error> {
    Document::parse_with_options(text, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() })
}

fn kind<'a, 'i>(n: Node<'a, 'i>, naam: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(naam))
}
fn tekst(n: Node, naam: &str) -> Option<String> {
    kind(n, naam).and_then(|c| c.text()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}
fn getal<T: std::str::FromStr>(n: Node, naam: &str) -> Option<T> {
    tekst(n, naam).and_then(|t| t.parse().ok())
}
fn pc_van(step: char) -> i32 {
    match step { 'C' => 0, 'D' => 2, 'E' => 4, 'F' => 5, 'G' => 7, 'A' => 9, 'B' => 11, _ => 0 }
}
/// Rastereenheden per kwart die een duur (in kwarten) nodig heeft: 1, 2, 4 of 8.
fn benodigde_q(kwarten: f64) -> u64 {
    for q in [1u64, 2, 4, 8] {
        let n = kwarten * q as f64;
        if (n - n.round()).abs() < 1e-6 { return q; }
    }
    8
}

/// Eén gelezen noot, tijden in kwartnoten (absoluut).
struct ImportEv {
    id: u64,
    midi: u8,
    start_q: f64,
    end_q: f64,
    voice: u8,
    lyrics: Vec<Lyric>,
    articulations: Vec<Articulation>,
    spelling: Option<Spelling>,
}

struct Melder { lijst: Vec<String> }
impl Melder {
    fn meld(&mut self, s: &str) { if !self.lijst.iter().any(|x| x == s) { self.lijst.push(s.to_string()); } }
}

/// Leest een partwise MusicXML-document.
pub fn read_musicxml(text: &str) -> Result<ImportResult, String> {
    let doc = parse_xml(text).map_err(|e| format!("Geen geldige XML: {}", e))?;
    let root = doc.root_element();
    if root.has_tag_name("score-timewise") {
        return Err("Dit is een timewise-MusicXML; exporteer het stuk als partwise".into());
    }
    if !root.has_tag_name("score-partwise") {
        return Err(format!("Geen MusicXML-partituur (hoofdelement <{}>)", root.tag_name().name()));
    }
    let mut w = Melder { lijst: Vec::new() };
    let mut sc = Score::new(0);
    sc.title = kind(root, "work").and_then(|n| tekst(n, "work-title")).unwrap_or_default();
    sc.subtitle = tekst(root, "movement-title").unwrap_or_default();
    sc.composer = kind(root, "identification")
        .and_then(|idn| idn.children().filter(|c| c.has_tag_name("creator"))
            .filter(|c| c.attribute("type").map(|t| t == "composer").unwrap_or(true))
            .filter_map(|c| c.text()).map(|t| t.trim().to_string()).find(|t| !t.is_empty()))
        .unwrap_or_default();
    // Alleen een movement-title (MuseScore): dat is de titel.
    if sc.title.is_empty() && !sc.subtitle.is_empty() { sc.title = std::mem::take(&mut sc.subtitle); }

    let mut part_namen: HashMap<String, String> = HashMap::new();
    if let Some(pl) = kind(root, "part-list") {
        for sp in pl.descendants().filter(|n| n.has_tag_name("score-part")) {
            if let Some(id) = sp.attribute("id") { part_namen.insert(id.to_string(), tekst(sp, "part-name").unwrap_or_default()); }
        }
    }

    let mut bpm: Option<f64> = None;
    let mut maatsoort: Option<(u8, u8)> = None;
    let mut toonsoort: Option<(i8, bool)> = None;
    let mut max_maten: usize = 0;
    let mut fijnste_q: u64 = 1;
    let mut eerste_divisions: Option<u64> = None;
    let mut bars: Vec<BarAttr> = Vec::new();
    // Per laag de gelezen noten; teksten en bogen met hun tijd in kwarten.
    let mut noten_per_laag: Vec<(u32, Vec<ImportEv>)> = Vec::new();
    let mut teksten: Vec<(TextMark, f64)> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();

    for (pi, part) in root.children().filter(|n| n.has_tag_name("part")).enumerate() {
        let pid = part.attribute("id").unwrap_or("").to_string();
        let naam_basis = part_namen.get(&pid).cloned().filter(|n| !n.is_empty()).unwrap_or_else(|| format!("Balk {}", pi + 1));
        // Vooraf: aantal balken en sleutels van deze part.
        let staves: usize = part.descendants().find(|n| n.has_tag_name("staves")).and_then(|n| n.text()).and_then(|t| t.trim().parse().ok()).unwrap_or(1usize).max(1);
        let mut clef_bas: HashMap<usize, bool> = HashMap::new();
        for clef in part.descendants().filter(|n| n.has_tag_name("clef")) {
            let nr = clef.attribute("number").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
            let bas = tekst(clef, "sign").map(|s| s == "F").unwrap_or(false);
            clef_bas.entry(nr).or_insert(bas);
        }
        let mut laag_ids: Vec<u32> = Vec::new();
        for st in 1..=staves {
            let naam = if staves == 1 { naam_basis.clone() } else { format!("{} {}", naam_basis, st) };
            let bas = clef_bas.get(&st).copied().unwrap_or_else(|| is_pedal_name(&naam));
            let id = sc.add_layer(naam, Some(bas));
            laag_ids.push(id);
            noten_per_laag.push((id, Vec::new()));
        }
        let laag_index_basis = noten_per_laag.len() - staves;
        let laag_van = |staff: usize| laag_ids[staff.clamp(1, staves) - 1];

        let mut divisions: f64 = 1.0;
        let mut pending_ties: HashMap<(usize, u8, u8), usize> = HashMap::new();
        let mut pending_slurs: HashMap<(usize, String), u64> = HashMap::new();
        let mut stemmen_per_balk: HashMap<usize, std::collections::HashSet<u32>> = HashMap::new();
        // Open haarspelden/octaveringen: (staff, soort, beginpositie in kwarten).
        let mut open_lijnen: Vec<(usize, SpanKind, f64)> = Vec::new();
        let mut dichte_lijnen: Vec<(usize, SpanKind, f64, f64)> = Vec::new();
        let mut octaaf_actief: HashMap<usize, i32> = HashMap::new();
        let mut maat_start_q: f64 = 0.0;
        let mut m_index: usize = 0;
        let mut ending_actief: Option<u8> = None;

        for measure in part.children().filter(|n| n.has_tag_name("measure")) {
            let mut cursor: f64 = 0.0;
            let mut bereik: f64 = 0.0;
            let mut laatste_start: Option<f64> = None;
            // Voor een opmaat: wat er in deze maat bijkomt, schuift straks op.
            let n0: Vec<usize> = noten_per_laag.iter().map(|l| l.1.len()).collect();
            let (t0, o0, d0) = (teksten.len(), open_lijnen.len(), dichte_lijnen.len());
            if pi == 0 {
                if let Some(n) = ending_actief {
                    let b = balk_attr(&mut bars, m_index as u32);
                    b.ending = Some(n);
                }
            }
            for el in measure.children().filter(|n| n.is_element()) {
                match el.tag_name().name() {
                    "attributes" => {
                        if let Some(d) = getal::<f64>(el, "divisions") {
                            if d > 0.0 { divisions = d; if eerste_divisions.is_none() { eerste_divisions = Some(d.round() as u64); } }
                        }
                        if let Some(t) = kind(el, "time") {
                            if let (Some(b), Some(u)) = (getal::<u8>(t, "beats"), getal::<u8>(t, "beat-type")) {
                                match maatsoort {
                                    None => maatsoort = Some((b, u)),
                                    Some(m) if m != (b, u) => w.meld("Maatsoortwisselingen onderweg zijn genegeerd (de eerste maatsoort geldt)"),
                                    _ => {}
                                }
                            }
                        }
                        if let Some(k) = kind(el, "key") {
                            if let Some(f) = getal::<i8>(k, "fifths") {
                                let minor = tekst(k, "mode").map(|m| m == "minor").unwrap_or(false);
                                match toonsoort {
                                    None => toonsoort = Some((f, minor)),
                                    Some(t) if t != (f, minor) => w.meld("Toonsoortwisselingen onderweg zijn genegeerd (de eerste toonsoort geldt)"),
                                    _ => {}
                                }
                            }
                        }
                    }
                    "note" => {
                        let dur_div = getal::<f64>(el, "duration").unwrap_or(0.0);
                        let is_chord = kind(el, "chord").is_some();
                        let staff = getal::<usize>(el, "staff").unwrap_or(1).clamp(1, staves);
                        if kind(el, "grace").is_some() { w.meld("Voorslagen (grace notes) zijn overgeslagen"); continue; }
                        let start_div = if is_chord { laatste_start.unwrap_or(cursor) } else { cursor };
                        if !is_chord { cursor = start_div + dur_div; bereik = bereik.max(cursor); }
                        laatste_start = Some(start_div);
                        if kind(el, "cue").is_some() { w.meld("Stichnoten (cue notes) zijn overgeslagen"); continue; }
                        if kind(el, "rest").is_some() { continue; }
                        if kind(el, "time-modification").is_some() { w.meld("Triolen en andere onregelmatige groepen zijn op het raster benaderd"); }
                        let Some(p) = kind(el, "pitch") else { continue; };
                        let step = tekst(p, "step").and_then(|s| s.chars().next()).unwrap_or('C');
                        let alter_f = getal::<f64>(p, "alter").unwrap_or(0.0);
                        let alter = alter_f.round() as i8;
                        if (alter_f - alter as f64).abs() > 1e-6 { w.meld("Microtonale alteraties zijn op een halve toon afgerond"); }
                        let octave = getal::<i32>(p, "octave").unwrap_or(4);
                        let midi_i = (octave + 1) * 12 + pc_van(step) + alter as i32 + *octaaf_actief.get(&staff).unwrap_or(&0);
                        let midi = midi_i.clamp(0, 127) as u8;
                        // MusicXML nummert stemmen per part: balk 2 krijgt 5..8 (MuseScore,
                        // Finale, Sibelius). Terug naar 1..4 op de balk; pas een melding als
                        // één balk echt meer dan vier verschillende stemmen draagt.
                        let voice = match tekst(el, "voice").and_then(|v| v.parse::<u32>().ok()) {
                            Some(n) if n >= 1 => {
                                let stem = ((n - 1) % 4 + 1) as u8;
                                let set = stemmen_per_balk.entry(staff).or_default();
                                set.insert(n);
                                if set.len() > 4 { w.meld("Meer dan vier stemmen op één balk: stemmen zijn samengevouwen"); }
                                stem
                            }
                            _ => 1,
                        };
                        let start_q = maat_start_q + start_div / divisions;
                        let end_q = maat_start_q + (start_div + dur_div.max(1e-9)) / divisions;
                        fijnste_q = fijnste_q.max(benodigde_q(dur_div / divisions)).max(benodigde_q(start_div / divisions));
                        let tie_start = el.children().any(|c| c.has_tag_name("tie") && c.attribute("type") == Some("start"))
                            || el.children().filter(|c| c.has_tag_name("notations")).flat_map(|n| n.children()).any(|c| c.has_tag_name("tied") && c.attribute("type") == Some("start"));
                        let tie_stop = el.children().any(|c| c.has_tag_name("tie") && c.attribute("type") == Some("stop"))
                            || el.children().filter(|c| c.has_tag_name("notations")).flat_map(|n| n.children()).any(|c| c.has_tag_name("tied") && c.attribute("type") == Some("stop"));
                        let sleutel = (staff, voice, midi);
                        if tie_stop {
                            if let Some(&ix) = pending_ties.get(&sleutel) {
                                let lijst = &mut noten_per_laag[laag_index_basis + staff - 1].1;
                                lijst[ix].end_q = lijst[ix].end_q.max(end_q);
                                let vervolg_id = lijst[ix].id;
                                if !tie_start { pending_ties.remove(&sleutel); }
                                // Een boog die op dit vervolgdeel eindigt of begint hoort
                                // bij het samengevoegde event (eigen export: slur-stop op
                                // het laatste deel).
                                verwerk_slurs(el, staff, vervolg_id, laag_van(staff), &mut pending_slurs, &mut spans, &mut sc);
                                continue;
                            }
                        }
                        let id = sc.new_event_id();
                        let mut lyrics: Vec<Lyric> = Vec::new();
                        for l in el.children().filter(|c| c.has_tag_name("lyric")) {
                            let number = l.attribute("number").and_then(|v| v.parse::<u8>().ok()).unwrap_or(1).clamp(1, 3);
                            // Meerdere <text> met <elision> (a‿e) in documentvolgorde; begin
                            // uit de eerste, einde uit de laatste lettergreep.
                            let mut text = String::new();
                            let mut eerste_syl: Option<String> = None;
                            let mut laatste_syl: Option<String> = None;
                            for c in l.children().filter(|c| c.is_element()) {
                                match c.tag_name().name() {
                                    "text" => text.push_str(c.text().unwrap_or("").trim()),
                                    "elision" => { let e = c.text().map(str::trim).unwrap_or(""); text.push_str(if e.is_empty() { "‿" } else { e }); }
                                    "syllabic" => { let sy = c.text().unwrap_or("").trim().to_string(); if eerste_syl.is_none() { eerste_syl = Some(sy.clone()); } laatste_syl = Some(sy); }
                                    _ => {}
                                }
                            }
                            // Koppelteken ervóór volgt uit de eerste lettergreep (end/middle),
                            // koppelteken erna uit de laatste (begin/middle).
                            let streep_voor = matches!(eerste_syl.as_deref(), Some("end") | Some("middle"));
                            let streep_na = matches!(laatste_syl.as_deref(), Some("begin") | Some("middle"));
                            let syllabic = match (streep_voor, streep_na) { (true, true) => Syllabic::Middle, (true, false) => Syllabic::End, (false, true) => Syllabic::Begin, (false, false) => Syllabic::Single };
                            let extend = kind(l, "extend").is_some();
                            if (!text.is_empty() || extend) && !lyrics.iter().any(|x| x.number == number) {
                                lyrics.push(Lyric { number, text, syllabic, extend });
                            }
                        }
                        let mut articulations: Vec<Articulation> = Vec::new();
                        let mut zet = |a: Articulation, lijst: &mut Vec<Articulation>| { if !lijst.contains(&a) { lijst.push(a); } };
                        for nt in el.children().filter(|c| c.has_tag_name("notations")) {
                            for c in nt.children().filter(|c| c.is_element()) {
                                match c.tag_name().name() {
                                    "articulations" => {
                                        for a in c.children().filter(|a| a.is_element()) {
                                            match a.tag_name().name() {
                                                "staccato" | "staccatissimo" | "spiccato" => zet(Articulation::Staccato, &mut articulations),
                                                "tenuto" | "detached-legato" => zet(Articulation::Tenuto, &mut articulations),
                                                "accent" | "strong-accent" => zet(Articulation::Accent, &mut articulations),
                                                "breath-mark" | "caesura" => zet(Articulation::Breath, &mut articulations),
                                                _ => {}
                                            }
                                        }
                                    }
                                    "fermata" => zet(Articulation::Fermata, &mut articulations),
                                    _ => {}
                                }
                            }
                        }
                        let spelling = if kind(el, "accidental").is_some() && (-2..=2).contains(&alter) { Some(Spelling { step, alter }) } else { None };
                        verwerk_slurs(el, staff, id, laag_van(staff), &mut pending_slurs, &mut spans, &mut sc);
                        let lijst = &mut noten_per_laag[laag_index_basis + staff - 1].1;
                        if tie_start { pending_ties.insert(sleutel, lijst.len()); }
                        lijst.push(ImportEv { id, midi, start_q, end_q, voice, lyrics, articulations, spelling });
                    }
                    "backup" => { cursor -= getal::<f64>(el, "duration").unwrap_or(0.0); if cursor < 0.0 { cursor = 0.0; } }
                    "forward" => { cursor += getal::<f64>(el, "duration").unwrap_or(0.0); bereik = bereik.max(cursor); }
                    "direction" => {
                        let placement = if el.attribute("placement") == Some("below") { Placement::Below } else { Placement::Above };
                        let staff = getal::<usize>(el, "staff").unwrap_or(1).clamp(1, staves);
                        let offset_div = getal::<f64>(el, "offset").unwrap_or(0.0);
                        let pos_q = maat_start_q + (cursor + offset_div).max(0.0) / divisions;
                        let sound_tempo = kind(el, "sound").and_then(|s| s.attribute("tempo")).and_then(|t| t.parse::<f64>().ok());
                        let heeft_words = el.children().filter(|c| c.has_tag_name("direction-type")).any(|dt| kind(dt, "words").is_some());
                        if let Some(t) = sound_tempo {
                            // Bij een tekst ("Allegro 120") hoort het tempo bij de tekst (eigen
                            // export); alleen zonder tekst telt het als het stuktempo.
                            match bpm { None => bpm = Some(t), Some(b) if !heeft_words && (b - t).abs() > 0.5 => w.meld("Tempowisselingen onderweg zijn genegeerd (het eerste tempo geldt)"), _ => {} }
                        }
                        for dt in el.children().filter(|c| c.has_tag_name("direction-type")) {
                            for c in dt.children().filter(|c| c.is_element()) {
                                match c.tag_name().name() {
                                    "words" => {
                                        let t = c.text().map(str::trim).unwrap_or("").to_string();
                                        if !t.is_empty() {
                                            let soort = if sound_tempo.is_some() { TextKind::Tempo } else { TextKind::Free };
                                            let tid = sc.new_text_id();
                                            teksten.push((TextMark { id: tid, layer_id: laag_van(staff), start_us: 0, kind: soort, text: t, placement }, pos_q));
                                        }
                                    }
                                    "dynamics" => {
                                        let naam = c.children().find(|d| d.is_element()).map(|d| d.tag_name().name().to_string())
                                            .or_else(|| c.text().map(|t| t.trim().to_string())).unwrap_or_default();
                                        if !naam.is_empty() {
                                            let tid = sc.new_text_id();
                                            teksten.push((TextMark { id: tid, layer_id: laag_van(staff), start_us: 0, kind: TextKind::Dynamic, text: naam, placement }, pos_q));
                                        }
                                    }
                                    "rehearsal" => {
                                        let t = c.text().map(str::trim).unwrap_or("").to_string();
                                        if !t.is_empty() {
                                            let tid = sc.new_text_id();
                                            teksten.push((TextMark { id: tid, layer_id: laag_van(staff), start_us: 0, kind: TextKind::Rehearsal, text: t, placement }, pos_q));
                                        }
                                    }
                                    "metronome" => {
                                        // sound@tempo in dezelfde direction staat al in kwarten.
                                        if sound_tempo.is_none() {
                                            if let Some(pm) = getal::<f64>(c, "per-minute") {
                                                let eenheid = tekst(c, "beat-unit").unwrap_or_else(|| "quarter".into());
                                                let basis = match eenheid.as_str() { "whole" => 4.0, "half" => 2.0, "quarter" => 1.0, "eighth" => 0.5, "16th" => 0.25, "32nd" => 0.125, _ => 1.0 };
                                                let punten = c.children().filter(|b| b.has_tag_name("beat-unit-dot")).count() as i32;
                                                let kw = pm * basis * 1.5f64.powi(punten);
                                                match bpm { None => bpm = Some(kw), Some(b) if (b - kw).abs() > 0.5 => w.meld("Tempowisselingen onderweg zijn genegeerd (het eerste tempo geldt)"), _ => {} }
                                            }
                                        }
                                    }
                                    "wedge" => {
                                        match c.attribute("type") {
                                            Some("crescendo") => open_lijnen.push((staff, SpanKind::Crescendo, pos_q)),
                                            Some("diminuendo") => open_lijnen.push((staff, SpanKind::Diminuendo, pos_q)),
                                            Some("stop") => {
                                                if let Some(i) = open_lijnen.iter().rposition(|(s, k, _)| *s == staff && matches!(k, SpanKind::Crescendo | SpanKind::Diminuendo)) {
                                                    let (s, k, van) = open_lijnen.remove(i);
                                                    dichte_lijnen.push((s, k, van, pos_q));
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    "octave-shift" => {
                                        let size: i32 = c.attribute("size").and_then(|v| v.parse::<i32>().ok()).unwrap_or(8);
                                        let halve = if size >= 15 { 24 } else { 12 };
                                        match c.attribute("type") {
                                            // "down" = geschreven een octaaf lager dan klinkend (8va).
                                            Some("down") => { octaaf_actief.insert(staff, halve); open_lijnen.push((staff, SpanKind::OctaveUp, pos_q)); }
                                            Some("up") => { octaaf_actief.insert(staff, -halve); open_lijnen.push((staff, SpanKind::OctaveDown, pos_q)); }
                                            Some("stop") => {
                                                octaaf_actief.remove(&staff);
                                                if let Some(i) = open_lijnen.iter().rposition(|(s, k, _)| *s == staff && matches!(k, SpanKind::OctaveUp | SpanKind::OctaveDown)) {
                                                    let (s, k, van) = open_lijnen.remove(i);
                                                    dichte_lijnen.push((s, k, van, pos_q));
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "sound" => {
                        if let Some(t) = el.attribute("tempo").and_then(|t| t.parse::<f64>().ok()) {
                            match bpm { None => bpm = Some(t), Some(b) if (b - t).abs() > 0.5 => w.meld("Tempowisselingen onderweg zijn genegeerd (het eerste tempo geldt)"), _ => {} }
                        }
                    }
                    "barline" => {
                        if pi != 0 { continue; }
                        let links = el.attribute("location") == Some("left");
                        let stijl = tekst(el, "bar-style");
                        let repeat = kind(el, "repeat").and_then(|r| r.attribute("direction").map(str::to_string));
                        let ending = kind(el, "ending").map(|e| (
                            e.attribute("number").unwrap_or("1").split(',').next().unwrap_or("1").trim().parse::<u8>().unwrap_or(1),
                            e.attribute("type").unwrap_or("start").to_string()));
                        let b = balk_attr(&mut bars, m_index as u32);
                        if links {
                            if repeat.as_deref() == Some("forward") { b.left = Some(BarStyle::RepeatStart); }
                            if let Some((n, t)) = ending { if t == "start" { ending_actief = Some(n); b.ending = Some(n); } }
                        } else {
                            if repeat.as_deref() == Some("backward") { b.right = Some(BarStyle::RepeatEnd); }
                            else if let Some(st) = stijl.as_deref() {
                                match st {
                                    "light-light" => b.right = Some(BarStyle::Double),
                                    "light-heavy" | "heavy" | "heavy-heavy" => b.right = Some(BarStyle::Final),
                                    _ => {}
                                }
                            }
                            if let Some((_, t)) = ending { if t == "stop" || t == "discontinue" { ending_actief = None; } }
                        }
                    }
                    _ => {}
                }
            }
            // Maatlengte uit dezelfde geklemde maatsoort als het stuk straks krijgt.
            let maat_len_q = match maatsoort {
                Some((b, u)) => b.clamp(1, 12) as f64 * 4.0 / klem_beat_unit(u) as f64,
                None => bereik / divisions,
            };
            // Opmaat (implicit of een te korte eerste maat): de inhoud tegen het
            // maateinde, niet op tel 1 — het stuk kent geen opmaatveld.
            let inhoud_q = bereik / divisions;
            let is_opmaat = measure.attribute("implicit") == Some("yes") || m_index == 0;
            if is_opmaat && maatsoort.is_some() && inhoud_q > 0.0 && inhoud_q + 1e-6 < maat_len_q {
                let off = maat_len_q - inhoud_q;
                for (li, l) in noten_per_laag.iter_mut().enumerate() {
                    for e in &mut l.1[n0[li]..] { e.start_q += off; e.end_q += off; }
                }
                for t in &mut teksten[t0..] { t.1 += off; }
                for l in &mut open_lijnen[o0..] { l.2 += off; }
                for l in &mut dichte_lijnen[d0..] { l.2 += off; l.3 += off; }
            }
            maat_start_q += maat_len_q.max(inhoud_q);
            m_index += 1;
        }
        if !open_lijnen.is_empty() { w.meld("Een haarspeld of octavering zonder einde is weggelaten"); }
        // Haarspelden en octaveringen aan noten hangen: van de eerste noot op of
        // na het begin tot de laatste noot die vóór het einde begint.
        for (staff, soort, van_q, tot_q) in dichte_lijnen {
            let lijst = &noten_per_laag[laag_index_basis + staff - 1].1;
            let from = lijst.iter().filter(|e| e.start_q >= van_q - 1e-6).min_by(|a, b| a.start_q.partial_cmp(&b.start_q).unwrap_or(std::cmp::Ordering::Equal).then(b.midi.cmp(&a.midi)));
            let to = lijst.iter().filter(|e| e.start_q < tot_q - 1e-6).max_by(|a, b| a.start_q.partial_cmp(&b.start_q).unwrap_or(std::cmp::Ordering::Equal).then(a.midi.cmp(&b.midi)));
            if let (Some(f), Some(t)) = (from, to) {
                if f.id != t.id && t.start_q > f.start_q {
                    let sid = sc.new_span_id();
                    spans.push(Span { id: sid, layer_id: laag_van(staff), kind: soort, from_event: f.id, to_event: t.id });
                }
            }
        }
        max_maten = max_maten.max(m_index);
    }
    if noten_per_laag.is_empty() { return Err("De partituur bevat geen parts".into()); }

    // Instellingen van het stuk.
    let bpm = bpm.filter(|b| b.is_finite() && (20.0..=300.0).contains(b)).unwrap_or(90.0);
    sc.bpm = bpm;
    if let Some((b, u)) = maatsoort {
        if b != b.clamp(1, 12) || u != klem_beat_unit(u) {
            w.meld(&format!("Maatsoort {}/{} is niet beschikbaar; de maten zijn herverdeeld als {}/{}", b, u, b.clamp(1, 12), klem_beat_unit(u)));
        }
        sc.beats_per_bar = b.clamp(1, 12); sc.beat_unit = klem_beat_unit(u);
    }
    if let Some((f, minor)) = toonsoort { sc.key_fifths = f.clamp(-7, 7); sc.minor = minor; }
    let basis = eerste_divisions.filter(|d| [1u64, 2, 4, 8].contains(d)).unwrap_or(1);
    sc.quantize = basis.max(fijnste_q).clamp(1, 8) as u8;
    sc.min_measures = (max_maten as u32).min(MAX_MIN_MEASURES);
    sc.tolerance_pct = 100;
    let naar_us = |q: f64| -> u64 { (q.max(0.0) * 60e6 / bpm).round() as u64 };
    for (laag_id, lijst) in noten_per_laag {
        let Some(laag) = sc.layers.iter_mut().find(|l| l.id == laag_id) else { continue; };
        let Some(take) = laag.takes.first_mut() else { continue; };
        for e in lijst {
            let start_us = naar_us(e.start_q);
            let end_us = naar_us(e.end_q).max(start_us + 1000);
            take.events.push(LayerEv {
                id: e.id, midi: e.midi, start_us, end_us, channel: KANAAL_ONBEKEND, locked: true, voice: e.voice,
                lyrics: e.lyrics, articulations: e.articulations, spelling: e.spelling, hand: None,
            });
        }
        take.events.sort_by_key(|e| (e.start_us, e.midi));
    }
    sc.texts = teksten.into_iter().map(|(mut t, q)| { t.start_us = naar_us(q); t }).collect();
    sc.spans = spans;
    sc.bars = bars;
    sc.herleid_na_laden();
    Ok(ImportResult { score: sc, warnings: w.lijst })
}

/// Bogen van een <note> (0.7.90): stops eerst (een boog die hier eindigt en
/// een nieuwe die hier begint delen vaak hetzelfde nummer), dan de starts.
fn verwerk_slurs(el: Node, staff: usize, id: u64, laag_id: u32, pending: &mut HashMap<(usize, String), u64>, spans: &mut Vec<Span>, sc: &mut Score) {
    let slurs: Vec<Node> = el.children().filter(|c| c.has_tag_name("notations")).flat_map(|n| n.children()).filter(|c| c.has_tag_name("slur")).collect();
    for c in slurs.iter().filter(|c| c.attribute("type") == Some("stop")) {
        let nummer = c.attribute("number").unwrap_or("1").to_string();
        if let Some(from) = pending.remove(&(staff, nummer)) {
            if from != id {
                let sid = sc.new_span_id();
                spans.push(Span { id: sid, layer_id: laag_id, kind: SpanKind::Slur, from_event: from, to_event: id });
            }
        }
    }
    for c in slurs.iter().filter(|c| c.attribute("type") == Some("start")) {
        pending.insert((staff, c.attribute("number").unwrap_or("1").to_string()), id);
    }
}

fn balk_attr(bars: &mut Vec<BarAttr>, m: u32) -> &mut BarAttr {
    if let Some(i) = bars.iter().position(|b| b.measure == m) { return &mut bars[i]; }
    bars.push(BarAttr { measure: m, left: None, right: None, ending: None });
    bars.last_mut().expect("zojuist toegevoegd")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::build_musicxml_from_score;

    /// De sterkste test: elke eigen fixture lezen en opnieuw schrijven geeft
    /// byte voor byte de fixture terug.
    #[test]
    fn rondreis_eigen_fixtures() {
        for (naam, fixture) in [
            ("0769", include_str!("testdata/notatie_0769.musicxml")),
            ("0770", include_str!("testdata/notatie_0770.musicxml")),
            ("0783", include_str!("testdata/notatie_0783.musicxml")),
        ] {
            let r = read_musicxml(fixture).unwrap_or_else(|e| panic!("{}: {}", naam, e));
            assert!(r.warnings.is_empty(), "{}: {:?}", naam, r.warnings);
            let xml = build_musicxml_from_score(&r.score).unwrap();
            assert_eq!(xml, fixture, "fixture {} komt niet terug", naam);
        }
    }

    const KOP: &str = r#"<?xml version="1.0" encoding="UTF-8"?><score-partwise version="3.1"><part-list><score-part id="P1"><part-name>Manuaal</part-name></score-part></part-list>"#;
    const ATTR: &str = r#"<attributes><divisions>4</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>"#;

    #[test]
    fn import_ties_samengevoegd_en_akkoord() {
        let xml = format!(r#"{KOP}<part id="P1"><measure number="1">{ATTR}
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><tie type="start"/><voice>1</voice><type>quarter</type></note>
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><tie type="stop"/><voice>1</voice><type>quarter</type></note>
            <note><pitch><step>E</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><type>half</type></note>
            <note><chord/><pitch><step>G</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><type>half</type></note>
            </measure></part></score-partwise>"#);
        let r = read_musicxml(&xml).unwrap();
        let ev = &r.score.layers[0].takes[0].events;
        assert_eq!(ev.len(), 3);
        assert_eq!((ev[0].midi, ev[0].start_us, ev[0].end_us), (60, 0, 1_333_333));
        assert_eq!((ev[1].midi, ev[1].start_us), (64, 1_333_333));
        assert_eq!((ev[2].midi, ev[2].start_us), (67, 1_333_333));
        assert_eq!(r.score.min_measures, 1);
        assert_eq!(r.score.quantize, 4);
    }

    #[test]
    fn import_staves_naar_lagen_en_stemmen() {
        let xml = format!(r#"{KOP}<part id="P1"><measure number="1"><attributes><divisions>2</divisions><staves>2</staves><clef number="1"><sign>G</sign><line>2</line></clef><clef number="2"><sign>F</sign><line>4</line></clef></attributes>
            <note><pitch><step>C</step><octave>5</octave></pitch><duration>8</duration><voice>1</voice><staff>1</staff></note>
            <backup><duration>8</duration></backup>
            <note><pitch><step>C</step><octave>3</octave></pitch><duration>8</duration><voice>5</voice><staff>2</staff></note>
            </measure></part></score-partwise>"#);
        let r = read_musicxml(&xml).unwrap();
        assert_eq!(r.score.layers.len(), 2);
        assert_eq!(r.score.layers[0].name, "Manuaal 1");
        assert_eq!(r.score.layers[1].name, "Manuaal 2");
        assert_eq!(r.score.layers[1].bass_clef, Some(true));
        assert_eq!(r.score.layers[1].takes[0].events[0].voice, 1, "stem 5 op balk 2 = stem 1 van die balk");
        assert!(!r.warnings.iter().any(|w| w.contains("vier stemmen")), "{:?}", r.warnings);
    }

    #[test]
    fn import_opmaat_aan_het_maateinde() {
        let xml = format!(r#"{KOP}<part id="P1"><measure number="0" implicit="yes">{ATTR}
            <note><pitch><step>G</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice></note>
            </measure><measure number="1">
            <note><pitch><step>C</step><octave>5</octave></pitch><duration>16</duration><voice>1</voice></note>
            </measure></part></score-partwise>"#);
        let r = read_musicxml(&xml).unwrap();
        let ev = &r.score.layers[0].takes[0].events;
        // 90 bpm: een kwart = 666 667 µs; de opmaat staat op tel 4 (3 kwarten), maat 2 op 4 kwarten.
        assert_eq!(ev[0].start_us, 2_000_000);
        assert_eq!(ev[1].start_us, 2_666_667);
        assert_eq!(r.score.min_measures, 2);
    }

    #[test]
    fn import_elisie_metronoom_en_boog_op_vervolgnoot() {
        let xml = format!(r#"{KOP}<part id="P1"><measure number="1">{ATTR}
            <direction placement="above"><direction-type><metronome><beat-unit>quarter</beat-unit><beat-unit-dot/><per-minute>60</per-minute></metronome></direction-type></direction>
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice><notations><slur type="start" number="1"/></notations><lyric number="1"><syllabic>end</syllabic><text>a</text><elision>‿</elision><syllabic>single</syllabic><text>e</text></lyric></note>
            <note><pitch><step>D</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice><notations><slur type="start" number="1"/><slur type="stop" number="1"/></notations></note>
            <note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice><tie type="start"/></note>
            <note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice><tie type="stop"/><notations><slur type="stop" number="1"/></notations></note>
            </measure></part></score-partwise>"#);
        let r = read_musicxml(&xml).unwrap();
        assert!((r.score.bpm - 90.0).abs() < 1e-6, "gepunteerde kwart = 60 → 90 kwarten");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let ev = &r.score.layers[0].takes[0].events;
        assert_eq!(ev.len(), 3);
        assert_eq!(ev[0].lyrics[0].text, "a‿e");
        assert_eq!(ev[0].lyrics[0].syllabic, Syllabic::End);
        // Twee bogen: C→D en D→E (de E is een samengevoegde overgebonden noot).
        assert_eq!(r.score.spans.len(), 2, "{:?}", r.score.spans);
        assert!(r.score.spans.iter().any(|s| s.from_event == ev[0].id && s.to_event == ev[1].id));
        assert!(r.score.spans.iter().any(|s| s.from_event == ev[1].id && s.to_event == ev[2].id));
    }

    #[test]
    fn import_raster_volgt_fijnste_part() {
        let xml = format!(r#"{KOP}<part id="P1"><measure number="1"><attributes><divisions>2</divisions><time><beats>4</beats><beat-type>4</beat-type></time></attributes>
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice></note></measure></part>
            <part id="P2"><measure number="1"><attributes><divisions>4</divisions></attributes>
            <note><pitch><step>C</step><octave>3</octave></pitch><duration>1</duration><voice>1</voice></note>
            <note><pitch><step>D</step><octave>3</octave></pitch><duration>15</duration><voice>1</voice></note></measure></part></score-partwise>"#);
        let r = read_musicxml(&xml).unwrap();
        assert_eq!(r.score.quantize, 4);
    }

    #[test]
    fn import_timewise_weigert_en_triool_meldt() {
        assert!(read_musicxml(r#"<?xml version="1.0"?><score-timewise/>"#).unwrap_err().contains("timewise"));
        let xml = format!(r#"{KOP}<part id="P1"><measure number="1">{ATTR}
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>3</duration><voice>1</voice><time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification></note>
            </measure></part></score-partwise>"#);
        let r = read_musicxml(&xml).unwrap();
        assert!(r.warnings.iter().any(|w| w.contains("Triolen")));
    }

    #[test]
    fn import_haarspeld_octavering_en_herhaling() {
        let xml = format!(r#"{KOP}<part id="P1"><measure number="1">{ATTR}
            <barline location="left"><bar-style>heavy-light</bar-style><repeat direction="forward"/></barline>
            <direction placement="below"><direction-type><wedge type="crescendo"/></direction-type></direction>
            <direction placement="above"><direction-type><octave-shift type="down" size="8"/></direction-type></direction>
            <note><pitch><step>C</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><notations><articulations><staccato/></articulations><slur type="start" number="1"/></notations></note>
            <note><pitch><step>D</step><octave>4</octave></pitch><duration>8</duration><voice>1</voice><notations><fermata/><slur type="stop" number="1"/></notations></note>
            <direction placement="below"><direction-type><wedge type="stop"/></direction-type></direction>
            <direction placement="above"><direction-type><octave-shift type="stop" size="8"/></direction-type></direction>
            <barline location="right"><bar-style>light-heavy</bar-style><ending number="1" type="stop"/><repeat direction="backward"/></barline>
            </measure></part></score-partwise>"#);
        let r = read_musicxml(&xml).unwrap();
        let ev = &r.score.layers[0].takes[0].events;
        assert_eq!((ev[0].midi, ev[1].midi), (72, 74), "8va: klinkend een octaaf hoger");
        assert_eq!(ev[0].articulations, vec![Articulation::Staccato]);
        assert_eq!(ev[1].articulations, vec![Articulation::Fermata]);
        let soorten: Vec<SpanKind> = r.score.spans.iter().map(|s| s.kind).collect();
        assert!(soorten.contains(&SpanKind::Slur) && soorten.contains(&SpanKind::Crescendo) && soorten.contains(&SpanKind::OctaveUp), "{:?}", soorten);
        assert!(r.score.spans.iter().all(|s| s.from_event == ev[0].id && s.to_event == ev[1].id));
        assert_eq!(r.score.bars.len(), 1);
        assert_eq!((r.score.bars[0].left, r.score.bars[0].right), (Some(BarStyle::RepeatStart), Some(BarStyle::RepeatEnd)));
    }

    #[test]
    fn import_mxl_uit_zip() {
        let xml = format!(r#"{KOP}<part id="P1"><measure number="1">{ATTR}<note><pitch><step>A</step><octave>4</octave></pitch><duration>16</duration><voice>1</voice></note></measure></part></score-partwise>"#);
        let dir = std::env::temp_dir().join(format!("jm_mxl_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pad = dir.join("stuk.mxl");
        {
            let f = std::fs::File::create(&pad).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opt: zip::write::SimpleFileOptions = Default::default();
            use std::io::Write;
            zw.start_file("META-INF/container.xml", opt).unwrap();
            zw.write_all(br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="stuk.xml"/></rootfiles></container>"#).unwrap();
            zw.start_file("stuk.xml", opt).unwrap();
            zw.write_all(xml.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        let r = read_musicxml_path(&pad).unwrap();
        assert_eq!(r.score.layers[0].takes[0].events[0].midi, 69);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
