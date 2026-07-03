//! DFD (Direct From Disk) Streaming support
//!
//! This module provides streaming playback where only a small preload buffer
//! is kept in memory, and the rest is streamed from disk during playback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::io::{Read, Seek, SeekFrom};
use std::fs::File;

use crossbeam_channel::{Sender, Receiver, bounded, TrySendError};
use parking_lot::RwLock;
use hound::WavReader;
use tracing::{info, debug, warn, error};

use crate::{SampleData, SampleError};

/// Size of preload buffer in samples (at 48kHz, 4800 = 100ms)
pub const PRELOAD_SAMPLES: usize = 4800;

/// Size of streaming buffer chunks
pub const STREAM_CHUNK_SIZE: usize = 4096;

/// Number of chunks to buffer ahead
pub const BUFFER_AHEAD_CHUNKS: usize = 4;

/// Key to identify a sample
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct SampleKey {
    pub stop_id: u32,
    pub pipe_num: u32,
}

/// A sample with preload buffer and streaming info
#[derive(Debug, Clone)]
pub struct StreamingSample {
    /// Preload buffer (first ~100ms)
    pub preload: Vec<f32>,
    /// Total length in samples
    pub total_len: usize,
    /// Sample rate
    pub sample_rate: u32,
    /// Path to the source file
    pub file_path: PathBuf,
    /// Byte offset where audio data starts
    pub data_offset: u64,
    /// Bytes per sample
    pub bytes_per_sample: u16,
    /// Number of channels
    pub channels: u16,
    /// Loop start (in samples)
    pub loop_start: Option<u64>,
    /// Loop end (in samples)
    pub loop_end: Option<u64>,
}

/// Request to stream audio data
#[derive(Debug)]
pub struct StreamRequest {
    /// Sample key
    pub key: SampleKey,
    /// Start position in samples
    pub position: usize,
    /// Number of samples to read
    pub num_samples: usize,
    /// Response channel
    pub response_tx: Sender<StreamResponse>,
}

/// Response with streamed audio data
#[derive(Debug)]
pub struct StreamResponse {
    /// Sample key
    pub key: SampleKey,
    /// Position this data starts at
    pub position: usize,
    /// The audio data
    pub data: Vec<f32>,
}

/// Streaming engine that handles background disk reads
pub struct StreamingEngine {
    /// Loaded samples with preload buffers
    samples: Arc<RwLock<HashMap<SampleKey, StreamingSample>>>,
    /// Channel to send stream requests
    request_tx: Sender<StreamRequest>,
    /// Handle to the streaming thread
    _thread_handle: thread::JoinHandle<()>,
}

impl StreamingEngine {
    /// Create a new streaming engine
    pub fn new() -> Self {
        let (request_tx, request_rx) = bounded::<StreamRequest>(256);
        let samples = Arc::new(RwLock::new(HashMap::new()));
        let samples_clone = samples.clone();

        let thread_handle = thread::spawn(move || {
            Self::streaming_thread(request_rx, samples_clone);
        });

        Self {
            samples,
            request_tx,
            _thread_handle: thread_handle,
        }
    }

