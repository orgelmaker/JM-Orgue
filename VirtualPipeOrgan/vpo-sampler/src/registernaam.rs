//! Het aantal koren van een mengwerk uit de registernaam (0.7.66).
//!
//! Onder een registerknop staat de voetmaat. Bij een mengwerk (Mixtuur,
//! Scherp, Cymbel, Sesquialter, Cornet …) verwacht een organist daar het
//! aantal koren: "4 st.", "IV", "4fach". Dat aantal is alleen betrouwbaar uit
//! wat de bouwer of producent zelf schreef: de naam. Uit de opnamen is het
//! niet af te leiden, want extra rangen zijn daar microfoonperspectieven,
//! tremulant-opnamen of samengestelde rangen, en verschillende harmonischen
//! binnen één rang zijn repeteringen.
//!
//! Vormen die in echte sets voorkomen: "Mixture IV Rks", "Plein-jeu III",
//! "Mixtur 2 fach", "Mixtur major 4-5f. 2 2/3'", "Mixtuur_4st". Plus de
//! gangbare schrijfwijzen per taal: "4 st.", "4 sterk", "4fach", "IV rangs",
//! "IV rgs", "4 chór.", "4 file", "4 h.".
//!
//! Regels:
//! - Een expliciete markering telt altijd, ook bij een register dat niet als
//!   mengwerk herkend wordt ("Prestant 2 st.").
//! - Een kaal Romeins cijfer telt alleen bij een mengwerk, en alleen I t/m
//!   VIII. Zo blijven "Prestant II", "Voix céleste II" en de Italiaanse
//!   intervalnamen ("Ottava XV", "Ripieno XIX-XXII") heel.
//! - Een kaal Arabisch getal ("Mixtuur 4") blijft staan: dat kan net zo goed
//!   een voetmaat zijn.
//!
//! Wordt een aantal gevonden, dan gaat het uit de naam. Staat er dan nog een
//! voetmaat mét voetteken of voetwoord achter ("… 2 2/3'"), dan gaat die
//! apart mee, zodat de knop "4-5 st. 2 2/3'" kan tonen zonder dat het dubbel
//! in de naam staat.

use serde::{Deserialize, Serialize};

/// Aantal koren, eventueel een bereik ("4-6"): min == max bij één getal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Koren {
    pub min: u8,
    pub max: u8,
}

/// Uitkomst van [`splits_koren`].
#[derive(Debug, Clone, PartialEq)]
pub struct KorenSplitsing {
    /// De naam zonder het aantal (en zonder de voetmaat die erachter stond).
    pub naam: String,
    pub koren: Option<Koren>,
    /// Voetmaat die mét voetteken of voetwoord in de naam stond, als
    /// "2 2/3'". Alleen gevuld als er ook koren gevonden zijn.
    pub voetmaat: Option<String>,
}

const MAX_KOREN: u8 = 12;

/// Markeringen die ná een getal het aantal koren aangeven (los woord).
/// "f", "h", "file" en "chor" alleen bij een mengwerk: "Principal 8 f." kan
/// net zo goed 8 Fuß betekenen, en "Chor" is in het Duits ook een werk.
fn is_los_merkteken(w: &str, mengwerk: bool) -> bool {
    match w.trim_end_matches('.').to_lowercase().as_str() {
        "st" | "sterk" | "fach" | "rks" | "rk" | "ranks" | "rank" | "rgs" | "rangs" => true,
        "f" | "file" | "h" | "hileras" | "chór" | "chor" | "chóry" => mengwerk,
        _ => false,
    }
}

fn romeins(s: &str) -> Option<u8> {
    match s.to_uppercase().as_str() {
        "I" => Some(1),
        "II" => Some(2),
        "III" => Some(3),
        "IV" => Some(4),
        "V" => Some(5),
        "VI" => Some(6),
        "VII" => Some(7),
        "VIII" => Some(8),
        _ => None,
    }
}

fn arabisch(s: &str) -> Option<u8> {
    if s.is_empty() || s.len() > 2 || !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse::<u8>().ok().filter(|v| (1..=MAX_KOREN).contains(v))
}

/// "4", "4-6", "IV", "III-V" (met de gegeven getallenleer) naar Koren.
fn bereik(s: &str, getal: fn(&str) -> Option<u8>) -> Option<Koren> {
    let mut delen = s.split(['-', '–']);
    let a = getal(delen.next()?)?;
    let b = match delen.next() {
        Some(d) => getal(d)?,
        None => a,
    };
    if delen.next().is_some() || b < a {
        return None;
    }
    Some(Koren { min: a, max: b })
}

