//! Custom organ definition and builder
//!
//! This module allows users to create their own sample sets by
//! combining samples from various sources.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};

/// A custom organ definition that can be saved/loaded
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomOrgan {
    /// Unique ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Builder info (e.g., "Custom", or user name)
    pub builder: String,
    /// Location (virtual/custom)
    pub location: String,
    /// Version of the format
    pub version: u32,
    /// Divisions (manuals/keyboards)
    pub divisions: Vec<CustomDivision>,
}

/// A division (manual/keyboard) in the custom organ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomDivision {
    /// Unique ID within the organ
    pub id: String,
    /// Display name (e.g., "Hoofdwerk", "Pedaal")
    pub name: String,
    /// MIDI channel (None = all channels)
    pub midi_channel: Option<u8>,
    /// First MIDI note
    pub first_midi_note: u8,
    /// Last MIDI note
    pub last_midi_note: u8,
    /// Stops in this division
    pub stops: Vec<CustomStop>,
}

/// A stop in the custom organ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomStop {
    /// Unique ID within the organ
    pub id: String,
    /// Display name (e.g., "Prestant 8'")
    pub name: String,
    /// Pitch in feet (e.g., 8.0, 4.0, 2.666)
    pub pitch_feet: f32,
    /// Optional custom pitch/rank label shown instead of "{feet}'".
    /// Used for "sterk"/rank stops, e.g. "4st" for "Mixtuur 4 sterk".
    #[serde(default)]
    pub pitch_label: Option<String>,
    /// Stop family for coloring
    pub family: StopFamily,
    /// Pipes/ranks for this stop (dry samples)
    pub pipes: Vec<CustomPipe>,
    /// Tremulant variant pipes (from _trem folders)
    /// When tremulant is active, these are crossfaded with the dry pipes
    #[serde(default)]
    pub tremulant_pipes: Vec<CustomPipe>,
    /// Whether this stop has a tremulant variant available
    #[serde(default)]
    pub has_tremulant: bool,
}

/// Stop family for UI coloring
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StopFamily {
    Principal,
    Flute,
    String,
    Reed,
    Mixture,
    Coupler,
    Tremulant,
}

impl Default for StopFamily {
    fn default() -> Self {
        Self::Principal
    }
}

/// A single pipe (sample) in a stop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPipe {
    /// MIDI note number
    pub midi_note: u8,
    /// Path to the sample file (relative to the organ file)
    pub sample_path: PathBuf,
    /// Loop start position (in samples)
    pub loop_start: Option<u64>,
    /// Loop end position (in samples)
    pub loop_end: Option<u64>,
    /// Volume adjustment in dB
    pub volume_db: f32,
    /// Pitch adjustment in cents
    pub pitch_cents: i16,
}

