//! Pakketten rechtstreeks openen (C6): een GrandOrgue-pakket (`.orgue`, een zip met de
//! orgeldefinitie en de samples) wordt eenmalig uitgepakt in de map `pakketten` van de app en
//! daarna als gewone set geladen. Een tweede keer openen kost niets: de uitgepakte map wordt
//! hergebruikt zolang het pakket niet is veranderd.
//!
//! Uitpakken gebeurt eerst in een tijdelijke map (`….bezig`) en wordt pas bij succes
//! hernoemd; een mislukte of geannuleerde poging laat dus niets halfs achter. De namen in het
//! archief worden gecontroleerd (`enclosed_name`): een pad met `..` of een absoluut pad wordt
//! overgeslagen en komt nooit buiten de uitpakmap.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Bestand in de uitpakmap met het pad (ten opzichte van die map, met `/`) van de orgeldefinitie.
const MARKER: &str = ".jm-uitgepakt";

/// Waar een pakket is uitgepakt en wat er geladen moet worden.
#[derive(Debug, Clone, PartialEq)]
pub struct Uitkomst {
    /// De orgeldefinitie (`.organ`) in de uitgepakte map.
    pub organ: PathBuf,
    /// De uitgepakte map.
    pub map: PathBuf,
    /// `true` als de map al van een eerdere keer was en niets is uitgepakt.
    pub hergebruikt: bool,
}

/// FNV-1a over bytes: stabiel tussen versies van Rust (`DefaultHasher` is dat niet), zodat een
/// app-update geen gigabytes opnieuw laat uitpakken.
fn vingerafdruk(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Naam van de uitpakmap: de bestandsnaam zonder extensie (alleen letters, cijfers, `-` en `_`)
/// plus een vingerafdruk van pad, grootte en wijzigingstijd. Een gewijzigd pakket wordt zo opnieuw
/// uitgepakt en twee pakketten met dezelfde naam raken elkaar niet.
pub fn map_naam(pakket: &Path, grootte: u64, gewijzigd_secs: u64) -> String {
    let stam: String = pakket
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(40)
        .collect();
    let stam = if stam.is_empty() { "pakket".to_string() } else { stam };
    let sleutel = format!("{}|{}|{}", pakket.to_string_lossy().to_lowercase(), grootte, gewijzigd_secs);
    format!("{}-{:08x}", stam, vingerafdruk(sleutel.as_bytes()) as u32)
}

/// Is dit een `.orgue`-pakket?
pub fn is_orgue(pad: &Path) -> bool {
    pad.extension().map(|e| e.eq_ignore_ascii_case("orgue")).unwrap_or(false)
}

/// De orgeldefinitie in een map: de ondiepste `.organ`, bij gelijke diepte de eerste op naam.
/// Zonder `.organ` een Hauptwerk-definitie, als die er is.
fn kies_organ(map: &Path) -> Option<PathBuf> {
    fn loop_dir(dir: &Path, diepte: usize, uit: &mut Vec<(usize, String, PathBuf)>, hw: &mut Vec<(usize, String, PathBuf)>) {
        if diepte > 8 {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                loop_dir(&p, diepte + 1, uit, hw);
                continue;
            }
            let naam = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if naam.starts_with('.') {
                continue;
            }
            if naam.ends_with(".organ") {
                uit.push((diepte, naam, p));
            } else if naam.ends_with(".organ_hauptwerk_xml") {
                hw.push((diepte, naam, p));
            }
        }
    }
    let mut organ = Vec::new();
    let mut hw = Vec::new();
    loop_dir(map, 0, &mut organ, &mut hw);
    let lijst = if organ.is_empty() { hw } else { organ };
    lijst.into_iter().min_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1))).map(|(_, _, p)| p)
}

/// Pad naar een voorheen uitgepakte map, als die compleet is (marker aanwezig en de definitie er nog).
fn gemarkeerd_organ(map: &Path) -> Option<PathBuf> {
    let inhoud = fs::read_to_string(map.join(MARKER)).ok()?;
    let organ = map.join(inhoud.trim());
    organ.is_file().then_some(organ)
}

