//! Audio playback module using cpal with preload buffer system
//!
//! This module handles audio output with a hybrid approach:
//! - Attack portion (~200ms) is preloaded for instant response
//! - Full samples are loaded in background on first play
//! - Similar to Kontakt's disk streaming but simplified

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread;
use std::path::PathBuf;
use parking_lot::RwLock;
use crossbeam_channel::{Sender, Receiver, bounded};
use tracing::{info, error, warn, debug};

use vpo_sampler::{LoadedOrgan, SampleRef, SampleData, load_audio, resample, PreloadBuffer};
use vpo_audio::{ConvolutionReverb, OnePoleFilter, WindModel, Tremulant, FdnReverb, ParametricEq};

/// Preload buffer size in samples (~2s at 48kHz)
/// Longer buffer = smoother loop while full sample loads in background
pub const PRELOAD_SAMPLES: usize = 96000;

/// Sample source with preload buffer for instant playback
#[derive(Debug, Clone)]
pub enum SampleSource {
    /// Fully loaded sample (for small samples or after background load)
    Loaded(SampleRef),
    /// Preload buffer only - attack ready, full sample not yet loaded
    Preloaded {
        buffer: Arc<PreloadBuffer>,
        target_rate: u32,
    },
    /// Both preload and full sample available
    Ready {
        preload: Arc<PreloadBuffer>,
        full_sample: SampleRef,
    },
}

/// Audio command sent to the audio thread
#[derive(Debug, Clone)]
pub enum AudioCommand {
    /// Note on: (stop_id, pipe_num, midi_note, velocity)
    NoteOn { stop_id: u32, pipe_num: u32, midi_note: u8, velocity: f32 },
    /// Note off: (stop_id, pipe_num)
    NoteOff { stop_id: u32, pipe_num: u32 },
    /// All notes off
    AllNotesOff,
    /// Set master volume in dB
    SetMasterGain(f32),
    /// Multiplicatieve expressie-factor 0.0..1.0 (MIDI-volumepedaal CC7/CC11) —
    /// schaalt op de master-gain zonder de slider-instelling te overschrijven.
    SetMasterExpression(f32),
    /// Set reverb mix (0.0-1.0)
    SetReverbMix(f32),
    /// Load samples (fully preloaded - legacy)
    LoadSamples(Arc<HashMap<(u32, u32), SampleRef>>),
    /// Register preload buffers (attack portions only - fast loading!)
    RegisterPreloadBuffers(Arc<HashMap<(u32, u32), Arc<PreloadBuffer>>>),
    /// Register tremulant preload buffers (keyed same as dry, used when trem is on)
    RegisterTremulantBuffers(Arc<HashMap<(u32, u32), Arc<PreloadBuffer>>>),
    /// Register a fully loaded sample (from background loading)
    RegisterFullSample { key: (u32, u32), sample: SampleRef },
    /// Enable/disable tremulant for a set of stop IDs
    SetTremulant { stop_ids: Vec<u32>, active: bool },
    /// Set per-division gain (0.0-1.0) for swell box
    SetDivisionGain { division_index: u8, gain: f32 },
    /// Configure swell box parameters per division
    SetSwellConfig { division_index: u8, min_db: f32, filter_cutoff_closed: f32 },
    /// Configure wind model per **groep** (was per division — wind groep met identity-default
    /// is backward compatible: divisie i → groep i)
    SetWindModel { division_index: u8, enabled: bool, reservoir_size: f32, damping: f32, max_sag: f32 },
    /// Wijs een divisie aan een wind-groep toe (0..31). Default identity: divisie i → groep i.
    SetDivisionWindGroup { division_index: u8, group: u8 },
    /// Configure tremulant LFO per division (fallback when no trem samples exist)
    SetTremulantLFO { division_index: u8, active: bool, rate: f32, amp_depth: f32, pitch_depth: f32 },
    /// Register stop_id → division_index mapping (set when organ loads)
    RegisterStopDivisionMap(HashMap<u32, u8>),
    /// Set temperament: 12 cent offsets [C,C#,D,...,B] + global fine-tuning in cents
    SetTemperament { note_offsets: [f32; 12], fine_tune: f32 },
    /// Load impulse response for convolution reverb
    LoadImpulseResponse(PathBuf),
    /// Configure algorithmic FDN reverb (preset=None means custom params)
    SetAlgorithmicReverb { preset: Option<u8>, rt60: f32, pre_delay_ms: f32, damping: f32, room_size: f32, mix: f32 },
    /// Switch reverb type: true=algorithmic, false=convolution
    SetReverbType { algorithmic: bool },
    /// Configure 3-band parametric EQ
    SetParametricEq { enabled: bool, low_freq: f32, low_gain: f32, mid_freq: f32, mid_gain: f32, mid_q: f32, high_freq: f32, high_gain: f32 },
    /// Route a division naar een willekeurige set fysieke output-kanalen.
    /// Lege lijst = standaard (voorste paar 0/1). Opeenvolgende kanalen worden als
    /// stereo-paren (L,R) gevuld; een los laatste kanaal krijgt mono.
    SetDivisionOutputChannels { division_index: u8, channels: Vec<u8> },
    /// Set per-division stereo pan (-1.0=left, 0.0=center, 1.0=right)
    SetDivisionPan { division_index: u8, pan: f32 },
    /// C/Cis-lade spreiding (globaal): sterkte 0..1, afval-met-toonhoogte 0..1, kanten omdraaien.
    /// Even tonen (C,D,E,Fis,Gis,Ais) → één kant, oneven (Cis,Dis,F,G,A,B) → de andere.
    SetCcisSpread { strength: f32, falloff: f32, swap: bool },
    /// Zet de C/Cis-spreiding aan/uit per divisie.
    SetDivisionCcis { division_index: u8, enabled: bool },
    /// Set per-pipe voicing (volume + pitch adjustment)
    SetPipeVoicing { stop_id: u32, pipe_num: u32, volume_db: f32, pitch_cents: f32 },
    /// Clear all samples
    ClearSamples,
    /// Shutdown
    Shutdown,
}

/// Sample data source for a playing voice
enum VoiceSampleSource {
    /// Full sample available
    Full(SampleRef),
    /// Only preload buffer (waiting for full sample)
    PreloadOnly {
        preload: Arc<PreloadBuffer>,
        /// Flag to request background loading
        needs_full_load: bool,
    },
}

/// A playing voice (internal to audio thread)
struct PlayingVoice {
    source: VoiceSampleSource,
    position: f64,
    /// Playback rate (sample_rate / output_rate) for pitch correction
    pub rate: f64,
    envelope: f32,
    target_envelope: f32,
    envelope_speed: f32,
    releasing: bool,
    stop_id: u32,
    pipe_num: u32,
    midi_note: u8,
    /// When using preload, track if we've requested full load
    load_requested: bool,
}

impl PlayingVoice {
    fn new_from_sample(sample: SampleRef, stop_id: u32, pipe_num: u32, midi_note: u8, velocity: f32) -> Self {
        Self {
            source: VoiceSampleSource::Full(sample),
            position: 0.0,
            rate: 1.0,
            envelope: 0.0,
            target_envelope: velocity,
            envelope_speed: 0.005,
            releasing: false,
            stop_id,
            pipe_num,
            midi_note,
            load_requested: false,
        }
    }

    fn new_from_preload(preload: Arc<PreloadBuffer>, stop_id: u32, pipe_num: u32, midi_note: u8, velocity: f32, output_sample_rate: u32) -> Self {
        // Calculate playback rate to correct for sample rate mismatch
        let rate = preload.sample_rate as f64 / output_sample_rate as f64;
        Self {
            source: VoiceSampleSource::PreloadOnly { preload, needs_full_load: true },
            position: 0.0,
            rate,
            envelope: 0.0,
            target_envelope: velocity,
            envelope_speed: 0.005,
            releasing: false,
            stop_id,
            pipe_num,
            midi_note,
            load_requested: false,
        }
    }

    /// Upgrade from preload to full sample (called when background load completes)
    /// The full sample is already resampled to output rate, so reset rate to 1.0
    fn upgrade_to_full(&mut self, sample: SampleRef) {
        // The preload buffer may have been playing at a different rate (sample rate mismatch).
        // The full sample is resampled to output rate (rate=1.0).
        // We need to convert the current position from preload-rate to full-sample-rate.
        let old_rate = self.rate;
        if old_rate > 0.0 && (old_rate - 1.0).abs() > 0.01 {
            // Position in preload was at preload sample rate;
            // full sample is at output rate. Convert position.
            self.position /= old_rate;
        }
        // Clamp position to valid range
        let max_pos = sample.data.len().saturating_sub(2) as f64;
        if self.position > max_pos {
            self.position = max_pos;
        }
        self.source = VoiceSampleSource::Full(sample);
        self.rate = 1.0;
    }

