//! Audio mixer
//!
//! Mixes multiple audio sources with gain, panning, and effects.

use crate::buffer::{AudioBuffer, StereoSample};
use vpo_core::Sample;

/// Channel strip with gain and effects
pub struct ChannelStrip {
    /// Channel name
    pub name: String,
    
    /// Gain in linear (0.0 - 1.0+)
    pub gain: f32,
    
    /// Target gain for smoothing
    pub target_gain: f32,
    
    /// Pan position (-1.0 to 1.0)
    pub pan: f32,
    
    /// Mute state
    pub muted: bool,
    
    /// Solo state
    pub solo: bool,
    
    /// Internal buffer
    buffer: AudioBuffer,
    
    /// Gain smoothing coefficient
    smoothing: f32,
}

impl ChannelStrip {
    pub fn new(name: impl Into<String>, buffer_size: usize) -> Self {
        Self {
            name: name.into(),
            gain: 1.0,
            target_gain: 1.0,
            pan: 0.0,
            muted: false,
            solo: false,
            buffer: AudioBuffer::new(2, buffer_size),
            smoothing: 0.001,
        }
    }

    /// Set gain in dB
    pub fn set_gain_db(&mut self, db: f32) {
        self.target_gain = db_to_linear(db);
    }

    /// Get the internal buffer for writing
    pub fn buffer_mut(&mut self) -> &mut AudioBuffer {
        &mut self.buffer
    }

    /// Process and mix to output
    pub fn process(&mut self, output: &mut AudioBuffer) {
        if self.muted {
            self.buffer.clear();
            return;
        }

        // Smooth gain changes
        self.gain += (self.target_gain - self.gain) * self.smoothing;

        // Calculate pan gains (equal power)
        let pan_angle = (self.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let left_gain = self.gain * pan_angle.cos();
        let right_gain = self.gain * pan_angle.sin();

        // Mix to output
        let frames = self.buffer.frames();
        for i in 0..frames {
            output.add(0, i, self.buffer.get(0, i) * left_gain);
            output.add(1, i, self.buffer.get(1, i) * right_gain);
        }

        // Clear for next block
        self.buffer.clear();
    }
}

/// Main audio mixer
pub struct Mixer {
    /// Input channels (divisions, effects returns, etc.)
    channels: Vec<ChannelStrip>,
    
    /// Master output gain
    master_gain: f32,
    
    /// Master output buffer
    master_buffer: AudioBuffer,
    
    /// Peak meters (left, right)
    peak_meters: (f32, f32),
    
    /// Peak hold time in samples
    peak_hold: usize,
    peak_hold_counter: usize,
}

impl Mixer {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            channels: Vec::new(),
            master_gain: 1.0,
            master_buffer: AudioBuffer::new(2, buffer_size),
            peak_meters: (0.0, 0.0),
            peak_hold: 44100, // ~1 second at 44.1kHz
            peak_hold_counter: 0,
        }
    }

    /// Add a channel strip
    pub fn add_channel(&mut self, name: impl Into<String>, buffer_size: usize) -> usize {
        let index = self.channels.len();
        self.channels.push(ChannelStrip::new(name, buffer_size));
        index
    }

    /// Get a channel by index
    pub fn channel(&self, index: usize) -> Option<&ChannelStrip> {
        self.channels.get(index)
    }

    /// Get a mutable channel by index
    pub fn channel_mut(&mut self, index: usize) -> Option<&mut ChannelStrip> {
        self.channels.get_mut(index)
    }

    /// Set master gain in dB
    pub fn set_master_gain_db(&mut self, db: f32) {
        self.master_gain = db_to_linear(db);
    }

    /// Check if any channel is soloed
    fn any_solo(&self) -> bool {
        self.channels.iter().any(|ch| ch.solo)
    }

    /// Process all channels and produce output
    pub fn process(&mut self, output: &mut [f32]) {
        self.master_buffer.clear();

        let any_solo = self.any_solo();

        // Mix all channels
        for channel in &mut self.channels {
            // Skip non-solo channels if any channel is soloed
            if any_solo && !channel.solo {
                channel.buffer.clear();
                continue;
            }
            channel.process(&mut self.master_buffer);
        }

        // Apply master gain and copy to output
        self.master_buffer.apply_gain(self.master_gain);

        // Update peak meters
        let mut left_peak = 0.0_f32;
        let mut right_peak = 0.0_f32;
        
        let frames = self.master_buffer.frames();
        for i in 0..frames {
            let left = self.master_buffer.get(0, i);
            let right = self.master_buffer.get(1, i);
            left_peak = left_peak.max(left.abs());
            right_peak = right_peak.max(right.abs());
        }

        // Peak hold
        if left_peak > self.peak_meters.0 || right_peak > self.peak_meters.1 {
            self.peak_meters = (left_peak, right_peak);
            self.peak_hold_counter = 0;
        } else {
            self.peak_hold_counter += frames;
            if self.peak_hold_counter > self.peak_hold {
                // Decay peaks
                self.peak_meters.0 *= 0.99;
                self.peak_meters.1 *= 0.99;
            }
        }

        // Soft clipping (tanh-like)
        for i in 0..frames {
            let left = soft_clip(self.master_buffer.get(0, i));
            let right = soft_clip(self.master_buffer.get(1, i));
            self.master_buffer.set(0, i, left);
            self.master_buffer.set(1, i, right);
        }

        // Copy to interleaved output
        self.master_buffer.copy_to_interleaved(output);
    }

    /// Get current peak meter values (0.0 - 1.0+)
    pub fn peak_meters(&self) -> (f32, f32) {
        self.peak_meters
    }

    /// Get peak meters in dB
    pub fn peak_meters_db(&self) -> (f32, f32) {
        (linear_to_db(self.peak_meters.0), linear_to_db(self.peak_meters.1))
    }
}

/// Convert dB to linear gain
pub fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Convert linear gain to dB
pub fn linear_to_db(linear: f32) -> f32 {
    if linear <= 0.0 {
        -96.0 // Floor
    } else {
        20.0 * linear.log10()
    }
}

/// Soft clipping function
fn soft_clip(x: f32) -> f32 {
    if x.abs() < 0.5 {
        x
    } else {
        x.signum() * (0.5 + (1.0 - (-2.0 * (x.abs() - 0.5)).exp()) * 0.5)
    }
}
