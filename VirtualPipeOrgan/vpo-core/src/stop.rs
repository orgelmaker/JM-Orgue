//! Stop definitions
//!
//! A stop controls one or more ranks of pipes. When drawn, it activates
//! the corresponding pipes when keys are pressed.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::MidiNote;

/// A single stop (register)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stop {
    /// Unique identifier
    pub id: String,
    
    /// Display name (e.g., "Prestant 8'", "Gedekt 16'")
    pub name: String,
    
    /// Stop family
    pub family: StopFamily,
    
    /// Pitch in feet (8' = unison, 4' = octave up, 16' = octave down)
    pub pitch_feet: f32,
    
    /// MIDI note range
    pub note_range: NoteRange,
    
    /// Is this stop currently drawn (active)
    pub drawn: bool,
    
    /// Ranks that this stop controls
    pub ranks: Vec<StopRank>,
    
    /// Volume adjustment in dB
    pub volume_db: f32,
    
    /// Stereo panning (-1.0 = left, 0.0 = center, 1.0 = right)
    pub pan: f32,
    
    /// Display order in the console
    pub display_order: u32,
    
    /// Color code for UI (optional)
    pub color: Option<String>,
    
    /// Whether this stop supports tremulant
    pub tremulant_affected: bool,
}

impl Stop {
    /// Create a new stop with default values
    pub fn new(id: impl Into<String>, name: impl Into<String>, family: StopFamily, pitch: f32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            family,
            pitch_feet: pitch,
            note_range: NoteRange::default(),
            drawn: false,
            ranks: Vec::new(),
            volume_db: 0.0,
            pan: 0.0,
            display_order: 0,
            color: None,
            tremulant_affected: true,
        }
    }
    
    /// Toggle the stop on/off
    pub fn toggle(&mut self) {
        self.drawn = !self.drawn;
    }
    
    /// Calculate the sounding pitch for a given MIDI note
    pub fn sounding_pitch(&self, note: MidiNote) -> Option<MidiNote> {
        // Convert feet to semitone offset
        // 8' = 0 semitones, 4' = +12, 16' = -12, 2' = +24, etc.
        let semitone_offset = match self.pitch_feet {
            32.0 => -24,
            16.0 => -12,
            8.0 => 0,
            4.0 => 12,
            2.0 => 24,
            1.0 => 36,
            // Mutations
            5.333 => 7,   // 5 1/3' (quint)
            2.666 => 19,  // 2 2/3' (nazard/quint)
            1.6 => 28,    // 1 3/5' (tierce)
            1.333 => 31,  // 1 1/3' (larigot)
            _ => 0,
        };
        
        let sounding = note as i32 + semitone_offset;
        if sounding >= 0 && sounding <= 127 {
            Some(sounding as MidiNote)
        } else {
            None
        }
    }
}

/// Reference to a rank within a stop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopRank {
    /// Rank ID
    pub rank_id: String,
    
    /// Volume scaling for this rank in this stop (0.0 - 1.0)
    pub volume_scale: f32,
    
    /// Optional pitch offset in cents
    pub pitch_offset_cents: f32,
}

/// Note range for a stop or keyboard
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NoteRange {
    /// Lowest MIDI note
    pub low: MidiNote,
    
    /// Highest MIDI note
    pub high: MidiNote,
}

impl NoteRange {
    pub fn new(low: MidiNote, high: MidiNote) -> Self {
        Self { low, high }
    }
    
    pub fn contains(&self, note: MidiNote) -> bool {
        note >= self.low && note <= self.high
    }
    
    pub fn len(&self) -> usize {
        (self.high - self.low + 1) as usize
    }
}

impl Default for NoteRange {
    fn default() -> Self {
        // Standard 61-note manual: C2 to C7
        Self { low: 36, high: 96 }
    }
}

/// Stop family classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopFamily {
    // Principal family
    Principal,
    Octave,
    Fifteenth,
    Mixture,
    
    // Flute family
    Flute,
    Gedackt,
    Bourdon,
    Rohrflute,
    
    // String family
    String,
    Gamba,
    Voix,
    Celeste,
    
    // Reed family
    Reed,
    Trumpet,
    Oboe,
    Cromorne,
    Vox,
    
    // Special
    Tremulant,
    Coupler,
    Other,
}

impl StopFamily {
    /// Get the default color for this stop family
    pub fn default_color(&self) -> &'static str {
        match self {
            Self::Principal | Self::Octave | Self::Fifteenth | Self::Mixture => "#FFFFFF",
            Self::Flute | Self::Gedackt | Self::Bourdon | Self::Rohrflute => "#FFE4B5",
            Self::String | Self::Gamba | Self::Voix | Self::Celeste => "#98FB98",
            Self::Reed | Self::Trumpet | Self::Oboe | Self::Cromorne | Self::Vox => "#FF6B6B",
            Self::Tremulant => "#ADD8E6",
            Self::Coupler => "#DDA0DD",
            Self::Other => "#D3D3D3",
        }
    }
}
