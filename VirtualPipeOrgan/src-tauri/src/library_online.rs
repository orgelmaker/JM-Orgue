//! Online zoeken naar orgelfoto's (0.8.74).
//!
//! Opt-in: standaard uit, zet aan via Instellingen. Gebruikt alleen vrije
//! bronnen (Wikimedia Commons en Wikipedia) zodat we het resultaat
//! rechtmatig mogen tonen en lokaal mogen opslaan. Elke treffer draagt zijn
//! bronverwijzing, maker en licentie mee; de UI laat die onder de foto zien.
//!
//! Alleen de orgelgegevens die de bibliotheek al heeft (naam, bouwer, plaats)
//! worden als zoekterm meegestuurd — nooit het pad naar de set, de
//! gebruikersnaam of iets anders.
//!
//! Dependencies: `ureq` (al in gebruik voor de updater), `serde_json`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{info, warn};

/// User-agent zoals de Wikimedia-regels dat voorschrijven (geldige mail +
/// contactpunt). Zonder deze header weigeren de API-eindpunten een deel van
/// de verzoeken; met deze header blijven we een "nette" API-gebruiker.
const USER_AGENT: &str = "JM-Orgue/0.8.93 (+https://github.com/orgelmaker/JM-Orgue)";

/// Maximaal aantal treffers dat we per zoekopdracht aan de UI tonen.
pub const MAX_HITS: usize = 3;

/// Minimale beeldbreedte/-hoogte voor een treffer (anders is het een icoon of
/// een logo).
const MIN_WIDTH: u32 = 400;
const MIN_HEIGHT: u32 = 300;

/// Maximaal bestandsformaat dat we binnenhalen bij het kiezen van een
/// treffer. Groter negeren we om de app-datamap klein te houden.
const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

/// Breedte van de thumbnail die de API teruglevert voor de voorvertoning.
const THUMB_WIDTH: u32 = 480;

/// Eén treffer uit de online zoektocht.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineImageHit {
    /// Directe URL van het volledige beeld (voor downloaden bij kiezen).
    pub full_url: String,
    /// URL van de thumbnail (voor de voorvertoning in de UI).
    pub thumb_url: String,
    /// Pagina op Commons of Wikipedia waar het beeld vandaan komt.
    pub source_page: String,
    /// "Wikimedia Commons" of "Wikipedia".
    pub source_name: String,
    /// Titel van het beeld ("File:…").
    pub title: String,
    /// Breedte en hoogte van het volledige beeld.
    pub width: u32,
    pub height: u32,
    /// Maker, als de metadata die noemt (vaak "Jan de Vries" of een
    /// gebruikersnaam op Commons).
    pub author: Option<String>,
    /// Licentie, als een vrije string ("CC BY-SA 4.0" / "Public Domain").
    pub license: Option<String>,
    /// Korte beschrijving/caption uit de metadata.
    pub caption: Option<String>,
}

/// Attributie bij een opgeslagen online afbeelding. Wordt onder de kaart
/// getoond en bewaard in de orgellijst.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageCredit {
    pub source_url: String,
    pub source_name: String,
    pub author: Option<String>,
    pub license: Option<String>,
    pub title: Option<String>,
}

fn clean_authority_name(s: &str) -> String {
    // HTML-sanitatie-light: tags eruit, meervoudige spaties samentrekken.
    let mut t = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => t.push(c),
            _ => {}
        }
    }
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Bouw de zoekterm uit (name, builder, location). Houdt het kort — Commons
/// doet beter werk met twee duidelijke woorden dan met een hele zin.
pub fn zoek_term(name: &str, builder: &str, location: &str) -> String {
    let mut delen: Vec<&str> = Vec::new();
    for s in [name, builder, location] {
        let s = s.trim();
        if !s.is_empty() && !delen.iter().any(|d| d.eq_ignore_ascii_case(s)) {
            delen.push(s);
        }
    }
    met_orgelwoord(delen.join(" "))
}

/// Zorg dat de zoekterm het woord orgel/organ/orgue bevat — anders vindt
/// Commons vooral kerkgevels en plattegronden.
pub fn met_orgelwoord(kern: String) -> String {
    let l = kern.to_lowercase();
    if l.contains("orgel") || l.contains("organ") || l.contains("orgue") {
        kern
    } else {
        format!("{} orgel", kern.trim())
    }
}

/// Alles wat we over een set kunnen verzamelen om kerk, plaats en bouwer te
/// vinden (0.8.88). De bibliotheek-entry alleen was te mager: een gescande
/// map heet "Custom Samples" met het pad als plaats, en bij Hauptwerk- en
/// GrandOrgue-sets staat de kerknaam vaak alleen in de ODF-opmerkingen of in
/// het meegeleverde info-HTML. Alle tekst hierin is lokaal gelezen; naar
/// buiten gaan alleen de afgeleide zoektermen (naam, kerk, plaats, bouwer).
#[derive(Debug, Default, Clone)]
pub struct ZoekContext {
    /// Naam uit de bibliotheek-entry.
    pub naam: String,
    /// Bouwer uit de bibliotheek-entry ("Custom Samples" telt niet).
    pub bouwer: String,
    /// Plaats uit de bibliotheek-entry (ChurchAddress / OrganInfo_Location);
    /// een pad telt niet.
    pub plaats: String,
    /// Kerknaam uit de ODF (ChurchName / Identification_Name) of het manifest.
    pub kerk: String,
    /// Opmerkingen uit de ODF (OrganComments / OrganInfo_Comments).
    pub opmerkingen: String,
    /// Platgeslagen tekst uit info-HTML, readme's en soortgelijke bestanden.
    pub teksten: Vec<String>,
}