/// Pak het archief uit in `doel` (zip-slip-veilig). `voortgang` krijgt 0 tot 100, op basis van de
/// bytes; `geannuleerd` wordt per bestand en per blok gecontroleerd.
fn pak_uit(
    pakket: &Path,
    doel: &Path,
    voortgang: &mut dyn FnMut(u32),
    geannuleerd: &dyn Fn() -> bool,
) -> Result<(), String> {
    let file = fs::File::open(pakket).map_err(|e| format!("Pakket niet te openen: {}", e))?;
    let mut archief = zip::ZipArchive::new(file).map_err(|e| format!("Geen geldig pakket (zip onleesbaar): {}", e))?;
    let mut totaal: u64 = 0;
    for i in 0..archief.len() {
        if let Ok(e) = archief.by_index_raw(i) {
            totaal = totaal.saturating_add(e.size());
        }
    }
    let totaal = totaal.max(1);
    let mut klaar: u64 = 0;
    let mut laatste_pct: i64 = -1;
    let mut buf = vec![0u8; 256 * 1024];
    for i in 0..archief.len() {
        if geannuleerd() {
            return Err(crate::sampleset_download::melding_geannuleerd());
        }
        let mut entry = archief.by_index(i).map_err(|e| format!("Pakket onleesbaar: {}", e))?;
        let Some(rel) = entry.enclosed_name() else { continue };
        let uit = doel.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&uit).map_err(|e| format!("Map aanmaken mislukt: {}", e))?;
            continue;
        }
        if let Some(ouder) = uit.parent() {
            fs::create_dir_all(ouder).map_err(|e| format!("Map aanmaken mislukt: {}", e))?;
        }
        let mut f = fs::File::create(&uit).map_err(|e| format!("Bestand aanmaken mislukt: {}", e))?;
        loop {
            let n = entry.read(&mut buf).map_err(|e| format!("Uitpakken mislukt: {}", e))?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n]).map_err(|e| format!("Schrijven mislukt (is de schijf vol?): {}", e))?;
            klaar += n as u64;
            let pct = (klaar * 100 / totaal).min(100) as i64;
            if pct != laatste_pct {
                laatste_pct = pct;
                voortgang(pct as u32);
            }
            if geannuleerd() {
                return Err(crate::sampleset_download::melding_geannuleerd());
            }
        }
    }
    voortgang(100);
    Ok(())
}

