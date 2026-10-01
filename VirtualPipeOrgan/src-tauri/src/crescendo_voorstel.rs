//! Voorstel voor de trappen van het generaal crescendo (walze), 0.7.67.
//!
//! Een walze loopt van zacht naar vol: eerst de zachtste 8' per manuaal met
//! een zachte 16' in het pedaal, dan zachte grondstemmen, 4' en de principalen,
//! 2' en aliquoten, mengwerken, en als laatste de tongwerken. Zo staan de
//! vaste combinaties van de Maerz-orgel in de Augsburger Dom (1903), waarvan
//! de walze dezelfde ladder volgt, en zo doen GrandOrgue, Allen en Hauptwerk-
//! sets het ook. Wat NOOIT in een walze hoort: tremulanten, zwevingen
//! (céleste, unda maris), Vox humana en effectregisters (Zimbelstern,
//! klokken).
//!
//! Deze module is puur: dezelfde dispositie en dezelfde opties geven altijd
//! hetzelfde voorstel, los van hoe de gebruiker de registers sorteert.
//!
//! Opbouw:
//! 1. Elk register krijgt een groep (1 pp-kern … 8 tutti), een subvolgorde
//!    binnen de groep, een voetmaat en een geschatte luidheid `p`.
//! 2. Per divisie lopen de groepen gelijk op (sleutel k = groep + positie
//!    binnen de groep), zodat manualen en pedaal samen groeien.
//! 3. Koppels krijgen een vaste plek: II/I en de zwelwerk-pedaalkoppel in de
//!    pp-kern, I/P bij de principalen.
//! 4. De trappen verdelen de reeks half op aantal registers en half op
//!    geschatte luidheid in dB: geen dode trappen bij zachte registers en
//!    geen sprongen bij de luide.

use vpo_audio::{familie_van_naam, PijpFamilie};
use vpo_sampler::Koren;

#[derive(Debug, Clone)]
pub struct VoorstelRegister {
    pub id: String,
    pub naam: String,
    /// Voetmaat als tekst ("8'", "2 2/3'", of leeg).
    pub pitch: String,
    pub koren: Option<Koren>,
    pub mengwerk: bool,
    pub tongwerk: bool,
    pub percussief: bool,
}

#[derive(Debug, Clone)]
pub struct VoorstelDivisie {
    pub naam: String,
    pub pedaal: bool,
    pub zwelkast: bool,
    pub registers: Vec<VoorstelRegister>,
}

