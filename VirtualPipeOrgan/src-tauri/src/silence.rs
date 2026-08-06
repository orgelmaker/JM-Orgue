//! Sampleset silence tools.
//!
//! Detects and trims **leading** silence from WAV/MP3 samples in an organ
//! sample folder. Trimming is non-lossy for WAV (native samples preserved) and
//! re-encodes MP3 (same format, as requested). Originals are backed up to a
//! sibling `<folder>_backup` directory before being overwritten.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tracing::{info, warn};

/// Only report/trim files whose leading silence exceeds this (avoids touching
/// samples that already start cleanly).
const MIN_LEADING_MS: f64 = 2.0;

/// Per-file silence info (for the scan report).
#[derive(Debug, Clone, Serialize)]
pub struct SilenceItem {
    pub rel_path: String,
    pub format: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub duration_ms: f64,
    pub leading_ms: f64,
}

/// Result of scanning a sample folder.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ScanReport {
    pub root: String,
    pub threshold_db: f32,
    pub files_scanned: usize,
    pub files_with_silence: usize,
    pub total_leading_ms: f64,
    pub items: Vec<SilenceItem>,
    pub errors: Vec<String>,
}

/// Result of trimming a sample folder.
#[derive(Debug, Clone, Serialize, Default)]
pub struct TrimReport {
    pub root: String,
    pub backup_dir: String,
    pub files_trimmed: usize,
    pub total_trimmed_ms: f64,
    pub errors: Vec<String>,
}

fn ext_lower(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default()
}
fn is_wav(p: &Path) -> bool { ext_lower(p) == "wav" }
fn is_mp3(p: &Path) -> bool { ext_lower(p) == "mp3" }

/// If `path` points to a `.organ` file, return its parent dir; otherwise the path.
fn resolve_root(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_file() {
        p.parent().map(|x| x.to_path_buf()).unwrap_or(p)
    } else {
        p
    }
}

fn collect_audio_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_audio_files(&p, out);
        } else if is_wav(&p) || is_mp3(&p) {
            out.push(p);
        }
    }
}

/// Index of the first frame whose peak (across channels) reaches `thr` (0..1).
/// Returns `frames` if the whole file is below threshold.
fn first_loud_frame_f32(samples: &[f32], channels: usize, thr: f32) -> usize {
    if channels == 0 { return 0; }
    let frames = samples.len() / channels;
    for f in 0..frames {
        let base = f * channels;
        let mut peak = 0.0f32;
        for c in 0..channels {
            let v = samples[base + c].abs();
            if v > peak { peak = v; }
        }
        if peak >= thr { return f; }
    }
    frames
}

/// Same as above for integer PCM, threshold given in the integer domain.
fn first_loud_frame_i32(samples: &[i32], channels: usize, thr_int: i64) -> usize {
    if channels == 0 { return 0; }
    let frames = samples.len() / channels;
    for f in 0..frames {
        let base = f * channels;
        let mut peak: i64 = 0;
        for c in 0..channels {
            let v = (samples[base + c] as i64).abs();
            if v > peak { peak = v; }
        }
        if peak >= thr_int { return f; }
    }
    frames
}

/// Lightweight info about an audio file's leading silence (for scanning).
struct LeadingInfo {
    channels: u16,
    sample_rate: u32,
    total_frames: usize,
    first_loud_frame: usize,
}

