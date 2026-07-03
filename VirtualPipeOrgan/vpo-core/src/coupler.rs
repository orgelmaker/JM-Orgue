//! Coupler definitions
//!
//! Couplers connect keyboards together, allowing one keyboard
//! to play stops from another division.

use serde::{Deserialize, Serialize};
use crate::MidiNote;

/// A coupler connecting two keyboards
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coupler {
    /// Unique identifier
    pub id: String,
    
    /// Display name (e.g., "Positief aan Hoofdwerk")
    pub name: String,
    
    /// Source keyboard ID (the one being played)
    pub source_keyboard: String,
    
    /// Destination keyboard ID (the one receiving notes)
    pub destination_keyboard: String,
    
    /// Coupler type
    pub coupler_type: CouplerType,
    
    /// Is coupler currently active
    pub active: bool,
    
    /// Volume adjustment when coupled (dB)
    pub volume_db: f32,
}

impl Coupler {
    /// Create a unison coupler (same pitch)
    pub fn unison(
        id: impl Into<String>,
        name: impl Into<String>,
        source: impl Into<String>,
        destination: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source_keyboard: source.into(),
            destination_keyboard: destination.into(),
            coupler_type: CouplerType::Unison,
            active: false,
            volume_db: 0.0,
        }
    }
    
    /// Create an octave coupler
    pub fn octave(
        id: impl Into<String>,
        name: impl Into<String>,
        source: impl Into<String>,
        destination: impl Into<String>,
        direction: OctaveDirection,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source_keyboard: source.into(),
            destination_keyboard: destination.into(),
            coupler_type: CouplerType::Octave(direction),
            active: false,
            volume_db: 0.0,
        }
    }
    
    /// Apply coupler transposition to a MIDI note
    pub fn apply(&self, note: MidiNote) -> Option<MidiNote> {
        let offset = match self.coupler_type {
            CouplerType::Unison => 0,
            CouplerType::Octave(OctaveDirection::Up) => 12,
            CouplerType::Octave(OctaveDirection::Down) => -12,
            CouplerType::SubOctave => -12,
            CouplerType::SuperOctave => 12,
            CouplerType::Melody => return None, // Complex logic needed
            CouplerType::Bass => return None,   // Complex logic needed
        };
        
        let result = note as i16 + offset;
        if result >= 0 && result <= 127 {
            Some(result as MidiNote)
        } else {
            None
        }
    }
}

/// Type of coupler
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CouplerType {
    /// Unison (same pitch)
    Unison,
    
    /// Octave coupler
    Octave(OctaveDirection),
    
    /// Sub-octave (octave down)
    SubOctave,
    
    /// Super-octave (octave up)
    SuperOctave,
    
    /// Melody coupler (highest note only)
    Melody,
    
    /// Bass coupler (lowest note only)
    Bass,
}

/// Direction for octave couplers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OctaveDirection {
    Up,
    Down,
}

/// Standard coupler names for Dutch organs
pub mod dutch_couplers {
    pub const POSITIEF_AAN_HOOFDWERK: &str = "Positief aan Hoofdwerk";
    pub const RUGWERK_AAN_HOOFDWERK: &str = "Rugwerk aan Hoofdwerk";
    pub const BOVENWERK_AAN_HOOFDWERK: &str = "Bovenwerk aan Hoofdwerk";
    pub const HOOFDWERK_AAN_PEDAAL: &str = "Hoofdwerk aan Pedaal";
    pub const POSITIEF_AAN_PEDAAL: &str = "Positief aan Pedaal";
    pub const MANUAALKOPPEL: &str = "Manuaalkoppel";
    pub const PEDAALKOPPEL: &str = "Pedaalkoppel";
}

/// Standard coupler names for German organs
pub mod german_couplers {
    pub const KOPPEL_I_II: &str = "Koppel I/II";
    pub const KOPPEL_III_II: &str = "Koppel III/II";
    pub const KOPPEL_I_P: &str = "Koppel I/P";
    pub const KOPPEL_II_P: &str = "Koppel II/P";
}

/// Standard coupler names for French organs
pub mod french_couplers {
    pub const TIRASSE_GO: &str = "Tirasse G.O.";
    pub const TIRASSE_POS: &str = "Tirasse Pos.";
    pub const TIRASSE_REC: &str = "Tirasse Réc.";
    pub const ACCOUPLEMENT: &str = "Accouplement";
    pub const OCTAVES_GRAVES: &str = "Octaves Graves";
    pub const OCTAVES_AIGUES: &str = "Octaves Aiguës";
}
