//! HTTP test API voor self-testing JM-Orgue.
//!
//! Geactiveerd met `--test-api` of `--test-api=PORT` op de commandline.
//! Standaard port: 8765. Bind aan 127.0.0.1.
//!
//! Endpoints (allemaal JSON):
//!   GET  /status                  - audio_running, sample_rate, voice_count, peak L/R, organ_loaded
//!   GET  /organ                   - geladen orgel info (DTO)
//!   GET  /stops/drawn             - lijst van getrokken stops
//!   POST /stops/{stop_id}/toggle  - toggle stop
//!   POST /notes/{midi_note}/on?velocity=N&channel=N
//!   POST /notes/{midi_note}/off
//!   POST /panic                   - all notes off
//!   GET  /logs?lines=N            - tail van logfile
//!   POST /load_organ              - body: {"path":"..."} laad een .organ bestand
//!   POST /load_directory          - body: {"path":"..."} scan + laad sample map
//!   GET  /library                 - bekende orgels uit de library
//!   GET  /version                 - JM-Orgue versie
//!   POST /audio_output            - body: {"host","device","buffer_frames"} wissel audio-uitvoer
//!   POST /reverb                  - configureer algoritmische FDN-reverb
//!   GET  /division_volumes        - per-divisie volumes
//!   POST /record/start|stop       - MP3-opname starten/stoppen
//!   POST /midi/inject             - injecteer MIDI-bytes in de BLE-loop
//!   POST /midi/record/start|stop|save, GET /midi/record/status - MIDI-opname
//!   POST /settings/master|temperament|eq|reverb|pan|trem|save - instellingen zetten
//!   GET  /settings/organ|mirror   - opgeslagen settings / runtime-mirror
//!   POST /sampleset/trim|loop     - sampleset-tools
//!   (zie de route-match in handle() voor het complete overzicht)
//!
//! Voorbeeld:
//!   curl http://127.0.0.1:8765/status
//!   curl -X POST http://127.0.0.1:8765/notes/60/on?velocity=100
//!   curl -X POST http://127.0.0.1:8765/stops/Prestant_8/toggle

use crate::audio::AudioCommand;
use crate::state::{AppState, MidiRecording};
use serde_json::{json, Value};
use vpo_midi::MidiMessage;
use std::io::Read;
use std::path::PathBuf;
use std::thread;
use tracing::{info, warn};

/// Start de test-API thread. Returns direct.
pub fn start_test_api(state: AppState, port: u16, log_path: PathBuf) {
    thread::Builder::new()
        .name("test-api".into())
        .spawn(move || {
            let bind = format!("127.0.0.1:{}", port);
            let server = match tiny_http::Server::http(&bind) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Test-API kon niet starten op {}: {}", bind, e);
                    return;
                }
            };
            info!("=== TEST API actief op http://{} ===", bind);
            info!("    Probeer: curl http://{}/status", bind);

            for mut request in server.incoming_requests() {
                let url = request.url().to_string();
                let method = request.method().clone();
                let body = if matches!(method, tiny_http::Method::Post | tiny_http::Method::Put) {
                    let mut s = String::new();
                    request.as_reader().read_to_string(&mut s).ok();
                    s
                } else {
                    String::new()
                };

                let response = route(&state, &log_path, method, &url, &body);
                let _ = request.respond(response);
            }
        })
        .expect("test-api thread spawn");
}

