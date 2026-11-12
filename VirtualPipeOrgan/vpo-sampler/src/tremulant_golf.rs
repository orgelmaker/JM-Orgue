//! De opgenomen tremulant-golfvorm van een Hauptwerk-set als modulatietabel (C5, stap 2).
//!
//! Een golfvorm-WAV (`…TremulantPitchAndFundamentalAmplitudeWaveform.wav`) is geen geluid maar een gemeten
//! modulatiecurve: links de toonhoogte en rechts de sterkte van de grondtoon, als fractie (±0,01 = ±1 %). De lus uit de
//! `smpl`-chunk omvat één of meer cycli; hoeveel, staat als grondtoon van de lus zeven octaven hoger (de unity-noot ÷ 128,
//! want MIDI kent geen 6 Hz), en anders telt deze module de nuldoorgangen. Zie `C5-golfvormen-onderzoek.md`.
//!
//! Uit dit bestand komt een tabel van `PUNTEN` stappen per cyclus (toonhoogte, sterkte), zonder gemiddelde op de
//! sterkte: de tremulant maakt een pijp niet blijvend harder of zachter.

use std::path::Path;

/// Punten per cyclus in de tabel.
pub const PUNTEN: usize = 512;

/// Eén cyclus van de modulatie: per punt (toonhoogte als fractie van de frequentie, sterkte als fractie).
#[derive(Debug, Clone, PartialEq)]
pub struct GolfTabel {
    pub punten: Vec<[f32; 2]>,
    /// Hoeveel cycli de lus in het bestand had (ter informatie).
    pub cycli_in_lus: u32,
}

struct Wav {
    kanalen: usize,
    rate: u32,
    frames: Vec<[f32; 2]>,
    lus: Option<(usize, usize)>,
    unity: Option<f64>,
}

fn lees_wav(bytes: &[u8]) -> Result<Wav, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("geen WAV-bestand".into());
    }
    let u16_ = |b: &[u8]| u16::from_le_bytes([b[0], b[1]]);
    let u32_ = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let (mut fmt, mut data, mut smpl) = (None, None, None);
    let mut i = 12usize;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let n = u32_(&bytes[i + 4..i + 8]) as usize;
        let eind = (i + 8).saturating_add(n).min(bytes.len());
        let body = &bytes[i + 8..eind];
        match id {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some(body),
            b"smpl" => smpl = Some(body),
            _ => {}
        }
        i = eind + (n & 1);
    }
    let fmt = fmt.filter(|f| f.len() >= 16).ok_or("geen fmt-chunk")?;
    let data = data.ok_or("geen data-chunk")?;
    let (code, kanalen, rate, bits) = (u16_(&fmt[0..2]), u16_(&fmt[2..4]) as usize, u32_(&fmt[4..8]), u16_(&fmt[14..16]));
    let code = if code == 0xFFFE && fmt.len() >= 26 { u16_(&fmt[24..26]) } else { code };
    if kanalen == 0 || rate == 0 {
        return Err("ongeldig formaat".into());
    }
    let bps = (bits as usize).div_ceil(8);
    let frame = bps * kanalen;
    if frame == 0 {
        return Err("ongeldig formaat".into());
    }
    let waarde = |b: &[u8]| -> Option<f32> {
        match (code, bits) {
            (3, 32) => Some(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            (1, 16) => Some(i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0),
            (1, 24) => Some((((b[0] as i32) << 8 | (b[1] as i32) << 16 | (b[2] as i32) << 24) >> 8) as f32 / 8_388_608.0),
            (1, 32) => Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0),
            _ => None,
        }
    };
    let mut frames = Vec::with_capacity(data.len() / frame);
    for f in data.chunks_exact(frame) {
        let l = waarde(&f[0..bps]).ok_or("niet ondersteund sampleformaat")?;
        let r = if kanalen > 1 { waarde(&f[bps..2 * bps]).ok_or("niet ondersteund sampleformaat")? } else { l };
        frames.push([l, r]);
    }
    let (mut lus, mut unity) = (None, None);
    if let Some(s) = smpl.filter(|s| s.len() >= 36) {
        let noot = u32_(&s[12..16]) as f64 + u32_(&s[16..20]) as f64 / 4_294_967_296.0;
        unity = Some(noot);
        if u32_(&s[28..32]) > 0 && s.len() >= 60 {
            let (st, en) = (u32_(&s[44..48]) as usize, u32_(&s[48..52]) as usize);
            if en > st && en < frames.len() {
                lus = Some((st, en));
            }
        }
    }
    Ok(Wav { kanalen, rate, frames, lus, unity })
}