/// Woorden waaraan we een kerk-/gebouwnaam herkennen, in de talen die in
/// samplesets voorkomen. Kleine letters; `contains`-match.
const KERK_WOORDEN: &[&str] = &[
    "kerk", "church", "kirche", "cathedral", "kathedraal", "cathédrale",
    "catedral", "cattedrale", "basilica", "basiliek", "basilika", "basilique",
    "église", "eglise", "iglesia", "chiesa", "kościół", "kosciol", "abbey",
    "abdij", "abtei", "abbaye", "chapel", "kapel", "kapelle", "chapelle",
    "cappella", "minster", "münster", "munster", "collegiale", "collégiale",
    "temple", "tempel", "klooster", "kloster", "priory", "stift", "katedr",
    "kostol", "kostel", "templom", "kyrka", "kirke", "kirkko", "bazylika",
    "museu", "museum", "musée",
];

/// Korte kerkwoorden die alleen als heel woord tellen: "Santa" wel,
/// "Santanyi" niet; "Dom" wel, "Kingdom" niet.
const KERK_HELE_WOORDEN: &[&str] = &[
    "dom", "st", "st.", "sint", "saint", "san", "santa", "sankt", "notre", "seo", "sv.",
];

fn bevat_kerkwoord(s: &str) -> bool {
    let l = s.to_lowercase();
    KERK_WOORDEN.iter().any(|w| l.contains(w))
        || l.split(|c: char| c.is_whitespace() || c == '-' || c == ',' || c == '(' || c == ')')
            .any(|w| KERK_HELE_WOORDEN.contains(&w.trim_matches(|c: char| c == '\'' || c == '"')))
}

/// HTML naar platte tekst: scripts/stijlen weg, tags weg, een handvol
/// entiteiten terug, witruimte samengevoegd.
pub fn strip_html(s: &str) -> String {
    fn knip_blok(s: &str, open: &str, sluit: &str) -> String {
        let mut uit = String::with_capacity(s.len());
        let mut rest = s;
        loop {
            // ASCII-lowercase houdt de byte-indexen gelijk aan het origineel
            // (to_lowercase kan bij sommige tekens de lengte veranderen).
            let l = rest.to_ascii_lowercase();
            match l.find(open) {
                Some(i) => {
                    uit.push_str(&rest[..i]);
                    match l[i..].find(sluit) {
                        Some(j) => rest = &rest[i + j + sluit.len()..],
                        None => { rest = ""; }
                    }
                }
                None => { uit.push_str(rest); break; }
            }
        }
        uit
    }
    let zonder = knip_blok(&knip_blok(s, "<script", "</script>"), "<style", "</style>");
    let mut uit = String::with_capacity(zonder.len());
    let mut in_tag = false;
    for c in zonder.chars() {
        match c {
            '<' => { in_tag = true; uit.push(' '); }
            '>' => { in_tag = false; }
            _ if !in_tag => uit.push(c),
            _ => {}
        }
    }
    let uit = uit
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&eacute;", "é")
        .replace("&egrave;", "è")
        .replace("&ouml;", "ö")
        .replace("&auml;", "ä")
        .replace("&uuml;", "ü")
        .replace("&szlig;", "ß");
    uit.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// De plaatsnaam uit een adres-/locatieveld: het stuk vóór de eerste komma,
/// zonder postcode. Een pad (mapscan zonder manifest) levert niets op.
fn plaats_kern(plaats: &str) -> String {
    let p = plaats.trim();
    if p.is_empty() || p.contains('\\') || p.contains('/') || p.contains(':') || is_opvulling(p) {
        return String::new();
    }
    for deel in p.split(',') {
        // Postcodes weg: cijfergroepen én de twee hoofdletters van een
        // Nederlandse postcode ("3512 JE Utrecht" → "Utrecht").
        let d: String = deel
            .split_whitespace()
            .filter(|w| !w.chars().all(|c| c.is_ascii_digit() || c == '-'))
            .filter(|w| !(w.len() <= 2 && w.chars().all(|c| c.is_ascii_uppercase())))
            .collect::<Vec<_>>()
            .join(" ");
        // Straatnaam ("High Street", "Hoofdstraat 12") is geen plaats.
        let laatste = d.split_whitespace().last().unwrap_or("").to_lowercase();
        let straat = ["street", "road", "avenue", "lane", "straat", "laan", "gracht",
            "strasse", "straße", "rue", "calle", "ulica"].iter()
            .any(|w| laatste == *w || laatste.ends_with(&format!("-{w}")) || (laatste.len() > w.len() + 2 && laatste.ends_with(w) && w.len() >= 5));
        if d.chars().any(|c| c.is_alphabetic()) && !straat {
            return d;
        }
    }
    String::new()
}

/// Kerknaam uit de verzamelde teksten: de kortste reeks eigennamen waarin
/// een kerkwoord staat ("Laurenskerk Rotterdam", "St. Mary's Church in
/// Landau"). Een omschrijving ("neobaroque modern church organ") of een
/// gewone zin ("The church was completed in 1525") levert niets op.
fn kerk_uit_tekst(teksten: &[String]) -> Option<String> {
    const VERBINDERS: &[&str] = &[
        "of", "in", "de", "la", "le", "van", "der", "den", "des", "du", "di", "del",
        "am", "an", "zu", "zum", "zur", "i", "y", "e", "and", "&",
    ];
    let hoofdletter = |w: &str| w.trim_start_matches(|c: char| !c.is_alphanumeric())
        .chars().next().is_some_and(|c| c.is_uppercase());
    let mut beste: Option<String> = None;
    for t in teksten {
        for regel in t.split(|c| matches!(c, '\n' | ';' | '|' | '!' | '?' | ',' | ':' | '(' | ')')) {
            let woorden: Vec<&str> = regel.split_whitespace().collect();
            let mut i = 0;
            while i < woorden.len() {
                if !hoofdletter(woorden[i]) { i += 1; continue; }
                // Reeks eigennamen, met verbindingswoordjes ertussen.
                let mut j = i + 1;
                while j < woorden.len() {
                    let w = woorden[j];
                    let vorige_eindigt = woorden[j - 1].ends_with('.') && !woorden[j - 1].eq_ignore_ascii_case("st.")
                        && !woorden[j - 1].eq_ignore_ascii_case("sv.");
                    if vorige_eindigt { break; }
                    if hoofdletter(w) {
                        j += 1;
                    } else if VERBINDERS.contains(&w.to_lowercase().as_str())
                        && woorden.get(j + 1).is_some_and(|n| hoofdletter(n)) {
                        j += 2;
                    } else {
                        break;
                    }
                }
                let kandidaat = woorden[i..j].join(" ");
                let k = kandidaat.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != ')');
                if k.len() >= 6 && k.len() <= 70 && bevat_kerkwoord(k)
                    && beste.as_ref().map_or(true, |b| k.len() < b.len())
                {
                    beste = Some(k.to_string());
                }
                i = j;
            }
        }
    }
    beste
}

