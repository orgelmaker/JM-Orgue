use super::*;

fn coupler(id: &str, src: &str, dest: &str, ctype: &str, offset: i32) -> CouplerDto {
    CouplerDto {
        id: id.to_string(),
        name: id.to_string(),
        source_division: src.to_string(),
        destination_division: dest.to_string(),
        active: false,
        display_in_division: src.to_string(),
        midi_action_code: 0,
        coupler_type: ctype.to_string(),
        pitch_offset: offset,
        speciaal: false,
        cascade_toegestaan: true,
    }
}

#[test]
fn doorkoppelen_pedaal_via_hoofdwerk_naar_nevenwerk() {
    // Pedaal→HW en HW→NW actief ⇒ ook een route Pedaal→NW (transitief).
    let list = vec![
        coupler("c1", "Pedaal", "Hoofdwerk", "unison", 0),
        coupler("c2", "Hoofdwerk", "Nevenwerk", "unison", 0),
    ];
    let active = vec!["c1".to_string(), "c2".to_string()];
    let routes = AppState::expand_coupler_routes(&list, &active);
    assert!(routes.iter().any(|r| r.source_division == "Pedaal" && r.destination_division == "Hoofdwerk"));
    assert!(routes.iter().any(|r| r.source_division == "Hoofdwerk" && r.destination_division == "Nevenwerk"));
    assert!(routes.iter().any(|r| r.source_division == "Pedaal" && r.destination_division == "Nevenwerk" && r.pitch_offset == 0),
            "transitieve route Pedaal→Nevenwerk ontbreekt: {:?}", routes.iter().map(|r| (r.source_division.clone(), r.destination_division.clone())).collect::<Vec<_>>());
}

#[test]
fn octaaf_offsets_sommeren_over_de_keten() {
    // Pedaal→HW 4' (+12) + HW→NW 16' (-12) ⇒ Pedaal→NW unisono (0).
    let list = vec![
        coupler("c1", "Pedaal", "Hoofdwerk", "super", 12),
        coupler("c2", "Hoofdwerk", "Nevenwerk", "sub", -12),
    ];
    let active = vec!["c1".to_string(), "c2".to_string()];
    let routes = AppState::expand_coupler_routes(&list, &active);
    assert!(routes.iter().any(|r| r.source_division == "Pedaal" && r.destination_division == "Nevenwerk" && r.pitch_offset == 0));
}

#[test]
fn cykel_wordt_afgekapt() {
    // HW→NW én NW→HW actief: mag niet oneindig doorlopen; beide directe
    // routes bestaan, en de terug-route levert geen self-route-explosie op.
    let list = vec![
        coupler("c1", "Hoofdwerk", "Nevenwerk", "unison", 0),
        coupler("c2", "Nevenwerk", "Hoofdwerk", "unison", 0),
    ];
    let active = vec!["c1".to_string(), "c2".to_string()];
    let routes = AppState::expand_coupler_routes(&list, &active);
    assert!(routes.len() <= 64);
    assert!(routes.iter().any(|r| r.source_division == "Hoofdwerk" && r.destination_division == "Nevenwerk"));
    assert!(routes.iter().any(|r| r.source_division == "Nevenwerk" && r.destination_division == "Hoofdwerk"));
}

#[test]
fn inactieve_en_ta_koppels_doen_niet_mee() {
    let list = vec![
        coupler("c1", "Pedaal", "Hoofdwerk", "unison", 0),
        coupler("c2", "Hoofdwerk", "Nevenwerk", "unison", 0),
        coupler("ta1", "Hoofdwerk", "Hoofdwerk", "ta", 0),
    ];
    let active = vec!["c1".to_string(), "ta1".to_string()]; // c2 NIET actief
    let routes = AppState::expand_coupler_routes(&list, &active);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].destination_division, "Hoofdwerk");
}
