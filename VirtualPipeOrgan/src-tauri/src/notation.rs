//! Muzieknotatie: MIDI-opname → MusicXML (partwise), volledig in eigen beheer
//! (geen externe software). De frontend rendert het resultaat met
//! OpenSheetMusicDisplay in het notatievenster.
//!
//! Opzet (fase 1):
//! - MIDI-bestand (de eigen opname of een willekeurige .mid) → noot-events met
//!   absolute seconden (tempo-meta's worden gevolgd, zoals vpo-midi's speler);
//! - noten per divisie/notenbalk gegroepeerd (de aanroeper levert de toewijzing
//!   op basis van de kanaal→divisie-mapping van het orgel; Pedaal → bassleutel);
//! - kwantisatie naar een instelbaar raster (bij het door de gebruiker gekozen
//!   tempo), gelijktijdige noten → akkoorden, rusten ingevuld, maten gesplitst
//!   met overbindingen (ties);
//! - toonhoogte-spelling op basis van de gekozen toonsoort (kruizen/mollen).
//!
//! Sinds 0.7.70 is de kwantisatie uitgelicht tot een gedeeld model
//! (`quantize_score` → `QuantizedScore` met per noot het event-ID): het
//! notenschrift (`render_musicxml`) en de klavar-weergave (`klavar.rs`) lezen
//! allebei dat model, zodat ze dezelfde maten tonen.
//!
//! Bewuste v1-vereenvoudigingen van het NOTENSCHRIFT (gedocumenteerd in de
//! CHANGELOG); klavar slaat ze over en tekent polyfoon:
//! - één stem per notenbalk: overlappende noten worden ingekort tot de volgende
//!   inzet (homofoon orgelspel werkt goed; polyfonie volgt in fase 2);
//! - akkoordduur = de langste noot van het akkoord.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct NotationOptions {
    /// Muzikaal tempo waarin de opname genoteerd wordt (kwartnoten per minuut).
    pub bpm: f64,
    /// Maatsoort-teller (noemer is altijd 4 in v1): 2, 3, 4, 6, ...
    pub beats_per_bar: u8,
    /// Rastereenheden per kwartnoot: 1 = kwart, 2 = achtste, 4 = zestiende, 8 = 32e.
    pub quantize: u8,
    /// Toonsoort als kwintenaantal (-7..7): 0 = C, 1 = G, -1 = F, enz.
    pub key_fifths: i8,
    /// Titel boven de partituur.
    pub title: Option<String>,
    /// Vrije balk-indeling: per notenbalk de aangevinkte divisies (meerdere
    /// divisies per balk mag; dezelfde divisie mag op meerdere balken).
    /// None/leeg = automatisch één balk per divisie.
    #[serde(default)]
    pub staves: Option<Vec<StaffSpec>>,
    /// Kwantisatie-tolerantie 0..=100 ("los" ↔ "strak"): 100 = hard raster,
    /// 0 = losser (afwijkingen krijgen een fijner sub-raster). None = 100.
    #[serde(default)]
    pub tolerance_pct: Option<u8>,
    /// Mineur (0.7.72): voor het toonsoortteken in klavar (ruit i.p.v. cirkel)
    /// en `<mode>minor</mode>` in de MusicXML. None = majeur.
    #[serde(default)]
    pub minor: Option<bool>,
}

/// Door de gebruiker samengestelde notenbalk (notatievenster → backend).
#[derive(Debug, Clone, Deserialize)]
pub struct StaffSpec {
    /// Naam boven/naast de balk; None = namen van de divisies.
    pub name: Option<String>,
    /// Divisienamen waarvan de noten op deze balk komen.
    pub divisions: Vec<String>,
    /// Bassleutel; None = automatisch (bas wanneer een pedaal-divisie meedoet).
    pub bass_clef: Option<bool>,
    /// Hand in klavar (0.7.70); None = standaardregel.
    #[serde(default)]
    pub klavar_hand: Option<KlavarHand>,
    /// Splitspunt bij R+L (MIDI-nummer): eronder links, erop en erboven rechts.
    #[serde(default)]
    pub klavar_split: Option<u8>,
}

/// Hand van een noot of balk in klavar (0.7.70): stok naar rechts, naar
/// links, of de pedaalbalk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KlavarHand { Right, Left, Pedal }

/// Eén (getransponeerde) noot, toegewezen aan een notenbalk.
#[derive(Debug, Clone)]
pub struct NoteEv {
    pub midi: u8,
    pub start_sec: f64,
    pub end_sec: f64,
    /// `LayerEv.id` in de live-flow, zodat een getekende noot (klavar) zijn
    /// event kent; None in de bestandsmodus en in tests.
    pub id: Option<u64>,
    /// Per-noot-hand (klavar); wint van het splitspunt en de laaghand.
    pub hand: Option<KlavarHand>,
}

impl NoteEv {
    /// Noot zonder event-ID (bestandsmodus, tests).
    pub fn anoniem(midi: u8, start_sec: f64, end_sec: f64) -> Self {
        NoteEv { midi, start_sec, end_sec, id: None, hand: None }
    }
}

/// Eén notenbalk (divisie) met zijn noten.
#[derive(Debug, Clone, Default)]
pub struct Staff {
    pub name: String,
    pub bass_clef: bool,
    pub notes: Vec<NoteEv>,
    /// `Layer.id` in de live-flow; None in de bestandsmodus.
    pub layer_id: Option<u32>,
    /// Pedaalbalk (op naam of divisie, niet op de sleutel: een manuaal mag
    /// in de bassleutel staan).
    pub pedal: bool,
    /// Hand van de balk in klavar; None = standaardregel (zie klavar.rs).
    pub hand: Option<KlavarHand>,
    /// Splitspunt bij R+L: noten onder dit MIDI-nummer links, erop en erboven rechts.
    pub split_midi: Option<u8>,
}

/// Pedaalbalk op naam: "Pedaal", "Pedal", "Pédale", "Pedał", "PED" — "ped"
/// aan een woordbegin, met diakrieten gestript. Eén plek voor de drie
/// aanroepers (notatie, bestandsmodus, nieuwe score).
pub fn is_pedal_name(s: &str) -> bool {
    // Diakrieten: vooraf samengesteld (é) én los combinerend (e + U+0301,
    // zoals macOS-bestandssystemen en sommige ODF-editors ze leveren).
    let plat: String = s.to_lowercase().chars()
        .filter(|c| !('\u{0300}'..='\u{036F}').contains(c))
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'ł' => 'l',
            'á' | 'à' | 'â' | 'ä' => 'a',
            _ => c,
        }).collect();
    plat.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with("ped"))
}

/// Ruwe noot uit het MIDI-bestand (nog niet aan een balk toegewezen).
#[derive(Debug, Clone)]
pub struct RawNote {
    pub channel: u8,
    pub note: u8,
    pub start_sec: f64,
    pub end_sec: f64,
}