fn route(
    state: &AppState,
    log_path: &PathBuf,
    method: tiny_http::Method,
    url: &str,
    body: &str,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    // Path + query splitsen
    let (path, query) = match url.find('?') {
        Some(i) => (&url[..i], &url[i + 1..]),
        None => (url, ""),
    };

    let method_name = method_str(&method);
    let result: Result<Value, (u16, String)> = match (method, path) {
        (tiny_http::Method::Get, "/status") => Ok(handle_status(state)),
        (tiny_http::Method::Get, "/organ") => Ok(handle_organ(state)),
        (tiny_http::Method::Get, "/stops/drawn") => Ok(handle_drawn_stops(state)),
        (tiny_http::Method::Get, "/version") => Ok(json!({ "version": env!("CARGO_PKG_VERSION") })),
        (tiny_http::Method::Get, "/logs") => Ok(handle_logs(log_path, query)),
        (tiny_http::Method::Get, "/division_volumes") => Ok(handle_division_volumes(state)),
        (tiny_http::Method::Get, "/library") => Ok(handle_library(state)),
        (tiny_http::Method::Post, "/panic") => {
            let _ = body;
            state.send_audio_command(AudioCommand::AllNotesOff);
            Ok(json!({"ok": true}))
        }
        (tiny_http::Method::Post, "/load_organ") => handle_load_organ(state, body),
        (tiny_http::Method::Post, "/load_directory") => handle_load_directory(state, body),
        (tiny_http::Method::Post, "/audio_output") => handle_audio_output(state, body),
        (tiny_http::Method::Post, "/reverb") => handle_reverb(state, body),
        (tiny_http::Method::Post, "/sampleset/scan") => handle_sampleset_scan(body),
        (tiny_http::Method::Post, "/sampleset/trim") => handle_sampleset_trim(body),
        (tiny_http::Method::Post, "/sampleset/loop_scan") => handle_sampleset_loop_scan(body),
        (tiny_http::Method::Post, "/sampleset/loop_apply") => handle_sampleset_loop_apply(body),
        (tiny_http::Method::Post, "/record/start") => handle_record_start(state, body),
        (tiny_http::Method::Post, "/record/stop") => handle_record_stop(state),
        // MIDI-opname (events) — los van de MP3-opname hierboven. Voor self-testing van
        // de capture-keten: injecteer MIDI door de échte BLE-loop heen.
        (tiny_http::Method::Post, "/midi/record/start") => Ok(handle_midi_record_start(state)),
        (tiny_http::Method::Get, "/midi/record/status") => Ok(handle_midi_record_status(state)),
        (tiny_http::Method::Post, "/midi/record/stop") => Ok(handle_midi_record_stop(state)),
        (tiny_http::Method::Post, "/midi/record/save") => handle_midi_record_save(state, body),
        (tiny_http::Method::Post, "/midi/inject") => handle_midi_inject(state, query),
        // Per-orgel instellingen (self-testing van de persistentie-migratie, Onderdeel 1).
        (tiny_http::Method::Post, "/settings/master") => handle_set_master(state, query),
        (tiny_http::Method::Post, "/settings/temperament") => handle_set_temperament(state, query),
        (tiny_http::Method::Post, "/settings/eq") => handle_set_eq(state, query),
        (tiny_http::Method::Post, "/settings/reverb") => handle_set_reverb(state, query),
        (tiny_http::Method::Post, "/settings/pan") => handle_set_pan(state, query),
        (tiny_http::Method::Post, "/settings/trem") => handle_set_trem(state, query),
        (tiny_http::Method::Post, "/settings/save") => Ok(handle_settings_save(state)),
        (tiny_http::Method::Get, "/settings/organ") => Ok(handle_settings_organ(state)),
        (tiny_http::Method::Get, "/settings/mirror") => Ok(handle_settings_mirror(state)),
        (tiny_http::Method::Post, p) if p.starts_with("/stops/") && p.ends_with("/toggle") => {
            let stop_id = &p["/stops/".len()..p.len() - "/toggle".len()];
            handle_toggle_stop(state, stop_id)
        }
        (tiny_http::Method::Post, p) if p.starts_with("/notes/") && p.ends_with("/on") => {
            let note_str = &p["/notes/".len()..p.len() - "/on".len()];
            handle_note_on(state, note_str, query)
        }
        (tiny_http::Method::Post, p) if p.starts_with("/notes/") && p.ends_with("/off") => {
            let note_str = &p["/notes/".len()..p.len() - "/off".len()];
            handle_note_off(state, note_str, query)
        }
        _ => Err((404, format!("Geen route voor {} {}", method_name, url))),
    };

    let (status, body_str) = match result {
        Ok(v) => (200, v.to_string()),
        Err((code, msg)) => (
            code,
            json!({ "error": msg }).to_string(),
        ),
    };

    tiny_http::Response::from_string(body_str)
        .with_status_code(status)
        .with_header(
            tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        )
}

fn method_str(m: &tiny_http::Method) -> &'static str {
    match m {
        tiny_http::Method::Get => "GET",
        tiny_http::Method::Post => "POST",
        tiny_http::Method::Put => "PUT",
        tiny_http::Method::Delete => "DELETE",
        _ => "?",
    }
}