/// Kerk, plaats en bouwer zoals de zoektocht ze gebruikt (0.9.38). Worden in
/// het zoekvenster als losse, bewerkbare velden getoond.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoekVelden {
    pub kerk: String,
    pub plaats: String,
    pub bouwer: String,
}

/// Opvulwaarden die geen echte plaats of bouwer zijn: placeholders van
/// editors ("[Generated by Custom Organ Design Module]"), "N/A", "Unknown".
fn is_opvulling(s: &str) -> bool {
    let t = s.trim();
    let l = t.to_lowercase();
    t.is_empty()
        || (t.starts_with('[') && t.ends_with(']'))
        || l.contains("generated by")
        || matches!(l.trim_matches(|c: char| !c.is_alphanumeric()),
            "" | "n/a" | "na" | "unknown" | "onbekend" | "unbekannt" | "inconnu"
            | "sconosciuto" | "desconocido" | "nieznany" | "none" | "tbd" | "custom samples")
}

/// Orgelnaam zonder setversie-ruis: haakjes, "demo", "extended", "surround",
/// "wet and dry", versienummers. "Nancy (demo)" → "Nancy",
/// "Rieger of Monor extend V2.0" → "Rieger of Monor".
pub fn schone_naam(naam: &str) -> String {
    let mut zonder = String::with_capacity(naam.len());
    let mut diepte = 0i32;
    for c in naam.chars() {
        match c {
            '(' | '[' => diepte += 1,
            ')' | ']' => diepte = (diepte - 1).max(0),
            _ if diepte == 0 => zonder.push(c),
            _ => {}
        }
    }
    const RUIS: &[&str] = &[
        "demo", "extended", "extend", "extension", "version", "versie", "surround",
        "wet", "dry", "and", "original", "virtual", "organs", "nl", "full", "lite",
    ];
    let woorden: Vec<&str> = zonder.split_whitespace()
        .filter(|w| {
            let kern = w.trim_matches(|c: char| !c.is_alphanumeric());
            let l = kern.to_lowercase();
            let versie = l.trim_start_matches('v').chars().all(|c| c.is_ascii_digit() || c == '.')
                && l.chars().any(|c| c.is_ascii_digit());
            !(kern.is_empty() || RUIS.contains(&l.as_str()) || versie)
        })
        .collect();
    woorden.join(" ").trim_matches(|c: char| c == '-' || c == ',' || c == ':' || c.is_whitespace()).to_string()
}

/// "Rieger Organ of Franciscan Church of Esztergom" → "Franciscan Church of
/// Esztergom": het gebouw staat na het eerste of/from/in, als daar een
/// kerkwoord staat en ervoor niet.
fn gebouw_uit_naam(s: &str) -> String {
    let schoon = schone_naam(s);
    let l = schoon.to_lowercase();
    for scheider in [" of ", " from ", " in ", " van ", " aus "] {
        if let Some(i) = l.find(scheider) {
            let (voor, rest) = (&schoon[..i], &schoon[i + scheider.len()..]);
            if bevat_kerkwoord(rest) && !bevat_kerkwoord(voor) {
                return rest.trim().to_string();
            }
        }
    }
    schoon
}