/// Eén woord met het merkteken eraan vast: "4st", "4st.", "4-6st", "4f.",
/// "4-5f.", "4fach", "2-fach", "4-fach". "4f" alleen bij een mengwerk.
fn vast_merkteken(w: &str, mengwerk: bool) -> Option<Koren> {
    let l = w.trim_end_matches('.').to_lowercase();
    let staarten: &[&str] = if mengwerk { &["-fach", "fach", "sterk", "st", "f"] } else { &["-fach", "fach", "sterk", "st"] };
    for staart in staarten {
        if let Some(kop) = l.strip_suffix(staart) {
            if let Some(k) = bereik(kop, arabisch) {
                return Some(k);
            }
        }
    }
    None
}

/// Voetmaat met voetteken of voetwoord aan het eind van de woorden: geeft het
/// aantal woorden en de voetmaat als "2 2/3'".
fn voetmaat_achteraan(woorden: &[&str]) -> Option<(usize, String)> {
    let is_voetwoord = |w: &str| {
        matches!(
            w.trim_end_matches('.').to_lowercase().as_str(),
            "fuss" | "fuß" | "voet" | "ft" | "feet" | "pieds" | "pied"
        )
    };
    let getal_of_breuk = |w: &str| {
        !w.is_empty()
            && w.chars().all(|c| c.is_ascii_digit() || c == '/' || ('\u{00BC}'..='\u{215E}').contains(&c))
            && w.chars().next().map_or(false, |c| c.is_ascii_digit())
    };
    let n = woorden.len();
    if n == 0 {
        return None;
    }
    // Voetwoord als laatste woord: "2 2/3 Fuß", "8 Fuß".
    let (eind, met_voetwoord) = if is_voetwoord(woorden[n - 1]) { (n - 1, true) } else { (n, false) };
    if eind == 0 {
        return None;
    }
    let laatste = woorden[eind - 1];
    let laatste_kaal = laatste.trim_end_matches(['\'', '\u{2032}', '\u{2019}']);
    let met_voetteken = laatste_kaal.len() < laatste.len();
    if !(met_voetwoord || met_voetteken) || !getal_of_breuk(laatste_kaal) {
        return None;
    }
    // Eventueel een heel getal ervoor: "2 2/3'".
    let mut begin = eind - 1;
    let mut delen = vec![laatste_kaal.to_string()];
    if laatste_kaal.contains('/') && begin > 0 && woorden[begin - 1].chars().all(|c| c.is_ascii_digit()) {
        begin -= 1;
        delen.insert(0, woorden[begin].to_string());
    }
    Some((n - begin, format!("{}'", delen.join(" "))))
}

