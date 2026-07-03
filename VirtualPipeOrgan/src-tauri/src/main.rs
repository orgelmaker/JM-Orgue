//! JM-Orgue Virtual Pipe Organ - Tauri Application
//!
//! Main entry point for the desktop application.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod commands;
mod library;
mod silence;
mod loop_tool;
mod recorder;
mod state;
mod test_api;

use std::sync::Arc;
use tauri::Manager;
use tracing::{info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::state::AppState;

fn parse_test_api_port() -> Option<u16> {
    // --test-api  (default 8765)
    // --test-api=PORT
    // --test-api PORT
    let args: Vec<String> = std::env::args().collect();
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--test-api" {
            // Volgende arg is poort, of default
            if let Some(next) = iter.next() {
                if let Ok(p) = next.parse() {
                    return Some(p);
                }
            }
            return Some(8765);
        }
        if let Some(rest) = arg.strip_prefix("--test-api=") {
            return rest.parse().ok().or(Some(8765));
        }
    }
    None
}

fn main() {
    // Set up logging to a file in %APPDATA%\nl.jm-orgue.app\jm-orgue.log
    // This works even without a visible console (windows_subsystem = "windows")
    let log_dir = dirs::data_dir()
        .map(|d| d.join("nl.jm-orgue.app"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&log_dir).ok();
    let log_path = log_dir.join("jm-orgue.log");

    // Logbestand wordt bij elke start GETRUNCATEERD: één sessie per bestand.
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
    {
        Ok(f) => Some(f),
        Err(e) => {
            eprintln!("Could not open log file {:?}: {}", log_path, e);
            None
        }
    };

    // Build subscriber: log to stderr AND to file (if available)
    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false);

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,vpo_midi=debug,vpo_app=debug"));

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer);

    if let Some(f) = file {
        let file_layer = fmt::layer()
            .with_writer(std::sync::Mutex::new(f))
            .with_ansi(false)
            .with_target(true);
        registry.with(file_layer).init();
    } else {
        registry.init();
    }

    info!("=== JM-Orgue starting ===");
    info!("Log file: {:?}", log_path);

    let test_api_port = parse_test_api_port();
    let log_path_for_api = log_path.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let app_data_dir = app.path().app_data_dir()
                .expect("Failed to get app data directory");
            std::fs::create_dir_all(&app_data_dir).ok();
            info!("App data directory: {:?}", app_data_dir);

            let state = AppState::new(app_data_dir);

            // Start test-API thread als --test-api flag aanwezig is.
            if let Some(port) = test_api_port {
                test_api::start_test_api(state.clone(), port, log_path_for_api.clone());
            }

            // Uitgestelde ASIO-wissel. Een kóúde start op ASIO levert (op ASIO4ALL)
            // een stille stream op, terwijl exact dezelfde wissel op een volledig
            // draaiende app betrouwbaar werkt (geverifieerd 2026-07-03; zie ook
            // BUILDING.md). AppState::new start daarom op de default host; hier
            // passen we de opgeslagen ASIO-voorkeur ~10s na de start alsnog toe en
            // herladen we het orgel, zoals de UI-wissel dat ook doet.
            {
                let prefs = state::load_audio_prefs(&state.app_data_dir);
                let asio_pref = prefs.host.as_deref()
                    .map(|h| h.eq_ignore_ascii_case("asio"))
                    .unwrap_or(false);
                if asio_pref {
                    let st = state.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(10));
                        // Lees de prefs vers: de gebruiker kan intussen handmatig
                        // gewisseld hebben — dan geldt diens laatste keuze.
                        let prefs = state::load_audio_prefs(&st.app_data_dir);
                        let still_asio = prefs.host.as_deref()
                            .map(|h| h.eq_ignore_ascii_case("asio"))
                            .unwrap_or(false);
                        let already_asio = st.audio_player.read().as_ref()
                            .map(|p| p.current_host.read().eq_ignore_ascii_case("asio"))
                            .unwrap_or(false);
                        if !still_asio || already_asio {
                            return;
                        }
                        info!("Uitgestelde ASIO-wissel wordt uitgevoerd (opgeslagen voorkeur)");
                        match st.switch_audio_output(audio::AudioOutputConfig {
                            host_name: prefs.host.clone(),
                            device_name: prefs.device.clone(),
                            buffer_frames: prefs.buffer_frames,
                        }) {
                            Ok(Some(id)) => {
                                let res = if id.to_lowercase().ends_with(".organ") {
                                    commands::do_load_organ(&st, &id)
                                } else {
                                    commands::do_load_samples_from_directory(&st, &id)
                                };
                                if let Err(e) = res {
                                    tracing::warn!("Orgel herladen na ASIO-wissel mislukte: {}", e);
                                }
                            }
                            Ok(None) => {}
                            Err(e) => tracing::warn!(
                                "Uitgestelde ASIO-wissel mislukte ({}); audio blijft op de default host", e),
                        }
                    });
                }
            }

            app.manage(state);
            info!("Application initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_audio_devices,
            commands::list_audio_hosts,
            commands::list_output_devices,
            commands::set_audio_output,
            commands::scan_sampleset_silence,
            commands::trim_sampleset_silence,
            commands::scan_sampleset_loops,
            commands::apply_sampleset_loops,
            commands::query_supported_sample_rates,
            commands::midi_play_file,
            commands::midi_stop_playback,
            commands::midi_pause_playback,
            commands::midi_resume_playback,
            commands::midi_seek,
            commands::midi_set_speed,
            commands::midi_player_status,
            commands::get_midi_devices,
            commands::start_audio,
            commands::stop_audio,
            commands::connect_midi,
            commands::connect_all_midi,
            commands::disconnect_midi,
            commands::set_midi_mapping,
            commands::get_midi_mappings,
            commands::learn_midi_channel,
            commands::learn_keyboard_range,
            commands::get_preset_bindings,
            commands::learn_preset_binding,
            commands::clear_preset_binding,
            commands::poll_preset_trigger,
            commands::load_organ,
            commands::get_organ_info,
            commands::set_stop,
            commands::set_drawn_stops,
            commands::toggle_stop,
            commands::play_note,
            commands::stop_note,
            commands::play_note_all_stops,
            commands::stop_note_all_stops,
            commands::set_master_volume,
            commands::set_reverb_mix,
            commands::set_tremulant,
            commands::get_status,
            commands::scan_samples,
            commands::create_custom_organ,
            commands::save_custom_organ,
            commands::load_custom_organ,
            commands::load_samples_from_directory,
            commands::toggle_coupler,
            commands::set_active_couplers,
            commands::set_division_volume,
            commands::get_organ_settings,
            commands::export_settings,
            commands::import_settings,
            commands::set_crescendo_config,
            commands::set_crescendo_stage,
            commands::get_crescendo_config,
            commands::get_crescendo_state,
            commands::learn_crescendo_pedal,
            commands::clear_crescendo_binding,
            commands::set_crescendo_invert,
            commands::set_crescendo_range,
            commands::get_crescendo_binding,
            commands::set_swell_invert,
            commands::set_swell_range,
            commands::get_autostart_enabled,
            commands::set_autostart_enabled,
            commands::shutdown_computer,
            commands::start_recording,
            commands::stop_recording,
            commands::get_recording_status,
            commands::get_recordings_dir,
            commands::set_restore_registration,
            commands::get_restore_registration,
            commands::refresh_midi_devices,
            commands::scan_ble_midi,
            commands::connect_ble_midi,
            commands::disconnect_ble_midi,
            commands::get_connected_ble_midi,
            commands::get_ble_midi_count,
            commands::start_midi_recording,
            commands::midi_recording_status,
            commands::stop_midi_recording,
            commands::suggest_midi_recording_path,
            commands::save_midi_recording,
            commands::clear_midi_recording,
            commands::set_pipe_voicing,
            commands::get_pipe_voicings,
            commands::reset_pipe_voicing,
            commands::reset_stop_voicing,
            commands::apply_saved_voicings,
            commands::apply_saved_output_channels,
            commands::apply_saved_wind_groups,
            commands::set_division_pan,
            commands::set_division_output_channels,
            commands::get_division_output_channels,
            commands::set_ccis_spread,
            commands::get_ccis_spread,
            commands::set_division_ccis,
            commands::get_division_ccis,
            commands::query_audio_channel_count,
            commands::set_parametric_eq,
            commands::persist_reverb_config,
            commands::set_algorithmic_reverb,
            commands::set_reverb_type,
            commands::set_tremulant_lfo,
            commands::persist_tremulant_config,
            commands::set_wind_model,
            commands::persist_wind_group_config,
            commands::set_division_wind_group,
            commands::get_division_wind_groups,
            commands::get_num_wind_groups,
            commands::set_num_wind_groups,
            commands::set_swell_config,
            commands::get_division_volumes,
            commands::learn_swell_pedal,
            commands::clear_swell_binding,
            commands::get_swell_bindings,
            commands::set_temperament,
            commands::export_organ_file,
            commands::load_impulse_response,
            commands::get_organ_library,
            commands::remove_from_library,
            commands::save_current_organ_settings,
            commands::get_organ_image,
            commands::restore_preset_bindings,
            commands::restore_swell_bindings,
            commands::get_saved_presets,
        ])
        .run(tauri::generate_context!())
        .expect("Error running application");
}
