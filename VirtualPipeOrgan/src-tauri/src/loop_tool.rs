//! Sampleset loop tools.
//!
//! Many home-made sample sets (e.g. recorded organ pipes) ship WITHOUT loop
//! points, so a held note stops when the recording ends. This module detects a
//! good sustain loop in each WAV, bakes an equal-power crossfade at the seam so
//! the wrap is click-free, and writes the loop into the WAV `smpl` chunk so it
//! persists (and is read back by the loader / other software).
//!
//! Only WAV is handled (MP3 can't carry loop points). Originals are backed up to
//! a sibling `<name>_loopbackup` folder before being overwritten.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tracing::{info, warn};

/// Don't loop samples shorter than this (need attack + a usable sustain).
const MIN_DURATION_S: f32 = 0.6;
/// Target maximum loop length.
const MAX_LOOP_S: f32 = 1.2;
/// Minimum acceptable loop length.
const MIN_LOOP_S: f32 = 0.20;
/// Max crossfade length baked at the seam.
const MAX_XFADE_S: f32 = 0.05;

#[derive(Debug, Clone, Serialize)]
pub struct LoopItem {
    pub rel_path: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub duration_ms: f64,
    pub had_loop: bool,
    pub detected: bool,
    pub loop_start: u64,
    pub loop_end: u64,
    pub period: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LoopScanReport {
    pub root: String,
    pub files_scanned: usize,
    pub files_without_loop: usize,
    pub files_loopable: usize,
    pub items: Vec<LoopItem>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LoopApplyReport {
    pub root: String,
    pub backup_dir: String,
    pub files_looped: usize,
    pub files_skipped: usize,
    pub errors: Vec<String>,
}

fn ext_lower(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default()
}
fn is_wav(p: &Path) -> bool { ext_lower(p) == "wav" }

fn resolve_root(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_file() { p.parent().map(|x| x.to_path_buf()).unwrap_or(p) } else { p }
}

fn collect_wav_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match fs::read_dir(dir) { Ok(rd) => rd, Err(_) => return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            // Skip our own backup folders.
            let name = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            if name.contains("_loopbackup") || name.contains("_backup") { continue; }
            collect_wav_files(&p, out);
        } else if is_wav(&p) {
            out.push(p);
        }
    }
}

/// MIDI note parsed from a leading number in the filename ("036-c.wav" -> 36).
pub(crate) fn midi_note_from_name(path: &Path) -> u8 {
    let stem = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let digits: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u8>().unwrap_or(60)
}

/// Loaded WAV in working form: interleaved samples kept in their native domain
/// (i32 for PCM, f32 for float), plus the spec for writing back unchanged.
struct LoadedWav {
    spec: hound::WavSpec,
    /// Interleaved. For Int specs these are i32 sample values (native range); for
    /// Float they are the raw f32. We process in f32 and convert back on write.
    data_f32: Vec<f32>,
    is_float: bool,
}

fn read_wav(path: &Path) -> Result<LoadedWav, String> {
    let reader = hound::WavReader::open(path).map_err(|e| format!("open: {}", e))?;
    let spec = reader.spec();
    let is_float = matches!(spec.sample_format, hound::SampleFormat::Float);
    let data_f32: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.into_samples::<f32>()
            .collect::<Result<_, _>>().map_err(|e| format!("read: {}", e))?,
        hound::SampleFormat::Int => reader.into_samples::<i32>()
            .collect::<Result<Vec<i32>, _>>().map_err(|e| format!("read: {}", e))?
            .into_iter().map(|v| v as f32).collect(),
    };
    Ok(LoadedWav { spec, data_f32, is_float })
}

/// Mono mix (average of channels) for analysis.
fn mono_mix(inter: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 { return inter.to_vec(); }
    let frames = inter.len() / channels;
    let mut m = Vec::with_capacity(frames);
    for f in 0..frames {
        let base = f * channels;
        let mut s = 0.0f32;
        for c in 0..channels { s += inter[base + c]; }
        m.push(s / channels as f32);
    }
    m
}

