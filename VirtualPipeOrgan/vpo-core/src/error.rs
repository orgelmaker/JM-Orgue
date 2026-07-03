//! Error types for the VPO system

use thiserror::Error;

#[derive(Error, Debug)]
pub enum VpoError {
    #[error("Failed to load organ definition: {0}")]
    OrganLoadError(String),

    #[error("Failed to load sample: {path}")]
    SampleLoadError { path: String },

    #[error("Invalid MIDI note: {0}")]
    InvalidMidiNote(u8),

    #[error("Stop not found: {0}")]
    StopNotFound(String),

    #[error("Keyboard not found: {0}")]
    KeyboardNotFound(String),

    #[error("Audio device error: {0}")]
    AudioDeviceError(String),

    #[error("MIDI device error: {0}")]
    MidiDeviceError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type Result<T> = std::result::Result<T, VpoError>;
