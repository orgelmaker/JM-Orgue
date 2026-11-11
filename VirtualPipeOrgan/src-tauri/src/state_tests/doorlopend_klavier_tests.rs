use super::{pijp_voor_noot, pijp_voor_noot_richting, naam_zonder_helft, markeer_registerhelften, Registerhelft, DOORLOPEND_MAX_VERSCHUIVING};
use crate::commands::StopDto;

// Een register van C2 (36) t/m G6 (91): 56 tonen, het gangbare bereik van
// een historische sampleset.
const EERSTE: u32 = 36;
const LAATSTE: u32 = 91;

#[test]
fn binnen_het_bereik_verandert_er_niets() {
    for n in EERSTE..=LAATSTE {
        assert_eq!(pijp_voor_noot(n, EERSTE, LAATSTE, false), Some((n - EERSTE + 1, 0)));
        assert_eq!(pijp_voor_noot(n, EERSTE, LAATSTE, true), Some((n - EERSTE + 1, 0)));
    }
}

#[test]
fn uit_zwijgen_toetsen_buiten_het_bereik() {
    assert_eq!(pijp_voor_noot(35, EERSTE, LAATSTE, false), None);
    assert_eq!(pijp_voor_noot(96, EERSTE, LAATSTE, false), None);
}

#[test]
fn aan_neemt_de_dichtstbijzijnde_pijp_op_de_echte_toonhoogte() {
    // Vijf toetsen boven het bereik → de hoogste pijp, vijf halve tonen
    // omhoog verschoven (tot 0.8.3 klonk hier de pijp een octaaf lager).
    assert_eq!(pijp_voor_noot(96, EERSTE, LAATSTE, true), Some((LAATSTE - EERSTE + 1, 5)));
    // Een octaaf onder het bereik → de laagste pijp, een octaaf omlaag.
    assert_eq!(pijp_voor_noot(24, EERSTE, LAATSTE, true), Some((1, -12)));
    // Eén halve toon erbuiten: minimale verschuiving.
    assert_eq!(pijp_voor_noot(92, EERSTE, LAATSTE, true), Some((56, 1)));
    assert_eq!(pijp_voor_noot(35, EERSTE, LAATSTE, true), Some((1, -1)));
}

#[test]
fn de_verschuiving_klopt_met_de_gespeelde_toon() {
    // Pijpnoot + verschuiving = gespeelde noot, voor elke toets die klinkt.
    for n in 0..=127u32 {
        if let Some((pijp, v)) = pijp_voor_noot(n, EERSTE, LAATSTE, true) {
            let pijpnoot = (EERSTE + pijp - 1) as i32;
            assert_eq!(pijpnoot + v as i32, n as i32, "toets {} → pijp {} met {}", n, pijp, v);
            assert!(v.abs() as i32 <= DOORLOPEND_MAX_VERSCHUIVING);
        }
    }
}

#[test]
fn ver_buiten_het_bereik_zwijgt_de_toets() {
    // Tot twee octaven verschuiven, daarboven stil (geen chipmunk-rate).
    let grens = LAATSTE + DOORLOPEND_MAX_VERSCHUIVING as u32;
    assert!(pijp_voor_noot(grens, EERSTE, LAATSTE, true).is_some());
    assert_eq!(pijp_voor_noot(grens + 1, EERSTE, LAATSTE, true), None);
    assert_eq!(pijp_voor_noot(127, EERSTE, LAATSTE, true), None);
    assert_eq!(pijp_voor_noot(0, EERSTE, LAATSTE, true), None);
}

#[test]
fn elke_toets_van_het_klavier_geeft_geluid() {
    // De hele reden van de functie: op een klavier van 61 toetsen (36..96)
    // valt met het doorlopende klavier geen enkel gat meer.
    for n in 36..=96u32 {
        assert!(pijp_voor_noot(n, EERSTE, LAATSTE, true).is_some(), "toets {} zwijgt", n);
    }
}

#[test]
fn een_leeg_of_omgekeerd_bereik_geeft_niets() {
    assert_eq!(pijp_voor_noot(60, 80, 40, true), None);
}

#[test]
fn een_bereik_van_minder_dan_een_octaaf_loopt_niet_vast() {
    // Een register van vijf tonen (60..64), zoals een losse discanthelft.
    assert_eq!(pijp_voor_noot(60, 60, 64, true), Some((1, 0)));
    assert_eq!(pijp_voor_noot(72, 60, 64, true), Some((5, 8)));
    assert_eq!(pijp_voor_noot(67, 60, 64, true), Some((5, 3)));
    assert_eq!(pijp_voor_noot(55, 60, 64, true), Some((1, -5)));
    for n in 0..=127u32 { let _ = pijp_voor_noot(n, 60, 64, true); }
}

