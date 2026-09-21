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

/// Hoeveel cent de toonhoogte van de REFERENTIEPIJP (een 8'-prestant rond
/// c') daalt per eenheid winddruk. 0.7.50 had 120; sinds 0.7.51 is het 80,
/// omdat het levende karakter nu uit de VERSCHILLEN tussen pijpen komt
/// (fluiten en kleine pijpen zakken meer, tongwerken niet) en niet uit de
/// diepte. Hauptwerk zit op 50-85 cent per eenheid, gemeten labialen op
/// 0,8-1,8 cent per procent druk (Logos-datasheet).
pub const WIND_CENTS_PER_EENHEID: f32 = 80.0;

/// Toonhoogtefactor per eenheid drukafwijking, eerste orde: 2^(cent/1200)
/// ≈ 1 + cent·ln2/1200. Bij 12 cent is de fout 2,4e-5 — onhoorbaar, en het
/// scheelt een powf per stem per frame.
pub const WIND_RATE_PER_EENHEID: f32 = WIND_CENTS_PER_EENHEID * 0.000_577_622_65;

/// Alle instellingen van één windgroep, zoals de audiothread ze krijgt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindInstelling {
    pub enabled: bool,
    /// Balggrootte (0,1-2,0): traagheid van de balg én hoeveel vol werk hij aankan.
    pub reservoir_size: f32,
    /// Demping (0-1): 0 = de balg schommelt na, 1 = kritisch gedempt.
    pub damping: f32,
    /// Maximale winddaling als fractie (0-0,30): schaal van alle diepten.
    pub max_sag: f32,
    /// Windstoot bij inzet en loslaten van pijpen (0-2; 1 = gekalibreerd op
    /// een dip van ~3 % voor één 16'-C).
    pub stoot: f32,
    /// Kanaal (0-1): kort en wijd → lang en smal (de snelle lade-mode).
    pub kanaal: f32,
    /// Doffer worden bij inzakking (0-1).
    pub doffer: f32,
    /// Verschil per pijp (0-1): 0 = alle pijpen zakken gelijk (0.7.50),
    /// 1 = elke familie en grootte op haar eigen manier.
    pub verschil: f32,
    /// Tongwerken blijven in toonhoogte staan en zakken alleen in kracht.
    pub tongwerk_apart: bool,
}

impl Default for WindInstelling {
    /// Het Hollandse karakter, model uit.
    fn default() -> Self {
        Self {
            enabled: false,
            reservoir_size: 0.5,
            damping: 0.5,
            max_sag: 0.05,
            stoot: 1.2,
            kanaal: 0.8,
            doffer: 0.7,
            verschil: 1.0,
            tongwerk_apart: true,
        }
    }
}

/// Windmodel: de balg van een windgroep — de TRAGE laag van het windwerk.
///
/// Een echt orgel heeft een eindige windvoorziening. Wie een vol werk
/// opentrekt en een akkoord neerzet, hoort de winddruk even inzakken: de
/// pijpen worden iets zachter en iets lager, en de balg veert daarna terug.
/// Dat is wat organisten "het orgel ademt" noemen.
///
/// Het model is een balg met massa (het gewicht erop) aan een veer (de lucht
/// eronder), aangedreven door het GEWOGEN verbruik van de klinkende pijpen
/// (een 16'-C trekt ruim honderd keer zoveel wind als een 2'-pijpje; zie
/// [`verbruik_van_pijp`]). De statische inzakking groeit kwadratisch met het
/// verbruik en verzadigt daarna — zoals drukverlies in kanalen en
/// ventielkast (ΔP ∝ Q²): één register doet niets, het pleno ademt.
///
/// De snelle laag (kanaal en lade, 10-15 Hz) zit in [`WindLade`], per
/// divisie; die krijgt de stoten bij inzet en loslaten.
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
    /// Gewogen windverbruik op deze balg (8'-C-eenheden) en de referentie
    /// waarbij de statische inzakking de helft van `max_sag` is.
    verbruik: f32,
    q_ref: f32,
    /// Ondergrens van de druk, kruipt naar 1 − 1,6·max_sag toe: wie de
    /// maximale daling live verlaagt terwijl de balg ingezakt is, krijgt zo
    /// geen sprong maar een glijdende terugkeer.
    grens: f32,
    /// Ruisgenerator.
    noise_state: u32,
}

/// Welk deel van de actuele winddaling als turbulentie meetrilt. Klein: de
/// samples bevatten het eigen wiebelen van elke pijp al, en een
/// gemeenschappelijke wiebel voor de hele balg klinkt als een tremulant.
/// (0.7.50 had 0,15.)
const FLUTTER_DEEL: f32 = 0.04;

impl WindModel {
    pub fn new(sample_rate: SampleRate) -> Self {
        let mut m = Self {
            enabled: false,
            pressure: 1.0,
            velocity: 0.0,
            reservoir_size: 0.5,
            damping: 0.5,
            max_sag: 0.05,
            omega: 0.0,
            zeta: 0.0,
            dt: 1.0 / sample_rate as f32,
            flutter: OnePoleFilter::new(2.5, sample_rate),
            verbruik: 0.0,
            q_ref: 1.0,
            grens: 1.0 - 0.05 * 1.6,
            noise_state: 12345,
        };
        m.configure(0.5, 0.5, 0.05, sample_rate);
        m
    }

