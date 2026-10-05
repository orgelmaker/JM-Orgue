//! Partituurbestand `.jmscore` (0.7.84): één JSON-document met het formaat,
//! een versienummer, de `Score` (alle lagen, takes, noten, handen, kop,
//! maatsoort, tempo) en de weergavevoorkeuren van het venster. Toevoegingen
//! aan het model hoeven géén versieverhoging: `#[serde(default)]` dekt
//! ontbrekende velden. Alleen een betekenisverandering verhoogt `VERSION` en
//! krijgt een stap in `migrate`.
//!
//! Dezelfde module schrijft de reservekopie (autosave) van het venster:
//! `<app-data>/notatie/autosave-<score-id>.jmscore`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::notation::Score;

pub const FORMAT: &str = "jmscore";
pub const VERSION: u32 = 1;
const AUTOSAVE_MAP: &str = "notatie";

/// Weergavevoorkeuren die met het stuk meegaan (geen muzikale inhoud).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UiPrefs {
    /// "staff" of "klavar"; leeg = laat de keuze van het venster staan.
    #[serde(default)]
    pub view_mode: String,
    /// OSMD-zoom (1 = 100 %); 0 = laat staan.
    #[serde(default)]
    pub zoom: f64,
    /// Klavar-bereik: "auto" of "klavier"; leeg = laat staan.
    #[serde(default)]
    pub klavar_bereik: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectFile {
    pub format: String,
    pub version: u32,
    /// Schrijvende programmaversie — alleen ter informatie.
    #[serde(default)]
    pub app: String,
    pub score: Score,
    #[serde(default)]
    pub ui: UiPrefs,
}

/// Eén gevonden reservekopie.
#[derive(Debug, Clone, Serialize)]
pub struct AutosaveInfo {
    pub path: String,
    pub title: String,
    /// Laatste wijziging in milliseconden sinds 1970 (UTC).
    pub mtime_ms: u64,
}

/// JSON-tekst van een stuk (pretty, UTF-8, `\n`).
pub fn naar_json(score: &Score, ui: &UiPrefs) -> Result<String, String> {
    let pf = ProjectFile {
        format: FORMAT.to_string(),
        version: VERSION,
        app: format!("JM-Orgue {}", env!("CARGO_PKG_VERSION")),
        score: score.clone(),
        ui: ui.clone(),
    };
    let mut s = serde_json::to_string_pretty(&pf).map_err(|e| format!("Partituur serialiseren mislukt: {}", e))?;
    s.push('\n');
    Ok(s)
}

/// Schrijf een stuk weg: eerst naar een tijdelijk bestand ernaast, dan
/// hernoemen, zodat een halve schrijfactie nooit een goed bestand vervangt.
pub fn save(score: &Score, ui: &UiPrefs, path: &Path) -> Result<(), String> {
    let json = naar_json(score, ui)?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Map aanmaken mislukt ({}): {}", parent.display(), e))?;
        }
    }
    let tmp = path.with_extension("jmscore.tmp");
    // Mislukt het schrijven of het hernoemen, dan blijft er geen .tmp achter.
    let uitkomst = std::fs::write(&tmp, json.as_bytes())
        .map_err(|e| format!("Schrijven mislukt ({}): {}", tmp.display(), e))
        .and_then(|_| std::fs::rename(&tmp, path).map_err(|e| format!("Opslaan mislukt ({}): {}", path.display(), e)));
    if uitkomst.is_err() { let _ = std::fs::remove_file(&tmp); }
    uitkomst
}

/// Hoogste score-id waarvoor nog een reservekopie bestaat (0 als geen).
/// De app begint zijn score-id's daarboven, zodat een kopie van een vorige
/// sessie nooit dezelfde naam krijgt als een stuk van deze sessie.
pub fn hoogste_autosave_id(app_data_dir: &Path) -> u32 {
    list_autosaves(app_data_dir).iter()
        .filter_map(|a| Path::new(&a.path).file_name().and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("autosave-")).and_then(|n| n.strip_suffix(".jmscore"))
            .and_then(|n| n.parse::<u32>().ok()))
        .max().unwrap_or(0)
}