fn analyze_wav(path: &Path, thr_lin: f32) -> Result<LeadingInfo, String> {
    let reader = hound::WavReader::open(path).map_err(|e| format!("WAV open: {}", e))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1);
    let (total_frames, first) = match spec.sample_format {
        hound::SampleFormat::Float => {
            let samples: Vec<f32> = reader.into_samples::<f32>()
                .collect::<Result<_, _>>().map_err(|e| format!("WAV read: {}", e))?;
            let ch = channels as usize;
            (samples.len() / ch, first_loud_frame_f32(&samples, ch, thr_lin))
        }
        hound::SampleFormat::Int => {
            let samples: Vec<i32> = reader.into_samples::<i32>()
                .collect::<Result<_, _>>().map_err(|e| format!("WAV read: {}", e))?;
            let ch = channels as usize;
            let maxv = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            let thr_int = (thr_lin * maxv) as i64;
            (samples.len() / ch, first_loud_frame_i32(&samples, ch, thr_int))
        }
    };
    Ok(LeadingInfo { channels, sample_rate: spec.sample_rate, total_frames, first_loud_frame: first })
}

/// Decode an MP3 to interleaved f32, preserving channels (mirrors the sampler's
/// symphonia path but without the mono downmix).
pub(crate) fn decode_mp3_interleaved(path: &Path) -> Result<(Vec<f32>, u16, u32), String> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = fs::File::open(path).map_err(|e| format!("open: {}", e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("probe: {}", e))?;
    let mut format = probed.format;
    let track = format.tracks().iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("no audio track")?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2) as u16;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("decoder: {}", e))?;

    let mut out: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(format!("packet: {}", e)),
        };
        if packet.track_id() != track_id { continue; }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let frames = decoded.frames();
                let mut buf = SampleBuffer::<f32>::new(frames as u64, spec);
                buf.copy_interleaved_ref(decoded);
                out.extend(buf.samples());
            }
            Err(e) => warn!("decode packet: {}", e),
        }
    }
    Ok((out, channels, sample_rate))
}

fn analyze_mp3(path: &Path, thr_lin: f32) -> Result<LeadingInfo, String> {
    let (samples, channels, sample_rate) = decode_mp3_interleaved(path)?;
    let ch = channels.max(1) as usize;
    let total_frames = samples.len() / ch;
    let first = first_loud_frame_f32(&samples, ch, thr_lin);
    Ok(LeadingInfo { channels: channels.max(1), sample_rate, total_frames, first_loud_frame: first })
}

