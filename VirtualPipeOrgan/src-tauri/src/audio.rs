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
use tracing::{info, error, warn};

use vpo_sampler::{LoadedOrgan, SampleRef, SampleData, load_audio, resample, PreloadBuffer};
use vpo_audio::{ConvolutionReverb, OnePoleFilter, WindModel, Tremulant, FdnReverb, ChannelEq, EqBandSpec, MasterLimiter};

/// Preload buffer size in samples (~2s at 48kHz)
/// Longer buffer = smoother loop while full sample loads in background
pub const PRELOAD_SAMPLES: usize = 96000;

/// Bit-vlag op `pipe_num` die een key markeert als RELEASE-sample van die pijp
/// (opgenomen kerkakoestiek, afgespeeld bij note-off). Release-buffers reizen zo
/// mee door de bestaande preload-/background-load-/upgrade-mechanismen zonder
/// aparte maps; echte pipe_nums blijven er ver onder.
pub const RELEASE_PIPE_FLAG: u32 = 0x8000_0000;

/// Harde bovengrens op het aantal gelijktijdige voices in het echte audiopad.
/// Dit is een vangnet tegen op hol geslagen groei (blijvende noten, koppel-
/// lussen, een tutti + release-staarten) — niet een muzikale limiet: bij een
/// normaal orgel blijf je er ruim onder. Zonder deze grens groeide de voice-Vec
/// onbeperkt en kon één zwaar akkoord de render-thread over de buffer-deadline
/// duwen → underruns → gekraak. Bij overschrijding wordt de zachtste voice
/// gestolen (voice-stealing). Gekoppeld aan de workspace-brede polyfonie-
/// constante zodat er maar één waarheid is.
pub const MAX_LIVE_VOICES: usize = vpo_audio::MAX_POLYPHONY;

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
    /// Vooraf gebouwde convolutiegalm inpluggen. Het decoderen + FFT-partitioneren
    /// gebeurt op de command-thread (commands.rs) — voorheen draaide disk-I/O en
    /// een volledige FFT-opbouw ín de realtime audio-callback (auditbevinding 12).
    LoadImpulseResponse(Box<ConvolutionReverb>),
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
    /// ODF-looppunten per bestand (GrandOrgue-regel: ODF wint van smpl-chunk).
    /// Nodig zodat óók de achtergrond-full-sample-load dezelfde loop gebruikt
    /// als de preload — anders verspringt de loop op het upgrademoment.
    RegisterOdfLoops(Arc<HashMap<PathBuf, (u32, u32)>>),
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
    /// Laatste twee RUWE samplewaarden (voor envelope/gain) — voor de
    /// release-fase-uitlijning (GrandOrgue BLOCK_HISTORY=2).
    hist_last: f32,
    hist_prev: f32,
    /// Leeftijd in output-samples (monotone klok; position loopt in de loop
    /// rond) — voor de geschaalde releases bij staccato (fase C).
    age_samples: u64,
    // ── Per-callback voorgerekende constanten ────────────────────────────
    // Deze waarden zijn constant over de duur van één audio-callback en worden
    // één keer per callback ververst (zie de pre-pass in de render-lus). Ze
    // stonden vroeger ín de per-sample-lus als HashMap-lookups + powf, wat bij
    // veel registers miljoenen keren per seconde draaide → CPU-overbelasting →
    // gekraak. Nu gebeurt dat werk één keer per voice per callback.
    /// Divisie-index (0..31) van deze voice; 0 als niet gemapt.
    c_div: usize,
    /// Lineaire volume-factor uit de gestapelde voicing (gebruiker + ODF-intonatie).
    c_voicing_gain: f32,
    /// Rate-vermenigvuldiger uit temperament × voicing-toonhoogte.
    c_pitch_mul: f64,
    /// Of de LFO-tremulant op deze voice moet werken (stops zonder trem-samples).
    c_use_lfo_trem: bool,
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
            hist_last: 0.0,
            hist_prev: 0.0,
            age_samples: 0,
            // Placeholders; de pre-pass in de render-lus ververst deze elke
            // callback vóór de voice voor het eerst gemixt wordt.
            c_div: 0,
            c_voicing_gain: 1.0,
            c_pitch_mul: 1.0,
            c_use_lfo_trem: true,
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
            hist_last: 0.0,
            hist_prev: 0.0,
            age_samples: 0,
            // Placeholders; de pre-pass in de render-lus ververst deze elke
            // callback vóór de voice voor het eerst gemixt wordt.
            c_div: 0,
            c_voicing_gain: 1.0,
            c_pitch_mul: 1.0,
            c_use_lfo_trem: true,
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
            hist_last: 0.0,
            hist_prev: 0.0,
            age_samples: 0,
            // Placeholders; de pre-pass in de render-lus ververst deze elke
            // callback vóór de voice voor het eerst gemixt wordt.
            c_div: 0,
            c_voicing_gain: 1.0,
            c_pitch_mul: 1.0,
            c_use_lfo_trem: true,
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
            self.releasing = true;
            self.target_envelope = 0.0;
            self.envelope = 0.0;
        }

        // Golfvorm-historie voor de release-fase-uitlijning: de laatste twee
        // RUWE waarden (zelfde domein als de release-sampledata).
        self.hist_prev = self.hist_last;
        self.hist_last = value;
        self.age_samples += 1;

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

/// Voeg een voice toe met respect voor [`MAX_LIVE_VOICES`]. Zit de Vec vol, dan
/// wordt eerst de "goedkoopste" voice gestolen: bij voorkeur een al uitklinkende
/// (releasing) voice, en daarbinnen die met het laagste envelope-niveau — dus de
/// minst hoorbare. `swap_remove` is O(1); de mix-lus en `retain` zijn ongevoelig
/// voor de volgorde. Alleen wanneer de grens bereikt is doen we de O(n)-scan.
#[inline]
fn push_voice_capped(voices: &mut Vec<PlayingVoice>, v: PlayingVoice, sample_rate: u32) {
    // Kies het slachtoffer: releasing voices eerst, daarbinnen het laagste
    // envelope-niveau.
    let pick_worst = |voices: &Vec<PlayingVoice>| {
        let mut worst = 0usize;
        let mut worst_score = f32::INFINITY;
        for (i, ex) in voices.iter().enumerate() {
            let score = ex.envelope + if ex.releasing { 0.0 } else { 10.0 };
            if score < worst_score {
                worst_score = score;
                worst = i;
            }
        }
        worst
    };
    if voices.len() >= MAX_LIVE_VOICES + 64 {
        // Harde noodgrens (zou zelden bereikt moeten worden): desnoods knippen.
        let worst = pick_worst(voices);
        voices.swap_remove(worst);
    } else if voices.len() >= MAX_LIVE_VOICES {
        // Zachte kap: een hoorbaar klinkende voice kreeg voorheen een harde
        // knip (klik, auditbevinding 54). Nu: al (bijna) stil → weghalen,
        // anders een 5ms-fade en heel even boven de kap laten meetellen.
        let worst = pick_worst(voices);
        if voices[worst].releasing && voices[worst].envelope < 0.01 {
            voices.swap_remove(worst);
        } else {
            voices[worst].release_ms(5.0, sample_rate);
        }
    }
    voices.push(v);
}

