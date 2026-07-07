//! Audio effects (tremulant, wind model)

use vpo_core::SampleRate;
use std::f32::consts::PI;

/// Low-frequency oscillator for tremulant and modulation
pub struct Lfo {
    phase: f32,
    frequency: f32,
    phase_inc: f32,
    waveform: LfoWaveform,
}

impl Lfo {
    pub fn new(frequency: f32, sample_rate: SampleRate, waveform: LfoWaveform) -> Self {
        let phase_inc = frequency / sample_rate as f32;
        Self { phase: 0.0, frequency, phase_inc, waveform }
    }

    pub fn set_frequency(&mut self, frequency: f32, sample_rate: SampleRate) {
        self.frequency = frequency;
        self.phase_inc = frequency / sample_rate as f32;
    }

    pub fn next(&mut self) -> f32 {
        let value = match self.waveform {
            LfoWaveform::Sine => (self.phase * 2.0 * PI).sin(),
            LfoWaveform::Triangle => {
                if self.phase < 0.5 { 4.0 * self.phase - 1.0 } 
                else { 3.0 - 4.0 * self.phase }
            }
            LfoWaveform::Square => if self.phase < 0.5 { 1.0 } else { -1.0 },
            LfoWaveform::Sawtooth => 2.0 * self.phase - 1.0,
        };
        self.phase += self.phase_inc;
        if self.phase >= 1.0 { self.phase -= 1.0; }
        value
    }

    pub fn reset(&mut self) { self.phase = 0.0; }
}

#[derive(Debug, Clone, Copy)]
pub enum LfoWaveform { Sine, Triangle, Square, Sawtooth }

/// Tremulant effect
pub struct Tremulant {
    pub active: bool,
    amp_lfo: Lfo,
    pitch_lfo: Lfo,
    pub amp_depth: f32,
    pub pitch_depth: f32,
    ramp: f32,
    ramp_rate: f32,
}

impl Tremulant {
    pub fn new(frequency: f32, sample_rate: SampleRate) -> Self {
        Self {
            active: false,
            amp_lfo: Lfo::new(frequency, sample_rate, LfoWaveform::Sine),
            pitch_lfo: Lfo::new(frequency, sample_rate, LfoWaveform::Sine),
            amp_depth: 0.1,
            pitch_depth: 10.0,
            ramp: 0.0,
            ramp_rate: 1.0 / (sample_rate as f32 * 0.5),
        }
    }

    pub fn set_speed(&mut self, frequency: f32, sample_rate: SampleRate) {
        self.amp_lfo.set_frequency(frequency, sample_rate);
        self.pitch_lfo.set_frequency(frequency, sample_rate);
    }

    pub fn process(&mut self) -> (f32, f32) {
        if self.active { self.ramp = (self.ramp + self.ramp_rate).min(1.0); }
        else { self.ramp = (self.ramp - self.ramp_rate).max(0.0); }
        if self.ramp <= 0.0 { return (1.0, 0.0); }
        let amp_mod = 1.0 + self.amp_lfo.next() * self.amp_depth * self.ramp;
        let pitch_mod = self.pitch_lfo.next() * self.pitch_depth * self.ramp;
        (amp_mod, pitch_mod)
    }
}

/// Wind model simulation
pub struct WindModel {
    pub enabled: bool,
    /// Current wind pressure (0.0-1.0 normalized, 1.0 = full pressure)
    pressure: f32,
    /// Reservoir size: larger = more stable, less sag (0.1-2.0)
    pub reservoir_size: f32,
    /// Damping coefficient: higher = less oscillation (0.0-1.0)
    pub damping: f32,
    /// Maximum pressure sag as fraction (0.0-0.3)
    pub max_sag: f32,
    /// Smoothing filter
    filter: OnePoleFilter,
    /// Current voice count on this division
    voice_count: u32,
    /// Noise generator state
    noise_state: u32,
}