/// Parse een SMF-bestand naar noten met absolute seconden. Volgt tempo-meta's
/// (zelfde aanpak als vpo-midi's speler); noten zonder note-off worden op het
/// einde van het bestand afgekapt.
pub fn extract_notes(bytes: &[u8]) -> Result<Vec<RawNote>, String> {
    use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

    let smf = Smf::parse(bytes).map_err(|e| format!("MIDI-bestand niet leesbaar: {}", e))?;
    let ppq: u64 = match smf.header.timing {
        Timing::Metrical(t) => t.as_int() as u64,
        Timing::Timecode(fps, sub) => {
            // Zeldzaam; benader met een vaste tijdsbasis.
            let _ = (fps, sub);
            return Err("SMPTE-tijdsbasis wordt niet ondersteund".into());
        }
    };
    if ppq == 0 {
        return Err("Ongeldige MIDI-tijdsbasis (ppq=0)".into());
    }

    // Events uit alle tracks samenvoegen op absolute ticks.
    let mut events: Vec<(u64, TrackEventKind)> = Vec::new();
    for track in smf.tracks.iter() {
        let mut tick: u64 = 0;
        for ev in track.iter() {
            tick += ev.delta.as_int() as u64;
            events.push((tick, ev.kind.clone()));
        }
    }
    events.sort_by_key(|(t, _)| *t);

    let mut tempo_us_per_qn: u64 = 500_000; // 120 BPM default
    let mut prev_tick: u64 = 0;
    let mut now_us: u64 = 0;
    // (channel, note) → (start_sec)
    let mut open: std::collections::HashMap<(u8, u8), f64> = std::collections::HashMap::new();
    let mut out: Vec<RawNote> = Vec::new();

    for (tick, kind) in events {
        let dticks = tick - prev_tick;
        prev_tick = tick;
        now_us += (dticks as u128 * tempo_us_per_qn as u128 / ppq as u128) as u64;
        let now_sec = now_us as f64 / 1_000_000.0;

        match kind {
            TrackEventKind::Meta(MetaMessage::Tempo(t)) => {
                tempo_us_per_qn = t.as_int() as u64;
            }
            TrackEventKind::Midi { channel, message } => {
                let ch = channel.as_int();
                match message {
                    MidiMessage::NoteOn { key, vel } => {
                        let note = key.as_int();
                        if vel.as_int() == 0 {
                            // Note-on met velocity 0 = note-off.
                            if let Some(start) = open.remove(&(ch, note)) {
                                out.push(RawNote { channel: ch, note, start_sec: start, end_sec: now_sec });
                            }
                        } else {
                            open.entry((ch, note)).or_insert(now_sec);
                        }
                    }
                    MidiMessage::NoteOff { key, .. } => {
                        let note = key.as_int();
                        if let Some(start) = open.remove(&(ch, note)) {
                            out.push(RawNote { channel: ch, note, start_sec: start, end_sec: now_sec });
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    // Hangende noten (geen note-off) afsluiten op het laatste event.
    let end_sec = now_us as f64 / 1_000_000.0;
    for ((ch, note), start) in open {
        out.push(RawNote { channel: ch, note, start_sec: start, end_sec: end_sec.max(start + 0.1) });
    }
    out.sort_by(|a, b| a.start_sec.partial_cmp(&b.start_sec).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

// ---- Kwantisatie ----

#[derive(Debug, Clone, PartialEq)]
struct Chord {
    start: u64,        // in rastereenheden
    end: u64,          // exclusief
    notes: Vec<u8>,    // MIDI-noten (gesorteerd)
}

/// Eén gekwantiseerde noot (0.7.70): alleen het raster toegepast, verder
/// niets. Het gedeelde tussenmodel van notenschrift en klavar.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct QNote {
    pub id: Option<u64>,
    pub midi: u8,
    /// Rastereenheden.
    pub start: u64,
    /// Exclusief; minimaal start + 1.
    pub end: u64,
    pub hand: Option<KlavarHand>,
}

/// Kwantiseer de noten van één balk naar rastereenheden (gesorteerd op inzet
/// en toonhoogte, minimaal één eenheid lang). Geen akkoordgroepering en geen
/// inkorting: dat doet `group_chords` voor het notenschrift.
///
/// `tolerance_pct`: 0..=100 — schuif "los ↔ strak":
/// - 100 = hard kwantiseren: elke noot naar het raster (huidig gedrag);
/// - 0   = losser: noten die verder van het raster af zitten dan de tolerantie
///   worden op een fijner sub-raster (halveringen, tot 8× fijner) neergezet.
/// Dit vermijdt onhoorbare drift zonder de partituur onleesbaar te maken.
pub(crate) fn quantize_notes(notes: &[NoteEv], bpm: f64, q: u8, tolerance_pct: u8) -> Vec<QNote> {
    let grid_per_sec = (bpm / 60.0) * q as f64;
    // Bij 100% tolerantie mag een noot maximaal 0,5 rastereenheid afwijken en
    // wordt hij toch gekwantiseerd — dat is precies het huidige gedrag.
    // Bij 0% tolerantie is de drempel klein (0,05) → bijna elke afwijking
    // duwt de noot naar een fijner sub-raster.
    let tol = (tolerance_pct.min(100) as f64 / 100.0) * 0.45 + 0.05;
    let quantize_one = |t: f64| -> u64 {
        if t <= 0.0 { return 0; }
        let base = t * grid_per_sec;
        let rounded = base.round();
        let mut diff = (base - rounded).abs();
        if diff <= tol { return rounded.max(0.0) as u64; }
        // Verfijn: sub-raster 1/2, 1/4, 1/8 van de rastereenheid.
        for sub in [2.0_f64, 4.0, 8.0] {
            let r = (base * sub).round() / sub;
            diff = (base - r).abs();
            if diff <= tol { return r.max(0.0).round() as u64; }
        }
        // Uiterste ondergrens: naar het 1/8-sub-raster.
        ((base * 8.0).round() / 8.0).max(0.0).round() as u64
    };
    let mut quantized: Vec<QNote> = notes.iter().map(|n| {
        let s = quantize_one(n.start_sec);
        let e = quantize_one(n.end_sec);
        let e = e.max(s + 1);
        QNote { id: n.id, midi: n.midi, start: s, end: e, hand: n.hand }
    }).collect();
    quantized.sort_by_key(|n| (n.start, n.midi));
    quantized
}

/// Akkoordgroepering voor het notenschrift: gelijke inzetten worden één
/// akkoord (dezelfde toon twee keer: de eerste wint, het tweede ID valt
/// weg), akkoordduur = de langste noot, en de v1-inkorting tot de volgende
/// inzet (één stem per balk). Klavar slaat dit over.
fn group_chords(qnotes: &[QNote]) -> Vec<Chord> {
    // Groepeer per gelijke start; akkoordduur = langste noot van de groep.
    let mut chords: Vec<Chord> = Vec::new();
    for n in qnotes {
        match chords.last_mut() {
            Some(c) if c.start == n.start => {
                if !c.notes.contains(&n.midi) { c.notes.push(n.midi); }
                if n.end > c.end { c.end = n.end; }
            }
            _ => chords.push(Chord { start: n.start, end: n.end, notes: vec![n.midi] }),
        }
    }
    // Eén stem per balk: vorige akkoord inkorten tot de volgende inzet.
    for i in 1..chords.len() {
        if chords[i - 1].end > chords[i].start {
            chords[i - 1].end = chords[i].start;
        }
    }
    for c in chords.iter_mut() {
        c.notes.sort_unstable();
    }
    chords
}

// ---- Duur → notenwaarden ----

/// Toegestane notenwaarden in rastereenheden voor raster q (eenheden per
/// kwartnoot), inclusief enkelvoudig gepunteerde waarden — grootste eerst.
fn allowed_durations(q: u64) -> Vec<(u64, &'static str, bool)> {
    let base: [(u64, &'static str); 6] = [
        (4 * q, "whole"), (2 * q, "half"), (q, "quarter"),
        (q / 2, "eighth"), (q / 4, "16th"), (q / 8, "32nd"),
    ];
    let mut out: Vec<(u64, &'static str, bool)> = Vec::new();
    for (len, name) in base {
        if len == 0 { continue; }
        let dotted = len + len / 2;
        if len >= 2 && len % 2 == 0 && dotted > len {
            out.push((dotted, name, true));
        }
        out.push((len, name, false));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.dedup_by_key(|x| x.0);
    out
}

/// Ontleed een duur (in rastereenheden) in notenwaarden, grootste eerst.
fn decompose(len: u64, q: u64) -> Vec<(u64, &'static str, bool)> {
    let allowed = allowed_durations(q);
    let mut rest = len;
    let mut out = Vec::new();
    while rest > 0 {
        let &(l, name, dot) = allowed.iter().find(|&&(l, _, _)| l <= rest)
            .unwrap_or(allowed.last().expect("allowed_durations is nooit leeg"));
        out.push((l.min(rest), name, dot && l <= rest));
        rest = rest.saturating_sub(l);
    }
    out
}

// ---- Toonhoogte-spelling ----

/// pitch-class → (letter, alteratie) op basis van de toonsoort:
/// mollen bij key_fifths < 0, anders kruizen.
fn spell(midi: u8, key_fifths: i8) -> (char, i8, i8) {
    let pc = (midi % 12) as usize;
    let octave = (midi / 12) as i8 - 1;
    let sharps: [(char, i8); 12] = [
        ('C', 0), ('C', 1), ('D', 0), ('D', 1), ('E', 0), ('F', 0),
        ('F', 1), ('G', 0), ('G', 1), ('A', 0), ('A', 1), ('B', 0),
    ];
    let flats: [(char, i8); 12] = [
        ('C', 0), ('D', -1), ('D', 0), ('E', -1), ('E', 0), ('F', 0),
        ('G', -1), ('G', 0), ('A', -1), ('A', 0), ('B', -1), ('B', 0),
    ];
    let (step, alter) = if key_fifths < 0 { flats[pc] } else { sharps[pc] };
    (step, alter, octave)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&apos;")
}

// ---- MusicXML-opbouw ----

/// Eén gekwantiseerde balk (0.7.70).
#[derive(Debug, Clone, serde::Serialize)]
pub struct QuantizedStaff {
    /// Index in de aangeleverde balkenlijst (ook lege balken tellen daar mee).
    pub staff_index: usize,
    pub name: String,
    pub layer_id: Option<u32>,
    pub pedal: bool,
    pub bass_clef: bool,
    pub hand: Option<KlavarHand>,
    pub split_midi: Option<u8>,
    pub notes: Vec<QNote>,
}

/// Het gedeelde gekwantiseerde model (0.7.70): raster, maatsoort en tempo
/// geklemd zoals het notenschrift ze altijd klemde, en per balk met noten de
/// gekwantiseerde noten mét event-ID. Het notenschrift groepeert hierop
/// akkoorden (`group_chords`) en schrijft MusicXML; klavar tekent de noten
/// rechtstreeks, polyfoon. Het aantal maten hoort hier níet bij: het
/// notenschrift rekent het uit de ingekorte akkoorden, klavar uit de
/// ongekorte einden.
#[derive(Debug, Clone, serde::Serialize)]
pub struct QuantizedScore {
    pub q: u64,
    pub beats_per_bar: u64,
    pub measure_len: u64,
    pub bpm: f64,
    /// Alleen balken met noten, in balkvolgorde (de UI leunt daarop voor de
    /// OSMD-klikcorrelatie: part n = n-de niet-lege balk).
    pub staves: Vec<QuantizedStaff>,
}

/// Kwantiseer alle balken. Een partituur zonder noten geeft een leeg model;
/// het notenschrift maakt daar een fout van (`render_musicxml`), klavar een
/// lege balk.
pub fn quantize_score(staves: &[Staff], opts: &NotationOptions) -> QuantizedScore {
    let q = opts.quantize.clamp(1, 8) as u64;
    let beats = opts.beats_per_bar.clamp(1, 12) as u64;
    let bpm = if opts.bpm.is_finite() && opts.bpm >= 20.0 && opts.bpm <= 300.0 { opts.bpm } else { 90.0 };
    let tolerance = opts.tolerance_pct.unwrap_or(100);
    let qstaves: Vec<QuantizedStaff> = staves.iter().enumerate()
        .filter(|(_, st)| !st.notes.is_empty())
        .map(|(i, st)| QuantizedStaff {
            staff_index: i, name: st.name.clone(), layer_id: st.layer_id, pedal: st.pedal,
            bass_clef: st.bass_clef, hand: st.hand, split_midi: st.split_midi,
            notes: quantize_notes(&st.notes, bpm, opts.quantize.clamp(1, 8), tolerance),
        })
        .collect();
    QuantizedScore { q, beats_per_bar: beats, measure_len: beats * q, bpm, staves: qstaves }
}

/// Bouw een partwise MusicXML-document uit de (per balk toegewezen) noten.
pub fn build_musicxml(staves: &[Staff], opts: &NotationOptions) -> Result<String, String> {
    render_musicxml(&quantize_score(staves, opts), opts)
}

/// MusicXML uit het gedeelde model: akkoorden, één stem per balk, rusten en
/// overbindingen (het notenschrift van 0.7.0, byte voor byte; zie de
/// fixture-test).
pub fn render_musicxml(qs: &QuantizedScore, opts: &NotationOptions) -> Result<String, String> {
    let q = qs.q;
    let beats = qs.beats_per_bar;
    let bpm = qs.bpm;
    let measure_len = qs.measure_len;

    let quantized: Vec<(&QuantizedStaff, Vec<Chord>)> = qs.staves.iter()
        .map(|st| (st, group_chords(&st.notes)))
        .collect();
    if quantized.is_empty() {
        return Err("Geen noten gevonden in de opname".into());
    }

    // Totale lengte: langste balk, afgerond op hele maten.
    let total = quantized.iter()
        .flat_map(|(_, cs)| cs.last().map(|c| c.end))
        .max().unwrap_or(0);
    let num_measures = ((total + measure_len - 1) / measure_len).max(1);

    let mut xml = String::with_capacity(64 * 1024);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 3.1 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n");
    xml.push_str("<score-partwise version=\"3.1\">\n");
    if let Some(t) = &opts.title {
        if !t.trim().is_empty() {
            xml.push_str(&format!("  <work><work-title>{}</work-title></work>\n", xml_escape(t)));
        }
    }
    xml.push_str("  <part-list>\n");
    for (idx, (staff, _)) in quantized.iter().enumerate() {
        xml.push_str(&format!(
            "    <score-part id=\"P{}\"><part-name>{}</part-name></score-part>\n",
            idx + 1, xml_escape(&staff.name)));
    }
    xml.push_str("  </part-list>\n");

    for (idx, (staff, chords)) in quantized.iter().enumerate() {
        xml.push_str(&format!("  <part id=\"P{}\">\n", idx + 1));

        // Segmentlijst opbouwen: (start, end, notes-of-leeg=rust)
        let mut segments: Vec<(u64, u64, Vec<u8>)> = Vec::new();
        let mut cursor: u64 = 0;
        for c in chords {
            if c.start > cursor { segments.push((cursor, c.start, Vec::new())); }
            segments.push((c.start, c.end, c.notes.clone()));
            cursor = c.end;
        }
        let staff_total = num_measures * measure_len;
        if cursor < staff_total { segments.push((cursor, staff_total, Vec::new())); }

        for m in 0..num_measures {
            let m_start = m * measure_len;
            let m_end = m_start + measure_len;
            xml.push_str(&format!("    <measure number=\"{}\">\n", m + 1));
            if m == 0 {
                xml.push_str("      <attributes>\n");
                xml.push_str(&format!("        <divisions>{}</divisions>\n", q));
                if opts.minor == Some(true) {
                    xml.push_str(&format!("        <key><fifths>{}</fifths><mode>minor</mode></key>\n", opts.key_fifths.clamp(-7, 7)));
                } else {
                    xml.push_str(&format!("        <key><fifths>{}</fifths></key>\n", opts.key_fifths.clamp(-7, 7)));
                }
                xml.push_str(&format!("        <time><beats>{}</beats><beat-type>4</beat-type></time>\n", beats));
                if staff.bass_clef {
                    xml.push_str("        <clef><sign>F</sign><line>4</line></clef>\n");
                } else {
                    xml.push_str("        <clef><sign>G</sign><line>2</line></clef>\n");
                }
                xml.push_str("      </attributes>\n");
                xml.push_str(&format!("      <direction placement=\"above\"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome></direction-type><sound tempo=\"{}\"/></direction>\n", bpm.round() as u64, bpm.round() as u64));
            }

            for &(s, e, ref notes) in segments.iter() {
                // Deel dat binnen deze maat valt.
                let ps = s.max(m_start);
                let pe = e.min(m_end);
                if pe <= ps { continue; }
                let tie_from_prev = !notes.is_empty() && s < m_start;
                let tie_to_next = !notes.is_empty() && e > m_end;

                // Binnen de maat ontleden in notenwaarden; delen van dezelfde
                // noot binnen de maat ook onderling overbinden.
                let parts = decompose(pe - ps, q);
                let mut part_start = ps;
                for (pi, (len, type_name, dotted)) in parts.iter().enumerate() {
                    let first_part = pi == 0;
                    let last_part = pi == parts.len() - 1;
                    if notes.is_empty() {
                        xml.push_str(&format!(
                            "      <note><rest/><duration>{}</duration><voice>1</voice><type>{}</type>{}</note>\n",
                            len, type_name, if *dotted { "<dot/>" } else { "" }));
                    } else {
                        let tie_start = tie_to_next || !last_part;
                        let tie_stop = tie_from_prev || !first_part;
                        for (ni, &midi) in notes.iter().enumerate() {
                            let (step, alter, octave) = spell(midi, opts.key_fifths);
                            xml.push_str("      <note>");
                            if ni > 0 { xml.push_str("<chord/>"); }
                            xml.push_str(&format!("<pitch><step>{}</step>", step));
                            if alter != 0 { xml.push_str(&format!("<alter>{}</alter>", alter)); }
                            xml.push_str(&format!("<octave>{}</octave></pitch>", octave));
                            xml.push_str(&format!("<duration>{}</duration>", len));
                            if tie_stop { xml.push_str("<tie type=\"stop\"/>"); }
                            if tie_start { xml.push_str("<tie type=\"start\"/>"); }
                            xml.push_str("<voice>1</voice>");
                            xml.push_str(&format!("<type>{}</type>", type_name));
                            if *dotted { xml.push_str("<dot/>"); }
                            if tie_stop || tie_start {
                                xml.push_str("<notations>");
                                if tie_stop { xml.push_str("<tied type=\"stop\"/>"); }
                                if tie_start { xml.push_str("<tied type=\"start\"/>"); }
                                xml.push_str("</notations>");
                            }
                            xml.push_str("</note>\n");
                        }
                    }
                    part_start += len;
                    let _ = part_start;
                }
            }
            xml.push_str("    </measure>\n");
        }
        xml.push_str("  </part>\n");
    }
    xml.push_str("</score-partwise>\n");
    Ok(xml)
}

// =============================================================================
// Live-notatie: Score/Layer/Take-model voor het notatievenster
// =============================================================================
//
// Datamodel voor de "live noteren"-workflow. Één laag per notenbalk (multi-
// voice-per-balk is bewust NIET in scope — te groot voor deze sessie); elke
// laag mag meerdere takes bevatten (overdub: neem opnieuw op, oude takes
// blijven staan en zijn per stuk aan/uit te vinken).
//
// De pairing van MIDI-NoteOn/NoteOff naar afgesloten `LayerEv`'s gebeurt in
// state.rs::MidiRecording::capture_midi_event (die schrijft ook de events
// die het notatievenster tijdens de opname live rendert).

use serde::Serialize;
use std::collections::HashMap;

/// Eén afgeronde noot in een take (nieuwe naam voor NoteEv-instantie in de
/// live-flow; NoteEv zelf blijft de "flat" struct voor de kwantiseerder).
#[derive(Debug, Clone, Serialize)]
pub struct LayerEv {
    /// Stabiel ID voor selectie/edit (monotoon per Score).
    pub id: u64,
    pub midi: u8,
    /// Starttijd t.o.v. het startpunt van de opname (microseconden).
    pub start_us: u64,
    pub end_us: u64,
    /// Origineel MIDI-kanaal (voor debug + latere balk-hertoewijzing).
    pub channel: u8,
    /// Handmatig verplaatst/gecorrigeerd → niet opnieuw kwantiseren.
    pub locked: bool,
    /// Hand in klavar (0.7.70); None = volgens de laag en het splitspunt.
    #[serde(default)]
    pub hand: Option<KlavarHand>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Take {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    pub events: Vec<LayerEv>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Layer {
    pub id: u32,
    pub name: String,
    /// None = automatisch bepalen op basis van de laagnaam.
    pub bass_clef: Option<bool>,
    /// Volgorde: eerste take = eerste opname; nieuwe take = laatste.
    pub takes: Vec<Take>,
    /// Take waar nieuwe MIDI-events in geschreven worden (armed).
    pub armed_take: Option<u32>,
    /// Divisienamen die tijdens het inspelen naar déze balk routeren (via de
    /// kanaal-inleer van die divisie). Leeg = geen routering; events zonder
    /// passende divisie/balk vallen terug op de armed laag.
    #[serde(default)]
    pub divisions: Vec<String>,
    /// Hand van deze balk in klavar (0.7.70); None = standaardregel
    /// (pedaal → pedaalbalk, één manuaallaag → R+L met splitspunt c',
    /// anders de eerste manuaallaag rechts en de rest links).
    #[serde(default)]
    pub klavar_hand: Option<KlavarHand>,
    /// Splitspunt bij R+L (MIDI-nummer); None = c' (60).
    #[serde(default)]
    pub klavar_split: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetronomeCfg {
    pub click_on: bool,
    pub count_in_beats: u8,
}
impl Default for MetronomeCfg {
    fn default() -> Self { Self { click_on: false, count_in_beats: 0 } }
}

#[derive(Debug, Clone, Serialize)]
pub struct Score {
    pub id: u32,
    pub layers: Vec<Layer>,
    pub bpm: f64,
    pub beats_per_bar: u8,
    pub quantize: u8,
    pub key_fifths: i8,
    pub title: String,
    /// Kwantisatie-tolerantie (0..=100) — schuif "los ↔ strak".
    pub tolerance_pct: u8,
    /// Mineur (0.7.72): toonsoortteken in klavar en `<mode>` in de MusicXML.
    #[serde(default)]
    pub minor: bool,
    pub metronome: MetronomeCfg,
    /// Alleen armed = actief onder één laag tegelijk; None = geen opname.
    pub armed_layer: Option<u32>,
    /// Monotoon oplopende counter — de UI regenereert MusicXML zodra deze wijzigt.
    pub generation: u64,
    /// Interne teller voor stabiele event-ID's; niet naar de UI.
    #[serde(skip)]
    next_event_id: u64,
    #[serde(skip)]
    next_layer_id: u32,
    #[serde(skip)]
    next_take_id: u32,
    /// Undo/redo-stapels (niet naar de UI; alleen counts worden geëxposeerd).
    #[serde(skip)]
    undo: Vec<EditCommand>,
    #[serde(skip)]
    redo: Vec<EditCommand>,
}

impl Score {
    pub fn new(id: u32) -> Self {
        Self {
            id, layers: Vec::new(),
            bpm: 90.0, beats_per_bar: 4, quantize: 4, key_fifths: 0,
            title: String::from("Live opname"),
            tolerance_pct: 80,
            minor: false,
            metronome: MetronomeCfg::default(),
            armed_layer: None,
            generation: 1,
            next_event_id: 1, next_layer_id: 1, next_take_id: 1,
            undo: Vec::new(), redo: Vec::new(),
        }
    }
    pub fn undo_len(&self) -> usize { self.undo.len() }
    pub fn redo_len(&self) -> usize { self.redo.len() }
    pub fn new_event_id(&mut self) -> u64 { let id = self.next_event_id; self.next_event_id += 1; id }
    pub fn bump_gen(&mut self) { self.generation = self.generation.saturating_add(1); }

    /// Push undo-inverse; wist de redo-stack (nieuwe bewerking = redo-tak dood).
    pub fn push_undo(&mut self, cmd: EditCommand) {
        self.undo.push(cmd);
        self.redo.clear();
    }
    /// Push undo-inverse zonder de redo-stack te wissen (voor de redo-flow zelf).
    pub fn push_undo_no_clear_redo(&mut self, cmd: EditCommand) {
        self.undo.push(cmd);
    }
    pub fn push_redo(&mut self, cmd: EditCommand) { self.redo.push(cmd); }
    pub fn pop_undo(&mut self) -> Option<EditCommand> { self.undo.pop() }
    pub fn pop_redo(&mut self) -> Option<EditCommand> { self.redo.pop() }

    pub fn add_layer(&mut self, name: String, bass_clef: Option<bool>) -> u32 {
        let id = self.next_layer_id; self.next_layer_id += 1;
        // Nieuwe laag krijgt automatisch een eerste (lege) take.
        let take_id = self.next_take_id; self.next_take_id += 1;
        self.layers.push(Layer {
            id, name, bass_clef,
            takes: vec![Take { id: take_id, name: "Take 1".into(), visible: true, events: Vec::new() }],
            armed_take: Some(take_id),
            divisions: Vec::new(),
            klavar_hand: None,
            klavar_split: None,
        });
        self.bump_gen();
        id
    }

    pub fn add_take(&mut self, layer_id: u32) -> Option<u32> {
        let take_id = self.next_take_id; self.next_take_id += 1;
        let layer = self.layers.iter_mut().find(|l| l.id == layer_id)?;
        let name = format!("Take {}", layer.takes.len() + 1);
        layer.takes.push(Take { id: take_id, name, visible: true, events: Vec::new() });
        layer.armed_take = Some(take_id);
        self.bump_gen();
        Some(take_id)
    }

    /// Zoek een event op ID in de score; geeft (laag-index, take-index, event-index).
    pub fn locate(&self, ev_id: u64) -> Option<(usize, usize, usize)> {
        for (li, l) in self.layers.iter().enumerate() {
            for (ti, t) in l.takes.iter().enumerate() {
                if let Some(ei) = t.events.iter().position(|e| e.id == ev_id) {
                    return Some((li, ti, ei));
                }
            }
        }
        None
    }
}

/// Snapshot van een Score → notenbalken die de bestaande `build_musicxml`
/// direct kan renderen. Alleen zichtbare takes tellen mee; verwijderde noten
/// (edit) zijn al uit `Take.events` gehaald. Take-events op dezelfde balk
/// worden samengevoegd op tijd (één stem per balk in het notenschrift; overlap
/// wordt daar door `group_chords` ingekort tot de volgende inzet).
pub fn score_to_staves(score: &Score) -> Vec<Staff> {
    score.layers.iter().map(|layer| {
        // Sleutel op naam (zoals altijd); de pedaalbalk van klavar ook op de
        // divisie-routering, want een laag mag anders heten dan "Pedaal".
        let bass = layer.bass_clef.unwrap_or_else(|| is_pedal_name(&layer.name));
        let pedal = is_pedal_name(&layer.name) || layer.divisions.iter().any(|d| is_pedal_name(d));
        let mut notes: Vec<NoteEv> = Vec::new();
        for take in layer.takes.iter().filter(|t| t.visible) {
            for ev in &take.events {
                notes.push(NoteEv {
                    midi: ev.midi,
                    start_sec: ev.start_us as f64 / 1_000_000.0,
                    end_sec: ev.end_us as f64 / 1_000_000.0,
                    id: Some(ev.id),
                    hand: ev.hand,
                });
            }
        }
        // Sorteer op starttijd zodat de kwantiseerder de akkoord-groepering doet.
        notes.sort_by(|a, b| a.start_sec.partial_cmp(&b.start_sec).unwrap_or(std::cmp::Ordering::Equal));
        Staff {
            name: layer.name.clone(), bass_clef: bass, notes,
            layer_id: Some(layer.id), pedal, hand: layer.klavar_hand, split_midi: layer.klavar_split,
        }
    }).collect()
}

/// Divisie → laag voor een import (0.7.70): eerst de divisie-routering van de
/// lagen (dezelfde regel als het inspelen: de eerste laag die de divisie
/// routeert wint), dan de laagnaam als terugval.
pub fn division_layer_map(score: &Score) -> HashMap<String, u32> {
    let mut kaart: HashMap<String, u32> = HashMap::new();
    for layer in &score.layers {
        for d in &layer.divisions {
            kaart.entry(d.clone()).or_insert(layer.id);
        }
    }
    for layer in &score.layers {
        kaart.entry(layer.name.clone()).or_insert(layer.id);
    }
    kaart
}

/// De notatie-opties van een Score (tempo, maatsoort, raster, toonsoort,
/// titel, tolerantie), voor notenschrift én klavar.
pub fn options_from_score(score: &Score) -> NotationOptions {
    NotationOptions {
        bpm: score.bpm,
        beats_per_bar: score.beats_per_bar,
        quantize: score.quantize,
        key_fifths: score.key_fifths,
        title: Some(score.title.clone()),
        staves: None,
        tolerance_pct: Some(score.tolerance_pct),
        minor: Some(score.minor),
    }
}

/// Bouw MusicXML uit een Score met de Score-eigen opties + tolerantie.
pub fn build_musicxml_from_score(score: &Score) -> Result<String, String> {
    build_musicxml(&score_to_staves(score), &options_from_score(score))
}

/// Exporteer de zichtbare takes van een Score als Standard MIDI File (format 1,
/// 480 ppq): track 0 = tempo + maatsoort, daarna één track per balk met noten
/// (originele MIDI-kanalen behouden; note-off vóór note-on bij gelijke tick).
pub fn score_to_smf_bytes(score: &Score) -> Result<Vec<u8>, String> {
    use midly::{Smf, Header, Format, Timing, TrackEvent, TrackEventKind, MetaMessage, MidiMessage};
    use midly::num::{u4, u7, u15, u24, u28};
    const PPQ: u64 = 480;
    let bpm = if score.bpm.is_finite() && score.bpm >= 20.0 && score.bpm <= 300.0 { score.bpm } else { 90.0 };
    let us_per_qn: u32 = (60_000_000.0 / bpm).round() as u32;
    let to_ticks = |us: u64| -> u64 { (us as u128 * PPQ as u128 / us_per_qn as u128) as u64 };

    let mut smf = Smf::new(Header::new(Format::Parallel, Timing::Metrical(u15::new(PPQ as u16))));
    let mut t0: Vec<TrackEvent> = Vec::new();
    t0.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(us_per_qn))) });
    // Maatsoort x/4 (noemer als macht van 2: 2 → kwartnoot).
    t0.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::TimeSignature(score.beats_per_bar.clamp(1, 12), 2, 24, 8)) });
    t0.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) });
    smf.tracks.push(t0);

    for layer in &score.layers {
        // (tick, soort 0=off/1=on, midi, kanaal) — off vóór on bij gelijke tick.
        let mut evs: Vec<(u64, u8, u8, u8)> = Vec::new();
        for take in layer.takes.iter().filter(|t| t.visible) {
            for e in &take.events {
                evs.push((to_ticks(e.start_us), 1, e.midi & 0x7F, e.channel & 0x0F));
                evs.push((to_ticks(e.end_us.max(e.start_us + 1000)), 0, e.midi & 0x7F, e.channel & 0x0F));
            }
        }
        if evs.is_empty() { continue; }
        evs.sort_by_key(|&(t, kind, m, _)| (t, kind, m));
        let mut track: Vec<TrackEvent> = Vec::new();
        track.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::TrackName(layer.name.as_bytes())) });
        let mut last: u64 = 0;
        for (t, kind, midi, ch) in evs {
            let delta = t.saturating_sub(last).min(u32::from(u28::max_value()) as u64) as u32;
            last = t;
            let message = if kind == 1 {
                MidiMessage::NoteOn { key: u7::new(midi), vel: u7::new(100) }
            } else {
                MidiMessage::NoteOff { key: u7::new(midi), vel: u7::new(0) }
            };
            track.push(TrackEvent { delta: u28::new(delta), kind: TrackEventKind::Midi { channel: u4::new(ch), message } });
        }
        track.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) });
        smf.tracks.push(track);
    }
    if smf.tracks.len() <= 1 {
        return Err("Geen noten om te exporteren".into());
    }
    let mut out: Vec<u8> = Vec::new();
    smf.write_std(&mut out).map_err(|e| format!("MIDI schrijven mislukt: {}", e))?;
    Ok(out)
}

