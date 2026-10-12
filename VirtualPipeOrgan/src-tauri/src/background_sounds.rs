//! Doorlopende achtergrondgeluiden (0.7.97): orgelmotor, blaasbalg en
//! ruimtegeluid die buiten de registratie om loopen zodra het orgel aan
//! staat. In 0.7.96 kwam de import-herkenning (GrandOrgue-stops met
//! AcceptsRetuning=N en naam blower/ambient); deze module speelt ze af.
//!
//! Ontwerp:
//! - Eén `BackgroundEngine` leeft in de audio-thread (geen mutex op het
//!   hete pad). Commands komen binnen via `AudioCommand::Background…` en
//!   worden door `BackgroundEngine::apply` verwerkt.
//! - Per geladen WAV één `BackgroundVoice`; die wordt bij orgellaad gestart
//!   en bij orgelwissel/afsluiten weggegooid.
//! - De voice resamplet met lineaire interpolatie van de bron-samplerate
//!   naar de output-rate (meeste bijgeluiden zijn 44.1/48 kHz, output is
//!   48 of 96).
//! - Mono-samples worden gelijk naar beide outputkanalen gemapt; stereo-
//!   samples blijven stereo.
//! - De voice loopt oneindig (`pos mod len`): bijgeluiden zijn naadloos
//!   geloopt opgenomen; voor niet-naadloze bronnen trillen ze iets, dat is
//!   een maker-keuze.
//! - Fade in/uit per voice voor de motor-knop (geen klik bij in-/uitschakelen).
//! - `global_gain` is een multiplier op alle voices, bediend door de
//!   bijgeluiden-schuif in de UI. Vermenigvuldigt zich daarna met de
//!   gewone master-gain in de audio-thread (zodat de blower zacht wordt
//!   als het orgel zacht staat).

use std::sync::Arc;
use vpo_sampler::grandorgue::BackgroundSoundKind;

/// Hoeveel frames duurt de fade bij motor aan/uit. 48 kHz × 0.3 s ≈ 14400.
/// Lang genoeg om een klikloze overgang te geven; kort genoeg om "motor aan"
/// voelbaar snel te laten zijn.
const FADE_FRAMES: f32 = 14_400.0;

/// Eén doorlopende stem (één WAV, geloopt met pos % len).
#[derive(Debug, Clone)]
pub struct BackgroundVoice {
    pub kind: BackgroundSoundKind,
    /// Linkerkanaal (of mono).
    pub data_l: Arc<Vec<f32>>,
    /// Rechterkanaal bij stereo-opname; None = mono (dan L→beide).
    pub data_r: Option<Arc<Vec<f32>>>,
    /// Afspeelsnelheid: bron-SR / output-SR. 1.0 = geen resample.
    pub rate: f64,
    /// Lineaire gain voor DEZE voice (voor set-up: bv. per sampleset
    /// verschillend). Default 1.0.
    pub voice_gain: f32,
}

/// Een BackgroundVoice klaar om af te spelen in de audio-thread, met de
/// loop-staat die alleen op de audio-thread leeft.
pub(crate) struct ActiveVoice {
    pub voice: BackgroundVoice,
    /// Positie in bron-samples (float, voor lineaire interpolatie).
    pub pos: f64,
    /// Huidige gain (volgt `target_gain` via fade).
    pub current_gain: f32,
    /// Doelgain na toepassen van actief/uit (0.0 = uit). Multipliceert zich
    /// met `voice_gain` en `engine.global_gain`.
    pub target_gain: f32,
}

/// Beheert alle doorlopende achtergrondgeluiden. Leeft in de audio-thread.
pub(crate) struct BackgroundEngine {
    voices: Vec<ActiveVoice>,
    /// Multiplicatieve master voor alle bijgeluiden samen; wordt gevoed door
    /// de UI-schuif. 0.0 = helemaal uit, 1.0 = volledig; eventueel >1.0.
    pub global_gain: f32,
    /// Motor-knop: true = blower-voices actief, false = fade-uit.
    pub blower_active: bool,
}

impl BackgroundEngine {
    pub fn new() -> Self {
        Self {
            voices: Vec::new(),
            // Standaard een beetje zacht, zodat de blower niet overheerst
            // als een organist voor het eerst een set met motor-geluid laadt.
            global_gain: 0.3,
            blower_active: true,
        }
    }

    /// Vervang alle voices door de nieuwe set (orgelwissel of eerste load).
    pub fn set_voices(&mut self, nieuw: Vec<BackgroundVoice>) {
        self.voices = nieuw.into_iter().map(|v| ActiveVoice {
            // Blower start uit als de motorknop uit staat.
            target_gain: match v.kind {
                BackgroundSoundKind::Blower if !self.blower_active => 0.0,
                _ => 1.0,
            },
            current_gain: 0.0, // begin stil, fade in
            voice: v,
            pos: 0.0,
        }).collect();
    }

