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
const USER_AGENT: &str = "JM-Orgue/0.8.74 (+https://github.com/orgelmaker/JM-Orgue)";

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
    let kern = delen.join(" ");
    if kern.to_lowercase().contains("orgel")
        || kern.to_lowercase().contains("organ")
        || kern.to_lowercase().contains("orgue")
    {
        kern
    } else {
        format!("{} orgel", kern)
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
    let ext = Path::new(&hit.full_url)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "jpg".into());
    if !["jpg", "jpeg", "png", "webp"].contains(&ext.as_str()) {
        return Err(format!("Onbekend afbeeldingsformaat: {}", ext));
    }
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