/// Scan a folder and report leading silence per audio file.
pub fn scan(directory: &str, threshold_db: f32) -> ScanReport {
    let root = resolve_root(directory);
    let thr_lin = 10f32.powf(threshold_db / 20.0);
    let mut files = Vec::new();
    collect_audio_files(&root, &mut files);

    let mut report = ScanReport {
        root: root.to_string_lossy().to_string(),
        threshold_db,
        files_scanned: files.len(),
        ..Default::default()
    };

    for path in &files {
        let res = if is_wav(path) { analyze_wav(path, thr_lin) } else { analyze_mp3(path, thr_lin) };
        match res {
            Ok(info) if info.sample_rate > 0 => {
                let leading_ms = info.first_loud_frame as f64 / info.sample_rate as f64 * 1000.0;
                let duration_ms = info.total_frames as f64 / info.sample_rate as f64 * 1000.0;
                if leading_ms >= MIN_LEADING_MS {
                    report.files_with_silence += 1;
                    report.total_leading_ms += leading_ms;
                    report.items.push(SilenceItem {
                        rel_path: path.strip_prefix(&root).unwrap_or(path).to_string_lossy().to_string(),
                        format: ext_lower(path),
                        channels: info.channels,
                        sample_rate: info.sample_rate,
                        duration_ms,
                        leading_ms,
                    });
                }
            }
            Ok(_) => {}
            Err(e) => report.errors.push(format!("{}: {}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), e)),
        }
    }

    // Largest offenders first.
    report.items.sort_by(|a, b| b.leading_ms.partial_cmp(&a.leading_ms).unwrap_or(std::cmp::Ordering::Equal));
    info!("Sampleset scan: {} files, {} with leading silence (>{} ms)", report.files_scanned, report.files_with_silence, MIN_LEADING_MS);
    report
}

fn pick_bitrate(file_bytes: u64, duration_s: f64) -> mp3lame_encoder::Bitrate {
    use mp3lame_encoder::Bitrate::*;
    if duration_s <= 0.0 { return Kbps192; }
    let kbps = (file_bytes as f64 * 8.0 / duration_s / 1000.0) as u32;
    match kbps {
        0..=128 => Kbps128,
        129..=160 => Kbps160,
        161..=192 => Kbps192,
        193..=224 => Kbps224,
        225..=256 => Kbps256,
        _ => Kbps320,
    }
}

fn encode_mp3(samples: &[f32], channels: u16, sample_rate: u32, bitrate: mp3lame_encoder::Bitrate) -> Result<Vec<u8>, String> {
    use mp3lame_encoder::{Builder, FlushNoGap, InterleavedPcm, MonoPcm, Quality};
    let mut builder = Builder::new().ok_or("LAME init failed")?;
    builder.set_num_channels(channels as u8).map_err(|e| format!("channels: {:?}", e))?;
    builder.set_sample_rate(sample_rate).map_err(|e| format!("rate: {:?}", e))?;
    builder.set_brate(bitrate).map_err(|e| format!("brate: {:?}", e))?;
    builder.set_quality(Quality::Best).map_err(|e| format!("quality: {:?}", e))?;
    let mut enc = builder.build().map_err(|e| format!("build: {:?}", e))?;

    let per_channel = samples.len() / channels.max(1) as usize;
    let mut out: Vec<u8> = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(per_channel) + 7200);
    let written = if channels == 1 {
        enc.encode(MonoPcm(samples), out.spare_capacity_mut())
    } else {
        enc.encode(InterleavedPcm(samples), out.spare_capacity_mut())
    }.map_err(|e| format!("encode: {:?}", e))?;
    unsafe { out.set_len(out.len() + written); }

    out.reserve(7200);
    let flushed = enc.flush::<FlushNoGap>(out.spare_capacity_mut()).map_err(|e| format!("flush: {:?}", e))?;
    unsafe { out.set_len(out.len() + flushed); }
    Ok(out)
}

/// Copy `src` to `<backup_root>/<rel>` (creating parent dirs).
fn backup_file(src: &Path, root: &Path, backup_root: &Path) -> Result<(), String> {
    let rel = src.strip_prefix(root).unwrap_or(src);
    let dest = backup_root.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("backup mkdir: {}", e))?;
    }
    // Don't clobber an existing backup of the same original.
    if !dest.exists() {
        fs::copy(src, &dest).map_err(|e| format!("backup copy: {}", e))?;
    }
    Ok(())
}

/// Schrijf een getrimde WAV terug. hound schrijft alleen fmt+data en gooit
/// overige chunks weg — dus een bestaande `smpl`-loop (van de looptool of uit
/// de sampleset zelf) zou verdwijnen en de looppunten zouden bovendien met
/// `keep_from` frames verschuiven. Daarom: looppunten vooraf lezen, na het
/// trimmen verschuiven en als nieuwe smpl-chunk terugschrijven.
fn write_trimmed_wav(
    path: &Path,
    spec: hound::WavSpec,
    trimmed_f32: &[f32],
    is_float: bool,
    keep_from: usize,
    orig_loop: Option<(u64, u64)>,
    orig_cue: Option<u64>,
) -> Result<(), String> {
    let wav = crate::loop_tool::encode_wav(spec, trimmed_f32, is_float)?;
    let frames = trimmed_f32.len() / spec.channels.max(1) as usize;
    let mut out = match orig_loop {
        // Loop alleen behouden als hij volledig ná het weggeknipte stuk ligt en
        // binnen het bestand blijft. `le` is hier EXCLUSIEF (read_wav_loop_points);
        // de smpl-chunk verwacht de laatste frame ínclusief, dus le-1.
        Some((ls, le)) if ls >= keep_from as u64 && le > ls + 1 && ((le - keep_from as u64) as usize) <= frames => {
            let new_ls = (ls - keep_from as u64) as u32;
            let new_le_incl = (le - 1 - keep_from as u64) as u32;
            let smpl = crate::loop_tool::build_smpl_chunk(
                spec.sample_rate, crate::loop_tool::midi_note_from_name(path), new_ls, new_le_incl,
            );
            crate::loop_tool::insert_smpl(&wav, &smpl)?
        }
        _ => wav,
    };
    // Cue-marker (release-segment in hetzelfde bestand) behouden en mee-
    // verschuiven: de loader weigert een same-file release zonder cue — de
    // trim gooide hem stilletjes weg (auditbevinding 2).
    if let Some(cf) = orig_cue {
        if cf >= keep_from as u64 && ((cf - keep_from as u64) as usize) < frames {
            let shifted = (cf - keep_from as u64) as u32;
            out = crate::loop_tool::insert_smpl(&out, &build_cue_chunk(shifted))?;
        }
    }
    // Veilig herschrijven: eerst temp, dan rename — een crash halverwege liet
    // anders een half/corrupt samplebestand achter (auditbevinding 24; de
    // .bak-kopie bestond al als vangnet).
    let tmp = path.with_extension("jm-tmp");
    fs::write(&tmp, &out).map_err(|e| format!("write temp: {}", e))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename: {}", e))
}

