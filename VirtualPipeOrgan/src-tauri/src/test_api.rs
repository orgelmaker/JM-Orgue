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
//!   POST /debug/restart_needed   - testhaak: watchdog-vlag zetten (noodherstel afdwingen)
//!   POST /stops/set               - body: {"ids":[..]} hele registratie zetten (setzer/General Cancel)
//!   POST /couplers/set            - body: {"ids":[..]} koppels in één keer zetten (tweede helft setzer)
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
//!   GET  /midi/archive/status     - automatisch MIDI-archief: enabled/archiving/event_count/files_written/last_file/dir
//!   POST /midi/archive/config?enabled=1|0&silence=N&min_notes=N&min_secs=N&dir=<url-encoded> (ontbrekende = huidige)
//!   POST /midi/archive/flush      - lopende archief-take nu afsluiten/schrijven → {"ok","file"}
//!   GET  /midi/archive/list       - 10 nieuwste archiefbestanden
//!   POST /midi/player/play        - body: {"path":"..."} .mid afspelen via de speler
//!   POST /midi/player/stop, GET /midi/player/status - speler stoppen / status
//!   POST /midi/inject?cc=N&value=V&channel=C - ControlChange door de MIDI-lus (zwel/crescendo meetbaar)
//!   POST /midi/mapping            - body {"division","channel"|null,"first"?,"last"?,"transpose"?} klavier-mapping
//!   GET  /held_notes              - ingedrukte toetsen [[ch,note,vel]..]
//!   GET  /crescendo               - {enabled,stage,stages,num_stages,stage_stops,binding,active_stops,hysteresis}
//!   POST /crescendo/config        - body {"stages":[[id..]..],"enabled":bool,"num_stages"?:n}
//!   POST /crescendo/binding       - body {"channel","cc","min"?,"max"?,"invert"?} | {"clear":true}
//!   POST /crescendo/stage?stage=N - trap zetten zoals een UI-klik
//!   GET  /swell                   - per divisie {division,index,position,binding,min_db,cutoff}
//!   POST /swell/binding           - body {"division","channel","cc","min"?,"max"?,"invert"?} | {"division","clear":true}
//!   POST /audio/test_signal       - ?channel=<n>&kind=0|1&level_db=<dB>; zonder channel = uit
//!   POST /audio/mix_chunks        - ?n=1..8: in hoeveel stukken de mengloop zijn stemmen verdeelt
//!   POST /tremulant?division=<naam>&active=0|1 - tremulant van een divisie live
//!        aan/uit (zelfde kern als het Tauri-commando set_tremulant) →
//!        {"ok","division","active","stops"}; stops = aantal registers met échte
//!        tremulant-OPNAMEN dat geschakeld is (0 = alleen de synth-LFO).
//!        Spiegelt in GET /state.tremulant[i]. Alleen test-scope, niet remote.
//!   POST /settings/master|temperament|eq|reverb|pan|trem|save - instellingen zetten
//!        /settings/trem?division=..&rate=..&enabled=0|1 (enabled default 1)
//!        /settings/temperament?name=..&fine=..&cents=c0,..,c11&retune=0|1 (retune default 0 = Origineel)
//!   POST /temperament             - body: {"name":"..","cents":[12],"retune":bool,"fine":..} (zelfde als hierboven, JSON)
//!   GET  /tuning                  - hertemperen: retune_pipes/retune_total (orgel) + retune/name/fine_tune (mirror)
//!   POST /settings/wind?enabled=0|1[&reservoir=&damping=&sag=] - windmodel voor alle groepen
//!   GET  /wind - live winddruk, stemmen en cent-afwijking per windgroep
//!   POST /record/stop             - geeft ook peak_hz (FFT-piek van de laatste ~2,7 s, toonhoogtemeting)
//!   GET  /settings/organ|mirror   - opgeslagen settings / runtime-mirror (mirror bevat ook perspectives)
//!   GET  /perspectives            - microfoonperspectieven van het geladen orgel: [{name,enabled,gain_db,loaded,slot,pipe_count}]
//!   POST /perspectives            - body {"name":"rear","enabled":true} en/of {"name":"rear","gain_db":-6} → {"reload_needed":bool}
//!   GET  /ranks                   - lagen per register: [{internal_stop_id,dto_id,name,layers:[{index,name,perspective,pipes_nonempty}]}] + stacked_stops
//!   GET  /status                  - bevat ook layered_stops (registers met ≥1 echte extra rank)
//!   POST /sampleset/trim|loop     - sampleset-tools
//!   (zie de route-match in handle() voor het complete overzicht)
//!
//! Voorbeeld:
//!   curl http://127.0.0.1:8765/status
//!   curl -X POST http://127.0.0.1:8765/notes/60/on?velocity=100
//!   curl -X POST http://127.0.0.1:8765/stops/Prestant_8/toggle
//!
//! Afstandsbediening in het netwerk (0.7.38) — dezelfde router als ApiServer
//! met scope Remote: bindt op 0.0.0.0:<poort> (standaard 8766, zie remote.rs),
//! token VERPLICHT (pad-prefix /r/<token>/… of header X-Remote-Token) en een
//! strikte whitelist (route_shared): GET / (ingebedde pagina), /organ, /status,
//! /state (compact), /version; POST /stops/{id}/toggle, /couplers/{id}/toggle,
//! /master?db=, /action/{code}, /panic. Alles daarbuiten geeft 404 op de
//! remote-poort. GET /organ geeft nu (ook op de test-API) altijd actuele
//! drawn-/koppelvlaggen (organ_info_merged). Test-only routes voor de
//! afstandsbediening zelf: GET /remote/status, POST /remote/enable
//! {"enabled":bool,"port":u16?}, POST /remote/new_token, GET/POST
//! /remote/layout (indeling; body = RemoteLayoutSaved-JSON).
//!
//! Indeling (0.7.39): GET /organ op de remote-scope volgt de indeling die het
//! hoofdvenster publiceerde (Tauri-commando set_remote_layout) — divisie-
//! volgorde/-selectie, registervolgorde, alléén de koppels die ook op het
//! orgelscherm zichtbaar zijn, per divisie `div_index` (DTO-index, zodat
//! actiecode 24+i blijft kloppen) en `show_tremulant`, plus een `layout`-blok
//! (coupler_placement/knob_shape/show_*/rev). GET /state draagt `layout_rev`
//! zodat de pagina bij een indelingswijziging /organ opnieuw ophaalt. Let op:
//! het filteren van koppels is cosmetisch — POST /couplers/{id}/toggle werkt
//! (met geldig token) voor élke koppel-id, ook een verborgen.

use crate::audio::AudioCommand;
use crate::state::{AppState, MidiRecording};
use serde_json::{json, Value};
use vpo_midi::MidiMessage;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::thread;
use tracing::{info, warn};

/// Poort waarop de test-API luistert (0 = niet gestart). remote::set_enabled_inner
/// weigert dezelfde poort voor de afstandsbediening.
pub static TEST_API_PORT: AtomicU16 = AtomicU16::new(0);

/// Ingebedde pagina van de afstandsbediening (GET / op de remote-poort).
pub const REMOTE_HTML: &str = include_str!("../remote/index.html");

/// Welke router-variant: Test (127.0.0.1, alles, geen token) of Remote
/// (0.0.0.0, whitelist, token verplicht).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ApiScope {
    Test,
    Remote,
}

/// Draaiende HTTP-server (tiny_http) met eigen thread. Geen Drop-impl (stop()
/// verplaatst velden).
pub struct ApiServer {
    server: Arc<tiny_http::Server>,
    thread: Option<thread::JoinHandle<()>>,
    pub port: u16,
    pub scope: ApiScope,
}

impl ApiServer {
    /// Bindt SYNCHROON; een bind-fout gaat naar de aanroeper.
    pub fn start(
        state: AppState,
        bind_addr: &str,
        port: u16,
        token: Option<String>,
        scope: ApiScope,
        log_path: PathBuf,
    ) -> Result<ApiServer, String> {
        let bind = format!("{}:{}", bind_addr, port);
        let server = Arc::new(
            tiny_http::Server::http(&bind).map_err(|e| format!("Kan niet luisteren op {}: {}", bind, e))?,
        );
        info!("=== API {:?} actief op http://{} ===", scope, bind);
        let srv = server.clone();
        let name = if scope == ApiScope::Test { "test-api" } else { "remote-api" };
        let thread = thread::Builder::new()
            .name(name.into())
            .spawn(move || serve_loop(srv, state, scope, token, log_path))
            .map_err(|e| e.to_string())?;
        Ok(ApiServer { server, thread: Some(thread), port, scope })
    }

    /// Netjes stoppen. tiny_http 0.12: unblock() laat recv() Err geven → de
    /// serve_loop eindigt; Drop van Server zet de close-vlag en verbindt naar het
    /// luisteradres om de accept-thread te wekken — bij bind 0.0.0.0 is dat
    /// TcpStream::connect("0.0.0.0:port") en dat FAALT op Windows, dus de
    /// listener bleef open tot de volgende verbinding. Daarom een loopback-kick.
    pub fn stop(mut self) {
        let (port, scope) = (self.port, self.scope);
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        drop(self.server);
        let _ = std::net::TcpStream::connect(("127.0.0.1", port));
        thread::sleep(std::time::Duration::from_millis(50));
        info!("API {:?} op poort {} gestopt", scope, port);
    }
}

/// Start de test-API (127.0.0.1, geen token). Leeft zolang het proces.
pub fn start_test_api(state: AppState, port: u16, log_path: PathBuf) {
    match ApiServer::start(state, "127.0.0.1", port, None, ApiScope::Test, log_path) {
        Ok(s) => {
            info!("    Probeer: curl http://127.0.0.1:{}/status", port);
            TEST_API_PORT.store(port, Ordering::Relaxed);
            Box::leak(Box::new(s));
        }
        Err(e) => warn!("Test-API kon niet starten: {}", e),
    }
}