impl WindModel {
    pub fn new(sample_rate: SampleRate) -> Self {
        // Damping of 0.5 → time constant ~200ms
        let cutoff = 5.0; // Hz, controls response speed
        Self {
            enabled: false,
            pressure: 1.0,
            reservoir_size: 0.5,
            damping: 0.5,
            max_sag: 0.10,
            filter: OnePoleFilter::new(cutoff, sample_rate),
            voice_count: 0,
            noise_state: 12345,
        }
    }

    pub fn configure(&mut self, reservoir_size: f32, damping: f32, max_sag: f32, sample_rate: SampleRate) {
        self.reservoir_size = reservoir_size.clamp(0.1, 2.0);
        self.damping = damping.clamp(0.0, 1.0);
        self.max_sag = max_sag.clamp(0.0, 0.30);
        // Damping controls filter cutoff: high damping = fast settling = higher cutoff
        let cutoff = 2.0 + damping * 15.0; // 2-17 Hz
        self.filter.set_cutoff(cutoff, sample_rate);
    }

    pub fn set_voice_count(&mut self, count: u32) { self.voice_count = count; }

    fn noise(&mut self) -> f32 {
        self.noise_state = self.noise_state.wrapping_mul(1103515245).wrapping_add(12345);
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Returns pressure ratio (0.7-1.0). Values below 1.0 = wind sag.
    pub fn process(&mut self) -> f32 {
        if !self.enabled { return 1.0; }
        // More voices = more wind consumption = more sag
        let consumption = self.voice_count as f32 / self.reservoir_size;
        let sag = (consumption * 0.02).min(self.max_sag);
        // Small random variation (wind turbulence)
        let noise = self.noise() * 0.002;
        let target = 1.0 - sag + noise;
        self.pressure = self.filter.process(target);
        self.pressure.clamp(1.0 - self.max_sag, 1.02)
    }
}

/// Simple one-pole low-pass filter
pub struct OnePoleFilter {
    state: f32,
    coeff: f32,
}

impl OnePoleFilter {
    pub fn new(cutoff_hz: f32, sample_rate: SampleRate) -> Self {
        let coeff = 1.0 - (-2.0 * PI * cutoff_hz / sample_rate as f32).exp();
        Self { state: 0.0, coeff }
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32, sample_rate: SampleRate) {
        self.coeff = 1.0 - (-2.0 * PI * cutoff_hz / sample_rate as f32).exp();
    }

    pub fn process(&mut self, input: f32) -> f32 {
        self.state += self.coeff * (input - self.state);
        self.state
    }

    pub fn reset(&mut self) { self.state = 0.0; }
}

/// Biquad filter for more precise filtering
pub struct BiquadFilter {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    x1: f32, x2: f32,
    y1: f32, y2: f32,
}

impl BiquadFilter {
    pub fn lowpass(cutoff_hz: f32, q: f32, sample_rate: SampleRate) -> Self {
        let omega = 2.0 * PI * cutoff_hz / sample_rate as f32;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * q);
        
        let b0 = (1.0 - cos_omega) / 2.0;
        let b1 = 1.0 - cos_omega;
        let b2 = (1.0 - cos_omega) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha;
        
        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    pub fn highpass(cutoff_hz: f32, q: f32, sample_rate: SampleRate) -> Self {
        let omega = 2.0 * PI * cutoff_hz / sample_rate as f32;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * q);
        
        let b0 = (1.0 + cos_omega) / 2.0;
        let b1 = -(1.0 + cos_omega);
        let b2 = (1.0 + cos_omega) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha;
        
        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
                   - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0; self.x2 = 0.0;
        self.y1 = 0.0; self.y2 = 0.0;
    }

