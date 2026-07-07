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
use vpo_audio::{ConvolutionReverb, OnePoleFilter, WindModel, Tremulant, FdnReverb, ChannelEq, EqBandSpec};

/// Preload buffer size in samples (~2s at 48kHz)
/// Longer buffer = smoother loop while full sample loads in background
pub const PRELOAD_SAMPLES: usize = 96000;

/// Bit-vlag op `pipe_num` die een key markeert als RELEASE-sample van die pijp
/// (opgenomen kerkakoestiek, afgespeeld bij note-off). Release-buffers reizen zo
/// mee door de bestaande preload-/background-load-/upgrade-mechanismen zonder
/// aparte maps; echte pipe_nums blijven er ver onder.
pub const RELEASE_PIPE_FLAG: u32 = 0x8000_0000;

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
    /// Stop-ids die one-shot afspelen (ODF Percussive: klok/glockenspiel —
    /// niet loopen maar één keer uitklinken).
    RegisterPercussiveStops(std::collections::HashSet<u32>),
    /// Note off: (stop_id, pipe_num)
    NoteOff { stop_id: u32, pipe_num: u32 },
    /// Laat ALLE klinkende pijpen van één register los (register weggetrokken
    /// tijdens het spelen). Eén commando i.p.v. een NoteOff per pijp: een
    /// preset-wissel die 20 registers wegtrekt zou anders >1000 berichten
    /// sturen en de bounded(512) command-queue laten blokkeren.
    ReleaseStop { stop_id: u32 },
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
    /// Vrije multi-band EQ (GrandOrgue-stijl): banden met type/freq/gain/band-
    /// breedte en optioneel doelkanaal. Wordt per fysiek uitgangskanaal als
    /// biquad-keten toegepast op de volledige uitgangsmix (droog + galm).
    SetEqBands { enabled: bool, bands: Vec<EqBandSpec> },
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
    /// ODF-intonatie uit de sampleset (GrandOrgue Gain/AmplitudeLevel/PitchTuning-
    /// hiërarchie), per (stop_id, pipe_num) → (volume_db, pitch_cents). Staat LOS
    /// van de gebruikers-voicing (SetPipeVoicing) en stapelt daarmee — zo blijft
    /// de eigen intonatie van de gebruiker gescheiden van wat de set voorschrijft.
    RegisterOdfVoicings(Arc<HashMap<(u32, u32), (f32, f32)>>),
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
    /// One-shot (ODF Percussive): niet loopen, één keer uitklinken.
    one_shot: bool,
    stop_id: u32,
    pipe_num: u32,
    midi_note: u8,
    /// When using preload, track if we've requested full load
    load_requested: bool,
}

impl PlayingVoice {
    fn new_from_sample(sample: SampleRef, stop_id: u32, pipe_num: u32, midi_note: u8, _velocity: f32) -> Self {
        Self {
            source: VoiceSampleSource::Full(sample),
            position: 0.0,
            rate: 1.0,
            envelope: 0.0,
            // Orgelpijpen zijn niet aanslaggevoelig: altijd vol volume,
            // ongeacht MIDI-velocity (zoals GrandOrgue/Hauptwerk).
            target_envelope: 1.0,
            // ~2 ms fade-in @48k: net genoeg om een klik bij retriggeren te
            // voorkomen, kort genoeg om de aanslag niet te vertragen.
            envelope_speed: 0.01,
            releasing: false,
            one_shot: false,
            stop_id,
            pipe_num,
            midi_note,
            load_requested: false,
        }
    }

    fn new_from_preload(preload: Arc<PreloadBuffer>, stop_id: u32, pipe_num: u32, midi_note: u8, _velocity: f32, output_sample_rate: u32) -> Self {
        // Calculate playback rate to correct for sample rate mismatch
        let rate = preload.sample_rate as f64 / output_sample_rate as f64;
        Self {
            source: VoiceSampleSource::PreloadOnly { preload, needs_full_load: true },
            position: 0.0,
            rate,
            envelope: 0.0,
            // Orgelpijpen zijn niet aanslaggevoelig (zie new_from_sample).
            target_envelope: 1.0,
            envelope_speed: 0.01, // ~2 ms fade-in (zie new_from_sample)
            releasing: false,
            one_shot: false,
            stop_id,
            pipe_num,
            midi_note,
            load_requested: false,
        }
    }