// ---- 0.9.12: bas- en discanthelften stoppen op het deelpunt ----

fn dto(name: &str, pitch: &str, eerste: u8, laatste: u8) -> StopDto {
    StopDto {
        id: name.to_string(), name: name.to_string(), pitch: pitch.to_string(), drawn: false,
        groep: "grond", familie: "principaal", has_tremulant: false, midi_action_code: 0, internal_stop_id: 1,
        first_midi_note: eerste, last_midi_note: laatste, is_reed: false, koren: None, mengwerk: false,
        wind_voet: 8.0, percussief: false, doorloop_onder: 0, doorloop_boven: 127,
    }
}
fn grenzen(s: &StopDto) -> (u8, u8) { (s.doorloop_onder, s.doorloop_boven) }

#[test]
fn richting_per_kant() {
    // Bashelft C2..B3 (36..59), grens boven = 59: boven het deelpunt stil, onderin door.
    assert_eq!(pijp_voor_noot_richting(60, 36, 59, 0, 59), None);
    assert_eq!(pijp_voor_noot_richting(72, 36, 59, 0, 59), None);
    assert_eq!(pijp_voor_noot_richting(35, 36, 59, 0, 59), Some((1, -1)));
    // Discanthelft c4..g6 (60..91), grens onder = 60: onder het deelpunt stil, bovenin door.
    assert_eq!(pijp_voor_noot_richting(59, 60, 91, 60, 127), None);
    assert_eq!(pijp_voor_noot_richting(48, 60, 91, 60, 127), None);
    assert_eq!(pijp_voor_noot_richting(96, 60, 91, 60, 127), Some((32, 5)));
    // Binnen het bereik spelen de grenzen geen rol.
    assert_eq!(pijp_voor_noot_richting(70, 60, 91, 60, 91), Some((11, 0)));
    assert_eq!(pijp_voor_noot_richting(36, 36, 59, 36, 59), Some((1, 0)));
    // Vrij (0..127) = het oude gedrag; eigen bereik als grens = schakelaar uit.
    for n in 0..=127u32 {
        assert_eq!(pijp_voor_noot_richting(n, EERSTE, LAATSTE, 0, 127), pijp_voor_noot(n, EERSTE, LAATSTE, true));
        assert_eq!(pijp_voor_noot_richting(n, EERSTE, LAATSTE, EERSTE, LAATSTE), pijp_voor_noot(n, EERSTE, LAATSTE, false));
    }
}

#[test]
fn helft_uit_de_naam() {
    use Registerhelft::*;
    let gevallen: &[(&str, &str, Option<Registerhelft>)] = &[
        ("Trompet (Bas)", "trompet", Some(Bas)),
        ("Trompet (Disc)", "trompet", Some(Discant)),
        ("Trompet B", "trompet", Some(Bas)),
        ("Trompet D", "trompet", Some(Discant)),
        ("Cornet V D", "cornet v", Some(Discant)),
        ("Mixtuur IV (Bas)", "mixtuur iv", Some(Bas)),
        ("Flûte dessus", "flûte", Some(Discant)),
        ("Trompette basse", "trompette", Some(Bas)),
        ("Posaune Diskant", "posaune", Some(Discant)),
        ("Trompete 8 Baß", "trompete", Some(Bas)),
        ("Oboe Treble", "oboe", Some(Discant)),
        ("Sesquialter disc.", "sesquialter", Some(Discant)),
        ("Vox humana 8 Bass", "vox humana", Some(Bas)),
        ("Vox humana Desc.", "vox humana", Some(Discant)),
        ("Cornet D 8", "cornet", Some(Discant)),
        ("Principale Bassi", "principale", Some(Bas)),
        ("Trompeta real Tiples", "trompeta real", Some(Discant)),
        ("Cornet MD", "cornet", Some(Discant)),
        ("Basse de Trompette", "trompette", Some(Bas)),
        ("Dessus de Cornet", "cornet", Some(Discant)),
        ("Trompet B D", "trompet", None),
        ("Prestant", "prestant", None),
        ("Prestant 8", "prestant 8", None),
        ("Subbas", "subbas", None),
        ("Bourdon", "bourdon", None),
        ("Mixtuur B/D", "mixtuur b/d", None),
        ("Bass Flute", "bass flute", None),
        ("B", "b", None),
    ];
    for (naam, basis, helft) in gevallen {
        assert_eq!(naam_zonder_helft(naam), (basis.to_string(), *helft), "{}", naam);
    }
}