/// Het aantal cycli in de lus: uit de unity-noot (frequentie ÷ 128) als dat een geheel aantal oplevert, anders de
/// stijgende nuldoorgangen van de toonhoogtecurve (zonder gemiddelde), rond de lus geteld, minstens 1.
fn cycli(lus: &[[f32; 2]], rate: u32, unity: Option<f64>) -> u32 {
    let duur = lus.len() as f64 / rate as f64;
    if let Some(noot) = unity {
        let hz = 440.0 * 2f64.powf((noot - 69.0) / 12.0) / 128.0;
        let k = hz * duur;
        if k >= 0.75 && (k - k.round()).abs() < 0.1 {
            return k.round() as u32;
        }
    }
    let gem = lus.iter().map(|p| p[0] as f64).sum::<f64>() / lus.len().max(1) as f64;
    // Rond de lus geteld (ook van het laatste naar het eerste punt): een lus die precies op een doorgang begint, telt
    // die doorgang dan één keer.
    let n = (0..lus.len())
        .filter(|&i| (lus[i][0] as f64) < gem && (lus[(i + 1) % lus.len()][0] as f64) >= gem)
        .count() as u32;
    n.max(1)
}

/// Tabel uit de bytes van een golfvorm-WAV.
pub fn golftabel_uit_bytes(bytes: &[u8]) -> Result<GolfTabel, String> {
    let wav = lees_wav(bytes)?;
    let (st, en) = wav.lus.unwrap_or((0, wav.frames.len().saturating_sub(1)));
    let lus = &wav.frames[st..=en];
    if lus.len() < 8 {
        return Err("lus te kort".into());
    }
    let k = cycli(lus, wav.rate, wav.unity);
    // De lus wordt als één geheel naar k × PUNTEN punten herbemonsterd en daarna tot één cyclus gemiddeld: zo blijft
    // de vorm, ook als de cycli in de lus iets verschillen.
    let n = lus.len();
    let totaal = PUNTEN * k as usize;
    let mut punten = vec![[0.0f32; 2]; PUNTEN];
    for j in 0..totaal {
        let x = j as f64 * n as f64 / totaal as f64;
        let i0 = x.floor() as usize % n;
        let i1 = (i0 + 1) % n;
        let t = (x - x.floor()) as f32;
        let p = &mut punten[j % PUNTEN];
        for c in 0..2 {
            p[c] += lus[i0][c] + (lus[i1][c] - lus[i0][c]) * t;
        }
    }
    for p in punten.iter_mut() {
        p[0] /= k as f32;
        p[1] /= k as f32;
    }
    // Een monobestand heeft geen aparte sterkte: alleen toonhoogte.
    if wav.kanalen == 1 {
        for p in punten.iter_mut() {
            p[1] = 0.0;
        }
    }
    let gem_sterkte = punten.iter().map(|p| p[1]).sum::<f32>() / PUNTEN as f32;
    for p in punten.iter_mut() {
        p[1] -= gem_sterkte;
    }
    Ok(GolfTabel { punten, cycli_in_lus: k })
}

