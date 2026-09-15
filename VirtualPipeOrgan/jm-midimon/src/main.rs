//! JM-Orgue MIDI-monitor (0.7.44).
//!
//! Klein los consoleprogramma dat ALLE MIDI-ingangen opent, elk binnenkomend
//! bericht ruw toont (tijd, poort, kanaal, soort, waarde, hex) en wegschrijft
//! naar een tekstbestand naast het programma, en de organist stap voor stap
//! door de zweltreden leidt: "beweeg trede 1 van OPEN naar DICHT en druk op
//! Enter", daarna trede 2, trede 3, en tot slot alle drie achter elkaar.
//! Na elke stap volgt een analyse: welke stroom (poort/kanaal/CC) bewoog,
//! bereik, richting, sprongen, tussentijd, overspraak van andere stromen,
//! 14-bits paren (CC n + CC n+32) en NRPN. Aan het eind een conclusie:
//! zijn de treden voor JM-Orgue van elkaar te onderscheiden?
//!
//! Draait los van JM-Orgue: sluit JM-Orgue eerst af, anders houdt die de
//! MIDI-poort bezet (Windows MIDI staat maar een luisteraar per poort toe).
//!
//! Gebruik:  jm-midimon.exe            (begeleide meting, 4 stappen)
//!           jm-midimon.exe --vrij     (alles loggen tot Enter, geen stappen)
//!
//! Bewust alleen ASCII in de consoletekst: de Windows-console staat vaak op
//! codepagina 850/437 en vervormt dan accenten. Het verslag is UTF-8.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::Instant;

/// Een ruw MIDI-bericht zoals de driver het aflevert.
#[derive(Clone, Debug)]
struct Bericht {
    t_ms: f64,
    poort: usize,
    bytes: Vec<u8>,
}

/// Sleutel van een "stroom": een controller op een kanaal op een poort.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Stroom {
    poort: usize,
    kanaal: u8, // 1..=16
    cc: u8,
}

/// Wat een stroom in een meetstap deed.
#[derive(Clone, Debug)]
struct StroomStat {
    waarden: Vec<(f64, u8)>, // (t_ms, waarde)
}

impl StroomStat {
    fn aantal(&self) -> usize { self.waarden.len() }
    fn min(&self) -> u8 { self.waarden.iter().map(|w| w.1).min().unwrap_or(0) }
    fn max(&self) -> u8 { self.waarden.iter().map(|w| w.1).max().unwrap_or(0) }
    fn bereik(&self) -> u8 { self.max().saturating_sub(self.min()) }
    fn eerste(&self) -> u8 { self.waarden.first().map(|w| w.1).unwrap_or(0) }
    fn laatste(&self) -> u8 { self.waarden.last().map(|w| w.1).unwrap_or(0) }
    /// Grootste sprong tussen twee opeenvolgende waarden.
    fn max_sprong(&self) -> u8 {
        self.waarden.windows(2).map(|w| (w[1].1 as i16 - w[0].1 as i16).unsigned_abs() as u8).max().unwrap_or(0)
    }
    /// Gemiddelde tijd tussen twee berichten in ms (0 bij minder dan 2).
    fn gem_tussentijd_ms(&self) -> f64 {
        if self.waarden.len() < 2 { return 0.0; }
        let d = self.waarden.last().unwrap().0 - self.waarden.first().unwrap().0;
        d / (self.waarden.len() - 1) as f64
    }
    /// Aantal richtingsomkeringen: ruis/jitter maakt dit groot.
    fn omkeringen(&self) -> usize {
        let mut n = 0usize;
        let mut vorige_richting: i8 = 0;
        for w in self.waarden.windows(2) {
            let d = w[1].1 as i16 - w[0].1 as i16;
            let r: i8 = if d > 0 { 1 } else if d < 0 { -1 } else { 0 };
            if r != 0 {
                if vorige_richting != 0 && r != vorige_richting { n += 1; }
                vorige_richting = r;
            }
        }
        n
    }
    fn richting(&self) -> &'static str {
        let (a, b) = (self.eerste() as i16, self.laatste() as i16);
        if b - a >= 8 { "oplopend (eerste < laatste)" }
        else if a - b >= 8 { "dalend (eerste > laatste)" }
        else { "geen netto verandering" }
    }
}