/// Geschaalde releases bij staccato (GrandOrgue m_IsScaledReleases, fase C).
/// Geeft (gain-schaal voor het startniveau, optionele staart-decay in ms):
/// - noot losgelaten tijdens de (geschatte) aanslag: release-gain 0,2..1,0;
/// - korte noot: de kerkgalm in de release-opname is nog niet opgebouwd
///   ("time to full reverb", 100-350 ms afhankelijk van de release-lengte) —
///   de staart wordt dan actief weggefaded (200 ms .. 6 s, evenredig met de
///   nootduur). Lange noten: (1.0, None) — volledig natuurlijk gedrag.
fn scaled_release_params(note_ms: f32, midi_note: u8, release_secs: f32) -> (f32, Option<f32>) {
    // Aanslagduur-schatting per toonhoogte: 50 ms boven MIDI 96, 500 ms onder
    // MIDI 24, lineair ertussen; onbekende noot -> gemiddelde pijp (60).
    let k = if midi_note == 0 || midi_note > 133 { 60 } else { midi_note };
    let attack_ms = if k >= 96 {
        50.0
    } else if k < 24 {
        500.0
    } else {
        500.0 + (24.0 - k as f32) * 6.25
    };
    let mut gain = 1.0f32;
    if note_ms < attack_ms {
        let ai = (note_ms / attack_ms).clamp(0.0, 1.0);
        gain = 0.2 + 0.8 * (2.0 * ai - ai * ai);
    }
    let ttfr = (60.0 * release_secs + 40.0).clamp(100.0, 350.0);
    let decay = if note_ms < ttfr {
        Some(ttfr + 6000.0 * note_ms / ttfr)
    } else {
        None
    };
    (gain, decay)
}

/// Begrens het aantal gelijktijdige release-staarten (one-shot voices): een
/// vol-werk-loslating op een natte GO-set spawnde per pijp × register een
/// release-voice (200+ in één klap, >600 stemmen totaal) — de mix verzoop,
/// het kraakte en de afbouw duurde heel lang. Boven het budget maakt de
/// stilste bestaande staart versneld plaats (30 ms fade) voor de nieuwe.
const MAX_RELEASE_VOICES: usize = 160;
fn budget_release_voices(voices: &mut Vec<PlayingVoice>, sample_rate: u32) {
    let count = voices.iter().filter(|v| v.one_shot).count();
    if count < MAX_RELEASE_VOICES {
        return;
    }
    let mut worst: Option<usize> = None;
    let mut worst_env = f32::INFINITY;
    for (i, v) in voices.iter().enumerate() {
        if v.one_shot && !v.releasing && v.envelope < worst_env {
            worst_env = v.envelope;
            worst = Some(i);
        }
    }
    if let Some(i) = worst {
        // 250 ms, zoals GrandOrgue's polyfonie-limiter (die gebruikt 370 ms):
        // kort genoeg om CPU te winnen, lang genoeg om onhoorbaar te blijven.
        voices[i].release_ms(250.0, sample_rate);
    }
}

/// Crossfade length for a loop of `loop_len` samples whose start is at index `ls`.
/// Long crossfades (Hauptwerk-style, up to ~500 ms) mask imperfect loop points;
/// bounded by half the loop and by the available pre-loop data (`ls`).
#[inline]
fn loop_xfade(loop_len: usize, ls: usize) -> usize {
    (loop_len / 2).min(ls).clamp(256, 24_000)
}

/// 4-punts Catmull-Rom/Hermite-interpolatie rond index `i` (leest i-1..i+2,
/// geklemd op de bufferranden). Lineaire interpolatie gaf bij fractionele
/// afspeelsnelheden (48kHz-set op een 44,1kHz-apparaat: rate≈1.088, en bij
/// temperament-/tremulant-detune) hoorbare aliasing/dofheid op heldere
/// registers; Hermite onderdrukt dat sterk voor ~5 extra mul/adds per sample.
/// Bij frac==0 (rate exact 1.0) is de uitkomst bit-exact data[i].
#[inline]
fn hermite4(data: &[f32], i: usize, frac: f32) -> f32 {
    if frac == 0.0 {
        return data[i];
    }
    let n = data.len();
    let xm1 = data[i.saturating_sub(1)];
    let x0 = data[i];
    let x1 = data[(i + 1).min(n - 1)];
    let x2 = data[(i + 2).min(n - 1)];
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b = w + a;
    ((a * frac - b) * frac + c) * frac + x0
}