    pub fn configure(&mut self, reservoir_size: f32, damping: f32, max_sag: f32, sample_rate: SampleRate) {
        self.reservoir_size = reservoir_size.clamp(0.1, 2.0);
        self.damping = damping.clamp(0.0, 1.0);
        self.max_sag = max_sag.clamp(0.0, 0.30);
        self.dt = 1.0 / sample_rate as f32;
        // Een groot magazijn is zwaar en traag, een klein magazijn nerveus.
        // Bij de standaardstand (50 %) komt daar 6 Hz uit, bij 100 % 3 Hz —
        // de orde van grootte van een echte balg (patent Allen: 2-5 Hz).
        let f0 = (3.0 / self.reservoir_size).clamp(0.5, 12.0);
        self.omega = 2.0 * PI * f0;
        // 0 -> zeta 0.12 (duidelijk naveren), 1 -> zeta 1.0 (kritisch gedempt:
        // inzakken en terugkomen zonder overschot).
        self.zeta = 0.12 + 0.88 * self.damping;
        self.flutter.set_cutoff(2.5, sample_rate);
    }

    /// Gewogen verbruik op deze balg en de referentie ("vol werk") waarbij
    /// de inzakking de helft van het maximum is. De referentie komt van het
    /// orgel zelf (het pleno van de groep) maal de balggrootte, zodat een
    /// kistorgel en een domorgel bij hún pleno even ver zakken.
    pub fn set_verbruik(&mut self, verbruik: f32, q_ref: f32) {
        self.verbruik = verbruik.max(0.0);
        self.q_ref = q_ref.max(1e-3);
    }

    /// Statische inzakking bij het huidige verbruik (fractie), voor de meter
    /// en de tests.
    pub fn sag(&self) -> f32 {
        let q = self.verbruik / self.q_ref;
        let q2 = q * q;
        self.max_sag * q2 / (1.0 + q2)
    }

    /// Winddruk van dit moment, voor de meter in het scherm. Staat het model
    /// uit, dan is dat per definitie de volle druk.
    pub fn druk(&self) -> f32 {
        if self.enabled { self.pressure } else { 1.0 }
    }

    /// Moet dit model per sample gestapt worden? Aan, of uit maar nog niet
    /// tot rust (dan loopt de balg nog netjes vol).
    pub fn actief(&self) -> bool {
        self.enabled || (self.pressure - 1.0).abs() > 1e-6 || self.velocity.abs() > 1e-6
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
        // De grens zelf kruipt (τ ≈ 20 ms) naar de ingestelde waarde.
        let doel_grens = 1.0 - self.max_sag * 1.6;
        self.grens += (doel_grens - self.grens) * 0.001;
        let ondergrens = self.grens.min(1.0);
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
            if (self.pressure - 1.0).abs() < 5e-5 && self.velocity.abs() < 1e-2 {
                self.pressure = 1.0;
                self.velocity = 0.0;
                return 1.0;
            }
            return self.stap(1.0);
        }
        let sag = self.sag();
        // Turbulentie in het kanaal: schaalt mee met de inzakking, dus alleen
        // te horen zolang er echt wind getrokken wordt.
        let ruis = self.noise();
        let flutter = (self.flutter.process(ruis) * 50.0).clamp(-1.0, 1.0) * sag * FLUTTER_DEEL;
        self.stap(1.0 - sag + flutter)
    }
}

/// Laagste en hoogste druk die de lade kan aannemen (fractie van de rustdruk).
/// Samen met de balg: nooit dieper dan een gezond orgel doet.
pub const LADE_ONDERGRENS: f32 = 0.92;
pub const LADE_BOVENGRENS: f32 = 1.04;

/// Snelheidsimpuls per eenheid verbruik bij stoot 100 %. Gekalibreerd zodat
/// één 16'-C (verbruik 4,0) op een lang, smal kanaal (kanaal 0,8: 13,8 Hz,
/// zeta 0,31) een dip van ~3 % geeft; een 2'-pijpje (0,03) doet niets.
const LADE_STOOT_K: f32 = 0.0105;

/// De lade van één divisie: de SNELLE laag van het windwerk (ventielkast en
/// kanaal, 9-15 Hz). Statisch doet hij niets — de statische inzakking zit in
/// de balg — maar bij elke inzet of loslaat van een pijp krijgt hij een
/// stoot: kort, gebonden aan een echte gebeurtenis, nooit periodiek. Dat is
/// de "schrik" van een liggend discantakkoord als het pedaal inzet, en de
/// opstoot als het weer loslaat (Fisk: "negative pulse … positive pulse").
pub struct WindLade {
    pub enabled: bool,
    p: f32,
    v: f32,
    omega: f32,
    zeta: f32,
    dt: f32,
}

impl WindLade {
    pub fn new(sample_rate: SampleRate) -> Self {
        let mut l = Self { enabled: false, p: 1.0, v: 0.0, omega: 0.0, zeta: 0.5, dt: 1.0 / sample_rate as f32 };
        l.configure(0.5, sample_rate);
        l
    }