impl CustomOrgan {
    /// Create a new empty custom organ
    pub fn new(name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: None,
            builder: "Custom".to_string(),
            location: "Virtual".to_string(),
            version: 1,
            divisions: Vec::new(),
        }
    }

    /// Add a division
    pub fn add_division(&mut self, name: &str, first_note: u8, last_note: u8) -> &mut CustomDivision {
        let id = format!("div_{}", self.divisions.len());
        self.divisions.push(CustomDivision {
            id,
            name: name.to_string(),
            midi_channel: None,
            first_midi_note: first_note,
            last_midi_note: last_note,
            stops: Vec::new(),
        });
        self.divisions.last_mut().unwrap()
    }

    /// Save to a file
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Load from a file
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    /// Get total number of stops
    pub fn stop_count(&self) -> usize {
        self.divisions.iter().map(|d| d.stops.len()).sum()
    }

    /// Get total number of pipes
    pub fn pipe_count(&self) -> usize {
        self.divisions.iter()
            .flat_map(|d| d.stops.iter())
            .map(|s| s.pipes.len())
            .sum()
    }

    /// Export as .organ definition file (INI format compatible with JM-Orgue)
    pub fn export_organ_file(&self, output_path: &Path) -> Result<(), std::io::Error> {
        use std::fmt::Write;
        let mut out = String::new();

        // Count totals
        let has_pedal = self.divisions.iter().any(|d| {
            let n = d.name.to_lowercase();
            n.contains("pedaal") || n.contains("pedal")
        });
        let num_manuals = self.divisions.iter().filter(|d| {
            let n = d.name.to_lowercase();
            !(n.contains("pedaal") || n.contains("pedal"))
        }).count() as u32;
        let num_windchests = self.divisions.len() as u32;

        // Count tremulants
        let has_any_trem = self.divisions.iter()
            .flat_map(|d| d.stops.iter())
            .any(|s| s.has_tremulant);
        let num_tremulants = if has_any_trem { num_windchests } else { 0 };

        // [Organ] section
        writeln!(out, "; JM-Orgue Organ Definition File").unwrap();
        writeln!(out, "; Generated by JM-Orgue").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "[Organ]").unwrap();
        writeln!(out, "ChurchName={}", self.name).unwrap();
        writeln!(out, "ChurchAddress={}", self.location).unwrap();
        writeln!(out, "OrganBuilder={}", self.builder).unwrap();
        writeln!(out, "OrganBuildDate=").unwrap();
        writeln!(out, "OrganComments={}", self.description.as_deref().unwrap_or("")).unwrap();
        writeln!(out, "RecordingDetails=JM-Orgue sampleset").unwrap();
        writeln!(out, "NumberOfManuals={}", num_manuals).unwrap();
        writeln!(out, "HasPedals={}", if has_pedal { "Y" } else { "N" }).unwrap();
        writeln!(out, "NumberOfEnclosures=0").unwrap();
        writeln!(out, "NumberOfTremulants={}", num_tremulants).unwrap();
        writeln!(out, "NumberOfWindchestGroups={}", num_windchests).unwrap();
        writeln!(out, "AmplitudeLevel=100").unwrap();
        writeln!(out).unwrap();

        // Windchest groups (one per division)
        for (i, div) in self.divisions.iter().enumerate() {
            let wc_num = i + 1;
            writeln!(out, "[WindchestGroup{:03}]", wc_num).unwrap();
            writeln!(out, "Name={}", div.name).unwrap();
            writeln!(out, "NumberOfEnclosures=0").unwrap();
            let div_has_trem = div.stops.iter().any(|s| s.has_tremulant);
            if div_has_trem && has_any_trem {
                writeln!(out, "NumberOfTremulants=1").unwrap();
                writeln!(out, "Tremulant001={:03}", wc_num).unwrap();
            } else {
                writeln!(out, "NumberOfTremulants=0").unwrap();
            }
            writeln!(out).unwrap();
        }

        // Tremulants (one per division that has trem stops)
        if has_any_trem {
            for (i, div) in self.divisions.iter().enumerate() {
                let div_has_trem = div.stops.iter().any(|s| s.has_tremulant);
                if div_has_trem {
                    writeln!(out, "[Tremulant{:03}]", i + 1).unwrap();
                    writeln!(out, "Name=Tremulant {}", div.name).unwrap();
                    writeln!(out, "Period=160").unwrap();
                    writeln!(out, "AmpModDepth=18").unwrap();
                    writeln!(out, "StartRate=8").unwrap();
                    writeln!(out, "StopRate=8").unwrap();
                    writeln!(out).unwrap();
                }
            }
        }

        // Assign manual numbers: pedal = 000, manuals = 001, 002, ...
        let mut manual_numbers: Vec<u32> = Vec::new();
        let mut manual_counter = 1u32;
        for div in &self.divisions {
            let n = div.name.to_lowercase();
            if n.contains("pedaal") || n.contains("pedal") {
                manual_numbers.push(0);
            } else {
                manual_numbers.push(manual_counter);
                manual_counter += 1;
            }
        }

        // Build global stop ID mapping
        let mut global_stop_id = 1u32;
        let mut stop_ids_per_div: Vec<Vec<u32>> = Vec::new();
        for div in &self.divisions {
            let mut ids = Vec::new();
            for _ in &div.stops {
                ids.push(global_stop_id);
                global_stop_id += 1;
            }
            stop_ids_per_div.push(ids);
        }

        // Manual sections
        for (div_idx, div) in self.divisions.iter().enumerate() {
            let man_num = manual_numbers[div_idx];
            let first_note = div.first_midi_note as u32;
            let last_note = div.last_midi_note as u32;
            let num_keys = last_note.saturating_sub(first_note) + 1;

            writeln!(out, "[Manual{:03}]", man_num).unwrap();
            writeln!(out, "Name={}", div.name).unwrap();
            writeln!(out, "MIDIInputNumber={}", man_num + 1).unwrap();
            writeln!(out, "NumberOfLogicalKeys={}", num_keys).unwrap();
            writeln!(out, "NumberOfAccessibleKeys={}", num_keys).unwrap();
            writeln!(out, "FirstAccessibleKeyMIDINoteNumber={}", first_note).unwrap();

            // Stops
            let stop_ids = &stop_ids_per_div[div_idx];
            writeln!(out, "NumberOfStops={}", stop_ids.len()).unwrap();
            for (j, sid) in stop_ids.iter().enumerate() {
                writeln!(out, "Stop{:03}={:03}", j + 1, sid).unwrap();
            }

            // Couplers and tremulants (empty for now)
            writeln!(out, "NumberOfCouplers=0").unwrap();

            let div_has_trem = div.stops.iter().any(|s| s.has_tremulant);
            if div_has_trem && has_any_trem {
                writeln!(out, "NumberOfTremulants=1").unwrap();
                writeln!(out, "Tremulant001={:03}", div_idx + 1).unwrap();
            } else {
                writeln!(out, "NumberOfTremulants=0").unwrap();
            }
            writeln!(out).unwrap();
        }

        // Stop sections with pipe definitions
        for (div_idx, div) in self.divisions.iter().enumerate() {
            let wc_num = div_idx + 1;
            let stop_ids = &stop_ids_per_div[div_idx];

            for (stop_idx, stop) in div.stops.iter().enumerate() {
                let sid = stop_ids[stop_idx];

                // GrandOrgue-standaard: harmonic = 64 / voet (8' → 8). De oude
                // tabel schreef 32/voet, waardoor elke geëxporteerde stop na
                // herimport een verdubbelde voetmaat kreeg (auditbevinding 38).
                let harmonic = if stop.pitch_feet > 0.01 {
                    ((64.0 / stop.pitch_feet).round() as u32).max(1)
                } else {
                    8
                };

                // Pijpen op hun échte toets: gesorteerd op midi_note, gaten als
                // DUMMY, en FirstAccessiblePipeLogicalKeyNumber = positie van de
                // laagste pijp binnen het klavier. De oude export schreef alles
                // aaneengesloten vanaf toets 1 en gooide midi_note weg, waardoor
                // discant-registers en gaten bij herimport alle toetsen
                // verschoven (auditbevinding 16).
                let mut by_note: std::collections::BTreeMap<u32, &CustomPipe> = std::collections::BTreeMap::new();
                for pipe in &stop.pipes {
                    by_note.entry(pipe.midi_note as u32).or_insert(pipe);
                }
                let div_first = div.first_midi_note as u32;
                let stop_first = by_note.keys().next().copied().unwrap_or(div_first);
                let stop_last = by_note.keys().next_back().copied().unwrap_or(stop_first);
                let span = if by_note.is_empty() { 0 } else { stop_last - stop_first + 1 };
                let first_pipe_key = stop_first.saturating_sub(div_first) + 1;

                writeln!(out, "[Stop{:03}]", sid).unwrap();
                writeln!(out, "Name={}", stop.name).unwrap();
                writeln!(out, "NumberOfLogicalPipes={}", span).unwrap();
                writeln!(out, "NumberOfAccessiblePipes={}", span).unwrap();
                writeln!(out, "FirstAccessiblePipeLogicalKeyNumber={}", first_pipe_key).unwrap();
                writeln!(out, "WindchestGroup={:03}", wc_num).unwrap();
                writeln!(out, "Percussive=N").unwrap();
                writeln!(out, "HarmonicNumber={}", harmonic).unwrap();
                writeln!(out, "AmplitudeLevel=100").unwrap();

                // Pipe definitions: dicht bereik stop_first..=stop_last, gaten DUMMY.
                if !by_note.is_empty() {
                    for (pipe_idx, note) in (stop_first..=stop_last).enumerate() {
                        match by_note.get(&note) {
                            Some(pipe) => {
                                let path_str = pipe.sample_path.to_string_lossy().replace('\\', "/");
                                if pipe.volume_db.abs() > 0.1 || pipe.pitch_cents != 0 {
                                    // Include amplitude and pitch correction
                                    let amp = 100.0 * 10.0_f32.powf(pipe.volume_db / 20.0);
                                    write!(out, "Pipe{:03}={}:{:.0}", pipe_idx + 1, path_str, amp).unwrap();
                                    if pipe.pitch_cents != 0 {
                                        write!(out, ":{}", pipe.pitch_cents).unwrap();
                                    }
                                    writeln!(out).unwrap();
                                } else {
                                    writeln!(out, "Pipe{:03}={}", pipe_idx + 1, path_str).unwrap();
                                }
                            }
                            None => {
                                writeln!(out, "Pipe{:03}=DUMMY", pipe_idx + 1).unwrap();
                            }
                        }
                    }
                }
                writeln!(out).unwrap();
            }
        }

        std::fs::write(output_path, out)
    }
}

