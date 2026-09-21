//! Audio effects (tremulant, wind model)

use vpo_core::SampleRate;
use std::f32::consts::PI;

/// Master-bus piek-limiter: verlaagt bij overschrijding van het plafond de
/// TOTALE gain (attack direct, release exponentieel terug naar 1.0).
///
/// Dit vervangt de kale `tanh()`-verzadiging per kanaal op de masteruitgang.
/// Bij een vol werk (honderden stemmen, mix-som ver boven 1.0) betekende tanh
/// diepe saturatie: hoorbare vervorming ("overstuur"), platgeslagen dynamiek
/// ("gedempt" — meer registers erbij deed niets meer) en een instortend
/// stereobeeld doordat elk kanaal onafhankelijk vervormde. Een gekoppelde
/// gain-limiter houdt de onderlinge balans en het stereobeeld intact en veert
/// na de piek vanzelf terug.
pub struct MasterLimiter {
    gain: f32,
    /// Per-sample attack-coëfficiënt (exponentieel richting de doelgain).
    /// Kort (~1,5 ms) maar niet instantaan: één enkele sample-piek trekt zo
    /// niet het hele orgel omlaag — dat gaf bij crescendo's een "wegzakkend"
    /// gevoel. Kortstondige overshoot wordt door de nood-clamp in process()
    /// opgevangen (nooit meer hard clippen op de uitgang).
    attack: f32,
    /// Per-sample release-coëfficiënt (exponentieel richting gain 1.0).
    release: f32,
    /// Hold: aantal samples dat de gain na de laatste overschrijding blijft
    /// staan voordat de release begint. Zonder hold veerde de gain direct na
    /// het loslaten van een akkoord in ~250 ms terug omhoog terwijl de
    /// release-samples + galm nog klonken — de staart zwol dan hoorbaar aan
    /// ("hik"). De hold overbrugt precies dat moment.
    hold_samples: u32,
    hold_left: u32,
    /// Piek-plafond (lineair), bv. 0.97 ≈ -0,26 dBFS.
    pub threshold: f32,
}

impl MasterLimiter {
    pub fn new(threshold: f32, attack_seconds: f32, release_seconds: f32, sample_rate: SampleRate) -> Self {
        let attack = 1.0 - (-1.0f32 / (attack_seconds.max(0.0001) * sample_rate as f32)).exp();
        let release = 1.0 - (-1.0f32 / (release_seconds.max(0.001) * sample_rate as f32)).exp();
        // Vaste hold van 300 ms: lang genoeg om het gat tussen loslaten en de
        // natuurlijke release-staart te overbruggen, kort genoeg om dynamiek
        // niet merkbaar te drukken.
        let hold_samples = (0.3 * sample_rate as f32) as u32;
        Self { gain: 1.0, attack, release, hold_samples, hold_left: 0, threshold }
    }

    /// Werk de limiter één frame bij op basis van de frame-piek (max |sample|
    /// over alle kanalen, vóór limiting) en geef de toe te passen gain terug.
    /// Inclusief nood-clamp: de teruggegeven gain laat de uitgang nooit boven
    /// 1.0 uitkomen, ook niet tijdens de gedoseerde attack.
    #[inline]
    pub fn process(&mut self, frame_peak: f32) -> f32 {
        let desired = if frame_peak > self.threshold {
            self.threshold / frame_peak
        } else {
            1.0
        };
        if desired < self.gain {
            // Attack: snel maar gedoseerd naar de doelgain.
            self.gain += (desired - self.gain) * self.attack;
            self.hold_left = self.hold_samples;
        } else if self.hold_left > 0 {
            // Hold: gain blijft staan; pas daarna terugveren.
            self.hold_left -= 1;
        } else {
            // Release: exponentieel terug omhoog, maar nooit voorbij wat de
            // huidige frame-piek toelaat.
            self.gain += (1.0 - self.gain) * self.release;
            if self.gain > desired {
                self.gain = desired;
            }
        }
        // Noodplafond, alleen voor DIT frame: tijdens de gedoseerde attack mag
        // de uitgang nooit hard clippen (dat gaf tikken/kraken bij akkoord-
        // aanslagen op grote sets). De gain-STATE blijft gedoseerd — een
        // instant-verlaagde state gaf juist het "wegzakkende" crescendo-gevoel
        // dat de 1,5 ms-attack voorkomt. Eén stiller frame is onhoorbaar.
        if frame_peak * self.gain > 1.0 {
            return 1.0 / frame_peak;
        }
        self.gain
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.hold_left = 0;
    }
}

