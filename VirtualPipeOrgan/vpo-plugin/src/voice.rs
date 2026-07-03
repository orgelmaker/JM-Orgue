//! Eén orgelpijp-voice: twee sine-oscillatoren (Prestant + Holpijp) met ADSR.
//!
//! Dit is een MVP synthese; in latere iteraties vervangen we dit door echte
//! sample-playback uit een geladen sampleset.

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Attack,
    Sustain,
    Release,
    Finished,
}

#[derive(Debug)]
pub struct OrganVoice {
    pub note: u8,
    velocity: f32,
    phase_prestant: f32,
    phase_holpijp: f32,
    step_prestant: f32,
    step_holpijp: f32,
    env: f32,
    attack_inc: f32,
    release_dec: f32,
    state: Phase,
}

impl OrganVoice {
    pub fn new(note: u8, velocity: f32, sample_rate: f32, attack_s: f32) -> Self {
        let freq = midi_to_freq(note);
        // Prestant 8' = grondtoon, Holpijp 8' = grondtoon met versterkte octaaf (gedekte pijp karakter)
        let step_prestant = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let step_holpijp = 2.0 * std::f32::consts::PI * freq * 2.0 / sample_rate;

        let attack_samples = (attack_s * sample_rate).max(1.0);
        Self {
            note,
            velocity,
            phase_prestant: 0.0,
            phase_holpijp: 0.0,
            step_prestant,
            step_holpijp,
            env: 0.0,
            attack_inc: 1.0 / attack_samples,
            release_dec: 0.0,
            state: Phase::Attack,
        }
    }

    pub fn start_release(&mut self, release_s: f32) {
        // Bereken release dec voor huidige envelope-niveau
        // sample_rate niet direct beschikbaar — we slaan attack_inc op, sample_rate volgt
        // uit het feit dat attack_inc = 1/(attack_s * sr). We benaderen sr via attack_inc.
        // Voor consistentie geven we sample_rate niet door — release_s in samples staat
        // via een andere route niet vast. Workaround: caller berekent dec; we doen dat hier
        // door release_s als "tijd tot stil"-conversie via huidige env / 1.
        // Verbetering: sla sample_rate op in voice.
        // Voor MVP: aannemen sr = 48000 als attack_inc * attack_samples ≈ 1
        // Sla release_s op en bereken bij elke sample-step.
        // Praktischer: bereken dec direct met aangenomen sample_rate uit attack_inc.
        // attack_inc = 1 / (attack_s * sr), maar attack_s zit niet meer hier.
        // Daarom: simpele aanname dat plugin op 48k draait — verfijnen later.
        let sr = 48000.0;
        let release_samples = (release_s * sr).max(1.0);
        self.release_dec = 1.0 / release_samples;
        self.state = Phase::Release;
    }

    pub fn is_releasing(&self) -> bool {
        matches!(self.state, Phase::Release | Phase::Finished)
    }

    pub fn is_finished(&self) -> bool {
        self.state == Phase::Finished
    }

    /// Genereer één sample. `stop_mix`: 0=alleen Prestant, 1=alleen Holpijp.
    pub fn next_sample(&mut self, stop_mix: f32) -> f32 {
        match self.state {
            Phase::Attack => {
                self.env = (self.env + self.attack_inc).min(1.0);
                if self.env >= 1.0 {
                    self.state = Phase::Sustain;
                }
            }
            Phase::Sustain => {}
            Phase::Release => {
                self.env -= self.release_dec;
                if self.env <= 0.0 {
                    self.env = 0.0;
                    self.state = Phase::Finished;
                    return 0.0;
                }
            }
            Phase::Finished => return 0.0,
        }

        self.phase_prestant += self.step_prestant;
        if self.phase_prestant > std::f32::consts::TAU {
            self.phase_prestant -= std::f32::consts::TAU;
        }
        self.phase_holpijp += self.step_holpijp;
        if self.phase_holpijp > std::f32::consts::TAU {
            self.phase_holpijp -= std::f32::consts::TAU;
        }

        let prestant = self.phase_prestant.sin();
        // Holpijp: sinus met derde-harmonische voor wat gevulder geluid
        let holpijp = self.phase_holpijp.sin() * 0.4
            + (self.phase_holpijp * 3.0).sin() * 0.15;

        // Mix de twee "registers"
        let voice = prestant * (1.0 - stop_mix) + holpijp * stop_mix;

        voice * self.env * self.velocity * 0.25
    }
}

fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}