impl CustomDivision {
    /// Add a stop to this division
    pub fn add_stop(&mut self, name: &str, pitch_feet: f32, family: StopFamily) -> &mut CustomStop {
        let id = format!("{}_{}", self.id, self.stops.len());
        self.stops.push(CustomStop {
            id,
            name: name.to_string(),
            pitch_feet,
            pitch_label: None,
            family,
            pipes: Vec::new(),
            tremulant_pipes: Vec::new(),
            has_tremulant: false,
        });
        self.stops.last_mut().unwrap()
    }
}

impl CustomStop {
    /// Add a pipe to this stop
    pub fn add_pipe(&mut self, midi_note: u8, sample_path: PathBuf) {
        self.pipes.push(CustomPipe {
            midi_note,
            sample_path,
            loop_start: None,
            loop_end: None,
            volume_db: 0.0,
            pitch_cents: 0,
        });
    }

    /// Add a pipe with loop points
    pub fn add_pipe_with_loop(
        &mut self,
        midi_note: u8,
        sample_path: PathBuf,
        loop_start: u64,
        loop_end: u64,
    ) {
        self.pipes.push(CustomPipe {
            midi_note,
            sample_path,
            loop_start: Some(loop_start),
            loop_end: Some(loop_end),
            volume_db: 0.0,
            pitch_cents: 0,
        });
    }
}

/// Builder for creating custom organs interactively
pub struct CustomOrganBuilder {
    organ: CustomOrgan,
    base_path: PathBuf,
}

impl CustomOrganBuilder {
    /// Create a new builder
    pub fn new(name: &str, base_path: PathBuf) -> Self {
        Self {
            organ: CustomOrgan::new(name),
            base_path,
        }
    }

    /// Set the organ name
    pub fn name(mut self, name: &str) -> Self {
        self.organ.name = name.to_string();
        self
    }

    /// Set the builder info
    pub fn builder(mut self, builder: &str) -> Self {
        self.organ.builder = builder.to_string();
        self
    }

    /// Set the description
    pub fn description(mut self, description: &str) -> Self {
        self.organ.description = Some(description.to_string());
        self
    }

    /// Add a division with stops
    pub fn add_division<F>(mut self, name: &str, first_note: u8, last_note: u8, builder: F) -> Self
    where
        F: FnOnce(&mut CustomDivision),
    {
        let division = self.organ.add_division(name, first_note, last_note);
        builder(division);
        self
    }

    /// Build the organ
    pub fn build(self) -> CustomOrgan {
        self.organ
    }
}

/// Parse MIDI note from filename like "036-C.wav" or "037-C#.wav"
fn parse_midi_note_from_filename(filename: &str) -> Option<u8> {
    // Try to extract number from the beginning of filename
    // Pattern: "036-C.wav" or "036-C#.wav" or just "036.wav"
    let stem = filename.trim_end_matches(".wav")
        .trim_end_matches(".WAV")
        .trim_end_matches(".mp3")
        .trim_end_matches(".MP3");

    // Try to find a number at the start
    let num_str: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !num_str.is_empty() {
        if let Ok(note) = num_str.parse::<u8>() {
            return Some(note);
        }
    }

    None
}

/// Check if a file is an audio file (WAV or MP3)
fn is_audio_file(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        return ext_lower == "wav" || ext_lower == "mp3";
    }
    false
}

/// Parse stop name, pitch and optional rank-label from a directory name.
///
/// Handles:
/// - Underscores as separators: "Prestant_8" -> name "Prestant", 8'
/// - Plain footage: "Principal 8", "Gedackt8", "Bourdon_16"
/// - Rank ("sterk") stops: "Mixtuur_4st" / "Cornet 4 sterk" -> label "4st"
///
/// Returns: (name, pitch_feet, family, pitch_label)
/// `pitch_label` is `Some("4st")` for rank stops, otherwise `None`.
fn parse_stop_info(dir_name: &str) -> (String, f32, StopFamily, Option<String>) {
    // Underscores act as separators; collapse whitespace.
    let normalized: String = dir_name
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let (name, pitch, pitch_label) = parse_pitch_and_name(&normalized);

    // Detect stop family from name
    let lower = name.to_lowercase();
    let family = if lower.contains("fluit") || lower.contains("flöjt") || lower.contains("gedackt") ||
                    lower.contains("gedekt") || lower.contains("bourdon") || lower.contains("holpijp") ||
                    lower.contains("rohr") {
        StopFamily::Flute
    } else if lower.contains("viola") || lower.contains("gamba") || lower.contains("celeste") ||
              lower.contains("salicional") || lower.contains("voix") {
        StopFamily::String
    } else if lower.contains("trompet") || lower.contains("trumpet") || lower.contains("bazuin") ||
              lower.contains("hobo") || lower.contains("schalmei") || lower.contains("cromorne") ||
              lower.contains("fagot") || lower.contains("dulciaan") {
        StopFamily::Reed
    } else if lower.contains("mixtuur") || lower.contains("mixture") || lower.contains("cymbel") ||
              lower.contains("scherp") || lower.contains("sesquialter") || lower.contains("cornet") {
        StopFamily::Mixture
    } else if lower.contains("koppel") || lower.contains("coupler") {
        StopFamily::Coupler
    } else if lower.contains("tremulant") || lower.contains("tremolo") {
        StopFamily::Tremulant
    } else {
        StopFamily::Principal
    };

    // Fall back to the full (normalized) name if stripping left nothing.
    let name = if name.is_empty() { normalized.clone() } else { name };

    (name, pitch, family, pitch_label)
}

