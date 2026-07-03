//! MP3-recorder voor de live audio-output.
//!
//! Architectuur:
//!  - De UI roept `start_recording(path)` aan → er wordt een background-thread
//!    gespawnd die een LAME-encoder opent en op een crossbeam-channel wacht.
//!  - De audio-render-callback (audio.rs) pusht na elke buffer een chunk
//!    interleaved stereo-frames (f32) via `try_send`. Bij overflow (channel vol)
//!    wordt de chunk gedropt en alleen geteld — de audio-thread blokkeert nooit.
//!  - `stop_recording()` zet de "stop"-vlag; de thread leegt het channel,
//!    flusht de encoder en sluit het bestand.
//!
//! Bestandsformaat: 320 kbps stereo MP3 in de native sample rate van de output.

use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use tracing::{info, error};

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, TrySendError};

/// Eén chunk = ~10ms bij 48 kHz stereo. We bufferen 200 chunks (~2 sec) voor pieken.
const CHANNEL_CAPACITY: usize = 200;

/// Live status van de recorder — pollbaar vanuit de UI.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RecorderStatus {
    pub recording: bool,
    pub path: Option<String>,
    pub frames_written: u64,
    pub frames_dropped: u64,
    pub seconds: f64,
    pub error: Option<String>,
}

pub struct Recorder {
    sender: RwLock<Option<Sender<Vec<f32>>>>,
    /// Handle van de encoder-thread; bij stop joinen we erop zodat het bestand
    /// volledig geflusht + gesloten is voordat stop() terugkeert.
    join_handle: RwLock<Option<thread::JoinHandle<()>>>,
    stop_flag: Arc<AtomicBool>,
    frames_written: Arc<AtomicU64>,
    frames_dropped: Arc<AtomicU64>,
    sample_rate: Arc<AtomicU64>,
    path: RwLock<Option<PathBuf>>,
    last_error: Arc<RwLock<Option<String>>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            sender: RwLock::new(None),
            join_handle: RwLock::new(None),
            stop_flag: Arc::new(AtomicBool::new(false)),
            frames_written: Arc::new(AtomicU64::new(0)),
            frames_dropped: Arc::new(AtomicU64::new(0)),
            sample_rate: Arc::new(AtomicU64::new(48000)),
            path: RwLock::new(None),
            last_error: Arc::new(RwLock::new(None)),
        }
    }

    pub fn is_recording(&self) -> bool {
        self.sender.read().is_some()
    }

    /// Door de audio-render-callback aangeroepen. Mag NOOIT blokkeren.
    /// Verwacht interleaved stereo f32 (L, R, L, R, ...). Bij overflow wordt
    /// het frame gedropt en geteld zodat de UI dat kan tonen.
    #[inline]
    pub fn push_stereo(&self, frame: &[f32]) {
        let sender_guard = self.sender.read();
        let Some(sender) = sender_guard.as_ref() else { return; };
        match sender.try_send(frame.to_vec()) {
            Ok(_) => {}
            Err(TrySendError::Full(v)) => {
                // Drop het frame — audio-thread mag niet blokkeren. Tel hoeveel frames
                // we missen zodat de gebruiker een waarschuwing krijgt.
                let dropped = v.len() / 2;
                self.frames_dropped.fetch_add(dropped as u64, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {
                // Thread is gestopt — niets doen, sender wordt zo geleegd.
            }
        }
    }

    /// Start een opname. `sample_rate` moet de actuele output-rate zijn.
    pub fn start(&self, path: PathBuf, sample_rate: u32) -> Result<(), String> {
        if self.is_recording() {
            return Err("Er loopt al een opname".to_string());
        }
        // Wacht tot een eventuele vorige encoder-thread volledig is afgesloten (bestand
        // gesloten), zodat we nooit twee threads naar (mogelijk) hetzelfde bestand laten
        // schrijven.
        if let Some(h) = self.join_handle.write().take() { let _ = h.join(); }

        // Reset tellers
        self.frames_written.store(0, Ordering::Relaxed);
        self.frames_dropped.store(0, Ordering::Relaxed);
        self.sample_rate.store(sample_rate as u64, Ordering::Relaxed);
        self.stop_flag.store(false, Ordering::Relaxed);
        *self.last_error.write() = None;

        // Zorg dat het doelmap-pad bestaat.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Map maken voor opname: {}", e))?;
        }

        let (tx, rx) = bounded::<Vec<f32>>(CHANNEL_CAPACITY);
        *self.sender.write() = Some(tx);
        *self.path.write() = Some(path.clone());

        let stop_flag = self.stop_flag.clone();
        let frames_written = self.frames_written.clone();
        let err_for_thread = self.last_error.clone();

        // Background encoder-thread; fouten landen in self.last_error (zichtbaar via status()).
        let handle = thread::spawn(move || {
            if let Err(e) = encoder_loop(&path, sample_rate, rx, stop_flag, frames_written) {
                error!("Recorder thread failed: {}", e);
                *err_for_thread.write() = Some(e);
            }
        });
        *self.join_handle.write() = Some(handle);

        info!("Opname gestart: {:?}", self.path.read().as_ref());
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        if !self.is_recording() {
            return Err("Geen opname actief".to_string());
        }
        self.stop_flag.store(true, Ordering::Relaxed);
        // Sluit de sender → encoder-loop leegt rx en stopt.
        *self.sender.write() = None;
        // Wacht tot de encoder-thread klaar is (flush + bestand sluiten), zodat het
        // MP3-bestand compleet is wanneer stop() terugkeert. Dekt ook app-afsluiten:
        // de frontend roept stop_recording aan vóór het venster sluit.
        if let Some(h) = self.join_handle.write().take() { let _ = h.join(); }
        info!("Opname gestopt");
        Ok(())
    }

    pub fn status(&self) -> RecorderStatus {
        let recording = self.is_recording();
        let frames = self.frames_written.load(Ordering::Relaxed);
        let dropped = self.frames_dropped.load(Ordering::Relaxed);
        let sr = self.sample_rate.load(Ordering::Relaxed).max(1) as f64;
        let path = self.path.read().as_ref().map(|p| p.to_string_lossy().to_string());
        let error = self.last_error.read().clone();
        RecorderStatus {
            recording,
            path,
            frames_written: frames,
            frames_dropped: dropped,
            seconds: frames as f64 / sr,
            error,
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // Vangnet: als de recorder wordt opgeruimd terwijl er nog een opname loopt,
        // netjes stoppen (flush + bestand sluiten). De normale route is stop() vanuit
        // de UI vóór het afsluiten.
        if self.is_recording() {
            let _ = self.stop();
        }
    }
}

fn encoder_loop(
    path: &std::path::Path,
    sample_rate: u32,
    rx: Receiver<Vec<f32>>,
    stop_flag: Arc<AtomicBool>,
    frames_written: Arc<AtomicU64>,
) -> Result<(), String> {
    use mp3lame_encoder::{Builder, FlushNoGap, InterleavedPcm, Quality, Bitrate};
    use std::io::Write;
    // We encoderen via het i16-pad (lame_encode_buffer_interleaved): dat is de meest
    // universeel-correcte route en omzeilt eventuele float-schaal-verschillen tussen
    // libmp3lame-builds. Genormaliseerde f32 [-1,1] → i16 [-32768,32767].

    let mut builder = Builder::new().ok_or("LAME init mislukt")?;
    builder.set_num_channels(2).map_err(|e| format!("LAME kanalen: {:?}", e))?;
    builder.set_sample_rate(sample_rate).map_err(|e| format!("LAME rate: {:?}", e))?;
    builder.set_brate(Bitrate::Kbps320).map_err(|e| format!("LAME brate: {:?}", e))?;
    builder.set_quality(Quality::Best).map_err(|e| format!("LAME quality: {:?}", e))?;
    let mut encoder = builder.build().map_err(|e| format!("LAME build: {:?}", e))?;

    let mut file = std::fs::File::create(path).map_err(|e| format!("Bestand openen: {}", e))?;

    let started = Instant::now();
    let mut last_log = Instant::now();

    loop {
        match rx.try_recv() {
            Ok(chunk) => {
                let per_channel = chunk.len() / 2;
                if per_channel == 0 { continue; }
                // f32 [-1,1] → i16 PCM (interleaved L/R).
                let pcm: Vec<i16> = chunk.iter()
                    .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
                    .collect();
                let mut buf: Vec<u8> = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(per_channel) + 7200);
                let written = encoder.encode(InterleavedPcm(&pcm), buf.spare_capacity_mut())
                    .map_err(|e| format!("LAME encode: {:?}", e))?;
                unsafe { buf.set_len(written); }
                file.write_all(&buf).map_err(|e| format!("Schrijven: {}", e))?;
                frames_written.fetch_add(per_channel as u64, Ordering::Relaxed);
            }
            Err(TryRecvError::Empty) => {
                if stop_flag.load(Ordering::Relaxed) { break; }
                thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(TryRecvError::Disconnected) => {
                break;
            }
        }
        if last_log.elapsed().as_secs() >= 10 {
            info!("Opname loopt: {:.1}s, {} frames", started.elapsed().as_secs_f32(), frames_written.load(Ordering::Relaxed));
            last_log = Instant::now();
        }
    }

    // Flush
    let mut tail: Vec<u8> = Vec::with_capacity(8192);
    let flushed = encoder.flush::<FlushNoGap>(tail.spare_capacity_mut())
        .map_err(|e| format!("LAME flush: {:?}", e))?;
    unsafe { tail.set_len(flushed); }
    file.write_all(&tail).map_err(|e| format!("Tail schrijven: {}", e))?;
    file.flush().ok();

    let secs = started.elapsed().as_secs_f32();
    info!("Opname klaar: {:?} ({:.1}s)", path, secs);
    Ok(())
}

#[cfg(test)]
mod tests {
    use mp3lame_encoder::{Builder, FlushNoGap, InterleavedPcm, Quality, Bitrate};

    // Encodeert genormaliseerde f32 stereo via hetzelfde i16-pad als de recorder.
    fn encode_stereo(samples_f32: &[f32], sr: u32) -> Vec<u8> {
        let mut b = Builder::new().unwrap();
        b.set_num_channels(2).unwrap();
        b.set_sample_rate(sr).unwrap();
        b.set_brate(Bitrate::Kbps192).unwrap();
        b.set_quality(Quality::Decent).unwrap();
        let mut enc = b.build().unwrap();
        let pcm: Vec<i16> = samples_f32.iter().map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
        let per_channel = pcm.len() / 2;
        let mut out: Vec<u8> = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(per_channel) + 7200);
        let w = enc.encode(InterleavedPcm(&pcm), out.spare_capacity_mut()).unwrap();
        unsafe { out.set_len(w); }
        let mut tail: Vec<u8> = Vec::with_capacity(7200);
        let f = enc.flush::<FlushNoGap>(tail.spare_capacity_mut()).unwrap();
        unsafe { tail.set_len(f); }
        out.extend_from_slice(&tail);
        out
    }

    // Bewijs dat het encode-pad echte AUDIO produceert (niet stilte): encodeer een
    // luide sinus naar een MP3-bestand, decodeer terug en meet de RMS. Bij CBR zegt de
    // bestandsgrootte niets (stilte wordt gepad tot dezelfde bitrate), dus we meten het
    // daadwerkelijke signaal. Een kapotte sample-schaal zou hier RMS ~0 geven.
    #[test]
    fn encoded_audio_is_not_silent() {
        let sr = 48000u32;
        let n = sr as usize; // 1,0 s
        let mut sine: Vec<f32> = Vec::with_capacity(n * 2);
        for i in 0..n {
            let s = (2.0 * std::f32::consts::PI * 440.0 * (i as f32) / sr as f32).sin() * 0.7;
            sine.push(s); sine.push(s);
        }
        let mp3 = encode_stereo(&sine, sr);
        let path = std::env::temp_dir().join("jm_orgue_rec_test.mp3");
        std::fs::write(&path, &mp3).unwrap();

        let (samples, ch, _rate) = crate::silence::decode_mp3_interleaved(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(!samples.is_empty(), "gedecodeerde MP3 is leeg");
        assert_eq!(ch, 2, "verwacht stereo");
        let sumsq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sumsq / samples.len() as f64).sqrt();
        // 0.7-amplitude sinus → RMS ~0.49. Eis ruim boven stilte.
        assert!(rms > 0.1, "RMS van opgenomen audio te laag ({:.4}) — opname is (bijna) stil", rms);
    }
}