// ============ Handlers ============

fn handle_status(state: &AppState) -> Value {
    let audio = state.audio_player.read();
    let (sample_rate, voice_count, peaks) = match audio.as_ref() {
        Some(p) => (*p.sample_rate.read(), p.voice_count(), p.peak_meters()),
        None => (0, 0, (0.0, 0.0)),
    };
    let organ = state.loaded_organ_info.read();
    let drawn = state.drawn_stops.read();

    json!({
        "audio_running": audio.is_some(),
        "sample_rate": sample_rate,
        "voice_count": voice_count,
        "peak_left": peaks.0,
        "peak_right": peaks.1,
        "organ_loaded": organ.is_some(),
        "organ_name": organ.as_ref().map(|o| o.name.clone()),
        "drawn_stop_count": drawn.len(),
        "midi_connected": *state.midi_connected.read(),
    })
}

fn handle_organ(state: &AppState) -> Value {
    let organ = state.loaded_organ_info.read();
    match organ.as_ref() {
        Some(o) => serde_json::to_value(o).unwrap_or(json!(null)),
        None => json!(null),
    }
}

fn handle_drawn_stops(state: &AppState) -> Value {
    let stops = state.drawn_stops.read();
    json!({ "drawn_stops": stops.clone() })
}

fn handle_division_volumes(state: &AppState) -> Value {
    let gains = state.division_gains.read();
    json!({ "gains": gains.clone() })
}

fn handle_library(state: &AppState) -> Value {
    let lib = state.organ_library.read();
    let entries: Vec<_> = lib.organs.iter().map(|o| json!({
        "id": o.id,
        "name": o.name,
        "builder": o.builder,
        "location": o.location,
        "year": o.year,
        "stop_count": o.stop_count,
        "source_type": o.source_type,
        "source_path": o.source_path,
    })).collect();
    json!({ "organs": entries })
}

fn handle_load_organ(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?;
    let path = v.get("path").and_then(|p| p.as_str())
        .ok_or((400u16, "Missing 'path' in body".to_string()))?;

    info!("test_api: load_organ verzocht: {}", path);
    let organ_info = crate::commands::do_load_organ(state, path)
        .map_err(|e| (500u16, e))?;

    Ok(json!({
        "loaded": true,
        "path": path,
        "organ_name": organ_info.name,
        "stop_count": organ_info.stop_count,
        "divisions": organ_info.divisions.len(),
    }))
}

fn handle_load_directory(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?;
    let path = v.get("path").and_then(|p| p.as_str())
        .ok_or((400u16, "Missing 'path' in body".to_string()))?;

    info!("test_api: load_directory verzocht: {}", path);
    let organ_info = crate::commands::do_load_samples_from_directory(state, path)
        .map_err(|e| (500u16, e))?;

    Ok(json!({
        "loaded": true,
        "path": path,
        "organ_name": organ_info.name,
        "stop_count": organ_info.stop_count,
        "divisions": organ_info.divisions.len(),
    }))
}

fn handle_audio_output(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?;
    let host = v.get("host").and_then(|x| x.as_str()).map(|s| s.to_string());
    let device = v.get("device").and_then(|x| x.as_str()).map(|s| s.to_string());
    let buffer_frames = v.get("buffer_frames").and_then(|x| x.as_u64()).map(|n| n as u32);

    info!("test_api: audio_output switch host={:?} device={:?} buffer={:?}", host, device, buffer_frames);
    let organ = state.switch_audio_output(crate::audio::AudioOutputConfig {
        host_name: host,
        device_name: device,
        buffer_frames,
    }).map_err(|e| (500u16, e))?;

    // Mimic the frontend: reload the current organ so its samples re-register
    // on the freshly created audio thread.
    if let Some(id) = organ.clone() {
        if id.to_lowercase().ends_with(".organ") {
            crate::commands::do_load_organ(state, &id).map_err(|e| (500u16, e))?;
        } else {
            crate::commands::do_load_samples_from_directory(state, &id).map_err(|e| (500u16, e))?;
        }
    }

    let (cur_host, cur_device, cur_buf) = state.audio_player.read().as_ref()
        .map(|p| (
            p.current_host.read().clone(),
            p.current_device.read().clone(),
            *p.current_buffer_frames.read(),
        ))
        .unwrap_or_default();
    Ok(json!({
        "ok": true,
        "reloaded_organ": organ,
        "host": cur_host,
        "device": cur_device,
        "buffer_frames": cur_buf,
    }))
}