/// Split a normalized stop name into (name, pitch_feet, pitch_label).
///
/// Recognises a trailing footage number, optionally followed by a "sterk"/rank
/// marker ("st" or "sterk"). For rank stops the returned label is "Nst" and the
/// numeric value is kept in `pitch_feet` for sorting/harmonic purposes.
fn parse_pitch_and_name(normalized: &str) -> (String, f32, Option<String>) {
    let s = normalized.trim();
    if s.is_empty() {
        return (String::new(), 8.0, None);
    }

    let lower = s.to_lowercase();
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();

    // Detect a trailing rank marker ("sterk" or "st"); only valid if digits precede it.
    let (marker_len, is_ranks) = if lower.ends_with("sterk") {
        (5usize, true)
    } else if lower.ends_with("st") {
        (2usize, true)
    } else {
        (0usize, false)
    };

    let mut digits_end = n.saturating_sub(marker_len);
    // Allow whitespace between the number and a spelled-out marker, e.g. "4 sterk".
    while digits_end > 0 && chars[digits_end - 1].is_whitespace() {
        digits_end -= 1;
    }

    // Walk backwards over the digit run that precedes the marker / end.
    let mut i = digits_end;
    while i > 0 && chars[i - 1].is_ascii_digit() {
        i -= 1;
    }
    let digit_start = i;

    if digit_start >= digits_end {
        // No trailing number -> no pitch token (keep whole name, ignore any "st" suffix).
        return (s.to_string(), 8.0, None);
    }

    let num_str: String = chars[digit_start..digits_end].iter().collect();
    let feet: f32 = num_str.parse().unwrap_or(8.0);
    let name: String = chars[..digit_start].iter().collect::<String>().trim().to_string();
    let label = if is_ranks { Some(format!("{}st", num_str)) } else { None };

    (name, feet, label)
}

/// Check if a stop belongs to pedal division
fn is_pedal_stop(dir_name: &str) -> bool {
    let lower = dir_name.to_lowercase();
    lower.starts_with("ped") || lower.contains("pedal") || lower.contains("pedaal")
}

/// Detect if stop is bass (Bascant) or treble (Discant) based on sample range
/// Returns: (is_bass, is_treble)
/// - Bass stops typically have samples from C (36) to b (59)
/// - Treble stops typically have samples from c1 (60) to end
fn detect_bass_treble(pipes: &[CustomPipe]) -> (bool, bool) {
    if pipes.is_empty() {
        return (false, false);
    }

    let first_note = pipes.first().map(|p| p.midi_note).unwrap_or(60);
    let last_note = pipes.last().map(|p| p.midi_note).unwrap_or(60);

    // Bass: starts low (around C/36) and ends before or at c1 (60)
    let is_bass = first_note < 48 && last_note <= 62;

    // Treble: starts at or after c1 (60)
    let is_treble = first_note >= 58;

    (is_bass, is_treble)
}

/// A scanned stop with its note range
struct ScannedStop {
    stop: CustomStop,
    first_note: u8,
    last_note: u8,
    is_pedal: bool,
    is_bass: bool,
    is_treble: bool,
}

/// Which half of a (possibly) divided register a sample set represents.
#[derive(Debug, Clone, Copy, PartialEq)]
enum StopHalf {
    /// Full compass (single knob)
    Full,
    /// Bass half (Bas)
    Bass,
    /// Discant/treble half (Disc)
    Discant,
}

/// Classify a sub-folder name as the bass or discant half of a divided stop.
/// Returns `None` for anything that is not clearly a bass/discant marker
/// (e.g. multi-microphone position folders).
fn classify_half(sub_name: &str) -> Option<StopHalf> {
    let l = sub_name.to_lowercase();
    if l.ends_with("bass") || l.ends_with("bas") {
        Some(StopHalf::Bass)
    } else if l.ends_with("discant") || l.ends_with("disc") || l.ends_with("dis") {
        Some(StopHalf::Discant)
    } else {
        None
    }
}

/// Determine the sample variants inside a stop folder.
///
/// Priority:
/// 1. Audio files directly in the folder -> one `Full` variant.
/// 2. `*_bas` / `*_dis` sub-folders -> separate `Bass` / `Discant` variants
///    (this is how bass/discant-divided registers are stored).
/// 3. Otherwise treat sub-folders as multi-microphone positions and use the
///    first one alphabetically as a single `Full` variant.
fn scan_stop_variants(
    stop_path: &Path,
    base_dir: &Path,
) -> Result<Vec<(StopHalf, Vec<CustomPipe>)>, std::io::Error> {
    // 1. Direct audio files -> full-compass stop.
    let direct = scan_audio_files(stop_path, base_dir)?;
    if !direct.is_empty() {
        return Ok(vec![(StopHalf::Full, direct)]);
    }

    // 2. Look for bass/discant sub-folders.
    let mut subdirs: Vec<_> = std::fs::read_dir(stop_path)?
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();
    subdirs.sort_by_key(|e| e.file_name());

    let mut bass: Vec<CustomPipe> = Vec::new();
    let mut disc: Vec<CustomPipe> = Vec::new();

    for sub in &subdirs {
        let sub_path = sub.path();
        let sub_name = sub.file_name().to_string_lossy().to_string();
        if sub_name.starts_with('.') {
            continue;
        }
        match classify_half(&sub_name) {
            Some(StopHalf::Bass) if bass.is_empty() => {
                bass = scan_audio_files_with_multimic(&sub_path, base_dir)?;
            }
            Some(StopHalf::Discant) if disc.is_empty() => {
                disc = scan_audio_files_with_multimic(&sub_path, base_dir)?;
            }
            _ => {}
        }
    }

    if !bass.is_empty() || !disc.is_empty() {
        let mut variants = Vec::new();
        if !bass.is_empty() {
            variants.push((StopHalf::Bass, bass));
        }
        if !disc.is_empty() {
            variants.push((StopHalf::Discant, disc));
        }
        return Ok(variants);
    }

    // 3. No bass/discant split -> multi-mic fallback (first sub-folder with audio).
    let full = scan_audio_files_with_multimic(stop_path, base_dir)?;
    if full.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(vec![(StopHalf::Full, full)])
    }
}