    /// Upgrade from preload to full sample (called when background load completes)
    /// The full sample is already resampled to output rate, so reset rate to 1.0
    fn upgrade_to_full(&mut self, sample: SampleRef) {
        // De preload-buffer is onset-getrimd (aanloopstilte weggeknipt); de
        // volledige sample NIET. De positie moet dus eerst terug naar de
        // coördinaten van het originele bestand (+ trim_start), en daarna van
        // bron-samplerate naar output-samplerate (de volledige sample is al
        // geresampled, rate=1.0). Zonder beide correcties verspringt de audio
        // hoorbaar (tik) op het upgrademoment.
        let trim = match &self.source {
            VoiceSampleSource::PreloadOnly { preload, .. } => preload.trim_start as f64,
            _ => 0.0,
        };
        let old_rate = self.rate;
        let mut pos = self.position + trim;
        if old_rate > 0.0 && (old_rate - 1.0).abs() > 1e-9 {
            pos /= old_rate;
        }
        self.position = pos;
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
            one_shot: false,
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

    /// Release met expliciete fade-duur (ms) — GrandOrgue's toonhoogte-
    /// afhankelijke release-crossfade (get_fader_length): lage pijpen sterven
    /// langzaam uit (~184 ms), hoge snel (~6 ms). Eén vaste 42 ms gaf bassen
    /// een te abrupte, en discanten een te trage overgang naar de release.
    fn release_ms(&mut self, ms: f32, sample_rate: u32) {
        self.releasing = true;
        self.target_envelope = 0.0;
        let frames = (ms.max(1.0) * 0.001 * sample_rate as f32).max(8.0);
        self.envelope_speed = 1.0 / frames;
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
        // One-shot (Percussive) leest net als release: lineair uitspelen, geen loop.
        let no_loop = self.releasing || self.one_shot;
        let value = match &self.source {
            VoiceSampleSource::Full(sample) => {
                let len = sample.data.len();
                let (ls, le) = match (loop_start, loop_end) {
                    (Some(a), Some(b)) if b > a && (b as usize) <= len => (a as usize, b as usize),
                    _ => (len / 4, len),
                };
                let xfade = loop_xfade(le.saturating_sub(ls), ls);
                read_voice(&sample.data, ls, le, xfade, &mut self.position, no_loop, &mut self.envelope)
            }
            VoiceSampleSource::PreloadOnly { preload, .. } => {
                // The preload holds only the attack chunk. Gebruik de ECHTE
                // looppunten wanneer die (geheel of gedeeltelijk) binnen de
                // attack-buffer vallen — de vorige ¾-fallback liet de attack-
                // transient periodiek terugkeren tot de full load klaar was.
                let alen = preload.attack_data.len();
                let (ls, le) = match (loop_start, loop_end) {
                    // Volledige loop binnen de preload: exact gebruiken.
                    (Some(a), Some(b)) if b > a && (b as usize) <= alen => (a as usize, b as usize),
                    // Loop begint binnen de preload maar eindigt erbuiten: loop
                    // vanaf het echte startpunt tot het einde van de buffer —
                    // de attack blijft dan tenminste buiten het loopgebied.
                    (Some(a), _) if (a as usize) + 1024 < alen => (a as usize, alen),
                    _ => (alen / 4, alen),
                };
                let xfade = loop_xfade(le.saturating_sub(ls), ls);
                read_voice(&preload.attack_data, ls, le, xfade, &mut self.position, no_loop, &mut self.envelope)
            }
        };

        self.position += self.rate;

        // One-shot: klaar zodra het einde bereikt is — daarna opruimen via retain.
        if self.one_shot && !self.releasing
            && self.position as usize + 1 >= self.get_sample_len()
        {
            debug!("One-shot klaar: stop={} pipe={:#x} pos={:.0} len={}",
                   self.stop_id, self.pipe_num, self.position, self.get_sample_len());
            self.releasing = true;
            self.target_envelope = 0.0;
            self.envelope = 0.0;
        }

        value * self.envelope
    }
}

/// GrandOrgue's automatische release-crossfade-duur (ms) per MIDI-noot
/// (`get_fader_length`): 184 ms onder noot 42, 6 ms boven noot 86, lineair
/// ertussen; 46 ms wanneer de noot onbekend is.
#[inline]
fn release_fade_ms(midi_note: u8) -> f32 {
    let n = midi_note as f32;
    if midi_note == 0 {
        46.0
    } else if n < 42.0 {
        184.0
    } else if n > 86.0 {
        6.0
    } else {
        184.0 - (n - 42.0) * 178.0 / 44.0
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

    // Klem het loop-einde op len-1: bij le == len (preload-/fallback-loops) kon
    // de positie in [len-1, len) belanden vóórdat de wrap-check (>= le) afging;
    // de leesindex i == len-1 gaf dan één sample 0.0 terug — een periodieke tik
    // bij elke loop-omloop. Met le <= len-1 wrapt de positie altijd eerst.
    let le = le.min(len - 1).max(ls + 1);
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
    fn loop_end_at_buffer_end_has_no_zero_sample_tick() {
        // le == len, zoals bij preload-buffers en fallback-loops. De oude code
        // liet de positie in [len-1, len) belanden vóór de wrap-check en gaf
        // dan één sample 0.0 terug → een periodieke tik bij elke loop-omloop.
        let data = sine(6000);
        let (ls, le) = (1000usize, 6000usize); // le == data.len()
        let xfade = super::loop_xfade(le - ls, ls);
        let mut pos = 0.0f64;
        let mut env = 1.0f32;
        let mut prev: Option<f32> = None;
        let mut max_jump = 0.0f32;
        for _ in 0..30_000 {
            let v = read_voice(&data, ls, le, xfade, &mut pos, false, &mut env);
            if let Some(p) = prev {
                max_jump = max_jump.max((v - p).abs());
            }
            prev = Some(v);
            pos += 1.0;
        }
        assert!(max_jump < 0.15, "tik bij loop-einde == bufferlengte: max jump = {}", max_jump);
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

/// Drivernamen van de ASIO-host, ZONDER een driver te laden of te initialiseren
/// (pure registry-scan). Belangrijk: een cpal-enumeratie van de ASIO-host laadt
/// en init't elke driver, en ASIO4ALL claimt bij init de WASAPI-endpoints
/// exclusief — dat doodt een lopende WASAPI-stream en verstoort volgende
/// ASIO-inits. Gebruik daarom déze functie voor apparaatlijsten in de UI.
#[cfg(feature = "asio")]
pub fn asio_driver_names() -> Vec<String> {
    asio_sys::Asio::new().driver_names()
}

#[cfg(not(feature = "asio"))]
pub fn asio_driver_names() -> Vec<String> {
    Vec::new()
}

// ===== Procesbrede ASIO-driver-cache =====
// ASIO4ALL kan binnen één proces maar één keer succesvol initialiseren: na een
// gedraaide sessie faalt élke her-init blijvend ("No audio output device
// found"), ook minuten later — empirisch vastgesteld op deze hardware met
// probes (2026-07-07). Daarom wordt de driver één keer geladen en daarna
// vastgehouden; elke volgende ASIO-wissel bouwt een nieuwe stream op de al
// geladen driver (dat werkt onbeperkt vaak). Prijs: zolang de cache leeft,
// houdt ASIO4ALL de door hem gewrapte WASAPI-endpoints exclusief bezet —
// `asio_release_cached_driver` geeft ze desgewenst weer vrij.
#[cfg(feature = "asio")]
struct AsioCache {
    host: cpal::Host,
    driver_name: String,
    // Houdt de driver via zijn interne Arc in leven; zonder deze keeper wordt
    // de driver bij de laatste stream-drop vernietigd en is ASIO voor de rest
    // van de procesduur onbruikbaar.
    _keeper: cpal::Device,
}

#[cfg(feature = "asio")]
static ASIO_CACHE: parking_lot::Mutex<Option<AsioCache>> = parking_lot::Mutex::new(None);

/// Geef een bruikbaar ASIO-`Device` terug: uit de cache (zonder her-init) of
/// vers geladen (en dan meteen gecachet). `want = None` betekent: de eerste
/// beschikbare driver.
#[cfg(feature = "asio")]
pub fn asio_get_or_load_device(want: Option<&str>) -> Result<cpal::Device, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let mut cache = ASIO_CACHE.lock();

    // Cache-hit: een enumeratie via de gecachte host vindt de al geladen
    // driver terug (weak-upgrade in asio-sys) en initialiseert dus NIET
    // opnieuw. Een verse host zou dat wél doen — en falen.
    if let Some(c) = cache.as_ref() {
        let name_ok = want.map(|w| w == c.driver_name).unwrap_or(true);
        if name_ok {
            let want_name = c.driver_name.clone();
            if let Some(d) = c.host.output_devices().ok().and_then(|mut devs| {
                devs.find(|d| d.name().map(|n| n == want_name).unwrap_or(false))
            }) {
                return Ok(d);
            }
            warn!("Gecachte ASIO-driver '{}' niet meer zichtbaar; cache wordt ververst", c.driver_name);
        } else {
            warn!("Andere ASIO-driver gevraagd ({:?} i.p.v. '{}'); cache wordt ververst — her-init kan falen",
                  want, c.driver_name);
        }
    }
    *cache = None; // oude driver loslaten vóór een nieuwe load

    let host_id = cpal::available_hosts()
        .into_iter()
        .find(|id| id.name().eq_ignore_ascii_case("asio"))
        .ok_or_else(|| "ASIO-host niet beschikbaar in deze build".to_string())?;
    let host = cpal::host_from_id(host_id).map_err(|e| format!("ASIO-host niet te openen: {}", e))?;
    let keeper = match want {
        Some(w) => host
            .output_devices()
            .ok()
            .and_then(|mut devs| devs.find(|d| d.name().map(|n| n == w).unwrap_or(false)))
            .ok_or_else(|| format!(
                "Uitvoerapparaat '{}' niet gevonden op host ASIO (bezet, of de driver kan in dit proces niet opnieuw initialiseren?)", w))?,
        None => host
            .output_devices()
            .ok()
            .and_then(|mut devs| devs.next())
            .ok_or_else(|| "Geen ASIO-apparaat gevonden".to_string())?,
    };
    let driver_name = keeper.name().map_err(|e| format!("ASIO-apparaatnaam onleesbaar: {}", e))?;
    info!("ASIO-driver '{}' geladen en gecachet voor de procesduur", driver_name);
    *cache = Some(AsioCache { host, driver_name: driver_name.clone(), _keeper: keeper });

    // Tweede exemplaar voor de aanroeper via de gecachte host: deelt de al
    // geladen driver, geen nieuwe init.
    let c = cache.as_ref().unwrap();
    c.host
        .output_devices()
        .ok()
        .and_then(|mut devs| devs.find(|d| d.name().map(|n| n == driver_name).unwrap_or(false)))
        .ok_or_else(|| format!("ASIO-apparaat '{}' direct na laden niet meer zichtbaar", driver_name))
}

/// Laat een idle gecachte ASIO-driver los, bv. omdat de gebruiker expliciet
/// een WASAPI-uitgang kiest op het endpoint dat ASIO4ALL bezet houdt.
/// Retourneert true als er iets vrijgegeven is. LET OP: door de
/// her-init-beperking van ASIO4ALL is ASIO daarna tot een app-herstart
/// meestal niet meer bruikbaar — alleen aanroepen als de gevraagde niet-ASIO-
/// uitgang anders niet kan starten.
#[cfg(feature = "asio")]
pub fn asio_release_cached_driver() -> bool {
    let mut cache = ASIO_CACHE.lock();
    if cache.is_some() {
        warn!("Gecachte ASIO-driver wordt vrijgegeven; ASIO is tot een app-herstart mogelijk niet meer beschikbaar");
        *cache = None;
        true
    } else {
        false
    }
}

#[cfg(not(feature = "asio"))]
pub fn asio_release_cached_driver() -> bool {
    false
}

/// Feature-onafhankelijke wrapper: zonder asio-feature een nette fout.
#[cfg(feature = "asio")]
fn asio_get_or_load_device_checked(want: Option<&str>) -> Result<cpal::Device, String> {
    asio_get_or_load_device(want)
}

#[cfg(not(feature = "asio"))]
fn asio_get_or_load_device_checked(_want: Option<&str>) -> Result<cpal::Device, String> {
    Err("ASIO niet beschikbaar in deze build (compileer met --features asio)".to_string())
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
    /// Actual number of output channels of the open stream
    pub current_channels: Arc<RwLock<u16>>,
    /// Noodrem: de watchdog kon de stream herhaaldelijk niet herbouwen (bv.
    /// apparaat verdwenen en de nieuwe default heeft een ander formaat). De
    /// herstel-thread (main.rs) ziet deze vlag en bouwt de HELE player opnieuw.
    pub restart_needed: Arc<AtomicBool>,
    /// True zodra de audio-thread volledig is afgerond (stream/driver vrij).
    thread_done: Arc<AtomicBool>,
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
        let current_channels = Arc::new(RwLock::new(2u16));
        let restart_needed = Arc::new(AtomicBool::new(false));
        // Teller van geleverde audio-callbacks. Een stream kan "starten" zonder
        // ooit een callback te leveren (ASIO4ALL koud/bezet) — pas als deze
        // teller loopt is er echt geluid mogelijk.
        let callbacks_seen = Arc::new(AtomicU64::new(0));
        // Wordt true zodra de audio-thread écht klaar is (stream + driver
        // vrijgegeven). shutdown_and_wait pollt hierop: een ASIO-init kan pas
        // slagen als de vorige stream het apparaat aantoonbaar heeft losgelaten.
        let thread_done = Arc::new(AtomicBool::new(false));

        let voice_count_clone = voice_count.clone();
        let peak_left_clone = peak_left.clone();
        let peak_right_clone = peak_right.clone();
        let running_clone = running.clone();

        // Spawn audio thread
        let sr_clone = sample_rate_shared.clone();
        let host_clone = current_host.clone();
        let device_clone = current_device.clone();
        let buffer_clone = current_buffer_frames.clone();
        let channels_clone = current_channels.clone();
        let restart_clone = restart_needed.clone();
        let callbacks_clone = callbacks_seen.clone();
        // Readiness channel: the audio thread reports whether the output stream
        // actually built+started, so we can fail cleanly (e.g. ASIO unavailable)
        // instead of silently ending up with no audio.
        let (ready_tx, ready_rx) = bounded::<Result<(), String>>(1);
        let done_clone = thread_done.clone();
        thread::spawn(move || {
            if let Err(e) = run_audio_thread(
                command_rx, voice_count_clone, peak_left_clone, peak_right_clone,
                running_clone, sr_clone, cfg, host_clone, device_clone, buffer_clone,
                channels_clone, restart_clone, callbacks_clone, ready_tx,
            ) {
                error!("Audio thread error: {}", e);
            }
            done_clone.store(true, Ordering::Relaxed);
        });

        // Wait for the stream to build/start (or fail) before returning.
        match ready_rx.recv_timeout(std::time::Duration::from_secs(8)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                // Zorg dat de gespawnde thread stopt — anders blijft een
                // "zombie" doorleven die later alsnog apparaten claimt.
                running.store(false, Ordering::Relaxed);
                return Err(e);
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                // Thread eindigde zonder ready-melding (zou met de expliciete
                // sends niet meer moeten gebeuren, maar wees eerlijk over de
                // oorzaak i.p.v. een misleidende 8s-timeout te rapporteren).
                running.store(false, Ordering::Relaxed);
                return Err("Audio-thread stopte tijdens het opstarten (apparaat/driver-fout)".to_string());
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                running.store(false, Ordering::Relaxed);
                return Err("Audio stream did not start within 8s".to_string());
            }
        }

        // Een stream die "start" is nog geen geluid: ASIO4ALL kan succesvol
        // bouwen+starten en vervolgens NOOIT een callback leveren (koud/bezet
        // apparaat — gedocumenteerd op deze hardware). Pas als de eerste
        // callback binnen is, is de wissel echt geslaagd; anders melden we een
        // fout zodat switch_audio_output op de oude uitgang kan blijven of
        // terug kan vallen. Normaal komt de eerste callback binnen ~10-50 ms.
        let verify_deadline = std::time::Instant::now() + std::time::Duration::from_millis(2500);
        loop {
            if callbacks_seen.load(Ordering::Relaxed) > 0 {
                break;
            }
            if std::time::Instant::now() >= verify_deadline {
                running.store(false, Ordering::Relaxed);
                return Err("Audiostream gestart maar levert geen audio-callbacks (apparaat bezet/offline?)".to_string());
            }
            thread::sleep(std::time::Duration::from_millis(10));
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
            current_channels,
            restart_needed,
            thread_done,
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
        // try_send, niet send: bij een volle command-queue (dode stream terwijl
        // er doorgespeeld werd) zou een blokkerende send de Drop — en daarmee de
        // hele audio-wissel onder de load_switch_gate — voorgoed laten hangen.
        // running=false volstaat: de watchdog-lus checkt dat elke 100 ms en de
        // render-callback bij elke buffer.
        let _ = self.command_tx.try_send(AudioCommand::Shutdown);
    }

    /// Stop en wacht (begrensd) tot de audio-thread écht klaar is. Nodig vóór
    /// een wissel van/naar ASIO: zolang de oude stream of driver nog leeft,
    /// houdt ASIO4ALL/WASAPI het apparaat vast en faalt de init van de nieuwe
    /// uitgang ("No audio output device found" / apparaat bezet).
    pub fn shutdown_and_wait(&self, timeout: std::time::Duration) {
        self.stop();
        let deadline = std::time::Instant::now() + timeout;
        while !self.thread_done.load(Ordering::Relaxed) {
            if std::time::Instant::now() >= deadline {
                warn!("Audio-thread niet gestopt binnen {:?}; wissel gaat toch verder", timeout);
                return;
            }
            thread::sleep(std::time::Duration::from_millis(20));
        }
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
    channels_out: Arc<RwLock<u16>>,
    restart_needed: Arc<AtomicBool>,
    callbacks_seen: Arc<AtomicU64>,
    ready_tx: crossbeam_channel::Sender<Result<(), String>>,
) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    // Resolve output device. STRIKT wanneer er een apparaatnaam gevraagd is:
    // stilletjes terugvallen op het default-apparaat betekent "wissel geslaagd"
    // terwijl het geluid ergens anders speelt (hoofdtelefoon↔speakers-bug).
    // Fouten gaan via ready_tx zodat AudioPlayer::new de échte oorzaak meldt
    // in plaats van een misleidende timeout (kanaal-disconnect ≠ 8s-timeout).
    // ASIO gaat via de procesbrede driver-cache: ASIO4ALL kan maar één keer
    // per proces initialiseren, dus de geladen driver wordt hergebruikt.
    let host_is_asio = cfg.host_name.as_deref()
        .map(|h| h.eq_ignore_ascii_case("asio"))
        .unwrap_or(false);
    let (device, host_display_name) = if host_is_asio {
        match asio_get_or_load_device_checked(cfg.device_name.as_deref()) {
            Ok(d) => (d, "ASIO".to_string()),
            Err(msg) => {
                let _ = ready_tx.send(Err(msg.clone()));
                return Err(msg);
            }
        }
    } else {
        let host = resolve_host(cfg.host_name.as_deref());
        let dev = match cfg.device_name.as_deref() {
            Some(want) => {
                let found = host.output_devices().ok().and_then(|mut devs| {
                    devs.find(|d| d.name().map(|n| n == want).unwrap_or(false))
                });
                match found {
                    Some(d) => d,
                    None => {
                        let msg = format!(
                            "Uitvoerapparaat '{}' niet gevonden op host {} (bezet of losgekoppeld?)",
                            want, host.id().name()
                        );
                        let _ = ready_tx.send(Err(msg.clone()));
                        return Err(msg);
                    }
                }
            }
            None => match host.default_output_device() {
                Some(d) => d,
                None => {
                    let msg = "No audio output device found".to_string();
                    let _ = ready_tx.send(Err(msg.clone()));
                    return Err(msg);
                }
            },
        };
        let name = host.id().name().to_string();
        (dev, name)
    };

    let supported = match device.default_output_config() {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("Failed to get audio config: {}", e);
            let _ = ready_tx.send(Err(msg.clone()));
            return Err(msg);
        }
    };

    // AudioPlayer::new kan intussen opgegeven hebben (8s-timeout of geen
    // callbacks): stop dan hier, anders blijft een zombie-thread het apparaat
    // claimen en er later mee vechten.
    if !running.load(Ordering::Relaxed) {
        return Ok(());
    }

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

    let host_name = host_display_name;
    let device_name = device.name().unwrap_or_default();
    *sample_rate_out.write() = sample_rate;
    *host_out.write() = host_name.clone();
    *device_out.write() = device_name.clone();
    *buffer_out.write() = used_buffer_frames;
    *channels_out.write() = stream_config.channels;
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
    // ODF-intonatie van de sampleset: (stop_id, pipe_num) → (volume_db, pitch_cents).
    // Stapelt met de gebruikers-voicing (pipe_voicing) in de render-lus.
    let odf_voicing: Arc<RwLock<HashMap<(u32, u32), (f32, f32)>>> = Arc::new(RwLock::new(HashMap::new()));
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
    // Vrije multi-band EQ: per fysiek uitgangskanaal een biquad-keten, gebouwd
    // uit de actuele bandenlijst (kanaal None = alle kanalen).
    let eq_enabled: Arc<RwLock<bool>> = Arc::new(RwLock::new(false));
    let eq_channels: Arc<RwLock<Vec<ChannelEq>>> = Arc::new(RwLock::new(Vec::new()));
    // Stop-ids die one-shot afspelen (ODF Percussive: klok e.d.)
    let percussive_stops: Arc<RwLock<std::collections::HashSet<u32>>> = Arc::new(RwLock::new(Default::default()));
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
    let percussive_stops_clone = percussive_stops.clone();
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
    let eq_enabled_clone = eq_enabled.clone();
    let eq_channels_clone = eq_channels.clone();
    let voicing_clone = pipe_voicing.clone();
    let odf_voicing_clone = odf_voicing.clone();
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

                        // One-shot (ODF Percussive)? Dan niet loopen maar uitklinken.
                        let one_shot = percussive_stops_clone.read().contains(&stop_id);

                        if let Some(sample) = sample_opt {
                            // Full sample available - instant playback
                            let mut voice = PlayingVoice::new_from_sample(sample.clone(), stop_id, pipe_num, midi_note, velocity);
                            voice.one_shot = one_shot;
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
                                voice.one_shot = one_shot;

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
                        // Echte release-samples (opgenomen kerkakoestiek): staan
                        // onder de gemarkeerde key. Gevonden → start een one-shot
                        // release-voice op het huidige niveau van de stervende
                        // voice; de sustain fade't toonhoogte-afhankelijk uit
                        // (GrandOrgue get_fader_length: bas ~184 ms, discant ~6 ms)
                        // wat als crossfade werkt. Niet gevonden → fade alleen.
                        let release_key = (stop_id, pipe_num | RELEASE_PIPE_FLAG);
                        let rel_full = samples_clone.read().get(&release_key).cloned();
                        // Preload óók bij een geladen full sample ophalen: die
                        // draagt trim_start = segment-start (release-uitsnede in
                        // hetzelfde bestand) — de full sample begint op frame 0.
                        let rel_pre = preloads_clone.read().get(&release_key).cloned();

                        let mut spawn: Option<PlayingVoice> = None;
                        let mut voices_lock = voices_clone.write();
                        for voice in voices_lock.iter_mut() {
                            // One-shot-voices (Percussive: klok) klinken uit en
                            // negeren note-off, zoals GrandOrgue dat doet.
                            if voice.stop_id == stop_id && voice.pipe_num == pipe_num
                                && !voice.releasing && !voice.one_shot
                            {
                                if spawn.is_none() && (rel_full.is_some() || rel_pre.is_some()) {
                                    // Niveau-overname: de release start op het
                                    // huidige envelope-niveau van de sustain.
                                    let level = voice.envelope.clamp(0.05, 1.0);
                                    let mut rv = if let Some(sample) = rel_full.clone() {
                                        let mut v = PlayingVoice::new_from_sample(sample, stop_id, release_key.1, voice.midi_note, 1.0);
                                        // Same-file release: de volledige sample is
                                        // het HELE bestand — spring naar het
                                        // release-segment (cue-marker), in
                                        // output-rate frames.
                                        if let Some(ref pre) = rel_pre {
                                            if pre.trim_start > 0 && pre.sample_rate > 0 {
                                                v.position = pre.trim_start as f64
                                                    * sample_rate as f64 / pre.sample_rate as f64;
                                            }
                                        }
                                        v
                                    } else {
                                        let pre = rel_pre.clone().unwrap();
                                        let mut v = PlayingVoice::new_from_preload(
                                            pre, stop_id, release_key.1, voice.midi_note, 1.0, sample_rate,
                                        );
                                        // Stream de volledige release-staart bij.
                                        if let Some(path) = v.needs_background_load() {
                                            v.mark_load_requested();
                                            let _ = load_tx_clone.try_send((release_key, path, sample_rate));
                                        }
                                        v
                                    };
                                    rv.one_shot = true;
                                    rv.envelope = level;
                                    rv.target_envelope = level;
                                    debug!("Release-voice gestart: stop={} pipe={} niveau={:.2} bron={} len={} rate={:.3} pos={:.0}",
                                           stop_id, pipe_num, level,
                                           if rel_full.is_some() { "full" } else { "preload" },
                                           rv.get_sample_len(), rv.rate, rv.position);
                                    spawn = Some(rv);
                                }
                                voice.release_ms(release_fade_ms(voice.midi_note), sample_rate);
                            }
                        }
                        if let Some(rv) = spawn {
                            voices_lock.push(rv);
                        }
                    }
                    AudioCommand::ReleaseStop { stop_id } => {
                        // Alle klinkende pijpen van dit register loslaten, met
                        // dezelfde release-sample-logica als NoteOff (opgenomen
                        // kerkakoestiek per pijp, maximaal één per pijp).
                        let mut voices_lock = voices_clone.write();
                        let mut spawned: std::collections::HashSet<u32> = Default::default();
                        let mut spawns: Vec<PlayingVoice> = Vec::new();
                        for voice in voices_lock.iter_mut() {
                            if voice.stop_id != stop_id || voice.releasing || voice.one_shot {
                                continue;
                            }
                            let release_key = (stop_id, voice.pipe_num | RELEASE_PIPE_FLAG);
                            if !spawned.contains(&voice.pipe_num) {
                                let rel_full = samples_clone.read().get(&release_key).cloned();
                                let rel_pre = preloads_clone.read().get(&release_key).cloned();
                                if rel_full.is_some() || rel_pre.is_some() {
                                    let level = voice.envelope.clamp(0.05, 1.0);
                                    let mut rv = if let Some(sample) = rel_full {
                                        let mut v = PlayingVoice::new_from_sample(sample, stop_id, release_key.1, voice.midi_note, 1.0);
                                        // Same-file release: spring naar het segment
                                        // (zie NoteOff voor de uitleg).
                                        if let Some(ref pre) = rel_pre {
                                            if pre.trim_start > 0 && pre.sample_rate > 0 {
                                                v.position = pre.trim_start as f64
                                                    * sample_rate as f64 / pre.sample_rate as f64;
                                            }
                                        }
                                        v
                                    } else {
                                        let pre = rel_pre.unwrap();
                                        let mut v = PlayingVoice::new_from_preload(
                                            pre, stop_id, release_key.1, voice.midi_note, 1.0, sample_rate,
                                        );
                                        if let Some(path) = v.needs_background_load() {
                                            v.mark_load_requested();
                                            let _ = load_tx_clone.try_send((release_key, path, sample_rate));
                                        }
                                        v
                                    };
                                    rv.one_shot = true;
                                    rv.envelope = level;
                                    rv.target_envelope = level;
                                    spawns.push(rv);
                                    spawned.insert(voice.pipe_num);
                                }
                            }
                            voice.release_ms(release_fade_ms(voice.midi_note), sample_rate);
                        }
                        voices_lock.extend(spawns);
                    }
                    AudioCommand::RegisterPercussiveStops(set) => {
                        if !set.is_empty() {
                            info!("Percussieve (one-shot) stops geregistreerd: {}", set.len());
                        }
                        *percussive_stops_clone.write() = set;
                    }
                    AudioCommand::RegisterOdfVoicings(map) => {
                        if !map.is_empty() {
                            info!("ODF-intonatie geregistreerd voor {} pijpen", map.len());
                        }
                        *odf_voicing_clone.write() = (*map).clone();
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
                        // Mix centraal bijhouden: de render-lus vermenigvuldigt het
                        // wet-signaal met reverb_mix, voor FDN én convolutie gelijk.
                        *reverb_mix_clone.write() = mix.clamp(0.0, 1.0);
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
                    AudioCommand::SetEqBands { enabled, bands } => {
                        *eq_enabled_clone.write() = enabled;
                        // Bouw per fysiek uitgangskanaal de biquad-keten opnieuw.
                        let chains: Vec<ChannelEq> = (0..channels)
                            .map(|ci| ChannelEq::build(&bands, ci as u8, sample_rate))
                            .collect();
                        *eq_channels_clone.write() = chains;
                        info!("EQ: enabled={}, {} banden over {} kanalen", enabled, bands.len(), channels);
                    }
                    AudioCommand::ClearSamples => {
                        samples_clone.write().clear();
                        preloads_clone.write().clear();
                        trem_preloads_clone.write().clear();
                        trem_active_clone.write().clear();
                        percussive_stops_clone.write().clear();
                        odf_voicing_clone.write().clear();
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
                let odf_voicing_map = odf_voicing_clone.read();
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

                // Galm + EQ: locks één keer per callback nemen (niet per sample).
                let use_algo = *use_algo_reverb_clone.read();
                let reverb_mix_now = *reverb_mix_clone.read();
                let mut fdn_lock = fdn_reverb_clone.write();
                let mut conv_lock = reverb_clone.write();
                let eq_on = *eq_enabled_clone.read();
                let mut eq_chains = eq_channels_clone.write();

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
                        // ODF-intonatie (uit de sampleset) en gebruikers-voicing stapelen:
                        // dB's en cents worden opgeteld. Release-voices (gemarkeerde
                        // pipe_num) erven de intonatie van hun pijp.
                        let vkey = (voice.stop_id, voice.pipe_num & !RELEASE_PIPE_FLAG);
                        let (user_vol, user_pitch) = pipe_voicing_map
                            .get(&vkey)
                            .copied()
                            .unwrap_or((0.0, 0.0));
                        let (odf_vol, odf_pitch) = odf_voicing_map
                            .get(&vkey)
                            .copied()
                            .unwrap_or((0.0, 0.0));
                        let (voicing_vol, voicing_pitch) = (user_vol + odf_vol, user_pitch + odf_pitch);
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

                    // Galm: stereo, puur WET, additief op het droge signaal (geen
                    // dry/wet-aftrek meer — die dempte het droge signaal en maakte
                    // de galm nauwelijks hoorbaar). FDN levert gedecorreleerd L/R.
                    // Convolutie (mono-IR) alleen wanneer er echt een IR geladen is;
                    // anders valt de engine terug op de FDN zodat er ALTIJD galm
                    // beschikbaar is zodra de mix > 0 staat.
                    let (mut wet_l, mut wet_r) = (0.0f32, 0.0f32);
                    if use_algo || conv_lock.is_none() {
                        let (l, r) = fdn_lock.process_stereo(sum_l, sum_r);
                        wet_l = l * reverb_mix_now;
                        wet_r = r * reverb_mix_now;
                    } else if let Some(ref mut rev) = *conv_lock {
                        let w = rev.process_wet_sample((sum_l + sum_r) * 0.5) * reverb_mix_now;
                        wet_l = w;
                        wet_r = w;
                    }

                    if channels >= 2 {
                        // Galm naar het voorste paar (0/1).
                        frame[0] += wet_l;
                        frame[1] += wet_r;
                        // Bij 6/8 kanalen: milde galm-center-fill (ch 2) als geen divisie
                        // die expliciet adresseert; LFE (ch 3) blijft leeg tenzij een
                        // divisie er naartoe routet.
                        if channels >= 6 && (touched & (1u64 << 2)) == 0 {
                            frame[2] += (wet_l + wet_r) * 0.5 * 0.6;
                        }
                    } else {
                        frame[0] += (wet_l + wet_r) * 0.5;
                    }

                    // Vrije multi-band EQ: per fysiek uitgangskanaal een eigen keten,
                    // toegepast op de volledige mix (droog + galm) van dat kanaal.
                    if eq_on {
                        for (ci, s) in frame.iter_mut().enumerate() {
                            if let Some(chain) = eq_chains.get_mut(ci) {
                                if !chain.is_empty() {
                                    *s = chain.process(*s);
                                }
                            }
                        }
                    }

                    // Soft clipping per kanaal + peak-meters (voorste paar).
                    for (ci, s) in frame.iter_mut().enumerate() {
                        *s = s.tanh();
                        if ci == 0 { peak_l = peak_l.max(s.abs()); }
                        else if ci == 1 { peak_r = peak_r.max(s.abs()); }
                    }
                    // Recorder-tap: neem de VOLLEDIGE stereo-downmix op (alle divisies +
                    // galm), onafhankelijk van de fysieke kanaalrouting. Anders zou een
                    // klavier dat naar surround-kanalen geroutet is niet in de opname
                    // belanden. sum_l/sum_r bevatten alle divisies vóór kanaalroutering.
                    if rec_active.is_some() {
                        let l = (sum_l + wet_l).tanh();
                        let r = (sum_r + wet_r).tanh();
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
    //
    // De render-closure staat achter een Arc<Mutex<..>> zodat de watchdog de
    // stream kan HERBOUWEN met dezelfde render-staat: een WASAPI-stream sterft
    // wanneer Windows het default-apparaat wisselt of een apparaat verdwijnt
    // (bv. bluetooth-koptelefoon verbindt) — zonder herstel blijft de app dan
    // geluidloos tot een herstart.
    // Diagnose: tel callbacks zodat de watchdog kan zien of de driver
    // überhaupt audio komt ophalen — een stream kan "starten" zonder ooit een
    // callback te leveren (bv. ASIO4ALL met bezet/offline apparaat) → stilte.
    // De teller is gedeeld met AudioPlayer::new, die de eerste callback
    // afwacht voordat de wissel als geslaagd geldt.
    let cb_count = callbacks_seen;
    let stream_error = Arc::new(AtomicBool::new(false));
    let render_shared: Arc<parking_lot::Mutex<dyn FnMut(&mut [f32]) + Send>> =
        Arc::new(parking_lot::Mutex::new(render));

    // Herbruikbare stream-bouwer: bouwt (opnieuw) een output-stream op `device`
    // rond de gedeelde render-closure.
    let build_stream = |device: &cpal::Device,
                        stream_config: &cpal::StreamConfig,
                        sample_format: cpal::SampleFormat,
                        render_shared: &Arc<parking_lot::Mutex<dyn FnMut(&mut [f32]) + Send>>,
                        cb_count: &Arc<AtomicU64>,
                        stream_error: &Arc<AtomicBool>|
     -> Result<cpal::Stream, String> {
        let err_flag = stream_error.clone();
        let err_cb = move |err| {
            error!("Audio stream error: {}", err);
            err_flag.store(true, Ordering::Relaxed);
        };
        let build = match sample_format {
            cpal::SampleFormat::F32 => {
                let r = render_shared.clone();
                let c = cb_count.clone();
                device.build_output_stream(
                    stream_config,
                    move |d: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        c.fetch_add(1, Ordering::Relaxed);
                        (r.lock())(d);
                    },
                    err_cb,
                    None,
                )
            }
            cpal::SampleFormat::I32 => {
                let r = render_shared.clone();
                let c = cb_count.clone();
                let mut scratch: Vec<f32> = Vec::new();
                device.build_output_stream(
                    stream_config,
                    move |d: &mut [i32], _: &cpal::OutputCallbackInfo| {
                        c.fetch_add(1, Ordering::Relaxed);
                        if scratch.len() != d.len() { scratch.resize(d.len(), 0.0); }
                        (r.lock())(&mut scratch);
                        for (o, s) in d.iter_mut().zip(scratch.iter()) {
                            *o = ((*s).clamp(-1.0, 1.0) * 2_147_483_647.0) as i32;
                        }
                    },
                    err_cb,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let r = render_shared.clone();
                let c = cb_count.clone();
                let mut scratch: Vec<f32> = Vec::new();
                device.build_output_stream(
                    stream_config,
                    move |d: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        c.fetch_add(1, Ordering::Relaxed);
                        if scratch.len() != d.len() { scratch.resize(d.len(), 0.0); }
                        (r.lock())(&mut scratch);
                        for (o, s) in d.iter_mut().zip(scratch.iter()) {
                            *o = ((*s).clamp(-1.0, 1.0) * 32_767.0) as i16;
                        }
                    },
                    err_cb,
                    None,
                )
            }
            other => return Err(format!("Unsupported sample format: {:?}", other)),
        };
        build.map_err(|e| format!("Failed to build audio stream: {}", e))
    };

    let mut stream = match build_stream(&device, &stream_config, sample_format, &render_shared, &cb_count, &stream_error) {
        Ok(s) => s,
        Err(msg) => {
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
    // Bij een stream-fout (apparaat verdwenen/gewisseld) of stilgevallen
    // callbacks probeert de watchdog de stream elke ~2s opnieuw op te bouwen op
    // het (dan geldende) standaardapparaat.
    let mut ticks = 0u64;
    let mut prev_count = 0u64;
    let mut was_flowing = false;
    let mut rebuild_pending = false;
    let mut rebuild_failures = 0u32;
    // Verificatievenster na een herbouw: (start-tick, callback-baseline).
    // Een herbouwde stream telt pas als hersteld wanneer er écht callbacks
    // komen — ASIO4ALL kan bouwen+starten en toch stil blijven.
    let mut verify_since: Option<(u64, u64)> = None;
    while running.load(Ordering::Relaxed) {
        thread::sleep(std::time::Duration::from_millis(100));
        ticks += 1;

        if stream_error.swap(false, Ordering::Relaxed) {
            rebuild_pending = true;
        }

        // Loopt er een verificatievenster? Callbacks gezien → herstel bevestigd;
        // na ~2,5 s zonder callbacks → als mislukking tellen en opnieuw proberen.
        if let Some((t0, baseline)) = verify_since {
            if cb_count.load(Ordering::Relaxed) > baseline {
                verify_since = None;
                rebuild_failures = 0;
                info!("Audio-watchdog: herstel bevestigd — callbacks stromen weer");
            } else if ticks.saturating_sub(t0) >= 25 {
                verify_since = None;
                rebuild_pending = true;
                rebuild_failures += 1;
                warn!("Audio-watchdog: herbouwde stream blijft stil ({} mislukte pogingen)", rebuild_failures);
                if rebuild_failures >= 5 && rebuild_failures % 5 == 0 && rebuild_failures <= 15 {
                    warn!("Audio-watchdog: volledige audio-herstart aangevraagd");
                    restart_needed.store(true, Ordering::Relaxed);
                }
            }
        }

        // Herbouwpoging (elke ~2s zolang nodig): zelfde host, apparaat opnieuw
        // resolven (het oorspronkelijke apparaat kan weg zijn → default).
        if rebuild_pending && ticks % 20 == 0 {
            warn!("Audio-watchdog: stream herbouwen (apparaat weggevallen of callbacks gestopt)...");
            let mut attempt_ok = false;
            // ASIO: altijd via de driver-cache — een verse host-enumeratie zou
            // de al geladen driver niet vinden (her-init faalt) en de rebuild
            // structureel laten mislukken.
            let new_device = if host_is_asio {
                asio_get_or_load_device_checked(cfg.device_name.as_deref()).ok()
            } else {
                let host = resolve_host(cfg.host_name.as_deref());
                cfg.device_name.as_deref()
                    .and_then(|want| {
                        host.output_devices().ok().and_then(|mut devs| {
                            devs.find(|d| d.name().map(|n| n == want).unwrap_or(false))
                        })
                    })
                    .or_else(|| host.default_output_device())
            };
            if let Some(new_device) = new_device {
                match new_device.default_output_config() {
                    Ok(new_supported) => {
                        // De render-closure is gebouwd op het oorspronkelijke
                        // kanaalaantal en de oorspronkelijke samplerate (samples
                        // zijn daarop geresampled). Forceer die config op het
                        // nieuwe apparaat — lukt dat niet, dan proberen we het
                        // over 2s opnieuw (garbled interleave of een systematische
                        // pitch-shift is erger dan even geen geluid).
                        let mut new_config: cpal::StreamConfig = new_supported.config();
                        new_config.channels = stream_config.channels;
                        new_config.sample_rate = cpal::SampleRate(sample_rate);
                        new_config.buffer_size = stream_config.buffer_size;
                        let dev_name = new_device.name().unwrap_or_default();
                        match build_stream(&new_device, &new_config, new_supported.sample_format(), &render_shared, &cb_count, &stream_error) {
                            Ok(new_stream) => match new_stream.play() {
                                Ok(()) => {
                                    stream = new_stream;
                                    *device_out.write() = dev_name.clone();
                                    rebuild_pending = false;
                                    was_flowing = false;
                                    attempt_ok = true;
                                    prev_count = cb_count.load(Ordering::Relaxed);
                                    // Nog niet als geslaagd tellen: eerst
                                    // verifiëren dat er callbacks komen.
                                    verify_since = Some((ticks, prev_count));
                                    info!("Audio-watchdog: stream herbouwd op '{}' — wachten op callbacks", dev_name);
                                }
                                Err(e) => warn!("Audio-watchdog: herstelde stream start niet: {}", e),
                            },
                            Err(e) => warn!("Audio-watchdog: herbouwen mislukt ({}); nieuwe poging over 2s", e),
                        }
                    }
                    Err(e) => warn!("Audio-watchdog: geen apparaat-config: {}", e),
                }
            } else {
                warn!("Audio-watchdog: geen uitvoerapparaat beschikbaar; nieuwe poging over 2s");
            }

            // Noodrem: in-thread herbouwen kan structureel falen wanneer het
            // vervangende apparaat een ander mix-formaat (samplerate/kanalen)
            // heeft dan waarop deze render-thread gebouwd is. Elke 5 mislukte
            // pogingen (~10 s) vragen we een VOLLEDIGE audio-herstart aan
            // (herhaalbaar, met plafond tegen eindeloos flapperen); de
            // herstel-thread (main.rs) bouwt dan een nieuwe player op de dan
            // geldende apparaat-config en herlaadt het orgel.
            // NB: bij een geslaagde build wordt de teller pas gereset zodra het
            // verificatievenster hierboven echte callbacks heeft gezien.
            if !attempt_ok {
                rebuild_failures += 1;
                if rebuild_failures >= 5 && rebuild_failures % 5 == 0 && rebuild_failures <= 15 {
                    warn!("Audio-watchdog: {} herbouwpogingen mislukt — volledige audio-herstart aangevraagd", rebuild_failures);
                    restart_needed.store(true, Ordering::Relaxed);
                }
            }
        }

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
            warn!("Audio-watchdog: callbacks GESTOPT ({} totaal) op host '{}' ({}) — herbouwpoging volgt",
                  n, host_name, device_name);
            was_flowing = false;
            rebuild_pending = true;
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