    /// Create a new voice for crossfade: starts from given position, fades in slowly
    fn new_crossfade(preload: Arc<PreloadBuffer>, stop_id: u32, pipe_num: u32, midi_note: u8, target_envelope: f32, position: f64, output_sample_rate: u32) -> Self {
        let rate = preload.sample_rate as f64 / output_sample_rate as f64;
        Self {
            source: VoiceSampleSource::PreloadOnly { preload, needs_full_load: true },
            position,
            rate,
            envelope: 0.0,           // Start silent
            target_envelope,         // Fade up to original level
            envelope_speed: 0.00015, // ~140ms crossfade at 48kHz
            releasing: false,
            stop_id,
            pipe_num,
            midi_note,
            load_requested: false,
        }
    }

    fn release(&mut self) {
        self.releasing = true;
        self.target_envelope = 0.0;
        self.envelope_speed = 0.0005;
    }

    /// Slow release for tremulant crossfade (fade out over ~140ms)
    fn crossfade_release(&mut self) {
        self.releasing = true;
        self.target_envelope = 0.0;
        self.envelope_speed = 0.00015; // Match crossfade speed
    }

    fn is_finished(&self) -> bool {
        self.releasing && self.envelope < 0.0001
    }

    fn needs_background_load(&self) -> Option<PathBuf> {
        if self.load_requested {
            return None;
        }
        match &self.source {
            VoiceSampleSource::PreloadOnly { preload, needs_full_load } if *needs_full_load => {
                Some(preload.source_path.clone())
            }
            _ => None,
        }
    }

    fn mark_load_requested(&mut self) {
        self.load_requested = true;
    }

    fn get_loop_points(&self) -> (Option<u64>, Option<u64>) {
        match &self.source {
            VoiceSampleSource::Full(sample) => (sample.loop_start, sample.loop_end),
            VoiceSampleSource::PreloadOnly { preload, .. } => (preload.loop_start, preload.loop_end),
        }
    }

    fn get_sample_len(&self) -> usize {
        match &self.source {
            VoiceSampleSource::Full(sample) => sample.data.len(),
            VoiceSampleSource::PreloadOnly { preload, .. } => preload.total_samples,
        }
    }

    fn next_sample(&mut self) -> f32 {
        // Update envelope
        if self.envelope < self.target_envelope {
            self.envelope = (self.envelope + self.envelope_speed).min(self.target_envelope);
        } else if self.envelope > self.target_envelope {
            self.envelope = (self.envelope - self.envelope_speed).max(self.target_envelope);
        }

        let (loop_start, loop_end) = self.get_loop_points();
        let total_len = self.get_sample_len();

        if total_len == 0 {
            return 0.0;
        }

        // Read the next sample with a seamless crossfade at the loop seam.
        // Loop region = ODF points when valid, else a fallback over the last 3/4
        // of the buffer. The previous code keyed its crossfade to the absolute
        // sample end, so ODF-looped samples wrapped with a HARD jump → a periodic
        // click ("tikken") on sustained notes. read_voice() blends the loop tail
        // into the pre-loop region so the seam is value-continuous.
        let value = match &self.source {
            VoiceSampleSource::Full(sample) => {
                let len = sample.data.len();
                let (ls, le) = match (loop_start, loop_end) {
                    (Some(a), Some(b)) if b > a && (b as usize) <= len => (a as usize, b as usize),
                    _ => (len / 4, len),
                };
                let xfade = loop_xfade(le.saturating_sub(ls), ls);
                read_voice(&sample.data, ls, le, xfade, &mut self.position, self.releasing, &mut self.envelope)
            }
            VoiceSampleSource::PreloadOnly { preload, .. } => {
                // The preload holds only the attack chunk; loop within it.
                let alen = preload.attack_data.len();
                let ls = alen / 4;
                let xfade = loop_xfade(alen.saturating_sub(ls), ls);
                read_voice(&preload.attack_data, ls, alen, xfade, &mut self.position, self.releasing, &mut self.envelope)
            }
        };

        self.position += self.rate;
        value * self.envelope
    }
}

/// Crossfade length for a loop of `loop_len` samples whose start is at index `ls`.
/// Long crossfades (Hauptwerk-style, up to ~500 ms) mask imperfect loop points;
/// bounded by half the loop and by the available pre-loop data (`ls`).
#[inline]
fn loop_xfade(loop_len: usize, ls: usize) -> usize {
    (loop_len / 2).min(ls).clamp(256, 24_000)
}

/// Read one linearly-interpolated sample from `data`, looping over `[ls, le)` with
/// a crossfaded seam so there is no click at the loop wrap. During release the
/// buffer plays out linearly to its end (no loop). `position` is wrapped in place;
/// `envelope` is forced to 0 when playback finishes.
///
/// The crossfade blends the loop tail `[le-xfade, le)` into the pre-loop region
/// `[ls-xfade, ls)` so the output lands exactly on `data[ls]` at the seam — making
/// it value-continuous regardless of whether the sample's loop points match phase.
#[inline]
fn read_voice(
    data: &[f32],
    ls: usize,
    le: usize,
    xfade: usize,
    position: &mut f64,
    releasing: bool,
    envelope: &mut f32,
) -> f32 {
    let len = data.len();
    if len < 2 {
        *envelope = 0.0;
        return 0.0;
    }

    // During release: no looping — play out to the end, then finish.
    if releasing {
        let i = *position as usize;
        if i >= len - 1 {
            *envelope = 0.0;
            return 0.0;
        }
        let frac = (*position - i as f64) as f32;
        return data[i] + (data[i + 1] - data[i]) * frac;
    }

    let loop_len = le.saturating_sub(ls);

    // Wrap the play position into the loop region.
    if loop_len > 0 && *position >= le as f64 {
        *position = ls as f64 + ((*position - le as f64) % loop_len as f64);
    }

    let p = *position;
    let i = p as usize;
    if i >= len - 1 {
        *position = ls as f64;
        return 0.0;
    }
    let frac = (p - i as f64) as f32;
    let primary = data[i] + (data[i + 1] - data[i]) * frac;

    // Seamless loop seam: within the last `xfade` samples before `le`, crossfade
    // the loop tail into the pre-loop region so the output reaches data[ls] exactly
    // at the wrap (continuous → no click). Needs pre-loop data (ls >= xfade) and a
    // loop longer than the crossfade.
    if loop_len > xfade && ls >= xfade {
        let dist = le as f64 - p; // samples until the seam
        if dist > 0.0 && dist < xfade as f64 {
            let fade = (dist / xfade as f64) as f32; // 1.0 at xfade start → 0.0 at seam
            // Equal-power (constant-energy) crossfade: weights are sin/cos so
            // primary²+blended² == 1 throughout. The loop tail and the pre-loop
            // region are not phase-aligned, so a linear/smoothstep fade would dip
            // ~3 dB at the midpoint every loop → a rhythmic amplitude "tick".
            // sin/cos keeps the level flat across the seam.
            let theta = fade * std::f32::consts::FRAC_PI_2; // 0 → π/2
            let w_primary = theta.sin(); // 1.0 at xfade start
            let w_blended = theta.cos(); // 1.0 at the seam
            let pre = ls as f64 - dist; // walks [ls - xfade, ls)
            let j = pre as usize;
            if j + 1 < len {
                let pfrac = (pre - j as f64) as f32;
                let blended = data[j] + (data[j + 1] - data[j]) * pfrac;
                return primary * w_primary + blended * w_blended;
            }
        }
    }

    primary
}

#[cfg(test)]
mod loop_seam_tests {
    use super::read_voice;

    // Build a sine whose value at `ls` differs from `le` so a naive hard wrap
    // would jump (click). The crossfade must keep the output continuous.
    fn sine(n: usize) -> Vec<f32> {
        (0..n).map(|i| (i as f32 * 0.05).sin()).collect()
    }

    #[test]
    fn loop_seam_is_click_free() {
        let data = sine(6000);
        let (ls, le) = (1000usize, 3000usize);
        let xfade = super::loop_xfade(le - ls, ls);
        let mut pos = 0.0f64;
        let mut env = 1.0f32;
        let mut prev: Option<f32> = None;
        let mut max_jump = 0.0f32;
        // Long enough to cross the seam several times.
        for _ in 0..20_000 {
            let v = read_voice(&data, ls, le, xfade, &mut pos, false, &mut env);
            if let Some(p) = prev {
                max_jump = max_jump.max((v - p).abs());
            }
            prev = Some(v);
            pos += 1.0;
        }
        // Per-sample slope of the sine is ~0.05; a hard-wrap click would be ~|sin(le)-sin(ls)| (up to ~2).
        assert!(max_jump < 0.15, "loop seam click detected: max consecutive jump = {}", max_jump);
    }

