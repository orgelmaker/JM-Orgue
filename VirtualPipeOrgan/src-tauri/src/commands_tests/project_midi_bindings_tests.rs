use super::*;

fn actions(items: &[(&str, u32)]) -> HashMap<String, u32> {
    items.iter().map(|(id, code)| ((*id).into(), *code)).collect()
}

fn binding(code: u32, note: u8) -> PresetBindingSaved {
    PresetBindingSaved {
        preset_num: code, trigger_type: "note".into(), note: Some(note), channel: Some(2),
        controller: None, value: None, program: None, sysex_hex: None, bit: None,
    }
}

fn codes(bindings: &[PresetBindingSaved]) -> Vec<(u32, Option<u8>)> {
    bindings.iter().map(|binding| (binding.preset_num, binding.note)).collect()
}

#[test]
fn verplaatst_register_houdt_dezelfde_fysieke_bediening() {
    let previous = actions(&[("project_1", 150), ("project_2", 151)]);
    let current = actions(&[("project_1", 151), ("project_2", 150)]);
    let bindings = vec![binding(150, 60), binding(151, 61)];
    let restored = project_bindings_for_restore(true, &bindings, Some(&previous), &current);
    assert_eq!(codes(&restored), vec![(151, Some(60)), (150, Some(61))]);
    assert_eq!(restored[0].channel, Some(2));
    assert_eq!(codes(&bindings), vec![(150, Some(60)), (151, Some(61))]);
}

#[test]
fn verwijderd_register_wordt_niet_aan_opgeschoven_register_gekoppeld() {
    let previous = actions(&[("project_1", 150), ("project_2", 151), ("project_3", 152)]);
    let current = actions(&[("project_2", 150), ("project_3", 151)]);
    let restored = project_bindings_for_restore(true, &[binding(150, 60), binding(151, 61), binding(152, 62)], Some(&previous), &current);
    assert_eq!(codes(&restored), vec![(150, Some(61)), (151, Some(62))]);
}

#[test]
fn nieuw_bronregister_met_dezelfde_runtimecode_erft_geen_binding() {
    let previous = actions(&[("project_1", 150)]);
    let current = actions(&[("project_99", 150)]);
    assert!(project_bindings_for_restore(true, &[binding(150, 60)], Some(&previous), &current).is_empty());
}

#[test]
fn koppels_worden_eveneens_op_hun_doel_id_hersteld() {
    let previous = actions(&[("project_coupler_1_2", 100), ("project_coupler_1_3", 101)]);
    let current = actions(&[("project_coupler_1_2", 101), ("project_coupler_1_3", 100)]);
    let restored = project_bindings_for_restore(true, &[binding(100, 60), binding(101, 61)], Some(&previous), &current);
    assert_eq!(codes(&restored), vec![(101, Some(60)), (100, Some(61))]);
}

#[test]
fn tremulantbinding_volgt_divisie_bij_gewijzigde_index() {
    let previous = actions(&[("division_trem:Hoofdwerk", 24), ("division_trem:Zwelwerk", 25)]);
    let current = actions(&[("division_trem:Hoofdwerk", 25), ("division_trem:Zwelwerk", 24)]);
    let restored = project_bindings_for_restore(true, &[binding(24, 60), binding(25, 61)], Some(&previous), &current);
    assert_eq!(codes(&restored), vec![(25, Some(60)), (24, Some(61))]);
    let renamed = actions(&[("division_trem:Ander werk", 24)]);
    assert!(project_bindings_for_restore(true, &[binding(24, 60)], Some(&previous), &renamed).is_empty());
}

#[test]
fn dubbele_oude_of_nieuwe_codes_worden_niet_geraden() {
    let previous = actions(&[("project_1", 255), ("project_2", 255)]);
    let current = actions(&[("project_1", 150), ("project_2", 151)]);
    assert!(project_bindings_for_restore(true, &[binding(255, 60)], Some(&previous), &current).is_empty());
    let previous = actions(&[("project_1", 150), ("project_2", 151)]);
    let current = actions(&[("project_1", 255), ("project_2", 255)]);
    assert!(project_bindings_for_restore(true, &[binding(150, 60), binding(151, 61)], Some(&previous), &current).is_empty());
}

