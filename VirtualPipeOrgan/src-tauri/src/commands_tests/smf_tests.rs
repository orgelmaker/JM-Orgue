use super::{encode_smf, encode_smf_with_name};

/// Lees een big-endian u32 op offset.
fn be_u32(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

#[test]
fn encode_smf_with_name_emits_ff03() {
    let events = vec![(0u64, 0xC0u8, 5u8, 0u8, 0u8)];
    let data = encode_smf_with_name(&events, Some("Bätz"));
    let track_len = be_u32(&data, 18) as usize;
    let track = &data[22..22 + track_len];
    // Track: [tempo-meta 7 bytes] [00 FF 03 len naam...] [PC] [eot]
    assert_eq!(&track[0..7], &[0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20]);
    let name = "Bätz".as_bytes();
    assert_eq!(&track[7..10], &[0x00, 0xFF, 0x03]);
    assert_eq!(track[10] as usize, name.len());
    assert_eq!(&track[11..11 + name.len()], name);
    // PC-event volgt direct op de naam.
    assert_eq!(&track[11 + name.len()..11 + name.len() + 3], &[0x00, 0xC0, 0x05]);
    // Zonder naam: byte-identiek aan de wrapper (geen FF 03).
    assert_eq!(encode_smf_with_name(&events, None), encode_smf(&events));
    assert!(!encode_smf(&events).windows(2).any(|w| w == [0xFF, 0x03]));
    // Lege naam → geen meta.
    assert_eq!(encode_smf_with_name(&events, Some("")), encode_smf(&events));
    // Lange naam wordt op tekens afgekapt (120) en blijft geldige UTF-8.
    let long: String = std::iter::repeat('ä').take(200).collect();
    let d2 = encode_smf_with_name(&events, Some(&long));
    let t2 = &d2[22..];
    // 120 tekens × 2 bytes = 240 → var-len in twee bytes (0x81 0x70).
    assert_eq!(&t2[7..12], &[0x00, 0xFF, 0x03, 0x81, 0x70]);
    let smf = midly::Smf::parse(&d2).expect("geldige SMF");
    let has_name = smf.tracks[0].iter().any(|ev| matches!(ev.kind,
        midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(n)) if n.len() == 240));
    assert!(has_name);
}

#[test]
fn encodes_valid_smf_header_and_trailer() {
    // Twee noten (on/off) op kanaal 0.
    let events = vec![
        (0u64, 0x90u8, 60u8, 100u8, 0u8),
        (500_000u64, 0x80u8, 60u8, 0u8, 0u8),
    ];
    let data = encode_smf(&events);

    // Header chunk
    assert_eq!(&data[0..4], b"MThd");
    assert_eq!(be_u32(&data, 4), 6); // header length
    assert_eq!(&data[8..10], &[0x00, 0x00]); // format 0
    assert_eq!(&data[10..12], &[0x00, 0x01]); // 1 track
    assert_eq!(&data[12..14], &[0x01, 0xE0]); // 480 PPQ

    // Track chunk
    assert_eq!(&data[14..18], b"MTrk");
    let track_len = be_u32(&data, 18) as usize;
    let track = &data[22..];
    assert_eq!(track.len(), track_len, "MTrk-lengteveld moet kloppen met de werkelijke trackbytes");
    // Eindigt op end-of-track meta-event.
    assert_eq!(&track[track.len() - 3..], &[0xFF, 0x2F, 0x00]);
    // Bevat het NoteOn-statusbyte ergens in de track.
    assert!(track.contains(&0x90));
}

#[test]
fn program_change_omits_second_data_byte() {
    // PC heeft maar één databyte; de writer mag data2 niet meeschrijven.
    let events = vec![(0u64, 0xC0u8, 5u8, 0u8, 0u8)];
    let data = encode_smf(&events);
    let track_len = be_u32(&data, 18) as usize;
    let track = &data[22..22 + track_len];
    // Track: [tempo-meta (7 bytes)] [delta=0 (1)] [0xC0] [05] [eot (4)]
    // Zoek het 0xC0-event en controleer dat het direct gevolgd wordt door 0x05 en daarna de eot-delta.
    let pc_pos = track.iter().position(|&b| b == 0xC0).expect("PC-event aanwezig");
    assert_eq!(track[pc_pos + 1], 0x05);
    // Na de PC (1 databyte) volgt de end-of-track delta (0x00) + 0xFF 0x2F 0x00.
    assert_eq!(&track[pc_pos + 2..pc_pos + 6], &[0x00, 0xFF, 0x2F, 0x00]);
}