    #[test]
    fn hard_wrap_without_crossfade_does_click() {
        // Sanity check: with the crossfade disabled (xfade = 0) the SAME loop jumps,
        // proving the test sample really has a discontinuous seam.
        let data = sine(6000);
        let (ls, le) = (1000usize, 3000usize);
        let mut pos = 0.0f64;
        let mut env = 1.0f32;
        let mut prev: Option<f32> = None;
        let mut max_jump = 0.0f32;
        for _ in 0..20_000 {
            let v = read_voice(&data, ls, le, 0, &mut pos, false, &mut env);
            if let Some(p) = prev {
                max_jump = max_jump.max((v - p).abs());
            }
            prev = Some(v);
            pos += 1.0;
        }
        assert!(max_jump > 0.3, "expected an audible jump without crossfade, got {}", max_jump);
    }
}

/// Audio player handle (Send + Sync safe)
/// Desired audio output selection. `None` fields mean "use the system default".
/// Stored by name (not cpal `HostId`) so it can be persisted and resolved later.
#[derive(Debug, Clone, Default)]
pub struct AudioOutputConfig {
    /// Host/driver name, e.g. "WASAPI" or "ASIO". None = default host.
    pub host_name: Option<String>,
    /// Output device name. None = default output device for the host.
    pub device_name: Option<String>,
    /// Requested buffer size in frames. None/0 = driver default.
    pub buffer_frames: Option<u32>,
}

/// Resolve a cpal host by (case-insensitive) name, falling back to the default host.
pub fn resolve_host(name: Option<&str>) -> cpal::Host {
    if let Some(want) = name {
        if let Some(id) = cpal::available_hosts()
            .into_iter()
            .find(|id| id.name().eq_ignore_ascii_case(want))
        {
            if let Ok(host) = cpal::host_from_id(id) {
                return host;
            }
        }
        warn!("Audio host '{}' not available, using default host", want);
    }
    cpal::default_host()
}

/// Globaal: de actieve recorder waar de audio-render-loop f32 stereo-frames naar pushed.
/// Wordt gezet bij het starten van een opname (commands/test_api); de Recorder zelf
/// checkt `is_recording()` om te bepalen of er gepushed mag worden.
pub static RECORDER: parking_lot::RwLock<Option<std::sync::Arc<crate::recorder::Recorder>>> = parking_lot::RwLock::new(None);

pub struct AudioPlayer {
    command_tx: Sender<AudioCommand>,
    voice_count: Arc<AtomicUsize>,
    peak_left: Arc<RwLock<f32>>,
    peak_right: Arc<RwLock<f32>>,
    running: Arc<AtomicBool>,
    pub sample_rate: Arc<RwLock<u32>>,
    /// Actual host name in use (resolved by the audio thread)
    pub current_host: Arc<RwLock<String>>,
    /// Actual output device name in use
    pub current_device: Arc<RwLock<String>>,
    /// Actual buffer size in frames (0 = driver default)
    pub current_buffer_frames: Arc<RwLock<u32>>,
}

// Explicitly implement Send and Sync since all our fields are thread-safe
unsafe impl Send for AudioPlayer {}
unsafe impl Sync for AudioPlayer {}

impl AudioPlayer {
    /// Create a new audio player
    /// Create an audio player using the system default host/device.
    pub fn new_default() -> Result<Self, String> {
        Self::new(AudioOutputConfig::default())
    }

    /// Create an audio player for a specific host/device/buffer selection.
    pub fn new(cfg: AudioOutputConfig) -> Result<Self, String> {
        let (command_tx, command_rx) = bounded::<AudioCommand>(512);
        let voice_count = Arc::new(AtomicUsize::new(0));
        let peak_left = Arc::new(RwLock::new(0.0f32));
        let peak_right = Arc::new(RwLock::new(0.0f32));
        let running = Arc::new(AtomicBool::new(true));
        let sample_rate_shared = Arc::new(RwLock::new(48000u32));
        let current_host = Arc::new(RwLock::new(String::new()));
        let current_device = Arc::new(RwLock::new(String::new()));
        let current_buffer_frames = Arc::new(RwLock::new(0u32));

        let voice_count_clone = voice_count.clone();
        let peak_left_clone = peak_left.clone();
        let peak_right_clone = peak_right.clone();
        let running_clone = running.clone();

        // Spawn audio thread
        let sr_clone = sample_rate_shared.clone();
        let host_clone = current_host.clone();
        let device_clone = current_device.clone();
        let buffer_clone = current_buffer_frames.clone();
        // Readiness channel: the audio thread reports whether the output stream
        // actually built+started, so we can fail cleanly (e.g. ASIO unavailable)
        // instead of silently ending up with no audio.
        let (ready_tx, ready_rx) = bounded::<Result<(), String>>(1);
        thread::spawn(move || {
            if let Err(e) = run_audio_thread(
                command_rx, voice_count_clone, peak_left_clone, peak_right_clone,
                running_clone, sr_clone, cfg, host_clone, device_clone, buffer_clone, ready_tx,
            ) {
                error!("Audio thread error: {}", e);
            }
        });

        // Wait for the stream to build/start (or fail) before returning.
        match ready_rx.recv_timeout(std::time::Duration::from_secs(8)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("Audio stream did not start within 8s".to_string()),
        }

        info!("Audio player started");

        Ok(Self {
            command_tx,
            voice_count,
            peak_left,
            peak_right,
            running,
            sample_rate: sample_rate_shared,
            current_host,
            current_device,
            current_buffer_frames,
        })
    }

    /// Load samples from a LoadedOrgan
    pub fn load_samples(&self, organ: &LoadedOrgan) {
        let samples = Arc::new(organ.samples.clone());
        // Bounded send with timeout: if the audio thread is stalled (e.g. its
        // output device was lost), give up instead of blocking forever.
        if let Err(e) = self.command_tx.send_timeout(AudioCommand::LoadSamples(samples), std::time::Duration::from_secs(3)) {
            warn!("Failed to send load samples command: {}", e);
        }
        info!("Sent {} samples to audio thread", organ.samples.len());
    }

    /// Send a command. Uses a timeout so a stalled audio thread can never freeze
    /// the caller (e.g. loading an organ from the UI).
    pub fn send_command(&self, cmd: AudioCommand) -> Result<(), String> {
        self.command_tx.send_timeout(cmd, std::time::Duration::from_secs(3))
            .map_err(|e| format!("audio command not delivered (audio thread stalled?): {}", e))
    }

    /// Get current voice count
    pub fn voice_count(&self) -> usize {
        self.voice_count.load(Ordering::Relaxed)
    }

    /// Get peak meters
    pub fn peak_meters(&self) -> (f32, f32) {
        (*self.peak_left.read(), *self.peak_right.read())
    }

    /// Stop playback
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.command_tx.send(AudioCommand::Shutdown);
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Channel for background loading requests
type LoadRequest = ((u32, u32), PathBuf, u32); // (key, path, target_sample_rate)

/// Run the audio thread with cpal
fn run_audio_thread(
    command_rx: Receiver<AudioCommand>,
    voice_count: Arc<AtomicUsize>,
    peak_left: Arc<RwLock<f32>>,
    peak_right: Arc<RwLock<f32>>,
    running: Arc<AtomicBool>,
    sample_rate_out: Arc<RwLock<u32>>,
    cfg: AudioOutputConfig,
    host_out: Arc<RwLock<String>>,
    device_out: Arc<RwLock<String>>,
    buffer_out: Arc<RwLock<u32>>,
    ready_tx: crossbeam_channel::Sender<Result<(), String>>,
) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    // Resolve host (by name, fallback to default host)
    let host = resolve_host(cfg.host_name.as_deref());

    // Resolve output device (by name within the host, fallback to default device)
    let device = cfg.device_name.as_deref()
        .and_then(|want| {
            host.output_devices().ok().and_then(|mut devs| {
                devs.find(|d| d.name().map(|n| n == want).unwrap_or(false))
            })
        })
        .or_else(|| host.default_output_device())
        .ok_or_else(|| "No audio output device found".to_string())?;

    let supported = device.default_output_config()
        .map_err(|e| format!("Failed to get audio config: {}", e))?;

    let sample_rate = supported.sample_rate().0;

    // Honor a requested fixed buffer size only when the device advertises a range
    // that contains it (ASIO does; WASAPI shared mode reports Unknown and ignores
    // fixed sizes, so we fall back to the driver default there).
    let requested = cfg.buffer_frames.filter(|&f| f > 0);
    let buffer_size = match (requested, supported.buffer_size()) {
        (Some(frames), cpal::SupportedBufferSize::Range { min, max })
            if frames >= *min && frames <= *max =>
        {
            cpal::BufferSize::Fixed(frames)
        }
        (Some(frames), _) => {
            warn!("Requested buffer of {} frames not honored by '{}'; using driver default",
                  frames, device.name().unwrap_or_default());
            cpal::BufferSize::Default
        }
        (None, _) => cpal::BufferSize::Default,
    };
    let used_buffer_frames = match buffer_size {
        cpal::BufferSize::Fixed(f) => f,
        _ => 0,
    };