#[test]
fn algemene_acties_blijven_en_project_zonder_identiteit_is_veilig() {
    let bindings = vec![binding(1, 60), binding(23, 61), binding(40, 62), binding(44, 63), binding(99, 64), binding(150, 65), binding(100, 66), binding(24, 67)];
    let restored = project_bindings_for_restore(true, &bindings, None, &HashMap::new());
    assert_eq!(codes(&restored), codes(&bindings[..5]));
}

#[test]
fn gewone_orgels_behouden_hun_bestaande_numerieke_bindings() {
    let bindings = vec![binding(24, 60), binding(100, 61), binding(150, 62), binding(255, 63)];
    let restored = project_bindings_for_restore(false, &bindings, None, &HashMap::new());
    assert_eq!(serde_json::to_value(&restored).unwrap(), serde_json::to_value(&bindings).unwrap());
}

#[test]
fn expliciete_restore_met_actuele_doelmap_remapt_niet_tweemaal() {
    let current = actions(&[("project_2", 150), ("project_1", 151)]);
    let bindings = vec![binding(151, 60), binding(150, 61)];
    let restored = project_bindings_for_restore(true, &bindings, Some(&current), &current);
    assert_eq!(codes(&restored), codes(&bindings));
}

#[test]
fn instellingen_blijven_achterwaarts_leesbaar_en_bewaren_de_doelmap() {
    let settings = OrganSettings { project_midi_actions: Some(actions(&[("project_1", 150)])), ..Default::default() };
    let mut encoded = serde_json::to_value(&settings).unwrap();
    let decoded: OrganSettings = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(decoded.project_midi_actions, settings.project_midi_actions);
    encoded.as_object_mut().unwrap().remove("project_midi_actions");
    let legacy: OrganSettings = serde_json::from_value(encoded).unwrap();
    assert!(legacy.project_midi_actions.is_none());
    assert!(legacy.project_midi_divisions.is_none());
}

#[test]
fn grote_project_acties_blijven_uniek_en_legacy_project_bindings_migreren_op_doel() {
    let action_codes: std::collections::HashSet<u32> = (1..=2048).map(project_stop_action).collect();
    assert_eq!(action_codes.len(), 2048);
    assert_eq!(project_stop_action(2048), 67_584);
    assert!(action_codes.iter().all(|code| *code > 255 && *code < EXTENDED_COUPLER_ACTION_BASE));
    let old = actions(&[("project_1", 150), ("project_2048", 255), ("real_coupler_65538", 100)]);
    let current = actions(&[("project_1", project_stop_action(1)), ("project_2048", project_stop_action(2048)),
        ("real_coupler_65538", EXTENDED_COUPLER_ACTION_BASE)]);
    let restored = project_bindings_for_restore(true, &[binding(150, 60), binding(255, 61), binding(100, 62)], Some(&old), &current);
    assert_eq!(codes(&restored), vec![(65_537, Some(60)), (67_584, Some(61)), (262_144, Some(62))]);
}

#[test]
fn brede_midi_acties_serialiseren_zonder_afkappen_en_laten_midi_bytes_ongewijzigd() {
    for action in [150, 255, project_stop_action(2048), EXTENDED_COUPLER_ACTION_BASE + 511] {
        let saved = binding(action, 127);
        let runtime = saved_to_preset_binding(&saved).unwrap();
        let restored = preset_binding_to_saved(&runtime);
        assert_eq!(restored.preset_num, action);
        assert_eq!(restored.note, Some(127));
        assert_eq!(restored.channel, Some(2));
        let encoded = serde_json::to_string(&restored).unwrap();
        let decoded: PresetBindingSaved = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.preset_num, action);
    }
    let old: PresetBindingSaved = serde_json::from_str(r#"{"preset_num":150,"trigger_type":"note","note":60,"channel":1,"controller":null,"value":null,"program":null}"#).unwrap();
    assert_eq!(old.preset_num, 150);
    assert_eq!(preset_binding_to_saved(&saved_to_preset_binding(&old).unwrap()).note, Some(60));
}