#[derive(Debug, Clone)]
pub struct VoorstelKoppel {
    pub id: String,
    /// Het klavier waarop je speelt.
    pub bron: String,
    /// Het klavier dat meeklinkt.
    pub doel: String,
    /// "unison", "super", "sub", "ta", "melody", "bass".
    pub soort: String,
    /// Een koppel met een bijzondere werking (Unison Off, melodie- of
    /// baskoppel uit de sampleset): nooit in de walze.
    pub speciaal: bool,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct VoorstelOpties {
    /// Koppels meenemen (II/I, pedaalkoppels).
    pub koppels: bool,
    /// Octaafkoppels (super) in de laatste trap.
    pub octaafkoppels: bool,
    /// Kleurregisters (Sesquialter, Cornet, losse terts) laat in de walze.
    pub kleur: bool,
    /// 32'-tongwerken en chamades in de laatste trap.
    pub tutti_extra: bool,
}

impl Default for VoorstelOpties {
    fn default() -> Self {
        VoorstelOpties { koppels: true, octaafkoppels: false, kleur: true, tutti_extra: true }
    }
}

fn bevat(n: &str, woorden: &[&str]) -> bool {
    woorden.iter().any(|w| n.contains(w))
}

/// Voetmaat uit "16'", "2 2/3'", "4/5'", "1/2'", "3.2'". None = onbekend.
fn voet_uit_tekst(tekst: &str) -> Option<f32> {
    let t: String = tekst.chars().filter(|c| !matches!(c, '\'' | '\u{2019}' | '\u{2032}')).collect();
    let mut som = 0.0f32;
    let mut gevonden = false;
    for deel in t.split_whitespace() {
        if let Some((a, b)) = deel.split_once('/') {
            let (a, b) = (a.parse::<f32>().ok()?, b.parse::<f32>().ok()?);
            if b <= 0.0 { return None; }
            som += a / b;
            gevonden = true;
        } else if let Ok(v) = deel.replace(',', ".").parse::<f32>() {
            som += v;
            gevonden = true;
        } else {
            return None;
        }
    }
    if gevonden && som > 0.0 { Some(som) } else { None }
}

/// Voetmaat uit de naam, voor sets zonder voetmaten (alles 8'): eerst een
/// getal achter de naam ("Octave 4", "Quinte 2 2/3"), anders de gangbare
/// betekenis van de naam.
fn voet_uit_naam(naam: &str, pedaal: bool) -> f32 {
    let woorden: Vec<&str> = naam.split_whitespace().collect();
    for aantal in [2usize, 1] {
        if woorden.len() > aantal {
            if let Some(v) = voet_uit_tekst(&woorden[woorden.len() - aantal..].join(" ")) {
                if (0.2..=64.0).contains(&v) { return v; }
            }
        }
    }
    let n = naam.to_lowercase();
    if bevat(&n, &["superoctav", "superoktav", "superoctaaf", "doublette", "woudfluit", "waldfl", "flageolet", "piccolo"]) { return 2.0; }
    if bevat(&n, &["terts", "tierce", "terz"]) { return 1.6; }
    if bevat(&n, &["quintade", "quintatön", "quintaton"]) { return if pedaal { 16.0 } else { 8.0 }; }
    if bevat(&n, &["quint", "nasard", "nazard"]) { return if pedaal { 10.667 } else { 2.667 }; }
    if bevat(&n, &["octaaf", "octave", "oktav", "ottava"]) { return if pedaal { 8.0 } else { 4.0 }; }
    if bevat(&n, &["subbas", "subbaß", "untersatz", "contrabas", "kontrabass", "violon"]) { return 16.0; }
    if bevat(&n, &["prestant", "praestant", "principa", "prinzipal"]) { return if pedaal { 16.0 } else { 8.0 }; }
    8.0
}

/// Twee helften van één register ("Mixtuur (Bas)" + "Mixtuur (Disc)",
/// "Cornet D") horen samen: zelfde plek in de walze.
fn zonder_helft(naam: &str) -> String {
    let mut woorden: Vec<String> = naam.to_lowercase().split_whitespace().map(|w| w.to_string()).collect();
    while woorden.len() > 1 {
        let laatste = woorden.last().unwrap().trim_matches(|c| c == '(' || c == ')' || c == '.').to_string();
        if matches!(laatste.as_str(), "bas" | "bass" | "basse" | "b" | "disc" | "disk" | "discant" | "diskant"
            | "desc" | "dessus" | "treble" | "d") {
            woorden.pop();
        } else {
            break;
        }
    }
    woorden.join(" ")
}

#[derive(Debug, Clone)]
struct Item {
    ids: Vec<String>,
    divisie: usize,
    groep: u8,
    sub: u8,
    voet: f32,
    p: f32,
    bron: usize,
    k: f32,
}

fn is_tutti(groep: u8) -> bool { groep >= 8 }

/// Groep, subvolgorde, voetmaat en geschatte luidheid van één register, of
/// None als het niet in de walze hoort.
fn classificeer(r: &VoorstelRegister, pedaal: bool, hoofd: bool, voet_onbekend: bool, o: &VoorstelOpties)
    -> Option<(u8, u8, f32, f32)>
{
    let n = r.naam.to_lowercase().replace('ß', "ss");
    // 1. Nooit: effecten, percussief, tremulant, zwevingen, solostemmen.
    //    Volgorde is belangrijk: "Zimbelstern" is geen Cymbel en "Carillon"
    //    is alleen een effect als het percussief is.
    if r.percussief { return None; }
    if bevat(&n, &["zimbelstern", "cymbelstern", "cymbelster", "cimbelster", "glocken", "glockenspiel",
        "klokken", "chimes", "campan", "nachtigall", "vogel", "rossignol", "usignolo", "pauke", "timpani",
        "tamboer", "tambour", "trommel", "harp", "celesta", "triangel", "kuckuck", "cuckoo"]) { return None; }
    if bevat(&n, &["tremulant", "tremolo"]) { return None; }
    if bevat(&n, &["celeste", "céleste", "coelestis", "celestis", "unda maris", "vox angelica",
        "schwebung", "zweving", "bifara", "piffaro"]) { return None; }
    if bevat(&n, &["vox humana", "voix humaine", "vox umana"]) { return None; }

    let fam = if r.mengwerk { PijpFamilie::Mixtuur } else { familie_van_naam(&r.naam, r.tongwerk) };
    let voet = if voet_onbekend { voet_uit_naam(&r.naam, pedaal) } else {
        voet_uit_tekst(&r.pitch).unwrap_or_else(|| voet_uit_naam(&r.naam, pedaal))
    };

    // 2. Chamades en hoogdruktongen: alleen in het tutti.
    if bevat(&n, &["chamade", "horizontal", "tuba mirabilis", "trompeta real", "batalla", "trompeta magna"]) {
        return if o.tutti_extra { Some((8, 1, voet, 10.0)) } else { None };
    }

    // 3. Tongwerken.
    if fam == PijpFamilie::Tongwerk {
        if voet >= 32.0 {
            return if o.tutti_extra { Some((8, 0, voet, 9.0)) } else { None };
        }
        let zacht = bevat(&n, &["hobo", "oboe", "hautbois", "kromhoorn", "krummhorn", "cromorne", "dulciaan",
            "dulzian", "dulcian", "schalmei", "schalmey", "chalumeau", "regaal", "regal", "klarinet",
            "clarinet", "cor anglais", "englischhorn", "engels hoorn", "musette", "sordun", "ranket", "rankett"]);
        if pedaal {
            let (sub, p) = if voet >= 16.0 { (1, 7.0) } else { (2, 4.0) };
            return Some((7, if zacht { 0 } else { sub }, voet, if zacht { 3.5 } else { p }));
        }
        return Some(if voet >= 16.0 { (7, 3, voet, 6.0) }
            else if zacht { (7, 0, voet, 2.5) }
            else if voet >= 6.0 { (7, 1, voet, 5.0) }
            else { (7, 2, voet, 3.0) });
    }

    // 4. Mengwerken.
    if fam == PijpFamilie::Mixtuur {
        let koren = r.koren.map(|k| k.max).unwrap_or(4) as f32;
        if bevat(&n, &["sesquialter", "sexquialter", "cornet", "kornet", "terzian", "tertiaan", "carillon", "carillion"]) {
            return if o.kleur { Some((6, 3, voet, 1.5)) } else { None };
        }
        let sub = if bevat(&n, &["cymbel", "zimbel", "cimbel", "cymbal"]) { 2 }
            else if bevat(&n, &["scherp", "scharf", "acuta", "sharp"]) { 1 }
            else { 0 };
        let p = if pedaal { 2.0 } else { 2.5 + 0.3 * (koren - 3.0).max(0.0) };
        return Some((6, sub, voet, p));
    }

    // 5. Labialen in het pedaal.
    if pedaal {
        let gedekt = fam == PijpFamilie::Gedekt;
        let strijker = fam == PijpFamilie::Strijker;
        return Some(if voet >= 32.0 { (6, 4, voet, 2.0) }
            else if voet >= 12.0 {
                if gedekt { (2, 0, voet, 1.5) } else if strijker { (3, 0, voet, 1.5) } else { (4, 0, voet, 3.0) }
            } else if voet >= 6.0 {
                if gedekt || fam == PijpFamilie::Fluit { (2, 1, voet, 1.2) } else if strijker { (3, 1, voet, 1.2) } else { (4, 1, voet, 1.2) }
            } else { (5, 0, voet, 0.8) });
    }

    // 6. Labialen op een manuaal.
    if bevat(&n, &["terts", "tierce", "terz", "septiem", "septime", "none", "nona"]) && voet < 2.0 {
        return if o.kleur { Some((6, 4, voet, 1.5)) } else { None };
    }
    let principaal = fam == PijpFamilie::Principaal;
    Some(if voet >= 12.0 { (5, 3, voet, 1.5) }
        else if voet >= 6.0 {
            match fam {
                PijpFamilie::Strijker => (2, 0, voet, 1.0),
                PijpFamilie::Gedekt => (2, 0, voet, 1.2),
                PijpFamilie::Fluit => (2, 0, voet, 1.5),
                _ => (if hoofd { 4 } else { 3 }, 0, voet, 2.5),
            }
        } else if voet >= 3.5 {
            if principaal { (4, 1, voet, 1.5) } else { (3, 1, voet, 0.8) }
        } else { (5, 0, voet, 0.8) })
}

/// De invoer voor [`stel_voor`] uit het geladen orgel, in de volgorde van de
/// sampleset (niet die van de gebruiker), met de huidige koppellijst.
pub fn uit_orgel(info: &crate::commands::OrganInfoDto) -> (Vec<VoorstelDivisie>, Vec<VoorstelKoppel>) {
    let divisies = info.divisions.iter().map(|d| VoorstelDivisie {
        naam: d.name.clone(),
        pedaal: d.is_pedal,
        zwelkast: d.has_swell,
        registers: d.stops.iter().map(|s| VoorstelRegister {
            id: s.id.clone(),
            naam: s.name.clone(),
            pitch: s.pitch.clone(),
            koren: s.koren,
            mengwerk: s.mengwerk,
            tongwerk: s.is_reed,
            percussief: s.percussief,
        }).collect(),
    }).collect();
    let koppels = info.couplers.iter().flatten().map(|c| VoorstelKoppel {
        id: c.id.clone(),
        bron: c.source_division.clone(),
        doel: c.destination_division.clone(),
        soort: c.coupler_type.clone(),
        speciaal: c.speciaal,
    }).collect();
    (divisies, koppels)
}

/// Het voorstel: `n` trappen, elke trap de lijst register- en koppel-id's die
/// dan aan staan. Cumulatief: trap s+1 bevat alles van trap s.
pub fn stel_voor(divisies: &[VoorstelDivisie], koppels: &[VoorstelKoppel], n: usize, o: &VoorstelOpties)
    -> Vec<Vec<String>>
{
    let n = n.max(1);
    // Het hoofdmanuaal: het manuaal met de meeste registers, bij gelijkstand
    // het eerste.
    let manualen: Vec<usize> = (0..divisies.len()).filter(|&i| !divisies[i].pedaal).collect();
    // Het hoofdmanuaal: op naam, anders het grootste manuaal zonder
    // zwelkast, anders het grootste. Niet zomaar het grootste: in een
    // romantisch orgel is het zwelwerk vaak groter dan het hoofdwerk, en in
    // een Nederlands orgel is manuaal I vaak het rugwerk.
    let grootste = |kandidaten: &mut dyn Iterator<Item = usize>| kandidaten.max_by(|&a, &b| {
        divisies[a].registers.len().cmp(&divisies[b].registers.len()).then(b.cmp(&a))
    });
    let op_naam = manualen.iter().copied().find(|&m| {
        let n = divisies[m].naam.to_lowercase();
        bevat(&n, &["hoofdwerk", "hauptwerk", "great", "grand orgue", "grand-orgue", "grand'orgue",
            "grand organo", "grand'organo", "grande organo", "organo maggiore", "órgano mayor",
            "organo mayor", "manuał główny", "manual główny", "hoofdmanuaal", "hauptmanual"])
            || n == "hw" || n == "gt" || n == "go" || n.starts_with("hw ") || n.starts_with("gt ")
    });
    let hoofd = op_naam
        .or_else(|| grootste(&mut manualen.iter().copied().filter(|&m| !divisies[m].zwelkast)))
        .or_else(|| grootste(&mut manualen.iter().copied()));
    let pedaal = divisies.iter().position(|d| d.pedaal);

    // Sets zonder voetmaten zetten alles op 8': dan uit de naam halen.
    let alle: Vec<&VoorstelRegister> = divisies.iter().flat_map(|d| d.registers.iter()).collect();
    let met_voet: Vec<f32> = alle.iter().filter(|r| !r.mengwerk).filter_map(|r| voet_uit_tekst(&r.pitch)).collect();
    let voet_onbekend = met_voet.len() >= 4 && met_voet.iter().all(|v| (*v - 8.0).abs() < 0.01);

    // Items per divisie, met helften samengevoegd.
    let mut items: Vec<Item> = Vec::new();
    let mut bron = 0usize;
    for (di, d) in divisies.iter().enumerate() {
        let is_hoofd = Some(di) == hoofd;
        for r in &d.registers {
            bron += 1;
            let Some((groep, sub, voet, p)) = classificeer(r, d.pedaal, is_hoofd, voet_onbekend, o) else { continue };
            let sleutel = zonder_helft(&r.naam);
            if let Some(bestaand) = items.iter_mut().find(|it| it.divisie == di && it.groep == groep
                && (it.voet - voet).abs() < 0.01
                && it.ids.first().and_then(|id| d.registers.iter().find(|x| &x.id == id))
                    .map_or(false, |x| zonder_helft(&x.naam) == sleutel)) {
                bestaand.ids.push(r.id.clone());
                continue;
            }
            items.push(Item { ids: vec![r.id.clone()], divisie: di, groep, sub, voet, p, bron, k: 0.0 });
        }
    }

    // pp-kern: per manuaal het zachtste 8'-labiaal, in het pedaal het
    // zachtste 16' (anders 8').
    for (di, d) in divisies.iter().enumerate() {
        // Zachtste (laagste p, dan bronvolgorde) labiaal binnen een voetbereik.
        let kies = |lo: f32, hi: f32| -> Option<usize> {
            let mut beste: Option<usize> = None;
            for (i, it) in items.iter().enumerate() {
                if it.divisie != di || it.groep > 4 || it.voet < lo || it.voet > hi { continue; }
                beste = match beste {
                    Some(b) if (items[b].p, items[b].bron) <= (it.p, it.bron) => Some(b),
                    _ => Some(i),
                };
            }
            beste
        };
        let gekozen = if d.pedaal { kies(12.0, 31.0).or_else(|| kies(6.0, 12.0)) } else { kies(6.0, 12.0) };
        if let Some(i) = gekozen {
            items[i].groep = 1;
        }
    }

    // Voortgang: binnen een groep eerst op subvolgorde, over ALLE divisies
    // heen (alle zachte tongen vóór de trompetten, ook als ze op een ander
    // klavier staan), en binnen (divisie, groep, sub) gelijk oplopend:
    // grote voet eerst, zachtst eerst, dan bronvolgorde. Zo groeien manualen
    // en pedaal samen.
    const SUBS: f32 = 5.0;
    items.sort_by(|a, b| {
        (a.divisie, a.groep, a.sub).cmp(&(b.divisie, b.groep, b.sub))
            .then(b.voet.partial_cmp(&a.voet).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.p.partial_cmp(&b.p).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.bron.cmp(&b.bron))
    });
    let mut i = 0;
    while i < items.len() {
        let (d, g, sb) = (items[i].divisie, items[i].groep, items[i].sub);
        let mut j = i;
        while j < items.len() && items[j].divisie == d && items[j].groep == g && items[j].sub == sb { j += 1; }
        let aantal = (j - i) as f32;
        for (pos, it) in items[i..j].iter_mut().enumerate() {
            it.k = g as f32 + (sb as f32 + (pos as f32 + 0.5) / aantal) / SUBS;
        }
        i = j;
    }

    // Koppels: alleen naar het hoofdmanuaal en naar het pedaal, nooit twee
    // wegen naar hetzelfde klavier.
    if o.koppels {
        let rol = |naam: &str| divisies.iter().position(|d| d.naam == naam);
        let pedaal_registers = pedaal.map_or(0, |p| items.iter().filter(|it| it.divisie == p).count());
        // Het zachtste nevenmanuaal: met zwelkast, anders het hoogste nummer.
        let neven: Vec<usize> = manualen.iter().copied().filter(|&m| Some(m) != hoofd).collect();
        let zachtste_neven = neven.iter().copied().rev().find(|&m| divisies[m].zwelkast)
            .or_else(|| neven.last().copied());
        let mut gezien: Vec<(usize, usize, String)> = Vec::new();
        // Echte koppels uit de sampleset gaan voor de zelf gemaakte.
        let mut volgorde: Vec<&VoorstelKoppel> = koppels.iter().collect();
        volgorde.sort_by_key(|c| if c.id.starts_with("real_coupler_") { 0 } else { 1 });
        for c in volgorde {
            if c.speciaal { continue; }
            let (Some(b), Some(dl)) = (rol(&c.bron), rol(&c.doel)) else { continue };
            let k = match c.soort.as_str() {
                "unison" if b == dl => None,
                "unison" if Some(b) == hoofd && !divisies[dl].pedaal => Some(1.0),
                "unison" if Some(b) == pedaal && Some(dl) == zachtste_neven => Some(1.0),
                "unison" if Some(b) == pedaal && Some(dl) == hoofd => Some(if pedaal_registers < 3 { 2.0 } else { 4.0 }),
                "unison" if Some(b) == pedaal => Some(3.0),
                "super" if o.octaafkoppels && Some(b) == hoofd && !divisies[dl].pedaal => Some(8.5),
                _ => None,
            };
            let Some(k): Option<f32> = k else { continue };
            let sleutel = (b, dl, c.soort.clone());
            if gezien.contains(&sleutel) { continue; }
            gezien.push(sleutel);
            let groep = k.floor() as u8;
            items.push(Item { ids: vec![c.id.clone()], divisie: b, groep, sub: 0, voet: 0.0, p: 0.0, bron: 0, k });
        }
    }

    // De reeks: op k, dan hoofdmanuaal, nevenmanualen, pedaal, dan bron.
    let prioriteit = |d: usize| -> usize {
        if Some(d) == hoofd { 0 } else if divisies[d].pedaal { 2 } else { 1 }
    };
    items.sort_by(|a, b| {
        a.k.partial_cmp(&b.k).unwrap_or(std::cmp::Ordering::Equal)
            .then(prioriteit(a.divisie).cmp(&prioriteit(b.divisie)))
            .then(a.divisie.cmp(&b.divisie))
            .then(a.bron.cmp(&b.bron))
    });
    if items.is_empty() {
        return vec![Vec::new(); n];
    }

    // Verdeling over de trappen.
    let m = items.len();
    let i1 = items.iter().rposition(|it| it.groep <= 1).unwrap_or(0);
    let eerste_tutti = items.iter().position(|it| is_tutti(it.groep)).unwrap_or(m);
    let mut cum = 0.0f32;
    let db: Vec<f32> = items.iter().map(|it| { cum += it.p; 10.0 * cum.max(0.05).log10() }).collect();
    let alpha = 0.5f32;
    let x: Vec<f32> = (0..m).map(|i| {
        if i <= i1 || m - 1 <= i1 { return 0.0; }
        let telling = (i - i1) as f32 / (m - 1 - i1) as f32;
        let spreiding = db[m - 1] - db[i1];
        let luid = if spreiding > 1e-3 { (db[i] - db[i1]) / spreiding } else { telling };
        alpha * telling + (1.0 - alpha) * luid
    }).collect();

    let mut trappen: Vec<Vec<String>> = Vec::with_capacity(n);
    for s in 1..=n {
        let tot = if s == n {
            m - 1
        } else if s == 1 {
            i1
        } else {
            let drempel = (s - 1) as f32 / (n - 1) as f32;
            let mut laatste = i1;
            for i in (i1 + 1)..m {
                if x[i] <= drempel + 1e-6 { laatste = i; } else { break; }
            }
            // Het tutti (32'-tongen, chamades, octaafkoppels) alleen in de laatste trap.
            laatste.min(eerste_tutti.saturating_sub(1)).max(i1)
        };
        let trap: Vec<String> = items[..=tot.min(m - 1)].iter().flat_map(|it| it.ids.iter().cloned()).collect();
        trappen.push(trap);
    }
    // Monotoon houden (een trap bevat nooit minder dan de vorige).
    for s in 1..trappen.len() {
        if trappen[s].len() < trappen[s - 1].len() {
            trappen[s] = trappen[s - 1].clone();
        }
    }
    trappen
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn reg(id: &str, naam: &str, pitch: &str) -> VoorstelRegister {
        VoorstelRegister { id: id.into(), naam: naam.into(), pitch: pitch.into(), koren: None,
            mengwerk: false, tongwerk: false, percussief: false }
    }
    fn meng(id: &str, naam: &str, koren: u8) -> VoorstelRegister {
        VoorstelRegister { koren: Some(Koren { min: koren, max: koren }), mengwerk: true, ..reg(id, naam, "") }
    }
    fn tong(id: &str, naam: &str, pitch: &str) -> VoorstelRegister {
        VoorstelRegister { tongwerk: true, ..reg(id, naam, pitch) }
    }
    fn koppel(id: &str, bron: &str, doel: &str, soort: &str) -> VoorstelKoppel {
        VoorstelKoppel { id: id.into(), bron: bron.into(), doel: doel.into(), soort: soort.into(), speciaal: false }
    }

    fn nederlands() -> (Vec<VoorstelDivisie>, Vec<VoorstelKoppel>) {
        let hw = VoorstelDivisie { naam: "Hoofdwerk".into(), pedaal: false, zwelkast: false, registers: vec![
            reg("p8", "Prestant", "8'"), reg("b16", "Bourdon", "16'"), reg("h8", "Holpijp", "8'"),
            reg("o4", "Octaaf", "4'"), reg("f4", "Fluit", "4'"), reg("q", "Quint", "2 2/3'"),
            reg("o2", "Octaaf", "2'"), meng("mix", "Mixtuur", 4), meng("cor", "Cornet", 5),
            tong("tr", "Trompet", "8'"),
        ]};
        let bw = VoorstelDivisie { naam: "Bovenwerk".into(), pedaal: false, zwelkast: true, registers: vec![
            reg("g8", "Gedekt", "8'"), reg("v8", "Viola", "8'"), reg("vc", "Voix céleste", "8'"),
            reg("bf4", "Fluit", "4'"), reg("wf2", "Woudfluit", "2'"), meng("sq", "Sesquialter", 2),
            tong("hobo", "Hobo", "8'"),
            VoorstelRegister { percussief: true, ..reg("zs", "Zimbelstern", "") },
            tong("vh", "Vox humana", "8'"),
        ]};
        let ped = VoorstelDivisie { naam: "Pedaal".into(), pedaal: true, zwelkast: false, registers: vec![
            reg("sb16", "Subbas", "16'"), reg("p16", "Prestant", "16'"), reg("ob8", "Octaafbas", "8'"),
            tong("bz", "Bazuin", "16'"), tong("cb32", "Contrabombarde", "32'"),
        ]};
        let koppels = vec![
            koppel("real_coupler_1", "Hoofdwerk", "Bovenwerk", "unison"),
            koppel("real_coupler_2", "Pedaal", "Hoofdwerk", "unison"),
            koppel("real_coupler_3", "Pedaal", "Bovenwerk", "unison"),
            // Zelf gemaakte dubbelen van de echte koppels: niet nog een keer.
            koppel("coupler_bovenwerk_hoofdwerk", "Hoofdwerk", "Bovenwerk", "unison"),
            // De andere kant op: nooit.
            koppel("coupler_hoofdwerk_bovenwerk", "Bovenwerk", "Hoofdwerk", "unison"),
            koppel("coupler_super_bw", "Hoofdwerk", "Bovenwerk", "super"),
            koppel("coupler_sub_bw", "Hoofdwerk", "Bovenwerk", "sub"),
            koppel("ta_hoofdwerk", "Hoofdwerk", "Hoofdwerk", "ta"),
            koppel("melody_hoofdwerk", "Hoofdwerk", "Hoofdwerk", "melody"),
            koppel("bass_hoofdwerk_pedaal", "Hoofdwerk", "Pedaal", "bass"),
            VoorstelKoppel { speciaal: true, ..koppel("real_coupler_9", "Hoofdwerk", "Hoofdwerk", "unison") },
        ];
        (vec![ped, hw, bw], koppels)
    }

    fn trap_van(trappen: &[Vec<String>], id: &str) -> Option<usize> {
        trappen.iter().position(|t| t.iter().any(|x| x == id))
    }

    #[test]
    fn vorm_monotoon_en_deterministisch() {
        let (d, k) = nederlands();
        for n in [1usize, 8, 15, 20, 32] {
            let t = stel_voor(&d, &k, n, &VoorstelOpties::default());
            assert_eq!(t.len(), n);
            for s in 1..t.len() {
                let vorige: HashSet<_> = t[s - 1].iter().collect();
                let deze: HashSet<_> = t[s].iter().collect();
                assert!(vorige.is_subset(&deze), "trap {} bevat niet alles van trap {}", s + 1, s);
            }
            assert_eq!(t, stel_voor(&d, &k, n, &VoorstelOpties::default()));
        }
    }

    #[test]
    fn nooit_in_de_walze() {
        let (d, k) = nederlands();
        let t = stel_voor(&d, &k, 20, &VoorstelOpties::default());
        let laatste = t.last().unwrap();
        for id in ["vc", "zs", "vh", "coupler_hoofdwerk_bovenwerk", "coupler_sub_bw", "coupler_super_bw",
            "ta_hoofdwerk", "melody_hoofdwerk", "bass_hoofdwerk_pedaal", "real_coupler_9", "coupler_bovenwerk_hoofdwerk"] {
            assert!(!laatste.contains(&id.to_string()), "{id} hoort er niet in");
        }
        for id in ["p8", "b16", "h8", "o4", "f4", "q", "o2", "mix", "cor", "tr", "g8", "v8", "bf4", "wf2",
            "sq", "hobo", "sb16", "p16", "ob8", "bz", "cb32", "real_coupler_1", "real_coupler_2", "real_coupler_3"] {
            assert!(laatste.contains(&id.to_string()), "{id} ontbreekt in het tutti");
        }
    }

    #[test]
    fn pp_kern_in_trap_1() {
        let (d, k) = nederlands();
        let t = stel_voor(&d, &k, 20, &VoorstelOpties::default());
        let eerste: HashSet<&str> = t[0].iter().map(|s| s.as_str()).collect();
        // Zachtste 8' per manuaal, zachtste 16' in het pedaal, II/I en II/P.
        assert_eq!(eerste, ["h8", "v8", "sb16", "real_coupler_1", "real_coupler_3"].into_iter().collect());
    }

    #[test]
    fn muzikale_volgorde() {
        let (d, k) = nederlands();
        let t = stel_voor(&d, &k, 32, &VoorstelOpties::default());
        let trap = |id: &str| trap_van(&t, id).unwrap_or_else(|| panic!("{id} ontbreekt"));
        // Geen 2' vóór de prestant 8'.
        assert!(trap("o2") >= trap("p8") && trap("wf2") >= trap("p8"));
        // Mengwerken ná de 2'-registers.
        assert!(trap("mix") >= trap("o2") && trap("mix") >= trap("wf2"));
        // Tongwerken als laatste groep: de trompet na de mixtuur, de hobo niet na de trompet.
        assert!(trap("tr") >= trap("mix"));
        assert!(trap("hobo") <= trap("tr"));
        // Pedaaltong niet vóór de manuaaltrompet; I/P bij de principalen.
        assert!(trap("bz") >= trap("tr"));
        assert!(trap("real_coupler_2") >= trap("bf4") && trap("real_coupler_2") <= trap("o2"));
        // 32'-tong alleen in de laatste trap.
        assert_eq!(trap("cb32"), t.len() - 1);
    }

    #[test]
    fn opties_werken() {
        let (d, k) = nederlands();
        let zonder = VoorstelOpties { koppels: false, kleur: false, tutti_extra: false, octaafkoppels: false };
        let t = stel_voor(&d, &k, 15, &zonder);
        let laatste = t.last().unwrap();
        for id in ["real_coupler_1", "real_coupler_2", "real_coupler_3", "cor", "sq", "cb32"] {
            assert!(!laatste.contains(&id.to_string()), "{id} hoort er met deze opties niet in");
        }
        let octaaf = VoorstelOpties { octaafkoppels: true, ..VoorstelOpties::default() };
        let t = stel_voor(&d, &k, 15, &octaaf);
        assert_eq!(trap_van(&t, "coupler_super_bw"), Some(14), "octaafkoppel alleen in het tutti");
    }

    #[test]
    fn set_zonder_voetmaten() {
        // GrandOrgue zonder HarmonicNumber: alles 8'. Dan telt de naam.
        let hw = VoorstelDivisie { naam: "Hauptwerk".into(), pedaal: false, zwelkast: false, registers: vec![
            reg("pr", "Principal", "8'"), reg("oc", "Octave", "8'"), reg("so", "Superoctave", "8'"),
            reg("gd", "Gedackt", "8'"), meng("mx", "Mixtur", 4), tong("tp", "Trompete", "8'"),
        ]};
        let ped = VoorstelDivisie { naam: "Pedal".into(), pedaal: true, zwelkast: false, registers: vec![
            reg("sb", "Subbass", "8'"), reg("ob", "Octavbass", "8'"),
        ]};
        let t = stel_voor(&[hw, ped], &[], 12, &VoorstelOpties::default());
        let trap = |id: &str| trap_van(&t, id).unwrap();
        assert!(t[0].contains(&"gd".to_string()) && t[0].contains(&"sb".to_string()));
        assert!(trap("so") >= trap("pr") && trap("so") >= trap("oc"));
        assert!(trap("mx") >= trap("so"));
    }

    #[test]
    fn hoofdmanuaal_op_naam_ook_als_het_zwelwerk_groter_is() {
        // Romantisch: het Schwellwerk heeft meer registers dan het Hauptwerk.
        let hw = VoorstelDivisie { naam: "Hauptwerk".into(), pedaal: false, zwelkast: false, registers: vec![
            reg("hg", "Gambe", "8'"), reg("hp", "Principal", "8'"),
        ]};
        let sw = VoorstelDivisie { naam: "Schwellwerk".into(), pedaal: false, zwelkast: true, registers: vec![
            reg("sc", "Corno dolce", "8'"), reg("sv", "Viola", "8'"), reg("sp", "Principal", "8'"), reg("sf", "Flöte", "4'"),
        ]};
        let ped = VoorstelDivisie { naam: "Pedal".into(), pedaal: true, zwelkast: false, registers: vec![
            reg("pc", "Contrabaß", "16'"), reg("ps", "Subbaß", "16'"),
        ]};
        let koppels = vec![
            koppel("real_coupler_1", "Hauptwerk", "Schwellwerk", "unison"),
            koppel("real_coupler_2", "Schwellwerk", "Hauptwerk", "unison"),
            koppel("real_coupler_3", "Pedal", "Schwellwerk", "unison"),
            koppel("real_coupler_4", "Pedal", "Hauptwerk", "unison"),
        ];
        let t = stel_voor(&[ped, hw, sw], &koppels, 10, &VoorstelOpties::default());
        let eerste: HashSet<&str> = t[0].iter().map(|s| s.as_str()).collect();
        // II/I (gespeeld op het Hauptwerk) en II/P in trap 1, de Subbaß
        // (gedekt, zachter dan de Contrabaß) als zachte 16'.
        assert!(eerste.contains("real_coupler_1") && eerste.contains("real_coupler_3"), "{eerste:?}");
        assert!(eerste.contains("ps") && !eerste.contains("pc"), "{eerste:?}");
        // De andere kant op (gespeeld op het Schwellwerk) nooit.
        assert!(!t.last().unwrap().contains(&"real_coupler_2".to_string()));
    }

    #[test]
    fn helften_samen() {
        let hw = VoorstelDivisie { naam: "HW".into(), pedaal: false, zwelkast: false, registers: vec![
            reg("g", "Gedekt", "8'"), reg("cb", "Cornet (Bas)", "8'"), reg("cd", "Cornet (Disc)", "8'"),
            reg("p", "Prestant", "8'"),
        ]};
        let t = stel_voor(&[hw], &[], 8, &VoorstelOpties::default());
        assert_eq!(trap_van(&t, "cb"), trap_van(&t, "cd"));
    }

    #[test]
    fn voetmaat_lezen() {
        assert_eq!(voet_uit_tekst("16'"), Some(16.0));
        assert!((voet_uit_tekst("2 2/3'").unwrap() - 2.667).abs() < 0.01);
        assert!((voet_uit_tekst("4/5'").unwrap() - 0.8).abs() < 0.01);
        assert_eq!(voet_uit_tekst("1/2'"), Some(0.5));
        assert_eq!(voet_uit_tekst("3.2'"), Some(3.2));
        assert_eq!(voet_uit_tekst(""), None);
    }
}
