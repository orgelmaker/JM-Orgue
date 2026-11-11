//! Toont per Hauptwerk-definitie de opgenomen tremulant-golfvormen zoals het importverslag ze meldt
//! (taak C5, stap 1), zonder iets af te spelen.
//!
//! Gebruik:
//!
//! ```text
//! cargo run --release -p vpo-sampler --example golfvormen_tonen -- <definitie>...
//! ```

use std::path::Path;

use vpo_sampler::hauptwerk::load_hauptwerk_met_verslag;

fn main() {
    let paden: Vec<String> = std::env::args().skip(1).collect();
    if paden.is_empty() {
        eprintln!("gebruik: golfvormen_tonen <definitie>...");
        std::process::exit(2);
    }
    for p in &paden {
        let (res, v) = load_hauptwerk_met_verslag(Path::new(p), None);
        let naam = Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        println!("\n## {} ({})", naam, if res.is_ok() { "geladen" } else { "niet geladen" });
        let meldingen: Vec<_> = v.meldingen.iter().filter(|m| m.object == "tremulantgolfvorm").collect();
        if v.tremulant_golfvormen.is_empty() && meldingen.is_empty() {
            println!("- geen opgenomen tremulant-golfvormen");
            continue;
        }
        let totaal_pijpen: usize = v.tremulant_golfvormen.iter().map(|g| g.pijpen).sum();
        println!("- {} golfvormen gelezen, samen voor {} pijpen", v.tremulant_golfvormen.len(), totaal_pijpen);
        for g in v.tremulant_golfvormen.iter().take(4) {
            println!(
                "  · {} \"{}\": {} pijpen; amplitude {:?} dB, toonhoogte {:?} %, filter {:?} dB; {} | {}",
                g.tremulant_id, g.tremulant_naam, g.pijpen, g.amplitude_db, g.toonhoogte_procent, g.harmonisch_filter_db,
                g.grondtoon_bestand, g.harmonische_bestand
            );
        }
        if v.tremulant_golfvormen.len() > 4 {
            println!("  · ... en {} meer", v.tremulant_golfvormen.len() - 4);
        }
        for m in meldingen.iter().take(2) {
            println!("- melding: {}", m.tekst);
        }
    }
}
