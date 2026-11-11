use super::*;
fn k(min: u8, max: u8) -> Option<Koren> { Some(Koren { min, max }) }

#[test]
fn vastgeplakt_voetwoord_geeft_de_voetmaat_uit_de_naam() {
    // Een set die elke rang als 8' opgeeft maar "16FT" in de naam schrijft.
    assert_eq!(registerweergave("BOURDON 16FT", false, 8, None),
        ("BOURDON".into(), "16'".into(), None));
    assert_eq!(registerweergave("OPEN DIAPASON 8FT", false, 8, None),
        ("OPEN DIAPASON".into(), "8'".into(), None));
    // Voetmaat midden in de naam, met een toelichting erachter.
    // Samengestelde voetmaat midden in de naam: hele 1 3/5', niet alleen 3/5'
    // (regressie 0.9.33).
    assert_eq!(registerweergave("Ters 1 3/5' Bas", false, 8, None),
        ("Ters Bas".to_string(), "1 3/5'".to_string(), None));
    assert_eq!(registerweergave("Principal 8' Disk.", false, 8, None),
        ("Principal Disk.".to_string(), "8'".to_string(), None));
    assert_eq!(registerweergave("BOURDON 16FT TONE", false, 8, None),
        ("BOURDON TONE".into(), "16'".into(), None));
}

#[test]
fn kale_voetmaat_harmonium_en_klavecimbel() {
    // Kale voetmaat achteraan, set geeft overal 8'.
    assert_eq!(registerweergave("FIFTEENTH 2", false, 8, None),
        ("FIFTEENTH".into(), "2'".into(), None));
    assert_eq!(registerweergave("Nasard 2 2/3", false, 8, None),
        ("Nasard".into(), "2 2/3'".into(), None));
    // Klopt de kale voetmaat met de set, dan is het de bekende strip.
    assert_eq!(registerweergave("Gedekt 8", false, 8, None),
        ("Gedekt".into(), "8'".into(), None));
    // Klein verschil (historische schrijfwijze): de set wint, zoals bij voetteken.
    assert_eq!(registerweergave("Quint 3", false, 24, None),
        ("Quint 3".into(), "2 2/3'".into(), None));
    // Mengwerk: kaal getal is geen voetmaat.
    assert_eq!(registerweergave("Mixtur 4", true, 8, None).1, "");
    // Geen gangbare voetmaat → blijft staan.
    assert_eq!(registerweergave("Trompet 3", false, 8, None),
        ("Trompet 3".into(), "8'".into(), None));
    // Harmonium: "P." = pieds; het registernummer vooraan blijft.
    assert_eq!(registerweergave("2 BOURDON 16 P.", false, 8, None),
        ("2 BOURDON".into(), "16'".into(), None));
    assert_eq!(registerweergave("7 CROMORNE 32 P.", false, 8, None),
        ("7 CROMORNE".into(), "32'".into(), None));
    // Klavecimbel: naam is alleen een voetmaat.
    assert_eq!(registerweergave("4", false, 8, None),
        ("4'".into(), "4'".into(), None));
    assert_eq!(registerweergave("I 8'", false, 8, None),
        ("I".into(), "8'".into(), None));
}

#[test]
fn voetmaat_en_aantal_uit_naam_en_set() {
    // Voetmaat met voetteken in de naam.
    assert_eq!(registerweergave("Mixtur major 4-5f. 2 2/3'", true, 8, None),
        ("Mixtur major".into(), "2 2/3'".into(), k(4, 5)));
    // Kale voetmaat achter het aantal: die van de set, en weg uit de naam.
    assert_eq!(registerweergave("Mixtur 4fach 2", true, 32, None),
        ("Mixtur".into(), "2'".into(), k(4, 4)));
    assert_eq!(registerweergave("Sesquialter II 2 2/3", true, 24, None),
        ("Sesquialter".into(), "2 2/3'".into(), k(2, 2)));
    // Mengwerk met aantal en zonder voetmaat in de naam: alleen het aantal.
    assert_eq!(registerweergave("Mixtur IV", true, 32, None),
        ("Mixtur".into(), String::new(), k(4, 4)));
    // Geen mengwerk, wel een markering: de voetmaat van de set blijft.
    assert_eq!(registerweergave("Prestant 2 st.", false, 8, None),
        ("Prestant".into(), "8'".into(), k(2, 2)));
}

