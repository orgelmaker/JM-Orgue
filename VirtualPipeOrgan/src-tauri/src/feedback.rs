//! MIDI-uit terugkoppeling (fase 1): stuurt de huidige registratie naar een
//! fysieke console (registerlampen/SAMs of een tekstdisplay). Drie protocollen:
//!  - NoteOnOff: per register een nootnummer op één kanaal (AAN = NoteOn vel 127,
//!    UIT = NoteOff). Dekt de meeste decoder-hardware (GrandOrgue/Hauptwerk-stijl).
//!  - HauptwerkLcd: 32-byte LCD-SysEx (F0 7D 01 …) — toont een statusregel.
//!  - Johannus: Program Change (toggle) per register + JOHAS all-off-SysEx.
//!
//! Gevoed door de UI: bij elke registratiewijziging (toggle, preset, tutti,
//! crescendo, orgel-load) roept de frontend `feedback_apply(active_ids)` aan.
//! Deze module diff't tegen de laatst-gezonden stand en stuurt alleen de
//! wijzigingen — gebatcht in één aanroep, dus geen 60 losse berichten bij een
//! tutti. De volgorde van `slots` bepaalt de register-index (LCD/Johannus).

use std::collections::{HashMap, HashSet};
use vpo_midi::{MidiOutputManager, MidiMessage};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FeedbackProtocol { Off, NoteOnOff, HauptwerkLcd, Johannus }

impl FeedbackProtocol {
    pub fn parse(s: &str) -> Self {
        match s {
            "note" => FeedbackProtocol::NoteOnOff,
            "lcd" => FeedbackProtocol::HauptwerkLcd,
            "johannus" => FeedbackProtocol::Johannus,
            _ => FeedbackProtocol::Off,
        }
    }
}

/// Eén register-slot. `note` is het nootnummer voor NoteOnOff; de positie in
/// `slots` is de register-index voor LCD/Johannus.
#[derive(Clone)]
pub struct FeedbackSlot { pub id: String, pub note: u8 }

pub struct FeedbackManager {
    out: Option<MidiOutputManager>,
    connected_port: Option<String>,
    protocol: FeedbackProtocol,
    channel: u8, // 0-based
    slots: Vec<FeedbackSlot>,
    index_of: HashMap<String, usize>,
    active: HashSet<String>, // laatst-gezonden actieve id's
    organ_name: String,
}

impl FeedbackManager {
    pub fn new() -> Self {
        Self {
            out: MidiOutputManager::new().ok(),
            connected_port: None,
            protocol: FeedbackProtocol::Off,
            channel: 0,
            slots: Vec::new(),
            index_of: HashMap::new(),
            active: HashSet::new(),
            organ_name: String::new(),
        }
    }

    pub fn list_outputs(&self) -> Vec<String> {
        self.out.as_ref()
            .map(|o| o.list_devices().into_iter().map(|d| d.name).collect())
            .unwrap_or_default()
    }

    /// Protocol/poort/kanaal/slots (her)instellen. Verbindt de uitgangspoort
    /// alleen opnieuw wanneer die wijzigt; wist de interne stand zodat de
    /// eerstvolgende `apply` een volledige (re)sync stuurt.
    pub fn configure(&mut self, protocol: FeedbackProtocol, port: Option<String>, channel: u8,
                     slots: Vec<FeedbackSlot>, organ_name: String) -> Result<(), String> {
        self.protocol = protocol;
        self.channel = channel.min(15);
        self.organ_name = organ_name;
        self.slots = slots;
        self.index_of = self.slots.iter().enumerate().map(|(i, s)| (s.id.clone(), i)).collect();
        if let Some(out) = self.out.as_mut() {
            match &port {
                Some(p) if self.connected_port.as_deref() != Some(p.as_str()) => {
                    out.disconnect_all();
                    out.connect(p).map_err(|e| format!("MIDI-uitgang '{}' verbinden mislukt: {}", p, e))?;
                    self.connected_port = Some(p.clone());
                }
                None => { out.disconnect_all(); self.connected_port = None; }
                _ => {}
            }
        }
        self.active.clear();
        Ok(())
    }

