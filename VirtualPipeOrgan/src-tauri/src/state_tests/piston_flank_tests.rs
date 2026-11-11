use super::*;

fn cc(kanaal: u8, controller: u8, waarde: u8) -> MidiMessage {
    MidiMessage::ControlChange { channel: kanaal, controller, value: waarde }
}

fn binding_cc(preset: u8, kanaal: Option<u8>, controller: u8, drempel: u8) -> MidiPresetBinding {
    MidiPresetBinding {
        preset_num: preset,
        trigger: MidiPresetTrigger::ControlChange { channel: kanaal, controller, value: drempel },
    }
}

/// Tussenpoos van opeenvolgende berichten in een pedaalsweep (ms). Ruim
/// onder `PresetCcGate::PULSPAUZE_MS`, zoals echte pedalen sturen.
const SWEEP_STAP_MS: u64 = 5;

/// Proefopstelling met een GESIMULEERDE klok: `check_preset_trigger` krijgt
/// het tijdstip expliciet mee, dus de wandklok speelt in deze tests geen rol.
struct Proef {
    bindings: Arc<RwLock<Vec<MidiPresetBinding>>>,
    gate: PresetCcGate,
    tx: Sender<u8>,
    rx: Receiver<u8>,
    klok_ms: u64,
}

impl Proef {
    fn new(bindings: Vec<MidiPresetBinding>) -> Self {
        let (tx, rx) = bounded::<u8>(64);
        Self {
            bindings: Arc::new(RwLock::new(bindings)),
            gate: PresetCcGate::default(),
            tx,
            rx,
            klok_ms: 1_000,
        }
    }

    /// Stuurt één bericht, `na_ms` ms na het vorige.
    fn stuur(&mut self, msg: MidiMessage, na_ms: u64) {
        self.klok_ms += na_ms;
        AppState::check_preset_trigger(&msg, &self.bindings, &self.tx, &mut self.gate, self.klok_ms);
    }

    /// Stuurt CC-waarden `tussenpoos_ms` uit elkaar en geeft terug welke
    /// presets daarbij gevuurd hebben.
    fn reeks(&mut self, kanaal: u8, controller: u8, waarden: &[u8], tussenpoos_ms: u64) -> Vec<u8> {
        for w in waarden {
            self.stuur(cc(kanaal, controller, *w), tussenpoos_ms);
        }
        self.gevuurd()
    }

    /// Pedaalsweep: waarden om de paar ms.
    fn sweep(&mut self, kanaal: u8, controller: u8, waarden: &[u8]) -> Vec<u8> {
        self.reeks(kanaal, controller, waarden, SWEEP_STAP_MS)
    }

    /// Losse knoppulsen: waarden minstens een pulspauze uit elkaar.
    fn pulsen(&mut self, kanaal: u8, controller: u8, waarden: &[u8]) -> Vec<u8> {
        self.reeks(kanaal, controller, waarden, 2 * PresetCcGate::PULSPAUZE_MS)
    }

    fn gevuurd(&mut self) -> Vec<u8> {
        self.rx.try_iter().collect()
    }
}

#[test]
fn piston_op_continue_cc_vuurt_alleen_op_de_opgaande_flank() {
    // De klacht: een piston ingeleerd op dezelfde CC als een pedaal gaf bij
    // één pedaalbeweging tientallen preset-triggers.
    let mut p = Proef::new(vec![binding_cc(3, Some(0), 11, 127)]);

    // Pedaal van dicht naar open: pas bij 127 één keer vuren, daarna niet meer.
    assert_eq!(p.sweep(0, 11, &[0, 20, 60, 100, 126, 127, 127, 127]), vec![3]);

    // Pedaal terug en weer omhoog: precies één nieuwe trigger.
    assert_eq!(p.sweep(0, 11, &[110, 60, 0, 127]), vec![3]);
}

