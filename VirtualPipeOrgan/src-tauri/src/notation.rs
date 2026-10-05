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
    /// Noemer van de maatsoort (0.7.83): 2, 4 of 8. None = 4.
    #[serde(default)]
    pub beat_unit: Option<u8>,
    /// Minimaal aantal maten op het blad (0.7.83); None/0 = alleen de inhoud.
    #[serde(default)]
    pub min_measures: Option<u32>,
    /// Componist (`<creator type="composer">`) en ondertitel
    /// (`<movement-title>`) op het blad (0.7.83).
    #[serde(default)]
    pub composer: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
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
    /// Stem binnen de balk (0.7.86): 1..=4, zoals de lagen in Finale.
    pub voice: u8,
}

impl NoteEv {
    /// Noot zonder event-ID (bestandsmodus, tests).
    pub fn anoniem(midi: u8, start_sec: f64, end_sec: f64) -> Self {
        NoteEv { midi, start_sec, end_sec, id: None, hand: None, voice: 1 }
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

/// Eén noot in een akkoordsegment (0.7.82): `tie_stop` = vervolg van dezelfde
/// toon uit het vorige segment, `tie_start` = loopt door in het volgende.
#[derive(Debug, Clone, PartialEq)]
struct ChordNote {
    midi: u8,
    tie_start: bool,
    tie_stop: bool,
    /// Event-ID (0.7.86) voor de notemap; None in de bestandsmodus.
    id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
struct Chord {
    start: u64,            // in rastereenheden
    end: u64,              // exclusief
    notes: Vec<ChordNote>, // gesorteerd op toonhoogte
}

#[cfg(test)]
impl Chord {
    fn midis(&self) -> Vec<u8> { self.notes.iter().map(|n| n.midi).collect() }
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
    /// Stem (0.7.86).
    pub voice: u8,
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
pub(crate) fn quantize_notes(notes: &[NoteEv], bpm: f64, q: u8, _tolerance_pct: u8) -> Vec<QNote> {
    let grid_per_sec = (bpm / 60.0) * q as f64;
    // Sinds 0.7.82 gewoon afronden op het raster. Het sub-raster van vroeger
    // (de "tolerantie") maakte van akkoordspreiding losse zestienden; die
    // spreiding wordt nu vóóraf geclusterd (cluster_akkoorden) en de schuif
    // "Akkoord-speling" bepaalt dát venster.
    let quantize_one = |t: f64| -> u64 {
        if t <= 0.0 { return 0; }
        (t * grid_per_sec).round().max(0.0) as u64
    };
    let mut quantized: Vec<QNote> = notes.iter().map(|n| {
        let s = quantize_one(n.start_sec);
        let e = quantize_one(n.end_sec);
        let e = e.max(s + 1);
        QNote { id: n.id, midi: n.midi, start: s, end: e, hand: n.hand, voice: n.voice }
    }).collect();
    quantized.sort_by_key(|n| (n.start, n.voice, n.midi));
    quantized
}

/// Venster (seconden) waarbinnen inzetten en loslaten als één akkoord
/// gelden: minstens 40 ms, hoogstens een halve rastereenheid; de schuif
/// "Akkoord-speling" (0..=100) kiest daartussen.
pub(crate) fn akkoord_venster_sec(bpm: f64, q: u64, tolerance_pct: u8) -> f64 {
    let cel = 60.0 / bpm / q.max(1) as f64;
    (tolerance_pct.min(100) as f64 / 100.0 * 0.5 * cel).max(0.040)
}

/// Akkoordclustering (0.7.82): noten die binnen `venster` ná de inzet van een
/// cluster beginnen, krijgen de inzet van die cluster (de vroegste); binnen
/// een cluster krijgen einden die binnen `venster` van elkaar liggen hetzelfde
/// einde (het laatste). Een orgelakkoord wordt met 20–80 ms spreiding
/// aangeslagen én losgelaten; zonder dit viel het bij een rastergrens uiteen
/// in zestiende-fragmenten. Een bewust vroeger losgelaten noot (verder dan het
/// venster) blijft korter, een gebroken akkoord blijft gebroken.
pub(crate) fn cluster_akkoorden(notes: &mut [NoteEv], venster: f64) {
    if notes.len() < 2 { return; }
    notes.sort_by(|a, b| a.start_sec.partial_cmp(&b.start_sec).unwrap_or(std::cmp::Ordering::Equal));
    let mut i = 0;
    while i < notes.len() {
        let inzet = notes[i].start_sec;
        let mut j = i + 1;
        while j < notes.len() && notes[j].start_sec - inzet <= venster { j += 1; }
        for n in &mut notes[i..j] {
            n.start_sec = inzet;
            if n.end_sec < inzet + 0.001 { n.end_sec = inzet + 0.001; }
        }
        let mut idx: Vec<usize> = (i..j).collect();
        idx.sort_by(|&a, &b| notes[a].end_sec.partial_cmp(&notes[b].end_sec).unwrap_or(std::cmp::Ordering::Equal));
        let mut k = 0;
        while k < idx.len() {
            let eerste = notes[idx[k]].end_sec;
            let mut l = k + 1;
            while l < idx.len() && notes[idx[l]].end_sec - eerste <= venster { l += 1; }
            let laatste = notes[idx[l - 1]].end_sec;
            for &m in &idx[k..l] { notes[m].end_sec = laatste; }
            k = l;
        }
        i = j;
    }
    // Tweede pas (reviewbevinding): gewoon vingerlegato laat een noot 30–100 ms
    // ná de VOLGENDE inzet los. Zonder dit rondde dat einde op de volgende
    // rastereenheid en maakte group_chords er een gebonden 32ste-fragment van.
    // Loslaten binnen het venster ná een latere inzet = op die inzet; een noot
    // die minstens het venster langer doorligt blijft doorgebonden.
    let mut inzetten: Vec<f64> = notes.iter().map(|n| n.start_sec).collect();
    inzetten.dedup();
    for n in notes.iter_mut() {
        for &k in &inzetten {
            if k > n.start_sec && k < n.end_sec && n.end_sec - k <= venster {
                n.end_sec = k;
                break;
            }
        }
    }
}

/// Akkoordgroepering voor het notenschrift (0.7.82). Dezelfde toon twee keer
/// op één inzet: de eerste wint (ID-dedup). Daarna segmenten tussen álle
/// grenzen (inzetten én einden): elk segment is één akkoord met één duur; een
/// noot die het segment overleeft loopt door in het volgende segment en wordt
/// overgebonden (`tie_start`/`tie_stop`) in plaats van afgekapt. Een herinzet
/// van dezelfde toon wint van het doorlopen: de oudere noot eindigt daar
/// zonder boog. Klavar slaat dit over (die tekent de noten zelf).
fn group_chords(qnotes: &[QNote]) -> Vec<Chord> {
    #[derive(Clone)]
    struct Actief { midi: u8, start: u64, end: u64, id: Option<u64> }
    let mut noten: Vec<Actief> = Vec::new();
    for n in qnotes {
        let end = n.end.max(n.start + 1);
        // Dezelfde toon twee keer op één inzet (overdub, twee lagen op één
        // balk): één notenkop, met het langste einde — wat klinkt is de
        // vereniging van beide (reviewbevinding).
        if let Some(a) = noten.iter_mut().find(|a| a.start == n.start && a.midi == n.midi) {
            // De noot die de getekende lengte bepaalt, levert ook het id (klik).
            if end > a.end { a.end = end; a.id = n.id; }
            continue;
        }
        noten.push(Actief { midi: n.midi, start: n.start, end, id: n.id });
    }
    if noten.is_empty() { return Vec::new(); }
    let inzetten: Vec<(u8, u64)> = noten.iter().map(|a| (a.midi, a.start)).collect();
    for a in noten.iter_mut() {
        for &(m, s) in &inzetten {
            if m == a.midi && s > a.start && s < a.end { a.end = s; }
        }
    }
    let mut grenzen: Vec<u64> = noten.iter().flat_map(|a| [a.start, a.end]).collect();
    grenzen.sort_unstable();
    grenzen.dedup();
    let mut chords: Vec<Chord> = Vec::new();
    for w in grenzen.windows(2) {
        let (t0, t1) = (w[0], w[1]);
        let mut notes: Vec<ChordNote> = noten.iter()
            .filter(|a| a.start <= t0 && a.end > t0)
            .map(|a| ChordNote { midi: a.midi, tie_stop: a.start < t0, tie_start: a.end > t1, id: a.id })
            .collect();
        if notes.is_empty() { continue; }
        notes.sort_by_key(|n| n.midi);
        notes.dedup_by_key(|n| n.midi);
        chords.push(Chord { start: t0, end: t1, notes });
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
    /// Noemer van de maatsoort: 2, 4 of 8 (0.7.83).
    pub beat_unit: u64,
    pub measure_len: u64,
    pub bpm: f64,
    /// Alle balken in balkvolgorde, ook lege (sinds 0.7.83: een lege balk
    /// wordt een part met hele-maatrusten; de UI koppelt OSMD-balk n aan laag n).
    pub staves: Vec<QuantizedStaff>,
}

/// Noemer van de maatsoort geklemd op 2, 4 of 8 (0.7.83).
pub fn klem_beat_unit(unit: u8) -> u8 {
    match unit { 2 => 2, 8 => 8, _ => 4 }
}

/// Raster (eenheden per kwart) en maatlengte in rastereenheden voor een
/// maatsoort (0.7.83): `measure_len = beats × (q × 4 / beat_unit)`. Bij /8 is
/// het raster minstens een achtste (q ≥ 2), anders past een tel niet op het
/// raster; de UI biedt bij /8 ook geen kwart aan.
pub fn raster_en_maatlengte(quantize: u8, beats_per_bar: u8, beat_unit: u8) -> (u64, u64) {
    let unit = klem_beat_unit(beat_unit) as u64;
    let mut q = quantize.clamp(1, 8) as u64;
    if unit == 8 && q < 2 { q = 2; }
    let beats = beats_per_bar.clamp(1, 12) as u64;
    (q, beats * (q * 4 / unit))
}

/// Tel voor de waardestrepen in rastereenheden (0.7.83): de maatlengte
/// gedeeld door het aantal tellen — bij /4 een kwart, bij /2 een halve — en
/// bij samengestelde achtstenmaten (6/8, 9/8, 12/8: tellen deelbaar door
/// drie) drie achtsten. Gedeeld door het notenschrift en klavar.
pub fn beam_tel(measure_len: u64, beats: u64, beat_unit: u64) -> u64 {
    let beats = beats.max(1);
    let tel = (measure_len / beats).max(1);
    if beat_unit == 8 && beats % 3 == 0 { tel * 3 } else { tel }
}

/// Kwantiseer alle balken, ook lege (0.7.83: een leeg stuk toont lege
/// notenbalken). Zonder balken geeft het notenschrift een fout
/// (`render_musicxml`), klavar een lege balk.
pub fn quantize_score(staves: &[Staff], opts: &NotationOptions) -> QuantizedScore {
    let beat_unit = klem_beat_unit(opts.beat_unit.unwrap_or(4)) as u64;
    let (q, measure_len) = raster_en_maatlengte(opts.quantize, opts.beats_per_bar, beat_unit as u8);
    let beats = opts.beats_per_bar.clamp(1, 12) as u64;
    let bpm = if opts.bpm.is_finite() && opts.bpm >= 20.0 && opts.bpm <= 300.0 { opts.bpm } else { 90.0 };
    let tolerance = opts.tolerance_pct.unwrap_or(100);
    let venster = akkoord_venster_sec(bpm, q, tolerance);
    let qstaves: Vec<QuantizedStaff> = staves.iter().enumerate()
        .map(|(i, st)| {
            // Akkoordclustering vóór het raster (0.7.82), ook voor klavar.
            let mut noten = st.notes.clone();
            // Akkoorden clusteren per stem (0.7.86): een liggende noot in
            // stem 2 hoort niet bij de inzetten van stem 1.
            noten.sort_by_key(|n| n.voice);
            for groep in noten.chunk_by_mut(|a, b| a.voice == b.voice) {
                cluster_akkoorden(groep, venster);
            }
            QuantizedStaff {
                staff_index: i, name: st.name.clone(), layer_id: st.layer_id, pedal: st.pedal,
                bass_clef: st.bass_clef, hand: st.hand, split_midi: st.split_midi,
                notes: quantize_notes(&noten, bpm, q as u8, tolerance),
            }
        })
        .collect();
    QuantizedScore { q, beats_per_bar: beats, beat_unit, measure_len, bpm, staves: qstaves }
}

/// Bouw een partwise MusicXML-document uit de (per balk toegewezen) noten.
pub fn build_musicxml(staves: &[Staff], opts: &NotationOptions) -> Result<String, String> {
    render_musicxml(&quantize_score(staves, opts), opts)
}

/// MusicXML uit het gedeelde model: akkoorden, één stem per balk, rusten en
/// overbindingen (het notenschrift van 0.7.0, byte voor byte; zie de
/// fixture-test).
/// Kleuren per stem op het scherm (0.7.86): stem 1 zwart, 2 blauw, 3 groen,
/// 4 oranje; de exportvariant blijft kleurloos.
pub const STEM_KLEUREN: [&str; 4] = ["#000000", "#2a6fdb", "#2e8b57", "#e07b00"];
const DIM_KLEUR: &str = "#bbbbbb";

/// Schermopties (0.7.86): per balk (in balkvolgorde) de actieve stem; met
/// `dim_andere` worden de andere stemmen lichtgrijs. `None` = exportvariant.
#[derive(Debug, Clone, Default)]
pub struct SchermOpties {
    pub actieve_stem: Vec<u8>,
    pub dim_andere: bool,
}

/// Eén geschreven noot op het blad (0.7.86): part, maat, stem, positie in
/// rastereenheden binnen de maat, toon en event-ID — de sleutel waarmee het
/// venster een OSMD-noot exact aan zijn event koppelt. Delen van een
/// overgebonden noot dragen dezelfde id.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct NoteRef {
    pub part: u16,
    pub measure: u32,
    pub voice: u8,
    pub pos: u64,
    pub midi: u8,
    pub id: Option<u64>,
}

/// MusicXML voor export en fixtures: één variant, zonder kleuren.
pub fn render_musicxml(qs: &QuantizedScore, opts: &NotationOptions) -> Result<String, String> {
    render_musicxml_met_notemap(qs, opts, None).map(|(xml, _)| xml)
}

/// Eén geschreven noot, akkoord of rust binnen een maat (0.7.83), met per
/// noot (midi, tie_stop, tie_start, event-id).
struct MaatItem { pos: u64, len: u64, type_name: &'static str, dotted: bool, hele_maat: bool, notes: Vec<(u8, bool, bool, Option<u64>)> }

/// De noten en rusten van één stem binnen één maat, uit de segmentlijst van
/// die stem: delen binnen de maat, met overbindingen over maat- en
/// segmentgrenzen (0.7.82), en één hele-maatrust voor een lege maat.
fn maat_items(segments: &[(u64, u64, Vec<ChordNote>)], m_start: u64, m_end: u64, measure_len: u64, q: u64) -> Vec<MaatItem> {
    let mut items: Vec<MaatItem> = Vec::new();
    for (s, e, notes) in segments.iter() {
        let (s, e) = (*s, *e);
        let ps = s.max(m_start);
        let pe = e.min(m_end);
        if pe <= ps { continue; }
        let tie_from_prev = !notes.is_empty() && s < m_start;
        let tie_to_next = !notes.is_empty() && e > m_end;
        if notes.is_empty() && ps == m_start && pe == m_end {
            items.push(MaatItem { pos: ps, len: measure_len, type_name: "", dotted: false, hele_maat: true, notes: Vec::new() });
            continue;
        }
        let parts = decompose(pe - ps, q);
        let mut pos = ps;
        for (pi, (len, type_name, dotted)) in parts.iter().enumerate() {
            let first_part = pi == 0;
            let last_part = pi == parts.len() - 1;
            let noten: Vec<(u8, bool, bool, Option<u64>)> = notes.iter().map(|cn| (
                cn.midi,
                cn.tie_stop || tie_from_prev || !first_part,
                cn.tie_start || tie_to_next || !last_part,
                cn.id,
            )).collect();
            items.push(MaatItem { pos, len: *len, type_name, dotted: *dotted, hele_maat: false, notes: noten });
            pos += len;
        }
    }
    items
}

/// Waardestrepen (0.7.83): aaneengesloten noten korter dan een kwart binnen
/// dezelfde tel (`beam_tel`). Een rust of een langere noot breekt de groep;
/// één losse korte noot houdt haar vlag.
fn beams_voor(items: &[MaatItem], m_start: u64, tel: u64, q: u64) -> Vec<Option<&'static str>> {
    let kort = |it: &MaatItem| !it.notes.is_empty() && it.len < q;
    let mut beam: Vec<Option<&'static str>> = vec![None; items.len()];
    let mut i = 0;
    while i < items.len() {
        if !kort(&items[i]) { i += 1; continue; }
        let groep = (items[i].pos - m_start) / tel;
        let mut j = i;
        while j + 1 < items.len() && kort(&items[j + 1]) && (items[j + 1].pos - m_start) / tel == groep { j += 1; }
        if j > i {
            beam[i] = Some("begin");
            for k in i + 1..j { beam[k] = Some("continue"); }
            beam[j] = Some("end");
        }
        i = j + 1;
    }
    beam
}

/// Schrijft de items van één stem in één maat en vult de notemap.
#[allow(clippy::too_many_arguments)]
fn schrijf_items(xml: &mut String, items: &[MaatItem], beam: &[Option<&'static str>], voice: u8, stem_richting: Option<&str>, kleur: Option<&str>, key_fifths: i8, part: u16, measure: u32, m_start: u64, notemap: &mut Vec<NoteRef>) {
    for (ii, it) in items.iter().enumerate() {
        if it.hele_maat {
            xml.push_str(&format!(
                "      <note><rest measure=\"yes\"/><duration>{}</duration><voice>{}</voice></note>\n",
                it.len, voice));
            continue;
        }
        if it.notes.is_empty() {
            xml.push_str(&format!(
                "      <note><rest/><duration>{}</duration><voice>{}</voice><type>{}</type>{}</note>\n",
                it.len, voice, it.type_name, if it.dotted { "<dot/>" } else { "" }));
            continue;
        }
        for (ni, &(midi, tie_stop, tie_start, id)) in it.notes.iter().enumerate() {
            let (step, alter, octave) = spell(midi, key_fifths);
            xml.push_str("      <note>");
            if ni > 0 { xml.push_str("<chord/>"); }
            xml.push_str(&format!("<pitch><step>{}</step>", step));
            if alter != 0 { xml.push_str(&format!("<alter>{}</alter>", alter)); }
            xml.push_str(&format!("<octave>{}</octave></pitch>", octave));
            xml.push_str(&format!("<duration>{}</duration>", it.len));
            if tie_stop { xml.push_str("<tie type=\"stop\"/>"); }
            if tie_start { xml.push_str("<tie type=\"start\"/>"); }
            xml.push_str(&format!("<voice>{}</voice>", voice));
            xml.push_str(&format!("<type>{}</type>", it.type_name));
            if it.dotted { xml.push_str("<dot/>"); }
            // Stokrichting alleen in een meerstemmige maat (1 en 3 omhoog, 2 en 4 omlaag).
            if let Some(r) = stem_richting { xml.push_str(&format!("<stem>{}</stem>", r)); }
            // Schermvariant: kleur per stem (OSMD leest `notehead color`).
            if let Some(k) = kleur { xml.push_str(&format!("<notehead color=\"{}\">normal</notehead>", k)); }
            // De waardestreep staat op de eerste noot van een akkoord.
            if ni == 0 {
                if let Some(b) = beam[ii] { xml.push_str(&format!("<beam number=\"1\">{}</beam>", b)); }
            }
            if tie_stop || tie_start {
                xml.push_str("<notations>");
                if tie_stop { xml.push_str("<tied type=\"stop\"/>"); }
                if tie_start { xml.push_str("<tied type=\"start\"/>"); }
                xml.push_str("</notations>");
            }
            xml.push_str("</note>\n");
            notemap.push(NoteRef { part, measure, voice, pos: it.pos - m_start, midi, id });
        }
    }
}

/// MusicXML uit het gedeelde model, per balk per stem (0.7.86): akkoorden
/// en overbindingen binnen de stem, elke stem vult de maat, `<backup>`
/// tussen de stemmen, stokrichting alleen in een meerstemmige maat. Met
/// `scherm` komen er stemkleuren in; de notemap koppelt elke noot aan haar
/// event. Eénstemmige partituren geven byte voor byte de uitvoer van 0.7.83.
pub fn render_musicxml_met_notemap(qs: &QuantizedScore, opts: &NotationOptions, scherm: Option<&SchermOpties>) -> Result<(String, Vec<NoteRef>), String> {
    let q = qs.q;
    let beats = qs.beats_per_bar;
    let bpm = qs.bpm;
    let measure_len = qs.measure_len;

    // Per balk: de aanwezige stemmen oplopend, per stem de akkoorden.
    let quantized: Vec<(&QuantizedStaff, Vec<(u8, Vec<Chord>)>)> = qs.staves.iter()
        .map(|st| {
            let mut stemmen: Vec<u8> = st.notes.iter().map(|n| n.voice.clamp(1, 4)).collect();
            stemmen.sort_unstable();
            stemmen.dedup();
            let per_stem: Vec<(u8, Vec<Chord>)> = stemmen.into_iter().map(|v| {
                let noten: Vec<QNote> = st.notes.iter().filter(|n| n.voice.clamp(1, 4) == v).cloned().collect();
                (v, group_chords(&noten))
            }).collect();
            (st, per_stem)
        })
        .collect();
    if quantized.is_empty() {
        return Err("Geen notenbalken in de partituur".into());
    }

    // Totale lengte: langste balk, afgerond op hele maten; minstens het
    // gevraagde aantal maten (0.7.83: een leeg stuk toont lege balken).
    let total = quantized.iter()
        .flat_map(|(_, stemmen)| stemmen.iter().flat_map(|(_, cs)| cs.last().map(|c| c.end)))
        .max().unwrap_or(0);
    let min_measures = opts.min_measures.unwrap_or(0) as u64;
    let num_measures = ((total + measure_len - 1) / measure_len).max(min_measures).max(1);

    let schoon = |v: &Option<String>| v.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    let title = schoon(&opts.title);
    let subtitle = schoon(&opts.subtitle);
    let composer = schoon(&opts.composer);

    let mut xml = String::with_capacity(64 * 1024);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 3.1 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n");
    xml.push_str("<score-partwise version=\"3.1\">\n");
    if let Some(t) = &title {
        xml.push_str(&format!("  <work><work-title>{}</work-title></work>\n", xml_escape(t)));
    }
    // Ondertitel als movement-title: lezers (OSMD, MuseScore, Finale) zetten
    // die onder de werktitel.
    if let Some(st) = &subtitle {
        xml.push_str(&format!("  <movement-title>{}</movement-title>\n", xml_escape(st)));
    }
    if let Some(c) = &composer {
        xml.push_str(&format!("  <identification><creator type=\"composer\">{}</creator></identification>\n", xml_escape(c)));
    }
    xml.push_str("  <part-list>\n");
    // Accolade om aaneengesloten manuaalbalken (minstens twee); de pedaalbalk
    // staat los — het orgelsjabloon van Finale (0.7.83).
    let is_ped: Vec<bool> = quantized.iter().map(|(st, _)| st.pedal || is_pedal_name(&st.name)).collect();
    let score_part = |xml: &mut String, idx: usize| {
        xml.push_str(&format!(
            "    <score-part id=\"P{}\"><part-name>{}</part-name></score-part>\n",
            idx + 1, xml_escape(&quantized[idx].0.name)));
    };
    let mut groep = 0u32;
    let mut i = 0usize;
    while i < quantized.len() {
        if is_ped[i] {
            score_part(&mut xml, i);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < quantized.len() && !is_ped[j] { j += 1; }
        if j - i >= 2 {
            groep += 1;
            xml.push_str(&format!("    <part-group type=\"start\" number=\"{}\"><group-symbol>brace</group-symbol><group-barline>yes</group-barline></part-group>\n", groep));
            for k in i..j { score_part(&mut xml, k); }
            xml.push_str(&format!("    <part-group type=\"stop\" number=\"{}\"/>\n", groep));
        } else {
            score_part(&mut xml, i);
        }
        i = j;
    }
    xml.push_str("  </part-list>\n");

    let tel = beam_tel(measure_len, beats, qs.beat_unit);
    let mut notemap: Vec<NoteRef> = Vec::new();

    for (idx, (staff, stemmen)) in quantized.iter().enumerate() {
        xml.push_str(&format!("  <part id=\"P{}\">\n", idx + 1));
        let staff_total = num_measures * measure_len;
        // Segmentlijst per stem: (start, end, notes-of-leeg=rust), de maat vullend.
        let segs_per_stem: Vec<(u8, Vec<(u64, u64, Vec<ChordNote>)>)> = stemmen.iter().map(|(v, chords)| {
            let mut segments: Vec<(u64, u64, Vec<ChordNote>)> = Vec::new();
            let mut cursor: u64 = 0;
            for c in chords {
                if c.start > cursor { segments.push((cursor, c.start, Vec::new())); }
                segments.push((c.start, c.end, c.notes.clone()));
                cursor = c.end;
            }
            if cursor < staff_total { segments.push((cursor, staff_total, Vec::new())); }
            (*v, segments)
        }).collect();
        let actieve = scherm.and_then(|s| s.actieve_stem.get(idx).copied()).unwrap_or(1).clamp(1, 4);

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
                xml.push_str(&format!("        <time><beats>{}</beats><beat-type>{}</beat-type></time>\n", beats, qs.beat_unit));
                if staff.bass_clef {
                    xml.push_str("        <clef><sign>F</sign><line>4</line></clef>\n");
                } else {
                    xml.push_str("        <clef><sign>G</sign><line>2</line></clef>\n");
                }
                xml.push_str("      </attributes>\n");
                xml.push_str(&format!("      <direction placement=\"above\"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{}</per-minute></metronome></direction-type><sound tempo=\"{}\"/></direction>\n", bpm.round() as u64, bpm.round() as u64));
            }

            // Alleen stemmen met noten in deze maat; zonder noten één
            // hele-maatrust in stem 1 (zoals altijd).
            let mut per_stem_items: Vec<(u8, Vec<MaatItem>)> = Vec::new();
            for (v, segments) in &segs_per_stem {
                let items = maat_items(segments, m_start, m_end, measure_len, q);
                if items.iter().any(|it| !it.notes.is_empty()) { per_stem_items.push((*v, items)); }
            }
            if per_stem_items.is_empty() {
                per_stem_items.push((1, vec![MaatItem { pos: m_start, len: measure_len, type_name: "", dotted: false, hele_maat: true, notes: Vec::new() }]));
            }
            let meerstemmig = per_stem_items.len() > 1;
            for (si, (v, items)) in per_stem_items.iter().enumerate() {
                if si > 0 { xml.push_str(&format!("      <backup><duration>{}</duration></backup>\n", measure_len)); }
                let beam = beams_voor(items, m_start, tel, q);
                let stem_richting = if meerstemmig { Some(if v % 2 == 1 { "up" } else { "down" }) } else { None };
                let kleur: Option<&str> = scherm.map(|s| if s.dim_andere && *v != actieve { DIM_KLEUR } else { STEM_KLEUREN[(*v - 1) as usize] })
                    .filter(|k| *k != STEM_KLEUREN[0]);
                schrijf_items(&mut xml, items, &beam, *v, stem_richting, kleur, opts.key_fifths, idx as u16, m as u32, m_start, &mut notemap);
            }
            xml.push_str("    </measure>\n");
        }
        xml.push_str("  </part>\n");
    }
    xml.push_str("</score-partwise>\n");
    Ok((xml, notemap))
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

fn waar() -> bool { true }
fn een() -> u8 { 1 }

/// Bovengrens voor het minimale aantal maten (wizard, groei met de cursor en
/// geladen bestanden): daarboven wordt renderen onwerkbaar.
pub const MAX_MIN_MEASURES: u32 = 10_000;

/// Eén afgeronde noot in een take (nieuwe naam voor NoteEv-instantie in de
/// live-flow; NoteEv zelf blijft de "flat" struct voor de kwantiseerder).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Stem binnen de balk (0.7.86): 1..=4; oudere bestanden hebben stem 1.
    #[serde(default = "een")]
    pub voice: u8,
    /// Hand in klavar (0.7.70); None = volgens de laag en het splitspunt.
    #[serde(default)]
    pub hand: Option<KlavarHand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Take {
    pub id: u32,
    pub name: String,
    #[serde(default = "waar")]
    pub visible: bool,
    pub events: Vec<LayerEv>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub id: u32,
    pub name: String,
    /// None = automatisch bepalen op basis van de laagnaam.
    pub bass_clef: Option<bool>,
    /// Volgorde: eerste take = eerste opname; nieuwe take = laatste.
    pub takes: Vec<Take>,
    /// Take waar nieuwe MIDI-events in geschreven worden (armed).
    #[serde(default)]
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
    /// Actieve stem (0.7.86): doel van opname, stapinvoer, klik en plakken.
    #[serde(default = "een")]
    pub active_voice: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetronomeCfg {
    pub click_on: bool,
    pub count_in_beats: u8,
}
impl Default for MetronomeCfg {
    fn default() -> Self { Self { click_on: false, count_in_beats: 0 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Noemer van de maatsoort (0.7.83): 2, 4 of 8.
    #[serde(default)]
    pub beat_unit: u8,
    /// Componist en ondertitel op het blad (0.7.83).
    #[serde(default)]
    pub composer: String,
    #[serde(default)]
    pub subtitle: String,
    /// Minimaal aantal maten op het blad (0.7.83); 0 = alleen de inhoud. De
    /// UI verhoogt het zodra de invoercursor voorbij de laatste maat komt.
    #[serde(default)]
    pub min_measures: u32,
    #[serde(default)]
    pub metronome: MetronomeCfg,
    /// Alleen armed = actief onder één laag tegelijk; None = geen opname.
    #[serde(default)]
    pub armed_layer: Option<u32>,
    /// Monotoon oplopende counter — de UI regenereert MusicXML zodra deze wijzigt.
    #[serde(default)]
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
            beat_unit: 4,
            composer: String::new(),
            subtitle: String::new(),
            min_measures: 0,
            metronome: MetronomeCfg::default(),
            armed_layer: None,
            generation: 1,
            next_event_id: 1, next_layer_id: 1, next_take_id: 1,
            undo: Vec::new(), redo: Vec::new(),
        }
    }
    /// Na het laden uit een bestand (0.7.84): tellers herleiden uit de inhoud
    /// zodat nieuwe ID's nooit botsen, undo/redo leeg, waarden geklemd, armed
    /// laag en take geldig, generation vers.
    pub fn herleid_na_laden(&mut self) {
        let max_ev = self.layers.iter().flat_map(|l| l.takes.iter()).flat_map(|t| t.events.iter()).map(|e| e.id).max().unwrap_or(0);
        let max_laag = self.layers.iter().map(|l| l.id).max().unwrap_or(0);
        let max_take = self.layers.iter().flat_map(|l| l.takes.iter()).map(|t| t.id).max().unwrap_or(0);
        self.next_event_id = max_ev.saturating_add(1);
        self.next_layer_id = max_laag.saturating_add(1);
        self.next_take_id = max_take.saturating_add(1);
        // Dubbele ID's (alleen uit een bewerkt of beschadigd bestand) krijgen
        // een vers nummer: elk bewerkcommando leunt op "stabiel, uniek ID".
        let mut gezien_laag: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut gezien_take: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut gezien_ev: std::collections::HashSet<u64> = std::collections::HashSet::new();
        for l in &mut self.layers {
            if !gezien_laag.insert(l.id) { l.id = self.next_layer_id; self.next_layer_id = self.next_layer_id.saturating_add(1); gezien_laag.insert(l.id); }
            for t in &mut l.takes {
                if !gezien_take.insert(t.id) { t.id = self.next_take_id; self.next_take_id = self.next_take_id.saturating_add(1); gezien_take.insert(t.id); }
                for e in &mut t.events {
                    if !gezien_ev.insert(e.id) { e.id = self.next_event_id; self.next_event_id = self.next_event_id.saturating_add(1); gezien_ev.insert(e.id); }
                }
            }
        }
        self.undo.clear();
        self.redo.clear();
        self.generation = 1;
        self.min_measures = self.min_measures.min(MAX_MIN_MEASURES);
        self.beat_unit = klem_beat_unit(self.beat_unit);
        self.beats_per_bar = self.beats_per_bar.clamp(1, 12);
        self.quantize = self.quantize.clamp(1, 8);
        self.key_fifths = self.key_fifths.clamp(-7, 7);
        self.tolerance_pct = self.tolerance_pct.min(100);
        if !self.bpm.is_finite() || !(20.0..=300.0).contains(&self.bpm) { self.bpm = 90.0; }
        if self.armed_layer.map(|a| !self.layers.iter().any(|l| l.id == a)).unwrap_or(true) {
            self.armed_layer = self.layers.first().map(|l| l.id);
        }
        for l in &mut self.layers {
            if l.armed_take.map(|a| !l.takes.iter().any(|t| t.id == a)).unwrap_or(true) {
                l.armed_take = l.takes.last().map(|t| t.id);
            }
        }
    }
    pub fn undo_len(&self) -> usize { self.undo.len() }
    pub fn redo_len(&self) -> usize { self.redo.len() }
    pub fn new_event_id(&mut self) -> u64 { let id = self.next_event_id; self.next_event_id = self.next_event_id.saturating_add(1); id }
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
        let id = self.next_layer_id; self.next_layer_id = self.next_layer_id.saturating_add(1);
        // Nieuwe laag krijgt automatisch een eerste (lege) take.
        let take_id = self.next_take_id; self.next_take_id = self.next_take_id.saturating_add(1);
        self.layers.push(Layer {
            id, name, bass_clef,
            takes: vec![Take { id: take_id, name: "Take 1".into(), visible: true, events: Vec::new() }],
            armed_take: Some(take_id),
            divisions: Vec::new(),
            klavar_hand: None,
            klavar_split: None,
            active_voice: 1,
        });
        self.bump_gen();
        id
    }

    pub fn add_take(&mut self, layer_id: u32) -> Option<u32> {
        let take_id = self.next_take_id; self.next_take_id = self.next_take_id.saturating_add(1);
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
                    voice: ev.voice.clamp(1, 4),
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
        beat_unit: Some(score.beat_unit),
        min_measures: Some(score.min_measures),
        composer: Some(score.composer.clone()),
        subtitle: Some(score.subtitle.clone()),
    }
}

/// Bouw MusicXML uit een Score met de Score-eigen opties + tolerantie.
pub fn build_musicxml_from_score(score: &Score) -> Result<String, String> {
    build_musicxml(&score_to_staves(score), &options_from_score(score))
}

/// Schermvariant (0.7.86): MusicXML met stemkleuren plus de notemap; de
/// actieve stem per balk komt uit de lagen, `dim_andere` grijst de rest.
pub fn build_notation_from_score(score: &Score, dim_andere: bool) -> Result<(String, Vec<NoteRef>), String> {
    let staves = score_to_staves(score);
    let opts = options_from_score(score);
    let qs = quantize_score(&staves, &opts);
    let scherm = SchermOpties { actieve_stem: score.layers.iter().map(|l| l.active_voice.clamp(1, 4)).collect(), dim_andere };
    render_musicxml_met_notemap(&qs, &opts, Some(&scherm))
}

/// "Akkoord splitsen in stemmen" (0.7.86): per akkoord de hoogste noot naar
/// stem 1, de rest naar stem 2. Een akkoord = inzetten binnen `venster_us`
/// van de eerste noot van de groep — hetzelfde venster als de akkoord-
/// clustering van het blad, zodat splitsen klopt met wat er staat. Invoer:
/// (event-id, midi, start_us); uitvoer: (event-id, stem).
pub fn verdeel_akkoord_in_stemmen(events: &[(u64, u8, u64)], venster_us: u64) -> Vec<(u64, u8)> {
    let mut gesorteerd: Vec<(u64, u8, u64)> = events.to_vec();
    gesorteerd.sort_by_key(|&(id, _, start)| (start, id));
    let mut groepen: Vec<Vec<(u64, u8)>> = Vec::new();
    let mut groep_start: u64 = 0;
    for &(id, midi, start) in &gesorteerd {
        if groepen.is_empty() || start.saturating_sub(groep_start) > venster_us {
            groepen.push(Vec::new());
            groep_start = start;
        }
        groepen.last_mut().expect("groep").push((id, midi));
    }
    let mut uit: Vec<(u64, u8)> = Vec::new();
    for groep in groepen {
        let hoogste = groep.iter().map(|&(_, m)| m).max().unwrap_or(0);
        let mut eerste_hoogste = true;
        for (id, midi) in groep {
            if midi == hoogste && eerste_hoogste { uit.push((id, 1)); eerste_hoogste = false; }
            else { uit.push((id, 2)); }
        }
    }
    uit
}

/// Exporteer de zichtbare takes van een Score als Standard MIDI File (format 1,
/// 480 ppq): track 0 = tempo + maatsoort, daarna één track per balk met noten
/// (originele MIDI-kanalen behouden; note-off vóór note-on bij gelijke tick).
/// Kanaal van een event zonder bronkanaal (stapinvoer, muisklik): bij de
/// MIDI-export wordt dan het ingeleerde kanaal van de laag gebruikt.
pub const KANAAL_ONBEKEND: u8 = 0xFF;

/// `kanaal_voor_laag`: ingeleerd MIDI-kanaal van de eerste divisie van een
/// laag (0.7.82); events zonder bronkanaal (KANAAL_ONBEKEND: stapinvoer/klik)
/// gaan daarheen, zodat een getypte pedaalnoot bij afspelen op het pedaal
/// klinkt; opgenomen events houden hun eigen kanaal.
pub fn score_to_smf_bytes(score: &Score, kanaal_voor_laag: &dyn Fn(&Layer) -> Option<u8>) -> Result<Vec<u8>, String> {
    use midly::{Smf, Header, Format, Timing, TrackEvent, TrackEventKind, MetaMessage, MidiMessage};
    use midly::num::{u4, u7, u15, u24, u28};
    const PPQ: u64 = 480;
    let bpm = if score.bpm.is_finite() && score.bpm >= 20.0 && score.bpm <= 300.0 { score.bpm } else { 90.0 };
    let us_per_qn: u32 = (60_000_000.0 / bpm).round() as u32;
    let to_ticks = |us: u64| -> u64 { (us as u128 * PPQ as u128 / us_per_qn as u128) as u64 };

    let mut smf = Smf::new(Header::new(Format::Parallel, Timing::Metrical(u15::new(PPQ as u16))));
    let mut t0: Vec<TrackEvent> = Vec::new();
    t0.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(us_per_qn))) });
    // Maatsoort (noemer als macht van 2: 1 → halve, 2 → kwart, 3 → achtste).
    let noemer_log2 = match klem_beat_unit(score.beat_unit) { 2 => 1, 8 => 3, _ => 2 };
    t0.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::TimeSignature(score.beats_per_bar.clamp(1, 12), noemer_log2, 24, 8)) });
    t0.push(TrackEvent { delta: u28::new(0), kind: TrackEventKind::Meta(MetaMessage::EndOfTrack) });
    smf.tracks.push(t0);

    for layer in &score.layers {
        // (tick, soort 0=off/1=on, midi, kanaal) — off vóór on bij gelijke tick.
        let mut evs: Vec<(u64, u8, u8, u8)> = Vec::new();
        let laag_kanaal = kanaal_voor_laag(layer).unwrap_or(0) & 0x0F;
        for take in layer.takes.iter().filter(|t| t.visible) {
            for e in &take.events {
                // KANAAL_ONBEKEND (stapinvoer/klik) → laagkanaal; een opgenomen
                // kanaal 0 is een gewoon kanaal en blijft staan (reviewbevinding).
                let ch = if e.channel > 0x0F { laag_kanaal } else { e.channel };
                evs.push((to_ticks(e.start_us), 1, e.midi & 0x7F, ch));
                evs.push((to_ticks(e.end_us.max(e.start_us + 1000)), 0, e.midi & 0x7F, ch));
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
    /// Wijzig maatsoort: (tellen per maat, noemer 2/4/8) — 0.7.83.
    SetMeter { old: (u8, u8), new: (u8, u8) },
    /// Balk hernoemen (0.7.83).
    SetLayerName { layer: u32, old: String, new: String },
    /// Balk verplaatsen in de volgorde (0.7.83): van index `from` naar `to`.
    MoveLayer { from: usize, to: usize },
    /// Kop van het stuk (0.7.83): (titel, componist, ondertitel).
    SetHeader { old: (String, String, String), new: (String, String, String) },
    /// Wijzig het toongeslacht (majeur/mineur, 0.7.72).
    SetMode { old: bool, new: bool },
    /// Hand en splitspunt van een balk in klavar (0.7.70).
    SetLayerHand { layer: u32, old: (Option<KlavarHand>, Option<u8>), new: (Option<KlavarHand>, Option<u8>) },
    /// Hand per noot in klavar (0.7.70): (event-id, nieuwe hand); undo bewaart de oude.
    SetHands { items: Vec<(u64, Option<KlavarHand>)> },
    /// Stem per noot (0.7.86): (event-id, stem 1..=4); undo bewaart de oude.
    SetVoice { items: Vec<(u64, u8)> },
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
                let new = (new.0.clamp(1, 12), klem_beat_unit(new.1));
                if new == (score.beats_per_bar, score.beat_unit) { return None; }
                score.beats_per_bar = new.0;
                score.beat_unit = new.1;
                score.bump_gen();
                Some(EditCommand::SetMeter { old: new, new: old })
            }
            EditCommand::SetLayerName { layer, old, new } => {
                let l = score.layers.iter_mut().find(|l| l.id == layer)?;
                let new = new.trim().to_string();
                if new.is_empty() || l.name == new { return None; }
                l.name = new.clone();
                score.bump_gen();
                Some(EditCommand::SetLayerName { layer, old: new, new: old })
            }
            EditCommand::MoveLayer { from, to } => {
                let n = score.layers.len();
                if from >= n || to >= n || from == to { return None; }
                let l = score.layers.remove(from);
                score.layers.insert(to, l);
                score.bump_gen();
                Some(EditCommand::MoveLayer { from: to, to: from })
            }
            EditCommand::SetHeader { old, new } => {
                let huidig = (score.title.clone(), score.composer.clone(), score.subtitle.clone());
                if huidig == new { return None; }
                score.title = new.0;
                score.composer = new.1;
                score.subtitle = new.2;
                score.bump_gen();
                Some(EditCommand::SetHeader { old: huidig, new: old })
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
            EditCommand::SetVoice { items } => {
                let mut old: Vec<(u64, u8)> = Vec::new();
                for (id, voice) in items.into_iter() {
                    let voice = voice.clamp(1, 4);
                    if let Some((li, ti, ei)) = score.locate(id) {
                        let ev = &mut score.layers[li].takes[ti].events[ei];
                        if ev.voice != voice {
                            old.push((id, ev.voice));
                            ev.voice = voice;
                        }
                    }
                }
                if old.is_empty() { return None; }
                score.bump_gen();
                Some(EditCommand::SetVoice { items: old })
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
        NotationOptions { bpm: 60.0, beats_per_bar: 4, quantize: 4, key_fifths: 0, title: Some("Test".into()), staves: None, tolerance_pct: None, minor: None, beat_unit: None, min_measures: None, composer: None, subtitle: None }
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
        // 0.7.82: C/E lopen dóór onder G (doorgebonden), niet afgekapt.
        assert_eq!(chords.len(), 3);
        assert_eq!(chords[0].midis(), vec![60, 64]);
        assert_eq!((chords[0].start, chords[0].end), (0, 2));
        assert!(chords[0].notes.iter().all(|n| n.tie_start && !n.tie_stop));
        assert_eq!(chords[1].midis(), vec![60, 64, 67]);
        assert_eq!((chords[1].start, chords[1].end), (2, 4));
        assert!(chords[1].notes.iter().filter(|n| n.midi != 67).all(|n| n.tie_stop && !n.tie_start));
        let g = chords[1].notes.iter().find(|n| n.midi == 67).unwrap();
        assert!(!g.tie_stop && g.tie_start);
        assert_eq!(chords[2].midis(), vec![67]);
        assert_eq!((chords[2].start, chords[2].end), (4, 6));
        assert!(chords[2].notes[0].tie_stop && !chords[2].notes[0].tie_start);
    }

    fn staf(notes: Vec<NoteEv>) -> Vec<Staff> {
        vec![Staff { name: "Test".into(), bass_clef: false, notes, ..Default::default() }]
    }
    fn akkoorden(notes: Vec<NoteEv>, tol: Option<u8>) -> Vec<Chord> {
        let mut o = opts();
        o.tolerance_pct = tol;
        let qs = quantize_score(&staf(notes), &o);
        group_chords(&qs.staves[0].notes)
    }

    #[test]
    fn venster_grenzen() {
        // 60 bpm, q 4: cel 0,25 s → 0 % = 40 ms, 100 % = halve cel.
        assert!((akkoord_venster_sec(60.0, 4, 0) - 0.040).abs() < 1e-9);
        assert!((akkoord_venster_sec(60.0, 4, 100) - 0.125).abs() < 1e-9);
        assert!((akkoord_venster_sec(60.0, 4, 50) - 0.0625).abs() < 1e-9);
    }

    #[test]
    fn akkoord_over_celgrens() {
        // Twee noten 30 ms uit elkaar rond de celgrens (0,125 s): één akkoord.
        let c = akkoorden(vec![NoteEv::anoniem(60, 0.11, 1.0), NoteEv::anoniem(64, 0.14, 1.0)], Some(100));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].midis(), vec![60, 64]);
        assert_eq!((c[0].start, c[0].end), (0, 4));
    }

    #[test]
    fn gebroken_akkoord_blijft_gebroken() {
        // 0,2 s uit elkaar (0,8 cel > venster van een halve cel): twee inzetten.
        let c = akkoorden(vec![NoteEv::anoniem(60, 0.0, 1.0), NoteEv::anoniem(64, 0.2, 1.0)], Some(100));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].midis(), vec![60]);
        assert_eq!(c[1].midis(), vec![60, 64]);
    }

    #[test]
    fn aangehouden_bas_wordt_doorgebonden() {
        // Bas vier tellen onder vier kwartakkoorden: vier keer de bas, overgebonden.
        let mut notes = vec![NoteEv::anoniem(36, 0.0, 4.0)];
        for k in 0..4 {
            notes.push(NoteEv::anoniem(60, k as f64, k as f64 + 1.0));
            notes.push(NoteEv::anoniem(64, k as f64, k as f64 + 1.0));
        }
        let c = akkoorden(notes, Some(80));
        assert_eq!(c.len(), 4);
        for (k, ch) in c.iter().enumerate() {
            assert_eq!(ch.midis(), vec![36, 60, 64]);
            let bas = &ch.notes[0];
            assert_eq!(bas.tie_stop, k > 0);
            assert_eq!(bas.tie_start, k < 3);
            assert!(ch.notes[1..].iter().all(|n| !n.tie_start && !n.tie_stop));
        }
        // En in de MusicXML: nergens een rust, de bas 4× met tie.
        let xml = build_musicxml(&staf(vec![NoteEv::anoniem(36, 0.0, 4.0), NoteEv::anoniem(60, 0.0, 1.0),
            NoteEv::anoniem(60, 1.0, 2.0), NoteEv::anoniem(60, 2.0, 3.0), NoteEv::anoniem(60, 3.0, 4.0)]), &opts()).unwrap();
        assert_eq!(xml.matches("<tie type=\"start\"/>").count(), 3);
        assert_eq!(xml.matches("<tie type=\"stop\"/>").count(), 3);
    }

    #[test]
    fn kortste_noot_bepaalt_akkoordduur() {
        let c = akkoorden(vec![NoteEv::anoniem(60, 0.0, 1.0), NoteEv::anoniem(64, 0.0, 2.0)], Some(100));
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].start, c[0].end), (0, 4));
        assert_eq!((c[1].start, c[1].end), (4, 8));
        assert_eq!(c[1].midis(), vec![64]);
        assert!(c[0].notes[1].tie_start && c[1].notes[0].tie_stop);
    }

    #[test]
    fn herinzet_wint_van_tie() {
        // Dezelfde toon opnieuw aangeslagen terwijl hij nog klinkt: geen boog.
        let c = akkoorden(vec![NoteEv::anoniem(60, 0.0, 2.0), NoteEv::anoniem(60, 1.0, 2.0)], Some(100));
        assert_eq!(c.len(), 2);
        assert!(!c[0].notes[0].tie_start);
        assert!(!c[1].notes[0].tie_stop);
    }

    #[test]
    fn legato_overlap_geeft_geen_fragment() {
        // 120 bpm, q 8 (cel 62,5 ms): 60 wordt 40 ms ná de inzet van 62 losgelaten
        // (vingerlegato) → twee losse noten, geen fragment, geen boog.
        let mut o = opts();
        o.bpm = 120.0; o.quantize = 8; o.tolerance_pct = Some(100);
        let qs = quantize_score(&staf(vec![NoteEv::anoniem(60, 0.0, 0.29), NoteEv::anoniem(62, 0.25, 0.50)]), &o);
        let c = group_chords(&qs.staves[0].notes);
        assert_eq!(c.len(), 2, "{:?}", c);
        assert_eq!((c[0].start, c[0].end, c[0].midis()), (0, 4, vec![60]));
        assert_eq!((c[1].start, c[1].end, c[1].midis()), (4, 8, vec![62]));
        assert!(c.iter().all(|ch| ch.notes.iter().all(|n| !n.tie_start && !n.tie_stop)));
        // Negatieve controle: 150 ms overlap (≥ cel + venster) blijft liggen en bindt door.
        let qs2 = quantize_score(&staf(vec![NoteEv::anoniem(60, 0.0, 0.40), NoteEv::anoniem(62, 0.25, 0.50)]), &o);
        let c2 = group_chords(&qs2.staves[0].notes);
        assert_eq!(c2.len(), 3);
        assert!(c2[0].notes[0].tie_start);
    }

    #[test]
    fn midi_export_kanaal_per_laag() {
        let mut sc = Score::new(1);
        sc.add_layer("Pedaal".into(), Some(true));
        let lid = sc.layers[0].id;
        sc.layers[0].divisions = vec!["Pedaal".into()];
        let _ = lid;
        let take = &mut sc.layers[0].takes[0]; // add_layer maakt "Take 1"
        take.visible = true;
        take.events.push(LayerEv { id: 1, midi: 36, start_us: 0, end_us: 500_000, channel: KANAAL_ONBEKEND, locked: true, voice: 1, hand: None });
        take.events.push(LayerEv { id: 2, midi: 38, start_us: 500_000, end_us: 1_000_000, channel: 0, locked: false, voice: 1, hand: None });
        let bytes = score_to_smf_bytes(&sc, &|_l| Some(2)).expect("smf");
        let smf = midly::Smf::parse(&bytes).expect("parse");
        let mut kanalen = Vec::new();
        for ev in &smf.tracks[1] {
            if let midly::TrackEventKind::Midi { channel, message: midly::MidiMessage::NoteOn { key, .. } } = ev.kind {
                kanalen.push((key.as_int(), channel.as_int()));
            }
        }
        assert_eq!(kanalen, vec![(36, 2), (38, 0)]);
    }

    #[test]
    fn release_spreiding_geeft_geen_fragment() {
        // Loslaten 40 ms uit elkaar: één akkoord zonder overbinding.
        let c = akkoorden(vec![NoteEv::anoniem(60, 0.0, 1.0), NoteEv::anoniem(64, 0.0, 0.96)], Some(100));
        assert_eq!(c.len(), 1);
        assert!(c[0].notes.iter().all(|n| !n.tie_start && !n.tie_stop));
        // Bewust vroeg losgelaten (0,3 s eerder): blijft korter, de rest bindt door.
        let c2 = akkoorden(vec![NoteEv::anoniem(60, 0.0, 1.0), NoteEv::anoniem(64, 0.0, 0.7)], Some(100));
        assert_eq!(c2.len(), 2);
        assert_eq!(c2[1].midis(), vec![60]);
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
            NoteEv { midi: 60, start_sec: 0.0, end_sec: 1.0, id: Some(7), hand: Some(KlavarHand::Left), voice: 1 },
            NoteEv { midi: 64, start_sec: 0.02, end_sec: 0.98, id: Some(8), hand: None, voice: 1 },
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
            NoteEv { midi: 60, start_sec: 0.0, end_sec: 1.0, id: Some(1), hand: None, voice: 1 },
            NoteEv { midi: 60, start_sec: 0.0, end_sec: 2.0, id: Some(2), hand: None, voice: 1 },
        ];
        let q = quantize_notes(&notes, 60.0, 4, 100);
        assert_eq!(q.len(), 2);
        let chords = group_chords(&q);
        assert_eq!(chords.len(), 1);
        assert_eq!(chords[0].midis(), vec![60]);
        // Eén notenkop met het langste einde (wat klinkt is de vereniging).
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
        sc.layers[0].takes[0].events.push(LayerEv { id, midi: 60, start_us: 0, end_us: 500_000, channel: 0, locked: false, voice: 1, hand: Some(KlavarHand::Left) });
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
        sc.layers[0].takes[0].events.push(LayerEv { id, midi: 60, start_us: 0, end_us: 1000, channel: 0, locked: false, voice: 1, hand: None });
        let cmd = EditCommand::SetHands { items: vec![(id, Some(KlavarHand::Right)), (999, Some(KlavarHand::Left))] };
        let inv = cmd.apply(&mut sc).expect("inverse");
        assert_eq!(sc.layers[0].takes[0].events[0].hand, Some(KlavarHand::Right));
        inv.apply(&mut sc);
        assert_eq!(sc.layers[0].takes[0].events[0].hand, None);
    }

    #[test]
    fn lege_balk_geeft_lege_maat_en_zonder_balken_fout() {
        // 0.7.83: een leeg stuk toont lege notenbalken (één maat met een
        // hele-maatrust); alleen zónder balken is er niets te tekenen.
        let staves = vec![Staff { name: "X".into(), bass_clef: false, notes: vec![], ..Default::default() }];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        assert_eq!(xml.matches("<measure number=").count(), 1);
        assert!(xml.contains("<rest measure=\"yes\"/><duration>16</duration>"));
        assert!(build_musicxml(&[], &opts()).is_err());
    }

    #[test]
    fn lege_score_rendert_min_measures() {
        let staves = vec![
            Staff { name: "Hoofdwerk".into(), bass_clef: false, notes: vec![], ..Default::default() },
            Staff { name: "Positief".into(), bass_clef: false, notes: vec![], ..Default::default() },
            Staff { name: "Pedaal".into(), bass_clef: true, notes: vec![], pedal: true, ..Default::default() },
        ];
        let mut o = opts();
        o.min_measures = Some(8);
        let xml = build_musicxml(&staves, &o).expect("xml");
        assert_eq!(xml.matches("<part id=").count(), 3);
        assert_eq!(xml.matches("<measure number=\"8\">").count(), 3);
        assert!(!xml.contains("<measure number=\"9\">"));
        assert_eq!(xml.matches("<rest measure=\"yes\"/>").count(), 24);
        // Accolade om de twee manualen, pedaal erbuiten.
        assert!(xml.contains("<part-group type=\"start\" number=\"1\"><group-symbol>brace</group-symbol>"));
        let start = xml.find("<part-group type=\"start\"").unwrap();
        let stop = xml.find("<part-group type=\"stop\"").unwrap();
        let p2 = xml.find("<score-part id=\"P2\"").unwrap();
        let p3 = xml.find("<score-part id=\"P3\"").unwrap();
        assert!(start < p2 && p2 < stop && stop < p3);
    }

    #[test]
    fn part_group_alleen_bij_twee_manualen() {
        // Eén manuaal + pedaal: geen accolade.
        let staves = vec![
            Staff { name: "Manuaal".into(), bass_clef: false, notes: vec![NoteEv::anoniem(60, 0.0, 1.0)], ..Default::default() },
            Staff { name: "Pedaal".into(), bass_clef: true, notes: vec![], pedal: true, ..Default::default() },
        ];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        assert!(!xml.contains("<part-group"));
        // Pedaal in het midden: de manualen eromheen vormen géén groep.
        let staves = vec![
            Staff { name: "A".into(), bass_clef: false, notes: vec![], ..Default::default() },
            Staff { name: "Pedaal".into(), bass_clef: true, notes: vec![], pedal: true, ..Default::default() },
            Staff { name: "B".into(), bass_clef: false, notes: vec![], ..Default::default() },
        ];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        assert!(!xml.contains("<part-group"));
    }

    #[test]
    fn hele_maat_rust_in_drie_kwarts() {
        // Een lege maat tussen noten wordt één hele-maatrust, geen gepunteerde halve.
        let staves = vec![Staff { name: "T".into(), bass_clef: false,
            notes: vec![NoteEv::anoniem(60, 0.0, 1.0), NoteEv::anoniem(60, 6.0, 7.0)], ..Default::default() }];
        let mut o = opts();
        o.beats_per_bar = 3;
        let xml = build_musicxml(&staves, &o).expect("xml");
        let m2 = xml.find("<measure number=\"2\">").unwrap();
        let m3 = xml.find("<measure number=\"3\">").unwrap();
        let maat2 = &xml[m2..m3];
        assert!(maat2.contains("<rest measure=\"yes\"/><duration>12</duration>"));
        assert!(!maat2.contains("<type>"));
    }

    #[test]
    fn zes_acht_measure_len_en_drie_halve() {
        assert_eq!(raster_en_maatlengte(4, 6, 8), (4, 12));
        assert_eq!(raster_en_maatlengte(4, 3, 2), (4, 24));
        assert_eq!(raster_en_maatlengte(2, 4, 4), (2, 8));
        // /8 eist minstens een achtste als raster.
        assert_eq!(raster_en_maatlengte(1, 6, 8), (2, 6));
        // Onbekende noemer → 4.
        assert_eq!(klem_beat_unit(3), 4);
        let staves = vec![Staff { name: "T".into(), bass_clef: false,
            notes: vec![NoteEv::anoniem(60, 0.0, 0.5), NoteEv::anoniem(62, 0.5, 1.0), NoteEv::anoniem(64, 1.0, 1.5)], ..Default::default() }];
        let mut o = opts();
        o.beats_per_bar = 6; o.beat_unit = Some(8); o.quantize = 2;
        let xml = build_musicxml(&staves, &o).expect("xml");
        assert!(xml.contains("<time><beats>6</beats><beat-type>8</beat-type></time>"));
        assert!(xml.contains("<divisions>2</divisions>"));
        // Drie achtsten (1,5 s bij 60 bpm) vullen de eerste maat van 6/8.
        assert_eq!(xml.matches("<type>eighth</type>").count(), 3);
        assert_eq!(xml.matches("<measure number=").count(), 1);
    }

    #[test]
    fn waardestrepen_per_tel() {
        // 4/4: twee achtsten in één tel krijgen begin/end; een losse achtste
        // en een achtste na een rust houden hun vlag.
        let staves = vec![Staff { name: "T".into(), bass_clef: false,
            notes: vec![NoteEv::anoniem(60, 0.0, 0.5), NoteEv::anoniem(62, 0.5, 1.0), NoteEv::anoniem(64, 1.0, 1.5), NoteEv::anoniem(65, 2.0, 2.5)],
            ..Default::default() }];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        assert_eq!(xml.matches("<beam number=\"1\">begin</beam>").count(), 1);
        assert_eq!(xml.matches("<beam number=\"1\">end</beam>").count(), 1);
        assert!(!xml.contains(">continue</beam>"));
        // 6/8 bij q=2: zes achtsten → twee groepen van drie.
        let mut o = opts();
        o.beats_per_bar = 6; o.beat_unit = Some(8); o.quantize = 2;
        let staves = vec![Staff { name: "T".into(), bass_clef: false,
            notes: (0..6).map(|i| NoteEv::anoniem(60 + i as u8, i as f64 * 0.5, i as f64 * 0.5 + 0.5)).collect(),
            ..Default::default() }];
        let xml = build_musicxml(&staves, &o).expect("xml");
        assert_eq!(xml.matches(">begin</beam>").count(), 2);
        assert_eq!(xml.matches(">continue</beam>").count(), 2);
        assert_eq!(xml.matches(">end</beam>").count(), 2);
        assert_eq!(beam_tel(6, 6, 8), 3);
        assert_eq!(beam_tel(24, 3, 2), 8);
        assert_eq!(beam_tel(16, 4, 4), 4);
        // Een akkoord van achtsten: de streep alleen op de eerste noot.
        let staves = vec![Staff { name: "T".into(), bass_clef: false,
            notes: vec![NoteEv::anoniem(60, 0.0, 0.5), NoteEv::anoniem(64, 0.0, 0.5), NoteEv::anoniem(62, 0.5, 1.0)],
            ..Default::default() }];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        assert_eq!(xml.matches("<beam").count(), 2);
        // Kwarten krijgen nooit een streep.
        let staves = vec![Staff { name: "T".into(), bass_clef: false,
            notes: vec![NoteEv::anoniem(60, 0.0, 1.0), NoteEv::anoniem(62, 1.0, 2.0)], ..Default::default() }];
        assert!(!build_musicxml(&staves, &opts()).expect("xml").contains("<beam"));
    }

    fn noot_stem(midi: u8, s: f64, e: f64, stem: u8) -> NoteEv {
        let mut n = NoteEv::anoniem(midi, s, e);
        n.voice = stem;
        n
    }

    #[test]
    fn twee_stemmen_backup_en_voice() {
        // 4/4, 60 bpm: vier kwarten in stem 1 boven een hele noot in stem 2.
        let staves = vec![Staff { name: "M".into(), bass_clef: false, notes: vec![
            noot_stem(72, 0.0, 1.0, 1), noot_stem(74, 1.0, 2.0, 1), noot_stem(76, 2.0, 3.0, 1), noot_stem(77, 3.0, 4.0, 1),
            noot_stem(60, 0.0, 4.0, 2)], ..Default::default() }];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        assert_eq!(xml.matches("<voice>1</voice>").count(), 4);
        assert_eq!(xml.matches("<voice>2</voice>").count(), 1);
        assert_eq!(xml.matches("<backup><duration>16</duration></backup>").count(), 1);
        assert_eq!(xml.matches("<stem>up</stem>").count(), 4);
        assert_eq!(xml.matches("<stem>down</stem>").count(), 1);
        assert!(xml.contains("<type>whole</type>"));
        // De liggende noot in stem 2 wordt niet ingekort door stem 1: geen overbinding.
        assert!(!xml.contains("<tie"));
        // Export: geen kleuren.
        assert!(!xml.contains("color="));
    }

    #[test]
    fn een_stem_geen_stem_element_en_alleen_stem_2() {
        let staves = vec![Staff { name: "M".into(), bass_clef: false, notes: vec![noot_stem(60, 0.0, 1.0, 1), noot_stem(62, 1.0, 2.0, 1)], ..Default::default() }];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        assert!(!xml.contains("<stem>") && !xml.contains("<backup>"));
        // Maat 2 met alleen stem 2: alleen stem 2, zonder stemrichting; maat 1 leeg in stem 1.
        let staves = vec![Staff { name: "M".into(), bass_clef: false, notes: vec![noot_stem(60, 4.0, 5.0, 2)], ..Default::default() }];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        let m2 = xml.find("<measure number=\"2\">").unwrap();
        assert!(xml[m2..].contains("<voice>2</voice>"));
        assert!(!xml[m2..].contains("<voice>1</voice>"));
        assert!(!xml.contains("<stem>"));
        assert!(xml[..m2].contains("<rest measure=\"yes\"/><duration>16</duration><voice>1</voice>"));
    }

    #[test]
    fn backup_som_klopt_en_rusten_per_stem() {
        // Stem 1: kwart op tel 1 en 3; stem 2: halve op tel 1. Elke stem vult de maat.
        let staves = vec![Staff { name: "M".into(), bass_clef: false, notes: vec![
            noot_stem(72, 0.0, 1.0, 1), noot_stem(74, 2.0, 3.0, 1), noot_stem(60, 0.0, 2.0, 2)], ..Default::default() }];
        let xml = build_musicxml(&staves, &opts()).expect("xml");
        let maat = &xml[xml.find("<measure number=\"1\">").unwrap()..xml.find("</measure>").unwrap()];
        let mut som: HashMap<u8, u64> = HashMap::new();
        for note in maat.split("<note>").skip(1) {
            let dur: u64 = note.split("<duration>").nth(1).unwrap().split('<').next().unwrap().parse().unwrap();
            let voice: u8 = note.split("<voice>").nth(1).unwrap().split('<').next().unwrap().parse().unwrap();
            *som.entry(voice).or_insert(0) += dur;
        }
        assert_eq!(som.get(&1), Some(&16));
        assert_eq!(som.get(&2), Some(&16));
        assert_eq!(maat.matches("<rest/>").count(), 3);
    }

    #[test]
    fn inkorting_alleen_binnen_stem_en_notemap() {
        let staves = vec![Staff { name: "M".into(), bass_clef: false, notes: vec![
            NoteEv { midi: 72, start_sec: 0.0, end_sec: 1.0, id: Some(1), hand: None, voice: 1 },
            NoteEv { midi: 60, start_sec: 0.0, end_sec: 6.0, id: Some(2), hand: None, voice: 2 }], ..Default::default() }];
        let qs = quantize_score(&staves, &opts());
        let (xml, map) = render_musicxml_met_notemap(&qs, &opts(), None).expect("xml");
        // Stem 2 loopt over de maatstreep: twee delen met dezelfde id.
        let s2: Vec<&NoteRef> = map.iter().filter(|r| r.voice == 2).collect();
        assert_eq!(s2.len(), 2);
        assert!(s2.iter().all(|r| r.id == Some(2)));
        assert_eq!((s2[0].measure, s2[0].pos), (0, 0));
        assert_eq!((s2[1].measure, s2[1].pos), (1, 0));
        assert_eq!(map.iter().filter(|r| r.voice == 1).count(), 1);
        // Elke geschreven noot heeft een notemap-regel; de liggende noot is niet ingekort.
        assert_eq!(xml.matches("<pitch>").count(), map.len());
        assert!(xml.contains("<tie type=\"start\"/>"));
    }

    #[test]
    fn schermvariant_kleurt_stemmen() {
        let staves = vec![Staff { name: "M".into(), bass_clef: false, notes: vec![noot_stem(72, 0.0, 1.0, 1), noot_stem(60, 0.0, 1.0, 2)], ..Default::default() }];
        let qs = quantize_score(&staves, &opts());
        let scherm = SchermOpties { actieve_stem: vec![1], dim_andere: false };
        let (xml, _) = render_musicxml_met_notemap(&qs, &opts(), Some(&scherm)).unwrap();
        assert_eq!(xml.matches(&format!("<notehead color=\"{}\">", STEM_KLEUREN[1])).count(), 1);
        assert!(!xml.contains(STEM_KLEUREN[0]));
        let scherm = SchermOpties { actieve_stem: vec![2], dim_andere: true };
        let (xml, _) = render_musicxml_met_notemap(&qs, &opts(), Some(&scherm)).unwrap();
        assert!(xml.contains("<notehead color=\"#bbbbbb\">"));
        assert_eq!(xml.matches("<notehead color=").count(), 2);
    }

    #[test]
    fn set_voice_undo_en_verdeling() {
        let mut sc = Score::new(1);
        let l = sc.add_layer("M".into(), Some(false));
        let t = sc.layers[0].takes[0].id;
        let ev = |id: u64, midi: u8, s: u64| LayerEv { id, midi, start_us: s, end_us: s + 500_000, channel: 0, locked: false, voice: 1, hand: None };
        EditCommand::InsertEvents { events: vec![(l, t, ev(1, 60, 0)), (l, t, ev(2, 64, 10_000)), (l, t, ev(3, 67, 0)), (l, t, ev(4, 62, 1_000_000))] }.apply(&mut sc);
        let inv = EditCommand::SetVoice { items: vec![(1, 2), (3, 2)] }.apply(&mut sc).expect("inverse");
        assert_eq!(sc.layers[0].takes[0].events[0].voice, 2);
        inv.apply(&mut sc);
        assert_eq!(sc.layers[0].takes[0].events[0].voice, 1);
        assert!(EditCommand::SetVoice { items: vec![(1, 1)] }.apply(&mut sc).is_none());
        // Stem buiten 1..=4 klemt.
        EditCommand::SetVoice { items: vec![(1, 9)] }.apply(&mut sc);
        assert_eq!(sc.layers[0].takes[0].events[0].voice, 4);
        // Akkoord splitsen: hoogste noot per akkoord stem 1, de rest stem 2.
        let mut items = verdeel_akkoord_in_stemmen(&[(1, 60, 0), (2, 64, 10_000), (3, 67, 0), (4, 62, 1_000_000)], 125_000);
        items.sort();
        assert_eq!(items, vec![(1, 2), (2, 2), (3, 1), (4, 1)]);
        // Een gespreid akkoord rond een celgrens blijft één akkoord (reviewbevinding).
        let mut items = verdeel_akkoord_in_stemmen(&[(1, 60, 120_000), (2, 64, 120_000), (3, 67, 130_000)], 125_000);
        items.sort();
        assert_eq!(items, vec![(1, 2), (2, 2), (3, 1)]);
    }

    #[test]
    fn header_in_identification() {
        let staves = vec![Staff { name: "T".into(), bass_clef: false, notes: vec![], ..Default::default() }];
        let mut o = opts();
        o.title = Some("Trio <3>".into());
        o.composer = Some("J. S. Bach & zonen".into());
        o.subtitle = Some("  BWV 525  ".into());
        let xml = build_musicxml(&staves, &o).expect("xml");
        assert!(xml.contains("<work><work-title>Trio &lt;3&gt;</work-title></work>"));
        assert!(xml.contains("<movement-title>BWV 525</movement-title>"));
        assert!(xml.contains("<identification><creator type=\"composer\">J. S. Bach &amp; zonen</creator></identification>"));
        // Volgorde volgens het schema: work, movement-title, identification, part-list.
        let w = xml.find("<work>").unwrap(); let m = xml.find("<movement-title>").unwrap();
        let i = xml.find("<identification>").unwrap(); let p = xml.find("<part-list>").unwrap();
        assert!(w < m && m < i && i < p);
        // Lege velden blijven weg.
        o.composer = Some("  ".into()); o.subtitle = None;
        let xml = build_musicxml(&staves, &o).expect("xml");
        assert!(!xml.contains("<identification>") && !xml.contains("<movement-title>"));
    }

    #[test]
    fn set_meter_undo_met_noemer() {
        let mut sc = Score::new(1);
        let inv = EditCommand::SetMeter { old: (4, 4), new: (6, 8) }.apply(&mut sc).expect("inverse");
        assert_eq!((sc.beats_per_bar, sc.beat_unit), (6, 8));
        inv.apply(&mut sc);
        assert_eq!((sc.beats_per_bar, sc.beat_unit), (4, 4));
        assert!(EditCommand::SetMeter { old: (4, 4), new: (4, 4) }.apply(&mut sc).is_none());
        // Onbekende noemer klemt op 4 en de maatlengte volgt de score-opties.
        EditCommand::SetMeter { old: (4, 4), new: (3, 5) }.apply(&mut sc);
        assert_eq!(sc.beat_unit, 4);
        let qs = quantize_score(&score_to_staves(&sc), &options_from_score(&sc));
        assert_eq!(qs.measure_len, 12);
    }

    #[test]
    fn rename_en_move_layer_undo() {
        let mut sc = Score::new(1);
        let a = sc.add_layer("A".into(), Some(false));
        let b = sc.add_layer("B".into(), Some(false));
        let c = sc.add_layer("Pedaal".into(), Some(true));
        let inv = EditCommand::SetLayerName { layer: b, old: "B".into(), new: "  Positief ".into() }.apply(&mut sc).expect("inverse");
        assert_eq!(sc.layers[1].name, "Positief");
        inv.apply(&mut sc);
        assert_eq!(sc.layers[1].name, "B");
        assert!(EditCommand::SetLayerName { layer: b, old: "B".into(), new: "   ".into() }.apply(&mut sc).is_none());
        assert!(EditCommand::SetLayerName { layer: 999, old: "x".into(), new: "y".into() }.apply(&mut sc).is_none());
        let inv = EditCommand::MoveLayer { from: 2, to: 0 }.apply(&mut sc).expect("inverse");
        assert_eq!(sc.layers.iter().map(|l| l.id).collect::<Vec<_>>(), vec![c, a, b]);
        inv.apply(&mut sc);
        assert_eq!(sc.layers.iter().map(|l| l.id).collect::<Vec<_>>(), vec![a, b, c]);
        assert!(EditCommand::MoveLayer { from: 1, to: 1 }.apply(&mut sc).is_none());
        assert!(EditCommand::MoveLayer { from: 0, to: 3 }.apply(&mut sc).is_none());
    }

    #[test]
    fn set_header_undo() {
        let mut sc = Score::new(1);
        let oud = (sc.title.clone(), String::new(), String::new());
        let nieuw = ("Toccata".to_string(), "Buxtehude".to_string(), "BuxWV 155".to_string());
        let inv = EditCommand::SetHeader { old: oud.clone(), new: nieuw.clone() }.apply(&mut sc).expect("inverse");
        assert_eq!((sc.title.as_str(), sc.composer.as_str(), sc.subtitle.as_str()), ("Toccata", "Buxtehude", "BuxWV 155"));
        inv.apply(&mut sc);
        assert_eq!(sc.title, oud.0);
        assert!(sc.composer.is_empty());
        assert!(EditCommand::SetHeader { old: oud.clone(), new: oud.clone() }.apply(&mut sc).is_none());
    }

    #[test]
    fn midi_export_maatsoort_noemer() {
        let mut sc = Score::new(1);
        let lid = sc.add_layer("M".into(), Some(false));
        sc.beats_per_bar = 6; sc.beat_unit = 8;
        let tid = sc.layers[0].takes[0].id;
        let ev = LayerEv { id: 1, midi: 60, start_us: 0, end_us: 500_000, channel: 0, locked: false, voice: 1, hand: None };
        EditCommand::InsertEvents { events: vec![(lid, tid, ev)] }.apply(&mut sc);
        let bytes = score_to_smf_bytes(&sc, &|_| None).expect("smf");
        // FF 58 04 nn dd cc bb: nn=6, dd=3 (achtste).
        let pos = bytes.windows(4).position(|w| w == [0xFF, 0x58, 0x04, 0x06]).expect("time signature");
        assert_eq!(bytes[pos + 4], 3);
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
        NotationOptions { bpm: 60.0, beats_per_bar: 3, quantize: 4, key_fifths: -2, title: Some("Fixture 0.7.69".into()), staves: None, tolerance_pct: Some(80), minor: None, beat_unit: None, min_measures: None, composer: None, subtitle: None }
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
        NotationOptions { bpm: 72.5, beats_per_bar: 3, quantize: 4, key_fifths: 3, title: Some("A & B <C>".into()), staves: None, tolerance_pct: Some(60), minor: None, beat_unit: None, min_measures: None, composer: None, subtitle: None }
    }

    const FIXTURE_PAD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/testdata/notatie_0769.musicxml");
    const FIXTURE2_PAD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/testdata/notatie_0770.musicxml");
    const FIXTURE3_PAD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/testdata/notatie_0783.musicxml");

    /// Derde fixture (0.7.83): 6/8 met achtsten over een maatstreep, twee
    /// manualen in een accolade plus pedaal, een lege balk, componist en
    /// ondertitel, minimaal vier maten (de inhoud vult er twee).
    fn fixture3_staves() -> Vec<Staff> {
        vec![
            Staff {
                name: "Hoofdwerk".into(), bass_clef: false,
                notes: vec![
                    NoteEv::anoniem(67, 0.0, 0.5), NoteEv::anoniem(69, 0.5, 1.0), NoteEv::anoniem(71, 1.0, 1.5),
                    NoteEv::anoniem(72, 1.5, 3.5),
                ],
                ..Default::default()
            },
            Staff { name: "Positief".into(), bass_clef: false, notes: vec![], ..Default::default() },
            Staff {
                name: "Pedaal".into(), bass_clef: true, pedal: true,
                notes: vec![NoteEv::anoniem(43, 0.0, 1.5), NoteEv::anoniem(38, 1.5, 3.0)],
                ..Default::default()
            },
        ]
    }

    fn fixture3_opts() -> NotationOptions {
        NotationOptions {
            bpm: 60.0, beats_per_bar: 6, quantize: 2, key_fifths: 1, title: Some("Fixture 0.7.83".into()),
            staves: None, tolerance_pct: Some(80), minor: None,
            beat_unit: Some(8), min_measures: Some(4), composer: Some("Anoniem".into()), subtitle: Some("Pastorale".into()),
        }
    }

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
        let xml3 = build_musicxml(&fixture3_staves(), &fixture3_opts()).expect("xml");
        std::fs::write(FIXTURE3_PAD, xml3.as_bytes()).expect("fixture 3 schrijven");
    }

    #[test]
    fn musicxml_gelijk_aan_fixture3() {
        let verwacht = include_str!("testdata/notatie_0783.musicxml").replace("\r\n", "\n");
        let xml = build_musicxml(&fixture3_staves(), &fixture3_opts()).expect("xml").replace("\r\n", "\n");
        assert!(!verwacht.is_empty(), "fixture 3 ontbreekt: eerst schrijf_fixture_musicxml draaien");
        assert_eq!(xml, verwacht);
        assert!(xml.contains("<beat-type>8</beat-type>"));
        assert_eq!(xml.matches("<measure number=\"4\">").count(), 3);
        assert!(xml.contains("<creator type=\"composer\">Anoniem</creator>"));
        assert!(xml.contains("<movement-title>Pastorale</movement-title>"));
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
        // De lege balk tussen de gevulde is sinds 0.7.83 een part met
        // hele-maatrusten; de pedaalbalk schuift door naar P3.
        assert!(xml.contains("<score-part id=\"P2\"><part-name>Leeg</part-name>"));
        assert!(xml.contains("<score-part id=\"P3\"><part-name>Pedaal</part-name>"));
        assert!(xml.contains("<part-group type=\"start\" number=\"1\">"));
        assert!(xml.contains("A &amp; B &lt;C&gt;"));
    }
}