/// Undo-bare bewerkingen. Elk commando kent zijn eigen inverse; `apply`
/// muteert de Score én push't de inverse op de undo-stack.
#[derive(Debug, Clone)]
pub enum EditCommand {
    /// Noten verwijderen; bewaar voor undo de originelen + hun locatie.
    DeleteEvents { events: Vec<(u32 /*layer*/, u32 /*take*/, LayerEv)> },
    /// Noten (opnieuw) invoegen — inverse van DeleteEvents en de motor achter
    /// plakken. Elke tuple = (laag, take, event); apply voegt toe en geeft een
    /// DeleteEvents-inverse terug.
    InsertEvents { events: Vec<(u32 /*layer*/, u32 /*take*/, LayerEv)> },
    /// Duur wijzigen: zet per event een nieuw absoluut eind (µs). Bewaart voor
    /// undo de oude einden zodat de round-trip exact is (geen kwantisatie-drift).
    SetEnds { items: Vec<(u64 /*event id*/, u64 /*end_us*/)> },
    /// Verschuiven in de tijd: zet per event nieuwe absolute (start, eind)-tijden
    /// (µs). Undo bewaart de oude tijden exact — de motor achter Shift+←/→.
    SetTimes { items: Vec<(u64 /*event id*/, u64 /*start_us*/, u64 /*end_us*/)> },
    /// Transponeer selectie met N halve tonen (mag negatief).
    Transpose { ids: Vec<u64>, semitones: i8 },
    /// Wijzig tolerantie (met inverse-waarde).
    SetTolerance { old: u8, new: u8 },
    /// Wijzig BPM.
    SetBpm { old: f64, new: f64 },
    /// Wijzig toonsoort.
    SetKey { old: i8, new: i8 },
    /// Wijzig maatsoort (tellen per maat, x/4).
    SetMeter { old: u8, new: u8 },
    /// Wijzig het toongeslacht (majeur/mineur, 0.7.72).
    SetMode { old: bool, new: bool },
    /// Hand en splitspunt van een balk in klavar (0.7.70).
    SetLayerHand { layer: u32, old: (Option<KlavarHand>, Option<u8>), new: (Option<KlavarHand>, Option<u8>) },
    /// Hand per noot in klavar (0.7.70): (event-id, nieuwe hand); undo bewaart de oude.
    SetHands { items: Vec<(u64, Option<KlavarHand>)> },
    /// Voeg een take toe (met inverse: verwijder die take).
    /// Voor de undo bewaren we ID + laag; content is leeg bij add.
    AddTake { layer: u32, take_id: u32 },
    /// Verwijder een laag met alle takes (undo herstelt de complete laag).
    RemoveLayer { snapshot: Layer, position: usize },
    /// Undo-inverse van AddTake: verwijderde take terugzetten op dezelfde plek.
    RestoreTake { layer: u32, index: usize, take: Take },
    /// Undo-inverse van RemoveLayer: laag terugzetten op dezelfde plek.
    RestoreLayer { snapshot: Layer, position: usize },
}