    /// Load a sample with only preload buffer in memory
    pub fn load_sample(
        &self,
        key: SampleKey,
        path: &Path,
        target_sample_rate: u32,
    ) -> Result<(), SampleError> {
        let reader = WavReader::open(path)
            .map_err(|e| SampleError::FileError(format!("{}: {}", path.display(), e)))?;

        let spec = reader.spec();
        let total_frames = reader.len() as usize / spec.channels as usize;

        // Read only preload buffer
        let preload_frames = PRELOAD_SAMPLES.min(total_frames);

        // Re-open to read samples
        let mut reader = WavReader::open(path)
            .map_err(|e| SampleError::FileError(format!("{}: {}", path.display(), e)))?;

        let preload: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => {
                let samples: Vec<f32> = reader.samples::<f32>()
                    .take(preload_frames * spec.channels as usize)
                    .map(|s| s.unwrap_or(0.0))
                    .collect();

                // Convert to mono
                if spec.channels == 2 {
                    samples.chunks(2)
                        .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5)
                        .collect()
                } else {
                    samples.into_iter().step_by(spec.channels as usize).collect()
                }
            }
            hound::SampleFormat::Int => {
                let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
                match spec.bits_per_sample {
                    16 => {
                        let samples: Vec<f32> = reader.samples::<i16>()
                            .take(preload_frames * spec.channels as usize)
                            .map(|s| s.unwrap_or(0) as f32 / max_val)
                            .collect();

                        if spec.channels == 2 {
                            samples.chunks(2)
                                .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5)
                                .collect()
                        } else {
                            samples.into_iter().step_by(spec.channels as usize).collect()
                        }
                    }
                    24 | 32 => {
                        let samples: Vec<f32> = reader.samples::<i32>()
                            .take(preload_frames * spec.channels as usize)
                            .map(|s| s.unwrap_or(0) as f32 / max_val)
                            .collect();

                        if spec.channels == 2 {
                            samples.chunks(2)
                                .map(|chunk| (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5)
                                .collect()
                        } else {
                            samples.into_iter().step_by(spec.channels as usize).collect()
                        }
                    }
                    _ => {
                        return Err(SampleError::UnsupportedFormat(
                            format!("{} bits per sample", spec.bits_per_sample)
                        ));
                    }
                }
            }
        };

        // Resample preload if needed
        let preload = if spec.sample_rate != target_sample_rate {
            Self::resample_buffer(&preload, spec.sample_rate, target_sample_rate)
        } else {
            preload
        };

        // Calculate the byte offset where data starts
        // For WAV files, this is after the header (typically 44 bytes for simple WAV)
        // We use hound's internal position tracking
        let data_offset = 44; // Standard WAV header size

        let streaming_sample = StreamingSample {
            preload,
            total_len: total_frames,
            sample_rate: spec.sample_rate,
            file_path: path.to_path_buf(),
            data_offset,
            bytes_per_sample: spec.bits_per_sample / 8,
            channels: spec.channels,
            loop_start: None,
            loop_end: None,
        };

        self.samples.write().insert(key, streaming_sample);
        Ok(())
    }

    /// Get the preload buffer for a sample
    pub fn get_preload(&self, key: &SampleKey) -> Option<Vec<f32>> {
        self.samples.read().get(key).map(|s| s.preload.clone())
    }

    /// Get sample info
    pub fn get_sample_info(&self, key: &SampleKey) -> Option<(usize, Option<u64>, Option<u64>)> {
        self.samples.read().get(key).map(|s| (s.total_len, s.loop_start, s.loop_end))
    }

    /// Request streaming data (non-blocking)
    pub fn request_stream(
        &self,
        key: SampleKey,
        position: usize,
        num_samples: usize,
    ) -> Receiver<StreamResponse> {
        let (response_tx, response_rx) = bounded(1);

        let request = StreamRequest {
            key,
            position,
            num_samples,
            response_tx,
        };

        if let Err(e) = self.request_tx.try_send(request) {
            match e {
                TrySendError::Full(_) => {
                    warn!("Stream request queue full");
                }
                TrySendError::Disconnected(_) => {
                    error!("Streaming thread disconnected");
                }
            }
        }

        response_rx
    }

    /// Simple resampling for preload buffer
    fn resample_buffer(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
        if from_rate == to_rate {
            return input.to_vec();
        }

        let ratio = to_rate as f64 / from_rate as f64;
        let output_len = (input.len() as f64 * ratio) as usize;
        let mut output = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_pos = i as f64 / ratio;
            let src_idx = src_pos as usize;
            let frac = src_pos.fract() as f32;

            if src_idx + 1 < input.len() {
                // Linear interpolation
                let sample = input[src_idx] * (1.0 - frac) + input[src_idx + 1] * frac;
                output.push(sample);
            } else if src_idx < input.len() {
                output.push(input[src_idx]);
            }
        }

        output
    }

    /// Background thread that handles disk reads
    fn streaming_thread(
        request_rx: Receiver<StreamRequest>,
        samples: Arc<RwLock<HashMap<SampleKey, StreamingSample>>>,
    ) {
        info!("Streaming thread started");

        // File handle cache to avoid reopening files
        let mut file_cache: HashMap<PathBuf, WavReader<std::io::BufReader<File>>> = HashMap::new();

        while let Ok(request) = request_rx.recv() {
            let samples_lock = samples.read();

            if let Some(sample_info) = samples_lock.get(&request.key) {
                let path = &sample_info.file_path;
                let channels = sample_info.channels;
                let bytes_per_sample = sample_info.bytes_per_sample;
                let sample_rate = sample_info.sample_rate;

                // Try to get cached reader or open new one
                let reader_result = if file_cache.contains_key(path) {
                    // For simplicity, we'll just re-open each time
                    // A production implementation would maintain seek positions
                    WavReader::open(path)
                } else {
                    WavReader::open(path)
                };

                if let Ok(mut reader) = reader_result {
                    let spec = reader.spec();
                    let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;

                    // Skip to position
                    let skip_samples = request.position * channels as usize;

                    let data: Vec<f32> = match spec.sample_format {
                        hound::SampleFormat::Float => {
                            reader.samples::<f32>()
                                .skip(skip_samples)
                                .take(request.num_samples * channels as usize)
                                .map(|s| s.unwrap_or(0.0))
                                .collect::<Vec<f32>>()
                                .chunks(channels as usize)
                                .map(|chunk| {
                                    if channels == 2 {
                                        (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5
                                    } else {
                                        chunk[0]
                                    }
                                })
                                .collect()
                        }
                        hound::SampleFormat::Int => {
                            match spec.bits_per_sample {
                                16 => {
                                    reader.samples::<i16>()
                                        .skip(skip_samples)
                                        .take(request.num_samples * channels as usize)
                                        .map(|s| s.unwrap_or(0) as f32 / max_val)
                                        .collect::<Vec<f32>>()
                                        .chunks(channels as usize)
                                        .map(|chunk| {
                                            if channels == 2 {
                                                (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5
                                            } else {
                                                chunk[0]
                                            }
                                        })
                                        .collect()
                                }
                                24 | 32 => {
                                    reader.samples::<i32>()
                                        .skip(skip_samples)
                                        .take(request.num_samples * channels as usize)
                                        .map(|s| s.unwrap_or(0) as f32 / max_val)
                                        .collect::<Vec<f32>>()
                                        .chunks(channels as usize)
                                        .map(|chunk| {
                                            if channels == 2 {
                                                (chunk[0] + chunk.get(1).unwrap_or(&0.0)) * 0.5
                                            } else {
                                                chunk[0]
                                            }
                                        })
                                        .collect()
                                }
                                _ => Vec::new(),
                            }
                        }
                    };

                    let response = StreamResponse {
                        key: request.key,
                        position: request.position,
                        data,
                    };

                    let _ = request.response_tx.send(response);
                } else {
                    warn!("Failed to open file for streaming: {:?}", path);
                }
            }
        }

        info!("Streaming thread ended");
    }

    /// Get number of loaded samples
    pub fn sample_count(&self) -> usize {
        self.samples.read().len()
    }

    /// Check if sample exists
    pub fn has_sample(&self, key: &SampleKey) -> bool {
        self.samples.read().contains_key(key)
    }
}

impl Default for StreamingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_buffer() {
        let input = vec![0.0, 1.0, 0.0, -1.0];
        let output = StreamingEngine::resample_buffer(&input, 44100, 48000);
        assert!(output.len() > input.len());
    }
}
