//! Sample loading from various formats (WAV, MP3)

use hound::{WavReader, WavSpec};
use std::path::Path;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use thiserror::Error;
use tracing::{info, warn, debug, trace};
use vpo_core::SampleRate;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Error, Debug)]
pub enum SampleError {
    #[error("Failed to open file: {0}")]
    FileError(String),
    
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),
    
    #[error("Decode error: {0}")]
    DecodeError(String),
    
    #[error("Resample error: {0}")]
    ResampleError(String),
}

/// Loaded sample data
#[derive(Debug, Clone)]
pub struct SampleData {
    /// Sample data (mono, normalized to -1.0 to 1.0)
    pub data: Vec<f32>,
    /// Original sample rate
    pub sample_rate: SampleRate,
    /// Number of channels in original file
    pub channels: u16,
    /// Loop start (in samples)
    pub loop_start: Option<u64>,
    /// Loop end (in samples)
    pub loop_end: Option<u64>,
}

impl SampleData {
    /// Get the duration in seconds
    pub fn duration(&self) -> f32 {
        self.data.len() as f32 / self.sample_rate as f32
    }
    
    /// Get the number of samples
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Refine `loop_end` so the audio just after it best matches the audio at
/// `loop_start` — i.e. snap the loop to a phase-aligned point. WAV `smpl` loop
/// points are often not phase-accurate, which makes the loop wrap a click. By
/// searching a small window around the authored `loop_end` for the position whose
/// forward window best matches the loop start (lowest sum-of-squared-difference),
/// the seam becomes (near) continuous even before any playback crossfade.
///
/// `loop_start` is kept fixed; only `loop_end` moves, within a fraction of the
/// loop so the loop length (and pitch) barely changes.
pub fn optimize_loop_points(sample: &mut SampleData) {
    if let (Some(a), Some(b)) = (sample.loop_start, sample.loop_end) {
        if b > a {
            if let Some(e) = optimize_loop_end(&sample.data, a as usize, b as usize) {
                sample.loop_end = Some(e as u64);
            }
        }
    }
}

/// Kern van de looppunt-optimalisatie op een kale samplebuffer: zoek rond het
/// aangeleverde exclusieve loop-einde `le` de positie waarvan het vervolg het
/// best fase-matcht met `ls`, en geef die terug (of None wanneer er niets te
/// verbeteren valt). Gedeeld door het full-sample-pad (optimize_loop_points)
/// en het preload-pad — de eerste ~2 seconden van elke noot loopen op de
/// preload-buffer, dus ook dáár moet de naad fase-uitgelijnd zijn.
pub fn optimize_loop_end(data: &[f32], ls: usize, le: usize) -> Option<usize> {
    let n = data.len();
    let win = 128usize; // comparison window (~3 ms @ 48k)
    if le <= ls || ls + win >= n || le + win >= n {
        return None;
    }
    let loop_len = le - ls;
    // Search ±(quarter loop) around the authored loop end, clamped to a few periods
    // and to the valid index range.
    let search = (loop_len / 4).clamp(64, 2400);
    let lo = le.saturating_sub(search).max(ls + 1);
    let hi = (le + search).min(n - win - 1);
    if hi <= lo {
        return None;
    }
    let mut best_e = le;
    let mut best_cost = f32::INFINITY;
    for e in lo..=hi {
        let mut ssd = 0.0f32;
        for k in 0..win {
            let d = data[ls + k] - data[e + k];
            ssd += d * d;
        }
        // Tiny bias toward the original loop end to avoid needless drift between
        // near-equal matches.
        let cost = ssd + (e as f32 - le as f32).abs() * 1e-6;
        if cost < best_cost {
            best_cost = cost;
            best_e = e;
        }
    }
    if best_e != le { Some(best_e) } else { None }
}

/// Alle sample-markers uit een WAV: loops (smpl), release-marker (cue) en het
/// aantal frames. Loop-eindes worden hier al naar EXCLUSIEF omgerekend
/// (GrandOrgue-semantiek: smpl dwEnd is de laatste frame, ínclusief).
#[derive(Debug, Clone, Default)]
pub struct WavMarkers {
    /// Gevalideerde loops als (start, einde-exclusief), in frame-offsets.
    pub loops: Vec<(u64, u64)>,
    /// Release-marker: hoogste cue-punt uit de `cue `-chunk (frame-offset).
    pub cue_frame: Option<u64>,
    /// Totaal aantal frames in de data-chunk (0 als fmt/data ontbreekt).
    pub frames: u64,
}

/// Lees smpl-loops, cue-marker en framecount uit een WAV (chunk-walk zonder de
/// audiodata te decoderen). Loops worden per GrandOrgue-regels gevalideerd:
/// start<einde, binnen het bestand, einde>0 — ongeldige records vervallen stil.
pub fn read_wav_markers(path: &Path) -> Option<WavMarkers> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;