/// Configureer de algoritmische FDN-reverb (voor self-testing van de galm-staart).
/// Body: { "preset"?: u8, "rt60"?: f32, "pre_delay_ms"?: f32, "damping"?: f32,
///         "room_size"?: f32, "mix": f32 }. Zet automatisch reverb-type op algoritmisch.
fn handle_reverb(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?;
    let preset = v.get("preset").and_then(|x| x.as_u64()).map(|n| n as u8);
    let rt60 = v.get("rt60").and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(2.0);
    let pre_delay_ms = v.get("pre_delay_ms").and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(25.0);
    let damping = v.get("damping").and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(0.4);
    let room_size = v.get("room_size").and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(1.0);
    let mix = v.get("mix").and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(0.5);

    info!("test_api: reverb preset={:?} rt60={} mix={}", preset, rt60, mix);
    state.send_audio_command(AudioCommand::SetReverbType { algorithmic: true });
    state.send_audio_command(AudioCommand::SetAlgorithmicReverb {
        preset, rt60, pre_delay_ms, damping, room_size, mix,
    });
    Ok(json!({ "ok": true, "preset": preset, "rt60": rt60, "mix": mix }))
}

fn handle_record_start(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let path = serde_json::from_str::<Value>(body).ok()
        .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(|s| s.to_string()));
    let out = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join("jm_rec_apitest.mp3"),
    };
    let sr = state.audio_player.read().as_ref().map(|p| *p.sample_rate.read()).unwrap_or(48000);
    *crate::audio::RECORDER.write() = Some(state.recorder.clone());
    state.recorder.start(out.clone(), sr).map_err(|e| (500u16, e))?;
    Ok(json!({"recording": true, "path": out.to_string_lossy(), "sample_rate": sr}))
}

fn handle_record_stop(state: &AppState) -> Result<Value, (u16, String)> {
    state.recorder.stop().map_err(|e| (500u16, e))?;
    let s = state.recorder.status();
    // Decodeer het resultaat en bereken RMS zodat we hard kunnen verifiëren dat de
    // opname écht audio bevat (niet stil).
    let mut rms = 0.0f64;
    let mut decoded_samples = 0usize;
    if let Some(ref p) = s.path {
        if let Ok((samples, _ch, _r)) = crate::silence::decode_mp3_interleaved(std::path::Path::new(p)) {
            decoded_samples = samples.len();
            if !samples.is_empty() {
                let sumsq: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
                rms = (sumsq / samples.len() as f64).sqrt();
            }
        }
    }
    Ok(json!({
        "frames_written": s.frames_written,
        "frames_dropped": s.frames_dropped,
        "seconds": s.seconds,
        "path": s.path,
        "error": s.error,
        "decoded_samples": decoded_samples,
        "rms": rms,
    }))
}

fn handle_sampleset_scan(body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("bad json: {}", e)))?;
    let dir = v.get("directory").and_then(|x| x.as_str()).ok_or((400u16, "missing 'directory'".to_string()))?;
    let thr = v.get("threshold_db").and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(-60.0);
    let report = crate::silence::scan(dir, thr);
    serde_json::to_value(&report).map_err(|e| (500u16, format!("serialize: {}", e)))
}

fn handle_sampleset_trim(body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("bad json: {}", e)))?;
    let dir = v.get("directory").and_then(|x| x.as_str()).ok_or((400u16, "missing 'directory'".to_string()))?;
    let thr = v.get("threshold_db").and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(-60.0);
    let pre = v.get("preroll_ms").and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(5.0);
    let report = crate::silence::trim(dir, thr, pre);
    serde_json::to_value(&report).map_err(|e| (500u16, format!("serialize: {}", e)))
}

fn handle_sampleset_loop_scan(body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("bad json: {}", e)))?;
    let dir = v.get("directory").and_then(|x| x.as_str()).ok_or((400u16, "missing 'directory'".to_string()))?;
    let report = crate::loop_tool::scan(dir);
    serde_json::to_value(&report).map_err(|e| (500u16, format!("serialize: {}", e)))
}

