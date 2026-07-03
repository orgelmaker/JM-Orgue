//! Test loading a GrandOrgue sample set
//!
//! Usage: cargo run --example test_grandorgue -- <path-to-.organ-file>

use std::path::Path;
use std::time::Instant;
use vpo_sampler::{OrganDefinition, GrandOrgueLoader, SampleCache, load_grandorgue_organ};
use std::sync::Arc;

fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Get ODF path from command line
    let args: Vec<String> = std::env::args().collect();
    let odf_path = if args.len() > 1 {
        &args[1]
    } else {
        // Default test path
        r"C:\Bronbestanden\JM-Orgue\testen\Burea_Funeral_Chapel\burea_gravkapell.organ"
    };

    println!("=== GrandOrgue Loader Test ===\n");
    println!("Loading: {}\n", odf_path);

    // First, just parse the ODF without loading samples
    println!("--- Parsing ODF ---");
    let parse_start = Instant::now();

    match OrganDefinition::load(Path::new(odf_path)) {
        Ok(definition) => {
            let parse_time = parse_start.elapsed();
            println!("Parse time: {:?}\n", parse_time);

            // Print organ info
            println!("Organ: {}", definition.organ.church_name);
            println!("Builder: {}", definition.organ.organ_builder);
            println!("Built: {}", definition.organ.organ_build_date);
            println!("Recording: {}", definition.organ.recording_details);
            println!();

            // Print manuals
            println!("--- Manuals ({}) ---", definition.manuals.len());
            for manual in &definition.manuals {
                println!("  Manual {:03}: {} ({} keys, MIDI ch {})",
                    manual.number,
                    manual.name,
                    manual.number_of_accessible_keys,
                    manual.midi_input_number
                );
                println!("    Stops: {:?}", manual.stop_ids);
            }
            println!();

            // Print stops
            println!("--- Stops ({}) ---", definition.stops.len());
            for stop in &definition.stops {
                let footage = OrganDefinition::harmonic_to_footage(stop.harmonic_number);
                println!("  Stop {:03}: {} {} ({} pipes, amp: {}%)",
                    stop.id,
                    stop.name,
                    footage,
                    stop.number_of_pipes,
                    stop.amplitude_level
                );
            }
            println!();

            // Print tremulants
            if !definition.tremulants.is_empty() {
                println!("--- Tremulants ({}) ---", definition.tremulants.len());
                for trem in &definition.tremulants {
                    println!("  {}: period={}ms, depth={}%",
                        trem.name, trem.period, trem.amp_mod_depth);
                }
                println!();
            }

            // Print enclosures
            if !definition.enclosures.is_empty() {
                println!("--- Enclosures ({}) ---", definition.enclosures.len());
                for enc in &definition.enclosures {
                    println!("  {}: min_level={}%", enc.name, enc.amp_minimum_level);
                }
                println!();
            }

            // Get sample paths
            let sample_paths = definition.get_sample_paths();
            println!("Total unique sample files: {}", sample_paths.len());
            println!();

            // Now load with samples
            println!("--- Loading Samples ---");
            let load_start = Instant::now();

            let sample_rate = 48000;
            let cache = Arc::new(SampleCache::new("", sample_rate));
            let loader = GrandOrgueLoader::new(cache, sample_rate);

            let progress_fn = Box::new(|current: usize, total: usize, name: &str| {
                if current % 50 == 0 || current == total {
                    println!("  [{}/{}] {}", current, total, name);
                }
            });

            match loader.load_with_progress(Path::new(odf_path), Some(progress_fn)) {
                Ok(organ) => {
                    let load_time = load_start.elapsed();
                    println!();
                    println!("Load time: {:?}", load_time);
                    println!("Samples loaded: {}", organ.sample_count());
                    println!("Memory usage: {:.2} MB", organ.memory_usage() as f64 / 1024.0 / 1024.0);
                    println!();
                    println!("=== Success! ===");
                }
                Err(e) => {
                    eprintln!("Failed to load samples: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to parse ODF: {}", e);
        }
    }
}
