//! Voice management for sample playback
//!
//! A voice represents a single playing note, handling sample playback,
//! looping, and envelope processing.

use vpo_core::{MidiNote, Velocity, Sample, SampleRate};
use crate::buffer::StereoSample;

/// State of a voice
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    /// Voice is not playing
    Idle,
    /// Voice is in attack phase
    Attack,
    /// Voice is sustaining (looping)
    Sustain,
    /// Voice is in release phase
    Release,
    /// Voice has finished and can be recycled
    Finished,
}

/// A single playing voice
pub struct Voice {
    /// Current state
    pub state: VoiceState,
    
    /// MIDI note being played
    pub note: MidiNote,
    
    /// Velocity of the note
    pub velocity: Velocity,
    
    /// Stop ID this voice belongs to
    pub stop_id: String,
    
    /// Sample data reference (index into sample pool)
    pub sample_index: usize,
    
    /// Current playback position (in samples, with fractional part for interpolation)
    pub position: f64,
    
    /// Playback rate (1.0 = normal, 2.0 = octave up, etc.)
    pub playback_rate: f64,
    
    /// Loop start position
    pub loop_start: u64,
    
    /// Loop end position
    pub loop_end: u64,
    
    /// Sample end position (for release)
    pub sample_end: u64,
    
    /// Current gain (0.0 - 1.0)
    pub gain: f32,
    
    /// Target gain (for smoothing)
    pub target_gain: f32,
    
    /// Pan position (-1.0 = left, 0.0 = center, 1.0 = right)
    pub pan: f32,
    
    /// Release gain (decreasing during release)
    pub release_gain: f32,
    
    /// Release rate (gain decrease per sample)
    pub release_rate: f32,
    
    /// Attack progress (0.0 - 1.0)
    pub attack_progress: f32,
    
    /// Attack rate (progress increase per sample)
    pub attack_rate: f32,
    
    /// Age of the voice (for voice stealing)
    pub age: u64,
    
    /// Is this voice using the release sample?
    pub using_release_sample: bool,
}

impl Voice {
    /// Create a new idle voice
    pub fn new() -> Self {
        Self {
            state: VoiceState::Idle,
            note: 0,
            velocity: 0,
            stop_id: String::new(),
            sample_index: 0,
            position: 0.0,
            playback_rate: 1.0,
            loop_start: 0,
            loop_end: 0,
            sample_end: 0,
            gain: 0.0,
            target_gain: 1.0,
            pan: 0.0,
            release_gain: 1.0,
            release_rate: 0.0,
            attack_progress: 0.0,
            attack_rate: 0.0,
            age: 0,
            using_release_sample: false,
        }
    }

    /// Start playing a note
    pub fn note_on(
        &mut self,
        note: MidiNote,
        velocity: Velocity,
        stop_id: String,
        sample_index: usize,
        loop_start: u64,
        loop_end: u64,
        sample_end: u64,
        playback_rate: f64,
        pan: f32,
        attack_ms: f32,
        sample_rate: SampleRate,
    ) {
        self.state = VoiceState::Attack;
        self.note = note;
        self.velocity = velocity;
        self.stop_id = stop_id;
        self.sample_index = sample_index;
        self.position = 0.0;
        self.playback_rate = playback_rate;
        self.loop_start = loop_start;
        self.loop_end = loop_end;
        self.sample_end = sample_end;
        
        // Calculate gain from velocity (simple linear mapping)
        self.target_gain = velocity as f32 / 127.0;
        self.gain = 0.0;
        self.pan = pan;
        
        // Calculate attack rate
        let attack_samples = (attack_ms / 1000.0) * sample_rate as f32;
        self.attack_rate = if attack_samples > 0.0 {
            1.0 / attack_samples
        } else {
            1.0 // Instant attack
        };
        self.attack_progress = 0.0;
        
        self.release_gain = 1.0;
        self.age = 0;
        self.using_release_sample = false;
    }

    /// Release the note
    pub fn note_off(&mut self, release_ms: f32, sample_rate: SampleRate) {
        if self.state == VoiceState::Idle || self.state == VoiceState::Finished {
            return;
        }
        
        self.state = VoiceState::Release;
        
        // Calculate release rate
        let release_samples = (release_ms / 1000.0) * sample_rate as f32;
        self.release_rate = if release_samples > 0.0 {
            1.0 / release_samples
        } else {
            1.0 // Instant release
        };
    }

