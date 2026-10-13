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
    /// Vriendelijke naam uit de orgeldefinitie — voor speelgoedregisters
    /// (0.7.98) de knop-tekst in de UI (Cymbelstern, Vogelsang, Cuculus).
    pub name: String,
    /// Linkerkanaal (of mono).
    pub data_l: Arc<Vec<f32>>,
    /// Rechterkanaal bij stereo-opname; None = mono (dan L→beide).
    pub data_r: Option<Arc<Vec<f32>>>,
    /// Afspeelsnelheid: bron-SR / output-SR. 1.0 = geen resample.
    pub rate: f64,
    /// Lineaire gain voor DEZE voice (voor set-up: bv. per sampleset
    /// verschillend). Default 1.0.
    pub voice_gain: f32,
    /// Loopgebied uit de smpl-chunk van de WAV, in bronframes (0.8.86).
    /// Een motor- of tremulantruis-opname bestaat vaak uit een aanloop
    /// (motor die opstart), een steady-state-loop en een staart; vóór 0.8.86
    /// loopten we het hele bestand, zodat de aanloop en de staart elke
    /// cyclus opnieuw te horen waren — "aan/uit/aan/uit". Nu spelen we van
    /// het begin tot `loop_end` en daarna rond tussen `loop_start` en
    /// `loop_end`. None of ongeldig = het hele bestand loopen (zoals vroeger).
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
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

impl ActiveVoice {
    /// (start, einde) van het loopgebied in bronframes: de smpl-loop als die
    /// geldig is, anders het hele bestand.
    #[inline]
    fn loop_bereik(&self) -> (usize, usize) {
        let len = self.voice.data_l.len();
        match (self.voice.loop_start, self.voice.loop_end) {
            (Some(ls), Some(le)) if le > ls + 1 && le <= len => (ls, le),
            _ => (0, len),
        }
    }

    /// Lineair geïnterpoleerd (L, R) op `pos`. Het tweede steunpunt wraps op
    /// het loop-einde naar de loop-start, zodat de naad geen klik geeft.
    /// Vereist `data_l` niet leeg.
    #[inline]
    fn lees(&self) -> (f32, f32) {
        let len = self.voice.data_l.len();
        let (ls, le) = self.loop_bereik();
        let p = self.pos;
        let p0 = (p.floor() as usize).min(len - 1);
        let p1 = if p0 + 1 >= le { ls } else { p0 + 1 }.min(len - 1);
        let frac = (p - p.floor()) as f32;
        let sl = self.voice.data_l[p0] * (1.0 - frac) + self.voice.data_l[p1] * frac;
        let sr = match self.voice.data_r.as_ref() {
            Some(r) if !r.is_empty() => {
                let r0 = p0.min(r.len() - 1);
                let r1 = p1.min(r.len() - 1);
                r[r0] * (1.0 - frac) + r[r1] * frac
            }
            _ => sl,
        };
        (sl, sr)
    }

    /// Schuif `rate` bronframes op; voorbij het loop-einde terug naar de
    /// loop-start, met behoud van de fractie.
    #[inline]
    fn stap(&mut self) {
        let (ls, le) = self.loop_bereik();
        self.pos += self.voice.rate;
        if self.pos >= le as f64 {
            let span = (le - ls) as f64;
            self.pos = ls as f64 + (self.pos - le as f64) % span;
        }
    }
}

/// Vier soorten achtergrondgeluiden; vaste volgorde voor de routing-array.
/// Zelfde volgorde als `BackgroundSoundKind`-indexer in `kind_index()`.
pub const BG_KIND_COUNT: usize = 4;

#[inline]
pub(crate) fn kind_index(kind: BackgroundSoundKind) -> usize {
    match kind {
        BackgroundSoundKind::Blower => 0,
        BackgroundSoundKind::Ambient => 1,
        BackgroundSoundKind::Toy => 2,
        BackgroundSoundKind::TremulantNoise => 3,
    }
}

