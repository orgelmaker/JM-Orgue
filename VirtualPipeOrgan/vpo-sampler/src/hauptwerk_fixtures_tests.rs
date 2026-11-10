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
