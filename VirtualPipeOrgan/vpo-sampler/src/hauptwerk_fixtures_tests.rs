//! Fixtures zonder samplebibliotheek (E11).
//!
//! Verzonnen Hauptwerk-definities waarmee het gedrag van de importer rond de
//! punten uit de optimalisatielijst vastligt, zonder dat er een sampleset op
//! schijf hoeft te staan: meerdere attacks per laag (E2), het schakelnetwerk
//! met een lus en met een spiegel (E3) en de beginstand van een register (E4).
//! De niveaus van de set (E1) en de crossfade van de maker (D3) hebben hun
//! eigen fixtures in `hauptwerk.rs`.
//!
//! Sommige tests leggen **het huidige gedrag** vast waar een eerdere taak
//! bewust niets veranderde (E2: de selectiecriteria worden niet gelezen; E4:
//! de beginstand wordt niet ingelezen). Ze zijn daar als zodanig benoemd: wie die
//! punten later bouwt, past de test met opzet aan.

use super::*;
use std::sync::mpsc;
use std::time::Duration;

fn fm(kv: &[(&str, &str)]) -> FieldMap {
    kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

// ---------------------------------------------------------------------------
// E2: meerdere attacks per laag
// ---------------------------------------------------------------------------

/// Eén register, één pijp (noot 36), één laag. `attack_rijen` zijn de
/// AttackSample-rijen van die laag; sample 1 is `Bourdon/eerste.wav`, sample 2
/// `Bourdon/tweede.wav`.
fn xml_met_attacks(attack_rijen: &str) -> String {
    format!(
        r#"<Hauptwerk FileFormat="Organ" FileFormatVersion="4.0">
<ObjectList ObjectType="_General"><_General><Identification_Name>Fixture</Identification_Name></_General></ObjectList>
<ObjectList ObjectType="Division"><Division><DivisionID>1</DivisionID><Name>Grand Orgue</Name></Division></ObjectList>
<ObjectList ObjectType="Stop"><Stop><StopID>1</StopID><Name>Bourdon 8</Name><DivisionID>1</DivisionID></Stop></ObjectList>
<ObjectList ObjectType="StopRank"><StopRank><StopID>1</StopID><RankID>1</RankID><Name>GO  Bourdon 8</Name><MIDINoteNumOfFirstMappedDivisionInputNode>36</MIDINoteNumOfFirstMappedDivisionInputNode><NumberOfMappedDivisionInputNodes>1</NumberOfMappedDivisionInputNodes><MIDINoteNumIncrementFromDivisionToRank>0</MIDINoteNumIncrementFromDivisionToRank></StopRank></ObjectList>
<ObjectList ObjectType="Rank"><Rank><RankID>1</RankID><Name>01. GO  Bourdon 8</Name></Rank></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01"><Pipe_SoundEngine01><PipeID>11</PipeID><RankID>1</RankID><NormalMIDINoteNumber>36</NormalMIDINoteNumber></Pipe_SoundEngine01></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_Layer"><Pipe_SoundEngine01_Layer><LayerID>111</LayerID><PipeID>11</PipeID><PipeLayerNumber>1</PipeLayerNumber></Pipe_SoundEngine01_Layer></ObjectList>
<ObjectList ObjectType="Pipe_SoundEngine01_AttackSample">{attack_rijen}</ObjectList>
<ObjectList ObjectType="Sample"><Sample><SampleID>1</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/eerste.wav</SampleFilename></Sample><Sample><SampleID>2</SampleID><InstallationPackageID>1</InstallationPackageID><SampleFilename>Bourdon/tweede.wav</SampleFilename></Sample></ObjectList>
</Hauptwerk>"#
    )
}

fn aanval(uniek: u32, sample: u32, extra_velden: &str) -> String {
    format!(
        "<Pipe_SoundEngine01_AttackSample><UniqueID>{uniek}</UniqueID><LayerID>111</LayerID><SampleID>{sample}</SampleID>{extra_velden}</Pipe_SoundEngine01_AttackSample>"
    )
}

/// Het pad van de enige pijp van de fixture, als tekst met `/`.
fn pad_van_de_pijp(xml: &str) -> String {
    let mut pkg = HashMap::new();
    pkg.insert(1u32, PathBuf::from("/pkg"));
    let def = build_definition_met(xml, PathBuf::from("/root"), &pkg, true, true).expect("build");
    let stop = def.stops.iter().find(|s| s.name == "Bourdon 8").expect("register");
    let PipeDef::Sample { path, .. } = &stop.pipes[0] else { panic!("pijp 36 hoort een sample te zijn") };
    path.to_string_lossy().replace('\\', "/")
}

/// De eerste attack-regel van de laag wint. Dat is ook wat Hauptwerk doet als
/// alle selectiecriteria op hun standaardwaarde staan (de situatie in alle
/// onderzochte sets, zie `E2-uitkomst.md`).
#[test]
fn bij_meerdere_attacks_wint_de_eerste_regel_van_het_bestand() {
    let eerst_1 = pad_van_de_pijp(&xml_met_attacks(&format!("{}{}", aanval(1, 1, ""), aanval(2, 2, ""))));
    assert!(eerst_1.ends_with("Bourdon/eerste.wav"), "{eerst_1}");
    // Omgekeerde volgorde in het bestand: nu wint de andere. De volgorde van het
    // bestand beslist dus, niet het nummer van het sample of de UniqueID.
    let eerst_2 = pad_van_de_pijp(&xml_met_attacks(&format!("{}{}", aanval(2, 2, ""), aanval(1, 1, ""))));
    assert!(eerst_2.ends_with("Bourdon/tweede.wav"), "{eerst_2}");
}

/// LEGT HET HUIDIGE GEDRAG VAST (E2): de selectiecriteria van een attack
/// (`AttackSelCriteria_HighestVelocity`, `…MinTimeSincePrevPipeCloseMs`) worden
/// niet gelezen, dus de eerste regel wint ook als de tweede een afwijkend
/// criterium heeft. In geen enkele onderzochte set komt zo'n afwijking voor. Komt
/// er ooit een set mee, dan hoort deze test om te vallen en te worden vervangen
/// door een test van de echte keuze.
#[test]
fn attackcriteria_worden_nu_niet_gelezen() {
    let rijen = format!(
        "{}{}",
        aanval(1, 1, ""),
        aanval(
            2,
            2,
            "<AttackSelCriteria_HighestVelocity>60</AttackSelCriteria_HighestVelocity>\
             <AttackSelCriteria_MinTimeSincePrevPipeCloseMs>200</AttackSelCriteria_MinTimeSincePrevPipeCloseMs>"
        )
    );
    let pad = pad_van_de_pijp(&xml_met_attacks(&rijen));
    assert!(pad.ends_with("Bourdon/eerste.wav"), "{pad}");
}

// ---------------------------------------------------------------------------
// E3: het schakelnetwerk met een lus en met een spiegel
// ---------------------------------------------------------------------------

type Netwerk = (Vec<FieldMap>, Vec<FieldMap>, Vec<FieldMap>, Vec<FieldMap>, Vec<FieldMap>);

/// Twee registers op één rang met pijpen op noot 36 en 37, zoals in de
/// bestaande proef, en de bijbehorende toetsschakelaars.
fn basisnetwerk() -> Netwerk {
    let stops = vec![
        fm(&[("StopID", "1"), ("Name", "Gambe 16"), ("DivisionID", "1"), ("ControllingSwitchID", "23001")]),
        fm(&[("StopID", "2"), ("Name", "Spiegel 16"), ("DivisionID", "1"), ("ControllingSwitchID", "23002")]),
    ];
    let division_inputs = vec![
        fm(&[("DivisionID", "1"), ("SwitchID", "9001"), ("NormalMIDINoteNumber", "36")]),
        fm(&[("DivisionID", "1"), ("SwitchID", "9002"), ("NormalMIDINoteNumber", "37")]),
    ];
    let ranks = vec![fm(&[("RankID", "4"), ("Name", "001. P  Gambe 16 (close)")])];
    let pipes = vec![
        fm(&[("PipeID", "401"), ("RankID", "4"), ("NormalMIDINoteNumber", "36"), ("ControllingPalletSwitchID", "5001")]),
        fm(&[("PipeID", "402"), ("RankID", "4"), ("NormalMIDINoteNumber", "37"), ("ControllingPalletSwitchID", "5002")]),
    ];
    let linkages = vec![
        fm(&[("SourceSwitchID", "9001"), ("DestSwitchID", "7001"), ("ConditionSwitchID", "23001")]),
        fm(&[("SourceSwitchID", "7001"), ("DestSwitchID", "5001")]),
        fm(&[("SourceSwitchID", "9002"), ("DestSwitchID", "7002"), ("ConditionSwitchID", "23001")]),
        fm(&[("SourceSwitchID", "7002"), ("DestSwitchID", "5002")]),
    ];
    (stops, pipes, linkages, division_inputs, ranks)
}

/// Voert de synthese uit op een eigen thread en wacht hoogstens vijf seconden:
/// een netwerk met een lus mag de import niet laten hangen.
fn synthese_met_tijdslimiet(n: Netwerk) -> Vec<FieldMap> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (stops, pipes, linkages, division_inputs, ranks) = n;
        let uit = synth_stopranks_via_switches(&stops, &pipes, &linkages, &division_inputs, &ranks);
        let _ = tx.send(uit);
    });
    rx.recv_timeout(Duration::from_secs(5)).expect("de synthese bleef hangen in het schakelnetwerk")
}

