use super::{
    divisie_heeft_tremulant_opnamen, nagebootste_tremulant_voor_herstel,
    tremulant_kind_voor_divisie, DivisionDto, StopDto,
};
use crate::library::DivisionTremulantSaved;

fn stop(id: &str, trem: bool) -> StopDto {
    StopDto {
        id: id.to_string(), name: id.to_string(), pitch: "8".to_string(), drawn: false,
        groep: "grond", familie: "principaal", has_tremulant: trem, midi_action_code: 0, internal_stop_id: 1,
        first_midi_note: 36, last_midi_note: 96, is_reed: false, koren: None, mengwerk: false, wind_voet: 8.0, percussief: false,
        doorloop_onder: 0, doorloop_boven: 127,
    }
}

fn divisie(naam: &str, kind: Option<&str>, stop_trem: bool) -> DivisionDto {
    DivisionDto {
        name: naam.to_string(), display_name: naam.to_string(),
        stops: vec![stop("Prestant_8", stop_trem)],
        has_tremulant: kind.is_some() || stop_trem,
        tremulant_kind: kind.map(str::to_string), has_swell: false, is_pedal: false,
    }
}

/// 0.9.12: een melodiekoppel koppelt naar een ÁNDER klavier (solo-koppel);
/// een baskoppel van elk manuaal naar het pedaal; T.A. per divisie.
#[test]
fn melodiekoppels_gaan_naar_een_ander_klavier() {
    use super::{generate_couplers, CouplerDto};
    let mut ped = divisie("Pedaal", None, false);
    ped.is_pedal = true;
    let divs = vec![ped, divisie("Hoofdwerk", None, false), divisie("Nevenwerk", None, false)];
    let k = generate_couplers(&divs);
    let melodie: Vec<&CouplerDto> = k.iter().filter(|c| c.coupler_type == "melody").collect();
    assert_eq!(melodie.len(), 2);
    assert!(melodie.iter().all(|c| c.source_division != c.destination_division));
    assert!(melodie.iter().all(|c| !c.source_division.contains("Pedaal") && !c.destination_division.contains("Pedaal")));
    assert!(melodie.iter().any(|c| c.id == "melody_nevenwerk_hoofdwerk" && c.source_division == "Hoofdwerk" && c.destination_division == "Nevenwerk"));
    let bas: Vec<&CouplerDto> = k.iter().filter(|c| c.coupler_type == "bass").collect();
    assert_eq!(bas.len(), 2);
    assert!(bas.iter().all(|c| c.destination_division == "Pedaal"));
    assert_eq!(k.iter().filter(|c| c.coupler_type == "ta").count(), 3);
    // Actiecodes uniek.
    let mut codes: Vec<u8> = k.iter().map(|c| c.midi_action_code).collect();
    codes.sort(); codes.dedup();
    assert_eq!(codes.len(), k.len());
}

fn opgeslagen(div: &str, enabled: bool) -> DivisionTremulantSaved {
    DivisionTremulantSaved {
        division: div.to_string(), enabled, rate: 5.5, amp_depth: 12.0, pitch_depth: 20.0,
    }
}

#[test]
fn echte_opnamen_herkend_aan_soort_of_registers() {
    assert!(divisie_heeft_tremulant_opnamen(&divisie("A", Some("wave"), false)));
    assert!(divisie_heeft_tremulant_opnamen(&divisie("B", Some("samples"), false)));
    assert!(divisie_heeft_tremulant_opnamen(&divisie("C", None, true)));
    assert!(!divisie_heeft_tremulant_opnamen(&divisie("D", Some("synth"), false)));
    assert!(!divisie_heeft_tremulant_opnamen(&divisie("E", None, false)));
}

#[test]
fn opgeslagen_lfo_gaat_uit_over_echte_opnamen() {
    // 0.7.38-bestand: enabled=true voor divisies die sinds 0.7.39 als
    // wave/samples gelden — opname én LFO klonken over elkaar.
    let divs = vec![
        divisie("Hoofdwerk", Some("wave"), true),
        divisie("Bovenwerk", Some("samples"), true),
        divisie("Rugwerk", None, true), // alleen registers met een trem-laag
    ];
    let saved = vec![
        opgeslagen("Hoofdwerk", true), opgeslagen("Bovenwerk", true), opgeslagen("Rugwerk", true),
    ];
    let (lijst, uitgezet) = nagebootste_tremulant_voor_herstel(&saved, &divs);
    assert_eq!(uitgezet, vec!["Hoofdwerk".to_string(), "Bovenwerk".to_string(), "Rugwerk".to_string()]);
    assert_eq!(lijst.len(), 3);
    assert!(lijst.iter().all(|t| !t.enabled));
    // Parameters blijven staan; alleen de aan/uit-stand verandert.
    assert_eq!(lijst[0].rate, 5.5);
    assert_eq!(lijst[0].amp_depth, 12.0);
    assert_eq!(lijst[0].pitch_depth, 20.0);
}

#[test]
fn opgeslagen_lfo_blijft_zonder_opnamen() {
    // Synth-ODF-tremulant, geen tremulant, of een divisie die dit orgel
    // niet kent: de bewaarde voorkeur blijft ongemoeid.
    let divs = vec![divisie("Hoofdwerk", Some("synth"), false), divisie("Pedaal", None, false)];
    let saved = vec![
        opgeslagen("Hoofdwerk", true), opgeslagen("Pedaal", true), opgeslagen("Onbekend", true),
    ];
    let (lijst, uitgezet) = nagebootste_tremulant_voor_herstel(&saved, &divs);
    assert!(uitgezet.is_empty());
    assert_eq!(lijst.len(), 3);
    assert!(lijst.iter().all(|t| t.enabled));
}

#[test]
fn uitstaande_lfo_wordt_niet_gemeld() {
    let divs = vec![divisie("Hoofdwerk", Some("wave"), true)];
    let (lijst, uitgezet) = nagebootste_tremulant_voor_herstel(&[opgeslagen("Hoofdwerk", false)], &divs);
    assert!(uitgezet.is_empty());
    assert!(!lijst[0].enabled);
}

#[test]
fn wave_vlag_zonder_opnamen_wordt_synth() {
    // Regressie: TremulantType=Wave zette de UI-LFO uit terwijl er geen
    // enkele trem-opname geladen was → helemaal geen tremulant meer.
    assert_eq!(tremulant_kind_voor_divisie(false, true, true), Some("synth"));
}

#[test]
fn opnamen_bepalen_wave_of_samples() {
    assert_eq!(tremulant_kind_voor_divisie(true, true, true), Some("wave"));
    // Hauptwerk-"tremmed"-lagen: opnamen zonder ODF-wave-vlag.
    assert_eq!(tremulant_kind_voor_divisie(true, false, false), Some("samples"));
}

#[test]
fn odf_tremulant_zonder_opnamen_is_synth_en_niets_is_niets() {
    assert_eq!(tremulant_kind_voor_divisie(false, false, true), Some("synth"));
    assert_eq!(tremulant_kind_voor_divisie(false, false, false), None);
}
