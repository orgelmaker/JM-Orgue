use super::display_identity;
use std::path::Path;
use vpo_sampler::grandorgue::OrganInfo;

/// Pad naar <map>/<map>.organ, opgebouwd met de scheidingstekens van het
/// draaiende platform. Een hardgecodeerd `C:\X\...` is op macOS en Linux
/// geen mappad maar één bestandsnaam, waardoor file_stem() de hele string
/// teruggaf en de mapcode-herkenning niet aansloeg (gevonden door de
/// platformcontrole in de bouwstraat).
fn odf_pad(map: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(map).join(format!("{}.organ", map))
}

fn info(kerk: &str, bouwer: &str, adres: &str, comments: &str) -> OrganInfo {
    OrganInfo {
        church_name: kerk.into(),
        church_address: adres.into(),
        organ_builder: bouwer.into(),
        organ_comments: comments.into(),
        recording_details: "44100 Hz, 16-bit, mp3".into(),
        ..Default::default()
    }
}

/// Schrijf een JM-Rec-projectmap in de tempmap: <stem>.organ bestaat niet
/// echt (display_identity leest hem niet), het manifest wel.
fn manifest_dir(naam: &str, json: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(naam);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("PuttBatz.jm-rec.json"), json).unwrap();
    d
}

#[test]
fn jmrec_marker_geeft_kerk_bouwer_plaats() {
    // Geen manifest: herkenning via OrganComments, delen uit de ODF.
    let o = info("Hervormde Kerk", "Bätz-Witte", "Puttershoek", "Opgenomen met JM-Rec v3.10");
    let (naam, bouwer, plaats) = display_identity(&odf_pad("PuttBatz"), &o);
    assert_eq!(naam, "Hervormde Kerk - Bätz-Witte - Puttershoek");
    assert_eq!(bouwer, "Bätz-Witte");
    assert_eq!(plaats, "Puttershoek");
}

#[test]
fn jmrec_mapcode_als_kerknaam_valt_weg() {
    // JM-Rec schrijft de mapcode als ChurchName zodra de kerknaam leeg is.
    let o = info("PuttBatz", "Bätz-Witte", "Puttershoek", "Opgenomen met JM-Rec v3.10");
    let (naam, _, _) = display_identity(&odf_pad("PuttBatz"), &o);
    assert_eq!(naam, "Bätz-Witte - Puttershoek");
}

#[test]
fn manifest_wint_van_de_odf_velden() {
    let d = manifest_dir(
        "jm_manifest_1",
        r#"{"jm_rec_version":"3.10","organ":"PuttBatz","kerk":"Hervormde Kerk","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
    );
    // ODF heeft de mapcode als kerknaam; het manifest de echte naam.
    let o = info("PuttBatz", "Onbekend", "", "");
    let (naam, bouwer, plaats) = display_identity(&d.join("PuttBatz.organ"), &o);
    assert_eq!(naam, "Hervormde Kerk - Bätz-Witte - Puttershoek");
    assert_eq!(bouwer, "Bätz-Witte");
    assert_eq!(plaats, "Puttershoek");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn manifest_met_lege_kerk_laat_dat_deel_weg() {
    let d = manifest_dir(
        "jm_manifest_2",
        r#"{"organ":"PuttBatz","kerk":"","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
    );
    let o = info("PuttBatz", "Bätz-Witte", "Puttershoek", "");
    let (naam, _, _) = display_identity(&d.join("PuttBatz.organ"), &o);
    assert_eq!(naam, "Bätz-Witte - Puttershoek");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn go_set_houdt_eigen_naam_en_krijgt_plaats() {
    // Geen JM-Rec-marker → naam ongewijzigd; location = ChurchAddress.
    let o = info("GreenPositiv", "Stanisław Pielczyk", "Katowice, Poland",
                 "Sample set was made by Piotr Grabowski.");
    let (naam, bouwer, plaats) = display_identity(Path::new(r"C:\X\GreenPositiv.organ"), &o);
    assert_eq!(naam, "GreenPositiv");
    assert_eq!(bouwer, "Stanisław Pielczyk");
    assert_eq!(plaats, "Katowice, Poland", "plaats uit ChurchAddress, niet RecordingDetails");
}

#[test]
fn lege_plaats_valt_terug_op_opnamedetails() {
    let o = info("Oud orgel", "Bouwer", "", "");
    let (_, _, plaats) = display_identity(Path::new(r"C:\X\Oud.organ"), &o);
    assert_eq!(plaats, "44100 Hz, 16-bit, mp3");
}
