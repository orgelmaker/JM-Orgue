//! Automatisch MIDI-archief (0.7.38).
//!
//! Een tweede, van de handmatige MIDI-opname onafhankelijke recorder die in de
//! MIDI-thread (state.rs::midi_thread) meeluistert op de LIVE events (USB- en
//! BLE-lus, exact naast `capture_midi_event`; bewust NIET in de speler-lus,
//! anders zou afspelen opnieuw gearchiveerd worden). Hij bewaart dezelfde ruwe
//! `(timestamp_us, status, data1, data2, channel)`-tuples als `MidiRecording`
//! (vóór klavier-mapping), zodat `midi_play_file` ze via `handle_midi_message`
//! weer op hetzelfde orgel/klavier afspeelt.
//!
//! Levenscyclus van een take:
//! - start bij de eerste NoteOn (velocity > 0);
//! - eindigt na `silence_secs` zonder noot-activiteit én zonder ingedrukte
//!   toetsen, bij uitzetten van de instelling, bij een harde limiet
//!   (`MAX_TAKE_EVENTS`, vastgelopen NoteOff → `STUCK_MIN`), of geforceerd bij
//!   afsluiten (`MidiCommand::ArchiveFlush`);
//! - keuring: minimaal `min_notes` NoteOns en `min_secs` seconden, anders weg.
//!
//! Bestand: `<map>/YYYY-MM-DD_HH-MM-SS_<orgelnaam>.mid` (lokale tijd van de
//! start van het spel), SMF format 0 via `commands::encode_smf_with_name` met
//! een Track-Name-meta (FF 03) met de orgelnaam.
//!
//! Hete pad: geen lock en geen allocatie per event — de buffer is een lokale
//! Vec in de MIDI-thread, instellingen/status zijn atomics in
//! `Arc<MidiArchiveShared>`; één read-lock op `loaded_organ_info` per
//! take-start; schrijven gebeurt op een korte spawn-thread (synchroon alleen
//! bij de exit-flush).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use vpo_midi::MidiMessage;

// ============ Constanten ============

pub const DEFAULT_SILENCE_SECS: u32 = 20;
pub const DEFAULT_MIN_NOTES: u32 = 4;
pub const DEFAULT_MIN_SECS: u32 = 5;
pub const MIN_SILENCE_SECS: u32 = 3;
pub const MAX_SILENCE_SECS: u32 = 600;
/// Harde grens per take (≈16 MB RAM); daarboven wordt de take afgesloten en
/// begint bij de volgende NoteOn een nieuwe.
pub const MAX_TAKE_EVENTS: usize = 1_000_000;
/// Vastgelopen NoteOff (kabel eruit): als er toetsen 'vast' staan en er is
/// max(30×stiltetijd, 10 min) geen noot-activiteit, toch afsluiten met
/// gesynthetiseerde NoteOffs.
pub const STUCK_MIN: Duration = Duration::from_secs(600);
const TAKE_RESERVE: usize = 4096;

fn d_silence() -> u32 { DEFAULT_SILENCE_SECS }
fn d_min_notes() -> u32 { DEFAULT_MIN_NOTES }
fn d_min_secs() -> u32 { DEFAULT_MIN_SECS }

// ============ Instellingen (persistent) ============

/// Instellingen van het automatische MIDI-archief, bewaard in
/// `<app_data_dir>/midi_archive.json` (zelfde patroon als audio_config.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiArchivePrefs {
    #[serde(default)]
    pub enabled: bool,
    /// Archiefmap; None = standaardmap (Documenten/JM-Orgue-opnames/MIDI-archief).
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default = "d_silence")]
    pub silence_secs: u32,
    #[serde(default = "d_min_notes")]
    pub min_notes: u32,
    #[serde(default = "d_min_secs")]
    pub min_secs: u32,
}

impl Default for MidiArchivePrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            dir: None,
            silence_secs: DEFAULT_SILENCE_SECS,
            min_notes: DEFAULT_MIN_NOTES,
            min_secs: DEFAULT_MIN_SECS,
        }
    }
}

/// Standaard-archiefmap: Documenten/JM-Orgue-opnames/MIDI-archief (dezelfde
/// opnamemap als de handmatige MP3-/MIDI-opnames), met fallback naar de
/// app-data-map als er geen Documenten-map bekend is.
pub fn default_dir() -> PathBuf {
    if let Some(docs) = dirs::document_dir() {
        return docs.join("JM-Orgue-opnames").join("MIDI-archief");
    }
    dirs::data_dir()
        .map(|d| d.join("nl.jm-orgue.app").join("MIDI-archief"))
        .unwrap_or_else(|| PathBuf::from("JM-Orgue-opnames").join("MIDI-archief"))
}