    /// Alle voices wissen (bv. bij orgel-ontladen).
    pub fn clear(&mut self) {
        self.voices.clear();
    }

    /// Motor aan/uit zetten; blower-voices faden in of uit.
    pub fn set_blower(&mut self, active: bool) {
        self.blower_active = active;
        for v in &mut self.voices {
            if matches!(v.voice.kind, BackgroundSoundKind::Blower) {
                v.target_gain = if active { 1.0 } else { 0.0 };
            }
        }
    }

    /// Bijgeluiden-schuif.
    pub fn set_global_gain(&mut self, gain_lineair: f32) {
        self.global_gain = gain_lineair.clamp(0.0, 4.0);
    }

    /// Zijn er voices die geluid produceren? Zo niet, dan kan de audio-
    /// thread de mix-stap overslaan.
    pub fn is_actief(&self) -> bool {
        !self.voices.is_empty() && self.global_gain > 1e-6
    }

    /// Meng direct in de INTERLEAVED output van cpal: elk frame bestaat uit
    /// `channels` samples; kanaal 0 krijgt L en kanaal 1 krijgt R (de
    /// gebruikelijke front-stereo-paring in surround). Bijgeluiden klinken
    /// buiten alle divisies en de galm om; ze gaan rechtstreeks naar de
    /// hoofduitgang en volgen de master-limiter niet. Overloopen de hoofduit-
    /// gang + bijgeluid >1.0, dan clampt de clamping-stap aan het eind van
    /// de callback (`frame.clamp(-1.0, 1.0)`) hem alsnog netjes.
    pub fn mix_interleaved(&mut self, data: &mut [f32], start_frame: usize, frames: usize, channels: usize) {
        if !self.is_actief() || channels == 0 { return; }
        for i in 0..frames {
            let base = (start_frame + i) * channels;
            if base + 1 >= data.len() { break; }
            let (l, r) = self.volgende_sample();
            data[base] += l;
            if channels > 1 { data[base + 1] += r; }
        }
    }

    /// Genereer één (L, R)-paar en schuif de voices één stap op.
    fn volgende_sample(&mut self) -> (f32, f32) {
        let gg = self.global_gain;
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;
        for v in &mut self.voices {
            let len_l = v.voice.data_l.len();
            if len_l == 0 { continue; }
            let effectief_doel = v.target_gain * v.voice.voice_gain * gg;
            let fade_step = (v.voice.voice_gain * gg) / FADE_FRAMES.max(1.0);
            if (v.current_gain - effectief_doel).abs() < fade_step {
                v.current_gain = effectief_doel;
            } else if v.current_gain < effectief_doel {
                v.current_gain += fade_step;
            } else {
                v.current_gain -= fade_step;
            }
            let rate = v.voice.rate;
            if v.current_gain > 0.0 || effectief_doel > 0.0 {
                let p = v.pos;
                let p0 = p.floor() as usize % len_l;
                let p1 = (p0 + 1) % len_l;
                let frac = (p - p.floor()) as f32;
                let sl = v.voice.data_l[p0] * (1.0 - frac) + v.voice.data_l[p1] * frac;
                let sr = match v.voice.data_r.as_ref() {
                    Some(r) if !r.is_empty() => {
                        let len_r = r.len();
                        let r0 = p0 % len_r;
                        let r1 = (r0 + 1) % len_r;
                        r[r0] * (1.0 - frac) + r[r1] * frac
                    }
                    _ => sl,
                };
                out_l += sl * v.current_gain;
                out_r += sr * v.current_gain;
            }
            v.pos += rate;
            if v.pos >= len_l as f64 {
                v.pos -= len_l as f64;
            }
        }
        (out_l, out_r)
    }