/// Request-lus van één ApiServer (eigen thread).
fn serve_loop(server: Arc<tiny_http::Server>, state: AppState, scope: ApiScope, token: Option<String>, log_path: PathBuf) {
    for mut request in server.incoming_requests() {
        let url = request.url().to_string();
        let method = request.method().clone();
        let header_token: Option<String> = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("X-Remote-Token"))
            .map(|h| h.value.as_str().to_string());
        // Body alleen bij POST/PUT en begrensd (64 KiB): Content-Length groter
        // → lege body → 400 in de handler; de lezer zelf is óók begrensd (take),
        // zodat een body zonder/met foutieve Content-Length nooit meer dan
        // 64 KiB in het geheugen trekt. Precies op de grens (64 KiB gelezen,
        // mogelijk afgekapt) → 413.
        const BODY_MAX: u64 = 65_536;
        let body = if matches!(method, tiny_http::Method::Post | tiny_http::Method::Put)
            && request.body_length().unwrap_or(0) <= BODY_MAX as usize
        {
            let mut s = String::new();
            std::io::Read::take(request.as_reader(), BODY_MAX).read_to_string(&mut s).ok();
            s
        } else {
            String::new()
        };
        if body.len() as u64 >= BODY_MAX {
            let _ = request.respond(json_response(413, json!({ "error": "body te groot (max 64 KiB)" })));
            continue;
        }
        // Bij scope Remote nooit de ruwe url loggen: die bevat het token.
        let log_url = match (scope, token.as_deref()) {
            (ApiScope::Remote, Some(t)) => strip_token_prefix(&url, t).unwrap_or_else(|| "<zonder token>".to_string()),
            _ => url.clone(),
        };

        // catch_unwind: een panic in een handler mag de API-thread niet
        // doden — de server-lus stopte dan voorgoed ("fetch failed" bij
        // elke volgende aanroep terwijl de app gewoon doorspeelde).
        let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatch(&state, &log_path, scope, token.as_deref(), header_token.as_deref(), method, &url, &body)
        }))
        .unwrap_or_else(|_| {
            warn!("API-handler panic op {} — 500 teruggegeven, server draait door", log_url);
            json_response(500, json!({ "error": "interne panic in api-handler" }))
        });
        let _ = request.respond(response);
    }
}

// ============ Token + dispatch ============

/// Vergelijking in constante tijd (geen vroege exit op het eerste verschil).
fn token_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// "/r/<token>/rest" → Some("/rest") als het token klopt; Some("") of Some("?…")
/// als er niets achter het token staat; None als prefix/token niet klopt.
fn strip_token_prefix(url: &str, token: &str) -> Option<String> {
    let rest = url.strip_prefix("/r/")?;
    let seg_end = rest.find(|c| c == '/' || c == '?').unwrap_or(rest.len());
    let segment = &rest[..seg_end];
    if segment.is_empty() || !token_eq(segment, token) {
        return None;
    }
    Some(rest[seg_end..].to_string())
}

fn json_response(status: u16, v: Value) -> tiny_http::Response<Cursor<Vec<u8>>> {
    tiny_http::Response::from_string(v.to_string())
        .with_status_code(status)
        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
}

fn html_response(html: &'static str) -> tiny_http::Response<Cursor<Vec<u8>>> {
    tiny_http::Response::from_string(html)
        .with_status_code(200)
        .with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap())
        .with_header(tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap())
}

fn redirect(location: String) -> tiny_http::Response<Cursor<Vec<u8>>> {
    tiny_http::Response::from_string("")
        .with_status_code(302)
        .with_header(tiny_http::Header::from_bytes(&b"Location"[..], location.as_bytes()).unwrap())
}

/// Token-controle + pagina, daarna route().
#[allow(clippy::too_many_arguments)]
fn dispatch(
    state: &AppState,
    log_path: &PathBuf,
    scope: ApiScope,
    token: Option<&str>,
    header_token: Option<&str>,
    method: tiny_http::Method,
    url: &str,
    body: &str,
) -> tiny_http::Response<Cursor<Vec<u8>>> {
    let mut url_owned = url.to_string();
    if let Some(tok) = token {
        match strip_token_prefix(url, tok) {
            Some(rest) if rest.is_empty() || rest.starts_with('?') => {
                // Relatieve fetches op de pagina vereisen een afsluitende slash.
                return redirect(format!("/r/{}/", tok));
            }
            Some(rest) => url_owned = rest,
            None => {
                if header_token.map(|h| token_eq(h, tok)) != Some(true) {
                    return json_response(401, json!({ "error": "token ontbreekt of onjuist" }));
                }
            }
        }
    }
    let path_only = url_owned.split('?').next().unwrap_or("");
    if scope == ApiScope::Remote && method == tiny_http::Method::Get && (path_only == "/" || path_only == "/index.html") {
        return html_response(REMOTE_HTML);
    }
    route(state, log_path, scope, method, &url_owned, body)
}

/// Whitelist van de afstandsbediening — ook beschikbaar op de test-API.
/// Retourneert None als de route hier niet bestaat.
fn route_shared(
    state: &AppState,
    scope: ApiScope,
    method: &tiny_http::Method,
    path: &str,
    query: &str,
) -> Option<Result<Value, (u16, String)>> {
    let r = match (method, path) {
        (tiny_http::Method::Get, "/status") => Ok(handle_status(state)),
        (tiny_http::Method::Get, "/organ") => Ok(handle_organ(state, scope)),
        (tiny_http::Method::Get, "/version") => Ok(json!({ "version": env!("CARGO_PKG_VERSION") })),
        (tiny_http::Method::Get, "/state") => Ok(handle_remote_state(state, scope)),
        // Testhaak (0.7.43): zet de watchdog-vlag restart_needed op de lopende
        // player, zodat het audio-noodherstel (main.rs) binnen ~3 s afgaat —
        // het testorgel-scenario "ASIO-stream dood, voorkeur ASIO" is zo zonder
        // hardware na te spelen (eerst de driver vrijgeven via een WASAPI-wissel).
        (tiny_http::Method::Post, "/debug/restart_needed") => {
            // Zelfde stappen als prepare_for_update: stream stoppen (callbacks
            // vallen echt stil, anders wuift het vals-alarm-filter van het
            // noodherstel de vlag weg) en dan de vlag zetten.
            let gezet = state.audio_player.read().as_ref().map(|p| {
                p.shutdown_and_wait(std::time::Duration::from_millis(1500));
                p.restart_needed.store(true, std::sync::atomic::Ordering::Relaxed);
                true
            }).unwrap_or(false);
            info!("test_api: restart_needed-vlag gezet ({})", gezet);
            Ok(json!({"ok": gezet}))
        }
        (tiny_http::Method::Post, "/panic") => {
            state.send_audio_command(AudioCommand::AllNotesOff);
            // Spooktoetsen mee opruimen (zie stop_audio in commands.rs).
            state.held_notes.write().clear();
            Ok(json!({"ok": true}))
        }
        (tiny_http::Method::Post, "/master") => handle_master(state, query),
        (tiny_http::Method::Post, p) if p.starts_with("/stops/") && p.ends_with("/toggle") => {
            let stop_id = url_decode(&p["/stops/".len()..p.len() - "/toggle".len()]);
            crate::commands::toggle_stop_inner(state, &stop_id)
                .map(|active| json!({ "stop_id": stop_id, "active": active }))
                .map_err(|e| (404u16, e))
        }
        (tiny_http::Method::Post, p) if p.starts_with("/couplers/") && p.ends_with("/toggle") => {
            let coupler_id = url_decode(&p["/couplers/".len()..p.len() - "/toggle".len()]);
            crate::commands::toggle_coupler_inner(state, &coupler_id)
                .map(|active| json!({ "coupler_id": coupler_id, "active": active }))
                .map_err(|e| (404u16, e))
        }
        (tiny_http::Method::Post, p) if p.starts_with("/action/") => handle_action(state, &p["/action/".len()..]),
        _ => return None,
    };
    Some(r)
}