/// Build one or more `ScannedStop`s from a single stop folder.
///
/// A folder normally yields one stop, but a bass/discant-divided register
/// (with `*_bas` and `*_dis` sub-folders) yields a separate stop per half so
/// both appear as their own draw-knob (e.g. "Trompet (Bas)" + "Trompet (Disc)").
///
/// `trem_pipes` are attached only to a single full-compass stop.
fn build_stops_from_folder(
    dir_name_str: &str,
    stop_path: &Path,
    base_dir: &Path,
    is_pedal: bool,
    id_prefix: &str,
    trem_pipes: Vec<CustomPipe>,
) -> Result<Vec<ScannedStop>, std::io::Error> {
    let (stop_name, pitch, family, pitch_label) = parse_stop_info(dir_name_str);
    let variants = scan_stop_variants(stop_path, base_dir)?;
    let is_divided = variants.len() > 1;

    let mut out = Vec::new();
    for (half, mut pipes) in variants {
        if pipes.is_empty() {
            continue;
        }
        pipes.sort_by_key(|p| p.midi_note);

        let (is_bass, is_treble) = match half {
            StopHalf::Bass => (true, false),
            StopHalf::Discant => (false, true),
            // Bereik-detectie alleen voor manualen: een pedaalregister beslaat
            // van nature precies het "bas"-bereik (C–f1) en zou anders altijd
            // onterecht "(Bas)" heten. Bas/discant bestaat op het pedaal niet.
            StopHalf::Full if is_pedal => (false, false),
            StopHalf::Full => detect_bass_treble(&pipes),
        };
        let first_note = pipes.first().map(|p| p.midi_note).unwrap_or(36);
        let last_note = pipes.last().map(|p| p.midi_note).unwrap_or(96);

        // Tremulant only makes sense for a single full-compass stop.
        let (tremulant_pipes, has_tremulant) =
            if half == StopHalf::Full && !is_divided && !trem_pipes.is_empty() {
                (trem_pipes.clone(), true)
            } else {
                (Vec::new(), false)
            };

        let display_name = format_stop_name(&stop_name, is_bass, is_treble);
        let half_suffix = match half {
            StopHalf::Bass => "_bas",
            StopHalf::Discant => "_dis",
            StopHalf::Full => "",
        };
        let id = format!(
            "{}_{}{}",
            id_prefix,
            dir_name_str.replace(' ', "_").to_lowercase(),
            half_suffix
        );

        info!(
            "  Stop '{}' [{}]: MIDI {}-{}, pipes={}, trem={}",
            display_name,
            id,
            first_note,
            last_note,
            pipes.len(),
            if has_tremulant { "ja" } else { "nee" }
        );

        out.push(ScannedStop {
            stop: CustomStop {
                id,
                name: display_name,
                pitch_feet: pitch,
                pitch_label: pitch_label.clone(),
                family: family.clone(),
                pipes,
                tremulant_pipes,
                has_tremulant,
            },
            first_note,
            last_note,
            is_pedal,
            is_bass,
            is_treble,
        });
    }

    Ok(out)
}

/// Scan a directory for sample files and create a simple organ
///
/// NEW Structure (preferred):
/// - OrganName/           <- Main folder = organ name
///   - Klaviernaam/       <- Subfolder = division (keyboard) name
///     - StopName1/       <- Stop folder
///       - 036-C.wav/mp3
///       - 037-C#.wav/mp3
///     - StopName2/
///       - 036-C.wav/mp3
///   - Pedaal/
///     - Subbas16/
///       - ...
///
/// OLD Structure (still supported):
/// - dir/
///   - StopName1/
///     - 036-C.wav
///   - StopName2/
///     - 036-C.wav
///   - PedStop/
///     - 036-C.wav
///
/// Automatically detects bass (Bascant) and treble (Discant) stops based on
/// their sample ranges. Each stop gets its own MIDI range for proper triggering.
pub fn scan_samples_directory(
    dir: &Path,
    organ_name: &str,
) -> Result<CustomOrgan, std::io::Error> {
    info!("Scanning directory for samples: {:?}", dir);

    // Check if this is the new structure (subdirs are divisions with stops inside)
    // or old structure (subdirs are stops directly)
    let is_new_structure = check_new_structure(dir);

    if is_new_structure {
        info!("Detected new structure: subfolders are divisions (keyboards)");
        scan_new_structure(dir, organ_name)
    } else {
        info!("Detected old structure: subfolders are stops");
        scan_old_structure(dir, organ_name)
    }
}