    // Check RIFF/WAVE header
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return None;
    }

    let mut markers = WavMarkers::default();
    let mut raw_loops: Vec<(u64, u64)> = Vec::new(); // (start, end_inclusief)
    let mut bytes_per_frame = 0u64;
    let mut data_bytes = 0u64;

    loop {
        let mut chunk_header = [0u8; 8];
        if file.read_exact(&mut chunk_header).is_err() {
            break;
        }

        let chunk_id = &chunk_header[0..4];
        let chunk_size = u32::from_le_bytes([
            chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7]
        ]) as u64;
        let skip = if chunk_size % 2 == 1 { chunk_size + 1 } else { chunk_size };

        match chunk_id {
            b"fmt " if chunk_size >= 16 => {
                let mut fmt = [0u8; 16];
                if file.read_exact(&mut fmt).is_err() { break; }
                let channels = u16::from_le_bytes([fmt[2], fmt[3]]) as u64;
                let bits = u16::from_le_bytes([fmt[14], fmt[15]]) as u64;
                bytes_per_frame = channels * (bits / 8).max(1);
                let rest = skip - 16;
                if rest > 0 && file.seek(SeekFrom::Current(rest as i64)).is_err() { break; }
            }
            b"data" => {
                data_bytes = chunk_size;
                if file.seek(SeekFrom::Current(skip as i64)).is_err() { break; }
            }
            b"smpl" if chunk_size >= 36 => {
                let mut smpl_data = vec![0u8; chunk_size as usize];
                if file.read_exact(&mut smpl_data).is_err() { break; }
                let num_loops = u32::from_le_bytes([
                    smpl_data[28], smpl_data[29], smpl_data[30], smpl_data[31]
                ]) as usize;
                for li in 0..num_loops {
                    // Loop-record: 24 bytes vanaf offset 36; start op +8, end op +12.
                    let base = 36 + li * 24;
                    if base + 24 > smpl_data.len() { break; }
                    let s = u32::from_le_bytes([
                        smpl_data[base + 8], smpl_data[base + 9], smpl_data[base + 10], smpl_data[base + 11]
                    ]) as u64;
                    let e = u32::from_le_bytes([
                        smpl_data[base + 12], smpl_data[base + 13], smpl_data[base + 14], smpl_data[base + 15]
                    ]) as u64;
                    raw_loops.push((s, e));
                }
            }
            b"cue " if chunk_size >= 4 => {
                let mut cue_data = vec![0u8; chunk_size as usize];
                if file.read_exact(&mut cue_data).is_err() { break; }
                let n = u32::from_le_bytes([cue_data[0], cue_data[1], cue_data[2], cue_data[3]]) as usize;
                // Cue-record: 24 bytes vanaf offset 4; dwSampleOffset op +20.
                // GrandOrgue neemt het MAXIMUM over alle cue-punten.
                let mut max_off: Option<u64> = None;
                for ci in 0..n {
                    let base = 4 + ci * 24;
                    if base + 24 > cue_data.len() { break; }
                    let off = u32::from_le_bytes([
                        cue_data[base + 20], cue_data[base + 21], cue_data[base + 22], cue_data[base + 23]
                    ]) as u64;
                    max_off = Some(max_off.map_or(off, |m: u64| m.max(off)));
                }
                markers.cue_frame = max_off;
            }
            _ => {
                if file.seek(SeekFrom::Current(skip as i64)).is_err() { break; }
            }
        }
    }

    if bytes_per_frame > 0 {
        markers.frames = data_bytes / bytes_per_frame;
    }

    // Valideer loops (GrandOrgue-regels) en reken einde om naar exclusief.
    let frames = markers.frames;
    for (s, e) in raw_loops {
        let valid = s < e && e > 0 && (frames == 0 || (s < frames && e < frames));
        if valid {
            markers.loops.push((s, e + 1)); // inclusief → exclusief
        }
    }

    Some(markers)
}

/// Read loop points from WAV smpl chunk.
/// Geeft de LANGSTE geldige loop terug als (start, einde-exclusief) — GrandOrgue
/// laadt standaard alle loops; onze engine speelt er één, dus de langste geeft
/// de meeste variatie en de minste herhaling.
pub fn read_wav_loop_points(path: &Path) -> Option<(u64, u64)> {
    let markers = read_wav_markers(path)?;
    let best = markers.loops.iter()
        .max_by_key(|(s, e)| e - s)
        .copied();
    if let Some((s, e)) = best {
        // debug!: dit draait per bestand (duizenden keren per orgel-load en
        // opnieuw per achtergrond-load) — op info-niveau verstopte het de log
        // en vulde het de stderr-pipe van beheerd gestarte processen.
        debug!("Found WAV loop points: start={}, end={} (excl., {} loops)", s, e, markers.loops.len());
    }
    best
}

/// Lees alleen de fmt-chunk-grootte van een WAV (chunk-walk over headers, geen
/// audiodata). Hound accepteert uitsluitend fmt-groottes 16/18/40; veel
/// sampleset-encoders (o.a. Piotr Grabowski-sets) schrijven afwijkende
/// groottes waardoor élk bestand eerst door hound én symphonia faalde voordat
/// de manual parser het las — twee mislukte parse-pogingen plus twee
/// warn-logregels per bestand, duizenden keren per orgel. Met deze probe
/// kiezen we direct de juiste parser.
fn wav_fmt_chunk_size(path: &Path) -> Option<u32> {
    use std::io::BufReader;
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut header = [0u8; 12];
    reader.read_exact(&mut header).ok()?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return None;
    }
    loop {
        let mut chunk_header = [0u8; 8];
        reader.read_exact(&mut chunk_header).ok()?;
        let chunk_size = u32::from_le_bytes([
            chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7],
        ]);
        if &chunk_header[0..4] == b"fmt " {
            return Some(chunk_size);
        }
        let skip = if chunk_size % 2 == 1 { chunk_size + 1 } else { chunk_size };
        reader.seek(SeekFrom::Current(skip as i64)).ok()?;
    }
}

