//! Standard MIDI File player.
//!
//! Leest een SMF, decodeert events naar `MidiMessage` en speelt ze
//! af op een aparte thread met correcte timing. Stuurt elke message
//! via een crossbeam channel naar de host (state.rs MIDI-thread of
//! direct naar audio).

use crate::MidiMessage;
use crossbeam_channel::{bounded, Sender, Receiver, TrySendError};
use midly::{Smf, Timing, MetaMessage, MidiMessage as MidlyMsg, TrackEventKind};
use parking_lot::RwLock;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Status van de player
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
struct AbsoluteEvent {
    /// Cumulatieve micro-seconden vanaf start
    micros: u64,
    msg: MidiMessage,
}

/// Decode een SMF naar één tijdgesorteerde stroom van events.
fn decode_smf(bytes: &[u8]) -> Result<(Vec<AbsoluteEvent>, u64), String> {
    let smf = Smf::parse(bytes).map_err(|e| format!("SMF parse error: {}", e))?;

    // Bepaal ticks-per-quarter (PPQ).
    // We ondersteunen alleen Metrical timing; SMPTE valt buiten scope.
    let ppq: u32 = match smf.header.timing {
        Timing::Metrical(t) => t.as_int() as u32,
        Timing::Timecode(_, _) => {
            return Err("SMPTE-timing wordt nog niet ondersteund".into());
        }
    };
    if ppq == 0 {
        return Err("PPQ is 0 — kan niet afspelen".into());
    }

    // Default tempo: 500_000 µs per quarter (= 120 BPM)
    let mut current_tempo_us_per_qn: u64 = 500_000;

    // We mergen alle tracks naar absolute tick-tijden, sorteren, en converteren naar micros.
    // Elke track heeft eigen running tick-counter (delta-times).
    #[derive(Debug, Clone)]
    struct TickEvent {
        tick: u64,
        kind: TrackEventKind<'static>,
    }

    let mut all_events: Vec<TickEvent> = Vec::new();

    for track in smf.tracks.iter() {
        let mut tick: u64 = 0;
        for ev in track {
            tick += ev.delta.as_int() as u64;
            // Cloning to 'static — we converteren TrackEventKind naar bytes-vrije variant
            // door alleen interesserende velden over te nemen.
            // midly's TrackEventKind heeft references; we maken een owned versie via match.
            let owned: Option<TrackEventKind<'static>> = match ev.kind {
                TrackEventKind::Midi { channel, message } => Some(TrackEventKind::Midi { channel, message }),
                TrackEventKind::Meta(MetaMessage::Tempo(t)) => Some(TrackEventKind::Meta(MetaMessage::Tempo(t))),
                TrackEventKind::Meta(MetaMessage::EndOfTrack) => Some(TrackEventKind::Meta(MetaMessage::EndOfTrack)),
                _ => None,
            };
            if let Some(k) = owned {
                all_events.push(TickEvent { tick, kind: k });
            }
        }
    }

    all_events.sort_by_key(|e| e.tick);

    // Loop in tick-volgorde, houd lopende tempo bij, bereken absolute micros.
    let mut last_tick: u64 = 0;
    let mut accumulated_micros: u64 = 0;
    let mut output: Vec<AbsoluteEvent> = Vec::new();
    let mut total_micros: u64 = 0;

    for ev in &all_events {
        // Tijd-delta sinds vorig event in micros, met huidige tempo
        let dticks = ev.tick - last_tick;
        let delta_micros = (dticks as u128 * current_tempo_us_per_qn as u128 / ppq as u128) as u64;
        accumulated_micros += delta_micros;
        last_tick = ev.tick;

        match ev.kind {
            TrackEventKind::Meta(MetaMessage::Tempo(t)) => {
                current_tempo_us_per_qn = t.as_int() as u64;
            }
            TrackEventKind::Meta(MetaMessage::EndOfTrack) => {
                total_micros = total_micros.max(accumulated_micros);
            }
            TrackEventKind::Midi { channel, message } => {
                let ch = channel.as_int();
                let m = match message {
                    MidlyMsg::NoteOn { key, vel } => {
                        let v = vel.as_int();
                        if v == 0 {
                            MidiMessage::NoteOff { channel: ch, note: key.as_int(), velocity: 0 }
                        } else {
                            MidiMessage::NoteOn { channel: ch, note: key.as_int(), velocity: v }
                        }
                    }
                    MidlyMsg::NoteOff { key, vel } => MidiMessage::NoteOff {
                        channel: ch,
                        note: key.as_int(),
                        velocity: vel.as_int(),
                    },
                    MidlyMsg::Controller { controller, value } => MidiMessage::ControlChange {
                        channel: ch,
                        controller: controller.as_int(),
                        value: value.as_int(),
                    },
                    MidlyMsg::ProgramChange { program } => MidiMessage::ProgramChange {
                        channel: ch,
                        program: program.as_int(),
                    },
                    MidlyMsg::PitchBend { bend } => MidiMessage::PitchBend {
                        channel: ch,
                        value: bend.0.as_int() as i16 - 8192,
                    },
                    MidlyMsg::ChannelAftertouch { vel } => MidiMessage::ChannelAftertouch {
                        channel: ch,
                        pressure: vel.as_int(),
                    },
                    MidlyMsg::Aftertouch { key, vel } => MidiMessage::PolyAftertouch {
                        channel: ch,
                        note: key.as_int(),
                        pressure: vel.as_int(),
                    },
                };
                output.push(AbsoluteEvent { micros: accumulated_micros, msg: m });
                total_micros = total_micros.max(accumulated_micros);
            }
            _ => {}
        }
    }

    Ok((output, total_micros))
}

