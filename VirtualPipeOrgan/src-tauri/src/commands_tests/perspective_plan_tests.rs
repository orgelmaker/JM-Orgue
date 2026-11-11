use super::{plan_perspectives_custom, plan_perspectives_from, PerspectiveSaved};
use vpo_sampler::custom_organ::{CustomOrgan, CustomPerspective, StopFamily};

fn saved(name: &str, enabled: bool, gain_db: f32) -> PerspectiveSaved {
    PerspectiveSaved { name: name.into(), enabled, gain_db }
}

#[test]
fn zonder_opslag_alle_labels_aan() {
    // Vanaf 0.7.95: zonder opgeslagen keuze staan ALLE
    // posities standaard aan, zodat de luisterpositie-schuif ze kan mengen.
    // Vóór 0.7.95 stond alleen de primaire positie aan en moest de
    // organist in de instellingen vinkjes bijzetten + herladen.
    let obs = vec![
        ("front".to_string(), true, 56), ("rear".to_string(), false, 56), ("dry".to_string(), false, 56),
        ("front".to_string(), true, 30), ("rear".to_string(), false, 30), ("dry".to_string(), false, 30),
    ];
    let plan = plan_perspectives_from(obs, &[]);
    let names: Vec<&str> = plan.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["front", "rear", "dry"]);
    assert_eq!(plan.iter().map(|p| p.enabled).collect::<Vec<_>>(), vec![true, true, true]);
    assert_eq!(plan.iter().map(|p| p.slot).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert_eq!(plan[0].pipe_count, 86);
    assert!(plan.iter().all(|p| !p.loaded && p.gain_db == 0.0));
}

#[test]
fn alles_laden_alleen_als_alle_posities_in_half_ram_passen() {
    // 0.9.34: "alle posities laden" haalde ook uitgeschakelde posities
    // binnen, zonder geheugenrem. Telling in buffers, ook voor UIT-posities.
    use super::{alle_posities_passen, preload_bytes_per_buffer};
    let obs = vec![("front".to_string(), true, 100), ("rear".to_string(), false, 100)];
    let plan = plan_perspectives_from(obs, &[saved("front", true, 0.0), saved("rear", false, 0.0)]);
    let buffers: std::collections::HashMap<String, usize> =
        [("front".to_string(), 1000), ("rear".to_string(), 1000)].into_iter().collect();
    let alles = 2000 * preload_bytes_per_buffer();
    assert!(alle_posities_passen(&plan, &buffers, Some(alles * 2)), "precies half RAM past");
    assert!(!alle_posities_passen(&plan, &buffers, Some(alles * 2 - 1)), "uitgeschakelde rear telt mee");
    assert!(alle_posities_passen(&plan, &buffers, None), "RAM onbekend: oud gedrag");
}

#[test]
fn opgeslagen_rear_aan_wordt_gehonoreerd_en_onbekend_genegeerd() {
    let obs = vec![("front".to_string(), true, 10), ("rear".to_string(), false, 10)];
    let plan = plan_perspectives_from(obs, &[saved("rear", true, -6.0), saved("front", false, 99.0), saved("spook", true, 0.0)]);
    assert_eq!(plan.len(), 2, "onbekend label 'spook' komt niet in de lijst");
    assert!(!plan[0].enabled && plan[0].gain_db == 12.0, "front uit, gain geclampt op +12");
    assert!(plan[1].enabled && (plan[1].gain_db + 6.0).abs() < 1e-6);
}

#[test]
fn extra_laag_label_zonder_primair_staat_standaard_uit() {
    // GO-set met ongelabelde laag 0 en een "(rear)"-rank als extra laag:
    // laag 0 staat altijd op vol (geen slot), dus een ingeschakelde rear
    // klinkt er dubbel overheen — en met één positie op de as zou de
    // schuif hem ook nog op vol zetten. Daarom uit (0.8.1 herstelt de
    // regel van vóór 0.7.95). Echte ranks zonder label leveren geen
    // waarneming en dus geen entry.
    let obs = vec![("rear".to_string(), false, 20)];
    let plan = plan_perspectives_from(obs, &[]);
    assert_eq!(plan.len(), 1);
    assert!(!plan[0].enabled);
    // Zodra één label primair is (SJDL: front laag 0), gaan ze allemaal aan.
    let obs2 = vec![("front".to_string(), true, 20), ("rear".to_string(), false, 20)];
    let plan2 = plan_perspectives_from(obs2, &[]);
    assert!(plan2.iter().all(|p| p.enabled));
    let empty = plan_perspectives_from(Vec::<(String, bool, usize)>::new(), &[]);
    assert!(empty.is_empty());
}

#[test]
fn hoogstens_vijftien_labels() {
    let obs: Vec<(String, bool, usize)> = (0..20).map(|i| (format!("mic{}", i), i == 0, 1)).collect();
    let plan = plan_perspectives_from(obs, &[]);
    assert_eq!(plan.len(), 15);
    assert_eq!(plan.last().unwrap().slot, 15);
}

#[test]
fn custom_plan_uit_submappen() {
    let mut organ = CustomOrgan::new("MicTest");
    let div = organ.add_division("HW", 36, 96);
    let s = div.add_stop("Prestant", 8.0, StopFamily::Principal);
    s.add_pipe(36, "HW/Prestant_8/Front/036-c.wav".into());
    s.primary_perspective = Some("Front".into());
    s.perspectives.push(CustomPerspective {
        name: "Rear".into(),
        pipes: vec![],
        tremulant_pipes: vec![],
    });
    let plan = plan_perspectives_custom(&organ, &[]);
    // Vanaf 0.7.95: alle posities standaard aan.
    assert_eq!(plan.iter().map(|p| (p.name.as_str(), p.enabled)).collect::<Vec<_>>(), vec![("Front", true), ("Rear", true)]);
    // Stop zonder posities → geen labels.
    let plain = CustomOrgan::new("Plain");
    assert!(plan_perspectives_custom(&plain, &[]).is_empty());
}
