//! MIDI device management

use midir::{MidiInput, MidiInputConnection, MidiInputPort, MidiOutput, MidiOutputConnection, MidiOutputPort};
use crossbeam_channel::{Sender, Receiver, bounded};
use parking_lot::{Mutex, RwLock};
use std::sync::Arc;
use std::collections::HashMap;
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

/// Herken MMSYSERR_ALLOCATED en vergelijkbare "device bezet"-fouten (0.8.98).
fn lijkt_bezet(e: &MidiError) -> bool {
    let s = e.to_string().to_lowercase();
    s.contains("mmsyserr_allocated") || s.contains("in use") || s.contains("already allocated")
}

/// Information about a MIDI device
#[derive(Debug, Clone)]
pub struct MidiDeviceInfo {
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

/// Per-apparaat kanaaltoewijzing (0.9.26): voor een invoerapparaat dat eruit
/// moet komen op een vast MIDI-kanaal, ongeacht op welk kanaal het zelf zendt.
/// Zo bedienen twee klavieren die allebei op kanaal 1 zenden tóch verschillende
/// werken. Staat een apparaat niet in de map, dan blijft zijn bericht
/// ongewijzigd (het bestaande gedrag). Toegepast op het moment dat het bericht
/// binnenkomt, vóór de gedeelde (gemengde) stroom.
pub fn apply_device_channel(msg: MidiMessage, device: &str, map: &HashMap<String, u8>) -> MidiMessage {
    match map.get(device) {
        Some(&ch) => msg.with_channel(ch),
        None => msg,
    }
}

/// MIDI input manager
pub struct MidiInputManager {
    midi_in: MidiInput,
    connections: Vec<MidiInputConnection<()>>,
    message_tx: Sender<MidiMessage>,
    message_rx: Receiver<MidiMessage>,
    /// Apparaatnaam → afgedwongen MIDI-kanaal (0-15). Gedeeld met elke
    /// verbindings-callback, zodat een wijziging meteen doorwerkt.
    device_channels: Arc<RwLock<HashMap<String, u8>>>,
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
            device_channels: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Stel de per-apparaat kanaaltoewijzing in (apparaatnaam → MIDI-kanaal
    /// 0-15). Werkt direct door op bestaande verbindingen omdat de map gedeeld
    /// is. Een lege map = alles ongewijzigd (standaardgedrag).
    pub fn set_device_channels(&self, map: HashMap<String, u8>) {
        *self.device_channels.write() = map;
    }

    /// De huidige per-apparaat kanaaltoewijzing.
    pub fn device_channels(&self) -> HashMap<String, u8> {
        self.device_channels.read().clone()
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
        // Bestaande verbindingen eerst sluiten (0.8.98): anders claimt Windows
        // de poort opnieuw en komt die als MMSYSERR_ALLOCATED terug —
        // Piotr's melding "could not create Windows MM MIDI input port".
        if !self.connections.is_empty() {
            info!("Sluit {} bestaande MIDI-verbinding(en) voor herverbinden", self.connections.len());
            self.connections.clear();
            // Windows zelf laat de poort pas na een handvol ms vrij.
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        self.connect_port_met_retry(port.clone(), device_name)
    }

    /// `connect_port` met één nette herpoging (0.8.98). MMSYSERR_ALLOCATED (code
    /// 4 van winmm) betekent dat de poort nog bezet is — soms door onszelf na
    /// een eerdere sessie, soms door een ander programma. Twee pogingen met
    /// 150 ms ertussen vangen de eerste af; voor de tweede geven we een
    /// begrijpelijke melding terug.
    fn connect_port_met_retry(&mut self, port: MidiInputPort, device_name: &str) -> Result<(), MidiError> {
        match self.connect_port(port.clone()) {
            Ok(()) => Ok(()),
            Err(e) if lijkt_bezet(&e) => {
                warn!("MIDI-poort bezet, één herpoging na 150 ms: {}", e);
                std::thread::sleep(std::time::Duration::from_millis(150));
                self.connect_port(port).map_err(|e2| {
                    if lijkt_bezet(&e2) { MidiError::ConnectionError(format!(
                        "'{}' is in gebruik door een ander programma. Sluit dat programma en probeer opnieuw, of herstart Windows.",
                        device_name)) } else { e2 }
                })
            }
            Err(e) => Err(e),
        }
    }
    
    /// Connect to a port
    fn connect_port(&mut self, port: MidiInputPort) -> Result<(), MidiError> {
        let port_name = self.midi_in.port_name(&port)
            .unwrap_or_else(|_| "Unknown".to_string());
        
        let tx = self.message_tx.clone();
        // Per-apparaat kanaaltoewijzing (0.9.26): gedeelde map + de naam van dít
        // apparaat, zodat de callback het binnenkomende bericht op het juiste
        // kanaal kan zetten vóór het de gemengde stroom in gaat.
        let dev_map = self.device_channels.clone();
        let dev_name = port_name.clone();

        // We need a new MidiInput for each connection
        let mut midi_in = MidiInput::new("VPO MIDI Input")
            .map_err(|e| MidiError::OpenError(e.to_string()))?;
        // Niets wegfilteren: speeltafels die hun registers via System Exclusive
        // melden (Hauptwerk-protocol, Johannus) moeten die berichten ook als
        // schakelaar kunnen gebruiken (0.7.47). Standaard laat de driver sysex
        // en timing weg.
        midi_in.ignore(midir::Ignore::None);
        
        let connection = midi_in.connect(
            &port,
            "vpo-input",
            move |_timestamp, data, _| {
                if let Some(msg) = MidiMessage::parse(data) {
                    let msg = apply_device_channel(msg, &dev_name, &dev_map.read());
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

        // Per POORT verbinden i.p.v. terug via de naam: twee interfaces met een
        // identieke naam verbonden anders twee keer dezelfde eerste poort en de
        // tweede bleef dood (auditbevinding 27).
        let ports: Vec<_> = self.midi_in.ports().into_iter().collect();
        let mut connected = 0;

        for port in ports {
            let name = self.midi_in.port_name(&port).unwrap_or_else(|_| "?".to_string());
            match self.connect_port_met_retry(port, &name) {
                Ok(()) => connected += 1,
                Err(e) => warn!("Verbinden met {} mislukt: {}", name, e),
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

#[cfg(test)]
mod device_channel_tests {
    use super::*;
    use crate::MidiMessage;

    #[test]
    fn apparaat_zonder_toewijzing_blijft_ongewijzigd() {
        let map = HashMap::new();
        let m = apply_device_channel(MidiMessage::NoteOn { channel: 0, note: 60, velocity: 100 }, "Klavier A", &map);
        assert!(matches!(m, MidiMessage::NoteOn { channel: 0, note: 60, .. }));
    }

    #[test]
    fn apparaat_krijgt_zijn_toegewezen_kanaal() {
        let mut map = HashMap::new();
        map.insert("Klavier B".to_string(), 2u8);
        let m = apply_device_channel(MidiMessage::NoteOn { channel: 0, note: 64, velocity: 90 }, "Klavier B", &map);
        assert!(matches!(m, MidiMessage::NoteOn { channel: 2, note: 64, .. }));
        // Niet in de map → ongewijzigd.
        let m2 = apply_device_channel(MidiMessage::NoteOn { channel: 0, note: 64, velocity: 90 }, "Klavier A", &map);
        assert!(matches!(m2, MidiMessage::NoteOn { channel: 0, .. }));
    }

    #[test]
    fn twee_klavieren_op_kanaal_1_naar_verschillende_kanalen() {
        let mut map = HashMap::new();
        map.insert("Manuaal I".to_string(), 0u8);
        map.insert("Manuaal II".to_string(), 1u8);
        let a = apply_device_channel(MidiMessage::NoteOn { channel: 0, note: 60, velocity: 100 }, "Manuaal I", &map);
        let b = apply_device_channel(MidiMessage::NoteOn { channel: 0, note: 60, velocity: 100 }, "Manuaal II", &map);
        assert_eq!(a.channel(), Some(0));
        assert_eq!(b.channel(), Some(1));
    }

    #[test]
    fn controlechange_volgt_ook_de_toewijzing() {
        let mut map = HashMap::new();
        map.insert("Zwelpedaal".to_string(), 5u8);
        let m = apply_device_channel(MidiMessage::ControlChange { channel: 0, controller: 11, value: 64 }, "Zwelpedaal", &map);
        assert!(matches!(m, MidiMessage::ControlChange { channel: 5, controller: 11, .. }));
    }

    #[test]
    fn systeembericht_blijft_ongemoeid() {
        let mut map = HashMap::new();
        map.insert("X".to_string(), 3u8);
        assert!(matches!(apply_device_channel(MidiMessage::Clock, "X", &map), MidiMessage::Clock));
    }
}
