use super::*;

#[test]
fn layer_key_zet_laag_in_bits_16_23() {
    assert_eq!(layer_key(5, 0), 5);
    assert_eq!(layer_key(5, 2), 0x0002_0005);
    assert_eq!(layer_key(1024, 15), 0x000F_0400);
    // Geen overlap met de release-bits (24–31): een release-variant op een
    // laagsleutel houdt beide intact.
    assert_eq!(layer_key(1024, 15) & RELEASE_KEY_MASK, 0);
    let rel = layer_key(7, 3) | RELEASE_PIPE_FLAG | (3 << 24);
    assert_eq!(rel & RANK_MASK, 3 << RANK_SHIFT);
    assert_eq!(rel & RELEASE_KEY_MASK, RELEASE_PIPE_FLAG | (3 << 24));
}

#[test]
fn base_pipe_strip_laag_en_release_bits() {
    assert_eq!(base_pipe(layer_key(5, 2) | RELEASE_PIPE_FLAG | (3 << 24)), 5);
    assert_eq!(base_pipe(layer_key(1536, 15)), 1536);
    assert_eq!(base_pipe(42), 42);
    // Laagsleutel zonder release-bits blijft de ODF-intonatie-sleutel.
    assert_eq!(layer_key(9, 4) & !RELEASE_KEY_MASK, layer_key(9, 4));
}

#[test]
fn trem_flag_ligt_binnen_het_release_masker() {
    // De vlag moet door base_pipe() en de intonatiesleutel gestript worden,
    // anders krijgt een tremulant-voice geen ODF-gain/hertemper-correctie.
    assert_eq!(TREM_FLAG & RELEASE_KEY_MASK, TREM_FLAG);
    assert_eq!(TREM_FLAG & (RANK_MASK | 0xFFFF), 0);
    assert_eq!(base_pipe(layer_key(5, 2) | TREM_FLAG), 5);
    assert_eq!(base_pipe(layer_key(5, 2) | TREM_FLAG | RELEASE_PIPE_FLAG | (3 << 24)), 5);
    let k = layer_key(9, 4);
    assert_eq!((k | TREM_FLAG) & !RELEASE_KEY_MASK, k);
    // Geen botsing met de release-index (idx+1) << 24 voor idx < 7.
    for idx in 0u32..7 {
        assert_eq!(((idx + 1) << 24) & TREM_FLAG, 0);
    }
    // En de laag blijft intact naast de vlag.
    assert_eq!((layer_key(7, 3) | TREM_FLAG) & RANK_MASK, 3 << RANK_SHIFT);
}