/// Beheert alle doorlopende achtergrondgeluiden. Leeft in de audio-thread.
pub(crate) struct BackgroundEngine {
    voices: Vec<ActiveVoice>,
    /// Eenmalige geluiden (0.7.99): register-klikken bij aan/uit, één keer
    /// afspelen en dan weg. Verzameld na elke buffer: afgespeelde voices
    /// worden eruit gegooid.
    one_shots: Vec<ActiveVoice>,
    /// Multiplicatieve master voor alle bijgeluiden samen; wordt gevoed door
    /// de UI-schuif. 0.0 = helemaal uit, 1.0 = volledig; eventueel >1.0.
    pub global_gain: f32,
    /// Motor-knop: true = blower-voices actief, false = fade-uit.
    pub blower_active: bool,
    /// Routing per soort (0.8.82): welke output-kanalen krijgen deze bijgeluid-
    /// soort? Standaard [0, 1] = front L/R, net als voor 0.8.82. Op surround-
    /// opstellingen waar de divisies naar 3+4 of 5+6 gaan, kon de organist
    /// de bijgeluiden en effectregisters niet horen — die gingen hard naar 0/1.
    /// Nu per soort instelbaar: Motor, Omgevingsruis, Effectregisters, Tremulantruis.
    /// `one_shots` (registerklikken) blijven standaard op [0,1]; die horen bij
    /// de divisie van hun register en zouden eigenlijk mee moeten, maar dat is
    /// een aparte stap. Zonder opgegeven kanalen = front-paar.
    pub channels_per_kind: [Vec<u16>; BG_KIND_COUNT],
}

impl BackgroundEngine {
    pub fn new() -> Self {
        Self {
            voices: Vec::new(),
            one_shots: Vec::new(),
            // Standaard een beetje zacht, zodat de blower niet overheerst
            // als een organist voor het eerst een set met motor-geluid laadt.
            global_gain: 0.3,
            blower_active: true,
            channels_per_kind: [
                vec![0, 1], vec![0, 1], vec![0, 1], vec![0, 1],
            ],
        }
    }

    /// Routing per soort instellen (0.8.82). `channels` leeg of ongeldig →
    /// terug naar [0, 1].
    pub fn set_channels_for_kind(&mut self, kind: BackgroundSoundKind, mut channels: Vec<u16>) {
        if channels.is_empty() { channels = vec![0, 1]; }
        self.channels_per_kind[kind_index(kind)] = channels;
    }

    /// Speel een registermechaniek-klik eenmalig af (0.7.99). De voice wordt
    /// toegevoegd aan de one-shot-lijst en vanzelf opgeruimd wanneer hij
    /// doorlopen is. Geen fade-in: een klik hoort meteen te starten.
    pub fn fire_one_shot(&mut self, voice: BackgroundVoice) {
        // Max 32 tegelijk; bij een preset-wissel van 20 registers slaan we
        // niets over, maar als de wachtrij ophoopt droppen we de oudste.
        if self.one_shots.len() >= 32 {
            self.one_shots.remove(0);
        }
        let voice_g = voice.voice_gain * self.global_gain;
        self.one_shots.push(ActiveVoice {
            target_gain: 1.0,
            current_gain: voice_g, // meteen op volle gain, geen fade
            voice,
            pos: 0.0,
        });
    }

    /// Vervang alle voices door de nieuwe set (orgelwissel of eerste load).
    /// Geeft de OUDE voices terug zodat de aanroeper ze buiten de audio-
    /// thread kan droppen (een blower-opname is tientallen MB; free() in de
    /// callback is niet realtime-veilig — zelfde regel als LoadSamples).
    pub fn set_voices(&mut self, nieuw: Vec<BackgroundVoice>) -> Vec<ActiveVoice> {
        let oud = std::mem::take(&mut self.voices);
        self.voices = nieuw.into_iter().map(|v| ActiveVoice {
            target_gain: match v.kind {
                // Blower start uit als de motorknop uit staat.
                BackgroundSoundKind::Blower if !self.blower_active => 0.0,
                // Speelgoedregisters staan standaard uit: de organist drukt
                // ze zelf aan via een knop in de werkbalk (0.7.98). Tremulant-
                // ruis (0.8.4) volgt de tremulantknop en start dus ook uit.
                BackgroundSoundKind::Toy | BackgroundSoundKind::TremulantNoise => 0.0,
                _ => 1.0,
            },
            current_gain: 0.0, // begin stil, fade in
            voice: v,
            pos: 0.0,
        }).collect();
        oud
    }

