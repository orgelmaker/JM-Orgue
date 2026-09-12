//! Afstandsbediening in het netwerk (0.7.38): tablet/telefoon als registers,
//! koppels, tremulant, setzer en volume.
//!
//! De router zelf is `test_api::ApiServer` met scope Remote (whitelist + token);
//! hier zitten token, LAN-IP-detectie, QR-code (SVG), start/stop van de server,
//! persistentie in AudioPrefs (audio_config.json: remote_enabled/remote_port/
//! remote_token) en de Tauri-commando's voor het instellingenblok.
//!
//! Beveiliging: alleen het token beschermt (pad-prefix /r/<token>/ of header
//! X-Remote-Token); verkeer is http zonder TLS binnen het eigen (wifi-)netwerk.
//! 'Nieuw token' trekt alle eerder gescande QR-codes in. Het token wordt nooit
//! gelogd.

use crate::state::{load_audio_prefs, save_audio_prefs, AppState, AudioPrefs};
use crate::test_api::{ApiScope, ApiServer};
use std::net::{ToSocketAddrs, UdpSocket};
use std::path::Path;
use tauri::State;
use tracing::{info, warn};

/// Standaardpoort van de afstandsbediening (test-API = 8765).
pub const DEFAULT_PORT: u16 = 8766;

/// 31 tekens zonder 0/o/1/l/i: handmatig overtypen blijft mogelijk. 31^12 ≈ 2^59.
const TOKEN_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
const TOKEN_LEN: usize = 12;

pub fn generate_token() -> String {
    use rand::Rng;
    let mut r = rand::thread_rng();
    (0..TOKEN_LEN)
        .map(|_| TOKEN_ALPHABET[r.gen_range(0..TOKEN_ALPHABET.len())] as char)
        .collect()
}

/// IPv4-adressen van deze pc in het LAN (geen loopback), gededupliceerd.
/// (1) UDP-connect-truc: het OS kiest de uitgaande interface van de default-
/// route (er wordt niets verstuurd; 192.0.2.1 is TEST-NET). (2) Aanvulling/
/// fallback zonder default-route: alle adressen van de eigen hostnaam.
pub fn lan_ips() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("192.0.2.1:9").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                let ip = addr.ip();
                if !ip.is_loopback() && !ip.is_unspecified() {
                    out.push(ip.to_string());
                }
            }
        }
    }
    let host = std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_default();
    if !host.is_empty() {
        if let Ok(addrs) = (host.as_str(), 0u16).to_socket_addrs() {
            for a in addrs {
                if let std::net::IpAddr::V4(v4) = a.ip() {
                    if v4.is_loopback() || v4.is_unspecified() || v4.is_link_local() {
                        continue;
                    }
                    let s = v4.to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        }
    }
    out
}

/// Adres voor de tablet — mét afsluitende slash (relatieve fetches op de pagina).
pub fn remote_url(ip: &str, port: u16, token: &str) -> String {
    format!("http://{}:{}/r/{}/", ip, port, token)
}

/// QR-code als SVG-string (qrcode-crate, pure Rust); None bij een ongeldige invoer.
pub fn qr_svg(data: &str) -> Option<String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(data.as_bytes()).ok()?;
    Some(
        code.render::<svg::Color>()
            .min_dimensions(180, 180)
            .quiet_zone(true)
            .dark_color(svg::Color("#000000"))
            .light_color(svg::Color("#ffffff"))
            .build(),
    )
}