/// Migreer een ruw document naar de huidige versie. Een nieuwer bestand
/// dan dit programma kent wordt geweigerd (niet stil verkeerd gelezen).
fn migrate(v: Value) -> Result<Value, String> {
    let formaat = v.get("format").and_then(|f| f.as_str()).unwrap_or("");
    if formaat != FORMAT {
        return Err("Dit is geen JM-Orgue-partituur (.jmscore)".into());
    }
    let versie = v.get("version").and_then(|n| n.as_u64()).unwrap_or(0) as u32;
    match versie {
        1 => Ok(v),
        n if n > VERSION => Err(format!(
            "Deze partituur is opgeslagen met een nieuwere JM-Orgue (bestandsversie {}, dit programma kent {})", n, VERSION)),
        _ => Err(format!("Onbekende bestandsversie {}", versie)),
    }
}

/// Lees een stuk uit JSON-tekst: formaat/versie controleren, migreren, en de
/// tellers herleiden uit de inhoud (ID's blijven stabiel, undo begint leeg).
pub fn uit_json(s: &str) -> Result<(Score, UiPrefs), String> {
    let v: Value = serde_json::from_str(s).map_err(|e| format!("Partituur onleesbaar: {}", e))?;
    let v = migrate(v)?;
    let pf: ProjectFile = serde_json::from_value(v).map_err(|e| format!("Partituur onleesbaar: {}", e))?;
    let mut score = pf.score;
    score.herleid_na_laden();
    Ok((score, pf.ui))
}

pub fn load(path: &Path) -> Result<(Score, UiPrefs), String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("Kan {} niet lezen: {}", path.display(), e))?;
    uit_json(&s)
}

/// Map voor reservekopieën.
pub fn autosave_map(app_data_dir: &Path) -> PathBuf { app_data_dir.join(AUTOSAVE_MAP) }

/// Pad van de reservekopie van één stuk.
pub fn autosave_pad(app_data_dir: &Path, score_id: u32) -> PathBuf {
    autosave_map(app_data_dir).join(format!("autosave-{}.jmscore", score_id))
}

/// Alle reservekopieën, nieuwste eerst. Onleesbare bestanden worden
/// overgeslagen (ze komen bij "Verwijderen" alsnog weg).
pub fn list_autosaves(app_data_dir: &Path) -> Vec<AutosaveInfo> {
    let map = autosave_map(app_data_dir);
    let Ok(rd) = std::fs::read_dir(&map) else { return Vec::new(); };
    let mut uit: Vec<AutosaveInfo> = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        let naam = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !(naam.starts_with("autosave-") && naam.ends_with(".jmscore")) { continue; }
        let Ok(meta) = e.metadata() else { continue; };
        let mtime_ms = meta.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64).unwrap_or(0);
        let title = std::fs::read_to_string(&p).ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v.get("score").and_then(|s| s.get("title")).and_then(|t| t.as_str()).map(str::to_string))
            .unwrap_or_default();
        uit.push(AutosaveInfo { path: p.to_string_lossy().to_string(), title, mtime_ms });
    }
    uit.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms));
    uit
}

