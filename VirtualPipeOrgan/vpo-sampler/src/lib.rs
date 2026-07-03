//! VPO Sampler - Sample loading and management
//!
//! Handles loading, caching, and streaming of audio samples.
//! Supports preload buffers for instant attack playback with background streaming.

pub mod loader;
pub mod cache;
pub mod pool;
pub mod grandorgue;
pub mod hauptwerk;
pub mod streaming;
pub mod custom_organ;

pub use loader::{
    SampleData, SampleError, PreloadBuffer,
    load_wav, load_wav_preload, load_wav_remainder,
    load_mp3, load_audio, load_audio_preload,
    resample, normalize, apply_fades
};
pub use cache::*;
pub use pool::*;
pub use grandorgue::*;
pub use hauptwerk::{load_hauptwerk, is_hauptwerk_path, clean_stop_name, clean_division_name};
pub use streaming::*;
pub use custom_organ::*;