impl MidiArchivePrefs {
    /// De feitelijke archiefmap (eigen keuze of standaard).
    pub fn effective_dir(&self) -> PathBuf {
        self.dir
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_dir)
    }

    /// Begrens de instellingen tot zinnige waarden.
    pub fn clamped(mut self) -> Self {
        self.silence_secs = self.silence_secs.clamp(MIN_SILENCE_SECS, MAX_SILENCE_SECS);
        self.min_notes = self.min_notes.min(100);
        self.min_secs = self.min_secs.min(600);
        self
    }
}

/// Laad de archief-instellingen uit `<app_data_dir>/midi_archive.json` (defaults als afwezig).
pub fn load_prefs(app_data_dir: &Path) -> MidiArchivePrefs {
    let path = app_data_dir.join("midi_archive.json");
    if let Ok(json) = std::fs::read_to_string(&path) {
        if let Ok(prefs) = serde_json::from_str::<MidiArchivePrefs>(&json) {
            return prefs.clamped();
        }
    }
    MidiArchivePrefs::default()
}

/// Bewaar de archief-instellingen in `<app_data_dir>/midi_archive.json`.
pub fn save_prefs(app_data_dir: &Path, prefs: &MidiArchivePrefs) {
    let path = app_data_dir.join("midi_archive.json");
    if let Ok(json) = serde_json::to_string_pretty(prefs) {
        if let Err(e) = std::fs::write(&path, json) {
            tracing::warn!("Failed to save midi_archive.json: {}", e);
        }
    }
}

// ============ Gedeelde runtime-status ============

/// Instellingen + live status als atomics, gedeeld tussen de MIDI-thread
/// (lezer per event), de commands/test-API (schrijver + statuslezer) en de
/// writer-thread. `dir` wordt door de MIDI-thread NOOIT per event gelezen,
/// alleen bij het schrijven op de writer-thread.
pub struct MidiArchiveShared {
    pub enabled: AtomicBool,
    pub silence_secs: AtomicU32,
    pub min_notes: AtomicU32,
    pub min_secs: AtomicU32,
    /// Er loopt op dit moment een take.
    pub archiving: AtomicBool,
    /// Aantal events in de lopende take (0 als er geen take loopt).
    pub event_count: AtomicU32,
    /// Starttijd van de lopende take (epoch-ms) voor de tijdweergave in de UI.
    pub take_started_epoch_ms: AtomicU64,
    /// Aantal in deze sessie geschreven bestanden (UI ververst de lijst bij wijziging).
    pub files_written: AtomicU32,
    pub dir: parking_lot::RwLock<PathBuf>,
    pub last_file: parking_lot::RwLock<Option<PathBuf>>,
    pub last_error: parking_lot::RwLock<Option<String>>,
}

impl MidiArchiveShared {
    pub fn from_prefs(p: &MidiArchivePrefs) -> Self {
        let p = p.clone().clamped();
        Self {
            enabled: AtomicBool::new(p.enabled),
            silence_secs: AtomicU32::new(p.silence_secs),
            min_notes: AtomicU32::new(p.min_notes),
            min_secs: AtomicU32::new(p.min_secs),
            archiving: AtomicBool::new(false),
            event_count: AtomicU32::new(0),
            take_started_epoch_ms: AtomicU64::new(0),
            files_written: AtomicU32::new(0),
            dir: parking_lot::RwLock::new(p.effective_dir()),
            last_file: parking_lot::RwLock::new(None),
            last_error: parking_lot::RwLock::new(None),
        }
    }

    /// Nieuwe instellingen doorzetten (na set_midi_archive_config).
    pub fn apply(&self, p: &MidiArchivePrefs) {
        let p = p.clone().clamped();
        self.silence_secs.store(p.silence_secs, Ordering::Relaxed);
        self.min_notes.store(p.min_notes, Ordering::Relaxed);
        self.min_secs.store(p.min_secs, Ordering::Relaxed);
        *self.dir.write() = p.effective_dir();
        // enabled als laatste: de MIDI-thread ziet dan de volledige set.
        self.enabled.store(p.enabled, Ordering::Relaxed);
    }
}

