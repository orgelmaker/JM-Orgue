use super::{
    bibliotheek_identiteit_licht, lees_organ_sectie_licht, start_opfris_eenmalig,
    ververs_bibliotheek_namen,
};
use crate::library::{OrganLibrary, OrganLibraryEntry};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn tempmap(naam: &str) -> PathBuf {
    let d = std::env::temp_dir().join(naam);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Een .organ met een [Organ]-sectie gevolgd door véél andere secties: de
/// lichte lezer mag alleen de eerste sectie gebruiken.
fn schrijf_organ(dir: &Path, stem: &str, organ_sectie: &str) -> PathBuf {
    let p = dir.join(format!("{}.organ", stem));
    let mut txt = format!("[Organ]\n{}\n", organ_sectie);
    for i in 1..=50 {
        txt.push_str(&format!("[Stop{:03}]\nChurchName=FOUT\nName=Prestant\n", i));
    }
    std::fs::write(&p, txt).unwrap();
    p
}

#[test]
fn leest_alleen_de_organ_sectie() {
    let d = tempmap("jm_lib_licht_1");
    let p = schrijf_organ(
        &d,
        "GreenPositiv",
        "ChurchName=GreenPositiv\nChurchAddress=Katowice, Poland\nOrganBuilder=Pielczyk\nRecordingDetails=Piotr Grabowski\nNumberOfManuals=1",
    );
    let info = lees_organ_sectie_licht(&p).expect("[Organ] moet gelezen worden");
    assert_eq!(info.church_name, "GreenPositiv", "latere secties mogen niet meetellen");
    assert_eq!(info.church_address, "Katowice, Poland");
    assert_eq!(info.organ_builder, "Pielczyk");

    // Geen JM-Rec-markering → naam ongewijzigd, plaats uit ChurchAddress.
    let (naam, bouwer, plaats) =
        bibliotheek_identiteit_licht(&p.to_string_lossy(), "organ_file").expect("identiteit");
    assert_eq!(naam, "GreenPositiv");
    assert_eq!(bouwer, "Pielczyk");
    assert_eq!(plaats, "Katowice, Poland");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn jmrec_manifest_geeft_de_nieuwe_kaartnaam() {
    // Punt 8: de kaart toonde nog de mapcode "PuttBatz".
    let d = tempmap("jm_lib_licht_2");
    let p = schrijf_organ(&d, "PuttBatz", "ChurchName=PuttBatz\nNumberOfManuals=2");
    std::fs::write(
        d.join("PuttBatz.jm-rec.json"),
        r#"{"jm_rec_version":"3.10","organ":"PuttBatz","kerk":"Hervormde Kerk","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
    )
    .unwrap();
    let (naam, bouwer, plaats) =
        bibliotheek_identiteit_licht(&p.to_string_lossy(), "organ_file").expect("identiteit");
    assert_eq!(naam, "Hervormde Kerk - Bätz-Witte - Puttershoek");
    assert_eq!(bouwer, "Bätz-Witte");
    assert_eq!(plaats, "Puttershoek");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn mapscan_zonder_manifest_laat_de_entry_staan() {
    let d = tempmap("jm_lib_licht_3");
    // Sample-map zonder manifest: niets te zeggen → None (niet wissen).
    assert!(bibliotheek_identiteit_licht(&d.to_string_lossy(), "sample_directory").is_none());
    std::fs::write(
        d.join("PuttBatz.jm-rec.json"),
        r#"{"organ":"PuttBatz","kerk":"","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
    )
    .unwrap();
    let (naam, _, plaats) =
        bibliotheek_identiteit_licht(&d.to_string_lossy(), "sample_directory").expect("manifest");
    assert_eq!(naam, "Bätz-Witte - Puttershoek");
    assert_eq!(plaats, "Puttershoek");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn weggehaalde_schijf_en_hauptwerk_geven_none() {
    // USB eruit: het pad bestaat niet → niets bijwerken, niets wissen.
    assert!(lees_organ_sectie_licht(Path::new(r"Q:\bestaat-niet\x.organ")).is_none());
    assert!(bibliotheek_identiteit_licht(r"Q:\bestaat-niet\x.organ", "organ_file").is_none());
    // Hauptwerk-XML bewust overgeslagen (niet lichtgewicht te lezen).
    assert!(bibliotheek_identiteit_licht(r"Q:\x\Set.Organ_Hauptwerk_xml", "organ_file").is_none());
}

#[test]
fn latin1_organ_sectie_blijft_leesbaar() {
    // Oude ODF's staan vaak in ISO-8859-1; "Bätz" mag niet verminken.
    let d = tempmap("jm_lib_licht_4");
    let p = d.join("Oud.organ");
    let mut bytes = b"[Organ]\nChurchName=B".to_vec();
    bytes.push(0xE4); // 'ä' in Latin-1
    bytes.extend_from_slice(b"tz-kerk\nOrganBuilder=Onbekend\n[Manual001]\n");
    std::fs::write(&p, bytes).unwrap();
    let info = lees_organ_sectie_licht(&p).expect("[Organ]");
    assert_eq!(info.church_name, "Bätz-kerk");
    let _ = std::fs::remove_dir_all(&d);
}

/// Onderdeel B: `get_organ_library` (UI-thread) mag NOOIT op de opfrisser
/// wachten. Het werk hier blokkeert tot de test het vrijgeeft — een
/// "losgekoppelde USB-schijf". Zou de starter erop wachten (zoals de
/// oude `recv_timeout` van 400 ms), dan kwam de aanroep pas na de
/// vrijgave terug, of nooit.
#[test]
fn get_organ_library_wacht_niet_op_de_opfrisser() {
    let vlag = AtomicBool::new(false);
    let (vrijgave_tx, vrijgave_rx) = std::sync::mpsc::channel::<()>();
    let (gestart_tx, gestart_rx) = std::sync::mpsc::channel::<()>();
    let teller = Arc::new(AtomicUsize::new(0));
    let t = teller.clone();

    let t0 = Instant::now();
    let handle = start_opfris_eenmalig(&vlag, move || {
        t.fetch_add(1, Ordering::SeqCst);
        let _ = gestart_tx.send(());
        let _ = vrijgave_rx.recv(); // trage schijf: hangt tot de test hem loslaat
    })
    .expect("eerste aanroep start de opfris");
    let duur = t0.elapsed();
    assert!(duur < Duration::from_millis(100), "aanroeper wachtte {:?} op de opfrisser", duur);
    assert!(vlag.load(Ordering::SeqCst), "vlag staat meteen (één keer per proces)");

    // De thread draait (en hangt nog) terwijl de aanroeper al terug is.
    gestart_rx.recv_timeout(Duration::from_secs(5)).expect("opfris-thread is gestart");
    assert_eq!(teller.load(Ordering::SeqCst), 1);

    // Bibliotheek opnieuw geopend: tweede aanroep doet niets en is meteen terug.
    let t1 = Instant::now();
    assert!(start_opfris_eenmalig(&vlag, || panic!("mag niet nog eens draaien")).is_none());
    assert!(t1.elapsed() < Duration::from_millis(100));

    vrijgave_tx.send(()).unwrap();
    handle.join().unwrap();
    assert_eq!(teller.load(Ordering::SeqCst), 1, "het werk draait precies één keer");
}

fn entry(pad: &Path, naam: &str) -> OrganLibraryEntry {
    OrganLibraryEntry {
        id: pad.to_string_lossy().to_lowercase(),
        name: naam.into(),
        builder: "Custom Samples".into(),
        location: String::new(),
        year: None,
        stop_count: 0,
        source_type: "organ_file".into(),
        source_path: pad.to_string_lossy().to_string(),
        image_path: None,
        image_manual: false,
        image_searched: None,
        image_credit: None,
    }
}

/// De achtergrondopfris werkt de kaart bij en slaat op; een entry zonder
/// wijziging of zonder leesbaar bestand blijft met rust, en zonder
/// wijziging wordt er niets naar schijf geschreven.
#[test]
fn opfris_werkt_kaartnamen_bij_en_slaat_alleen_bij_wijziging_op() {
    let d = tempmap("jm_lib_licht_5");
    let data = d.join("appdata");
    std::fs::create_dir_all(&data).unwrap();
    let p = schrijf_organ(&d, "PuttBatz", "ChurchName=PuttBatz\nNumberOfManuals=2");
    std::fs::write(
        d.join("PuttBatz.jm-rec.json"),
        r#"{"jm_rec_version":"3.10","organ":"PuttBatz","kerk":"Hervormde Kerk","plaats":"Puttershoek","bouwer":"Bätz-Witte"}"#,
    )
    .unwrap();
    let weg = d.join("weg.organ"); // bestaat niet: USB eruit

    let lib = RwLock::new(OrganLibrary {
        organs: vec![entry(&p, "PuttBatz"), entry(&weg, "Oude naam")],
        settings: Default::default(),
        online_image_search_enabled: false,
        ..Default::default()
    });
    assert_eq!(ververs_bibliotheek_namen(&lib, &data), 1);
    {
        let l = lib.read();
        assert_eq!(l.organs[0].name, "Hervormde Kerk - Bätz-Witte - Puttershoek");
        assert_eq!(l.organs[0].builder, "Bätz-Witte");
        assert_eq!(l.organs[0].location, "Puttershoek");
        assert_eq!(l.organs[1].name, "Oude naam", "onleesbaar pad: entry blijft staan");
        assert_eq!(l.organs.len(), 2, "er wordt nooit iets gewist");
    }
    let json = data.join("organ_library.json");
    let txt = std::fs::read_to_string(&json).expect("bibliotheek is opgeslagen");
    assert!(txt.contains("Hervormde Kerk - Bätz-Witte - Puttershoek"));

    // Tweede ronde: niets veranderd → niets opgeslagen.
    std::fs::remove_file(&json).unwrap();
    assert_eq!(ververs_bibliotheek_namen(&lib, &data), 0);
    assert!(!json.exists(), "zonder wijziging wordt er niet opgeslagen");
    let _ = std::fs::remove_dir_all(&d);
}
