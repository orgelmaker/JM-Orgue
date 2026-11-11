//! Toont per Hauptwerk-definitie de zwelkastwaarden van de set zoals het
//! importverslag ze meldt (taak E5), zonder iets af te spelen.
//!
//! Gebruik:
//!
//! ```text
//! cargo run --release -p vpo-sampler --example zwelkast_tonen -- <definitie>...
//! ```

use std::path::Path;

use vpo_sampler::hauptwerk::load_hauptwerk_met_verslag;

fn main() {
    let paden: Vec<String> = std::env::args().skip(1).collect();
    if paden.is_empty() {
        eprintln!("gebruik: zwelkast_tonen <definitie>...");
        std::process::exit(2);
    }
    for p in &paden {
        let (res, v) = load_hauptwerk_met_verslag(Path::new(p), None);
        let naam = Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let kasten: Vec<_> = v.meldingen.iter().filter(|m| m.object == "zwelkast").collect();
        let aantal_def = res.as_ref().map(|d| d.enclosures.len()).unwrap_or(0);
        println!("\n## {} ({}; {} zwelkasten in de definitie, {} met filterwaarden)", naam, if res.is_ok() { "geladen" } else { "niet geladen" }, aantal_def, kasten.len());
        for m in kasten {
            println!("- {}", m.tekst);
        }
    }
}