/// Detect the fundamental period (in frames) via normalized autocorrelation over
/// a window starting at `from`. Returns None if no clear pitch is found.
fn detect_period(mono: &[f32], from: usize, sample_rate: u32) -> Option<usize> {
    let min_lag = (sample_rate as usize / 1500).max(16); // up to ~1500 Hz
    let max_lag = (sample_rate as usize / 55).max(min_lag + 1); // down to ~55 Hz
    let win = (sample_rate as usize / 2).min(mono.len().saturating_sub(from)); // ~0.5 s
    if win <= max_lag * 2 { return None; }
    let seg = &mono[from..from + win];
    let n = win - max_lag;

    let energy0: f32 = seg[..n].iter().map(|x| x * x).sum();
    if energy0 < 1e-6 { return None; }

    let mut best_lag = 0usize;
    let mut best_r = 0.0f32;
    let mut r_at: Vec<f32> = vec![0.0; max_lag + 1];
    for lag in min_lag..=max_lag {
        let mut num = 0.0f32;
        let mut den2 = 0.0f32;
        for i in 0..n {
            num += seg[i] * seg[i + lag];
            den2 += seg[i + lag] * seg[i + lag];
        }
        let den = (energy0 * den2).sqrt();
        let r = if den > 1e-9 { num / den } else { 0.0 };
        r_at[lag] = r;
        if r > best_r { best_r = r; best_lag = lag; }
    }
    if best_r < 0.5 { return None; } // not pitched enough to loop reliably

    // Prefer the smallest lag that is near the global peak (avoids octave-down
    // errors where autocorrelation also peaks at multiples of the true period).
    let thresh = best_r * 0.92;
    for lag in min_lag..=best_lag {
        if r_at[lag] >= thresh
            && r_at[lag] >= r_at[lag - 1]
            && (lag + 1 > max_lag || r_at[lag] >= r_at[lag + 1])
        {
            return Some(lag);
        }
    }
    Some(best_lag)
}

/// Sum of squared differences between `a[ls..]` and `a[e..]` over `w` samples.
fn seam_ssd(a: &[f32], ls: usize, e: usize, w: usize) -> f32 {
    let mut s = 0.0f32;
    for k in 0..w {
        let d = a[ls + k] - a[e + k];
        s += d * d;
    }
    s
}

/// Detected loop in frames: (loop_start, loop_end_exclusive, period).
fn detect_loop(mono: &[f32], sample_rate: u32) -> Option<(usize, usize, usize)> {
    let frames = mono.len();
    if (frames as f32) < MIN_DURATION_S * sample_rate as f32 { return None; }

    // Search the steady sustain: skip the attack, leave the tail alone.
    let search_start = ((frames as f32 * 0.35) as usize).max((0.25 * sample_rate as f32) as usize);
    let search_end = (frames as f32 * 0.88) as usize;
    if search_end <= search_start + sample_rate as usize / 4 { return None; }

    let period = detect_period(mono, search_start, sample_rate)?;
    if period < 8 { return None; }

    // loop_start: first upward zero-crossing at/after search_start (cosmetic; the
    // crossfade does the real work).
    let mut ls = search_start;
    let zc_limit = (search_start + period).min(frames - 1);
    while ls + 1 < zc_limit {
        if mono[ls] <= 0.0 && mono[ls + 1] > 0.0 { break; }
        ls += 1;
    }

    // Loop length: as many whole periods as fit up to MAX_LOOP_S, ≥ MIN_LOOP_S.
    let max_len = ((MAX_LOOP_S * sample_rate as f32) as usize).min(search_end - ls);
    let min_len = (MIN_LOOP_S * sample_rate as f32) as usize;
    if max_len < min_len.max(2 * period) { return None; }
    let n_periods = (max_len / period).max(1);
    let approx_le = ls + n_periods * period;

    // Refine loop_end within ±half a period to best phase-match loop_start.
    let w = period.min(512).max(32);
    let lo = approx_le.saturating_sub(period / 2).max(ls + period);
    let hi = (approx_le + period / 2).min(frames - w - 1);
    if hi <= lo { return None; }
    let mut best_e = approx_le.min(hi);
    let mut best = f32::INFINITY;
    for e in lo..=hi {
        let s = seam_ssd(mono, ls, e, w);
        if s < best { best = s; best_e = e; }
    }
    if best_e <= ls + min_len.min(period) { return None; }
    Some((ls, best_e, period))
}