/// Een lus in de linkages (A→B→C→A, een schakelaar die naar zichzelf wijst, en
/// een terugweg van de pallet naar de toets) laat de import niet hangen en
/// verandert de uitkomst voor de gewone pijpen niet.
#[test]
fn schakelnetwerk_met_een_lus_eindigt_met_hetzelfde_resultaat() {
    let basis = synthese_met_tijdslimiet(basisnetwerk());
    assert_eq!(basis.len(), 1, "{basis:?}");
    let (stops, pipes, mut linkages, division_inputs, ranks) = basisnetwerk();
    linkages.extend([
        fm(&[("SourceSwitchID", "5001"), ("DestSwitchID", "9001")]), // pallet terug naar de toets
        fm(&[("SourceSwitchID", "7002"), ("DestSwitchID", "7002")]), // naar zichzelf
        fm(&[("SourceSwitchID", "7101"), ("DestSwitchID", "7102")]), // driehoek 7101→7102→7103→7101
        fm(&[("SourceSwitchID", "7102"), ("DestSwitchID", "7103")]),
        fm(&[("SourceSwitchID", "7103"), ("DestSwitchID", "7101")]),
        fm(&[("SourceSwitchID", "7103"), ("DestSwitchID", "5001"), ("ConditionSwitchID", "23001")]),
    ]);
    let met_lus = synthese_met_tijdslimiet((stops, pipes, linkages, division_inputs, ranks));
    assert_eq!(met_lus.len(), basis.len(), "{met_lus:?}");
    for veld in ["StopID", "RankID", "MIDINoteNumOfFirstMappedDivisionInputNode",
                 "NumberOfMappedDivisionInputNodes", "MIDINoteNumIncrementFromDivisionToRank"] {
        assert_eq!(met_lus[0].get(veld), basis[0].get(veld), "veld {veld} verandert door de lus");
    }
}