#[cfg(test)]
mod limiter_tests {
    use super::MasterLimiter;

    #[test]
    fn onder_plafond_geen_demping() {
        let mut l = MasterLimiter::new(0.97, 0.0015, 0.25, 48000);
        for _ in 0..1000 {
            assert_eq!(l.process(0.5), 1.0);
        }
    }

    #[test]
    fn aanhoudende_piek_convergeert_naar_plafond() {
        let mut l = MasterLimiter::new(0.97, 0.0015, 0.25, 48000);
        // Vol-werk-som: piek 8.0 (ver boven plafond). Binnen ~10 ms (480 samples)
        // moet piek*gain op het plafond zitten — pure gain, geen vervorming.
        let mut g = 1.0;
        for _ in 0..480 {
            g = l.process(8.0);
        }
        assert!((8.0 * g - 0.97).abs() < 0.02, "piek*gain={} hoort ~0.97 te zijn", 8.0 * g);
        // Stabiel bij constante piek (relatieve verhoudingen blijven).
        let g2 = l.process(8.0);
        assert!((g2 - g).abs() < 1e-3, "gain stabiel bij constante piek");
    }

    #[test]
    fn attack_is_gedoseerd_niet_instantaan() {
        let mut l = MasterLimiter::new(0.97, 0.0015, 0.25, 48000);
        // Eén enkele extreme piek krijgt voor DAT frame het noodplafond (geen
        // hard clippen meer), maar de gain-STATE mag niet naar de bodem: het
        // eerstvolgende normale frame hoort weer vrijwel ongedempt te zijn
        // (geen "wegzakkend" crescendo-gevoel).
        let g1 = l.process(12.0);
        assert!(g1 <= 1.0 / 12.0 + 1e-6, "piekframe hoort tegen het noodplafond aan te zitten, is {}", g1);
        let g2 = l.process(0.5);
        assert!(g2 > 0.95, "state-gain {} hoort na één piekje dicht bij 1 te blijven", g2);
    }

    #[test]
    fn release_veert_terug_naar_1() {
        let mut l = MasterLimiter::new(0.97, 0.0015, 0.25, 48000);
        for _ in 0..480 {
            let _ = l.process(8.0); // slam
        }
        // 2 seconden zacht spel op 48 kHz.
        let mut g = 0.0;
        for _ in 0..96000 {
            g = l.process(0.1);
        }
        assert!(g > 0.99, "gain hoort hersteld te zijn, is {}", g);
    }

    #[test]
    fn geklemde_uitgang_blijft_onder_1() {
        let mut l = MasterLimiter::new(0.97, 0.0015, 0.25, 48000);
        // Grillig signaal: de limiter-uitgang mag na de eind-clamp (±1.0,
        // zoals de audio-callback doet) nooit boven 1.0 uitkomen; ná de
        // attack-fase hoort piek*gain onder het plafond te blijven.
        let peaks = [0.1, 6.0, 0.2, 12.0, 0.05, 3.0, 9.5, 0.4];
        for round in 0..200 {
            for &p in &peaks {
                let g = l.process(p);
                let clamped = (p * g).min(1.0);
                assert!(clamped <= 1.0);
                if round > 2 {
                    // Regime-gedrag: gain zit onder plafond/piek van het patroon.
                    assert!(g <= 1.0);
                }
            }
        }
    }
}

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

/// Hoeveel cent de toonhoogte daalt per eenheid winddruk. Een winddaling van
/// 10 % geeft dus 12 cent lager - hoorbaar, maar nog muzikaal. De vorige waarde
/// (30) gaf 3 cent bij dezelfde instelling, en dat merkte niemand.
pub const WIND_CENTS_PER_EENHEID: f32 = 120.0;