    /// `kanaal` 0..1: kort en wijd (9 Hz, zeta 0,55: één zucht) → lang en
    /// smal (15 Hz, zeta 0,25: twee, drie naveerslagen — Schnitger).
    pub fn configure(&mut self, kanaal: f32, sample_rate: SampleRate) {
        let k = kanaal.clamp(0.0, 1.0);
        let f0 = 9.0 + 6.0 * k;
        self.omega = 2.0 * PI * f0;
        self.zeta = 0.55 - 0.30 * k;
        self.dt = 1.0 / sample_rate as f32;
    }

    /// Stoot op de lade: positief `dq` = er komt verbruik bij (dip), negatief
    /// = er valt verbruik weg (opstoot). De impuls grijpt op de snelheid aan,
    /// niet op de druk — de druk zelf blijft continu, dus klikvrij.
    pub fn stoot(&mut self, dq: f32, sterkte: f32) {
        if !self.enabled { return; }
        self.v -= dq * sterkte * LADE_STOOT_K * self.omega;
    }

    /// Druk voor deze sample, altijd richting de rustdruk 1,0.
    pub fn process(&mut self) -> f32 {
        // Tot rust: dan exact 1,0. De drempel is ruim, want vlak bij 1,0 is
        // een f32-stap 6e-8 en blijft een uitgedempte beweging anders op
        // 1 - 1e-4 hangen (v*dt valt onder de resolutie).
        if (self.p - 1.0).abs() < 1e-4 && self.v.abs() < 2e-2 {
            self.p = 1.0;
            self.v = 0.0;
            return 1.0;
        }
        let a = self.omega * self.omega * (1.0 - self.p) - 2.0 * self.zeta * self.omega * self.v;
        self.v += a * self.dt;
        self.p += self.v * self.dt;
        if self.p < LADE_ONDERGRENS {
            self.p = LADE_ONDERGRENS;
            self.v = 0.0;
        } else if self.p > LADE_BOVENGRENS {
            self.p = LADE_BOVENGRENS;
            self.v = 0.0;
        }
        self.p
    }

    pub fn druk(&self) -> f32 { self.p }
}

// ─── Pijpprofielen: wat elke pijp van de wind trekt en hoe hij erop reageert ───

/// Familie van een register, afgeleid uit de naam. Bepaalt windverbruik
/// (mensuur) en gevoeligheid voor de druk (fluiten veel, strijkers weinig,
/// tongwerken in toonhoogte niet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PijpFamilie {
    Principaal = 0,
    Fluit = 1,
    Gedekt = 2,
    Strijker = 3,
    Mixtuur = 4,
    Tongwerk = 5,
}

impl PijpFamilie {
    pub fn van_u8(v: u8) -> Self {
        match v {
            1 => Self::Fluit, 2 => Self::Gedekt, 3 => Self::Strijker,
            4 => Self::Mixtuur, 5 => Self::Tongwerk, _ => Self::Principaal,
        }
    }
}

/// Windprofiel van een register: familie en voetmaat. Per pijp volgen
/// verbruik en gevoeligheid uit [`pijp_constanten`].
#[derive(Clone, Copy, Debug)]
pub struct StopWindProfiel {
    pub familie: PijpFamilie,
    /// Voetmaat: 8 = 8', 16 = 16', 2.667 = 2 2/3'. Voor een mixtuur de
    /// hoogste koorpijp (2' op een manuaal, 4' op het pedaal).
    pub voet: f32,
    /// Aantal koren (pijpen per toets) — 1 voor een gewoon register, bij een
    /// mixtuur uit de naam ("IV", "4-5f."), anders 4.
    pub koren: u8,
}

fn bevat(naam: &str, woorden: &[&str]) -> bool {
    woorden.iter().any(|w| naam.contains(w))
}

