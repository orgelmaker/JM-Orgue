use super::*;

fn new_active() -> Arc<RwLock<Option<MidiRecording>>> {
    Arc::new(RwLock::new(Some(MidiRecording {
        events: Vec::new(),
        start_time: std::time::Instant::now(),
        active: true,
        stopped_elapsed: None,
    })))
}

#[test]
fn captures_note_on_and_off_with_correct_bytes() {
    let rec = new_active();
    AppState::capture_midi_event(&rec, &MidiMessage::NoteOn { channel: 2, note: 60, velocity: 100 });
    AppState::capture_midi_event(&rec, &MidiMessage::NoteOff { channel: 2, note: 60, velocity: 0 });
    let g = rec.read();
    let evs = &g.as_ref().unwrap().events;
    assert_eq!(evs.len(), 2);
    // (timestamp_us, status, data1, data2, channel)
    assert_eq!(evs[0].1, 0x92); // NoteOn op kanaal 2
    assert_eq!(evs[0].2, 60);
    assert_eq!(evs[0].3, 100);
    assert_eq!(evs[0].4, 2);
    assert_eq!(evs[1].1, 0x82); // NoteOff op kanaal 2
    // Timestamps lopen niet terug.
    assert!(evs[1].0 >= evs[0].0);
}

#[test]
fn captures_control_change_and_program_change() {
    let rec = new_active();
    AppState::capture_midi_event(&rec, &MidiMessage::ControlChange { channel: 0, controller: 7, value: 64 });
    AppState::capture_midi_event(&rec, &MidiMessage::ProgramChange { channel: 1, program: 5 });
    let g = rec.read();
    let evs = &g.as_ref().unwrap().events;
    assert_eq!(evs.len(), 2);
    assert_eq!(evs[0].1, 0xB0); // CC
    assert_eq!(evs[0].2, 7);
    assert_eq!(evs[0].3, 64);
    assert_eq!(evs[1].1, 0xC1); // PC op kanaal 1
    assert_eq!(evs[1].2, 5);
}

#[test]
fn skips_realtime_and_sysex() {
    let rec = new_active();
    AppState::capture_midi_event(&rec, &MidiMessage::Clock);
    AppState::capture_midi_event(&rec, &MidiMessage::Start);
    AppState::capture_midi_event(&rec, &MidiMessage::SysEx(vec![1, 2, 3]));
    assert_eq!(rec.read().as_ref().unwrap().events.len(), 0);
}

#[test]
fn noop_when_not_recording() {
    let rec: Arc<RwLock<Option<MidiRecording>>> = Arc::new(RwLock::new(None));
    AppState::capture_midi_event(&rec, &MidiMessage::NoteOn { channel: 0, note: 60, velocity: 100 });
    assert!(rec.read().is_none());
}