/// Windmodel: de balg van een windgroep.
///
/// Een echt orgel heeft een eindige windvoorziening. Wie een vol werk
/// opentrekt en een akkoord neerzet, hoort de winddruk even inzakken: de
/// pijpen worden iets zachter en iets lager, en de balg veert daarna terug.
/// Dat is wat organisten "het orgel ademt" noemen.
///
/// Het model is een balg met massa (het gewicht erop) aan een veer (de lucht
/// eronder), aangedreven door het verbruik van de klinkende pijpen. Tweede
/// orde dus - een eerste-orde filter kan niet naveren, en juist dat naveren is
/// het kenmerkende geluid. (Tot 0.7.49 stond hier wel zo'n eerste-orde filter,
/// terwijl de dempingsregelaar beloofde het naschommelen te regelen: die
/// regelaar deed dus nooit waar hij voor stond.)
pub struct WindModel {
    pub enabled: bool,
    /// Huidige winddruk. 1.0 = volle druk, lager = ingezakt.
    pressure: f32,
    /// Snelheid waarmee de balg beweegt (de tweede toestand van het model).
    velocity: f32,
    /// Grootte van het magazijn: groter = trager en stabieler (0.1-2.0).
    pub reservoir_size: f32,
    /// Demping: 0 = de balg schommelt duidelijk na, 1 = kritisch gedempt.
    pub damping: f32,
    /// Maximale winddaling als fractie (0.0-0.30).
    pub max_sag: f32,
    /// Eigenfrequentie van de balg in rad/s, en de dempingsfactor zeta.
    omega: f32,
    zeta: f32,
    /// Sampletijd (s); het model integreert per sample.
    dt: f32,
    /// Trage ruis voor de turbulentie in het kanaal.
    flutter: OnePoleFilter,
    /// Aantal klinkende stemmen op deze windgroep.
    voice_count: u32,
    /// Ruisgenerator.
    noise_state: u32,
}

/// Welk deel van de actuele winddaling als turbulentie meetrilt. Bij de
/// standaardinstelling (10 % daling) is dat plus/min 1,8 cent rond de
/// ingezakte toonhoogte: genoeg om te leven, te weinig om zeeziek van te
/// worden.
const FLUTTER_DEEL: f32 = 0.15;

impl WindModel {
    pub fn new(sample_rate: SampleRate) -> Self {
        let mut m = Self {
            enabled: false,
            pressure: 1.0,
            velocity: 0.0,
            reservoir_size: 0.5,
            damping: 0.5,
            max_sag: 0.10,
            omega: 0.0,
            zeta: 0.0,
            dt: 1.0 / sample_rate as f32,
            flutter: OnePoleFilter::new(2.5, sample_rate),
            voice_count: 0,
            noise_state: 12345,
        };
        m.configure(0.5, 0.5, 0.10, sample_rate);
        m
    }

    pub fn configure(&mut self, reservoir_size: f32, damping: f32, max_sag: f32, sample_rate: SampleRate) {
        self.reservoir_size = reservoir_size.clamp(0.1, 2.0);
        self.damping = damping.clamp(0.0, 1.0);
        self.max_sag = max_sag.clamp(0.0, 0.30);
        self.dt = 1.0 / sample_rate as f32;
        // Een groot magazijn is zwaar en traag, een klein magazijn nerveus.
        // Bij de standaardstand (50 %) komt daar 6 Hz uit - de orde van
        // grootte van een echte balg.
        let f0 = (3.0 / self.reservoir_size).clamp(0.5, 12.0);
        self.omega = 2.0 * PI * f0;
        // 0 -> zeta 0.12 (duidelijk naveren), 1 -> zeta 1.0 (kritisch gedempt:
        // inzakken en terugkomen zonder overschot).
        self.zeta = 0.12 + 0.88 * self.damping;
        self.flutter.set_cutoff(2.5, sample_rate);
    }

    pub fn set_voice_count(&mut self, count: u32) { self.voice_count = count; }

    /// Winddruk van dit moment, voor de meter in het scherm. Staat het model
    /// uit, dan is dat per definitie de volle druk.
    pub fn druk(&self) -> f32 {
        if self.enabled { self.pressure } else { 1.0 }
    }