/// Pijp 403 (noot 38) hangt aan pallet 6000, met `lengte` routing-schakelaars
/// tussen de pallet en toets 9003 (noot 38); het register (voorwaarde 23001) zit
/// aan de toetskant van de keten. Geeft de uitkomst van de synthese.
fn netwerk_met_keten(lengte: u32) -> Vec<FieldMap> {
    let (stops, mut pipes, mut linkages, mut division_inputs, ranks) = basisnetwerk();
    pipes.push(fm(&[("PipeID", "403"), ("RankID", "4"), ("NormalMIDINoteNumber", "38"), ("ControllingPalletSwitchID", "6000")]));
    // Een eigen toets voor noot 38 aan het uiteinde van de keten, zodat het
    // bereik van het register zichtbaar groeit zodra de pijp gevonden wordt.
    division_inputs.push(fm(&[("DivisionID", "1"), ("SwitchID", "9003"), ("NormalMIDINoteNumber", "38")]));
    let mut vorige = 6000u32;
    for i in 1..=lengte {
        let r = 6000 + i;
        linkages.push(fm(&[("SourceSwitchID", &r.to_string()), ("DestSwitchID", &vorige.to_string())]));
        vorige = r;
    }
    // Toets en register aan het uiteinde van de keten.
    linkages.push(fm(&[("SourceSwitchID", "9003"), ("DestSwitchID", &vorige.to_string()), ("ConditionSwitchID", "23001")]));
    synthese_met_tijdslimiet((stops, pipes, linkages, division_inputs, ranks))
}

/// Het netwerk wordt hoogstens acht stappen diep afgelopen. Met drie
/// tussenschakelaars wordt de pijp van noot 38 gevonden (het bereik loopt dan
/// van 36 t/m 38); met tien ligt het register buiten bereik en valt de pijp
/// buiten, zonder dat de andere pijpen veranderen.
#[test]
fn netwerk_wordt_hoogstens_acht_stappen_diep_afgelopen() {
    let ondiep = netwerk_met_keten(3);
    assert_eq!(ondiep.len(), 1, "{ondiep:?}");
    assert_eq!(ondiep[0].get_u32("NumberOfMappedDivisionInputNodes"), Some(3), "ondiepe keten: pijp 38 hoort er bij");

    let diep = netwerk_met_keten(10);
    assert_eq!(diep.len(), 1, "{diep:?}");
    assert_eq!(diep[0].get_u32("MIDINoteNumOfFirstMappedDivisionInputNode"), Some(36));
    assert_eq!(diep[0].get_u32("NumberOfMappedDivisionInputNodes"), Some(2), "diepe keten: pijp 38 valt buiten bereik");
}

/// Een spiegel: dezelfde pallet bereikbaar via twee gelijke routes naar hetzelfde
/// register. Elke pijp telt één keer (het bereik zwelt niet op) en de volgorde
/// van de linkages in het bestand verandert de uitkomst niet.
#[test]
fn schakelnetwerk_met_een_spiegel_telt_niet_dubbel_en_is_volgordeonafhankelijk() {
    let (stops, pipes, mut linkages, division_inputs, ranks) = basisnetwerk();
    // Tweede route naar dezelfde pallets, via andere routing-schakelaars.
    linkages.extend([
        fm(&[("SourceSwitchID", "9001"), ("DestSwitchID", "7201"), ("ConditionSwitchID", "23001")]),
        fm(&[("SourceSwitchID", "7201"), ("DestSwitchID", "5001")]),
        fm(&[("SourceSwitchID", "9002"), ("DestSwitchID", "7202"), ("ConditionSwitchID", "23001")]),
        fm(&[("SourceSwitchID", "7202"), ("DestSwitchID", "5002")]),
    ]);
    let normaal = synthese_met_tijdslimiet((stops.clone(), pipes.clone(), linkages.clone(), division_inputs.clone(), ranks.clone()));
    assert_eq!(normaal.len(), 1, "{normaal:?}");
    assert_eq!(normaal[0].get_u32("NumberOfMappedDivisionInputNodes"), Some(2), "de spiegel verdubbelt het bereik niet");

    let mut omgekeerd = linkages;
    omgekeerd.reverse();
    let andersom = synthese_met_tijdslimiet((stops, pipes, omgekeerd, division_inputs, ranks));
    assert_eq!(andersom.len(), 1);
    for veld in ["StopID", "RankID", "MIDINoteNumOfFirstMappedDivisionInputNode",
                 "NumberOfMappedDivisionInputNodes", "MIDINoteNumIncrementFromDivisionToRank"] {
        assert_eq!(andersom[0].get(veld), normaal[0].get(veld), "veld {veld} hangt van de volgorde af");
    }
}

