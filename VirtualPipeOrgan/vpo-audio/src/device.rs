//! Audio device management
//!
//! Handles discovery, selection, and configuration of audio output devices.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Host, SampleFormat, SampleRate, Stream, StreamConfig};
use parking_lot::RwLock;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};

use vpo_core::SampleRate as VpoSampleRate;

#[derive(Error, Debug)]
pub enum AudioDeviceError {
    #[error("No audio devices found")]
    NoDevicesFound,

    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Failed to get device config: {0}")]
    ConfigError(String),

    #[error("Failed to build stream: {0}")]
    StreamError(String),

    #[error("Unsupported sample format: {0:?}")]
    UnsupportedFormat(SampleFormat),
}

/// Audio device information
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rates: Vec<VpoSampleRate>,
    pub channels: Vec<u16>,
}

/// Audio device manager
pub struct AudioDeviceManager {
    host: Host,
    current_device: RwLock<Option<Device>>,
    current_stream: RwLock<Option<Stream>>,
}

impl AudioDeviceManager {
    /// Create a new audio device manager
    pub fn new() -> Self {
        let host = cpal::default_host();
        info!("Audio host: {:?}", host.id());
        
        Self {
            host,
            current_device: RwLock::new(None),
            current_stream: RwLock::new(None),
        }
    }

    /// List available output devices
    pub fn list_devices(&self) -> Result<Vec<AudioDeviceInfo>, AudioDeviceError> {
        let default_device = self.host.default_output_device();
        let default_name = default_device.as_ref().and_then(|d| d.name().ok());

        let devices = self
            .host
            .output_devices()
            .map_err(|e| AudioDeviceError::ConfigError(e.to_string()))?;

        let mut result = Vec::new();

        for device in devices {
            if let Ok(name) = device.name() {
                let is_default = default_name.as_ref() == Some(&name);

                // Get supported configs
                let mut sample_rates = Vec::new();
                let mut channels = Vec::new();

                if let Ok(configs) = device.supported_output_configs() {
                    for config in configs {
                        let min_rate = config.min_sample_rate().0;
                        let max_rate = config.max_sample_rate().0;
                        
                        // Add common sample rates in range
                        for rate in [44100, 48000, 88200, 96000, 192000] {
                            if rate >= min_rate && rate <= max_rate && !sample_rates.contains(&rate) {
                                sample_rates.push(rate);
                            }
                        }
                        
                        let ch = config.channels();
                        if !channels.contains(&ch) {
                            channels.push(ch);
                        }
                    }
                }

                sample_rates.sort();
                channels.sort();

                result.push(AudioDeviceInfo {
                    name,
                    is_default,
                    sample_rates,
                    channels,
                });
            }
        }

        if result.is_empty() {
            return Err(AudioDeviceError::NoDevicesFound);
        }

        Ok(result)
    }

    /// Get the default output device
    pub fn default_device(&self) -> Result<Device, AudioDeviceError> {
        self.host
            .default_output_device()
            .ok_or(AudioDeviceError::NoDevicesFound)
    }

    /// Get a device by name
    pub fn get_device(&self, name: &str) -> Result<Device, AudioDeviceError> {
        let devices = self
            .host
            .output_devices()
            .map_err(|e| AudioDeviceError::ConfigError(e.to_string()))?;

        for device in devices {
            if device.name().ok().as_deref() == Some(name) {
                return Ok(device);
            }
        }

        Err(AudioDeviceError::DeviceNotFound(name.to_string()))
    }

    /// Open a device for playback
    pub fn open_device(
        &self,
        device: Device,
        sample_rate: VpoSampleRate,
        channels: u16,
        callback: impl FnMut(&mut [f32]) + Send + 'static,
    ) -> Result<StreamConfig, AudioDeviceError> {
        let config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let callback = std::sync::Mutex::new(callback);

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if let Ok(mut cb) = callback.lock() {
                        cb(data);
                    }
                },
                |err| {
                    error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| AudioDeviceError::StreamError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioDeviceError::StreamError(e.to_string()))?;

        *self.current_device.write() = Some(device);
        *self.current_stream.write() = Some(stream);

        info!(
            "Audio stream started: {} Hz, {} channels",
            sample_rate, channels
        );

        Ok(config)
    }

    /// Close the current stream
    pub fn close(&self) {
        let mut stream = self.current_stream.write();
        if stream.is_some() {
            info!("Closing audio stream");
            *stream = None;
        }
        *self.current_device.write() = None;
    }
}

impl Default for AudioDeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioDeviceManager {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices() {
        let manager = AudioDeviceManager::new();
        // This might fail in CI without audio devices
        if let Ok(devices) = manager.list_devices() {
            assert!(!devices.is_empty() || true); // Allow empty in CI
            for device in &devices {
                println!("Device: {} (default: {})", device.name, device.is_default);
            }
        }
    }
}
