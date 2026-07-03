//! Main audio engine orchestrating all audio processing

use crossbeam_channel::{Receiver, Sender, bounded};
use parking_lot::RwLock;
use std::sync::Arc;
use vpo_core::{MidiNote, Velocity, SampleRate, Organ};

use crate::{
    buffer::AudioBuffer,
    device::AudioDeviceManager,
    mixer::Mixer,
    voice::{Voice, VoicePool},
    effects::{Tremulant, WindModel},
    convolution::StereoConvolutionReverb,
    BLOCK_SIZE, MAX_POLYPHONY,
};

/// Commands sent to the audio engine
pub enum AudioCommand {
    NoteOn { note: MidiNote, velocity: Velocity, stop_id: String },
    NoteOff { note: MidiNote, stop_id: String },
    AllNotesOff,
    SetMasterGain(f32),
    SetReverbMix(f32),
    SetTremulantActive(String, bool),
    LoadOrgan(Box<Organ>),
    Shutdown,
}

/// Events sent from the audio engine
pub enum AudioEvent {
    PeakMeter(f32, f32),
    VoiceCount(usize),
    Error(String),
}

/// Audio engine configuration
pub struct AudioEngineConfig {
    pub sample_rate: SampleRate,
    pub buffer_size: usize,
    pub channels: u16,
    pub max_polyphony: usize,
}

impl Default for AudioEngineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            buffer_size: BLOCK_SIZE,
            channels: 2,
            max_polyphony: MAX_POLYPHONY,
        }
    }
}

/// Shared state between audio thread and main thread
struct SharedState {
    voice_pool: VoicePool,
    mixer: Mixer,
    tremulants: Vec<Tremulant>,
    wind_model: WindModel,
    reverb: Option<StereoConvolutionReverb>,
    sample_rate: SampleRate,
    
    // Sample data (loaded from organ)
    samples: Vec<Vec<f32>>,
}

/// The main audio engine
pub struct AudioEngine {
    config: AudioEngineConfig,
    device_manager: AudioDeviceManager,
    command_tx: Sender<AudioCommand>,
    command_rx: Receiver<AudioCommand>,
    event_tx: Sender<AudioEvent>,
    event_rx: Receiver<AudioEvent>,
    state: Arc<RwLock<SharedState>>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl AudioEngine {
    /// Create a new audio engine
    pub fn new(config: AudioEngineConfig) -> Self {
        let (command_tx, command_rx) = bounded(256);
        let (event_tx, event_rx) = bounded(256);
        
        let state = Arc::new(RwLock::new(SharedState {
            voice_pool: VoicePool::new(config.max_polyphony, config.sample_rate),
            mixer: Mixer::new(config.buffer_size),
            tremulants: Vec::new(),
            wind_model: WindModel::new(config.sample_rate),
            reverb: None,
            sample_rate: config.sample_rate,
            samples: Vec::new(),
        }));
        
        Self {
            config,
            device_manager: AudioDeviceManager::new(),
            command_tx,
            command_rx,
            event_tx,
            event_rx,
            state,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    
    /// Get command sender for external control
    pub fn command_sender(&self) -> Sender<AudioCommand> {
        self.command_tx.clone()
    }
    
    /// Get event receiver for status updates
    pub fn event_receiver(&self) -> Receiver<AudioEvent> {
        self.event_rx.clone()
    }
    
    /// List available audio devices
    pub fn list_devices(&self) -> Result<Vec<crate::device::AudioDeviceInfo>, crate::device::AudioDeviceError> {
        self.device_manager.list_devices()
    }
    
    /// Start the audio engine with the default device
    pub fn start(&self) -> Result<(), crate::device::AudioDeviceError> {
        let device = self.device_manager.default_device()?;
        self.start_with_device(device)
    }
    
    /// Start the audio engine with a specific device
    pub fn start_with_device(&self, device: cpal::Device) -> Result<(), crate::device::AudioDeviceError> {
        let state = self.state.clone();
        let command_rx = self.command_rx.clone();
        let event_tx = self.event_tx.clone();
        let running = self.running.clone();
        let sample_rate = self.config.sample_rate;
        let buffer_size = self.config.buffer_size;
        
        running.store(true, std::sync::atomic::Ordering::SeqCst);
        
        self.device_manager.open_device(
            device,
            sample_rate,
            self.config.channels,
            move |output: &mut [f32]| {
                // Process commands
                while let Ok(cmd) = command_rx.try_recv() {
                    let mut state = state.write();
                    match cmd {
                        AudioCommand::NoteOn { note, velocity, stop_id } => {
                            if let Some(voice) = state.voice_pool.allocate() {
                                // TODO: Look up sample from organ definition
                                // For now, just trigger a placeholder
                                voice.note_on(
                                    note, velocity, stop_id, 
                                    0, // sample_index 
                                    0, 44100, 88200, // loop points
                                    1.0, 0.0, // rate, pan
                                    10.0, sample_rate // attack
                                );
                            }
                        }
                        AudioCommand::NoteOff { note, stop_id } => {
                            for voice in state.voice_pool.find_voices(note, &stop_id) {
                                voice.note_off(200.0, sample_rate);
                            }
                        }
                        AudioCommand::AllNotesOff => {
                            state.voice_pool.release_all(100.0);
                        }
                        AudioCommand::SetMasterGain(db) => {
                            state.mixer.set_master_gain_db(db);
                        }
                        AudioCommand::SetReverbMix(mix) => {
                            if let Some(ref mut reverb) = state.reverb {
                                reverb.set_mix(mix);
                            }
                        }
                        AudioCommand::SetTremulantActive(div, active) => {
                            // TODO: Find tremulant by division
                        }
                        AudioCommand::LoadOrgan(organ) => {
                            // TODO: Load samples from organ
                        }
                        AudioCommand::Shutdown => {
                            running.store(false, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                }
                
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    output.fill(0.0);
                    return;
                }
                
                // Process audio
                let mut state = state.write();
                
                // Clear output
                output.fill(0.0);
                
                // Process voices
                // TODO: Implement full voice processing with samples
                
                // Apply mixer
                state.mixer.process(output);
                
                // Send meter updates (throttled)
                let meters = state.mixer.peak_meters();
                let _ = event_tx.try_send(AudioEvent::PeakMeter(meters.0, meters.1));
                let _ = event_tx.try_send(AudioEvent::VoiceCount(state.voice_pool.active_count()));
            },
        )?;
        
        Ok(())
    }
    
    /// Stop the audio engine
    pub fn stop(&self) {
        self.running.store(false, std::sync::atomic::Ordering::SeqCst);
        self.device_manager.close();
    }
    
    /// Check if engine is running
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}
