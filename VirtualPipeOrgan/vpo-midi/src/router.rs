//! MIDI routing - maps MIDI messages to organ actions

use vpo_core::{MidiNote, Keyboard};
use crate::{MidiMessage, cc};
use std::collections::HashMap;

/// Action to perform based on MIDI input
#[derive(Debug, Clone)]
pub enum OrganAction {
    /// Play a note on a keyboard
    NoteOn { keyboard: String, note: MidiNote, velocity: u8 },
    /// Release a note
    NoteOff { keyboard: String, note: MidiNote },
    /// Toggle a stop
    ToggleStop { stop_id: String },
    /// Set stop state
    SetStop { stop_id: String, active: bool },
    /// Set expression pedal value
    Expression { division: String, value: f32 },
    /// Set crescendo pedal value
    Crescendo { value: f32 },
    /// Trigger general piston
    GeneralPiston { number: u8 },
    /// Trigger divisional piston
    DivisionalPiston { division: String, number: u8 },
    /// General cancel
    GeneralCancel,
    /// Toggle coupler
    ToggleCoupler { coupler_id: String },
    /// Toggle tremulant
    ToggleTremulant { division: String },
    /// Transpose keyboard
    Transpose { keyboard: String, semitones: i8 },
    /// All notes off
    AllNotesOff,
}

/// Mapping from MIDI input to organ action
#[derive(Debug, Clone)]
pub struct MidiMapping {
    /// MIDI channel (None = any)
    pub channel: Option<u8>,
    /// MIDI message type and parameters
    pub trigger: MidiTrigger,
    /// Action to perform
    pub action: OrganAction,
}

/// What MIDI event triggers this mapping
#[derive(Debug, Clone)]
pub enum MidiTrigger {
    /// Note on/off for a keyboard
    Note { low: MidiNote, high: MidiNote },
    /// Specific note for a button/piston
    NoteButton { note: MidiNote },
    /// Control change
    ControlChange { controller: u8 },
    /// Program change
    ProgramChange { program: u8 },
}

/// MIDI router - routes incoming MIDI to organ actions
pub struct MidiRouter {
    mappings: Vec<MidiMapping>,
    keyboard_channels: HashMap<String, u8>,
    expression_ccs: HashMap<String, u8>,
}

impl MidiRouter {
    pub fn new() -> Self {
        Self {
            mappings: Vec::new(),
            keyboard_channels: HashMap::new(),
            expression_ccs: HashMap::new(),
        }
    }
    
    /// Add a keyboard-to-channel mapping
    pub fn map_keyboard(&mut self, keyboard: impl Into<String>, channel: u8) {
        self.keyboard_channels.insert(keyboard.into(), channel);
    }
    
    /// Add an expression pedal CC mapping
    pub fn map_expression(&mut self, division: impl Into<String>, cc: u8) {
        self.expression_ccs.insert(division.into(), cc);
    }
    
    /// Add a custom mapping
    pub fn add_mapping(&mut self, mapping: MidiMapping) {
        self.mappings.push(mapping);
    }
    
    /// Create default organ mappings
    pub fn default_organ_mappings(&mut self) {
        // Default keyboard channels
        self.map_keyboard("Hoofdwerk", 0);
        self.map_keyboard("Positief", 1);
        self.map_keyboard("Rugwerk", 2);
        self.map_keyboard("Bovenwerk", 3);
        self.map_keyboard("Pedaal", 4);
        
        // Default expression pedals
        self.map_expression("Hoofdwerk", cc::EXPRESSION);
        self.map_expression("Zwelwerk", cc::VOLUME);
        
        // CC 64 (sustain) as general cancel
        self.add_mapping(MidiMapping {
            channel: None,
            trigger: MidiTrigger::ControlChange { controller: cc::SUSTAIN },
            action: OrganAction::GeneralCancel,
        });
    }
    
    /// Route a MIDI message to organ actions
    pub fn route(&self, message: &MidiMessage) -> Vec<OrganAction> {
        let mut actions = Vec::new();
        
        match message {
            MidiMessage::NoteOn { channel, note, velocity } => {
                // Check keyboard mappings
                for (keyboard, &kb_channel) in &self.keyboard_channels {
                    if *channel == kb_channel {
                        actions.push(OrganAction::NoteOn {
                            keyboard: keyboard.clone(),
                            note: *note,
                            velocity: *velocity,
                        });
                    }
                }
                
                // Check custom note button mappings
                for mapping in &self.mappings {
                    if mapping.channel.map_or(true, |c| c == *channel) {
                        if let MidiTrigger::NoteButton { note: trigger_note } = &mapping.trigger {
                            if note == trigger_note {
                                actions.push(mapping.action.clone());
                            }
                        }
                    }
                }
            }
            
            MidiMessage::NoteOff { channel, note, .. } => {
                // Check keyboard mappings
                for (keyboard, &kb_channel) in &self.keyboard_channels {
                    if *channel == kb_channel {
                        actions.push(OrganAction::NoteOff {
                            keyboard: keyboard.clone(),
                            note: *note,
                        });
                    }
                }
            }
            
            MidiMessage::ControlChange { channel, controller, value } => {
                // Check expression mappings
                for (division, &exp_cc) in &self.expression_ccs {
                    if *controller == exp_cc {
                        actions.push(OrganAction::Expression {
                            division: division.clone(),
                            value: *value as f32 / 127.0,
                        });
                    }
                }
                
                // Check custom CC mappings
                for mapping in &self.mappings {
                    if mapping.channel.map_or(true, |c| c == *channel) {
                        if let MidiTrigger::ControlChange { controller: trigger_cc } = &mapping.trigger {
                            if controller == trigger_cc && *value > 63 {
                                actions.push(mapping.action.clone());
                            }
                        }
                    }
                }
                
                // Handle all notes off
                if *controller == cc::ALL_NOTES_OFF || *controller == cc::ALL_SOUND_OFF {
                    actions.push(OrganAction::AllNotesOff);
                }
            }
            
            MidiMessage::ProgramChange { channel, program } => {
                // Map program changes to pistons
                for mapping in &self.mappings {
                    if mapping.channel.map_or(true, |c| c == *channel) {
                        if let MidiTrigger::ProgramChange { program: trigger_prog } = &mapping.trigger {
                            if program == trigger_prog {
                                actions.push(mapping.action.clone());
                            }
                        }
                    }
                }
            }
            
            _ => {}
        }
        
        actions
    }
}

impl Default for MidiRouter {
    fn default() -> Self {
        let mut router = Self::new();
        router.default_organ_mappings();
        router
    }
}