/// Begint de naam als een kerknaam ("St Augustines", "Holy Spirit Budapest",
/// "Sants Just i Pastor")?
fn begint_als_kerknaam(s: &str) -> bool {
    let l = s.trim().to_lowercase();
    ["st ", "st.", "saint", "sankt", "sint ", "san ", "santa ", "sants ", "santo ", "holy ", "notre"]
        .iter().any(|w| l.starts_with(w))
}

/// De velden die uit de set zelf af te leiden zijn: kerk uit ODF/manifest,
/// naam, opmerkingen of info-teksten; plaatskern; bouwer zonder opvulling.
pub fn afgeleide_velden(ctx: &ZoekContext) -> ZoekVelden {
    let naam = ctx.naam.trim();
    let bouwer = {
        let b = ctx.bouwer.trim().trim_end_matches('?').trim();
        if is_opvulling(b) { String::new() } else { b.to_string() }
    };
    let mut plaats = plaats_kern(&ctx.plaats);
    let kerk = if bevat_kerkwoord(&ctx.kerk) {
        gebouw_uit_naam(&ctx.kerk)
    } else if bevat_kerkwoord(naam) || begint_als_kerknaam(naam) {
        gebouw_uit_naam(naam)
    } else if bevat_kerkwoord(&plaats) {
        // Locatieveld noemt het gebouw ("Cathédrale Notre-Dame-…"): dat is de
        // kerk; een naam van één woord ("Nancy (demo)") is dan de plaats.
        let k = std::mem::take(&mut plaats);
        let n = schone_naam(naam);
        if !n.is_empty() && !n.contains(' ') { plaats = n; }
        k
    } else {
        let mut bronnen = vec![ctx.opmerkingen.clone()];
        bronnen.extend(ctx.teksten.iter().cloned());
        kerk_uit_tekst(&bronnen).unwrap_or_default()
    };
    if plaats.is_empty() {
        plaats = plaats_uit_naam(&kerk).or_else(|| plaats_uit_naam(naam)).unwrap_or_default();
    }
    ZoekVelden { kerk, plaats, bouwer }
}

/// Een plaats die in een naam verstopt zit: "Eule Organ of Szombathely
/// (Hungary)" → "Szombathely", "Museu de la Música de Barcelona" →
/// "Barcelona", "Angster Cistercian extended[Szekesfehervar Hungary]" →
/// "Szekesfehervar". Alleen één woord met een hoofdletter, geen kerkwoord.
fn plaats_uit_naam(s: &str) -> Option<String> {
    let als_plaats = |w: &str| {
        let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        (w.chars().next().is_some_and(|c| c.is_uppercase()) && w.chars().count() >= 3 && !bevat_kerkwoord(w))
            .then(|| w.to_string())
    };
    let schoon = schone_naam(s);
    let l = schoon.to_lowercase();
    for scheider in [" of ", " from ", " in ", " de ", " di ", " van "] {
        if let Some(i) = l.rfind(scheider) {
            let rest = schoon[i + scheider.len()..].trim();
            if !rest.contains(' ') {
                if let Some(p) = als_plaats(rest) { return Some(p); }
            }
        }
    }
    // Plaats + land tussen rechte haken.
    let i = s.find('[')?;
    let j = s[i..].find(']')? + i;
    let binnen: Vec<&str> = s[i + 1..j].split_whitespace().collect();
    if binnen.len() >= 2 { als_plaats(binnen[0]) } else { None }
}

/// Tot vier zoektermen, de meest specifieke eerst: kerk + plaats, naam +
/// plaats, bouwer + plaats, en als laatste de oude combinatie van 0.8.74.
pub fn zoek_termen(ctx: &ZoekContext) -> Vec<String> {
    zoek_termen_uit_velden(&ctx.naam, &afgeleide_velden(ctx))
}

/// Zoektermen uit de orgelnaam en (eventueel door de gebruiker bijgestuurde)
/// velden.
pub fn zoek_termen_uit_velden(naam: &str, v: &ZoekVelden) -> Vec<String> {
    let naam = match schone_naam(naam) { n if n.is_empty() => naam.trim().to_string(), n => n };
    let kerk = v.kerk.trim().to_string();
    let plaats = v.plaats.trim().to_string();
    let bouwer = v.bouwer.trim().to_string();

    let mut termen: Vec<String> = Vec::new();
    let mut voeg = |delen: &[&str]| {
        let kern = delen.iter().map(|d| d.trim()).filter(|d| !d.is_empty())
            .collect::<Vec<_>>().join(" ");
        if kern.is_empty() { return; }
        let t = met_orgelwoord(kern);
        if !termen.iter().any(|x| x.eq_ignore_ascii_case(&t)) && termen.len() < 4 {
            termen.push(t);
        }
    };
    if !kerk.is_empty() { voeg(&[kerk.as_str(), plaats.as_str()]); }
    if !naam.is_empty() && !naam.eq_ignore_ascii_case(&kerk) { voeg(&[naam.as_str(), plaats.as_str()]); }
    if !bouwer.is_empty() { voeg(&[bouwer.as_str(), "orgel", plaats.as_str()]); }
    // De combinatie van 0.8.74 als vangnet — met de plaatskern, niet het
    // ruwe locatieveld (dat kan een pad zijn).
    let oud = zoek_term(&naam, &bouwer, &plaats);
    voeg(&[oud.as_str()]);
    termen
}

