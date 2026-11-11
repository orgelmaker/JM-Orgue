use super::motor_start_aan;

#[test]
fn nieuw_orgel_start_altijd_met_motor_aan() {
    // Ook zonder blower-opname: motor uit blokkeert nieuwe noten (0.8.81),
    // een set zonder motor-opname klonk daardoor niet (0.8.98 t/m 0.9.18).
    assert!(motor_start_aan(false, false));
    assert!(motor_start_aan(false, true));
}

#[test]
fn herlaad_behoudt_de_stand_van_de_organist() {
    assert!(motor_start_aan(true, true));
    assert!(!motor_start_aan(true, false));
}