/// Bake an equal-power crossfade at the loop seam, per channel. The loop tail
/// `[le-xf, le)` is blended with the pre-loop region `[ls-xf, ls)` so that the
/// wrap le→ls is continuous.
fn bake_crossfade(inter: &mut [f32], channels: usize, ls: usize, le: usize, period: usize, sample_rate: u32) {
    let max_xf = (MAX_XFADE_S * sample_rate as f32) as usize;
    let mut xf = (period * 2).min(max_xf).min((le - ls) / 3);
    if xf > ls { xf = ls; } // need pre-loop audio
    if xf < 8 { return; }
    for c in 0..channels {
        for i in 0..xf {
            let t = (i as f32 + 0.5) / xf as f32;
            // Equal-power fades: tail fades out, pre-loop fades in.
            let fo = (t * std::f32::consts::FRAC_PI_2).cos();
            let fi = (t * std::f32::consts::FRAC_PI_2).sin();
            let tail_idx = (le - xf + i) * channels + c;
            let pre_idx = (ls - xf + i) * channels + c;
            inter[tail_idx] = inter[tail_idx] * fo + inter[pre_idx] * fi;
        }
    }
}

/// Build a `smpl` chunk (one forward loop) as raw bytes.
pub(crate) fn build_smpl_chunk(sample_rate: u32, midi_note: u8, loop_start: u32, loop_end_incl: u32) -> Vec<u8> {
    let mut d = Vec::with_capacity(68);
    let mut w32 = |v: u32, out: &mut Vec<u8>| out.extend_from_slice(&v.to_le_bytes());
    let sample_period = if sample_rate > 0 { 1_000_000_000u32 / sample_rate } else { 0 };
    let mut body = Vec::with_capacity(60);
    w32(0, &mut body);                 // manufacturer
    w32(0, &mut body);                 // product
    w32(sample_period, &mut body);     // sample period (ns)
    w32(midi_note as u32, &mut body);  // MIDI unity note
    w32(0, &mut body);                 // MIDI pitch fraction
    w32(0, &mut body);                 // SMPTE format
    w32(0, &mut body);                 // SMPTE offset
    w32(1, &mut body);                 // num sample loops
    w32(0, &mut body);                 // sampler data
    // loop record
    w32(0, &mut body);                 // cue point id
    w32(0, &mut body);                 // type: 0 = forward
    w32(loop_start, &mut body);        // start frame
    w32(loop_end_incl, &mut body);     // end frame (inclusive)
    w32(0, &mut body);                 // fraction
    w32(0, &mut body);                 // play count (0 = infinite)
    d.extend_from_slice(b"smpl");
    d.extend_from_slice(&(body.len() as u32).to_le_bytes());
    d.extend_from_slice(&body);
    d
}

/// Insert a `smpl` chunk into a RIFF/WAVE byte buffer (right before the `data`
/// chunk) and fix the RIFF size.
pub(crate) fn insert_smpl(wav: &[u8], smpl: &[u8]) -> Result<Vec<u8>, String> {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE buffer".into());
    }
    // Walk chunks from offset 12 to find "data".
    let mut pos = 12usize;
    let mut data_pos = None;
    while pos + 8 <= wav.len() {
        let id = &wav[pos..pos + 4];
        let size = u32::from_le_bytes([wav[pos + 4], wav[pos + 5], wav[pos + 6], wav[pos + 7]]) as usize;
        if id == b"data" { data_pos = Some(pos); break; }
        pos += 8 + size + (size & 1); // chunks are word-aligned
    }
    let insert_at = data_pos.ok_or("no data chunk")?;
    let mut out = Vec::with_capacity(wav.len() + smpl.len());
    out.extend_from_slice(&wav[..insert_at]);
    out.extend_from_slice(smpl);
    out.extend_from_slice(&wav[insert_at..]);
    // Fix RIFF size = file size - 8.
    let riff_size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());
    Ok(out)
}