    /// Alle voices wissen (bv. bij orgel-ontladen).
    pub fn clear(&mut self) {
        self.voices.clear();
        self.one_shots.clear();
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

    /// Speelgoedregister aan of uit (0.7.98). Match op naam (case-insensitief);
    /// zonder match verandert er niets.
    pub fn set_toy(&mut self, name: &str, active: bool) {
        let want = name.to_lowercase();
        for v in &mut self.voices {
            if matches!(v.voice.kind, BackgroundSoundKind::Toy | BackgroundSoundKind::TremulantNoise)
                && v.voice.name.to_lowercase() == want
            {
                v.target_gain = if active { 1.0 } else { 0.0 };
            }
        }
    }

    /// Zijn er voices die geluid produceren? Zo niet, dan kan de audio-
    /// thread de mix-stap overslaan.
    pub fn is_actief(&self) -> bool {
        (!self.voices.is_empty() || !self.one_shots.is_empty()) && self.global_gain > 1e-6
    }

    /// Meng direct in de INTERLEAVED output van cpal: elk frame bestaat uit
    /// `channels` samples. Voor elk voice-soort (motor / omgevingsruis /
    /// effectregister / tremulantruis) gelden eigen uitgangskanalen
    /// (`channels_per_kind`, standaard [0, 1] = front L/R). Zo kunnen
    /// bijgeluiden en effectregisters op surround-opstellingen daar klinken
    /// waar de organist ze wíl — meestal het front-paar, maar op een opstelling
    /// met één monitor voor de speeltafel bijvoorbeeld kanaal 3+4.
    /// Registerklikken (one_shots) delen de routing van hun eigen voice-kind
    /// (Toy = effectregister-klik, enz.); dat voorkomt dat je ze alleen op
    /// front hoort terwijl de effectregisters elders klinken.
    /// Bijgeluiden lopen buiten de master-limiter om; de clamping-stap aan
    /// het eind van de callback (`frame.clamp(-1.0, 1.0)`) begrenst wat we
    /// hier bijtellen alsnog.
    pub fn mix_interleaved(&mut self, data: &mut [f32], start_frame: usize, frames: usize, channels: usize) {
        if !self.is_actief() || channels == 0 { return; }
        let gg = self.global_gain;
        // Lokale buffer om per kind L/R te verzamelen vóór we naar de output
        // schrijven. Vier kinds × (L, R).
        let max_chan = channels;
        for i in 0..frames {
            let base = (start_frame + i) * channels;
            if base >= data.len() { break; }
            let mut per_kind: [(f32, f32); BG_KIND_COUNT] = [(0.0, 0.0); BG_KIND_COUNT];

            // Doorlopende voices.
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
                if v.current_gain > 0.0 || effectief_doel > 0.0 {
                    let (sl, sr) = v.lees();
                    let k = kind_index(v.voice.kind);
                    per_kind[k].0 += sl * v.current_gain;
                    per_kind[k].1 += sr * v.current_gain;
                }
                v.stap();
            }

            // One-shots: eenmalig; delen de routing van hun kind.
            for v in &mut self.one_shots {
                let len_l = v.voice.data_l.len();
                if len_l == 0 { continue; }
                let rate = v.voice.rate;
                if v.pos < len_l as f64 {
                    let p = v.pos;
                    let p0 = p.floor() as usize;
                    if p0 >= len_l { v.pos = len_l as f64; continue; }
                    let p1 = (p0 + 1).min(len_l - 1);
                    let frac = (p - p.floor()) as f32;
                    let sl = v.voice.data_l[p0] * (1.0 - frac) + v.voice.data_l[p1] * frac;
                    let sr = match v.voice.data_r.as_ref() {
                        Some(r) if r.len() > p1 => r[p0] * (1.0 - frac) + r[p1] * frac,
                        _ => sl,
                    };
                    let k = kind_index(v.voice.kind);
                    per_kind[k].0 += sl * v.current_gain;
                    per_kind[k].1 += sr * v.current_gain;
                    v.pos += rate;
                }
            }

            // Schrijven: per kind naar z'n eigen kanalenlijst. 1 kanaal =
            // mono-mix (L+R)/2; 2 of meer = eerste twee als stereo-paar en
            // volgende als duplicaat van het stereo-signaal (zelden gebruikt).
            for (k, (l, r)) in per_kind.iter().enumerate() {
                if *l == 0.0 && *r == 0.0 { continue; }
                let chans = &self.channels_per_kind[k];
                if chans.is_empty() {
                    // Mag niet voorkomen (set_channels_for_kind clampt), maar
                    // zekerheidshalve naar [0, 1].
                    let a = base;
                    let b = base + 1;
                    if a < data.len() { data[a] = (data[a] + *l).clamp(-1.0, 1.0); }
                    if b < data.len() && max_chan > 1 { data[b] = (data[b] + *r).clamp(-1.0, 1.0); }
                    continue;
                }
                if chans.len() == 1 {
                    let c = chans[0] as usize;
                    let a = base + c;
                    if c < max_chan && a < data.len() {
                        let m = (*l + *r) * 0.5;
                        data[a] = (data[a] + m).clamp(-1.0, 1.0);
                    }
                } else {
                    let c0 = chans[0] as usize;
                    let c1 = chans[1] as usize;
                    let a = base + c0;
                    let b = base + c1;
                    if c0 < max_chan && a < data.len() { data[a] = (data[a] + *l).clamp(-1.0, 1.0); }
                    if c1 < max_chan && b < data.len() { data[b] = (data[b] + *r).clamp(-1.0, 1.0); }
                }
            }
        }
        // Afgespeelde one-shots opruimen (0.7.99): pos is doorgaans ≥ len.
        self.one_shots.retain(|v| v.pos < v.voice.data_l.len() as f64);
    }

