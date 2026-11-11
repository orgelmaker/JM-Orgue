use super::lees_beperkt;
use std::path::PathBuf;

fn tijdelijk(naam: &str, inhoud: &[u8]) -> PathBuf {
    let p = std::env::temp_dir().join(format!("jm_instellingen_{}_{}.json", naam, std::process::id()));
    std::fs::write(&p, inhoud).unwrap();
    p
}

#[test]
fn een_gewoon_klein_bestand_wordt_gelezen() {
    let p = tijdelijk("klein", b"{\"a\": 1}");
    assert_eq!(lees_beperkt(&p, 1000).unwrap(), "{\"a\": 1}");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn een_te_groot_bestand_wordt_geweigerd_zonder_het_te_lezen() {
    let p = tijdelijk("groot", &vec![b' '; 2_000_000]);
    let fout = lees_beperkt(&p, 1_000_000).unwrap_err();
    assert!(fout.contains("te groot"), "{fout}");
    // Precies op de grens mag nog.
    assert!(lees_beperkt(&p, 2_000_000).is_ok());
    let _ = std::fs::remove_file(&p);
}

#[test]
fn een_map_of_ontbrekend_bestand_geeft_een_nette_fout() {
    let map = std::env::temp_dir();
    assert!(lees_beperkt(&map, 1000).unwrap_err().contains("geen bestand"));
    let weg = map.join("jm_instellingen_bestaat_niet_xyz.json");
    assert!(lees_beperkt(&weg, 1000).unwrap_err().contains("Kan niet lezen"));
}
