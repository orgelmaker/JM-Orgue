//! VPO Sampler - Sample loading and management
//!
//! Handles loading, caching, and streaming of audio samples.
//! Supports preload buffers for instant attack playback with background streaming.

pub mod loader;
pub mod cache;
pub mod pool;
pub mod grandorgue;
pub mod hauptwerk;
pub mod padzoek;
pub mod hauptwerk_codm;
pub mod import_diagnose;
pub mod import_report;
pub(crate) mod hauptwerk_compact;
pub mod streaming;
pub mod custom_organ;
pub mod perspective;
pub mod registernaam;
pub mod wavpack;
pub mod stille_staart;

pub use loader::{
    SampleData, SampleError, PreloadBuffer, PreloadSegment, WavMarkers, ReleaseAlignTable,
    load_wav, load_wav_preload, load_wav_preload_segment, load_wav_preload_segment_ms, load_audio_preload_ms, PreloadLen, load_wav_remainder,
    load_mp3, load_audio, load_audio_preload, optimize_loop_end, optimize_loop_end_lr, optimize_loop_points,
    split_channels, set_stereo_loading, stereo_loading,
    resample, normalize, apply_fades, read_wav_loop_points, read_wav_markers, HarmShelf
};
pub use stille_staart::{
    knip_stille_staart, set_staart_afkap_aan, staart_afkap_aan, set_staart_drempel, staart_drempel,
    staart_afkap, geldige_staart_drempel,
    STAART_AFKAP_STANDAARD_DB, STAART_AFKAP_MIN_DB, STAART_AFKAP_MAX_DB,
};
pub use cache::*;
pub use pool::*;
pub use grandorgue::*;
pub use hauptwerk::{load_hauptwerk, load_hauptwerk_met_verslag, voorcontrole_hauptwerk, is_hauptwerk_path, clean_stop_name, strip_pitch_from_name, zonder_vastgeplakte_voetmaat,
                   set_niveaus_toepassen, niveaus_toepassen,
                   set_overgangen_toepassen, overgangen_toepassen,
                   gedeeld_voorvoegsel, strip_voorvoegsel, clean_division_name, find_package_root};
pub use streaming::*;
pub use import_report::{ImportReport, Ernst, Melding, PakketRol, PakketStatus, Tellingen, OvergeslagenStop};
pub use custom_organ::*;
pub use perspective::*;
pub use registernaam::{splits_koren, voetmaat_uit_naam, kale_voetmaat_achteraan, alleen_voetmaat, Koren, KorenSplitsing};