/// Analyse van een meetstap.
#[derive(Clone, Debug, Default)]
struct StapAnalyse {
    stromen: BTreeMap<Stroom, StroomStat>,
    /// (poort, kanaal, cc_msb) waarvoor ook cc_msb+32 bewoog.
    paren_14bit: Vec<(usize, u8, u8)>,
    nrpn: Vec<(usize, u8)>, // (poort, kanaal)
    overige: usize,         // niet-CC-berichten
    noten: usize,
}

impl StapAnalyse {
    fn verwerk(berichten: &[Bericht]) -> StapAnalyse {
        let mut a = StapAnalyse::default();
        for b in berichten {
            let Some(&status) = b.bytes.first() else { continue };
            let soort = status & 0xF0;
            let kanaal = (status & 0x0F) + 1;
            match soort {
                0xB0 if b.bytes.len() >= 3 => {
                    let cc = b.bytes[1] & 0x7F;
                    let waarde = b.bytes[2] & 0x7F;
                    a.stromen.entry(Stroom { poort: b.poort, kanaal, cc }).or_insert_with(|| StroomStat { waarden: Vec::new() })
                        .waarden.push((b.t_ms, waarde));
                }
                0x80 | 0x90 => a.noten += 1,
                _ => a.overige += 1,
            }
        }
        // 14-bits paren: CC n (0..=31) samen met CC n+32 op dezelfde poort/kanaal.
        let sleutels: Vec<Stroom> = a.stromen.keys().copied().collect();
        for s in &sleutels {
            if s.cc < 32 && a.stromen.contains_key(&Stroom { poort: s.poort, kanaal: s.kanaal, cc: s.cc + 32 }) {
                a.paren_14bit.push((s.poort, s.kanaal, s.cc));
            }
        }
        // NRPN: CC 99/98 (parameter) plus CC 6 (data entry) op dezelfde poort/kanaal.
        for s in &sleutels {
            if s.cc == 99 || s.cc == 98 {
                if a.stromen.contains_key(&Stroom { poort: s.poort, kanaal: s.kanaal, cc: 6 })
                    && !a.nrpn.contains(&(s.poort, s.kanaal)) {
                    a.nrpn.push((s.poort, s.kanaal));
                }
            }
        }
        a
    }

    /// LSB van een 14-bits paar (CC 32-63 terwijl CC n-32 op dezelfde poort en
    /// hetzelfde kanaal ook bewoog): hoort bij de MSB en is geen eigen trede.
    fn is_lsb(&self, s: &Stroom) -> bool {
        (32..=63).contains(&s.cc)
            && self.stromen.contains_key(&Stroom { poort: s.poort, kanaal: s.kanaal, cc: s.cc - 32 })
    }

    /// De stroom die het meest bewoog (grootste bereik, dan meeste berichten).
    /// Dezelfde ondergrens als het inleren in JM-Orgue (slag >= 8).
    fn dominant(&self) -> Option<(Stroom, &StroomStat)> {
        self.stromen.iter()
            .filter(|(s, st)| !self.is_lsb(s) && st.aantal() >= 3 && st.bereik() >= 8)
            .max_by_key(|(_, st)| (st.bereik(), st.aantal()))
            .map(|(s, st)| (*s, st))
    }

    /// Andere stromen die ook duidelijk bewogen (overspraak of tweede trede).
    fn ook_bewogen(&self, behalve: Option<Stroom>) -> Vec<(Stroom, &StroomStat)> {
        self.stromen.iter()
            .filter(|(s, st)| Some(**s) != behalve && !self.is_lsb(s) && st.aantal() >= 2 && st.bereik() >= 3)
            .map(|(s, st)| (*s, st))
            .collect()
    }
}

fn beschrijf_stroom(s: &Stroom) -> String {
    format!("poort {} kanaal {:>2} CC {:>3}", s.poort, s.kanaal, s.cc)
}

