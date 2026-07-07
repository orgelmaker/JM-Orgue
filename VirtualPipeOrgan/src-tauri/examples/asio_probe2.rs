//! Probe 2.0: test of een procesbreed GECACHTE ASIO4ALL-driver het
//! wisselscenario betrouwbaar maakt.
//!   1. ASIO-device laden en VASTHOUDEN (cache).
//!   2. WASAPI-stream bouwen+spelen terwijl ASIO idle vastgehouden wordt — callbacks?
//!   3. ASIO-stream #1 op gecacht device — callbacks? Dan stream droppen (device blijft).
//!   4. WASAPI-stream opnieuw — callbacks?
//!   5. ASIO-stream #2 op HETZELFDE gecachte device — callbacks?  ← de her-wissel
//!   6. Ter controle: ASIO-stream #3.
//! Draaien onder de exe-naam vpo-app.exe zodat het ASIO4ALL-profiel van de
//! echte app gebruikt wordt.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn count_callbacks(label: &str, device: &cpal::Device, secs: u64) -> u64 {
    let cfg = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            println!("[{label}] default_output_config faalde: {e}");
            return 0;
        }
    };
    println!(
        "[{label}] bouwen: {} Hz, {} ch, {:?}",
        cfg.sample_rate().0,
        cfg.channels(),
        cfg.sample_format()
    );
    let callbacks = Arc::new(AtomicU64::new(0));
    let cb = callbacks.clone();
    let build = match cfg.sample_format() {
        cpal::SampleFormat::I32 => device.build_output_stream(
            &cfg.config(),
            move |data: &mut [i32], _| {
                cb.fetch_add(1, Ordering::Relaxed);
                for s in data.iter_mut() {
                    *s = 0;
                }
            },
            move |e| println!("    stream-fout: {e}"),
            None,
        ),
        _ => device.build_output_stream(
            &cfg.config(),
            move |data: &mut [f32], _| {
                cb.fetch_add(1, Ordering::Relaxed);
                for s in data.iter_mut() {
                    *s = 0.0;
                }
            },
            move |e| println!("    stream-fout: {e}"),
            None,
        ),
    };
    match build {
        Ok(s) => {
            if let Err(e) = s.play() {
                println!("[{label}] play() faalde: {e}");
                return 0;
            }
            std::thread::sleep(Duration::from_secs(secs));
            let n = callbacks.load(Ordering::Relaxed);
            println!(
                "[{label}] callbacks na {secs}s: {n} {}",
                if n > 0 { "— OK" } else { "— STIL!" }
            );
            n
        }
        Err(e) => {
            println!("[{label}] build faalde: {e}");
            0
        }
    }
}

fn main() {
    println!("=== Stap 1: ASIO-device laden en cachen ===");
    let asio_host = cpal::available_hosts()
        .into_iter()
        .find(|h| h.name().eq_ignore_ascii_case("asio"))
        .and_then(|id| cpal::host_from_id(id).ok())
        .expect("ASIO-host niet beschikbaar");
    let asio_device = asio_host
        .output_devices()
        .ok()
        .and_then(|mut d| d.next())
        .expect("geen ASIO-apparaat gevonden");
    println!("Gecacht: '{}'", asio_device.name().unwrap_or_default());

    println!("\n=== Stap 2: WASAPI spelen terwijl ASIO idle vastgehouden wordt ===");
    let wasapi = cpal::default_host();
    let wdev = wasapi.default_output_device().expect("geen WASAPI-device");
    println!("WASAPI-device: '{}'", wdev.name().unwrap_or_default());
    count_callbacks("wasapi-1", &wdev, 2);

    println!("\n=== Stap 3: ASIO-stream #1 op gecacht device ===");
    count_callbacks("asio-1", &asio_device, 3);

    println!("\n=== Stap 4: WASAPI opnieuw (na ASIO-stream-drop) ===");
    count_callbacks("wasapi-2", &wdev, 2);

    println!("\n=== Stap 5: ASIO-stream #2 op gecacht device (de her-wissel!) ===");
    count_callbacks("asio-2", &asio_device, 3);

    println!("\n=== Stap 6: ASIO-stream #3 (nog eens) ===");
    count_callbacks("asio-3", &asio_device, 3);

    println!("\nKlaar.");
}