/// Load a WAV file
pub fn load_wav(path: &Path) -> Result<SampleData, SampleError> {
    // First, try to read loop points from smpl chunk
    let loop_points = read_wav_loop_points(path);

    // fmt-grootte die hound niet accepteert → direct de manual parser (die is
    // compleet voor 8/16/24/32-bit int + 32-bit float + extensible). Zo vervalt
    // de dubbele fail-cascade per bestand. Belangrijk: óók hier de looppunt-
    // optimalisatie toepassen — die draaide eerst alleen in het hound-pad,
    // waardoor juist deze samplesets loop-naad-tikken hielden.
    let fmt_size = wav_fmt_chunk_size(path);
    if !matches!(fmt_size, None | Some(16) | Some(18) | Some(40)) {
        if let Ok(mut sd) = load_wav_manual(path) {
            if let Some((s, e)) = loop_points {
                sd.loop_start = Some(s);
                sd.loop_end = Some(e);
            }
            optimize_loop_points(&mut sd);
            return Ok(sd);
        }
        // Manual faalde onverwacht → alsnog de normale cascade proberen.
    }

    let reader = match WavReader::open(path) {
        Ok(r) => r,
        Err(hound_err) => {
            // Hound is strict over fmt chunk size — sommige WAV encoders schrijven extra
            // bytes of niet-standaard chunks. Probeer eerst symphonia (ondersteunt diverse
            // WAV-varianten), bij verdere failure val terug op onze eigen manual parser
            // die simpelweg de fmt chunk leest zonder strict-checks.
            // debug! i.p.v. warn!: dit pad is verwacht gedrag voor hele samplesets
            // en spamde het log met twee regels per bestand (duizenden per orgel).
            debug!("Hound kon {} niet lezen ({}), probeer symphonia", path.display(), hound_err);
            match load_via_symphonia(path, "wav") {
                Ok(mut sd) => {
                    if let Some((s, e)) = loop_points {
                        sd.loop_start = Some(s);
                        sd.loop_end = Some(e);
                    }
                    optimize_loop_points(&mut sd);
                    return Ok(sd);
                }
                Err(symph_err) => {
                    debug!("Symphonia kon {} ook niet lezen ({}), probeer manual WAV parser", path.display(), symph_err);
                    return load_wav_manual(path).map(|mut sd| {
                        if let Some((s, e)) = loop_points {
                            sd.loop_start = Some(s);
                            sd.loop_end = Some(e);
                        }
                        optimize_loop_points(&mut sd);
                        sd
                    });
                }
            }
        }
    };

    let spec = reader.spec();
    debug!("Loading WAV: {:?}, {} Hz, {} ch, {} bits",
           path.file_name(), spec.sample_rate, spec.channels, spec.bits_per_sample);

    // Leesfouten (afgeknot bestand) niet meer stil naar 0.0 mappen zonder spoor:
    // tellen en één warn per bestand — anders leek een load "geslaagd" terwijl
    // de staart stilte was (auditbevinding 36).
    let mut read_errors: usize = 0;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.into_samples::<f32>()
                .map(|s| s.unwrap_or_else(|_| { read_errors += 1; 0.0 }))
                .collect()
        }
        hound::SampleFormat::Int => {
            let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
            match spec.bits_per_sample {
                8 => {
                    reader.into_samples::<i8>()
                        .map(|s| s.unwrap_or_else(|_| { read_errors += 1; 0 }) as f32 / max_val)
                        .collect()
                }
                16 => {
                    reader.into_samples::<i16>()
                        .map(|s| s.unwrap_or_else(|_| { read_errors += 1; 0 }) as f32 / max_val)
                        .collect()
                }
                24 | 32 => {
                    reader.into_samples::<i32>()
                        .map(|s| s.unwrap_or_else(|_| { read_errors += 1; 0 }) as f32 / max_val)
                        .collect()
                }
                _ => return Err(SampleError::UnsupportedFormat(
                    format!("{} bits per sample", spec.bits_per_sample)
                )),
            }
        }
    };
    if read_errors > 0 {
        warn!("{}: {} sample-leesfouten (afgeknot/corrupt bestand?) — ontbrekende data als stilte ingevuld",
            path.display(), read_errors);
    }

    // Convert to mono if stereo
    let mono = if spec.channels == 2 {
        samples.chunks(2)
            .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5)
            .collect()
    } else if spec.channels == 1 {
        samples
    } else {
        // Take first channel only for multi-channel
        samples.into_iter().step_by(spec.channels as usize).collect()
    };

    // Adjust loop points for mono conversion (divide by channels if stereo)
    let (loop_start, loop_end) = if let Some((start, end)) = loop_points {
        // Loop points in smpl chunk are per-frame, not per-sample
        // So they should work directly for mono
        (Some(start), Some(end))
    } else {
        (None, None)
    };

    let mut sd = SampleData {
        data: mono,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        loop_start,
        loop_end,
    };
    optimize_loop_points(&mut sd);
    Ok(sd)
}

/// Resample to target sample rate
pub fn resample(sample: &SampleData, target_rate: SampleRate) -> Result<SampleData, SampleError> {
    if sample.sample_rate == target_rate {
        return Ok(sample.clone());
    }
    
    use rubato::{FftFixedIn, Resampler};

    let ratio = target_rate as f64 / sample.sample_rate as f64;
    let chunk_size = 1024;

    let mut resampler = FftFixedIn::<f32>::new(
        sample.sample_rate as usize,
        target_rate as usize,
        chunk_size,
        2,
        1
    ).map_err(|e| SampleError::ResampleError(e.to_string()))?;

    // De FFT-resampler heeft een vaste in-/uitloopvertraging: de output begint
    // met `delay` samples "aanloop" en loopt evenzoveel achter op de input. Die
    // vertraging moet worden weggeknipt, anders verschuift ALLE audio in de tijd
    // t.o.v. de (geschaalde) looppunten én t.o.v. de preload-buffer — dat gaf
    // een hoorbare tik op het moment dat een spelende voice naar de volledige
    // sample upgradet, en een extra aanslag-vertraging (latency) bij 44.1k-sets.
    let delay = resampler.output_delay();
    let expected_len = (sample.data.len() as f64 * ratio).round() as usize;

    let mut output = Vec::with_capacity(expected_len + delay + chunk_size);
    let mut input_pos = 0usize;
    // Voer ook na het einde van de input nul-chunks aan totdat de delay-staart
    // eruit gespoeld is en we de verwachte lengte hebben.
    while output.len() < expected_len + delay {
        let mut chunk: Vec<f32> = if input_pos < sample.data.len() {
            let end = (input_pos + chunk_size).min(sample.data.len());
            sample.data[input_pos..end].to_vec()
        } else {
            Vec::new()
        };
        chunk.resize(chunk_size, 0.0);
        input_pos += chunk_size;

        let input = vec![chunk];
        let result = resampler.process(&input, None)
            .map_err(|e| SampleError::ResampleError(e.to_string()))?;
        output.extend(&result[0]);

        // Veiligheidsklep tegen een oneindige lus bij onverwacht resampler-gedrag.
        if input_pos > sample.data.len() + 8 * chunk_size {
            break;
        }
    }

    // Delay-aanloop wegknippen en op de verwachte lengte trimmen → output is
    // tijd-uitgelijnd met de input (frame i ↔ input-frame i/ratio).
    let start = delay.min(output.len());
    let end = (start + expected_len).min(output.len());
    let aligned: Vec<f32> = output[start..end].to_vec();

    // Adjust loop points
    let loop_start = sample.loop_start.map(|l| (l as f64 * ratio) as u64);
    let loop_end = sample.loop_end.map(|l| (l as f64 * ratio) as u64);

    let mut sd = SampleData {
        data: aligned,
        sample_rate: target_rate,
        channels: 1,
        loop_start,
        loop_end,
    };
    optimize_loop_points(&mut sd);
    Ok(sd)
}

/// Normalize sample to peak level
pub fn normalize(sample: &mut SampleData, target_db: f32) {
    let peak = sample.data.iter()
        .map(|s| s.abs())
        .fold(0.0_f32, |a, b| a.max(b));
    
    if peak > 0.0 {
        let target_linear = 10.0_f32.powf(target_db / 20.0);
        let gain = target_linear / peak;
        
        for s in &mut sample.data {
            *s *= gain;
        }
    }
}