    /// Peak/bell EQ filter for parametric EQ bands
    pub fn peaking_eq(center_hz: f32, gain_db: f32, q: f32, sample_rate: SampleRate) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let omega = 2.0 * PI * center_hz / sample_rate as f32;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * q);

        let b0 = 1.0 + alpha * a;
        let b1 = -2.0 * cos_omega;
        let b2 = 1.0 - alpha * a;
        let a0 = 1.0 + alpha / a;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha / a;

        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    /// Low-shelf filter
    pub fn low_shelf(cutoff_hz: f32, gain_db: f32, sample_rate: SampleRate) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let omega = 2.0 * PI * cutoff_hz / sample_rate as f32;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / 2.0 * ((a + 1.0 / a) * (1.0 / 0.9 - 1.0) + 2.0).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) - (a - 1.0) * cos_omega + two_sqrt_a_alpha);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos_omega);
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos_omega - two_sqrt_a_alpha);
        let a0 = (a + 1.0) + (a - 1.0) * cos_omega + two_sqrt_a_alpha;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos_omega);
        let a2 = (a + 1.0) + (a - 1.0) * cos_omega - two_sqrt_a_alpha;

        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }

    /// High-shelf filter
    pub fn high_shelf(cutoff_hz: f32, gain_db: f32, sample_rate: SampleRate) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let omega = 2.0 * PI * cutoff_hz / sample_rate as f32;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / 2.0 * ((a + 1.0 / a) * (1.0 / 0.9 - 1.0) + 2.0).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_omega + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_omega);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_omega - two_sqrt_a_alpha);
        let a0 = (a + 1.0) - (a - 1.0) * cos_omega + two_sqrt_a_alpha;
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_omega);
        let a2 = (a + 1.0) - (a - 1.0) * cos_omega - two_sqrt_a_alpha;

        Self {
            b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
            a1: a1 / a0, a2: a2 / a0,
            x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
        }
    }
}

/// Type van een vrije EQ-band (GrandOrgue-stijl).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EqBandType {
    Peak,
    LowPass,
    HighPass,
    BandPass,
    LowShelf,
    HighShelf,
}

impl EqBandType {
    /// Parse vanaf de wire-representatie (frontend stuurt strings).
    pub fn from_str(s: &str) -> Self {
        match s {
            "lowpass" => Self::LowPass,
            "highpass" => Self::HighPass,
            "bandpass" => Self::BandPass,
            "lowshelf" => Self::LowShelf,
            "highshelf" => Self::HighShelf,
            _ => Self::Peak,
        }
    }
}

/// Eén vrij instelbare EQ-band: type, frequentie, gain, bandbreedte (octaven)
/// en doelkanaal (None = alle uitgangskanalen).
#[derive(Debug, Clone, Copy)]
pub struct EqBandSpec {
    pub enabled: bool,
    pub band_type: EqBandType,
    pub freq: f32,
    pub gain_db: f32,
    /// Bandbreedte in octaven (RBJ-cookbook); bepaalt de Q van peak/band/pass-types.
    pub bandwidth_oct: f32,
    /// Fysiek uitgangskanaal (0-based); None = alle kanalen.
    pub channel: Option<u8>,
}

