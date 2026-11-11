use super::*;

fn aan(n: u32) -> AudioCommand { AudioCommand::NoteOn { stop_id: 1, pipe_num: n, midi_note: 60, velocity: 1.0, verschuiving: 0 } }
fn uit(n: u32) -> AudioCommand { AudioCommand::NoteOff { stop_id: 1, pipe_num: n, verschuiving: 0 } }
fn sleutel(c: &AudioCommand) -> (char, u32) {
    match c {
        AudioCommand::NoteOn { pipe_num, .. } => ('A', *pipe_num),
        AudioCommand::NoteOff { pipe_num, .. } => ('U', *pipe_num),
        AudioCommand::ReleaseStop { stop_id } => ('R', *stop_id),
        AudioCommand::SetMasterGain(_) => ('G', 0),
        _ => ('?', 0),
    }
}
/// Alles ophalen, afwisselend uit het kanaal en via nazending.
fn alles(ov: &RtOverloop, tx: &Sender<AudioCommand>, rx: &Receiver<AudioCommand>) -> Vec<(char, u32)> {
    let mut uitkomst = Vec::new();
    loop {
        while let Ok(c) = rx.try_recv() { uitkomst.push(sleutel(&c)); }
        if ov.len() == 0 { break; }
        ov.nazend(tx);
    }
    uitkomst
}

#[test]
fn volle_wachtrij_buffert_alles_in_volgorde() {
    let ov = RtOverloop::new();
    let (tx, rx) = bounded::<AudioCommand>(2);
    for n in 0..5 { ov.send(&tx, aan(n), 100); }
    ov.send(&tx, uit(0), 100);
    assert_eq!(rx.len(), 2, "de eerste twee passen in het kanaal");
    assert_eq!(ov.len(), 4, "de rest wacht, inclusief de NoteOff");
    // Er komt ruimte, maar de buffer is niet leeg: een nieuw commando komt
    // achteraan, nooit vóór de achterstand.
    let _ = rx.recv().unwrap();
    ov.send(&tx, uit(1), 100);
    let rest = alles(&ov, &tx, &rx);
    assert_eq!(rest, vec![('A', 1), ('A', 2), ('A', 3), ('A', 4), ('U', 0), ('U', 1)]);
    assert_eq!(ov.noteon_weggelaten(), 0);
    assert!(ov.piek() >= 4);
}

#[test]
fn boven_de_grens_vervalt_alleen_noteon() {
    let ov = RtOverloop::new();
    let (tx, rx) = bounded::<AudioCommand>(1);
    ov.send(&tx, aan(0), 3);           // in het kanaal
    for n in 1..=3 { ov.send(&tx, aan(n), 3); } // buffer vol (3)
    ov.send(&tx, aan(9), 3);           // weggelaten
    ov.send(&tx, uit(0), 3);           // bewaard
    ov.send(&tx, AudioCommand::ReleaseStop { stop_id: 7 }, 3); // bewaard
    assert_eq!(ov.noteon_weggelaten(), 1);
    assert_eq!(ov.len(), 5);
    let rest = alles(&ov, &tx, &rx);
    assert_eq!(rest, vec![('A', 0), ('A', 1), ('A', 2), ('A', 3), ('U', 0), ('R', 7)]);
}

#[test]
fn weggelaten_noteon_verrekent_zijn_noteoff() {
    let ov = RtOverloop::new();
    let (tx, rx) = bounded::<AudioCommand>(1);
    ov.send(&tx, aan(0), 2);            // kanaal
    ov.send(&tx, aan(1), 2);            // buffer 1
    ov.send(&tx, aan(2), 2);            // buffer 2 = grens
    ov.send(&tx, aan(9), 2);            // weggelaten
    ov.send(&tx, aan(9), 2);            // nog eens weggelaten (zelfde pijp via twee routes)
    assert_eq!(ov.noteon_weggelaten(), 2);
    ov.send(&tx, uit(9), 2);            // verrekend: vervalt
    ov.send(&tx, uit(9), 2);            // verrekend: vervalt
    ov.send(&tx, uit(9), 2);            // derde NoteOff hoort bij géén weggelaten NoteOn: bewaard
    ov.send(&tx, uit(1), 2);            // NoteOff van een gebufferde NoteOn: bewaard
    assert_eq!(ov.len(), 4);
    let rest = alles(&ov, &tx, &rx);
    assert_eq!(rest, vec![('A', 0), ('A', 1), ('A', 2), ('U', 9), ('U', 1)]);
    // ReleaseStop maakt het register stil en wist de verrekenlijst van
    // dat register: een latere NoteOff voor die pijp blijft bewaard.
    let (tx, rx) = bounded::<AudioCommand>(1);
    ov.send(&tx, aan(0), 1);
    ov.send(&tx, aan(1), 1);
    ov.send(&tx, aan(5), 1);            // weggelaten
    ov.send(&tx, AudioCommand::ReleaseStop { stop_id: 1 }, 1);
    ov.send(&tx, uit(5), 1);            // bewaard
    let rest = alles(&ov, &tx, &rx);
    assert_eq!(rest, vec![('A', 0), ('A', 1), ('R', 1), ('U', 5)]);
}

#[test]
fn leeg_noten_bewaart_configuratie_in_volgorde() {
    let ov = RtOverloop::new();
    let (tx, _rx) = bounded::<AudioCommand>(1);
    ov.send(&tx, aan(0), 100);
    ov.send(&tx, aan(1), 100);
    ov.send(&tx, AudioCommand::SetMasterGain(-6.0), 100);
    ov.send(&tx, uit(1), 100);
    ov.leeg_noten();
    assert_eq!(ov.len(), 1);
    ov.leeg();
    assert_eq!(ov.len(), 0);
}

#[test]
fn zonder_ontvanger_bewaren_en_later_afleveren() {
    let ov = RtOverloop::new();
    let (tx_oud, rx_oud) = bounded::<AudioCommand>(4);
    drop(rx_oud);
    for n in 0..3 { ov.send(&tx_oud, aan(n), 100); }
    ov.send(&tx_oud, uit(0), 100);
    assert_eq!(ov.len(), 4, "zender zonder ontvanger: alles bewaard, niets gedropt");
    let (tx, rx) = bounded::<AudioCommand>(8);
    let rest = alles(&ov, &tx, &rx);
    assert_eq!(rest, vec![('A', 0), ('A', 1), ('A', 2), ('U', 0)]);
}

#[test]
fn twee_zenders_houden_elk_hun_volgorde() {
    let ov = std::sync::Arc::new(RtOverloop::new());
    let (tx, rx) = bounded::<AudioCommand>(2);
    let handles: Vec<_> = (0..2u32).map(|t| {
        let ov = ov.clone(); let tx = tx.clone();
        std::thread::spawn(move || {
            for i in 0..500u32 { ov.send(&tx, aan(t * 100_000 + i), 1_000_000); }
        })
    }).collect();
    for h in handles { h.join().unwrap(); }
    let rest = alles(&ov, &tx, &rx);
    for t in 0..2u32 {
        let eigen: Vec<u32> = rest.iter().filter(|(_, n)| n / 100_000 == t).map(|(_, n)| *n).collect();
        assert_eq!(eigen, (0..500u32).map(|i| t * 100_000 + i).collect::<Vec<_>>());
    }
    assert_eq!(rest.len(), 1000);
}