fn cc_naam(cc: u8) -> &'static str {
    match cc {
        1 => "modulatie", 4 => "voetcontroller", 6 => "data entry MSB", 7 => "volume", 11 => "expressie",
        38 => "data entry LSB", 39 => "volume LSB", 43 => "expressie LSB", 64 => "sustain",
        98 => "NRPN LSB", 99 => "NRPN MSB", 100 => "RPN LSB", 101 => "RPN MSB",
        120 => "all sound off", 121 => "reset controllers", 123 => "all notes off", _ => "",
    }
}

/// Een bericht als regel voor scherm en verslag.
fn regel(b: &Bericht) -> String {
    let hex: Vec<String> = b.bytes.iter().map(|x| format!("{:02X}", x)).collect();
    let hex = hex.join(" ");
    let Some(&status) = b.bytes.first() else { return format!("[{:9.3} s] poort {} leeg bericht", b.t_ms / 1000.0, b.poort) };
    let kanaal = (status & 0x0F) + 1;
    let uitleg = match status & 0xF0 {
        0xB0 if b.bytes.len() >= 3 => {
            let naam = cc_naam(b.bytes[1]);
            format!("kanaal {:>2}  CC {:>3} = {:>3}{}", kanaal, b.bytes[1], b.bytes[2],
                if naam.is_empty() { String::new() } else { format!("  ({})", naam) })
        }
        0x90 if b.bytes.len() >= 3 && b.bytes[2] > 0 => format!("kanaal {:>2}  noot aan  {:>3} snelheid {}", kanaal, b.bytes[1], b.bytes[2]),
        0x90 | 0x80 if b.bytes.len() >= 3 => format!("kanaal {:>2}  noot uit  {:>3}", kanaal, b.bytes[1]),
        0xC0 if b.bytes.len() >= 2 => format!("kanaal {:>2}  program change {}", kanaal, b.bytes[1]),
        0xE0 if b.bytes.len() >= 3 => format!("kanaal {:>2}  pitch bend {}", kanaal, ((b.bytes[2] as u16) << 7 | b.bytes[1] as u16) as i32 - 8192),
        0xD0 if b.bytes.len() >= 2 => format!("kanaal {:>2}  channel pressure {}", kanaal, b.bytes[1]),
        0xA0 if b.bytes.len() >= 3 => format!("kanaal {:>2}  poly pressure noot {} = {}", kanaal, b.bytes[1], b.bytes[2]),
        0xF0 => match status {
            0xF0 => format!("sysex ({} bytes)", b.bytes.len()),
            0xF8 => "clock".to_string(),
            0xFE => "active sensing".to_string(),
            _ => format!("systeem {:02X}", status),
        },
        _ => "onbekend".to_string(),
    };
    format!("[{:9.3} s] poort {}  {:<44} {}", b.t_ms / 1000.0, b.poort, uitleg, hex)
}