/// Apply fade in/out to sample
pub fn apply_fades(sample: &mut SampleData, fade_in_ms: f32, fade_out_ms: f32) {
    let sr = sample.sample_rate as f32;
    let fade_in_samples = (fade_in_ms / 1000.0 * sr) as usize;
    let fade_out_samples = (fade_out_ms / 1000.0 * sr) as usize;

    // Fade in
    for (i, s) in sample.data.iter_mut().take(fade_in_samples).enumerate() {
        let t = i as f32 / fade_in_samples as f32;
        *s *= t * t; // Quadratic fade
    }

    // Fade out
    let len = sample.data.len();
    if len > fade_out_samples {
        for (i, s) in sample.data.iter_mut().skip(len - fade_out_samples).enumerate() {
            let t = 1.0 - (i as f32 / fade_out_samples as f32);
            *s *= t * t;
        }
    }
}

/// Release-fase-uitlijningstabel (naar het GrandOrgue-algoritme,
/// GOSoundReleaseAlignTable): per (helling-bucket x amplitude-bucket) de eerste
/// samplepositie in de release met die golfvorm-toestand. Bij het loslaten van
/// een noot start de release-sample dan op de positie die qua fase aansluit op
/// het klinkende signaal, in plaats van op frame 0 — dat scheelt de hoorbare
/// fase-discontinuiteit ("tik") bij elke NoteOff op natte samplesets.
pub const ALIGN_AMPS: usize = 16;
pub const ALIGN_DERIVS: usize = 16;
/// Analysevenster: de periode van de laagst verwachte pijp (20 Hz).
const ALIGN_MIN_FREQUENCY: u32 = 20;

#[derive(Debug, Clone)]
pub struct ReleaseAlignTable {
    max_amp: f32,
    max_deriv: f32,
    positions: [[u32; ALIGN_AMPS]; ALIGN_DERIVS],
}

impl ReleaseAlignTable {
    fn bucket(x: f32, max: f32, n: usize) -> usize {
        if max <= 0.0 {
            return n / 2;
        }
        let t = ((x + max) / (2.0 * max)).clamp(0.0, 1.0);
        ((t * n as f32) as usize).min(n - 1)
    }

    /// Bouw de tabel over het begin van een release-segment (mono data op
    /// bestands-samplerate). Geeft None wanneer het segment te kort of te stil
    /// is — uitlijning helpt dan toch niet (GrandOrgue doet hetzelfde).
    pub fn compute(data: &[f32], sample_rate: SampleRate) -> Option<Self> {
        let search = (sample_rate / ALIGN_MIN_FREQUENCY) as usize;
        if data.len() < search + 2 || search < ALIGN_AMPS * ALIGN_DERIVS * 2 {
            return None;
        }
        let mut max_amp = 0.0f32;
        let mut max_deriv = 0.0f32;
        for i in 1..search {
            max_amp = max_amp.max(data[i].abs());
            max_deriv = max_deriv.max((data[i] - data[i - 1]).abs());
        }
        if max_amp <= 1e-5 || max_deriv <= 1e-7 {
            return None;
        }
        let mut positions = [[0u32; ALIGN_AMPS]; ALIGN_DERIVS];
        let mut filled = [[false; ALIGN_AMPS]; ALIGN_DERIVS];
        for i in 1..search {
            let f = data[i];
            let v = f - data[i - 1];
            let ai = Self::bucket(f, max_amp, ALIGN_AMPS);
            let di = Self::bucket(v, max_deriv, ALIGN_DERIVS);
            if !filled[di][ai] {
                positions[di][ai] = (i + 1) as u32;
                filled[di][ai] = true;
            }
        }
        // Lege cellen vullen met de dichtstbijzijnde gevulde (Manhattan-afstand).
        let any = filled.iter().flatten().any(|&b| b);
        if !any {
            return None;
        }
        for di in 0..ALIGN_DERIVS {
            for ai in 0..ALIGN_AMPS {
                if !filled[di][ai] {
                    let mut best = u32::MAX;
                    let mut best_pos = 0u32;
                    for dj in 0..ALIGN_DERIVS {
                        for aj in 0..ALIGN_AMPS {
                            if filled[dj][aj] {
                                let dist = (di as i32 - dj as i32).unsigned_abs()
                                    + (ai as i32 - aj as i32).unsigned_abs();
                                if dist < best {
                                    best = dist;
                                    best_pos = positions[dj][aj];
                                }
                            }
                        }
                    }
                    positions[di][ai] = best_pos;
                }
            }
        }
        Some(Self { max_amp, max_deriv, positions })
    }

    /// Startpositie (frames binnen het release-segment) die past bij de laatste
    /// twee ruwe samplewaarden van de stervende stem.
    pub fn position_for(&self, last: f32, prev: f32) -> u32 {
        let ai = Self::bucket(last, self.max_amp, ALIGN_AMPS);
        let di = Self::bucket(last - prev, self.max_deriv, ALIGN_DERIVS);
        self.positions[di][ai]
    }
}

/// Preload buffer info - contains attack portion for instant playback
#[derive(Debug, Clone)]
pub struct PreloadBuffer {
    /// Attack portion of the sample (first ~100-200ms)
    pub attack_data: Vec<f32>,
    /// Original sample rate
    pub sample_rate: SampleRate,
    /// Total sample length (for reference)
    pub total_samples: usize,
    /// Loop start point (if any)
    pub loop_start: Option<u64>,
    /// Loop end point (if any)
    pub loop_end: Option<u64>,
    /// Path to full sample file for streaming
    pub source_path: std::path::PathBuf,
    /// Number of channels in original file
    pub channels: u16,
    /// Bits per sample
    pub bits_per_sample: u16,
    /// Byte offset where audio data starts in the WAV file
    pub data_offset: u64,
    /// Fase-uitlijningstabel (alleen voor release-segmenten): startpositie in
    /// dit segment die aansluit op de golfvorm van de stervende stem.
    pub align_table: Option<ReleaseAlignTable>,
    /// Aantal frames aanloopstilte dat bij het preloaden is weggeknipt (onset-
    /// alignment). De volledige sample die op de achtergrond laadt is NIET
    /// geknipt; de afspeelpositie moet bij de upgrade met dit aantal (in
    /// bron-samplerate-frames) worden verschoven, anders verspringt de audio
    /// hoorbaar (tik) op het upgrademoment.
    pub trim_start: usize,
}

