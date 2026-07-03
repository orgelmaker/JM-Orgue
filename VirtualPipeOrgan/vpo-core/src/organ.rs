//! Organ definition and structure
//!
//! An organ consists of multiple divisions (Hoofdwerk, Positief, Pedaal, etc.),
//! each containing stops, which reference ranks of pipes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{Keyboard, Stop, Coupler, Temperament, SampleRate, Frequency};

/// Complete organ definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organ {
    /// Unique identifier
    pub id: String,
    
    /// Display name (e.g., "Kam Orgel - Grote Kerk Dordrecht")
    pub name: String,
    
    /// Builder name
    pub builder: String,
    
    /// Year built
    pub year: Option<u16>,
    
    /// Location (church/venue name)
    pub location: String,
    
    /// Description text
    pub description: String,
    
    /// Base path for sample files
    pub sample_path: PathBuf,
    
    /// Sample rate of the samples
    pub sample_rate: SampleRate,
    
    /// Concert pitch (A4 frequency, typically 440 Hz)
    pub pitch: Frequency,
    
    /// Temperament used
    pub temperament: Temperament,
    
    /// Divisions (Hoofdwerk, Positief, Pedaal, etc.)
    pub divisions: Vec<Division>,
    
    /// Available couplers
    pub couplers: Vec<Coupler>,
    
    /// General pistons / combinations
    pub combinations: Vec<Combination>,
    
    /// Impulse responses for reverb
    pub impulse_responses: Vec<ImpulseResponse>,
    
    /// Wind model settings
    pub wind_model: WindModel,
    
    /// Metadata
    pub metadata: OrganMetadata,
}

impl Organ {
    /// Get a division by name
    pub fn get_division(&self, name: &str) -> Option<&Division> {
        self.divisions.iter().find(|d| d.name == name)
    }
    
    /// Get a division by name (mutable)
    pub fn get_division_mut(&mut self, name: &str) -> Option<&mut Division> {
        self.divisions.iter_mut().find(|d| d.name == name)
    }
    
    /// Get all stops across all divisions
    pub fn all_stops(&self) -> impl Iterator<Item = &Stop> {
        self.divisions.iter().flat_map(|d| d.stops.iter())
    }
    
    /// Count total number of stops
    pub fn stop_count(&self) -> usize {
        self.divisions.iter().map(|d| d.stops.len()).sum()
    }
}

/// A division of the organ (e.g., Hoofdwerk, Positief, Pedaal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Division {
    /// Division name
    pub name: String,
    
    /// Display name (localized)
    pub display_name: String,
    
    /// Associated keyboard
    pub keyboard: Keyboard,
    
    /// Stops in this division
    pub stops: Vec<Stop>,
    
    /// Tremulant settings (if available)
    pub tremulant: Option<Tremulant>,
    
    /// Division-specific volume/expression
    pub expression: Expression,
}

/// Tremulant settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tremulant {
    /// Is tremulant currently active
    pub active: bool,
    
    /// Tremulant speed in Hz
    pub speed: f32,
    
    /// Tremulant depth (0.0 - 1.0)
    pub depth: f32,
    
    /// Attack time in ms
    pub attack_ms: f32,
    
    /// Does this tremulant have separate samples?
    pub has_samples: bool,
}

impl Default for Tremulant {
    fn default() -> Self {
        Self {
            active: false,
            speed: 6.0,
            depth: 0.5,
            attack_ms: 500.0,
            has_samples: false,
        }
    }
}

/// Expression/swell settings for a division
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expression {
    /// Is this division enclosed (has swell box)
    pub enclosed: bool,
    
    /// Current expression level (0.0 - 1.0)
    pub level: f32,
    
    /// MIDI CC for expression pedal
    pub midi_cc: Option<u8>,
}

impl Default for Expression {
    fn default() -> Self {
        Self {
            enclosed: false,
            level: 1.0,
            midi_cc: None,
        }
    }
}

/// Impulse response for convolution reverb
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpulseResponse {
    /// Name (e.g., "Front", "Rear", "Balcony")
    pub name: String,
    
    /// File path relative to organ
    pub file: PathBuf,
    
    /// Gain adjustment in dB
    pub gain_db: f32,
    
    /// Pre-delay in ms
    pub predelay_ms: f32,
}

/// Wind model simulation settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindModel {
    /// Is wind model enabled
    pub enabled: bool,
    
    /// Wind pressure variation amount (0.0 - 1.0)
    pub pressure_variation: f32,
    
    /// Wind noise level (0.0 - 1.0)
    pub noise_level: f32,
    
    /// Bellows refill speed
    pub refill_speed: f32,
    
    /// Tracker noise enabled
    pub tracker_noise: bool,
    
    /// Tracker noise volume (0.0 - 1.0)
    pub tracker_noise_level: f32,
}

impl Default for WindModel {
    fn default() -> Self {
        Self {
            enabled: true,
            pressure_variation: 0.3,
            noise_level: 0.1,
            refill_speed: 1.0,
            tracker_noise: true,
            tracker_noise_level: 0.2,
        }
    }
}

/// A stored combination (piston)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Combination {
    /// Combination name
    pub name: String,
    
    /// Which piston number (1-8, etc.)
    pub number: u8,
    
    /// Is this a general or divisional piston
    pub is_general: bool,
    
    /// For divisional pistons, which division
    pub division: Option<String>,
    
    /// Active stop IDs
    pub active_stops: Vec<String>,
    
    /// Active coupler IDs
    pub active_couplers: Vec<String>,
}

/// Organ metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganMetadata {
    /// Version of the organ definition format
    pub format_version: String,
    
    /// Who created this sample set
    pub creator: String,
    
    /// Creation date
    pub created: String,
    
    /// Last modified date
    pub modified: String,
    
    /// Copyright notice
    pub copyright: String,
    
    /// Total size in bytes
    pub total_size_bytes: u64,
    
    /// Preview image path
    pub preview_image: Option<PathBuf>,
    
    /// Tags for searching
    pub tags: Vec<String>,
}

impl Default for OrganMetadata {
    fn default() -> Self {
        Self {
            format_version: "1.0".to_string(),
            creator: String::new(),
            created: String::new(),
            modified: String::new(),
            copyright: String::new(),
            total_size_bytes: 0,
            preview_image: None,
            tags: Vec::new(),
        }
    }
}
