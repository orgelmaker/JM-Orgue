//! Herkenning van microfoonperspectieven in rank-/StopRank-/submapnamen.
//!
//! Sets met meerdere microfoonposities (Hauptwerk "(front)/(rear)/(dry)",
//! GrandOrgue-ranks "Principal 8' Rear", JM-Rec-submappen "Front"/"Rear")
//! coderen het perspectief in de naam. Deze module haalt daar een canoniek
//! label uit (`front`, `close`, `dry`, `rear`, `diffuse` of `micN`) zodat de
//! laadpijplijn lagen met hetzelfde label over alle stops als één perspectief
//! kan behandelen (aan/uit + volume per orgel).
//!
//! De detectie is bewust heuristisch en conservatief: alleen een token tussen
//! haakjes aan het eind, het laatste woord (na spatie/`-`/`_`/`|`) of de hele
//! naam wordt tegen de tabel gelegd. "Mixtur rank 2" of "Principal 8'" geven
//! dus `None` — dat zijn echte (gestapelde) ranks, geen perspectieven.

/// Stereo-helft van één opname. Hauptwerk schrijft de linker- en rechterkant
/// van hetzelfde perspectief als aparte rangen: "(front-L component)" en
/// "(front-R component)" (Rotterdam), of kaal "(L component)" / "(R component)"
/// als er maar één perspectief is (Utrecht). Die twee horen bij elkaar — samen
/// vormen ze het stereobeeld — en krijgen daarom hetzelfde label.
fn is_stereo_helft(rest: &str) -> bool {
    let r = rest.trim_start_matches(['-', '_', ' ', '/']).trim();
    matches!(
        r,
        "l" | "r" | "left" | "right"
            | "l component" | "r component"
            | "left component" | "right component"
            | "l-component" | "r-component"
    )
}

/// Canoniek label voor één token, of None als het geen perspectief is.
fn canonical(token: &str) -> Option<String> {
    let t = token.trim().to_lowercase();
    // "front-L component" is het front-perspectief, alleen de linkerhelft
    // ervan; het label moet dus gewoon "front" zijn.
    for scheiding in ['-', ' ', '_'] {
        if let Some((kop, rest)) = t.split_once(scheiding) {
            if is_stereo_helft(rest) {
                if let Some(c) = canonical(kop) {
                    return Some(c);
                }
            }
        }
    }
    let c = match t.as_str() {
        "front" | "frontal" | "vorne" => "front",
        "close" | "near" | "nah" | "proche" => "close",
        "dry" | "direct" => "dry",
        "rear" | "back" | "far" | "distant" | "fern" | "hinten" | "lointain" | "arrière" | "arriere" => "rear",
        "diffuse" | "ambient" | "ambience" | "room" | "wet" | "surround" | "hall" => "diffuse",
        _ => {
            // Mic-nummers ("mic1", "Mic_2", "mic-3"): label = de naam zelf, lowercase.
            let rest = t.strip_prefix("mic")?;
            let digits = rest.trim_start_matches(|c: char| c == '_' || c == ' ' || c == '-');
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                return Some(t);
            }
            return None;
        }
    };
    Some(c.to_string())
}

/// Zoek het perspectief-token in `name`. Geeft (canoniek label, byte-offset
/// waar het token — inclusief voorafgaand scheidingsteken — begint).
fn find_token(name: &str) -> Option<(String, usize)> {
    let s = name.trim_end();
    if s.is_empty() {
        return None;
    }
    // 1. Token tussen haakjes aan het eind: "Bourdon 16 (front)".
    if s.ends_with(')') {
        if let Some(open) = s.rfind('(') {
            let inner = &s[open + 1..s.len() - 1];
            if let Some(c) = canonical(inner) {
                return Some((c, open));
            }
        }
    }
    // 2. De hele naam ("front", "Mic_2").
    if let Some(c) = canonical(s) {
        return Some((c, 0));
    }
    // 3. Het laatste woord, na spatie/-/_/|: "Principal 8 Rear", "Prestant_8_Rear".
    let sep = s
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace() || *ch == '-' || *ch == '_' || *ch == '|');
    let (start, last) = match sep {
        Some((i, ch)) => (i, &s[i + ch.len_utf8()..]),
        None => return None, // hele naam is al geprobeerd in stap 2
    };
    if last.is_empty() {
        return None;
    }
    let c = canonical(last)?;
    Some((c, start))
}

/// Herken een microfoonperspectief in een rank-/StopRank-/submapnaam.
/// Retourneert het canonieke label (`front`, `close`, `dry`, `rear`,
/// `diffuse`, `micN`) of None.
pub fn detect_perspective(name: &str) -> Option<String> {
    find_token(name).map(|(c, _)| c)
}