/// Welke uitsnede van het bestand een preload-buffer moet bevatten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreloadSegment {
    /// De attack vanaf frame 0 (normale pijp-sample).
    Full,
    /// Het release-segment: vanaf de cue-marker (of de override uit de ODF)
    /// tot het einde van het bestand — GrandOrgue-semantiek voor releases.
    /// `cue_override` = ODF CuePoint; None = cue-chunk van het bestand, anders
    /// frame 0. `require_cue` = alleen laden als het bestand écht een
    /// release-marker + loop heeft (voor same-file releases uit de attack).
    Release { cue_override: Option<u32>, require_cue: bool },
}

/// Load only the attack portion of a WAV file (first N samples)
/// This enables instant playback while the rest streams from disk
pub fn load_wav_preload(path: &Path, preload_samples: usize) -> Result<PreloadBuffer, SampleError> {
    load_wav_preload_segment(path, preload_samples, PreloadSegment::Full)
}

/// Als `load_wav_preload`, maar met keuze van de uitsnede (attack of release-
/// segment). Zie `PreloadSegment`.
pub fn load_wav_preload_segment(path: &Path, preload_samples: usize, segment: PreloadSegment) -> Result<PreloadBuffer, SampleError> {
    use std::io::BufReader;

    // Markers (loops + cue) uit de chunk-walk; bepaalt looppunten én — voor
    // release-segmenten — het startframe.
    let markers = read_wav_markers(path).unwrap_or_default();
    let loop_points = markers.loops.iter().max_by_key(|(s, e)| e - s).copied();

    // Release-segment: startframe bepalen (ODF-override > cue-chunk > 0).
    let segment_start: u64 = match segment {
        PreloadSegment::Full => 0,
        PreloadSegment::Release { cue_override, require_cue } => {
            if require_cue && (markers.cue_frame.is_none() || markers.loops.is_empty()) {
                // Same-file release vereist loop + cue in het bestand (GO-regel);
                // ontbreekt die, dan is er simpelweg geen release-segment.
                return Err(SampleError::UnsupportedFormat("geen cue-marker/loop voor release-segment".to_string()));
            }
            cue_override.map(|c| c as u64).or(markers.cue_frame).unwrap_or(0)
        }
    };

    let file = File::open(path)
        .map_err(|e| SampleError::FileError(format!("{}: {}", path.display(), e)))?;
    let mut reader = BufReader::new(file);

    // Read RIFF header
    let mut header = [0u8; 12];
    reader.read_exact(&mut header)
        .map_err(|e| SampleError::FileError(format!("Failed to read header: {}", e)))?;

    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(SampleError::UnsupportedFormat("Not a valid WAV file".to_string()));
    }

    // Find fmt and data chunks
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits_per_sample = 0u16;
    let mut sample_format = hound::SampleFormat::Int;
    let mut data_offset = 0u64;
    let mut data_size = 0u32;
    let mut current_pos = 12u64;

    loop {
        let mut chunk_header = [0u8; 8];
        if reader.read_exact(&mut chunk_header).is_err() {
            break;
        }
        current_pos += 8;

        let chunk_id = &chunk_header[0..4];
        let chunk_size = u32::from_le_bytes([
            chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7]
        ]);

        if chunk_id == b"fmt " {
            // Guard tegen misvormde fmt-chunks: minimaal 16 bytes nodig voor de
            // velden hieronder; een gigantische chunk_size mag geen giga-alloc
            // worden (auditbevinding 15).
            if chunk_size < 16 || chunk_size > 65536 {
                return Err(SampleError::UnsupportedFormat(
                    format!("fmt-chunk met onbruikbare grootte {}", chunk_size)
                ));
            }
            let mut fmt_data = vec![0u8; chunk_size as usize];
            reader.read_exact(&mut fmt_data)
                .map_err(|e| SampleError::FileError(format!("Failed to read fmt chunk: {}", e)))?;

            let audio_format = u16::from_le_bytes([fmt_data[0], fmt_data[1]]);
            channels = u16::from_le_bytes([fmt_data[2], fmt_data[3]]);
            sample_rate = u32::from_le_bytes([fmt_data[4], fmt_data[5], fmt_data[6], fmt_data[7]]);
            bits_per_sample = u16::from_le_bytes([fmt_data[14], fmt_data[15]]);

            // Alleen PCM (1), float (3) en extensible (0xFFFE) zijn leesbaar met
            // de vaste bytes-per-sample-aanname hieronder; ADPCM e.d. (formaat 2,
            // 17, ...) gaf een deling-door-nul-panic.
            if !matches!(audio_format, 1 | 3 | 0xFFFE) {
                return Err(SampleError::UnsupportedFormat(
                    format!("WAV audio-formaat {} (gecomprimeerd?) wordt niet ondersteund", audio_format)
                ));
            }

            sample_format = match audio_format {
                3 => hound::SampleFormat::Float,
                // WAVE_FORMAT_EXTENSIBLE: het echte formaat staat in het
                // subformaat (bytes 24-25). Vóór deze check werden extensible
                // FLOAT-bestanden als int gedecodeerd → luide ruis.
                0xFFFE if fmt_data.len() >= 26 => {
                    let subformat = u16::from_le_bytes([fmt_data[24], fmt_data[25]]);
                    if subformat == 3 { hound::SampleFormat::Float } else { hound::SampleFormat::Int }
                }
                _ => hound::SampleFormat::Int,
            };

            current_pos += chunk_size as u64;
        } else if chunk_id == b"data" {
            data_offset = current_pos;
            data_size = chunk_size;
            break;
        } else {
            // Skip chunk
            let skip = if chunk_size % 2 == 1 { chunk_size + 1 } else { chunk_size };
            reader.seek(SeekFrom::Current(skip as i64))
                .map_err(|e| SampleError::FileError(format!("Failed to seek: {}", e)))?;
            current_pos += skip as u64;
        }
    }

    if data_offset == 0 {
        return Err(SampleError::UnsupportedFormat("No data chunk found".to_string()));
    }

    // Calculate total samples. Guard: bits<8 (ADPCM die door de check glipt) of
    // channels==0 (data-chunk vóór fmt-chunk) gaf hier een deling door nul.
    let bytes_per_sample = bits_per_sample as usize / 8;
    if bytes_per_sample == 0 || channels == 0 {
        return Err(SampleError::UnsupportedFormat(
            format!("onbruikbare WAV-parameters: {} bits, {} kanalen", bits_per_sample, channels)
        ));
    }
    let total_frames = data_size as usize / (bytes_per_sample * channels as usize);

    // Release-segment: start bij de cue-marker in plaats van frame 0.
    let seg_start = (segment_start as usize).min(total_frames);
    if seg_start > 0 {
        reader.seek(SeekFrom::Current((seg_start * bytes_per_sample * channels as usize) as i64))
            .map_err(|e| SampleError::FileError(format!("Failed to seek to segment: {}", e)))?;
    }

    // Read preload portion
    let frames_to_read = preload_samples.min(total_frames - seg_start);
    let bytes_to_read = frames_to_read * bytes_per_sample * channels as usize;

    let mut raw_data = vec![0u8; bytes_to_read];
    reader.read_exact(&mut raw_data)
        .map_err(|e| SampleError::FileError(format!("Failed to read audio data: {}", e)))?;

    // Convert to f32
    let samples: Vec<f32> = match (sample_format, bits_per_sample) {
        (hound::SampleFormat::Float, 32) => {
            raw_data.chunks(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        }
        (hound::SampleFormat::Int, 16) => {
            let max_val = i16::MAX as f32;
            raw_data.chunks(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / max_val)
                .collect()
        }
        (hound::SampleFormat::Int, 24) => {
            let max_val = (1i32 << 23) as f32;
            raw_data.chunks(3)
                .map(|b| {
                    let val = ((b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i32) << 16))
                        << 8 >> 8; // Sign extend
                    val as f32 / max_val
                })
                .collect()
        }
        (hound::SampleFormat::Int, 32) => {
            let max_val = i32::MAX as f32;
            raw_data.chunks(4)
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / max_val)
                .collect()
        }
        (hound::SampleFormat::Int, 8) => {
            // WAV 8-bit is UNSIGNED met 0x80 als nulpunt (GrandOrgue-conform).
            raw_data.iter()
                .map(|&b| (b as f32 - 128.0) / 128.0)
                .collect()
        }
        _ => {
            return Err(SampleError::UnsupportedFormat(
                format!("{} bits {:?} not supported for preload", bits_per_sample, sample_format)
            ));
        }
    };

    // Convert to mono if stereo
    let mono = if channels == 2 {
        samples.chunks(2)
            .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5)
            .collect()
    } else if channels == 1 {
        samples
    } else {
        // Take first channel for multi-channel
        samples.into_iter().step_by(channels as usize).collect()
    };

    // Release-segmenten loopen niet (one-shot uitklinken); alleen de attack
    // krijgt looppunten mee.
    let (loop_start, loop_end) = match (segment, loop_points) {
        (PreloadSegment::Full, Some((start, end))) => (Some(start), Some(end)),
        _ => (None, None),
    };

    // Attack alignment: detect onset and trim leading silence.
    // Alleen voor de attack (Full) — bij een release-segment zou de onset-trim
    // juist het begin van de kerkakoestiek weghappen.
    let onset_trim = if segment == PreloadSegment::Full {
        let onset_threshold = 0.002; // ~-54dB
        let pre_onset_keep = 32;     // samples to keep before detected onset
        let onset_pos = mono.iter().position(|&s| s.abs() > onset_threshold).unwrap_or(0);
        onset_pos.saturating_sub(pre_onset_keep)
    } else {
        0
    };
    let aligned_data = if onset_trim > 0 {
        debug!("Attack alignment: trimmed {} silent samples from {:?}", onset_trim, path.file_name());
        mono[onset_trim..].to_vec()
    } else {
        mono
    };

    // Totale verschuiving t.o.v. het originele bestand: segment-start (release
    // vanaf cue) + onset-trim. De upgrade naar de volledige sample corrigeert
    // de afspeelpositie met precies dit aantal bron-frames.
    let trim_start = seg_start + onset_trim;

    debug!("Preloaded {} samples from {:?} (total: {}, loop: {:?}-{:?}, trim: {})",
           aligned_data.len(), path.file_name(), total_frames, loop_start, loop_end, trim_start);

    // Looppunten naar buffer-coördinaten verschuiven, mét twee correcties die
    // het full-sample-pad al had maar de preload miste:
    // 1. Ligt het loop-begin vóór de trim (onset-trim at het loopgebied aan),
    //    dan is de loop in dit segment onbetrouwbaar → geen looppunten (de
    //    engine valt terug op zijn ¾-buffer-fallback) i.p.v. een stilletjes
    //    naar 0 geklemde, verschoven loop.
    // 2. Past de loop volledig in de buffer, lijn dan het loop-einde fase-uit
    //    (optimize_loop_end) — de eerste ~2 s van elke noot loopt op déze
    //    buffer en tikte anders op de naad ("geen mooie loops").
    let (adj_loop_start, adj_loop_end) = match (loop_start, loop_end) {
        (Some(ls), Some(le)) if ls >= trim_start as u64 && le > ls => {
            let a = ls - trim_start as u64;
            let mut e = le - trim_start as u64;
            if (e as usize) <= aligned_data.len() {
                if let Some(better) = optimize_loop_end(&aligned_data, a as usize, e as usize) {
                    e = better as u64;
                }
            }
            (Some(a), Some(e))
        }
        _ => (None, None),
    };

    // Fase-uitlijningstabel voor release-segmenten (GrandOrgue-stijl): het
    // scanvenster (periode van 20 Hz) valt ruim binnen de preload.
    let align_table = if matches!(segment, PreloadSegment::Release { .. }) {
        ReleaseAlignTable::compute(&aligned_data, sample_rate)
    } else {
        None
    };

    Ok(PreloadBuffer {
        attack_data: aligned_data,
        sample_rate,
        total_samples: total_frames.saturating_sub(trim_start),
        loop_start: adj_loop_start,
        loop_end: adj_loop_end,
        source_path: path.to_path_buf(),
        channels,
        bits_per_sample,
        data_offset,
        trim_start,
        align_table,
    })
}

