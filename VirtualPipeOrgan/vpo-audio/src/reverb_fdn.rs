//! 8-channel Feedback Delay Network (FDN) algorithmic reverb
//!
//! Designed for organ acoustics: long RT60, natural decay, HF damping.
//! Uses Hadamard mixing matrix for dense echo distribution.

use vpo_core::SampleRate;

/// Allpass filter for input diffusion
struct AllpassFilter {
    buffer: Vec<f32>,
    pos: usize,
    gain: f32,
}

impl AllpassFilter {
    fn new(delay_samples: usize, gain: f32) -> Self {
        Self {
            buffer: vec![0.0; delay_samples.max(1)],
            pos: 0,
            gain,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.pos];
        let temp = input - self.gain * delayed;
        self.buffer[self.pos] = temp;
        self.pos = (self.pos + 1) % self.buffer.len();
        delayed + self.gain * temp
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.pos = 0;
    }
}

/// One-pole low-pass filter for HF damping
struct DampingFilter {
    state: f32,
    coeff: f32, // 0.0 = no filtering, 1.0 = full bypass
}

impl DampingFilter {
    fn new(damping: f32) -> Self {
        // damping: 0.0 = no HF cut, 1.0 = maximum HF cut
        let coeff = 1.0 - damping.clamp(0.0, 0.95);
        Self { state: 0.0, coeff }
    }

    fn set_damping(&mut self, damping: f32) {
        self.coeff = 1.0 - damping.clamp(0.0, 0.95);
    }

    fn process(&mut self, input: f32) -> f32 {
        self.state = self.state + self.coeff * (input - self.state);
        self.state
    }

    fn clear(&mut self) { self.state = 0.0; }
}

/// Delay line lengths (in samples at 48kHz). Prime numbers for minimal periodicity.
const DELAY_PRIMES_48K: [usize; 8] = [
    1087, 1283, 1531, 1789, 2053, 2311, 2617, 2927,
];

/// 8-channel Feedback Delay Network reverb
pub struct FdnReverb {
    delay_lines: [Vec<f32>; 8],
    delay_pos: [usize; 8],
    delay_lengths: [usize; 8],
    feedback_gains: [f32; 8],
    damping_filters: [DampingFilter; 8],
    input_diffusers: [AllpassFilter; 4],
    pre_delay: Vec<f32>,
    pre_delay_pos: usize,
    pre_delay_len: usize,
    pub mix: f32,
    pub rt60: f32,
    pub damping: f32,
    pub room_size: f32,
    sample_rate: f32,
}

impl FdnReverb {
    pub fn new(sample_rate: SampleRate) -> Self {
        let sr = sample_rate as f32;
        let room_size = 1.0;
        let rt60 = 2.0;
        let damping = 0.5;

        let delay_lengths = Self::scale_delays(room_size, sr);
        let feedback_gains = Self::compute_feedback(rt60, &delay_lengths, sr);

        let delay_lines = core::array::from_fn(|i| vec![0.0; delay_lengths[i]]);
        let damping_filters = core::array::from_fn(|_| DampingFilter::new(damping));

        // Input diffusers with prime-length delays
        let input_diffusers = [
            AllpassFilter::new((142.0 * sr / 48000.0) as usize, 0.7),
            AllpassFilter::new((107.0 * sr / 48000.0) as usize, 0.7),
            AllpassFilter::new((379.0 * sr / 48000.0) as usize, 0.7),
            AllpassFilter::new((277.0 * sr / 48000.0) as usize, 0.7),
        ];

        let pre_delay_ms = 25.0;
        let pre_delay_len = (pre_delay_ms * sr / 1000.0) as usize;

        Self {
            delay_lines,
            delay_pos: [0; 8],
            delay_lengths,
            feedback_gains,
            damping_filters,
            input_diffusers,
            pre_delay: vec![0.0; pre_delay_len.max(1)],
            pre_delay_pos: 0,
            pre_delay_len,
            mix: 0.3,
            rt60,
            damping,
            room_size,
            sample_rate: sr,
        }
    }

    fn scale_delays(room_size: f32, sample_rate: f32) -> [usize; 8] {
        let ratio = sample_rate / 48000.0;
        core::array::from_fn(|i| {
            ((DELAY_PRIMES_48K[i] as f32 * ratio * room_size) as usize).max(1)
        })
    }

    fn compute_feedback(rt60: f32, delay_lengths: &[usize; 8], sample_rate: f32) -> [f32; 8] {
        // feedback = 10^(-3 * delay_length / (rt60 * sample_rate))
        // This gives -60dB decay at rt60 seconds
        core::array::from_fn(|i| {
            let delay_sec = delay_lengths[i] as f32 / sample_rate;
            10.0_f32.powf(-3.0 * delay_sec / rt60.max(0.1))
        })
    }