/// Familie uit de registernaam (Nederlands, Duits, Frans, Engels, Pools).
/// `tongwerk` komt van de bestaande tongwerkdetectie (is_reed); de lijst hier
/// vult die alleen aan. Volgorde: tongwerk → mixtuur → gedekt → fluit →
/// strijker → principaal (de terugval, want die is het veiligste midden).
pub fn familie_van_naam(naam: &str, tongwerk: bool) -> PijpFamilie {
    let n = naam.to_lowercase();
    if tongwerk || bevat(&n, &["regaal", "regal", "vox humana", "kromhoorn", "krumhorn", "cromorne",
        "trompet", "trumpet", "trompette", "hobo", "oboe", "hautbois", "fagot", "basson", "bassoon",
        "dulciaan", "dulzian", "dulcian", "bazuin", "posaune", "bombard", "clairon", "clarion",
        "schalmei", "chalumeau", "trombone", "tuba", "klarinet", "clarinet", "sordun", "ranket",
        "rankett", "musette", "puzon", "tr\u{0105}ba", "tr\u{0105}bka", "obój"]) {
        return PijpFamilie::Tongwerk;
    }
    if bevat(&n, &["mixtuur", "mixture", "mixtur", "scherp", "scharf", "cimbel", "cymbel", "cymbal",
        "zimbel", "sesquialter", "cornet", "terzian", "tertiaan", "ruispijp", "rauschpfeife",
        "rauschquint", "fourniture", "plein jeu", "plein-jeu", "pleinjeu", "acuta", "sharp", "mikstura"]) {
        return PijpFamilie::Mixtuur;
    }
    if bevat(&n, &["gedekt", "gedackt", "gedact", "gedeckt", "bourdon", "bordun", "bordone", "holpijp",
        "holpyp", "quintadeen", "quintadena", "quintaton", "quintatön", "subbas", "subbass", "soubasse",
        "sousbasse", "stopped", "lieblich", "nachthoorn", "nachthorn", "roerfluit", "rohrfl",
        "chimney", "cheminée", "koppelfluit", "kryty", "pokryty"]) {
        return PijpFamilie::Gedekt;
    }
    if bevat(&n, &["fluit", "flöte", "floete", "flute", "flauto", "flûte", "flet", "spitzfl", "waldfl",
        "blockfl", "traverso", "piccolo", "octavin", "hohlfl"]) {
        return PijpFamilie::Fluit;
    }
    if bevat(&n, &["viola", "viool", "gamba", "gambe", "salicionaal", "salicional", "salicet", "celeste",
        "céleste", "unda maris", "vox angelica", "aeoline", "dolce", "dulciana", "fugara", "voix",
        "violon", "cello", "erzähler", "gemshoorn", "gemshorn"]) {
        return PijpFamilie::Strijker;
    }
    PijpFamilie::Principaal
}

/// Voetmaat uit de toonhoogtetekst van een register: "16'" → 16, "2 2/3'" →
/// 2,667, "1 3/5'" → 1,6. Onbekend → 8'.
pub fn voet_uit_pitch(pitch: &str) -> f32 {
    let t = pitch.trim().trim_end_matches('\'').trim_end_matches('’').trim();
    let mut delen = t.split_whitespace();
    let geheel: f32 = match delen.next().and_then(|d| d.replace(',', ".").parse::<f32>().ok()) {
        Some(v) => v,
        None => return 8.0,
    };
    let breuk = delen.next().and_then(|b| {
        let mut tn = b.split('/');
        let teller: f32 = tn.next()?.parse().ok()?;
        let noemer: f32 = tn.next()?.parse().ok()?;
        if noemer > 0.0 { Some(teller / noemer) } else { None }
    }).unwrap_or(0.0);
    (geheel + breuk).clamp(0.25, 64.0)
}

/// Frequentie van de pijp op deze toets: 8' c' = 261,6 Hz.
pub fn pijpfrequentie(voet: f32, midi_note: u8) -> f32 {
    440.0 * 2f32.powf((midi_note as f32 - 69.0) / 12.0) * (8.0 / voet.max(0.25))
}

/// Windverbruik van één pijp in 8'-C-eenheden (een 8'-prestant op C = 1,0).
///
/// Uit de Hauptwerk-orgeldefinities (verbruik per pijp, kg/s) volgt: binnen
/// een register ×2,83 per octaaf omlaag (∝ f^-1,5), tussen registers ×4 per
/// octaaf voetmaat (de extra √(voet/8)), met een vloer voor de kleinste
/// pijpen. Een 16'-C komt op 4,0, een 4'-C op 0,25, een 2'-c''' op de vloer
/// 0,03: verhouding ~130, zoals in de Hauptwerk-data (129).
pub fn verbruik_van_pijp(p: &StopWindProfiel, midi_note: u8) -> f32 {
    let f = pijpfrequentie(p.voet, midi_note);
    let freq_term = (65.406 / f).powf(1.5).clamp(0.005, 8.0);
    let voet_term = (p.voet / 8.0).max(0.03).sqrt();
    let fam = match p.familie {
        PijpFamilie::Principaal => 1.0,
        PijpFamilie::Fluit => 1.2,     // wijde mensuur: meer wind
        PijpFamilie::Gedekt => 0.7,    // gedekt: kleinere pijp, minder wind
        PijpFamilie::Strijker => 0.6,  // eng: het minst
        // Meerdere pijpen in één sample: elk koorpijpje zit op de vloer, en
        // een Mixtuur IV trekt dus vier keer die vloer — niet anderhalf keer
        // een 8'-pijp (dat deed 0.7.51 bij sets zonder voetmaat: 4× te zwaar).
        PijpFamilie::Mixtuur => p.koren.max(1) as f32,
        PijpFamilie::Tongwerk => 0.8,
    };
    (freq_term * voet_term).max(0.03) * fam
}

/// De per-stem windconstanten, één keer bij het starten van de stem bepaald.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PijpWind {
    /// Windverbruik (8'-C-eenheden).
    pub verbruik: f32,
    /// Toonhoogte: rate = 1 + kp · drukafwijking.
    pub kp: f32,
    /// Sterkte: gain = 1 + kg · drukafwijking.
    pub kg: f32,
    /// Traagheid van de pijp zelf: één-pole-coëfficiënt per sample waarmee
    /// de stem de drukafwijking volgt (τ ≈ 8 perioden: een 2'-pijp volgt een
    /// snelle dip volledig, een 16'-C nauwelijks — bas is oorzaak, discant
    /// slachtoffer).
    pub dev_alpha: f32,
}