impl BiquadFilter {
    /// Bouw een biquad voor een EQ-band volgens de RBJ-cookbook, met alpha
    /// afgeleid van de bandbreedte in octaven:
    ///   alpha = sin(w0) * sinh( ln2/2 * BW * w0/sin(w0) )
    pub fn from_band_spec(spec: &EqBandSpec, sample_rate: SampleRate) -> Self {
        let freq = spec.freq.clamp(10.0, sample_rate as f32 * 0.45);
        let bw = spec.bandwidth_oct.clamp(0.05, 8.0);
        let omega = 2.0 * PI * freq / sample_rate as f32;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let ln2_2 = 0.5 * std::f32::consts::LN_2;
        let alpha = sin_omega * (ln2_2 * bw * omega / sin_omega).sinh();

        match spec.band_type {
            EqBandType::Peak => {
                let a = 10.0_f32.powf(spec.gain_db / 40.0);
                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * cos_omega;
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha / a;
                Self {
                    b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
                    a1: a1 / a0, a2: a2 / a0,
                    x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
                }
            }
            EqBandType::LowPass => {
                let b0 = (1.0 - cos_omega) / 2.0;
                let b1 = 1.0 - cos_omega;
                let b2 = (1.0 - cos_omega) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;
                Self {
                    b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
                    a1: a1 / a0, a2: a2 / a0,
                    x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
                }
            }
            EqBandType::HighPass => {
                let b0 = (1.0 + cos_omega) / 2.0;
                let b1 = -(1.0 + cos_omega);
                let b2 = (1.0 + cos_omega) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;
                Self {
                    b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
                    a1: a1 / a0, a2: a2 / a0,
                    x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
                }
            }
            EqBandType::BandPass => {
                // Constant 0 dB peak gain bandpass
                let b0 = alpha;
                let b1 = 0.0;
                let b2 = -alpha;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;
                Self {
                    b0: b0 / a0, b1: b1 / a0, b2: b2 / a0,
                    a1: a1 / a0, a2: a2 / a0,
                    x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0,
                }
            }
            EqBandType::LowShelf => Self::low_shelf(freq, spec.gain_db, sample_rate),
            EqBandType::HighShelf => Self::high_shelf(freq, spec.gain_db, sample_rate),
        }
    }
}

/// Vrije multi-band EQ voor één audiokanaal: een serieschakeling van biquads,
/// gebouwd uit de banden die op dit kanaal van toepassing zijn.
pub struct ChannelEq {
    filters: Vec<BiquadFilter>,
}

impl ChannelEq {
    /// Bouw de keten voor kanaal `channel` uit `bands` (banden met een ander
    /// expliciet kanaal worden overgeslagen; None = alle kanalen).
    pub fn build(bands: &[EqBandSpec], channel: u8, sample_rate: SampleRate) -> Self {
        let filters = bands.iter()
            .filter(|b| b.enabled && b.channel.map_or(true, |c| c == channel))
            .map(|b| BiquadFilter::from_band_spec(b, sample_rate))
            .collect();
        Self { filters }
    }

    #[inline]
    pub fn is_empty(&self) -> bool { self.filters.is_empty() }

    #[inline]
    pub fn process(&mut self, input: f32) -> f32 {
        let mut x = input;
        for f in &mut self.filters {
            x = f.process(x);
        }
        x
    }
}

/// 3-band Parametric EQ (Low shelf + Mid peak + High shelf)
pub struct ParametricEq {
    pub enabled: bool,
    low: BiquadFilter,
    mid: BiquadFilter,
    high: BiquadFilter,
}

impl ParametricEq {
    pub fn new(sample_rate: SampleRate) -> Self {
        Self {
            enabled: false,
            low: BiquadFilter::low_shelf(200.0, 0.0, sample_rate),
            mid: BiquadFilter::peaking_eq(1000.0, 0.0, 1.0, sample_rate),
            high: BiquadFilter::high_shelf(4000.0, 0.0, sample_rate),
        }
    }

    pub fn configure(&mut self, low_freq: f32, low_gain: f32, mid_freq: f32, mid_gain: f32, mid_q: f32, high_freq: f32, high_gain: f32, sample_rate: SampleRate) {
        self.low = BiquadFilter::low_shelf(low_freq, low_gain, sample_rate);
        self.mid = BiquadFilter::peaking_eq(mid_freq, mid_gain, mid_q, sample_rate);
        self.high = BiquadFilter::high_shelf(high_freq, high_gain, sample_rate);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        if !self.enabled { return input; }
        let x = self.low.process(input);
        let y = self.mid.process(x);
        self.high.process(y)
    }

    pub fn reset(&mut self) {
        self.low.reset();
        self.mid.reset();
        self.high.reset();
    }
}