/// Is dit pad een reservekopie in onze eigen map? (Verwijderen mag alleen daar.)
pub fn is_autosave_pad(app_data_dir: &Path, path: &Path) -> bool {
    let map = autosave_map(app_data_dir);
    let naam = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    path.parent().map(|p| p == map).unwrap_or(false)
        && naam.starts_with("autosave-") && naam.ends_with(".jmscore")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::{EditCommand, KlavarHand, LayerEv};

    /// Een stuk met twee balken, twee takes, handen, routering en kop.
    fn voorbeeld() -> Score {
        let mut sc = Score::new(7);
        sc.title = "Trio".into(); sc.composer = "Anoniem".into(); sc.subtitle = "BWV 0".into();
        sc.bpm = 72.0; sc.beats_per_bar = 6; sc.beat_unit = 8; sc.key_fifths = -2; sc.minor = true;
        sc.quantize = 2; sc.tolerance_pct = 60; sc.min_measures = 4;
        sc.metronome.click_on = true; sc.metronome.count_in_beats = 2;
        let m = sc.add_layer("Manuaal".into(), Some(false));
        let p = sc.add_layer("Pedaal".into(), Some(true));
        sc.layers[0].divisions = vec!["Hauptwerk".into(), "Positief".into()];
        sc.layers[0].klavar_hand = Some(KlavarHand::Right);
        sc.layers[1].klavar_split = Some(53);
        let t1 = sc.layers[0].takes[0].id;
        let t2 = sc.add_take(m).unwrap();
        let tp = sc.layers[1].takes[0].id;
        let ev = |id: u64, midi: u8, s: u64, e: u64, ch: u8, hand: Option<KlavarHand>| LayerEv { id, midi, start_us: s, end_us: e, channel: ch, locked: false, hand };
        EditCommand::InsertEvents { events: vec![
            (m, t1, ev(1, 60, 0, 400_000, 0, None)),
            (m, t1, ev(2, 64, 400_000, 800_000, 0, Some(KlavarHand::Left))),
            (m, t2, ev(5, 67, 0, 1_600_000, 1, None)),
            (p, tp, ev(9, 43, 0, 1_600_000, 2, None)),
        ] }.apply(&mut sc);
        sc.layers[0].takes[1].visible = false;
        sc.armed_layer = Some(p);
        sc
    }

    #[test]
    fn rondreis_byte_gelijk() {
        let sc = voorbeeld();
        let ui = UiPrefs { view_mode: "klavar".into(), zoom: 1.2, klavar_bereik: "klavier".into() };
        let json = naar_json(&sc, &ui).unwrap();
        let (sc2, ui2) = uit_json(&json).unwrap();
        assert_eq!(ui2, ui);
        // Opnieuw schrijven geeft hetzelfde document, op `generation` na
        // (die is per sessie en begint na het laden weer bij 1).
        let json2 = naar_json(&sc2, &ui2).unwrap();
        let zonder_gen = |t: &str| {
            let mut v: Value = serde_json::from_str(t).unwrap();
            v["score"].as_object_mut().unwrap().remove("generation");
            v
        };
        assert_eq!(zonder_gen(&json), zonder_gen(&json2));
        assert_eq!(sc2.layers.len(), 2);
        assert_eq!(sc2.layers[0].takes.len(), 2);
        assert!(!sc2.layers[0].takes[1].visible);
        assert_eq!(sc2.layers[0].takes[0].events[1].hand, Some(KlavarHand::Left));
        assert_eq!(sc2.layers[0].divisions, vec!["Hauptwerk".to_string(), "Positief".to_string()]);
        assert_eq!((sc2.beats_per_bar, sc2.beat_unit, sc2.key_fifths, sc2.minor), (6, 8, -2, true));
        assert_eq!(sc2.composer, "Anoniem");
        assert_eq!(sc2.armed_layer, Some(2));
        // De MusicXML van het geladen stuk is gelijk aan die van vóór het opslaan.
        let x1 = crate::notation::build_musicxml_from_score(&sc).unwrap();
        let x2 = crate::notation::build_musicxml_from_score(&sc2).unwrap();
        assert_eq!(x1, x2);
    }

    #[test]
    fn laden_herleidt_tellers() {
        let sc = voorbeeld();
        let (mut sc2, _) = uit_json(&naar_json(&sc, &UiPrefs::default()).unwrap()).unwrap();
        assert_eq!(sc2.generation, 1);
        // Nieuw event-ID > alle geladen ID's; nieuwe laag en take ook.
        assert!(sc2.new_event_id() > 9);
        let l = sc2.add_layer("Extra".into(), None);
        assert!(l > 2);
        assert!(sc2.layers.last().unwrap().takes[0].id > 2);
        assert_eq!(sc2.undo_len(), 0);
        assert_eq!(sc2.redo_len(), 0);
    }

    #[test]
    fn oude_velden_ontbreken_is_ok() {
        // Minimale score zonder de velden van 0.7.70+ laadt met standaardwaarden.
        let json = r#"{"format":"jmscore","version":1,"score":{"id":1,"layers":[{"id":1,"name":"Balk 1","bass_clef":null,"takes":[{"id":1,"name":"Take 1","visible":true,"events":[{"id":1,"midi":60,"start_us":0,"end_us":500000,"channel":0,"locked":false}]}]}],"bpm":90.0,"beats_per_bar":4,"quantize":4,"key_fifths":0,"title":"Oud","tolerance_pct":80}}"#;
        let (sc, ui) = uit_json(json).unwrap();
        assert_eq!(sc.beat_unit, 4);
        assert_eq!(sc.min_measures, 0);
        assert!(sc.composer.is_empty());
        // Ontbrekende armed take → de laatste take van de laag.
        assert_eq!(sc.layers[0].armed_take, Some(1));
        assert_eq!(sc.layers[0].takes[0].events[0].hand, None);
        assert_eq!(ui, UiPrefs::default());
        // Een onbekende noemer in een bestand klemt op 4; armed_layer naar een
        // niet-bestaande laag vervalt.
        let json = r#"{"format":"jmscore","version":1,"score":{"id":1,"layers":[{"id":3,"name":"B","bass_clef":null,"takes":[]}],"bpm":90.0,"beats_per_bar":4,"beat_unit":5,"quantize":4,"key_fifths":0,"title":"","tolerance_pct":80,"armed_layer":9}}"#;
        let (sc, _) = uit_json(json).unwrap();
        assert_eq!(sc.beat_unit, 4);
        assert_eq!(sc.armed_layer, Some(3));
    }

    #[test]
    fn geladen_waarden_geklemd_en_ids_uniek() {
        // Een te groot min_measures uit een bestand wordt geklemd …
        let json = r#"{"format":"jmscore","version":1,"score":{"id":1,"layers":[{"id":1,"name":"A","bass_clef":null,"takes":[{"id":1,"name":"T","visible":true,"events":[{"id":1,"midi":60,"start_us":0,"end_us":1,"channel":0,"locked":false},{"id":1,"midi":62,"start_us":1,"end_us":2,"channel":0,"locked":false}]},{"id":1,"name":"T2","visible":true,"events":[]}]},{"id":1,"name":"B","bass_clef":null,"takes":[]}],"bpm":90.0,"beats_per_bar":4,"quantize":4,"key_fifths":0,"title":"","tolerance_pct":80,"min_measures":4000000000}}"#;
        let (mut sc, _) = uit_json(json).unwrap();
        assert_eq!(sc.min_measures, crate::notation::MAX_MIN_MEASURES);
        // … en dubbele laag-, take- en event-ID's worden hernummerd.
        assert_ne!(sc.layers[0].id, sc.layers[1].id);
        assert_ne!(sc.layers[0].takes[0].id, sc.layers[0].takes[1].id);
        let (id0, id1) = (sc.layers[0].takes[0].events[0].id, sc.layers[0].takes[0].events[1].id);
        assert_ne!(id0, id1);
        assert!(sc.new_event_id() > id0.max(id1));
        // Een absurd groot event-id laat de teller niet overlopen.
        let json = r#"{"format":"jmscore","version":1,"score":{"id":1,"layers":[{"id":4294967295,"name":"A","bass_clef":null,"takes":[{"id":1,"name":"T","visible":true,"events":[{"id":18446744073709551615,"midi":60,"start_us":0,"end_us":1,"channel":0,"locked":false}]}]}],"bpm":90.0,"beats_per_bar":4,"quantize":4,"key_fifths":0,"title":"","tolerance_pct":80}}"#;
        let (mut sc, _) = uit_json(json).unwrap();
        let _ = sc.new_event_id();
        let _ = sc.add_layer("B".into(), None);
    }

    #[test]
    fn hoogste_autosave_id_uit_map() {
        let map = std::env::temp_dir().join(format!("jm-orgue-test-ids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&map);
        assert_eq!(hoogste_autosave_id(&map), 0);
        let sc = voorbeeld();
        save(&sc, &UiPrefs::default(), &autosave_pad(&map, 3)).unwrap();
        save(&sc, &UiPrefs::default(), &autosave_pad(&map, 12)).unwrap();
        assert_eq!(hoogste_autosave_id(&map), 12);
        let _ = std::fs::remove_dir_all(&map);
    }

    #[test]
    fn nieuwere_versie_en_onbekend_formaat_weigeren() {
        let e = uit_json(r#"{"format":"jmscore","version":2,"score":{}}"#).unwrap_err();
        assert!(e.contains("nieuwere JM-Orgue"), "{}", e);
        let e = uit_json(r#"{"format":"iets","version":1,"score":{}}"#).unwrap_err();
        assert!(e.contains("geen JM-Orgue-partituur"), "{}", e);
        assert!(uit_json("dit is geen json").is_err());
        assert!(uit_json(r#"{"format":"jmscore","version":1}"#).is_err());
    }

    #[test]
    fn autosave_pad_per_score_en_herkenning() {
        let map = std::env::temp_dir().join("jm-orgue-test-autosave");
        let a = autosave_pad(&map, 3);
        let b = autosave_pad(&map, 4);
        assert_ne!(a, b);
        assert!(a.ends_with("notatie/autosave-3.jmscore") || a.ends_with("notatie\\autosave-3.jmscore"));
        assert!(is_autosave_pad(&map, &a));
        assert!(!is_autosave_pad(&map, &map.join("notatie").join("stuk.jmscore")));
        assert!(!is_autosave_pad(&map, &map.join("autosave-3.jmscore")));
    }

    #[test]
    fn save_load_en_lijst_op_schijf() {
        let map = std::env::temp_dir().join(format!("jm-orgue-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&map);
        let sc = voorbeeld();
        let pad = autosave_pad(&map, sc.id);
        save(&sc, &UiPrefs::default(), &pad).unwrap();
        assert!(!pad.with_extension("jmscore.tmp").exists());
        let (sc2, _) = load(&pad).unwrap();
        assert_eq!(sc2.title, "Trio");
        let lijst = list_autosaves(&map);
        assert_eq!(lijst.len(), 1);
        assert_eq!(lijst[0].title, "Trio");
        assert!(lijst[0].mtime_ms > 0);
        // Een tweede keer opslaan vervangt het bestand (rename over bestaand).
        save(&sc, &UiPrefs::default(), &pad).unwrap();
        assert_eq!(list_autosaves(&map).len(), 1);
        let _ = std::fs::remove_dir_all(&map);
    }

    const FIXTURE_PAD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/testdata/stuk_v1.jmscore");

    /// Schrijft de fixture, alleen met JM_SCHRIJF_FIXTURE=1 (zie notation.rs).
    #[test]
    #[ignore]
    fn schrijf_fixture_jmscore() {
        if std::env::var("JM_SCHRIJF_FIXTURE").as_deref() != Ok("1") { return; }
        let json = naar_json(&voorbeeld(), &UiPrefs { view_mode: "staff".into(), zoom: 1.0, klavar_bereik: "auto".into() }).unwrap();
        std::fs::write(FIXTURE_PAD, json.as_bytes()).unwrap();
    }

    /// Een bestand van versie 1 moet altijd blijven laden — elke latere
    /// migratiestap krijgt een eigen fixture.
    #[test]
    fn fixture_jmscore_v1() {
        let s = include_str!("testdata/stuk_v1.jmscore");
        assert!(!s.is_empty(), "fixture ontbreekt: eerst schrijf_fixture_jmscore draaien");
        let (sc, ui) = uit_json(s).unwrap();
        assert_eq!(sc.title, "Trio");
        assert_eq!(sc.layers.len(), 2);
        assert_eq!(sc.layers[0].takes[0].events.len(), 2);
        assert_eq!(ui.view_mode, "staff");
        assert_eq!(crate::notation::build_musicxml_from_score(&sc).unwrap(),
                   crate::notation::build_musicxml_from_score(&voorbeeld()).unwrap());
    }
}
