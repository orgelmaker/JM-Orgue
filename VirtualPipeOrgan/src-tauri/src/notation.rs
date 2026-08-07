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
//! Bewuste v1-vereenvoudigingen (gedocumenteerd in de CHANGELOG):
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
}

/// Eén (getransponeerde) noot, toegewezen aan een notenbalk.
#[derive(Debug, Clone)]
pub struct NoteEv {
    pub midi: u8,
    pub start_sec: f64,
    pub end_sec: f64,
}

/// Eén notenbalk (divisie) met zijn noten.
#[derive(Debug, Clone)]
pub struct Staff {
    pub name: String,
    pub bass_clef: bool,
    pub notes: Vec<NoteEv>,
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

/// Kwantiseer de noten van één balk naar rastereenheden en groepeer
/// gelijktijdige inzetten tot akkoorden. Overlap wordt ingekort tot de
/// volgende inzet (één stem per balk in v1).
///
/// `tolerance_pct`: 0..=100 — schuif "los ↔ strak":
/// - 100 = hard kwantiseren: elke noot naar het raster (huidig gedrag);
/// - 0   = losser: noten die verder van het raster af zitten dan de tolerantie
///   worden op een fijner sub-raster (halveringen, tot 8× fijner) neergezet.
/// Dit vermijdt onhoorbare drift zonder de partituur onleesbaar te maken.
fn quantize_staff(notes: &[NoteEv], bpm: f64, q: u8, tolerance_pct: u8) -> Vec<Chord> {
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
    let mut quantized: Vec<(u64, u64, u8)> = notes.iter().map(|n| {
        let s = quantize_one(n.start_sec);
        let e = quantize_one(n.end_sec);
        let e = e.max(s + 1);
        (s, e, n.midi)
    }).collect();
    quantized.sort_by_key(|&(s, _, m)| (s, m));

    // Groepeer per gelijke start; akkoordduur = langste noot van de groep.
    let mut chords: Vec<Chord> = Vec::new();
    for (s, e, m) in quantized {
        match chords.last_mut() {
            Some(c) if c.start == s => {
                if !c.notes.contains(&m) { c.notes.push(m); }
                if e > c.end { c.end = e; }
            }
            _ => chords.push(Chord { start: s, end: e, notes: vec![m] }),
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

/// Bouw een partwise MusicXML-document uit de (per balk toegewezen) noten.
pub fn build_musicxml(staves: &[Staff], opts: &NotationOptions) -> Result<String, String> {
    let q = opts.quantize.clamp(1, 8) as u64;
    let beats = opts.beats_per_bar.clamp(1, 12) as u64;
    let bpm = if opts.bpm.is_finite() && opts.bpm >= 20.0 && opts.bpm <= 300.0 { opts.bpm } else { 90.0 };
    let measure_len = beats * q;

    let tolerance = opts.tolerance_pct.unwrap_or(100);
    let quantized: Vec<(usize, Vec<Chord>)> = staves.iter().enumerate()
        .filter(|(_, st)| !st.notes.is_empty())
        .map(|(i, st)| (i, quantize_staff(&st.notes, bpm, opts.quantize.clamp(1, 8), tolerance)))
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
    for (idx, (staff_i, _)) in quantized.iter().enumerate() {
        xml.push_str(&format!(
            "    <score-part id=\"P{}\"><part-name>{}</part-name></score-part>\n",
            idx + 1, xml_escape(&staves[*staff_i].name)));
    }
    xml.push_str("  </part-list>\n");

    for (idx, (staff_i, chords)) in quantized.iter().enumerate() {
        let staff = &staves[*staff_i];
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
                xml.push_str(&format!("        <key><fifths>{}</fifths></key>\n", opts.key_fifths.clamp(-7, 7)));
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
/// worden samengevoegd op tijd (één stem per balk in v1; overlap wordt door
/// `quantize_staff` ingekort tot de volgende inzet).
pub fn score_to_staves(score: &Score) -> Vec<Staff> {
    fn is_pedal_name(s: &str) -> bool {
        let l = s.to_lowercase();
        l.contains("pedaal") || l.contains("pedal")
    }
    score.layers.iter().map(|layer| {
        let bass = layer.bass_clef.unwrap_or_else(|| is_pedal_name(&layer.name));
        let mut notes: Vec<NoteEv> = Vec::new();
        for take in layer.takes.iter().filter(|t| t.visible) {
            for ev in &take.events {
                notes.push(NoteEv {
                    midi: ev.midi,
                    start_sec: ev.start_us as f64 / 1_000_000.0,
                    end_sec: ev.end_us as f64 / 1_000_000.0,
                });
            }
        }
        // Sorteer op starttijd zodat de kwantiseerder de akkoord-groepering doet.
        notes.sort_by(|a, b| a.start_sec.partial_cmp(&b.start_sec).unwrap_or(std::cmp::Ordering::Equal));
        Staff { name: layer.name.clone(), bass_clef: bass, notes }
    }).collect()
}

/// Bouw MusicXML uit een Score met de Score-eigen opties + tolerantie.
pub fn build_musicxml_from_score(score: &Score) -> Result<String, String> {
    let staves = score_to_staves(score);
    let opts = NotationOptions {
        bpm: score.bpm,
        beats_per_bar: score.beats_per_bar,
        quantize: score.quantize,
        key_fifths: score.key_fifths,
        title: Some(score.title.clone()),
        staves: None,
        tolerance_pct: Some(score.tolerance_pct),
    };
    build_musicxml(&staves, &opts)
}

/// Undo-bare bewerkingen. Elk commando kent zijn eigen inverse; `apply`
/// muteert de Score én push't de inverse op de undo-stack.
#[derive(Debug, Clone)]
pub enum EditCommand {
    /// Noten verwijderen; bewaar voor undo de originelen + hun locatie.
    DeleteEvents { events: Vec<(u32 /*layer*/, u32 /*take*/, LayerEv)> },
    /// Transponeer selectie met N halve tonen (mag negatief).
    Transpose { ids: Vec<u64>, semitones: i8 },
    /// Wijzig tolerantie (met inverse-waarde).
    SetTolerance { old: u8, new: u8 },
    /// Wijzig BPM.
    SetBpm { old: f64, new: f64 },
    /// Wijzig toonsoort.
    SetKey { old: i8, new: i8 },
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
                Some(EditCommand::DeleteEvents { events: removed })
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
        NotationOptions { bpm: 60.0, beats_per_bar: 4, quantize: 4, key_fifths: 0, title: Some("Test".into()), staves: None, tolerance_pct: None }
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
            NoteEv { midi: 60, start_sec: 0.0, end_sec: 1.0 },   // C4, 4 eenheden
            NoteEv { midi: 64, start_sec: 0.02, end_sec: 0.98 }, // E4 → zelfde inzet (akkoord)
            NoteEv { midi: 67, start_sec: 0.5, end_sec: 1.5 },   // G4 → nieuwe inzet, kort C/E in
        ];
        let chords = quantize_staff(&notes, 60.0, 4, 100);
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
            notes: vec![NoteEv { midi: 60, start_sec: 0.0, end_sec: 1.5 }],
        }];
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
            notes: vec![NoteEv { midi: 43, start_sec: 0.0, end_sec: 1.0 }],
        }];
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
    fn lege_opname_geeft_fout() {
        let staves = vec![Staff { name: "X".into(), bass_clef: false, notes: vec![] }];
        assert!(build_musicxml(&staves, &opts()).is_err());
    }
}