/// Bouw een minimale `cue `-chunk met één marker op `cue_frame`.
fn build_cue_chunk(cue_frame: u32) -> Vec<u8> {
    let mut c = Vec::with_capacity(8 + 4 + 24);
    c.extend_from_slice(b"cue ");
    c.extend_from_slice(&28u32.to_le_bytes());
    c.extend_from_slice(&1u32.to_le_bytes());      // aantal cues
    c.extend_from_slice(&1u32.to_le_bytes());      // dwName (id)
    c.extend_from_slice(&cue_frame.to_le_bytes()); // dwPosition
    c.extend_from_slice(b"data");                  // fccChunk
    c.extend_from_slice(&0u32.to_le_bytes());      // dwChunkStart
    c.extend_from_slice(&0u32.to_le_bytes());      // dwBlockStart
    c.extend_from_slice(&cue_frame.to_le_bytes()); // dwSampleOffset
    c
}

fn trim_wav_file(path: &Path, thr_lin: f32, preroll_ms: f32) -> Result<f64, String> {
    // Looppunten en cue-marker VOOR het herschrijven lezen (hound bewaart
    // geen smpl-/cue-chunks).
    let orig_loop = vpo_sampler::read_wav_loop_points(path);
    let orig_cue = vpo_sampler::read_wav_markers(path).and_then(|m| m.cue_frame);

    let reader = hound::WavReader::open(path).map_err(|e| format!("WAV open: {}", e))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let preroll_frames = (preroll_ms as f64 / 1000.0 * spec.sample_rate as f64) as usize;

    match spec.sample_format {
        hound::SampleFormat::Float => {
            let samples: Vec<f32> = reader.into_samples::<f32>()
                .collect::<Result<_, _>>().map_err(|e| format!("WAV read: {}", e))?;
            let first = first_loud_frame_f32(&samples, channels, thr_lin);
            if first >= samples.len() / channels { return Ok(0.0); } // entirely silent — leave file alone
            let keep_from = first.saturating_sub(preroll_frames);
            if keep_from == 0 { return Ok(0.0); }
            let trimmed = &samples[keep_from * channels..];
            write_trimmed_wav(path, spec, trimmed, true, keep_from, orig_loop, orig_cue)?;
            Ok(keep_from as f64 / spec.sample_rate as f64 * 1000.0)
        }
        hound::SampleFormat::Int => {
            let samples: Vec<i32> = reader.into_samples::<i32>()
                .collect::<Result<_, _>>().map_err(|e| format!("WAV read: {}", e))?;
            let maxv = (1i64 << (spec.bits_per_sample.saturating_sub(1))) as f32;
            let thr_int = (thr_lin * maxv) as i64;
            let first = first_loud_frame_i32(&samples, channels, thr_int);
            if first >= samples.len() / channels { return Ok(0.0); } // entirely silent — leave file alone
            let keep_from = first.saturating_sub(preroll_frames);
            if keep_from == 0 { return Ok(0.0); }
            // Widen i32 → f32 (native waarden); encode_wav rondt terug naar Int.
            let trimmed: Vec<f32> = samples[keep_from * channels..].iter().map(|&v| v as f32).collect();
            write_trimmed_wav(path, spec, &trimmed, false, keep_from, orig_loop, orig_cue)?;
            Ok(keep_from as f64 / spec.sample_rate as f64 * 1000.0)
        }
    }
}