/// Encode interleaved samples to a WAV byte buffer via hound (native format).
pub(crate) fn encode_wav(spec: hound::WavSpec, data_f32: &[f32], is_float: bool) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = hound::WavWriter::new(cursor, spec).map_err(|e| format!("wav writer: {}", e))?;
        if is_float {
            for &s in data_f32 { writer.write_sample(s).map_err(|e| format!("write: {}", e))?; }
        } else {
            // Native i32 values were widened to f32 on read; round back.
            let maxv = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32 - 1.0;
            for &s in data_f32 {
                let v = s.round().clamp(-(maxv + 1.0), maxv) as i32;
                writer.write_sample(v).map_err(|e| format!("write: {}", e))?;
            }
        }
        writer.finalize().map_err(|e| format!("finalize: {}", e))?;
    }
    Ok(buf)
}

/// Analyze one WAV: does it have a loop, and can we detect one?
fn analyze(path: &Path) -> Result<LoopItem, String> {
    let reader = hound::WavReader::open(path).map_err(|e| format!("open: {}", e))?;
    let spec = reader.spec();
    drop(reader);
    let had_loop = crate_has_smpl(path);

    let w = read_wav(path)?;
    let channels = spec.channels.max(1) as usize;
    let frames = w.data_f32.len() / channels;
    let mono = mono_mix(&w.data_f32, channels);
    let detected = detect_loop(&mono, spec.sample_rate);
    let (det, ls, le, period) = match detected {
        Some((ls, le, p)) => (true, ls as u64, le as u64, p as u32),
        None => (false, 0, 0, 0),
    };
    Ok(LoopItem {
        rel_path: String::new(), // filled by caller
        channels: spec.channels,
        sample_rate: spec.sample_rate,
        duration_ms: frames as f64 / spec.sample_rate.max(1) as f64 * 1000.0,
        had_loop,
        detected: det,
        loop_start: ls,
        loop_end: le,
        period,
    })
}

/// Cheap check whether a WAV already contains a `smpl` chunk.
fn crate_has_smpl(path: &Path) -> bool {
    use std::io::Read;
    let mut f = match fs::File::open(path) { Ok(f) => f, Err(_) => return false };
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() { return false; }
    buf.windows(4).any(|w| w == b"smpl")
}

/// Scan a folder: count WAVs without loops and how many we can loop.
pub fn scan(directory: &str) -> LoopScanReport {
    let root = resolve_root(directory);
    let mut files = Vec::new();
    collect_wav_files(&root, &mut files);
    let mut report = LoopScanReport {
        root: root.to_string_lossy().to_string(),
        files_scanned: files.len(),
        ..Default::default()
    };
    for path in &files {
        match analyze(path) {
            Ok(mut item) => {
                item.rel_path = path.strip_prefix(&root).unwrap_or(path).to_string_lossy().to_string();
                if !item.had_loop { report.files_without_loop += 1; }
                if item.detected { report.files_loopable += 1; }
                report.items.push(item);
            }
            Err(e) => report.errors.push(format!("{}: {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), e)),
        }
    }
    info!("Loop scan: {} WAVs, {} without loop, {} loopable", report.files_scanned, report.files_without_loop, report.files_loopable);
    report
}

fn backup_file(src: &Path, root: &Path, backup_root: &Path) -> Result<(), String> {
    let rel = src.strip_prefix(root).unwrap_or(src);
    let dest = backup_root.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("backup mkdir: {}", e))?;
    }
    if !dest.exists() {
        fs::copy(src, &dest).map_err(|e| format!("backup copy: {}", e))?;
    }
    Ok(())
}

/// Process one WAV: detect a loop, bake the crossfade, write the smpl chunk.
/// Returns true if a loop was written. `only_missing` skips files that already
/// have a loop.
fn loop_one(path: &Path, only_missing: bool) -> Result<bool, String> {
    if only_missing && crate_has_smpl(path) { return Ok(false); }
    let mut w = read_wav(path)?;
    let channels = w.spec.channels.max(1) as usize;
    let mono = mono_mix(&w.data_f32, channels);
    let (ls, le, period) = match detect_loop(&mono, w.spec.sample_rate) {
        Some(v) => v,
        None => return Ok(false),
    };
    bake_crossfade(&mut w.data_f32, channels, ls, le, period, w.spec.sample_rate);
    let wav = encode_wav(w.spec, &w.data_f32, w.is_float)?;
    // smpl end is the inclusive last frame of the loop.
    let smpl = build_smpl_chunk(w.spec.sample_rate, midi_note_from_name(path), ls as u32, le.saturating_sub(1) as u32);
    let out = insert_smpl(&wav, &smpl)?;
    fs::write(path, &out).map_err(|e| format!("write: {}", e))?;
    Ok(true)
}

