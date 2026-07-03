//! JM-Orgue VST3/CLAP plugin
//!
//! MVP: ontvangt MIDI, speelt sine-wave voices per noot (organ-like flute).
//! Doel: bewijzen dat de DAW-route werkt; sample playback komt in latere iteraties.
//!
//! Volledige architectuur staat in `docs/VST_AU_FEASIBILITY.md`.

use nih_plug::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

mod voice;
use voice::OrganVoice;

/// Maximaal aantal gelijktijdige voices in de plugin.
const MAX_VOICES: usize = 128;

/// MIDI-CC nummers die we kunnen reageren (toekomstig: swell, crescendo, etc.)
#[allow(dead_code)]
mod cc {
    pub const SUSTAIN: u8 = 64;
    pub const EXPRESSION: u8 = 11;
}

#[derive(Params)]
pub struct JmOrgueParams {
    /// Master gain in dB
    #[id = "gain"]
    pub gain_db: FloatParam,

    /// Stop-mix balans tussen "Prestant" (helder) en "Holpijp" (zacht)
    #[id = "stop_mix"]
    pub stop_mix: FloatParam,

    /// Aanslag-attack tijd (ms) — hoe snel een pijp aanspreekt
    #[id = "attack_ms"]
    pub attack_ms: FloatParam,

    /// Release-tijd (ms)
    #[id = "release_ms"]
    pub release_ms: FloatParam,

    /// Tremulant-snelheid (Hz)
    #[id = "tremulant_rate"]
    pub tremulant_rate: FloatParam,

    /// Tremulant-diepte (0..1)
    #[id = "tremulant_depth"]
    pub tremulant_depth: FloatParam,
}

impl Default for JmOrgueParams {
    fn default() -> Self {
        Self {
            gain_db: FloatParam::new(
                "Master Gain",
                -6.0,
                FloatRange::Linear { min: -60.0, max: 6.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(50.0)),

            stop_mix: FloatParam::new(
                "Stop Mix (Prestant ↔ Holpijp)",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(50.0)),

            attack_ms: FloatParam::new(
                "Attack",
                25.0,
                FloatRange::Skewed { min: 5.0, max: 500.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" ms"),

            release_ms: FloatParam::new(
                "Release",
                250.0,
                FloatRange::Skewed { min: 20.0, max: 2000.0, factor: FloatRange::skew_factor(-1.0) },
            )
            .with_unit(" ms"),

            tremulant_rate: FloatParam::new(
                "Tremulant rate",
                6.0,
                FloatRange::Linear { min: 2.0, max: 10.0 },
            )
            .with_unit(" Hz"),

            tremulant_depth: FloatParam::new(
                "Tremulant depth",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
        }
    }
}

pub struct JmOrgue {
    params: Arc<JmOrgueParams>,
    sample_rate: f32,

    /// Polyfoon voice pool, beschermd voor cross-thread access (UI ↔ audio).
    voices: Mutex<Vec<OrganVoice>>,

    /// Tremulant LFO fase (radians)
    trem_phase: f32,
}

impl Default for JmOrgue {
    fn default() -> Self {
        Self {
            params: Arc::new(JmOrgueParams::default()),
            sample_rate: 48000.0,
            voices: Mutex::new(Vec::with_capacity(MAX_VOICES)),
            trem_phase: 0.0,
        }
    }
}

impl Plugin for JmOrgue {
    const NAME: &'static str = "JM-Orgue";
    const VENDOR: &'static str = "JM-Orgue";
    const URL: &'static str = "https://github.com/3bm/virtual-pipe-organ";
    const EMAIL: &'static str = "orgelmaker@live.nl";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.voices.lock().clear();
        self.trem_phase = 0.0;
        nih_log!("JM-Orgue plugin initialized at {} Hz", self.sample_rate);
        true
    }

    fn reset(&mut self) {
        self.voices.lock().clear();
        self.trem_phase = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let sr = self.sample_rate;
        let attack_s = self.params.attack_ms.value() * 0.001;
        let release_s = self.params.release_ms.value() * 0.001;
        let stop_mix = self.params.stop_mix.value();
        let trem_rate = self.params.tremulant_rate.value();
        let trem_depth = self.params.tremulant_depth.value();
        let gain_db = self.params.gain_db.smoothed.next();
        let gain = 10f32.powf(gain_db / 20.0);

        // Verzamel MIDI events voor dit blok
        // Note: nih-plug levert events met sample-precieze timing; voor MVP behandelen
        // we ze "blok-niveau" (alle events aan begin van blok). Verfijnen kan later.
        let mut voices = self.voices.lock();

        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, .. } => {
                    if voices.len() >= MAX_VOICES {
                        // Steel oudste releasing voice, of skip
                        if let Some(pos) = voices.iter().position(|v| v.is_releasing()) {
                            voices.remove(pos);
                        } else if let Some(pos) = voices.iter().position(|v| v.note > 0) {
                            voices.remove(pos);
                        }
                    }
                    voices.push(OrganVoice::new(note, velocity, sr, attack_s));
                }
                NoteEvent::NoteOff { note, .. } => {
                    for v in voices.iter_mut() {
                        if v.note == note && !v.is_releasing() {
                            v.start_release(release_s);
                        }
                    }
                }
                NoteEvent::MidiCC { cc: _, value: _, .. } => {
                    // Toekomst: sustain, expressie. Voor MVP: niets.
                }
                _ => {}
            }
        }

        // Audio synthese per sample
        let num_samples = buffer.samples();
        let trem_step = 2.0 * std::f32::consts::PI * trem_rate / sr;

        let mut out_buf = buffer.as_slice();
        let num_ch = out_buf.len();

        for s in 0..num_samples {
            // Tremulant amplitude modulatie
            self.trem_phase = (self.trem_phase + trem_step) % (2.0 * std::f32::consts::PI);
            let trem = 1.0 - trem_depth * 0.5 * (1.0 - self.trem_phase.sin());

            let mut mix = 0.0f32;
            for v in voices.iter_mut() {
                mix += v.next_sample(stop_mix);
            }

            let sample = (mix * trem * gain).tanh(); // soft clip

            for ch in 0..num_ch {
                out_buf[ch][s] = sample;
            }
        }

        // Verwijder voltooide voices
        voices.retain(|v| !v.is_finished());

        ProcessStatus::Normal
    }
}

impl ClapPlugin for JmOrgue {
    const CLAP_ID: &'static str = "nl.jm-orgue.plugin";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Virtueel pijporgel — sample-based instrument");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Sampler,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for JmOrgue {
    const VST3_CLASS_ID: [u8; 16] = *b"JMOrgueVST3Plug2";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Sampler,
    ];
}

nih_export_clap!(JmOrgue);
nih_export_vst3!(JmOrgue);