/// Load remaining samples after the preload buffer (for background loading)
pub fn load_wav_remainder(preload: &PreloadBuffer, target_rate: SampleRate) -> Result<SampleData, SampleError> {
    // Load the full sample
    let mut full_sample = load_wav(&preload.source_path)?;

    // Resample if needed
    if full_sample.sample_rate != target_rate {
        full_sample = resample(&full_sample, target_rate)?;
    }

    Ok(full_sample)
}

/// Load an MP3 file using symphonia
pub fn load_mp3(path: &Path) -> Result<SampleData, SampleError> {
    load_via_symphonia(path, "mp3")
}

/// Handmatige WAV-loader — tolerante fallback voor bestanden waar hound én symphonia op klagen.
///
/// Negeert strict-checks op fmt chunk size en leest direct de essentiële velden.
/// Werkt voor PCM 8/16/24/32-bit Int en 32-bit Float WAV-bestanden.
pub fn load_wav_manual(path: &Path) -> Result<SampleData, SampleError> {
    use std::io::BufReader;

    let file = File::open(path)
        .map_err(|e| SampleError::FileError(format!("{}: {}", path.display(), e)))?;
    let mut reader = BufReader::new(file);

    // RIFF header
    let mut header = [0u8; 12];
    reader.read_exact(&mut header)
        .map_err(|e| SampleError::FileError(format!("Failed to read RIFF header: {}", e)))?;

    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(SampleError::UnsupportedFormat("Not a valid RIFF/WAVE file".to_string()));
    }

    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits_per_sample = 0u16;
    let mut sample_format = hound::SampleFormat::Int;
    let mut data: Option<Vec<u8>> = None;

    loop {
        let mut chunk_header = [0u8; 8];
        if reader.read_exact(&mut chunk_header).is_err() {
            break;
        }
        let chunk_id = &chunk_header[0..4];
        let chunk_size = u32::from_le_bytes([
            chunk_header[4], chunk_header[5], chunk_header[6], chunk_header[7],
        ]) as usize;

        if chunk_id == b"fmt " {
            // Lees zoveel bytes als het bestand zegt te hebben — geen aanname over precieze grootte.
            // We lezen min(chunk_size, 40) want na byte 16 (PCM) of byte 18 (extensible) hebben we wat
            // we nodig hebben; rest is padding/extra fmt info die we negeren.
            let to_read = chunk_size.min(40);
            let mut fmt_data = vec![0u8; to_read];
            reader.read_exact(&mut fmt_data)
                .map_err(|e| SampleError::FileError(format!("Failed to read fmt chunk: {}", e)))?;

            if fmt_data.len() < 16 {
                return Err(SampleError::UnsupportedFormat("fmt chunk te klein".to_string()));
            }

            let audio_format = u16::from_le_bytes([fmt_data[0], fmt_data[1]]);
            channels = u16::from_le_bytes([fmt_data[2], fmt_data[3]]);
            sample_rate = u32::from_le_bytes([fmt_data[4], fmt_data[5], fmt_data[6], fmt_data[7]]);
            bits_per_sample = u16::from_le_bytes([fmt_data[14], fmt_data[15]]);

            sample_format = match audio_format {
                1 => hound::SampleFormat::Int,           // WAVE_FORMAT_PCM
                3 => hound::SampleFormat::Float,         // WAVE_FORMAT_IEEE_FLOAT
                0xFFFE => {
                    // WAVE_FORMAT_EXTENSIBLE: subformaat staat in bytes 24-39 (eerste 2 bytes)
                    if fmt_data.len() >= 26 {
                        let subformat = u16::from_le_bytes([fmt_data[24], fmt_data[25]]);
                        if subformat == 3 { hound::SampleFormat::Float } else { hound::SampleFormat::Int }
                    } else {
                        hound::SampleFormat::Int
                    }
                }
                _ => hound::SampleFormat::Int, // optimistische default
            };

            // Skip eventuele extra fmt-bytes
            if chunk_size > to_read {
                let extra = chunk_size - to_read;
                let skip = if extra % 2 == 1 { extra + 1 } else { extra };
                reader.seek(SeekFrom::Current(skip as i64))
                    .map_err(|e| SampleError::FileError(format!("Failed to skip extra fmt: {}", e)))?;
            } else if chunk_size % 2 == 1 {
                reader.seek(SeekFrom::Current(1)).ok();
            }
        } else if chunk_id == b"data" {
            let mut raw = vec![0u8; chunk_size];
            reader.read_exact(&mut raw)
                .map_err(|e| SampleError::FileError(format!("Failed to read data chunk: {}", e)))?;
            data = Some(raw);
            break;
        } else {
            // Onbekende chunk → overslaan (incl. word-padding)
            let skip = if chunk_size % 2 == 1 { chunk_size + 1 } else { chunk_size };
            reader.seek(SeekFrom::Current(skip as i64))
                .map_err(|e| SampleError::FileError(format!("Failed to skip chunk: {}", e)))?;
        }
    }

    let raw_data = data.ok_or_else(|| SampleError::UnsupportedFormat("Geen data chunk gevonden".into()))?;

    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
        return Err(SampleError::UnsupportedFormat("Ongeldige fmt-velden".into()));
    }

    let samples: Vec<f32> = match (sample_format, bits_per_sample) {
        (hound::SampleFormat::Float, 32) => raw_data
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
        (hound::SampleFormat::Int, 16) => {
            let max = i16::MAX as f32;
            raw_data.chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / max)
                .collect()
        }
        (hound::SampleFormat::Int, 24) => {
            let max = (1i32 << 23) as f32;
            raw_data.chunks_exact(3)
                .map(|b| {
                    let val = ((b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i32) << 16))
                        << 8 >> 8; // sign extend
                    val as f32 / max
                })
                .collect()
        }
        (hound::SampleFormat::Int, 32) => {
            let max = i32::MAX as f32;
            raw_data.chunks_exact(4)
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / max)
                .collect()
        }
        // Zelfde normalisatie (/128) als het hound-pad, anders klinkt een
        // 8-bit bestand ~0,07 dB harder afhankelijk van welke parser hem las.
        (hound::SampleFormat::Int, 8) => raw_data.iter()
            .map(|&b| ((b as i16) - 128) as f32 / 128.0)
            .collect(),
        _ => {
            return Err(SampleError::UnsupportedFormat(
                format!("{:?} {} bits niet ondersteund in manual loader", sample_format, bits_per_sample),
            ));
        }
    };

    // Stereo → mono of multi-channel → eerste kanaal
    let mono: Vec<f32> = if channels == 2 {
        samples.chunks_exact(2).map(|c| (c[0] + c[1]) * 0.5).collect()
    } else if channels == 1 {
        samples
    } else {
        samples.into_iter().step_by(channels as usize).collect()
    };

    Ok(SampleData {
        data: mono,
        sample_rate,
        channels,
        loop_start: None,
        loop_end: None,
    })
}

