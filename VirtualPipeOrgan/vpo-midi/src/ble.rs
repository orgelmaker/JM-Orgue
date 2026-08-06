//! Bluetooth LE MIDI client
//!
//! Implements direct BLE MIDI 1.0 over GATT (no OS pairing required for input).
//! Uses a dedicated worker thread with its own tokio runtime to isolate from
//! Tauri's main thread (avoids Windows WinRT COM threading conflicts).
//!
//! BLE MIDI Service: 03B80E5A-EDE8-4B33-A751-6CE34EC4C700
//! BLE MIDI Characteristic: 7772E5DB-3868-4112-A1A9-F2669D106BF3

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, CharPropFlags};
use btleplug::platform::{Manager, Peripheral};
use crossbeam_channel::{bounded, Sender, Receiver};
use futures::stream::StreamExt;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Builder;
use tracing::{info, warn, error, debug};
use uuid::Uuid;

use crate::MidiMessage;
use vpo_core::{MidiNote, Velocity};

const BLE_MIDI_SERVICE: Uuid = Uuid::from_u128(0x03B80E5A_EDE8_4B33_A751_6CE34EC4C700);
const BLE_MIDI_CHAR: Uuid = Uuid::from_u128(0x7772E5DB_3868_4112_A1A9_F2669D106BF3);

#[derive(Debug, Clone)]
pub struct BleMidiDevice {
    pub id: String,
    pub name: String,
    pub address: String,
    pub rssi: Option<i16>,
    pub connected: bool,
}

/// Commands sent to the BLE worker thread
enum BleCommand {
    Scan { duration_ms: u64, response: Sender<Result<Vec<BleMidiDevice>, String>> },
    Connect { device_id: String, response: Sender<Result<String, String>> },
    Disconnect { device_id: String, response: Sender<Result<(), String>> },
    Shutdown,
}

/// BLE MIDI manager - communicates with worker thread via channel.
/// Tauri commands never block on async work directly - they send a command
/// and wait on a channel with a timeout.
pub struct BleMidiManager {
    cmd_tx: Sender<BleCommand>,
    connected: Arc<Mutex<HashMap<String, String>>>, // id -> name (mirror of worker state)
}

impl BleMidiManager {
    pub fn new(message_tx: Sender<MidiMessage>) -> Result<Self, String> {
        let (cmd_tx, cmd_rx) = bounded::<BleCommand>(16);
        let connected = Arc::new(Mutex::new(HashMap::new()));
        let connected_for_thread = connected.clone();

        // Spawn dedicated worker thread with its own tokio runtime
        std::thread::Builder::new()
            .name("ble-midi-worker".to_string())
            .spawn(move || {
                let runtime = match Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(e) => {
                        error!("BLE: failed to create runtime: {}", e);
                        return;
                    }
                };

                runtime.block_on(async move {
                    ble_worker_loop(cmd_rx, message_tx, connected_for_thread).await;
                });
                info!("BLE worker thread shut down");
            })
            .map_err(|e| format!("Kan BLE thread niet starten: {}", e))?;

        Ok(Self { cmd_tx, connected })
    }

    pub fn scan(&self, duration_ms: u64) -> Result<Vec<BleMidiDevice>, String> {
        let (tx, rx) = bounded(1);
        self.cmd_tx
            .send(BleCommand::Scan { duration_ms, response: tx })
            .map_err(|e| format!("BLE worker offline: {}", e))?;
        // Wait at most duration + 5s for scan to complete
        rx.recv_timeout(Duration::from_millis(duration_ms + 5000))
            .map_err(|_| "Scan timed out".to_string())?
    }

    pub fn connect(&self, device_id: &str) -> Result<String, String> {
        let (tx, rx) = bounded(1);
        self.cmd_tx
            .send(BleCommand::Connect {
                device_id: device_id.to_string(),
                response: tx,
            })
            .map_err(|e| format!("BLE worker offline: {}", e))?;
        let result = rx
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| "Verbinden duurde te lang (>15s)".to_string())??;
        // Mirror to local state
        self.connected.lock().insert(device_id.to_string(), result.clone());
        Ok(result)
    }

    pub fn disconnect(&self, device_id: &str) -> Result<(), String> {
        let (tx, rx) = bounded(1);
        self.cmd_tx
            .send(BleCommand::Disconnect {
                device_id: device_id.to_string(),
                response: tx,
            })
            .map_err(|e| format!("BLE worker offline: {}", e))?;
        let result = rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "Disconnect timeout".to_string())?;
        self.connected.lock().remove(device_id);
        result
    }

    pub fn connected_devices(&self) -> Vec<(String, String)> {
        self.connected
            .lock()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

impl Drop for BleMidiManager {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(BleCommand::Shutdown);
    }
}