fn route(
    state: &AppState,
    log_path: &PathBuf,
    scope: ApiScope,
    method: tiny_http::Method,
    url: &str,
    body: &str,
) -> tiny_http::Response<Cursor<Vec<u8>>> {
    // Path + query splitsen
    let (path, query) = match url.find('?') {
        Some(i) => (&url[..i], &url[i + 1..]),
        None => (url, ""),
    };

    let result: Result<Value, (u16, String)> = match route_shared(state, scope, &method, path, query) {
        Some(r) => r,
        None if scope == ApiScope::Remote => Err((404, "Geen route (afstandsbediening)".to_string())),
        None => route_test_only(state, log_path, method, path, query, body),
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

/// Alle overige routes: alleen op de test-API (127.0.0.1, geen token).
fn route_test_only(
    state: &AppState,
    log_path: &PathBuf,
    method: tiny_http::Method,
    path: &str,
    query: &str,
    body: &str,
) -> Result<Value, (u16, String)> {
    let method_name = method_str(&method);
    match (method, path) {
        (tiny_http::Method::Get, "/stops/drawn") => Ok(handle_drawn_stops(state)),
        // Setzer-oproep / General Cancel: hele registratie in één keer zetten
        // (zelfde kern als het Tauri-commando set_drawn_stops). Body {"ids":[..]}.
        (tiny_http::Method::Post, "/stops/set") => {
            let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON: {}", e)))?;
            let ids: Vec<String> = v.get("ids").and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or_default();
            crate::commands::set_drawn_stops_inner(state, ids).map_err(|e| (409u16, e))?;
            Ok(handle_drawn_stops(state))
        }
        // Koppels in één keer zetten (tweede helft van een setzer-oproep). Body {"ids":[..]}.
        (tiny_http::Method::Post, "/couplers/set") => {
            let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON: {}", e)))?;
            let ids: Vec<String> = v.get("ids").and_then(|x| serde_json::from_value(x.clone()).ok()).unwrap_or_default();
            crate::commands::set_active_couplers_inner(state, ids).map_err(|e| (409u16, e))?;
            Ok(json!({ "ok": true, "active_couplers": state.get_active_couplers() }))
        }
        (tiny_http::Method::Get, "/logs") => Ok(handle_logs(log_path, query)),
        (tiny_http::Method::Get, "/division_volumes") => Ok(handle_division_volumes(state)),
        (tiny_http::Method::Get, "/library") => Ok(handle_library(state)),
        // Afstandsbediening beheren (zelftest zonder UI). Strikt Test-only: via de
        // remote-server zelf zou /remote/enable zijn eigen serve-thread joinen.
        (tiny_http::Method::Get, "/remote/status") => {
            serde_json::to_value(crate::remote::status_inner(state)).map_err(|e| (500u16, e.to_string()))
        }
        (tiny_http::Method::Post, "/remote/enable") => {
            let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?;
            let enabled = v.get("enabled").and_then(|x| x.as_bool()).ok_or((400u16, "Veld 'enabled' ontbreekt".to_string()))?;
            let port = v.get("port").and_then(|x| x.as_u64()).map(|p| p as u16);
            let dto = crate::remote::set_enabled_inner(state, enabled, port).map_err(|e| (500u16, e))?;
            serde_json::to_value(dto).map_err(|e| (500u16, e.to_string()))
        }
        // Indeling van de afstandsbediening (0.7.39). Strikt Test-only: een
        // tablet mag de indeling van het hoofdscherm NIET herschrijven —
        // route_shared (de whitelist van de remote-poort) kent deze routes niet.
        (tiny_http::Method::Get, "/remote/layout") => {
            let layout = state.remote_layout.read().clone();
            Ok(json!({
                "layout": layout,
                "rev": state.remote_layout_revision(),
            }))
        }
        (tiny_http::Method::Post, "/remote/layout") => {
            let layout: crate::library::RemoteLayoutSaved = serde_json::from_str(body)
                .map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?;
            crate::commands::set_remote_layout_inner(state, layout);
            Ok(json!({ "ok": true, "rev": state.remote_layout_revision() }))
        }
        (tiny_http::Method::Post, "/remote/new_token") => {
            let dto = crate::remote::new_token_inner(state).map_err(|e| (500u16, e))?;
            serde_json::to_value(dto).map_err(|e| (500u16, e.to_string()))
        }
        (tiny_http::Method::Post, "/load_organ") => handle_load_organ(state, body),
        (tiny_http::Method::Post, "/load_directory") => handle_load_directory(state, body),
        (tiny_http::Method::Post, "/audio_output") => handle_audio_output(state, body),
        (tiny_http::Method::Post, "/reverb") => handle_reverb(state, body),
        (tiny_http::Method::Post, "/sampleset/scan") => handle_sampleset_scan(body),
        (tiny_http::Method::Post, "/sampleset/trim") => handle_sampleset_trim(body),
        (tiny_http::Method::Post, "/sampleset/loop_scan") => handle_sampleset_loop_scan(body),
        (tiny_http::Method::Post, "/sampleset/loop_apply") => handle_sampleset_loop_apply(body),
        (tiny_http::Method::Get, "/division_channels") => Ok(handle_division_channels(state)),
        (tiny_http::Method::Post, "/division_channels") => handle_set_division_channels(state, body),
        (tiny_http::Method::Post, "/polyphony") => {
            let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
            let n = v.get("voices").and_then(|x| x.as_u64()).unwrap_or(1024) as usize;
            crate::audio::set_polyphony_target(n);
            Ok(json!({"polyphony": crate::audio::polyphony_target()}))
        }
        (tiny_http::Method::Post, "/stereo_samples") => {
            let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
            let on = v.get("on").and_then(|x| x.as_bool()).unwrap_or(true);
            vpo_sampler::set_stereo_loading(on);
            Ok(json!({"stereo_samples": vpo_sampler::stereo_loading()}))
        }
        (tiny_http::Method::Post, "/record/start") => handle_record_start(state, body),
        (tiny_http::Method::Post, "/record/stop") => handle_record_stop(state),
        // MIDI-opname (events) — los van de MP3-opname hierboven. Voor self-testing van
        // de capture-keten: injecteer MIDI door de échte BLE-loop heen.
        (tiny_http::Method::Post, "/midi/record/start") => Ok(handle_midi_record_start(state)),
        (tiny_http::Method::Get, "/midi/record/status") => Ok(handle_midi_record_status(state)),
        (tiny_http::Method::Post, "/midi/record/stop") => Ok(handle_midi_record_stop(state)),
        (tiny_http::Method::Post, "/midi/record/save") => handle_midi_record_save(state, body),
        (tiny_http::Method::Post, "/midi/inject") => handle_midi_inject(state, query),
        (tiny_http::Method::Post, "/midi/mapping") => handle_midi_mapping(state, body),
        (tiny_http::Method::Get, "/held_notes") => Ok(handle_held_notes(state)),
        // Generaal crescendo + zwelkast (meetplan zwel/crescendo)
        (tiny_http::Method::Get, "/crescendo") => Ok(handle_crescendo_get(state)),
        (tiny_http::Method::Post, "/crescendo/config") => handle_crescendo_config(state, body),
        (tiny_http::Method::Post, "/crescendo/binding") => handle_crescendo_binding(state, body),
        (tiny_http::Method::Post, "/crescendo/stage") => handle_crescendo_stage(state, query),
        (tiny_http::Method::Get, "/swell") => Ok(handle_swell_get(state)),
        (tiny_http::Method::Post, "/swell/binding") => handle_swell_binding(state, body),
        // Luidspreker-testsignaal (0.7.47): ?channel=<n>&kind=0|1, zonder
        // channel gaat hij uit. Zo is te meten dat er geluid uit precies
        // een kanaal komt (peak_left/peak_right in /status).
        (tiny_http::Method::Post, "/audio/test_signal") => Ok(handle_test_signal(query)),
        // Mengloop in N stukken verdelen (fase 2): ?n=1..8. 1 = het oude,
        // bit-identieke gedrag.
        (tiny_http::Method::Post, "/audio/mix_chunks") => Ok(handle_mix_chunks(query)),
        // Automatisch MIDI-archief (0.7.38)
        (tiny_http::Method::Get, "/midi/archive/status") => Ok(handle_midi_archive_status(state)),
        (tiny_http::Method::Post, "/midi/archive/config") => handle_midi_archive_config(state, query),
        (tiny_http::Method::Post, "/midi/archive/flush") => Ok(handle_midi_archive_flush(state)),
        (tiny_http::Method::Get, "/midi/archive/list") => Ok(handle_midi_archive_list(state)),
        // MIDI-speler (afspelen van .mid-bestanden, o.a. archiefbestanden)
        (tiny_http::Method::Post, "/midi/player/play") => handle_midi_player_play(state, body),
        (tiny_http::Method::Post, "/midi/player/stop") => Ok(handle_midi_player_stop(state)),
        (tiny_http::Method::Get, "/midi/player/status") => Ok(handle_midi_player_status(state)),
        // Per-orgel instellingen (self-testing van de persistentie-migratie, Onderdeel 1).
        (tiny_http::Method::Post, "/settings/master") => handle_set_master(state, query),
        (tiny_http::Method::Post, "/settings/temperament") => handle_set_temperament(state, query),
        // Hertemperen op gemeten pijptoonhoogte (0.7.38): JSON-variant, status en windmodel.
        (tiny_http::Method::Post, "/temperament") => handle_set_temperament_json(state, body),
        (tiny_http::Method::Get, "/tuning") => Ok(handle_tuning(state)),
        (tiny_http::Method::Post, "/settings/wind") => handle_set_wind(state, query),
        (tiny_http::Method::Get, "/wind") => handle_wind_status(),
        (tiny_http::Method::Post, "/settings/eq") => handle_set_eq(state, query),
        (tiny_http::Method::Post, "/settings/reverb") => handle_set_reverb(state, query),
        (tiny_http::Method::Post, "/settings/pan") => handle_set_pan(state, query),
        (tiny_http::Method::Post, "/settings/trem") => handle_set_trem(state, query),
        // Tremulant live schakelen zonder UI (het Orgel-tabblad hoeft niet open
        // te staan, anders dan bij POST /action/{24+i}).
        (tiny_http::Method::Post, "/tremulant") => handle_tremulant(state, query),
        (tiny_http::Method::Post, "/settings/save") => Ok(handle_settings_save(state)),
        (tiny_http::Method::Get, "/settings/organ") => Ok(handle_settings_organ(state)),
        (tiny_http::Method::Get, "/settings/mirror") => Ok(handle_settings_mirror(state)),
        // Gestapelde ranks en microfoonperspectieven (0.7.38)
        (tiny_http::Method::Get, "/perspectives") => Ok(handle_perspectives(state)),
        (tiny_http::Method::Post, "/perspectives") => handle_set_perspective(state, body),
        (tiny_http::Method::Get, "/ranks") => Ok(handle_ranks(state)),
        (tiny_http::Method::Post, p) if p.starts_with("/notes/") && p.ends_with("/on") => {
            let note_str = &p["/notes/".len()..p.len() - "/on".len()];
            handle_note_on(state, note_str, query)
        }
        (tiny_http::Method::Post, p) if p.starts_with("/notes/") && p.ends_with("/off") => {
            let note_str = &p["/notes/".len()..p.len() - "/off".len()];
            handle_note_off(state, note_str, query)
        }
        _ => Err((404, format!("Geen route voor {} {}", method_name, path))),
    }
}

/// Toegestane actiecodes voor POST /action/{code} op de afstandsbediening:
/// 0-9 cijfers, 10 SET, 11 GC, 12 −1, 13 +1, 14 −10, 15 +10, 16-23 M1-M8,
/// 24+i tremulant van divisie i (alleen bestaande divisies), 40 EQ aan/uit,
/// 41 crescendo aan/uit, 43 profielwissel. 42 (computer afsluiten) en ≥44 nooit.
fn allowed_action_code(code: u8, n_divisions: usize) -> bool {
    match code {
        0..=23 => true,
        24..=39 => ((code - 24) as usize) < n_divisions,
        40 | 41 | 43 => true,
        _ => false,
    }
}

/// POST /action/{code}: als een MIDI-piston via het preset-trigger-kanaal
/// (SetzerBar leegt het elke 100 ms in het hoofdvenster).
fn handle_action(state: &AppState, code_str: &str) -> Result<Value, (u16, String)> {
    let code: u8 = code_str.parse().map_err(|_| (400u16, format!("Ongeldige actiecode: {}", code_str)))?;
    let n_div = state.loaded_organ_info.read().as_ref().map(|o| o.divisions.len()).unwrap_or(0);
    if !allowed_action_code(code, n_div) {
        return Err((403u16, "actie niet toegestaan via de afstandsbediening".to_string()));
    }
    // Zonder consument (Orgel-tabblad niet open) blijft de actie in de wachtrij
    // hangen en gooit SetzerBar hem bij het mounten weg — daarom 409.
    let stale = state.preset_poll_seen.read()
        .map(|t| t.elapsed() > std::time::Duration::from_millis(1500))
        .unwrap_or(true);
    if stale {
        return Err((409u16, "orgelscherm niet actief — open het Orgel-tabblad in JM-Orgue".to_string()));
    }
    state.preset_trigger_tx.try_send(code)
        .map_err(|_| (503u16, "piston-wachtrij vol — staat het orgelscherm open?".to_string()))?;
    info!("remote: actie {}", code);
    Ok(json!({ "code": code, "queued": true }))
}

/// POST /master?db=<f32> — geclamped op het sliderbereik (−40..6 dB) en het
/// hoofdvenster krijgt een event zodat zijn slider meespringt.
fn handle_master(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let db: f32 = parse_query(query, "db").ok_or((400u16, "Parameter 'db' ontbreekt".to_string()))?;
    if !db.is_finite() {
        return Err((400u16, "Parameter 'db' is geen getal".to_string()));
    }
    let db = db.clamp(-40.0, 6.0);
    crate::commands::set_master_volume_inner(state, db);
    if let Some(h) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = h.emit("jm-orgue:remote-master-volume", json!({ "db": db }));
    }
    Ok(json!({ "db": db }))
}

/// GET /state — compacte toestand voor de afstandsbediening (2×/s per client).
/// Lock-volgorde: audio_player apart en direct los; loaded_organ_info alleen
/// voor id/naam/divisienamen en dan los; drawn_stops en active_couplers elk
/// apart als clone (nooit genest — de MIDI-thread nest ze zelf, audit 44).
fn handle_remote_state(state: &AppState, scope: ApiScope) -> Value {
    let (audio_running, voices) = {
        let g = state.audio_player.read();
        match g.as_ref() {
            Some(p) => (true, p.voice_count()),
            None => (false, 0),
        }
    };
    // Op de afstandsbediening geen bestandspad als id (index.html vergelijkt
    // organ_id alleen met het id uit GET /organ → dezelfde korte hash).
    let organ: Option<(String, String, Vec<String>)> = state.loaded_organ_info.read().as_ref()
        .map(|o| (
            if scope == ApiScope::Remote { short_id(&o.id) } else { o.id.clone() },
            o.name.clone(),
            o.divisions.iter().map(|d| d.name.clone()).collect(),
        ));
    let drawn: Vec<String> = state.drawn_stops.read().clone();
    let couplers: Vec<String> = state.active_couplers.read().clone();
    let tremulant: Vec<bool> = {
        let live = state.tremulant_live.read();
        organ.as_ref().map(|(_, _, divs)| divs.iter().map(|d| live.get(d).copied().unwrap_or(false)).collect()).unwrap_or_default()
    };
    let master_db = state.master_volume_db.read().unwrap_or(-6.0);
    let setzer = state.setzer_mirror.read().clone();
    let cresc_enabled = *state.crescendo_enabled.read();
    let cresc_stage = *state.crescendo_stage.read();
    let cresc_stages = state.crescendo_stages.read().len();
    json!({
        "organ_loaded": organ.is_some(),
        "organ_id": organ.as_ref().map(|o| o.0.clone()),
        "organ_name": organ.as_ref().map(|o| o.1.clone()),
        "drawn": drawn,
        "couplers": couplers,
        "tremulant": tremulant,
        "master_db": master_db,
        "voices": voices,
        "audio_running": audio_running,
        "setzer": { "level": setzer.level, "preset": setzer.preset, "set_mode": setzer.set_mode, "data": setzer.data },
        "crescendo": { "enabled": cresc_enabled, "stage": cresc_stage, "stages": cresc_stages },
        // Revisie van de indeling: wijzigt hij, dan haalt de remote-pagina
        // /organ opnieuw op (ook zonder orgelwissel).
        "layout_rev": state.remote_layout_revision(),
    })
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
    let (sample_rate, voice_count, peaks, channels, audio_host, audio_device, buffer_frames) = match audio.as_ref() {
        Some(p) => (*p.sample_rate.read(), p.voice_count(), p.peak_meters(),
                    *p.current_channels.read(), p.current_host.read().clone(),
                    p.current_device.read().clone(), *p.current_buffer_frames.read()),
        None => (0, 0, (0.0, 0.0), 0u16, String::new(), String::new(), 0u32),
    };
    let organ = state.loaded_organ_info.read();
    let drawn = state.drawn_stops.read();

    json!({
        "audio_running": audio.is_some(),
        "sample_rate": sample_rate,
        "channels": channels,
        "audio_host": audio_host,
        "audio_device": audio_device,
        "buffer_frames": buffer_frames,
        "voice_count": voice_count,
        "polyphony": crate::audio::polyphony_target(),
        "render_load": crate::audio::render_load().0,
        "render_peak": crate::audio::render_load().1,
        "render_overloads": crate::audio::render_overload_count(),
        "rt_drops": crate::state::rt_drop_count(),
        "stereo_samples": vpo_sampler::stereo_loading(),
        "peak_left": peaks.0,
        "peak_right": peaks.1,
        "organ_loaded": organ.is_some(),
        "organ_name": organ.as_ref().map(|o| o.name.clone()),
        "drawn_stop_count": drawn.len(),
        "midi_connected": *state.midi_connected.read(),
        "midi_archiving": state.midi_archive.archiving.load(std::sync::atomic::Ordering::Relaxed),
        "audio_ready": state.audio_ready.load(std::sync::atomic::Ordering::Relaxed),
        "asio_restart_advice": state.asio_restart_advice.read().as_ref().map(|a| a.device.clone()),
        "backend_reloads": state.backend_reloads.load(std::sync::atomic::Ordering::Relaxed),
        "render_frames": crate::audio::render_frames_now(),
        "layered_stops": state.rank_summary.read().iter().filter(|r| r.is_stacked()).count(),
        // Mengloop-meting (fase 1 van het meerkernige plan): rendertijd per
        // pas als fractie van de buffertijd, plus het aantal stukken.
        "mix_pass_load": {
            "wind_trem": crate::audio::render_pass_load().0,
            "voices": crate::audio::render_pass_load().1,
            "chain": crate::audio::render_pass_load().2,
            "reduce": crate::audio::render_pass_load().3,
        },
        "mix_chunks": crate::audio::meng_stukken(),
        "release_pipes": state.loaded_organ_info.read().as_ref().map(|o| o.release_pipes).unwrap_or(0),
    })
}

/// Microfoonperspectieven van het geladen orgel (runtime-staat).
fn handle_perspectives(state: &AppState) -> Value {
    let list: Vec<Value> = state.perspectives.read().iter().map(|p| json!({
        "name": p.name,
        "enabled": p.enabled,
        "gain_db": p.gain_db,
        "loaded": p.loaded,
        "slot": p.slot,
        "pipe_count": p.pipe_count,
    })).collect();
    Value::Array(list)
}

/// Body {"name":..., "enabled":bool?, "gain_db":f32?} — dezelfde kern als de
/// Tauri-commando's; antwoord {"reload_needed":bool}.
fn handle_set_perspective(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?;
    let name = v.get("name").and_then(|x| x.as_str())
        .ok_or((400u16, "Missing 'name' in body".to_string()))?;
    let mut reload_needed = state.perspectives.read().iter().any(|p| p.enabled != p.loaded);
    if let Some(en) = v.get("enabled").and_then(|x| x.as_bool()) {
        reload_needed = crate::commands::do_set_perspective_enabled(state, name, en)
            .map_err(|e| (404u16, e))?;
    }
    if let Some(g) = v.get("gain_db").and_then(|x| x.as_f64()) {
        crate::commands::do_set_perspective_gain(state, name, g as f32)
            .map_err(|e| (404u16, e))?;
    }
    Ok(json!({ "ok": true, "reload_needed": reload_needed }))
}

/// Lagen per (niet-ruis-)register — diagnose van gestapelde ranks/perspectieven.
fn handle_ranks(state: &AppState) -> Value {
    let rs = state.rank_summary.read();
    let stops: Vec<Value> = rs.iter().map(|r| json!({
        "internal_stop_id": r.stop_id,
        "dto_id": r.dto_id,
        "name": r.name,
        "layers": r.layers,
    })).collect();
    json!({
        "stops": stops,
        "stacked_stops": rs.iter().filter(|r| r.is_stacked()).count(),
    })
}

/// Orgel-DTO met actuele drawn-/koppelvlaggen (zelfde bron als het Tauri-
/// commando get_organ_info); `null` zolang er geen orgel geladen is. Op de
/// afstandsbediening een uitgeklede variant: geen bestandspaden (id → korte
/// stabiele hash), geen interne stop-ids/nootbereiken/perspectieven — alleen
/// wat index.html rendert.
fn handle_organ(state: &AppState, scope: ApiScope) -> Value {
    match crate::commands::organ_info_merged(state) {
        Some(o) if scope == ApiScope::Remote => remote_organ_dto(state, &o),
        Some(o) => serde_json::to_value(o).unwrap_or(json!(null)),
        None => json!(null),
    }
}

/// Korte, stabiele (FNV-1a 64) hash van een orgel-id voor de afstandsbediening.
fn short_id(id: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:016x}", h)
}