/// Haal het aantal koren uit een registernaam. `mengwerk`: hoort het register
/// tot de mengwerkfamilie (alleen dan telt een kaal Romeins cijfer).
pub fn splits_koren(naam: &str, mengwerk: bool) -> KorenSplitsing {
    let ongewijzigd = || KorenSplitsing { naam: naam.to_string(), koren: None, voetmaat: None };
    let origineel: Vec<&str> = naam.split_whitespace().collect();

    // Woorden om in te zoeken: haakjes en komma's van de randen af ("(IV)",
    // "IV,"), en een bereik met losse streep ("3 - 4 st") als één woord.
    // Elk zoekwoord onthoudt welke originele woorden het beslaat, zodat de
    // naam die overblijft precies de oorspronkelijke schrijfwijze houdt.
    let schoon = |w: &str| w.trim_matches(|c| matches!(c, '(' | ')' | '[' | ']' | ',' | ';')).to_string();
    let mut zoek: Vec<(String, usize, usize)> = Vec::new(); // (woord, eerste origineel, aantal)
    let mut i = 0;
    while i < origineel.len() {
        let w = schoon(origineel[i]);
        if i + 2 < origineel.len() && matches!(origineel[i + 1], "-" | "–") {
            let samen = format!("{}-{}", w, schoon(origineel[i + 2]));
            if bereik(&samen, arabisch).is_some() || bereik(&samen, romeins).is_some()
                || vast_merkteken(&samen, mengwerk).is_some() {
                zoek.push((samen, i, 3));
                i += 3;
                continue;
            }
        }
        zoek.push((w, i, 1));
        i += 1;
    }
    if zoek.len() < 2 && vast_merkteken(zoek.first().map_or("", |z| z.0.as_str()), mengwerk).is_none() {
        return ongewijzigd();
    }

    // Zoek van rechts naar links: het aantal staat meestal achteraan, en bij
    // "II Mixtur IV" is het rechtse het aantal en het linkse een divisie.
    let mut treffer: Option<(usize, usize, Koren)> = None; // (eerste zoekwoord, aantal, koren)
    for i in (0..zoek.len()).rev() {
        let w = zoek[i].0.as_str();
        if let Some(k) = vast_merkteken(w, mengwerk) {
            treffer = Some((i, 1, k));
            break;
        }
        if let Some(m) = zoek.get(i + 1) {
            if is_los_merkteken(&m.0, mengwerk) {
                if let Some(k) = bereik(w, arabisch).or_else(|| bereik(w, romeins)) {
                    treffer = Some((i, 2, k));
                    break;
                }
            }
        }
    }
    if treffer.is_none() && mengwerk {
        for i in (0..zoek.len()).rev() {
            if let Some(k) = bereik(&zoek[i].0, romeins) {
                treffer = Some((i, 1, k));
                break;
            }
        }
    }
    let Some((begin, aantal, koren)) = treffer else { return ongewijzigd() };
    // Buiten een mengwerk is "1 st" geen aantal koren, en "1st" in het Engels
    // een rangtelwoord ("1st Open Diapason").
    if !mengwerk && koren.min < 2 {
        return ongewijzigd();
    }

    let van = zoek[begin].1;
    let tot = zoek[begin + aantal - 1].1 + zoek[begin + aantal - 1].2;
    // Een komma die alleen het aantal van de voetmaat scheidde ("IV, 2'") mag weg.
    let rest: Vec<String> = origineel[..van].iter().chain(origineel[tot..].iter())
        .map(|w| w.trim_end_matches(',').to_string())
        .filter(|w| !w.is_empty())
        .collect();
    let mut rest: Vec<&str> = rest.iter().map(|w| w.as_str()).collect();
    let voetmaat = match voetmaat_achteraan(&rest) {
        Some((n, v)) if n < rest.len() => {
            rest.truncate(rest.len() - n);
            Some(v)
        }
        _ => None,
    };
    let nieuwe_naam = rest.join(" ");
    // Er moet een naam overblijven, en die moet met een letter beginnen.
    if !nieuwe_naam.chars().next().map_or(false, |c| c.is_alphabetic()) {
        return ongewijzigd();
    }
    KorenSplitsing { naam: nieuwe_naam, koren: Some(koren), voetmaat }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(min: u8, max: u8) -> Option<Koren> { Some(Koren { min, max }) }

    fn check(naam: &str, mengwerk: bool, verwacht_naam: &str, koren: Option<Koren>, voet: Option<&str>) {
        let s = splits_koren(naam, mengwerk);
        assert_eq!(s.naam, verwacht_naam, "naam bij {naam:?}");
        assert_eq!(s.koren, koren, "koren bij {naam:?}");
        assert_eq!(s.voetmaat.as_deref(), voet, "voetmaat bij {naam:?}");
    }

    #[test]
    fn vormen_uit_echte_sets() {
        check("Mixtur major 4-5f. 2 2/3'", true, "Mixtur major", k(4, 5), Some("2 2/3'"));
        check("Mixtur minor 4f. 1 1/3'", true, "Mixtur minor", k(4, 4), Some("1 1/3'"));
        check("Plein Jeu 4-5f. 2'", true, "Plein Jeu", k(4, 5), Some("2'"));
        check("Scharff 4f. 1'", true, "Scharff", k(4, 4), Some("1'"));
        check("Cornet a pavilon 1-8f. 8'", true, "Cornet a pavilon", k(1, 8), Some("8'"));
        check("Great Mixture IV Rks", true, "Great Mixture", k(4, 4), None);
        check("Swell Mixture III Rks", true, "Swell Mixture", k(3, 3), None);
        check("Mixtur 2 fach", true, "Mixtur", k(2, 2), None);
        check("Plein-jeu III", true, "Plein-jeu", k(3, 3), None);
        check("Mixtuur 4st", true, "Mixtuur", k(4, 4), None);
    }

    #[test]
    fn schrijfwijzen_per_taal() {
        check("Mixtuur 4 sterk", true, "Mixtuur", k(4, 4), None);
        check("Mixtuur 4-6 st.", true, "Mixtuur", k(4, 6), None);
        check("Mixtuur 4-6st.", true, "Mixtuur", k(4, 6), None);
        check("V Cornet", true, "Cornet", k(5, 5), None);
        check("Mixtur IV-VI 2'", true, "Mixtur", k(4, 6), Some("2'"));
        check("Sesquialter II 2 2/3'", true, "Sesquialter", k(2, 2), Some("2 2/3'"));
        check("Cornet V 8'", true, "Cornet", k(5, 5), Some("8'"));
        check("Fourniture IV rangs", true, "Fourniture", k(4, 4), None);
        check("Plein-jeu V rgs", true, "Plein-jeu", k(5, 5), None);
        check("Lleno 4 h.", true, "Lleno", k(4, 4), None);
        check("Ripieno 4 file", true, "Ripieno", k(4, 4), None);
        check("Mikstura 4-5 chór.", true, "Mikstura", k(4, 5), None);
        check("Rauschpfeife 2fach", true, "Rauschpfeife", k(2, 2), None);
        check("Mixtur 4-fach", true, "Mixtur", k(4, 4), None);
        check("Cymbel III", true, "Cymbel", k(3, 3), None);
        check("Mixtur 4fach 2 Fuß", true, "Mixtur", k(4, 4), Some("2'"));
        check("II Mixtur IV", true, "II Mixtur", k(4, 4), None);
        // Haakjes, komma's en een bereik met losse streep.
        check("Mixtuur (IV)", true, "Mixtuur", k(4, 4), None);
        check("Scherp (III)", true, "Scherp", k(3, 3), None);
        check("Mixtur IV, 2'", true, "Mixtur", k(4, 4), Some("2'"));
        check("Mixtuur 3 - 4 st", true, "Mixtuur", k(3, 4), None);
        check("Mixtuur 3 – 4 st.", true, "Mixtuur", k(3, 4), None);
        check("Mixtuur 3 - 4st", true, "Mixtuur", k(3, 4), None);
    }

    #[test]
    fn blijft_heel() {
        // Geen aantal in de naam.
        check("Mixtuur", true, "Mixtuur", None, None);
        check("Tertiaan", true, "Tertiaan", None, None);
        check("Ruijspijp", true, "Ruijspijp", None, None);
        check("Mixtuur Desc.", true, "Mixtuur Desc.", None, None);
        // Een kaal getal kan een voetmaat zijn.
        check("Mixtuur 4", true, "Mixtuur 4", None, None);
        // Britse intervalreeks en Italiaanse intervallen.
        check("Mixture 19.22.26", true, "Mixture 19.22.26", None, None);
        check("Ripieno XIX-XXII", true, "Ripieno XIX-XXII", None, None);
        check("Ottava XV", false, "Ottava XV", None, None);
        // Romeins bij een register dat geen mengwerk is.
        check("Prestant II", false, "Prestant II", None, None);
        check("Voix céleste II", false, "Voix céleste II", None, None);
        check("Trompet 8", false, "Trompet 8", None, None);
        check("Quint 2 2/3", false, "Quint 2 2/3", None, None);
        // Alleen een aantal, geen naam: niet aankomen.
        check("IV", true, "IV", None, None);
        check("4st", true, "4st", None, None);
        // Een woord dat toevallig op "st" eindigt is geen aantal.
        check("Holpijp Vorst", false, "Holpijp Vorst", None, None);
        // Vaststaand: te veel koren is geen aantal.
        check("Mixtuur 16 st", true, "Mixtuur 16 st", None, None);
        // Engels rangtelwoord en een Duits werk zijn geen aantal koren.
        check("1st Open Diapason", false, "1st Open Diapason", None, None);
        check("Open Diapason 1st", false, "Open Diapason 1st", None, None);
        check("Principal 8 Chor", false, "Principal 8 Chor", None, None);
    }

    #[test]
    fn expliciete_markering_telt_ook_buiten_mengwerken() {
        check("Prestant 2 st.", false, "Prestant", k(2, 2), None);
        // Maar "f" kan buiten een mengwerk ook Fuß zijn.
        check("Principal 8 f.", false, "Principal 8 f.", None, None);
        check("Octave 4f", false, "Octave 4f", None, None);
    }
}