/// Samenvatting van een meetstap als tekstregels.
fn samenvatting(naam: &str, a: &StapAnalyse) -> Vec<String> {
    let mut uit = Vec::new();
    uit.push(format!("--- Analyse {} ---", naam));
    if a.stromen.is_empty() {
        uit.push("  Geen enkel CC-bericht ontvangen. Beweegt de trede wel, en is dit de juiste MIDI-ingang?".to_string());
        if a.noten > 0 || a.overige > 0 {
            uit.push(format!("  Wel ontvangen: {} nootberichten, {} overige berichten.", a.noten, a.overige));
        }
        return uit;
    }
    let dom = a.dominant();
    match dom {
        Some((s, st)) => {
            uit.push(format!("  Bewogen trede: {} ({})", beschrijf_stroom(&s), if cc_naam(s.cc).is_empty() { "-" } else { cc_naam(s.cc) }));
            uit.push(format!("    {} berichten, bereik {}..{} (slag {}), eerste {} -> laatste {}: {}",
                st.aantal(), st.min(), st.max(), st.bereik(), st.eerste(), st.laatste(), st.richting()));
            uit.push(format!("    grootste sprong {} stappen, gemiddeld {:.1} ms tussen berichten, {} richtingsomkeringen",
                st.max_sprong(), st.gem_tussentijd_ms(), st.omkeringen()));
            if st.bereik() < 40 {
                uit.push("    LET OP: kleine slag (< 40 van 127). Trede niet helemaal bewogen, of de trede levert maar een deel van het bereik.".to_string());
            }
            if st.max_sprong() >= 32 {
                uit.push("    LET OP: grote sprongen; mogelijk grof potmeter, of een 14-bits LSB die als losse waarde binnenkomt.".to_string());
            }
            if st.omkeringen() > st.aantal() / 3 && st.aantal() >= 6 {
                uit.push("    LET OP: veel richtingsomkeringen = ruis/jitter op deze trede.".to_string());
            }
        }
        None => uit.push("  Geen stroom met een duidelijke beweging (slag >= 8 en >= 3 berichten).".to_string()),
    }
    let ook = a.ook_bewogen(dom.map(|d| d.0));
    if !ook.is_empty() {
        uit.push("  Ook bewogen (overspraak, tweede trede, of LSB van een 14-bits paar):".to_string());
        for (s, st) in ook {
            uit.push(format!("    {}: {} berichten, bereik {}..{}", beschrijf_stroom(&s), st.aantal(), st.min(), st.max()));
        }
    }
    let stil: Vec<String> = a.stromen.iter()
        .filter(|(s, st)| Some(**s) != dom.map(|d| d.0) && st.bereik() < 3)
        .map(|(s, st)| format!("{} ({}x, waarde {})", beschrijf_stroom(s), st.aantal(), st.laatste()))
        .collect();
    if !stil.is_empty() {
        uit.push(format!("  Stromen die alleen dezelfde waarde herhaalden: {}", stil.join("; ")));
    }
    for (p, k, cc) in &a.paren_14bit {
        uit.push(format!("  14-BITS PAAR: poort {} kanaal {} CC {} + CC {} bewegen samen. JM-Orgue gebruikt de 7-bits waarde (CC {}); leer de trede op CC {} in.", p, k, cc, cc + 32, cc, cc));
    }
    for (p, k) in &a.nrpn {
        uit.push(format!("  NRPN gezien op poort {} kanaal {} (CC 99/98 + CC 6): deze trede stuurt via NRPN, dat verstaat JM-Orgue nog niet als trede.", p, k));
    }
    if a.noten > 0 { uit.push(format!("  ({} nootberichten genegeerd)", a.noten)); }
    uit
}

/// Eindconclusie over drie treden: zijn ze te onderscheiden?
fn conclusie(stappen: &[(String, StapAnalyse)]) -> Vec<String> {
    let mut uit = Vec::new();
    uit.push("=== Conclusie ===".to_string());
    let mut per_trede: Vec<(String, Option<Stroom>)> = Vec::new();
    for (naam, a) in stappen.iter().take(3) {
        per_trede.push((naam.clone(), a.dominant().map(|d| d.0)));
    }
    for (naam, s) in &per_trede {
        match s {
            Some(s) => uit.push(format!("  {}: {}", naam, beschrijf_stroom(s))),
            None => uit.push(format!("  {}: geen duidelijke beweging gezien", naam)),
        }
    }
    let mut probleem = false;
    for i in 0..per_trede.len() {
        for j in (i + 1)..per_trede.len() {
            if let (Some(a), Some(b)) = (per_trede[i].1, per_trede[j].1) {
                // JM-Orgue voegt alle MIDI-ingangen samen en kijkt alleen naar
                // kanaal + CC; twee treden op verschillende poorten met hetzelfde
                // kanaal en CC zijn dus ook niet te onderscheiden.
                if a.kanaal == b.kanaal && a.cc == b.cc {
                    probleem = true;
                    let poorten = if a.poort == b.poort { String::new() } else { format!(" (via poort {} en {})", a.poort, b.poort) };
                    uit.push(format!("  PROBLEEM: {} en {} sturen hetzelfde kanaal {} en CC {}{}. JM-Orgue kan ze dan NIET onderscheiden: wat je op de ene inleert geldt ook voor de andere. Dit moet in de speeltafel/interface anders ingesteld worden (ander kanaal of ander CC-nummer per trede).",
                        per_trede[i].0, per_trede[j].0, a.kanaal, a.cc, poorten));
                } else if a.cc == b.cc {
                    uit.push(format!("  {} en {} gebruiken hetzelfde CC-nummer ({}) op verschillende kanalen ({} en {}): JM-Orgue onderscheidt ze op kanaal; leer ze allebei in en controleer dat beide koppelingen blijven staan.",
                        per_trede[i].0, per_trede[j].0, a.cc, a.kanaal, b.kanaal));
                }
            }
        }
    }
    if let Some((naam, a)) = stappen.get(3) {
        let bewogen: Vec<String> = a.stromen.iter().filter(|(_, st)| st.bereik() >= 3).map(|(s, _)| beschrijf_stroom(s)).collect();
        uit.push(format!("  {}: {} stromen bewogen: {}", naam, bewogen.len(), if bewogen.is_empty() { "-".to_string() } else { bewogen.join(" | ") }));
        if bewogen.len() < 3 && per_trede.iter().filter(|p| p.1.is_some()).count() == 3 {
            uit.push("  LET OP: bij het achter elkaar bewegen kwamen minder dan drie verschillende stromen binnen.".to_string());
        }
    }
    if !probleem {
        uit.push("  Alle treden die bewogen zijn afzonderlijk herkenbaar. Stuur dit verslag mee met de feedback als JM-Orgue ze toch verwart.".to_string());
    }
    uit
}

