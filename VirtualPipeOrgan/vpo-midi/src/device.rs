//! MIDI device management

use midir::{MidiInput, MidiInputConnection, MidiInputPort, MidiOutput, MidiOutputConnection, MidiOutputPort};
use crossbeam_channel::{Sender, Receiver, bounded};
use parking_lot::Mutex;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn, error, debug};

use crate::MidiMessage;

#[derive(Error, Debug)]
pub enum MidiError {
    #[error("No MIDI devices found")]
    NoDevicesFound,
    
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    
    #[error("Failed to open device: {0}")]
    OpenError(String),
    
    #[error("Connection error: {0}")]
    ConnectionError(String),
}

/// Information about a MIDI device
#[derive(Debug, Clone)]
pub struct MidiDeviceInfo {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

/// MIDI input manager
pub struct MidiInputManager {
    midi_in: MidiInput,
    connections: Vec<MidiInputConnection<()>>,
    message_tx: Sender<MidiMessage>,
    message_rx: Receiver<MidiMessage>,
}

impl MidiInputManager {
    /// Create a new MIDI input manager
    pub fn new() -> Result<Self, MidiError> {
        let midi_in = MidiInput::new("VPO MIDI Input")
            .map_err(|e| MidiError::OpenError(e.to_string()))?;
        
        let (message_tx, message_rx) = bounded(1024);
        
        Ok(Self {
            midi_in,
            connections: Vec::new(),
            message_tx,
            message_rx,
        })
    }
    
    /// List available input devices
    pub fn list_devices(&self) -> Vec<MidiDeviceInfo> {
        self.midi_in.ports().iter().filter_map(|port| {
            self.midi_in.port_name(port).ok().map(|name| MidiDeviceInfo {
                name,
                is_input: true,
                is_output: false,
            })
        }).collect()
    }
    
    /// Connect to a device by name
    pub fn connect(&mut self, device_name: &str) -> Result<(), MidiError> {
        let ports = self.midi_in.ports();
        let port = ports.iter()
            .find(|p| self.midi_in.port_name(p).ok().as_deref() == Some(device_name))
            .ok_or_else(|| MidiError::DeviceNotFound(device_name.to_string()))?;
        
        self.connect_port(port.clone())
    }
    
    /// Connect to a port
    fn connect_port(&mut self, port: MidiInputPort) -> Result<(), MidiError> {
        let port_name = self.midi_in.port_name(&port)
            .unwrap_or_else(|_| "Unknown".to_string());
        
        let tx = self.message_tx.clone();
        
        // We need a new MidiInput for each connection
        let midi_in = MidiInput::new("VPO MIDI Input")
            .map_err(|e| MidiError::OpenError(e.to_string()))?;
        
        let connection = midi_in.connect(
            &port,
            "vpo-input",
            move |_timestamp, data, _| {
                if let Some(msg) = MidiMessage::parse(data) {
                    let _ = tx.try_send(msg);
                }
            },
            ()
        ).map_err(|e| MidiError::ConnectionError(e.to_string()))?;
        
        info!("Connected to MIDI input: {}", port_name);
        self.connections.push(connection);
        
        Ok(())
    }
    
    /// Connect to all available devices
    pub fn connect_all(&mut self) -> Result<usize, MidiError> {
        // Disconnect existing connections first to avoid MMSYSERR_ALLOCATED
        if !self.connections.is_empty() {
            info!("Disconnecting existing MIDI connections before reconnecting");
            self.connections.clear();
        }

        // Get fresh port list
        let midi_in = MidiInput::new("VPO MIDI Input Scan")
            .map_err(|e| MidiError::OpenError(e.to_string()))?;
        let ports: Vec<_> = midi_in.ports().into_iter().collect();
        let mut connected = 0;

        for port in ports {
            if let Ok(name) = midi_in.port_name(&port) {
                match self.connect(&name) {
                    Ok(()) => connected += 1,
                    Err(e) => warn!("Failed to connect to {}: {}", name, e),
                }
            }
        }

        Ok(connected)
    }
    
    /// Get the message receiver
    pub fn receiver(&self) -> Receiver<MidiMessage> {
        self.message_rx.clone()
    }

    /// Get a clone of the message sender (for BLE MIDI to feed into the same stream)
    pub fn sender(&self) -> Sender<MidiMessage> {
        self.message_tx.clone()
    }
    
    /// Disconnect all devices
    pub fn disconnect_all(&mut self) {
        self.connections.clear();
        info!("Disconnected all MIDI inputs");
    }
}

/// MIDI output manager
pub struct MidiOutputManager {
    midi_out: MidiOutput,
    connections: Vec<(String, MidiOutputConnection)>,
}

impl MidiOutputManager {
    /// Create a new MIDI output manager
    pub fn new() -> Result<Self, MidiError> {
        let midi_out = MidiOutput::new("VPO MIDI Output")
            .map_err(|e| MidiError::OpenError(e.to_string()))?;
        
        Ok(Self {
            midi_out,
            connections: Vec::new(),
        })
    }
    
    /// List available output devices
    pub fn list_devices(&self) -> Vec<MidiDeviceInfo> {
        self.midi_out.ports().iter().filter_map(|port| {
            self.midi_out.port_name(port).ok().map(|name| MidiDeviceInfo {
                name,
                is_input: false,
                is_output: true,
            })
        }).collect()
    }
    
    /// Connect to a device by name
    pub fn connect(&mut self, device_name: &str) -> Result<(), MidiError> {
        let ports = self.midi_out.ports();
        let port = ports.iter()
            .find(|p| self.midi_out.port_name(p).ok().as_deref() == Some(device_name))
            .ok_or_else(|| MidiError::DeviceNotFound(device_name.to_string()))?;
        
        let midi_out = MidiOutput::new("VPO MIDI Output")
            .map_err(|e| MidiError::OpenError(e.to_string()))?;
        
        let connection = midi_out.connect(port, "vpo-output")
            .map_err(|e| MidiError::ConnectionError(e.to_string()))?;
        
        info!("Connected to MIDI output: {}", device_name);
        self.connections.push((device_name.to_string(), connection));
        
        Ok(())
    }
    
    /// Send a MIDI message to all connected outputs
    pub fn send(&mut self, message: &MidiMessage) {
        let bytes = message.to_bytes();
        for (name, conn) in &mut self.connections {
            if let Err(e) = conn.send(&bytes) {
                warn!("Failed to send MIDI to {}: {}", name, e);
            }
        }
    }
    
    /// Disconnect all devices
    pub fn disconnect_all(&mut self) {
        self.connections.clear();
        info!("Disconnected all MIDI outputs");
    }
}