// ============ Event-filter ============

/// Channel-voice-bytes (status, data1, data2, channel) van een MIDI-bericht,
/// zonder allocatie (géén `to_bytes()`, dat alloceert een Vec per aanroep).
/// Zelfde filter als `capture_midi_event`: alleen 0x80..=0xE0; realtime,
/// systeem en SysEx → None.
pub fn channel_voice_bytes(msg: &MidiMessage) -> Option<(u8, u8, u8, u8)> {
    match msg {
        MidiMessage::NoteOff { channel, note, velocity } => {
            Some((0x80 | (channel & 0x0F), *note, *velocity, channel & 0x0F))
        }
        MidiMessage::NoteOn { channel, note, velocity } => {
            Some((0x90 | (channel & 0x0F), *note, *velocity, channel & 0x0F))
        }
        MidiMessage::PolyAftertouch { channel, note, pressure } => {
            Some((0xA0 | (channel & 0x0F), *note, *pressure, channel & 0x0F))
        }
        MidiMessage::ControlChange { channel, controller, value } => {
            Some((0xB0 | (channel & 0x0F), *controller, *value, channel & 0x0F))
        }
        MidiMessage::ProgramChange { channel, program } => {
            Some((0xC0 | (channel & 0x0F), *program, 0, channel & 0x0F))
        }
        MidiMessage::ChannelAftertouch { channel, pressure } => {
            Some((0xD0 | (channel & 0x0F), *pressure, 0, channel & 0x0F))
        }
        MidiMessage::PitchBend { channel, value } => {
            let adjusted = (*value as i32 + 8192).clamp(0, 16383) as u16;
            let lsb = (adjusted & 0x7F) as u8;
            let msb = ((adjusted >> 7) & 0x7F) as u8;
            Some((0xE0 | (channel & 0x0F), lsb, msb, channel & 0x0F))
        }
        MidiMessage::SysEx(_)
        | MidiMessage::Clock
        | MidiMessage::Start
        | MidiMessage::Stop
        | MidiMessage::Continue
        | MidiMessage::ActiveSensing
        | MidiMessage::SystemReset => None,
    }
}

// ============ Take ============

/// Lopende take (lokaal in de MIDI-thread).
pub struct ArchiveTake {
    pub events: Vec<(u64, u8, u8, u8, u8)>,
    pub start: Instant,
    pub start_local: (u32, u32, u32, u32, u32, u32),
    pub last_note_activity: Instant,
    pub note_on_count: u32,
    /// Ingedrukte toetsen per kanaal (bit = nootnummer).
    pub held: [u128; 16],
    pub organ_name: String,
}

/// Afgesloten en goedgekeurde take, klaar om weg te schrijven.
pub struct FinishedTake {
    pub events: Vec<(u64, u8, u8, u8, u8)>,
    pub start_local: (u32, u32, u32, u32, u32, u32),
    pub organ_name: String,
    pub note_on_count: u32,
    pub duration_secs: f64,
}

fn epoch_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// De recorder zelf: leeft in de MIDI-thread, buffer zonder lock.
pub struct ArchiveRecorder {
    shared: Arc<MidiArchiveShared>,
    take: Option<ArchiveTake>,
}

impl ArchiveRecorder {
    pub fn new(shared: Arc<MidiArchiveShared>) -> Self {
        Self { shared, take: None }
    }

    pub fn shared(&self) -> Arc<MidiArchiveShared> {
        self.shared.clone()
    }

    /// Loopt er een take? (tests/debug; de MIDI-thread zelf gebruikt poll()).
    #[allow(dead_code)]
    pub fn is_taking(&self) -> bool {
        self.take.is_some()
    }

