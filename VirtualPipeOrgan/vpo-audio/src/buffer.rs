//! Audio buffer types

use bytemuck::{Pod, Zeroable};
use std::ops::{Index, IndexMut};

use crate::{BLOCK_SIZE, MAX_CHANNELS};

/// A stereo audio sample
#[derive(Debug, Clone, Copy, Default, Pod, Zeroable)]
#[repr(C)]
pub struct StereoSample {
    pub left: f32,
    pub right: f32,
}

impl StereoSample {
    pub fn new(left: f32, right: f32) -> Self {
        Self { left, right }
    }

    pub fn mono(value: f32) -> Self {
        Self {
            left: value,
            right: value,
        }
    }

    pub fn silence() -> Self {
        Self::default()
    }

    pub fn add(&mut self, other: StereoSample) {
        self.left += other.left;
        self.right += other.right;
    }

    pub fn scale(&mut self, factor: f32) {
        self.left *= factor;
        self.right *= factor;
    }

    pub fn pan(value: f32, pan: f32) -> Self {
        // Equal power panning
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        Self {
            left: value * angle.cos(),
            right: value * angle.sin(),
        }
    }
}

/// A multi-channel audio sample
#[derive(Debug, Clone, Copy)]
pub struct MultiChannelSample {
    pub channels: [f32; MAX_CHANNELS],
    pub num_channels: usize,
}

impl MultiChannelSample {
    pub fn new(num_channels: usize) -> Self {
        Self {
            channels: [0.0; MAX_CHANNELS],
            num_channels,
        }
    }

    pub fn stereo(left: f32, right: f32) -> Self {
        let mut s = Self::new(2);
        s.channels[0] = left;
        s.channels[1] = right;
        s
    }
}

/// Audio buffer for block processing
#[derive(Debug, Clone)]
pub struct AudioBuffer {
    data: Vec<f32>,
    channels: usize,
    frames: usize,
}

impl AudioBuffer {
    /// Create a new buffer with the specified number of channels and frames
    pub fn new(channels: usize, frames: usize) -> Self {
        Self {
            data: vec![0.0; channels * frames],
            channels,
            frames,
        }
    }

    /// Create a stereo buffer with the default block size
    pub fn stereo() -> Self {
        Self::new(2, BLOCK_SIZE)
    }

    /// Get the number of channels
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Get the number of frames
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Clear the buffer (fill with zeros)
    pub fn clear(&mut self) {
        self.data.fill(0.0);
    }

    /// Get a sample at (channel, frame)
    pub fn get(&self, channel: usize, frame: usize) -> f32 {
        self.data[channel * self.frames + frame]
    }

    /// Set a sample at (channel, frame)
    pub fn set(&mut self, channel: usize, frame: usize, value: f32) {
        self.data[channel * self.frames + frame] = value;
    }

    /// Add a value to a sample
    pub fn add(&mut self, channel: usize, frame: usize, value: f32) {
        self.data[channel * self.frames + frame] += value;
    }

    /// Get a channel slice
    pub fn channel(&self, channel: usize) -> &[f32] {
        let start = channel * self.frames;
        &self.data[start..start + self.frames]
    }

    /// Get a mutable channel slice
    pub fn channel_mut(&mut self, channel: usize) -> &mut [f32] {
        let start = channel * self.frames;
        &mut self.data[start..start + self.frames]
    }

    /// Mix another buffer into this one
    pub fn mix(&mut self, other: &AudioBuffer, gain: f32) {
        assert_eq!(self.channels, other.channels);
        assert_eq!(self.frames, other.frames);

        for (dst, src) in self.data.iter_mut().zip(other.data.iter()) {
            *dst += src * gain;
        }
    }

    /// Apply gain to all samples
    pub fn apply_gain(&mut self, gain: f32) {
        for sample in &mut self.data {
            *sample *= gain;
        }
    }

    /// Copy to interleaved output (for audio device)
    pub fn copy_to_interleaved(&self, output: &mut [f32]) {
        assert!(output.len() >= self.channels * self.frames);

        for frame in 0..self.frames {
            for channel in 0..self.channels {
                output[frame * self.channels + channel] = self.get(channel, frame);
            }
        }
    }
}

/// Ring buffer for delay effects and streaming
pub struct RingBuffer {
    data: Vec<f32>,
    write_pos: usize,
    size: usize,
}

impl RingBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0.0; size],
            write_pos: 0,
            size,
        }
    }

    pub fn write(&mut self, value: f32) {
        self.data[self.write_pos] = value;
        self.write_pos = (self.write_pos + 1) % self.size;
    }

    pub fn read(&self, delay: usize) -> f32 {
        let read_pos = (self.write_pos + self.size - delay) % self.size;
        self.data[read_pos]
    }

    pub fn read_interpolated(&self, delay: f32) -> f32 {
        let delay_floor = delay.floor() as usize;
        let frac = delay - delay_floor as f32;

        let s0 = self.read(delay_floor);
        let s1 = self.read(delay_floor + 1);

        s0 + frac * (s1 - s0)
    }

    pub fn clear(&mut self) {
        self.data.fill(0.0);
        self.write_pos = 0;
    }
}
