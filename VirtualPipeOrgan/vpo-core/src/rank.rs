//! Rank definitions
//!
//! A rank is a row of pipes that produces sound. Each rank contains
//! samples for each note (or uses sample stretching for missing notes).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::{MidiNote, NoteRange};

/// A rank of pipes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rank {
    /// Unique identifier
    pub id: String,
    
    /// Display name
    pub name: String,
    
    /// Pipe type
    pub pipe_type: PipeType,
    
    /// Note range that this rank covers
    pub note_range: NoteRange,
    
    /// Sample definitions for each note
    pub pipes: Vec<Pipe>,
    
    /// Release samples (if separate)
    pub releases: Vec<Release>,
    
    /// Attack characteristics
    pub attack: AttackType,
    
    /// Default volume in dB
    pub volume_db: f32,
    
    /// Harmonic content description
    pub harmonic_content: HarmonicContent,
}

impl Rank {
    /// Get the pipe sample for a given MIDI note
    pub fn get_pipe(&self, note: MidiNote) -> Option<&Pipe> {
        self.pipes.iter().find(|p| p.note == note)
    }
    
    /// Get the nearest pipe for a note (for sample stretching)
    pub fn get_nearest_pipe(&self, note: MidiNote) -> Option<&Pipe> {
        if let Some(pipe) = self.get_pipe(note) {
            return Some(pipe);
        }
        
        // Find nearest sampled note
        self.pipes.iter()
            .min_by_key(|p| (p.note as i32 - note as i32).abs())
    }
}

/// A single pipe (sample for one note)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipe {
    /// MIDI note this pipe sounds
    pub note: MidiNote,
    
    /// Sample file path (relative to rank folder)
    pub sample_file: PathBuf,
    
    /// Attack sample file (if separate)
    pub attack_file: Option<PathBuf>,
    
    /// Sample start position in frames
    pub sample_start: u64,
    
    /// Loop start position in frames
    pub loop_start: u64,
    
    /// Loop end position in frames
    pub loop_end: u64,
    
    /// Sample end position in frames (for release)
    pub sample_end: u64,
    
    /// Crossfade length for loop in frames
    pub loop_crossfade: u32,
    
    /// Volume adjustment for this specific pipe in dB
    pub volume_db: f32,
    
    /// Pitch correction in cents
    pub pitch_cents: f32,
    
    /// Harmonic tuning adjustments
    pub tuning: PipeTuning,
}

/// Release sample definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    /// MIDI note
    pub note: MidiNote,
    
    /// Release sample file
    pub sample_file: PathBuf,
    
    /// Maximum key hold time this release is for (in seconds)
    /// None means this is the default release
    pub max_key_time: Option<f32>,
    
    /// Crossfade time from sustain to release in ms
    pub crossfade_ms: f32,
}

/// Pipe tuning parameters
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct PipeTuning {
    /// Fundamental frequency adjustment in cents
    pub fundamental_cents: f32,
    
    /// Temperament-specific adjustment (applied on top of fundamental)
    pub temperament_cents: f32,
    
    /// Random tuning variation in cents (for realism)
    pub random_cents: f32,
}

/// Type of pipe
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipeType {
    /// Open flue pipe (principals, flutes)
    OpenFlue,
    
    /// Stopped flue pipe (gedackts, bourdons)
    StoppedFlue,
    
    /// Chimney flue pipe (rohrflutes)
    ChimneyFlue,
    
    /// String pipe (gambas, violins)
    String,
    
    /// Reed pipe (trumpets, oboes)
    Reed,
}

/// Attack characteristics
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AttackType {
    /// Natural attack from sample
    Natural,
    
    /// Separate attack sample that crossfades to sustain
    Separate {
        /// Crossfade time in ms
        crossfade_ms: f32,
    },
    
    /// Synthetic attack envelope
    Synthetic {
        /// Attack time in ms
        attack_ms: f32,
    },
}

impl Default for AttackType {
    fn default() -> Self {
        Self::Natural
    }
}

/// Harmonic content description for a rank
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarmonicContent {
    /// Primary harmonics present (1 = fundamental, 2 = octave, etc.)
    pub harmonics: Vec<u8>,
    
    /// Is this a mixture/compound stop?
    pub is_mixture: bool,
    
    /// Mixture composition (pitch relationships)
    pub mixture_ranks: Option<Vec<MixtureRank>>,
}

impl Default for HarmonicContent {
    fn default() -> Self {
        Self {
            harmonics: vec![1],
            is_mixture: false,
            mixture_ranks: None,
        }
    }
}

/// A rank within a mixture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixtureRank {
    /// Pitch in feet
    pub pitch_feet: f32,
    
    /// Break point (MIDI note where this rank changes)
    pub break_note: Option<MidiNote>,
}