/// Token uit de prefs, of een nieuw token aanmaken en direct opslaan.
fn ensure_token(prefs: &mut AudioPrefs, app_data_dir: &Path) -> String {
    match prefs.remote_token.as_deref() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => {
            let t = generate_token();
            prefs.remote_token = Some(t.clone());
            save_audio_prefs(app_data_dir, prefs);
            t
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RemoteStatusDto {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub token: String,
    pub urls: Vec<String>,
    pub qr_svg: Option<String>,
    pub error: Option<String>,
}

/// Server starten (no-op als hij al draait). Bind met retry: na stop() kan de
/// poort nog heel even bezet zijn.
pub fn start(state: &AppState, port: u16, token: String) -> Result<(), String> {
    let mut g = state.remote_api.lock();
    if g.is_some() {
        return Ok(());
    }
    let mut last_err = String::new();
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        match ApiServer::start(state.clone(), "0.0.0.0", port, Some(token.clone()), ApiScope::Remote, std::path::PathBuf::new()) {
            Ok(srv) => {
                *g = Some(srv);
                *state.remote_last_error.write() = None;
                info!("Afstandsbediening actief op poort {}", port);
                return Ok(());
            }
            Err(e) => last_err = e,
        }
    }
    *state.remote_last_error.write() = Some(last_err.clone());
    Err(last_err)
}

pub fn stop(state: &AppState) {
    let srv = state.remote_api.lock().take();
    if let Some(srv) = srv {
        srv.stop();
        info!("Afstandsbediening gestopt");
    }
}

/// Status voor de UI/test-API. Maakt zo nodig een token aan zodat URL/QR al
/// vóór het inschakelen zichtbaar zijn.
pub fn status_inner(state: &AppState) -> RemoteStatusDto {
    let mut prefs = load_audio_prefs(&state.app_data_dir);
    let token = ensure_token(&mut prefs, &state.app_data_dir);
    let port = prefs.remote_port.unwrap_or(DEFAULT_PORT);
    let running = state.remote_api.lock().is_some();
    let urls: Vec<String> = lan_ips().iter().map(|ip| remote_url(ip, port, &token)).collect();
    let qr = urls.first().and_then(|u| qr_svg(u));
    RemoteStatusDto {
        enabled: prefs.remote_enabled.unwrap_or(false),
        running,
        port,
        token,
        urls,
        qr_svg: qr,
        error: state.remote_last_error.read().clone(),
    }
}

/// Aan/uit (+ optioneel poort) zetten, server starten/stoppen en prefs opslaan.
pub fn set_enabled_inner(state: &AppState, enabled: bool, port: Option<u16>) -> Result<RemoteStatusDto, String> {
    let mut prefs = load_audio_prefs(&state.app_data_dir);
    let port = port.or(prefs.remote_port).unwrap_or(DEFAULT_PORT);
    if !(1024..=65535).contains(&port) {
        return Err("Poort moet tussen 1024 en 65535 liggen".to_string());
    }
    let test_port = crate::test_api::TEST_API_PORT.load(std::sync::atomic::Ordering::Relaxed);
    if enabled && test_port != 0 && port == test_port {
        return Err(format!("Poort {} is in gebruik door de test-API", port));
    }
    if enabled {
        let token = ensure_token(&mut prefs, &state.app_data_dir);
        // Poortwissel: altijd eerst stoppen (start() is anders een no-op).
        stop(state);
        if let Err(e) = start(state, port, token) {
            prefs.remote_enabled = Some(false);
            prefs.remote_port = Some(port);
            save_audio_prefs(&state.app_data_dir, &prefs);
            return Err(e);
        }
        prefs.remote_enabled = Some(true);
    } else {
        stop(state);
        prefs.remote_enabled = Some(false);
        *state.remote_last_error.write() = None;
    }
    prefs.remote_port = Some(port);
    save_audio_prefs(&state.app_data_dir, &prefs);
    Ok(status_inner(state))
}

/// Nieuw token: trekt alle bestaande adressen/QR-codes in; herstart de server
/// als hij draait.
pub fn new_token_inner(state: &AppState) -> Result<RemoteStatusDto, String> {
    let mut prefs = load_audio_prefs(&state.app_data_dir);
    let token = generate_token();
    prefs.remote_token = Some(token.clone());
    save_audio_prefs(&state.app_data_dir, &prefs);
    let running = state.remote_api.lock().is_some();
    if running {
        let port = prefs.remote_port.unwrap_or(DEFAULT_PORT);
        stop(state);
        start(state, port, token)?;
    }
    Ok(status_inner(state))
}

/// Bij app-start: server starten als hij in de prefs aan staat (main.rs setup).
pub fn start_from_prefs(state: &AppState) {
    let mut prefs = load_audio_prefs(&state.app_data_dir);
    if prefs.remote_enabled == Some(true) {
        let token = ensure_token(&mut prefs, &state.app_data_dir);
        let port = prefs.remote_port.unwrap_or(DEFAULT_PORT);
        if let Err(e) = start(state, port, token) {
            warn!("Afstandsbediening niet gestart: {}", e);
        }
    }
}

// ============ Tauri-commando's ============
// Async: set/new_token joinen de serve-thread en lan_ips() doet een DNS-lookup
// op de hostnaam — sync-commands draaien op de UI-thread (les 2026-07-29).

#[tauri::command]
pub async fn get_remote_status(state: State<'_, AppState>) -> Result<RemoteStatusDto, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || status_inner(&st))
        .await
        .map_err(|e| format!("Task join error: {}", e))
}

#[tauri::command]
pub async fn set_remote_enabled(state: State<'_, AppState>, enabled: bool, port: Option<u16>) -> Result<RemoteStatusDto, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || set_enabled_inner(&st, enabled, port))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
pub async fn new_remote_token(state: State<'_, AppState>) -> Result<RemoteStatusDto, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || new_token_inner(&st))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_lengte_en_alfabet() {
        let t = generate_token();
        assert_eq!(t.len(), TOKEN_LEN);
        assert!(t.bytes().all(|b| TOKEN_ALPHABET.contains(&b)), "token {} buiten alfabet", t);
        assert!(!t.contains(['0', 'o', '1', 'l', 'i']));
        let t2 = generate_token();
        assert_ne!(t, t2, "twee tokens moeten verschillen");
    }

    #[test]
    fn remote_url_formaat() {
        assert_eq!(remote_url("192.168.1.5", 8766, "abc"), "http://192.168.1.5:8766/r/abc/");
        assert!(remote_url("10.0.0.2", 9000, "x").ends_with('/'));
    }

    #[test]
    fn qr_svg_is_svg() {
        let svg = qr_svg("http://192.168.1.5:8766/r/abcdefghjkmn/").expect("qr");
        assert!(svg.starts_with("<svg") || svg.starts_with("<?xml"), "begin: {}", &svg[..svg.len().min(40)]);
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn lan_ips_zonder_loopback() {
        let ips = lan_ips();
        assert!(!ips.iter().any(|ip| ip == "127.0.0.1" || ip == "0.0.0.0"));
        let mut dedup = ips.clone();
        dedup.dedup();
        assert_eq!(dedup.len(), ips.len(), "geen dubbele adressen");
    }
}