    /// Live event uit de USB-/BLE-lus. `organ_name` wordt alleen bij
    /// take-start aangeroepen (één read-lock per take). CC/PC/pitchbend/
    /// aftertouch worden ná de start meegenomen, vóór de eerste NoteOn genegeerd.
    pub fn on_event(&mut self, msg: &MidiMessage, now: Instant, organ_name: impl FnOnce() -> String) {
        // Snelste uitgang: één atomic-load als er niets loopt en het archief uit staat.
        if self.take.is_none() && !self.shared.enabled.load(Ordering::Relaxed) {
            return;
        }
        let (status, d1, d2, ch) = match channel_voice_bytes(msg) {
            Some(b) => b,
            None => return,
        };
        let kind = status & 0xF0;
        let is_on = kind == 0x90 && d2 > 0;
        let is_off = kind == 0x80 || (kind == 0x90 && d2 == 0);

        if self.take.is_none() {
            if !is_on {
                return; // alleen starten op een echte NoteOn
            }
            self.take = Some(ArchiveTake {
                events: Vec::with_capacity(TAKE_RESERVE),
                start: now,
                start_local: local_ymdhms(),
                last_note_activity: now,
                note_on_count: 0,
                held: [0; 16],
                organ_name: organ_name(),
            });
            self.shared.take_started_epoch_ms.store(epoch_ms_now(), Ordering::Relaxed);
            self.shared.archiving.store(true, Ordering::Relaxed);
        }
        let take = self.take.as_mut().expect("take aanwezig");
        let ts = now.duration_since(take.start).as_micros() as u64;
        take.events.push((ts, status, d1, d2, ch));
        let bit: u128 = 1u128 << (d1 & 0x7F);
        let slot = (ch & 0x0F) as usize;
        if is_on {
            take.note_on_count += 1;
            take.held[slot] |= bit;
            take.last_note_activity = now;
        } else if is_off {
            take.held[slot] &= !bit;
            take.last_note_activity = now;
        }
        self.shared.event_count.store(take.events.len().min(u32::MAX as usize) as u32, Ordering::Relaxed);
    }

    /// Periodieke controle (elke lus-iteratie): sluit de take af bij stilte,
    /// uitzetten, harde limiet of vastgelopen NoteOff.
    pub fn poll(&mut self, now: Instant) -> Option<FinishedTake> {
        let take = self.take.as_ref()?;
        let silence = Duration::from_secs(self.shared.silence_secs.load(Ordering::Relaxed) as u64);
        let idle = now.saturating_duration_since(take.last_note_activity);
        let held_any = take.held.iter().any(|w| *w != 0);
        let stuck_limit = std::cmp::max(silence * 30, STUCK_MIN);
        let close = !self.shared.enabled.load(Ordering::Relaxed)
            || take.events.len() >= MAX_TAKE_EVENTS
            || (!held_any && idle >= silence)
            || idle >= stuck_limit;
        if !close {
            return None;
        }
        let take = self.take.take()?;
        self.finish(take, now)
    }

    /// Geforceerd afsluiten (ArchiveFlush bij afsluiten van de app / test-API).
    pub fn force_finish(&mut self, now: Instant) -> Option<FinishedTake> {
        let take = self.take.take()?;
        self.finish(take, now)
    }

    fn finish(&mut self, mut take: ArchiveTake, now: Instant) -> Option<FinishedTake> {
        // Nog ingedrukte toetsen netjes afsluiten met gesynthetiseerde NoteOffs.
        let ts = now.saturating_duration_since(take.start).as_micros() as u64;
        for (ch, word) in take.held.iter().enumerate() {
            let mut w = *word;
            while w != 0 {
                let note = w.trailing_zeros() as u8;
                take.events.push((ts, 0x80 | ch as u8, note, 0, ch as u8));
                w &= !(1u128 << note);
            }
        }
        self.shared.archiving.store(false, Ordering::Relaxed);
        self.shared.event_count.store(0, Ordering::Relaxed);

        let duration_secs = take.events.last().map(|e| e.0 as f64 / 1e6).unwrap_or(0.0);
        let min_notes = self.shared.min_notes.load(Ordering::Relaxed);
        let min_secs = self.shared.min_secs.load(Ordering::Relaxed) as f64;
        if take.note_on_count < min_notes || duration_secs < min_secs {
            tracing::debug!(
                "MIDI-archief: take afgekeurd ({} noten, {:.1} s)",
                take.note_on_count, duration_secs
            );
            return None;
        }
        tracing::info!(
            "MIDI-archief: take afgesloten ({} noten, {} events, {:.1} s)",
            take.note_on_count, take.events.len(), duration_secs
        );
        Some(FinishedTake {
            events: take.events,
            start_local: take.start_local,
            organ_name: take.organ_name,
            note_on_count: take.note_on_count,
            duration_secs,
        })
    }
}

// ============ Schrijven ============

/// Orgelnaam geschikt maken voor een bestandsnaam: trim, whitespace → '_',
/// verboden tekens en control-chars weg, max. 40 tekens.
pub fn sanitize_organ_name(name: &str) -> String {
    name.trim()
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .map(|c| if c.is_whitespace() { '_' } else { c })
        .take(40)
        .collect()
}