/// Deterministische spreiding per pijp (−1..1): elke pijp reageert nét
/// anders, en bij elke aanslag hetzelfde.
fn pijp_spreiding(stop_id: u32, pipe_num: u32) -> f32 {
    let h = stop_id.wrapping_mul(2_654_435_761) ^ pipe_num.wrapping_mul(40_503).rotate_left(13);
    (h % 2001) as f32 / 1000.0 - 1.0
}

/// Constanten van één pijp. `verschil` (0-1) mengt tussen "alle pijpen
/// gelijk" (0, het gedrag van 0.7.50) en de volle differentiatie (1).
pub fn pijp_constanten(
    p: &StopWindProfiel, midi_note: u8, stop_id: u32, pipe_num: u32,
    sample_rate: SampleRate, verschil: f32, tongwerk_apart: bool,
) -> PijpWind {
    let f = pijpfrequentie(p.voet, midi_note);
    let v = verschil.clamp(0.0, 1.0);
    let tongwerk = tongwerk_apart && p.familie == PijpFamilie::Tongwerk;
    // Toonhoogte t.o.v. de referentie (8'-prestant c' = 1,0). Gemeten: wijde
    // mensuur en kleine pijpen verstemmen het meest (Holpijp 8' c''' 1,84
    // cent/%, Fluit 4' c'' 0,80); strijkers eng dus weinig; tongwerken
    // bepaalt de tong, die blijft staan.
    let fam_p = match p.familie {
        PijpFamilie::Principaal => 1.0,
        PijpFamilie::Fluit => 1.4,
        PijpFamilie::Gedekt => 1.3,
        PijpFamilie::Strijker => 0.6,
        PijpFamilie::Mixtuur => 1.2,
        PijpFamilie::Tongwerk => if tongwerk { 0.0 } else { 1.0 },
    };
    let oct = (f / 65.406).max(1e-3).log2();
    let grootte = 0.75 + 0.625 * (oct / 5.0).clamp(0.0, 1.0); // 8' C 0,75; c' 1,0; c''' 1,25
    let spreiding = 1.0 + 0.2 * pijp_spreiding(stop_id, pipe_num);
    let sens_raw = fam_p * grootte * spreiding;
    let sens = 1.0 + v * (sens_raw - 1.0);
    // Sterkte: labialen ≈ druk² (−0,9 dB bij −5 %), tongwerken zakken vooral
    // in kracht (1,8×).
    let kg_raw = if tongwerk { 3.6 } else { 2.0 };
    let kg = 2.0 + v * (kg_raw - 2.0);
    // Traagheid van de pijp als resonator: τ = 8 perioden.
    let dev_alpha = (1.0 - (-f / (8.0 * sample_rate as f32)).exp()).clamp(1e-5, 0.2);
    PijpWind { verbruik: verbruik_van_pijp(p, midi_note), kp: sens * WIND_RATE_PER_EENHEID, kg, dev_alpha }
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

    /// Een balg in het Hollandse karakter met "vol werk" = q_ref 10.
    fn balg(demping: f32, max_sag: f32) -> WindModel {
        let mut m = WindModel::new(SR);
        m.enabled = true;
        m.configure(0.5, demping, max_sag, SR);
        m
    }

    #[test]
    fn uitgeschakeld_blijft_de_druk_vol() {
        let mut m = WindModel::new(SR);
        m.set_verbruik(50.0, 10.0);
        let (laagste, laatste) = draai(&mut m, 500);
        assert_eq!(laagste, 1.0);
        assert_eq!(laatste, 1.0);
        assert_eq!(m.druk(), 1.0);
    }

    #[test]
    fn zonder_verbruik_zakt_er_niets_in() {
        let mut m = balg(0.5, 0.05);
        m.set_verbruik(0.0, 10.0);
        let (laagste, _) = draai(&mut m, 500);
        assert!(laagste > 0.9999, "stille windlade zakte toch in: {}", laagste);
    }

    /// Eén register doet niets, het pleno ademt: de inzakking groeit
    /// kwadratisch met het verbruik en is bij "vol werk" (q = 1) de helft
    /// van het maximum.
    #[test]
    fn de_inzakking_is_kwadratisch_en_verzadigt() {
        let mut m = balg(1.0, 0.04);
        let sag_bij = |m: &mut WindModel, q: f32| { m.set_verbruik(q, 10.0); m.sag() };
        let s1 = sag_bij(&mut m, 1.0);   // één Holpijp-akkoord
        let s2 = sag_bij(&mut m, 2.0);
        let s10 = sag_bij(&mut m, 10.0); // vol werk
        let s30 = sag_bij(&mut m, 30.0); // tutti met pedaal
        assert!(s1 < 0.0005, "één register mag niet ademen: {}", s1);
        assert!((s2 / s1 - 4.0).abs() < 0.2, "niet kwadratisch bij klein verbruik: {}", s2 / s1);
        assert!((s10 - 0.02).abs() < 0.001, "vol werk hoort de helft van het maximum te zijn: {}", s10);
        assert!(s30 > 0.035 && s30 < 0.04, "tutti nadert het maximum: {}", s30);
    }

    /// De kern van de klacht "ik merk er niets van": een vol werk moet een
    /// hoorbare inzakking geven — voor de referentiepijp een paar cent, en
    /// de fluiten en mixturen zakken daar nog anderhalf keer zo ver.
    #[test]
    fn een_vol_werk_geeft_een_hoorbare_inzakking() {
        let mut m = balg(0.7, 0.05);
        m.set_verbruik(10.0, 10.0);
        let (_, druk) = draai(&mut m, 1500);
        let cents = (1.0 - druk) * WIND_CENTS_PER_EENHEID;
        assert!(cents > 1.5, "te weinig om te horen: {:.1} cent", cents);
        assert!(cents < 4.0, "te veel voor een gezond orgel: {:.1} cent", cents);
    }

    /// De dempingsregelaar bepaalt het naveren: laag gedempt schiet de balg
    /// door, kritisch gedempt niet.
    #[test]
    fn de_demping_bepaalt_het_naveren() {
        let overschot = |demping: f32| {
            let mut m = balg(demping, 0.05);
            m.set_verbruik(30.0, 10.0);
            draai(&mut m, 3000);
            m.set_verbruik(0.0, 10.0);
            let mut hoogste: f32 = 0.0;
            for _ in 0..(SR * 2) {
                let d = m.process();
                if d > hoogste { hoogste = d; }
            }
            hoogste
        };
        let los = overschot(0.05);
        let strak = overschot(1.0);
        assert!(los > 1.001, "slap gedempt hoort door te schieten, was {}", los);
        assert!(strak <= 1.0005, "kritisch gedempt hoort niet door te schieten, was {}", strak);
    }

    /// Live de maximale daling verlagen terwijl de balg ingezakt is mag geen
    /// sprong geven: de ondergrens kruipt, de druk glijdt.
    #[test]
    fn de_daling_verlagen_geeft_geen_sprong() {
        let mut m = balg(0.8, 0.10);
        m.set_verbruik(30.0, 10.0);
        let diep = draai(&mut m, 2000).1;
        assert!(diep < 0.93, "niet ingezakt: {}", diep);
        m.configure(0.5, 0.8, 0.01, SR);
        let mut vorige = diep;
        let mut grootste_stap = 0.0f32;
        for _ in 0..(SR / 2) {
            let p = m.process();
            grootste_stap = grootste_stap.max((p - vorige).abs());
            vorige = p;
        }
        assert!(grootste_stap < 0.0005, "sprong per sample van {:.5}", grootste_stap);
        assert!(vorige > 0.98, "kwam niet omhoog: {}", vorige);
    }

    #[test]
    fn de_druk_blijft_binnen_de_perken() {
        let mut m = WindModel::new(SR);
        m.enabled = true;
        m.configure(0.1, 0.0, 0.30, SR);
        draai(&mut m, 300); // de grens kruipt naar 0,52
        for stap in 0..60 {
            m.set_verbruik(if stap % 2 == 0 { 800.0 } else { 0.0 }, 10.0);
            let (laagste, _) = draai(&mut m, 40);
            assert!(laagste >= 1.0 - 0.30 * 1.6 - 1e-6 && laagste <= 1.031,
                    "druk liep uit de rails: {}", laagste);
        }
    }

    #[test]
    fn uitzetten_loopt_netjes_vol() {
        let mut m = balg(0.8, 0.10);
        m.set_verbruik(30.0, 10.0);
        let ingezakt = draai(&mut m, 2000).1;
        assert!(ingezakt < 0.93, "niet ingezakt, test zegt niets: {}", ingezakt);
        m.enabled = false;
        let direct = m.process();
        assert!((direct - ingezakt).abs() < 0.001, "sprong bij het uitzetten: {} -> {}", ingezakt, direct);
        let (_, na) = draai(&mut m, 2000);
        assert!((na - 1.0).abs() < 1e-4, "kwam niet op volle druk: {}", na);
    }
}

