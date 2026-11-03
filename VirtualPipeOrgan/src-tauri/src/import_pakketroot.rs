//! Per orgel onthouden waar de samplepakketten staan.
//!
//! Hauptwerk-sets en CODM-orgels verwijzen met een pakket-ID naar samples in
//! `OrganInstallationPackages`. Staat die map niet naast of boven de definitie,
//! dan kiest de gebruiker hem eenmalig; de keuze wordt per orgel bewaard in
//! `<app_data_dir>/import_pakketroot.json`. Er worden geen bestanden verplaatst
//! of omgezet: alleen het pad wordt onthouden.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const BESTAND: &str = "import_pakketroot.json";

type Kaart = BTreeMap<String, String>;

fn lees(app_data_dir: &Path) -> Kaart {
    std::fs::read_to_string(app_data_dir.join(BESTAND))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn schrijf(app_data_dir: &Path, kaart: &Kaart) {
    let Ok(json) = serde_json::to_string_pretty(kaart) else { return };
    let tmp = app_data_dir.join(format!("{BESTAND}.tmp"));
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, app_data_dir.join(BESTAND));
    }
}

/// De opgeslagen pakketroot van dit orgel, mits die nog bestaat.
pub fn opgeslagen(app_data_dir: &Path, orgel: &str) -> Option<PathBuf> {
    let sleutel = crate::library::pad_sleutel(orgel);
    lees(app_data_dir)
        .into_iter()
        .find(|(k, _)| crate::library::pad_sleutel(k) == sleutel)
        .map(|(_, v)| PathBuf::from(v))
        .filter(|p| p.join("OrganInstallationPackages").is_dir())
}

/// Bewaar (`Some`) of wis (`None`) de pakketroot van dit orgel.
pub fn bewaar(app_data_dir: &Path, orgel: &str, root: Option<&Path>) {
    let sleutel = crate::library::pad_sleutel(orgel);
    let mut kaart = lees(app_data_dir);
    kaart.retain(|k, _| crate::library::pad_sleutel(k) != sleutel);
    if let Some(r) = root {
        kaart.insert(orgel.to_string(), r.to_string_lossy().to_string());
    }
    schrijf(app_data_dir, &kaart);
}

/// Normaliseer een door de gebruiker gekozen map tot de root die
/// `OrganInstallationPackages` bevat. Accepteert zowel die root als de map
/// `OrganInstallationPackages` zelf. Controleert de mapstructuur vóór gebruik.
pub fn normaliseer_keuze(gekozen: &Path) -> Result<PathBuf, String> {
    if !gekozen.is_dir() {
        return Err(format!("{} is geen map", gekozen.display()));
    }
    let is_pakketmap = gekozen
        .file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case("OrganInstallationPackages"))
        .unwrap_or(false);
    let root = if is_pakketmap {
        gekozen.parent().map(|p| p.to_path_buf()).ok_or_else(|| "map heeft geen bovenliggende map".to_string())?
    } else {
        gekozen.to_path_buf()
    };
    let pakketten = root.join("OrganInstallationPackages");
    if !pakketten.is_dir() {
        return Err(format!(
            "in {} staat geen map OrganInstallationPackages; kies de map die OrganInstallationPackages bevat",
            root.display()
        ));
    }
    let heeft_pakket = std::fs::read_dir(&pakketten)
        .map(|rd| rd.flatten().any(|e| e.path().is_dir()))
        .unwrap_or(false);
    if !heeft_pakket {
        return Err(format!("{} bevat geen pakketmappen", pakketten.display()));
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tijdelijk(naam: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("jm-pakketroot-{}-{}", naam, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn keuze_wordt_genormaliseerd_en_gecontroleerd() {
        let d = tijdelijk("keuze");
        std::fs::create_dir_all(d.join("OrganInstallationPackages/000758")).unwrap();
        assert_eq!(normaliseer_keuze(&d).unwrap(), d);
        assert_eq!(normaliseer_keuze(&d.join("OrganInstallationPackages")).unwrap(), d);
        // Zonder OrganInstallationPackages: duidelijke afwijzing.
        let leeg = tijdelijk("leeg");
        assert!(normaliseer_keuze(&leeg).unwrap_err().contains("OrganInstallationPackages"));
        // Map zonder pakketmappen: afgewezen.
        let kaal = tijdelijk("kaal");
        std::fs::create_dir_all(kaal.join("OrganInstallationPackages")).unwrap();
        assert!(normaliseer_keuze(&kaal).unwrap_err().contains("geen pakketmappen"));
        assert!(normaliseer_keuze(&d.join("bestaat-niet")).is_err());
    }

    #[test]
    fn keuze_wordt_per_orgel_bewaard_en_gewist() {
        let data = tijdelijk("data");
        let root = tijdelijk("root");
        std::fs::create_dir_all(root.join("OrganInstallationPackages/1")).unwrap();
        let orgel = r"C:\Sets\Orgel\OrganDefinitions\x.CustomOrgan_Hauptwerk_xml";
        assert!(opgeslagen(&data, orgel).is_none());
        bewaar(&data, orgel, Some(&root));
        assert_eq!(opgeslagen(&data, orgel), Some(root.clone()));
        assert!(opgeslagen(&data, r"C:\Sets\Ander\y.xml").is_none());
        bewaar(&data, orgel, None);
        assert!(opgeslagen(&data, orgel).is_none());
    }

    #[test]
    fn verdwenen_map_geeft_geen_opgeslagen_root() {
        let data = tijdelijk("data2");
        let root = tijdelijk("root2");
        std::fs::create_dir_all(root.join("OrganInstallationPackages/1")).unwrap();
        bewaar(&data, "orgel.xml", Some(&root));
        std::fs::remove_dir_all(&root).unwrap();
        assert!(opgeslagen(&data, "orgel.xml").is_none());
    }
}