/// Naam zonder het perspectief-token: "Bourdon 16 (front)" → "Bourdon 16",
/// "Principal 8 Rear" → "Principal 8". Voettallen blijven intact; zonder
/// herkend token komt de (getrimde) naam ongewijzigd terug.
pub fn strip_perspective(name: &str) -> String {
    match find_token(name) {
        Some((_, start)) => name[..start]
            .trim_end_matches(|c: char| c.is_whitespace() || c == '-' || c == '_' || c == '|')
            .trim()
            .to_string(),
        None => name.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_haakjes() {
        assert_eq!(detect_perspective("01. GO  Bourdon 16 (front)").as_deref(), Some("front"));
        assert_eq!(detect_perspective("PED  Soubasse 16 (rear)").as_deref(), Some("rear"));
        assert_eq!(detect_perspective("Bourdon 8 (dry)").as_deref(), Some("dry"));
        assert_eq!(detect_perspective("Bourdon 8 (Diffuse)").as_deref(), Some("diffuse"));
    }

    #[test]
    fn test_detect_laatste_woord_en_synoniemen() {
        assert_eq!(detect_perspective("Principal 8 Rear").as_deref(), Some("rear"));
        assert_eq!(detect_perspective("Prestant_8_Front").as_deref(), Some("front"));
        assert_eq!(detect_perspective("Flöte 4 - Close").as_deref(), Some("close"));
        assert_eq!(detect_perspective("Trompete 8 | hinten").as_deref(), Some("rear"));
        assert_eq!(detect_perspective("Bourdon direct").as_deref(), Some("dry"));
        assert_eq!(detect_perspective("Bourdon 8 near").as_deref(), Some("close"));
        assert_eq!(detect_perspective("Plein jeu lointain").as_deref(), Some("rear"));
    }

    #[test]
    fn test_detect_hele_naam_en_micnummers() {
        assert_eq!(detect_perspective("Front").as_deref(), Some("front"));
        assert_eq!(detect_perspective("Rear").as_deref(), Some("rear"));
        assert_eq!(detect_perspective("mic1").as_deref(), Some("mic1"));
        assert_eq!(detect_perspective("Mic_2").as_deref(), Some("mic_2"));
        assert_eq!(detect_perspective("Bourdon 8 (Mic 3)").as_deref(), Some("mic 3"));
    }

    #[test]
    fn test_detect_geen_perspectief() {
        assert_eq!(detect_perspective("Principal 8'"), None);
        assert_eq!(detect_perspective("Mixtur rank 2"), None);
        assert_eq!(detect_perspective("Mixtur 1"), None);
        assert_eq!(detect_perspective("Fernflöte 8"), None);
        assert_eq!(detect_perspective("Microphone"), None);
        assert_eq!(detect_perspective("Voix céleste 8"), None);
        assert_eq!(detect_perspective(""), None);
        assert_eq!(detect_perspective("   "), None);
    }

    #[test]
    fn test_strip_laat_voettallen_intact() {
        assert_eq!(strip_perspective("Bourdon 16 (front)"), "Bourdon 16");
        assert_eq!(strip_perspective("Principal 8 Rear"), "Principal 8");
        assert_eq!(strip_perspective("Principal 8' (rear)"), "Principal 8'");
        assert_eq!(strip_perspective("Kwinta 1 1/3' - far"), "Kwinta 1 1/3'");
        assert_eq!(strip_perspective("Prestant_8_Rear"), "Prestant_8");
        // Zonder perspectief: alleen trimmen.
        assert_eq!(strip_perspective("  Principal 8'  "), "Principal 8'");
        assert_eq!(strip_perspective("Mixtur rank 2"), "Mixtur rank 2");
    }
}

#[cfg(test)]
mod stereo_helften {
    use super::*;

    /// Hauptwerk splitst één perspectief soms in een linker- en rechterrang.
    /// Beide horen onder hetzelfde label, anders klinkt een orgel met
    /// front-L, front-R én rear alle drie tegelijk.
    #[test]
    fn linker_en_rechterhelft_delen_hun_perspectief() {
        assert_eq!(detect_perspective("4 Octaaf 2 (front-L component)").as_deref(), Some("front"));
        assert_eq!(detect_perspective("4 Octaaf 2 (front-R component)").as_deref(), Some("front"));
        assert_eq!(detect_perspective("Bourdon 16 (rear-L component)").as_deref(), Some("rear"));
        assert_eq!(detect_perspective("Principal 8 (dry L)").as_deref(), Some("dry"));
    }

    /// Zonder perspectief ervoor is het géén perspectief maar gewoon de ene
    /// helft van een stereo-opname: die lagen klinken samen.
    #[test]
    fn een_kale_helft_is_geen_perspectief() {
        assert_eq!(detect_perspective("Praestant 8 (L component)"), None);
        assert_eq!(detect_perspective("Praestant 8 (R component)"), None);
    }

    /// En een gewone naam met een streepje blijft met rust.
    #[test]
    fn gewone_namen_blijven_ongemoeid() {
        assert_eq!(detect_perspective("Bourdon 16"), None);
        assert_eq!(detect_perspective("Vox humana 8 (tremmed)"), None);
        assert_eq!(detect_perspective("Trompette-en-chamade 8"), None);
    }
}