    fn send(&mut self, msg: MidiMessage) {
        if let Some(out) = self.out.as_mut() { out.send(&msg); }
    }

    /// Nieuwe actieve registratie toepassen: diff tegen de vorige stand en de
    /// wijzigingen (gebatcht) sturen. `active_ids` = alle nu-actieve register-
    /// én koppel-id's.
    pub fn apply(&mut self, active_ids: &[String]) {
        let new_active: HashSet<String> = active_ids.iter().cloned().collect();
        // Uit of geen poort: alleen de interne stand bijwerken, niets sturen.
        if self.protocol == FeedbackProtocol::Off || self.connected_port.is_none() {
            self.active = new_active;
            return;
        }
        match self.protocol {
            FeedbackProtocol::NoteOnOff => {
                let ch = self.channel;
                let mut msgs: Vec<MidiMessage> = Vec::new();
                for id in new_active.difference(&self.active) {
                    if let Some(&i) = self.index_of.get(id) {
                        msgs.push(MidiMessage::NoteOn { channel: ch, note: self.slots[i].note, velocity: 127 });
                    }
                }
                for id in self.active.difference(&new_active) {
                    if let Some(&i) = self.index_of.get(id) {
                        msgs.push(MidiMessage::NoteOff { channel: ch, note: self.slots[i].note, velocity: 0 });
                    }
                }
                for m in msgs { self.send(m); }
            }
            FeedbackProtocol::Johannus => {
                // Program Change toggelt een tab; stuur PC voor elke gewijzigde slot.
                let ch = self.channel;
                let mut changed: Vec<usize> = new_active.symmetric_difference(&self.active)
                    .filter_map(|id| self.index_of.get(id).copied()).collect();
                changed.sort_unstable();
                for i in changed {
                    self.send(MidiMessage::ProgramChange { channel: ch, program: (i as u8).min(127) });
                }
            }
            FeedbackProtocol::HauptwerkLcd => {
                // 2×16 tekstdisplay: regel 1 orgelnaam, regel 2 aantal registers.
                let count = new_active.len();
                let line1 = format!("{:<16}", trunc(&self.organ_name, 16));
                let line2 = format!("{:<16}", trunc(&format!("Registers: {}", count), 16));
                let text = format!("{}{}", line1, line2);
                self.send(MidiMessage::SysEx(lcd_body(&text)));
            }
            FeedbackProtocol::Off => {}
        }
        self.active = new_active;
    }

    /// Alles uit (en bij Johannus een JOHAS-reset); interne stand wissen.
    pub fn all_off(&mut self) {
        if self.connected_port.is_some() {
            match self.protocol {
                FeedbackProtocol::NoteOnOff => {
                    let ch = self.channel;
                    let notes: Vec<u8> = self.slots.iter().map(|s| s.note).collect();
                    for note in notes {
                        self.send(MidiMessage::NoteOff { channel: ch, note, velocity: 0 });
                    }
                }
                FeedbackProtocol::Johannus => {
                    // "JOHAS" all-off (F0 00 4A 4F 48 41 53 7F F7).
                    self.send(MidiMessage::SysEx(vec![0x00, 0x4A, 0x4F, 0x48, 0x41, 0x53, 0x7F]));
                }
                FeedbackProtocol::HauptwerkLcd => {
                    self.send(MidiMessage::SysEx(lcd_body(&" ".repeat(32))));
                }
                FeedbackProtocol::Off => {}
            }
        }
        self.active.clear();
    }
}

/// SysEx-body voor de Hauptwerk 32-teken LCD: 7D 01 <panel hi> <panel lo>
/// <kleur> + 32 ASCII (7-bit). Panel-ID 0 voor GO/HW-compatibiliteit.
fn lcd_body(text: &str) -> Vec<u8> {
    let mut body: Vec<u8> = vec![0x7D, 0x01, 0x00, 0x00, 0x00];
    let mut n = 0usize;
    for b in text.bytes() {
        if n >= 32 { break; }
        body.push(b & 0x7F);
        n += 1;
    }
    while n < 32 { body.push(b' '); n += 1; }
    body
}

fn trunc(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