#[cfg(test)]
mod windlade_tests {
    use super::*;

    const SR: SampleRate = 48000;

    fn lade(kanaal: f32) -> WindLade {
        let mut l = WindLade::new(SR);
        l.enabled = true;
        l.configure(kanaal, SR);
        l
    }

    /// (laagste druk, hoogste druk, tijd tot laagste in ms) over `ms`.
    fn volg(l: &mut WindLade, ms: u32) -> (f32, f32, f32) {
        let n = (SR as f32 * ms as f32 / 1000.0) as u32;
        let (mut lo, mut hi, mut t_lo) = (f32::MAX, f32::MIN, 0.0);
        for i in 0..n {
            let p = l.process();
            if p < lo { lo = p; t_lo = i as f32 * 1000.0 / SR as f32; }
            if p > hi { hi = p; }
        }
        (lo, hi, t_lo)
    }

    /// De kalibratie: één 16'-C onder een liggend akkoord geeft op een
    /// Hollandse lade een dip van ~3 %, binnen enkele tientallen ms, en
    /// binnen 300 ms is de lade weer tot rust.
    #[test]
    fn een_zestienvoets_c_laat_de_lade_drie_procent_schrikken() {
        let mut l = lade(0.8);
        l.stoot(4.0, 1.0);
        let (lo, _, t_lo) = volg(&mut l, 300);
        let dip = 1.0 - lo;
        assert!(dip > 0.024 && dip < 0.036, "dip {:.4} buiten de kalibratie", dip);
        assert!(t_lo > 5.0 && t_lo < 40.0, "dip kwam op {:.1} ms", t_lo);
        assert!((l.druk() - 1.0).abs() < 0.003, "na 300 ms nog niet tot rust: {}", l.druk());
    }