fn handle_sampleset_loop_apply(body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("bad json: {}", e)))?;
    let dir = v.get("directory").and_then(|x| x.as_str()).ok_or((400u16, "missing 'directory'".to_string()))?;
    let only_missing = v.get("only_missing").and_then(|x| x.as_bool()).unwrap_or(true);
    let report = crate::loop_tool::apply(dir, only_missing);
    serde_json::to_value(&report).map_err(|e| (500u16, format!("serialize: {}", e)))
}

fn handle_toggle_stop(state: &AppState, stop_id: &str) -> Result<Value, (u16, String)> {
    match state.toggle_stop(stop_id) {
        Some(active) => Ok(json!({ "stop_id": stop_id, "active": active })),
        None => Err((404u16, format!("Onbekende stop_id: {}", stop_id))),
    }
}

fn handle_note_on(state: &AppState, note_str: &str, query: &str) -> Result<Value, (u16, String)> {
    let note: u8 = note_str
        .parse()
        .map_err(|_| (400u16, format!("Ongeldige MIDI noot: {}", note_str)))?;
    let velocity: u8 = parse_query(query, "velocity").unwrap_or(100);
    let channel: u8 = parse_query(query, "channel").unwrap_or(0);

    // Stuur een NoteOn naar de audio thread voor alle getrokken stops
    let drawn: Vec<String> = state.drawn_stops.read().clone();
    let organ = state.loaded_organ_info.read();
    let organ = organ.as_ref().ok_or((400u16, "Geen orgel geladen".to_string()))?;

    let mut triggered = 0;
    for drawn_id in &drawn {
        for div in &organ.divisions {
            for stop in &div.stops {
                if &stop.id == drawn_id {
                    if note >= stop.first_midi_note && note <= stop.last_midi_note {
                        // Pijpnummers zijn 1-based in de audio engine (consistent met commands.rs en state.rs)
                        let pipe_num = (note - stop.first_midi_note) as u32 + 1;
                        state.send_audio_command(AudioCommand::NoteOn {
                            stop_id: stop.internal_stop_id,
                            pipe_num,
                            midi_note: note,
                            velocity: velocity as f32 / 127.0,
                        });
                        triggered += 1;
                    }
                }
            }
        }
    }

    Ok(json!({
        "note": note,
        "velocity": velocity,
        "channel": channel,
        "voices_triggered": triggered,
    }))
}

fn handle_note_off(state: &AppState, note_str: &str, _query: &str) -> Result<Value, (u16, String)> {
    let note: u8 = note_str
        .parse()
        .map_err(|_| (400u16, format!("Ongeldige MIDI noot: {}", note_str)))?;

    let drawn: Vec<String> = state.drawn_stops.read().clone();
    let organ = state.loaded_organ_info.read();
    let organ = organ.as_ref().ok_or((400u16, "Geen orgel geladen".to_string()))?;

    let mut released = 0;
    for drawn_id in &drawn {
        for div in &organ.divisions {
            for stop in &div.stops {
                if &stop.id == drawn_id {
                    if note >= stop.first_midi_note && note <= stop.last_midi_note {
                        // Pijpnummers zijn 1-based in de audio engine (consistent met commands.rs en state.rs)
                        let pipe_num = (note - stop.first_midi_note) as u32 + 1;
                        state.send_audio_command(AudioCommand::NoteOff {
                            stop_id: stop.internal_stop_id,
                            pipe_num,
                        });
                        released += 1;
                    }
                }
            }
        }
    }

    Ok(json!({ "note": note, "voices_released": released }))
}

// ============ MIDI-opname (events) — self-testing van de capture-keten ============

fn handle_midi_record_start(state: &AppState) -> Value {
    *state.midi_recording.write() = Some(MidiRecording {
        events: Vec::new(),
        start_time: std::time::Instant::now(),
        active: true,
    });
    json!({ "ok": true })
}

fn handle_midi_record_status(state: &AppState) -> Value {
    let rec = state.midi_recording.read();
    match rec.as_ref() {
        Some(r) => json!({
            "recording": r.active,
            "event_count": r.events.len(),
            "seconds": r.start_time.elapsed().as_secs_f64(),
        }),
        None => json!({ "recording": false, "event_count": 0, "seconds": 0.0 }),
    }
}

