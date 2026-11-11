use super::afbeeldingsbron_leesbaar;

/// Punt 4: de negatieve afbeeldingscache mag "gezocht en niets gevonden"
/// niet verwarren met "kon niet zoeken". Alleen bij een leesbare bronmap
/// mag `image_searched` gezet worden.
#[test]
fn losgekoppelde_schijf_geldt_niet_als_gezocht() {
    // USB eruit / netwerkschijf weg: er viel niets te zoeken.
    assert!(!afbeeldingsbron_leesbaar(r"Q:\bestaat-niet\Orgel", "sample_directory"));
    assert!(!afbeeldingsbron_leesbaar(r"Q:\bestaat-niet\Orgel\set.organ", "organ_file"));
    assert!(!afbeeldingsbron_leesbaar(r"\\geen-server\orgels\set.organ", "organ_file"));
}

#[test]
fn bereikbare_map_geldt_wel_als_gezocht() {
    let d = std::env::temp_dir().join("jm_afbeeldingsbron_1");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let odf = d.join("set.organ");
    std::fs::write(&odf, b"[Organ]\n").unwrap();

    // Sample-map: het pad zelf. Orgelbestand: de map eromheen.
    assert!(afbeeldingsbron_leesbaar(&d.to_string_lossy(), "sample_directory"));
    assert!(afbeeldingsbron_leesbaar(&odf.to_string_lossy(), "organ_file"));
    // Een ODF dat zelf weg is, maar in een bestaande map stond: er kán
    // gezocht worden, dus "geen foto" mag hier wél vastgelegd worden.
    assert!(afbeeldingsbron_leesbaar(&d.join("weg.organ").to_string_lossy(), "organ_file"));
    // Een bestand als sample-map opgegeven is geen leesbare map.
    assert!(!afbeeldingsbron_leesbaar(&odf.to_string_lossy(), "sample_directory"));

    let _ = std::fs::remove_dir_all(&d);
}

/// Hauptwerk: de foto staat in OrganInstallationPackages/<pakket>/, niet
/// naast de definitie. Een leesbare definitiemap zonder bereikbare
/// pakketten (losgekoppelde schijf) mag dus NIET als "doorzocht" gelden.
#[test]
fn hauptwerk_zonder_bereikbare_pakketten_geldt_niet_als_gezocht() {
    let d = std::env::temp_dir().join("jm_afbeeldingsbron_hw");
    let _ = std::fs::remove_dir_all(&d);
    let defs = d.join("OrganDefinitions");
    let pakketten = d.join("OrganInstallationPackages");
    std::fs::create_dir_all(&defs).unwrap();
    std::fs::create_dir_all(pakketten.join("000690")).unwrap();
    let odf = defs.join("Set.Organ_Hauptwerk_xml");
    std::fs::write(&odf, b"<Hauptwerk/>").unwrap();
    let odf_s = odf.to_string_lossy().to_string();

    // Pakketten bereikbaar: definitiemap én pakketmap leesbaar -> doorzocht.
    assert!(afbeeldingsbron_leesbaar(&odf_s, "organ_file"));

    // Pakketten weg (schijf eruit): definitiemap nog leesbaar, maar de
    // plek waar de foto had moeten staan was onbereikbaar -> niet doorzocht.
    std::fs::remove_dir_all(&pakketten).unwrap();
    assert!(afbeeldingsbron_leesbaar(&defs.to_string_lossy(), "sample_directory"));
    assert!(!afbeeldingsbron_leesbaar(&odf_s, "organ_file"));

    // GrandOrgue (.organ) in dezelfde map kent geen pakketten: onveranderd.
    let go = defs.join("set.organ");
    std::fs::write(&go, b"[Organ]").unwrap();
    assert!(afbeeldingsbron_leesbaar(&go.to_string_lossy(), "organ_file"));

    let _ = std::fs::remove_dir_all(&d);
}
