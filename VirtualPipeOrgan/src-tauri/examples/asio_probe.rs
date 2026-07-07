//! Diagnose-probe voor ASIO4ALL-gedrag op deze machine.
//! Test 1: koude ASIO-enumeratie + stream + callbacks (geen andere stream actief).
//! Test 2: ASIO-enumeratie terwijl een WASAPI-stream draait (warm) — verwacht falen.
//! Test 3: WASAPI-stream dicht, korte pauze, ASIO opnieuw — herstelt het?
//! Draaien: cargo run --release -p vpo-app --features asio --example asio_probe

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn asio_host() -> Option<cpal::Host> {
    cpal::available_hosts()
        .into_iter()
        .find(|h| h.name().eq_ignore_ascii_case("asio"))
        .and_then(|id| cpal::host_from_id(id).ok())
}

fn list_asio(tag: &str) -> Vec<String> {
    let Some(host) = asio_host() else {
        println!("[{tag}] ASIO-host niet beschikbaar (feature ontbreekt?)");
        return vec![];
    };
    let names: Vec<String> = host
        .output_devices()
        .map(|devs| devs.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default();
    println!("[{tag}] ASIO-apparaten: {:?}", names);
    names
}

fn try_asio_stream(tag: &str) {
    let Some(host) = asio_host() else { return };
    let Some(device) = host.output_devices().ok().and_then(|mut d| d.next()) else {
        println!("[{tag}] geen ASIO-apparaat om stream op te bouwen");
        return;
    };
    let name = device.name().unwrap_or_default();
    let cfg = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            println!("[{tag}] default_output_config faalde op '{name}': {e}");
            return;
        }
    };
    println!(
        "[{tag}] stream bouwen op '{}': {} Hz, {} kanalen, {:?}",
        name,
        cfg.sample_rate().0,
        cfg.channels(),
        cfg.sample_format()
    );
    let callbacks = Arc::new(AtomicU64::new(0));
    let cb = callbacks.clone();
    let stream = device.build_output_stream(
        &cfg.config(),
        move |data: &mut [f32], _| {
            cb.fetch_add(1, Ordering::Relaxed);
            for s in data.iter_mut() {
                *s = 0.0;
            }
        },
        move |e| println!("    stream-fout: {e}"),
        None,
    );
    match stream {
        Ok(s) => {
            if let Err(e) = s.play() {
                println!("[{tag}] play() faalde: {e}");
                return;
            }
            std::thread::sleep(Duration::from_millis(3000));
            let n = callbacks.load(Ordering::Relaxed);
            println!("[{tag}] callbacks na 3 s: {n} {}", if n > 0 { "— OK, ASIO levert audio" } else { "— STIL (geen callbacks!)" });
        }
        Err(e) => println!("[{tag}] build_output_stream faalde: {e}"),
    }
}

fn main() {
    println!("=== Test 1: koude ASIO (geen andere stream) ===");
    let cold = list_asio("koud");
    if !cold.is_empty() {
        try_asio_stream("koud");
    }

    println!("\n=== Test 2: ASIO terwijl WASAPI-stream draait ===");
    let wasapi = cpal::default_host();
    let dev = wasapi
        .default_output_device()
        .expect("geen WASAPI default device");
    println!(
        "WASAPI-stream starten op '{}'...",
        dev.name().unwrap_or_default()
    );
    let cfg = dev.default_output_config().expect("wasapi config");
    let wstream = dev
        .build_output_stream(
            &cfg.config(),
            move |data: &mut [f32], _| {
                for s in data.iter_mut() {
                    *s = 0.0;
                }
            },
            move |e| println!("    wasapi-fout: {e}"),
            None,
        )
        .expect("wasapi stream bouwen");
    wstream.play().expect("wasapi play");
    std::thread::sleep(Duration::from_millis(500));
    let warm = list_asio("warm");
    if !warm.is_empty() {
        try_asio_stream("warm");
    }

    println!("\n=== Test 3: WASAPI dicht, 400 ms wachten, ASIO opnieuw ===");
    drop(wstream);
    std::thread::sleep(Duration::from_millis(400));
    let after = list_asio("na-sluiten");
    if !after.is_empty() {
        try_asio_stream("na-sluiten");
    }

    println!("\nKlaar.");
}
