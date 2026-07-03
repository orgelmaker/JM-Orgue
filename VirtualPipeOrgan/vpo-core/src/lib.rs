//! VPO Core - Core data structures for Virtual Pipe Organ
//! 
//! This crate contains the fundamental types and structures used throughout
//! the virtual pipe organ system, including organ definitions, stops, ranks,
//! and keyboard layouts.

pub mod organ;
pub mod stop;
pub mod rank;
pub mod keyboard;
pub mod coupler;
pub mod temperament;
pub mod error;

pub use organ::*;
pub use stop::*;
pub use rank::*;
pub use keyboard::*;
pub use coupler::*;
pub use temperament::*;
pub use error::*;

/// MIDI note number (0-127)
pub type MidiNote = u8;

/// MIDI velocity (0-127)  
pub type Velocity = u8;

/// Sample rate in Hz
pub type SampleRate = u32;

/// Audio sample value (-1.0 to 1.0)
pub type Sample = f32;

/// Frequency in Hz
pub type Frequency = f32;

/// Standard concert pitch A4
pub const A4_FREQUENCY: Frequency = 440.0;

/// Standard MIDI note for A4
pub const A4_MIDI_NOTE: MidiNote = 69;

/// Convert MIDI note to frequency using equal temperament
pub fn midi_to_frequency(note: MidiNote, a4_freq: Frequency) -> Frequency {
    a4_freq * 2.0_f32.powf((note as f32 - A4_MIDI_NOTE as f32) / 12.0)
}

/// Convert frequency to MIDI note (rounded)
pub fn frequency_to_midi(freq: Frequency, a4_freq: Frequency) -> MidiNote {
    let note = 12.0 * (freq / a4_freq).log2() + A4_MIDI_NOTE as f32;
    note.round() as MidiNote
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_to_frequency() {
        // A4 should be 440 Hz
        assert!((midi_to_frequency(69, 440.0) - 440.0).abs() < 0.01);
        // A3 should be 220 Hz
        assert!((midi_to_frequency(57, 440.0) - 220.0).abs() < 0.01);
        // A5 should be 880 Hz
        assert!((midi_to_frequency(81, 440.0) - 880.0).abs() < 0.01);
    }
}