/// Generic loader via symphonia — voor MP3 en als WAV-fallback wanneer hound faalt.
pub fn load_via_symphonia(path: &Path, extension_hint: &str) -> Result<SampleData, SampleError> {
    let file = File::open(path)
        .map_err(|e| SampleError::FileError(format!("{}: {}", path.display(), e)))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    // Create a hint to help the prober identify the format
    let mut hint = Hint::new();
    hint.with_extension(extension_hint);

    // Probe the media source
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| SampleError::DecodeError(format!("Failed to probe: {}", e)))?;

    let mut format = probed.format;

    // Find the first audio track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| SampleError::DecodeError("No audio track found".to_string()))?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2) as u16;

    debug!("Loading MP3: {:?}, {} Hz, {} ch", path.file_name(), sample_rate, channels);

    // Create a decoder
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| SampleError::DecodeError(format!("Failed to create decoder: {}", e)))?;

    let mut all_samples: Vec<f32> = Vec::new();

    // Decode all packets
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(SampleError::DecodeError(format!("Failed to read packet: {}", e))),
        };

        // Skip packets from other tracks
        if packet.track_id() != track_id {
            continue;
        }

        // Decode the packet
        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(e) => {
                warn!("Failed to decode packet: {}", e);
                continue;
            }
        };

        // Get the audio buffer spec
        let spec = *decoded.spec();
        let num_frames = decoded.frames();

        // Create a sample buffer
        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);

        // Extend our samples
        all_samples.extend(sample_buf.samples());
    }

    // Convert to mono if stereo
    let mono = if channels == 2 {
        all_samples.chunks(2)
            .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5)
            .collect()
    } else if channels == 1 {
        all_samples
    } else {
        // Take first channel for multi-channel
        all_samples.into_iter().step_by(channels as usize).collect()
    };

    Ok(SampleData {
        data: mono,
        sample_rate,
        channels,
        loop_start: None, // MP3 doesn't support loop points
        loop_end: None,
    })
}