fn handle_midi_record_stop(state: &AppState) -> Value {
    // Zet de capture echt uit (zelfde semantiek als commands::stop_midi_recording);
    // de events blijven staan voor save/afspelen.
    let mut rec = state.midi_recording.write();
    let count = match rec.as_mut() {
        Some(r) => {
            r.active = false;
            r.events.len()
        }
        None => 0,
    };
    json!({ "ok": true, "event_count": count })
}

fn handle_midi_record_save(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON: {}", e)))?;
    let path = v.get("path").and_then(|p| p.as_str())
        .ok_or((400u16, "Veld 'path' ontbreekt".to_string()))?;
    let rec = state.midi_recording.read();
    let recording = rec.as_ref().ok_or((400u16, "Geen opname actief".to_string()))?;
    if recording.events.is_empty() {
        return Err((400u16, "Geen events opgenomen".to_string()));
    }
    let data = crate::commands::encode_smf(&recording.events);
    if let Some(parent) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, &data).map_err(|e| (500u16, format!("Schrijven mislukt: {}", e)))?;
    Ok(json!({ "ok": true, "bytes": data.len(), "event_count": recording.events.len() }))
}

/// Injecteer een NoteOn/NoteOff door de échte BLE-MIDI-loop (die de capture-hook aanroept).
/// `?note=N&on=1|0&velocity=N&channel=N`. Hiermee is de volledige opname-keten testbaar.
fn handle_midi_inject(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let note: u8 = parse_query(query, "note")
        .ok_or((400u16, "Parameter 'note' ontbreekt".to_string()))?;
    let on_raw: String = parse_query(query, "on").unwrap_or_else(|| "1".to_string());
    let on = on_raw == "1" || on_raw.eq_ignore_ascii_case("true");
    let velocity: u8 = parse_query(query, "velocity").unwrap_or(100);
    let channel: u8 = parse_query(query, "channel").unwrap_or(0);
    let msg = if on {
        MidiMessage::NoteOn { channel, note, velocity }
    } else {
        MidiMessage::NoteOff { channel, note, velocity: 0 }
    };
    state.ble_message_tx.send(msg)
        .map_err(|e| (500u16, format!("Injectie mislukt: {}", e)))?;
    Ok(json!({ "ok": true, "note": note, "on": on, "channel": channel }))
}

// ============ Per-orgel instellingen — self-testing van de persistentie-migratie ============

fn handle_set_master(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let db: f32 = parse_query(query, "db").ok_or((400u16, "Parameter 'db' ontbreekt".to_string()))?;
    state.send_audio_command(AudioCommand::SetMasterGain(db));
    *state.master_volume_db.write() = Some(db);
    Ok(json!({ "ok": true, "db": db }))
}

fn handle_set_temperament(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let fine: f32 = parse_query(query, "fine").unwrap_or(0.0);
    let name: String = parse_query(query, "name").unwrap_or_else(|| "test".to_string());
    let offsets = [0.0f32; 12];
    state.send_audio_command(AudioCommand::SetTemperament { note_offsets: offsets, fine_tune: fine });
    *state.temperament_settings.write() = Some(crate::library::TemperamentSettingsSaved {
        name: name.clone(),
        custom_cents: Some(offsets),
        fine_tune_cents: fine,
        a4_hz: 440.0,
    });
    Ok(json!({ "ok": true, "name": name, "fine": fine }))
}

fn handle_set_eq(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let gain: f32 = parse_query(query, "gain").unwrap_or(0.0);
    let eq = crate::library::EqSettingsSaved {
        enabled: true, low_freq: 200.0, low_gain: 0.0,
        mid_freq: 1000.0, mid_gain: gain, mid_q: 1.0,
        high_freq: 4000.0, high_gain: 0.0,
    };
    state.send_audio_command(AudioCommand::SetParametricEq {
        enabled: eq.enabled, low_freq: eq.low_freq, low_gain: eq.low_gain,
        mid_freq: eq.mid_freq, mid_gain: eq.mid_gain, mid_q: eq.mid_q,
        high_freq: eq.high_freq, high_gain: eq.high_gain,
    });
    *state.eq_settings.write() = Some(eq);
    Ok(json!({ "ok": true, "mid_gain": gain }))
}

