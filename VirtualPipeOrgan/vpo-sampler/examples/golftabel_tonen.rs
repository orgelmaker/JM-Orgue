//! Toont per Hauptwerk-definitie de tremulant-golfvormen als modulatietabel (C5, stap 2): aantal cycli in de lus,
//! bereik van toonhoogte en sterkte, en de diepte die een pijp met de mediaanwaarden krijgt.
//!
//! ```text
//! cargo run --release -p vpo-sampler --example golftabel_tonen -- <definitie>...
//! ```
use std::path::Path;
use vpo_sampler::hauptwerk::load_hauptwerk_met_verslag;
use vpo_sampler::tremulant_golf::lees_golftabel;

fn main() {
    for p in std::env::args().skip(1) {
        let (_, v) = load_hauptwerk_met_verslag(Path::new(&p), None);
        println!("\n## {}", Path::new(&p).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
        for g in &v.tremulant_golfvormen {
            let Some(pad) = &g.grondtoon_pad else { println!("- '{}': geen bestand", g.tremulant_naam); continue };
            match lees_golftabel(pad) {
                Err(e) => println!("- '{}': {}", g.tremulant_naam, e),
                Ok(t) => {
                    let (mut pmin, mut pmax, mut amin, mut amax) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                    for q in &t.punten {
                        pmin = pmin.min(q[0]); pmax = pmax.max(q[0]); amin = amin.min(q[1]); amax = amax.max(q[1]);
                    }
                    let kp = g.toonhoogte_procent.unwrap_or(100.0) / 100.0;
                    let ka = 10f32.powf(g.amplitude_db.unwrap_or(0.0) / 20.0);
                    let cent = |x: f32| 1200.0 * (1.0 + x * kp).log2();
                    println!(
                        "- '{}' {:?} Hz: {} cycli in de lus; toonhoogte {:+.4}..{:+.4}, sterkte {:+.4}..{:+.4}; met de mediaan: {:+.1}..{:+.1} cent, sterkte {:+.1}..{:+.1} %",
                        g.tremulant_naam, g.frequentie_hz, t.cycli_in_lus, pmin, pmax, amin, amax,
                        cent(pmin), cent(pmax), amin * ka * 100.0, amax * ka * 100.0
                    );
                }
            }
        }
    }
}