/// Lokale datum/tijd (jaar, maand, dag, uur, min, sec).
#[cfg(windows)]
pub fn local_ymdhms() -> (u32, u32, u32, u32, u32, u32) {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    (
        st.wYear as u32, st.wMonth as u32, st.wDay as u32,
        st.wHour as u32, st.wMinute as u32, st.wSecond as u32,
    )
}

/// Niet-Windows: UTC via de bestaande kalender-conversie.
#[cfg(not(windows))]
pub fn local_ymdhms() -> (u32, u32, u32, u32, u32, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    crate::commands::epoch_to_ymdhms(secs)
}

/// Bestandsnaam: YYYY-MM-DD_HH-MM-SS[_<orgel>].mid
pub fn archive_filename(start_local: (u32, u32, u32, u32, u32, u32), organ: &str) -> String {
    let (y, mo, d, h, mi, s) = start_local;
    let organ = sanitize_organ_name(organ);
    let suffix = if organ.is_empty() { String::new() } else { format!("_{}", organ) };
    format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}{}.mid", y, mo, d, h, mi, s, suffix)
}

/// Schrijf een afgesloten take als SMF in `dir`; bij een bestaande naam suffix
/// -1, -2 … (zelfde truc als make_timestamped_recording_path).
pub fn write_take(dir: &Path, take: &FinishedTake) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Archiefmap niet aan te maken ({}): {}", dir.display(), e))?;
    let name = archive_filename(take.start_local, &take.organ_name);
    let stem = name.trim_end_matches(".mid").to_string();
    let mut path = dir.join(&name);
    let mut counter = 1;
    while path.exists() && counter < 1000 {
        path = dir.join(format!("{}-{}.mid", stem, counter));
        counter += 1;
    }
    let organ = take.organ_name.trim();
    let data = crate::commands::encode_smf_with_name(
        &take.events,
        if organ.is_empty() { None } else { Some(organ) },
    );
    std::fs::write(&path, &data)
        .map_err(|e| format!("Kan archiefbestand niet schrijven ({}): {}", path.display(), e))?;
    Ok(path)
}

fn record_written(shared: &MidiArchiveShared, take: &FinishedTake, result: Result<PathBuf, String>) -> Option<PathBuf> {
    match result {
        Ok(p) => {
            *shared.last_file.write() = Some(p.clone());
            *shared.last_error.write() = None;
            shared.files_written.fetch_add(1, Ordering::Relaxed);
            tracing::info!(
                "MIDI-archief geschreven: {:?} ({} noten, {} events, {:.1} s)",
                p, take.note_on_count, take.events.len(), take.duration_secs
            );
            Some(p)
        }
        Err(e) => {
            tracing::warn!("MIDI-archief schrijven mislukt: {}", e);
            *shared.last_error.write() = Some(e);
            None
        }
    }
}

/// Schrijf op een korte eigen thread (buiten de MIDI-thread).
pub fn spawn_write(shared: Arc<MidiArchiveShared>, take: FinishedTake) {
    let res = std::thread::Builder::new()
        .name("midi-archive-writer".into())
        .spawn(move || {
            let dir = shared.dir.read().clone();
            let result = write_take(&dir, &take);
            record_written(&shared, &take, result);
        });
    if let Err(e) = res {
        tracing::warn!("MIDI-archief: writer-thread niet gestart: {}", e);
    }
}

/// Synchroon schrijven (exit-flush / test-API).
pub fn write_now(shared: &MidiArchiveShared, take: FinishedTake) -> Option<PathBuf> {
    let dir = shared.dir.read().clone();
    let result = write_take(&dir, &take);
    record_written(shared, &take, result)
}

// ============ Lijst / verwijderen ============

#[derive(Debug, Serialize, Clone)]
pub struct ArchiveFileInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_epoch: u64,
}

/// De `n` nieuwste .mid-bestanden in de archiefmap (nieuwste eerst).
pub fn list_recent(dir: &Path, n: usize) -> Vec<ArchiveFileInfo> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let mut files: Vec<ArchiveFileInfo> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            let is_mid = path.extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("mid"))
                .unwrap_or(false);
            if !is_mid { return None; }
            let md = e.metadata().ok()?;
            if !md.is_file() { return None; }
            let modified_epoch = md.modified().ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Some(ArchiveFileInfo {
                name: path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                path: path.to_string_lossy().to_string(),
                size_bytes: md.len(),
                modified_epoch,
            })
        })
        .collect();
    files.sort_by(|a, b| b.modified_epoch.cmp(&a.modified_epoch).then_with(|| b.name.cmp(&a.name)));
    files.truncate(n);
    files
}