/// Tabel uit een golfvorm-bestand op schijf.
pub fn lees_golftabel(pad: &Path) -> Result<GolfTabel, String> {
    let bytes = std::fs::read(pad).map_err(|e| format!("{}: {}", pad.display(), e))?;
    golftabel_uit_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Een stereo-float-WAV met `n` frames, lus over alles en de gegeven unity-noot.
    fn wav(n: usize, rate: u32, links: impl Fn(usize) -> f32, rechts: impl Fn(usize) -> f32, unity: (u32, u32)) -> Vec<u8> {
        let mut data = Vec::new();
        for i in 0..n {
            data.extend_from_slice(&links(i).to_le_bytes());
            data.extend_from_slice(&rechts(i).to_le_bytes());
        }
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&3u16.to_le_bytes());
        fmt.extend_from_slice(&2u16.to_le_bytes());
        fmt.extend_from_slice(&rate.to_le_bytes());
        fmt.extend_from_slice(&(rate * 8).to_le_bytes());
        fmt.extend_from_slice(&8u16.to_le_bytes());
        fmt.extend_from_slice(&32u16.to_le_bytes());
        let mut smpl = vec![0u8; 36];
        smpl[12..16].copy_from_slice(&unity.0.to_le_bytes());
        smpl[16..20].copy_from_slice(&unity.1.to_le_bytes());
        smpl[28..32].copy_from_slice(&1u32.to_le_bytes());
        let mut lus = vec![0u8; 24];
        lus[8..12].copy_from_slice(&0u32.to_le_bytes());
        lus[12..16].copy_from_slice(&((n - 1) as u32).to_le_bytes());
        smpl.extend_from_slice(&lus);
        let mut out = b"RIFF\0\0\0\0WAVE".to_vec();
        for (id, body) in [(b"fmt ", &fmt), (b"smpl", &smpl), (b"data", &data)] {
            out.extend_from_slice(id);
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(body);
        }
        out
    }

    #[test]
    fn twee_sinuscycli_zonder_unity_worden_een_cyclus() {
        // Zoals de kunstmatige golfvorm van één set: 0,4 s lus met twee cycli van 5 Hz, unity-noot 60 (niet ingevuld).
        let rate = 1000;
        let s = |i: usize| 0.1585 * (2.0 * std::f32::consts::PI * 5.0 * i as f32 / rate as f32).sin();
        let t = golftabel_uit_bytes(&wav(400, rate, s, s, (60, 0))).unwrap();
        assert_eq!(t.cycli_in_lus, 2);
        let max = t.punten.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        assert!((max - 0.1585).abs() < 0.005, "top {}", max);
        // Een kwart cyclus verder ligt de top.
        assert!((t.punten[PUNTEN / 4][0] - 0.1585).abs() < 0.005);
    }

    #[test]
    fn unity_noot_geeft_het_aantal_cycli() {
        // 6,375 Hz × 128 = 816 Hz ≈ MIDI 79,69; een lus van 1 cyclus bij 22050 Hz.
        let rate = 22050;
        let n = (rate as f64 / 6.375).round() as usize;
        let noot = 69.0 + 12.0 * (816.0f64 / 440.0).log2();
        let frac = ((noot.fract()) * 4_294_967_296.0) as u32;
        let p = |i: usize| 0.01 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).cos();
        let a = |i: usize| 0.065 * (2.0 * std::f32::consts::PI * i as f32 / n as f32).sin() + 0.02;
        let t = golftabel_uit_bytes(&wav(n, rate, p, a, (noot.trunc() as u32, frac))).unwrap();
        assert_eq!(t.cycli_in_lus, 1);
        assert!((t.punten[0][0] - 0.01).abs() < 1e-3);
        // De sterkte zonder gemiddelde.
        let gem: f32 = t.punten.iter().map(|p| p[1]).sum::<f32>() / PUNTEN as f32;
        assert!(gem.abs() < 1e-5);
    }

    #[test]
    fn geen_wav_geeft_een_fout() {
        assert!(golftabel_uit_bytes(b"geen wav").is_err());
    }
}