/// Meerdere zoektermen na elkaar; treffers samengevoegd (uniek op URL),
/// hoogstens 2 × [`MAX_HITS`] in totaal.
pub fn zoek_commons_meervoudig(termen: &[String]) -> Vec<OnlineImageHit> {
    let mut uit: Vec<OnlineImageHit> = Vec::new();
    for t in termen {
        for h in zoek_commons(t) {
            if !uit.iter().any(|u| u.full_url == h.full_url) {
                uit.push(h);
            }
        }
        if uit.len() >= MAX_HITS * 2 { break; }
    }
    uit.truncate(MAX_HITS * 2);
    uit
}

/// De `_General`-velden van een Hauptwerk-ODF, lichtgewicht: alleen de kop
/// (eerste 64 KB) wordt gelezen, geen volledige parse. Ook in de
/// "compacted" vorm blijft `_General` leesbaar.
#[derive(Debug, Default, Clone)]
pub struct HwGeneralLicht {
    pub naam: String,
    pub plaats: String,
    pub bouwer: String,
    pub opmerkingen: String,
    pub info_bestand: String,
    pub pakket_id: Option<u32>,
}

fn tag_waarde(txt: &str, tag: &str) -> String {
    let open = format!("<{}>", tag);
    let Some(i) = txt.find(&open) else { return String::new() };
    let rest = &txt[i + open.len()..];
    let Some(j) = rest.find('<') else { return String::new() };
    strip_html(&rest[..j])
}

pub fn lees_hw_general_licht(path: &Path) -> Option<HwGeneralLicht> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(64 * 1024).read_to_end(&mut buf).ok()?;
    let txt = String::from_utf8_lossy(&buf);
    let g = HwGeneralLicht {
        naam: tag_waarde(&txt, "Identification_Name"),
        plaats: tag_waarde(&txt, "OrganInfo_Location"),
        bouwer: tag_waarde(&txt, "OrganInfo_Builder"),
        opmerkingen: tag_waarde(&txt, "OrganInfo_Comments"),
        info_bestand: tag_waarde(&txt, "OrganInfo_InfoFilename"),
        pakket_id: tag_waarde(&txt, "OrganInfo_InstallationPackageID").parse().ok(),
    };
    if g.naam.is_empty() && g.plaats.is_empty() && g.bouwer.is_empty() { None } else { Some(g) }
}

/// Platgeslagen tekst uit readme-/info-bestanden in een map (niet
/// recursief) plus een eventueel expliciet info-bestand. Hoogstens zes
/// bestanden van elk hoogstens 256 KB — het gaat om de kop van de tekst.
pub fn teksten_uit_map(dir: Option<&Path>, extra: Option<&Path>) -> Vec<String> {
    const MAX_BYTES: u64 = 256 * 1024;
    fn lees(p: &Path) -> Option<String> {
        use std::io::Read;
        let f = std::fs::File::open(p).ok()?;
        let mut buf = Vec::new();
        f.take(MAX_BYTES).read_to_end(&mut buf).ok()?;
        let s = String::from_utf8_lossy(&buf);
        let platt = if p.extension().map_or(false, |e| {
            let e = e.to_string_lossy().to_lowercase();
            e == "html" || e == "htm"
        }) { strip_html(&s) } else { s.split_whitespace().collect::<Vec<_>>().join(" ") };
        if platt.trim().is_empty() { None } else { Some(platt) }
    }
    let mut uit = Vec::new();
    if let Some(e) = extra {
        if let Some(t) = lees(e) { uit.push(t); }
    }
    let Some(dir) = dir else { return uit };
    let Ok(rd) = std::fs::read_dir(dir) else { return uit };
    let mut kandidaten: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| {
        if !p.is_file() { return false; }
        let naam = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let ext_ok = naam.ends_with(".txt") || naam.ends_with(".md") || naam.ends_with(".html") || naam.ends_with(".htm");
        let naam_ok = ["readme", "info", "about", "history", "geschiedenis", "beschrijving", "description", "index", "orgel", "organ", "lies", "read"]
            .iter().any(|w| naam.contains(w));
        ext_ok && naam_ok
    }).collect();
    kandidaten.sort();
    for p in kandidaten.into_iter().take(6) {
        if extra.map_or(false, |e| e == p) { continue; }
        if let Some(t) = lees(&p) { uit.push(t); }
    }
    uit
}

#[cfg(test)]
mod tests_zoekcontext {
    use super::*;

    fn velden(naam: &str, kerk: &str, plaats: &str, bouwer: &str) -> ZoekVelden {
        afgeleide_velden(&ZoekContext {
            naam: naam.into(), kerk: kerk.into(), plaats: plaats.into(), bouwer: bouwer.into(),
            ..Default::default()
        })
    }

    #[test]
    fn opvulling_en_setruis_vallen_weg() {
        let v = velden("Les-Pitea-945", "", "[Generated by Custom Organ Design Module]", "");
        assert_eq!((v.kerk.as_str(), v.plaats.as_str()), ("", ""));
        assert_eq!(velden("Celesta", "", "", "N/A").bouwer, "");
        assert_eq!(velden("Psaltery Organ", "", "", "Unknown").bouwer, "");
        assert_eq!(velden("X", "", "", "Richter?").bouwer, "Richter");
        assert_eq!(schone_naam("Nancy (demo)"), "Nancy");
        assert_eq!(schone_naam("Rieger of Monor extend V2.0"), "Rieger of Monor");
        assert_eq!(schone_naam("Lorris wet and dry"), "Lorris");
        assert_eq!(schone_naam("Utrecht Surround v.2"), "Utrecht");
        assert_eq!(schone_naam("Bätz-Witte Puttershoek"), "Bätz-Witte Puttershoek");
    }