/// Pak een `.orgue`-pakket uit in `basis/<map_naam>` (of hergebruik een eerdere uitpakking) en geef
/// de orgeldefinitie terug.
pub fn pak_orgue_uit(
    pakket: &Path,
    basis: &Path,
    voortgang: &mut dyn FnMut(u32),
    geannuleerd: &dyn Fn() -> bool,
) -> Result<Uitkomst, String> {
    let meta = fs::metadata(pakket).map_err(|e| format!("Pakket niet leesbaar: {}", e))?;
    let gewijzigd = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let naam = map_naam(pakket, meta.len(), gewijzigd);
    let doel = basis.join(&naam);
    if let Some(organ) = gemarkeerd_organ(&doel) {
        return Ok(Uitkomst { organ, map: doel, hergebruikt: true });
    }
    fs::create_dir_all(basis).map_err(|e| format!("Map voor pakketten aanmaken mislukt: {}", e))?;
    let tijdelijk = basis.join(format!("{}.bezig", naam));
    let _ = fs::remove_dir_all(&tijdelijk);
    fs::create_dir_all(&tijdelijk).map_err(|e| format!("Uitpakmap aanmaken mislukt: {}", e))?;
    let resultaat = (|| -> Result<PathBuf, String> {
        pak_uit(pakket, &tijdelijk, voortgang, geannuleerd)?;
        let organ = kies_organ(&tijdelijk)
            .ok_or_else(|| "Geen orgeldefinitie (.organ) in dit pakket gevonden".to_string())?;
        let rel = organ.strip_prefix(&tijdelijk).map_err(|e| e.to_string())?;
        let rel_tekst: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
        fs::write(tijdelijk.join(MARKER), rel_tekst.join("/")).map_err(|e| format!("Marker schrijven mislukt: {}", e))?;
        // Een halve map van een eerdere poging (zonder marker) maakt plaats.
        if doel.exists() {
            fs::remove_dir_all(&doel).map_err(|e| format!("Oude uitpakmap verwijderen mislukt: {}", e))?;
        }
        fs::rename(&tijdelijk, &doel).map_err(|e| format!("Uitpakmap hernoemen mislukt: {}", e))?;
        Ok(doel.join(rel_tekst.join("/")))
    })();
    match resultaat {
        Ok(organ) => Ok(Uitkomst { organ, map: doel, hergebruikt: false }),
        Err(e) => {
            let _ = fs::remove_dir_all(&tijdelijk);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn tijdelijke_map(naam: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("jm_pakket_{}_{}", naam, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Maakt een zip met de gegeven (naam, inhoud)-paren; een naam die op `/` eindigt is een map.
    fn maak_zip(pad: &Path, bestanden: &[(&str, &[u8])]) {
        let f = fs::File::create(pad).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opties = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (naam, inhoud) in bestanden {
            if naam.ends_with('/') {
                z.add_directory(*naam, opties).unwrap();
            } else {
                z.start_file(*naam, opties).unwrap();
                z.write_all(inhoud).unwrap();
            }
        }
        z.finish().unwrap();
    }

    fn nooit() -> bool {
        false
    }

    #[test]
    fn map_naam_is_stabiel_veilig_en_verschilt_per_pakket() {
        let a = map_naam(Path::new("C:/sets/Mijn orgel (v2).orgue"), 1000, 5);
        assert_eq!(a, map_naam(Path::new("C:/sets/Mijn orgel (v2).orgue"), 1000, 5), "zelfde invoer, zelfde naam");
        assert!(a.starts_with("Mijn_orgel__v2_-"), "{a}");
        assert!(a.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'));
        assert_ne!(a, map_naam(Path::new("C:/sets/Mijn orgel (v2).orgue"), 1001, 5), "andere grootte");
        assert_ne!(a, map_naam(Path::new("C:/sets/Mijn orgel (v2).orgue"), 1000, 6), "andere wijzigingstijd");
        assert_ne!(a, map_naam(Path::new("D:/elders/Mijn orgel (v2).orgue"), 1000, 5), "ander pad");
        // Zonder bestandsnaam valt de naam terug op "pakket"; een verborgen bestand houdt zijn stam.
        assert!(map_naam(Path::new(""), 1, 1).starts_with("pakket-"));
        assert!(map_naam(Path::new(".orgue"), 1, 1).starts_with("_orgue-"));
    }

    #[test]
    fn is_orgue_kijkt_alleen_naar_de_extensie() {
        assert!(is_orgue(Path::new("a/b/Set.orgue")));
        assert!(is_orgue(Path::new("SET.ORGUE")));
        assert!(!is_orgue(Path::new("set.organ")));
        assert!(!is_orgue(Path::new("orgue")));
    }

    #[test]
    fn pakt_een_pakket_uit_en_hergebruikt_het_daarna() {
        let d = tijdelijke_map("uitpakken");
        let pakket = d.join("Set.orgue");
        maak_zip(&pakket, &[
            ("Set/", b""),
            ("Set/Set.organ", b"[Organ]\nChurchName=Proef\n"),
            ("Set/Samples/a.wav", b"RIFFxxxx"),
        ]);
        let basis = d.join("pakketten");
        let mut pcts = Vec::new();
        let u = pak_orgue_uit(&pakket, &basis, &mut |p| pcts.push(p), &nooit).unwrap();
        assert!(!u.hergebruikt);
        assert!(u.organ.is_file(), "{:?}", u.organ);
        assert!(u.organ.ends_with("Set/Set.organ") || u.organ.ends_with("Set\\Set.organ"), "{:?}", u.organ);
        assert!(u.map.join("Set/Samples/a.wav").is_file());
        assert_eq!(pcts.last(), Some(&100));
        assert!(pcts.windows(2).all(|w| w[0] <= w[1]), "voortgang loopt op: {pcts:?}");
        assert!(!basis.join(format!("{}.bezig", u.map.file_name().unwrap().to_string_lossy())).exists(), "geen tijdelijke map over");
        // Tweede keer: hergebruikt, zonder uit te pakken (de voortgang wordt niet aangeroepen).
        let mut geroepen = false;
        let u2 = pak_orgue_uit(&pakket, &basis, &mut |_| geroepen = true, &nooit).unwrap();
        assert!(u2.hergebruikt);
        assert!(!geroepen);
        assert_eq!(u2.organ, u.organ);
        // Verdwijnt de definitie, dan wordt opnieuw uitgepakt.
        fs::remove_file(&u.organ).unwrap();
        let u3 = pak_orgue_uit(&pakket, &basis, &mut |_| {}, &nooit).unwrap();
        assert!(!u3.hergebruikt);
        assert!(u3.organ.is_file());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn een_pad_buiten_de_uitpakmap_wordt_overgeslagen() {
        let d = tijdelijke_map("zipslip");
        let pakket = d.join("Slecht.orgue");
        maak_zip(&pakket, &[
            ("../ontsnapt.txt", b"kwaad"),
            ("/absoluut.txt", b"kwaad"),
            ("a/../../ook_niet.txt", b"kwaad"),
            ("Set/Set.organ", b"[Organ]\n"),
        ]);
        // Negatieve controle: het testarchief bevat die namen echt (de schrijver heeft ze niet schoongemaakt).
        let gevaarlijk = {
            let a = zip::ZipArchive::new(fs::File::open(&pakket).unwrap()).unwrap();
            a.file_names().filter(|n| n.contains("..") || n.starts_with('/')).count()
        };
        assert_eq!(gevaarlijk, 3);
        let basis = d.join("pakketten");
        let u = pak_orgue_uit(&pakket, &basis, &mut |_| {}, &nooit).unwrap();
        assert!(u.organ.is_file());
        assert!(!d.join("ontsnapt.txt").exists());
        assert!(!basis.join("ontsnapt.txt").exists());
        assert!(!d.join("ook_niet.txt").exists());
        assert!(!basis.join("ook_niet.txt").exists());
        // Binnen de uitpakmap is geen enkel van die bestanden terechtgekomen.
        assert!(!u.map.join("absoluut.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn kiest_de_ondiepste_definitie_en_meldt_een_pakket_zonder_definitie() {
        let d = tijdelijke_map("kiezen");
        let basis = d.join("pakketten");
        let diep = d.join("Diep.orgue");
        maak_zip(&diep, &[("a/b/Z.organ", b"x"), ("a/Y.organ", b"x"), ("X.txt", b"x")]);
        let u = pak_orgue_uit(&diep, &basis, &mut |_| {}, &nooit).unwrap();
        assert!(u.organ.ends_with("a/Y.organ") || u.organ.ends_with("a\\Y.organ"), "{:?}", u.organ);
        let leeg = d.join("Leeg.orgue");
        maak_zip(&leeg, &[("readme.txt", b"geen orgel")]);
        let e = pak_orgue_uit(&leeg, &basis, &mut |_| {}, &nooit).unwrap_err();
        assert!(e.contains("Geen orgeldefinitie"), "{e}");
        // Niets halfs achtergelaten voor het pakket zonder definitie.
        let over: Vec<String> = fs::read_dir(&basis).unwrap().flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("Leeg"))
            .collect();
        assert!(over.is_empty(), "{over:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn geen_zip_geeft_een_nette_fout_en_annuleren_laat_niets_achter() {
        let d = tijdelijke_map("fout");
        let basis = d.join("pakketten");
        let nep = d.join("Nep.orgue");
        fs::write(&nep, b"dit is geen zip").unwrap();
        let e = pak_orgue_uit(&nep, &basis, &mut |_| {}, &nooit).unwrap_err();
        assert!(e.contains("Geen geldig pakket"), "{e}");
        let echt = d.join("Echt.orgue");
        maak_zip(&echt, &[("S.organ", b"x"), ("grote.bin", &vec![7u8; 600_000][..])]);
        let e = pak_orgue_uit(&echt, &basis, &mut |_| {}, &|| true).unwrap_err();
        assert_eq!(e, crate::sampleset_download::melding_geannuleerd());
        let over: Vec<String> = fs::read_dir(&basis).unwrap().flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("Echt") || n.starts_with("Nep"))
            .collect();
        assert!(over.is_empty(), "{over:?}");
        // En een pakket dat niet bestaat.
        assert!(pak_orgue_uit(&d.join("bestaat_niet.orgue"), &basis, &mut |_| {}, &nooit).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
