//! GrandOrgue ODF (Organ Definition File) parser
//!
//! Parses .organ files used by GrandOrgue sample sets.
//! Format is INI-like with sections for Organ, Manuals, Stops, Pipes, etc.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum OdfError {
    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parse error at line {line}: {message}")]
    ParseError { line: usize, message: String },

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid reference: {0}")]
    InvalidReference(String),

    #[error("Sample not found: {0}")]
    SampleNotFound(String),
}

/// Parsed GrandOrgue organ definition
#[derive(Debug, Clone)]
pub struct OrganDefinition {
    /// Base path for resolving relative sample paths
    pub base_path: PathBuf,
    /// General organ information
    pub organ: OrganInfo,
    /// Manual definitions (including pedal as Manual000)
    pub manuals: Vec<ManualDef>,
    /// Stop definitions
    pub stops: Vec<StopDef>,
    /// Windchest groups
    pub windchests: Vec<WindchestDef>,
    /// Enclosures (swell boxes)
    pub enclosures: Vec<EnclosureDef>,
    /// Tremulants
    pub tremulants: Vec<TremulantDef>,
    /// Couplers
    pub couplers: Vec<CouplerDef>,
}

/// General organ information from [Organ] section
#[derive(Debug, Clone, Default)]
pub struct OrganInfo {
    pub church_name: String,
    pub church_address: String,
    pub organ_builder: String,
    pub organ_build_date: String,
    pub organ_comments: String,
    pub recording_details: String,
    pub number_of_manuals: u32,
    pub has_pedals: bool,
    pub number_of_enclosures: u32,
    pub number_of_tremulants: u32,
    pub number_of_windchest_groups: u32,
    pub amplitude_level: f32,
    /// Orgel-brede Gain in dB ([Organ] Gain, default 0.0). Onderdeel van de
    /// GrandOrgue-inschalingshiërarchie: Organ -> Windchest -> Stop/Rank -> Pipe.
    pub gain_db: f32,
    /// Orgel-brede PitchTuning in cents ([Organ] PitchTuning, default 0.0) —
    /// wordt over de hele hiërarchie gesommeerd (GO "original temperament").
    pub pitch_tuning_cents: f32,
    /// Orgel-brede PitchCorrection in cents ([Organ] PitchCorrection, default
    /// 0.0) — alleen in hertemper-modus, gesommeerd over de hiërarchie.
    pub pitch_correction_cents: f32,
}

/// Manual/keyboard definition from [ManualXXX] section
#[derive(Debug, Clone)]
pub struct ManualDef {
    /// Manual number (000 = pedal, 001+ = manuals)
    pub number: u32,
    pub name: String,
    pub midi_input_number: u32,
    pub number_of_logical_keys: u32,
    pub number_of_accessible_keys: u32,
    pub first_accessible_key_midi_note: u32,
    /// Stop IDs assigned to this manual
    pub stop_ids: Vec<u32>,
    /// Coupler IDs
    pub coupler_ids: Vec<u32>,
    /// Tremulant IDs
    pub tremulant_ids: Vec<u32>,
}

/// Stop definition from [StopXXX] section
#[derive(Debug, Clone)]
pub struct StopDef {
    /// Stop ID (e.g., 001 for pedal, 101+ for manual)
    pub id: u32,
    pub name: String,
    /// Harmonic number (8 = 8', 16 = 4', 32 = 2', etc.)
    pub harmonic_number: u32,
    /// Pitch correction in cents (stop-niveau). GrandOrgue-semantiek: een
    /// BEWUSTE, OPGETELDE afwijking t.o.v. de getemperde toon (bv. de zwever
    /// van een Voix céleste) die alléén bij hertemperen (niet-"Original"
    /// temperament) meetelt, met plusteken: retune = (doel − gemeten) +
    /// PitchCorrection. In "Origineel" telt alléén PitchTuning.
    pub pitch_correction: f32,
    /// PitchTuning in cents (stop-niveau) — onderdeel van de gesommeerde
    /// stemmingshiërarchie (Organ + Windchest + Stop/Rank + Pipe).
    pub pitch_tuning_cents: f32,
    pub number_of_pipes: u32,
    pub first_accessible_pipe_logical_key: u32,
    pub windchest_group: u32,
    pub amplitude_level: f32,
    /// Stop-niveau Gain in dB (ODF [Stop]/[Rank] Gain)
    pub gain_db: f32,
    pub percussive: bool,
    /// Whether the stop's pipes are pitched (accept retuning). GrandOrgue sets
    /// AcceptsRetuning=N on mechanical/noise recordings (key action, stop/blower/
    /// ambient noise); real registers accept retuning. Default true when absent.
    pub accepts_retuning: bool,
    /// MIDI note of pipe index 0, when the stop does not start at its manual's
    /// first key. `None` = start at the manual's first accessible key (GrandOrgue
    /// default). `Some(n)` is used by the Hauptwerk importer, where stops in one
    /// division can have different compasses.
    pub first_midi_note: Option<u32>,
    /// Pipe definitions (path or reference) — de primaire pijplaag (laag 0).
    pub pipes: Vec<PipeDef>,
    /// Extra pijplagen (gestapelde ranks / microfoonperspectieven), elk dicht
    /// uitgelijnd op dezelfde eerste toets als `pipes`. Invariant: elke laag
    /// heeft `pipes.len() <= number_of_pipes`.
    pub layers: Vec<PipeLayer>,
    /// Canoniek perspectief-label van laag 0 (bv. "front") — alléén gezet als
    /// ten minste één extra laag een perspectief heeft; anders None.
    pub primary_perspective: Option<String>,
}

impl StopDef {
    /// Alle lagen inclusief de primaire: (laagindex, perspectief, pijpen).
    /// Laag 0 = `pipes` met `primary_perspective`; daarna `layers` op volgorde.
    pub fn all_layers(&self) -> impl Iterator<Item = (u8, Option<&str>, &[PipeDef])> {
        std::iter::once((0u8, self.primary_perspective.as_deref(), self.pipes.as_slice()))
            .chain(self.layers.iter().map(|l| (l.index, l.perspective.as_deref(), l.pipes.as_slice())))
    }

    /// Heuristic: is this "stop" actually a mechanical/noise recording rather
    /// than a playable musical register?
    ///
    /// GrandOrgue sample sets model key-action clatter, stop on/off noise,
    /// blower hum and room ambience as their own stops/ranks (and manuals
    /// reference them alongside the real registers). We exclude those so only
    /// real registers appear as draw-knobs.
    ///
    /// Two signals, OR-combined:
    /// 1. `AcceptsRetuning=N` — the recording is unpitched (structural, language-
    ///    independent; how producers flag noise samples).
    /// 2. The name contains a noise/mechanical marker (covers sets that don't set
    ///    the flag; markers cover the common producer languages).
    pub fn is_noise_or_mechanical(&self) -> bool {
        if !self.accepts_retuning {
            return true;
        }
        let n = self.name.to_lowercase();
        const NOISE_MARKERS: &[&str] = &[
            "noise", "action", "blower", "bellows", "ambient", "ambience",
            "motor", "rausch", "geräusch", "geraeusch", "bruit", "soufflerie",
            "moteur",
        ];
        NOISE_MARKERS.iter().any(|m| n.contains(m))
    }
}

/// Extra per-pijp informatie uit de ODF: inschaling, stemming, loops en
/// release-samples. Alles optioneel; `Default` = "niets opgegeven".
#[derive(Debug, Clone, Default)]
pub struct PipeExtra {
    /// AmplitudeLevel in procenten (GrandOrgue: 0-1000, 100 = neutraal)
    pub amplitude_level: Option<f32>,
    /// Gain in dB (GrandOrgue PipeXXXGain / [Organ] Gain-hiërarchie)
    pub gain_db: Option<f32>,
    /// PitchTuning in cents (float! bv. "4.79062")
    pub pitch_tuning_cents: Option<f32>,
    /// PitchCorrection in cents
    pub pitch_correction_cents: Option<f32>,
    /// One-shot afspelen (klok/glockenspiel) — pipe-niveau override op de stop
    pub percussive: Option<bool>,
    /// Beste ODF-looppunten (langste van alle LoopNNN). GrandOrgue-regel: als de
    /// ODF ten minste één loop opgeeft, worden de smpl-loops van de WAV volledig
    /// GENEGEERD. `loop_end` is hier al EXCLUSIEF (ODF LoopEnd is inclusief).
    pub loop_start: Option<u32>,
    pub loop_end: Option<u32>,
    /// Release-samples (opgenomen kerkakoestiek per toetsduur)
    pub releases: Vec<ReleaseDef>,
    /// PipeXXXLoadRelease (Y/N): release-segment uit het attack-bestand nemen.
    /// None = default (true, tenzij percussive).
    pub load_release: Option<bool>,
    /// PipeXXXCuePoint: override van de release-marker (cue-chunk) van het
    /// attack-bestand, in frames.
    pub cue_point: Option<u32>,
    /// Aparte tremulant-opname van deze pijp (Hauptwerk "tremmed"-laag)
    pub tremulant_sample: Option<PathBuf>,
    // ── Toonhoogte-metadata voor hertemperen (GO GOSoundingPipe-semantiek) ──
    /// ODF `{key}MIDIKeyNumber` (0..127): MIDI-toets van de opgenomen sample;
    /// overschrijft de smpl-chunk van de WAV. −1/afwezig → None.
    pub midi_key_number: Option<u32>,
    /// ODF `{key}MIDIPitchFraction` (cents 0..100 boven MIDIKeyNumber).
    pub midi_pitch_fraction: Option<f32>,
    /// `{key}HarmonicNumber` (pijp-niveau, anders rank-niveau; 8 = 8').
    pub harmonic_number: Option<u32>,
    /// MIDI-toets waarop deze pijp klinkt volgens de rank
    /// (FirstMidiNoteNumber + pijpindex). None = afleiden uit de stop.
    pub key_midi_note: Option<u32>,
    /// Hauptwerk: absolute samplepitch in cents (6900 = a' 440 Hz), uit
    /// Pitch_ExactSamplePitch of Pitch_NormalMIDINoteNumber.
    pub sample_pitch_cents: Option<f32>,
    /// Hauptwerk: gemeten toonhoogte van de originele pijp in cents
    /// (Pitch_OriginalOrgan_PitchHz) — terugval als sample/smpl ontbreken.
    pub original_pitch_cents: Option<f32>,
    /// Hauptwerk Pitch_Tempered_BaseTuningDeviation: percentage van de
    /// originele afwijking dat bij hertemperen behouden blijft (100 = niet
    /// hertemperen).
    pub retune_keep_pct: Option<f32>,
}