#[test]
fn ruis_rond_de_drempel_herwapent_niet() {
    // Drempel 64 → herwapengrens 56. Trillen tussen 58 en 70 mag maar één
    // keer vuren; pas onder 56 staat de piston weer scherp.
    let mut p = Proef::new(vec![binding_cc(1, Some(0), 7, 64)]);

    assert_eq!(p.sweep(0, 7, &[63, 64, 70, 62, 65, 58, 66, 60, 64]), vec![1]);
    assert_eq!(p.sweep(0, 7, &[55, 64]), vec![1]);
}

#[test]
fn ingeleerde_waarde_nul_vuurt_niet_bij_elk_bericht() {
    // Een knop die 0 stuurde levert drempel 0. Vroeger werd dat stilzwijgend
    // drempel 1, waardoor zo'n knop pas bij het LOSLATEN (127) vuurde. Nu is
    // de 0 zelf de flank: vuren op de eerste 0, niet op de herhalingen die
    // om de paar ms volgen, en niet op de 127.
    let mut p = Proef::new(vec![binding_cc(2, Some(0), 20, 0)]);

    assert_eq!(p.sweep(0, 20, &[0, 0, 0]), vec![2],
        "de eerste 0 is de flank, de herhalingen vlak erna niet");

    assert_eq!(p.sweep(0, 20, &[127, 127, 0, 127]), vec![2],
        "loslaten (127) vuurt niet, opnieuw indrukken (0) wel");
}

#[test]
fn andere_controller_of_kanaal_raakt_de_stand_niet() {
    let mut p = Proef::new(vec![binding_cc(5, Some(1), 11, 127)]);

    // Ander kanaal en andere CC: nooit vuren, en de eigen stand blijft scherp.
    assert!(p.sweep(0, 11, &[127, 0, 127]).is_empty());
    assert!(p.sweep(1, 12, &[127, 0, 127]).is_empty());
    assert_eq!(p.sweep(1, 11, &[127]), vec![5]);
}

#[test]
fn twee_pistons_op_dezelfde_cc_houden_hun_eigen_stand() {
    let mut p = Proef::new(vec![
        binding_cc(1, Some(0), 11, 64),
        binding_cc(2, Some(0), 11, 127),
    ]);

    assert_eq!(p.sweep(0, 11, &[0, 64, 90, 127, 127]), vec![1, 2]);
}

#[test]
fn reset_zet_alle_bindingen_weer_scherp() {
    let mut p = Proef::new(vec![binding_cc(4, Some(0), 11, 127)]);

    assert_eq!(p.sweep(0, 11, &[127, 127]), vec![4]);
    p.gate.reset();
    assert_eq!(p.sweep(0, 11, &[127]), vec![4]);
}

#[test]
fn noten_en_programmawissels_blijven_ongemoeid() {
    let mut p = Proef::new(vec![
        MidiPresetBinding { preset_num: 6, trigger: MidiPresetTrigger::Note { channel: Some(0), note: 36 } },
        MidiPresetBinding { preset_num: 7, trigger: MidiPresetTrigger::ProgramChange { channel: Some(0), program: 2 } },
    ]);

    for msg in [
        MidiMessage::NoteOn { channel: 0, note: 36, velocity: 100 },
        MidiMessage::NoteOn { channel: 0, note: 36, velocity: 0 },
        MidiMessage::NoteOn { channel: 0, note: 36, velocity: 100 },
        MidiMessage::ProgramChange { channel: 0, program: 2 },
        MidiMessage::ProgramChange { channel: 0, program: 2 },
    ] {
        p.stuur(msg, SWEEP_STAP_MS);
    }
    assert_eq!(p.gevuurd(), vec![6, 6, 7, 7]);
}

#[test]
fn puls_zonder_loslaatbericht_vuurt_bij_elke_druk() {
    // Voetcontrollers in CC-modus en DIY-encoders sturen per druk alleen
    // één 127, nooit iets eronder. De hysterese alleen zou zo'n knop één
    // keer per sessie laten vuren; de pauze tussen de drukken herwapent.
    let mut p = Proef::new(vec![binding_cc(3, Some(0), 22, 127)]);

    assert_eq!(p.pulsen(0, 22, &[127, 127, 127]), vec![3, 3, 3],
        "drie losse drukken = drie triggers");

    // Precies op de pulspauze telt ook als nieuwe druk.
    assert_eq!(p.reeks(0, 22, &[127], PresetCcGate::PULSPAUZE_MS), vec![3]);

    // Contactdender: dezelfde druk nog eens binnen enkele ms is géén nieuwe druk.
    assert_eq!(p.reeks(0, 22, &[127, 127], 3), Vec::<u8>::new(),
        "herhaling vlak na de druk is dender, geen tweede druk");
    assert_eq!(p.pulsen(0, 22, &[127]), vec![3], "de volgende echte druk vuurt weer");
}