impl EditCommand {
    /// Voer het commando uit op de score en geef het INVERSE terug voor de undo-stack.
    pub fn apply(self, score: &mut Score) -> Option<EditCommand> {
        match self {
            EditCommand::DeleteEvents { events } => {
                let mut removed: Vec<(u32, u32, LayerEv)> = Vec::new();
                for (layer_id, take_id, ev) in events.iter() {
                    if let Some(l) = score.layers.iter_mut().find(|l| l.id == *layer_id) {
                        if let Some(t) = l.takes.iter_mut().find(|t| t.id == *take_id) {
                            if let Some(pos) = t.events.iter().position(|e| e.id == ev.id) {
                                removed.push((*layer_id, *take_id, t.events.remove(pos)));
                            }
                        }
                    }
                }
                if removed.is_empty() { return None; }
                score.bump_gen();
                // Inverse van verwijderen = opnieuw invoegen (herstelt de noten).
                Some(EditCommand::InsertEvents { events: removed })
            }
            EditCommand::InsertEvents { events } => {
                let mut inserted: Vec<(u32, u32, LayerEv)> = Vec::new();
                for (layer_id, take_id, ev) in events.into_iter() {
                    if let Some(l) = score.layers.iter_mut().find(|l| l.id == layer_id) {
                        if let Some(t) = l.takes.iter_mut().find(|t| t.id == take_id) {
                            // Dubbele id's weren (idempotent bij herhaalde redo).
                            if !t.events.iter().any(|e| e.id == ev.id) {
                                t.events.push(ev.clone());
                                inserted.push((layer_id, take_id, ev));
                            }
                        }
                    }
                }
                if inserted.is_empty() { return None; }
                score.bump_gen();
                Some(EditCommand::DeleteEvents { events: inserted })
            }
            EditCommand::SetEnds { items } => {
                let mut old: Vec<(u64, u64)> = Vec::new();
                for (id, new_end) in items.into_iter() {
                    if let Some((li, ti, ei)) = score.locate(id) {
                        let ev = &mut score.layers[li].takes[ti].events[ei];
                        let clamped = new_end.max(ev.start_us + 1000); // ≥1 ms
                        if clamped != ev.end_us {
                            old.push((id, ev.end_us));
                            ev.end_us = clamped;
                            ev.locked = true;
                        }
                    }
                }
                if old.is_empty() { return None; }
                score.bump_gen();
                Some(EditCommand::SetEnds { items: old })
            }
            EditCommand::SetTimes { items } => {
                let mut old: Vec<(u64, u64, u64)> = Vec::new();
                for (id, new_start, new_end) in items.into_iter() {
                    if let Some((li, ti, ei)) = score.locate(id) {
                        let ev = &mut score.layers[li].takes[ti].events[ei];
                        let start = new_start;
                        let end = new_end.max(start + 1000); // ≥1 ms
                        if start != ev.start_us || end != ev.end_us {
                            old.push((id, ev.start_us, ev.end_us));
                            ev.start_us = start;
                            ev.end_us = end;
                            ev.locked = true;
                        }
                    }
                }
                if old.is_empty() { return None; }
                score.bump_gen();
                Some(EditCommand::SetTimes { items: old })
            }
            EditCommand::Transpose { ids, semitones } => {
                let mut changed = 0usize;
                for id in &ids {
                    if let Some((li, ti, ei)) = score.locate(*id) {
                        let ev = &mut score.layers[li].takes[ti].events[ei];
                        let new_midi = (ev.midi as i16 + semitones as i16).clamp(0, 127) as u8;
                        if new_midi != ev.midi {
                            ev.midi = new_midi;
                            ev.locked = true;
                            changed += 1;
                        }
                    }
                }
                if changed == 0 { return None; }
                score.bump_gen();
                Some(EditCommand::Transpose { ids, semitones: -semitones })
            }
            EditCommand::SetTolerance { old, new } => {
                score.tolerance_pct = new.min(100);
                score.bump_gen();
                Some(EditCommand::SetTolerance { old: new, new: old })
            }
            EditCommand::SetBpm { old, new } => {
                score.bpm = new;
                score.bump_gen();
                Some(EditCommand::SetBpm { old: new, new: old })
            }
            EditCommand::SetKey { old, new } => {
                score.key_fifths = new.clamp(-7, 7);
                score.bump_gen();
                Some(EditCommand::SetKey { old: new, new: old })
            }
            EditCommand::SetMeter { old, new } => {
                score.beats_per_bar = new.clamp(1, 12);
                score.bump_gen();
                Some(EditCommand::SetMeter { old: new, new: old })
            }
            EditCommand::SetMode { old, new } => {
                if score.minor == new { return None; }
                score.minor = new;
                score.bump_gen();
                Some(EditCommand::SetMode { old: new, new: old })
            }
            EditCommand::SetLayerHand { layer, old: _, new } => {
                let Some(l) = score.layers.iter_mut().find(|l| l.id == layer) else { return None };
                let oud = (l.klavar_hand, l.klavar_split);
                if oud == new { return None; }
                l.klavar_hand = new.0;
                l.klavar_split = new.1;
                score.bump_gen();
                Some(EditCommand::SetLayerHand { layer, old: new, new: oud })
            }
            EditCommand::SetHands { items } => {
                let mut old: Vec<(u64, Option<KlavarHand>)> = Vec::new();
                for (id, hand) in items.into_iter() {
                    if let Some((li, ti, ei)) = score.locate(id) {
                        let ev = &mut score.layers[li].takes[ti].events[ei];
                        if ev.hand != hand {
                            old.push((id, ev.hand));
                            ev.hand = hand;
                        }
                    }
                }
                if old.is_empty() { return None; }
                score.bump_gen();
                Some(EditCommand::SetHands { items: old })
            }
            EditCommand::AddTake { layer, take_id } => {
                // "Inverse" van add = de zojuist toegevoegde take verwijderen.
                if let Some(l) = score.layers.iter_mut().find(|l| l.id == layer) {
                    if let Some(idx) = l.takes.iter().position(|t| t.id == take_id) {
                        let snapshot = l.takes.remove(idx);
                        if l.armed_take == Some(take_id) {
                            l.armed_take = l.takes.last().map(|t| t.id);
                        }
                        score.bump_gen();
                        return Some(EditCommand::RestoreTake { layer, index: idx, take: snapshot });
                    }
                }
                None
            }
            EditCommand::RestoreTake { layer, index, take } => {
                if let Some(l) = score.layers.iter_mut().find(|l| l.id == layer) {
                    let take_id = take.id;
                    let idx = index.min(l.takes.len());
                    l.takes.insert(idx, take);
                    score.bump_gen();
                    return Some(EditCommand::AddTake { layer, take_id });
                }
                None
            }
            EditCommand::RemoveLayer { snapshot, position } => {
                let idx = score.layers.iter().position(|l| l.id == snapshot.id);
                if let Some(i) = idx {
                    let removed = score.layers.remove(i);
                    if score.armed_layer == Some(removed.id) {
                        score.armed_layer = score.layers.first().map(|l| l.id);
                    }
                    score.bump_gen();
                    Some(EditCommand::RestoreLayer { snapshot: removed, position: position.min(score.layers.len()) })
                } else {
                    None
                }
            }
            EditCommand::RestoreLayer { snapshot, position } => {
                let snapshot_id = snapshot.id;
                let idx = position.min(score.layers.len());
                score.layers.insert(idx, snapshot);
                score.bump_gen();
                Some(EditCommand::RemoveLayer {
                    snapshot: score.layers[idx].clone(),
                    position: idx,
                })
                    .map(|inv| {
                        // De inverse "RemoveLayer" die we teruggeven bevat een verse snapshot;
                        // dat is redundant maar corrrect: bij redo verwijdert hij dezelfde laag opnieuw.
                        let _ = snapshot_id; // silence unused warning in de release-build
                        inv
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> NotationOptions {
        NotationOptions { bpm: 60.0, beats_per_bar: 4, quantize: 4, key_fifths: 0, title: Some("Test".into()), staves: None, tolerance_pct: None, minor: None }
    }

    #[test]
    fn decompose_vijf_zestienden() {
        // 5 rastereenheden bij q=4: kwart (4) + zestiende (1)
        let parts = decompose(5, 4);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], (4, "quarter", false));
        assert_eq!(parts[1], (1, "16th", false));
    }

    #[test]
    fn decompose_gepunteerde_halve() {
        // 12 eenheden bij q=4 = gepunteerde halve
        let parts = decompose(12, 4);
        assert_eq!(parts, vec![(12, "half", true)]);
    }

    #[test]
    fn quantize_akkoord_en_overlap() {
        // Bij 60 BPM en q=4 is één rastereenheid 0,25 s.
        let notes = vec![
            NoteEv::anoniem(60, 0.0, 1.0),   // C4, 4 eenheden
            NoteEv::anoniem(64, 0.02, 0.98), // E4 → zelfde inzet (akkoord)
            NoteEv::anoniem(67, 0.5, 1.5),   // G4 → nieuwe inzet, kort C/E in
        ];
        let chords = group_chords(&quantize_notes(&notes, 60.0, 4, 100));
        assert_eq!(chords.len(), 2);
        assert_eq!(chords[0].notes, vec![60, 64]);
        assert_eq!(chords[0].start, 0);
        assert_eq!(chords[0].end, 2); // ingekort tot de inzet van G4 (0,5 s = 2 eenheden)
        assert_eq!(chords[1].notes, vec![67]);
        assert_eq!(chords[1].end, 6);
    }

    #[test]
    fn musicxml_maten_en_ties() {
        // Noot van 6 eenheden over de maatgrens bij 1 tel per maat (q=4, 1 maat = 4 eenheden):
        // maat 1: kwart (tie start), maat 2: achtste (tie stop) + rust.
        let staves = vec![Staff {
            name: "Test".into(), bass_clef: false,
            notes: vec![NoteEv::anoniem(60, 0.0, 1.5)], ..Default::default() }];
        let mut o = opts();
        o.beats_per_bar = 1;
        let xml = build_musicxml(&staves, &o).expect("xml");
        assert!(xml.contains("<measure number=\"2\">"));
        assert!(xml.contains("<tie type=\"start\"/>"));
        assert!(xml.contains("<tie type=\"stop\"/>"));
        assert!(xml.contains("<rest/>"));
        assert!(xml.contains("<part-name>Test</part-name>"));
    }

    #[test]
    fn musicxml_bassleutel_en_toonsoort() {
        let staves = vec![Staff {
            name: "Pedaal".into(), bass_clef: true,
            notes: vec![NoteEv::anoniem(43, 0.0, 1.0)], ..Default::default() }];
        let mut o = opts();
        o.key_fifths = -2; // Bes-groot → mollen
        let xml = build_musicxml(&staves, &o).expect("xml");
        assert!(xml.contains("<clef><sign>F</sign><line>4</line></clef>"));
        assert!(xml.contains("<fifths>-2</fifths>"));
    }

    #[test]
    fn musicxml_mollen_spelling() {
        // MIDI 61 = Cis/Des: bij mollen-toonsoort als D-mol spellen.
        let (step, alter, octave) = spell(61, -1);
        assert_eq!((step, alter, octave), ('D', -1, 4));
        let (step, alter, _) = spell(61, 1);
        assert_eq!((step, alter), ('C', 1));
    }

    #[test]
    fn quantize_notes_behoudt_ids() {
        let notes = vec![
            NoteEv { midi: 60, start_sec: 0.0, end_sec: 1.0, id: Some(7), hand: Some(KlavarHand::Left) },
            NoteEv { midi: 64, start_sec: 0.02, end_sec: 0.98, id: Some(8), hand: None },
        ];
        let q = quantize_notes(&notes, 60.0, 4, 100);
        assert_eq!(q.len(), 2);
        assert_eq!((q[0].id, q[0].midi, q[0].start, q[0].end, q[0].hand), (Some(7), 60, 0, 4, Some(KlavarHand::Left)));
        assert_eq!((q[1].id, q[1].midi, q[1].start, q[1].end), (Some(8), 64, 0, 4));
    }

    #[test]
    fn anonieme_noten_hebben_geen_id() {
        let q = quantize_notes(&[NoteEv::anoniem(60, 0.0, 0.5)], 60.0, 4, 100);
        assert_eq!(q[0].id, None);
        assert_eq!(q[0].end, 2);
    }

    #[test]
    fn dubbele_noot_zelfde_inzet_houdt_eerste_id() {
        // Twee takes met dezelfde toon op dezelfde inzet: het notenschrift
        // kent één notenkop (de tweede valt weg in de dedup), klavar houdt
        // beide noten, met hun eigen ID.
        let notes = vec![
            NoteEv { midi: 60, start_sec: 0.0, end_sec: 1.0, id: Some(1), hand: None },
            NoteEv { midi: 60, start_sec: 0.0, end_sec: 2.0, id: Some(2), hand: None },
        ];
        let q = quantize_notes(&notes, 60.0, 4, 100);
        assert_eq!(q.len(), 2);
        let chords = group_chords(&q);
        assert_eq!(chords.len(), 1);
        assert_eq!(chords[0].notes, vec![60]);
        assert_eq!(chords[0].end, 8);
    }

    #[test]
    fn is_pedal_name_varianten() {
        for n in ["Pedaal", "PEDAL", "Pédale", "Pedał", "PED", "Pedaalkoppel", "Hoofdwerk + Pedaal", "Pe\u{301}dale"] {
            assert!(is_pedal_name(n), "{n}");
        }
        for n in ["Hoofdwerk", "Zwelwerk", "Great", "Expedition"] {
            assert!(!is_pedal_name(n), "{n}");
        }
    }

    #[test]
    fn score_to_staves_vult_ids_en_pedaal() {
        let mut sc = Score::new(1);
        let hw = sc.add_layer("Hoofdwerk".into(), None);
        let p = sc.add_layer("Balk 2".into(), None);
        sc.layers[1].divisions = vec!["Pedaal".into()];
        sc.layers[0].klavar_hand = Some(KlavarHand::Right);
        let id = sc.new_event_id();
        sc.layers[0].takes[0].events.push(LayerEv { id, midi: 60, start_us: 0, end_us: 500_000, channel: 0, locked: false, hand: Some(KlavarHand::Left) });
        let staves = score_to_staves(&sc);
        assert_eq!(staves[0].layer_id, Some(hw));
        assert_eq!(staves[0].notes[0].id, Some(id));
        assert_eq!(staves[0].notes[0].hand, Some(KlavarHand::Left));
        assert_eq!(staves[0].hand, Some(KlavarHand::Right));
        assert!(!staves[0].pedal);
        // Pedaal via de divisie-routering, maar de sleutel blijft op naam.
        assert_eq!(staves[1].layer_id, Some(p));
        assert!(staves[1].pedal);
        assert!(!staves[1].bass_clef);
    }

    #[test]
    fn import_routeert_via_divisies() {
        let mut sc = Score::new(1);
        let a = sc.add_layer("Balk A".into(), None);
        let b = sc.add_layer("Hoofdwerk".into(), None);
        let c = sc.add_layer("Balk C".into(), None);
        sc.layers[0].divisions = vec!["Hoofdwerk".into(), "Pedaal".into()];
        sc.layers[2].divisions = vec!["Pedaal".into()];
        let kaart = division_layer_map(&sc);
        // Routering wint van de laagnaam, en de eerste laag wint bij dubbele routering.
        assert_eq!(kaart.get("Hoofdwerk"), Some(&a));
        assert_eq!(kaart.get("Pedaal"), Some(&a));
        // Laagnaam als terugval.
        assert_eq!(kaart.get("Balk C"), Some(&c));
        assert_eq!(kaart.get("Balk A"), Some(&a));
        let _ = b;
        assert_eq!(kaart.get("Zwelwerk"), None);
    }

    #[test]
    fn set_mode_undo_en_musicxml_mode() {
        let mut sc = Score::new(1);
        let inv = EditCommand::SetMode { old: false, new: true }.apply(&mut sc).expect("inverse");
        assert!(sc.minor);
        inv.apply(&mut sc);
        assert!(!sc.minor);
        assert!(EditCommand::SetMode { old: false, new: false }.apply(&mut sc).is_none());
        // Mineur zet <mode>minor</mode>; majeur laat de XML zoals hij was.
        let staves = vec![Staff { name: "T".into(), bass_clef: false, notes: vec![NoteEv::anoniem(60, 0.0, 1.0)], ..Default::default() }];
        let mut o = opts();
        o.minor = Some(true);
        assert!(build_musicxml(&staves, &o).expect("xml").contains("<fifths>0</fifths><mode>minor</mode>"));
        o.minor = None;
        assert!(!build_musicxml(&staves, &o).expect("xml").contains("<mode>"));
    }

    #[test]
    fn set_layer_hand_undo() {
        let mut sc = Score::new(1);
        let l = sc.add_layer("Hoofdwerk".into(), None);
        let cmd = EditCommand::SetLayerHand { layer: l, old: (None, None), new: (Some(KlavarHand::Left), Some(60)) };
        let inv = cmd.apply(&mut sc).expect("inverse");
        assert_eq!((sc.layers[0].klavar_hand, sc.layers[0].klavar_split), (Some(KlavarHand::Left), Some(60)));
        inv.apply(&mut sc);
        assert_eq!((sc.layers[0].klavar_hand, sc.layers[0].klavar_split), (None, None));
        // Ongewijzigd = geen undo-stap.
        let niets = EditCommand::SetLayerHand { layer: l, old: (None, None), new: (None, None) };
        assert!(niets.apply(&mut sc).is_none());
    }

    #[test]
    fn set_hands_undo() {
        let mut sc = Score::new(1);
        sc.add_layer("Hoofdwerk".into(), None);
        let id = sc.new_event_id();
        sc.layers[0].takes[0].events.push(LayerEv { id, midi: 60, start_us: 0, end_us: 1000, channel: 0, locked: false, hand: None });
        let cmd = EditCommand::SetHands { items: vec![(id, Some(KlavarHand::Right)), (999, Some(KlavarHand::Left))] };
        let inv = cmd.apply(&mut sc).expect("inverse");
        assert_eq!(sc.layers[0].takes[0].events[0].hand, Some(KlavarHand::Right));
        inv.apply(&mut sc);
        assert_eq!(sc.layers[0].takes[0].events[0].hand, None);
    }

    #[test]
    fn lege_opname_geeft_fout() {
        let staves = vec![Staff { name: "X".into(), bass_clef: false, notes: vec![], ..Default::default() }];
        assert!(build_musicxml(&staves, &opts()).is_err());
    }

    // ---- Regressie-fixture (0.7.70): de MusicXML-uitvoer van vóór het
    // uitlichten van de kwantisatie, byte voor byte. De fixture is één keer
    // gegenereerd met `cargo test -p vpo-app -- --ignored schrijf_fixture_musicxml`
    // op de code van 0.7.69 en staat in src/testdata/notatie_0769.musicxml
    // (LF, zie .gitattributes).

    /// Drie balken: akkoord, overlap (inkorting), noot over de maatstreep,
    /// rust, een pedaalbalk in de bassleutel, en mollen-spelling.
    fn fixture_staves() -> Vec<Staff> {
        vec![
            Staff {
                name: "Hoofdwerk".into(), bass_clef: false,
                notes: vec![
                    NoteEv::anoniem(60, 0.0, 1.0),
                    NoteEv::anoniem(64, 0.02, 0.98),
                    NoteEv::anoniem(67, 0.5, 1.5),
                    NoteEv::anoniem(61, 2.0, 2.3),
                    NoteEv::anoniem(63, 3.5, 5.5),
                    NoteEv::anoniem(65, 6.0, 6.25),
                ], ..Default::default() },
            Staff {
                name: "Zwelwerk".into(), bass_clef: false,
                notes: vec![
                    NoteEv::anoniem(72, 1.0, 1.75),
                    NoteEv::anoniem(70, 1.75, 2.0),
                    NoteEv::anoniem(68, 2.0, 4.0),
                ], ..Default::default() },
            Staff {
                name: "Pedaal".into(), bass_clef: true,
                notes: vec![
                    NoteEv::anoniem(43, 0.0, 2.0),
                    NoteEv::anoniem(46, 2.0, 3.0),
                    NoteEv::anoniem(36, 4.0, 7.0),
                ], ..Default::default() },
        ]
    }

    fn fixture_opts() -> NotationOptions {
        NotationOptions { bpm: 60.0, beats_per_bar: 3, quantize: 4, key_fifths: -2, title: Some("Fixture 0.7.69".into()), staves: None, tolerance_pct: Some(80), minor: None }
    }

    /// Tweede fixture (0.7.70): de paden die de eerste niet raakt — sub-raster
    /// (0,46 eenheid), dezelfde toon twee keer op één inzet, minimale lengte,
    /// akkoord met ongelijke duren, niet-toegestane duur (5 eenheden) in twee
    /// overgebonden waarden, noot over twee maatstrepen, lege balk tussen
    /// gevulde (part-hernummering), kruizen, XML-escaping, niet-geheel tempo.
    /// Gegenereerd met de code van 0.7.70, waarvan een differentiële test
    /// (3000 willekeurige partituren) de gelijkheid met 0.7.69 had bevestigd.
    fn fixture2_staves() -> Vec<Staff> {
        vec![
            Staff {
                name: "Hoofdwerk & Positief".into(), bass_clef: false,
                notes: vec![
                    NoteEv::anoniem(60, 0.115, 0.5),
                    NoteEv::anoniem(60, 1.0, 1.5),
                    NoteEv::anoniem(60, 1.0, 2.0),
                    NoteEv::anoniem(64, 1.0, 1.005),
                    NoteEv::anoniem(67, 2.5, 3.75),
                    NoteEv::anoniem(72, 4.0, 11.0),
                ],
                ..Default::default()
            },
            Staff { name: "Leeg".into(), bass_clef: false, notes: vec![], ..Default::default() },
            Staff {
                name: "Pedaal".into(), bass_clef: true,
                notes: vec![NoteEv::anoniem(43, 0.0, 1.25), NoteEv::anoniem(38, 1.25, 3.0)],
                ..Default::default()
            },
        ]
    }

    fn fixture2_opts() -> NotationOptions {
        NotationOptions { bpm: 72.5, beats_per_bar: 3, quantize: 4, key_fifths: 3, title: Some("A & B <C>".into()), staves: None, tolerance_pct: Some(60), minor: None }
    }

    const FIXTURE_PAD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/testdata/notatie_0769.musicxml");
    const FIXTURE2_PAD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/testdata/notatie_0770.musicxml");

    /// Schrijft de fixtures opnieuw, maar ALLEEN met JM_SCHRIJF_FIXTURE=1: de
    /// referentie is de uitvoer van een oudere versie en mag nooit stilzwijgend
    /// door de huidige code worden overschreven (`--ignored` zou dat anders doen).
    #[test]
    #[ignore]
    fn schrijf_fixture_musicxml() {
        if std::env::var("JM_SCHRIJF_FIXTURE").as_deref() != Ok("1") {
            eprintln!("JM_SCHRIJF_FIXTURE=1 ontbreekt: fixtures niet herschreven");
            return;
        }
        let xml = build_musicxml(&fixture_staves(), &fixture_opts()).expect("xml");
        std::fs::write(FIXTURE_PAD, xml.as_bytes()).expect("fixture schrijven");
        let xml2 = build_musicxml(&fixture2_staves(), &fixture2_opts()).expect("xml");
        std::fs::write(FIXTURE2_PAD, xml2.as_bytes()).expect("fixture 2 schrijven");
    }

    #[test]
    fn musicxml_gelijk_aan_fixture() {
        let verwacht = include_str!("testdata/notatie_0769.musicxml").replace("\r\n", "\n");
        let xml = build_musicxml(&fixture_staves(), &fixture_opts()).expect("xml").replace("\r\n", "\n");
        assert!(!verwacht.is_empty(), "fixture ontbreekt: eerst schrijf_fixture_musicxml draaien");
        assert_eq!(xml, verwacht);
    }

    #[test]
    fn musicxml_gelijk_aan_fixture2() {
        let verwacht = include_str!("testdata/notatie_0770.musicxml").replace("\r\n", "\n");
        let xml = build_musicxml(&fixture2_staves(), &fixture2_opts()).expect("xml").replace("\r\n", "\n");
        assert!(!verwacht.is_empty(), "fixture 2 ontbreekt: eerst schrijf_fixture_musicxml draaien");
        assert_eq!(xml, verwacht);
        // De lege balk tussen de gevulde telt niet mee als part.
        assert!(xml.contains("<score-part id=\"P2\"><part-name>Pedaal</part-name>"));
        assert!(!xml.contains("Leeg"));
        assert!(xml.contains("A &amp; B &lt;C&gt;"));
    }
}