fn handle_set_reverb(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let mix: f32 = parse_query(query, "mix").unwrap_or(30.0);
    let rtype: String = parse_query(query, "type").unwrap_or_else(|| "algorithmic".to_string());
    *state.reverb_settings.write() = Some(crate::library::ReverbSettingsSaved {
        reverb_type: rtype.clone(), mix, algorithmic_preset: None,
        rt60: 2.5, pre_delay_ms: 25.0, damping: 50.0, room_size: 80.0, ir_path: None,
    });
    Ok(json!({ "ok": true, "mix": mix, "type": rtype }))
}

fn handle_set_pan(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let division: String = parse_query(query, "division").unwrap_or_else(|| "Manuaal".to_string());
    let pan: f32 = parse_query(query, "pan").unwrap_or(0.0);
    state.division_pans.write().insert(division.clone(), pan);
    Ok(json!({ "ok": true, "division": division, "pan": pan }))
}

fn handle_set_trem(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let division: String = parse_query(query, "division").unwrap_or_else(|| "Manuaal".to_string());
    let rate: f32 = parse_query(query, "rate").unwrap_or(6.0);
    state.division_tremulants.write().insert(division.clone(), (true, rate, 10.0, 15.0));
    Ok(json!({ "ok": true, "division": division, "rate": rate }))
}

fn handle_settings_save(state: &AppState) -> Value {
    // Behoud bestaande presets (do_save_organ_settings overschrijft het preset-veld).
    let organ_id = state.current_organ_id.read().clone();
    let presets = organ_id.as_ref()
        .and_then(|id| state.organ_library.read().settings.get(id).map(|s| s.presets.clone()))
        .unwrap_or_default();
    crate::commands::do_save_organ_settings(state, presets);
    json!({ "ok": true })
}

fn handle_settings_organ(state: &AppState) -> Value {
    let organ_id = match state.current_organ_id.read().clone() {
        Some(id) => id,
        None => return json!({ "loaded": false }),
    };
    let lib = state.organ_library.read();
    match lib.settings.get(&organ_id) {
        Some(s) => serde_json::to_value(s).unwrap_or_else(|_| json!({})),
        None => json!({ "loaded": true, "settings": null }),
    }
}

/// Lees de runtime AppState per-divisie DSP-mirrors direct (isoleert restore van save/webview).
fn handle_settings_mirror(state: &AppState) -> Value {
    let pans: Vec<Value> = state.division_pans.read().iter()
        .map(|(d, p)| json!({ "division": d, "pan": p })).collect();
    let trems: Vec<Value> = state.division_tremulants.read().iter()
        .map(|(d, &(en, r, a, p))| json!({ "division": d, "enabled": en, "rate": r, "amp": a, "pitch": p })).collect();
    let swells: Vec<Value> = state.division_swell_configs.read().iter()
        .map(|(d, &(mdb, c))| json!({ "division": d, "min_db": mdb, "cutoff": c })).collect();
    let winds: Vec<Value> = state.wind_group_configs.read().iter()
        .map(|(g, &(en, r, dmp, s))| json!({ "group": g, "enabled": en, "reservoir": r, "damping": dmp, "sag": s })).collect();
    json!({ "pans": pans, "tremulants": trems, "swells": swells, "winds": winds })
}

fn handle_logs(log_path: &PathBuf, query: &str) -> Value {
    let n: usize = parse_query(query, "lines").unwrap_or(50);
    let content = match std::fs::read_to_string(log_path) {
        Ok(s) => s,
        Err(e) => return json!({ "error": e.to_string(), "log_path": log_path.to_string_lossy() }),
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    let tail: Vec<String> = lines[start..].iter().map(|s| s.to_string()).collect();
    json!({ "lines": tail, "log_path": log_path.to_string_lossy() })
}

// ============ Helpers ============

fn parse_query<T: std::str::FromStr>(query: &str, key: &str) -> Option<T> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return url_decode(v).parse().ok();
            }
        }
    }
    None
}

/// Minimale percent-decode (+%XX) zodat query-waarden met spaties/speciale tekens kloppen.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => { out.push(b' '); i += 1; }
            c => { out.push(c); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