/// Worker loop running in dedicated thread with own runtime
async fn ble_worker_loop(
    cmd_rx: Receiver<BleCommand>,
    message_tx: Sender<MidiMessage>,
    connected_state: Arc<Mutex<HashMap<String, String>>>,
) {
    info!("BLE worker started");

    // Lazy-initialize Bluetooth manager (avoid blocking at startup)
    let mut manager: Option<Manager> = None;
    let mut peripherals: HashMap<String, Peripheral> = HashMap::new();
    let mut connected_peripherals: HashMap<String, Peripheral> = HashMap::new();

    // Process commands. Use blocking_recv via spawn_blocking to avoid blocking the runtime.
    loop {
        let cmd = match tokio::task::spawn_blocking({
            let cmd_rx = cmd_rx.clone();
            move || cmd_rx.recv()
        })
        .await
        {
            Ok(Ok(c)) => c,
            _ => break,
        };

        match cmd {
            BleCommand::Shutdown => break,

            BleCommand::Scan { duration_ms, response } => {
                let result = scan_impl(&mut manager, &mut peripherals, duration_ms).await;
                let _ = response.send(result);
            }

            BleCommand::Connect { device_id, response } => {
                let result = connect_impl(
                    &peripherals,
                    &mut connected_peripherals,
                    &device_id,
                    message_tx.clone(),
                )
                .await;
                if let Ok(ref name) = result {
                    connected_state.lock().insert(device_id.clone(), name.clone());
                }
                let _ = response.send(result);
            }

            BleCommand::Disconnect { device_id, response } => {
                let result = disconnect_impl(&mut connected_peripherals, &device_id).await;
                connected_state.lock().remove(&device_id);
                let _ = response.send(result);
            }
        }
    }

    // Cleanup: disconnect all
    for (_, p) in connected_peripherals.drain() {
        let _ = p.disconnect().await;
    }
    info!("BLE worker exiting");
}

async fn scan_impl(
    manager_slot: &mut Option<Manager>,
    peripherals: &mut HashMap<String, Peripheral>,
    duration_ms: u64,
) -> Result<Vec<BleMidiDevice>, String> {
    if manager_slot.is_none() {
        let m = Manager::new()
            .await
            .map_err(|e| format!("Bluetooth manager error: {}. Staat Bluetooth aan?", e))?;
        *manager_slot = Some(m);
    }
    let manager = manager_slot.as_ref().unwrap();

    let adapters = manager
        .adapters()
        .await
        .map_err(|e| format!("Geen Bluetooth adapter: {}", e))?;
    let central = adapters
        .into_iter()
        .next()
        .ok_or("Geen Bluetooth adapter gevonden. Controleer dat Bluetooth aanstaat.")?;

    info!("BLE: scan start ({}ms)", duration_ms);
    central
        .start_scan(ScanFilter::default())
        .await
        .map_err(|e| format!("Scan starten mislukt: {}", e))?;

    tokio::time::sleep(Duration::from_millis(duration_ms)).await;

    let _ = central.stop_scan().await;

    let found_peripherals = central
        .peripherals()
        .await
        .map_err(|e| format!("Peripherals ophalen mislukt: {}", e))?;

    let mut found = Vec::new();
    peripherals.clear();

    for p in found_peripherals {
        let props = match p.properties().await {
            Ok(Some(props)) => props,
            _ => continue,
        };

        let has_midi_service = props.services.contains(&BLE_MIDI_SERVICE);
        let name = props
            .local_name
            .clone()
            .unwrap_or_else(|| "Onbekend".to_string());
        let name_has_midi = name.to_lowercase().contains("midi")
            || name.to_lowercase().contains("esp32")
            || name.to_lowercase().contains("widi");

        if !has_midi_service && !name_has_midi {
            continue;
        }

        let id = p.id().to_string();
        found.push(BleMidiDevice {
            id: id.clone(),
            name,
            address: props.address.to_string(),
            rssi: props.rssi,
            connected: false,
        });
        peripherals.insert(id, p);
    }

    info!("BLE: found {} candidate(s)", found.len());
    Ok(found)
}