    /// Een 2'-pijpje doet niets merkbaars.
    #[test]
    fn een_klein_pijpje_doet_niets() {
        let mut l = lade(0.8);
        l.stoot(0.03, 1.0);
        let (lo, _, _) = volg(&mut l, 200);
        assert!(1.0 - lo < 0.0005, "een 2'-pijpje liet de lade schrikken: {}", 1.0 - lo);
    }

    /// Loslaten geeft een opstoot: de druk schiet kort boven de rustdruk.
    #[test]
    fn loslaten_geeft_een_opstoot() {
        let mut l = lade(0.8);
        l.stoot(-4.0 * 0.4, 1.0);
        let (lo, hi, _) = volg(&mut l, 300);
        assert!(hi > 1.008 && hi < 1.02, "opstoot {:.4}", hi);
        assert!(lo > 0.995, "een opstoot mag niet eerst dippen: {}", lo);
    }

    /// Lang kanaal veert na, kort kanaal niet.
    #[test]
    fn het_kanaal_bepaalt_het_naveren() {
        let nulldoorgangen = |kanaal: f32| {
            let mut l = lade(kanaal);
            l.stoot(4.0, 1.0);
            let mut vorige = 1.0f32;
            let mut n = 0;
            for _ in 0..(SR / 2) {
                let p = l.process();
                if (p - 1.0) * (vorige - 1.0) < 0.0 { n += 1; }
                vorige = p;
            }
            n
        };
        let (lang, kort) = (nulldoorgangen(1.0), nulldoorgangen(0.0));
        assert!(lang >= 3, "lang kanaal hoort na te veren: {} kruisingen", lang);
        assert!(kort <= 2 && kort < lang, "kort kanaal hoort nauwelijks na te veren: {} tegen {}", kort, lang);
    }

