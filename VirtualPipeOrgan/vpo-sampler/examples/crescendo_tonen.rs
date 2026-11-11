//! Toont per Hauptwerk-definitie de crescendo van de set zoals het importverslag
//! ze meldt (taak E4), zonder iets af te spelen.
//!
//! Gebruik:
//!
//! ```text
//! cargo run --release -p vpo-sampler --example crescendo_tonen -- <definitie>...
//! ```

use std::path::Path;

use vpo_sampler::hauptwerk::load_hauptwerk_met_verslag;

fn main() {
    let paden: Vec<String> = std::env::args().skip(1).collect();
    if paden.is_empty() {
        eprintln!("gebruik: crescendo_tonen <definitie>...");
        std::process::exit(2);
    }
    for p in &paden {
        let (res, v) = load_hauptwerk_met_verslag(Path::new(p), None);
        let naam = Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let aantal_registers = res.as_ref().map(|d| d.stops.len()).unwrap_or(0);
        println!("\n## {} ({}; {} registers)", naam, if res.is_ok() { "geladen" } else { "niet geladen" }, aantal_registers);
        match &v.set_crescendo {
            None => println!("- geen crescendo van de set gelezen"),
            Some(c) => {
                println!("- regelaar \"{}\": {} trappen, {} schakelaars niet gelezen", c.naam, c.trappen.len(), c.niet_gelezen);
                let eerste = c.trappen.first().map(|t| t.stops.len()).unwrap_or(0);
                let laatste = c.trappen.last().map(|t| t.stops.len()).unwrap_or(0);
                println!("- van {} tot {} registers", eerste, laatste);
                // Een trap hoeft niet alleen maar te groeien; tel de trappen die minder registers
                // hebben dan de vorige.
                let terug = c.trappen.windows(2).filter(|w| w[1].stops.len() < w[0].stops.len()).count();
                println!("- {} trappen met minder registers dan de vorige", terug);
                for t in c.trappen.iter().take(3).chain(c.trappen.iter().rev().take(2).rev()) {
                    println!("  · waarde {:>3} \"{}\": {} registers {:?}", t.waarde, t.naam, t.stops.len(), t.stops);
                }
            }
        }
        for m in v.meldingen.iter().filter(|m| m.object == "crescendo") {
            println!("- melding: {}", m.tekst);
        }
    }
}