    let mut stream_config: cpal::StreamConfig = supported.config();
    stream_config.buffer_size = buffer_size;

    let host_name = host.id().name().to_string();
    let device_name = device.name().unwrap_or_default();
    *sample_rate_out.write() = sample_rate;
    *host_out.write() = host_name.clone();
    *device_out.write() = device_name.clone();
    *buffer_out.write() = used_buffer_frames;
    info!("Audio host: {}, device: {}, sample rate: {}, buffer: {}, format: {:?}, kanalen: {}",
          host_name, device_name, sample_rate,
          if used_buffer_frames > 0 { used_buffer_frames.to_string() } else { "default".to_string() },
          supported.sample_format(), stream_config.channels);

    // Audio state (local to audio thread)
    // Fully loaded samples (legacy or after background load)
    let samples: Arc<RwLock<HashMap<(u32, u32), SampleRef>>> = Arc::new(RwLock::new(HashMap::new()));
    // Preload buffers (attack portions for instant playback)
    let preloads: Arc<RwLock<HashMap<(u32, u32), Arc<PreloadBuffer>>>> = Arc::new(RwLock::new(HashMap::new()));
    // Tremulant preload buffers (same keys as dry, used when tremulant is active)
    let trem_preloads: Arc<RwLock<HashMap<(u32, u32), Arc<PreloadBuffer>>>> = Arc::new(RwLock::new(HashMap::new()));
    // Set of stop IDs that currently have tremulant active
    let trem_active: Arc<RwLock<std::collections::HashSet<u32>>> = Arc::new(RwLock::new(std::collections::HashSet::new()));
    let voices: Arc<RwLock<Vec<PlayingVoice>>> = Arc::new(RwLock::new(Vec::new()));
    let master_gain: Arc<RwLock<f32>> = Arc::new(RwLock::new(0.5));
    // Expressie-factor van het MIDI-volumepedaal (multiplicatief op master_gain).
    let master_expression: Arc<RwLock<f32>> = Arc::new(RwLock::new(1.0));
    // Per-division gain for swell boxes (max 32 divisions, default 1.0 = fully open)
    let division_gains: Arc<RwLock<Vec<f32>>> = Arc::new(RwLock::new(vec![1.0; 32]));
    // Per-division swell config: (min_db, filter_cutoff_closed_hz)
    let swell_configs: Arc<RwLock<Vec<(f32, f32)>>> = Arc::new(RwLock::new(vec![(-20.0, 800.0); 32]));
    // Per-division stereo pan: -1.0=left, 0.0=center, 1.0=right
    let division_pans: Arc<RwLock<Vec<f32>>> = Arc::new(RwLock::new(vec![0.0; 32]));
    // Per-division output-kanalen: lijst fysieke kanaalindices (leeg = standaard voorste paar)
    let division_output_channels: Arc<RwLock<Vec<Vec<u8>>>> = Arc::new(RwLock::new(vec![Vec::new(); 32]));
    // C/Cis-lade spreiding: per-divisie aan/uit + globale parameters (sterkte, afval, swap)
    let ccis_enabled: Arc<RwLock<Vec<bool>>> = Arc::new(RwLock::new(vec![false; 32]));
    let ccis_params: Arc<RwLock<(f32, f32, bool)>> = Arc::new(RwLock::new((0.7, 0.6, false)));
    // Per-divisie wind-groep toewijzing (default: identity — elke divisie eigen groep)
    let division_wind_groups: Arc<RwLock<Vec<u8>>> = Arc::new(RwLock::new((0..32u8).collect()));
    // Per-division tremulant LFO (fallback when no trem samples)
    let trem_lfos: Arc<RwLock<Vec<Tremulant>>> = Arc::new(RwLock::new(
        (0..32).map(|_| Tremulant::new(6.0, sample_rate)).collect()
    ));
    // Set of stop_ids that have tremulant samples (populated at load time)
    let stops_with_trem_samples: Arc<RwLock<std::collections::HashSet<u32>>> = Arc::new(RwLock::new(std::collections::HashSet::new()));
    // Per-division wind models
    let wind_models: Arc<RwLock<Vec<WindModel>>> = Arc::new(RwLock::new(
        (0..32).map(|_| WindModel::new(sample_rate)).collect()
    ));
    // Per-division low-pass filters for swell box timbral effect (stereo: L+R per divisie
    // zodat per-noot C/Cis-panning behouden blijft door de zwelkast-filtering heen)
    let swell_filters: Arc<RwLock<Vec<(OnePoleFilter, OnePoleFilter)>>> = Arc::new(RwLock::new(
        (0..32).map(|_| (OnePoleFilter::new(20000.0, sample_rate), OnePoleFilter::new(20000.0, sample_rate))).collect()
    ));
    // Mapping from stop_id → division_index (set when organ loads)
    let stop_to_division: Arc<RwLock<HashMap<u32, u8>>> = Arc::new(RwLock::new(HashMap::new()));
    // Global tuning ratio (derived from cents offset)
    let temperament_ratios: Arc<RwLock<[f64; 12]>> = Arc::new(RwLock::new([1.0; 12]));
    // Convolution reverb (initialized when IR is loaded)
    let reverb: Arc<RwLock<Option<ConvolutionReverb>>> = Arc::new(RwLock::new(None));
    let reverb_mix: Arc<RwLock<f32>> = Arc::new(RwLock::new(0.0));
    // Algorithmic FDN reverb
    let fdn_reverb: Arc<RwLock<FdnReverb>> = Arc::new(RwLock::new(FdnReverb::new(sample_rate)));
    let use_algorithmic_reverb: Arc<RwLock<bool>> = Arc::new(RwLock::new(false));
    // Parametric EQ
    let parametric_eq: Arc<RwLock<ParametricEq>> = Arc::new(RwLock::new(ParametricEq::new(sample_rate)));
    // Per-pipe voicing: (stop_id, pipe_num) → (volume_db, pitch_cents)
    let pipe_voicing: Arc<RwLock<HashMap<(u32, u32), (f32, f32)>>> = Arc::new(RwLock::new(HashMap::new()));

    // Channel for background loading
    let (load_tx, load_rx) = bounded::<LoadRequest>(64);
    let (loaded_tx, loaded_rx) = bounded::<((u32, u32), SampleRef)>(64);