    #[test]
    fn kerk_uit_naam_zonder_orgel_ervoor() {
        assert_eq!(velden("Rieger Organ of Franciscan Church of Esztergom demo", "Rieger Organ of Franciscan Church of Esztergom demo", "", "").kerk,
            "Franciscan Church of Esztergom");
        assert_eq!(velden("Angster Organ from Jak Abbey (Hungary)", "Angster Organ from Jak Abbey (Hungary)", "", "").kerk, "Jak Abbey");
        assert_eq!(velden("Bureå Gravkapell (Original)", "Bureå Gravkapell (Original)", "Bureå", "").kerk, "Bureå Gravkapell");
        assert_eq!(velden("St Augustines", "", "Neutral Bay", "").kerk, "St Augustines");
        assert_eq!(velden("Holy Spirit Budapest", "", "", "").kerk, "Holy Spirit Budapest");
    }

    #[test]
    fn gebouw_in_het_locatieveld_wordt_kerk() {
        let v = velden("Nancy (demo)", "", "Cathédrale Notre-Dame-de-l'Annonciation", "Cavaillé-Coll");
        assert_eq!(v.kerk, "Cathédrale Notre-Dame-de-l'Annonciation");
        assert_eq!(v.plaats, "Nancy");
        assert_eq!(velden("Nitra (demo)", "", "Katedrála sv. Emeráma", "").plaats, "Nitra");
    }

    #[test]
    fn straat_is_geen_plaats_en_omschrijving_geen_kerk() {
        assert_eq!(velden("St Stephen's Anglican", "", "High Street", "").plaats, "");
        assert_eq!(velden("X", "", "Hoofdstraat 12, Zwolle", "").plaats, "Zwolle");
        let ctx = ZoekContext {
            naam: "Rotterdam Hoofdorgel Surround".into(),
            plaats: "Rotterdam".into(),
            opmerkingen: "A neobaroque modern church organ".into(),
            teksten: vec!["Laurenskerk Rotterdam. The neobaroque modern church organ".into()],
            ..Default::default()
        };
        assert_eq!(afgeleide_velden(&ctx).kerk, "Laurenskerk Rotterdam");
        let ctx = ZoekContext {
            naam: "Rotterdam Hoofdorgel Surround".into(),
            teksten: vec!["The church was completed in 1525".into()],
            ..Default::default()
        };
        assert_eq!(afgeleide_velden(&ctx).kerk, "");
        let ctx = ZoekContext {
            naam: "Demo set".into(),
            teksten: vec!["Built in 1831 by Baetz for the St. Mary's Church in Landau. The restoration of the church was done after the World War".into()],
            ..Default::default()
        };
        assert_eq!(afgeleide_velden(&ctx).kerk, "St. Mary's Church in Landau");
        assert!(!bevat_kerkwoord("Santanyi") && !bevat_kerkwoord("Kingdom"));
        assert!(bevat_kerkwoord("Santa Maria") && bevat_kerkwoord("Utrecht Dom") && bevat_kerkwoord("Domkerk"));
    }

    #[test]
    fn plaats_uit_de_naam() {
        assert_eq!(velden("Eule Organ of Szombathely (Hungary) dry demo version", "", "", "").plaats, "Szombathely");
        assert_eq!(velden("Angster Cistercian extended[Szekesfehervar Hungary]", "", "", "").plaats, "Szekesfehervar");
        assert_eq!(velden("Orgue Josep Bosca", "", "Museu de la Música de Barcelona", "").plaats, "Barcelona");
        assert_eq!(velden("Bells from St. Andrew Church of Komarom", "Bells from St. Andrew Church of Komarom", "", "").plaats, "Komarom");
        assert_eq!(velden("Angster Organ from Jak Abbey (Hungary)", "", "", "").plaats, "");
        assert_eq!(velden("Hungarian Village Organs", "", "", "").plaats, "");
    }

    #[test]
    fn bijgestuurde_velden_bepalen_de_termen() {
        let ctx = ZoekContext {
            naam: "Grote Kerk".into(),
            bouwer: "Custom Samples".into(),
            plaats: "Utrecht, Netherlands".into(),
            ..Default::default()
        };
        let v = afgeleide_velden(&ctx);
        assert_eq!(v, ZoekVelden { kerk: "Grote Kerk".into(), plaats: "Utrecht".into(), bouwer: String::new() });
        // Ongewijzigd teruggestuurd = dezelfde termen als automatisch.
        assert_eq!(zoek_termen_uit_velden(&ctx.naam, &v), zoek_termen(&ctx));
        // Kerk zonder kerkwoord telt als de gebruiker hem zo intypt.
        let eigen = ZoekVelden { kerk: "Laurens".into(), plaats: "Rotterdam".into(), bouwer: "Marcussen".into() };
        let t = zoek_termen_uit_velden("Hoofdorgel", &eigen);
        assert_eq!(t[0], "Laurens Rotterdam orgel");
        assert!(t.iter().any(|x| x == "Marcussen orgel Rotterdam"), "{:?}", t);
    }