    pub fn configure(&mut self, rt60: f32, pre_delay_ms: f32, damping: f32, room_size: f32, mix: f32) {
        self.rt60 = rt60.clamp(0.3, 15.0);
        self.damping = damping.clamp(0.0, 0.95);
        self.room_size = room_size.clamp(0.3, 3.0);
        self.mix = mix.clamp(0.0, 1.0);

        // Resize delay lines
        self.delay_lengths = Self::scale_delays(self.room_size, self.sample_rate);
        for i in 0..8 {
            self.delay_lines[i].resize(self.delay_lengths[i], 0.0);
            self.delay_lines[i].fill(0.0);
            self.delay_pos[i] = 0;
        }

        // Update feedback gains
        self.feedback_gains = Self::compute_feedback(self.rt60, &self.delay_lengths, self.sample_rate);

        // Update damping
        for f in &mut self.damping_filters {
            f.set_damping(self.damping);
        }

        // Update pre-delay
        self.pre_delay_len = ((pre_delay_ms.clamp(0.0, 150.0) * self.sample_rate / 1000.0) as usize).max(1);
        self.pre_delay.resize(self.pre_delay_len, 0.0);
        self.pre_delay.fill(0.0);
        self.pre_delay_pos = 0;

        // Clear diffusers
        for d in &mut self.input_diffusers {
            d.clear();
        }
    }

    /// Apply a preset configuration
    pub fn apply_preset(&mut self, preset: u8, mix: f32) {
        let (rt60, pre_delay, damping, room_size) = match preset {
            0 => (1.2, 10.0, 0.6, 0.5),   // Kleine Kapel
            1 => (2.0, 25.0, 0.5, 0.8),   // Dorpskerk
            2 => (3.5, 40.0, 0.4, 1.2),   // Grote Kerk
            3 => (6.0, 60.0, 0.3, 1.8),   // Kathedraal
            4 => (10.0, 80.0, 0.2, 2.5),  // Gotische Kathedraal
            5 => (2.2, 30.0, 0.5, 1.0),   // Concertzaal
            _ => (2.0, 25.0, 0.5, 1.0),   // Standaard
        };
        self.configure(rt60, pre_delay, damping, room_size, mix);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        // Pre-delay
        let pre_out = self.pre_delay[self.pre_delay_pos];
        self.pre_delay[self.pre_delay_pos] = input;
        self.pre_delay_pos = (self.pre_delay_pos + 1) % self.pre_delay_len;

        // Input diffusion (smear the input for density)
        let mut diffused = pre_out;
        for d in &mut self.input_diffusers {
            diffused = d.process(diffused);
        }

        // Read from delay lines
        let mut taps = [0.0f32; 8];
        for i in 0..8 {
            taps[i] = self.delay_lines[i][self.delay_pos[i]];
        }

        // Hadamard-like mixing (normalized orthogonal matrix)
        // Using simple butterfly structure for 8 channels
        let scale = 0.3536; // 1/sqrt(8)
        let mixed = [
            (taps[0] + taps[1] + taps[2] + taps[3] + taps[4] + taps[5] + taps[6] + taps[7]) * scale,
            (taps[0] - taps[1] + taps[2] - taps[3] + taps[4] - taps[5] + taps[6] - taps[7]) * scale,
            (taps[0] + taps[1] - taps[2] - taps[3] + taps[4] + taps[5] - taps[6] - taps[7]) * scale,
            (taps[0] - taps[1] - taps[2] + taps[3] + taps[4] - taps[5] - taps[6] + taps[7]) * scale,
            (taps[0] + taps[1] + taps[2] + taps[3] - taps[4] - taps[5] - taps[6] - taps[7]) * scale,
            (taps[0] - taps[1] + taps[2] - taps[3] - taps[4] + taps[5] - taps[6] + taps[7]) * scale,
            (taps[0] + taps[1] - taps[2] - taps[3] - taps[4] - taps[5] + taps[6] + taps[7]) * scale,
            (taps[0] - taps[1] - taps[2] + taps[3] - taps[4] + taps[5] + taps[6] - taps[7]) * scale,
        ];

        // Write back: feedback * damping + input distribution
        let input_dist = diffused * 0.125; // distribute input to all 8 lines
        for i in 0..8 {
            let fb = self.feedback_gains[i] * mixed[i];
            let damped = self.damping_filters[i].process(fb);
            self.delay_lines[i][self.delay_pos[i]] = damped + input_dist;
            self.delay_pos[i] = (self.delay_pos[i] + 1) % self.delay_lengths[i];
        }

        // Output: sum of first 4 taps (for density)
        let wet = (taps[0] + taps[1] + taps[2] + taps[3]) * 0.25;

        // Mix dry/wet
        input * (1.0 - self.mix) + wet * self.mix
    }

    pub fn clear(&mut self) {
        for dl in &mut self.delay_lines {
            dl.fill(0.0);
        }
        self.delay_pos = [0; 8];
        for f in &mut self.damping_filters {
            f.clear();
        }
        for d in &mut self.input_diffusers {
            d.clear();
        }
        self.pre_delay.fill(0.0);
        self.pre_delay_pos = 0;
    }
}
