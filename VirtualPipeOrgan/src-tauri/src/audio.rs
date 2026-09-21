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
/// Masker om van een release-pipe_num weer de kale pijp te maken: de vlag
/// (bit 31) plus de release-index (bits 24-30) van de MaxKeyPressTime-
/// varianten (0.7.26) én de tremulant-vlag (bit 30). Gebruik dit — niet alleen
/// de vlag — bij voicing-lookups.
pub const RELEASE_KEY_MASK: u32 = 0xFF00_0000;

/// Bit-vlag op `pipe_num` die een key markeert als TREMULANT-opname van die
/// pijp (GrandOrgue `IsTremulant=1`, Hauptwerk "tremmed"-laag, JM-Rec
/// `_trem`-map). Attack én release van de tremulantstand krijgen zo hun eigen
/// sleutel in dezelfde preload-/sample-/inflight-maps als de droge opname —
/// zonder aparte maps, en zonder dat een achtergrond-load van de ene stand die
/// van de andere overschrijft.
///
/// Bitindeling van `pipe_num` (samen met `RANK_MASK`/`RELEASE_KEY_MASK`):
///   bits 0-15  pijpindex binnen de stop (ODF-cap 1024 + 512)
///   bits 16-23 laagindex (gestapelde rank / microfoonperspectief)
///   bits 24-26 release-index van de MaxKeyPressTime-variant ((idx+1) << 24)
///   bit  30    tremulant-opname (deze vlag)
///   bit  31    release-sample van de pijp (`RELEASE_PIPE_FLAG`)
/// Bit 30 valt binnen `RELEASE_KEY_MASK`, dus `base_pipe()` en de
/// intonatie-/hertemper-sleutel (`& !RELEASE_KEY_MASK`) strippen hem vanzelf:
/// een tremulant-voice erft de voicing van zijn droge pijp.
pub const TREM_FLAG: u32 = 0x4000_0000;

/// Laagindex (gestapelde rank / microfoonperspectief, 0.7.38) in bits 16–23
/// van pipe_num; de pijpindex blijft in bits 0–15 (ODF-cap 1024 + 512). Alle
/// engine-maps (preloads, trem, releases, release_meta, odf_voicings,
/// inflight_loads) werken ongewijzigd per laagsleutel; alleen de NoteOn-
/// fan-out en de NoteOff-match kennen de lagen.
pub const RANK_SHIFT: u32 = 16;
pub const RANK_MASK: u32 = 0x00FF_0000;
/// Maximaal aantal lagen per stop (laag 0 + 15 extra); tevens het aantal
/// perspectief-gain-slots (slot 0 = geen perspectief, altijd 1.0).
pub const MAX_LAYERS: usize = 16;
/// Laadplan-default voor stops zonder lagen: alleen laag 0, slot 0.
static DEFAULT_LAYOUT: [(u8, u8); 1] = [(0, 0)];

/// Engine-sleutel van pijp `pipe_num` in laag `layer`.
#[inline]
pub fn layer_key(pipe_num: u32, layer: u8) -> u32 {
    debug_assert!(
        pipe_num & (RANK_MASK | RELEASE_KEY_MASK) == 0 && (layer as usize) < MAX_LAYERS,
        "layer_key: pipe_num {:#x} / laag {} buiten bereik", pipe_num, layer
    );
    pipe_num | ((layer as u32) << RANK_SHIFT)
}

/// Kale pijpindex (zonder laag- en release-bits) — voor de gebruikers-voicing
/// (VoicingPanel keyt op de kale pipe_num) en de NoteOff-match over lagen.
#[inline]
pub fn base_pipe(pipe_num: u32) -> u32 {
    pipe_num & !(RANK_MASK | RELEASE_KEY_MASK)
}

#[cfg(test)]
mod layer_key_tests {
    use super::*;

    #[test]
    fn layer_key_zet_laag_in_bits_16_23() {
        assert_eq!(layer_key(5, 0), 5);
        assert_eq!(layer_key(5, 2), 0x0002_0005);
        assert_eq!(layer_key(1024, 15), 0x000F_0400);
        // Geen overlap met de release-bits (24–31): een release-variant op een
        // laagsleutel houdt beide intact.
        assert_eq!(layer_key(1024, 15) & RELEASE_KEY_MASK, 0);
        let rel = layer_key(7, 3) | RELEASE_PIPE_FLAG | (3 << 24);
        assert_eq!(rel & RANK_MASK, 3 << RANK_SHIFT);
        assert_eq!(rel & RELEASE_KEY_MASK, RELEASE_PIPE_FLAG | (3 << 24));
    }

    #[test]
    fn base_pipe_strip_laag_en_release_bits() {
        assert_eq!(base_pipe(layer_key(5, 2) | RELEASE_PIPE_FLAG | (3 << 24)), 5);
        assert_eq!(base_pipe(layer_key(1536, 15)), 1536);
        assert_eq!(base_pipe(42), 42);
        // Laagsleutel zonder release-bits blijft de ODF-intonatie-sleutel.
        assert_eq!(layer_key(9, 4) & !RELEASE_KEY_MASK, layer_key(9, 4));
    }

    #[test]
    fn trem_flag_ligt_binnen_het_release_masker() {
        // De vlag moet door base_pipe() en de intonatiesleutel gestript worden,
        // anders krijgt een tremulant-voice geen ODF-gain/hertemper-correctie.
        assert_eq!(TREM_FLAG & RELEASE_KEY_MASK, TREM_FLAG);
        assert_eq!(TREM_FLAG & (RANK_MASK | 0xFFFF), 0);
        assert_eq!(base_pipe(layer_key(5, 2) | TREM_FLAG), 5);
        assert_eq!(base_pipe(layer_key(5, 2) | TREM_FLAG | RELEASE_PIPE_FLAG | (3 << 24)), 5);
        let k = layer_key(9, 4);
        assert_eq!((k | TREM_FLAG) & !RELEASE_KEY_MASK, k);
        // Geen botsing met de release-index (idx+1) << 24 voor idx < 7.
        for idx in 0u32..7 {
            assert_eq!(((idx + 1) << 24) & TREM_FLAG, 0);
        }
        // En de laag blijft intact naast de vlag.
        assert_eq!((layer_key(7, 3) | TREM_FLAG) & RANK_MASK, 3 << RANK_SHIFT);
    }
}

/// Harde bovengrens op het aantal gelijktijdige voices in het echte audiopad.
/// Dit is een vangnet tegen op hol geslagen groei (blijvende noten, koppel-
/// lussen, een tutti + release-staarten) — niet een muzikale limiet: bij een
/// normaal orgel blijf je er ruim onder. Zonder deze grens groeide de voice-Vec
/// onbeperkt en kon één zwaar akkoord de render-thread over de buffer-deadline
/// duwen → underruns → gekraak. Bij overschrijding wordt de zachtste voice
/// gestolen (voice-stealing). Gekoppeld aan de workspace-brede polyfonie-
/// constante zodat er maar één waarheid is.
pub const MAX_LIVE_VOICES: usize = vpo_audio::MAX_POLYPHONY;
/// Ondergrens van de instelbare kap.
pub const MIN_LIVE_VOICES: usize = 256;

/// Instelbare polyfonie-kap (Algemene instellingen → Audio-uitvoer). Proces-
/// breed zodat een audio-wissel (nieuwe AudioPlayer) hem automatisch overneemt;
/// de render-lus leest hem één keer per callback.
static POLYPHONY_TARGET: AtomicUsize = AtomicUsize::new(vpo_audio::DEFAULT_POLYPHONY);
pub fn set_polyphony_target(n: usize) {
    POLYPHONY_TARGET.store(n.clamp(MIN_LIVE_VOICES, MAX_LIVE_VOICES), Ordering::Relaxed);
}
pub fn polyphony_target() -> usize {
    POLYPHONY_TARGET.load(Ordering::Relaxed).clamp(MIN_LIVE_VOICES, MAX_LIVE_VOICES)
}

/// Telt de deel-opteltabellen van de stukken 1.. op bij die van stuk 0.
///
/// Losse functie omdat dit het enige stuk nieuwe rekenwerk is dat het verdelen
/// met zich meebrengt, en omdat de volgorde ertoe doet: drijvende-kommasommen
/// zijn niet associatief, dus de stukken worden altijd in dezelfde volgorde
/// opgeteld (0, dan 1, dan 2, …). Daardoor is het resultaat bij een gelijk
/// aantal stukken reproduceerbaar — het verschilt wél in de laatste decimaal
/// van de som op één stuk.
///
/// `n_div` is het aantal divisies dat dit orgel echt heeft; de tabellen zijn
/// altijd 32 breed, maar de rest is nul en hoeft niet aangeraakt.
#[inline]
pub(crate) fn reduceer_deeltabellen(
    doel_l: &mut [[f32; 32]],
    doel_r: &mut [[f32; 32]],
    delen_l: &[Vec<[f32; 32]>],
    delen_r: &[Vec<[f32; 32]>],
    stukken: usize,
    n_div: usize,
    bn: usize,
) {
    let n_div = n_div.min(32);
    for k in 0..stukken.saturating_sub(1) {
        let (pl, pr) = (&delen_l[k], &delen_r[k]);
        for f in 0..bn {
            for d in 0..n_div {
                doel_l[f][d] += pl[f][d];
                doel_r[f][d] += pr[f][d];
            }
        }
    }
}

/// Hoogste aantal stukken waarin de mengloop zijn stemmen verdeelt. Elk stuk
/// heeft een eigen opteltabel; zie `MENG_STUKKEN`.
pub const MAX_MENG_STUKKEN: usize = 8;

/// In hoeveel stukken de stemmen van een blok verdeeld worden (fase 2 van de
/// meerkernige mengloop). 1 = precies het oude gedrag: één opteltabel, geen
/// reductie, bit-identiek resultaat.
///
/// Vanaf 2 krijgt elk stuk zijn eigen opteltabel die daarna in vaste volgorde
/// wordt opgeteld. In fase 2 gebeurt dat nog gewoon achter elkaar op de
/// audiothread — de structuur is dan klaar, en het verschil in optelvolgorde
/// (drijvende komma is niet associatief) is los te beoordelen vóórdat er een
/// thread bij komt.
static MENG_STUKKEN: AtomicUsize = AtomicUsize::new(1);

pub fn set_meng_stukken(n: usize) {
    MENG_STUKKEN.store(n.clamp(1, MAX_MENG_STUKKEN), Ordering::Relaxed);
}

pub fn meng_stukken() -> usize {
    MENG_STUKKEN.load(Ordering::Relaxed).clamp(1, MAX_MENG_STUKKEN)
}

/// Onder dit aantal klinkende stemmen blijft de mengloop op één stuk: de
/// reductie (en straks de barrière) kost dan meer dan het verdelen oplevert.
/// Wordt in fase 1 gemeten en zo nodig bijgesteld.
pub const MENG_DREMPEL_STEMMEN: usize = 64;

/// Testsignaal voor het opzoeken van luidsprekers (0.7.47). Wie zes of acht
/// kanalen aansluit weet daarna nog niet welke stekker in welke kast zit; dit
/// stuurt een herkenbaar signaal naar precies één uitgang. Proces-breed en
/// atomair, zodat de knop meteen werkt en een audio-wissel hem overneemt.
///
/// Codering: 0 = uit; anders `(kanaal + 1) | (soort << 16)`.
/// Soort 0 = roze ruis (de standaard om luidsprekers uit te zoeken: breedbandig
/// en niet schel), soort 1 = sinus 440 Hz (voor het natrekken van een kanaal op
/// een meetapparaat).
static TEST_SIGNAL: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Niveau van het testsignaal als f32-bits (piek, 0..1).
static TEST_SIGNAL_GAIN: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

/// Zet het testsignaal aan op `channel` of uit (`None`). `gain` is een piek
/// 0..1; de render-lus klemt hem nog een keer.
pub fn set_test_signal(channel: Option<u8>, kind: u8, gain: f32) {
    match channel {
        Some(ch) => {
            TEST_SIGNAL_GAIN.store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
            TEST_SIGNAL.store((ch as u32 + 1) | ((kind.min(1) as u32) << 16), Ordering::Relaxed);
        }
        None => TEST_SIGNAL.store(0, Ordering::Relaxed),
    }
}

/// (kanaal, soort, piekniveau) van het lopende testsignaal, of None.
pub fn test_signal() -> Option<(u8, u8, f32)> {
    let v = TEST_SIGNAL.load(Ordering::Relaxed);
    if v == 0 { return None; }
    let ch = ((v & 0xFFFF) - 1) as u8;
    let kind = ((v >> 16) & 0xFF) as u8;
    let gain = f32::from_bits(TEST_SIGNAL_GAIN.load(Ordering::Relaxed)).clamp(0.0, 1.0);
    Some((ch, kind, gain))
}

/// Roze-ruisgenerator (Paul Kellett, 3-polige benadering) plus een sinusfase.
/// Klein genoeg om op de audio-thread te draaien zonder allocatie.
#[derive(Default)]
pub(crate) struct Testsignaal {
    rng: u32,
    b0: f32,
    b1: f32,
    b2: f32,
    fase: f32,
}

impl Testsignaal {
    pub(crate) fn nieuw() -> Self {
        Testsignaal { rng: 0x2545_F491, ..Default::default() }
    }

    /// Eén sample. `kind` 0 = roze ruis, 1 = sinus 440 Hz.
    pub(crate) fn sample(&mut self, kind: u8, sample_rate: u32) -> f32 {
        if kind == 1 {
            let stap = 440.0 * std::f32::consts::TAU / sample_rate.max(1) as f32;
            self.fase += stap;
            if self.fase >= std::f32::consts::TAU { self.fase -= std::f32::consts::TAU; }
            return self.fase.sin();
        }
        // xorshift32 → wit in -1..1
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        let wit = (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
        self.b0 = 0.99765 * self.b0 + wit * 0.0990460;
        self.b1 = 0.96300 * self.b1 + wit * 0.2965164;
        self.b2 = 0.57000 * self.b2 + wit * 1.0526913;
        // Deler: houdt de piek van de som ruwweg binnen ±1.
        (self.b0 + self.b1 + self.b2 + wit * 0.1848) * 0.2
    }
}

/// Belasting van de render-thread: rendertijd / beschikbare buffertijd, als
/// promille (EMA en piek over de laatste seconden). Boven ~800‰ dreigen
/// underruns — de UI toont dit naast het stemmenaantal zodat de gebruiker de
/// kap bewust kan kiezen.
static RENDER_LOAD_PM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static RENDER_PEAK_PM: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn render_load() -> (f32, f32) {
    (RENDER_LOAD_PM.load(Ordering::Relaxed) as f32 / 1000.0,
     RENDER_PEAK_PM.load(Ordering::Relaxed) as f32 / 1000.0)
}

/// Cumulatief aantal callbacks dat over 80 % van zijn buffertijd ging, sinds het
/// starten van de stream (nooit gereset). Stond eerder alleen als closure-lokale
/// teller in de warn-tekst en was daardoor niet meetbaar; nu leest de status
/// (UI + test-API) hem uit, zodat "hapert het?" een getal wordt in plaats van een
/// gevoel: neemt hij toe tijdens een crescendo-sweep, dan is de buffer te klein.
static RENDER_OVERLOAD_COUNT: AtomicU64 = AtomicU64::new(0);
pub fn render_overload_count() -> u64 {
    RENDER_OVERLOAD_COUNT.load(Ordering::Relaxed)
}

/// Rendertijd per pas van de mengloop, in tienduizendsten van de buffertijd
/// (EMA over 16 callbacks) — fase 1 van de meerkernige mengloop.
///
/// Fijner dan de totale belastingsmeter (promille), omdat juist de goedkope
/// passen interessant zijn: de reductie van de deeltabellen kost op deze
/// machine minder dan een promille en zou anders altijd als 0 verschijnen.
///
/// De mengloop werkt in blokken met drie passen: (1) wind en tremulant per
/// frame, (2) de stemmen, (3) zwelkast/routering/galm/EQ/limiter per frame.
/// Alleen pas 2 is parallel te maken, dus de verhouding bepaalt wat dat
/// maximaal kan opleveren. Pas 4 is de reductie van de deel-opteltabellen
/// (fase 2); die is de prijs van het parallelliseren.
///
/// De meting kost zes kloklezingen per blok (~300 ns) en staat daarom altijd
/// aan: op een buffer van 10 ms is dat 0,003 %.
static RENDER_PASS_PM: [std::sync::atomic::AtomicU32; 4] = [
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0),
    std::sync::atomic::AtomicU32::new(0),
];

/// (wind/tremulant, stemmen, keten, reductie) als fractie van de buffertijd.
pub fn render_pass_load() -> (f32, f32, f32, f32) {
    let v = |i: usize| RENDER_PASS_PM[i].load(Ordering::Relaxed) as f32 / 10_000.0;
    (v(0), v(1), v(2), v(3))
}

/// Framegrootte van de laatste audio-callback (0 vóór de eerste callback).
/// Onafhankelijk van de gevráágde buffer: houdt de driver zijn eigen paneel-
/// instelling aan ("Requested buffer … not honored"), dan is buffer_frames in
/// de status 0 terwijl hier het echte getal staat — het bufferadvies in de UI
/// kijkt hiernaar (0.7.43).
static RENDER_FRAMES_NOW: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn render_frames_now() -> u32 {
    RENDER_FRAMES_NOW.load(Ordering::Relaxed)
}

/// Diagnose-teller: hoe vaak een release-staart het EINDE van zijn preload
/// bereikte terwijl de volledige WAV nog niet geladen was. Elke treffer is een
/// staart die op ~2 s werd afgekapt (laadlatentie bij grote akkoorden). Alleen
/// een atomaire optelling in de callback — de watchdog-thread logt de stand
/// hooguit 1x per 5 s, zodat er geen log-I/O in het audiopad komt.
static RELEASE_PRELOAD_EDGE_HITS: AtomicU64 = AtomicU64::new(0);