/// Read one interpolated sample from `data`, looping over `[ls, le)` with
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
        return hermite4(data, i, frac);
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
    let primary = hermite4(data, i, frac);

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
                let blended = hermite4(data, j, pfrac);
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
        // 4096 i.p.v. 512: een tutti-akkoord op een groot orgel met koppels
        // produceert in één klap honderden NoteOn-commando's (stops × noten).
        // Op 512 liep de queue vol en blokkeerde send_timeout de aanroepende
        // thread (UI/MIDI/test-API) tot 3 s per commando — dat voelde als een
        // bevroren app. AudioCommand is klein; 4096 kost vrijwel niets.
        let (command_tx, command_rx) = bounded::<AudioCommand>(4096);
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
// (key, path, target_sample_rate, load_generation)
// De generatie voorkomt dat een achtergrond-load die pas ná een orgelwissel
// afrondt zijn sample onder een hergebruikte (stop,pipe)-key van het NIEUWE
// orgel registreert — dat gaf potentieel de verkeerde pijp of de verkeerde
// samplerate na een snelle orgelwissel.
type LoadRequest = ((u32, u32), PathBuf, u32, u64, Option<(u32, u32)>);

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
    // Arc-in-RwLock: RegisterOdfVoicings wisselt alleen de Arc om — de oude
    // code kloonde een duizenden-entries HashMap ín de audio-callback (audit 55).
    let odf_voicing: Arc<RwLock<Arc<HashMap<(u32, u32), (f32, f32)>>>> = Arc::new(RwLock::new(Arc::new(HashMap::new())));
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

    // Channel for background loading.
    // 1024 i.p.v. 64: een tutti-akkoord loslaten op een GO-set vraagt tientallen
    // release-loads tegelijk; bij een volle queue werden verzoeken STIL gedropt
    // en bleef de voice voorgoed op zijn 2s-attackloop hangen ("zombie").
    let (load_tx, load_rx) = bounded::<LoadRequest>(1024);
    // None = laden mislukt: de render-thread moet dan wél zijn in-flight-marker
    // opruimen, anders zou die pijp deze sessie nooit meer bijgeladen worden.
    let (loaded_tx, loaded_rx) = bounded::<((u32, u32), Option<SampleRef>, u64)>(256);

    // Opruim-kanaal ("janitor"): grote datastructuren die de audio-callback
    // moet loslaten (oude sample-maps bij een orgelwissel, geëvicte samples)
    // worden hierheen verplaatst en op een gewone thread gedropt. Gigabytes
    // aan Arc's vrijgeven ín de callback stalde de audio-thread seconden lang
    // (drop-storm) — precies genoeg om de watchdog te triggeren en gekraak of
    // volledige uitval te veroorzaken bij het wisselen van orgel na lang spelen.
    let (gc_tx, gc_rx) = bounded::<Box<dyn std::any::Any + Send>>(64);
    {
        let running_gc = running.clone();
        thread::spawn(move || {
            while running_gc.load(Ordering::Relaxed) {
                match gc_rx.recv_timeout(std::time::Duration::from_millis(200)) {
                    Ok(garbage) => drop(garbage),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                }
            }
        });
    }

    // Achtergrondlader-POOL (3 workers, crossbeam-kanalen zijn MPMC): één
    // thread werkte een tutti-loslating (tientallen release-bestanden ineens)
    // serieel af, waardoor late releases seconden op hun staart wachtten en
    // zolang op het preload-fragment bleven hangen. Gedeelde staat:
    // - decode_cache (pad → Weak): GO-stops delen vaak één rank (REF) en
    //   same-file releases gebruiken hetzelfde bestand als de attack — zonder
    //   cache werd zo'n bestand per (stop,pijp)-key opnieuw gedecodeerd én
    //   geresampled. Weak: dedupliceert zolang de engine de sample vasthoudt,
    //   maar houdt zelf geen geheugen in leven (RAM-cap blijft leidend).
    // - max_generation: verzoeken van vóór een orgelwissel overslaan.
    let decode_cache: Arc<parking_lot::Mutex<HashMap<PathBuf, (u64, std::sync::Weak<SampleData>)>>> =
        Arc::new(parking_lot::Mutex::new(HashMap::new()));
    let max_generation = Arc::new(AtomicU64::new(0));
    for worker in 0..3 {
        let loaded_tx_clone = loaded_tx.clone();
        let load_rx = load_rx.clone();
        let running_bg = running.clone();
        let decode_cache = decode_cache.clone();
        let max_generation = max_generation.clone();
        thread::Builder::new()
            .name(format!("sample-loader-{}", worker))
            .spawn(move || {
                // Onder normale prioriteit: WAV-decodes van vol werk (20+
                // registers loslaten/aanslaan op een natte GO-set) verdrongen
                // de audio-callback van zijn CPU-tijd → hapers/tikken. De
                // loads duren iets langer; de preload-buffers overbruggen dat.
                #[cfg(windows)]
                unsafe {
                    use windows_sys::Win32::System::Threading::{
                        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
                    };
                    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                }
                while running_bg.load(Ordering::Relaxed) {
                    match load_rx.recv_timeout(std::time::Duration::from_millis(100)) {
                        Ok((key, path, target_rate, generation, odf_loop)) => {
                            let seen = max_generation.fetch_max(generation, Ordering::Relaxed).max(generation);
                            if generation < seen {
                                continue; // verouderd verzoek van een vorig orgel
                            }
                            {
                                let cache = decode_cache.lock();
                                if let Some((g, weak)) = cache.get(&path) {
                                    if *g == generation {
                                        if let Some(cached) = weak.upgrade() {
                                            let _ = loaded_tx_clone.send((key, Some(cached), generation));
                                            continue;
                                        }
                                    }
                                }
                            }
                            match load_audio(&path) {
                                Ok(mut sample_data) => {
                                    // GO-regel: ODF-looppunten winnen van de smpl-chunk —
                                    // zelfde override als het preload-pad, anders verspringt
                                    // de loop op het preload→full-upgrademoment.
                                    if let Some((ls, le)) = odf_loop {
                                        if le > ls {
                                            sample_data.loop_start = Some(ls as u64);
                                            sample_data.loop_end = Some(le as u64);
                                            vpo_sampler::optimize_loop_points(&mut sample_data);
                                        }
                                    }
                                    // Resample if needed
                                    if sample_data.sample_rate != target_rate {
                                        match resample(&sample_data, target_rate) {
                                            Ok(resampled) => sample_data = resampled,
                                            Err(e) => {
                                                warn!("Failed to resample {:?}: {}", path, e);
                                                // Faal-marker: render-thread ruimt
                                                // zijn in-flight-administratie op.
                                                let _ = loaded_tx_clone.send((key, None, generation));
                                                continue;
                                            }
                                        }
                                    }
                                    let sample = Arc::new(sample_data);
                                    {
                                        // Verlopen generaties en dode verwijzingen opruimen.
                                        let mut cache = decode_cache.lock();
                                        cache.retain(|_, (g, w)| *g == generation && w.strong_count() > 0);
                                        cache.insert(path.clone(), (generation, Arc::downgrade(&sample)));
                                    }
                                    let _ = loaded_tx_clone.send((key, Some(sample), generation));
                                }
                                Err(e) => {
                                    warn!("Failed to background load {:?}: {}", path, e);
                                    let _ = loaded_tx_clone.send((key, None, generation));
                                }
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(_) => break,
                    }
                }
                info!("Background loader thread stopped");
            })
            .expect("sample-loader thread spawn");
    }

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
    // ODF-looppunt-overrides per bestandspad (RegisterOdfLoops); alleen de
    // render-closure gebruikt dit — lookup bij het versturen van een
    // achtergrond-laadverzoek zodat de full sample dezelfde loop krijgt.
    let odf_loops_clone: Arc<RwLock<Arc<HashMap<PathBuf, (u32, u32)>>>> = Arc::new(RwLock::new(Arc::new(HashMap::new())));

    let channels = stream_config.channels as usize;

    // Achtergrondlaad-administratie van de render-thread (closure-state, geen
    // locks): welke keys al onderweg zijn (dedup — een tutti stuurt anders
    // hetzelfde bestand tientallen keren de queue in) en de huidige laad-
    // generatie (resultaten van vóór een orgelwissel worden genegeerd).
    let mut inflight_loads: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    // Pijpen zonder sample waarvoor al één warn is gelogd (max 1 per pijp per orgel).
    let mut missing_sample_warned: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut load_generation: u64 = 0;
    // RAM-plafond op volledig geladen samples. Zonder plafond groeide het
    // geheugen tijdens lang spelen onbegrensd (elke gespeelde pijp + release
    // blijft resident): uren spelen op een grote GO-set = vele GB's → paging →
    // gekraak, en een orgelwissel daarna dropte al die GB's in één callback.
    // Eviction is altijd veilig: klinkende voices houden hun eigen Arc vast;
    // een geëvicte pijp start de volgende keer gewoon weer van zijn preload.
    const SAMPLES_BYTES_CAP: usize = 1_500_000_000; // ~1,5 GB
    let mut samples_bytes: usize = 0;
    let mut sample_fifo: std::collections::VecDeque<(u32, u32)> = std::collections::VecDeque::new();
    let gc_tx_render = gc_tx.clone();
    // Master-limiter (unit-getest in vpo-audio): plafond 0.97 (~-0,26 dBFS),
    // attack ~1,5 ms (gedoseerd — één piek trekt niet het hele orgel omlaag),
    // release ~250 ms. Zie het limiterblok in de frame-lus voor waarom dit de
    // kale tanh-vervorming vervangt.
    let mut master_limiter = MasterLimiter::new(0.97, 0.0015, 0.25, sample_rate);
    // Wachtrij voor opruimwerk dat de janitor even niet aankon (kanaal vol):
    // vasthouden en de volgende callback opnieuw aanbieden — NOOIT inline
    // droppen, want dat is precies de drop-storm die de janitor voorkomt.
    let mut gc_backlog: Vec<Box<dyn std::any::Any + Send>> = Vec::new();
    // Voor de galm-bypass: bij de overgang naar mix 0 één keer de staart
    // wissen — anders klinkt bij her-inschakelen eerst een bevroren, oude
    // staart die niets met het huidige spel te maken heeft.
    let mut reverb_was_audible = true;

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

            // Achtergestelde opruimboxen alsnog naar de janitor proberen te
            // schuiven (kanaal was vol op het moment van ontstaan).
            while let Some(item) = gc_backlog.pop() {
                if let Err(e) = gc_tx_render.try_send(item) {
                    gc_backlog.push(e.into_inner());
                    break;
                }
            }

            // Check for completed background loads. Gecapt per callback: elke
            // integratie doet een map-insert + voice-scan; tientallen tegelijk
            // verwerken zou de callback over zijn deadline duwen. De rest wacht
            // gewoon één callback (~3-6 ms) langer in het kanaal.
            let mut integrated = 0;
            while integrated < 16 {
                match loaded_rx.try_recv() {
                    Ok((key, sample_opt, generation)) => {
                        // Resultaat van vóór een orgelwissel → volledig negeren:
                        // de key kan in het nieuwe orgel een ándere pijp
                        // betekenen, en zijn in-flight-marker (van een eventuele
                        // verse aanvraag onder dezelfde key) mag hij níet wissen.
                        if generation != load_generation {
                            continue;
                        }
                        inflight_loads.remove(&key);
                        // None = laden mislukt; alleen de administratie opruimen
                        // zodat een volgende NoteOn het opnieuw mag proberen.
                        let Some(sample) = sample_opt else { continue };
                        let size = sample.data.len() * std::mem::size_of::<f32>();
                        if let Some(old) = samples_clone.write().insert(key, sample.clone()) {
                            // Zelfde key opnieuw geladen: oude telling eraf.
                            samples_bytes = samples_bytes
                                .saturating_sub(old.data.len() * std::mem::size_of::<f32>());
                        } else {
                            sample_fifo.push_back(key);
                        }
                        samples_bytes += size;
                        // Upgrade any playing voices using this sample
                        for voice in voices_clone.write().iter_mut() {
                            if voice.stop_id == key.0 && voice.pipe_num == key.1 {
                                voice.upgrade_to_full(sample.clone());
                            }
                        }
                        integrated += 1;
                    }
                    Err(_) => break,
                }
            }

            // RAM-plafond handhaven: oudste full samples wijken (max 4 per
            // callback — begrensd werk). De drop zelf gaat naar de janitor-
            // thread zodat de callback geen grote deallocaties doet.
            let mut evicted = 0;
            while samples_bytes > SAMPLES_BYTES_CAP && evicted < 4 {
                let Some(old_key) = sample_fifo.pop_front() else { break; };
                if let Some(old) = samples_clone.write().remove(&old_key) {
                    samples_bytes = samples_bytes
                        .saturating_sub(old.data.len() * std::mem::size_of::<f32>());
                    if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                    evicted += 1;
                }
            }

            // Process commands. Gecapt per callback: een tutti-akkoord op veel
            // registers levert in één klap honderden NoteOn/NoteOff-commando's;
            // die allemaal binnen één callback verwerken (met map-lookups en
            // voice-scans per commando) kon de realtime-deadline overschrijden.
            // De rest wacht één callback (~3-6 ms) — onhoorbaar, en de FIFO-
            // volgorde blijft intact.
            let mut cmds_done = 0u32;
            while cmds_done < 256 {
                let Ok(cmd) = command_rx_clone.try_recv() else { break };
                cmds_done += 1;
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
                            push_voice_capped(&mut voices_clone.write(), voice, sample_rate);
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

                                // Request background load of full sample.
                                // Dedup: loopt er al een load voor deze key, dan
                                // niet opnieuw insturen — de upgrade-scan pakt
                                // álle voices met deze key. Alleen markeren bij
                                // een geslaagde send: bij een volle queue mag een
                                // volgende NoteOn het opnieuw proberen.
                                if inflight_loads.contains(&key) {
                                    voice.mark_load_requested();
                                } else {
                                    let path = preload.source_path.clone();
                                    let odf_lp = odf_loops_clone.read().get(&path).copied();
                                    if load_tx_clone.try_send((key, path, sample_rate, load_generation, odf_lp)).is_ok() {
                                        inflight_loads.insert(key);
                                        voice.mark_load_requested();
                                    }
                                }

                                push_voice_capped(&mut voices_clone.write(), voice, sample_rate);
                            } else {
                                // Max één warn per pijp: dit pad is bereikbaar in
                                // normaal spel (onopgeloste REF-pijpen) en logde
                                // voorheen bij ÉLKE toetsaanslag — bestand-I/O
                                // onder een globale mutex in de audio-callback
                                // (auditbevinding 3).
                                if missing_sample_warned.insert((stop_id, pipe_num)) {
                                    warn!("NoteOn: No sample or preload for stop={}, pipe={}", stop_id, pipe_num);
                                }
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
                                    // huidige envelope-niveau van de sustain,
                                    // geschaald voor staccato (fase C).
                                    let note_ms = voice.age_samples as f32 * 1000.0 / (sample_rate.max(1)) as f32;
                                    let rel_secs = rel_pre.as_ref()
                                        .map(|p| p.total_samples as f32 / (p.sample_rate.max(1)) as f32)
                                        .unwrap_or(2.0);
                                    let (stacc_gain, stacc_decay) = scaled_release_params(note_ms, voice.midi_note, rel_secs);
                                    let level = (voice.envelope * stacc_gain).clamp(0.05, 1.0);
                                    // Fase-uitlijning (GrandOrgue-stijl, fase B):
                                    // start de release op de positie waarvan de
                                    // golfvorm aansluit op de stervende stem —
                                    // start op frame 0 gaf een hoorbare tik.
                                    let align_pos: u64 = rel_pre.as_ref()
                                        .and_then(|p| p.align_table.as_ref())
                                        .map(|t| t.position_for(voice.hist_last, voice.hist_prev) as u64)
                                        .unwrap_or(0);
                                    let mut rv = if let Some(sample) = rel_full.clone() {
                                        let mut v = PlayingVoice::new_from_sample(sample, stop_id, release_key.1, voice.midi_note, 1.0);
                                        // Same-file release: de volledige sample is
                                        // het HELE bestand — spring naar het
                                        // release-segment (cue-marker) + de
                                        // fase-uitlijn-offset, in output-rate frames
                                        // (de full sample is naar output-rate
                                        // geresampled).
                                        if let Some(ref pre) = rel_pre {
                                            if pre.sample_rate > 0 {
                                                v.position = (pre.trim_start as u64 + align_pos) as f64
                                                    * sample_rate as f64 / pre.sample_rate as f64;
                                            }
                                        }
                                        v
                                    } else {
                                        let pre = rel_pre.clone().unwrap();
                                        let mut v = PlayingVoice::new_from_preload(
                                            pre, stop_id, release_key.1, voice.midi_note, 1.0, sample_rate,
                                        );
                                        // Fase-uitlijning: preload-data begint op het
                                        // segment; positie is in bron-frames.
                                        if align_pos > 0 {
                                            v.position = align_pos as f64;
                                        }
                                        // Stream de volledige release-staart bij (met dedup).
                                        if let Some(path) = v.needs_background_load() {
                                            let odf_lp = odf_loops_clone.read().get(&path).copied();
                                            if inflight_loads.contains(&release_key) {
                                                v.mark_load_requested();
                                            } else if load_tx_clone.try_send((release_key, path, sample_rate, load_generation, odf_lp)).is_ok() {
                                                inflight_loads.insert(release_key);
                                                v.mark_load_requested();
                                            }
                                        }
                                        v
                                    };
                                    rv.one_shot = true;
                                    rv.envelope = level;
                                    rv.target_envelope = level;
                                    // Korte noot: staart actief afbouwen — de galm
                                    // in de opname is nog niet volledig opgebouwd.
                                    if let Some(decay_ms) = stacc_decay {
                                        rv.release_ms(decay_ms, sample_rate);
                                    }
                                    spawn = Some(rv);
                                }
                                voice.release_ms(release_fade_ms(voice.midi_note), sample_rate);
                            }
                        }
                        if let Some(rv) = spawn {
                            budget_release_voices(&mut voices_lock, sample_rate);
                            push_voice_capped(&mut voices_lock, rv, sample_rate);
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
                                    // Staccato-schaal + fase-uitlijning — zie NoteOff.
                                    let note_ms = voice.age_samples as f32 * 1000.0 / (sample_rate.max(1)) as f32;
                                    let rel_secs = rel_pre.as_ref()
                                        .map(|p| p.total_samples as f32 / (p.sample_rate.max(1)) as f32)
                                        .unwrap_or(2.0);
                                    let (stacc_gain, stacc_decay) = scaled_release_params(note_ms, voice.midi_note, rel_secs);
                                    let level = (voice.envelope * stacc_gain).clamp(0.05, 1.0);
                                    // Fase-uitlijning — zie NoteOff.
                                    let align_pos: u64 = rel_pre.as_ref()
                                        .and_then(|p| p.align_table.as_ref())
                                        .map(|t| t.position_for(voice.hist_last, voice.hist_prev) as u64)
                                        .unwrap_or(0);
                                    let mut rv = if let Some(sample) = rel_full {
                                        let mut v = PlayingVoice::new_from_sample(sample, stop_id, release_key.1, voice.midi_note, 1.0);
                                        // Same-file release: spring naar het segment
                                        // + fase-offset (zie NoteOff voor de uitleg).
                                        if let Some(ref pre) = rel_pre {
                                            if pre.sample_rate > 0 {
                                                v.position = (pre.trim_start as u64 + align_pos) as f64
                                                    * sample_rate as f64 / pre.sample_rate as f64;
                                            }
                                        }
                                        v
                                    } else {
                                        let pre = rel_pre.unwrap();
                                        let mut v = PlayingVoice::new_from_preload(
                                            pre, stop_id, release_key.1, voice.midi_note, 1.0, sample_rate,
                                        );
                                        if align_pos > 0 {
                                            v.position = align_pos as f64;
                                        }
                                        if let Some(path) = v.needs_background_load() {
                                            let odf_lp = odf_loops_clone.read().get(&path).copied();
                                            if inflight_loads.contains(&release_key) {
                                                v.mark_load_requested();
                                            } else if load_tx_clone.try_send((release_key, path, sample_rate, load_generation, odf_lp)).is_ok() {
                                                inflight_loads.insert(release_key);
                                                v.mark_load_requested();
                                            }
                                        }
                                        v
                                    };
                                    rv.one_shot = true;
                                    rv.envelope = level;
                                    rv.target_envelope = level;
                                    if let Some(decay_ms) = stacc_decay {
                                        rv.release_ms(decay_ms, sample_rate);
                                    }
                                    spawns.push(rv);
                                    spawned.insert(voice.pipe_num);
                                }
                            }
                            voice.release_ms(release_fade_ms(voice.midi_note), sample_rate);
                        }
                        for rv in spawns {
                            budget_release_voices(&mut voices_lock, sample_rate);
                            push_voice_capped(&mut voices_lock, rv, sample_rate);
                        }
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
                        *odf_voicing_clone.write() = map;
                    }
                    AudioCommand::RegisterOdfLoops(map) => {
                        if !map.is_empty() {
                            info!("ODF-looppunten geregistreerd voor {} bestanden (ook voor full-sample-loads)", map.len());
                        }
                        *odf_loops_clone.write() = map;
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
                        {
                            let mut samples_lock = samples_clone.write();
                            // Oude map (kan GB's zijn na lang spelen) naar de
                            // janitor-thread — droppen in de callback stalde de
                            // audio-thread seconden lang.
                            let old = std::mem::take(&mut *samples_lock);
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                            samples_bytes = 0;
                            sample_fifo.clear();
                            for (k, v) in new_samples.iter() {
                                samples_bytes += v.data.len() * std::mem::size_of::<f32>();
                                sample_fifo.push_back(*k);
                                samples_lock.insert(*k, v.clone());
                            }
                        }
                        {
                            let mut preloads_lock = preloads_clone.write();
                            let old = std::mem::take(&mut *preloads_lock);
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                        }
                        {
                            let mut voices_lock = voices_clone.write();
                            let old = std::mem::take(&mut *voices_lock);
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                        }
                        // Nieuwe laad-generatie: uitstaande achtergrond-loads van
                        // het vorige orgel mogen hun (hergebruikte) keys niet vullen.
                        load_generation += 1;
                        inflight_loads.clear();
                        missing_sample_warned.clear();
                        // Vers orgel = verse limiter-staat (geen ingehouden gain
                        // van een luide passage op het vorige orgel).
                        master_limiter.reset();
                        info!("Loaded {} preloaded samples", new_samples.len());
                    }
                    AudioCommand::RegisterPreloadBuffers(buffers) => {
                        // Clear previous and register new preload buffers.
                        // Oude maps naar de janitor (zie LoadSamples): dit is
                        // het orgelwissel-pad — na uren spelen zaten hier GB's.
                        {
                            let mut samples_lock = samples_clone.write();
                            let old = std::mem::take(&mut *samples_lock);
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                            samples_bytes = 0;
                            sample_fifo.clear();
                        }
                        {
                            let mut preloads_lock = preloads_clone.write();
                            let old = std::mem::take(&mut *preloads_lock);
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                            for (k, v) in buffers.iter() {
                                preloads_lock.insert(*k, v.clone());
                            }
                        }
                        {
                            let mut voices_lock = voices_clone.write();
                            let old = std::mem::take(&mut *voices_lock);
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                        }
                        // Nieuwe laad-generatie (zie LoadSamples).
                        load_generation += 1;
                        inflight_loads.clear();
                        missing_sample_warned.clear();
                        // Vers orgel = verse limiter-staat (geen ingehouden gain
                        // van een luide passage op het vorige orgel).
                        master_limiter.reset();
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
                        let size = sample.data.len() * std::mem::size_of::<f32>();
                        if let Some(old) = samples_clone.write().insert(key, sample) {
                            samples_bytes = samples_bytes
                                .saturating_sub(old.data.len() * std::mem::size_of::<f32>());
                        } else {
                            sample_fifo.push_back(key);
                        }
                        samples_bytes += size;
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
                                        // Request background load for new voice (met dedup)
                                        if let Some(path) = nv.needs_background_load() {
                                            let odf_lp = odf_loops_clone.read().get(&path).copied();
                                            if inflight_loads.contains(&key) {
                                                nv.mark_load_requested();
                                            } else if load_tx_clone.try_send((key, path, sample_rate, load_generation, odf_lp)).is_ok() {
                                                inflight_loads.insert(key);
                                                nv.mark_load_requested();
                                            }
                                        }
                                        new_voices.push(nv);
                                    }
                                }
                            }
                            // Add new crossfade voices
                            for nv in new_voices { push_voice_capped(&mut voices_lock, nv, sample_rate); }
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
                    AudioCommand::LoadImpulseResponse(rev) => {
                        // Kant-en-klaar aangeleverd; hier alleen inpluggen.
                        let mut rev = *rev;
                        rev.mix = *reverb_mix_clone.read();
                        *reverb_clone.write() = Some(rev);
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
                        // Grote maps naar de janitor-thread (zie LoadSamples).
                        {
                            let old = std::mem::take(&mut *samples_clone.write());
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                            samples_bytes = 0;
                            sample_fifo.clear();
                        }
                        {
                            let old = std::mem::take(&mut *preloads_clone.write());
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                        }
                        {
                            let old = std::mem::take(&mut *trem_preloads_clone.write());
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                        }
                        trem_active_clone.write().clear();
                        percussive_stops_clone.write().clear();
                        *odf_voicing_clone.write() = Arc::new(HashMap::new());
                        {
                            let old = std::mem::take(&mut *voices_clone.write());
                            if let Err(e) = gc_tx_render.try_send(Box::new(old)) { gc_backlog.push(e.into_inner()); }
                        }
                        // Nieuwe laad-generatie (zie LoadSamples).
                        load_generation += 1;
                        inflight_loads.clear();
                        missing_sample_warned.clear();
                        // Vers orgel = verse limiter-staat (geen ingehouden gain
                        // van een luide passage op het vorige orgel).
                        master_limiter.reset();
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

            // Generate audio.
            // Deze config-locks worden UITSLUITEND geschreven in de command-fase
            // hierboven (zelfde thread, vóór dit punt). Tijdens het renderen is er
            // dus geen enkele schrijver — we houden read-guards vast i.p.v. elke
            // callback zes Vecs te klonen (incl. de geneste Vec<Vec<u8>> van de
            // routing). Dat scheelt zes heap-allocaties per callback; op ASIO4ALL
            // (zeer hoge callback-frequentie) is dat merkbaar minder allocatie-
            // jitter op de audio-thread — precies wat gekraak veroorzaakt.
            let gain = *master_gain_clone.read() * *master_expression_clone.read();
            let div_gains = division_gains_clone.read();
            let div_pans = division_pans_clone.read();
            let out_chans_lock = output_channels_clone.read();
            let ccis_en_lock = ccis_enabled_clone.read();
            let (ccis_strength, ccis_falloff, ccis_swap) = *ccis_params_clone.read();
            let wind_group_assignment = wind_groups_clone.read();
            let swell_cfgs = swell_configs_clone.read();
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

                // Galm-bypass-overgang: zodra de mix naar 0 gaat wordt het
                // galmblok in de frame-lus overgeslagen (rekenwerk besparen),
                // maar de interne vertragingslijnen bevriezen dan met de oude
                // staart erin. Eén keer wissen bij de overgang, zodat her-
                // inschakelen met een schone (stille) galm begint.
                let reverb_audible = reverb_mix_now > 1e-6;
                if !reverb_audible && reverb_was_audible {
                    fdn_lock.clear();
                    if let Some(ref mut rev) = *conv_lock {
                        rev.reset();
                    }
                }
                reverb_was_audible = reverb_audible;

                // ── Voorrekenen per voice (één keer per callback, niet per sample) ──
                // De divisie-index, de gestapelde voicing (gebruiker + ODF-intonatie)
                // en of de LFO-tremulant meedoet zijn constant over deze callback.
                // Vroeger deed de per-sample-lus hiervoor 4 HashMap-lookups + tot 2
                // powf PER VOICE PER SAMPLE — bij ~200 voices op 44,1 kHz zijn dat
                // tientallen miljoenen bewerkingen per seconde op één kern, wat het
                // buffer-budget oversteeg en gekraak gaf. Nu gebeurt het één keer per
                // voice per callback (~6 ms), dus honderden malen minder vaak.
                // Voicing/temperament-wijzigingen worden zo de volgende callback
                // (onhoorbaar snel) opgepakt — live bijregelen blijft dus werken.
                for voice in voices_lock.iter_mut() {
                    voice.c_div = stop_div_map.get(&voice.stop_id).copied().unwrap_or(0) as usize;

                    let vkey = (voice.stop_id, voice.pipe_num & !RELEASE_PIPE_FLAG);
                    let (user_vol, user_pitch) = pipe_voicing_map.get(&vkey).copied().unwrap_or((0.0, 0.0));
                    let (odf_vol, odf_pitch) = odf_voicing_map.get(&vkey).copied().unwrap_or((0.0, 0.0));
                    let voicing_vol = user_vol + odf_vol;
                    let voicing_pitch = user_pitch + odf_pitch;
                    voice.c_voicing_gain = if voicing_vol.abs() > 1e-6 {
                        10.0_f32.powf(voicing_vol / 20.0)
                    } else {
                        1.0
                    };

                    let note_class = (voice.midi_note % 12) as usize;
                    let temp = temp_ratios[note_class] as f64;
                    let pitch_mul = if voicing_pitch.abs() > 0.01 {
                        2.0_f64.powf(voicing_pitch as f64 / 1200.0)
                    } else {
                        1.0
                    };
                    voice.c_pitch_mul = temp * pitch_mul;

                    voice.c_use_lfo_trem = !stops_with_trem.contains(&voice.stop_id);
                }

                // Per-divisie basis-pan → constant-power L/R (cos/sin), één keer per
                // callback. div_pans wordt elke callback opnieuw gelezen, dus een
                // divisie live verpannen blijft direct hoorbaar. Alleen wanneer de
                // C/Cis-spreiding voor een divisie aanstaat wijkt de pan per noot af
                // en rekenen we cos/sin daar in de lus opnieuw uit.
                let mut div_pan_cos = [0.0f32; 32];
                let mut div_pan_sin = [0.0f32; 32];
                for d in 0..32 {
                    let bp = div_pans.get(d).copied().unwrap_or(0.0);
                    let angle = (bp + 1.0) * 0.25 * std::f32::consts::PI;
                    div_pan_cos[d] = angle.cos();
                    div_pan_sin[d] = angle.sin();
                }

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

                    // Wind- en tremulant-modulatie zijn per DIVISIE gelijk voor alle
                    // voices in die divisie. Bereken de dure powf/sqrt hier één keer
                    // per divisie (max 32) i.p.v. per voice — dat scheelt bij veel
                    // voices het leeuwendeel van het rekenwerk. In de rustsituatie
                    // (geen wind, geen LFO-trem) blijft alles op 1.0 en draait er
                    // geen enkele powf.
                    let mut wind_rate = [1.0f64; 32];
                    let mut wind_gain = [1.0f32; 32];
                    for d in 0..32 {
                        let wp = wind_pressures[d];
                        if (wp - 1.0).abs() > 1e-6 {
                            wind_rate[d] = 2.0_f64.powf(((wp - 1.0) * 30.0) as f64 / 1200.0);
                            wind_gain[d] = wp.sqrt();
                        }
                    }
                    let mut trem_rate = [1.0f64; 32];
                    for d in 0..32 {
                        let tp = trem_mods[d].1;
                        if tp.abs() > 0.01 {
                            trem_rate[d] = 2.0_f64.powf(tp as f64 / 1200.0);
                        }
                    }

                    for voice in voices_lock.iter_mut() {
                        let div_idx = voice.c_div;

                        // Effectieve afspeel-rate = basis (samplerate-correctie) ×
                        // gecachte temperament+voicing-pitch × per-sample wind/trem.
                        // next_sample() leest self.rate; we zetten hem tijdelijk en
                        // herstellen daarna de basis (anders compoundeert de detune).
                        let base_rate = voice.rate;
                        let mut eff_rate = base_rate * voice.c_pitch_mul;
                        if div_idx < 32 {
                            eff_rate *= wind_rate[div_idx];
                            if voice.c_use_lfo_trem {
                                eff_rate *= trem_rate[div_idx];
                            }
                        }
                        voice.rate = eff_rate;
                        let sample = voice.next_sample();
                        voice.rate = base_rate;

                        // Volume = gecachte voicing-gain × per-sample wind/trem-amplitude.
                        let mut g = voice.c_voicing_gain;
                        if div_idx < 32 {
                            g *= wind_gain[div_idx];
                            if voice.c_use_lfo_trem {
                                g *= trem_mods[div_idx].0;
                            }
                        }
                        let contribution = sample * g;
                        if div_idx >= 32 { continue; }

                        // Panning: standaard de per-callback voorgerekende constant-
                        // power cos/sin van de divisie (geen transcendente functies in
                        // het hete pad). Alleen wanneer de C/Cis-lade-spreiding voor
                        // deze divisie actief is, wijkt de pan per noot af — even MIDI-
                        // noten naar de ene kant, oneven naar de andere, sterkst bij de
                        // laagste pijpen — en rekenen we cos/sin hier opnieuw uit.
                        let (pan_cos, pan_sin) = if ccis_on
                            && ccis_en_lock.get(div_idx).copied().unwrap_or(false)
                        {
                            let n = voice.midi_note as f32;
                            let norm = ((n - 36.0) / 60.0).clamp(0.0, 1.0); // 0 bij C groot, 1 bij c''''
                            let pitch_factor = (1.0 - ccis_falloff * norm).clamp(0.0, 1.0);
                            let sep = (ccis_strength * pitch_factor).clamp(0.0, 0.95);
                            let mut side = if voice.midi_note % 2 == 0 { -1.0 } else { 1.0 };
                            if ccis_swap { side = -side; }
                            let note_pan = side * sep;
                            let base_pan = div_pans.get(div_idx).copied().unwrap_or(0.0);
                            let total_pan = (base_pan + note_pan).clamp(-1.0, 1.0);
                            let angle = (total_pan + 1.0) * 0.25 * std::f32::consts::PI;
                            (angle.cos(), angle.sin())
                        } else {
                            (div_pan_cos[div_idx], div_pan_sin[div_idx])
                        };
                        div_l[div_idx] += contribution * pan_cos;
                        div_r[div_idx] += contribution * pan_sin;
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
                        // NaN/Inf hier al afvangen, vóór de stateful keten (zwel-
                        // filters/FDN/EQ): een niet-eindige som vergiftigt anders
                        // de filter-staat blijvend — het vangnet aan het einde
                        // schoont alleen het uitgangsframe (auditbevinding 14).
                        let l_raw = if l_raw.is_finite() { l_raw } else { 0.0 };
                        let r_raw = if r_raw.is_finite() { r_raw } else { 0.0 };
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
                                let mut routed = false;
                                while i < list.len() {
                                    let lc = list[i] as usize;
                                    if i + 1 < list.len() {
                                        let rc = list[i + 1] as usize;
                                        if lc < channels { frame[lc] += l; touched |= 1u64 << lc.min(63); routed = true; }
                                        if rc < channels { frame[rc] += r; touched |= 1u64 << rc.min(63); routed = true; }
                                        i += 2;
                                    } else {
                                        if lc < channels { frame[lc] += (l + r) * 0.5; touched |= 1u64 << lc.min(63); routed = true; }
                                        i += 1;
                                    }
                                }
                                // Alle geconfigureerde kanalen vallen buiten dit
                                // apparaat (bv. 8-kanaals routing op een stereo-
                                // uitgang na een profielwissel): val terug op het
                                // voorste paar i.p.v. het droge signaal geluidloos
                                // te laten verdwijnen terwijl de galm wél klinkt
                                // (auditbevinding 37).
                                if !routed {
                                    if channels >= 2 {
                                        frame[0] += l; frame[1] += r;
                                        touched |= 0b11;
                                    } else if channels >= 1 {
                                        frame[0] += (l + r) * 0.5;
                                        touched |= 0b1;
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
                    // Staat de galm op 0 (of uit), sla dan het HELE galm-blok over:
                    // de FDN/convolutie draaide voorheen elke sample door, zelfs bij
                    // mix 0 — puur weggegooid rekenwerk. Bij mix 0 is er ook geen
                    // staart om te bewaren, dus overslaan is klik-vrij.
                    if reverb_mix_now > 1e-6 {
                        if use_algo || conv_lock.is_none() {
                            let (l, r) = fdn_lock.process_stereo(sum_l, sum_r);
                            wet_l = l * reverb_mix_now;
                            wet_r = r * reverb_mix_now;
                        } else if let Some(ref mut rev) = *conv_lock {
                            let w = rev.process_wet_sample((sum_l + sum_r) * 0.5) * reverb_mix_now;
                            wet_l = w;
                            wet_r = w;
                        }
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

                    // Master-limiter + peak-meters (voorste paar). Dit verving de
                    // kale tanh-per-kanaal: bij een vol werk (honderden stemmen,
                    // som ver boven 1.0) betekende tanh diepe saturatie — hoorbare
                    // vervorming ("overstuur") én platgeslagen dynamiek ("gedempt"),
                    // waarbij elk kanaal onafhankelijk vervormde en het stereobeeld
                    // instortte. De limiter verlaagt in plaats daarvan de TOTALE
                    // gain: attack direct (piek kan nooit boven het plafond uit),
                    // release ~250 ms terug naar 1.0. Geen vervorming, de onderlinge
                    // balans en het stereobeeld blijven intact, en na de piek veert
                    // het niveau vanzelf terug.
                    let mut frame_peak = 0.0f32;
                    for s in frame.iter() {
                        frame_peak = frame_peak.max(s.abs());
                    }
                    // NaN/Inf-vangnet (dit deed de oude tanh impliciet: tanh(±inf)=±1).
                    // Zonder deze guard zou een niet-eindige sample (defect bestand,
                    // instabiel filter) via threshold/inf → gain 0 → inf×0 = NaN de
                    // hele uitgang vergiftigen.
                    if !frame_peak.is_finite() {
                        for s in frame.iter_mut() {
                            if !s.is_finite() { *s = 0.0; }
                        }
                        frame_peak = frame.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                    }
                    let limiter_gain = master_limiter.process(frame_peak);
                    for (ci, s) in frame.iter_mut().enumerate() {
                        *s = (*s * limiter_gain).clamp(-1.0, 1.0);
                        if ci == 0 { peak_l = peak_l.max(s.abs()); }
                        else if ci == 1 { peak_r = peak_r.max(s.abs()); }
                    }
                    // Recorder-tap: neem de VOLLEDIGE stereo-downmix op (alle divisies +
                    // galm), onafhankelijk van de fysieke kanaalrouting. Anders zou een
                    // klavier dat naar surround-kanalen geroutet is niet in de opname
                    // belanden. sum_l/sum_r bevatten alle divisies vóór kanaalroutering.
                    // Zelfde limiter-gain als de uitgang, zodat de opname klinkt als
                    // wat er gespeeld werd.
                    if rec_active.is_some() {
                        let l = ((sum_l + wet_l) * limiter_gain).clamp(-1.0, 1.0);
                        let r = ((sum_r + wet_r) * limiter_gain).clamp(-1.0, 1.0);
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
    // Een enkele stream-fout is onder piekbelasting vaak een kortstondige
    // ASIO-underrun: de driver meldt een fout maar blíjft callbacks leveren.
    // De stream dán meteen afbreken maakt een korte hapering juist een langere
    // stilte (en kan op ASIO4ALL escaleren naar een volledige herstart +
    // orgel-herlaad). We markeren de fout daarom als "verdacht" en herbouwen
    // pas als de callbacks ook echt stoppen. (start-tick, callback-baseline)
    let mut error_suspect: Option<(u64, u64)> = None;
    while running.load(Ordering::Relaxed) {
        thread::sleep(std::time::Duration::from_millis(100));
        ticks += 1;

        if stream_error.swap(false, Ordering::Relaxed)
            && error_suspect.is_none()
            && verify_since.is_none()
        {
            error_suspect = Some((ticks, cb_count.load(Ordering::Relaxed)));
        }
        // Verdachte stream-fout uitzoeken: lopen de callbacks door → tijdelijke
        // glitch, negeren. Staan ze ~0,8 s later nog stil → echte uitval, herbouw.
        if let Some((t0, baseline)) = error_suspect {
            if cb_count.load(Ordering::Relaxed) > baseline + 1 {
                error_suspect = None;
            } else if ticks.saturating_sub(t0) >= 8 {
                error_suspect = None;
                rebuild_pending = true;
                warn!("Audio-watchdog: stream-fout + callbacks gestopt — herbouw volgt");
            }
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