    /// Genereer één (L, R)-paar en schuif de voices één stap op. Mixt de
    /// doorlopende voices én de actieve one-shots samen.
    fn volgende_sample(&mut self) -> (f32, f32) {
        let gg = self.global_gain;
        let mut out_l = 0.0f32;
        let mut out_r = 0.0f32;
        // One-shots: eenmalig afspelen tot `pos >= len`. Opruiming daarvan
        // gebeurt na elke buffer in `mix_interleaved`.
        for v in &mut self.one_shots {
            let len_l = v.voice.data_l.len();
            if len_l == 0 { continue; }
            let rate = v.voice.rate;
            if v.pos < len_l as f64 {
                let p = v.pos;
                let p0 = p.floor() as usize;
                if p0 >= len_l { v.pos = len_l as f64; continue; }
                let p1 = (p0 + 1).min(len_l - 1);
                let frac = (p - p.floor()) as f32;
                let sl = v.voice.data_l[p0] * (1.0 - frac) + v.voice.data_l[p1] * frac;
                let sr = match v.voice.data_r.as_ref() {
                    Some(r) if r.len() > p1 => r[p0] * (1.0 - frac) + r[p1] * frac,
                    _ => sl,
                };
                out_l += sl * v.current_gain;
                out_r += sr * v.current_gain;
                v.pos += rate;
            }
        }
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
            if v.current_gain > 0.0 || effectief_doel > 0.0 {
                let (sl, sr) = v.lees();
                out_l += sl * v.current_gain;
                out_r += sr * v.current_gain;
            }
            v.stap();
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
                    v.stap();
                    continue;
                }
                let (sl, sr) = v.lees();
                out_l[i] += sl * v.current_gain;
                out_r[i] += sr * v.current_gain;
                v.stap();
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
            name: "test".into(),
            data_l: Arc::new(vec![waarde; len]),
            data_r: None,
            rate: 1.0,
            voice_gain: 1.0,
            loop_start: None,
            loop_end: None,
        }
    }

    /// Loop-punten worden gevolgd (0.8.86): na het loop-einde speelt de voice
    /// rond tussen start en einde, nooit meer de aanloop of de staart.
    #[test]
    fn loop_punten_worden_gevolgd() {
        // Bestand van 100 frames: aanloop 0..10 = 1.0, loopgebied 10..60 = 0.5,
        // staart 60..100 = -1.0.
        let mut data = vec![1.0f32; 100];
        for s in &mut data[10..60] { *s = 0.5; }
        for s in &mut data[60..] { *s = -1.0; }
        let mut v = maak_mono(100, 0.0);
        v.data_l = Arc::new(data);
        v.loop_start = Some(10);
        v.loop_end = Some(60);
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        e.set_voices(vec![v]);
        // Lang genoeg om de fade af te maken én meerdere loops te doorlopen.
        let mut l = vec![0.0; 40_000];
        let mut r = vec![0.0; 40_000];
        e.mix_stereo(&mut l, &mut r);
        // Na de fade-in zitten we allang in het loopgebied: alles 0.5, nooit
        // de staart (-1.0) of de aanloop (1.0).
        let staart = &l[20_000..];
        assert!(staart.iter().all(|&s| (s - 0.5).abs() < 1e-3),
            "loop moest op 0.5 blijven, min={} max={}",
            staart.iter().cloned().fold(f32::MAX, f32::min),
            staart.iter().cloned().fold(f32::MIN, f32::max));
    }

    /// Zonder (geldige) loop-punten loopt het hele bestand, zoals vóór 0.8.86.
    #[test]
    fn zonder_loop_punten_loopt_het_hele_bestand() {
        let mut data = vec![0.5f32; 100];
        for s in &mut data[60..] { *s = -1.0; }
        let mut v = maak_mono(100, 0.0);
        v.data_l = Arc::new(data);
        v.loop_start = Some(80);
        v.loop_end = Some(500); // voorbij het bestand → ongeldig → heel bestand
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        e.set_voices(vec![v]);
        let mut l = vec![0.0; 40_000];
        let mut r = vec![0.0; 40_000];
        e.mix_stereo(&mut l, &mut r);
        let staart = &l[20_000..];
        assert!(staart.iter().any(|&s| s < -0.9), "staart (-1.0) moest nog meelopen");
    }

    fn maak_toy(len: usize, waarde: f32, name: &str) -> BackgroundVoice {
        let mut v = maak_mono(len, waarde);
        v.kind = BackgroundSoundKind::Toy;
        v.name = name.into();
        v
    }

    #[test]
    fn speelgoed_begint_uit_en_gaat_aan_via_set_toy() {
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        e.set_voices(vec![maak_toy(1000, 1.0, "Cymbelstern")]);
        // Zonder set_toy klinkt het register niet: target_gain is 0 bij start.
        let mut l = vec![0.0; 20_000];
        let mut r = vec![0.0; 20_000];
        e.mix_stereo(&mut l, &mut r);
        assert!(l[19_999].abs() < 1e-3, "toy moest stil blijven: {}", l[19_999]);
        // Nu aanklikken.
        e.set_toy("Cymbelstern", true);
        let mut l2 = vec![0.0; 20_000];
        let mut r2 = vec![0.0; 20_000];
        e.mix_stereo(&mut l2, &mut r2);
        assert!(l2[19_999] > 0.9, "toy moest doorlopen: {}", l2[19_999]);
        // Uit → fade terug naar 0.
        e.set_toy("Cymbelstern", false);
        let mut l3 = vec![0.0; 20_000];
        let mut r3 = vec![0.0; 20_000];
        e.mix_stereo(&mut l3, &mut r3);
        assert!(l3[19_999].abs() < 1e-3, "toy moest uit zijn: {}", l3[19_999]);
    }

    #[test]
    fn one_shot_speelt_eenmaal_en_wordt_opgeruimd() {
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        e.fire_one_shot(BackgroundVoice {
            kind: BackgroundSoundKind::Ambient,
            name: "klik".into(),
            data_l: Arc::new(vec![0.5; 100]),
            data_r: None,
            rate: 1.0,
            voice_gain: 1.0,
            loop_start: None,
            loop_end: None,
        });
        // Eén buffer van 100 samples is net genoeg om de klik af te spelen.
        let mut data = vec![0.0f32; 200]; // 100 frames × 2 ch
        e.mix_interleaved(&mut data, 0, 100, 2);
        // Er zou geluid moeten zijn in de eerste 100 frames.
        assert!(data[0] > 0.1, "sample 0: {}", data[0]);
        // Daarna is de one-shot op; nog een buffer mixen levert niets meer op.
        let mut data2 = vec![0.0f32; 200];
        e.mix_interleaved(&mut data2, 0, 100, 2);
        assert!(data2.iter().all(|&s| s == 0.0), "one-shot moest opgeruimd zijn");
    }

    #[test]
    fn set_toy_matcht_hoofdlettergevoelloos_en_laat_andere_voices_met_rust() {
        let mut e = BackgroundEngine::new();
        e.global_gain = 1.0;
        e.set_voices(vec![
            maak_toy(100, 1.0, "Vogelsang"),
            maak_toy(100, 1.0, "Cymbelstern"),
        ]);
        // Alleen Vogelsang (met verschillende hoofdletters) aanklikken.
        e.set_toy("vogelsang", true);
        let mut l = vec![0.0; 20_000];
        let mut r = vec![0.0; 20_000];
        e.mix_stereo(&mut l, &mut r);
        // Mix bevat alleen vogelsang: 1 van 2 voices actief, dus ongeveer 1.0
        // (equal-gain mix). Cymbelstern moet 0 blijven.
        assert!(l[19_999] > 0.9, "vogelsang moest doorlopen");
        // Zet nu Cymbelstern aan: dan hoor je beide, mix wordt groter.
        e.set_toy("Cymbelstern", true);
        let mut l2 = vec![0.0; 20_000];
        let mut r2 = vec![0.0; 20_000];
        e.mix_stereo(&mut l2, &mut r2);
        assert!(l2[19_999] > 1.8, "beide toys actief, mix groter: {}", l2[19_999]);
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