/// MIDI-bestand player. Spawn een thread die events doorgeeft via een
/// crossbeam channel. De caller leest het channel en routeert messages
/// naar de gebruikelijke MIDI-handler (zelfde pad als live MIDI).
pub struct MidiFilePlayer {
    state: Arc<RwLock<PlayerState>>,
    pos_micros: Arc<AtomicU64>,
    duration_micros: Arc<AtomicU64>,
    seek_to_micros: Arc<RwLock<Option<u64>>>,
    /// 0.25 .. 4.0 (multiplier on real-time)
    speed_x100: Arc<std::sync::atomic::AtomicU32>,
    stop_flag: Arc<AtomicBool>,
    out_rx: Receiver<MidiMessage>,
    /// Wanneer deze drop wordt, krijgt de thread een None bij recv en stopt.
    _out_tx_handle: Sender<MidiMessage>,
    file_loaded: Arc<RwLock<bool>>,
}

impl MidiFilePlayer {
    pub fn new() -> Self {
        let (tx, rx) = bounded::<MidiMessage>(1024);
        Self {
            state: Arc::new(RwLock::new(PlayerState::Stopped)),
            pos_micros: Arc::new(AtomicU64::new(0)),
            duration_micros: Arc::new(AtomicU64::new(0)),
            seek_to_micros: Arc::new(RwLock::new(None)),
            speed_x100: Arc::new(std::sync::atomic::AtomicU32::new(100)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            out_rx: rx,
            _out_tx_handle: tx,
            file_loaded: Arc::new(RwLock::new(false)),
        }
    }

    pub fn out_receiver(&self) -> Receiver<MidiMessage> {
        self.out_rx.clone()
    }

    pub fn state(&self) -> PlayerState {
        *self.state.read()
    }

    pub fn position_micros(&self) -> u64 {
        self.pos_micros.load(Ordering::Relaxed)
    }

    pub fn duration_micros(&self) -> u64 {
        self.duration_micros.load(Ordering::Relaxed)
    }

    pub fn is_loaded(&self) -> bool {
        *self.file_loaded.read()
    }

    /// 0.25 .. 4.0 — speedup van afspelen.
    pub fn set_speed(&self, speed: f32) {
        let clamped = speed.clamp(0.25, 4.0);
        self.speed_x100.store((clamped * 100.0) as u32, Ordering::Relaxed);
    }

    pub fn seek_to(&self, micros: u64) {
        *self.seek_to_micros.write() = Some(micros);
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        *self.state.write() = PlayerState::Stopped;
    }

    pub fn pause(&self) {
        if *self.state.read() == PlayerState::Playing {
            *self.state.write() = PlayerState::Paused;
        }
    }

    pub fn resume(&self) {
        if *self.state.read() == PlayerState::Paused {
            *self.state.write() = PlayerState::Playing;
        }
    }

    /// Laad een MIDI bestand en start playback vanaf 0.
    pub fn load_and_play(&self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Kan bestand niet lezen: {}", e))?;
        let (events, duration) = decode_smf(&bytes)?;
        info!("MIDI player: {} events geladen, duur {} ms", events.len(), duration / 1000);

        // Stop een eventuele lopende thread
        self.stop_flag.store(true, Ordering::Relaxed);
        // korte slaap om oude thread te laten exiten
        thread::sleep(Duration::from_millis(20));

        self.stop_flag.store(false, Ordering::Relaxed);
        self.pos_micros.store(0, Ordering::Relaxed);
        self.duration_micros.store(duration, Ordering::Relaxed);
        *self.seek_to_micros.write() = None;
        *self.state.write() = PlayerState::Playing;
        *self.file_loaded.write() = true;

        let state = self.state.clone();
        let pos = self.pos_micros.clone();
        let seek = self.seek_to_micros.clone();
        let stop_flag = self.stop_flag.clone();
        let speed = self.speed_x100.clone();
        let tx = self._out_tx_handle.clone();

        thread::spawn(move || {
            let mut idx = 0usize;
            let start = Instant::now();
            let mut play_origin_micros: u64 = 0; // virtual time at "real start"

            while !stop_flag.load(Ordering::Relaxed) {
                let s = *state.read();
                if s == PlayerState::Stopped {
                    break;
                }
                if s == PlayerState::Paused {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }

                // Apply pending seek
                let pending_seek = seek.write().take();
                if let Some(target) = pending_seek {
                    pos.store(target, Ordering::Relaxed);
                    // Reset start anchor — virtual time = target, real time = now
                    play_origin_micros = target;
                    let _start = Instant::now();
                    // Find first event >= target
                    idx = events.partition_point(|e| e.micros < target);
                    let _ = _start;  // unused
                    // Update reference instant
                    let _ = play_origin_micros;
                }

                if idx >= events.len() {
                    *state.write() = PlayerState::Stopped;
                    break;
                }

                // Bereken huidige virtual time op basis van real elapsed * speed
                let real_elapsed = start.elapsed().as_micros() as u64;
                let speed_factor = speed.load(Ordering::Relaxed) as f64 / 100.0;
                let virtual_now = play_origin_micros + (real_elapsed as f64 * speed_factor) as u64;
                pos.store(virtual_now, Ordering::Relaxed);

                let next_evt = &events[idx];
                if virtual_now >= next_evt.micros {
                    // Verzend
                    match tx.try_send(next_evt.msg.clone()) {
                        Ok(_) => {}
                        Err(TrySendError::Full(_)) => {
                            // Channel vol — wacht even, probeer opnieuw
                            thread::sleep(Duration::from_millis(2));
                            continue;
                        }
                        Err(TrySendError::Disconnected(_)) => {
                            warn!("MIDI player channel disconnected");
                            break;
                        }
                    }
                    idx += 1;
                } else {
                    // Slaap tot het volgende event
                    let wait_micros = (next_evt.micros - virtual_now) as f64 / speed_factor;
                    let cap_us = wait_micros.min(20_000.0) as u64; // max 20 ms voor responsiviteit
                    if cap_us > 0 {
                        thread::sleep(Duration::from_micros(cap_us));
                    }
                }
            }

            info!("MIDI player thread stopped");
        });

        Ok(())
    }
}
