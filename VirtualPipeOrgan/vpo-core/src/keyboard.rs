//! Keyboard definitions
//!
//! Keyboards (manuals and pedal) define the physical interface
//! and MIDI mapping for playing the organ.

use serde::{Deserialize, Serialize};
use crate::{MidiNote, NoteRange};

/// A keyboard (manual or pedal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyboard {
    /// Unique identifier
    pub id: String,
    
    /// Display name (e.g., "Hoofdwerk", "Positief", "Pedaal")
    pub name: String,
    
    /// Keyboard type
    pub keyboard_type: KeyboardType,
    
    /// Note range
    pub note_range: NoteRange,
    
    /// MIDI channel (0-15)
    pub midi_channel: u8,
    
    /// Transpose in semitones
    pub transpose: i8,
    
    /// Velocity sensitivity curve
    pub velocity_curve: VelocityCurve,
    
    /// Key contact type (affects response)
    pub contact_type: ContactType,
}

impl Keyboard {
    /// Create a standard 61-note manual
    pub fn manual(id: impl Into<String>, name: impl Into<String>, channel: u8) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            keyboard_type: KeyboardType::Manual,
            note_range: NoteRange::new(36, 96), // C2 to C7
            midi_channel: channel,
            transpose: 0,
            velocity_curve: VelocityCurve::Linear,
            contact_type: ContactType::Single,
        }
    }
    
    /// Create a standard 32-note pedalboard
    pub fn pedal(id: impl Into<String>, channel: u8) -> Self {
        Self {
            id: id.into(),
            name: "Pedaal".into(),
            keyboard_type: KeyboardType::Pedal,
            note_range: NoteRange::new(36, 67), // C2 to G4
            midi_channel: channel,
            transpose: 0,
            velocity_curve: VelocityCurve::Linear,
            contact_type: ContactType::Single,
        }
    }
    
    /// Create a standard 30-note pedalboard (AGO)
    pub fn pedal_ago(id: impl Into<String>, channel: u8) -> Self {
        Self {
            id: id.into(),
            name: "Pedaal".into(),
            keyboard_type: KeyboardType::Pedal,
            note_range: NoteRange::new(36, 65), // C2 to F4
            midi_channel: channel,
            transpose: 0,
            velocity_curve: VelocityCurve::Linear,
            contact_type: ContactType::Single,
        }
    }
    
    /// Map a MIDI note to the actual sounding note
    pub fn map_note(&self, midi_note: MidiNote) -> Option<MidiNote> {
        let transposed = midi_note as i16 + self.transpose as i16;
        if transposed >= 0 && transposed <= 127 && self.note_range.contains(midi_note) {
            Some(transposed as MidiNote)
        } else {
            None
        }
    }
}

/// Type of keyboard
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyboardType {
    /// Manual (hand keyboard)
    Manual,
    
    /// Pedalboard
    Pedal,
    
    /// Expression pedal
    Expression,
    
    /// Crescendo pedal
    Crescendo,
    
    /// Piston (combination control)
    Pistons,
}

/// Velocity curve type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VelocityCurve {
    /// Linear response
    Linear,
    
    /// Logarithmic (more sensitive at low velocities)
    Logarithmic,
    
    /// Exponential (more sensitive at high velocities)
    Exponential,
    
    /// Fixed velocity (ignore input velocity)
    Fixed(u8),
    
    /// Custom curve (lookup table index)
    Custom(u8),
}

impl VelocityCurve {
    /// Apply the velocity curve to an input velocity
    pub fn apply(&self, input: u8) -> u8 {
        match self {
            Self::Linear => input,
            Self::Logarithmic => {
                let normalized = input as f32 / 127.0;
                let curved = normalized.sqrt();
                (curved * 127.0) as u8
            }
            Self::Exponential => {
                let normalized = input as f32 / 127.0;
                let curved = normalized * normalized;
                (curved * 127.0) as u8
            }
            Self::Fixed(vel) => *vel,
            Self::Custom(_) => input, // Would look up in table
        }
    }
}

/// Key contact type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContactType {
    /// Single contact (standard)
    Single,
    
    /// Double contact (can measure velocity)
    Double,
    
    /// Triple contact (velocity + aftertouch)
    Triple,
    
    /// Continuous (weighted keyboard with full velocity)
    Continuous,
}