fn division(id: u32, name: &str, midi_channel: u8) -> crate::organ_project::ProjectDivision {
    crate::organ_project::ProjectDivision {
        id, name: name.into(), midi_channel, first_midi_note: 36, key_count: 61, pedal: false,
    }
}

fn learned(name: &str) -> MidiMappingSaved {
    MidiMappingSaved { division: name.into(), channel: Some(9), transpose: 12,
        first_midi_note: Some(40), last_midi_note: Some(100), short_octave: true }
}

#[test]
fn projectmodel_zet_werkelijk_nulgebaseerd_kanaal_en_toetsbereik() {
    let mut model = division(3, " Hoofdwerk ", 5);
    model.first_midi_note = 48;
    model.key_count = 49;
    let mappings = project_mappings_for_restore(&[model], None, &[learned("Hoofdwerk")]);
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].division, "Hoofdwerk");
    assert_eq!(mappings[0].channel, Some(4));
    assert_eq!(mappings[0].first_midi_note, Some(48));
    assert_eq!(mappings[0].last_midi_note, Some(96));
    assert_eq!(mappings[0].transpose, 0);
    assert!(!mappings[0].short_octave);
}

#[test]
fn bewust_ingeleerde_override_blijft_bij_ongewijzigd_projectmodel() {
    let model = vec![division(3, "Hoofdwerk", 2), division(9, "Zwelwerk", 3)];
    let saved = vec![learned("Hoofdwerk"), MidiMappingSaved { channel: None, ..learned("Zwelwerk") }];
    let mut reordered = model.clone();
    reordered.reverse();
    let mappings = project_mappings_for_restore(&reordered, Some(&model), &saved);
    assert_eq!(mappings[0].division, "Zwelwerk");
    assert_eq!(mappings[0].channel, None);
    assert_eq!(mappings[1].channel, Some(9));
    assert_eq!(mappings[1].transpose, 12);
    assert!(mappings[1].short_octave);
}

#[test]
fn bouwerwijziging_wint_van_oude_ingeleerde_mapping() {
    let old = division(3, "Hoofdwerk", 2);
    let mut variants = Vec::new();
    let mut changed = old.clone(); changed.midi_channel = 5; variants.push(changed);
    let mut changed = old.clone(); changed.name = "Nieuw werk".into(); variants.push(changed);
    let mut changed = old.clone(); changed.first_midi_note = 24; variants.push(changed);
    let mut changed = old.clone(); changed.key_count = 49; variants.push(changed);
    let mut changed = old.clone(); changed.pedal = true; variants.push(changed);
    for changed in variants {
        let restored = project_mappings_for_restore(&[changed.clone()], Some(std::slice::from_ref(&old)), &[learned("Hoofdwerk")]);
        assert_eq!(restored[0].division, changed.name);
        assert_eq!(restored[0].channel, Some(changed.midi_channel - 1));
        assert_eq!(restored[0].first_midi_note, Some(changed.first_midi_note));
        assert_eq!(restored[0].last_midi_note, Some((u32::from(changed.first_midi_note) + changed.key_count - 1) as u8));
        assert_eq!(restored[0].transpose, 0);
        assert!(!restored[0].short_octave);
    }
}

#[test]
fn verdwenen_divisie_en_nieuwe_divisie_met_dezelfde_naam_erven_niets() {
    let previous = vec![division(3, "Hoofdwerk", 2), division(9, "Verdwenen", 3)];
    let current = vec![division(15, "Hoofdwerk", 7)];
    let restored = project_mappings_for_restore(&current, Some(&previous), &[learned("Hoofdwerk"), learned("Verdwenen")]);
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].division, "Hoofdwerk");
    assert_eq!(restored[0].channel, Some(6));
    assert_eq!(restored[0].transpose, 0);
}