    #[test]
    fn kerk_en_plaats_gaan_voorop() {
        let ctx = ZoekContext {
            naam: "Grote Kerk".into(),
            bouwer: "Bätz".into(),
            plaats: "Utrecht, Netherlands".into(),
            ..Default::default()
        };
        let t = zoek_termen(&ctx);
        assert_eq!(t[0], "Grote Kerk Utrecht orgel");
        assert!(t.iter().any(|x| x == "Bätz orgel Utrecht"), "{:?}", t);
        assert!(t.len() <= 4);
    }

    #[test]
    fn kerknaam_uit_info_html() {
        let html = "<html><head><style>p{}</style></head><body><h1>Organ</h1>\
            <p>Built in 1831 by Baetz for the St. Mary&#39;s Church in Landau.</p>\
            <script>var x=1;</script></body></html>";
        let plat = strip_html(html);
        assert!(!plat.contains("var x"));
        assert!(plat.contains("St. Mary's Church"));
        let ctx = ZoekContext {
            naam: "Demo set".into(),
            plaats: "".into(),
            teksten: vec![plat],
            ..Default::default()
        };
        let t = zoek_termen(&ctx);
        assert!(t[0].to_lowercase().contains("church"), "{:?}", t);
    }

    #[test]
    fn mapscan_zonder_manifest_geeft_geen_pad_of_custom_samples_door() {
        let ctx = ZoekContext {
            naam: "Testorgel".into(),
            bouwer: "Custom Samples".into(),
            plaats: r"C:\Orgels\Testorgel".into(),
            ..Default::default()
        };
        let t = zoek_termen(&ctx);
        assert!(t.iter().all(|x| !x.contains("Custom") && !x.contains("C:\\")), "{:?}", t);
        assert_eq!(t[0], "Testorgel");
    }

    #[test]
    fn plaats_kern_laat_postcode_en_land_weg() {
        assert_eq!(plaats_kern("3512 JE Utrecht, Netherlands"), "Utrecht");
        assert_eq!(plaats_kern("Landau, Pfalz"), "Landau");
        assert_eq!(plaats_kern(r"D:\Samplesets\X"), "");
    }

