// Test van de omwissel-stap bij het bijwerken van een sampleset (commands::wissel_setmap_om):
// de oude set wordt pas verwijderd als de nieuwe veilig op zijn plek staat, en bij een mislukking
// komt de oude set terug. Draait met `cargo test -p vpo-app`.
use super::wissel_setmap_om;
use std::fs;
use std::path::PathBuf;

/// Een lege, unieke werkmap voor deze test (zonder extra crate).
fn werkmap(sub: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("jm-setmap-test-{}-{}", std::process::id(), sub));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn schrijf(dir: &PathBuf, naam: &str, inhoud: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(naam), inhoud).unwrap();
}

#[test]
fn omwisselen_vervangt_de_oude_set_en_ruimt_op() {
    let root = werkmap("ok");
    let doel = root.join("set");
    let temp = root.join("set.jm-update");
    let opzij = root.join("set.jm-oud");
    schrijf(&doel, "a.wav", "oud");
    schrijf(&temp, "a.wav", "nieuw");

    wissel_setmap_om(&doel, &temp, &opzij).unwrap();

    assert_eq!(fs::read_to_string(doel.join("a.wav")).unwrap(), "nieuw", "doel heeft nu de nieuwe inhoud");
    assert!(!temp.exists(), "de tijdelijke map is op zijn plek gezet");
    assert!(!opzij.exists(), "de opzij-map is opgeruimd");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn mislukt_plaatsen_zet_de_oude_terug() {
    // temp bestaat bewust NIET -> rename(temp, doel) faalt -> de oude set moet terugkomen op zijn plek.
    let root = werkmap("rollback");
    let doel = root.join("set");
    let temp = root.join("set.jm-update");
    let opzij = root.join("set.jm-oud");
    schrijf(&doel, "a.wav", "oud");

    let res = wissel_setmap_om(&doel, &temp, &opzij);

    assert!(res.is_err(), "zonder nieuwe map mislukt het omwisselen");
    assert_eq!(fs::read_to_string(doel.join("a.wav")).unwrap(), "oud", "de oude set staat weer op zijn plek");
    assert!(!opzij.exists(), "er blijft geen opzij-map slingeren");
    let _ = fs::remove_dir_all(&root);
}