/// Load an audio file (auto-detect format based on extension)
pub fn load_audio(path: &Path) -> Result<SampleData, SampleError> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "wav" => load_wav(path),
        "mp3" => load_mp3(path),
        _ => Err(SampleError::UnsupportedFormat(format!("Unknown extension: {}", ext))),
    }
}

/// Load preload buffer for any audio format
pub fn load_audio_preload(path: &Path, preload_samples: usize) -> Result<PreloadBuffer, SampleError> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "wav" => load_wav_preload(path, preload_samples),
        "mp3" => {
            // For MP3, we need to load the entire file and take the first N samples
            // (MP3 doesn't support random access like WAV)
            let full_sample = load_mp3(path)?;
            let attack_len = preload_samples.min(full_sample.data.len());
            let raw_attack = full_sample.data[..attack_len].to_vec();

            // Attack alignment: trim leading silence
            let onset_threshold = 0.002;
            let pre_onset_keep = 32;
            let onset_pos = raw_attack.iter().position(|&s| s.abs() > onset_threshold).unwrap_or(0);
            let trim_start = onset_pos.saturating_sub(pre_onset_keep);
            let attack_data = if trim_start > 0 {
                raw_attack[trim_start..].to_vec()
            } else {
                raw_attack
            };

            Ok(PreloadBuffer {
                attack_data,
                sample_rate: full_sample.sample_rate,
                total_samples: full_sample.data.len().saturating_sub(trim_start),
                loop_start: None,
                loop_end: None,
                source_path: path.to_path_buf(),
                channels: full_sample.channels,
                bits_per_sample: 16, // MP3 is typically decoded to 16-bit equivalent
                data_offset: 0,
                trim_start,
                align_table: None,
            })
        }
        _ => Err(SampleError::UnsupportedFormat(format!("Unknown extension: {}", ext))),
    }
}

#[cfg(test)]
mod loop_opt_tests {
    use super::*;

    fn seam_mismatch(sd: &SampleData) -> f32 {
        let ls = sd.loop_start.unwrap() as usize;
        let le = sd.loop_end.unwrap() as usize;
        let win = 128;
        let mut ssd = 0.0f32;
        for k in 0..win {
            let d = sd.data[ls + k] - sd.data[le + k];
            ssd += d * d;
        }
        ssd
    }

    #[test]
    fn resample_compenseert_fft_delay() {
        // Impuls op een bekende positie: na resamplen 44100→48000 moet de piek
        // (vrijwel) exact op positie×ratio liggen. Zonder delay-compensatie lag
        // hij honderden samples later — dat gaf een positiesprong (tik) bij de
        // preload→full-upgrade en extra aanslagvertraging bij 44.1k-sets.
        let sr_in = 44100u32;
        let sr_out = 48000u32;
        let n = 44100usize;
        let mut data = vec![0.0f32; n];
        let impulse_pos = 10_000usize;
        data[impulse_pos] = 1.0;
        let sd = SampleData { data, sample_rate: sr_in, channels: 1, loop_start: None, loop_end: None };
        let out = resample(&sd, sr_out).expect("resample");

        let ratio = sr_out as f64 / sr_in as f64;
        let expected = (impulse_pos as f64 * ratio) as i64;
        let peak_pos = out.data.iter().enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .map(|(i, _)| i as i64)
            .unwrap();
        let err = (peak_pos - expected).abs();
        assert!(err <= 4, "resample-piek verschoven met {} samples (verwacht ≤4)", err);

        let expected_len = (n as f64 * ratio).round() as i64;
        assert!((out.data.len() as i64 - expected_len).abs() <= 2,
                "resample-lengte {} wijkt af van verwacht {}", out.data.len(), expected_len);
    }

    #[test]
    fn optimize_snaps_loop_end_to_phase_match() {
        // A pure sine with period 200 samples. Phase-matched loop ends sit at
        // ls + k*200. We author a loop end ~0.45 period off → a bad (clicky) seam.
        let period = 200.0f32;
        let n = 8000usize;
        let data: Vec<f32> = (0..n)
            .map(|i| (i as f32 / period * std::f32::consts::TAU).sin())
            .collect();
        let ls = 1000usize;
        let le_authored = 5000 + 90; // 90 samples ≈ 0.45 period off a true match at 5000
        let mut sd = SampleData {
            data,
            sample_rate: 48000,
            channels: 1,
            loop_start: Some(ls as u64),
            loop_end: Some(le_authored as u64),
        };

        let before = seam_mismatch(&sd);
        optimize_loop_points(&mut sd);
        let after = seam_mismatch(&sd);

        assert!(
            after < before * 0.25,
            "loop optimization should sharply reduce seam mismatch: before={} after={}",
            before,
            after
        );
    }
}
