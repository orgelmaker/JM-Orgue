//! VPO Audio Engine
//!
//! Real-time audio processing for virtual pipe organ.
//! Handles sample playback, mixing, effects, and output.

pub mod device;
pub mod engine;
pub mod voice;
pub mod mixer;
pub mod effects;
pub mod convolution;
pub mod reverb_fdn;
pub mod buffer;

pub use device::*;
pub use engine::*;
pub use voice::*;
pub use mixer::*;
pub use effects::*;
pub use convolution::*;
pub use reverb_fdn::*;
pub use buffer::*;

/// Audio processing block size
pub const BLOCK_SIZE: usize = 256;

/// Hard plafond voor het aantal gelijktijdige stemmen (Vec-capaciteit, klem
/// van de instelling). De werkelijke kap is instelbaar (zie DEFAULT_POLYPHONY
/// en `vpo_app::audio::set_polyphony_target`).
pub const MAX_POLYPHONY: usize = 4096;

/// Standaard-polyfonie: was een vaste 512 (beschermlaag uit 0.6.2 tegen
/// underruns); een sampleset-maker meldde dat dit bij een tutti op een grote
/// set te krap is. 1024 is op een moderne quad-core ruim haalbaar.
pub const DEFAULT_POLYPHONY: usize = 1024;

/// Maximum number of audio channels
pub const MAX_CHANNELS: usize = 8;