/// Check if directory uses new structure (subfolders are divisions containing stop folders)
/// New structure: OrganName / DivisionName / StopName / samples
/// Old structure: OrganName / StopName / samples
fn check_new_structure(dir: &Path) -> bool {
    // Check first few subdirectories to see if they contain audio files directly
    // or if they contain more subdirectories (division -> stop -> samples)
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten().take(10) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Skip hidden and special directories
        let dir_name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if dir_name.starts_with('.') ||
           dir_name.to_lowercase() == "organinfo" ||
           dir_name.to_lowercase() == "images" {
            continue;
        }

        // Check what's inside this subdirectory
        let mut has_audio_directly = false;
        let mut has_subdirs_with_audio = false;
        let mut has_variant_subdirs = false;

        let dir_lower = dir_name.to_lowercase();
        for sub_entry in std::fs::read_dir(&path).into_iter().flatten().flatten() {
            let sub_path = sub_entry.path();

            if sub_path.is_file() && is_audio_file(&sub_path) {
                has_audio_directly = true;
            }

            if sub_path.is_dir() {
                // Bas/discant- en tremulant-varianten (Stop/Stop_bas, Stop_dis,
                // *_trem) zijn conventies van de OUDE structuur: die submappen
                // maakten deze map voorheen ten onrechte een "division" — elke
                // stop werd dan een klavier (auditbevinding 37).
                let sub_name = sub_path.file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if sub_name == format!("{}_bas", dir_lower)
                    || sub_name == format!("{}_dis", dir_lower)
                    || is_tremulant_folder(&sub_name) {
                    has_variant_subdirs = true;
                }
                // Check if this subdir contains audio files (would be a stop folder)
                for file_entry in std::fs::read_dir(&sub_path).into_iter().flatten().flatten().take(5) {
                    if file_entry.path().is_file() && is_audio_file(&file_entry.path()) {
                        has_subdirs_with_audio = true;
                        break;
                    }
                }
            }
        }

        // If we found audio files directly in the first level subdir -> old structure
        if has_audio_directly {
            info!("Detected OLD structure: {} contains audio files directly", dir_name);
            return false;
        }

        // Bas/dis/tremulant-varianten → dit is een STOPmap van de oude structuur.
        if has_variant_subdirs {
            info!("Detected OLD structure: {} bevat bas/dis/trem-variantmappen", dir_name);
            return false;
        }

        // If we found subdirs with audio files -> new structure
        if has_subdirs_with_audio {
            info!("Detected NEW structure: {} contains stop folders with audio", dir_name);
            return true;
        }
    }

    // Default to old structure if we can't determine
    info!("Could not determine structure, defaulting to old structure");
    false
}

/// Check if a directory name is a tremulant variant (ends with _trem or _TR)
fn is_tremulant_folder(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with("_trem") || lower.ends_with("_tr")
}

/// Get the base stop name from a tremulant folder name
/// "Prestant_8_trem" -> "Prestant_8", "Holpijp_8_TR" -> "Holpijp_8"
fn tremulant_base_name(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with("_trem") {
        name[..name.len() - 5].to_string()
    } else if lower.ends_with("_tr") {
        name[..name.len() - 3].to_string()
    } else {
        name.to_string()
    }
}

/// Scan using new structure: division folders containing stop folders
/// Supports:
/// - Tremulant variants: StopName_trem/ folders are linked to StopName/
/// - Multi-mic: if a stop folder contains subfolders with audio files,
///   the first subfolder's samples are used (primary mic position)
fn scan_new_structure(dir: &Path, organ_name: &str) -> Result<CustomOrgan, std::io::Error> {
    let mut organ = CustomOrgan::new(organ_name);

    // Scan subdirectories as divisions (keyboards)
    for division_entry in std::fs::read_dir(dir)? {
        let division_entry = division_entry?;
        let division_path = division_entry.path();

        if !division_path.is_dir() {
            continue;
        }

        let division_name = division_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        // Skip special directories
        if division_name.starts_with('.') ||
           division_name.to_lowercase() == "organinfo" ||
           division_name.to_lowercase() == "images" {
            continue;
        }

        info!("Scanning division: {}", division_name);

        // First pass: collect all stop folders and tremulant folders separately
        let mut dry_stops: Vec<(String, PathBuf)> = Vec::new();       // (dir_name, path)
        let mut trem_stops: Vec<(String, String, PathBuf)> = Vec::new(); // (dir_name, base_name, path)

        for stop_entry in std::fs::read_dir(&division_path)? {
            let stop_entry = stop_entry?;
            let stop_path = stop_entry.path();

            if !stop_path.is_dir() {
                continue;
            }

            let dir_name_str = stop_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if dir_name_str.starts_with('.') {
                continue;
            }

            if is_tremulant_folder(&dir_name_str) {
                let base = tremulant_base_name(&dir_name_str);
                trem_stops.push((dir_name_str, base, stop_path));
            } else {
                dry_stops.push((dir_name_str, stop_path));
            }
        }

        // Second pass: build stops (splitting bass/discant folders into separate
        // knobs) with optional tremulant variants.
        let mut scanned_stops: Vec<ScannedStop> = Vec::new();
        let mut matched_trem_indices: Vec<bool> = vec![false; trem_stops.len()];
        let is_pedal = is_pedal_stop(&division_name);
        let id_prefix = division_name.replace(' ', "_").to_lowercase();

        for (dir_name_str, stop_path) in &dry_stops {
            // Look for a matching tremulant folder.
            let dir_name_lower = dir_name_str.to_lowercase();
            let mut tremulant_pipes: Vec<CustomPipe> = Vec::new();
            for (idx, (trem_dir, trem_base, trem_path)) in trem_stops.iter().enumerate() {
                if trem_base.to_lowercase() == dir_name_lower {
                    info!("  Found tremulant variant: {} -> {}", trem_dir, dir_name_str);
                    tremulant_pipes = scan_audio_files_with_multimic(trem_path, dir)?;
                    tremulant_pipes.sort_by_key(|p| p.midi_note);
                    matched_trem_indices[idx] = true;
                    break;
                }
            }

            let stops = build_stops_from_folder(
                dir_name_str, stop_path, dir, is_pedal, &id_prefix, tremulant_pipes,
            )?;
            scanned_stops.extend(stops);
        }

        // Third pass: unmatched _trem folders become standalone stops
        for (idx, (trem_dir, _trem_base, trem_path)) in trem_stops.iter().enumerate() {
            if matched_trem_indices[idx] {
                continue; // Already matched to a dry stop
            }
            let base_name = tremulant_base_name(trem_dir);
            let stops = build_stops_from_folder(
                &base_name, trem_path, dir, is_pedal, &id_prefix, Vec::new(),
            )?;
            scanned_stops.extend(stops);
        }

        // Add division with its stops
        if !scanned_stops.is_empty() {
            let first = scanned_stops.iter().map(|s| s.first_note).min().unwrap_or(36);
            let last = scanned_stops.iter().map(|s| s.last_note).max().unwrap_or(96);
            let division = organ.add_division(&division_name, first, last);

            for s in scanned_stops {
                division.stops.push(s.stop);
            }
        }
    }

    info!("Found {} stops with {} total pipes in {} divisions",
          organ.stop_count(), organ.pipe_count(), organ.divisions.len());

    Ok(organ)
}

