//! MIDI message types

use vpo_core::{MidiNote, Velocity};

/// Parsed MIDI message
#[derive(Debug, Clone)]
pub enum MidiMessage {
    /// Note on (channel, note, velocity)
    NoteOn { channel: u8, note: MidiNote, velocity: Velocity },
    
    /// Note off (channel, note, velocity)
    NoteOff { channel: u8, note: MidiNote, velocity: Velocity },
    
    /// Control change (channel, controller, value)
    ControlChange { channel: u8, controller: u8, value: u8 },
    
    /// Program change (channel, program)
    ProgramChange { channel: u8, program: u8 },
    
    /// Pitch bend (channel, value: -8192 to 8191)
    PitchBend { channel: u8, value: i16 },
    
    /// Channel aftertouch (channel, pressure)
    ChannelAftertouch { channel: u8, pressure: u8 },
    
    /// Polyphonic aftertouch (channel, note, pressure)
    PolyAftertouch { channel: u8, note: MidiNote, pressure: u8 },
    
    /// System exclusive (data without F0 and F7)
    SysEx(Vec<u8>),
    
    /// Timing clock
    Clock,
    
    /// Start
    Start,
    
    /// Stop
    Stop,
    
    /// Continue
    Continue,
    
    /// Active sensing
    ActiveSensing,
    
    /// System reset
    SystemReset,
}

impl MidiMessage {
    /// Parse raw MIDI bytes into a message
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        
        let status = data[0];
        let msg_type = status & 0xF0;
        let channel = status & 0x0F;
        
        match msg_type {
            0x80 => {
                // Note Off
                if data.len() >= 3 {
                    Some(MidiMessage::NoteOff {
                        channel,
                        note: data[1],
                        velocity: data[2],
                    })
                } else {
                    None
                }
            }
            0x90 => {
                // Note On (velocity 0 = Note Off)
                if data.len() >= 3 {
                    if data[2] == 0 {
                        Some(MidiMessage::NoteOff {
                            channel,
                            note: data[1],
                            velocity: 0,
                        })
                    } else {
                        Some(MidiMessage::NoteOn {
                            channel,
                            note: data[1],
                            velocity: data[2],
                        })
                    }
                } else {
                    None
                }
            }
            0xA0 => {
                // Poly Aftertouch
                if data.len() >= 3 {
                    Some(MidiMessage::PolyAftertouch {
                        channel,
                        note: data[1],
                        pressure: data[2],
                    })
                } else {
                    None
                }
            }
            0xB0 => {
                // Control Change
                if data.len() >= 3 {
                    Some(MidiMessage::ControlChange {
                        channel,
                        controller: data[1],
                        value: data[2],
                    })
                } else {
                    None
                }
            }
            0xC0 => {
                // Program Change
                if data.len() >= 2 {
                    Some(MidiMessage::ProgramChange {
                        channel,
                        program: data[1],
                    })
                } else {
                    None
                }
            }
            0xD0 => {
                // Channel Aftertouch
                if data.len() >= 2 {
                    Some(MidiMessage::ChannelAftertouch {
                        channel,
                        pressure: data[1],
                    })
                } else {
                    None
                }
            }
            0xE0 => {
                // Pitch Bend
                if data.len() >= 3 {
                    let lsb = data[1] as i16;
                    let msb = data[2] as i16;
                    let value = ((msb << 7) | lsb) - 8192;
                    Some(MidiMessage::PitchBend { channel, value })
                } else {
                    None
                }
            }
            0xF0 => {
                // System messages
                match status {
                    0xF0 => {
                        // SysEx
                        let end = data.iter().position(|&b| b == 0xF7).unwrap_or(data.len());
                        Some(MidiMessage::SysEx(data[1..end].to_vec()))
                    }
                    0xF8 => Some(MidiMessage::Clock),
                    0xFA => Some(MidiMessage::Start),
                    0xFB => Some(MidiMessage::Continue),
                    0xFC => Some(MidiMessage::Stop),
                    0xFE => Some(MidiMessage::ActiveSensing),
                    0xFF => Some(MidiMessage::SystemReset),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    
    /// Convert message to raw MIDI bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            MidiMessage::NoteOff { channel, note, velocity } => {
                vec![0x80 | channel, *note, *velocity]
            }
            MidiMessage::NoteOn { channel, note, velocity } => {
                vec![0x90 | channel, *note, *velocity]
            }
            MidiMessage::PolyAftertouch { channel, note, pressure } => {
                vec![0xA0 | channel, *note, *pressure]
            }
            MidiMessage::ControlChange { channel, controller, value } => {
                vec![0xB0 | channel, *controller, *value]
            }
            MidiMessage::ProgramChange { channel, program } => {
                vec![0xC0 | channel, *program]
            }
            MidiMessage::ChannelAftertouch { channel, pressure } => {
                vec![0xD0 | channel, *pressure]
            }
            MidiMessage::PitchBend { channel, value } => {
                let adjusted = (*value + 8192) as u16;
                let lsb = (adjusted & 0x7F) as u8;
                let msb = ((adjusted >> 7) & 0x7F) as u8;
                vec![0xE0 | channel, lsb, msb]
            }
            MidiMessage::SysEx(data) => {
                let mut bytes = vec![0xF0];
                bytes.extend(data);
                bytes.push(0xF7);
                bytes
            }
            MidiMessage::Clock => vec![0xF8],
            MidiMessage::Start => vec![0xFA],
            MidiMessage::Continue => vec![0xFB],
            MidiMessage::Stop => vec![0xFC],
            MidiMessage::ActiveSensing => vec![0xFE],
            MidiMessage::SystemReset => vec![0xFF],
        }
    }
    
    /// Get the channel (0-15) if applicable
    pub fn channel(&self) -> Option<u8> {
        match self {
            MidiMessage::NoteOn { channel, .. } |
            MidiMessage::NoteOff { channel, .. } |
            MidiMessage::ControlChange { channel, .. } |
            MidiMessage::ProgramChange { channel, .. } |
            MidiMessage::PitchBend { channel, .. } |
            MidiMessage::ChannelAftertouch { channel, .. } |
            MidiMessage::PolyAftertouch { channel, .. } => Some(*channel),
            _ => None,
        }
    }
}

/// Common MIDI Control Change numbers
pub mod cc {
    pub const MODULATION: u8 = 1;
    pub const BREATH: u8 = 2;
    pub const FOOT: u8 = 4;
    pub const VOLUME: u8 = 7;
    pub const BALANCE: u8 = 8;
    pub const PAN: u8 = 10;
    pub const EXPRESSION: u8 = 11;
    pub const SUSTAIN: u8 = 64;
    pub const PORTAMENTO: u8 = 65;
    pub const SOSTENUTO: u8 = 66;
    pub const SOFT: u8 = 67;
    pub const LEGATO: u8 = 68;
    pub const ALL_SOUND_OFF: u8 = 120;
    pub const RESET_ALL: u8 = 121;
    pub const ALL_NOTES_OFF: u8 = 123;
}