    /// Meng de achtergrondgeluiden in een stereo-planair paar (L, R) van
    /// lengte `frames`. Loopt onvoorwaardelijk door; fade in/uit op basis
    /// van target_gain. Vooral voor tests.
    pub fn mix_stereo(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        if !self.is_actief() { return; }
        debug_assert_eq!(out_l.len(), out_r.len());
        let gg = self.global_gain;
        for v in &mut self.voices {
            let len_l = v.voice.data_l.len();
            if len_l == 0 { continue; }
            let data_r = v.voice.data_r.as_ref();
            let rate = v.voice.rate;
            let voice_g = v.voice.voice_gain * gg;
            let fade_step = voice_g / FADE_FRAMES.max(1.0);
            for i in 0..out_l.len() {
                // Fade naar target_gain * voice_g (effectieve target in audio).
                let effectief_doel = v.target_gain * voice_g;
                if (v.current_gain - effectief_doel).abs() < fade_step {
                    v.current_gain = effectief_doel;
                } else if v.current_gain < effectief_doel {
                    v.current_gain += fade_step;
                } else {
                    v.current_gain -= fade_step;
                }
                if v.current_gain <= 0.0 && effectief_doel <= 0.0 {
                    // Volledig uit en blijft uit — niets te mengen.
                    v.pos = (v.pos + rate) % len_l as f64;
                    continue;
                }
                // Lineaire interpolatie tussen pos0 en pos1 (mod len).
                let p = v.pos;
                let p0 = p.floor() as usize % len_l;
                let p1 = (p0 + 1) % len_l;
                let frac = (p - p.floor()) as f32;
                let sl = v.voice.data_l[p0] * (1.0 - frac) + v.voice.data_l[p1] * frac;
                let sr = match data_r {
                    Some(r) if !r.is_empty() => {
                        let len_r = r.len();
                        let r0 = p0 % len_r;
                        let r1 = (r0 + 1) % len_r;
                        r[r0] * (1.0 - frac) + r[r1] * frac
                    }
                    _ => sl,
                };
                out_l[i] += sl * v.current_gain;
                out_r[i] += sr * v.current_gain;
                v.pos += rate;
                if v.pos >= len_l as f64 {
                    v.pos -= len_l as f64;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maak_mono(len: usize, waarde: f32) -> BackgroundVoice {
        BackgroundVoice {
            kind: BackgroundSoundKind::Blower,
            data_l: Arc::new(vec![waarde; len]),
            data_r: None,
            rate: 1.0,
            voice_gain: 1.0,
        }
    }

    #[test]
    fn lege_engine_doet_niets() {
        let mut e = BackgroundEngine::new();
        let mut l = vec![0.0; 32];
        let mut r = vec![0.0; 32];
        e.mix_stereo(&mut l, &mut r);
        assert!(l.iter().all(|s| *s == 0.0));
        assert!(r.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn eén_mono_voice_fade_in_tot_doel() {
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        e.set_voices(vec![maak_mono(1000, 1.0)]);
        let mut l = vec![0.0; 20_000];
        let mut r = vec![0.0; 20_000];
        e.mix_stereo(&mut l, &mut r);
        // Fade duurt FADE_FRAMES ≈ 14400; na 20000 samples zit hij op doel.
        assert!((l[19_999] - 1.0).abs() < 1e-3, "{}", l[19_999]);
        // L == R want mono.
        assert!((r[19_999] - 1.0).abs() < 1e-3);
    }

    #[test]
    fn blower_uit_fade_uit() {
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        e.set_voices(vec![maak_mono(1000, 1.0)]);
        // Eerst even inlopen.
        let mut l = vec![0.0; 20_000];
        let mut r = vec![0.0; 20_000];
        e.mix_stereo(&mut l, &mut r);
        assert!(l[19_999] > 0.9, "{}", l[19_999]);
        // Nu motor uit — fade naar 0.
        e.set_blower(false);
        let mut l2 = vec![0.0; 20_000];
        let mut r2 = vec![0.0; 20_000];
        e.mix_stereo(&mut l2, &mut r2);
        assert!(l2[19_999].abs() < 1e-3, "{}", l2[19_999]);
    }

    #[test]
    fn loop_wrapt_rond() {
        // Een korte sample van 10 frames moet soepel loopen zonder vastlopen.
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        e.set_voices(vec![maak_mono(10, 0.5)]);
        let mut l = vec![0.0; 100];
        let mut r = vec![0.0; 100];
        e.mix_stereo(&mut l, &mut r);
        // Elke sample is 0.5, dus na fade-in moet alles ≈ 0.5 * fade zijn.
        // Belangrijk: geen NaN of panic.
        assert!(l.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn ambient_blijft_altijd_aan() {
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        let mut v = maak_mono(1000, 1.0);
        v.kind = BackgroundSoundKind::Ambient;
        e.set_voices(vec![v]);
        // Motor uit mag ambient niet raken.
        e.set_blower(false);
        let mut l = vec![0.0; 20_000];
        let mut r = vec![0.0; 20_000];
        e.mix_stereo(&mut l, &mut r);
        assert!(l[19_999] > 0.9, "ambient was mute: {}", l[19_999]);
    }

    #[test]
    fn stereo_voice_houdt_links_rechts_apart() {
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        let mut v = maak_mono(100, 1.0);
        v.data_r = Some(Arc::new(vec![-1.0; 100])); // tegengesteld rechts
        e.set_voices(vec![v]);
        let mut l = vec![0.0; 20_000];
        let mut r = vec![0.0; 20_000];
        e.mix_stereo(&mut l, &mut r);
        assert!(l[19_999] > 0.9);
        assert!(r[19_999] < -0.9, "rechts moet negatief zijn: {}", r[19_999]);
    }
}