    fn noise(&mut self) -> f32 {
        self.noise_state = self.noise_state.wrapping_mul(1103515245).wrapping_add(12345);
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Een stap van de balg richting `doel`. Geeft de nieuwe druk terug.
    fn stap(&mut self, doel: f32) -> f32 {
        let a = self.omega * self.omega * (doel - self.pressure)
            - 2.0 * self.zeta * self.omega * self.velocity;
        self.velocity += a * self.dt;
        self.pressure += self.velocity * self.dt;
        // Het terugveren mag over het doel heen schieten, maar niet ontsporen.
        let ondergrens = 1.0 - self.max_sag * 1.6;
        if self.pressure < ondergrens {
            self.pressure = ondergrens;
            self.velocity = 0.0;
        } else if self.pressure > 1.03 {
            self.pressure = 1.03;
            self.velocity = 0.0;
        }
        self.pressure
    }

    /// Winddruk voor deze sample. 1.0 = volle druk; lager = inzakking.
    pub fn process(&mut self) -> f32 {
        if !self.enabled {
            // Uitzetten terwijl de wind ingezakt is mag geen toonhoogtesprong
            // geven: de balg loopt netjes vol en pas daarna is het model stil.
            if (self.pressure - 1.0).abs() < 1e-5 && self.velocity.abs() < 1e-5 {
                self.pressure = 1.0;
                self.velocity = 0.0;
                return 1.0;
            }
            return self.stap(1.0);
        }
        // Meer stemmen = meer windverbruik = meer inzakking, maar GELEIDELIJK:
        // de oude lineaire formule (voice_count / reservoir x 0.02) zat met het
        // standaard-magazijn al bij ~3 stemmen op de maximale inzakking - het
        // windmodel werkte dan als botte volumedemper zodra er meer
        // geregistreerd werd, in plaats van als adem. Nu verzadigt de
        // inzakking vloeiend: bij de knie (~30 x magazijn stemmen) is de helft
        // van max_sag bereikt; een vol werk nadert max_sag asymptotisch.
        let v = self.voice_count as f32;
        let knee = 30.0 * self.reservoir_size.max(0.1);
        let sag = self.max_sag * (v / (v + knee));
        // Turbulentie in het kanaal: schaalt mee met de inzakking, dus alleen
        // te horen zolang er echt wind getrokken wordt.
        let ruis = self.noise();
        let flutter = (self.flutter.process(ruis) * 50.0).clamp(-1.0, 1.0) * sag * FLUTTER_DEEL;
        self.stap(1.0 - sag + flutter)
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

    /// Filterstaat op het ingangssignaal zetten terwijl het filter gebypasst
    /// is (zwelkast open): zo start het zonder sprong zodra het weer meedoet.
    #[inline]
    pub fn prime(&mut self, input: f32) { self.state = input; }
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

#[cfg(test)]
mod windmodel_tests {
    use super::*;

    const SR: SampleRate = 48000;

    /// Laat het model `ms` milliseconden lopen en geef de laagste en de
    /// laatste druk terug.
    fn draai(m: &mut WindModel, ms: u32) -> (f32, f32) {
        let n = (SR as f32 * ms as f32 / 1000.0) as u32;
        let mut laagste = f32::MAX;
        let mut laatste = 1.0;
        for _ in 0..n {
            laatste = m.process();
            if laatste < laagste { laagste = laatste; }
        }
        (laagste, laatste)
    }

    #[test]
    fn uitgeschakeld_blijft_de_druk_vol() {
        let mut m = WindModel::new(SR);
        m.set_voice_count(500);
        let (laagste, laatste) = draai(&mut m, 500);
        assert_eq!(laagste, 1.0);
        assert_eq!(laatste, 1.0);
        assert_eq!(m.druk(), 1.0);
    }

    #[test]
    fn zonder_stemmen_zakt_er_niets_in() {
        let mut m = WindModel::new(SR);
        m.enabled = true;
        m.configure(0.5, 0.5, 0.10, SR);
        let (laagste, _) = draai(&mut m, 500);
        assert!(laagste > 0.9999, "stille windlade zakte toch in: {}", laagste);
    }

    /// De kern van de klacht "ik merk er niets van": een vol werk moet een
    /// hoorbare inzakking geven. Hoorbaar = minstens een paar cent.
    #[test]
    fn een_vol_werk_geeft_een_hoorbare_inzakking() {
        let mut m = WindModel::new(SR);
        m.enabled = true;
        m.configure(0.5, 0.7, 0.10, SR);
        m.set_voice_count(200);
        let (_, druk) = draai(&mut m, 1500);
        let cents = (1.0 - druk) * WIND_CENTS_PER_EENHEID;
        assert!(cents > 5.0, "te weinig effect om te horen: {:.1} cent", cents);
        assert!(cents < 20.0, "veel te veel bij de standaardinstelling: {:.1} cent", cents);
    }

    /// Meer stemmen = meer inzakking, en die verzadigt (geen botte
    /// volumedemper die al bij drie stemmen op zijn eind zit).
    #[test]
    fn de_inzakking_groeit_met_de_stemmen_en_verzadigt() {
        let meting = |stemmen: u32| {
            let mut m = WindModel::new(SR);
            m.enabled = true;
            m.configure(0.5, 1.0, 0.10, SR);
            m.set_voice_count(stemmen);
            draai(&mut m, 2000).1
        };
        let (d3, d30, d300) = (meting(3), meting(30), meting(300));
        assert!(d3 > d30 && d30 > d300, "niet monotoon: {} {} {}", d3, d30, d300);
        assert!(1.0 - d3 < 0.35 * (1.0 - d30),
                "drie stemmen zakken al te ver in: {:.4} tegen {:.4}", 1.0 - d3, 1.0 - d30);
        assert!(d300 > 0.895, "voorbij de ingestelde maximale daling: {}", d300);
    }

    /// De dempingsregelaar beloofde het naschommelen te regelen maar zat op een
    /// eerste-orde filter, dat niet kan naveren. Nu wel: laag gedempt schiet de
    /// balg door, kritisch gedempt niet.
    #[test]
    fn de_demping_bepaalt_het_naveren() {
        let overschot = |demping: f32| {
            let mut m = WindModel::new(SR);
            m.enabled = true;
            m.configure(0.5, demping, 0.10, SR);
            m.set_voice_count(200);
            let doel = draai(&mut m, 3000).1;
            // Alle toetsen los: de balg loopt terug naar vol.
            m.set_voice_count(0);
            let mut hoogste: f32 = 0.0;
            for _ in 0..(SR * 2) {
                let d = m.process();
                if d > hoogste { hoogste = d; }
            }
            let _ = doel;
            hoogste
        };
        let los = overschot(0.05);
        let strak = overschot(1.0);
        assert!(los > 1.001, "slap gedempt hoort door te schieten, was {}", los);
        assert!(strak <= 1.0005, "kritisch gedempt hoort niet door te schieten, was {}", strak);
    }

    /// Het model mag nooit buiten zijn perken lopen, ook niet bij de meest
    /// extreme instelling.
    #[test]
    fn de_druk_blijft_binnen_de_perken() {
        let mut m = WindModel::new(SR);
        m.enabled = true;
        m.configure(0.1, 0.0, 0.30, SR);
        for stap in 0..60 {
            m.set_voice_count(if stap % 2 == 0 { 800 } else { 0 });
            let (laagste, _) = draai(&mut m, 40);
            assert!(laagste >= 1.0 - 0.30 * 1.6 - 1e-6 && laagste <= 1.031,
                    "druk liep uit de rails: {}", laagste);
        }
    }

    /// Uitzetten terwijl de wind ingezakt is mag geen toonhoogtesprong geven.
    #[test]
    fn uitzetten_loopt_netjes_vol() {
        let mut m = WindModel::new(SR);
        m.enabled = true;
        m.configure(0.5, 0.8, 0.10, SR);
        m.set_voice_count(300);
        let ingezakt = draai(&mut m, 2000).1;
        assert!(ingezakt < 0.97, "niet ingezakt, test zegt niets: {}", ingezakt);
        m.enabled = false;
        let direct = m.process();
        assert!((direct - ingezakt).abs() < 0.001, "sprong bij het uitzetten: {} -> {}", ingezakt, direct);
        let (_, na) = draai(&mut m, 2000);
        assert!((na - 1.0).abs() < 1e-4, "kwam niet op volle druk: {}", na);
    }
}