/// Scan using old structure: stop folders directly under root
fn scan_old_structure(dir: &Path, organ_name: &str) -> Result<CustomOrgan, std::io::Error> {
    let mut organ = CustomOrgan::new(organ_name);

    // First pass: collect dry stop folders and tremulant folders
    let mut dry_folders: Vec<(String, PathBuf)> = Vec::new();
    let mut trem_folders: Vec<(String, String, PathBuf)> = Vec::new(); // (dir_name, base_name, path)

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let dir_name_str = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip special directories
        if dir_name_str.starts_with('.') ||
           dir_name_str.to_lowercase() == "organinfo" ||
           dir_name_str.to_lowercase() == "images" {
            continue;
        }

        if is_tremulant_folder(&dir_name_str) {
            let base = tremulant_base_name(&dir_name_str);
            trem_folders.push((dir_name_str, base, path));
        } else {
            dry_folders.push((dir_name_str, path));
        }
    }

    // Second pass: build stops (splitting bass/discant folders into separate
    // knobs) with optional tremulant variants.
    let mut scanned_stops: Vec<ScannedStop> = Vec::new();
    let mut matched_trem_indices: Vec<bool> = vec![false; trem_folders.len()];

    for (dir_name_str, path) in &dry_folders {
        let is_pedal = is_pedal_stop(dir_name_str);
        let id_prefix = if is_pedal { "ped" } else { "manual" };

        // Look for a matching tremulant folder.
        let dir_name_lower = dir_name_str.to_lowercase();
        let mut tremulant_pipes: Vec<CustomPipe> = Vec::new();
        for (idx, (trem_dir, trem_base, trem_path)) in trem_folders.iter().enumerate() {
            if trem_base.to_lowercase() == dir_name_lower {
                info!("Found tremulant variant: {} -> {}", trem_dir, dir_name_str);
                tremulant_pipes = scan_audio_files_with_multimic(trem_path, dir)?;
                tremulant_pipes.sort_by_key(|p| p.midi_note);
                matched_trem_indices[idx] = true;
                break;
            }
        }

        let stops = build_stops_from_folder(
            dir_name_str, path, dir, is_pedal, id_prefix, tremulant_pipes,
        )?;
        scanned_stops.extend(stops);
    }

    // Third pass: unmatched _trem folders become standalone stops
    for (idx, (trem_dir, _trem_base, trem_path)) in trem_folders.iter().enumerate() {
        if matched_trem_indices[idx] {
            continue;
        }
        let base_name = tremulant_base_name(trem_dir);
        let is_pedal = is_pedal_stop(&base_name);
        let id_prefix = if is_pedal { "ped" } else { "manual" };
        let stops = build_stops_from_folder(
            &base_name, trem_path, dir, is_pedal, id_prefix, Vec::new(),
        )?;
        scanned_stops.extend(stops);
    }

    // Group stops: pedal, manual full-range, manual bass, manual treble
    let pedal_stops: Vec<_> = scanned_stops.iter()
        .filter(|s| s.is_pedal)
        .collect();
    let manual_full: Vec<_> = scanned_stops.iter()
        .filter(|s| !s.is_pedal && !s.is_bass && !s.is_treble)
        .collect();
    let manual_bass: Vec<_> = scanned_stops.iter()
        .filter(|s| !s.is_pedal && s.is_bass)
        .collect();
    let manual_treble: Vec<_> = scanned_stops.iter()
        .filter(|s| !s.is_pedal && s.is_treble)
        .collect();

    // Add pedal division
    if !pedal_stops.is_empty() {
        let first = pedal_stops.iter().map(|s| s.first_note).min().unwrap_or(24);
        let last = pedal_stops.iter().map(|s| s.last_note).max().unwrap_or(67);
        let division = organ.add_division("Pedaal", first, last);
        for s in pedal_stops {
            division.stops.push(s.stop.clone());
        }
    }

    // Add manual division(s)
    if !manual_full.is_empty() || !manual_bass.is_empty() || !manual_treble.is_empty() {
        let all_manual: Vec<_> = manual_full.iter()
            .chain(manual_bass.iter())
            .chain(manual_treble.iter())
            .collect();

        let first = all_manual.iter().map(|s| s.first_note).min().unwrap_or(36);
        let last = all_manual.iter().map(|s| s.last_note).max().unwrap_or(96);

        let division = organ.add_division("Manuaal", first, last);

        for s in manual_full {
            division.stops.push(s.stop.clone());
        }
        for s in manual_bass {
            division.stops.push(s.stop.clone());
        }
        for s in manual_treble {
            division.stops.push(s.stop.clone());
        }
    }

    info!("Found {} stops with {} total pipes in {} divisions",
          organ.stop_count(), organ.pipe_count(), organ.divisions.len());

    Ok(organ)
}

/// Scan audio files with multi-mic support.
/// First tries to find audio files directly in the stop folder.
/// If no audio files found directly, looks for subfolders (multi-mic positions)
/// and uses the first one found as the primary mic position.
fn scan_audio_files_with_multimic(stop_path: &Path, base_dir: &Path) -> Result<Vec<CustomPipe>, std::io::Error> {
    // First try direct audio files in the stop folder
    let pipes = scan_audio_files(stop_path, base_dir)?;
    if !pipes.is_empty() {
        return Ok(pipes);
    }

    // No direct audio files - check for multi-mic subfolders
    // Use the first subfolder that contains audio files (primary mic position)
    let mut subdirs: Vec<_> = std::fs::read_dir(stop_path)?
        .flatten()
        .filter(|e| e.path().is_dir())
        .collect();

    // Sort for deterministic selection (alphabetical = first mic position)
    subdirs.sort_by_key(|e| e.file_name());

    for sub_entry in subdirs {
        let sub_path = sub_entry.path();
        let sub_name = sub_entry.file_name().to_string_lossy().to_string();

        if sub_name.starts_with('.') {
            continue;
        }

        let sub_pipes = scan_audio_files(&sub_path, base_dir)?;
        if !sub_pipes.is_empty() {
            info!("    Multi-mic: using '{}' as primary ({} samples)", sub_name, sub_pipes.len());
            return Ok(sub_pipes);
        }
    }

    Ok(Vec::new())
}