fn trim_mp3_file(path: &Path, thr_lin: f32, preroll_ms: f32) -> Result<f64, String> {
    let (samples, channels, sample_rate) = decode_mp3_interleaved(path)?;
    if channels == 0 || sample_rate == 0 { return Err("empty/invalid mp3".into()); }
    if channels > 2 { return Err(format!("unsupported channel count {} for MP3", channels)); }
    let ch = channels as usize;
    let preroll_frames = (preroll_ms as f64 / 1000.0 * sample_rate as f64) as usize;
    let first = first_loud_frame_f32(&samples, ch, thr_lin);
    if first >= samples.len() / ch { return Ok(0.0); } // entirely silent — leave file alone
    let keep_from = first.saturating_sub(preroll_frames);
    if keep_from == 0 { return Ok(0.0); }

    let trimmed = &samples[keep_from * ch..];
    let total_frames = samples.len() / ch;
    let duration_s = total_frames as f64 / sample_rate as f64;
    let file_bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let bitrate = pick_bitrate(file_bytes, duration_s);
    let mp3 = encode_mp3(trimmed, channels, sample_rate, bitrate)?;
    fs::write(path, &mp3).map_err(|e| format!("mp3 write: {}", e))?;
    Ok(keep_from as f64 / sample_rate as f64 * 1000.0)
}

/// Trim leading silence from every audio file in `directory`, backing up the
/// originals to a sibling `<name>_backup` folder first.
pub fn trim(directory: &str, threshold_db: f32, preroll_ms: f32) -> TrimReport {
    let root = resolve_root(directory);
    let thr_lin = 10f32.powf(threshold_db / 20.0);

    // Sibling backup folder: "<name>_backup" (de-duplicated with a numeric suffix).
    let parent = root.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| root.clone());
    let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "sampleset".into());
    let mut backup_root = parent.join(format!("{}_backup", name));
    let mut n = 2;
    while backup_root.exists() && n < 100 {
        backup_root = parent.join(format!("{}_backup_{}", name, n));
        n += 1;
    }

    let mut report = TrimReport {
        root: root.to_string_lossy().to_string(),
        backup_dir: backup_root.to_string_lossy().to_string(),
        ..Default::default()
    };

    let mut files = Vec::new();
    collect_audio_files(&root, &mut files);

    for path in &files {
        // Backup BEFORE any modification.
        if let Err(e) = backup_file(path, &root, &backup_root) {
            report.errors.push(format!("{}: {}", path.display(), e));
            continue;
        }
        let res = if is_wav(path) {
            trim_wav_file(path, thr_lin, preroll_ms)
        } else {
            trim_mp3_file(path, thr_lin, preroll_ms)
        };
        match res {
            Ok(ms) if ms > 0.0 => { report.files_trimmed += 1; report.total_trimmed_ms += ms; }
            Ok(_) => {}
            Err(e) => report.errors.push(format!("{}: {}", path.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), e)),
        }
    }

    info!("Sampleset trim: {} files trimmed, backup at {}", report.files_trimmed, report.backup_dir);
    report
}