/// Eén release-sample van een pijp. `max_key_press_time_ms = None` of `-1`
/// betekent: de default/langste release (GrandOrgue MaxKeyPressTime-conventie).
#[derive(Debug, Clone)]
pub struct ReleaseDef {
    pub path: PathBuf,
    pub max_key_press_time_ms: Option<i32>,
    pub cue_point: Option<u32>,
    pub release_end: Option<u32>,
}

/// Pipe definition - either a direct sample path or a reference
#[derive(Debug, Clone)]
pub enum PipeDef {
    /// Direct path to WAV file
    Sample {
        path: PathBuf,
        extra: PipeExtra,
    },
    /// Reference to another pipe: REF:manual:stop:pipe
    Reference {
        manual: u32,
        stop: u32,
        pipe: u32,
    },
    /// No pipe for this key slot (keeps a stop's pipe list aligned to the
    /// keyboard when a note is missing within its compass). Resolves to no
    /// sample, so the key is simply silent.
    Empty,
}

/// Extra pijplaag van een stop: een gestapelde rank (mixtuurkoor) óf een
/// microfoonperspectief. De engine-sleutel blijft (stop_id, pipe_num); de
/// laagindex reist mee in bits 16–23 van pipe_num (zie audio.rs `layer_key`).
#[derive(Debug, Clone)]
pub struct PipeLayer {
    /// Laagindex 1..=15 (0 = `StopDef.pipes`); gaat in bits 16–23 van pipe_num.
    pub index: u8,
    /// Bronnaam (ODF [Rank] Name / HW StopRank-/Rank-Name / submapnaam).
    pub name: String,
    /// Canoniek perspectief-label; None = echte extra rank (altijd laden).
    pub perspective: Option<String>,
    /// Dicht vanaf toets 1 van de stop, zelfde uitlijning als `StopDef.pipes`;
    /// gaten = PipeDef::Empty.
    pub pipes: Vec<PipeDef>,
}

/// Windchest group definition
#[derive(Debug, Clone)]
pub struct WindchestDef {
    pub number: u32,
    pub comment: String,
    pub enclosure_ids: Vec<u32>,
    pub tremulant_ids: Vec<u32>,
    /// Windchest-niveau inschaling (GrandOrgue-hiërarchie: Organ → Windchest →
    /// Stop/Rank → Pipe). AmplitudeLevel in % (100 = neutraal), Gain in dB,
    /// PitchTuning in cents.
    pub amplitude_level: f32,
    pub gain_db: f32,
    pub pitch_tuning_cents: f32,
    /// PitchCorrection in cents (alleen hertemper-modus, gesommeerd).
    pub pitch_correction_cents: f32,
}

/// Enclosure (swell box) definition
#[derive(Debug, Clone)]
pub struct EnclosureDef {
    pub number: u32,
    pub name: String,
    pub amp_minimum_level: f32,
    pub midi_input_number: u32,
}

/// Tremulant definition
#[derive(Debug, Clone)]
pub struct TremulantDef {
    pub number: u32,
    pub name: String,
    /// Period in milliseconds
    pub period: f32,
    /// Amplitude modulation depth (%)
    pub amp_mod_depth: f32,
    pub start_rate: f32,
    pub stop_rate: f32,
}

/// Coupler definition
#[derive(Debug, Clone)]
pub struct CouplerDef {
    pub number: u32,
    pub name: String,
    pub source_manual: u32,
    pub destination_manual: u32,
    pub destination_keyshift: i32,
    pub unison_off: bool,
}

/// Parser for GrandOrgue ODF files
pub struct OdfParser {
    sections: HashMap<String, HashMap<String, String>>,
    base_path: PathBuf,
}

impl OdfParser {
    /// Parse an ODF file
    pub fn parse(path: &Path) -> Result<OrganDefinition, OdfError> {
        // Read file as bytes first, then try to decode
        let bytes = fs::read(path)?;

        // Try UTF-8 first, then fall back to Latin-1 (ISO-8859-1) which is common for older European files
        let content = String::from_utf8(bytes.clone())
            .unwrap_or_else(|_| {
                // Fallback: decode as Latin-1 (every byte is valid)
                bytes.iter().map(|&b| b as char).collect()
            });

        let base_path = path.parent().unwrap_or(Path::new(".")).to_path_buf();

        let mut parser = Self {
            sections: HashMap::new(),
            base_path,
        };

        parser.parse_ini(&content)?;
        parser.build_organ_definition()
    }