/// Een pallet die op gelijke diepte door twee verschillende registers bereikt
/// wordt (een "gespiegeld" register): de pijp hoort bij precies één van de twee,
/// nooit bij beide en nooit bij geen.
#[test]
fn pallet_van_twee_registers_op_gelijke_diepte_gaat_naar_precies_een() {
    let (stops, pipes, mut linkages, division_inputs, ranks) = basisnetwerk();
    // Naast register 1 (23001) ook register 2 (23002) op dezelfde pallets.
    linkages.extend([
        fm(&[("SourceSwitchID", "9001"), ("DestSwitchID", "7301"), ("ConditionSwitchID", "23002")]),
        fm(&[("SourceSwitchID", "7301"), ("DestSwitchID", "5001")]),
        fm(&[("SourceSwitchID", "9002"), ("DestSwitchID", "7302"), ("ConditionSwitchID", "23002")]),
        fm(&[("SourceSwitchID", "7302"), ("DestSwitchID", "5002")]),
    ]);
    let uit = synthese_met_tijdslimiet((stops, pipes, linkages, division_inputs, ranks));
    assert_eq!(uit.len(), 1, "precies één register krijgt de rang: {uit:?}");
    let stop = uit[0].get_u32("StopID").unwrap();
    assert!(stop == 1 || stop == 2, "{stop}");
    assert_eq!(uit[0].get_u32("NumberOfMappedDivisionInputNodes"), Some(2));
}

// ---------------------------------------------------------------------------
// E4: de beginstand
// ---------------------------------------------------------------------------

/// LEGT HET HUIDIGE GEDRAG VAST (E4): de beginstand van een gewoon register
/// (`DefaultToEngaged` op zijn schakelaar) wordt niet ingelezen; de lijst met
/// registers is dezelfde met en zonder dat veld. (`DefaultToEngaged` telt
/// alleen voor bijgeluiden en speelgoedregisters, zie `hauptwerk.rs`.) Wie de
/// beginstand van de set later inleest, past deze test met opzet aan.
#[test]
fn beginstand_van_een_gewoon_register_wordt_nu_niet_ingelezen() {
    let basis = xml_met_attacks(&aanval(1, 1, ""));
    let met_switch = |default: &str| {
        basis.replace(
            "<Stop><StopID>1</StopID><Name>Bourdon 8</Name><DivisionID>1</DivisionID></Stop></ObjectList>",
            &format!(
                "<Stop><StopID>1</StopID><Name>Bourdon 8</Name><DivisionID>1</DivisionID><ControllingSwitchID>23001</ControllingSwitchID></Stop></ObjectList>\
                 <ObjectList ObjectType=\"Switch\"><Switch><SwitchID>23001</SwitchID><Name>Bourdon 8</Name><DefaultToEngaged>{default}</DefaultToEngaged></Switch></ObjectList>"
            ),
        )
    };
    let mut pkg = HashMap::new();
    pkg.insert(1u32, PathBuf::from("/pkg"));
    let bouw = |x: &str| build_definition_met(x, PathBuf::from("/root"), &pkg, true, true).expect("build");
    let aan = bouw(&met_switch("Y"));
    let uit = bouw(&met_switch("N"));
    let namen = |d: &OrganDefinition| d.stops.iter().map(|s| (s.name.clone(), s.pipes.len())).collect::<Vec<_>>();
    assert_eq!(namen(&aan), namen(&uit));
    assert!(namen(&aan).iter().any(|(n, _)| n == "Bourdon 8"), "het register blijft gewoon een register");
}

// ---------------------------------------------------------------------------
// E7: een set waarvan de mappen op schijf anders gespeld zijn dan de definitie
// ---------------------------------------------------------------------------