/// Registernaam zonder het voettal dat er al achter staat — dezelfde
/// opschoning als cleanStopName op het orgelscherm (Console.svelte), zodat het
/// externe scherm "Prestant" + "8'" toont en niet "Prestant 8" + "8'".
fn strip_pitch_suffix(name: &str, pitch: &str) -> String {
    if pitch.is_empty() {
        return name.to_string();
    }
    if let Some(rest) = name.trim_end().strip_suffix(pitch) {
        let rest = rest.trim_end();
        if !rest.is_empty() {
            return rest.to_string();
        }
    }
    name.to_string()
}

/// Uitgeklede orgel-DTO voor de afstandsbediening (zie handle_organ). Volgt de
/// indeling die het hoofdvenster publiceerde (set_remote_layout).
/// Lock-volgorde: organ_info_merged gaf al een clone terug; remote_layout en
/// division_tremulants worden daarna apart gelezen, nooit genest (audit 44).
fn remote_organ_dto(state: &AppState, o: &crate::commands::OrganInfoDto) -> Value {
    let layout = state.remote_layout.read().clone();
    let trems = state.division_tremulants.read().clone();
    let rev = state.remote_layout_revision();
    remote_organ_dto_inner(o, layout.as_ref(), &trems, rev)
}

/// Kern van remote_organ_dto zonder locks (testbaar).
/// - divisies: volgorde + selectie uit de indeling; onbekende namen vervallen
///   en een lege uitkomst (bv. indeling van een ánder orgel) toont alles;
/// - `div_index` is de index in de orgel-DTO, zodat actiecode 24+i (tremulant)
///   blijft kloppen ook als de remote een andere volgorde toont;
/// - registers: de gesleepte volgorde van het orgelscherm, onbekende achteraan;
/// - `show_tremulant` per divisie = hetzelfde criterium als het orgelscherm;
/// - koppels: alléén de koppels die ook op het orgelscherm zichtbaar zijn.
fn remote_organ_dto_inner(
    o: &crate::commands::OrganInfoDto,
    layout: Option<&crate::library::RemoteLayoutSaved>,
    trems: &std::collections::HashMap<String, (bool, f32, f32, f32)>,
    rev: u64,
) -> Value {
    let order: Vec<usize> = {
        let sel: Vec<usize> = layout
            .map(|l| {
                l.divisions.iter()
                    .filter_map(|n| o.divisions.iter().position(|d| &d.name == n))
                    .collect()
            })
            .unwrap_or_default();
        if sel.is_empty() { (0..o.divisions.len()).collect() } else { sel }
    };
    let divisions: Vec<Value> = order.iter().filter_map(|&di| {
        let d = o.divisions.get(di)?;
        let mut ordered: Vec<&crate::commands::StopDto> = Vec::with_capacity(d.stops.len());
        if let Some(ids) = layout.and_then(|l| l.stop_order.get(&d.name)) {
            for id in ids {
                if let Some(s) = d.stops.iter().find(|s| &s.id == id) {
                    if !ordered.iter().any(|x| x.id == s.id) { ordered.push(s); }
                }
            }
        }
        for s in &d.stops {
            if !ordered.iter().any(|x| x.id == s.id) { ordered.push(s); }
        }
        // Orgelscherm: stops.some(has_tremulant) || tremLfoEnabled[divisie],
        // met als default van tremLfoEnabled: has_tremulant én geen
        // golfvormtremulant (die heeft echte opnamen, geen synth-LFO).
        let show_trem = d.stops.iter().any(|s| s.has_tremulant)
            || trems.get(&d.name).map(|t| t.0)
                .unwrap_or(d.has_tremulant && d.tremulant_kind.as_deref() != Some("wave"));
        Some(json!({
            "name": d.name,
            "div_index": di,
            "show_tremulant": show_trem,
            "stops": ordered.iter().map(|s| json!({
                "id": s.id,
                "name": strip_pitch_suffix(&s.name, &s.pitch),
                "pitch": s.pitch,
                "color": s.color,
                "drawn": s.drawn,
            })).collect::<Vec<_>>(),
        }))
    }).collect();
    let couplers: Vec<Value> = o.couplers.as_ref().map(|cl| cl.iter()
        .filter(|c| match layout {
            Some(l) => l.visible_couplers.iter().any(|id| id == &c.id),
            // Nog niets gepubliceerd: dezelfde default als het orgelscherm —
            // echte ODF/JM-Rec-koppels en de gegenereerde unison-koppels.
            None => c.id.starts_with("real_coupler_") || c.coupler_type == "unison",
        })
        .map(|c| json!({
            "id": c.id,
            "name": c.name,
            "active": c.active,
            "display_in_division": c.display_in_division,
            "coupler_type": c.coupler_type,
        })).collect()).unwrap_or_default();
    let (placement, shape, show_couplers, show_trem, show_setzer, show_volume, show_panic) = match layout {
        Some(l) => (
            if l.coupler_placement == "division" { "division" } else { "bar" },
            if l.knob_shape == "round" { "round" } else { "rect" },
            l.show_couplers, l.show_tremulant, l.show_setzer, l.show_volume, l.show_panic,
        ),
        None => ("bar", "rect", true, true, true, true, true),
    };
    json!({
        "id": short_id(&o.id),
        "name": o.name,
        "divisions": divisions,
        "couplers": couplers,
        "layout": {
            "coupler_placement": placement,
            "knob_shape": shape,
            "show_couplers": show_couplers,
            "show_tremulant": show_trem,
            "show_setzer": show_setzer,
            "show_volume": show_volume,
            "show_panic": show_panic,
            "rev": rev,
        },
    })
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
        "image_path": o.image_path,
        "image_manual": o.image_manual,
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
    let sample_rate = v.get("sample_rate").and_then(|x| x.as_u64()).map(|n| n as u32);

    info!("test_api: audio_output switch host={:?} device={:?} buffer={:?} rate={:?}",
        host, device, buffer_frames, sample_rate);
    let outcome = state.switch_audio_output(crate::audio::AudioOutputConfig {
        host_name: host,
        device_name: device,
        buffer_frames,
        sample_rate,
    }).map_err(|e| (500u16, e))?;

    // Mimic the frontend: reload the current organ so its samples re-register
    // on the freshly created audio thread — ook wanneer de wissel zelf faalde
    // maar er wél een nieuwe (herstelde/fallback-)audio-thread is.
    let mut reloaded = None;
    if outcome.player_rebuilt {
        if let Some(id) = outcome.organ_id.clone() {
            if crate::commands::is_organ_file_id(&id) {
                crate::commands::do_load_organ(state, &id).map_err(|e| (500u16, e))?;
            } else {
                crate::commands::do_load_samples_from_directory(state, &id).map_err(|e| (500u16, e))?;
            }
            reloaded = Some(id);
            // Zelfde als de backend-paden in main.rs (ASIO-wissel/noodherstel):
            // de verse audio-thread staat anders op alle DSP-defaults
            // (temperament/retune, master, galm, EQ …) terwijl de mirror de
            // oude waarden toont.
            crate::commands::apply_dsp_after_backend_reload(state);
        }
    }

    let cur_buf = state.audio_player.read().as_ref()
        .map(|p| *p.current_buffer_frames.read())
        .unwrap_or_default();
    Ok(json!({
        "ok": outcome.switched,
        "player_rebuilt": outcome.player_rebuilt,
        "message": outcome.message,
        "reloaded_organ": reloaded,
        "host": outcome.actual_host,
        "device": outcome.actual_device,
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

/// Kanaalaantal van de lopende stream + per divisie de gekozen uitgangen.
/// Lock-volgorde: eerst loaded_organ_info, dan division_output_channels
/// (audit 44); audio_player apart en direct weer los.
fn handle_division_channels(state: &AppState) -> Value {
    let channels = {
        let g = state.audio_player.read();
        g.as_ref().map(|p| *p.current_channels.read()).unwrap_or(0)
    };
    let organ = state.loaded_organ_info.read();
    let chans = state.division_output_channels.read();
    let divisions: Vec<Value> = organ.as_ref().map(|o| o.divisions.iter().enumerate().map(|(i, d)| json!({
        "name": d.name,
        "channels": chans.get(i).cloned().unwrap_or_default(),
    })).collect()).unwrap_or_default();
    json!({ "channels": channels, "divisions": divisions })
}

fn handle_set_division_channels(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body)
        .map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?;
    let division = v.get("division").and_then(|d| d.as_str())
        .ok_or((400u16, "Missing 'division' in body".to_string()))?;
    let channels: Vec<u16> = v.get("channels").and_then(|c| c.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_u64()).map(|x| x as u16).collect())
        .unwrap_or_default();
    crate::commands::set_division_output_channels_inner(state, division, channels.clone())
        .map_err(|e| (500u16, e))?;
    Ok(json!({ "ok": true, "division": division, "channels": channels }))
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
    // Toonhoogtemeting (hertemperen): FFT-piek van de laatste ~2,7 s.
    let mut peak_hz = 0.0f64;
    if let Some(ref p) = s.path {
        // samples zijn INTERLEAVED (ch kanalen), rate = samplerate van de mp3.
        if let Ok((samples, ch, rate)) = crate::silence::decode_mp3_interleaved(std::path::Path::new(p)) {
            decoded_samples = samples.len();
            if !samples.is_empty() {
                let sumsq: f64 = samples.iter().map(|&x| (x as f64) * (x as f64)).sum();
                rms = (sumsq / samples.len() as f64).sqrt();
                peak_hz = crate::silence::dominant_peak_hz(&samples, ch as usize, rate);
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
        "peak_hz": peak_hz,
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
        stopped_elapsed: None,
    });
    json!({ "ok": true })
}

fn handle_midi_record_status(state: &AppState) -> Value {
    let rec = state.midi_recording.read();
    match rec.as_ref() {
        Some(r) => json!({
            "recording": r.active,
            "event_count": r.events.len(),
            // Na stop: bevroren duur i.p.v. eeuwig doorlopende wandkloktijd.
            "seconds": r.stopped_elapsed.unwrap_or_else(|| r.start_time.elapsed().as_secs_f64()),
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
            if r.active {
                r.active = false;
                r.stopped_elapsed = Some(r.start_time.elapsed().as_secs_f64());
            }
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
/// Of een ControlChange: `?cc=N&value=V&channel=C` — doorloopt dezelfde lus
/// (coalescing, process_crescendo_cc, zwelkast), zodat zwel en crescendo
/// meetbaar zijn zonder fysieke trede.
fn handle_midi_inject(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    if let Some(cc) = parse_query::<u8>(query, "cc") {
        let value: u8 = parse_query(query, "value").unwrap_or(0).min(127);
        let channel: u8 = parse_query(query, "channel").unwrap_or(0).min(15);
        let msg = MidiMessage::ControlChange { channel, controller: cc.min(127), value };
        state.ble_message_tx.send(msg)
            .map_err(|e| (500u16, format!("Injectie mislukt: {}", e)))?;
        return Ok(json!({ "ok": true, "cc": cc.min(127), "value": value, "channel": channel }));
    }
    let note: u8 = parse_query(query, "note")
        .ok_or((400u16, "Parameter 'note' (of 'cc') ontbreekt".to_string()))?;
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

/// Body {"division": "...", "channel": int|null, "first"?: int, "last"?: int,
/// "transpose"?: int} — vervangt de MIDI-klavier-mapping van die divisie
/// (geïnjecteerde noten vereisen een mapping om in held_notes te komen).
fn handle_midi_mapping(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    use crate::state::MidiChannelMapping;
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON: {}", e)))?;
    let division = v.get("division").and_then(|x| x.as_str())
        .ok_or((400u16, "Veld 'division' ontbreekt".to_string()))?.to_string();
    let channel = v.get("channel").and_then(|x| x.as_u64()).map(|c| (c as u8).min(15));
    let first = v.get("first").and_then(|x| x.as_u64()).map(|n| (n as u8).min(127));
    let last = v.get("last").and_then(|x| x.as_u64()).map(|n| (n as u8).min(127));
    let transpose = v.get("transpose").and_then(|x| x.as_i64()).unwrap_or(0).clamp(-48, 48) as i8;
    let mut mappings = state.get_midi_mappings();
    mappings.retain(|m| m.division != division);
    mappings.push(MidiChannelMapping {
        division: division.clone(), channel, transpose,
        first_midi_note: first, last_midi_note: last,
        short_octave: false,
    });
    let n = mappings.len();
    state.set_midi_mappings(mappings);
    Ok(json!({ "ok": true, "division": division, "channel": channel, "mappings": n }))
}

/// Ingedrukte toetsen (kanaal, noot, velocity) — diagnose van hangers.
fn handle_held_notes(state: &AppState) -> Value {
    let held: Vec<Value> = state.held_notes.read().iter()
        .map(|&(ch, n, v)| json!([ch, n, v])).collect();
    json!({ "held": held })
}

// ============ Generaal crescendo + zwelkast (meetplan) ============

fn crescendo_binding_json(b: Option<(u8, u8, u8, u8, bool)>) -> Value {
    match b {
        Some((ch, cc, mn, mx, inv)) => json!({ "channel": ch, "cc": cc, "min": mn, "max": mx, "invert": inv }),
        None => Value::Null,
    }
}

/// GET /crescendo → {enabled, stage, stages: N, num_stages, stage_stops, binding, active_stops}.
fn handle_crescendo_get(state: &AppState) -> Value {
    let cfg = crate::commands::crescendo_config_dto(state);
    json!({
        "enabled": cfg.enabled,
        "stage": cfg.stage,
        "stages": cfg.stages.len(),
        "num_stages": cfg.num_stages,
        "stage_stops": cfg.stages,
        "binding": crescendo_binding_json(*state.crescendo_binding.read()),
        "active_stops": cfg.active_stops,
        "hysteresis": crate::state::CRESCENDO_HYSTERESIS,
    })
}

/// POST /crescendo/config {"stages": [[id..]..], "enabled": bool, "num_stages"?: n}
/// = commands::set_crescendo_config (past de huidige trap direct opnieuw toe).
fn handle_crescendo_config(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON: {}", e)))?;
    let stages: Vec<Vec<String>> = match v.get("stages") {
        Some(s) => serde_json::from_value(s.clone()).map_err(|e| (400u16, format!("Veld 'stages': {}", e)))?,
        None => state.crescendo_stages.read().clone(),
    };
    let enabled = v.get("enabled").and_then(|x| x.as_bool()).unwrap_or(*state.crescendo_enabled.read());
    let num_stages = v.get("num_stages").and_then(|x| x.as_u64()).map(|n| n.clamp(1, 64) as u8);
    crate::commands::set_crescendo_config_inner(state, stages, enabled, num_stages);
    Ok(handle_crescendo_get(state))
}

/// POST /crescendo/binding {"channel","cc","min"?,"max"?,"invert"?} | {"clear": true}.
/// Zet de trede op 0, wist zwelbindingen op dezelfde (kanaal, CC) via dezelfde
/// gedeelde helper als de UI-paden (claim_pedal_cc — laatst ingesteld wint) en
/// herstelt de master-expressie. Het antwoord bevat "displaced": wat er is
/// verdrongen.
fn handle_crescendo_binding(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON: {}", e)))?;
    if v.get("clear").and_then(|x| x.as_bool()).unwrap_or(false) {
        // Zelfde volgorde als het Tauri-commando (0.7.44): eerst trap 0, dan weg.
        let _ = state.apply_crescendo_stage(0);
        *state.crescendo_binding.write() = None;
        return Ok(handle_crescendo_get(state));
    }
    let channel = v.get("channel").and_then(|x| x.as_u64())
        .ok_or((400u16, "Veld 'channel' ontbreekt".to_string()))? as u8;
    let cc = v.get("cc").and_then(|x| x.as_u64())
        .ok_or((400u16, "Veld 'cc' ontbreekt".to_string()))? as u8;
    let (channel, cc) = (channel.min(15), cc.min(127));
    let existing = *state.crescendo_binding.read();
    // Bereik/spiegel alleen erven van dezelfde trede (kanaal, CC) — zoals
    // set_crescendo_binding_manual (0.7.44).
    let (mut mn, mut mx, mut inv) = match existing {
        Some((och, occ, mn, mx, inv)) if och == channel && occ == cc => (mn, mx, inv),
        _ => (0, 127, false),
    };
    if let Some(x) = v.get("min").and_then(|x| x.as_u64()) { mn = (x as u8).min(127); }
    if let Some(x) = v.get("max").and_then(|x| x.as_u64()) { mx = (x as u8).min(127); }
    if let Some(x) = v.get("invert").and_then(|x| x.as_bool()) { inv = x; }
    *state.crescendo_binding.write() = Some((channel, cc, mn, mx, inv));
    let verdrongen = state.claim_pedal_cc(crate::state::PedalCcKind::Crescendo, channel, cc);
    state.apply_crescendo_stage(0);
    state.send_audio_command(AudioCommand::SetMasterExpression(1.0));
    let mut out = handle_crescendo_get(state);
    if let Some(obj) = out.as_object_mut() {
        obj.insert("displaced".into(), serde_json::to_value(&verdrongen).unwrap_or(json!([])));
    }
    Ok(out)
}

/// POST /crescendo/stage?stage=N — trap zetten zoals een UI-klik.
fn handle_crescendo_stage(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let stage: u8 = parse_query(query, "stage").ok_or((400u16, "Parameter 'stage' ontbreekt".to_string()))?;
    crate::commands::set_crescendo_stage_inner(state, stage).map_err(|e| (409u16, e))?;
    Ok(handle_crescendo_get(state))
}

fn swell_binding_json(b: &crate::state::SwellBinding) -> Value {
    json!({
        "channel": b.channel, "cc": b.cc_num, "min": b.min_val, "max": b.max_val,
        "invert": b.invert, "last_value": b.last_value,
    })
}

/// GET /swell → per divisie {division, index, position (spiegel), binding|null, min_db, cutoff}.
fn handle_swell_get(state: &AppState) -> Value {
    let names: Vec<String> = state.loaded_organ_info.read().as_ref()
        .map(|o| o.divisions.iter().map(|d| d.name.clone()).collect()).unwrap_or_default();
    let gains = state.get_division_gains();
    let bindings = state.get_swell_bindings();
    let cfgs = state.division_swell_configs.read().clone();
    let list: Vec<Value> = names.iter().enumerate().map(|(i, name)| {
        let (min_db, cutoff) = cfgs.get(name).copied().unwrap_or((-20.0, 800.0));
        json!({
            "division": name,
            "index": i,
            "position": gains.get(i).copied().unwrap_or(1.0),
            "binding": bindings.iter().find(|b| &b.division_name == name).map(swell_binding_json).unwrap_or(Value::Null),
            "min_db": min_db,
            "cutoff": cutoff,
        })
    }).collect();
    Value::Array(list)
}

/// POST /swell/binding {"division","channel","cc","min"?,"max"?,"invert"?} | {"division","clear":true}.
fn handle_mix_chunks(query: &str) -> Value {
    let n: usize = parse_query(query, "n").unwrap_or(1);
    crate::audio::set_meng_stukken(n);
    json!({ "ok": true, "mix_chunks": crate::audio::meng_stukken() })
}

fn handle_test_signal(query: &str) -> Value {
    let kind: u8 = parse_query(query, "kind").unwrap_or(0);
    let level: f32 = parse_query(query, "level_db").unwrap_or(-12.0);
    match parse_query::<u8>(query, "channel") {
        Some(ch) => {
            crate::audio::set_test_signal(Some(ch), kind, 10f32.powf(level.clamp(-60.0, 0.0) / 20.0));
            json!({ "ok": true, "channel": ch, "kind": kind })
        }
        None => {
            crate::audio::set_test_signal(None, 0, 0.0);
            json!({ "ok": true, "channel": Value::Null })
        }
    }
}

fn handle_swell_binding(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON: {}", e)))?;
    let division = v.get("division").and_then(|x| x.as_str())
        .ok_or((400u16, "Veld 'division' ontbreekt".to_string()))?.to_string();
    let idx = state.loaded_organ_info.read().as_ref()
        .and_then(|o| o.divisions.iter().position(|d| d.name == division))
        .ok_or((404u16, format!("Divisie onbekend: {}", division)))?;
    if v.get("clear").and_then(|x| x.as_bool()).unwrap_or(false) {
        state.clear_swell_binding(&division);
        return Ok(handle_swell_get(state));
    }
    let channel = v.get("channel").and_then(|x| x.as_u64())
        .ok_or((400u16, "Veld 'channel' ontbreekt".to_string()))? as u8;
    let cc = v.get("cc").and_then(|x| x.as_u64())
        .ok_or((400u16, "Veld 'cc' ontbreekt".to_string()))? as u8;
    // Gedeelde helper (in set_swell_binding_manual): een crescendo-koppeling op
    // dezelfde (kanaal, CC) vervalt hier net als in de UI-paden. Het antwoord
    // blijft de zwel-lijst (vorm ongewijzigd); de verdringing is zichtbaar via
    // GET /crescendo (binding null) en in het log.
    state.set_swell_binding_manual(&division, idx as u8, channel.min(15), cc.min(127));
    {
        let mut bindings = state.swell_bindings.write();
        if let Some(b) = bindings.iter_mut().find(|b| b.division_name == division) {
            if let Some(x) = v.get("min").and_then(|x| x.as_u64()) { b.min_val = (x as u8).min(127); }
            if let Some(x) = v.get("max").and_then(|x| x.as_u64()) { b.max_val = (x as u8).min(127); }
            if b.min_val > b.max_val { std::mem::swap(&mut b.min_val, &mut b.max_val); }
            if let Some(x) = v.get("invert").and_then(|x| x.as_bool()) { b.invert = x; }
        }
    }
    state.zwel_hertoepassen(&division); // bereik/spiegel meteen hoorbaar (0.7.44)
    Ok(handle_swell_get(state))
}

// ============ Automatisch MIDI-archief ============

fn handle_midi_archive_status(state: &AppState) -> Value {
    let dto = crate::commands::midi_archive_status_dto(state);
    let cfg = crate::commands::midi_archive_config_dto(state);
    let mut v = serde_json::to_value(&dto).unwrap_or(json!({}));
    if let Some(obj) = v.as_object_mut() {
        obj.insert("dir".into(), json!(state.midi_archive.dir.read().to_string_lossy().to_string()));
        obj.insert("dir_is_default".into(), json!(cfg.dir_is_default));
        obj.insert("silence_secs".into(), json!(cfg.silence_secs));
        obj.insert("min_notes".into(), json!(cfg.min_notes));
        obj.insert("min_secs".into(), json!(cfg.min_secs));
    }
    v
}

/// `?enabled=1|0&silence=N&min_notes=N&min_secs=N&dir=<url-encoded>`; ontbrekende
/// parameters behouden de huidige waarde. `dir=` (leeg) → standaardmap.
fn handle_midi_archive_config(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let mut cfg = crate::commands::midi_archive_config_dto(state);
    if let Some(e) = parse_query::<String>(query, "enabled") {
        cfg.enabled = e == "1" || e.eq_ignore_ascii_case("true");
    }
    if let Some(s) = parse_query::<u32>(query, "silence") { cfg.silence_secs = s; }
    if let Some(n) = parse_query::<u32>(query, "min_notes") { cfg.min_notes = n; }
    if let Some(n) = parse_query::<u32>(query, "min_secs") { cfg.min_secs = n; }
    if let Some(d) = parse_query::<String>(query, "dir") {
        cfg.dir = d; // leeg = standaardmap
    } else if cfg.dir_is_default {
        cfg.dir = String::new(); // standaard blijft standaard (niet als eigen keuze vastleggen)
    }
    let dto = crate::commands::apply_midi_archive_config(state, cfg).map_err(|e| (400u16, e))?;
    serde_json::to_value(&dto).map_err(|e| (500u16, e.to_string()))
}

fn handle_midi_archive_flush(state: &AppState) -> Value {
    let file = state.midi_archive_flush(std::time::Duration::from_millis(1500))
        .map(|p| p.to_string_lossy().to_string());
    json!({ "ok": true, "file": file })
}

fn handle_midi_archive_list(state: &AppState) -> Value {
    let dir = state.midi_archive.dir.read().clone();
    let files = crate::midi_archive::list_recent(&dir, 10);
    json!({ "files": files, "dir": dir.to_string_lossy().to_string() })
}

// ============ MIDI-speler ============

fn handle_midi_player_play(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON: {}", e)))?;
    let path = v.get("path").and_then(|p| p.as_str())
        .ok_or((400u16, "Veld 'path' ontbreekt".to_string()))?;
    let player = state.midi_player.read();
    let p = player.as_ref().ok_or((500u16, "MIDI player niet beschikbaar".to_string()))?;
    p.load_and_play(std::path::Path::new(path)).map_err(|e| (400u16, e))?;
    Ok(json!({ "ok": true, "path": path }))
}

/// Zoals commands::midi_stop_playback: stop + AllNotesOff + held_notes wissen.
fn handle_midi_player_stop(state: &AppState) -> Value {
    if let Some(p) = state.midi_player.read().as_ref() {
        p.stop();
        state.send_audio_command(AudioCommand::AllNotesOff);
        state.held_notes.write().clear();
    }
    json!({ "ok": true })
}

fn handle_midi_player_status(state: &AppState) -> Value {
    let player = state.midi_player.read();
    match player.as_ref() {
        Some(p) => {
            let s = match p.state() {
                vpo_midi::PlayerState::Stopped => "stopped",
                vpo_midi::PlayerState::Playing => "playing",
                vpo_midi::PlayerState::Paused => "paused",
            };
            json!({
                "state": s,
                "position_ms": p.position_micros() / 1000,
                "duration_ms": p.duration_micros() / 1000,
                "loaded": p.is_loaded(),
            })
        }
        None => json!({ "state": "stopped", "position_ms": 0, "duration_ms": 0, "loaded": false }),
    }
}

// ============ Per-orgel instellingen — self-testing van de persistentie-migratie ============

fn handle_set_master(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let db: f32 = parse_query(query, "db").ok_or((400u16, "Parameter 'db' ontbreekt".to_string()))?;
    state.send_audio_command(AudioCommand::SetMasterGain(db));
    *state.master_volume_db.write() = Some(db);
    Ok(json!({ "ok": true, "db": db }))
}

/// Gedeelde kern van /settings/temperament (query) en /temperament (JSON):
/// zelfde pad als commands::set_temperament (audio + mirror).
fn apply_temperament(state: &AppState, name: String, cents: &[f32], fine: f32, retune: bool) -> Value {
    let mut offsets = [0.0f32; 12];
    for (i, &v) in cents.iter().take(12).enumerate() {
        offsets[i] = v;
    }
    state.send_audio_command(AudioCommand::SetTemperament { note_offsets: offsets, fine_tune: fine, retune });
    *state.temperament_settings.write() = Some(crate::library::TemperamentSettingsSaved {
        name: name.clone(),
        custom_cents: Some(offsets),
        fine_tune_cents: fine,
        a4_hz: 440.0,
        retune: Some(retune),
    });
    json!({ "ok": true, "name": name, "fine": fine, "cents": offsets.to_vec(), "retune": retune })
}

/// ?name=..&fine=..&cents=c0,c1,…,c11 (komma-gescheiden, default nullen)
/// &retune=0|1 (default 0 = Origineel, niet opgegeven = geen hertemperen).
fn handle_set_temperament(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let fine: f32 = parse_query(query, "fine").unwrap_or(0.0);
    let name: String = parse_query(query, "name").unwrap_or_else(|| "test".to_string());
    let cents: Vec<f32> = parse_query::<String>(query, "cents")
        .map(|s| s.split(',').filter_map(|c| c.trim().parse::<f32>().ok()).collect())
        .unwrap_or_default();
    let retune = parse_query::<u8>(query, "retune").map(|v| v != 0).unwrap_or(false);
    Ok(apply_temperament(state, name, &cents, fine, retune))
}

/// JSON-variant: {"name": "...", "cents": [12 getallen], "retune": bool, "fine": f}.
/// Alle velden optioneel (name "test", cents nullen, retune true, fine 0).
fn handle_set_temperament_json(state: &AppState, body: &str) -> Result<Value, (u16, String)> {
    let v: Value = if body.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(body).map_err(|e| (400u16, format!("Ongeldige JSON body: {}", e)))?
    };
    let name = v.get("name").and_then(|x| x.as_str()).unwrap_or("test").to_string();
    let cents: Vec<f32> = v
        .get("cents")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|c| c.as_f64()).map(|c| c as f32).collect())
        .unwrap_or_default();
    let fine = v.get("fine").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
    let retune = v.get("retune").and_then(|x| x.as_bool()).unwrap_or(false);
    Ok(apply_temperament(state, name, &cents, fine, retune))
}

/// Hertemper-status: aantallen uit de geladen orgel-DTO + de temperament-mirror.
fn handle_tuning(state: &AppState) -> Value {
    let (retune_pipes, retune_total) = state
        .loaded_organ_info
        .read()
        .as_ref()
        .map(|o| (o.retune_pipes, o.retune_total))
        .unwrap_or((0, 0));
    let t = state.temperament_settings.read().clone();
    json!({
        "retune_pipes": retune_pipes,
        "retune_total": retune_total,
        "retune": t.as_ref().and_then(|t| t.retune).unwrap_or(false),
        "name": t.as_ref().map(|t| t.name.clone()).unwrap_or_default(),
        "fine_tune": t.as_ref().map(|t| t.fine_tune_cents).unwrap_or(0.0),
        "cents": t.as_ref().and_then(|t| t.custom_cents).map(|c| c.to_vec()).unwrap_or_default(),
    })
}

/// Windmodel voor alle 32 wind-groepen aan/uit (default uit, maar per orgel
/// opgeslagen aanstaan is mogelijk; een toonhoogtemeting mag geen wind-sag
/// bevatten). division_index wordt door de handler als groep-index gelezen.
fn handle_set_wind(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let enabled = parse_query::<u8>(query, "enabled").map(|v| v != 0).unwrap_or(false);
    // Optioneel de drie regelaars meegeven, zodat een meetscript ook de
    // uiterste standen kan nalopen (0.7.50).
    let reservoir: f32 = parse_query(query, "reservoir").unwrap_or(1.0);
    let damping: f32 = parse_query(query, "damping").unwrap_or(0.5);
    let sag: f32 = parse_query(query, "sag").unwrap_or(0.10);
    for g in 0..32u8 {
        state.send_audio_command(AudioCommand::SetWindModel {
            division_index: g,
            enabled,
            reservoir_size: reservoir,
            damping,
            max_sag: sag,
        });
    }
    Ok(json!({ "ok": true, "enabled": enabled, "reservoir": reservoir,
               "damping": damping, "sag": sag }))
}

/// Live winddruk per groep — hetzelfde getal dat de meter in het scherm laat
/// zien. Hiermee is met een meting aan te tonen dát het windmodel werkt.
fn handle_wind_status() -> Result<Value, (u16, String)> {
    let drukken = crate::audio::wind_drukken();
    let stemmen = crate::audio::wind_stemmen();
    let groepen: Vec<Value> = (0..8usize)
        .map(|g| {
            let p = drukken[g];
            json!({
                "group": g,
                "pressure": p,
                "voices": stemmen[g],
                "cents": (p - 1.0) * vpo_audio::WIND_CENTS_PER_EENHEID,
            })
        })
        .collect();
    Ok(json!({ "groups": groepen }))
}

fn handle_set_eq(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let gain: f32 = parse_query(query, "gain").unwrap_or(0.0);
    let freq: f32 = parse_query(query, "freq").unwrap_or(1000.0);
    let band_type: String = parse_query(query, "type").unwrap_or_else(|| "peak".to_string());
    let bandwidth: f32 = parse_query(query, "bw").unwrap_or(1.0);
    let channel: Option<u8> = parse_query(query, "channel");
    let enabled: bool = parse_query::<u8>(query, "enabled").map(|v| v != 0).unwrap_or(true);
    // Zelfde pad als de frontend: vrije banden (hier één band als test).
    let bands = vec![crate::library::EqBandSaved {
        enabled,
        band_type,
        freq,
        gain_db: gain,
        bandwidth,
        channel,
    }];
    state.send_audio_command(AudioCommand::SetEqBands {
        enabled: true,
        bands: bands.iter().map(|b| vpo_audio::EqBandSpec {
            enabled: b.enabled,
            band_type: vpo_audio::EqBandType::from_str(&b.band_type),
            freq: b.freq,
            gain_db: b.gain_db,
            bandwidth_oct: b.bandwidth,
            channel: b.channel,
        }).collect(),
    });
    *state.eq_settings.write() = Some(crate::library::EqSettingsSaved {
        enabled: true, bands, ..Default::default()
    });
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
    // `enabled` (beschikbaarheid van de LFO-tremulant voor deze divisie) werd
    // eerder genegeerd en stond altijd hard op true.
    let enabled = parse_bool_query(query, "enabled").unwrap_or(true);
    state.division_tremulants.write().insert(division.clone(), (enabled, rate, 10.0, 15.0));
    Ok(json!({ "ok": true, "division": division, "rate": rate, "enabled": enabled }))
}

/// POST /tremulant?division=<naam>&active=0|1 — tremulant van een divisie live
/// aan/uit via dezelfde kern als het Tauri-commando `set_tremulant`.
fn handle_tremulant(state: &AppState, query: &str) -> Result<Value, (u16, String)> {
    let division: String = parse_query(query, "division")
        .ok_or((400u16, "Ontbrekende parameter 'division'".to_string()))?;
    let active = parse_bool_query(query, "active").unwrap_or(true);
    let stops = crate::commands::do_set_tremulant(state, &division, active)
        .map_err(|e| (404u16, e))?;
    Ok(json!({ "ok": true, "division": division, "active": active, "stops": stops }))
}

/// Booleaanse queryparameter: 1/true/on/yes/aan = waar, 0/false/off/no/uit =
/// onwaar; ontbreekt hij (of is hij onleesbaar) dan None.
fn parse_bool_query(query: &str, key: &str) -> Option<bool> {
    let raw: String = parse_query(query, key)?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" | "aan" => Some(true),
        "0" | "false" | "off" | "no" | "uit" => Some(false),
        _ => None,
    }
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
    let perspectives = handle_perspectives(state);
    json!({ "pans": pans, "tremulants": trems, "swells": swells, "winds": winds, "perspectives": perspectives })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_eq_vergelijkt_exact() {
        assert!(token_eq("abc", "abc"));
        assert!(!token_eq("abc", "abd"));
        assert!(!token_eq("abc", "abcd"));
        assert!(!token_eq("", "a"));
        assert!(token_eq("", ""));
    }

    #[test]
    fn strip_token_prefix_varianten() {
        assert_eq!(strip_token_prefix("/r/abc/stops/x/toggle", "abc"), Some("/stops/x/toggle".to_string()));
        assert_eq!(strip_token_prefix("/r/abc", "abc"), Some(String::new()));
        assert_eq!(strip_token_prefix("/r/abc/", "abc"), Some("/".to_string()));
        assert_eq!(strip_token_prefix("/r/abc?x=1", "abc"), Some("?x=1".to_string()));
        assert_eq!(strip_token_prefix("/r/abd/state", "abc"), None);
        assert_eq!(strip_token_prefix("/state", "abc"), None);
        assert_eq!(strip_token_prefix("/r//state", "abc"), None);
        assert_eq!(strip_token_prefix("/r/", "abc"), None);
    }

    #[test]
    fn allowed_action_codes() {
        for c in 0..=23u8 {
            assert!(allowed_action_code(c, 0), "code {} moet altijd mogen", c);
        }
        assert!(allowed_action_code(24, 1));
        assert!(allowed_action_code(24 + 2, 3));
        assert!(!allowed_action_code(24 + 3, 3));
        assert!(!allowed_action_code(24, 0));
        assert!(allowed_action_code(40, 0));
        assert!(allowed_action_code(41, 0));
        assert!(allowed_action_code(43, 0));
        assert!(!allowed_action_code(42, 5), "computer afsluiten nooit via de afstandsbediening");
        assert!(!allowed_action_code(44, 5));
        assert!(!allowed_action_code(150, 5));
        assert!(!allowed_action_code(255, 5));
    }

    #[test]
    fn remote_html_is_ingebed() {
        assert!(REMOTE_HTML.contains("<meta name=\"viewport\""));
        assert!(REMOTE_HTML.contains("fetch("));
        // Geen externe resources: de pagina moet offline werken.
        assert!(!REMOTE_HTML.contains("src=\"http"));
        assert!(!REMOTE_HTML.contains("href=\"http"));
    }

    #[test]
    fn url_decode_stop_ids() {
        assert_eq!(url_decode("Prestant%208"), "Prestant 8");
        assert_eq!(url_decode("Prestant_8"), "Prestant_8");
    }

    // ---- Indeling van de afstandsbediening (0.7.39) ----

    fn testorgel() -> crate::commands::OrganInfoDto {
        use crate::commands::{CouplerDto, DivisionDto, OrganInfoDto, StopDto};
        let stop = |id: &str, name: &str, pitch: &str, trem: bool| StopDto {
            id: id.to_string(),
            name: name.to_string(),
            pitch: pitch.to_string(),
            drawn: false,
            color: None,
            has_tremulant: trem,
            midi_action_code: 150,
            internal_stop_id: 0,
            first_midi_note: 36,
            last_midi_note: 96,
            is_reed: false,
        };
        let coupler = |id: &str, kind: &str, div: &str| CouplerDto {
            id: id.to_string(),
            name: id.to_string(),
            source_division: div.to_string(),
            destination_division: div.to_string(),
            active: false,
            display_in_division: div.to_string(),
            midi_action_code: 60,
            coupler_type: kind.to_string(),
            pitch_offset: 0,
        };
        OrganInfoDto {
            id: r"C:\Orgels\Test\organ.organ".to_string(),
            name: "Testorgel".to_string(),
            builder: String::new(),
            location: String::new(),
            year: None,
            stop_count: 3,
            divisions: vec![
                DivisionDto {
                    name: "Hoofdwerk".to_string(),
                    display_name: "Hoofdwerk (I)".to_string(),
                    stops: vec![stop("hw_prestant", "Prestant 8'", "8'", false), stop("hw_octaaf", "Octaaf", "4'", false)],
                    has_tremulant: false,
                    tremulant_kind: None,
                    has_swell: false,
                },
                DivisionDto {
                    name: "Pedaal".to_string(),
                    display_name: "Pedaal".to_string(),
                    stops: vec![stop("ped_subbas", "Subbas", "16'", false)],
                    has_tremulant: true,
                    tremulant_kind: Some("synth".to_string()),
                    has_swell: false,
                },
            ],
            couplers: Some(vec![
                coupler("real_coupler_0", "unison", "Pedaal"),
                coupler("coupler_hw_ped_unison", "unison", "Pedaal"),
                coupler("coupler_hw_ped_super", "super", "Pedaal"),
            ]),
            retune_pipes: 0,
            retune_total: 0,
            perspectives: Vec::new(),
            layered_stops: 0,
            release_pipes: 0,
        }
    }

    fn div_namen(v: &Value) -> Vec<String> {
        v["divisions"].as_array().unwrap().iter()
            .map(|d| d["name"].as_str().unwrap().to_string()).collect()
    }
    fn koppel_ids(v: &Value) -> Vec<String> {
        v["couplers"].as_array().unwrap().iter()
            .map(|c| c["id"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn remote_organ_dto_default_zonder_layout() {
        let o = testorgel();
        let trems = std::collections::HashMap::new();
        let v = remote_organ_dto_inner(&o, None, &trems, 0);
        // Zelfde default als het orgelscherm: real_coupler_* + unison.
        assert_eq!(koppel_ids(&v), vec!["real_coupler_0", "coupler_hw_ped_unison"]);
        assert_eq!(div_namen(&v), vec!["Hoofdwerk", "Pedaal"]);
        assert_eq!(v["divisions"][0]["div_index"], json!(0));
        assert_eq!(v["divisions"][1]["div_index"], json!(1));
        assert_eq!(v["layout"]["coupler_placement"], json!("bar"));
        assert_eq!(v["layout"]["show_setzer"], json!(true));
        // Divisiekop = de naam van het orgelscherm, niet display_name.
        assert!(v["divisions"][0].get("display_name").is_none());
        // Voettal niet dubbel: "Prestant 8'" + pitch "8'" → "Prestant".
        assert_eq!(v["divisions"][0]["stops"][0]["name"], json!("Prestant"));
        assert_eq!(v["divisions"][0]["stops"][1]["name"], json!("Octaaf"));
    }

    #[test]
    fn remote_organ_dto_filtert_koppels_en_onderdelen() {
        let o = testorgel();
        let trems = std::collections::HashMap::new();
        let layout = crate::library::RemoteLayoutSaved {
            visible_couplers: vec!["coupler_hw_ped_super".to_string()],
            coupler_placement: "division".to_string(),
            knob_shape: "round".to_string(),
            show_setzer: false,
            show_volume: false,
            ..Default::default()
        };
        let v = remote_organ_dto_inner(&o, Some(&layout), &trems, 7);
        assert_eq!(koppel_ids(&v), vec!["coupler_hw_ped_super"]);
        assert_eq!(v["couplers"][0]["display_in_division"], json!("Pedaal"));
        assert_eq!(v["layout"]["coupler_placement"], json!("division"));
        assert_eq!(v["layout"]["knob_shape"], json!("round"));
        assert_eq!(v["layout"]["show_setzer"], json!(false));
        assert_eq!(v["layout"]["show_volume"], json!(false));
        assert_eq!(v["layout"]["show_panic"], json!(true));
        assert_eq!(v["layout"]["rev"], json!(7));
    }

    #[test]
    fn remote_organ_dto_volgt_divisievolgorde_en_registervolgorde() {
        let o = testorgel();
        let trems = std::collections::HashMap::new();
        let mut stop_order = std::collections::HashMap::new();
        stop_order.insert("Hoofdwerk".to_string(), vec!["hw_octaaf".to_string()]);
        let layout = crate::library::RemoteLayoutSaved {
            divisions: vec!["Pedaal".to_string(), "Hoofdwerk".to_string()],
            stop_order,
            ..Default::default()
        };
        let v = remote_organ_dto_inner(&o, Some(&layout), &trems, 1);
        assert_eq!(div_namen(&v), vec!["Pedaal", "Hoofdwerk"]);
        // div_index blijft de DTO-index (actiecode 24+i).
        assert_eq!(v["divisions"][0]["div_index"], json!(1));
        assert_eq!(v["divisions"][1]["div_index"], json!(0));
        // Gesleepte volgorde eerst, onbekende registers achteraan.
        let ids: Vec<&str> = v["divisions"][1]["stops"].as_array().unwrap().iter()
            .map(|s| s["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["hw_octaaf", "hw_prestant"]);
        // Onbekende divisienaam (indeling van een ánder orgel) → alles tonen.
        let vreemd = crate::library::RemoteLayoutSaved {
            divisions: vec!["Rugwerk".to_string()],
            ..Default::default()
        };
        let v2 = remote_organ_dto_inner(&o, Some(&vreemd), &trems, 1);
        assert_eq!(div_namen(&v2), vec!["Hoofdwerk", "Pedaal"]);
    }

    #[test]
    fn remote_organ_dto_tremulantcriterium_als_orgelscherm() {
        let o = testorgel();
        // Zonder opgeslagen keuze: has_tremulant (en geen golfvorm) → knop.
        let leeg = std::collections::HashMap::new();
        let v = remote_organ_dto_inner(&o, None, &leeg, 0);
        assert_eq!(v["divisions"][0]["show_tremulant"], json!(false));
        assert_eq!(v["divisions"][1]["show_tremulant"], json!(true));
        // Gebruiker zette de LFO-tremulant uit → knop weg (geen stop met
        // tremulant-opnamen in deze divisie).
        let mut uit = std::collections::HashMap::new();
        uit.insert("Pedaal".to_string(), (false, 6.0, 10.0, 15.0));
        let v2 = remote_organ_dto_inner(&o, None, &uit, 0);
        assert_eq!(v2["divisions"][1]["show_tremulant"], json!(false));
        // En aan voor een divisie zonder bron-tremulant.
        let mut aan = std::collections::HashMap::new();
        aan.insert("Hoofdwerk".to_string(), (true, 6.0, 10.0, 15.0));
        let v3 = remote_organ_dto_inner(&o, None, &aan, 0);
        assert_eq!(v3["divisions"][0]["show_tremulant"], json!(true));
    }

    #[test]
    fn strip_pitch_suffix_zoals_orgelscherm() {
        assert_eq!(strip_pitch_suffix("Prestant 8'", "8'"), "Prestant");
        assert_eq!(strip_pitch_suffix("Prestant8'", "8'"), "Prestant");
        assert_eq!(strip_pitch_suffix("Octaaf", "4'"), "Octaaf");
        assert_eq!(strip_pitch_suffix("Mixtuur", ""), "Mixtuur");
        // Alleen een voettal als naam blijft staan (nooit leeg).
        assert_eq!(strip_pitch_suffix("8'", "8'"), "8'");
    }

    #[test]
    fn remote_layout_json_zonder_velden_toont_alles() {
        // Een oude .jm-settings.json (of een minimale POST) moet leesbaar zijn
        // en standaard alle onderdelen tonen.
        let l: crate::library::RemoteLayoutSaved = serde_json::from_str("{}").unwrap();
        assert!(l.show_couplers && l.show_tremulant && l.show_setzer && l.show_volume && l.show_panic);
        assert!(l.divisions.is_empty() && l.visible_couplers.is_empty());
        let l2: crate::library::RemoteLayoutSaved =
            serde_json::from_str(r#"{"show_setzer":false,"divisions":["Pedaal"]}"#).unwrap();
        assert!(!l2.show_setzer && l2.show_volume);
        assert_eq!(l2.divisions, vec!["Pedaal".to_string()]);
    }

    #[test]
    fn short_id_stabiel_en_zonder_pad() {
        let a = short_id(r"C:\Orgels\Bätz\organ.organ");
        assert_eq!(a, short_id(r"C:\Orgels\Bätz\organ.organ"));
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, short_id(r"C:\Orgels\Ander\organ.organ"));
        assert!(!a.contains("Orgels"));
    }
}