    #[test]
    fn hw_general_licht_leest_de_kop() {
        let dir = std::env::temp_dir().join(format!("jm_hw_general_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.Organ_Hauptwerk_xml");
        std::fs::write(&p, "<Hauptwerk><ObjectList ObjectType=\"_General\"><_General>\
            <Identification_Name>Demo Surround</Identification_Name>\
            <OrganInfo_Location>Utrecht, Netherlands</OrganInfo_Location>\
            <OrganInfo_Builder>Baetz</OrganInfo_Builder>\
            <OrganInfo_InstallationPackageID>883</OrganInfo_InstallationPackageID>\
            <OrganInfo_InfoFilename>html/index.html</OrganInfo_InfoFilename>\
            </_General></ObjectList></Hauptwerk>").unwrap();
        let g = lees_hw_general_licht(&p).unwrap();
        assert_eq!(g.naam, "Demo Surround");
        assert_eq!(g.plaats, "Utrecht, Netherlands");
        assert_eq!(g.pakket_id, Some(883));
        assert_eq!(g.info_bestand, "html/index.html");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Vraag de Wikimedia Commons MediaWiki-API om foto's die bij deze zoekterm
/// passen. Levert hoogstens [`MAX_HITS`] treffers.
pub fn zoek_commons(term: &str) -> Vec<OnlineImageHit> {
    let url = format!(
        "https://commons.wikimedia.org/w/api.php?action=query&format=json\
         &generator=search&gsrsearch={}&gsrnamespace=6&gsrlimit=12\
         &prop=imageinfo&iiprop=url|size|user|extmetadata&iiurlwidth={}",
        urlencode(term),
        THUMB_WIDTH
    );
    let json = match ophalen_json(&url) {
        Some(j) => j,
        None => return Vec::new(),
    };
    let pages = json
        .get("query")
        .and_then(|q| q.get("pages"))
        .and_then(|p| p.as_object())
        .cloned()
        .unwrap_or_default();
    let mut hits: Vec<OnlineImageHit> = Vec::new();
    for (_pid, page) in pages {
        let title = page.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !is_bruikbare_extensie(&title) {
            continue;
        }
        let ii = match page.get("imageinfo").and_then(|v| v.as_array()).and_then(|a| a.first()) {
            Some(x) => x,
            None => continue,
        };
        let full_url = ii.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let thumb_url = ii.get("thumburl").and_then(|v| v.as_str()).unwrap_or(&full_url).to_string();
        let source_page = ii.get("descriptionurl").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let w = ii.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let h = ii.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if w < MIN_WIDTH || h < MIN_HEIGHT {
            continue;
        }
        let meta = ii.get("extmetadata").cloned().unwrap_or(serde_json::Value::Null);
        let author = meta_veld(&meta, "Artist")
            .or_else(|| ii.get("user").and_then(|v| v.as_str()).map(|s| s.to_string()));
        let license = meta_veld(&meta, "LicenseShortName").or_else(|| meta_veld(&meta, "License"));
        let caption = meta_veld(&meta, "ImageDescription").or_else(|| meta_veld(&meta, "ObjectName"));
        hits.push(OnlineImageHit {
            full_url,
            thumb_url,
            source_page,
            source_name: "Wikimedia Commons".to_string(),
            title,
            width: w,
            height: h,
            author: author.map(|s| clean_authority_name(&s)).filter(|s| !s.is_empty()),
            license: license.filter(|s| !s.is_empty()),
            caption: caption
                .map(|s| clean_authority_name(&s))
                .filter(|s| !s.is_empty()),
        });
        if hits.len() >= MAX_HITS * 3 {
            break;
        }
    }
    // Grootste afbeelding eerst — meestal de meest representatieve foto.
    hits.sort_by(|a, b| (b.width * b.height).cmp(&(a.width * a.height)));
    hits.truncate(MAX_HITS);
    hits
}

fn meta_veld(meta: &serde_json::Value, key: &str) -> Option<String> {
    meta.get(key)
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn is_bruikbare_extensie(title: &str) -> bool {
    let lower = title.to_lowercase();
    [".jpg", ".jpeg", ".png", ".webp"].iter().any(|e| lower.ends_with(e))
}

fn urlencode(s: &str) -> String {
    // Minimale percent-encoding — alleen tekens die een MediaWiki-URL kapot
    // zouden maken. Letters/cijfers/`-._~` blijven staan.
    let mut uit = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => uit.push(*b as char),
            b' ' => uit.push('+'),
            _ => uit.push_str(&format!("%{:02X}", b)),
        }
    }
    uit
}

fn ophalen_json(url: &str) -> Option<serde_json::Value> {
    match ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/json")
        .timeout(Duration::from_secs(10))
        .call()
    {
        Ok(resp) => match resp.into_string() {
            Ok(body) => serde_json::from_str(&body).ok(),
            Err(e) => {
                warn!("Online zoeken: lezen van antwoord mislukt: {}", e);
                None
            }
        },
        Err(e) => {
            warn!("Online zoeken: HTTP-verzoek mislukt: {}", e);
            None
        }
    }
}

/// Download een treffer naar de bibliotheek-afbeeldingenmap en geef het pad
/// terug. Weigert te grote bestanden en bestanden zonder bruikbare extensie.
pub fn download_naar(dir: &Path, stem: &str, hit: &OnlineImageHit) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Kan map niet maken: {}", e))?;
    // Extensie uit het URL-pad zónder querystring (Commons plakt
    // "?utm_source=…&utm_content=original" achter de bestands-URL, waardoor
    // de "extensie" voorheen "org&utm_campaign=…" werd), met de Commons-titel
    // ("File:Orgel.jpg") als terugval.
    fn ext_van(s: &str) -> Option<String> {
        let kaal = s.split(['?', '#']).next().unwrap_or(s);
        Path::new(kaal).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase())
    }
    let ext = ext_van(&hit.full_url)
        .filter(|e| ["jpg", "jpeg", "png", "webp"].contains(&e.as_str()))
        .or_else(|| ext_van(&hit.title).filter(|e| ["jpg", "jpeg", "png", "webp"].contains(&e.as_str())))
        .ok_or_else(|| format!("Onbekend afbeeldingsformaat: {}", hit.full_url))?;
    let doel = dir.join(format!("{}.{}", stem, ext));
    let resp = ureq::get(&hit.full_url)
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(30))
        .call()
        .map_err(|e| format!("Download mislukt: {}", e))?;
    let aangekondigd = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    if aangekondigd > MAX_IMAGE_BYTES {
        return Err(format!(
            "Afbeelding te groot ({} bytes, maximum {})",
            aangekondigd, MAX_IMAGE_BYTES
        ));
    }
    use std::io::Read;
    let mut reader = resp.into_reader();
    let mut buf = Vec::with_capacity(aangekondigd.min(MAX_IMAGE_BYTES) as usize);
    reader.by_ref().take(MAX_IMAGE_BYTES).read_to_end(&mut buf).map_err(|e| format!("Lezen mislukt: {}", e))?;
    if buf.is_empty() {
        return Err("Lege download".into());
    }
    std::fs::write(&doel, &buf).map_err(|e| format!("Opslaan mislukt: {}", e))?;
    info!(
        "Online orgelfoto opgeslagen: {} ({} bytes) van {}",
        doel.display(),
        buf.len(),
        hit.source_page
    );
    Ok(doel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoekterm_vult_aan_met_orgel() {
        assert_eq!(zoek_term("Grote Kerk", "Bätz", "Dordrecht"), "Grote Kerk Bätz Dordrecht orgel");
        // "organ" al aanwezig → geen toevoeging
        assert_eq!(
            zoek_term("Pipe organ", "Mascioni", "Giubiasco"),
            "Pipe organ Mascioni Giubiasco"
        );
    }

    #[test]
    fn urlencode_latin_blijft_leesbaar() {
        assert_eq!(urlencode("Hello World"), "Hello+World");
        assert_eq!(urlencode("Nitra (demo)"), "Nitra+%28demo%29");
    }

    #[test]
    fn clean_authority_name_strip_tags() {
        assert_eq!(clean_authority_name("<a href=\"x\">Jan de Vries</a>"), "Jan de Vries");
        assert_eq!(clean_authority_name("  extra   spatie "), "extra spatie");
    }
}
