use super::*;
use crossbeam_channel::unbounded;
use std::time::Duration;

#[test]
fn noot_via_het_ble_kanaal_wordt_geleerd_zonder_usb() {
    let (ble_tx, ble_rx) = unbounded::<MidiMessage>();
    ble_tx.send(MidiMessage::NoteOn { channel: 2, note: 40, velocity: 90 }).unwrap();
    let r = AppState::wait_for_note_on_dual(None, &ble_rx, Duration::from_secs(3), &|_| {});
    assert_eq!(r, Some((2, 40)));
}

#[test]
fn usb_kanaal_werkt_nog_zoals_voorheen() {
    let (usb_tx, usb_rx) = unbounded::<MidiMessage>();
    usb_tx.send(MidiMessage::NoteOn { channel: 0, note: 60, velocity: 100 }).unwrap();
    let r = AppState::wait_for_note_on(&usb_rx, Duration::from_secs(3), &|_| {});
    assert_eq!(r, Some((0, 60)));
}

#[test]
fn noteoff_en_velocity_nul_tellen_niet_en_gaan_door_naar_de_audio() {
    let (ble_tx, ble_rx) = unbounded::<MidiMessage>();
    ble_tx.send(MidiMessage::NoteOff { channel: 1, note: 50, velocity: 0 }).unwrap();
    ble_tx.send(MidiMessage::NoteOn { channel: 1, note: 51, velocity: 0 }).unwrap();
    ble_tx.send(MidiMessage::NoteOn { channel: 1, note: 52, velocity: 80 }).unwrap();
    let doorgestuurd = std::cell::RefCell::new(0usize);
    let r = AppState::wait_for_note_on_dual(None, &ble_rx, Duration::from_secs(3), &|_| { *doorgestuurd.borrow_mut() += 1; });
    assert_eq!(r, Some((1, 52)));
    assert_eq!(*doorgestuurd.borrow(), 2, "beide releases horen bij de audio te komen");
}

#[test]
fn zonder_noot_geeft_hij_na_de_time_out_niets() {
    let (_ble_tx, ble_rx) = unbounded::<MidiMessage>();
    let t0 = std::time::Instant::now();
    let r = AppState::wait_for_note_on_dual(None, &ble_rx, Duration::from_millis(300), &|_| {});
    assert_eq!(r, None);
    assert!(t0.elapsed() >= Duration::from_millis(300));
}