    /// Parse INI-style content into sections
    fn parse_ini(&mut self, content: &str) -> Result<(), OdfError> {
        let mut current_section = String::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with(';') {
                continue;
            }

            // Section header
            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len()-1].to_string();
                self.sections.entry(current_section.clone())
                    .or_insert_with(HashMap::new);
                continue;
            }

            // Key=Value pair
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim().to_string();
                let value = line[eq_pos+1..].trim().to_string();

                if let Some(section) = self.sections.get_mut(&current_section) {
                    section.insert(key, value);
                }
            }
        }

        Ok(())
    }

    /// Build organ definition from parsed sections
    fn build_organ_definition(&self) -> Result<OrganDefinition, OdfError> {
        let organ = self.parse_organ_section()?;
        let manuals = self.parse_manuals(organ.number_of_manuals)?;
        let stops = self.parse_stops()?;
        let windchests = self.parse_windchests(organ.number_of_windchest_groups)?;
        let enclosures = self.parse_enclosures(organ.number_of_enclosures)?;
        let tremulants = self.parse_tremulants(organ.number_of_tremulants)?;
        let couplers = self.parse_couplers()?;

        info!("Parsed organ: {} with {} manuals, {} stops",
              organ.church_name, manuals.len(), stops.len());

        Ok(OrganDefinition {
            base_path: self.base_path.clone(),
            organ,
            manuals,
            stops,
            windchests,
            enclosures,
            tremulants,
            couplers,
        })
    }

    /// Parse [Organ] section
    fn parse_organ_section(&self) -> Result<OrganInfo, OdfError> {
        let section = self.sections.get("Organ")
            .ok_or_else(|| OdfError::MissingField("Organ section".into()))?;

        Ok(OrganInfo {
            church_name: section.get("ChurchName").cloned().unwrap_or_default(),
            church_address: section.get("ChurchAddress").cloned().unwrap_or_default(),
            organ_builder: section.get("OrganBuilder").cloned().unwrap_or_default(),
            organ_build_date: section.get("OrganBuildDate").cloned().unwrap_or_default(),
            organ_comments: section.get("OrganComments").cloned().unwrap_or_default(),
            recording_details: section.get("RecordingDetails").cloned().unwrap_or_default(),
            number_of_manuals: self.parse_u32(section, "NumberOfManuals").unwrap_or(1),
            has_pedals: section.get("HasPedals").map(|v| v == "Y").unwrap_or(false),
            number_of_enclosures: self.parse_u32(section, "NumberOfEnclosures").unwrap_or(0),
            number_of_tremulants: self.parse_u32(section, "NumberOfTremulants").unwrap_or(0),
            number_of_windchest_groups: self.parse_u32(section, "NumberOfWindchestGroups").unwrap_or(1),
            amplitude_level: self.parse_f32(section, "AmplitudeLevel").unwrap_or(100.0),
            gain_db: self.parse_f32(section, "Gain").unwrap_or(0.0),
            pitch_tuning_cents: self.parse_f32(section, "PitchTuning").unwrap_or(0.0),
            pitch_correction_cents: self.parse_f32(section, "PitchCorrection").unwrap_or(0.0),
        })
    }

    /// Parse manual sections
    fn parse_manuals(&self, num_manuals: u32) -> Result<Vec<ManualDef>, OdfError> {
        let mut manuals = Vec::new();

        // Manual000 is pedal, Manual001+ are manuals
        for i in 0..=num_manuals {
            let section_name = format!("Manual{:03}", i);
            if let Some(section) = self.sections.get(&section_name) {
                let num_stops = self.parse_u32(section, "NumberOfStops").unwrap_or(0);
                let mut stop_ids = Vec::new();
                for s in 1..=num_stops {
                    if let Some(stop_ref) = section.get(&format!("Stop{:03}", s)) {
                        if let Ok(id) = stop_ref.parse::<u32>() {
                            stop_ids.push(id);
                        }
                    }
                }

                let num_couplers = self.parse_u32(section, "NumberOfCouplers").unwrap_or(0);
                let mut coupler_ids = Vec::new();
                for c in 1..=num_couplers {
                    if let Some(coupler_ref) = section.get(&format!("Coupler{:03}", c)) {
                        if let Ok(id) = coupler_ref.parse::<u32>() {
                            coupler_ids.push(id);
                        }
                    }
                }

                let num_tremulants = self.parse_u32(section, "NumberOfTremulants").unwrap_or(0);
                let mut tremulant_ids = Vec::new();
                for t in 1..=num_tremulants {
                    if let Some(trem_ref) = section.get(&format!("Tremulant{:03}", t)) {
                        if let Ok(id) = trem_ref.parse::<u32>() {
                            tremulant_ids.push(id);
                        }
                    }
                }

                manuals.push(ManualDef {
                    number: i,
                    name: section.get("Name").cloned().unwrap_or_default(),
                    midi_input_number: self.parse_u32(section, "MIDIInputNumber").unwrap_or(i + 1),
                    number_of_logical_keys: self.parse_u32(section, "NumberOfLogicalKeys").unwrap_or(61),
                    number_of_accessible_keys: self.parse_u32(section, "NumberOfAccessibleKeys").unwrap_or(61),
                    first_accessible_key_midi_note: self.parse_u32(section, "FirstAccessibleKeyMIDINoteNumber").unwrap_or(36),
                    stop_ids,
                    coupler_ids,
                    tremulant_ids,
                });
            }
        }

        Ok(manuals)
    }

    /// Parse all stop sections
    fn parse_stops(&self) -> Result<Vec<StopDef>, OdfError> {
        let mut stops = Vec::new();

        for (section_name, section) in &self.sections {
            if section_name.starts_with("Stop") && section_name.len() == 7 {
                if let Ok(id) = section_name[4..].parse::<u32>() {
                    // Modern GrandOrgue: a stop references one or more [RankNNN]
                    // sections that hold the pipes. Older/simple ODFs list the pipes
                    // inline in the [StopNNN] section. Try ranks first, then inline.
                    let (pipes, harmonic, layers, primary_perspective) = match self.parse_stop_ranks(section) {
                        Some((rank_pipes, rank_harmonic, layers, primary)) => {
                            let h = self.parse_u32(section, "HarmonicNumber")
                                .unwrap_or(if rank_harmonic > 0 { rank_harmonic } else { 8 });
                            (rank_pipes, h, layers, primary)
                        }
                        None => {
                            let num_pipes = self.parse_u32(section, "NumberOfLogicalPipes")
                                .or_else(|| self.parse_u32(section, "NumberOfAccessiblePipes"))
                                .unwrap_or(0);
                            let inline = self.parse_pipes(section, num_pipes)?;
                            (inline, self.parse_u32(section, "HarmonicNumber").unwrap_or(8), Vec::new(), None)
                        }
                    };
                    debug_assert!(
                        layers.iter().all(|l| l.pipes.len() <= pipes.len()),
                        "laag langer dan de primaire pijplijst"
                    );

                    stops.push(StopDef {
                        id,
                        name: section.get("Name").cloned().unwrap_or_default(),
                        harmonic_number: harmonic,
                        pitch_correction: self.parse_f32(section, "PitchCorrection").unwrap_or(0.0),
                        pitch_tuning_cents: self.parse_f32(section, "PitchTuning").unwrap_or(0.0),
                        gain_db: self.parse_f32(section, "Gain").unwrap_or(0.0),
                        number_of_pipes: pipes.len() as u32,
                        first_accessible_pipe_logical_key: self.parse_u32(section, "FirstAccessiblePipeLogicalKeyNumber").unwrap_or(1),
                        // Moderne rank-gebaseerde ODF's zetten WindchestGroup op de
                        // [RankNNN]-sectie, niet op de stop zelf. Zonder deze fallback
                        // kregen alle rank-stops groep 1 → de enclosure-koppeling
                        // (windchest → Enclosure → has_swell) vuurde nooit en
                        // zwelkasten uit de orgeldefinitie werden genegeerd.
                        windchest_group: self.parse_u32(section, "WindchestGroup")
                            .or_else(|| {
                                self.parse_u32(section, "Rank001")
                                    .and_then(|rid| self.sections.get(&format!("Rank{:03}", rid)))
                                    .and_then(|rs| self.parse_u32(rs, "WindchestGroup"))
                            })
                            .unwrap_or(1),
                        amplitude_level: self.parse_f32(section, "AmplitudeLevel").unwrap_or(100.0),
                        percussive: section.get("Percussive").map(|v| v == "Y").unwrap_or(false),
                        accepts_retuning: section.get("AcceptsRetuning").map(|v| v != "N").unwrap_or(true),
                        first_midi_note: None,
                        pipes,
                        layers,
                        primary_perspective,
                    });
                }
            }
        }

        // Sort by ID
        stops.sort_by_key(|s| s.id);

        Ok(stops)
    }

    /// Build a stop's pipe list from the [RankNNN] sections it references (modern
    /// GrandOrgue format). Honors RankNNNFirstPipeNumber / RankNNNPipeCount so a
    /// stop can use a slice of a shared rank. Returns the pipes (in keyboard
    /// order), the detected harmonic number, de extra pijplagen (gestapelde
    /// ranks / perspectieven) en het perspectief van laag 0, or None if the stop
    /// has no rank refs (older inline-pipe format — handled by parse_pipes instead).
    fn parse_stop_ranks(
        &self,
        stop_section: &HashMap<String, String>,
    ) -> Option<(Vec<PipeDef>, u32, Vec<PipeLayer>, Option<String>)> {
        let num_ranks = self.parse_u32(stop_section, "NumberOfRanks")?;
        if num_ranks == 0 {
            return None;
        }

        let accessible = self.parse_u32(stop_section, "NumberOfAccessiblePipes").unwrap_or(0);
        let mut harmonic = 0u32;

        // Pijpen worden op TOETS-positie gelegd i.p.v. blind achter elkaar
        // geplakt. Multi-rank stops zijn meestal splices (elke rank dekt een
        // ander toetsbereik, via RankNNNFirstAccessibleKeyNumber — bv. een
        // geleende bas of een repeterende kwint); de oude platte append ging
        // alleen goed wanneer de ranks toevallig aansluitend in ODF-volgorde
        // stonden, en schoof bovendien álle volgende pijpen een positie op
        // wanneer een REF-regel misvormd was (→ verkeerde pijp op elke toets
        // erna). Overlappende ranks (gestapelde mixtuurkoren, of microfoon-
        // perspectieven als aparte ranks) gaan in EXTRA LAGEN (PipeLayer):
        // zodra een rank op een al bezette toets komt, krijgt die rank een
        // eigen laag en gaan ál zijn volgende pijpen daarin (rank-intact: één
        // ODF-[Rank] = één PipeLayer). Splices met disjuncte toetsen blijven
        // laagloos. Maximaal 15 lagen; daarboven droppen met een warn.
        use std::collections::BTreeMap;
        let mut by_key: BTreeMap<u32, PipeDef> = BTreeMap::new();
        // laagindex → (ranknaam, toets → pijp)
        let mut extra: BTreeMap<u8, (String, BTreeMap<u32, PipeDef>)> = BTreeMap::new();
        let mut next_layer: u8 = 1;
        let mut stacked_dropped = 0usize;
        let mut first_rank_name: Option<String> = None;

        for r in 1..=num_ranks {
            let rank_id = match self.parse_u32(stop_section, &format!("Rank{:03}", r)) {
                Some(v) => v,
                None => continue,
            };
            let rank_section = match self.sections.get(&format!("Rank{:03}", rank_id)) {
                Some(s) => s,
                None => continue,
            };
            let rank_name = rank_section
                .get("Name")
                .cloned()
                .unwrap_or_else(|| format!("Rank {}", rank_id));
            if first_rank_name.is_none() {
                first_rank_name = Some(rank_name.clone());
            }
            // Laag van deze rank: None zolang hij (nog) in de primaire lijst
            // past; Some zodra hij ergens overlapt (rank-intact daarna).
            let mut layer: Option<u8> = None;
            let first_pipe = self
                .parse_u32(stop_section, &format!("Rank{:03}FirstPipeNumber", r))
                .unwrap_or(1);
            // Caps op ODF-tellers: een corrupte/kwaadaardige waarde mag geen
            // gigalussen of een alloc-abort veroorzaken (auditbevindingen 22/45).
            let count = self
                .parse_u32(stop_section, &format!("Rank{:03}PipeCount", r))
                .or_else(|| self.parse_u32(rank_section, "NumberOfLogicalPipes"))
                .unwrap_or(accessible)
                .min(1024);
            // Op welke toets (1-based, binnen het bereik van de stop) deze
            // rank-slice begint. Zonder deze sleutel: toets 1 (zoals GO).
            let first_key = self
                .parse_u32(stop_section, &format!("Rank{:03}FirstAccessibleKeyNumber", r))
                .unwrap_or(1)
                .min(512);

            // Rank-niveau inschaling (sectie-brede sleutels zónder Pipe-prefix):
            // onderdeel van de GO-hiërarchie Organ→Windchest→Stop/Rank→Pipe.
            // AmplitudeLevel is multiplicatief (%), Gain (dB) en PitchTuning
            // (cents) zijn additief — hieronder in elke pijp-extra gevouwen.
            let rank_ampl = self.parse_f32(rank_section, "AmplitudeLevel").unwrap_or(100.0);
            let rank_gain = self.parse_f32(rank_section, "Gain").unwrap_or(0.0);
            let rank_pitch = self.parse_f32(rank_section, "PitchTuning").unwrap_or(0.0);
            // Rank-brede PitchCorrection (hertemperen), HarmonicNumber (GO-
            // default voor pijpen zonder eigen waarde) en FirstMidiNoteNumber
            // (MIDI-toets van Pipe001 → key_midi_note per pijp).
            let rank_pc = self.parse_f32(rank_section, "PitchCorrection").unwrap_or(0.0);
            let rank_h = self.parse_u32(rank_section, "HarmonicNumber");
            let rank_first_midi = self.parse_u32(rank_section, "FirstMidiNoteNumber");
            let rank_neutral = (rank_ampl - 100.0).abs() < 1e-6
                && rank_gain.abs() < 1e-6
                && rank_pitch.abs() < 1e-6
                && rank_pc.abs() < 1e-6;

            for i in 0..count {
                let key = format!("Pipe{:03}", first_pipe + i);
                let value = match rank_section.get(&key) {
                    Some(v) => v,
                    None => break, // past the end of the rank
                };
                if harmonic == 0 {
                    if let Some(h) = self
                        .parse_u32(rank_section, &format!("{}HarmonicNumber", key))
                        .or(rank_h)
                    {
                        harmonic = h;
                    }
                }
                let key_num = first_key + i;
                let pd = if value.eq_ignore_ascii_case("DUMMY") {
                    // GO-plaatshouder: bewust geen sample op deze toets.
                    PipeDef::Empty
                } else if let Some(rest) = value.strip_prefix("REF:") {
                    // Misvormd (verkeerd aantal delen óf niet-numeriek): slot
                    // leeg laten en loggen — een unwrap_or(0)-terugval wees
                    // voorheen stilletjes naar de eerste pijp van de eerste
                    // pedaalstop (auditbevinding 39).
                    let parts: Vec<&str> = rest.split(':').collect();
                    let parsed = if parts.len() == 3 {
                        match (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>(), parts[2].trim().parse::<u32>()) {
                            (Ok(m), Ok(s), Ok(p)) => Some((m, s, p)),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    match parsed {
                        Some((manual, stop, pipe)) => PipeDef::Reference { manual, stop, pipe },
                        None => {
                            warn!("Misvormde REF '{}' (toets {}): slot leeg gelaten", value, key_num);
                            PipeDef::Empty
                        }
                    }
                } else {
                    // Inschaling, stemming, percussive, loops en releases
                    // komen uit de rank-sectie met dezelfde sleutelnamen.
                    let mut extra = self.parse_pipe_extra(rank_section, &key);
                    if !rank_neutral {
                        // Vouw de rank-brede inschaling in de pijp-extra:
                        // percentages vermenigvuldigen, dB/cents optellen.
                        let pipe_ampl = extra.amplitude_level.unwrap_or(100.0);
                        extra.amplitude_level = Some(pipe_ampl * rank_ampl / 100.0);
                        extra.gain_db = Some(extra.gain_db.unwrap_or(0.0) + rank_gain);
                        extra.pitch_tuning_cents = Some(extra.pitch_tuning_cents.unwrap_or(0.0) + rank_pitch);
                        extra.pitch_correction_cents = Some(extra.pitch_correction_cents.unwrap_or(0.0) + rank_pc);
                    }
                    // Rank-HarmonicNumber als default per pijp (GORank::Load →
                    // GOSoundingPipe::Load) en de MIDI-toets van deze pijp.
                    extra.harmonic_number = extra.harmonic_number.or(rank_h);
                    extra.key_midi_note = rank_first_midi.map(|f| f + first_pipe + i - 1);
                    PipeDef::Sample {
                        path: self.resolve_path(value),
                        extra,
                    }
                };
                if let Some(l) = layer {
                    // Rank-intact: deze rank heeft al een laag → alles erin.
                    if !matches!(pd, PipeDef::Empty) {
                        if let Some((_, m)) = extra.get_mut(&l) {
                            m.insert(key_num, pd);
                        }
                    }
                    continue;
                }
                match by_key.entry(key_num) {
                    std::collections::btree_map::Entry::Vacant(e) => { e.insert(pd); }
                    std::collections::btree_map::Entry::Occupied(mut e) => {
                        // Zelfde toets al bezet door een eerdere rank. Een échte
                        // pijp verdringt een leeg slot (DUMMY); een echte
                        // botsing maakt van deze rank een extra laag.
                        if matches!(e.get(), PipeDef::Empty) && !matches!(pd, PipeDef::Empty) {
                            e.insert(pd);
                        } else if !matches!(pd, PipeDef::Empty) {
                            if (next_layer as usize) < 16 {
                                let l = next_layer;
                                next_layer += 1;
                                let mut m = BTreeMap::new();
                                m.insert(key_num, pd);
                                extra.insert(l, (rank_name.clone(), m));
                                layer = Some(l);
                            } else {
                                stacked_dropped += 1;
                            }
                        }
                    }
                }
            }
        }

        let stop_name = stop_section.get("Name").map(String::as_str).unwrap_or("?");
        if stacked_dropped > 0 {
            warn!(
                "Stop '{}': {} gestapelde rank-pijpen genegeerd — meer dan 15 extra lagen worden niet ondersteund",
                stop_name, stacked_dropped
            );
        }

        if by_key.is_empty() && extra.is_empty() {
            None
        } else {
            // Dichte lijst vanaf toets 1: gaten blijven Empty zodat de
            // noot→pijp-uitlijning (pipe_num = noot - eerste + 1) klopt. Het
            // bereik loopt tot de hoogste toets over ALLE lagen, zodat elke
            // laag binnen `number_of_pipes` past (invariant).
            let max_key = by_key
                .keys()
                .copied()
                .chain(extra.values().flat_map(|(_, m)| m.keys().copied()))
                .max()
                .unwrap_or(0);
            let mut pipes = Vec::with_capacity(max_key as usize);
            for k in 1..=max_key {
                pipes.push(by_key.remove(&k).unwrap_or(PipeDef::Empty));
            }
            let mut layers: Vec<PipeLayer> = Vec::with_capacity(extra.len());
            let mut layered_pipes = 0usize;
            for (index, (name, mut m)) in extra {
                layered_pipes += m.len();
                let mut lp = Vec::with_capacity(max_key as usize);
                for k in 1..=max_key {
                    lp.push(m.remove(&k).unwrap_or(PipeDef::Empty));
                }
                let perspective = crate::perspective::detect_perspective(&name);
                layers.push(PipeLayer { index, name, perspective, pipes: lp });
            }
            // Perspectief van laag 0 alléén wanneer ≥1 laag er een heeft —
            // anders krijgt een enkel-perspectief-set met "(front)" in alle
            // ranknamen een fantoomperspectief.
            let primary_perspective = if layers.iter().any(|l| l.perspective.is_some()) {
                first_rank_name.as_deref().and_then(crate::perspective::detect_perspective)
            } else {
                None
            };
            if !layers.is_empty() {
                info!(
                    "Stop '{}': {} pijpen in {} extra laag/lagen (gestapelde ranks/perspectieven): {}",
                    stop_name,
                    layered_pipes,
                    layers.len(),
                    layers
                        .iter()
                        .map(|l| match &l.perspective {
                            Some(p) => format!("{} [{}]", l.name, p),
                            None => l.name.clone(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Some((pipes, harmonic, layers, primary_perspective))
        }
    }

    /// Parse alle extra pijp-attributen voor de pijp met sleutelprefix `key`
    /// (bv. "Pipe001") uit `section`. Gedeeld door het inline-pad (parse_pipes)
    /// en het rank-pad (parse_stop_ranks); de sleutelnamen zijn identiek.
    ///
    /// LET OP eenheden: `{key}Gain` is dB, `{key}AmplitudeLevel` is % en
    /// PitchTuning/PitchCorrection zijn cents (floats, bv. "4.79062").
    fn parse_pipe_extra(&self, section: &HashMap<String, String>, key: &str) -> PipeExtra {
        // ODF-looppunten: parse ALLE LoopNNN-records en kies de langste geldige
        // (onze engine speelt één loop). GrandOrgue-semantiek: LoopEnd is de
        // laatste frame ínclusief → hier omgerekend naar exclusief; en zodra de
        // ODF een loop opgeeft, winnen die van de smpl-chunk in de WAV.
        let loop_count = self
            .parse_u32(section, &format!("{}LoopCount", key))
            .unwrap_or(0)
            .min(100);
        let mut best_loop: Option<(u32, u32)> = None;
        for li in 1..=loop_count {
            let s = self.parse_u32(section, &format!("{}Loop{:03}Start", key, li)).unwrap_or(0);
            let e = match self.parse_u32(section, &format!("{}Loop{:03}End", key, li)) {
                Some(e) => e,
                None => continue,
            };
            if e <= s {
                continue; // ongeldig record
            }
            let e_excl = e.saturating_add(1);
            if best_loop.map_or(true, |(bs, be)| e_excl - s > be - bs) {
                best_loop = Some((s, e_excl));
            }
        }
        let (loop_start, loop_end) = match best_loop {
            Some((s, e)) => (Some(s), Some(e)),
            None => (None, None),
        };

        PipeExtra {
            amplitude_level: self.parse_f32(section, &format!("{}AmplitudeLevel", key)),
            gain_db: self.parse_f32(section, &format!("{}Gain", key)),
            pitch_tuning_cents: self.parse_f32(section, &format!("{}PitchTuning", key)),
            pitch_correction_cents: self.parse_f32(section, &format!("{}PitchCorrection", key)),
            // Pipe-niveau Percussive (Y/N) overschrijft het stop-niveau; None = niet opgegeven.
            percussive: section.get(&format!("{}Percussive", key)).map(|v| v == "Y"),
            loop_start,
            loop_end,
            releases: self.parse_pipe_releases(section, key),
            load_release: section.get(&format!("{}LoadRelease", key)).map(|v| v.trim().eq_ignore_ascii_case("y")),
            cue_point: self.parse_u32(section, &format!("{}CuePoint", key)),
            tremulant_sample: None,
            // Toonhoogte-metadata (hertemperen). MIDIKeyNumber=-1 geeft via
            // parse_u32 al None; MIDIPitchFraction=-1 parset wél (-1.0) en
            // moet expliciet buiten 0..=100 vallen.
            midi_key_number: self
                .parse_u32(section, &format!("{}MIDIKeyNumber", key))
                .filter(|k| *k <= 127),
            midi_pitch_fraction: self
                .parse_f32(section, &format!("{}MIDIPitchFraction", key))
                .filter(|f| (0.0..=100.0).contains(f)),
            harmonic_number: self.parse_u32(section, &format!("{}HarmonicNumber", key)),
            key_midi_note: None,
            sample_pitch_cents: None,
            original_pitch_cents: None,
            retune_keep_pct: None,
        }
    }

    /// Parse de release-samples van één pijp: `{key}ReleaseCount` gevolgd door
    /// per release `{key}Release{jjj}` (pad), `...MaxKeyPressTime` (ms, -1 =
    /// default/langste release), `...CuePoint` en `...ReleaseEnd` (samplenummers).
    fn parse_pipe_releases(&self, section: &HashMap<String, String>, key: &str) -> Vec<ReleaseDef> {
        // Cap zoals bij LoopCount: een ODF-waarde als 4294967295 gaf hier een
        // ~200 GB Vec::with_capacity → alloc-abort van het hele proces.
        let count = self
            .parse_u32(section, &format!("{}ReleaseCount", key))
            .unwrap_or(0)
            .min(100);
        let mut releases = Vec::with_capacity(count as usize);

        for j in 1..=count {
            let release_key = format!("{}Release{:03}", key, j);
            if let Some(path) = section.get(&release_key) {
                releases.push(ReleaseDef {
                    path: self.resolve_path(path),
                    max_key_press_time_ms: self
                        .parse_i32(section, &format!("{}MaxKeyPressTime", release_key)),
                    cue_point: self.parse_u32(section, &format!("{}CuePoint", release_key)),
                    release_end: self.parse_u32(section, &format!("{}ReleaseEnd", release_key)),
                });
            }
        }

        releases
    }

    /// Parse pipe definitions for a stop
    fn parse_pipes(&self, section: &HashMap<String, String>, num_pipes: u32) -> Result<Vec<PipeDef>, OdfError> {
        let mut pipes = Vec::new();

        // Cap tegen giga-lussen op een corrupte NumberOfPipes-achtige teller.
        for i in 1..=num_pipes.min(1024) {
            let key = format!("Pipe{:03}", i);
            if let Some(value) = section.get(&key) {
                let pipe = if value.eq_ignore_ascii_case("DUMMY") {
                    // GO-plaatshouder: bewust geen sample op deze toets (ook
                    // door onze eigen export gebruikt voor gaten in een stop).
                    PipeDef::Empty
                } else if value.starts_with("REF:") {
                    // Reference format: REF:manual:stop:pipe. Misvormd (delen of
                    // niet-numeriek) → slot leeg + log, net als in het rank-pad.
                    let parts: Vec<&str> = value[4..].split(':').collect();
                    let parsed = if parts.len() == 3 {
                        match (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>(), parts[2].trim().parse::<u32>()) {
                            (Ok(m), Ok(s), Ok(p)) => Some((m, s, p)),
                            _ => None,
                        }
                    } else {
                        None
                    };
                    match parsed {
                        Some((manual, stop, pipe)) => PipeDef::Reference { manual, stop, pipe },
                        None => {
                            warn!("Misvormde REF '{}' (pijp {}): slot leeg gelaten", value, i);
                            PipeDef::Empty
                        }
                    }
                } else {
                    // Sample path
                    let path = self.resolve_path(value);
                    PipeDef::Sample {
                        path,
                        // Inschaling, stemming, percussive, loops en releases.
                        extra: self.parse_pipe_extra(section, &key),
                    }
                };

                pipes.push(pipe);
            }
        }

        Ok(pipes)
    }

    /// Parse windchest groups
    fn parse_windchests(&self, num: u32) -> Result<Vec<WindchestDef>, OdfError> {
        let mut windchests = Vec::new();

        for i in 1..=num {
            let section_name = format!("WindchestGroup{:03}", i);
            if let Some(section) = self.sections.get(&section_name) {
                let num_enclosures = self.parse_u32(section, "NumberOfEnclosures").unwrap_or(0);
                let mut enclosure_ids = Vec::new();
                for e in 1..=num_enclosures {
                    if let Some(enc_ref) = section.get(&format!("Enclosure{:03}", e)) {
                        if let Ok(id) = enc_ref.parse::<u32>() {
                            enclosure_ids.push(id);
                        }
                    }
                }

                let num_tremulants = self.parse_u32(section, "NumberOfTremulants").unwrap_or(0);
                let mut tremulant_ids = Vec::new();
                for t in 1..=num_tremulants {
                    if let Some(trem_ref) = section.get(&format!("Tremulant{:03}", t)) {
                        if let Ok(id) = trem_ref.parse::<u32>() {
                            tremulant_ids.push(id);
                        }
                    }
                }

                windchests.push(WindchestDef {
                    number: i,
                    // Prefer the windchest's Name (used to match it to its division
                    // for swell-box detection); fall back to Comment.
                    comment: section.get("Name")
                        .or_else(|| section.get("Comment"))
                        .cloned()
                        .unwrap_or_default(),
                    enclosure_ids,
                    tremulant_ids,
                    amplitude_level: self.parse_f32(section, "AmplitudeLevel").unwrap_or(100.0),
                    gain_db: self.parse_f32(section, "Gain").unwrap_or(0.0),
                    pitch_tuning_cents: self.parse_f32(section, "PitchTuning").unwrap_or(0.0),
                    pitch_correction_cents: self.parse_f32(section, "PitchCorrection").unwrap_or(0.0),
                });
            }
        }

        Ok(windchests)
    }

    /// Parse enclosures
    fn parse_enclosures(&self, num: u32) -> Result<Vec<EnclosureDef>, OdfError> {
        let mut enclosures = Vec::new();

        for i in 1..=num {
            let section_name = format!("Enclosure{:03}", i);
            if let Some(section) = self.sections.get(&section_name) {
                enclosures.push(EnclosureDef {
                    number: i,
                    name: section.get("Name").cloned().unwrap_or_default(),
                    amp_minimum_level: self.parse_f32(section, "AmpMinimumLevel").unwrap_or(0.0),
                    midi_input_number: self.parse_u32(section, "MIDIInputNumber").unwrap_or(0),
                });
            }
        }

        Ok(enclosures)
    }

    /// Parse tremulants
    fn parse_tremulants(&self, num: u32) -> Result<Vec<TremulantDef>, OdfError> {
        let mut tremulants = Vec::new();

        for i in 1..=num {
            let section_name = format!("Tremulant{:03}", i);
            if let Some(section) = self.sections.get(&section_name) {
                tremulants.push(TremulantDef {
                    number: i,
                    name: section.get("Name").cloned().unwrap_or_default(),
                    period: self.parse_f32(section, "Period").unwrap_or(160.0),
                    amp_mod_depth: self.parse_f32(section, "AmpModDepth").unwrap_or(18.0),
                    start_rate: self.parse_f32(section, "StartRate").unwrap_or(8.0),
                    stop_rate: self.parse_f32(section, "StopRate").unwrap_or(8.0),
                });
            }
        }

        Ok(tremulants)
    }

    /// Parse couplers
    fn parse_couplers(&self) -> Result<Vec<CouplerDef>, OdfError> {
        let mut couplers = Vec::new();

        for (section_name, section) in &self.sections {
            if section_name.starts_with("Coupler") && section_name.len() == 10 {
                if let Ok(id) = section_name[7..].parse::<u32>() {
                    couplers.push(CouplerDef {
                        number: id,
                        name: section.get("Name").cloned().unwrap_or_default(),
                        source_manual: 0, // Determined by which manual references this coupler
                        destination_manual: self.parse_u32(section, "DestinationManual").unwrap_or(0),
                        destination_keyshift: self.parse_i32(section, "DestinationKeyshift").unwrap_or(0),
                        unison_off: section.get("UnisonOff").map(|v| v == "Y").unwrap_or(false),
                    });
                }
            }
        }

        // Sort by ID
        couplers.sort_by_key(|c| c.number);

        Ok(couplers)
    }

    /// Resolve a relative path to absolute
    fn resolve_path(&self, path: &str) -> PathBuf {
        // Normalize path separators and remove .\ or ./
        let normalized = path
            .replace('\\', "/")
            .trim_start_matches("./")
            .trim_start_matches(".\\")
            .to_string();

        self.base_path.join(normalized)
    }

    /// Parse a u32 from section
    fn parse_u32(&self, section: &HashMap<String, String>, key: &str) -> Option<u32> {
        section.get(key).and_then(|v| v.parse().ok())
    }

    /// Parse an i32 from section
    fn parse_i32(&self, section: &HashMap<String, String>, key: &str) -> Option<i32> {
        section.get(key).and_then(|v| v.parse().ok())
    }

    /// Parse a f32 from section
    fn parse_f32(&self, section: &HashMap<String, String>, key: &str) -> Option<f32> {
        section.get(key).and_then(|v| v.parse().ok())
    }
}

impl OrganDefinition {
    /// Load organ definition from file
    pub fn load(path: &Path) -> Result<Self, OdfError> {
        OdfParser::parse(path)
    }

    /// Get all unique sample paths needed for this organ
    pub fn get_sample_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        for stop in &self.stops {
            for (_, _, pipes) in stop.all_layers() {
                for pipe in pipes {
                    if let PipeDef::Sample { path, .. } = pipe {
                        if !paths.contains(path) {
                            paths.push(path.clone());
                        }
                    }
                }
            }
        }

        paths
    }

    /// Resolve a pipe reference to its actual sample path
    pub fn resolve_reference<'a>(&'a self, reference: &'a PipeDef) -> Option<&'a PipeDef> {
        self.resolve_reference_depth(reference, 0)
    }

    /// Als `resolve_reference`, maar geeft óók de EIGENAAR van de uiteindelijke
    /// sample terug: (eigenaar-stop, 0-based pijpindex binnen die stop, de
    /// `PipeDef::Sample`). Voor een directe Sample is dat (`owner`, `pipe_idx`,
    /// self). Nodig voor hertemperen: een REF-pijp (bv. Octaaf 4' → Prestant
    /// 8') moet de HarmonicNumber/PitchCorrection/windchest van de EIGENAAR
    /// gebruiken (GO: GOReferencePipe speelt de GOSoundingPipe van de eigenaar),
    /// niet die van de lenende stop — anders ±1200-ct-uitschieters.
    pub fn resolve_reference_owner<'a>(
        &'a self,
        owner: &'a StopDef,
        pipe_idx: usize,
        reference: &'a PipeDef,
    ) -> Option<(&'a StopDef, usize, &'a PipeDef)> {
        self.resolve_reference_owner_depth(owner, pipe_idx, reference, 0)
    }

    fn resolve_reference_owner_depth<'a>(
        &'a self,
        owner: &'a StopDef,
        pipe_idx: usize,
        reference: &'a PipeDef,
        depth: u32,
    ) -> Option<(&'a StopDef, usize, &'a PipeDef)> {
        if depth > 8 {
            return None; // cyclus/pathologische keten (zie resolve_reference_depth)
        }
        match reference {
            PipeDef::Sample { .. } => Some((owner, pipe_idx, reference)),
            PipeDef::Empty => None,
            PipeDef::Reference { manual, stop, pipe } => {
                // Zelfde keten als resolve_reference_depth: manual-relatieve
                // stopindex, terugval op de globale stop-id.
                let resolved_stop = self.manuals.iter()
                    .find(|m| m.number == *manual)
                    .and_then(|m| m.stop_ids.get((*stop as usize).saturating_sub(1)))
                    .and_then(|sid| self.stops.iter().find(|s| s.id == *sid))
                    .or_else(|| self.stops.iter().find(|s| s.id == *stop))?;
                let idx = (*pipe as usize).saturating_sub(1);
                let p = resolved_stop.pipes.get(idx)?;
                self.resolve_reference_owner_depth(resolved_stop, idx, p, depth + 1)
            }
        }
    }

    fn resolve_reference_depth<'a>(&'a self, reference: &'a PipeDef, depth: u32) -> Option<&'a PipeDef> {
        // Dieptebegrenzer: een REF kan naar een REF wijzen; een cyclus of een
        // pathologische keten in een misvormde ODF gaf hier voorheen een
        // stack-overflow (crash van de hele app tijdens het laden).
        if depth > 8 {
            warn!("REF-keten dieper dan 8 schakels — vermoedelijk een cyclus in de ODF; verwijzing genegeerd");
            return None;
        }
        match reference {
            PipeDef::Sample { .. } => Some(reference),
            PipeDef::Empty => None,
            PipeDef::Reference { manual, stop, pipe } => {
                // GrandOrgue's REF:mm:ss:pp gebruikt de MANUAL-RELATIEVE stopindex
                // (1-based positie in de stoplijst van manual mm), niet de globale
                // stop-id. Zoek dus eerst via het manual; val terug op de globale
                // id voor sets die (incorrect maar voorkomend) zo verwijzen.
                let resolved_stop = self.manuals.iter()
                    .find(|m| m.number == *manual)
                    .and_then(|m| m.stop_ids.get((*stop as usize).saturating_sub(1)))
                    .and_then(|sid| self.stops.iter().find(|s| s.id == *sid))
                    .or_else(|| self.stops.iter().find(|s| s.id == *stop));
                resolved_stop
                    .and_then(|s| s.pipes.get((*pipe as usize).saturating_sub(1)))
                    .and_then(|p| self.resolve_reference_depth(p, depth + 1))
            }
        }
    }

    /// Get the sample path for a specific stop and pipe number
    pub fn get_pipe_sample(&self, stop_id: u32, pipe_num: u32) -> Option<PathBuf> {
        self.stops.iter()
            .find(|s| s.id == stop_id)
            .and_then(|s| s.pipes.get(pipe_num as usize - 1))
            .and_then(|p| self.resolve_reference(p))
            .and_then(|p| match p {
                PipeDef::Sample { path, .. } => Some(path.clone()),
                _ => None,
            })
    }

    /// Find a stop by ID
    pub fn get_stop(&self, id: u32) -> Option<&StopDef> {
        self.stops.iter().find(|s| s.id == id)
    }

    /// Find a manual by number
    pub fn get_manual(&self, number: u32) -> Option<&ManualDef> {
        self.manuals.iter().find(|m| m.number == number)
    }

    /// Convert a GrandOrgue HarmonicNumber to a footage string (e.g. 8 -> "8'",
    /// 16 -> "4'", 48 -> "1 1/3'").
    ///
    /// GrandOrgue expresses pitch as a harmonic where the 8' unison = 8, so the
    /// footage in feet is `64 / harmonic`. Besides the octave pitches this also
    /// covers the common mutations (quints/tierces): 2 2/3' = 24, 1 3/5' = 40,
    /// 1 1/3' = 48, etc. A non-standard value falls back to the derived decimal.
    pub fn harmonic_to_footage(harmonic: u32) -> String {
        match harmonic {
            1 => "64'".to_string(),
            2 => "32'".to_string(),
            3 => "21 1/3'".to_string(),
            4 => "16'".to_string(),
            6 => "10 2/3'".to_string(),
            8 => "8'".to_string(),
            12 => "5 1/3'".to_string(),
            16 => "4'".to_string(),
            24 => "2 2/3'".to_string(),
            32 => "2'".to_string(),
            40 => "1 3/5'".to_string(),
            48 => "1 1/3'".to_string(),
            64 => "1'".to_string(),
            80 => "4/5'".to_string(),
            96 => "2/3'".to_string(),
            128 => "1/2'".to_string(),
            0 => "8'".to_string(),
            _ => {
                // Non-standard harmonic: derive feet = 64 / harmonic and show it
                // trimmed (e.g. 64/20 = 3.2 -> "3.2'").
                let feet = 64.0_f32 / harmonic as f32;
                let s = format!("{:.2}", feet);
                let trimmed = s.trim_end_matches('0').trim_end_matches('.');
                format!("{}'", trimmed)
            }
        }
    }
}

// ============================================================================
// GrandOrgue Sample Loader
// ============================================================================

use std::sync::Arc;
use crate::{SampleCache, SampleData, SampleError, SampleRef, load_wav};
use vpo_core::SampleRate;

/// Loaded organ with all samples ready to play
pub struct LoadedOrgan {
    /// Original organ definition
    pub definition: OrganDefinition,
    /// Loaded samples indexed by (stop_id, pipe_num)
    pub samples: HashMap<(u32, u32), SampleRef>,
    /// Sample rate
    pub sample_rate: SampleRate,
}

impl LoadedOrgan {
    /// Get sample for a stop and pipe
    pub fn get_sample(&self, stop_id: u32, pipe_num: u32) -> Option<&SampleRef> {
        self.samples.get(&(stop_id, pipe_num))
    }

    /// Get stop info
    pub fn get_stop(&self, stop_id: u32) -> Option<&StopDef> {
        self.definition.get_stop(stop_id)
    }

    /// Get manual info
    pub fn get_manual(&self, manual_num: u32) -> Option<&ManualDef> {
        self.definition.get_manual(manual_num)
    }

    /// Get number of loaded samples
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Get total memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.samples.values()
            .map(|s| s.bytes())
            .sum()
    }
}

/// Progress callback type
pub type LoadProgressFn = Box<dyn Fn(usize, usize, &str) + Send + Sync>;

/// Loader for GrandOrgue sample sets
pub struct GrandOrgueLoader {
    /// Sample cache
    cache: Arc<SampleCache>,
    /// Target sample rate
    sample_rate: SampleRate,
}

impl GrandOrgueLoader {
    /// Create a new loader
    pub fn new(cache: Arc<SampleCache>, sample_rate: SampleRate) -> Self {
        Self { cache, sample_rate }
    }

    /// Load an organ from an ODF file
    pub fn load(&self, odf_path: &Path) -> Result<LoadedOrgan, OdfError> {
        self.load_with_progress(odf_path, None)
    }

    /// Load an organ with progress callback
    pub fn load_with_progress(
        &self,
        odf_path: &Path,
        progress: Option<LoadProgressFn>,
    ) -> Result<LoadedOrgan, OdfError> {
        // Parse the ODF
        info!("Parsing ODF: {:?}", odf_path);
        let definition = OrganDefinition::load(odf_path)?;

        // Collect all samples to load (resolving references)
        let mut samples_to_load: Vec<(u32, u32, PathBuf)> = Vec::new();

        for stop in &definition.stops {
            for (pipe_idx, pipe) in stop.pipes.iter().enumerate() {
                let pipe_num = (pipe_idx + 1) as u32;

                // Resolve references to get actual sample path
                if let Some(resolved) = definition.resolve_reference(pipe) {
                    if let PipeDef::Sample { path, .. } = resolved {
                        samples_to_load.push((stop.id, pipe_num, path.clone()));
                    }
                }
            }
        }

        let total = samples_to_load.len();
        info!("Loading {} samples for organ: {}", total, definition.organ.church_name);

        // Load all samples in parallel using rayon
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let loaded_count = AtomicUsize::new(0);
        let failed_count = AtomicUsize::new(0);
        let sample_rate = self.sample_rate;

        let results: Vec<_> = samples_to_load
            .par_iter()
            .map(|(stop_id, pipe_num, path)| {
                // Load sample
                match Self::load_sample_static(path, sample_rate) {
                    Ok(sample) => {
                        loaded_count.fetch_add(1, Ordering::Relaxed);
                        Some((*stop_id, *pipe_num, sample))
                    }
                    Err(e) => {
                        warn!("Failed to load sample {:?}: {}", path, e);
                        failed_count.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                }
            })
            .collect();

        // Collect results into HashMap
        let mut samples = HashMap::new();
        for result in results.into_iter().flatten() {
            samples.insert((result.0, result.1), result.2);
        }

        let loaded = loaded_count.load(Ordering::Relaxed);
        let failed = failed_count.load(Ordering::Relaxed);
        info!("Loaded {} samples ({} failed)", loaded, failed);

        Ok(LoadedOrgan {
            definition,
            samples,
            sample_rate: self.sample_rate,
        })
    }

    /// Load a single sample, using cache
    fn load_sample(&self, path: &Path) -> Result<SampleRef, OdfError> {
        Self::load_sample_static(path, self.sample_rate)
    }

    /// Static version of load_sample for parallel loading
    fn load_sample_static(path: &Path, sample_rate: SampleRate) -> Result<SampleRef, OdfError> {
        // Try loading via cache with empty base path since path is already absolute
        let sample = load_wav(path)
            .map_err(|e| OdfError::SampleNotFound(format!("{:?}: {}", path, e)))?;

        // Resample if needed
        let sample = if sample.sample_rate != sample_rate {
            crate::resample(&sample, sample_rate)
                .map_err(|e| OdfError::SampleNotFound(format!("Resample error: {}", e)))?
        } else {
            sample
        };

        Ok(Arc::new(sample))
    }
}

/// Convenience function to load a GrandOrgue organ
pub fn load_grandorgue_organ(
    odf_path: &Path,
    sample_rate: SampleRate,
) -> Result<LoadedOrgan, OdfError> {
    let cache = Arc::new(SampleCache::new("", sample_rate));
    let loader = GrandOrgueLoader::new(cache, sample_rate);
    loader.load(odf_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harmonic_to_footage() {
        // Octave pitches
        assert_eq!(OrganDefinition::harmonic_to_footage(8), "8'");
        assert_eq!(OrganDefinition::harmonic_to_footage(4), "16'");
        assert_eq!(OrganDefinition::harmonic_to_footage(16), "4'");
        assert_eq!(OrganDefinition::harmonic_to_footage(32), "2'");
        assert_eq!(OrganDefinition::harmonic_to_footage(64), "1'");
        // Mutations (quints/tierces) must not show "Nh"
        assert_eq!(OrganDefinition::harmonic_to_footage(24), "2 2/3'");
        assert_eq!(OrganDefinition::harmonic_to_footage(48), "1 1/3'");
        assert_eq!(OrganDefinition::harmonic_to_footage(40), "1 3/5'");
        assert_eq!(OrganDefinition::harmonic_to_footage(96), "2/3'");
        // Non-standard falls back to a derived decimal, not "Nh"
        assert_eq!(OrganDefinition::harmonic_to_footage(20), "3.2'");
    }

    #[test]
    fn test_is_noise_or_mechanical() {
        let mk = |name: &str, retune: bool| StopDef {
            id: 1,
            name: name.to_string(),
            harmonic_number: 8,
            pitch_correction: 0.0,
            pitch_tuning_cents: 0.0,
            number_of_pipes: 1,
            first_accessible_pipe_logical_key: 1,
            windchest_group: 1,
            amplitude_level: 100.0,
            gain_db: 0.0,
            percussive: false,
            accepts_retuning: retune,
            first_midi_note: None,
            pipes: Vec::new(),
            layers: Vec::new(),
            primary_perspective: None,
        };
        // Real registers are kept.
        assert!(!mk("Prestant 8'", true).is_noise_or_mechanical());
        assert!(!mk("Kwinta 1 1/3'", true).is_noise_or_mechanical());
        // Noise/mechanical names are filtered.
        assert!(mk("Stop attack noise Flet kryty 8'", true).is_noise_or_mechanical());
        assert!(mk("Key action Manual 1 Release", true).is_noise_or_mechanical());
        assert!(mk("Blower", true).is_noise_or_mechanical());
        assert!(mk("Ambient noise", true).is_noise_or_mechanical());
        // Unpitched recording (AcceptsRetuning=N) is filtered regardless of name.
        assert!(mk("Whatever", false).is_noise_or_mechanical());
    }

    /// Testhelper: schrijf een mini-ODF naar een uniek tempbestand, parse hem
    /// en ruim het bestand weer op. `name` moet per test uniek zijn zodat
    /// parallel draaiende tests elkaar niet in de weg zitten.
    fn parse_odf_str(name: &str, content: &str) -> OrganDefinition {
        let path = std::env::temp_dir().join(format!(
            "vpo_go_test_{}_{}.organ",
            name,
            std::process::id()
        ));
        fs::write(&path, content).expect("kan test-ODF niet schrijven");
        let result = OdfParser::parse(&path);
        let _ = fs::remove_file(&path);
        result.expect("test-ODF moet parsen")
    }

    #[test]
    fn test_parse_release_samples() {
        // Inline-pad: releases met MaxKeyPressTime, CuePoint en ReleaseEnd,
        // plus [Organ] Gain (taak 1).
        let organ = parse_odf_str("releases", "\
[Organ]
ChurchName=Testkerk
NumberOfManuals=1
Gain=14
[Manual001]
Name=Manuaal
NumberOfStops=1
Stop001=001
[Stop001]
Name=Prestant 8'
NumberOfLogicalPipes=1
Pipe001=samples\\036-c.wav
Pipe001ReleaseCount=2
Pipe001Release001=rel\\short\\036-c.wav
Pipe001Release001MaxKeyPressTime=500
Pipe001Release001CuePoint=1000
Pipe001Release001ReleaseEnd=48000
Pipe001Release002=rel\\long\\036-c.wav
Pipe001Release002MaxKeyPressTime=-1
");
        // [Organ] Gain in dB
        assert_eq!(organ.organ.gain_db, 14.0);

        let stop = organ.get_stop(1).expect("stop 1 moet bestaan");
        let PipeDef::Sample { extra, .. } = &stop.pipes[0] else {
            panic!("pijp 1 moet een Sample zijn");
        };
        assert_eq!(extra.releases.len(), 2);

        // Korte release: alle attributen aanwezig
        let r1 = &extra.releases[0];
        assert!(r1.path.ends_with("rel/short/036-c.wav"));
        assert_eq!(r1.max_key_press_time_ms, Some(500));
        assert_eq!(r1.cue_point, Some(1000));
        assert_eq!(r1.release_end, Some(48000));

        // Lange release: MaxKeyPressTime=-1 (= default/langste), rest afwezig
        let r2 = &extra.releases[1];
        assert!(r2.path.ends_with("rel/long/036-c.wav"));
        assert_eq!(r2.max_key_press_time_ms, Some(-1));
        assert_eq!(r2.cue_point, None);
        assert_eq!(r2.release_end, None);
    }

    #[test]
    fn test_parse_release_samples_rank_path() {
        // Rank-pad: dezelfde release-sleutels maar dan in de [Rank]-sectie.
        let organ = parse_odf_str("releases_rank", "\
[Organ]
ChurchName=Testkerk
NumberOfManuals=1
[Manual001]
Name=Manuaal
NumberOfStops=1
Stop001=001
[Stop001]
Name=Bourdon 16'
NumberOfRanks=1
Rank001=001
NumberOfAccessiblePipes=1
[Rank001]
Name=Bourdon 16'
NumberOfLogicalPipes=1
Pipe001=samples\\036-c.wav
Pipe001ReleaseCount=1
Pipe001Release001=rel\\036-c.wav
Pipe001Release001MaxKeyPressTime=-1
Pipe001Percussive=Y
Pipe001LoopCount=1
Pipe001Loop001Start=4410
Pipe001Loop001End=88200
");
        let stop = organ.get_stop(1).expect("stop 1 moet bestaan");
        let PipeDef::Sample { extra, .. } = &stop.pipes[0] else {
            panic!("pijp 1 moet een Sample zijn");
        };
        // Release uit de rank-sectie
        assert_eq!(extra.releases.len(), 1);
        assert!(extra.releases[0].path.ends_with("rel/036-c.wav"));
        assert_eq!(extra.releases[0].max_key_press_time_ms, Some(-1));
        // Per-pipe Percussive en ODF-looppunten (taak 3 en 4, rank-pad).
        // GrandOrgue: LoopEnd is de laatste frame ínclusief → de parser levert
        // het EXCLUSIEVE einde (88200 + 1), consistent met de engine-semantiek.
        assert_eq!(extra.percussive, Some(true));
        assert_eq!(extra.loop_start, Some(4410));
        assert_eq!(extra.loop_end, Some(88201));
    }

    #[test]
    fn test_parse_pitch_tuning_float() {
        // PitchTuning is een float in cents (bv. "4.79062" uit GreenPositiv);
        // een integer-parse zou dit stilletjes laten vallen.
        let organ = parse_odf_str("pitch_tuning", "\
[Organ]
ChurchName=Testkerk
NumberOfManuals=1
[Manual001]
Name=Manuaal
NumberOfStops=1
Stop001=001
[Stop001]
Name=Flet kryty 8'
NumberOfLogicalPipes=2
Pipe001=samples\\036-c.wav
Pipe001PitchTuning=4.79062
Pipe001Percussive=N
Pipe002=samples\\037-c#.wav
Pipe002PitchTuning=-7.48608
");
        let stop = organ.get_stop(1).expect("stop 1 moet bestaan");
        let PipeDef::Sample { extra: e1, .. } = &stop.pipes[0] else {
            panic!("pijp 1 moet een Sample zijn");
        };
        let PipeDef::Sample { extra: e2, .. } = &stop.pipes[1] else {
            panic!("pijp 2 moet een Sample zijn");
        };
        assert!((e1.pitch_tuning_cents.unwrap() - 4.79062).abs() < 1e-4);
        assert!((e2.pitch_tuning_cents.unwrap() - -7.48608).abs() < 1e-4);
        // Per-pipe Percussive=N is Some(false); afwezig is None
        assert_eq!(e1.percussive, Some(false));
        assert_eq!(e2.percussive, None);
        // Geen loops of releases opgegeven
        assert_eq!(e1.loop_start, None);
        assert!(e1.releases.is_empty());
    }

    #[test]
    fn test_parse_pitch_metadata_rank_path() {
        // Hertemperen: PitchCorrection wordt over Organ+Windchest+Rank+Pipe
        // gesommeerd (rank+pijp in de extra; organ/windchest apart), de rank-
        // FirstMidiNoteNumber levert key_midi_note per pijp, en per pijp
        // MIDIKeyNumber/MIDIPitchFraction/HarmonicNumber (rank-default).
        let organ = parse_odf_str("pitch_metadata", "\
[Organ]
ChurchName=Testkerk
NumberOfManuals=1
NumberOfWindchestGroups=1
PitchCorrection=1
[WindchestGroup001]
Name=Main
PitchCorrection=2
[Manual001]
Name=Manuaal
NumberOfStops=1
Stop001=001
[Stop001]
Name=Prestant 8'
NumberOfRanks=1
Rank001=001
HarmonicNumber=8
[Rank001]
Name=Prestant 8'
FirstMidiNoteNumber=48
PitchCorrection=3
HarmonicNumber=8
NumberOfLogicalPipes=3
Pipe001=samples\\048-c.wav
Pipe001PitchCorrection=4
Pipe001MIDIKeyNumber=48
Pipe001MIDIPitchFraction=12.5
Pipe001HarmonicNumber=16
Pipe002=samples\\049-cs.wav
Pipe003=samples\\050-d.wav
Pipe003MIDIKeyNumber=-1
Pipe003MIDIPitchFraction=-1
");
        assert!((organ.organ.pitch_correction_cents - 1.0).abs() < 1e-6);
        assert!((organ.windchests[0].pitch_correction_cents - 2.0).abs() < 1e-6);
        let stop = organ.get_stop(1).expect("stop 1 moet bestaan");
        let PipeDef::Sample { extra: e1, .. } = &stop.pipes[0] else { panic!("pijp 1 moet een Sample zijn") };
        let PipeDef::Sample { extra: e2, .. } = &stop.pipes[1] else { panic!("pijp 2 moet een Sample zijn") };
        let PipeDef::Sample { extra: e3, .. } = &stop.pipes[2] else { panic!("pijp 3 moet een Sample zijn") };
        // Rank (3) + pijp (4) = 7; pijp 2 alleen rank (3).
        assert!((e1.pitch_correction_cents.unwrap() - 7.0).abs() < 1e-5);
        assert!((e2.pitch_correction_cents.unwrap() - 3.0).abs() < 1e-5);
        assert_eq!(e1.key_midi_note, Some(48));
        assert_eq!(e2.key_midi_note, Some(49));
        assert_eq!(e3.key_midi_note, Some(50));
        assert_eq!(e1.midi_key_number, Some(48));
        assert!((e1.midi_pitch_fraction.unwrap() - 12.5).abs() < 1e-5);
        assert_eq!(e1.harmonic_number, Some(16));
        // Rank-HarmonicNumber als default; -1-waarden vallen weg.
        assert_eq!(e2.harmonic_number, Some(8));
        assert_eq!(e2.midi_key_number, None);
        assert_eq!(e2.midi_pitch_fraction, None);
        assert_eq!(e3.midi_key_number, None);
        assert_eq!(e3.midi_pitch_fraction, None);
        // PitchTuning blijft neutraal (0) door PitchCorrection.
        assert!(e1.pitch_tuning_cents.unwrap_or(0.0).abs() < 1e-6);
    }

    #[test]
    fn test_resolve_reference_owner() {
        // Octaaf 4' leent via REF van Prestant 8': de eigenaar-resolutie moet
        // de Prestant-stop + 0-based index van de verwezen pijp teruggeven.
        let organ = parse_odf_str("ref_owner", "\
[Organ]
ChurchName=Testkerk
NumberOfManuals=1
[Manual001]
Name=Manuaal
NumberOfStops=2
Stop001=001
Stop002=002
[Stop001]
Name=Prestant 8'
NumberOfLogicalPipes=2
Pipe001=samples\\036-c.wav
Pipe002=samples\\037-cs.wav
[Stop002]
Name=Octaaf 4'
HarmonicNumber=16
NumberOfLogicalPipes=1
Pipe001=REF:001:001:002
");
        let octaaf = organ.get_stop(2).unwrap();
        let (owner, idx, pd) = organ
            .resolve_reference_owner(octaaf, 0, &octaaf.pipes[0])
            .expect("REF moet oplossen");
        assert_eq!(owner.id, 1);
        assert_eq!(idx, 1);
        assert!(matches!(pd, PipeDef::Sample { path, .. } if path.ends_with("037-cs.wav")));
        // Directe sample: eigenaar = de stop zelf, index ongewijzigd.
        let prestant = organ.get_stop(1).unwrap();
        let (o2, i2, _) = organ.resolve_reference_owner(prestant, 1, &prestant.pipes[1]).unwrap();
        assert_eq!((o2.id, i2), (1, 1));
    }

    /// Mini-ODF met één stop van twee ranks; `rank2_first_key` = de
    /// RankNNNFirstAccessibleKeyNumber van Rank002, `names` = de Rank-namen.
    fn stacked_odf(name: &str, rank2_first_key: u32, rank2_count: u32, names: (&str, &str)) -> OrganDefinition {
        parse_odf_str(name, &format!("\
[Organ]
ChurchName=Testkerk
NumberOfManuals=1
[Manual001]
Name=Manuaal
NumberOfStops=1
Stop001=001
[Stop001]
Name=Gestapeld
NumberOfRanks=2
Rank001=001
Rank001PipeCount=3
Rank001FirstAccessibleKeyNumber=1
Rank002=002
Rank002PipeCount={count}
Rank002FirstAccessibleKeyNumber={first}
[Rank001]
Name={n1}
NumberOfLogicalPipes=3
Pipe001=r1\\036-c.wav
Pipe002=r1\\037-cs.wav
Pipe003=r1\\038-d.wav
[Rank002]
Name={n2}
NumberOfLogicalPipes=3
Pipe001=r2\\036-c.wav
Pipe002=r2\\037-cs.wav
Pipe003=r2\\038-d.wav
", count = rank2_count, first = rank2_first_key, n1 = names.0, n2 = names.1))
    }

    #[test]
    fn test_stacked_ranks_worden_lagen() {
        // Twee ranks over dezelfde toetsen → Rank002 wordt laag 1, met zijn
        // eigen paden; laag 0 houdt Rank001.
        let organ = stacked_odf("stacked", 1, 3, ("Mixtur rank 1", "Mixtur rank 2"));
        let stop = organ.get_stop(1).unwrap();
        assert_eq!(stop.pipes.len(), 3);
        assert_eq!(stop.number_of_pipes, 3);
        assert_eq!(stop.layers.len(), 1);
        assert_eq!(stop.layers[0].index, 1);
        assert_eq!(stop.layers[0].name, "Mixtur rank 2");
        assert_eq!(stop.layers[0].pipes.len(), 3);
        let PipeDef::Sample { path, .. } = &stop.layers[0].pipes[1] else { panic!("laag 1 pijp 2 moet Sample zijn") };
        assert!(path.ends_with("r2/037-cs.wav"), "{path:?}");
        let PipeDef::Sample { path: p0, .. } = &stop.pipes[1] else { panic!("laag 0 pijp 2 moet Sample zijn") };
        assert!(p0.ends_with("r1/037-cs.wav"), "{p0:?}");
        // all_layers levert beide lagen in volgorde.
        let idx: Vec<u8> = stop.all_layers().map(|(i, _, _)| i).collect();
        assert_eq!(idx, vec![0, 1]);
        // get_sample_paths ziet ook de laag-bestanden.
        assert_eq!(organ.get_sample_paths().len(), 6);
    }

    #[test]
    fn test_splice_geeft_geen_lagen() {
        // GreenPositiv-vorm: Rank002 begint op toets 4 → disjunct → geen lagen,
        // dichte lijst van 6 pijpen.
        let organ = stacked_odf("splice", 4, 3, ("Kwinta", "Kwinta rep"));
        let stop = organ.get_stop(1).unwrap();
        assert!(stop.layers.is_empty());
        assert_eq!(stop.pipes.len(), 6);
        assert_eq!(stop.primary_perspective, None);
        let PipeDef::Sample { path, .. } = &stop.pipes[3] else { panic!("toets 4 moet Sample zijn") };
        assert!(path.ends_with("r2/036-c.wav"), "{path:?}");
    }

    #[test]
    fn test_deeloverlap_blijft_een_rank() {
        // Rank002 op toets 2..=4 overlapt gedeeltelijk met Rank001 (1..=3):
        // rank-intact → ÁL zijn pijpen in laag 1 (ook toets 4, die in laag 0
        // vrij was); laag 0 krijgt daar Empty en het bereik loopt tot toets 4.
        let organ = stacked_odf("deeloverlap", 2, 3, ("A", "B"));
        let stop = organ.get_stop(1).unwrap();
        assert_eq!(stop.layers.len(), 1);
        assert_eq!(stop.pipes.len(), 4);
        assert!(matches!(stop.pipes[3], PipeDef::Empty));
        let l = &stop.layers[0];
        assert_eq!(l.pipes.len(), 4);
        assert!(matches!(l.pipes[0], PipeDef::Empty));
        for k in 1..=3 {
            let PipeDef::Sample { path, .. } = &l.pipes[k] else { panic!("laag pijp {k} moet Sample zijn") };
            assert!(path.starts_with(organ.base_path.join("r2")) || path.to_string_lossy().contains("r2"), "{path:?}");
        }
        // Geen pijp van Rank002 in laag 0.
        for p in &stop.pipes {
            if let PipeDef::Sample { path, .. } = p {
                assert!(!path.to_string_lossy().contains("r2"), "Rank002-pijp in laag 0: {path:?}");
            }
        }
    }

    #[test]
    fn test_rank_naam_perspectief() {
        let organ = stacked_odf("persp", 1, 3, ("Principal 8' (front)", "Principal 8' (rear)"));
        let stop = organ.get_stop(1).unwrap();
        assert_eq!(stop.primary_perspective.as_deref(), Some("front"));
        assert_eq!(stop.layers[0].perspective.as_deref(), Some("rear"));
    }

    #[test]
    fn test_mixtuur_zonder_perspectief() {
        let organ = stacked_odf("mixtuur", 1, 3, ("Mixtur rank 1", "Mixtur rank 2"));
        let stop = organ.get_stop(1).unwrap();
        assert_eq!(stop.primary_perspective, None);
        assert_eq!(stop.layers[0].perspective, None);
        // Eén perspectief-naam in de PRIMAIRE rank alleen → geen fantoom.
        let organ2 = stacked_odf("fantoom", 1, 3, ("Principal 8' (front)", "Mixtur rank 2"));
        let s2 = organ2.get_stop(1).unwrap();
        assert_eq!(s2.primary_perspective, None);
    }

    #[test]
    fn test_ref_resolution_manual_relative() {
        // REF:mm:ss:pp gebruikt de MANUAL-RELATIEVE stopindex: REF:001:002:001
        // moet de TWEEDE stop uit de stoplijst van manual 1 pakken (hier globale
        // stop-id 4), niet de stop met globale id 2 (die bij manual 2 hoort).
        let organ = parse_odf_str("ref_manual_relative", "\
[Organ]
ChurchName=Testkerk
NumberOfManuals=2
[Manual001]
Name=Hoofdwerk
NumberOfStops=2
Stop001=003
Stop002=004
[Manual002]
Name=Zwelwerk
NumberOfStops=2
Stop001=001
Stop002=002
[Stop001]
Name=Zwelstop A
NumberOfLogicalPipes=1
Pipe001=zwel_a\\036-c.wav
[Stop002]
Name=Zwelstop B
NumberOfLogicalPipes=1
Pipe001=zwel_b\\036-c.wav
[Stop003]
Name=Hoofdstop A
NumberOfLogicalPipes=1
Pipe001=hoofd_a\\036-c.wav
[Stop004]
Name=Hoofdstop B
NumberOfLogicalPipes=1
Pipe001=hoofd_b\\036-c.wav
[Stop005]
Name=Koppelstop
NumberOfLogicalPipes=1
Pipe001=REF:001:002:001
");
        // De REF verwijst naar manual 1, stopindex 2 => globale stop-id 4
        // ("Hoofdstop B"), dus hoofd_b — NIET zwel_b (globale stop-id 2).
        let stop = organ.get_stop(5).expect("stop 5 moet bestaan");
        let resolved = organ
            .resolve_reference(&stop.pipes[0])
            .expect("REF moet resolven");
        let PipeDef::Sample { path, .. } = resolved else {
            panic!("REF moet naar een Sample resolven");
        };
        assert!(
            path.ends_with("hoofd_b/036-c.wav"),
            "REF:001:002:001 resolvde naar {:?} maar moet de tweede stop van manual 1 zijn",
            path
        );
    }
}