#[test]
fn pedaalsweep_heen_en_terug_vuurt_hooguit_een_keer() {
    // Drempels midden op de schaal en op het maximum: een volledige
    // pedaalbeweging 0→127→0 mag per binding hooguit één trigger geven.
    let mut p = Proef::new(vec![
        binding_cc(1, Some(0), 11, 64),
        binding_cc(2, Some(0), 11, 100),
        binding_cc(3, Some(0), 11, 127),
    ]);

    let heen: Vec<u8> = (0..=127).collect();
    let terug: Vec<u8> = (0..=127).rev().collect();
    let mut sweep = heen;
    sweep.extend(terug);

    assert_eq!(p.sweep(0, 11, &sweep), vec![1, 2, 3],
        "elke binding precies één keer, op de opgaande flank");

    // Tweede sweep meteen erachteraan: opnieuw één keer per binding.
    assert_eq!(p.sweep(0, 11, &sweep), vec![1, 2, 3]);
}

#[test]
fn nul_bij_indrukken_vuurt_op_de_nul_niet_op_de_127() {
    // Knop die 0 stuurt bij indrukken en 127 bij loslaten (ingeleerd op 0).
    let mut p = Proef::new(vec![binding_cc(4, Some(0), 30, 0)]);

    p.stuur(cc(0, 30, 0), 500);      // indrukken
    assert_eq!(p.gevuurd(), vec![4], "vuurt op het indrukken (0)");
    p.stuur(cc(0, 30, 127), 80);     // loslaten
    assert_eq!(p.gevuurd(), Vec::<u8>::new(), "loslaten (127) vuurt niet");
    p.stuur(cc(0, 30, 0), 300);      // opnieuw indrukken
    assert_eq!(p.gevuurd(), vec![4]);
    p.stuur(cc(0, 30, 127), 80);
    assert_eq!(p.gevuurd(), Vec::<u8>::new());

    // Knop die alléén 0-pulsen stuurt: vuurt bij elke druk dankzij de pauze.
    let mut q = Proef::new(vec![binding_cc(5, Some(0), 31, 0)]);
    assert_eq!(q.pulsen(0, 31, &[0, 0, 0]), vec![5, 5, 5]);

    // Ruis 0↔1 om de paar ms (gespiegelde hysterese): niet blijven vuren.
    assert_eq!(q.sweep(0, 31, &[1, 0, 1, 0, 1, 0]), Vec::<u8>::new());
    // Pas als de waarde ver genoeg van 0 af is geweest, is de volgende 0 weer een druk.
    assert_eq!(q.sweep(0, 31, &[8, 0]), vec![5]);
}

#[test]
fn ruis_120_127_om_de_paar_ms_vuurt_niet_herhaald() {
    // Pedaal tegen de bovenaanslag dat tussen 120 en 127 trilt: één trigger,
    // geen salvo. 120 ligt binnen de hysterese (herwapengrens 119).
    let mut p = Proef::new(vec![binding_cc(6, Some(0), 11, 127)]);

    let ruis: Vec<u8> = [127u8, 120].iter().copied().cycle().take(40).collect();
    assert_eq!(p.reeks(0, 11, &ruis, 3), vec![6]);

    // Een waarde onder de drempel na een lange stilte vuurt óók niet: de
    // tijdregel geldt alleen voor berichten op/boven de drempel.
    assert_eq!(p.reeks(0, 11, &[120], 2_000), Vec::<u8>::new());

    // Een echte nieuwe druk na een pauze vuurt wel.
    assert_eq!(p.reeks(0, 11, &[127], 2_000), vec![6]);
}