/// Veilig verwijderen: alleen een bestaand .mid-bestand dat direct in de
/// (gecanonicaliseerde) archiefmap staat.
pub fn is_inside_archive_dir(dir: &Path, file: &Path) -> bool {
    let file_c = match std::fs::canonicalize(file) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let dir_c = match std::fs::canonicalize(dir) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let is_mid = file_c.extension()
        .and_then(|x| x.to_str())
        .map(|x| x.eq_ignore_ascii_case("mid"))
        .unwrap_or(false);
    is_mid && file_c.parent() == Some(dir_c.as_path())
}

// ============ Tests ============

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn shared(enabled: bool, silence: u32, min_notes: u32, min_secs: u32) -> Arc<MidiArchiveShared> {
        Arc::new(MidiArchiveShared::from_prefs(&MidiArchivePrefs {
            enabled,
            dir: None,
            silence_secs: silence,
            min_notes,
            min_secs,
        }))
    }

    fn on(ch: u8, note: u8) -> MidiMessage { MidiMessage::NoteOn { channel: ch, note, velocity: 100 } }
    fn off(ch: u8, note: u8) -> MidiMessage { MidiMessage::NoteOff { channel: ch, note, velocity: 0 } }

    #[test]
    fn take_starts_on_first_note_on_only() {
        let sh = shared(true, 3, 1, 0);
        let mut rec = ArchiveRecorder::new(sh.clone());
        let t0 = Instant::now();
        let calls = Cell::new(0u32);
        let name = || { calls.set(calls.get() + 1); "Test Orgel".to_string() };
        rec.on_event(&MidiMessage::ControlChange { channel: 0, controller: 7, value: 100 }, t0, name);
        rec.on_event(&off(2, 60), t0, name);
        assert!(!rec.is_taking());
        assert!(!sh.archiving.load(Ordering::Relaxed));
        assert_eq!(calls.get(), 0);
        rec.on_event(&on(2, 60), t0, name);
        assert!(rec.is_taking());
        assert!(sh.archiving.load(Ordering::Relaxed));
        assert_eq!(sh.event_count.load(Ordering::Relaxed), 1);
        assert_eq!(calls.get(), 1, "organ_name-closure precies één keer bij take-start");
        // Tweede event: closure niet opnieuw.
        rec.on_event(&off(2, 60), t0 + Duration::from_millis(100), name);
        assert_eq!(calls.get(), 1);
        assert_eq!(sh.event_count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn silence_closes_take_after_timeout() {
        let sh = shared(true, 3, 1, 0);
        let mut rec = ArchiveRecorder::new(sh.clone());
        let t0 = Instant::now();
        rec.on_event(&on(2, 60), t0, || "O".into());
        rec.on_event(&off(2, 60), t0 + Duration::from_millis(500), || "O".into());
        assert!(rec.poll(t0 + Duration::from_millis(600)).is_none());
        assert!(rec.poll(t0 + Duration::from_secs(3)).is_none(), "3 s na start is pas 2,5 s stilte");
        let fin = rec.poll(t0 + Duration::from_millis(3600)).expect("take na stilte afgesloten");
        assert!(!sh.archiving.load(Ordering::Relaxed));
        assert_eq!(sh.event_count.load(Ordering::Relaxed), 0);
        assert_eq!(fin.events.len(), 2);
        assert_eq!(fin.events[0].1, 0x92);
        assert_eq!(fin.events[1].1, 0x82);
        assert!(fin.events[0].0 < fin.events[1].0);
        assert_eq!(fin.note_on_count, 1);
        assert!((fin.duration_secs - 0.5).abs() < 0.01);
    }

    #[test]
    fn held_note_blocks_silence_and_gets_closed_on_force() {
        let sh = shared(true, 3, 1, 0);
        let mut rec = ArchiveRecorder::new(sh.clone());
        let t0 = Instant::now();
        rec.on_event(&on(1, 64), t0, || "O".into());
        assert!(rec.poll(t0 + Duration::from_secs(6)).is_none(), "toets vast → niet afsluiten");
        let fin = rec.force_finish(t0 + Duration::from_secs(7)).expect("geforceerd afgesloten");
        let last = fin.events.last().unwrap();
        assert_eq!((last.1, last.2, last.3, last.4), (0x81, 64, 0, 1));
        assert_eq!(fin.events.len(), 2);
        assert!(!sh.archiving.load(Ordering::Relaxed));
    }

    #[test]
    fn stuck_note_closes_after_hard_limit() {
        let sh = shared(true, 3, 1, 0);
        let mut rec = ArchiveRecorder::new(sh.clone());
        let t0 = Instant::now();
        rec.on_event(&on(0, 60), t0, || "O".into());
        assert!(rec.poll(t0 + Duration::from_secs(599)).is_none());
        let fin = rec.poll(t0 + Duration::from_secs(601)).expect("harde grens");
        assert_eq!(fin.events.last().unwrap().1, 0x80);
    }

    #[test]
    fn min_notes_and_min_secs_reject_short_takes() {
        // min_notes=4, één noot → None
        let sh = shared(true, 3, 4, 0);
        let mut rec = ArchiveRecorder::new(sh.clone());
        let t0 = Instant::now();
        rec.on_event(&on(0, 60), t0, || "O".into());
        rec.on_event(&off(0, 60), t0 + Duration::from_millis(200), || "O".into());
        assert!(rec.force_finish(t0 + Duration::from_secs(1)).is_none());

        // min_secs=5 met 4 noten binnen 1 s → None
        let sh = shared(true, 3, 4, 5);
        let mut rec = ArchiveRecorder::new(sh.clone());
        for i in 0..4u8 {
            let t = t0 + Duration::from_millis(100 * i as u64);
            rec.on_event(&on(0, 60 + i), t, || "O".into());
            rec.on_event(&off(0, 60 + i), t + Duration::from_millis(50), || "O".into());
        }
        assert!(rec.force_finish(t0 + Duration::from_millis(900)).is_none());

        // 4 noten en laatste timestamp ≥ 5 s → Some
        let sh = shared(true, 3, 4, 5);
        let mut rec = ArchiveRecorder::new(sh.clone());
        for i in 0..4u8 {
            let t = t0 + Duration::from_secs(i as u64 * 2);
            rec.on_event(&on(0, 60 + i), t, || "O".into());
            rec.on_event(&off(0, 60 + i), t + Duration::from_millis(500), || "O".into());
        }
        let fin = rec.force_finish(t0 + Duration::from_millis(6600)).expect("lang genoeg");
        assert_eq!(fin.note_on_count, 4);
        assert!(fin.duration_secs >= 5.0);
    }

    #[test]
    fn disabled_while_taking_closes_take() {
        let sh = shared(true, 3, 1, 0);
        let mut rec = ArchiveRecorder::new(sh.clone());
        let t0 = Instant::now();
        rec.on_event(&on(0, 60), t0, || "O".into());
        rec.on_event(&off(0, 60), t0 + Duration::from_millis(100), || "O".into());
        assert!(rec.poll(t0 + Duration::from_millis(200)).is_none());
        sh.enabled.store(false, Ordering::Relaxed);
        assert!(rec.poll(t0 + Duration::from_millis(300)).is_some());
        // Uit → nieuwe NoteOn start géén take.
        rec.on_event(&on(0, 62), t0 + Duration::from_millis(400), || "O".into());
        assert!(!rec.is_taking());
    }

    #[test]
    fn player_and_non_channel_voice_ignored() {
        // De speler-lus is per constructie niet gehookt (state.rs); hier alleen
        // het byte-filter: realtime/systeem/SysEx levert geen event.
        assert!(channel_voice_bytes(&MidiMessage::Clock).is_none());
        assert!(channel_voice_bytes(&MidiMessage::Start).is_none());
        assert!(channel_voice_bytes(&MidiMessage::SysEx(vec![1, 2, 3])).is_none());
        assert_eq!(channel_voice_bytes(&MidiMessage::ProgramChange { channel: 3, program: 7 }), Some((0xC3, 7, 0, 3)));
        assert_eq!(channel_voice_bytes(&MidiMessage::PitchBend { channel: 0, value: 0 }), Some((0xE0, 0, 64, 0)));

        let sh = shared(true, 3, 1, 0);
        let mut rec = ArchiveRecorder::new(sh.clone());
        let t0 = Instant::now();
        rec.on_event(&on(0, 60), t0, || "O".into());
        rec.on_event(&MidiMessage::Clock, t0, || "O".into());
        rec.on_event(&MidiMessage::SysEx(vec![1]), t0, || "O".into());
        assert_eq!(sh.event_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn write_take_creates_valid_smf_with_track_name() {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("jm-archief-test-{}", nanos));
        let take = FinishedTake {
            events: vec![(0, 0x90, 60, 100, 0), (500_000, 0x80, 60, 0, 0)],
            start_local: (2026, 9, 12, 14, 5, 9),
            organ_name: "Test Orgel".into(),
            note_on_count: 1,
            duration_secs: 0.5,
        };
        let p = write_take(&dir, &take).expect("schrijven");
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(name, "2026-09-12_14-05-09_Test_Orgel.mid");
        let bytes = std::fs::read(&p).unwrap();
        let smf = midly::Smf::parse(&bytes).expect("geldige SMF");
        assert!(matches!(smf.header.format, midly::Format::SingleTrack));
        assert_eq!(smf.tracks.len(), 1);
        let mut has_name = false;
        let mut notes = 0;
        for ev in &smf.tracks[0] {
            match ev.kind {
                midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(n)) => {
                    assert_eq!(n, b"Test Orgel");
                    has_name = true;
                }
                midly::TrackEventKind::Midi { .. } => notes += 1,
                _ => {}
            }
        }
        assert!(has_name, "Track-Name-meta ontbreekt");
        assert_eq!(notes, 2);
        // Tweede take met dezelfde starttijd → suffix -1.
        let p2 = write_take(&dir, &take).expect("schrijven 2");
        assert_eq!(p2.file_name().unwrap().to_string_lossy(), "2026-09-12_14-05-09_Test_Orgel-1.mid");
        // Lijst + veilig verwijderen.
        let list = list_recent(&dir, 10);
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|f| f.size_bytes > 20));
        assert!(is_inside_archive_dir(&dir, &p));
        assert!(!is_inside_archive_dir(&dir, &dir.join("bestaat-niet.mid")));
        assert!(!is_inside_archive_dir(&dir.join("sub"), &p));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitize_organ_name_cases() {
        assert_eq!(sanitize_organ_name("Bätz-Witte Puttershoek"), "Bätz-Witte_Puttershoek");
        assert_eq!(sanitize_organ_name("A/B:C*D?"), "ABCD");
        let long: String = std::iter::repeat('x').take(60).collect();
        assert_eq!(sanitize_organ_name(&long).chars().count(), 40);
        assert_eq!(sanitize_organ_name("  "), "");
        assert_eq!(archive_filename((2026, 1, 2, 3, 4, 5), ""), "2026-01-02_03-04-05.mid");
    }

    #[test]
    fn prefs_roundtrip_and_defaults() {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("jm-archief-prefs-{}", nanos));
        std::fs::create_dir_all(&dir).unwrap();
        // Ontbrekend bestand → defaults.
        let d = load_prefs(&dir);
        assert!(!d.enabled);
        assert_eq!((d.silence_secs, d.min_notes, d.min_secs), (20, 4, 5));
        assert!(d.dir.is_none());
        // Roundtrip.
        let p = MidiArchivePrefs { enabled: true, dir: Some("C:/x".into()), silence_secs: 7, min_notes: 2, min_secs: 1 };
        save_prefs(&dir, &p);
        let l = load_prefs(&dir);
        assert!(l.enabled);
        assert_eq!(l.dir.as_deref(), Some("C:/x"));
        assert_eq!((l.silence_secs, l.min_notes, l.min_secs), (7, 2, 1));
        // Alleen {"enabled":true} → overige defaults.
        std::fs::write(dir.join("midi_archive.json"), "{\"enabled\":true}").unwrap();
        let l = load_prefs(&dir);
        assert!(l.enabled);
        assert_eq!((l.silence_secs, l.min_notes, l.min_secs), (20, 4, 5));
        // clamped
        let c = MidiArchivePrefs { silence_secs: 1, ..Default::default() }.clamped();
        assert_eq!(c.silence_secs, 3);
        let c = MidiArchivePrefs { silence_secs: 9999, min_notes: 500, min_secs: 5000, ..Default::default() }.clamped();
        assert_eq!((c.silence_secs, c.min_notes, c.min_secs), (600, 100, 600));
        assert!(MidiArchivePrefs { dir: Some("  ".into()), ..Default::default() }.effective_dir() == default_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