/// Een set die op Windows is gemaakt: de definitie noemt `Bourdon/eerste.wav`,
/// maar op schijf staat `bourdon/EERSTE.wav`, in een map `organinstallationpackages`
/// (kleine letters) met een pakketmarkering in nog een andere spelling. Op Windows
/// en macOS (hoofdletterongevoelig) slaagt dit altijd; op Linux bewijst het de
/// padzoeker (E7) over de hele importroute: pakketroot vinden, pakketmap vinden
/// en het samplepad oplossen.
#[test]
fn set_met_andere_hoofdletters_op_schijf_vindt_pakket_en_sample() {
    let root = std::env::temp_dir().join(format!("jm_e7_integratie_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let pakket = root.join("organinstallationpackages").join("000001");
    std::fs::create_dir_all(pakket.join("bourdon")).unwrap();
    std::fs::write(pakket.join("PackageID000001.INSTALLATIONPACKAGEDEFINITION_Hauptwerk_xml"), b"x").unwrap();
    std::fs::write(pakket.join("bourdon").join("EERSTE.wav"), b"x").unwrap();
    std::fs::create_dir_all(root.join("OrganDefinitions")).unwrap();

    // 1. De pakketroot wordt gevonden vanaf de plek van de definitie.
    let odf = root.join("OrganDefinitions").join("Set.Organ_Hauptwerk_xml");
    let gevonden_root = find_package_root(&odf).expect("pakketroot niet gevonden");
    assert_eq!(gevonden_root, root);

    // 2. De pakketmap wordt gevonden en het pakket herkend aan zijn markering.
    let pkgs = scan_package_dirs(&gevonden_root);
    assert!(pkgs.contains_key(&1), "pakket 1 niet herkend: {pkgs:?}");

    // 3. Het samplepad uit de definitie wijst naar het echte bestand.
    let xml = xml_met_attacks(&aanval(1, 1, ""));
    let def = build_definition_met(&xml, root.clone(), &pkgs, true, true).expect("build");
    let stop = def.stops.iter().find(|s| s.name == "Bourdon 8").expect("register");
    let PipeDef::Sample { path, .. } = &stop.pipes[0] else { panic!("sample") };
    assert!(path.exists(), "samplepad bestaat niet op schijf: {path:?}");
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// E10: ontbrekende pakketten bij een volledige definitie
// ---------------------------------------------------------------------------

fn verslag_voor_pakkettest() -> crate::import_report::ImportReport {
    crate::import_report::ImportReport::uit_definitie("hauptwerk", "4.0", "proef", "Fixture", 1, 1, 0)
}

/// De fixture met een tabel `RequiredInstallationPackage`: pakket 1 (door het
/// sample gebruikt) en pakket 7 (alleen genoemd, bijvoorbeeld beeld of documentatie).
fn xml_met_vereiste_pakketten() -> String {
    xml_met_attacks(&aanval(1, 1, "")).replace(
        "</Hauptwerk>",
        "<ObjectList ObjectType=\"RequiredInstallationPackage\">         <RequiredInstallationPackage><InstallationPackageID>1</InstallationPackageID><Name>Testpakket</Name></RequiredInstallationPackage>         <RequiredInstallationPackage><InstallationPackageID>7</InstallationPackageID><Name>Beeldpakket</Name></RequiredInstallationPackage>         </ObjectList></Hauptwerk>",
    )
}

#[test]
fn aanwezig_samplepakket_geeft_geen_melding() {
    let mut pkg = HashMap::new();
    pkg.insert(1u32, PathBuf::from("/pkg"));
    pkg.insert(7u32, PathBuf::from("/beeld"));
    let mut v = verslag_voor_pakkettest();
    voeg_pakketten_toe(&mut v, &xml_met_vereiste_pakketten(), &pkg);
    assert_eq!(v.pakketten.len(), 2);
    assert!(v.pakketten.iter().all(|p| p.gevonden));
    assert_eq!(v.pakketten.iter().find(|p| p.id == 1).unwrap().naam, "Testpakket");
    assert!(!v.onvolledig && !v.niet_speelbaar);
    assert!(v.meldingen.iter().all(|m| m.object != "pakketten"), "{:?}", v.meldingen);
}

#[test]
fn ontbrekend_samplepakket_maakt_het_verslag_onvolledig_en_noemt_het_met_naam() {
    let mut v = verslag_voor_pakkettest();
    voeg_pakketten_toe(&mut v, &xml_met_vereiste_pakketten(), &HashMap::new());
    let ontbrekend: Vec<u32> = v.ontbrekende_klankpakketten().iter().map(|p| p.id).collect();
    assert_eq!(ontbrekend, vec![1], "alleen het samplepakket is nodig voor de klank");
    assert!(v.onvolledig);
    let melding = v.meldingen.iter().find(|m| m.object == "pakketten").expect("melding over pakketten");
    assert!(melding.tekst.contains("Testpakket (nr. 1)"), "{}", melding.tekst);
    assert!(!melding.tekst.contains("Beeldpakket"), "een ontbrekend beeldpakket telt niet mee: {}", melding.tekst);
}

#[test]
fn zonder_vereistentabel_heten_de_pakketten_naar_hun_nummer() {
    let mut v = verslag_voor_pakkettest();
    voeg_pakketten_toe(&mut v, &xml_met_attacks(&aanval(1, 1, "")), &HashMap::new());
    assert_eq!(v.pakketten.len(), 1);
    assert_eq!(v.pakketten[0].naam, "pakket 1");
    assert!(v.onvolledig);
}

// ---------------------------------------------------------------------------
// E5: filterwaarden van de zwelkast uit de letters van een compacte definitie
// ---------------------------------------------------------------------------

/// Eén `EnclosurePipe`-rij zoals de compacte lezer die oplevert: de letters
/// `c` t/m `h` blijven staan zoals ze in het bestand staan.
fn kastpijp(c: &str, d: &str, e: &str, f: &str, g: &str, h: &str) -> FieldMap {
    let rij: HashMap<String, String> = [("EnclosureID", "7"), ("c", c), ("d", d), ("e", e), ("f", f), ("g", g), ("h", h)]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    FieldMap::rijen_van_tabel("EnclosurePipe", vec![rij]).pop().unwrap()
}

/// Drie pijpen met de waarden van de formaatversie waarvoor de letters zijn
/// vastgelegd (`c` overall, `d` dicht-max, `e` dicht-min, `f` extra, `g` open-max, `h` open-min).
fn drie_kastpijpen() -> Vec<FieldMap> {
    vec![
        kastpijp("6.240", "59.959", "29.979", "3.9445", "269.076", "16.817"),
        kastpijp("6.324", "131.78", "47.69", "4.0", "374.72", "25.71"),
        kastpijp("6.408415", "203.60", "65.39", "4.083", "480.37", "34.61"),
    ]
}

#[test]
fn zwelkast_filter_uit_compacte_pijpen_geeft_de_mediaan() {
    let pijpen = drie_kastpijpen();
    let rijen: Vec<&FieldMap> = pijpen.iter().collect();
    let f = zwelkast_filter_uit_pijpen(&rijen).expect("filter");
    assert_eq!(f.dicht_demping_db, 6.324);
    assert_eq!(f.dicht_extra_db, Some(4.0));
    assert_eq!((f.dicht_min_hz, f.dicht_max_hz), (47.69, 131.78));
    assert_eq!((f.open_min_hz, f.open_max_hz), (25.71, 374.72));
    assert_eq!((f.bron, f.aantal), ("compact", 3));
    assert_eq!(f.diepte_db(), -6.324);
    assert!((f.kantelpunt_hz() - (47.69f32 * 131.78).sqrt()).abs() < 1e-3, "kantelpunt = meetkundig gemiddelde");
}

/// In een andere formaatversie staan de twee frequenties van een paar omgedraaid
/// (`d` = minimum, `e` = maximum): de uitkomst moet hetzelfde zijn.
#[test]
fn zwelkast_filter_met_omgedraaide_frequentiepaar_geeft_dezelfde_uitkomst() {
    let normaal = drie_kastpijpen();
    let omgedraaid = vec![
        kastpijp("6.240", "29.979", "59.959", "3.9445", "16.817", "269.076"),
        kastpijp("6.324", "47.69", "131.78", "4.0", "25.71", "374.72"),
        kastpijp("6.408415", "65.39", "203.60", "4.083", "34.61", "480.37"),
    ];
    let a = zwelkast_filter_uit_pijpen(&normaal.iter().collect::<Vec<_>>()).unwrap();
    let b = zwelkast_filter_uit_pijpen(&omgedraaid.iter().collect::<Vec<_>>()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn zwelkast_filter_vraagt_negentig_procent_volledige_pijpen() {
    // Twaalf pijpen, één mist een letter: 11 van 12 is 91,7 %, dus goed.
    let mut pijpen: Vec<FieldMap> = (0..11).map(|_| kastpijp("6.3", "130", "47", "4.0", "370", "25")).collect();
    pijpen.push(kastpijp("6.3", "130", "47", "4.0", "370", ""));
    assert!(zwelkast_filter_uit_pijpen(&pijpen.iter().collect::<Vec<_>>()).is_some());
    // Twee van de twaalf mist er een: 83 %, dus niet.
    pijpen.pop();
    pijpen.pop();
    pijpen.push(kastpijp("6.3", "130", "47", "4.0", "370", ""));
    pijpen.push(kastpijp("6.3", "130", "47", "4.0", "370", "n.v.t."));
    assert!(zwelkast_filter_uit_pijpen(&pijpen.iter().collect::<Vec<_>>()).is_none());
    // Een leesbare definitie heeft de letters helemaal niet.
    let leeg: Vec<HashMap<String, String>> = vec![[("EnclosureID".to_string(), "7".to_string())].into_iter().collect()];
    let leesbaar = FieldMap::rijen_van_tabel("EnclosurePipe", leeg);
    assert!(zwelkast_filter_uit_pijpen(&leesbaar.iter().collect::<Vec<_>>()).is_none());
    assert!(zwelkast_filter_uit_pijpen(&[]).is_none());
}

#[test]
fn zwelkast_filter_wijst_ongeloofwaardige_waarden_af() {
    let kies = |c: &str, d: &str, e: &str, g: &str, h: &str| {
        let p = vec![kastpijp(c, d, e, "4.0", g, h)];
        zwelkast_filter_uit_pijpen(&p.iter().collect::<Vec<_>>())
    };
    assert!(kies("6.3", "130", "47", "370", "25").is_some(), "basis klopt");
    assert!(kies("70", "130", "47", "370", "25").is_none(), "meer dan 60 dB demping");
    assert!(kies("-1", "130", "47", "370", "25").is_none(), "negatieve demping");
    assert!(kies("6.3", "130", "130", "370", "25").is_none(), "minimum gelijk aan maximum (dicht)");
    assert!(kies("6.3", "130", "47", "370", "370").is_none(), "minimum gelijk aan maximum (open)");
    assert!(kies("6.3", "0", "47", "370", "25").is_none(), "frequentie onder 1 Hz");
    assert!(kies("6.3", "130", "47", "370", "NaN").is_none(), "geen getal");
}

/// Een leesbare definitie noemt de waarden bij hun naam (zo staan ze in een van de
/// echte sets); de "Max" is daar de laagste frequentie. De uitkomst is dezelfde
/// soort filter, met de bron "definitie".
#[test]
fn zwelkast_filter_uit_leesbare_veldnamen() {
    let rij = |c: &str, max_d: &str, min_d: &str, extra: &str, max_o: &str, min_o: &str| -> FieldMap {
        let r: HashMap<String, String> = [
            ("EnclosureID", "1"),
            ("FiltParamWhenClsd_OverallAttnDb", c),
            ("FiltParamWhenClsd_MaxFreqHz", max_d),
            ("FiltParamWhenClsd_MinFreqHz", min_d),
            ("FiltParamWhenClsd_ExtraAttnAtMinDb", extra),
            ("FiltParamWhenOpen_MaxFreqHz", max_o),
            ("FiltParamWhenOpen_MinFreqHz", min_o),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        FieldMap::rijen_van_tabel("EnclosurePipe", vec![r]).pop().unwrap()
    };
    let pijpen = vec![
        rij("7", "65.4064", "457.845", "6", "260.626", "654.064"),
        rij("7", "65.4064", "457.845", "6", "260.626", "654.064"),
        rij("8", "130.8128", "915.69", "6", "521.252", "1308.128"),
    ];
    let f = zwelkast_filter_uit_pijpen(&pijpen.iter().collect::<Vec<_>>()).expect("filter");
    assert_eq!(f.bron, "definitie");
    assert_eq!(f.dicht_demping_db, 7.0);
    assert_eq!(f.dicht_extra_db, Some(6.0));
    assert_eq!((f.dicht_min_hz, f.dicht_max_hz), (65.4064, 457.845), "het kleinste van het paar is het minimum");
    assert_eq!((f.open_min_hz, f.open_max_hz), (260.626, 654.064));
    // Ontbreekt een veld in de meeste rijen, dan geen filter.
    let kale = HashMap::from([("EnclosureID".to_string(), "1".to_string()), ("FiltParamWhenClsd_OverallAttnDb".to_string(), "7".to_string())]);
    let kaal = FieldMap::rijen_van_tabel("EnclosurePipe", vec![kale]).pop().unwrap();
    assert!(zwelkast_filter_uit_pijpen(&[&kaal]).is_none());
}

// ---------------------------------------------------------------------------
// E4: de crescendo van de set uit de tabellen van een leesbare definitie
// ---------------------------------------------------------------------------

/// Een kleine leesbare definitie: drie registers (schakelaars 1801 tot 1803; register 3 via een
/// tussenschakelaar 500), een koppelschakelaar (51) zonder register, een crescendo (regelaar 2) met
/// drie trappen in verkeerde volgorde, een zwelregelaar (regelaar 1) met trapschakelaars die niets
/// met een combinatie te maken hebben, en een gewone piston (type 1).
fn crescendo_xml(extra_element: &str, kan_uitzetten: &str) -> String {
    let combinatie = |id: u32, naam: &str, sw: u32| format!(
        "<Combination><CombinationID>{id}</CombinationID><Name>{naam}</Name><CombinationTypeCode>4</CombinationTypeCode>\
         <ActivatingSwitchID>{sw}</ActivatingSwitchID><CanEngageControlledSwitches>Y</CanEngageControlledSwitches>\
         <CanDisengageControlledSwitches>{kan_uitzetten}</CanDisengageControlledSwitches></Combination>"
    );
    let element = |comb: u32, sw: u32, aan: &str| format!(
        "<CombinationElement><CombinationID>{comb}</CombinationID><ControlledSwitchID>{sw}</ControlledSwitchID>\
         <InitialStoredStateIsEngaged>{aan}</InitialStoredStateIsEngaged><InvertStoredStateWhenActivating>N</InvertStoredStateWhenActivating></CombinationElement>"
    );
    let trap = |waarde: u32, sw: u32| format!(
        "<ContinuousControlStageSwitch><ContinuousControlID>2</ContinuousControlID><ContinuousControlValue>{waarde}</ContinuousControlValue><ControlledSwitchID>{sw}</ControlledSwitchID></ContinuousControlStageSwitch>"
    );
    format!(
        r#"<Hauptwerk>
<ObjectList ObjectType="Stop"><Stop><StopID>1</StopID><ControllingSwitchID>1801</ControllingSwitchID></Stop><Stop><StopID>2</StopID><ControllingSwitchID>1802</ControllingSwitchID></Stop><Stop><StopID>3</StopID><ControllingSwitchID>1803</ControllingSwitchID></Stop></ObjectList>
<ObjectList ObjectType="SwitchLinkage"><SwitchLinkage><SourceSwitchID>11</SourceSwitchID><DestSwitchID>1801</DestSwitchID></SwitchLinkage><SwitchLinkage><SourceSwitchID>12</SourceSwitchID><DestSwitchID>1802</DestSwitchID></SwitchLinkage><SwitchLinkage><SourceSwitchID>13</SourceSwitchID><DestSwitchID>500</DestSwitchID></SwitchLinkage><SwitchLinkage><SourceSwitchID>500</SourceSwitchID><DestSwitchID>1803</DestSwitchID></SwitchLinkage></ObjectList>
<ObjectList ObjectType="ContinuousControl"><ContinuousControl><ControlID>1</ControlID><Name>Zwel</Name></ContinuousControl><ContinuousControl><ControlID>2</ControlID><Name>Walze</Name></ContinuousControl></ObjectList>
<ObjectList ObjectType="Combination">{c1}{c2}{c3}<Combination><CombinationID>201</CombinationID><Name>Tutti</Name><CombinationTypeCode>1</CombinationTypeCode><ActivatingSwitchID>68</ActivatingSwitchID></Combination></ObjectList>
<ObjectList ObjectType="CombinationElement">{e1}{e2}{e2b}{e2c}{e3a}{e3b}{e3c}{e3d}{extra}</ObjectList>
<ObjectList ObjectType="ContinuousControlStageSwitch">{t3}{t1}{t2}<ContinuousControlStageSwitch><ContinuousControlID>1</ContinuousControlID><ContinuousControlValue>10</ContinuousControlValue><ControlledSwitchID>90</ControlledSwitchID></ContinuousControlStageSwitch></ObjectList>
</Hauptwerk>"#,
        c1 = combinatie(101, "Trap 1", 2001), c2 = combinatie(102, "Trap 2", 2002), c3 = combinatie(103, "Trap 3", 2003),
        e1 = element(101, 11, "Y"), e2 = element(102, 11, "Y"), e2b = element(102, 12, "Y"), e2c = element(102, 51, "Y"),
        e3a = element(103, 11, "Y"), e3b = element(103, 12, "Y"), e3c = element(103, 13, "Y"), e3d = element(103, 51, "Y"),
        extra = extra_element,
        t1 = trap(0, 2001), t2 = trap(40, 2002), t3 = trap(90, 2003),
    )
}

fn crescendo_uit(xml: &str) -> Option<crate::import_report::SetCrescendo> {
    crescendo_uit_tabellen(
        &extract_objects(xml, "Combination"),
        &extract_objects(xml, "CombinationElement"),
        &extract_objects(xml, "ContinuousControlStageSwitch"),
        &extract_objects(xml, "Stop"),
        &extract_objects(xml, "SwitchLinkage"),
        &extract_objects(xml, "ContinuousControl"),
    )
}

#[test]
fn crescendo_van_de_set_staat_op_volgorde_van_de_regelaar_met_absolute_registers() {
    let c = crescendo_uit(&crescendo_xml("", "Y")).expect("een crescendo");
    assert_eq!(c.naam, "Walze", "de naam van de regelaar met de trappen");
    // Ook al staan de trapschakelaars in het bestand in verkeerde volgorde.
    assert_eq!(c.trappen.iter().map(|t| t.waarde).collect::<Vec<_>>(), vec![0, 40, 90]);
    assert_eq!(c.trappen.iter().map(|t| t.naam.as_str()).collect::<Vec<_>>(), vec!["Trap 1", "Trap 2", "Trap 3"]);
    // Register 3 hangt via de tussenschakelaar 500 aan schakelaar 13.
    assert_eq!(c.trappen[0].stops, vec![1]);
    assert_eq!(c.trappen[1].stops, vec![1, 2]);
    assert_eq!(c.trappen[2].stops, vec![1, 2, 3]);
    // Schakelaar 51 is een koppel: niet overgenomen, één keer geteld ondanks twee trappen.
    assert_eq!(c.niet_gelezen, 1);
}

#[test]
fn een_element_met_omgekeerde_beginstand_wordt_niet_geraden() {
    let omgekeerd = "<CombinationElement><CombinationID>101</CombinationID><ControlledSwitchID>12</ControlledSwitchID>\
        <InitialStoredStateIsEngaged>Y</InitialStoredStateIsEngaged><InvertStoredStateWhenActivating>Y</InvertStoredStateWhenActivating></CombinationElement>";
    let c = crescendo_uit(&crescendo_xml(omgekeerd, "Y")).unwrap();
    assert_eq!(c.trappen[0].stops, vec![1], "register 2 komt niet in trap 1 door het omgekeerde element");
    assert_eq!(c.niet_gelezen, 2, "het omgekeerde element telt mee als niet gelezen (schakelaar 12), naast het koppel");
}

#[test]
fn een_combinatie_die_niet_mag_uitzetten_geeft_geen_crescendo() {
    assert!(crescendo_uit(&crescendo_xml("", "N")).is_none());
}

#[test]
fn zonder_crescendotrappen_of_met_maar_een_trap_geen_crescendo() {
    // Geen combinatie van het trapsoort: gewoon pistons.
    let zonder = crescendo_xml("", "Y").replace("<CombinationTypeCode>4</CombinationTypeCode>", "<CombinationTypeCode>1</CombinationTypeCode>");
    assert!(crescendo_uit(&zonder).is_none());
    // Maar één trap is geen crescendo.
    let een = crescendo_xml("", "Y")
        .replace("<ControlledSwitchID>2002</ControlledSwitchID>", "<ControlledSwitchID>7</ControlledSwitchID>")
        .replace("<ControlledSwitchID>2003</ControlledSwitchID>", "<ControlledSwitchID>8</ControlledSwitchID>");
    assert!(crescendo_uit(&een).is_none());
    assert!(crescendo_uit(&xml_met_attacks(&aanval(1, 1, ""))).is_none(), "een definitie zonder deze tabellen");
}

#[test]
fn het_verslag_meldt_de_crescendo_van_de_set() {
    let mut v = crate::import_report::ImportReport::uit_definitie("hauptwerk", "4.0", "proef", "Fixture", 3, 1, 0);
    voeg_crescendo_toe(&mut v, &crescendo_xml("", "Y"));
    let c = v.set_crescendo.as_ref().expect("crescendo in het verslag");
    assert_eq!(c.trappen.len(), 3);
    let m = v.meldingen.iter().find(|m| m.object == "crescendo").expect("melding");
    assert!(m.tekst.contains("3 trappen van 1 tot 3 registers"), "{}", m.tekst);
    assert!(m.tekst.contains("1 koppels of tremulanten"), "{}", m.tekst);
}