/// Scan audio files (WAV/MP3) in a directory
fn scan_audio_files(stop_path: &Path, base_dir: &Path) -> Result<Vec<CustomPipe>, std::io::Error> {
    let mut pipes: Vec<CustomPipe> = Vec::new();

    for file_entry in std::fs::read_dir(stop_path)? {
        let file_entry = file_entry?;
        let file_path = file_entry.path();

        if file_path.is_file() && is_audio_file(&file_path) {
            if let Some(file_name) = file_path.file_name() {
                let file_name_str = file_name.to_string_lossy();

                if let Some(midi_note) = parse_midi_note_from_filename(&file_name_str) {
                    // Make path relative
                    let relative_path = if let Ok(stripped) = file_path.strip_prefix(base_dir) {
                        stripped.to_path_buf()
                    } else {
                        file_path.clone()
                    };

                    pipes.push(CustomPipe {
                        midi_note,
                        sample_path: relative_path,
                        loop_start: None,
                        loop_end: None,
                        volume_db: 0.0,
                        pitch_cents: 0,
                    });
                }
            }
        }
    }

    Ok(pipes)
}

/// Format the stop display name with an optional bass/discant indicator.
///
/// The footage/rank is shown separately in the UI (via the `pitch` field), so
/// it is intentionally not embedded in the name here.
fn format_stop_name(stop_name: &str, is_bass: bool, is_treble: bool) -> String {
    if is_bass {
        format!("{} (Bas)", stop_name)
    } else if is_treble {
        format!("{} (Disc)", stop_name)
    } else {
        stop_name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_custom_organ() {
        let mut organ = CustomOrgan::new("Test Organ");
        organ.builder = "Test Builder".to_string();

        let division = organ.add_division("Great", 36, 96);
        let stop = division.add_stop("Principal 8'", 8.0, StopFamily::Principal);
        stop.add_pipe(60, PathBuf::from("samples/principal/C4.wav"));

        assert_eq!(organ.stop_count(), 1);
        assert_eq!(organ.pipe_count(), 1);
    }

    #[test]
    fn test_serialize_deserialize() {
        let organ = CustomOrgan::new("Test");
        let json = serde_json::to_string(&organ).unwrap();
        let loaded: CustomOrgan = serde_json::from_str(&json).unwrap();
        assert_eq!(organ.name, loaded.name);
    }

    #[test]
    fn test_parse_underscore_names() {
        // Trailing underscore separator must be stripped, not kept.
        let (name, pitch, _, label) = parse_stop_info("Prestant_8");
        assert_eq!(name, "Prestant");
        assert_eq!(pitch, 8.0);
        assert_eq!(label, None);

        let (name, pitch, _, _) = parse_stop_info("Bourdon_16");
        assert_eq!(name, "Bourdon");
        assert_eq!(pitch, 16.0);

        let (name, _, _, _) = parse_stop_info("FluteTravers_8");
        assert_eq!(name, "FluteTravers");
    }

    #[test]
    fn test_parse_rank_suffix() {
        // "4st" (4 sterk / ranks) must be preserved as a label, not turned into feet.
        let (name, pitch, fam, label) = parse_stop_info("Cornet_4st");
        assert_eq!(name, "Cornet");
        assert_eq!(pitch, 4.0);
        assert_eq!(label, Some("4st".to_string()));
        assert_eq!(fam, StopFamily::Mixture);

        let (name, _, _, label) = parse_stop_info("Mixtuur_4st");
        assert_eq!(name, "Mixtuur");
        assert_eq!(label, Some("4st".to_string()));

        // "sterk" spelled out normalises to the same "Nst" label.
        let (_, _, _, label) = parse_stop_info("Mixtuur 5 sterk");
        assert_eq!(label, Some("5st".to_string()));
    }

    #[test]
    fn test_parse_spaced_names() {
        let (name, pitch, fam, _) = parse_stop_info("Voix Celeste 8");
        assert_eq!(name, "Voix Celeste");
        assert_eq!(pitch, 8.0);
        assert_eq!(fam, StopFamily::String);
    }

    #[test]
    fn test_classify_half() {
        assert_eq!(classify_half("Trompet_8_bas"), Some(StopHalf::Bass));
        assert_eq!(classify_half("Trompet_8_dis"), Some(StopHalf::Discant));
        assert_eq!(classify_half("Cornet_dis"), Some(StopHalf::Discant));
        assert_eq!(classify_half("Mixtuur_bas"), Some(StopHalf::Bass));
        assert_eq!(classify_half("Discant"), Some(StopHalf::Discant));
        // Multi-mic position folders must not be treated as a half.
        assert_eq!(classify_half("Close"), None);
        assert_eq!(classify_half("Far"), None);
    }

    #[test]
    fn test_format_stop_name() {
        assert_eq!(format_stop_name("Prestant", false, false), "Prestant");
        assert_eq!(format_stop_name("Trompet", true, false), "Trompet (Bas)");
        assert_eq!(format_stop_name("Trompet", false, true), "Trompet (Disc)");
    }

    #[test]
    fn test_pedaalregister_krijgt_geen_bas_label() {
        // Een pedaalregister beslaat van nature het "bas"-bereik (36-60); de
        // bereik-heuristiek mag daar dus géén "(Bas)" van maken. Voor een
        // manuaal met hetzelfde bereik is "(Bas)" juist wél correct (halve stop).
        let base = std::env::temp_dir().join(format!("jm_pedaal_bas_test_{}", std::process::id()));
        let stop = base.join("Subbas_16");
        std::fs::create_dir_all(&stop).unwrap();
        for n in [36u32, 48, 60] {
            std::fs::write(stop.join(format!("{:03}-C.wav", n)), b"x").unwrap();
        }

        let ped = build_stops_from_folder("Subbas_16", &stop, &base, true, "p", Vec::new()).unwrap();
        assert_eq!(ped.len(), 1);
        assert_eq!(ped[0].stop.name, "Subbas", "pedaal: geen (Bas)-suffix");
        assert!(!ped[0].is_bass);

        let man = build_stops_from_folder("Subbas_16", &stop, &base, false, "m", Vec::new()).unwrap();
        assert_eq!(man[0].stop.name, "Subbas (Bas)", "manuaal: zelfde bereik is wél een bas-helft");

        std::fs::remove_dir_all(&base).ok();
    }
}
