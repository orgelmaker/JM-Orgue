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

/// Het afspeel-keuzemenu leunt op vpo_midi::scan_channels: welke kanalen zitten
/// er in het bestand, met notental en bereik. Twee kanalen, elk een eigen noot.
#[test]
fn scan_channels_vindt_kanalen_en_bereik() {
    let events = vec![
        (0u64, 0x90u8, 60u8, 100u8, 0u8),   // kanaal 0 (gebruiker: 1), noot 60
        (0u64, 0x90u8, 72u8, 90u8, 0u8),    // kanaal 0, noot 72 → bereik 60..72
        (0u64, 0x92u8, 36u8, 80u8, 0u8),    // kanaal 2 (gebruiker: 3), noot 36
        (240u64, 0x80u8, 60u8, 0u8, 0u8),
        (240u64, 0x80u8, 72u8, 0u8, 0u8),
        (240u64, 0x82u8, 36u8, 0u8, 0u8),
    ];
    let data = super::encode_smf(&events);
    let dir = std::env::temp_dir().join(format!("jm_scan_{}.mid", std::process::id()));
    std::fs::write(&dir, &data).unwrap();
    let ks = vpo_midi::scan_channels(&dir).unwrap();
    std::fs::remove_file(&dir).ok();
    assert_eq!(ks.len(), 2, "twee kanalen met noten");
    assert_eq!((ks[0].channel, ks[0].note_count, ks[0].low, ks[0].high), (0, 2, 60, 72));
    assert_eq!((ks[1].channel, ks[1].note_count, ks[1].low, ks[1].high), (2, 1, 36, 36));
}

/// Het afspeel-keuzemenu bouwt per kanaal een routeringsregel. Kanaal 1-16 → 0-15,
/// octaaf → transpose (×12), een regel zonder divisie valt weg, en de regel routeert
/// de noot naar het gekozen klavier met de octaafverschuiving.
#[test]
fn bouw_speler_routering_zet_kanaal_octaaf_en_klavier() {
    use super::{bouw_speler_routering, PlayerRouteDto};
    let routes = vec![
        PlayerRouteDto { channel: 1, division: Some("Hauptwerk".into()), octave: 1 },
        PlayerRouteDto { channel: 2, division: Some(String::new()), octave: 0 }, // niet laten klinken
        PlayerRouteDto { channel: 3, division: Some("Pedal".into()), octave: -1 },
        PlayerRouteDto { channel: 4, division: None, octave: 0 },                 // ook weg
    ];
    let m = bouw_speler_routering(&routes);
    assert_eq!(m.len(), 2, "alleen regels met een klavier");

    assert_eq!(m[0].division, "Hauptwerk");
    assert_eq!(m[0].channel, Some(0));      // kanaal 1 → intern 0
    assert_eq!(m[0].transpose, 12);         // +1 octaaf
    assert!(m[0].accepts(0, 60));           // kanaal klopt
    assert!(!m[0].accepts(1, 60));          // ander kanaal niet
    assert!(m[0].accepts(0, 20));           // geen bereikgrens: elke noot
    assert_eq!(m[0].speelnoot(60), 72);     // een octaaf hoger

    assert_eq!(m[1].division, "Pedal");
    assert_eq!(m[1].channel, Some(2));      // kanaal 3 → intern 2
    assert_eq!(m[1].transpose, -12);
    assert_eq!(m[1].speelnoot(48), 36);
}