async fn connect_impl(
    peripherals: &HashMap<String, Peripheral>,
    connected: &mut HashMap<String, Peripheral>,
    device_id: &str,
    message_tx: Sender<MidiMessage>,
) -> Result<String, String> {
    let peripheral = peripherals
        .get(device_id)
        .cloned()
        .ok_or_else(|| format!("Apparaat niet in scan-lijst: {}", device_id))?;

    info!("BLE: connect start for {}", device_id);
    // Connect with timeout
    tokio::time::timeout(Duration::from_secs(8), peripheral.connect())
        .await
        .map_err(|_| "Verbinden duurde te lang".to_string())?
        .map_err(|e| format!("Verbinden mislukt: {}", e))?;
    info!("BLE: connected, waiting for connection to stabilize");

    // Brief stabilization delay (Windows BLE often needs this)
    tokio::time::sleep(Duration::from_millis(300)).await;

    info!("BLE: discovering services");
    tokio::time::timeout(Duration::from_secs(5), peripheral.discover_services())
        .await
        .map_err(|_| "Service discovery timeout".to_string())?
        .map_err(|e| format!("Service discovery mislukt: {}", e))?;

    let chars = peripheral.characteristics();
    info!("BLE: discovered {} characteristics", chars.len());
    for c in chars.iter() {
        info!("  char {}: props {:?}", c.uuid, c.properties);
    }

    let midi_char = chars
        .iter()
        .find(|c| c.uuid == BLE_MIDI_CHAR)
        .ok_or("Geen BLE MIDI characteristic gevonden — is dit echt een BLE MIDI apparaat?")?
        .clone();

    if !midi_char.properties.contains(CharPropFlags::NOTIFY) {
        return Err("Apparaat ondersteunt geen notificaties".into());
    }

    // CRITICAL ORDER on Windows BLE (btleplug):
    // 1. Subscribe to enable notifications at the GATT level
    // 2. Open the notifications stream (await is async, returns the Stream)
    // 3. Spawn the handler that reads from the stream
    //
    // Doing it in any other order can cause HRESULT 0x80000013 ("object closed")
    // because Windows WinRT closes the GattCharacteristic before we subscribe.

    info!("BLE: subscribing to MIDI characteristic");
    // Try subscribe with up to 3 retries (Windows BLE is flaky)
    let mut last_err: Option<String> = None;
    let mut subscribed = false;
    for attempt in 1..=3 {
        match tokio::time::timeout(
            Duration::from_secs(5),
            peripheral.subscribe(&midi_char),
        )
        .await
        {
            Ok(Ok(())) => {
                info!("BLE: subscribe succeeded on attempt {}", attempt);
                subscribed = true;
                break;
            }
            Ok(Err(e)) => {
                let msg = format!("{}", e);
                warn!("BLE: subscribe attempt {} failed: {}", attempt, msg);
                last_err = Some(msg);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(_) => {
                warn!("BLE: subscribe attempt {} timeout", attempt);
                last_err = Some("timeout".to_string());
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
    if !subscribed {
        return Err(format!(
            "Subscribe mislukt na 3 pogingen: {}. Probeer: device opnieuw aan/uit, of opnieuw scannen.",
            last_err.unwrap_or_default()
        ));
    }

    // Now open the notification stream
    info!("BLE: opening notification stream");
    let mut notifications_stream = tokio::time::timeout(
        Duration::from_secs(3),
        peripheral.notifications(),
    )
    .await
    .map_err(|_| "Notifications stream timeout".to_string())?
    .map_err(|e| format!("Notifications stream error: {}", e))?;

    info!("BLE: subscribed and stream open — should receive MIDI now");

    // Spawn the handler that reads from the stream
    let tx_clone = message_tx.clone();
    tokio::spawn(async move {
        info!("BLE: notification handler task started");
        let mut count: u64 = 0;
        while let Some(data) = notifications_stream.next().await {
            count += 1;
            if data.uuid == BLE_MIDI_CHAR {
                info!("BLE MIDI #{}: {} bytes: {:02X?}", count, data.value.len(), data.value);
                parse_ble_midi_packet(&data.value, &tx_clone);
            } else {
                debug!("BLE: notification on other char {}", data.uuid);
            }
        }
        warn!("BLE: notification stream ENDED after {} messages", count);
    });

    let props = peripheral.properties().await.ok().flatten();
    let name = props
        .and_then(|p| p.local_name)
        .unwrap_or_else(|| device_id.to_string());

    connected.insert(device_id.to_string(), peripheral);
    Ok(name)
}

async fn disconnect_impl(
    connected: &mut HashMap<String, Peripheral>,
    device_id: &str,
) -> Result<(), String> {
    if let Some(p) = connected.remove(device_id) {
        let _ = tokio::time::timeout(Duration::from_secs(3), p.disconnect()).await;
    }
    Ok(())
}

/// Parse BLE MIDI packet and forward MIDI messages.
///
/// BLE MIDI packet format (Apple BLE-MIDI 1.0 spec):
/// - byte 0:    Header byte (0x80 | timestampHigh6)
/// - byte 1:    First timestamp byte (0x80 | timestampLow7)
/// - byte 2..N: MIDI bytes (with optional intermediate timestamp bytes for additional events)
///
/// State machine: after header, we expect timestamp+midi pairs.
/// MIDI status bytes have high bit set (0x80-0xFF).
/// Data bytes have high bit clear (0x00-0x7F).
/// Timestamp bytes have high bit set, so we distinguish via state.
fn parse_ble_midi_packet(data: &[u8], tx: &Sender<MidiMessage>) {
    if data.len() < 3 {
        debug!("BLE MIDI: packet too short ({} bytes)", data.len());
        return;
    }

    // Skip header byte (data[0])
    let mut i = 1;
    let mut running_status: u8 = 0;
    // Track whether we just consumed a timestamp (so next high-bit byte is a status, not another timestamp)
    let mut after_timestamp = false;

    while i < data.len() {
        let b = data[i];

        if b & 0x80 != 0 {
            // High bit set: either timestamp or status byte
            if !after_timestamp {
                // This must be a timestamp byte (start of an event)
                after_timestamp = true;
                i += 1;
                continue;
            }
            // We just had a timestamp; this is a status byte
            after_timestamp = false;
            if b >= 0xF8 {
                // System real-time message (single byte)
                i += 1;
                continue;
            }
            if b == 0xF0 {
                // SysEx: payloadbytes overslaan t/m EOX (0xF7) — anders werden
                // ze via running status als spook-events geëmit (audit 53).
                running_status = 0;
                i += 1;
                while i < data.len() && data[i] != 0xF7 { i += 1; }
                i += 1; // voorbij EOX (of einde pakket)
                continue;
            }
            if b >= 0xF0 {
                // System common (F1-F7): status + eventuele databytes overslaan.
                running_status = 0;
                let skip = match b { 0xF1 | 0xF3 => 1, 0xF2 => 2, _ => 0 };
                i += 1 + skip;
                continue;
            }
            // Channel voice status (0x80-0xEF)
            running_status = b;
            i += 1;
            // Now read data bytes
            let need = data_bytes_for_status(running_status);
            if i + need > data.len() {
                warn!("BLE MIDI: truncated event for status 0x{:02X}", running_status);
                break;
            }
            emit_midi_event(running_status, &data[i..i + need], tx);
            i += need;
        } else {
            // Data byte (high bit clear) — running status
            after_timestamp = false;
            if running_status == 0 {
                i += 1;
                continue;
            }
            let need = data_bytes_for_status(running_status);
            if i + need > data.len() {
                break;
            }
            emit_midi_event(running_status, &data[i..i + need], tx);
            i += need;
        }
    }
}

fn data_bytes_for_status(status: u8) -> usize {
    match status & 0xF0 {
        0xC0 | 0xD0 => 1, // Program Change, Channel Aftertouch
        _ => 2,           // NoteOn/Off, CC, Poly Aftertouch, Pitch Bend
    }
}

fn emit_midi_event(status: u8, data: &[u8], tx: &Sender<MidiMessage>) {
    let channel = status & 0x0F;
    let msg = match status & 0xF0 {
        0x80 => MidiMessage::NoteOff {
            channel,
            note: (data[0] & 0x7F) as MidiNote,
            velocity: (data[1] & 0x7F) as Velocity,
        },
        0x90 => {
            let note = (data[0] & 0x7F) as MidiNote;
            let vel = data[1] & 0x7F;
            if vel == 0 {
                MidiMessage::NoteOff { channel, note, velocity: 0 }
            } else {
                MidiMessage::NoteOn { channel, note, velocity: vel as Velocity }
            }
        }
        0xA0 => MidiMessage::PolyAftertouch {
            channel,
            note: (data[0] & 0x7F) as MidiNote,
            pressure: data[1] & 0x7F,
        },
        0xB0 => MidiMessage::ControlChange {
            channel,
            controller: data[0] & 0x7F,
            value: data[1] & 0x7F,
        },
        0xC0 => MidiMessage::ProgramChange {
            channel,
            program: data[0] & 0x7F,
        },
        0xD0 => MidiMessage::ChannelAftertouch {
            channel,
            pressure: data[0] & 0x7F,
        },
        0xE0 => {
            let lsb = data[0] & 0x7F;
            let msb = data[1] & 0x7F;
            let value = (((msb as i16) << 7) | (lsb as i16)) - 8192;
            MidiMessage::PitchBend { channel, value }
        }
        _ => return,
    };
    debug!("BLE MIDI parsed: {:?}", msg);
    let _ = tx.try_send(msg);
}