struct Verslag {
    bestand: Option<std::fs::File>,
}

impl Verslag {
    fn open() -> (Verslag, String) {
        let stempel = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let naam = format!("midi-monitor-{}.txt", stempel);
        let mut kandidaten: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() { kandidaten.push(dir.join(&naam)); }
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            kandidaten.push(std::path::Path::new(&home).join("Documents").join("JM-Orgue-opnames").join(&naam));
        }
        kandidaten.push(std::env::temp_dir().join(&naam));
        for pad in kandidaten {
            if let Some(dir) = pad.parent() { let _ = std::fs::create_dir_all(dir); }
            if let Ok(f) = std::fs::File::create(&pad) {
                return (Verslag { bestand: Some(f) }, pad.display().to_string());
            }
        }
        (Verslag { bestand: None }, "(geen schrijfbare map gevonden; alleen scherm)".to_string())
    }
    fn schrijf(&mut self, tekst: &str) {
        println!("{}", tekst);
        if let Some(f) = self.bestand.as_mut() {
            let _ = writeln!(f, "{}", tekst);
            let _ = f.flush();
        }
    }
}

/// Lees een Enter van de gebruiker in een aparte thread, zodat de hoofdlus
/// intussen berichten kan blijven tonen.
fn enter_wachter() -> mpsc::Receiver<()> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut lijn = String::new();
        loop {
            lijn.clear();
            match stdin.lock().read_line(&mut lijn) {
                Ok(0) | Err(_) => break, // EOF (bv. omgeleide invoer): niet rondtollen
                Ok(_) => {}
            }
            if tx.send(()).is_err() { break; }
        }
    });
    rx
}

