use super::registration_without_crescendo;

fn ids(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

#[test]
fn opslaan_minus_trede_claims() {
    let drawn = ids(&["Prestant_8", "Trompet_8", "Octaaf_4"]);
    let couplers = ids(&["real_coupler_1", "coupler_II_I"]);
    let claims = ids(&["Octaaf_4", "real_coupler_1"]);
    let (stops, cpl) = registration_without_crescendo(&drawn, &couplers, &claims);
    assert_eq!(stops, ids(&["Prestant_8", "Trompet_8"]));
    assert_eq!(cpl, ids(&["coupler_II_I"]));
    // Zonder claims blijft alles staan; lege registratie blijft leeg.
    let (s2, c2) = registration_without_crescendo(&drawn, &couplers, &[]);
    assert_eq!(s2, drawn);
    assert_eq!(c2, couplers);
    let (s3, c3) = registration_without_crescendo(&[], &[], &claims);
    assert!(s3.is_empty() && c3.is_empty());
}
