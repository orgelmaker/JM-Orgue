//! Toont per Hauptwerk-definitie het windwerk van de set zoals het importverslag het meldt (taak D4, fase 0), zonder
//! iets af te spelen.
//!
//! Gebruik:
//!
//! ```text
//! cargo run --release -p vpo-sampler --example windwerk_tonen -- <definitie>...
//! ```

use std::path::Path;

use vpo_sampler::grandorgue::PipeDef;
use vpo_sampler::hauptwerk::load_hauptwerk_met_verslag;
use vpo_sampler::windwerk::PijpWindIn;

fn main() {
    let paden: Vec<String> = std::env::args().skip(1).collect();
    if paden.is_empty() {
        eprintln!("gebruik: windwerk_tonen <definitie>...");
        std::process::exit(2);
    }
    for p in &paden {
        let (res, v) = load_hauptwerk_met_verslag(Path::new(p), None);
        let naam = Path::new(p).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        println!("\n## {}", naam);
        let Ok(def) = res else {
            println!("- niet geladen");
            continue;
        };
        match &v.windwerk {
            None => println!("- geen windwerk gelezen (gecomprimeerd of niet aanwezig)"),
            Some(w) => {
                for m in v.meldingen.iter().filter(|m| m.object == "windwerk") {
                    println!("- {}", m.tekst);
                }
                // Verbruik per balg (kg/s) bij alle registers aan en alle toetsen tegelijk, tegenover de toevoer.
                let mut per_balg: std::collections::BTreeMap<u32, f32> = Default::default();
                for s in &def.stops {
                    for pd in &s.pipes {
                        if let PipeDef::Sample { extra, .. } = pd {
                            if let (Some(l), Some(q)) = (extra.wind_lade, extra.wind_verbruik_kg_s) {
                                if let Some(b) = w.balg_van_lade(l) {
                                    *per_balg.entry(b).or_default() += q;
                                }
                            }
                        }
                    }
                }
                for (b, q) in per_balg {
                    let naam = w.lade(b).map(|l| l.naam.trim().to_string()).unwrap_or_default();
                    println!("  · balg \"{}\": alle pijpen samen {:.3} kg/s, toevoer {:.3} kg/s", naam, q, w.toevoer_kg_s(b));
                }
                // Fase 1: afgeleide groepen en factoren (divisie = manuaal hier; de schatting doet voor de groepen en
                // factoren niet ter zake en staat op 1).
                let mut pijpen = Vec::new();
                for (d, m) in def.manuals.iter().enumerate() {
                    for sid in &m.stop_ids {
                        let Some(s) = def.stops.iter().find(|s| s.id == *sid) else { continue };
                        for (i, pd) in s.pipes.iter().enumerate() {
                            let Some(PipeDef::Sample { extra, .. }) = def.resolve_reference(pd) else { continue };
                            pijpen.push(PijpWindIn {
                                sleutel: (s.id, i as u32 + 1), divisie: d, lade: extra.wind_lade,
                                kg_s: extra.wind_verbruik_kg_s, schatting: 1.0,
                            });
                        }
                    }
                }
                let namen: Vec<String> = def.manuals.iter().map(|m| m.name.clone()).collect();
                match w.afleiden(&pijpen, def.manuals.len()) {
                    None => println!("  · fase 1: gewone indeling (geen pijpen aan een balg, of te veel groepen)"),
                    Some(a) => for r in a.regels(&namen) {
                        println!("  · fase 1: {}", r);
                    },
                }
            }
        }
    }
}
