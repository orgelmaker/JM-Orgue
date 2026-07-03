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
    /// Pitch correction in cents
    pub pitch_correction: i32,
    pub number_of_pipes: u32,
    pub first_accessible_pipe_logical_key: u32,
    pub windchest_group: u32,
    pub amplitude_level: f32,
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
    /// Pipe definitions (path or reference)
    pub pipes: Vec<PipeDef>,
}

impl StopDef {
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

/// Pipe definition - either a direct sample path or a reference
#[derive(Debug, Clone)]
pub enum PipeDef {
    /// Direct path to WAV file
    Sample {
        path: PathBuf,
        amplitude_level: Option<f32>,
        pitch_correction: Option<i32>,
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

/// Windchest group definition
#[derive(Debug, Clone)]
pub struct WindchestDef {
    pub number: u32,
    pub comment: String,
    pub enclosure_ids: Vec<u32>,
    pub tremulant_ids: Vec<u32>,
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
                    let (pipes, harmonic) = match self.parse_stop_ranks(section) {
                        Some((rank_pipes, rank_harmonic)) => {
                            let h = self.parse_u32(section, "HarmonicNumber")
                                .unwrap_or(if rank_harmonic > 0 { rank_harmonic } else { 8 });
                            (rank_pipes, h)
                        }
                        None => {
                            let num_pipes = self.parse_u32(section, "NumberOfLogicalPipes")
                                .or_else(|| self.parse_u32(section, "NumberOfAccessiblePipes"))
                                .unwrap_or(0);
                            let inline = self.parse_pipes(section, num_pipes)?;
                            (inline, self.parse_u32(section, "HarmonicNumber").unwrap_or(8))
                        }
                    };

                    stops.push(StopDef {
                        id,
                        name: section.get("Name").cloned().unwrap_or_default(),
                        harmonic_number: harmonic,
                        pitch_correction: self.parse_i32(section, "PitchCorrection").unwrap_or(0),
                        number_of_pipes: pipes.len() as u32,
                        first_accessible_pipe_logical_key: self.parse_u32(section, "FirstAccessiblePipeLogicalKeyNumber").unwrap_or(1),
                        windchest_group: self.parse_u32(section, "WindchestGroup").unwrap_or(1),
                        amplitude_level: self.parse_f32(section, "AmplitudeLevel").unwrap_or(100.0),
                        percussive: section.get("Percussive").map(|v| v == "Y").unwrap_or(false),
                        accepts_retuning: section.get("AcceptsRetuning").map(|v| v != "N").unwrap_or(true),
                        first_midi_note: None,
                        pipes,
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
    /// stop can use a slice of a shared rank. Returns the pipes (in keyboard order)
    /// and the detected harmonic number, or None if the stop has no rank refs
    /// (older inline-pipe format — handled by parse_pipes instead).
    fn parse_stop_ranks(&self, stop_section: &HashMap<String, String>) -> Option<(Vec<PipeDef>, u32)> {
        let num_ranks = self.parse_u32(stop_section, "NumberOfRanks")?;
        if num_ranks == 0 {
            return None;
        }

        let accessible = self.parse_u32(stop_section, "NumberOfAccessiblePipes").unwrap_or(0);
        let mut pipes = Vec::new();
        let mut harmonic = 0u32;

        for r in 1..=num_ranks {
            let rank_id = match self.parse_u32(stop_section, &format!("Rank{:03}", r)) {
                Some(v) => v,
                None => continue,
            };
            let rank_section = match self.sections.get(&format!("Rank{:03}", rank_id)) {
                Some(s) => s,
                None => continue,
            };
            let first_pipe = self
                .parse_u32(stop_section, &format!("Rank{:03}FirstPipeNumber", r))
                .unwrap_or(1);
            let count = self
                .parse_u32(stop_section, &format!("Rank{:03}PipeCount", r))
                .or_else(|| self.parse_u32(rank_section, "NumberOfLogicalPipes"))
                .unwrap_or(accessible);

            for i in 0..count {
                let key = format!("Pipe{:03}", first_pipe + i);
                let value = match rank_section.get(&key) {
                    Some(v) => v,
                    None => break, // past the end of the rank
                };
                if harmonic == 0 {
                    if let Some(h) = self.parse_u32(rank_section, &format!("{}HarmonicNumber", key)) {
                        harmonic = h;
                    }
                }
                if let Some(rest) = value.strip_prefix("REF:") {
                    let parts: Vec<&str> = rest.split(':').collect();
                    if parts.len() == 3 {
                        pipes.push(PipeDef::Reference {
                            manual: parts[0].parse().unwrap_or(0),
                            stop: parts[1].parse().unwrap_or(0),
                            pipe: parts[2].parse().unwrap_or(0),
                        });
                    }
                } else {
                    pipes.push(PipeDef::Sample {
                        path: self.resolve_path(value),
                        amplitude_level: self.parse_f32(rank_section, &format!("{}Gain", key)),
                        pitch_correction: self.parse_i32(rank_section, &format!("{}PitchCorrection", key)),
                    });
                }
            }
        }

        if pipes.is_empty() {
            None
        } else {
            Some((pipes, harmonic))
        }
    }

    /// Parse pipe definitions for a stop
    fn parse_pipes(&self, section: &HashMap<String, String>, num_pipes: u32) -> Result<Vec<PipeDef>, OdfError> {
        let mut pipes = Vec::new();

        for i in 1..=num_pipes {
            let key = format!("Pipe{:03}", i);
            if let Some(value) = section.get(&key) {
                let pipe = if value.starts_with("REF:") {
                    // Reference format: REF:manual:stop:pipe
                    let parts: Vec<&str> = value[4..].split(':').collect();
                    if parts.len() == 3 {
                        PipeDef::Reference {
                            manual: parts[0].parse().unwrap_or(0),
                            stop: parts[1].parse().unwrap_or(0),
                            pipe: parts[2].parse().unwrap_or(0),
                        }
                    } else {
                        return Err(OdfError::InvalidReference(value.clone()));
                    }
                } else {
                    // Sample path
                    let path = self.resolve_path(value);
                    let amp_key = format!("Pipe{:03}AmplitudeLevel", i);
                    let pitch_key = format!("Pipe{:03}PitchCorrection", i);

                    PipeDef::Sample {
                        path,
                        amplitude_level: self.parse_f32(section, &amp_key),
                        pitch_correction: self.parse_i32(section, &pitch_key),
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
            for pipe in &stop.pipes {
                if let PipeDef::Sample { path, .. } = pipe {
                    if !paths.contains(path) {
                        paths.push(path.clone());
                    }
                }
            }
        }

        paths
    }

    /// Resolve a pipe reference to its actual sample path
    pub fn resolve_reference<'a>(&'a self, reference: &'a PipeDef) -> Option<&'a PipeDef> {
        match reference {
            PipeDef::Sample { .. } => Some(reference),
            PipeDef::Empty => None,
            PipeDef::Reference { manual: _, stop, pipe } => {
                // Find the referenced stop
                self.stops.iter()
                    .find(|s| s.id == *stop)
                    .and_then(|s| s.pipes.get(*pipe as usize - 1))
                    .and_then(|p| self.resolve_reference(p))
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
            .map(|s| s.data.len() * std::mem::size_of::<f32>())
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
            pitch_correction: 0,
            number_of_pipes: 1,
            first_accessible_pipe_logical_key: 1,
            windchest_group: 1,
            amplitude_level: 100.0,
            percussive: false,
            accepts_retuning: retune,
            first_midi_note: None,
            pipes: Vec::new(),
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
}
