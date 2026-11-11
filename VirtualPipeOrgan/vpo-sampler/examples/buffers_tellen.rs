//! Controle "één bestand, één buffer" (taak F2, deel 2): hoeveel preload-buffers
//! telt de geheugenschatting van de app, en hoeveel unieke bestanden zijn dat
//! werkelijk?
//!
//! De app laadt elke unieke (bestand, segment) één keer en deelt de buffer over alle
//! pijp-sleutels (zie `commands.rs`, "GrandOrgue-stops delen vaak één rank"). De
//! schatting vooraf (`preload_buffers_per_label`) telt daarentegen per pijp: een
//! `Reference` telt als 1 buffer, ook al deelt zij het bestand van de pijp waarnaar
//! ze verwijst. Dit programma zet beide naast elkaar.
//!
//! Gebruik:
//!
//! ```text
//! cargo run --release -p vpo-sampler --example buffers_tellen -- <odf>... [--uit rapport.md]
//! ```
//!
//! `<odf>` is een GrandOrgue-`.organ` of een Hauptwerk-`.Organ_Hauptwerk_xml`.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use vpo_sampler::grandorgue::{OdfParser, OrganDefinition, PipeDef};
use vpo_sampler::hauptwerk::load_hauptwerk_met_verslag;

struct Uitkomst {
    sleutels_schatting: usize,
    referenties: usize,
    unieke_bestanden: usize,
    unieke_segmenten: usize,
}

fn tel(def: &OrganDefinition) -> Uitkomst {
    let mut sleutels = 0usize;
    let mut referenties = 0usize;
    let mut bestanden: HashSet<PathBuf> = HashSet::new();
    let mut segmenten: HashSet<(PathBuf, Option<u32>, bool)> = HashSet::new();
    for s in def.stops.iter().filter(|s| !s.is_noise_or_mechanical()) {
        for (_idx, _persp, pipes) in s.all_layers() {
            for p in pipes {
                match p {
                    PipeDef::Sample { path, extra } => {
                        // Zelfde formule als de schatting in de app.
                        sleutels += 1
                            + extra.releases.len()
                            + extra.tremulant_attack.as_ref().map_or(0, |_| 1 + extra.tremulant_releases.len());
                        bestanden.insert(path.clone());
                        segmenten.insert((path.clone(), None, false));
                        for r in &extra.releases {
                            bestanden.insert(r.path.clone());
                            segmenten.insert((r.path.clone(), r.cue_point, true));
                        }
                        if let Some(t) = &extra.tremulant_attack {
                            bestanden.insert(t.path.clone());
                            segmenten.insert((t.path.clone(), None, false));
                            for r in &extra.tremulant_releases {
                                bestanden.insert(r.path.clone());
                                segmenten.insert((r.path.clone(), r.cue_point, true));
                            }
                        }
                    }
                    PipeDef::Reference { .. } => {
                        sleutels += 1;
                        referenties += 1;
                    }
                    PipeDef::Empty => {}
                }
            }
        }
    }
    Uitkomst { sleutels_schatting: sleutels, referenties, unieke_bestanden: bestanden.len(), unieke_segmenten: segmenten.len() }
}

fn laad(pad: &Path) -> Result<OrganDefinition, String> {
    let naam = pad.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if naam.ends_with(".organ") {
        OdfParser::parse(pad).map_err(|e| e.to_string())
    } else {
        load_hauptwerk_met_verslag(pad, None).0.map_err(|e| e.to_string())
    }
}

fn main() {
    let mut paden: Vec<PathBuf> = Vec::new();
    let mut uit: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--uit" {
            uit = args.next().map(PathBuf::from);
        } else {
            paden.push(PathBuf::from(a));
        }
    }
    if paden.is_empty() {
        eprintln!("gebruik: buffers_tellen <odf>... [--uit rapport.md]");
        std::process::exit(2);
    }
    let mut rapport = String::new();
    let _ = writeln!(rapport, "| definitie | telling in de app (buffers) | waarvan Reference | unieke bestanden | unieke (bestand, segment) | te hoog geschat |");
    let _ = writeln!(rapport, "|---|---:|---:|---:|---:|---:|");
    for p in &paden {
        let naam = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        match laad(p) {
            Ok(def) => {
                let u = tel(&def);
                let factor = if u.unieke_segmenten > 0 { u.sleutels_schatting as f64 / u.unieke_segmenten as f64 } else { 0.0 };
                let _ = writeln!(
                    rapport,
                    "| {} | {} | {} | {} | {} | ×{:.2} |",
                    naam, u.sleutels_schatting, u.referenties, u.unieke_bestanden, u.unieke_segmenten, factor
                );
                eprintln!("{naam}: {} geschat, {} unieke segmenten", u.sleutels_schatting, u.unieke_segmenten);
            }
            Err(e) => {
                let _ = writeln!(rapport, "| {} | laden mislukt: {} | | | | |", naam, e);
            }
        }
    }
    match uit {
        Some(p) => std::fs::write(p, rapport).expect("rapport schrijven"),
        None => println!("{rapport}"),
    }
}