/// Diagnose-tellers voor tremulantwissels (zelfde patroon als hierboven). Het
/// loggen van een wissel hoorde in de commandolus van de audio-callback thuis
/// noch binnen de write-guard op `trem_active`: de logschrijver is ongebufferd
/// en zit achter een mutex, en zolang die guard openstaat kan géén NoteOn de
/// tremulantstand van zijn register opzoeken. De callback telt nu alleen; de
/// watchdog-thread meldt hooguit 1x per 5 s hoeveel wissels er waren en hoeveel
/// registers op tremulant staan.
static TREMULANT_SWITCHES: AtomicU64 = AtomicU64::new(0);
static TREMULANT_ACTIVE_STOPS: AtomicU64 = AtomicU64::new(0);

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
    /// Toetsduur-afhankelijke releases (GrandOrgue MaxKeyPressTime, 0.7.26):
    /// per (stop, pijp) een oplopend gesorteerde lijst (max_ms, release-key-
    /// pipe_num); -1 = onbegrensd (default) en staat achteraan. NoteOff kiest
    /// de eerste entry waarvan max_ms ≥ de gespeelde duur.
    RegisterReleaseMeta(std::collections::HashMap<(u32, u32), Vec<(i32, u32)>>),
    /// Stop-ids die one-shot afspelen (ODF Percussive: klok/glockenspiel —
    /// niet loopen maar één keer uitklinken).
    RegisterPercussiveStops(std::collections::HashSet<u32>),
    /// Note off: (stop_id, pipe_num)
    NoteOff { stop_id: u32, pipe_num: u32 },
    /// Laat ALLE klinkende pijpen van één register los (register weggetrokken
    /// tijdens het spelen). Eén commando i.p.v. een NoteOff per pijp: een
    /// preset-wissel die 20 registers wegtrekt zou anders >1000 berichten
    /// sturen en de (bounded) command-queue laten vollopen.
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
    /// Stop-ids met echte tremulant-OPNAMEN. De buffers zelf staan sinds 0.7.39
    /// gewoon in de preload-map onder `pipe_num | TREM_FLAG`; deze set zegt
    /// alleen voor welke registers de synth-LFO moet zwijgen (die stops hebben
    /// hun tremulant al in de opname).
    RegisterTremStops(std::collections::HashSet<u32>),
    /// Door de sampleset opgegeven crossfade-duur (ms) per RELEASE-key
    /// (GrandOrgue `ReleaseCrossfadeLength`). Ontbreekt een key, dan geldt de
    /// automatische, toonhoogte-afhankelijke duur (`release_fade_ms`).
    RegisterReleaseCrossfade(Arc<HashMap<(u32, u32), u32>>),
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
    /// Set temperament: 12 cent offsets [C,C#,D,...,B] + global fine-tuning in cents.
    /// `retune` = hertemperen per pijp vanaf de gemeten toonhoogte (de
    /// RegisterOdfRetune-tabel vervangt dan de ODF-PitchTuning); false =
    /// "Origineel (zoals opgenomen)".
    SetTemperament { note_offsets: [f32; 12], fine_tune: f32, retune: bool },
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
    SetDivisionOutputChannels { division_index: u8, channels: Vec<u16> },
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
    /// Hertemper-tabel: per (stop_id, pipe_num) de cents die in hertemper-
    /// modus (SetTemperament.retune) de ODF-PitchTuning van die pijp
    /// VERVANGEN — (doeltoon − gemeten samplepitch) + PitchCorrection. Bevat
    /// alleen pijpen met een plausibele meting; de rest blijft op PitchTuning.
    RegisterOdfRetune(Arc<HashMap<(u32, u32), f32>>),
    /// ODF-looppunten per bestand (GrandOrgue-regel: ODF wint van smpl-chunk).
    /// Nodig zodat óók de achtergrond-full-sample-load dezelfde loop gebruikt
    /// als de preload — anders verspringt de loop op het upgrademoment.
    RegisterOdfLoops(Arc<HashMap<PathBuf, (u32, u32)>>),
    /// Per stop de te spawnen lagen bij een NoteOn: (laagindex, perspectief-
    /// slot 0..15; 0 = geen perspectief). Stops zonder entry → [(0, 0)].
    /// Altijd sturen bij een load (ook leeg): vervangt de map van het vorige
    /// orgel. Arc-swap in de audio-thread, geen kloon in de callback.
    RegisterRankLayout(Arc<HashMap<u32, Vec<(u8, u8)>>>),
    /// Lineaire gain per perspectief-slot (live, geen herlaad). Slot 0 is
    /// altijd 1.0 en wordt genegeerd.
    SetPerspectiveGain { slot: u8, gain_db: f32 },
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
    /// Aantal actieve triggers voor deze pijp (directe route + koppels).
    /// Eén luchtkolom per pijp: een NoteOn op een reeds klinkende pijp bumpt
    /// deze teller i.p.v. een tweede voice te spawnen; NoteOff decrementeert
    /// en releast pas bij 0. Voorkomt (i) verdubbelde amplitude/comb-filter
    /// wanneer een pijp via meerdere bronnen (bv. direct + koppel) klinkt en
    /// (ii) het "onkoppelt"-effect waarbij één bron loslaten de pijp stilzet
    /// terwijl een andere bron nog vasthoudt.
    note_on_count: u32,
    /// Perspectief-gain-slot (0 = geen perspectief). Release- en crossfade-
    /// voices erven dit van de voice waaruit ze ontstaan, zodat de
    /// perspectief-gain niet wegvalt bij loslaten of een tremulant-wissel.
    persp_slot: u8,
    /// Uitgestelde staart-decay (in output-frames) voor een geschaalde release
    /// bij een korte noot, zoals GrandOrgue's GOSoundFader: de release fadet
    /// eerst IN (complementair aan de uitfadende speelnoot) en begint pas
    /// daarna aan zijn eigen uitsterving. Zolang dit Some is, is de voice nog
    /// aan het infaden en telt hij niet mee als slachtoffer voor het
    /// staartbudget.
    pending_decay_frames: Option<u32>,
    /// Leeftijd (`age_samples`) waarop de lopende staart-decay op 0 hoort te
    /// staan. Wordt gezet zodra de decay begint, en gebruikt door
    /// `upgrade_to_full` om de fadesnelheid opnieuw te berekenen wanneer de
    /// volledige sample halverwege de decay binnenkomt.
    decay_end_age: Option<u64>,
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
            note_on_count: 1,
            persp_slot: 0,
            pending_decay_frames: None,
            decay_end_age: None,
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
            note_on_count: 1,
            persp_slot: 0,
            pending_decay_frames: None,
            decay_end_age: None,
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
        // Loopt er een staart-decay, dan was de fadesnelheid mogelijk ingekort
        // tot de preload-rand (clamp_release_to_buffer): met de volledige
        // sample erbij is er weer data genoeg, dus de decay mag zijn
        // oorspronkelijke tempo terugkrijgen — anders sterft een 5 s-staart in
        // ~1,8 s uit ("galm stopt abrupt vroeg"). De clamp blijft daarna staan
        // als vangrail voor het einde van de sample.
        if let Some(end) = self.decay_end_age {
            let rest = end.saturating_sub(self.age_samples).max(8) as f32;
            self.envelope_speed = self.envelope.max(1e-6) / rest;
            self.clamp_release_to_buffer();
        }
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
            note_on_count: 1,
            persp_slot: 0,
            pending_decay_frames: None,
            decay_end_age: None,
        }
    }

    /// Rekent de HUIDIGE afspeelpositie van deze stem om naar het domein van
    /// `dst` — de doelbuffer van een tremulant-crossfade (de droge en de
    /// tremulant-opname van dezelfde pijp wisselen elkaar af).
    ///
    /// Een positie is nooit universeel:
    ///   * `Full`    — frames van de VOLLEDIGE, naar de uitgangs-samplerate
    ///                 geresamplede sample; frame 0 = begin van het bestand.
    ///   * preload   — frames van `attack_data` op de BRON-samplerate, waarbij
    ///                 frame 0 hoort bij bestandsframe `trim_start`
    ///                 (segment-start + weggeknipte aanloopstilte).
    ///
    /// De doelbuffer is een ándere opname: eigen lengte, eigen `trim_start` en
    /// mogelijk een andere samplerate. De ruwe positie doorgeven liet de nieuwe
    /// stem daardoor op een willekeurige plek landen — bij een 44,1 kHz-set op
    /// een 48 kHz-uitgang en een lang vastgehouden noot ruim voorbij het einde
    /// van de (korte) trem-preload, dus stilte of een terugspringende attack.
    ///
    /// De omrekening gaat daarom via de TIJD, het enige domein dat beide
    /// opnamen delen:
    ///   1. positie → seconden sinds het begin van de opname
    ///      (preload: positie + eigen trim, gedeeld door de bronrate;
    ///       full: positie gedeeld door de uitgangsrate — die sample staat al
    ///       op uitgangsrate en is niet getrimd);
    ///   2. seconden → frames van de doelbuffer (× doelrate, − diens trim);
    ///   3. klemmen binnen de data die er écht ligt.
    /// Valt de uitkomst buiten de buffer, dan vouwen we hem terug in de loop van
    /// de doelbuffer: een vastgehouden noot klinkt daar sowieso, dus elke plek
    /// in de loop is muzikaal juist. Heeft de doelbuffer geen bruikbare loop,
    /// dan het midden van de buffer — nooit opnieuw door de attack, nooit
    /// voorbij het einde.
    fn crossfade_position_in(&self, dst: &PreloadBuffer, output_sample_rate: u32) -> f64 {
        let dst_len = dst.attack_data.len();
        if dst_len < 2 {
            return 0.0;
        }
        let max_pos = (dst_len - 2) as f64;
        // 1. Huidige positie → seconden sinds het begin van de bronopname.
        let secs = match &self.source {
            VoiceSampleSource::Full(_) => self.position / output_sample_rate.max(1) as f64,
            VoiceSampleSource::PreloadOnly { preload, .. } => {
                (self.position + preload.trim_start as f64)
                    / preload.sample_rate.max(1) as f64
            }
        };
        // 2. Seconden → frames van de doelbuffer (frame 0 = diens trim_start).
        let mut pos = secs * dst.sample_rate.max(1) as f64 - dst.trim_start as f64;
        // 3. Buiten de beschikbare data? Terugvouwen in de loop van het doel.
        //    Alleen looppunten die écht in de buffer passen tellen (dezelfde
        //    voorwaarde als de leeslus in `next_sample`).
        if !pos.is_finite() || pos < 0.0 || pos > max_pos {
            let loop_region = match (dst.loop_start, dst.loop_end) {
                (Some(a), Some(b)) if b > a + 1 && (b as usize) <= dst_len => {
                    Some((a as f64, b as f64))
                }
                _ => None,
            };
            pos = match loop_region {
                Some((ls, le)) if pos.is_finite() && pos > ls => ls + (pos - ls) % (le - ls),
                Some((ls, _)) => ls,
                None => dst_len as f64 * 0.5,
            };
        }
        pos.clamp(0.0, max_pos)
    }

    fn release(&mut self) {
        self.releasing = true;
        self.target_envelope = 0.0;
        self.envelope_speed = 0.0005;
        self.clamp_release_to_buffer();
    }

    /// Zorg dat de lopende uitfade de beschikbare sampledata niet overschrijdt.
    /// Tijdens de release loopt de voice niet meer (read_voice speelt lineair
    /// uit), dus er is alleen data tot het buffereinde; duurt de fade langer,
    /// dan kapt `i >= len - 1` de klank hard af op een willekeurige golfwaarde →
    /// tik bij het loslaten. Verhoogt zonodig de fadesnelheid; verlaagt nooit.
    fn clamp_release_to_buffer(&mut self) {
        let avail = self.available_len() as f64;
        if avail <= 0.0 { return; }
        let rest_src = (avail - 1.0 - self.position).max(0.0);
        let rest_out = ((rest_src / self.rate.max(1e-6)) as f32 * 0.9).max(8.0);
        let min_speed = self.envelope.max(1e-6) / rest_out;
        if self.envelope_speed < min_speed {
            self.envelope_speed = min_speed;
        }
    }

    /// Release met expliciete fade-duur (ms) — GrandOrgue's toonhoogte-
    /// afhankelijke release-crossfade (get_fader_length): lage pijpen sterven
    /// langzaam uit (~184 ms), hoge snel (~6 ms). Eén vaste 42 ms gaf bassen
    /// een te abrupte, en discanten een te trage overgang naar de release.
    ///
    /// De fade wordt ingekort tot wat er nog aan sampledata ligt: tijdens de
    /// release wordt NIET meer geloopt (read_voice speelt lineair uit), dus een
    /// noot die in de loop werd losgelaten heeft alleen het stuk tussen de
    /// huidige positie en het bufferEINDE nog. Was de fade langer dan dat, dan
    /// kapte `i >= len - 1` de klank hard af op een willekeurige golfwaarde —
    /// een hoorbare TIK bij élk loslaten (gebruikersmelding testorgel; hier
    /// gemeten bij 4 van de 5 loslatingen).
    fn release_ms(&mut self, ms: f32, sample_rate: u32) {
        self.releasing = true;
        self.target_envelope = 0.0;
        // Een expliciete release overschrijft een (uitgestelde of lopende)
        // staart-decay: de boekhouding daarvan mag `upgrade_to_full` straks niet
        // verleiden de zojuist opgelegde fade weer te verlengen.
        self.pending_decay_frames = None;
        self.decay_end_age = None;
        let frames = (ms.max(1.0) * 0.001 * sample_rate as f32).max(8.0);
        // Vanaf het HUIDIGE niveau in `frames` samples naar 0 (envelope kan al
        // lager staan, bv. bij een zachte staccato-noot).
        self.envelope_speed = self.envelope.max(1e-6) / frames;
        self.clamp_release_to_buffer();
    }

    /// Slow release for tremulant crossfade (fade out over ~140ms)
    fn crossfade_release(&mut self) {
        self.releasing = true;
        self.target_envelope = 0.0;
        self.envelope_speed = 0.00015; // Match crossfade speed
        self.clamp_release_to_buffer();
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

    /// Aantal bronframes dat NU echt in het geheugen staat. Verschilt van
    /// `get_sample_len` bij een preload: die meldt de lengte van het volledige
    /// bestand, terwijl read_voice alleen uit `attack_data` leest zolang de
    /// achtergrondlading niet klaar is. De release-fade moet zich naar déze
    /// lengte richten, anders kapt de klank af op de preload-rand.
    fn available_len(&self) -> usize {
        match &self.source {
            VoiceSampleSource::Full(sample) => sample.data.len(),
            VoiceSampleSource::PreloadOnly { preload, .. } => preload.attack_data.len(),
        }
    }

    fn next_sample(&mut self) -> (f32, f32) {
        // Update envelope
        if self.envelope < self.target_envelope {
            self.envelope = (self.envelope + self.envelope_speed).min(self.target_envelope);
        } else if self.envelope > self.target_envelope {
            self.envelope = (self.envelope - self.envelope_speed).max(self.target_envelope);
        }

        // Geschaalde release van een korte noot: de infade is klaar (het
        // snijpunt van op- en neergaande helling is bereikt), dus nu begint de
        // staart-decay over de RESTERENDE tijd — samen precies GrandOrgue's
        // GOSoundFader (Setup + StartDecreasingVolume).
        if let Some(frames) = self.pending_decay_frames {
            if !self.releasing && self.envelope >= self.target_envelope {
                self.pending_decay_frames = None;
                self.decay_end_age = Some(self.age_samples + frames as u64);
                self.releasing = true;
                self.target_envelope = 0.0;
                self.envelope_speed = self.envelope.max(1e-6) / (frames as f32).max(8.0);
                self.clamp_release_to_buffer();
            }
        }

        let (loop_start, loop_end) = self.get_loop_points();
        let total_len = self.get_sample_len();

        if total_len == 0 {
            return (0.0, 0.0);
        }

        // Release-voice (one-shot) die nog op zijn preload draait en het einde
        // daarvan nadert terwijl de volledige WAV nog niet geladen is (tutti-
        // loslating → laadqueue vol): vloeiend uitfaden over de resterende
        // samples in plaats van hard stilvallen en later terug-poppen zodra de
        // load alsnog landt. Landt de load tijdens de fade, dan is de source
        // Full en vuurt deze tak niet meer — de staart speelt gewoon door.
        let mut preload_edge_fade = 1.0f32;
        if self.one_shot {
            if let VoiceSampleSource::PreloadOnly { preload, .. } = &self.source {
                const EDGE_FADE: f64 = 4096.0; // ~90 ms bij 44,1 kHz
                let rest = preload.attack_data.len() as f64 - self.position;
                if rest <= 1.0 {
                    // Einde bereikt zonder full load: definitief klaar — een
                    // late load mag de staart niet laten herrijzen.
                    // Eenmalig tellen (daarna is envelope 0 én releasing true),
                    // zodat de watchdog de laadlatentie kan melden.
                    if !self.releasing || self.envelope > 0.0 {
                        RELEASE_PRELOAD_EDGE_HITS.fetch_add(1, Ordering::Relaxed);
                    }
                    self.releasing = true;
                    self.envelope = 0.0;
                    self.target_envelope = 0.0;
                    return (0.0, 0.0);
                }
                if rest < EDGE_FADE {
                    preload_edge_fade = (rest / EDGE_FADE) as f32;
                }
            }
        }

        // Read the next sample with a seamless crossfade at the loop seam.
        // Loop region = ODF points when valid, else a fallback over the last 3/4
        // of the buffer. The previous code keyed its crossfade to the absolute
        // sample end, so ODF-looped samples wrapped with a HARD jump → a periodic
        // click ("tikken") on sustained notes. read_voice() blends the loop tail
        // into the pre-loop region so the seam is value-continuous.
        // One-shot (Percussive) leest net als release: lineair uitspelen, geen loop.
        let no_loop = self.releasing || self.one_shot;
        let (vl, vr) = match &self.source {
            VoiceSampleSource::Full(sample) => {
                let len = sample.data.len();
                let (ls, le) = match (loop_start, loop_end) {
                    (Some(a), Some(b)) if b > a && (b as usize) <= len => (a as usize, b as usize),
                    _ => (len / 4, len),
                };
                let xfade = loop_xfade(le.saturating_sub(ls), ls);
                read_voice_lr(&sample.data, sample.right.as_deref(), ls, le, xfade, &mut self.position, no_loop, &mut self.envelope)
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
                read_voice_lr(&preload.attack_data, preload.attack_right.as_deref(), ls, le, xfade, &mut self.position, no_loop, &mut self.envelope)
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
        // Bij stereo de monosom — hetzelfde domein als de align-tabel van het
        // release-segment; voor mono is (v+v)·0,5 exact v.
        self.hist_prev = self.hist_last;
        self.hist_last = (vl + vr) * 0.5;
        self.age_samples += 1;

        (vl * self.envelope * preload_edge_fade, vr * self.envelope * preload_edge_fade)
    }
}

/// Laat een net gestarte release-voice INFADEN naar `level` over `fade_ms` —
/// dezelfde duur waarmee de speelnoot uitfadet, zodat de twee samen één vloeiende
/// crossfade vormen. Zonder dit startte de release instant op vol niveau bovenop
/// de nog klinkende speelnoot: een niveausprong bij élk loslaten (op Friesach
/// gemeten als +4 dB en een viervoudige sample-sprong) — de "tik na loslaten".
#[inline]
fn crossfade_in(voice: &mut PlayingVoice, level: f32, fade_ms: f32, sample_rate: u32) {
    let frames = (fade_ms.max(1.0) * 0.001 * sample_rate as f32).max(8.0);
    voice.envelope = 0.0;
    voice.target_envelope = level;
    voice.envelope_speed = (level / frames).max(1e-7);
}

/// Korte noot met geschaalde release: infade EN staart-decay tegelijk, precies
/// zoals GrandOrgue's `GOSoundFader` (`Setup(gain, vel, crossfade)` gevolgd door
/// `StartDecreasingVolume(gain_decay_length)`). De opgaande helling blijft
/// `level / F` — complementair aan de uitfadende speelnoot — maar het DOEL zakt
/// naar het snijpunt van beide hellingen, T' = level·D/(F+D); vanaf daar loopt
/// de neergaande helling (`level/D`) door naar 0, wat nog D·D/(F+D) frames duurt.
///
/// Het oude gedrag startte de release instant op vol niveau bovenop de nog
/// klinkende speelnoot ("die twee doelen gaan niet samen") — een niveaubult van
/// enkele dB bij élke korte noot, en bij een kort aangeslagen slotakkoord op
/// álle pijpen tegelijk.
#[inline]
fn crossfade_in_with_decay(voice: &mut PlayingVoice, level: f32, fade_ms: f32, decay_ms: f32, sample_rate: u32) {
    let f = (fade_ms.max(1.0) * 0.001 * sample_rate as f32).max(8.0);
    let d = (decay_ms.max(1.0) * 0.001 * sample_rate as f32).max(8.0);
    let ratio = d / (f + d);
    // Zet eerst de volle infade (helling level/F), verlaag daarna het doel naar
    // het snijpunt: de helling blijft zo exact complementair aan de speelnoot.
    crossfade_in(voice, level, fade_ms, sample_rate);
    voice.target_envelope = level * ratio;
    voice.pending_decay_frames = Some((d * ratio).max(8.0) as u32);
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

/// Eén stuk opruimwerk voor de janitor-thread (het `gc_tx`-kanaal in `start`).
/// `Boxed` is de algemene route voor grote structuren (oude sample-maps, een
/// hele stemmenlijst bij een orgelwissel). `Sample` en `Preload` dragen de Arc
/// van één losse sample rechtstreeks — zonder Box, dus zonder allocatie in de
/// audio-callback — voor het pad dat per eindigende stem kan lopen.
/// De velden worden nergens gelezen: ze bestaan om op de janitor-thread
/// gedropt te worden (vandaar de dead_code-uitzondering).
#[allow(dead_code)]
enum GcItem {
    Boxed(Box<dyn std::any::Any + Send>),
    Sample(SampleRef),
    Preload(Arc<PreloadBuffer>),
}

/// Afvoerluik van de audio-callback naar de janitor-thread: de zender plus de
/// wachtrij voor wat de janitor even niet aankon (kanaal vol). Dat wordt
/// vastgehouden en de volgende callback opnieuw aangeboden — NOOIT inline
/// gedropt, want dat is precies de drop-storm die de janitor voorkomt.
///
/// Waarom óók per stem (0.7.40): boven de RAM-kap (`SAMPLES_BYTES_CAP`) haalt
/// de eviction de Arc netjes uit de samples-map en stuurt die naar de janitor,
/// maar een KLINKENDE pijp houdt intussen zijn eigen `SampleRef` vast en de
/// decode_cache bewaart alleen een Weak. Eindigt zo'n stem, dan was het
/// verwijderen uit de stemmenlijst de plek waar de LAATSTE Arc viel: een
/// `Vec<f32>` van 1-3 MB gaat dan via HeapFree/NtFreeVirtualMemory terug naar
/// het systeem (honderden pagina's plus TLB-shootdowns, ~50-150 µs per stuk).
/// Bij een akkoordloslating waarbij 5-10 stemmen in dezelfde callback eindigen
/// is dat 0,5-3 ms — ruim over een blok van 32 frames. Daarom gaat de bron van
/// elke verdwijnende stem eerst langs [`GcSink::dispose_voice`].
struct GcSink {
    tx: Sender<GcItem>,
    backlog: Vec<GcItem>,
}

impl GcSink {
    fn new(tx: Sender<GcItem>) -> Self {
        // Vooraf gereserveerd: een push in de wachtrij mag in de callback
        // normaal gesproken geen allocatie kosten.
        Self { tx, backlog: Vec::with_capacity(64) }
    }

    /// Eén item afvoeren; bij een vol kanaal in de wachtrij (nooit droppen).
    #[inline]
    fn send(&mut self, item: GcItem) {
        if let Err(e) = self.tx.try_send(item) {
            self.backlog.push(e.into_inner());
        }
    }

    /// Grote structuur (oude map, hele stemmenlijst) afvoeren. De Box is hier
    /// één kleine allocatie op het zeldzame orgelwissel-pad, niet per stem.
    fn send_boxed<T: std::any::Any + Send>(&mut self, garbage: T) {
        self.send(GcItem::Boxed(Box::new(garbage)));
    }

    /// Achtergestelde items alsnog aanbieden (één keer per callback).
    fn flush_backlog(&mut self) {
        while let Some(item) = self.backlog.pop() {
            if let Err(e) = self.tx.try_send(item) {
                self.backlog.push(e.into_inner());
                break;
            }
        }
    }

    /// Laat een `SampleRef` los die de callback niet meer nodig heeft. Houdt
    /// de samples-map of een klinkende stem de Arc nog vast, dan is loslaten
    /// alleen een tellerdecrement en gebeurt dat gewoon hier. Zijn wij de
    /// LAATSTE houder, dan gaat de Arc naar de janitor — de eigenlijke
    /// deallocatie hoort niet in de callback.
    #[inline]
    fn dispose_sample(&mut self, sample: SampleRef) {
        if Arc::strong_count(&sample) > 1 {
            return; // alleen een teller: de data blijft leven
        }
        self.send(GcItem::Sample(sample));
    }

    /// Idem voor een preload-buffer (attack-fragment, honderden kB).
    #[inline]
    fn dispose_preload(&mut self, preload: Arc<PreloadBuffer>) {
        if Arc::strong_count(&preload) > 1 {
            return;
        }
        self.send(GcItem::Preload(preload));
    }

    /// Voert een verdwenen stem af: de bron gaat langs de laatste-houder-check,
    /// de rest van de stem is plat en kost niets.
    #[inline]
    fn dispose_voice(&mut self, voice: PlayingVoice) {
        match voice.source {
            VoiceSampleSource::Full(sample) => self.dispose_sample(sample),
            VoiceSampleSource::PreloadOnly { preload, .. } => self.dispose_preload(preload),
        }
    }
}

/// Verwijdert uitgeklonken stemmen uit de lijst — stabiele volgorde, geen
/// allocatie — en voert hun bron af via `gc` (zie [`GcSink::dispose_voice`]).
/// Vervangt de kale `retain`, die de laatste Arc van een geëvicte sample ín de
/// callback liet vallen.
#[inline]
fn remove_finished_voices(voices: &mut Vec<PlayingVoice>, gc: &mut GcSink) {
    for v in voices.extract_if(.., |v| v.is_finished()) {
        gc.dispose_voice(v);
    }
}

/// Voeg een voice toe met respect voor [`MAX_LIVE_VOICES`]. Zit de Vec vol, dan
/// wordt eerst de "goedkoopste" voice gestolen: bij voorkeur een al uitklinkende
/// (releasing) voice, en daarbinnen die met het laagste envelope-niveau — dus de
/// minst hoorbare. `swap_remove` is O(1); de mix-lus en `retain` zijn ongevoelig
/// voor de volgorde. Alleen wanneer de grens bereikt is doen we de O(n)-scan.
/// Een direct verwijderd slachtoffer gaat via `gc` (zie [`GcSink`]): is die
/// stem de laatste houder van zijn sample, dan valt de Arc niet hier maar op
/// de janitor-thread.
#[inline]
fn push_voice_capped(voices: &mut Vec<PlayingVoice>, v: PlayingVoice, sample_rate: u32, cap: usize, gc: &mut GcSink) {
    let cap = cap.clamp(MIN_LIVE_VOICES, MAX_LIVE_VOICES);
    if voices.len() >= cap {
        // Een net gestarte stem (< 20 ms) is nooit slachtoffer — anders stal
        // een tutti-akkoord zijn eigen zojuist aangeslagen pijpen (hoorbaar
        // "wegvallen" van noten, feedback sampleset-maker). Zijn álle stemmen
        // zo jong, dan toch de minst hoorbare kiezen: de kap blijft hard.
        let min_age = (sample_rate as u64 / 50).max(1);
        let victim = pick_victim(voices, min_age).or_else(|| pick_victim(voices, 0));
        if let Some(i) = victim {
            if voices.len() >= cap + 64 || (voices[i].releasing && voices[i].envelope < 0.01) {
                // Harde noodgrens of al (bijna) stil: direct weg.
                let stolen = voices.swap_remove(i);
                gc.dispose_voice(stolen);
            } else {
                // Hoorbaar klinkend: korte fade i.p.v. harde knip (klik,
                // auditbevinding 54); telt heel even boven de kap mee.
                let ms = if voices[i].one_shot { 30.0 } else { 20.0 };
                voices[i].release_ms(ms, sample_rate);
            }
        }
    }
    voices.push(v);
}

/// Slachtofferkeuze bij een volle kap, in oplopende hoorbaarheid (GrandOrgue-
/// stijl "oldest first" voor klinkende pijpen):
///  (a) al uitklinkende release-staarten, stilste eerst;
///  (b) overige release-staarten, oudste eerst;
///  (c) uitklinkende (losgelaten) pijpen, stilste eerst;
///  (d) klinkende pijpen, oudste eerst.
/// Stemmen jonger dan `min_age` output-samples doen niet mee.
fn pick_victim(voices: &[PlayingVoice], min_age: u64) -> Option<usize> {
    let mut best: Option<(u8, f32, usize)> = None;
    for (i, ex) in voices.iter().enumerate() {
        if ex.age_samples < min_age {
            continue;
        }
        let (class, score) = match (ex.one_shot, ex.releasing) {
            (true, true) => (0u8, ex.envelope),
            (true, false) => (1u8, -(ex.age_samples as f32)),
            (false, true) => (2u8, ex.envelope),
            (false, false) => (3u8, -(ex.age_samples as f32)),
        };
        let better = match best {
            None => true,
            Some((bc, bs, _)) => class < bc || (class == bc && score < bs),
        };
        if better {
            best = Some((class, score, i));
        }
    }
    best.map(|b| b.2)
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
/// OUDSTE bestaande staart versneld plaats (250 ms fade) voor de nieuwe.
///
/// Slachtofferkeuze (GrandOrgue-stijl, `GOSoundSamplerPlayer::ProcessSampler`:
/// boven de zachte polyfonie-grens faden alleen staarten die al even lopen):
///  - alleen one-shot staarten die NIET al aan het uitfaden zijn;
///  - minstens 20 ms oud (zelfde jongeren-bescherming als `push_voice_capped`);
///  - infade helemaal klaar (`envelope >= target_envelope`) en geen uitgestelde
///    staart-decay meer open;
///  - daarbinnen de OUDSTE.
///
/// De oude keuze ("laagste envelope") koos gegarandeerd de zojuist gespawnde
/// release: die staat door `crossfade_in` op envelope 0,0, terwijl natuurlijke
/// staarten op hun startniveau blijven (de afname zit in de sampledata). Bij een
/// akkoord-loslating onder budgetdruk overleefde daardoor alleen de láátst
/// gespawnde staart — spawn 2 doodde spawn 1, spawn 3 doodde spawn 2 — en klonk
/// een slotakkoord na een staccato-passage vrijwel zonder nagalm.
const MAX_RELEASE_VOICES: usize = 160;
fn budget_release_voices(voices: &mut Vec<PlayingVoice>, sample_rate: u32, cap: usize) {
    // Staartbudget groeit mee met de kap (minimaal het oude 160); daarnaast
    // een zachte limiet à la GrandOrgue: boven 80% van de kap maakt bij elke
    // nieuwe stem eerst onhoorbaar een staart plaats, zodat klinkende pijpen
    // pas als allerlaatste gestolen worden.
    let budget = (cap / 3).max(MAX_RELEASE_VOICES);
    let soft_limit = cap * 4 / 5;
    let count = voices.iter().filter(|v| v.one_shot).count();
    if count < budget && voices.len() < soft_limit {
        return;
    }
    let min_age = (sample_rate as u64 / 50).max(1);
    let mut oldest: Option<usize> = None;
    let mut oldest_age = 0u64;
    for (i, v) in voices.iter().enumerate() {
        if !v.one_shot || v.releasing || v.pending_decay_frames.is_some() {
            continue;
        }
        if v.age_samples < min_age || v.envelope < v.target_envelope {
            continue;
        }
        if oldest.is_none() || v.age_samples > oldest_age {
            oldest_age = v.age_samples;
            oldest = Some(i);
        }
    }
    if let Some(i) = oldest {
        // 250 ms, zoals GrandOrgue's polyfonie-limiter (die gebruikt 370 ms):
        // kort genoeg om CPU te winnen, lang genoeg om onhoorbaar te blijven.
        voices[i].release_ms(250.0, sample_rate);
    }
}

/// Voeg een reeks nieuwe stemmen toe zoals de commandolus dat overal doet: vóór
/// ELKE push eerst het staartbudget bijstellen ([`budget_release_voices`]) en
/// daarna de polyfonie-kap toepassen ([`push_voice_capped`]). De volgorde is
/// wezenlijk: het staartbudget kiest een onhoorbaar slachtoffer, de kap een
/// hoorbaar — doe je het budget maar één keer vooraf, dan staat de kap er bij
/// een burst alsnog alleen voor en worden klinkende pijpen gestolen.
///
/// Geeft het aantal toegevoegde stemmen terug, zodat de aanroeper dat als
/// werkeenheden op het spawn-budget van de callback kan boeken.
#[inline]
fn push_voices_budgeted(
    voices: &mut Vec<PlayingVoice>,
    new_voices: Vec<PlayingVoice>,
    sample_rate: u32,
    cap: usize,
    gc: &mut GcSink,
) -> usize {
    let n = new_voices.len();
    for v in new_voices {
        budget_release_voices(voices, sample_rate, cap);
        push_voice_capped(voices, v, sample_rate, cap, gc);
    }
    n
}

/// Crossfade length for a loop of `loop_len` samples whose start is at index `ls`.
/// Long crossfades (Hauptwerk-style, up to ~500 ms) mask imperfect loop points;
/// bounded by half the loop and by the available pre-loop data (`ls`).
/// GEEN ondergrens meer: de oude .clamp(256, …) kon bij weinig pre-loop-data
/// (ls < 256) een xfade GROTER dan ls opleveren, waarna read_voice de blend
/// helemaal oversloeg (guard `ls >= xfade`) — een harde naad die elke
/// loop-omloop tikte. Een korte blend is altijd beter dan géén.
#[inline]
fn loop_xfade(loop_len: usize, ls: usize) -> usize {
    (loop_len / 2).min(ls).min(24_000)
}

/// Maximaal aantal fysieke uitgangskanalen waarvoor galm-gewichten worden
/// bijgehouden (ruim boven elke praktijkkaart; 8 bij een GIGAPORT eX).
/// Plafond voor het aantal uitgangskanalen van de galm-weging. Verhoogd van
/// 64 naar 1024 (0.7.48): de weegtabellen zijn vaste arrays die één keer per
/// callback berekend worden, dus 2 × 1024 floats (8 kB) kost niets, en
/// meerkanaals-interfaces boven de 64 uitgangen bestaan.
const MAX_OUT_CH: usize = 1024;

/// Galm-gewicht per fysiek uitgangskanaal, afgeleid uit de effectieve
/// divisie-routing: de galm hoort te klinken uit dezélfde luidsprekers als het
/// droge signaal, in dezélfde verhouding.
///
/// Per échte divisie (0..n_divs) worden de (L,R)-paren geparseerd zoals de
/// droge routering dat doet (even index = L, oneven = R, los laatste kanaal =
/// mono); een lege of volledig buiten het apparaat vallende lijst valt — net
/// als droog — terug op het voorste paar. Elke divisie draagt `1/n_divs` bij
/// aan de kanalen die zíj gebruikt. Daardoor geldt:
///   * alle divisies op één paar (hoofdtelefoonprofiel) → dat paar krijgt de
///     volledige galm, de overige kanalen niets;
///   * elke divisie op een eigen paar → elk paar 1/n, samen precies één keer
///     de galm (geen N-voudige optelling die alles te nat maakt);
///   * overlappende routings → een gedeeld kanaal telt per divisie één keer,
///     dus geen dubbele galm op dat kanaal.
/// Vaste arrays: geen heap-allocatie op de audio-thread.
/// Retourneert (gewicht_links, gewicht_rechts, aantal_actieve_kanalen).
fn wet_channel_weights(
    out_chans: &[Vec<u16>],
    n_divs: usize,
    channels: usize,
) -> ([f32; MAX_OUT_CH], [f32; MAX_OUT_CH], usize) {
    let mut wl = [0.0f32; MAX_OUT_CH];
    let mut wr = [0.0f32; MAX_OUT_CH];
    let ch = channels.min(MAX_OUT_CH);
    if ch == 0 { return (wl, wr, 0); }
    let divs = n_divs.max(1);
    let share = 1.0 / divs as f32;
    let mut hoogste = 0usize;
    for idx in 0..divs {
        let mut routed = false;
        if let Some(list) = out_chans.get(idx) {
            let mut i = 0;
            while i < list.len() {
                let lc = list[i] as usize;
                let rc = if i + 1 < list.len() { Some(list[i + 1] as usize) } else { None };
                match rc {
                    // Volwaardig (L,R)-paar binnen het apparaat.
                    Some(rc) if lc < ch && rc < ch => {
                        wl[lc] += share; wr[rc] += share;
                        hoogste = hoogste.max(lc + 1).max(rc + 1);
                        routed = true;
                    }
                    // Eén helft valt buiten het apparaat, of een los laatste
                    // kanaal: de overgebleven kant krijgt de gesomde (mono)
                    // galm — net zoals droog daar mono terechtkomt.
                    Some(rc) => {
                        let keep = if lc < ch { Some(lc) } else if rc < ch { Some(rc) } else { None };
                        if let Some(c) = keep {
                            wl[c] += share * 0.5; wr[c] += share * 0.5;
                            hoogste = hoogste.max(c + 1);
                            routed = true;
                        }
                    }
                    None => {
                        if lc < ch {
                            wl[lc] += share * 0.5; wr[lc] += share * 0.5;
                            hoogste = hoogste.max(lc + 1);
                            routed = true;
                        }
                    }
                }
                i += if rc.is_some() { 2 } else { 1 };
            }
        }
        if !routed {
            // Geen (bruikbare) routing: voorste paar, gelijk aan het droge pad.
            if ch >= 2 {
                wl[0] += share; wr[1] += share;
                hoogste = hoogste.max(2);
            } else {
                wl[0] += share * 0.5; wr[0] += share * 0.5;
                hoogste = hoogste.max(1);
            }
        }
    }
    (wl, wr, hoogste.min(ch))
}

/// Zwelkast-DSP (pure functies, unit-getest): smoothing van de pedaalstand,
/// volume en logaritmische filtermapping. Gebruikt door de render-lus per
/// MIX_BLOCK; niets hiervan hoort in de per-frame-lus.
pub(crate) mod swell_dsp {
    /// Tijdconstante van de zwel-smoothing (s): ~20 ms voor de volle weg,
    /// snel genoeg voor een ruk aan de trede, traag genoeg tegen zipper/tikken.
    pub const TAU_S: f32 = 0.020;
    /// Vanaf deze (gesmoothede) stand wordt het low-pass-filter overgeslagen
    /// (kast open, of divisie zonder zwelkast die altijd op 1.0 staat).
    pub const BYPASS_ABOVE: f32 = 0.999;

    /// One-pole-coëfficiënt voor een blok van `frames` frames.
    pub fn block_alpha(frames: usize, sample_rate: u32, tau_s: f32) -> f32 {
        let sr = sample_rate.max(1) as f32;
        let tau = tau_s.max(1e-4);
        (1.0 - (-(frames.max(1) as f32) / (tau * sr)).exp()).clamp(0.0, 1.0)
    }

    /// Eén smoothing-stap richting `target`; snapt op het doel zodra het
    /// verschil onhoorbaar klein is (voorkomt eeuwig 'bijna'-rekenen).
    pub fn smooth_step(g: f32, target: f32, alpha: f32) -> f32 {
        let next = g + (target - g) * alpha;
        if (next - target).abs() < 1e-4 { target } else { next }
    }

    /// Lineair volume bij pedaalstand `g` (0 = dicht → min_db, 1 = open → 0 dB).
    pub fn volume(min_db: f32, g: f32) -> f32 {
        10.0_f32.powf(min_db * (1.0 - g.clamp(0.0, 1.0)) / 20.0)
    }

    /// Logaritmische cutoff-mapping: `cutoff_closed` bij dicht, 20 kHz bij open;
    /// halverwege het meetkundig gemiddelde (lineair lag de cutoff bij 0,5 al
    /// op ~10 kHz, dus het timbre-effect zat vrijwel geheel onderin de weg).
    pub fn cutoff_hz(cutoff_closed: f32, g: f32) -> f32 {
        let c = cutoff_closed.clamp(20.0, 20000.0);
        c * (20000.0 / c).powf(g.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod swell_dsp_tests {
    use super::swell_dsp::*;

    #[test]
    fn alpha_in_bereik_en_groter_bij_groter_blok() {
        let a256 = block_alpha(256, 48000, TAU_S);
        let a64 = block_alpha(64, 48000, TAU_S);
        assert!(a256 > 0.0 && a256 < 1.0);
        assert!(a64 > 0.0 && a64 < a256);
        // Kapotte invoer valt veilig terug.
        assert!(block_alpha(0, 0, 0.0) <= 1.0);
    }

    #[test]
    fn smoothing_loopt_monotoon_en_convergeert_binnen_5_tau() {
        let sr = 48000u32;
        let alpha = block_alpha(256, sr, TAU_S);
        let mut g = 1.0f32;
        let mut prev = g;
        let mut blocks = 0;
        while g != 0.0 && blocks < 10_000 {
            g = smooth_step(g, 0.0, alpha);
            assert!(g <= prev, "niet monotoon dalend");
            prev = g;
            blocks += 1;
        }
        // Binnen ~5τ (100 ms = 4800 frames ≈ 19 blokken van 256) op het doel.
        assert!(blocks <= 40, "convergeert te traag: {} blokken", blocks);
        assert!(blocks >= 2, "te snel (geen smoothing): {} blokken", blocks);
        // Na 1τ ≈ 63 % van de weg afgelegd.
        let mut g2 = 1.0f32;
        let per_tau = (TAU_S * sr as f32 / 256.0).round() as usize;
        for _ in 0..per_tau { g2 = smooth_step(g2, 0.0, alpha); }
        assert!((g2 - (-1.0f32).exp()).abs() < 0.08, "na 1 tau: {}", g2);
    }

    #[test]
    fn volume_en_cutoff_mapping() {
        assert!((volume(-20.0, 0.0) - 0.1).abs() < 1e-5);
        assert!((volume(-20.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((volume(-30.0, 0.5) - 10f32.powf(-15.0 / 20.0)).abs() < 1e-5);
        assert!((cutoff_hz(800.0, 0.0) - 800.0).abs() < 0.01);
        assert!((cutoff_hz(800.0, 1.0) - 20000.0).abs() < 0.5);
        assert!((cutoff_hz(800.0, 0.5) - 4000.0).abs() < 1.0); // meetkundig gemiddelde
        assert!(cutoff_hz(800.0, 0.25) < cutoff_hz(800.0, 0.75));
    }
}

#[cfg(test)]
mod wet_weights_tests {
    use super::wet_channel_weights;

    /// Compacte weergave: alleen kanalen met gewicht, als (kanaal, wl, wr).
    fn actief(wl: &[f32], wr: &[f32], n: usize) -> Vec<(usize, f32, f32)> {
        (0..n).filter(|&c| wl[c] > 1e-6 || wr[c] > 1e-6)
            .map(|c| (c, (wl[c] * 1000.0).round() / 1000.0, (wr[c] * 1000.0).round() / 1000.0))
            .collect()
    }

    #[test]
    fn standaard_stereo_naar_voorste_paar() {
        // Geen routing geconfigureerd: volle galm op (0,1), zoals vóór 0.7.33.
        let chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
        let (wl, wr, n) = wet_channel_weights(&chans, 3, 2);
        assert_eq!(actief(&wl, &wr, n), vec![(0, 1.0, 0.0), (1, 0.0, 1.0)]);
    }

    #[test]
    fn hoofdtelefoonprofiel_volgt_override() {
        // Testorgel-klacht: alle divisies via de override op [0,1] van een
        // 8-kanaals apparaat → volle galm op (0,1), niets op 2..7.
        let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
        for i in 0..3 { chans[i] = vec![0, 1]; }
        let (wl, wr, n) = wet_channel_weights(&chans, 3, 8);
        assert_eq!(actief(&wl, &wr, n), vec![(0, 1.0, 0.0), (1, 0.0, 1.0)]);
    }

    #[test]
    fn speakerprofiel_alle_divisies_op_alle_paren() {
        // Elke divisie speelt droog op alle drie de paren → elk paar krijgt ook
        // de volle galm; de droog/galm-balans per luidspreker blijft gelijk.
        let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
        for i in 0..3 { chans[i] = vec![2, 3, 4, 5, 6, 7]; }
        let (wl, wr, n) = wet_channel_weights(&chans, 3, 8);
        assert_eq!(actief(&wl, &wr, n), vec![
            (2, 1.0, 0.0), (3, 0.0, 1.0), (4, 1.0, 0.0), (5, 0.0, 1.0), (6, 1.0, 0.0), (7, 0.0, 1.0)]);
    }

    #[test]
    fn divisie_per_paar_telt_op_tot_een_keer_galm() {
        // Klassieke multikanaals-opstelling: elk werk zijn eigen paar. Elk paar
        // krijgt 1/3 galm — samen precies één keer, niet drie keer (anders
        // wordt alles fors natter zodra je de werken uit elkaar trekt).
        let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
        chans[0] = vec![0, 1]; chans[1] = vec![2, 3]; chans[2] = vec![4, 5];
        let (wl, wr, n) = wet_channel_weights(&chans, 3, 8);
        let a = actief(&wl, &wr, n);
        assert_eq!(a, vec![(0, 0.333, 0.0), (1, 0.0, 0.333), (2, 0.333, 0.0),
                           (3, 0.0, 0.333), (4, 0.333, 0.0), (5, 0.0, 0.333)]);
        let totaal_l: f32 = (0..n).map(|c| wl[c]).sum();
        assert!((totaal_l - 1.0).abs() < 0.01, "totale galm moet 1x zijn, is {}", totaal_l);
    }

    #[test]
    fn gedeeld_kanaal_krijgt_galm_niet_dubbel() {
        // Overlappende routings: kanaal 0 wordt door beide divisies gebruikt en
        // krijgt daarom precies hun sommatie (1.0), niet twee volle kopieën.
        let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
        chans[0] = vec![0, 1]; chans[1] = vec![0, 2];
        let (wl, wr, n) = wet_channel_weights(&chans, 2, 8);
        assert_eq!(actief(&wl, &wr, n), vec![(0, 1.0, 0.0), (1, 0.0, 0.5), (2, 0.0, 0.5)]);
    }

    #[test]
    fn los_laatste_kanaal_wordt_mono() {
        let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
        chans[0] = vec![4, 5, 6];
        let (wl, wr, n) = wet_channel_weights(&chans, 1, 8);
        assert_eq!(actief(&wl, &wr, n), vec![(4, 1.0, 0.0), (5, 0.0, 1.0), (6, 0.5, 0.5)]);
    }

    #[test]
    fn routing_buiten_apparaat_valt_terug() {
        // 8-kanaals routing op een stereo-apparaat (na profielwissel): terugval
        // op het voorste paar, net als het droge signaal (auditbevinding 37).
        let mut chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
        chans[0] = vec![2, 3, 4, 5];
        let (wl, wr, n) = wet_channel_weights(&chans, 1, 2);
        assert_eq!(actief(&wl, &wr, n), vec![(0, 1.0, 0.0), (1, 0.0, 1.0)]);
    }

    #[test]
    fn mono_apparaat_somt_naar_kanaal_nul() {
        let chans: Vec<Vec<u16>> = vec![Vec::new(); 32];
        let (wl, wr, n) = wet_channel_weights(&chans, 2, 1);
        assert_eq!(actief(&wl, &wr, n), vec![(0, 0.5, 0.5)]);
    }
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
/// Equal-power crossfade-gewichten uit een sinustabel (0..π/2, 1024 stappen,
/// lineair geïnterpoleerd) i.p.v. sin()+cos() per sample in de loop-naad. Bij
/// lange Hauptwerk-achtige crossfades (tot 0,5 s per loop-omloop) zat een
/// flink deel van alle voice-samples in de naadzone — twee transcendenten per
/// sample per stem waren daar de grootste kostenpost. Fout < 1e-6.
static XFADE_LUT: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
const XFADE_LUT_N: usize = 1024;
/// Tabel opbouwen (één keer; wordt bij AudioPlayer::new aangeroepen zodat de
/// audio-thread nooit hoeft te alloceren).
pub fn xfade_lut() -> &'static [f32] {
    XFADE_LUT.get_or_init(|| {
        (0..=XFADE_LUT_N)
            .map(|i| (i as f32 / XFADE_LUT_N as f32 * std::f32::consts::FRAC_PI_2).sin())
            .collect()
    })
}
#[inline]
fn lut_sin_quarter(lut: &[f32], x: f32) -> f32 {
    // x in 0..=1 ↔ θ in 0..=π/2
    let xs = x.clamp(0.0, 1.0) * XFADE_LUT_N as f32;
    let i = xs as usize;
    if i >= XFADE_LUT_N {
        return lut[XFADE_LUT_N];
    }
    let t = xs - i as f32;
    lut[i] + (lut[i + 1] - lut[i]) * t
}
/// (w_primary, w_blended) = (sin θ, cos θ) met θ = fade·π/2; primary²+blended² ≈ 1.
#[inline]
fn xfade_weights(fade: f32) -> (f32, f32) {
    let lut = xfade_lut();
    (lut_sin_quarter(lut, fade), lut_sin_quarter(lut, 1.0 - fade))
}

/// Mono-lezer: dunne wrapper om [`read_voice_lr`] (zelfde code, geen rechtervlak).
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
    read_voice_lr(data, None, ls, le, xfade, position, releasing, envelope).0
}

/// Read one interpolated sample from `data`, advancing/wrapping `position` within
/// the loop [ls, le) and blending the loop tail into the pre-loop region to make
/// it value-continuous regardless of whether the sample's loop points match phase.
/// Gefuseerd voor mono én stereo: indexrekenwerk, loop-wrap en crossfade-
/// gewichten één keer; met een rechtervlak wordt alleen een tweede hermite4 op
/// dezelfde index gedaan. Mono geeft (v, v).
#[inline]
fn read_voice_lr(
    data: &[f32],
    right: Option<&[f32]>,
    ls: usize,
    le: usize,
    xfade: usize,
    position: &mut f64,
    releasing: bool,
    envelope: &mut f32,
) -> (f32, f32) {
    let len = data.len();
    if len < 2 {
        *envelope = 0.0;
        return (0.0, 0.0);
    }

    // During release: no looping — play out to the end, then finish.
    if releasing {
        let i = *position as usize;
        if i >= len - 1 {
            *envelope = 0.0;
            return (0.0, 0.0);
        }
        let frac = (*position - i as f64) as f32;
        let l = hermite4(data, i, frac);
        let r = match right { Some(rd) => hermite4(rd, i, frac), None => l };
        return (l, r);
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
        return (0.0, 0.0);
    }
    let frac = (p - i as f64) as f32;
    let pl = hermite4(data, i, frac);
    let pr = match right { Some(rd) => hermite4(rd, i, frac), None => pl };

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
            let (w_primary, w_blended) = xfade_weights(fade); // 1.0 at xfade start / 1.0 at the seam
            let pre = ls as f64 - dist; // walks [ls - xfade, ls)
            let j = pre as usize;
            if j + 1 < len {
                let pfrac = (pre - j as f64) as f32;
                let bl = hermite4(data, j, pfrac);
                let br = match right { Some(rd) => hermite4(rd, j, pfrac), None => bl };
                return (pl * w_primary + bl * w_blended, pr * w_primary + br * w_blended);
            }
        }
    }

    (pl, pr)
}

#[cfg(test)]
mod loop_seam_tests {
    use super::{read_voice, read_voice_lr, xfade_weights};

    /// Sinustabel ≈ sin/cos en blijft equal-power over de hele fade.
    #[test]
    fn xfade_lut_matches_sin_cos() {
        for k in 0..=2000 {
            let fade = k as f32 / 2000.0;
            let (wp, wb) = xfade_weights(fade);
            let theta = fade * std::f32::consts::FRAC_PI_2;
            assert!((wp - theta.sin()).abs() < 2e-6, "sin bij {}", fade);
            assert!((wb - theta.cos()).abs() < 2e-6, "cos bij {}", fade);
            assert!((wp * wp + wb * wb - 1.0).abs() < 5e-6, "power bij {}", fade);
        }
    }

    /// Stereo-lezer: rechtervlak volgt exact dezelfde positie/loop/crossfade als links.
    #[test]
    fn read_voice_lr_reads_both_planes_at_same_index() {
        let n = 4000usize;
        let l: Vec<f32> = (0..n).map(|i| (i as f32 * 0.05).sin()).collect();
        let r: Vec<f32> = l.iter().map(|v| -v * 0.5).collect();
        let (ls, le, xfade) = (1000usize, 3000usize, 500usize);
        let mut pos = 0.0f64;
        let mut env = 1.0f32;
        let mut pos_m = 0.0f64;
        let mut env_m = 1.0f32;
        for _ in 0..6000 {
            let (a, b) = read_voice_lr(&l, Some(&r), ls, le, xfade, &mut pos, false, &mut env);
            let m = read_voice(&l, ls, le, xfade, &mut pos_m, false, &mut env_m);
            assert_eq!(a, m);
            assert!((b + 0.5 * a).abs() < 1e-6);
            pos += 1.0;
            pos_m += 1.0;
        }
    }

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

/// Release-gedrag bij korte noten en onder staartbudget-druk (onderwerp 7:
/// "galm gaat niet goed bij GrandOrgue-sets; kort aanslaan klinkt raar bij het
/// slotakkoord").
#[cfg(test)]
mod release_tests {
    use super::*;

    /// Constante sample: `read_voice` levert dan exact de envelope terug, zodat
    /// een test het NIVEAUVERLOOP meet en niet de golfvorm.
    fn vlakke_sample(frames: usize) -> SampleRef {
        std::sync::Arc::new(SampleData {
            data: vec![1.0; frames],
            right: None,
            sample_rate: 48_000,
            channels: 1,
            loop_start: None,
            loop_end: None,
        })
    }

    /// Natuurlijke release-staart: speelt op zijn startniveau door (de afname
    /// zit in de sampledata), dus envelope == target_envelope.
    fn staart(age_samples: u64) -> PlayingVoice {
        let mut v = PlayingVoice::new_from_sample(
            vlakke_sample(48_000), 1, 100 | RELEASE_PIPE_FLAG, 60, 1.0,
        );
        v.one_shot = true;
        v.envelope = 1.0;
        v.target_envelope = 1.0;
        v.age_samples = age_samples;
        v
    }

    /// Het staartbudget mag NOOIT de zojuist gespawnde release stelen (die
    /// staat nog op envelope 0 door de crossfade-infade); het slachtoffer is de
    /// oudste natuurlijke staart.
    #[test]
    fn staartbudget_spaart_verse_crossfade_en_kiest_de_oudste() {
        let cap = 1024usize;
        let budget = (cap / 3).max(MAX_RELEASE_VOICES);
        let mut voices: Vec<PlayingVoice> =
            (0..budget).map(|i| staart(48_000 + i as u64)).collect();
        let mut vers = staart(0);
        crossfade_in(&mut vers, 1.0, release_fade_ms(36), 48_000);
        voices.push(vers);

        budget_release_voices(&mut voices, 48_000, cap);

        assert!(!voices[budget].releasing, "de verse crossfade-release werd gestolen");
        assert!(voices[budget].target_envelope > 0.0, "verse release kreeg doel 0");
        let slachtoffers: Vec<usize> = voices.iter().enumerate()
            .filter(|(_, v)| v.releasing).map(|(i, _)| i).collect();
        assert_eq!(slachtoffers.len(), 1, "verwacht precies één slachtoffer, kreeg {:?}", slachtoffers);
        assert_eq!(slachtoffers[0], budget - 1, "niet de OUDSTE staart gekozen");
    }

    /// Ook een release die nog aan zijn uitgestelde staart-decay moet beginnen
    /// is beschermd — anders komt de "kettingmoord" via de achterdeur terug.
    #[test]
    fn staartbudget_spaart_ook_een_release_met_uitgestelde_decay() {
        let cap = 1024usize;
        let budget = (cap / 3).max(MAX_RELEASE_VOICES);
        let mut voices: Vec<PlayingVoice> =
            (0..budget).map(|i| staart(48_000 + i as u64)).collect();
        let mut vers = staart(0);
        crossfade_in_with_decay(&mut vers, 1.0, release_fade_ms(36), 1200.0, 48_000);
        // Doe alsof de infade al klaar is: zonder de pending-decay-guard zou
        // deze verse staart nu kandidaat zijn (age 0 beschermt hem niet meer).
        vers.envelope = vers.target_envelope;
        vers.age_samples = 48_000 * 2;
        voices.push(vers);

        budget_release_voices(&mut voices, 48_000, cap);

        assert!(!voices[budget].releasing, "release met uitgestelde decay werd gestolen");
    }

    /// Een tremulantwissel op vol werk: elke klinkende pijp krijgt er een stem
    /// bij terwijl de oude uitfadet. Die burst moet langs het STAARTBUDGET —
    /// vóór elke push één keer — anders groeit het stemmenaantal ongeremd
    /// terwijl de staarten al op hun grens staan. De teruggegeven telling is
    /// tegelijk het werk dat de callback op zijn spawn-budget boekt.
    #[test]
    fn tremulantburst_gaat_langs_het_staartbudget_en_telt_het_werk() {
        let cap = 1024usize;
        let budget = (cap / 3).max(MAX_RELEASE_VOICES);
        // Staarten precies op het budget: elke extra stem moet er nu één
        // onhoorbaar laten plaatsmaken.
        let mut voices: Vec<PlayingVoice> =
            (0..budget).map(|i| staart(48_000 + i as u64)).collect();

        // Vier nieuwe crossfade-stemmen zoals SetTremulant ze maakt (klinkende
        // pijpen, dus géén one-shot staarten).
        let nieuw: Vec<PlayingVoice> = (0..4)
            .map(|_| {
                let mut v = PlayingVoice::new_from_sample(vlakke_sample(48_000), 1, 100, 60, 1.0);
                v.envelope = 1.0;
                v.target_envelope = 1.0;
                v
            })
            .collect();

        let (gc_tx, _gc_rx) = bounded::<GcItem>(8);
        let mut gc = GcSink::new(gc_tx);
        let gespawnd = push_voices_budgeted(&mut voices, nieuw, 48_000, cap, &mut gc);

        assert_eq!(gespawnd, 4, "werkeenheden niet teruggegeven");
        assert_eq!(voices.len(), budget + 4, "stemmen niet toegevoegd");
        let uitfadend = voices.iter().filter(|v| v.one_shot && v.releasing).count();
        assert_eq!(uitfadend, 4, "het staartbudget liep niet mee met elke push (kreeg {})", uitfadend);
        assert!(
            voices[budget..].iter().all(|v| !v.releasing),
            "een zojuist toegevoegde crossfade-stem werd meteen weer losgelaten"
        );
    }

    /// Zijn álle staarten net gespawnd (< 20 ms), dan wordt er niets gestolen:
    /// liever even boven het budget dan een akkoord zijn hele nagalm afnemen.
    #[test]
    fn staartbudget_laat_alles_met_rust_als_alle_staarten_vers_zijn() {
        let cap = 1024usize;
        let budget = (cap / 3).max(MAX_RELEASE_VOICES);
        let mut voices: Vec<PlayingVoice> = (0..=budget).map(|_| {
            let mut v = staart(0);
            crossfade_in(&mut v, 1.0, release_fade_ms(36), 48_000);
            v
        }).collect();

        budget_release_voices(&mut voices, 48_000, cap);

        assert!(voices.iter().all(|v| !v.releasing), "er werd toch een verse staart gestolen");
    }

    /// Korte noot: de som van de uitfadende speelnoot en de inkomende release
    /// blijft ≈ constant (geen niveaubult), en de staart dooft daarna netjes
    /// uit op 0.
    #[test]
    fn korte_noot_houdt_niveau_constant_en_dooft_uit() {
        let sr = 48_000u32;
        let midi = 36u8;
        let fade = release_fade_ms(midi); // 184 ms in de bas
        let decay_ms = 1200.0f32;

        let mut speel = PlayingVoice::new_from_sample(vlakke_sample(480_000), 1, 100, midi, 1.0);
        speel.envelope = 1.0;
        speel.target_envelope = 1.0;
        speel.release_ms(fade, sr);

        let mut rel = PlayingVoice::new_from_sample(
            vlakke_sample(480_000), 1, 100 | RELEASE_PIPE_FLAG, midi, 1.0,
        );
        rel.one_shot = true;
        crossfade_in_with_decay(&mut rel, 1.0, fade, decay_ms, sr);

        let fade_frames = (fade * 0.001 * sr as f32) as usize;
        let mut max_som = 0.0f32;
        for _ in 0..fade_frames {
            let (a, _) = speel.next_sample();
            let (b, _) = rel.next_sample();
            max_som = max_som.max(a + b);
        }
        assert!(max_som <= 1.02,
                "niveaubult bij het loslaten: {:.3} (instant-start gaf ~2,0)", max_som);
        assert!(rel.pending_decay_frames.is_none(), "de uitgestelde decay is niet gestart");
        assert!(rel.releasing, "de staart is na de infade niet gaan uitsterven");

        let rest = (decay_ms * 0.001 * sr as f32) as usize + fade_frames;
        for _ in 0..rest {
            if rel.is_finished() { break; }
            rel.next_sample();
        }
        assert!(rel.is_finished(), "staart dooft niet uit (envelope {:.5})", rel.envelope);
    }

    /// Controle op dezelfde opstelling: het OUDE gedrag (release instant op vol
    /// niveau) geeft wél een bult — bewijst dat de test hem zou zien.
    #[test]
    fn instant_start_release_geeft_wel_een_bult() {
        let sr = 48_000u32;
        let midi = 36u8;
        let fade = release_fade_ms(midi);

        let mut speel = PlayingVoice::new_from_sample(vlakke_sample(480_000), 1, 100, midi, 1.0);
        speel.envelope = 1.0;
        speel.target_envelope = 1.0;
        speel.release_ms(fade, sr);

        let mut rel = PlayingVoice::new_from_sample(
            vlakke_sample(480_000), 1, 100 | RELEASE_PIPE_FLAG, midi, 1.0,
        );
        rel.one_shot = true;
        rel.envelope = 1.0;
        rel.target_envelope = 1.0;
        rel.release_ms(1200.0, sr);

        let mut max_som = 0.0f32;
        for _ in 0..(fade * 0.001 * sr as f32) as usize {
            let (a, _) = speel.next_sample();
            let (b, _) = rel.next_sample();
            max_som = max_som.max(a + b);
        }
        assert!(max_som > 1.5, "verwachtte een niveaubult, kreeg {:.3}", max_som);
    }

    /// Landt de volledige sample halverwege de decay, dan mag de staart niet
    /// blijven hangen op de (tot de preload-rand ingekorte) snelheid: de
    /// resterende decay wordt herberekend en de staart dooft uit op tijd.
    #[test]
    fn upgrade_to_full_herberekent_de_resterende_decay() {
        let sr = 48_000u32;
        let mut rel = PlayingVoice::new_from_sample(
            vlakke_sample(480_000), 1, 100 | RELEASE_PIPE_FLAG, 48, 1.0,
        );
        rel.one_shot = true;
        rel.envelope = 1.0;
        rel.target_envelope = 1.0;
        rel.releasing = true;
        rel.age_samples = 0;
        rel.decay_end_age = Some(240_000); // 5 s decay
        // Doe alsof de fade was ingekort tot de preload-rand (~1,8 s).
        rel.envelope_speed = 1.0 / 86_400.0;

        rel.upgrade_to_full(vlakke_sample(480_000));

        let verwacht = 1.0f32 / 240_000.0;
        assert!((rel.envelope_speed - verwacht).abs() < verwacht * 0.05,
                "fadesnelheid niet hersteld: {} (verwacht ~{})", rel.envelope_speed, verwacht);
    }
}

/// Janitor per stem (0.7.40): een eindigende stem die de LAATSTE houder van
/// zijn sample is (de map heeft hem boven de RAM-kap al geëvict) mag die Arc
/// niet in de verwijderstap van de callback laten vallen — de deallocatie van
/// 1-3 MB hoort op de janitor-thread. Een bron die nog gedeeld wordt (map of
/// andere stem) is alleen een teller en wordt gewoon gedropt.
#[cfg(test)]
mod janitor_tests {
    use super::*;
    use std::sync::Weak;

    /// Sample zoals de laadpijplijn hem aflevert (1 s mono op uitgangsrate).
    fn sample() -> SampleRef {
        Arc::new(SampleData {
            data: vec![0.5; 48_000],
            right: None,
            sample_rate: 48_000,
            channels: 1,
            loop_start: None,
            loop_end: None,
        })
    }

    /// Uitgeklonken stem op `sample`: losgelaten en onder de stiltedrempel,
    /// dus `is_finished()` — precies wat de verwijderstap opruimt.
    fn uitgeklonken(sample: SampleRef, pipe: u32) -> PlayingVoice {
        let mut v = PlayingVoice::new_from_sample(sample, 1, pipe, 60, 1.0);
        v.releasing = true;
        v.envelope = 0.0;
        v.age_samples = 48_000;
        v
    }

    /// Klinkende stem (niet losgelaten, vol niveau, ouder dan 20 ms).
    fn klinkend(sample: SampleRef, pipe: u32) -> PlayingVoice {
        let mut v = PlayingVoice::new_from_sample(sample, 1, pipe, 60, 1.0);
        v.envelope = 1.0;
        v.age_samples = 48_000;
        v
    }

    /// Afvoerluik met een eigen kanaal, zodat de test kan zien wat de janitor
    /// zou krijgen.
    fn luik(kanaal: usize) -> (GcSink, Receiver<GcItem>) {
        let (tx, rx) = bounded::<GcItem>(kanaal);
        (GcSink::new(tx), rx)
    }

    /// De stem is de enige houder (de map heeft de sample geëvict; de
    /// decode_cache heeft alleen een Weak): de Arc moet de verwijderstap
    /// OVERLEVEN en via het gc-kanaal aankomen; pas het droppen van dat item
    /// — het werk van de janitor-thread — geeft de sample vrij.
    #[test]
    fn laatste_houder_gaat_via_het_gc_kanaal_niet_via_de_verwijderstap() {
        let s = sample();
        let zwak: Weak<SampleData> = Arc::downgrade(&s);
        let mut voices = vec![uitgeklonken(s, 100)];
        assert_eq!(zwak.strong_count(), 1, "opzet: de stem hoort de enige houder te zijn");
        let (mut gc, rx) = luik(8);

        remove_finished_voices(&mut voices, &mut gc);

        assert!(voices.is_empty(), "uitgeklonken stem niet verwijderd");
        assert_eq!(zwak.strong_count(), 1, "de sample werd ín de verwijderstap gedropt");
        assert!(gc.backlog.is_empty(), "kanaal had ruimte; niets hoort in de wachtrij");
        let item = rx.try_recv().expect("geen item op het gc-kanaal");
        match &item {
            GcItem::Sample(ontvangen) => {
                let nog = zwak.upgrade().expect("sample al weg");
                assert!(Arc::ptr_eq(&nog, ontvangen), "ander object op het kanaal");
            }
            _ => panic!("verkeerd item-type op het gc-kanaal (verwacht GcItem::Sample)"),
        }
        assert!(rx.try_recv().is_err(), "meer dan één item verstuurd");
        drop(item); // wat de janitor-thread doet
        assert_eq!(zwak.strong_count(), 0, "na de janitor-drop hoort de sample vrij te zijn");
    }

    /// Gedeelde bron — nog in de samples-map én bij een andere (klinkende)
    /// stem: de eindigende stem laat alleen zijn teller los, er gaat niets
    /// over het kanaal, en de overblijvende stemmen houden hun volgorde.
    #[test]
    fn gedeelde_bron_wordt_gewoon_gedropt_en_volgorde_blijft() {
        let s = sample();
        let mut map: HashMap<(u32, u32), SampleRef> = HashMap::new();
        map.insert((1, 100), s.clone());
        let mut voices = vec![
            uitgeklonken(s.clone(), 100),
            klinkend(s.clone(), 101),
            uitgeklonken(s.clone(), 100),
            klinkend(s.clone(), 102),
        ];
        assert_eq!(Arc::strong_count(&s), 6); // s + map + 4 stemmen
        let (mut gc, rx) = luik(8);

        remove_finished_voices(&mut voices, &mut gc);

        assert_eq!(Arc::strong_count(&s), 4, "tellers niet losgelaten: s + map + 2 klinkende");
        assert!(rx.try_recv().is_err(), "gedeelde bron hoort NIET naar de janitor");
        assert!(gc.backlog.is_empty());
        let over: Vec<u32> = voices.iter().map(|v| v.pipe_num).collect();
        assert_eq!(over, vec![101, 102], "klinkende stemmen weg of van volgorde veranderd");

        // Ook wanneer alleen een ANDERE stem hem nog vasthoudt (geen map):
        // koppel + directe aanslag op dezelfde pijp, één ervan klinkt uit.
        let t = sample();
        let zwak_t = Arc::downgrade(&t);
        voices.push(uitgeklonken(t.clone(), 104));
        voices.push(klinkend(t, 103));
        assert_eq!(zwak_t.strong_count(), 2);
        remove_finished_voices(&mut voices, &mut gc);
        assert!(rx.try_recv().is_err(), "bron die een andere stem nog vasthoudt ging naar de janitor");
        assert_eq!(zwak_t.strong_count(), 1, "teller van de uitgeklonken stem niet losgelaten");
        let over: Vec<u32> = voices.iter().map(|v| v.pipe_num).collect();
        assert_eq!(over, vec![101, 102, 103]);
    }

    /// Voice-stealing bij een volle kap: het direct verwijderde slachtoffer
    /// (bijna stille staart, enige houder van zijn geëvicte sample) gaat óók
    /// langs de janitor in plaats van in `swap_remove` te vallen.
    #[test]
    fn gestolen_stem_bij_volle_kap_gaat_ook_langs_de_janitor() {
        let cap = MIN_LIVE_VOICES;
        let gedeeld = sample();
        let mut voices: Vec<PlayingVoice> =
            (0..cap - 1).map(|i| klinkend(gedeeld.clone(), i as u32)).collect();
        let eigen = sample();
        let zwak = Arc::downgrade(&eigen);
        let mut staart = PlayingVoice::new_from_sample(eigen, 1, 999, 60, 1.0);
        staart.releasing = true;
        staart.envelope = 0.005; // < 0,01: mag hard weg
        staart.age_samples = 48_000;
        voices.push(staart);
        assert_eq!(voices.len(), cap);
        let (mut gc, rx) = luik(8);

        push_voice_capped(&mut voices, klinkend(gedeeld.clone(), 5000), 48_000, cap, &mut gc);

        assert_eq!(voices.len(), cap, "slachtoffer niet verwijderd of nieuwe stem niet toegevoegd");
        assert!(voices.iter().all(|v| v.pipe_num != 999), "niet de stille staart gestolen");
        assert!(voices.iter().any(|v| v.pipe_num == 5000), "nieuwe stem ontbreekt");
        assert_eq!(zwak.strong_count(), 1, "sample van de gestolen stem viel in de callback");
        let item = rx.try_recv();
        assert!(matches!(item, Ok(GcItem::Sample(_))), "gestolen sample niet op het gc-kanaal");
        drop(item);
        assert_eq!(zwak.strong_count(), 0);
    }

    /// Vol kanaal: het tweede item wordt in de wachtrij geparkeerd — nooit
    /// inline gedropt — en schuift bij de volgende callback alsnog door.
    #[test]
    fn vol_kanaal_parkeert_in_de_wachtrij_en_dropt_niets_inline() {
        let (mut gc, rx) = luik(1);
        let (a, b) = (sample(), sample());
        let (za, zb) = (Arc::downgrade(&a), Arc::downgrade(&b));
        let mut voices = vec![uitgeklonken(a, 100), uitgeklonken(b, 101)];

        remove_finished_voices(&mut voices, &mut gc);

        assert!(voices.is_empty());
        assert_eq!(za.strong_count() + zb.strong_count(), 2, "een sample viel inline");
        assert_eq!(gc.backlog.len(), 1, "tweede item niet in de wachtrij");
        // De janitor haalt het kanaal leeg; de volgende callback schuift de
        // wachtrij door.
        drop(rx.try_recv().expect("kanaal leeg"));
        gc.flush_backlog();
        assert!(gc.backlog.is_empty(), "wachtrij niet doorgeschoven");
        drop(rx.try_recv().expect("doorgeschoven item niet op het kanaal"));
        assert_eq!(za.strong_count() + zb.strong_count(), 0, "niet alles vrijgegeven");
    }

    /// Een stem op zijn preload-buffer: zelfde regel (laatste houder → janitor,
    /// als `GcItem::Preload`).
    #[test]
    fn preload_bron_volgt_dezelfde_regel() {
        let pb = Arc::new(PreloadBuffer {
            attack_data: vec![0.25; 4_800],
            attack_right: None,
            sample_rate: 48_000,
            total_samples: 4_800,
            loop_start: None,
            loop_end: None,
            source_path: std::path::PathBuf::from("pijp.wav"),
            channels: 1,
            bits_per_sample: 16,
            data_offset: 0,
            align_table: None,
            trim_start: 0,
            smpl_unity_note: None,
            smpl_pitch_fraction_cents: None,
        });
        let zwak = Arc::downgrade(&pb);
        let mut v = PlayingVoice::new_from_preload(pb, 1, 100, 60, 1.0, 48_000);
        v.releasing = true;
        v.envelope = 0.0;
        let mut voices = vec![v];
        let (mut gc, rx) = luik(8);

        remove_finished_voices(&mut voices, &mut gc);

        assert!(voices.is_empty());
        assert_eq!(zwak.strong_count(), 1, "preload viel in de verwijderstap");
        let item = rx.try_recv();
        assert!(matches!(item, Ok(GcItem::Preload(_))), "preload niet als GcItem::Preload verstuurd");
        drop(item);
        assert_eq!(zwak.strong_count(), 0);
    }
}

/// Tremulant-crossfade: de nieuwe stem moet ALTIJD binnen de doelbuffer landen.
/// De positie van de klinkende stem staat in het domein van zijn eigen bron
/// (volledige sample op uitgangsrate, óf preload op bronrate mét trim); de
/// doelbuffer is een andere opname met eigen lengte, trim en samplerate.
#[cfg(test)]
mod tremulant_crossfade_tests {
    use super::*;

    /// Kale preload-buffer: vlakke data (0,25) zodat een test posities meet en
    /// niet de golfvorm.
    fn preload(frames: usize, rate: u32, trim: usize, lp: Option<(u64, u64)>) -> Arc<PreloadBuffer> {
        Arc::new(PreloadBuffer {
            attack_data: vec![0.25; frames],
            attack_right: None,
            sample_rate: rate,
            total_samples: frames + trim,
            loop_start: lp.map(|(a, _)| a),
            loop_end: lp.map(|(_, b)| b),
            source_path: std::path::PathBuf::from("trem.wav"),
            channels: 1,
            bits_per_sample: 16,
            data_offset: 0,
            align_table: None,
            trim_start: trim,
            smpl_unity_note: None,
            smpl_pitch_fraction_cents: None,
        })
    }

    /// Klinkende stem die nog op zijn preload draait (bronrate ≠ uitgangsrate).
    fn stem_op_preload(pos: f64, frames: usize, rate: u32, trim: usize, out_rate: u32) -> PlayingVoice {
        let mut v = PlayingVoice::new_from_preload(
            preload(frames, rate, trim, Some((frames as u64 / 4, frames as u64))),
            7, 60, 60, 1.0, out_rate,
        );
        v.position = pos;
        v
    }

    /// Klinkende stem op de volledige sample: die staat al op uitgangsrate
    /// (rate = 1,0) en is NIET getrimd — positie = frames sinds bestandsbegin.
    fn stem_op_volledige_sample(pos: f64, frames: usize, out_rate: u32) -> PlayingVoice {
        let sample: SampleRef = Arc::new(SampleData {
            data: vec![0.25; frames],
            right: None,
            sample_rate: out_rate,
            channels: 1,
            loop_start: None,
            loop_end: None,
        });
        let mut v = PlayingVoice::new_from_sample(sample, 7, 60, 60, 1.0);
        v.position = pos;
        v
    }

    /// Gelijke samplerate, verschillende trim: de nieuwe stem landt op exact
    /// hetzelfde TIJDSTIP in de doelopname (trimverschil netjes verrekend).
    #[test]
    fn crossfade_rekent_trimverschil_om() {
        let bron = stem_op_preload(40_000.0, 200_000, 44_100, 1_000, 48_000);
        let doel = preload(200_000, 44_100, 500, Some((50_000, 190_000)));

        let pos = bron.crossfade_position_in(&doel, 48_000);

        // (40 000 + 1 000)/44 100 s → ×44 100 − 500 = 40 500 doelframes.
        assert!((pos - 40_500.0).abs() < 1.0, "verwachtte ~40500, kreeg {pos}");
        assert!(pos <= (doel.attack_data.len() - 2) as f64);
    }

    /// 44,1 kHz-set op een 48 kHz-uitgang, lang vastgehouden noot op de
    /// VOLLEDIGE sample: de ruwe positie (480 000 output-frames = 10 s) ligt
    /// ver voorbij het einde van de trem-preload (2 s). De omrekening moet hem
    /// in de loop van de doelbuffer terugvouwen in plaats van in stilte.
    #[test]
    fn crossfade_vouwt_lange_noot_terug_in_de_loop() {
        let bron = stem_op_volledige_sample(480_000.0, 960_000, 48_000);
        let (ls, le) = (22_050u64, 80_000u64);
        let doel = preload(88_200, 44_100, 0, Some((ls, le)));

        // Zonder omrekening (het oude gedrag) lag de startpositie buiten de
        // buffer — bewijs dat deze test de regressie zou zien.
        assert!(bron.position > doel.attack_data.len() as f64);

        let pos = bron.crossfade_position_in(&doel, 48_000);

        assert!(pos >= ls as f64 && pos < le as f64,
                "landde buiten de loop van de doelbuffer: {pos}");
        // Exacte terugvouwing: 10 s = 441 000 doelframes → ls + (441000−ls) mod (le−ls).
        let verwacht = ls as f64 + (441_000.0 - ls as f64) % (le - ls) as f64;
        assert!((pos - verwacht).abs() < 1.0, "verwachtte ~{verwacht}, kreeg {pos}");
    }

    /// Doelbuffer zonder bruikbare looppunten: veilig midden in de buffer —
    /// nooit opnieuw door de attack en nooit voorbij het einde.
    #[test]
    fn crossfade_zonder_looppunten_landt_midden_in_de_buffer() {
        let bron = stem_op_preload(300_000.0, 400_000, 48_000, 0, 48_000);
        let doel = preload(40_000, 44_100, 0, None);

        let pos = bron.crossfade_position_in(&doel, 48_000);

        assert!((pos - 20_000.0).abs() < 1.0, "verwachtte het midden, kreeg {pos}");
        assert!(pos <= (doel.attack_data.len() - 2) as f64);
    }

    /// Alle combinaties van bron-/doelrate, trim en lengte: de uitkomst ligt
    /// ALTIJD binnen de doelbuffer, en de gemaakte stem klinkt ook echt
    /// (leest geldige data i.p.v. stilte).
    #[test]
    fn crossfade_blijft_bij_elke_ratio_binnen_de_doelbuffer() {
        let rates = [44_100u32, 48_000, 96_000];
        let out_rates = [44_100u32, 48_000];
        for &out in &out_rates {
            for &br in &rates {
                for &dr in &rates {
                    for &trim in &[0usize, 5_000] {
                        for &pos in &[0.0f64, 1_000.0, 250_000.0, 5_000_000.0] {
                            let bron = stem_op_preload(pos, 300_000, br, trim, out);
                            let doel = preload(60_000, dr, trim / 2, Some((15_000, 58_000)));
                            let p = bron.crossfade_position_in(&doel, out);
                            assert!(p.is_finite() && p >= 0.0 && p <= 59_998.0,
                                    "buiten de doelbuffer: {p} (bron {br} Hz, doel {dr} Hz, uit {out} Hz, pos {pos})");

                            // De stem die hiermee ontstaat moet klinken: na de
                            // infade komt er signaal uit, en de leespositie
                            // blijft binnen de data.
                            let mut nv = PlayingVoice::new_crossfade(
                                doel.clone(), 7, 60 | TREM_FLAG, 60, 1.0, p, out,
                            );
                            let mut laatste = 0.0f32;
                            for _ in 0..2_000 {
                                laatste = nv.next_sample().0;
                            }
                            assert!(laatste > 0.0,
                                    "crossfade-stem bleef stil (bron {br} Hz, doel {dr} Hz, pos {pos})");
                            assert!(nv.position >= 0.0 && nv.position < 60_000.0,
                                    "leespositie liep de buffer uit: {}", nv.position);
                        }
                    }
                }
            }
        }
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
    /// Gevraagde samplerate in Hz (0.7.48). None/0 = wat het apparaat als
    /// standaard opgeeft. Alleen gehonoreerd als het apparaat hem aanbiedt;
    /// anders blijft de standaard staan en komt er een waarschuwing in het log.
    pub sample_rate: Option<u32>,
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

/// Is de ASIO-driver in dít proces ooit succesvol geladen? Onderscheidt de
/// éénrichtingsdeur (driver werkte, is vrijgegeven en kan niet her-initialiseren
/// → een app-herstart helpt) van "ASIO doet het hier gewoon niet" (geen
/// herstart proberen).
static ASIO_WORKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn asio_worked_this_process() -> bool {
    ASIO_WORKED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Is ASIO in dit proces in de éénrichtingsdeur beland? Twee bekende vormen:
/// de gecachte driver is vrijgegeven (voor een WASAPI-wissel; ASIO4ALL kan per
/// proces maar één keer initialiseren), of de cache leeft nog maar de driver
/// antwoordt niet meer op enumeratie (ESI GIGAPORT eX na een WASAPI-uitstap).
/// Alleen déze twee rechtvaardigen een herstel-herstart; een gewoon afwezig of
/// bezet apparaat moet gewoon een nette foutmelding geven.
static ASIO_DOOR_CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn asio_door_closed() -> bool {
    ASIO_DOOR_CLOSED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Wis de "deur dicht"-vlag; aanroepen zodra ASIO weer werkt (na een geslaagde
/// wissel/herstart), zodat een látere vastloper opnieuw hersteld mag worden.
pub fn asio_clear_door_closed() {
    ASIO_DOOR_CLOSED.store(false, std::sync::atomic::Ordering::Relaxed);
}

// (asio_cache_present is vervallen: de herstel-herstart in state.rs vuurt nu
// óók wanneer de cache nog bestaat maar de driver dood is voor enumeratie —
// het GIGAPORT-geval — dus de aanwezigheidscheck deed er niet meer toe.)

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
            // Transiënte enumeratie-hapering niet afstraffen met een cache-drop:
            // droppen vernietigt de driver en maakt ASIO door de her-init-
            // beperking permanent onbruikbaar. Kort retryen en anders de cache
            // BEHOUDEN en een tijdelijke fout teruggeven.
            for poging in 0..3u8 {
                if let Some(d) = c.host.output_devices().ok().and_then(|mut devs| {
                    devs.find(|d| d.name().map(|n| n == want_name).unwrap_or(false))
                }) {
                    return Ok(d);
                }
                if poging < 2 { std::thread::sleep(std::time::Duration::from_millis(100)); }
            }
            warn!("Gecachte ASIO-driver '{}' antwoordt niet op enumeratie; cache blijft behouden", c.driver_name);
            // Éénrichtingsdeur-vorm 2: driver leeft, maar praat niet meer. Alleen
            // een verse processtart krijgt hem terug (zie asio_door_closed).
            ASIO_DOOR_CLOSED.store(true, std::sync::atomic::Ordering::Relaxed);
            return Err(format!(
                "ASIO-driver '{}' is geladen maar reageert tijdelijk niet; probeer de wissel opnieuw",
                c.driver_name));
        } else {
            warn!("Andere ASIO-driver gevraagd ({:?} i.p.v. '{}'); cache wordt ververst — her-init kan falen",
                  want, c.driver_name);
        }
    }
    *cache = None; // alleen bij een expliciet ándere driver: oude loslaten vóór een nieuwe load

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
    ASIO_WORKED.store(true, std::sync::atomic::Ordering::Relaxed);
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
        // Éénrichtingsdeur-vorm 1: vrijgegeven driver kan in dit proces niet
        // meer initialiseren (zie asio_door_closed).
        ASIO_DOOR_CLOSED.store(true, std::sync::atomic::Ordering::Relaxed);
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
    /// Teller van geleverde audio-callbacks (loopt zolang de stream leeft).
    /// Gebruikt door het noodherstel om een vals alarm uit te sluiten.
    pub callbacks_seen: Arc<AtomicU64>,
    /// Watchdog-pauze: tijdens een orgel-load is een callback-stilte verwacht
    /// (zware disk/CPU hapert ASIO4ALL). Zolang deze vlag staat escaleert de
    /// watchdog niet (geen herbouwpogingen, geen volledige-herstart-vlag).
    pub watchdog_hold: Arc<AtomicBool>,
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
        // Crossfade-tabel nu opbouwen: de audio-thread mag niet alloceren.
        let _ = xfade_lut();
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
        let watchdog_hold = Arc::new(AtomicBool::new(false));
        let watchdog_hold_clone = watchdog_hold.clone();
        // Readiness channel: the audio thread reports whether the output stream
        // actually built+started, so we can fail cleanly (e.g. ASIO unavailable)
        // instead of silently ending up with no audio.
        let (ready_tx, ready_rx) = bounded::<Result<(), String>>(1);
        let done_clone = thread_done.clone();
        thread::spawn(move || {
            if let Err(e) = run_audio_thread(
                command_rx, voice_count_clone, peak_left_clone, peak_right_clone,
                running_clone, sr_clone, cfg, host_clone, device_clone, buffer_clone,
                channels_clone, restart_clone, callbacks_clone, watchdog_hold_clone, ready_tx,
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
            callbacks_seen,
            watchdog_hold,
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

    /// Kloon van de command-zender. Voor het realtime-notenpad (NoteOn/NoteOff):
    /// de aanroeper pakt deze onder een KORTE player-read-lock, laat de lock los,
    /// en doet daarna non-blocking `try_send` — zo houdt een reeks noot-sends de
    /// read-lock niet vast terwijl een audio-wissel op de write-lock wacht.
    pub fn command_sender(&self) -> Sender<AudioCommand> {
        self.command_tx.clone()
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
    watchdog_hold: Arc<AtomicBool>,
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

    // Gevraagde samplerate (0.7.48): alleen overnemen als het apparaat hem
    // écht aanbiedt. cpal meldt per configuratie een min/max-bereik; valt de
    // gevraagde waarde daarbinnen én klopt het kanaalaantal, dan gebruiken we
    // hem. Zo is 96 kHz te kiezen op een interface die dat kan, zonder dat een
    // onmogelijke waarde de stream laat mislukken.
    let gevraagde_rate = cfg.sample_rate.filter(|&r| r > 0);
    let supported = match gevraagde_rate {
        Some(r) if r != supported.sample_rate().0 => {
            let kanalen = supported.channels();
            let passend = device.supported_output_configs().ok().and_then(|it| {
                it.filter(|c| c.channels() == kanalen
                        && c.min_sample_rate().0 <= r && r <= c.max_sample_rate().0)
                    .map(|c| c.with_sample_rate(cpal::SampleRate(r)))
                    .next()
            });
            match passend {
                Some(c) => {
                    info!("Samplerate {} Hz gevraagd en beschikbaar", r);
                    c
                }
                None => {
                    warn!("Samplerate {} Hz niet beschikbaar op '{}'; {} Hz blijft staan",
                          r, device.name().unwrap_or_default(), supported.sample_rate().0);
                    supported
                }
            }
        }
        _ => supported,
    };
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

    // Kanaalaantal van de stream = wat cpal als default meldt: onder ASIO
    // `ASIOGetChannels().outs` (ALLE uitgangen van de driver, bv. 8 op een
    // 8-kanaals interface), onder WASAPI het shared-mode mix-formaat van het
    // endpoint (= de luidsprekerconfiguratie in Windows; cpal 0.15 kent geen
    // exclusive mode en accepteert in shared mode geen ander aantal). Meer
    // aanvragen is dus onmogelijk én onnodig — de UI moet dít aantal tonen
    // (StatusDto.channels), niet zelf enumereren. De logregel hieronder
    // ("kanalen: N") is het verificatiepunt: op een 8-uits ASIO-interface
    // hoort hier 8 te staan.
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
    let division_output_channels: Arc<RwLock<Vec<Vec<u16>>>> = Arc::new(RwLock::new(vec![Vec::new(); 32]));
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
    // Hertemper-tabel (zelfde Arc-swap-patroon): (stop_id, pipe_num) → cents die
    // in hertemper-modus de ODF-PitchTuning van die pijp vervangen.
    let odf_retune: Arc<RwLock<Arc<HashMap<(u32, u32), f32>>>> = Arc::new(RwLock::new(Arc::new(HashMap::new())));
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
    // Global tuning ratio (derived from cents offset) + hertemper-vlag
    // (true = odf_retune vervangt de ODF-PitchTuning per pijp).
    let temperament_ratios: Arc<RwLock<([f64; 12], bool)>> = Arc::new(RwLock::new(([1.0; 12], false)));
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
    // Sinds 0.7.40 ook per eindigende stem (zie `GcSink`): een stem die de
    // laatste houder van een geëvicte sample is, stuurt zijn Arc hierheen.
    let (gc_tx, gc_rx) = bounded::<GcItem>(64);
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
    let odf_retune_clone = odf_retune.clone();
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
    // Toetsduur-afhankelijke releases: (stop, pijp) → gesorteerde (max_ms, key).
    let mut release_meta: std::collections::HashMap<(u32, u32), Vec<(i32, u32)>> = std::collections::HashMap::new();
    // Door de sampleset opgegeven crossfade-duur per release-key (ms).
    let mut release_xfade: Arc<HashMap<(u32, u32), u32>> = Arc::new(HashMap::new());
    let mut load_generation: u64 = 0;
    // Gestapelde ranks / perspectieven (0.7.38): laadplan per stop (Arc-swap
    // via RegisterRankLayout), lineaire gain per perspectief-slot en een
    // vooraf gereserveerde scratch-Vec voor de release-spawns per laag in
    // NoteOff (nooit krimpen; groeit hooguit één keer bij >32 lagen).
    let mut rank_layout: Arc<HashMap<u32, Vec<(u8, u8)>>> = Arc::new(HashMap::new());
    let mut persp_gain: [f32; MAX_LAYERS] = [1.0; MAX_LAYERS];
    let mut release_spawn_scratch: Vec<PlayingVoice> = Vec::with_capacity(32);
    // RAM-plafond op volledig geladen samples. Zonder plafond groeide het
    // geheugen tijdens lang spelen onbegrensd (elke gespeelde pijp + release
    // blijft resident): uren spelen op een grote GO-set = vele GB's → paging →
    // gekraak, en een orgelwissel daarna dropte al die GB's in één callback.
    // Eviction is altijd veilig: klinkende voices houden hun eigen Arc vast;
    // een geëvicte pijp start de volgende keer gewoon weer van zijn preload.
    const SAMPLES_BYTES_CAP: usize = 1_500_000_000; // ~1,5 GB
    let mut samples_bytes: usize = 0;
    let mut sample_fifo: std::collections::VecDeque<(u32, u32)> = std::collections::VecDeque::new();
    // Afvoerluik naar de janitor (zender + wachtrij voor wat de janitor even
    // niet aankon, zie `GcSink`): alles wat de callback loslaat en groot kan
    // zijn — oude maps, stemmenlijsten, geëvicte samples én de bron van een
    // eindigende stem — gaat hierlangs in plaats van inline te droppen.
    let mut gc = GcSink::new(gc_tx.clone());
    // Master-limiter (unit-getest in vpo-audio): plafond 0.97 (~-0,26 dBFS),
    // attack ~1,5 ms (gedoseerd — één piek trekt niet het hele orgel omlaag),
    // release ~250 ms. Zie het limiterblok in de frame-lus voor waarom dit de
    // kale tanh-vervorming vervangt.
    // Release 1,2 s (was 0,25 s): na het loslaten van een groot akkoord veerde
    // de gain in een kwart seconde terug omhoog terwijl release-samples + galm
    // nog klonken — de staart zwol daardoor hoorbaar aan ("hik"). Met de
    // tragere release (plus de 300 ms-hold in de limiter zelf) volgt de gain
    // het natuurlijke uitsterven in plaats van er tegenin te pompen.
    let mut master_limiter = MasterLimiter::new(0.97, 0.0015, 1.2, sample_rate);
    // Generator voor het luidspreker-testsignaal (zie set_test_signal).
    let mut testsignaal = Testsignaal::nieuw();
    // Voor de galm-bypass: bij de overgang naar mix 0 één keer de staart
    // wissen — anders klinkt bij her-inschakelen eerst een bevroren, oude
    // staart die niets met het huidige spel te maken heeft.
    let mut reverb_was_audible = true;
    // Zwelkast per divisie (alleen op de render-thread): `division_gains` is
    // de DOELstand (gezet door SetDivisionGain); hier de gesmoothede stand
    // (τ ≈ 20 ms, per MIX_BLOCK bijgewerkt), het lineaire volume daaruit en
    // of het low-pass-filter overgeslagen wordt (kast open / geen zwelkast).
    // Voorheen sprong gain + cutoff hard per callback (zipper/trapjes) en
    // stond er powf + set_cutoff in de per-frame-lus.
    let mut swell_g = [1.0f32; 32];
    let mut swell_vol = [1.0f32; 32];
    let mut swell_bypass = [true; 32];
    let mut swell_dirty = [false; 32];

    let sample_format = supported.sample_format();
    // Blokbuffers voor de stem-major mengloop (zie render): per frame in het
    // blok de wind/trem-modulatie en de divisie-sommen. Eén keer op de heap,
    // verplaatst in de closure — de callback alloceert niets.
    const MIX_BLOCK: usize = 256;
    let mut blk_div_l: Vec<[f32; 32]> = vec![[0.0; 32]; MIX_BLOCK];
    let mut blk_div_r: Vec<[f32; 32]> = vec![[0.0; 32]; MIX_BLOCK];
    // Deel-opteltabellen voor de stukken 1.. (stuk 0 is blk_div_l/r zelf, zodat
    // het geval "één stuk" geen enkele extra bewerking kost). Eén keer op de
    // heap; de callback alloceert niets.
    let mut blk_part_l: Vec<Vec<[f32; 32]>> =
        (1..MAX_MENG_STUKKEN).map(|_| vec![[0.0f32; 32]; MIX_BLOCK]).collect();
    let mut blk_part_r: Vec<Vec<[f32; 32]>> =
        (1..MAX_MENG_STUKKEN).map(|_| vec![[0.0f32; 32]; MIX_BLOCK]).collect();
    let mut blk_wind_rate: Vec<[f64; 32]> = vec![[1.0; 32]; MIX_BLOCK];
    let mut blk_wind_gain: Vec<[f32; 32]> = vec![[1.0; 32]; MIX_BLOCK];
    let mut blk_trem_rate: Vec<[f64; 32]> = vec![[1.0; 32]; MIX_BLOCK];
    let mut blk_trem_amp: Vec<[f32; 32]> = vec![[1.0; 32]; MIX_BLOCK];
    let mut overload_callbacks: u64 = 0;
    let mut last_overload_warn: Option<std::time::Instant> = None;
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

            // Polyfonie-kap (instelbaar) één keer per callback lezen; starttijd
            // voor de belastingsmeter.
            let live_cap = polyphony_target();
            let render_t0 = std::time::Instant::now();

            // Aantal frames van DEZE callback = het werkbudget. De vaste kappen
            // (16 integraties, 256 commando's) waren bedacht voor 3-6 ms-buffers;
            // bij ASIO met 32 frames (0,67 ms) is dezelfde hoeveelheid werk een
            // veelvoud van de deadline — precies het haperen bij een trapwissel
            // van het crescendo (per trap: ReleaseStop per weggetrokken register
            // + een NoteOn per ingedrukte toets per bijgetrokken register).
            // Daarom schalen de budgetten nu mee met de buffergrootte; wat niet
            // past blijft FIFO in de queue voor de volgende callback (bij 32
            // frames dus hooguit enkele ms later — onhoorbaar).
            let frames_now = data.len() / channels.max(1);
            RENDER_FRAMES_NOW.store(frames_now as u32, Ordering::Relaxed);
            // Stemstarts (NoteOn-fan-out, release-spawns van NoteOff én van
            // ReleaseStop): het dure werk, elk met voice-scans onder de
            // write-lock. Óók de NoteOff zelf telt mee (één eenheid per bericht):
            // een slotakkoord loslaten stuurt een NoteOff per register per toets
            // — inclusief alle koppeldoelen — en elk daarvan doet minimaal een
            // O(stemmen)-scan. frames/8 komt neer op een constante ~6000
            // eenheden per seconde (onafhankelijk van de buffergrootte): een
            // tutti-registratie van 300 stemmen klinkt dan binnen ~50 ms
            // volledig — ruim binnen wat een echte registerlade doet, en per
            // callback hooguit ~10 % van de rendertijd.
            // De ondergrens van 8 is essentieel: het budget kan nooit 0 worden,
            // dus élke callback verwerkt minstens één bericht en de queue kan
            // niet vollopen (wat niet past blijft FIFO staan).
            let spawn_budget = (frames_now / 8).max(8);
            // Afgeronde achtergrond-loads integreren: map-insert + O(V)-scan.
            let integrate_budget = (frames_now / 32).max(2);

            // Achtergesteld opruimwerk alsnog naar de janitor proberen te
            // schuiven (kanaal was vol op het moment van ontstaan).
            gc.flush_backlog();

            // Check for completed background loads. Gecapt per callback (budget
            // geschaald met de buffergrootte, zie boven): elke integratie doet een
            // map-insert + voice-scan; tientallen tegelijk verwerken zou de
            // callback over zijn deadline duwen. De rest wacht gewoon één
            // callback langer in het kanaal.
            let mut integrated = 0;
            while integrated < integrate_budget {
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
                        let size = sample.bytes();
                        if let Some(old) = samples_clone.write().insert(key, sample.clone()) {
                            // Zelfde key opnieuw geladen: oude telling eraf en
                            // de oude Arc niet hier laten vallen (janitor-check).
                            samples_bytes = samples_bytes
                                .saturating_sub(old.bytes());
                            gc.dispose_sample(old);
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
            // thread zodat de callback geen grote deallocaties doet. Klinkt de
            // pijp nog, dan blijft de stem de sample vasthouden en verhuist de
            // deallocatie naar het moment dat die stem eindigt — óók dan via de
            // janitor (zie `GcSink::dispose_voice`).
            let mut evicted = 0;
            while samples_bytes > SAMPLES_BYTES_CAP && evicted < 4 {
                let Some(old_key) = sample_fifo.pop_front() else { break; };
                if let Some(old) = samples_clone.write().remove(&old_key) {
                    samples_bytes = samples_bytes
                        .saturating_sub(old.bytes());
                    gc.dispose_sample(old);
                    evicted += 1;
                }
            }

            // Process commands. Gecapt per callback: een tutti-akkoord op veel
            // registers (of een crescendo-trapwissel) levert in één klap honderden
            // NoteOn/ReleaseStop-commando's; die allemaal binnen één callback
            // verwerken (met map-lookups en voice-scans per commando) overschrijdt
            // de realtime-deadline. Het DURE werk (stemstarts) heeft nu een budget
            // dat met de buffergrootte meeschaalt; goedkope commando's (gains,
            // config) blijven onder de vaste bovengrens van 256 vallen zodat een
            // stroom kleine commando's de callback ook niet kan vullen.
            // Zodra het budget op is stoppen we met tappen — NIET alleen voor
            // NoteOn: de FIFO-volgorde moet intact blijven, anders komt een
            // NoteOff vóór zijn NoteOn te liggen (hanger).
            let mut cmds_done = 0u32;
            let mut spawns_done = 0usize;
            while cmds_done < 256 && spawns_done < spawn_budget {
                let Ok(cmd) = command_rx_clone.try_recv() else { break };
                cmds_done += 1;
                match cmd {
                    AudioCommand::NoteOn { stop_id, pipe_num, midi_note, velocity } => {
                        // Fan-out over de lagen van deze stop (gestapelde ranks
                        // en ingeschakelde perspectieven, 0.7.38): één NoteOn
                        // → één voice per laag, elk onder zijn eigen laagsleutel.
                        // Stops zonder lagen: precies het oude pad ([(0,0)]).
                        let layout: &[(u8, u8)] = rank_layout
                            .get(&stop_id)
                            .map(|v| v.as_slice())
                            .unwrap_or(&DEFAULT_LAYOUT);
                        let mut spawned_any = false;
                        // Tremulantstand van dit register: één lookup per
                        // NoteOn (gold voorheen pas ná de refcount-check).
                        let trem_on = trem_active_clone.read().contains(&stop_id);
                        for &(layer, slot) in layout {
                        // Staat de tremulant aan én bestaat er een trem-opname
                        // voor deze pijp, dan is de TREM-sleutel de speelsleutel:
                        // attack, achtergrond-load, full sample en release staan
                        // daar allemaal onder. Zonder deze scheiding overschreef
                        // de eerste achtergrond-load de gedeelde sleutel en
                        // negeerde elke volgende NoteOn de tremulantstand.
                        let base_key = layer_key(pipe_num, layer);
                        let key_pipe = if trem_on
                            && preloads_clone.read().contains_key(&(stop_id, base_key | TREM_FLAG))
                        {
                            base_key | TREM_FLAG
                        } else {
                            base_key
                        };
                        let key = (stop_id, key_pipe);

                        // Één luchtkolom per pijp: klinkt deze pijp al (via de
                        // directe route of via een koppel), dan géén tweede voice
                        // spawnen maar de refcount bumpen. NoteOff decrementeert;
                        // de release komt pas als álle bronnen hebben losgelaten.
                        // Voorkomt (a) dubbele amplitude + comb-filter bij
                        // dubbele aanslag, (b) het "ontkoppelt"-effect waarbij
                        // één bron loslaten de pijp stilzette terwijl een andere
                        // nog vasthield (pedaal-koppel + zelfde noot op HW).
                        {
                            let mut voices_lock = voices_clone.write();
                            if let Some(v) = voices_lock.iter_mut().find(|v|
                                v.stop_id == stop_id && v.pipe_num == key.1
                                    && !v.releasing && !v.one_shot
                            ) {
                                v.note_on_count = v.note_on_count.saturating_add(1);
                                spawned_any = true;
                                continue;
                            }
                        }

                        // First check for fully loaded sample
                        let sample_opt = {
                            let samples_lock = samples_clone.read();
                            samples_lock.get(&key).cloned()
                        };

                        // One-shot (ODF Percussive)? Dan niet loopen maar uitklinken.
                        let one_shot = percussive_stops_clone.read().contains(&stop_id);

                        if let Some(sample) = sample_opt {
                            // Full sample available - instant playback
                            let mut voice = PlayingVoice::new_from_sample(sample.clone(), stop_id, key.1, midi_note, velocity);
                            voice.one_shot = one_shot;
                            voice.persp_slot = slot;
                            spawned_any = true;
                            { let mut vl = voices_clone.write(); budget_release_voices(&mut vl, sample_rate, live_cap); push_voice_capped(&mut vl, voice, sample_rate, live_cap, &mut gc); }
                        } else {
                            // Preload-buffer (droog of tremulant — de sleutel
                            // draagt de stand al).
                            let preload_opt = {
                                let preloads_lock = preloads_clone.read();
                                preloads_lock.get(&key).cloned()
                            };

                            if let Some(preload) = preload_opt {
                                // Start playing from preload buffer immediately
                                let mut voice = PlayingVoice::new_from_preload(preload.clone(), stop_id, key.1, midi_note, velocity, sample_rate);
                                voice.one_shot = one_shot;
                                voice.persp_slot = slot;
                                spawned_any = true;

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

                                { let mut vl = voices_clone.write(); budget_release_voices(&mut vl, sample_rate, live_cap); push_voice_capped(&mut vl, voice, sample_rate, live_cap, &mut gc); }
                            }
                        }
                        } // for layer
                        // Elke laag is een mogelijke stemstart: tel de hele
                        // fan-out mee in het werkbudget van deze callback.
                        spawns_done += layout.len();
                        if !spawned_any {
                            // Max één warn per pijp: dit pad is bereikbaar in
                            // normaal spel (onopgeloste REF-pijpen) en logde
                            // voorheen bij ÉLKE toetsaanslag — bestand-I/O
                            // onder een globale mutex in de audio-callback
                            // (auditbevinding 3). Alleen als géén enkele laag
                            // iets spawnde (een laag met een gat is normaal).
                            if missing_sample_warned.insert((stop_id, pipe_num)) {
                                warn!("NoteOn: No sample or preload for stop={}, pipe={}", stop_id, pipe_num);
                            }
                        }
                    }
                    AudioCommand::RegisterReleaseMeta(m) => {
                        info!("Toetsduur-releases geregistreerd voor {} pijpen (MaxKeyPressTime)", m.len());
                        release_meta = m;
                    }
                    AudioCommand::RegisterRankLayout(m) => {
                        let multi = m.values().filter(|v| v.len() > 1).count();
                        if multi > 0 {
                            info!("Rank-layout: {} stops met meerdere lagen (gestapelde ranks/perspectieven)", multi);
                        }
                        rank_layout = m;
                    }
                    AudioCommand::SetPerspectiveGain { slot, gain_db } => {
                        if slot > 0 && (slot as usize) < MAX_LAYERS {
                            persp_gain[slot as usize] = 10f32.powf(gain_db / 20.0);
                        }
                    }
                    AudioCommand::NoteOff { stop_id, pipe_num } => {
                        // Echte release-samples (opgenomen kerkakoestiek): staan
                        // onder de gemarkeerde key. Gevonden → start een one-shot
                        // release-voice op het huidige niveau van de stervende
                        // voice; de sustain fade't toonhoogte-afhankelijk uit
                        // (GrandOrgue get_fader_length: bas ~184 ms, discant ~6 ms)
                        // wat als crossfade werkt. Niet gevonden → fade alleen.
                        // De KEUZE van de release (kort R0 / lang R1 / default)
                        // hangt af van de gespeelde duur en gebeurt per voice in
                        // de lus (MaxKeyPressTime, 0.7.26). Match op de KALE
                        // pijp (base_pipe): één NoteOff laat alle lagen van
                        // die toets los, elk met de release van zijn eigen laag
                        // (release_spawn_scratch: één release-voice per laag).
                        let mut voices_lock = voices_clone.write();
                        for voice in voices_lock.iter_mut() {
                            // One-shot-voices (Percussive: klok) klinken uit en
                            // negeren note-off, zoals GrandOrgue dat doet.
                            if voice.stop_id == stop_id && base_pipe(voice.pipe_num) == pipe_num
                                && !voice.releasing && !voice.one_shot
                            {
                                // Refcount: er is meer dan één bron actief op
                                // deze pijp → alleen decrementeren, geen release.
                                if voice.note_on_count > 1 {
                                    voice.note_on_count -= 1;
                                    continue;
                                }
                                voice.note_on_count = 0;
                                // Crossfade-duur tussen speelnoot en release: de
                                // automatische (GrandOrgue get_fader_length) tenzij
                                // de gekozen release een eigen lengte meebrengt.
                                let mut fade_ms = release_fade_ms(voice.midi_note);
                                {
                                    // Laagsleutel van déze voice (niet de inkomende
                                    // kale pipe_num): release-meta en -buffers
                                    // staan per laag geregistreerd. De TREM-vlag
                                    // reist mee: een tremulant-voice zoekt eerst
                                    // zijn eigen trem-release op.
                                    let vpipe = voice.pipe_num;
                                    let note_ms = voice.age_samples as f32 * 1000.0 / (sample_rate.max(1)) as f32;
                                    // Toetsduur-release kiezen: eerste variant met
                                    // max_ms ≥ gespeelde duur; anders de default
                                    // (-1, achteraan). Geen meta → klassieke key.
                                    let pick = |vp: u32| -> u32 {
                                        release_meta.get(&(stop_id, vp))
                                            .and_then(|list| list.iter()
                                                .find(|(max_ms, _)| *max_ms >= 0 && note_ms as i64 <= *max_ms as i64)
                                                .or_else(|| list.last())
                                                .map(|(_, key)| *key))
                                            .unwrap_or(vp | RELEASE_PIPE_FLAG)
                                    };
                                    let mut chosen_pipe = pick(vpipe);
                                    let mut release_key = (stop_id, chosen_pipe);
                                    let mut rel_full = samples_clone.read().get(&release_key).cloned();
                                    // Preload óók bij een geladen full sample ophalen:
                                    // die draagt trim_start = segment-start (release-
                                    // uitsnede in hetzelfde bestand).
                                    let mut rel_pre = preloads_clone.read().get(&release_key).cloned();
                                    if rel_full.is_none() && rel_pre.is_none() && vpipe & TREM_FLAG != 0 {
                                        // Sampleset zonder aparte trem-release: de
                                        // droge release van dezelfde pijp geldt dan
                                        // ook met tremulant aan (GO IsTremulant=-1).
                                        chosen_pipe = pick(vpipe & !TREM_FLAG);
                                        release_key = (stop_id, chosen_pipe);
                                        rel_full = samples_clone.read().get(&release_key).cloned();
                                        rel_pre = preloads_clone.read().get(&release_key).cloned();
                                    }
                                    if rel_full.is_none() && rel_pre.is_none() {
                                        voice.release_ms(fade_ms, sample_rate);
                                        continue;
                                    }
                                    // Door de sampleset opgegeven crossfade-duur
                                    // (ReleaseCrossfadeLength) wint van de
                                    // automatische, toonhoogte-afhankelijke duur.
                                    if let Some(ms) = release_xfade.get(&release_key) {
                                        fade_ms = *ms as f32;
                                    }
                                    // Niveau-overname: de release start op het
                                    // huidige envelope-niveau van de sustain.
                                    // Staccato-schaling geldt voor ÉLKE gekozen
                                    // release, ook een MaxKeyPressTime-variant
                                    // (GrandOrgue CreateReleaseSampler kent geen
                                    // uitzondering voor de default): de "time to
                                    // full reverb" wordt berekend uit de lengte
                                    // van de GEKOZEN release. Een R1 die bij
                                    // 150 ms toetsdruk is opgenomen dekt een tik
                                    // van 40 ms niet — zonder schaling bloeide
                                    // die staart op vol niveau op.
                                    let rel_secs = rel_pre.as_ref()
                                        .map(|p| p.total_samples as f32 / (p.sample_rate.max(1)) as f32)
                                        .unwrap_or(2.0);
                                    let (stacc_gain, stacc_decay) =
                                        scaled_release_params(note_ms, voice.midi_note, rel_secs);
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
                                    // Perspectief-gain overerven: de release van
                                    // een rear-laag klinkt op rear-niveau (anders
                                    // een niveausprong bij loslaten).
                                    rv.persp_slot = voice.persp_slot;
                                    if let Some(decay_ms) = stacc_decay {
                                        // Korte noot: staart actief afbouwen — de galm
                                        // in de opname is nog niet volledig opgebouwd.
                                        // Infade én afbouw combineren zoals
                                        // GOSoundFader: eerst complementair opkomen
                                        // met de uitfadende speelnoot (tot het
                                        // snijpunt), daarna uitsterven. Instant op
                                        // vol niveau starten gaf een niveaubult.
                                        crossfade_in_with_decay(
                                            &mut rv, level, fade_ms,
                                            decay_ms, sample_rate,
                                        );
                                    } else {
                                        // Échte crossfade: de release komt op terwijl
                                        // de speelnoot uitfadet, met DEZELFDE duur.
                                        // Startte de release instant op vol niveau
                                        // (het oude gedrag), dan telden beide even
                                        // samen op — een niveausprong van enkele dB
                                        // bij élk loslaten, hoorbaar als tik/bump
                                        // (gemeten op Friesach: +4 dB, |dx| ×4).
                                        crossfade_in(&mut rv, level, fade_ms, sample_rate);
                                    }
                                    release_spawn_scratch.push(rv);
                                }
                                voice.release_ms(fade_ms, sample_rate);
                            }
                        }
                        // Werkbudget: één eenheid voor het bericht zelf (élke
                        // NoteOff doet de O(stemmen)-scan hierboven onder de
                        // write-lock, ook als er niets los te laten valt) plus
                        // één per daadwerkelijk gespawnde release-stem — dat is
                        // het dure deel (sample-lookups, fase-uitlijning,
                        // staartbudget). Zonder deze telling glipte een
                        // slotakkoord loslaten (NoteOff per register per toets,
                        // koppeldoelen incluis) ongelimiteerd door één callback
                        // heen: precies de piek die als haperen te horen was.
                        // Het bericht is hier al volledig afgehandeld — we
                        // splitsen dus niets; de rest van de wachtrij schuift
                        // gewoon FIFO door naar de volgende callback.
                        spawns_done += 1 + release_spawn_scratch.len();
                        for rv in release_spawn_scratch.drain(..) {
                            budget_release_voices(&mut voices_lock, sample_rate, live_cap);
                            push_voice_capped(&mut voices_lock, rv, sample_rate, live_cap, &mut gc);
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
                            // Toetsduur-release kiezen — zelfde model als NoteOff.
                            let note_ms = voice.age_samples as f32 * 1000.0 / (sample_rate.max(1)) as f32;
                            let vpipe = voice.pipe_num;
                            let pick = |vp: u32| -> u32 {
                                release_meta.get(&(stop_id, vp))
                                    .and_then(|list| list.iter()
                                        .find(|(max_ms, _)| *max_ms >= 0 && note_ms as i64 <= *max_ms as i64)
                                        .or_else(|| list.last())
                                        .map(|(_, key)| *key))
                                    .unwrap_or(vp | RELEASE_PIPE_FLAG)
                            };
                            let mut release_key = (stop_id, pick(vpipe));
                            let mut fade_ms = release_fade_ms(voice.midi_note);
                            // Eén release per pijp+laag; de TREM-vlag telt hier niet
                            // mee zodat een droge en een trem-voice op dezelfde pijp
                            // samen één staart geven.
                            if !spawned.contains(&(vpipe & !TREM_FLAG)) {
                                let mut rel_full = samples_clone.read().get(&release_key).cloned();
                                let mut rel_pre = preloads_clone.read().get(&release_key).cloned();
                                if rel_full.is_none() && rel_pre.is_none() && vpipe & TREM_FLAG != 0 {
                                    // Geen trem-release → droge release (zie NoteOff).
                                    release_key = (stop_id, pick(vpipe & !TREM_FLAG));
                                    rel_full = samples_clone.read().get(&release_key).cloned();
                                    rel_pre = preloads_clone.read().get(&release_key).cloned();
                                }
                                if let Some(ms) = release_xfade.get(&release_key) {
                                    fade_ms = *ms as f32;
                                }
                                if rel_full.is_some() || rel_pre.is_some() {
                                    // Staccato-schaal op ELKE gekozen release
                                    // (ook MaxKeyPressTime-varianten) + fase-
                                    // uitlijning — zie NoteOff.
                                    let rel_secs = rel_pre.as_ref()
                                        .map(|p| p.total_samples as f32 / (p.sample_rate.max(1)) as f32)
                                        .unwrap_or(2.0);
                                    let (stacc_gain, stacc_decay) =
                                        scaled_release_params(note_ms, voice.midi_note, rel_secs);
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
                                    // Perspectief-gain overerven (zie NoteOff).
                                    rv.persp_slot = voice.persp_slot;
                                    if let Some(decay_ms) = stacc_decay {
                                        // Infade + afbouw gecombineerd — zie NoteOff.
                                        crossfade_in_with_decay(
                                            &mut rv, level, fade_ms,
                                            decay_ms, sample_rate,
                                        );
                                    } else {
                                        // Zie NoteOff: complementaire crossfade i.p.v.
                                        // instant vol niveau (anders een niveausprong).
                                        crossfade_in(&mut rv, level, fade_ms, sample_rate);
                                    }
                                    spawns.push(rv);
                                    spawned.insert(vpipe & !TREM_FLAG);
                                }
                            }
                            voice.release_ms(fade_ms, sample_rate);
                        }
                        // ATOMAIR houden: een half uitgevoerde ReleaseStop zou bij
                        // pedaaljitter rond een trapgrens (hysterese H=2) door de
                        // direct volgende NoteOn van hetzelfde register worden
                        // ingehaald — de nieuwe stem raakt dan in de lopende
                        // release verstrikt en de noot valt dood. Wel telt het
                        // gedane werk mee, zodat de rest van de queue naar de
                        // volgende callback doorschuift.
                        //
                        // Boeking net als bij NoteOff: één eenheid voor het
                        // bericht ZELF plus één per gespawnde staart. De
                        // verzendkant stuurt ReleaseStop onvoorwaardelijk bij
                        // elke registerwissel, dus een ReleaseStop die niets
                        // loslaat is de regel — en die doet wél de volledige
                        // O(stemmen)-scan hierboven onder de write-lock. Op 0
                        // boeken liet een crescendo-trapwissel (tientallen
                        // registers ineens) gratis door de callback glippen.
                        spawns_done += 1 + spawns.len();
                        for rv in spawns {
                            budget_release_voices(&mut voices_lock, sample_rate, live_cap);
                            push_voice_capped(&mut voices_lock, rv, sample_rate, live_cap, &mut gc);
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
                    AudioCommand::RegisterOdfRetune(map) => {
                        if !map.is_empty() {
                            info!("Hertemper-tabel geregistreerd voor {} pijpen", map.len());
                        }
                        *odf_retune_clone.write() = map;
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
                            gc.send_boxed(old);
                            samples_bytes = 0;
                            sample_fifo.clear();
                            for (k, v) in new_samples.iter() {
                                samples_bytes += v.bytes();
                                sample_fifo.push_back(*k);
                                samples_lock.insert(*k, v.clone());
                            }
                        }
                        {
                            let mut preloads_lock = preloads_clone.write();
                            let old = std::mem::take(&mut *preloads_lock);
                            gc.send_boxed(old);
                        }
                        {
                            let mut voices_lock = voices_clone.write();
                            let old = std::mem::take(&mut *voices_lock);
                            gc.send_boxed(old);
                        }
                        // Nieuwe laad-generatie: uitstaande achtergrond-loads van
                        // het vorige orgel mogen hun (hergebruikte) keys niet vullen.
                        load_generation += 1;
                        inflight_loads.clear();
                        // Vooraf reserveren: insert tijdens het spelen (NoteOn →
                        // inflight_loads, achtergrond-load → samples) mag in de
                        // callback geen rehash-allocatie veroorzaken.
                        inflight_loads.reserve(new_samples.len().min(8192));
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
                            gc.send_boxed(old);
                            // Ruimte voor de volledige WAV's die straks per
                            // achtergrond-load IN de callback worden ingevoegd.
                            samples_lock.reserve(buffers.len().min(8192));
                            samples_bytes = 0;
                            sample_fifo.clear();
                        }
                        {
                            let mut preloads_lock = preloads_clone.write();
                            let old = std::mem::take(&mut *preloads_lock);
                            gc.send_boxed(old);
                            preloads_lock.reserve(buffers.len());
                            for (k, v) in buffers.iter() {
                                preloads_lock.insert(*k, v.clone());
                            }
                        }
                        {
                            let mut voices_lock = voices_clone.write();
                            let old = std::mem::take(&mut *voices_lock);
                            gc.send_boxed(old);
                        }
                        // Nieuwe laad-generatie (zie LoadSamples).
                        load_generation += 1;
                        inflight_loads.clear();
                        // Zie LoadSamples: reserveren zodat de callback niet rehasht.
                        inflight_loads.reserve(buffers.len().min(8192));
                        missing_sample_warned.clear();
                        // Vers orgel = verse limiter-staat (geen ingehouden gain
                        // van een luide passage op het vorige orgel).
                        master_limiter.reset();
                        info!("Registered {} preload buffers for instant playback", buffers.len());
                    }
                    AudioCommand::RegisterTremStops(set) => {
                        if !set.is_empty() {
                            info!("Registers met echte tremulant-opnamen: {}", set.len());
                        }
                        *stops_with_trem_clone.write() = set;
                    }
                    AudioCommand::RegisterReleaseCrossfade(map) => {
                        if !map.is_empty() {
                            info!("ReleaseCrossfadeLength uit de sampleset: {} release-keys", map.len());
                        }
                        release_xfade = map;
                    }
                    AudioCommand::RegisterFullSample { key, sample } => {
                        let size = sample.bytes();
                        if let Some(old) = samples_clone.write().insert(key, sample) {
                            samples_bytes = samples_bytes
                                .saturating_sub(old.bytes());
                            // Vervangen sample niet inline droppen (janitor-check).
                            gc.dispose_sample(old);
                        } else {
                            sample_fifo.push_back(key);
                        }
                        samples_bytes += size;
                    }
                    AudioCommand::SetTremulant { stop_ids, active } => {
                        // Crossfade: for currently playing voices on these stops,
                        // fade out old source and start new voice from alternate buffer
                        let mut new_voices: Vec<PlayingVoice> = Vec::new();
                        // Aantal daadwerkelijk gestarte crossfade-stemmen; gaat
                        // na afloop als werkeenheden op `spawns_done`.
                        let spawned_count;
                        {
                            let mut voices_lock = voices_clone.write();
                            for voice in voices_lock.iter_mut() {
                                if stop_ids.contains(&voice.stop_id) && !voice.releasing && !voice.one_shot {
                                    // Andere stand van DEZELFDE pijp: aan → TREM-vlag
                                    // erbij, uit → eraf. Staat de voice al in de
                                    // gevraagde stand, dan is er niets te doen.
                                    let alt_pipe = if active {
                                        voice.pipe_num | TREM_FLAG
                                    } else {
                                        voice.pipe_num & !TREM_FLAG
                                    };
                                    if alt_pipe == voice.pipe_num {
                                        continue;
                                    }
                                    let key = (voice.stop_id, alt_pipe);
                                    let alt_buffer = preloads_clone.read().get(&key).cloned();
                                    if let Some(alt_buf) = alt_buffer {
                                        // Niveau van de klinkende stem BEWAREN vóór
                                        // de uitfade: crossfade_release() zet
                                        // target_envelope op 0, en dat veld ging
                                        // daarna als startniveau naar de nieuwe stem
                                        // — die fadete dus naar stilte in plaats van
                                        // naar het niveau van de oude (tremulant
                                        // omschakelen tijdens het klinken maakte de
                                        // noot stil; bestond al vóór 0.7.39, maar was
                                        // pas meetbaar met echte tremulant-opnamen).
                                        let level = voice.target_envelope.max(voice.envelope);
                                        // Startpositie in het domein van de DOEL-
                                        // buffer: de positie van de klinkende stem
                                        // staat in de coördinaten van zijn eigen
                                        // bron (volledige sample op uitgangsrate,
                                        // óf preload op bronrate mét trim). Ruw
                                        // doorgeven liet de nieuwe stem op een
                                        // willekeurige plek of voorbij het einde
                                        // landen — zie crossfade_position_in.
                                        let start_pos =
                                            voice.crossfade_position_in(&alt_buf, sample_rate);
                                        // Fade out current voice slowly
                                        voice.crossfade_release();
                                        // Create new voice from alternate buffer at same position.
                                        // De nieuwe voice draagt de ALT-sleutel: zijn
                                        // achtergrond-load landt daardoor onder de
                                        // juiste (trem- of droge) key.
                                        let mut nv = PlayingVoice::new_crossfade(
                                            alt_buf, voice.stop_id, alt_pipe, voice.midi_note,
                                            level, start_pos, sample_rate
                                        );
                                        // Perspectief-gain overerven: de trem-laag van
                                        // een rear-perspectief blijft op rear-niveau.
                                        nv.persp_slot = voice.persp_slot;
                                        // Refcount meenemen: houdt een koppel de
                                        // pijp ook vast, dan mag de eerste NoteOff
                                        // hem niet al loslaten.
                                        nv.note_on_count = voice.note_on_count;
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
                            // Nieuwe crossfade-stemmen erbij, mét staartbudget —
                            // net als bij NoteOff en ReleaseStop. Juist hier kan
                            // het stemmenaantal in één klap VERDUBBELEN (elke
                            // klinkende pijp houdt zijn uitfadende oude stem én
                            // krijgt een nieuwe uit de andere buffer), dus dit
                            // was de laatste plek waar de kap ongeremd overschreden
                            // kon worden: een tremulant aanzetten op vol werk
                            // duwde de staarten voorbij hun budget.
                            spawned_count = push_voices_budgeted(
                                &mut voices_lock, new_voices, sample_rate, live_cap, &mut gc,
                            );
                        }
                        // Update tremulant state for future notes. De guard in een
                        // eigen blok: hij mag NIET openstaan tijdens het loggen
                        // (zie hieronder) — elke NoteOn leest deze map.
                        {
                            let mut trem = trem_active_clone.write();
                            for id in stop_ids {
                                if active {
                                    trem.insert(id);
                                } else {
                                    trem.remove(&id);
                                }
                            }
                            TREMULANT_ACTIVE_STOPS.store(trem.len() as u64, Ordering::Relaxed);
                        }
                        // Geen info!() in dit hete pad: de logschrijver is
                        // ongebufferd en zit achter een mutex, dus één regel kan
                        // de realtime-deadline opeten. Alleen tellen; de
                        // watchdog-thread meldt de stand (zelfde patroon als
                        // RELEASE_PRELOAD_EDGE_HITS).
                        TREMULANT_SWITCHES.fetch_add(1, Ordering::Relaxed);
                        // Werkbudget: één eenheid voor het bericht zelf (de
                        // O(stemmen)-scan hierboven onder de write-lock gebeurt
                        // ook als er niets te wisselen valt) plus één per gestarte
                        // crossfade-stem. Dit was het ENIGE stemstart-pad in de
                        // commandolus zonder boeking: een tremulant-wissel midden
                        // in een tutti startte ongelimiteerd stemmen binnen één
                        // callback. Het bericht blijft ATOMAIR — half uitgevoerd
                        // zou een deel van de pijpen in de verkeerde stand
                        // achterlaten; we boeken alleen het gedane werk, zodat de
                        // rest van de wachtrij FIFO naar de volgende callback
                        // doorschuift.
                        spawns_done += 1 + spawned_count;
                    }
                    AudioCommand::SetDivisionGain { division_index, gain } => {
                        // Alleen de DOELstand; de render-lus loopt er per blok
                        // gesmootheerd naartoe en leidt volume + cutoff daaruit af.
                        let mut gains = division_gains_clone.write();
                        if (division_index as usize) < gains.len() {
                            gains[division_index as usize] = gain.clamp(0.0, 1.0);
                        }
                    }
                    AudioCommand::SetSwellConfig { division_index, min_db, filter_cutoff_closed } => {
                        let mut configs = swell_configs_clone.write();
                        if (division_index as usize) < configs.len() {
                            configs[division_index as usize] = (min_db, filter_cutoff_closed);
                            // Volume + cutoff bij de huidige stand herberekenen.
                            swell_dirty[division_index as usize] = true;
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
                        // Reset all division gains to 1.0 (fully open) — inclusief
                        // de gesmoothede stand en de zwelfilters (die bleven anders
                        // op de cutoff van het vórige orgel staan: doffe divisie).
                        *division_gains_clone.write() = vec![1.0; 32];
                        swell_g = [1.0; 32];
                        swell_vol = [1.0; 32];
                        swell_bypass = [true; 32];
                        swell_dirty = [false; 32];
                        for (fl, fr) in swell_filters_clone.write().iter_mut() {
                            fl.set_cutoff(20000.0, sample_rate); fl.reset();
                            fr.set_cutoff(20000.0, sample_rate); fr.reset();
                        }
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
                    AudioCommand::SetTemperament { note_offsets, fine_tune, retune } => {
                        let mut ratios = [0.0f64; 12];
                        for i in 0..12 {
                            let total_cents = note_offsets[i] as f64 + fine_tune as f64;
                            ratios[i] = 2.0_f64.powf(total_cents / 1200.0);
                        }
                        *temperament_clone.write() = (ratios, retune);
                        info!("Temperament set: fine_tune={} cents, retune={}", fine_tune, retune);
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
                            gc.send_boxed(old);
                            samples_bytes = 0;
                            sample_fifo.clear();
                        }
                        {
                            let old = std::mem::take(&mut *preloads_clone.write());
                            gc.send_boxed(old);
                        }
                        trem_active_clone.write().clear();
                        stops_with_trem_clone.write().clear();
                        percussive_stops_clone.write().clear();
                        *odf_voicing_clone.write() = Arc::new(HashMap::new());
                        *odf_retune_clone.write() = Arc::new(HashMap::new());
                        {
                            let old = std::mem::take(&mut *voices_clone.write());
                            gc.send_boxed(old);
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
                        swell_g = [1.0; 32];
                        swell_vol = [1.0; 32];
                        swell_bypass = [true; 32];
                        swell_dirty = [false; 32];
                        for (fl, fr) in swell_filters_clone.write().iter_mut() {
                            fl.set_cutoff(20000.0, sample_rate); fl.reset();
                            fr.set_cutoff(20000.0, sample_rate); fr.reset();
                        }
                        *temperament_clone.write() = ([1.0; 12], false);
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
            // callback zes Vecs te klonen (incl. de geneste Vec<Vec<u16>> van de
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
            // Galm-gewichten per kanaal: de galm moet over DEZELFDE kanalen
            // lopen als het droge signaal (testorgel-feedback: op het
            // hoofdtelefoonprofiel [0,1] klonk de galm door op de speaker-
            // kanalen [2..7]). Eén keer per callback afgeleid uit de effectieve
            // routing van de ÉCHTE divisies (aantal uit de stop→divisie-map);
            // vaste arrays, dus geen allocatie op de audio-thread.
            let n_real_divs = stop_div_map.values().map(|&d| d as usize + 1).max().unwrap_or(0);
            let (wet_w_l, wet_w_r, wet_ch) = wet_channel_weights(&out_chans_lock, n_real_divs, channels);
            let (temp_ratios, retune_on) = *temperament_clone.read();
            let mut peak_l = 0.0f32;
            let mut peak_r = 0.0f32;

            {
                let mut voices_lock = voices_clone.write();
                let mut swell_flt = swell_filters_clone.write();
                let pipe_voicing_map = voicing_clone.read();
                let odf_voicing_map = odf_voicing_clone.read();
                let odf_retune_map = odf_retune_clone.read();
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

                    // Gebruikers-voicing (VoicingPanel) keyt op de KALE pijp en
                    // geldt dus voor álle lagen van die toets; ODF-intonatie en
                    // hertemper-tabel staan per laag (laagsleutel zonder
                    // release-bits).
                    let ukey = (voice.stop_id, base_pipe(voice.pipe_num));
                    let okey = (voice.stop_id, voice.pipe_num & !RELEASE_KEY_MASK);
                    let (user_vol, user_pitch) = pipe_voicing_map.get(&ukey).copied().unwrap_or((0.0, 0.0));
                    let (odf_vol, odf_pitch) = odf_voicing_map.get(&okey).copied().unwrap_or((0.0, 0.0));
                    // Hertemper-modus: de gemeten-toonhoogte-correctie van deze
                    // pijp VERVANGT de ODF-PitchTuning; zonder meting blijft
                    // PitchTuning staan. Eén HashMap-lookup per voice per
                    // callback (zelfde orde als de bestaande lookups); okey
                    // maskeert de release-vlag, dus release- en tremulantlaag-
                    // voices volgen dezelfde correctie.
                    let odf_pitch_eff = if retune_on {
                        odf_retune_map.get(&okey).copied().unwrap_or(odf_pitch)
                    } else {
                        odf_pitch
                    };
                    let voicing_vol = user_vol + odf_vol;
                    let voicing_pitch = user_pitch + odf_pitch_eff;
                    voice.c_voicing_gain = if voicing_vol.abs() > 1e-6 {
                        10.0_f32.powf(voicing_vol / 20.0)
                    } else {
                        1.0
                    };
                    // Perspectief-volume (live, per slot): één array-read + mul.
                    voice.c_voicing_gain *= persp_gain[(voice.persp_slot as usize) & (MAX_LAYERS - 1)];

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

                // ── Mengloop, stem-major in blokken (0.7.36) ─────────────────────
                // Voorheen frame-major: per frame langs álle stemmen. Bij honderden
                // stemmen raakt elke frame honderden verschillende samplebuffers →
                // cache-/TLB-thrash: de rendertijd groeide véél sneller dan lineair
                // (45 stemmen 7%, 900 stemmen 500% van de buffertijd). Nu per blok
                // van MIX_BLOCK frames: (1) wind/trem per frame vooruit rekenen,
                // (2) elke stem zijn hele blok sequentieel laten lezen, (3) daarna
                // per frame de bestaande keten (zwel → routing → galm → EQ →
                // limiter). Optelvolgorde per (frame, divisie) is ongewijzigd, dus
                // het resultaat is identiek — alleen het geheugenpatroon verschilt.
                let n_frames = data.len() / channels.max(1);
                // Hoeveel stukken voor deze callback? Onder de drempel blijft
                // alles op één stuk: de reductie kost dan meer dan het
                // verdelen oplevert. Eén keer per callback bepaald, zodat het
                // aantal binnen de callback niet kan wisselen.
                let stukken = {
                    let n = voices_lock.len();
                    if n < MENG_DREMPEL_STEMMEN { 1 } else { meng_stukken().min(n) }
                };
                // Tijdmeting per pas (fase 1). Opgeteld over alle blokken van
                // deze callback; aan het eind omgerekend naar promille van de
                // buffertijd.
                let mut ns_pass = [0u128; 4];
                let mut fi = 0usize;
                while fi < n_frames {
                    let bn = (n_frames - fi).min(MIX_BLOCK);
                    let t_pas = std::time::Instant::now();

                    // ── Zwelkast per blok: doelstand → gesmoothede stand → volume + filter ──
                    // Eén exp per blok + per gewijzigde divisie één powf en één
                    // set_cutoff (exp); niets hiervan meer in de per-frame-lus.
                    {
                        let alpha = swell_dsp::block_alpha(bn, sample_rate, swell_dsp::TAU_S);
                        for idx in 0..32 {
                            let target = div_gains.get(idx).copied().unwrap_or(1.0);
                            let g_prev = swell_g[idx];
                            let moved = g_prev != target;
                            if moved {
                                swell_g[idx] = swell_dsp::smooth_step(g_prev, target, alpha);
                            }
                            if moved || swell_dirty[idx] {
                                swell_dirty[idx] = false;
                                let g = swell_g[idx];
                                let (min_db, cutoff_closed) = swell_cfgs.get(idx).copied().unwrap_or((-20.0, 800.0));
                                swell_vol[idx] = swell_dsp::volume(min_db, g);
                                let bypass = g >= swell_dsp::BYPASS_ABOVE;
                                swell_bypass[idx] = bypass;
                                if !bypass {
                                    if let Some((fl, fr)) = swell_flt.get_mut(idx) {
                                        let c = swell_dsp::cutoff_hz(cutoff_closed, g);
                                        fl.set_cutoff(c, sample_rate);
                                        fr.set_cutoff(c, sample_rate);
                                    }
                                }
                            }
                        }
                    }

                    // ── Pass 1: wind/tremulant per frame (modellen in volgorde stappen) ──
                    for f in 0..bn {
                        let mut group_pressures = [1.0f32; 32];
                        for (i, model) in wm.iter_mut().enumerate() {
                            group_pressures[i] = model.process();
                        }
                        let mut wind_pressures = [1.0f32; 32];
                        for div_idx in 0..32 {
                            let grp = wind_group_assignment.get(div_idx).copied().unwrap_or(div_idx as u8) as usize;
                            if grp < 32 {
                                wind_pressures[div_idx] = group_pressures[grp];
                            }
                        }
                        let mut trem_mods = [(1.0f32, 0.0f32); 32]; // (amp_mod, pitch_cents)
                        for (i, trem) in trem_lfo.iter_mut().enumerate() {
                            trem_mods[i] = trem.process();
                        }
                        // Dure powf/sqrt één keer per divisie per frame (max 32), niet
                        // per stem; in rust (geen wind/LFO) blijft alles 1.0.
                        let wr = &mut blk_wind_rate[f];
                        let wg = &mut blk_wind_gain[f];
                        let tr = &mut blk_trem_rate[f];
                        let ta = &mut blk_trem_amp[f];
                        for d in 0..32 {
                            let wp = wind_pressures[d];
                            if (wp - 1.0).abs() > 1e-6 {
                                wr[d] = 2.0_f64.powf(((wp - 1.0) * 30.0) as f64 / 1200.0);
                                wg[d] = wp.sqrt();
                            } else {
                                wr[d] = 1.0;
                                wg[d] = 1.0;
                            }
                            let tp = trem_mods[d].1;
                            tr[d] = if tp.abs() > 0.01 { 2.0_f64.powf(tp as f64 / 1200.0) } else { 1.0 };
                            ta[d] = trem_mods[d].0;
                        }
                        blk_div_l[f] = [0.0f32; 32];
                        blk_div_r[f] = [0.0f32; 32];
                    }
                    // Deel-opteltabellen van de stukken 1.. wissen. Alleen de
                    // stukken die deze callback echt gebruikt worden.
                    for k in 0..stukken.saturating_sub(1) {
                        for f in 0..bn {
                            blk_part_l[k][f] = [0.0f32; 32];
                            blk_part_r[k][f] = [0.0f32; 32];
                        }
                    }
                    ns_pass[0] += t_pas.elapsed().as_nanos();

                    // ── Pass 2: stemmen, elk zijn hele blok ──
                    // Verdeeld over `stukken` aaneengesloten stukken, elk met
                    // een eigen opteltabel. Bij één stuk is dit letterlijk de
                    // oude lus over alle stemmen met blk_div_l/r als doel.
                    let t_pas = std::time::Instant::now();
                    let per_stuk = voices_lock.len().div_ceil(stukken.max(1));
                    for (stuk, deel) in voices_lock.chunks_mut(per_stuk.max(1)).enumerate() {
                    // Stuk 0 schrijft rechtstreeks in de hoofdtabel; de rest in
                    // zijn eigen deeltabel.
                    let (doel_l, doel_r): (&mut Vec<[f32; 32]>, &mut Vec<[f32; 32]>) = if stuk == 0 {
                        (&mut blk_div_l, &mut blk_div_r)
                    } else {
                        (&mut blk_part_l[stuk - 1], &mut blk_part_r[stuk - 1])
                    };
                    for voice in deel.iter_mut() {
                        let div_idx = voice.c_div;
                        let in_range = div_idx < 32;
                        // Effectieve afspeel-rate = basis (samplerate-correctie) ×
                        // gecachte temperament+voicing-pitch × per-sample wind/trem.
                        // next_sample() leest self.rate; we zetten hem per frame en
                        // herstellen daarna de basis (anders compoundeert de detune).
                        let base_rate = voice.rate;
                        let pitch_mul = voice.c_pitch_mul;
                        let use_lfo = voice.c_use_lfo_trem;
                        let vgain = voice.c_voicing_gain;
                        // Panning: per stem één keer per blok. Standaard de per-callback
                        // voorgerekende constant-power cos/sin van de divisie; met C/Cis-
                        // lade-spreiding wijkt de pan per noot af (even MIDI-noten naar
                        // de ene kant, oneven naar de andere, sterkst bij de laagste
                        // pijpen) — noot en instellingen zijn constant binnen de callback.
                        let (pan_cos, pan_sin) = if in_range && ccis_on
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
                        } else if in_range {
                            (div_pan_cos[div_idx], div_pan_sin[div_idx])
                        } else {
                            (0.0, 0.0)
                        };
                        for f in 0..bn {
                            let mut eff_rate = base_rate * pitch_mul;
                            if in_range {
                                eff_rate *= blk_wind_rate[f][div_idx];
                                if use_lfo {
                                    eff_rate *= blk_trem_rate[f][div_idx];
                                }
                            }
                            voice.rate = eff_rate;
                            let (sl, sr) = voice.next_sample();
                            voice.rate = base_rate;

                            // Volume = gecachte voicing-gain × per-sample wind/trem-amplitude.
                            let mut g = vgain;
                            if in_range {
                                g *= blk_wind_gain[f][div_idx];
                                if use_lfo {
                                    g *= blk_trem_amp[f][div_idx];
                                }
                            }
                            // Mono: sl == sr → exact het oude pad. Stereo: L en R
                            // gescheiden, met dezelfde pan-gewichten (pan werkt dan als
                            // balans; op pan 0 blijft het niveau gelijk aan mono-sets).
                            let contribution_l = sl * g;
                            let contribution_r = sr * g;
                            if !in_range { continue; }
                            doel_l[f][div_idx] += contribution_l * pan_cos;
                            doel_r[f][div_idx] += contribution_r * pan_sin;
                        }
                    }
                    }
                    ns_pass[1] += t_pas.elapsed().as_nanos();

                    // ── Pass 4: de deeltabellen optellen ──
                    // In VASTE stukvolgorde, zodat het resultaat bij een gelijk
                    // aantal stukken reproduceerbaar is. Bij één stuk gebeurt
                    // hier niets. Alleen de divisies die dit orgel heeft.
                    let t_pas = std::time::Instant::now();
                    reduceer_deeltabellen(
                        &mut blk_div_l, &mut blk_div_r,
                        &blk_part_l, &blk_part_r,
                        stukken, n_real_divs.clamp(1, 32), bn,
                    );
                    ns_pass[3] += t_pas.elapsed().as_nanos();

                    // ── Pass 3: per frame de bestaande keten ──
                    let t_pas = std::time::Instant::now();
                    for f in 0..bn {
                        let frame = &mut data[(fi + f) * channels..(fi + f + 1) * channels];
                        let div_l = &blk_div_l[f];
                        let div_r = &blk_div_r[f];
                        // Reset frame
                        for s in frame.iter_mut() { *s = 0.0; }

                        // Per divisie: zwelkast (stereo low-pass + gain) + master gain, dan routen
                        // naar de toegewezen fysieke kanalen. Som ook tot mono voor de galm.
                        let mut sum_l = 0.0f32;
                        let mut sum_r = 0.0f32;
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
                            // Zwelkast: volume + cutoff zijn per blok voorgerekend.
                            // Kast (vrijwel) open → filter overslaan, maar wel
                            // 'primen' zodat het zonder sprong invalt zodra de
                            // trede weer sluit.
                            let (lf, rf) = match swell_flt.get_mut(idx) {
                                Some((fl, fr)) if swell_bypass[idx] => {
                                    fl.prime(l_raw);
                                    fr.prime(r_raw);
                                    (l_raw, r_raw)
                                }
                                Some((fl, fr)) => (fl.process(l_raw), fr.process(r_raw)),
                                None => (l_raw, r_raw),
                            };
                            let vol = swell_vol[idx];
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
                                            if lc < channels { frame[lc] += l; routed = true; }
                                            if rc < channels { frame[rc] += r; routed = true; }
                                            i += 2;
                                        } else {
                                            if lc < channels { frame[lc] += (l + r) * 0.5; routed = true; }
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
                                        } else if channels >= 1 {
                                            frame[0] += (l + r) * 0.5;
                                        }
                                    }
                                }
                                _ => {
                                    if channels >= 2 {
                                        frame[0] += l; frame[1] += r;
                                    } else if channels >= 1 {
                                        frame[0] += (l + r) * 0.5;
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

                        // Galm over dezelfde kanalen als het droge signaal, gewogen
                        // naar het aandeel dat elke luidspreker droog krijgt (zie
                        // wet_channel_weights). De oude vaste routering (altijd 0/1
                        // + center-fill op kanaal 2) stuurde op het testorgel de galm
                        // naar de hoofdtelefoon terwijl de speakers droog speelden —
                        // en andersom lekte hij naar de speakers bij spelen op de
                        // hoofdtelefoon.
                        if wet_l != 0.0 || wet_r != 0.0 {
                            for c in 0..wet_ch {
                                let w_l = wet_w_l[c];
                                let w_r = wet_w_r[c];
                                if w_l != 0.0 { frame[c] += wet_l * w_l; }
                                if w_r != 0.0 { frame[c] += wet_r * w_r; }
                            }
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
                    ns_pass[2] += t_pas.elapsed().as_nanos();
                    fi += bn;
                }
                // Tijdmeting per pas wegschrijven (fase 1): promille van de
                // buffertijd, zelfde EMA-venster (16 callbacks) als de totale
                // belastingsmeter zodat de getallen vergelijkbaar zijn.
                if frames_now > 0 && sample_rate > 0 {
                    let budget_ns = frames_now as f64 * 1_000_000_000.0 / sample_rate as f64;
                    for i in 0..4 {
                        let pm = ((ns_pass[i] as f64 / budget_ns) * 10_000.0).clamp(0.0, 99_999.0) as u32;
                        let prev = RENDER_PASS_PM[i].load(Ordering::Relaxed);
                        RENDER_PASS_PM[i].store((prev * 15 + pm) / 16, Ordering::Relaxed);
                    }
                }
                // Push naar recorder (try_send — audio mag niet blokkeren).
                if let Some(rc) = rec_active {
                    if !rec_buf.is_empty() { rc.push_stereo(&rec_buf); }
                }

                // Uitgeklonken stemmen verwijderen. Niet met een kale `retain`:
                // die liet de laatste Arc van een (boven de RAM-kap geëvicte)
                // sample ín de callback vallen — 1-3 MB per stem terug naar het
                // systeem, bij een akkoordloslating 0,5-3 ms. De bron gaat nu
                // langs de janitor-check.
                remove_finished_voices(&mut voices_lock, &mut gc);

                // Update voice count
                voice_count_clone.store(voices_lock.len(), Ordering::Relaxed);
            }

            // Luidspreker-testsignaal: overschrijft de uitgang volledig, zodat
            // er gegarandeerd uit precies één kanaal geluid komt. Bewust hier en
            // niet met een vroege return: de commandowachtrij en de stemmen
            // worden gewoon afgehandeld, zodat aan- en uitzetten geen
            // opgespaarde berichten achterlaat. Tijdens het uitzoeken van
            // luidsprekers speelt er toch niets.
            if let Some((tch, kind, gain)) = test_signal() {
                let tch = tch as usize;
                peak_l = 0.0;
                peak_r = 0.0;
                for frame in data.chunks_mut(channels.max(1)) {
                    let v = testsignaal.sample(kind, sample_rate) * gain;
                    for (ci, s) in frame.iter_mut().enumerate() {
                        *s = if ci == tch { v.clamp(-1.0, 1.0) } else { 0.0 };
                        // De niveaumeter moet het testsignaal tonen; anders lijkt
                        // het alsof er niets gebeurt terwijl de ruis loopt.
                        if ci == 0 { peak_l = peak_l.max(s.abs()); }
                        else if ci == 1 { peak_r = peak_r.max(s.abs()); }
                    }
                }
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

            // Belastingsmeter: rendertijd t.o.v. de buffertijd van deze callback.
            // (frames_now is bovenaan de callback bepaald — zie werkbudget.)
            if frames_now > 0 && sample_rate > 0 {
                let budget = frames_now as f64 / sample_rate as f64;
                let load = render_t0.elapsed().as_secs_f64() / budget;
                let pm = (load * 1000.0).clamp(0.0, 9999.0) as u32;
                let prev = RENDER_LOAD_PM.load(Ordering::Relaxed);
                // EMA met alpha 1/16: het venster is 16 CALLBACKS, niet een vaste
                // tijd — bij 32 frames ≈ 11 ms, bij 512 frames ≈ 170 ms. Een
                // getoond percentage vlak na een zware callback is dus een
                // momentopname, geen volgehouden gemiddelde. Piek met traag verval.
                RENDER_LOAD_PM.store((prev * 15 + pm) / 16, Ordering::Relaxed);
                let peak_prev = RENDER_PEAK_PM.load(Ordering::Relaxed);
                RENDER_PEAK_PM.store(pm.max(peak_prev.saturating_sub(peak_prev / 200 + 1)), Ordering::Relaxed);
                if pm > 800 {
                    overload_callbacks += 1;
                    RENDER_OVERLOAD_COUNT.store(overload_callbacks, Ordering::Relaxed);
                    if last_overload_warn.map_or(true, |t: std::time::Instant| t.elapsed().as_secs() >= 5) {
                        // Concreet advies: de buffergrootte is de knop die het
                        // vaakst helpt (128 frames geeft 4x meer tijd per callback
                        // dan 32 en is voor een huisorgel ruim snel genoeg).
                        warn!("Audio-render zwaar belast: {}% van de buffertijd bij {} frames ({:.2} ms), {} stemmen (kap {}); {} zware callbacks sinds start — zet de buffergrootte op ten minste 128 frames, verlaag de polyfonie of zet de galm uit",
                            pm / 10, frames_now, frames_now as f64 * 1000.0 / sample_rate as f64,
                            voice_count_clone.load(Ordering::Relaxed), live_cap, overload_callbacks);
                        last_overload_warn = Some(std::time::Instant::now());
                    }
                }
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
    // Laatst gemelde stand van de preload-rand-teller (release-staarten die op
    // de preload afliepen omdat de volledige WAV nog niet geladen was).
    let mut preload_edge_prev = RELEASE_PRELOAD_EDGE_HITS.load(Ordering::Relaxed);
    // Idem voor de tremulantwissels: de callback telt, deze thread meldt.
    let mut trem_switch_prev = TREMULANT_SWITCHES.load(Ordering::Relaxed);
    while running.load(Ordering::Relaxed) {
        thread::sleep(std::time::Duration::from_millis(100));
        ticks += 1;

        // Orgel-load bezig: callback-stilte is dan verwacht (zware disk/CPU kan
        // ASIO4ALL laten haperen). Niet meten en zeker niet escaleren; alle
        // tellers schoon zodat er na de load met een verse lei gemeten wordt.
        // Dit stopt de watchdog-amplificatie die bij een orgelwissel "soms de
        // ASIO kwijt" raakte (herbouw-spiraal → volledige herstart → WASAPI).
        if watchdog_hold.load(Ordering::Relaxed) {
            rebuild_pending = false;
            rebuild_failures = 0;
            verify_since = None;
            error_suspect = None;
            was_flowing = false;
            prev_count = cb_count.load(Ordering::Relaxed);
            let _ = stream_error.swap(false, Ordering::Relaxed);
            continue;
        }

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

        // Laadlatentie-diagnose: hooguit 1x per 5 s, en alleen als er iets te
        // melden valt. De telling zelf gebeurt in de callback (één atomaire
        // optelling per afgekapte staart), het loggen hier.
        if ticks % 50 == 0 {
            let hits = RELEASE_PRELOAD_EDGE_HITS.load(Ordering::Relaxed);
            if hits > preload_edge_prev {
                info!("Release-staarten afgekapt op de preload-rand (volledige sample nog niet geladen): +{} (totaal {})",
                      hits - preload_edge_prev, hits);
                preload_edge_prev = hits;
            }
            // Tremulantwissels: idem — geteld in de callback, hier gemeld.
            let trem_hits = TREMULANT_SWITCHES.load(Ordering::Relaxed);
            if trem_hits > trem_switch_prev {
                info!("Tremulant gewisseld: +{} (nu {} registers met tremulant)",
                      trem_hits - trem_switch_prev,
                      TREMULANT_ACTIVE_STOPS.load(Ordering::Relaxed));
                trem_switch_prev = trem_hits;
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

#[cfg(test)]
mod veel_kanalen_tests {
    use super::{wet_channel_weights, MAX_OUT_CH};

    #[test]
    fn het_kanaalplafond_is_ruim_genoeg_voor_grote_interfaces() {
        assert!(MAX_OUT_CH >= 1024, "plafond te laag: {}", MAX_OUT_CH);
    }

    #[test]
    fn honderden_kanalen_wegen_zonder_paniek() {
        // Drie divisies, elk op een eigen paar hoog in een interface met 256
        // uitgangen. Vóór 0.7.48 viel alles boven kanaal 63 buiten de tabel.
        // Kanalen ver boven de oude u8-grens van 255, in een interface met 600
        // uitgangen: pas sinds 0.7.48 kán een divisie die aanwijzen.
        let chans = vec![vec![500u16, 501], vec![502, 503], vec![504, 505]];
        let (wl, wr, n) = wet_channel_weights(&chans, 3, 600);
        assert_eq!(n, 506, "hoogste gebruikte kanaal + 1");
        for (l, r) in [(500usize, 501usize), (502, 503), (504, 505)] {
            assert!((wl[l] - 1.0 / 3.0).abs() < 1e-6, "kanaal {} links: {}", l, wl[l]);
            assert!((wr[r] - 1.0 / 3.0).abs() < 1e-6, "kanaal {} rechts: {}", r, wr[r]);
        }
        // Ongebruikte kanalen krijgen niets.
        assert_eq!(wl[100], 0.0);
        assert_eq!(wr[255], 0.0);
        assert_eq!(wl[599], 0.0);
    }

    #[test]
    fn kanalen_boven_het_plafond_worden_geklemd() {
        // Een apparaat dat meer kanalen meldt dan de tabel aankan mag geen
        // index-paniek geven.
        let chans = vec![vec![0u16, 1]];
        let (_, _, n) = wet_channel_weights(&chans, 1, MAX_OUT_CH + 500);
        assert!(n <= MAX_OUT_CH);
    }
}

#[cfg(test)]
mod mengloop_stukken_tests {
    use super::{meng_stukken, reduceer_deeltabellen, set_meng_stukken,
                MAX_MENG_STUKKEN, MENG_DREMPEL_STEMMEN};

    /// Bouwt een deeltabel waarin elke waarde uniek is, zodat een
    /// vergeten stuk of divisie meteen opvalt.
    fn deel(k: usize, bn: usize) -> Vec<[f32; 32]> {
        (0..bn).map(|f| {
            let mut rij = [0.0f32; 32];
            for d in 0..32 { rij[d] = (k * 1000 + f * 10 + d) as f32; }
            rij
        }).collect()
    }

    #[test]
    fn een_stuk_verandert_niets() {
        let bn = 8;
        let mut l = deel(9, bn);
        let mut r = deel(9, bn);
        let voor_l = l.clone();
        let delen: Vec<Vec<[f32; 32]>> = Vec::new();
        reduceer_deeltabellen(&mut l, &mut r, &delen, &delen, 1, 4, bn);
        assert_eq!(l, voor_l, "bij één stuk hoort er niets opgeteld te worden");
    }

    #[test]
    fn alle_stukken_en_divisies_komen_erbij() {
        let bn = 4;
        let n_div = 6;
        let mut l = vec![[0.0f32; 32]; bn];
        let mut r = vec![[0.0f32; 32]; bn];
        let dl: Vec<Vec<[f32; 32]>> = (0..3).map(|k| deel(k + 1, bn)).collect();
        let dr: Vec<Vec<[f32; 32]>> = (0..3).map(|k| deel(k + 1, bn)).collect();
        reduceer_deeltabellen(&mut l, &mut r, &dl, &dr, 4, n_div, bn);
        for f in 0..bn {
            for d in 0..n_div {
                let verwacht: f32 = (0..3).map(|k| dl[k][f][d]).sum();
                assert_eq!(l[f][d], verwacht, "frame {} divisie {}", f, d);
                assert_eq!(r[f][d], verwacht, "frame {} divisie {} (rechts)", f, d);
            }
        }
    }

    #[test]
    fn divisies_boven_n_div_blijven_onaangeroerd() {
        // De tabellen zijn altijd 32 breed; alles boven het aantal divisies van
        // dit orgel is nul en hoeft niet aangeraakt — dat scheelt geheugenverkeer.
        let bn = 2;
        let mut l = vec![[0.0f32; 32]; bn];
        let mut r = vec![[0.0f32; 32]; bn];
        let dl: Vec<Vec<[f32; 32]>> = vec![deel(1, bn)];
        reduceer_deeltabellen(&mut l, &mut r, &dl, &dl, 2, 3, bn);
        for f in 0..bn {
            assert_ne!(l[f][2], 0.0, "divisie 2 hoort wel opgeteld te zijn");
            for d in 3..32 {
                assert_eq!(l[f][d], 0.0, "divisie {} hoort nul te blijven", d);
            }
        }
    }

    #[test]
    fn de_optelvolgorde_ligt_vast() {
        // Reproduceerbaarheid is de eis: twee keer dezelfde invoer moet
        // bit-identiek hetzelfde geven, ook bij waarden waar de volgorde
        // aantoonbaar uitmaakt (groot + klein + klein).
        let bn = 1;
        let maak = |v: f32| -> Vec<[f32; 32]> {
            let mut rij = [0.0f32; 32];
            rij[0] = v;
            vec![rij; bn]
        };
        let dl = vec![maak(1e-8), maak(1e-8), maak(1.0)];
        let draai = |dl: &Vec<Vec<[f32; 32]>>| {
            let mut l = vec![[0.0f32; 32]; bn];
            let mut r = vec![[0.0f32; 32]; bn];
            reduceer_deeltabellen(&mut l, &mut r, dl, dl, 4, 1, bn);
            l[0][0]
        };
        assert_eq!(draai(&dl), draai(&dl));
    }

    #[test]
    fn de_instelling_wordt_geklemd() {
        set_meng_stukken(0);
        assert_eq!(meng_stukken(), 1, "0 stukken bestaat niet");
        set_meng_stukken(9999);
        assert_eq!(meng_stukken(), MAX_MENG_STUKKEN);
        set_meng_stukken(1);
        assert_eq!(meng_stukken(), 1);
    }

    #[test]
    fn de_drempel_is_zinnig() {
        // Onder de drempel loont verdelen niet; hij moet wel ruim onder een
        // gewoon akkoord met een paar registers liggen.
        assert!(MENG_DREMPEL_STEMMEN >= 16 && MENG_DREMPEL_STEMMEN <= 256);
    }
}