/// Detect + bake loops for every WAV in `directory`, backing up originals first.
/// `only_missing`: only process files that don't already have a loop.
pub fn apply(directory: &str, only_missing: bool) -> LoopApplyReport {
    let root = resolve_root(directory);
    let parent = root.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| root.clone());
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "sampleset".into());
    let mut backup_root = parent.join(format!("{}_loopbackup", name));
    let mut n = 2;
    while backup_root.exists() && n < 100 {
        backup_root = parent.join(format!("{}_loopbackup_{}", name, n));
        n += 1;
    }

    let mut report = LoopApplyReport {
        root: root.to_string_lossy().to_string(),
        backup_dir: backup_root.to_string_lossy().to_string(),
        ..Default::default()
    };
    let mut files = Vec::new();
    collect_wav_files(&root, &mut files);

    for path in &files {
        if let Err(e) = backup_file(path, &root, &backup_root) {
            report.errors.push(format!("{}: {}", path.display(), e));
            continue;
        }
        match loop_one(path, only_missing) {
            Ok(true) => report.files_looped += 1,
            Ok(false) => report.files_skipped += 1,
            Err(e) => report.errors.push(format!("{}: {}", path.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), e)),
        }
    }
    info!("Loop apply: {} looped, {} skipped, backup at {}", report.files_looped, report.files_skipped, report.backup_dir);
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_period_of_sine() {
        // 100 Hz sine at 48k -> period 480 frames.
        let sr = 48000u32;
        let n = sr as usize; // 1 s
        let period = 480.0f32;
        let mono: Vec<f32> = (0..n).map(|i| (i as f32 / period * std::f32::consts::TAU).sin()).collect();
        let p = detect_period(&mono, sr as usize / 4, sr).expect("period");
        // Allow octave/near matches but expect a multiple/divisor near 480.
        assert!((p as i32 - 480).abs() <= 5 || (p as i32 - 240).abs() <= 5 || (p as i32 - 960).abs() <= 5, "got {}", p);
    }

    #[test]
    fn detects_loop_and_builds_smpl() {
        let sr = 48000u32;
        let n = (sr as f32 * 2.0) as usize; // 2 s
        let period = 480.0f32;
        let mono: Vec<f32> = (0..n).map(|i| (i as f32 / period * std::f32::consts::TAU).sin() * 0.5).collect();
        let (ls, le, p) = detect_loop(&mono, sr).expect("loop");
        assert!(le > ls);
        assert!(p > 8);
        let smpl = build_smpl_chunk(sr, 60, ls as u32, (le - 1) as u32);
        assert_eq!(&smpl[0..4], b"smpl");
        assert_eq!(u32::from_le_bytes([smpl[4], smpl[5], smpl[6], smpl[7]]) as usize, smpl.len() - 8);
    }

    #[test]
    fn insert_smpl_into_minimal_wav() {
        // Minimal RIFF/WAVE with fmt + data.
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&0u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[0u8; 16]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&4u32.to_le_bytes());
        wav.extend_from_slice(&[1u8, 2, 3, 4]);
        let riff = (wav.len() - 8) as u32;
        wav[4..8].copy_from_slice(&riff.to_le_bytes());

        let smpl = build_smpl_chunk(48000, 60, 100, 500);
        let out = insert_smpl(&wav, &smpl).expect("insert");
        assert!(out.windows(4).any(|w| w == b"smpl"));
        // data must still be present and after smpl.
        let smpl_at = out.windows(4).position(|w| w == b"smpl").unwrap();
        let data_at = out.windows(4).position(|w| w == b"data").unwrap();
        assert!(smpl_at < data_at);
        assert_eq!(u32::from_le_bytes([out[4], out[5], out[6], out[7]]) as usize, out.len() - 8);
    }
}