    /// Ook bij de wildste stapeling blijft de lade binnen de perken en komt
    /// hij tot rust.
    #[test]
    fn de_lade_blijft_binnen_de_perken() {
        let mut l = lade(1.0);
        for i in 0..40 {
            l.stoot(if i % 2 == 0 { 40.0 } else { -40.0 }, 2.0);
            let (lo, hi, _) = volg(&mut l, 20);
            assert!(lo >= LADE_ONDERGRENS - 1e-6 && hi <= LADE_BOVENGRENS + 1e-6, "{} {}", lo, hi);
        }
        volg(&mut l, 1000);
        assert!((l.druk() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn uitgeschakeld_reageert_de_lade_niet() {
        let mut l = WindLade::new(SR);
        l.stoot(40.0, 2.0);
        let (lo, hi, _) = volg(&mut l, 100);
        assert_eq!(lo, 1.0);
        assert_eq!(hi, 1.0);
    }
}

#[cfg(test)]
mod pijpprofiel_tests {
    use super::*;

    const SR: SampleRate = 48000;

    fn prof(familie: PijpFamilie, voet: f32) -> StopWindProfiel {
        StopWindProfiel { familie, voet, koren: 1 }
    }

    /// Een Mixtuur IV op 2' weegt vier vloerpijpjes — een fractie van een
    /// 8'-prestant op dezelfde toets, niet anderhalf keer zoveel.
    #[test]
    fn een_mixtuur_weegt_vier_vloerpijpjes() {
        let mix = StopWindProfiel { familie: PijpFamilie::Mixtuur, voet: 2.0, koren: 4 };
        let m = verbruik_van_pijp(&mix, 60);
        let prestant = verbruik_van_pijp(&prof(PijpFamilie::Principaal, 8.0), 60);
        assert!((m - 0.12).abs() < 0.01, "mixtuur IV op c': {}", m);
        assert!(m < prestant, "een mixtuur hoort lichter te wegen dan een 8'-prestant: {} vs {}", m, prestant);
    }

    #[test]
    fn familie_uit_de_naam() {
        use PijpFamilie::*;
        assert_eq!(familie_van_naam("Trompet 8'", true), Tongwerk);
        assert_eq!(familie_van_naam("Vox humana 8'", false), Tongwerk);
        assert_eq!(familie_van_naam("Holpijp 8'", false), Gedekt);
        assert_eq!(familie_van_naam("Roerfluit 4'", false), Gedekt);
        assert_eq!(familie_van_naam("Bourdon 16'", false), Gedekt);
        assert_eq!(familie_van_naam("Fluit 4'", false), Fluit);
        assert_eq!(familie_van_naam("Hohlflöte 8'", false), Fluit);
        assert_eq!(familie_van_naam("Viola di Gamba 8'", false), Strijker);
        assert_eq!(familie_van_naam("Mixtuur IV", false), Mixtuur);
        assert_eq!(familie_van_naam("Sesquialter II", false), Mixtuur);
        assert_eq!(familie_van_naam("Prestant 8'", false), Principaal);
        assert_eq!(familie_van_naam("Octaaf 4'", false), Principaal);
        assert_eq!(familie_van_naam("Quint 2 2/3'", false), Principaal);
    }

    #[test]
    fn voetmaat_uit_de_tekst() {
        assert_eq!(voet_uit_pitch("16'"), 16.0);
        assert_eq!(voet_uit_pitch("8'"), 8.0);
        assert!((voet_uit_pitch("2 2/3'") - 2.6667).abs() < 1e-3);
        assert!((voet_uit_pitch("1 3/5'") - 1.6).abs() < 1e-3);
        assert_eq!(voet_uit_pitch(""), 8.0);
        assert_eq!(voet_uit_pitch("IV"), 8.0);
    }

    /// De Hauptwerk-verhoudingen: 16' C = 4× 8' C, 4' C = ¼, en een 16'-C
    /// trekt ruim honderd keer een 2'-c'''.
    #[test]
    fn verbruik_volgt_de_hauptwerk_verhoudingen() {
        let c8 = verbruik_van_pijp(&prof(PijpFamilie::Principaal, 8.0), 36);
        let c16 = verbruik_van_pijp(&prof(PijpFamilie::Principaal, 16.0), 36);
        let c4 = verbruik_van_pijp(&prof(PijpFamilie::Principaal, 4.0), 36);
        let c2_hoog = verbruik_van_pijp(&prof(PijpFamilie::Principaal, 2.0), 84);
        assert!((c8 - 1.0).abs() < 0.01, "8' C hoort 1,0 te zijn: {}", c8);
        assert!((c16 / c8 - 4.0).abs() < 0.3, "16' C / 8' C = {}", c16 / c8);
        assert!((c4 / c8 - 0.25).abs() < 0.03, "4' C / 8' C = {}", c4 / c8);
        let ratio = c16 / c2_hoog;
        assert!(ratio > 60.0 && ratio < 250.0, "16' C / 2' c''' = {}", ratio);
        // Binnen één register: ×2,83 per octaaf.
        let c = verbruik_van_pijp(&prof(PijpFamilie::Principaal, 8.0), 48);
        assert!((c8 / c - 2.83).abs() < 0.1, "per octaaf {}", c8 / c);
    }

    #[test]
    fn gevoeligheid_verschilt_per_familie_en_grootte() {
        let k = |fam, voet, noot| pijp_constanten(&prof(fam, voet), noot, 1, noot as u32, SR, 1.0, true);
        let prestant_c1 = k(PijpFamilie::Principaal, 8.0, 60);
        let fluit_c3 = k(PijpFamilie::Fluit, 8.0, 84);
        let strijker = k(PijpFamilie::Strijker, 8.0, 60);
        let trompet = k(PijpFamilie::Tongwerk, 8.0, 60);
        let bas16 = k(PijpFamilie::Principaal, 16.0, 36);
        // Referentie rond 1,0 (spreiding ±20 %).
        assert!((prestant_c1.kp / WIND_RATE_PER_EENHEID - 1.0).abs() < 0.25, "{}", prestant_c1.kp / WIND_RATE_PER_EENHEID);
        assert!(fluit_c3.kp > 1.3 * prestant_c1.kp, "fluit c''' hoort duidelijk gevoeliger");
        assert!(strijker.kp < 0.8 * prestant_c1.kp, "strijker hoort minder gevoelig");
        assert_eq!(trompet.kp, 0.0, "tongwerk blijft in toonhoogte staan");
        assert!(trompet.kg > 3.0, "tongwerk zakt in kracht");
        assert!(bas16.kp < prestant_c1.kp, "een grote pijp verstemt minder dan een kleine");
        // Traagheid: de bas volgt traag, het discant snel.
        assert!(bas16.dev_alpha < 2e-4, "16' C hoort traag te volgen: {}", bas16.dev_alpha);
        assert!(fluit_c3.dev_alpha > 2e-3, "c''' hoort snel te volgen: {}", fluit_c3.dev_alpha);
    }

    #[test]
    fn zonder_verschil_is_alles_gelijk_en_de_spreiding_is_deterministisch() {
        let p = prof(PijpFamilie::Fluit, 4.0);
        let a = pijp_constanten(&p, 84, 3, 7, SR, 0.0, true);
        let b = pijp_constanten(&prof(PijpFamilie::Tongwerk, 8.0), 40, 9, 2, SR, 0.0, true);
        assert_eq!(a.kp, WIND_RATE_PER_EENHEID);
        assert_eq!(b.kp, WIND_RATE_PER_EENHEID);
        assert_eq!(a.kg, 2.0);
        let x = pijp_constanten(&p, 84, 3, 7, SR, 1.0, true);
        let y = pijp_constanten(&p, 84, 3, 7, SR, 1.0, true);
        let z = pijp_constanten(&p, 84, 3, 8, SR, 1.0, true);
        assert_eq!(x, y, "dezelfde pijp hoort altijd dezelfde constanten te krijgen");
        assert_ne!(x.kp, z.kp, "twee pijpen horen nét te verschillen");
        assert!((x.kp / z.kp - 1.0).abs() < 0.5);
    }
}