#[test]
fn bas_en_discant_lopen_niet_door_op_het_deelpunt() {
    let mut stops = vec![
        dto("Trompet (Bas)", "8", 36, 59),
        dto("Trompet (Disc)", "8", 60, 96),
        dto("Prestant", "8", 36, 96),
    ];
    markeer_registerhelften(&mut stops, false);
    assert_eq!(grenzen(&stops[0]), (0, 59), "bashelft: onderin door, niet over het deelpunt");
    assert_eq!(grenzen(&stops[1]), (60, 127), "discanthelft: bovenin door, niet onder het deelpunt");
    assert_eq!(grenzen(&stops[2]), (0, 127), "heel register: beide kanten");
}

#[test]
fn helften_zonder_aanduiding_herkend_aan_het_paar() {
    let mut stops = vec![dto("Trompet", "8", 36, 59), dto("Trompet", "8", 60, 96), dto("Trompet", "4", 36, 96)];
    markeer_registerhelften(&mut stops, false);
    assert_eq!(grenzen(&stops[0]), (0, 59));
    assert_eq!(grenzen(&stops[1]), (60, 127));
    // Andere voetmaat is een ander register: ongemoeid.
    assert_eq!(grenzen(&stops[2]), (0, 127));
}

#[test]
fn gat_tussen_de_helften_valt_aan_de_bas_toe() {
    // Bas tot a (57), discant vanaf c' (60): a# en b klinken via de bas, niet dubbel.
    let mut stops = vec![dto("Trompet (Bas)", "8", 36, 57), dto("Trompet (Disc)", "8", 60, 96)];
    markeer_registerhelften(&mut stops, false);
    assert_eq!(grenzen(&stops[0]), (0, 59));
    assert_eq!(grenzen(&stops[1]), (60, 127));
    assert_eq!(pijp_voor_noot_richting(58, 36, 57, 0, 59), Some((22, 1)));
    assert_eq!(pijp_voor_noot_richting(59, 36, 57, 0, 59), Some((22, 2)));
    assert_eq!(pijp_voor_noot_richting(60, 36, 57, 0, 59), None);
    assert_eq!(pijp_voor_noot_richting(59, 60, 96, 60, 127), None);
}

#[test]
fn zwevende_stem_vanaf_g_blijft_aan_beide_kanten_doorlopen() {
    // 0.8.4: korter bereik, geen helft, geen partner, begint onder c' → vrij.
    let mut stops = vec![dto("Vox Coelestis", "8", 55, 91), dto("Prestant", "8", 36, 91)];
    markeer_registerhelften(&mut stops, false);
    assert_eq!(grenzen(&stops[0]), (0, 127));
    assert_eq!(grenzen(&stops[1]), (0, 127));
}

#[test]
fn losse_discanthelft_zonder_partner_zwijgt_in_de_bas() {
    let mut stops = vec![dto("Cornet V D", "", 60, 96)];
    markeer_registerhelften(&mut stops, false);
    assert_eq!(grenzen(&stops[0]), (60, 127));
}

#[test]
fn discant_zonder_aanduiding_herkend_aan_het_bereik() {
    // "RW Cornet" 60..91 naast registers vanaf 36: discant, ook zonder "D".
    let mut stops = vec![dto("Cornet", "", 60, 91), dto("Prestant", "8", 36, 91)];
    markeer_registerhelften(&mut stops, false);
    assert_eq!(grenzen(&stops[0]), (60, 127));
    assert_eq!(grenzen(&stops[1]), (0, 127));
    // Alleen: het klavierbereik is onbekend → vrij.
    let mut alleen = vec![dto("Cornet", "", 60, 91)];
    markeer_registerhelften(&mut alleen, false);
    assert_eq!(grenzen(&alleen[0]), (0, 127));
}

#[test]
fn bas_zonder_aanduiding_herkend_aan_het_bereik() {
    let mut stops = vec![dto("Fagot", "16", 36, 59), dto("Prestant", "8", 36, 91)];
    markeer_registerhelften(&mut stops, false);
    assert_eq!(grenzen(&stops[0]), (0, 59));
    assert_eq!(grenzen(&stops[1]), (0, 127));
}

#[test]
fn pedaal_kent_geen_helften() {
    // "Sub Bass" en "Choral Bass" eindigen op Bass maar zijn hele registers;
    // een korte pedaalset (30 toetsen) moet bovenin blijven doorlopen.
    let mut stops = vec![dto("Sub Bass", "16", 36, 65), dto("Choral Bass", "4", 36, 65), dto("Posaune", "16", 36, 67)];
    markeer_registerhelften(&mut stops, true);
    for s in &stops { assert_eq!(grenzen(s), (0, 127), "{}", s.name); }
}

#[test]
fn grenzen_worden_opnieuw_gezet_bij_herhaald_markeren() {
    let mut stops = vec![dto("Prestant", "8", 36, 96)];
    stops[0].doorloop_onder = 40;
    markeer_registerhelften(&mut stops, false);
    assert_eq!(grenzen(&stops[0]), (0, 127));
}