/// Een definitie die elke rang als 8' noteert terwijl de naam "16'"
/// zegt: het label van de naam wint; een kale of ontbrekende voetmaat in
/// de naam laat de afgeleide staan.
#[test]
fn voetmaat_met_voetteken_in_naam_wint() {
    assert_eq!(registerweergave("Contrabass 16'", false, 8, None),
        ("Contrabass".into(), "16'".into(), None));
    assert_eq!(registerweergave("Quinta 5 1/3'", false, 8, None),
        ("Quinta".into(), "5 1/3'".into(), None));
    assert_eq!(registerweergave("Subbaß 16 Fuß", false, 4, None),
        ("Subbaß".into(), "16'".into(), None));
    assert_eq!(registerweergave("Principal", false, 8, None),
        ("Principal".into(), "8'".into(), None));
    assert_eq!(registerweergave("Gedekt 8", false, 8, None),
        ("Gedekt".into(), "8'".into(), None));
    // Vastgeplakte voetmaat gaat altijd weg, ook als hij afwijkt.
    assert_eq!(registerweergave("Bourdon8", false, 4, None),
        ("Bourdon".into(), "16'".into(), None));
    assert_eq!(registerweergave("Principal8", false, 8, None),
        ("Principal".into(), "8'".into(), None));
    assert_eq!(registerweergave("Cornet5", false, 8, None),
        ("Cornet5".into(), "8'".into(), None));
    // Geen octaafverschil: historische schrijfwijze, de afgeleide blijft.
    assert_eq!(registerweergave("Quint 3'", false, 24, None),
        ("Quint 3'".into(), "2 2/3'".into(), None));
}

#[test]
fn unison_koppel_zonder_8() {
    assert_eq!(zonder_unison_voetmaat("Grt. Ped.8"), "Grt. Ped.");
    assert_eq!(zonder_unison_voetmaat("Sw. Grt.8"), "Sw. Grt.");
    assert_eq!(zonder_unison_voetmaat("II/I 8'"), "II/I");
    assert_eq!(zonder_unison_voetmaat("Koppel 18"), "Koppel 18");
    assert_eq!(zonder_unison_voetmaat("I/P"), "I/P");
    assert_eq!(zonder_unison_voetmaat("8"), "8");
}

#[test]
fn manifest_en_zonder_aantal() {
    assert_eq!(registerweergave("Mixtuur", true, 8, k(6, 6)),
        ("Mixtuur".into(), String::new(), k(6, 6)));
    // Geen aantal, standaard voetmaat: blijft zoals het was.
    // 0.8.71: een mengwerk zonder bekende sterkte toont geen voetmaat.
    assert_eq!(registerweergave("Mixtuur", true, 32, None),
        ("Mixtuur".into(), String::new(), None));
    // Toevalswaarde van halverwege een repetering: geen voetmaat.
    assert_eq!(registerweergave("Cymbel", true, 336, None),
        ("Cymbel".into(), String::new(), None));
    // Gewoon register: ongewijzigd, voetmaat uit de naam gehaald.
    assert_eq!(registerweergave("Gedekt 8", false, 8, None),
        ("Gedekt".into(), "8'".into(), None));
    // Manifest met een echte voetmaat in de set (geen JM-Rec-terugval 8').
    assert_eq!(registerweergave("Sesquialter", true, 24, k(2, 2)),
        ("Sesquialter".into(), "2 2/3'".into(), k(2, 2)));
}

fn manifest(json: &str) -> JmRecManifest {
    serde_json::from_str(json).expect("manifest moet altijd te lezen zijn")
}

#[test]
fn manifest_oud_formaat_houdt_de_naam() {
    // Kale klaviernamen (oudere JM-Rec): de naamregel mag niet wegvallen.
    let m = manifest(r#"{"kerk":"St. Jan","bouwer":"B","keyboards":["Hoofdwerk"]}"#);
    assert_eq!(m.kerk, "St. Jan");
    assert!(jm_rec_koren_van(&m).is_empty());
}

#[test]
fn manifest_per_klavier_en_submappen() {
    let m = manifest(r#"{
            "keyboards": [
                {"name": "Hoofdwerk", "registers": [{"name": "Mixtuur", "foot": "4-6st"}, {"name": "Cornet", "foot": "IV"}]},
                {"name": "Rugwerk", "registers": [{"name": "Mixtuur", "foot": "3st"}]}
            ],
            "pedal_registers": [{"name": "Mixtuur", "foot": ""}]
        }"#);
    let kaart = jm_rec_koren_van(&m);
    let pad = |p: &str| manifest_koren_voor_pad(Path::new(p), &kaart);
    // Zelfde naam op twee klavieren: elk zijn eigen aantal.
    assert_eq!(pad(r"Hoofdwerk\Mixtuur\036-c.wav"), Some(Koren { min: 4, max: 6 }));
    assert_eq!(pad("D:/Sets/X/Rugwerk/Mixtuur/036-c.wav"), Some(Koren { min: 3, max: 3 }));
    // Het pedaal heeft geen aantal: niet dat van een manuaal lenen.
    assert_eq!(pad(r"Pedaal\Mixtuur\036-c.wav"), None);
    // Microfoon-submap en bas/discant-map.
    assert_eq!(pad(r"Hoofdwerk\Mixtuur\Front\036-c.wav"), Some(Koren { min: 4, max: 6 }));
    assert_eq!(pad(r"Hoofdwerk\Mixtuur\Mixtuur_bas\036-c.wav"), Some(Koren { min: 4, max: 6 }));
    // Romeins in het voetveld telt ook; de naam staat maar op één klavier.
    assert_eq!(pad(r"Elders\Cornet\060-c.wav"), Some(Koren { min: 4, max: 4 }));
}
