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

/// Maximum polyphony (number of simultaneous voices)
pub const MAX_POLYPHONY: usize = 512;

/// Maximum number of audio channels
pub const MAX_CHANNELS: usize = 8;