    /// Process a block of samples
    pub fn process(
        &mut self,
        sample_data: &[f32],
        output: &mut [StereoSample],
        sample_rate: SampleRate,
    ) {
        if self.state == VoiceState::Idle || self.state == VoiceState::Finished {
            return;
        }

        for out in output.iter_mut() {
            // Update attack envelope
            if self.state == VoiceState::Attack {
                self.attack_progress += self.attack_rate;
                if self.attack_progress >= 1.0 {
                    self.attack_progress = 1.0;
                    self.state = VoiceState::Sustain;
                }
                self.gain = self.target_gain * self.attack_progress;
            }

            // Update release envelope
            if self.state == VoiceState::Release {
                self.release_gain -= self.release_rate;
                if self.release_gain <= 0.0 {
                    self.release_gain = 0.0;
                    self.state = VoiceState::Finished;
                    return;
                }
            }

            // Get sample with linear interpolation
            let pos_floor = self.position as usize;
            let frac = self.position - pos_floor as f64;

            if pos_floor + 1 >= sample_data.len() {
                self.state = VoiceState::Finished;
                return;
            }

            let s0 = sample_data[pos_floor];
            let s1 = sample_data[pos_floor + 1];
            let sample = s0 + (s1 - s0) * frac as f32;

            // Apply gain and envelope
            let final_gain = self.gain * self.release_gain;
            let output_sample = sample * final_gain;

            // Apply panning
            let panned = StereoSample::pan(output_sample, self.pan);
            out.add(panned);

            // Advance position
            self.position += self.playback_rate;

            // Handle looping
            if self.state == VoiceState::Sustain || self.state == VoiceState::Attack {
                if self.position as u64 >= self.loop_end && self.loop_end > self.loop_start {
                    let loop_length = self.loop_end - self.loop_start;
                    self.position -= loop_length as f64;
                }
            }

            // Check for end of sample
            if self.position as u64 >= self.sample_end {
                self.state = VoiceState::Finished;
                return;
            }

            self.age += 1;
        }
    }

    /// Check if voice can be stolen
    pub fn can_steal(&self) -> bool {
        self.state == VoiceState::Release || self.age > 44100 // Over 1 second old
    }

    /// Get the priority for voice stealing (lower = more likely to be stolen)
    pub fn steal_priority(&self) -> u64 {
        match self.state {
            VoiceState::Idle | VoiceState::Finished => 0,
            VoiceState::Release => self.age / 2,
            _ => self.age,
        }
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

/// Voice pool for managing multiple voices
pub struct VoicePool {
    voices: Vec<Voice>,
    sample_rate: SampleRate,
}

impl VoicePool {
    /// Create a new voice pool with the specified number of voices
    pub fn new(num_voices: usize, sample_rate: SampleRate) -> Self {
        Self {
            voices: (0..num_voices).map(|_| Voice::new()).collect(),
            sample_rate,
        }
    }

    /// Find an available voice or steal one
    pub fn allocate(&mut self) -> Option<&mut Voice> {
        // Find the best voice index to use
        let mut best_idx: Option<usize> = None;
        let mut best_priority = u64::MAX;

        for (idx, voice) in self.voices.iter().enumerate() {
            match voice.state {
                VoiceState::Idle => {
                    // Idle voice is best - use immediately
                    best_idx = Some(idx);
                    break;
                }
                VoiceState::Finished => {
                    // Finished voice is second best
                    if best_idx.is_none() || matches!(self.voices.get(best_idx.unwrap()), Some(v) if v.state != VoiceState::Idle) {
                        best_idx = Some(idx);
                        best_priority = 0;
                    }
                }
                _ => {
                    // Check if this voice can be stolen
                    if voice.can_steal() {
                        let priority = voice.steal_priority();
                        if priority < best_priority {
                            best_priority = priority;
                            if best_idx.is_none() || matches!(self.voices.get(best_idx.unwrap()), Some(v) if v.state != VoiceState::Idle && v.state != VoiceState::Finished) {
                                best_idx = Some(idx);
                            }
                        }
                    }
                }
            }
        }

        best_idx.map(|idx| &mut self.voices[idx])
    }

    /// Find voices matching a note and stop
    pub fn find_voices(&mut self, note: MidiNote, stop_id: &str) -> Vec<&mut Voice> {
        let stop_id_owned = stop_id.to_string();
        self.voices.iter_mut().filter(move |v| {
            v.note == note
            && v.stop_id == stop_id_owned
            && v.state != VoiceState::Idle
            && v.state != VoiceState::Finished
        }).collect()
    }

    /// Get all active voices
    pub fn active_voices(&mut self) -> impl Iterator<Item = &mut Voice> {
        self.voices.iter_mut().filter(|v| {
            v.state != VoiceState::Idle && v.state != VoiceState::Finished
        })
    }

    /// Count active voices
    pub fn active_count(&self) -> usize {
        self.voices
            .iter()
            .filter(|v| v.state != VoiceState::Idle && v.state != VoiceState::Finished)
            .count()
    }

    /// Release all voices
    pub fn release_all(&mut self, release_ms: f32) {
        for voice in &mut self.voices {
            if voice.state != VoiceState::Idle && voice.state != VoiceState::Finished {
                voice.note_off(release_ms, self.sample_rate);
            }
        }
    }

    /// Kill all voices immediately
    pub fn kill_all(&mut self) {
        for voice in &mut self.voices {
            voice.state = VoiceState::Idle;
        }
    }
}