fn main() {
    let vrij = std::env::args().any(|a| a == "--vrij");
    let (mut verslag, pad) = Verslag::open();
    verslag.schrijf(&format!("JM-Orgue MIDI-monitor {}  ({})", env!("CARGO_PKG_VERSION"), chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
    verslag.schrijf(&format!("Verslag wordt geschreven naar: {}", pad));
    verslag.schrijf("Sluit JM-Orgue (en GrandOrgue/Hauptwerk) eerst af: Windows laat maar een programma per MIDI-poort luisteren.");
    verslag.schrijf("");

    let (tx, rx) = mpsc::channel::<Bericht>();
    let start = Instant::now();
    let mut verbindingen = Vec::new();
    let mut poortnamen = Vec::new();
    match midir::MidiInput::new("JM-Orgue MIDI-monitor") {
        Ok(input) => {
            let poorten = input.ports();
            if poorten.is_empty() {
                verslag.schrijf("Geen MIDI-ingangen gevonden. Zit de USB-kabel van de speeltafel erin en is JM-Orgue afgesloten?");
            }
            for (i, p) in poorten.iter().enumerate() {
                let naam = input.port_name(p).unwrap_or_else(|_| format!("poort {}", i + 1));
                let mi = match midir::MidiInput::new(&format!("JM-Orgue MIDI-monitor {}", i + 1)) {
                    Ok(m) => m,
                    Err(e) => { verslag.schrijf(&format!("  poort {}: {} -> niet te openen ({})", i + 1, naam, e)); continue; }
                };
                let txc = tx.clone();
                let idx = i + 1;
                match mi.connect(p, "jm-midimon", move |_stamp, bytes, _| {
                    let _ = txc.send(Bericht { t_ms: start.elapsed().as_secs_f64() * 1000.0, poort: idx, bytes: bytes.to_vec() });
                }, ()) {
                    Ok(c) => { verslag.schrijf(&format!("  poort {}: {} -> geopend", idx, naam)); verbindingen.push(c); poortnamen.push(naam); }
                    Err(e) => verslag.schrijf(&format!("  poort {}: {} -> niet te openen ({}). Houdt een ander programma hem vast?", idx, naam, e)),
                }
            }
        }
        Err(e) => verslag.schrijf(&format!("MIDI kon niet gestart worden: {}", e)),
    }
    verslag.schrijf("");

    let enter = enter_wachter();
    let mut alle_stappen: Vec<(String, StapAnalyse)> = Vec::new();
    let stappen: Vec<(&str, &str)> = if vrij {
        vec![("Vrije meting", "Beweeg wat je wilt. Alles wordt getoond en gelogd. Druk op Enter om te stoppen.")]
    } else {
        vec![
            ("Stap 1 - trede 1", "Beweeg ALLEEN trede 1 rustig helemaal van OPEN naar DICHT (2-3 seconden), laat de andere treden staan, en druk daarna op Enter."),
            ("Stap 2 - trede 2", "Beweeg ALLEEN trede 2 rustig helemaal van OPEN naar DICHT en druk daarna op Enter."),
            ("Stap 3 - trede 3", "Beweeg ALLEEN trede 3 rustig helemaal van OPEN naar DICHT en druk daarna op Enter."),
            ("Stap 4 - alle drie achter elkaar", "Beweeg nu trede 1, dan 2, dan 3 kort achter elkaar van open naar dicht en weer terug (zonder Enter ertussen), en druk daarna op Enter."),
        ]
    };

    if verbindingen.is_empty() {
        verslag.schrijf("Geen enkele MIDI-ingang geopend; er valt niets te meten. Sluit andere MIDI-programma's, controleer de USB-kabel en start dit programma opnieuw.");
        verslag.schrijf("Druk op Enter om af te sluiten.");
        let _ = enter.recv();
        return;
    }

    for (naam, instructie) in &stappen {
        verslag.schrijf(&format!("=== {} ===", naam));
        verslag.schrijf(instructie);
        // Berichten en Enters die nog van de vorige stap in de wachtrij staan opruimen.
        while rx.try_recv().is_ok() {}
        while enter.try_recv().is_ok() {}
        let mut berichten: Vec<Bericht> = Vec::new();
        loop {
            if enter.try_recv().is_ok() { break; }
            match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(b) => { verslag.schrijf(&regel(&b)); berichten.push(b); }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let analyse = StapAnalyse::verwerk(&berichten);
        for r in samenvatting(naam, &analyse) { verslag.schrijf(&r); }
        verslag.schrijf("");
        alle_stappen.push((naam.to_string(), analyse));
    }

    if !vrij {
        for r in conclusie(&alle_stappen) { verslag.schrijf(&r); }
        verslag.schrijf("");
    }
    verslag.schrijf(&format!("Klaar. Het volledige verslag staat in: {}", pad));
    verslag.schrijf("Stuur dit bestand mee via Algemene Instellingen -> Over & feedback -> Feedback sturen (of als bijlage).");
    verslag.schrijf("Druk op Enter om af te sluiten.");
    let _ = enter.recv();
    drop(verbindingen);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(t: f64, poort: usize, kanaal: u8, cc: u8, waarde: u8) -> Bericht {
        Bericht { t_ms: t, poort, bytes: vec![0xB0 | (kanaal - 1), cc, waarde] }
    }

    #[test]
    fn dominante_stroom_en_richting() {
        let mut b = Vec::new();
        for (i, v) in (0..=127u8).rev().step_by(8).enumerate() {
            b.push(cc(i as f64 * 20.0, 1, 1, 11, v));
        }
        b.push(cc(500.0, 1, 2, 11, 64)); // ruis op een andere trede: 1 bericht, geen beweging
        let a = StapAnalyse::verwerk(&b);
        let (s, st) = a.dominant().expect("dominante stroom");
        assert_eq!(s, Stroom { poort: 1, kanaal: 1, cc: 11 });
        assert_eq!(st.bereik(), 127 - 7);
        assert!(st.richting().starts_with("dalend"));
        assert!(a.ook_bewogen(Some(s)).is_empty());
        assert_eq!(st.omkeringen(), 0);
    }

    #[test]
    fn paar_14bit_en_nrpn_worden_herkend() {
        let mut b = Vec::new();
        for i in 0..10u8 {
            b.push(cc(i as f64 * 10.0, 1, 3, 7, i * 12));
            b.push(cc(i as f64 * 10.0 + 1.0, 1, 3, 39, ((i as u16 * 37) % 128) as u8));
        }
        b.push(cc(200.0, 2, 1, 99, 1));
        b.push(cc(201.0, 2, 1, 98, 2));
        for i in 0..5u8 { b.push(cc(210.0 + i as f64, 2, 1, 6, i * 20)); }
        let a = StapAnalyse::verwerk(&b);
        assert_eq!(a.paren_14bit, vec![(1, 3, 7)]);
        assert_eq!(a.nrpn, vec![(2, 1)]);
        assert_eq!(a.dominant().map(|d| d.0), Some(Stroom { poort: 1, kanaal: 3, cc: 7 }), "MSB is de trede, niet de LSB");
        assert!(a.ook_bewogen(a.dominant().map(|d| d.0)).iter().all(|(s, _)| s.cc != 39), "LSB niet als tweede trede");
    }

    #[test]
    fn conclusie_meldt_identieke_treden() {
        let sweep = |kanaal: u8| -> StapAnalyse {
            let b: Vec<Bericht> = (0..12u8).map(|i| cc(i as f64 * 15.0, 1, kanaal, 11, 127 - i * 10)).collect();
            StapAnalyse::verwerk(&b)
        };
        let stappen = vec![
            ("Stap 1 - trede 1".to_string(), sweep(1)),
            ("Stap 2 - trede 2".to_string(), sweep(1)),
            ("Stap 3 - trede 3".to_string(), sweep(2)),
        ];
        let c = conclusie(&stappen).join("\n");
        assert!(c.contains("PROBLEEM: Stap 1 - trede 1 en Stap 2 - trede 2"), "{}", c);
        // Zelfde kanaal+CC op een andere poort is voor JM-Orgue óók niet te onderscheiden.
        let b: Vec<Bericht> = (0..12u8).map(|i| Bericht { t_ms: i as f64 * 15.0, poort: 2, bytes: vec![0xB0, 11, 127 - i * 10] }).collect();
        let stappen2 = vec![("Stap 1 - trede 1".to_string(), sweep(1)), ("Stap 2 - trede 2".to_string(), StapAnalyse::verwerk(&b))];
        let c2 = conclusie(&stappen2).join("\n");
        assert!(c2.contains("PROBLEEM") && c2.contains("via poort 1 en 2"), "{}", c2);
        assert!(c.contains("hetzelfde CC-nummer (11) op verschillende kanalen"), "{}", c);
    }

    #[test]
    fn regel_toont_cc_met_naam_en_hex() {
        let r = regel(&cc(1234.5, 1, 2, 11, 100));
        assert!(r.contains("kanaal  2  CC  11 = 100  (expressie)"), "{}", r);
        assert!(r.ends_with("B1 0B 64"), "{}", r);
    }

    #[test]
    fn jitter_wordt_geteld() {
        let b: Vec<Bericht> = [60u8, 61, 60, 61, 60, 61, 60].iter().enumerate()
            .map(|(i, v)| cc(i as f64 * 5.0, 1, 1, 4, *v)).collect();
        let a = StapAnalyse::verwerk(&b);
        let st = a.stromen.get(&Stroom { poort: 1, kanaal: 1, cc: 4 }).unwrap();
        assert_eq!(st.omkeringen(), 5);
        assert!(a.dominant().is_none(), "bereik 1 telt niet als beweging");
    }
}