    // Spawn background loader thread
    let loaded_tx_clone = loaded_tx.clone();
    let running_bg = running.clone();
    thread::spawn(move || {
        while running_bg.load(Ordering::Relaxed) {
            match load_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok((key, path, target_rate)) => {
                    debug!("Background loading: {:?}", path);
                    match load_audio(&path) {
                        Ok(mut sample_data) => {
                            // Resample if needed
                            if sample_data.sample_rate != target_rate {
                                match resample(&sample_data, target_rate) {
                                    Ok(resampled) => sample_data = resampled,
                                    Err(e) => {
                                        warn!("Failed to resample {:?}: {}", path, e);
                                        continue;
                                    }
                                }
                            }
                            let sample = Arc::new(sample_data);
                            let _ = loaded_tx_clone.send((key, sample));
                            debug!("Background load complete: {:?}", path);
                        }
                        Err(e) => {
                            warn!("Failed to background load {:?}: {}", path, e);
                        }
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
        }
        info!("Background loader thread stopped");
    });

    let samples_clone = samples.clone();
    let preloads_clone = preloads.clone();
    let trem_preloads_clone = trem_preloads.clone();
    let trem_active_clone = trem_active.clone();
    let voices_clone = voices.clone();
    let master_gain_clone = master_gain.clone();
    let master_expression_clone = master_expression.clone();
    let division_gains_clone = division_gains.clone();
    let trem_lfos_clone = trem_lfos.clone();
    let stops_with_trem_clone = stops_with_trem_samples.clone();
    let wind_models_clone = wind_models.clone();
    let division_pans_clone = division_pans.clone();
    let swell_configs_clone = swell_configs.clone();
    let swell_filters_clone = swell_filters.clone();
    let stop_to_division_clone = stop_to_division.clone();
    let temperament_clone = temperament_ratios.clone();
    let reverb_clone = reverb.clone();
    let reverb_mix_clone = reverb_mix.clone();
    let fdn_reverb_clone = fdn_reverb.clone();
    let use_algo_reverb_clone = use_algorithmic_reverb.clone();
    let eq_clone = parametric_eq.clone();
    let voicing_clone = pipe_voicing.clone();
    let output_channels_clone = division_output_channels.clone();
    let ccis_enabled_clone = ccis_enabled.clone();
    let ccis_params_clone = ccis_params.clone();
    let wind_groups_clone = division_wind_groups.clone();
    let voice_count_clone = voice_count.clone();
    let peak_left_clone = peak_left.clone();
    let peak_right_clone = peak_right.clone();
    let running_clone = running.clone();
    let command_rx_clone = command_rx.clone();
    let load_tx_clone = load_tx.clone();

    let channels = stream_config.channels as usize;

    let sample_format = supported.sample_format();
    let mut render = move |data: &mut [f32]| {
            // Flush-to-zero / denormals-are-zero op de audio-thread. De reverb-,
            // EQ- en filter-staarten dalen door denormale float-waarden (< ~1e-38),
            // die op x86 ~100x trager zijn en CPU-pieken geven → buffer-underruns →
            // hoorbaar gekraak. FTZ+DAZ klemt die waarden naar 0 (ruim onder
            // hoorbaar niveau) en houdt de rendertijd stabiel.
            #[cfg(target_arch = "x86_64")]
            #[allow(deprecated)]
            unsafe {
                use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
                _mm_setcsr(_mm_getcsr() | 0x8040); // FTZ (bit 15) | DAZ (bit 6)
            }

            // Check for completed background loads
            while let Ok((key, sample)) = loaded_rx.try_recv() {
                samples_clone.write().insert(key, sample.clone());
                // Upgrade any playing voices using this sample
                for voice in voices_clone.write().iter_mut() {
                    if voice.stop_id == key.0 && voice.pipe_num == key.1 {
                        voice.upgrade_to_full(sample.clone());
                    }
                }
            }

            // Process commands
            while let Ok(cmd) = command_rx_clone.try_recv() {
                match cmd {
                    AudioCommand::NoteOn { stop_id, pipe_num, midi_note, velocity } => {
                        let key = (stop_id, pipe_num);

                        // Check if tremulant is active for this stop
                        let trem_on = trem_active_clone.read().contains(&stop_id);

                        // First check for fully loaded sample
                        let sample_opt = {
                            let samples_lock = samples_clone.read();
                            samples_lock.get(&key).cloned()
                        };

                        if let Some(sample) = sample_opt {
                            // Full sample available - instant playback
                            let voice = PlayingVoice::new_from_sample(sample.clone(), stop_id, pipe_num, midi_note, velocity);
                            voices_clone.write().push(voice);
                            debug!("NoteOn (full): stop={}, pipe={}, note={}", stop_id, pipe_num, midi_note);
                        } else {
                            // Check for preload buffer - use tremulant if active and available
                            let preload_opt = if trem_on {
                                let trem_lock = trem_preloads_clone.read();
                                trem_lock.get(&key).cloned()
                            } else {
                                None
                            }.or_else(|| {
                                let preloads_lock = preloads_clone.read();
                                preloads_lock.get(&key).cloned()
                            });

                            if let Some(preload) = preload_opt {
                                // Start playing from preload buffer immediately
                                let mut voice = PlayingVoice::new_from_preload(preload.clone(), stop_id, pipe_num, midi_note, velocity, sample_rate);

                                // Request background load of full sample
                                let path = preload.source_path.clone();
                                voice.mark_load_requested();
                                let _ = load_tx_clone.try_send((key, path, sample_rate));

                                let rate = voice.rate;
                                voices_clone.write().push(voice);
                                debug!("NoteOn (preload{}): stop={}, pipe={}, rate={}",
                                    if trem_on { "+trem" } else { "" }, stop_id, pipe_num, rate);
                            } else {
                                warn!("NoteOn: No sample or preload for stop={}, pipe={}", stop_id, pipe_num);
                            }
                        }
                    }
                    AudioCommand::NoteOff { stop_id, pipe_num } => {
                        for voice in voices_clone.write().iter_mut() {
                            if voice.stop_id == stop_id && voice.pipe_num == pipe_num && !voice.releasing {
                                voice.release();
                            }
                        }
                    }
                    AudioCommand::AllNotesOff => {
                        for voice in voices_clone.write().iter_mut() {
                            voice.release();
                        }
                    }
                    AudioCommand::SetMasterGain(db) => {
                        *master_gain_clone.write() = 10.0_f32.powf(db / 20.0);
                    }
                    AudioCommand::SetMasterExpression(factor) => {
                        *master_expression_clone.write() = factor.clamp(0.0, 1.0);
                    }
                    AudioCommand::SetReverbMix(mix) => {
                        *reverb_mix_clone.write() = mix;
                        if let Some(ref mut rev) = *reverb_clone.write() {
                            rev.mix = mix;
                        }
                        info!("Reverb mix set to {:.2}", mix);
                    }
                    AudioCommand::LoadSamples(new_samples) => {
                        let mut samples_lock = samples_clone.write();
                        samples_lock.clear();
                        for (k, v) in new_samples.iter() {
                            samples_lock.insert(*k, v.clone());
                        }
                        preloads_clone.write().clear();
                        voices_clone.write().clear();
                        info!("Loaded {} preloaded samples", new_samples.len());
                    }
                    AudioCommand::RegisterPreloadBuffers(buffers) => {
                        // Clear previous and register new preload buffers
                        samples_clone.write().clear();
                        let mut preloads_lock = preloads_clone.write();
                        preloads_lock.clear();
                        for (k, v) in buffers.iter() {
                            preloads_lock.insert(*k, v.clone());
                        }
                        voices_clone.write().clear();
                        info!("Registered {} preload buffers for instant playback", buffers.len());
                    }
                    AudioCommand::RegisterTremulantBuffers(buffers) => {
                        let mut trem_lock = trem_preloads_clone.write();
                        trem_lock.clear();
                        let mut trem_stops = stops_with_trem_clone.write();
                        trem_stops.clear();
                        for (k, v) in buffers.iter() {
                            trem_lock.insert(*k, v.clone());
                            trem_stops.insert(k.0); // stop_id
                        }
                        info!("Registered {} tremulant preload buffers ({} stops with trem samples)", buffers.len(), trem_stops.len());
                    }
                    AudioCommand::RegisterFullSample { key, sample } => {
                        samples_clone.write().insert(key, sample);
                    }
                    AudioCommand::SetTremulant { stop_ids, active } => {
                        // Crossfade: for currently playing voices on these stops,
                        // fade out old source and start new voice from alternate buffer
                        let mut new_voices: Vec<PlayingVoice> = Vec::new();
                        {
                            let mut voices_lock = voices_clone.write();
                            for voice in voices_lock.iter_mut() {
                                if stop_ids.contains(&voice.stop_id) && !voice.releasing {
                                    let key = (voice.stop_id, voice.pipe_num);
                                    // Get alternate buffer: if turning ON → trem buffer, OFF → dry buffer
                                    let alt_buffer = if active {
                                        trem_preloads_clone.read().get(&key).cloned()
                                    } else {
                                        preloads_clone.read().get(&key).cloned()
                                    };
                                    if let Some(alt_buf) = alt_buffer {
                                        // Fade out current voice slowly
                                        voice.crossfade_release();
                                        // Create new voice from alternate buffer at same position
                                        let mut nv = PlayingVoice::new_crossfade(
                                            alt_buf, voice.stop_id, voice.pipe_num, voice.midi_note,
                                            voice.target_envelope, voice.position, sample_rate
                                        );
                                        // Request background load for new voice
                                        if let Some(path) = nv.needs_background_load() {
                                            nv.mark_load_requested();
                                            let _ = load_tx_clone.try_send((key, path, sample_rate));
                                        }
                                        new_voices.push(nv);
                                    }
                                }
                            }
                            // Add new crossfade voices
                            voices_lock.extend(new_voices);
                        }
                        // Update tremulant state for future notes
                        let mut trem = trem_active_clone.write();
                        for id in stop_ids {
                            if active {
                                trem.insert(id);
                            } else {
                                trem.remove(&id);
                            }
                        }
                        info!("Tremulant {}: active stops = {}", if active { "ON" } else { "OFF" }, trem.len());
                    }
                    AudioCommand::SetDivisionGain { division_index, gain } => {
                        let mut gains = division_gains_clone.write();
                        if (division_index as usize) < gains.len() {
                            gains[division_index as usize] = gain;
                            // Update swell filter cutoff based on gain and config
                            let configs = swell_configs_clone.read();
                            if let Some(&(_min_db, cutoff_closed)) = configs.get(division_index as usize) {
                                let cutoff = cutoff_closed + gain * (20000.0 - cutoff_closed);
                                let mut filters = swell_filters_clone.write();
                                if let Some((fl, fr)) = filters.get_mut(division_index as usize) {
                                    fl.set_cutoff(cutoff, sample_rate);
                                    fr.set_cutoff(cutoff, sample_rate);
                                }
                            }
                        }
                    }
                    AudioCommand::SetSwellConfig { division_index, min_db, filter_cutoff_closed } => {
                        let mut configs = swell_configs_clone.write();
                        if (division_index as usize) < configs.len() {
                            configs[division_index as usize] = (min_db, filter_cutoff_closed);
                            info!("Swell config div {}: min={}dB, filter={}Hz", division_index, min_db, filter_cutoff_closed);
                        }
                    }
                    AudioCommand::SetTremulantLFO { division_index, active, rate, amp_depth, pitch_depth } => {
                        let mut lfos = trem_lfos_clone.write();
                        if let Some(trem) = lfos.get_mut(division_index as usize) {
                            trem.active = active;
                            trem.set_speed(rate, sample_rate);
                            trem.amp_depth = amp_depth;
                            trem.pitch_depth = pitch_depth;
                            info!("Tremulant LFO div {}: active={}, rate={}Hz, amp={}dB, pitch={}ct",
                                  division_index, active, rate, amp_depth, pitch_depth);
                        }
                    }
                    AudioCommand::SetWindModel { division_index, enabled, reservoir_size, damping, max_sag } => {
                        // `division_index` is hier eigenlijk de **groep-index** sinds wind-groepen zijn geïntroduceerd.
                        let mut wm = wind_models_clone.write();
                        if let Some(model) = wm.get_mut(division_index as usize) {
                            model.enabled = enabled;
                            model.configure(reservoir_size, damping, max_sag, sample_rate);
                            info!("Wind model group {}: enabled={}, reservoir={}, damping={}, sag={}",
                                  division_index, enabled, reservoir_size, damping, max_sag);
                        }
                    }
                    AudioCommand::SetDivisionWindGroup { division_index, group } => {
                        let mut wg = wind_groups_clone.write();
                        if (division_index as usize) < wg.len() {
                            wg[division_index as usize] = group.min(31);
                            info!("Division {} → wind group {}", division_index, group);
                        }
                    }
                    AudioCommand::RegisterStopDivisionMap(map) => {
                        *stop_to_division_clone.write() = map;
                        // Reset all division gains to 1.0 (fully open)
                        *division_gains_clone.write() = vec![1.0; 32];
                        // Reset alle tremulant-LFO active-vlaggen: anders blijft een
                        // tremulant die op het vórige orgel aanstond doormoduleren op
                        // dezelfde divisie-index van het nieuwe orgel (stale state).
                        {
                            let mut lfos = trem_lfos_clone.write();
                            for trem in lfos.iter_mut() { trem.active = false; }
                        }
                        trem_active_clone.write().clear();
                        // Reset per-divisie output-routing + C/Cis-spreiding naar default,
                        // zodat divisie-index i van het nieuwe orgel niet de routing/C-Cis
                        // van het vorige orgel erft. De frontend past hierna de opgeslagen
                        // routing van dít orgel opnieuw toe.
                        *output_channels_clone.write() = vec![Vec::new(); 32];
                        *ccis_enabled_clone.write() = vec![false; 32];
                        info!("Registered stop→division map ({} entries); gains, tremulant, routing + C/Cis reset", stop_to_division_clone.read().len());
                    }
                    AudioCommand::SetTemperament { note_offsets, fine_tune } => {
                        let mut ratios = [0.0f64; 12];
                        for i in 0..12 {
                            let total_cents = note_offsets[i] as f64 + fine_tune as f64;
                            ratios[i] = 2.0_f64.powf(total_cents / 1200.0);
                        }
                        *temperament_clone.write() = ratios;
                        info!("Temperament set: fine_tune={} cents", fine_tune);
                    }
                    AudioCommand::LoadImpulseResponse(ir_path) => {
                        match ConvolutionReverb::from_wav(&ir_path, 1024) {
                            Ok(mut rev) => {
                                rev.mix = *reverb_mix_clone.read();
                                *reverb_clone.write() = Some(rev);
                                info!("Loaded impulse response: {:?}", ir_path);
                            }
                            Err(e) => {
                                warn!("Failed to load impulse response {:?}: {}", ir_path, e);
                            }
                        }
                    }
                    AudioCommand::SetAlgorithmicReverb { preset, rt60, pre_delay_ms, damping, room_size, mix } => {
                        let mut fdn = fdn_reverb_clone.write();
                        if let Some(p) = preset {
                            fdn.apply_preset(p, mix);
                            info!("FDN reverb preset {} applied, mix={}", p, mix);
                        } else {
                            fdn.configure(rt60, pre_delay_ms, damping, room_size, mix);
                            info!("FDN reverb configured: rt60={}, pre_delay={}ms, damping={}, room={}, mix={}",
                                  rt60, pre_delay_ms, damping, room_size, mix);
                        }
                    }
                    AudioCommand::SetReverbType { algorithmic } => {
                        *use_algo_reverb_clone.write() = algorithmic;
                        info!("Reverb type: {}", if algorithmic { "algorithmic FDN" } else { "convolution IR" });
                    }
                    AudioCommand::SetPipeVoicing { stop_id, pipe_num, volume_db, pitch_cents } => {
                        let mut v = voicing_clone.write();
                        if volume_db.abs() < 0.01 && pitch_cents.abs() < 0.01 {
                            v.remove(&(stop_id, pipe_num));
                        } else {
                            v.insert((stop_id, pipe_num), (volume_db, pitch_cents));
                        }
                    }
                    AudioCommand::SetDivisionPan { division_index, pan } => {
                        let mut pans = division_pans_clone.write();
                        if (division_index as usize) < pans.len() {
                            pans[division_index as usize] = pan.clamp(-1.0, 1.0);
                            info!("Division {} pan: {:.2}", division_index, pan);
                        }
                    }
                    AudioCommand::SetDivisionOutputChannels { division_index, channels } => {
                        let mut chans = output_channels_clone.write();
                        if (division_index as usize) < chans.len() {
                            info!("Division {} output channels: {:?}", division_index, channels);
                            chans[division_index as usize] = channels;
                        }
                    }
                    AudioCommand::SetCcisSpread { strength, falloff, swap } => {
                        *ccis_params_clone.write() = (strength.clamp(0.0, 1.0), falloff.clamp(0.0, 1.0), swap);
                        info!("C/Cis spread: strength={:.2}, falloff={:.2}, swap={}", strength, falloff, swap);
                    }
                    AudioCommand::SetDivisionCcis { division_index, enabled } => {
                        let mut en = ccis_enabled_clone.write();
                        if (division_index as usize) < en.len() {
                            en[division_index as usize] = enabled;
                            info!("Division {} C/Cis spread: {}", division_index, enabled);
                        }
                    }
                    AudioCommand::SetParametricEq { enabled, low_freq, low_gain, mid_freq, mid_gain, mid_q, high_freq, high_gain } => {
                        let mut eq = eq_clone.write();
                        eq.enabled = enabled;
                        eq.configure(low_freq, low_gain, mid_freq, mid_gain, mid_q, high_freq, high_gain, sample_rate);
                        info!("Parametric EQ: enabled={}, low={:.0}Hz/{:.1}dB, mid={:.0}Hz/{:.1}dB/Q{:.1}, high={:.0}Hz/{:.1}dB",
                              enabled, low_freq, low_gain, mid_freq, mid_gain, mid_q, high_freq, high_gain);
                    }
                    AudioCommand::ClearSamples => {
                        samples_clone.write().clear();
                        preloads_clone.write().clear();
                        trem_preloads_clone.write().clear();
                        trem_active_clone.write().clear();
                        voices_clone.write().clear();
                        stop_to_division_clone.write().clear();
                        *division_gains_clone.write() = vec![1.0; 32];
                        *temperament_clone.write() = [1.0; 12];
                        // Reset ook de per-divisie tremulant-LFO active-vlaggen.
                        {
                            let mut lfos = trem_lfos_clone.write();
                            for trem in lfos.iter_mut() { trem.active = false; }
                        }
                    }
                    AudioCommand::Shutdown => {
                        running_clone.store(false, Ordering::Relaxed);
                    }
                }
            }

            if !running_clone.load(Ordering::Relaxed) {
                data.fill(0.0);
                return;
            }

            // Recorder-tap: bouw alleen een interleaved stereo-buffer als er een
            // actieve opname loopt. Het checken is goedkoop (Arc::clone + atomic bool).
            let rec_active = {
                let r = RECORDER.read();
                r.as_ref().map(|rc| rc.clone()).filter(|rc| rc.is_recording())
            };
            let mut rec_buf: Vec<f32> = if rec_active.is_some() {
                Vec::with_capacity(data.len() / channels.max(1) * 2)
            } else {
                Vec::new()
            };

            // Generate audio
            let gain = *master_gain_clone.read() * *master_expression_clone.read();
            let div_gains = division_gains_clone.read().clone();
            let div_pans = division_pans_clone.read().clone();
            let out_chans_lock = output_channels_clone.read().clone();
            let ccis_en_lock = ccis_enabled_clone.read().clone();
            let (ccis_strength, ccis_falloff, ccis_swap) = *ccis_params_clone.read();
            let wind_group_assignment = wind_groups_clone.read().clone();
            let swell_cfgs = swell_configs_clone.read().clone();
            let stop_div_map = stop_to_division_clone.read();
            let temp_ratios = *temperament_clone.read();
            let mut peak_l = 0.0f32;
            let mut peak_r = 0.0f32;

            {
                let mut voices_lock = voices_clone.write();
                let mut swell_flt = swell_filters_clone.write();
                let pipe_voicing_map = voicing_clone.read();
                let mut wm = wind_models_clone.write();
                let mut trem_lfo = trem_lfos_clone.write();
                let stops_with_trem = stops_with_trem_clone.read();

                // Count voices per division and aggregeer naar wind-groepen.
                // Elke divisie hoort bij één groep (default identity); divisies in dezelfde
                // groep delen één wind-reservoir (gecombineerd voice-count).
                let mut div_voice_counts = [0u32; 32];
                for voice in voices_lock.iter() {
                    if !voice.releasing {
                        let div_idx = stop_div_map.get(&voice.stop_id).copied().unwrap_or(0) as usize;
                        if div_idx < 32 { div_voice_counts[div_idx] += 1; }
                    }
                }
                let mut group_voice_counts = [0u32; 32];
                for div_idx in 0..32 {
                    let grp = wind_group_assignment.get(div_idx).copied().unwrap_or(div_idx as u8) as usize;
                    if grp < 32 {
                        group_voice_counts[grp] += div_voice_counts[div_idx];
                    }
                }
                for (i, model) in wm.iter_mut().enumerate() {
                    model.set_voice_count(group_voice_counts[i]);
                }

                // C/Cis-spreiding is globaal actief zodra de sterkte > 0; per-divisie
                // aan/uit wordt per voice gecheckt.
                let ccis_on = ccis_strength > 1e-4;

                for frame in data.chunks_mut(channels) {
                    // Per-division STEREO mix buckets (L/R): per-noot C/Cis-panning + per-
                    // divisie pan worden hier toegepast zodat losse noten links/rechts
                    // gespreid kunnen worden (C-lade vs Cis-lade).
                    let mut div_l = [0.0f32; 32];
                    let mut div_r = [0.0f32; 32];

                    // Get wind pressure per groep (één per sample)
                    let mut group_pressures = [1.0f32; 32];
                    for (i, model) in wm.iter_mut().enumerate() {
                        group_pressures[i] = model.process();
                    }
                    // Vertaal naar per-divisie pressure via group assignment
                    let mut wind_pressures = [1.0f32; 32];
                    for div_idx in 0..32 {
                        let grp = wind_group_assignment.get(div_idx).copied().unwrap_or(div_idx as u8) as usize;
                        if grp < 32 {
                            wind_pressures[div_idx] = group_pressures[grp];
                        }
                    }

                    // Get tremulant LFO modulation per division (once per sample)
                    let mut trem_mods = [(1.0f32, 0.0f32); 32]; // (amp_mod, pitch_cents)
                    for (i, trem) in trem_lfo.iter_mut().enumerate() {
                        trem_mods[i] = trem.process();
                    }

                    for voice in voices_lock.iter_mut() {
                        let div_idx = stop_div_map.get(&voice.stop_id)
                            .copied()
                            .unwrap_or(0) as usize;

                        // Apply per-note temperament tuning
                        let original_rate = voice.rate;
                        let note_class = (voice.midi_note % 12) as usize;
                        let ratio = temp_ratios[note_class];
                        if (ratio - 1.0_f64).abs() > 1e-10 {
                            voice.rate = original_rate * ratio;
                        }

                        // Apply wind model pitch effect
                        let wind_p = if div_idx < 32 { wind_pressures[div_idx] } else { 1.0 };
                        if (wind_p - 1.0).abs() > 1e-6 {
                            let pitch_cents = (wind_p - 1.0) * 30.0;
                            let wind_ratio = 2.0_f64.powf(pitch_cents as f64 / 1200.0);
                            voice.rate *= wind_ratio;
                        }

                        // Apply tremulant LFO pitch effect (only for stops WITHOUT trem samples)
                        let (trem_amp, trem_pitch) = if div_idx < 32 { trem_mods[div_idx] } else { (1.0, 0.0) };
                        let use_lfo_trem = !stops_with_trem.contains(&voice.stop_id);
                        if use_lfo_trem && trem_pitch.abs() > 0.01 {
                            let trem_ratio = 2.0_f64.powf(trem_pitch as f64 / 1200.0);
                            voice.rate *= trem_ratio;
                        }

                        // Per-pipe voicing (volume + pitch). De pitch-detune moet VÓÓR het lezen
                        // van de sample worden toegepast (zodat ze de huidige sample beïnvloedt)
                        // én valt onder de `voice.rate = original_rate`-reset hieronder. Anders
                        // compoundeert de detune elke sample en loopt de toonhoogte weg.
                        let (voicing_vol, voicing_pitch) = pipe_voicing_map
                            .get(&(voice.stop_id, voice.pipe_num))
                            .copied()
                            .unwrap_or((0.0, 0.0));
                        if voicing_pitch.abs() > 0.01 {
                            let vr = 2.0_f64.powf(voicing_pitch as f64 / 1200.0);
                            voice.rate *= vr;
                        }

                        let sample = voice.next_sample();
                        voice.rate = original_rate;

                        // Apply wind model volume effect
                        let wind_vol = wind_p.sqrt();
                        // Apply tremulant LFO amplitude effect
                        let trem_vol = if use_lfo_trem { trem_amp } else { 1.0 };
                        // Apply per-pipe volume voicing
                        let voicing_gain = 10.0_f32.powf(voicing_vol / 20.0);

                        let contribution = sample * wind_vol * trem_vol * voicing_gain;
                        if div_idx >= 32 { continue; }

                        // C/Cis-lade spreiding: even MIDI-noten (C,D,E,Fis,Gis,Ais) naar de ene
                        // kant, oneven (Cis,Dis,F,G,A,B) naar de andere. Het effect is het sterkst
                        // bij de laagste (grootste) pijpen en loopt naar boven toe steeds meer in
                        // elkaar over (kleine pijpen, dicht bij elkaar in het front).
                        let mut note_pan = 0.0f32;
                        if ccis_on && ccis_en_lock.get(div_idx).copied().unwrap_or(false) {
                            let n = voice.midi_note as f32;
                            let norm = ((n - 36.0) / 60.0).clamp(0.0, 1.0); // 0 bij C groot, 1 bij c''''
                            let pitch_factor = (1.0 - ccis_falloff * norm).clamp(0.0, 1.0);
                            let sep = (ccis_strength * pitch_factor).clamp(0.0, 0.95);
                            let mut side = if voice.midi_note % 2 == 0 { -1.0 } else { 1.0 };
                            if ccis_swap { side = -side; }
                            note_pan = side * sep;
                        }

                        // Combineer met de per-divisie basis-pan → constant-power L/R.
                        let base_pan = div_pans.get(div_idx).copied().unwrap_or(0.0);
                        let total_pan = (base_pan + note_pan).clamp(-1.0, 1.0);
                        let angle = (total_pan + 1.0) * 0.25 * std::f32::consts::PI;
                        div_l[div_idx] += contribution * angle.cos();
                        div_r[div_idx] += contribution * angle.sin();
                    }

                    // Reset frame
                    for s in frame.iter_mut() { *s = 0.0; }

                    // Per divisie: zwelkast (stereo low-pass + gain) + master gain, dan routen
                    // naar de toegewezen fysieke kanalen. Som ook tot mono voor de galm.
                    let mut sum_l = 0.0f32;
                    let mut sum_r = 0.0f32;
                    let mut touched: u64 = 0;
                    for idx in 0..32 {
                        let l_raw = div_l[idx];
                        let r_raw = div_r[idx];
                        if l_raw.abs() < 1e-10 && r_raw.abs() < 1e-10 { continue; }
                        let pedal_pos = div_gains.get(idx).copied().unwrap_or(1.0);
                        let (min_db, _cutoff_closed) = swell_cfgs.get(idx).copied().unwrap_or((-20.0, 800.0));
                        let (lf, rf) = if let Some((fl, fr)) = swell_flt.get_mut(idx) {
                            (fl.process(l_raw), fr.process(r_raw))
                        } else {
                            (l_raw, r_raw)
                        };
                        let db = min_db * (1.0 - pedal_pos);
                        let vol = 10.0_f32.powf(db / 20.0);
                        let l = lf * vol * gain;
                        let r = rf * vol * gain;
                        sum_l += l;
                        sum_r += r;

                        // Route naar kanalen: opeenvolgende kanalen vormen (L,R)-paren; een los
                        // laatste kanaal krijgt mono. Lege lijst = standaard voorste paar (0/1).
                        match out_chans_lock.get(idx) {
                            Some(list) if !list.is_empty() => {
                                let mut i = 0;
                                while i < list.len() {
                                    let lc = list[i] as usize;
                                    if i + 1 < list.len() {
                                        let rc = list[i + 1] as usize;
                                        if lc < channels { frame[lc] += l; touched |= 1u64 << lc.min(63); }
                                        if rc < channels { frame[rc] += r; touched |= 1u64 << rc.min(63); }
                                        i += 2;
                                    } else {
                                        if lc < channels { frame[lc] += (l + r) * 0.5; touched |= 1u64 << lc.min(63); }
                                        i += 1;
                                    }
                                }
                            }
                            _ => {
                                if channels >= 2 {
                                    frame[0] += l; frame[1] += r;
                                    touched |= 0b11;
                                } else if channels >= 1 {
                                    frame[0] += (l + r) * 0.5;
                                    touched |= 0b1;
                                }
                            }
                        }
                    }

                    // Reverb (convolutie IR of algoritmische FDN) + EQ op het mono droog-signaal.
                    let dry_mono = (sum_l + sum_r) * 0.5;
                    let mut processed = dry_mono;
                    if *use_algo_reverb_clone.read() {
                        let mut fdn = fdn_reverb_clone.write();
                        processed = fdn.process(processed);
                    } else if let Some(ref mut rev) = *reverb_clone.write() {
                        let mut out = [0.0f32; 1];
                        rev.process(&[processed], &mut out);
                        processed = out[0];
                    }
                    {
                        let mut eq = eq_clone.write();
                        processed = eq.process(processed);
                    }
                    // Het "diffuse" deel (galm-staart + EQ-kleuring) blijft hoorbaar als de
                    // toetsen losgelaten zijn (dry_mono == 0), zodat de galm netjes uitklinkt.
                    let diffuse = processed - dry_mono;

                    if channels >= 2 {
                        // Diffuse galm naar het voorste paar (0/1).
                        frame[0] += diffuse;
                        frame[1] += diffuse;
                        // Bij 6/8 kanalen: milde center-fill (ch 2) als geen divisie die expliciet
                        // adresseert; LFE (ch 3) blijft leeg tenzij een divisie er naartoe routet.
                        if channels >= 6 && (touched & (1u64 << 2)) == 0 {
                            frame[2] += processed * 0.3;
                        }
                        // Soft clipping per kanaal + peak-meters (voorste paar).
                        for (ci, s) in frame.iter_mut().enumerate() {
                            *s = s.tanh();
                            if ci == 0 { peak_l = peak_l.max(s.abs()); }
                            else if ci == 1 { peak_r = peak_r.max(s.abs()); }
                        }
                    } else {
                        let out = (frame[0] + diffuse).tanh();
                        frame[0] = out;
                        peak_l = peak_l.max(out.abs());
                        peak_r = peak_r.max(out.abs());
                    }
                    // Recorder-tap: neem de VOLLEDIGE stereo-downmix op (alle divisies +
                    // galm), onafhankelijk van de fysieke kanaalrouting. Anders zou een
                    // klavier dat naar surround-kanalen geroutet is niet in de opname
                    // belanden. sum_l/sum_r bevatten alle divisies vóór kanaalroutering.
                    if rec_active.is_some() {
                        let l = (sum_l + diffuse).tanh();
                        let r = (sum_r + diffuse).tanh();
                        rec_buf.push(l);
                        rec_buf.push(r);
                    }
                }
                // Push naar recorder (try_send — audio mag niet blokkeren).
                if let Some(rc) = rec_active {
                    if !rec_buf.is_empty() { rc.push_stereo(&rec_buf); }
                }

                // Remove finished voices
                voices_lock.retain(|v| !v.is_finished());

                // Update voice count
                voice_count_clone.store(voices_lock.len(), Ordering::Relaxed);
            }

            // Update peak meters with decay
            {
                let mut pl = peak_left_clone.write();
                *pl = *pl * 0.95 + peak_l * 0.05;
            }
            {
                let mut pr = peak_right_clone.write();
                *pr = *pr * 0.95 + peak_r * 0.05;
            }
    };

    // Build the output stream in the device's native sample format. WASAPI is
    // typically F32; ASIO drivers (e.g. ASIO4ALL) are often I32/I16. For non-f32
    // formats we render into an f32 scratch buffer and convert.
    // Diagnose: tel callbacks zodat de watchdog verderop kan zien of de driver
    // überhaupt audio komt ophalen — een stream kan "starten" zonder ooit een
    // callback te leveren (bv. ASIO4ALL met bezet/offline apparaat) → stilte.
    let cb_count = Arc::new(AtomicU64::new(0));
    let cb_f32 = cb_count.clone();
    let cb_i32 = cb_count.clone();
    let cb_i16 = cb_count.clone();
    let build = match sample_format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &stream_config,
            move |d: &mut [f32], _: &cpal::OutputCallbackInfo| { cb_f32.fetch_add(1, Ordering::Relaxed); render(d); },
            |err| error!("Audio stream error: {}", err),
            None,
        ),
        cpal::SampleFormat::I32 => {
            let mut scratch: Vec<f32> = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |d: &mut [i32], _: &cpal::OutputCallbackInfo| {
                    cb_i32.fetch_add(1, Ordering::Relaxed);
                    if scratch.len() != d.len() { scratch.resize(d.len(), 0.0); }
                    render(&mut scratch);
                    for (o, s) in d.iter_mut().zip(scratch.iter()) {
                        *o = ((*s).clamp(-1.0, 1.0) * 2_147_483_647.0) as i32;
                    }
                },
                |err| error!("Audio stream error: {}", err),
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let mut scratch: Vec<f32> = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |d: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    cb_i16.fetch_add(1, Ordering::Relaxed);
                    if scratch.len() != d.len() { scratch.resize(d.len(), 0.0); }
                    render(&mut scratch);
                    for (o, s) in d.iter_mut().zip(scratch.iter()) {
                        *o = ((*s).clamp(-1.0, 1.0) * 32_767.0) as i16;
                    }
                },
                |err| error!("Audio stream error: {}", err),
                None,
            )
        }
        other => {
            let msg = format!("Unsupported sample format: {:?}", other);
            let _ = ready_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };

    let stream = match build {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("Failed to build audio stream: {}", e);
            let _ = ready_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };

    if let Err(e) = stream.play() {
        let msg = format!("Failed to start audio: {}", e);
        let _ = ready_tx.send(Err(msg.clone()));
        return Err(msg);
    }
    // Signal successful startup to AudioPlayer::new.
    let _ = ready_tx.send(Ok(()));
    info!("Audio stream started");

    // Keep thread alive while running.
    // Watchdog: meet callbacks op t=3s en t=6s (init-diagnose) en daarna elke 30s
    // een delta-check. Een driver kan een init-burst leveren en daarna stoppen
    // (ASIO4ALL na een koude start), of zelfs pas na minuten uitvallen: de stream
    // lijkt dan actief maar is stil. Alleen een positieve delta bewijst audio.
    let mut ticks = 0u64;
    let mut prev_count = 0u64;
    let mut was_flowing = false;
    while running.load(Ordering::Relaxed) {
        thread::sleep(std::time::Duration::from_millis(100));
        ticks += 1;
        let check = ticks == 30 || ticks == 60 || ticks % 300 == 0;
        if !check { continue; }
        let n = cb_count.load(Ordering::Relaxed);
        let delta = n - prev_count;
        prev_count = n;
        if ticks == 30 {
            if n == 0 {
                warn!("Audio-watchdog: 0 callbacks in 3s op host '{}' ({}) — driver levert geen audio (apparaat bezet/offline?)",
                      host_name, device_name);
            }
            was_flowing = n > 0;
        } else if delta == 0 && was_flowing {
            warn!("Audio-watchdog: callbacks GESTOPT ({} totaal) op host '{}' ({}) — er stroomt geen audio meer (apparaat bezet/offline? start desnoods op WASAPI en wissel daarna naar ASIO)",
                  n, host_name, device_name);
            was_flowing = false;
        } else if delta > 0 && !was_flowing {
            info!("Audio-watchdog: callbacks (weer) op gang ({} in dit venster)", delta);
            was_flowing = true;
        } else if ticks == 60 && delta > 0 {
            info!("Audio-watchdog: audio stroomt stabiel ({} callbacks in 3-6s)", delta);
        }
    }

    info!("Audio thread stopping");
    Ok(())
}
