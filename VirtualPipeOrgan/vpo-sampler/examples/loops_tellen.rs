//! Telt per sampleset hoeveel opnames meer dan één geldige loop hebben (taak C1).
//!
//! De engine speelt één loop per pijp (de langste). Voordat er iets gebouwd wordt
//! om uit meer loops te kiezen, wil je weten hoeveel pijpen dat überhaupt kunnen.
//! Dit programma leest van elk WAV-bestand alleen de `smpl`-chunk (via
//! `read_wav_markers`, dus met dezelfde validatie als de engine) en telt.
//!
//! Gebruik:
//!
//! ```text
//! cargo run --release -p vpo-sampler --example loops_tellen -- <map>... [--uit rapport.md]
//! ```
//!
//! Elke opgegeven map telt als één set; mappen met WAV-bestanden diep in de
//! boom worden helemaal doorzocht. WavPack-bestanden (`.wv`) hebben geen
//! `smpl`-chunk die hier gelezen wordt en worden alleen geteld als "niet gelezen".

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use vpo_sampler::read_wav_markers;

#[derive(Default)]
struct Telling {
    /// aantal wav-bestanden in de map, en hoeveel daarvan gelezen zijn (steekproef)
    totaal_wav: u64,
    wav: u64,
    wv: u64,
    onleesbaar: u64,
    zonder_loop: u64,
    /// aantal loops → aantal bestanden
    verdeling: BTreeMap<usize, u64>,
    /// bestanden met minstens twee loops waarvan er twee elkaar overlappen
    met_overlap: u64,
    /// bestanden met minstens twee loops die precies even lang zijn
    gelijke_lengte: u64,
    /// langste loop / kortste loop bij bestanden met meer dan één loop (som en teller)
    verhouding_som: f64,
    verhouding_n: u64,
    voorbeelden: Vec<String>,
}

fn zoek(pad: &Path, uit: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(pad) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            zoek(&p, uit);
        } else if p.is_file() {
            uit.push(p);
        }
    }
}

fn overlappen(loops: &[(u64, u64)]) -> bool {
    for i in 0..loops.len() {
        for j in (i + 1)..loops.len() {
            if loops[i].0 < loops[j].1 && loops[j].0 < loops[i].1 {
                return true;
            }
        }
    }
    false
}

fn tel(map: &Path, max: usize) -> Telling {
    let mut bestanden = Vec::new();
    zoek(map, &mut bestanden);
    let mut t = Telling::default();
    // Steekproef: gelijkmatig verdeeld over de (gesorteerde) lijst, zodat alle
    // registers en toonhoogtes meetellen. Het lezen van duizenden bestanden op een
    // gewone schijf duurt anders uren.
    bestanden.sort();
    t.wv = bestanden.iter().filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wv"))).count() as u64;
    bestanden.retain(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")));
    let totaal_wav = bestanden.len();
    if max > 0 && totaal_wav > max {
        let stap = totaal_wav as f64 / max as f64;
        bestanden = (0..max).map(|i| bestanden[(i as f64 * stap) as usize].clone()).collect();
    }
    t.totaal_wav = totaal_wav as u64;
    for p in bestanden {
        t.wav += 1;
        let Some(m) = read_wav_markers(&p) else {
            t.onleesbaar += 1;
            continue;
        };
        let n = m.loops.len();
        if n == 0 {
            t.zonder_loop += 1;
            continue;
        }
        *t.verdeling.entry(n).or_insert(0) += 1;
        if n >= 2 {
            if overlappen(&m.loops) {
                t.met_overlap += 1;
            }
            let lengtes: Vec<u64> = m.loops.iter().map(|(a, b)| b - a).collect();
            let mut gesorteerd = lengtes.clone();
            gesorteerd.sort_unstable();
            if gesorteerd.windows(2).any(|w| w[0] == w[1]) {
                t.gelijke_lengte += 1;
            }
            let kort = *gesorteerd.first().unwrap() as f64;
            let lang = *gesorteerd.last().unwrap() as f64;
            if kort > 0.0 {
                t.verhouding_som += lang / kort;
                t.verhouding_n += 1;
            }
            if t.voorbeelden.len() < 3 {
                t.voorbeelden.push(format!("{} ({} loops)", p.file_name().unwrap().to_string_lossy(), n));
            }
        }
    }
    t
}

fn main() {
    let mut mappen: Vec<PathBuf> = Vec::new();
    let mut uit: Option<PathBuf> = None;
    let mut max: usize = 300;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--uit" {
            uit = args.next().map(PathBuf::from);
        } else if a == "--max" {
            max = args.next().and_then(|v| v.parse().ok()).unwrap_or(300);
        } else {
            mappen.push(PathBuf::from(a));
        }
    }
    if mappen.is_empty() {
        eprintln!("gebruik: loops_tellen <map>... [--uit rapport.md]");
        std::process::exit(2);
    }
    let mut rapport = String::new();
    let _ = writeln!(rapport, "| set | wav in map | gelezen | zonder loop | 1 loop | 2 loops | 3+ loops | met overlap | gelijke lengte | gem. lang/kort | .wv |");
    let _ = writeln!(rapport, "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    let (mut s_wav, mut s_een, mut s_meer) = (0u64, 0u64, 0u64);
    let mut details = String::new();
    for m in &mappen {
        let t = tel(m, max);
        let een = t.verdeling.get(&1).copied().unwrap_or(0);
        let twee = t.verdeling.get(&2).copied().unwrap_or(0);
        let drie_plus: u64 = t.verdeling.iter().filter(|(k, _)| **k >= 3).map(|(_, v)| *v).sum();
        let naam = m.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let gem = if t.verhouding_n > 0 { format!("{:.2}", t.verhouding_som / t.verhouding_n as f64) } else { "-".to_string() };
        let _ = writeln!(
            rapport,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            naam, t.totaal_wav, t.wav, t.zonder_loop, een, twee, drie_plus, t.met_overlap, t.gelijke_lengte, gem, t.wv
        );
        s_wav += t.wav;
        s_een += een;
        s_meer += twee + drie_plus;
        if !t.voorbeelden.is_empty() {
            let _ = writeln!(details, "- {}: {}", naam, t.voorbeelden.join(", "));
        }
        eprintln!("{}: {} gelezen van {}, {} met meer dan één loop", naam, t.wav, t.totaal_wav, twee + drie_plus);
    }
    let _ = writeln!(rapport, "\nTotaal (steekproef van ten hoogste {} per set): {} gelezen, {} met één loop, {} met meer dan één loop ({:.2} %).", max,
        s_wav, s_een, s_meer, if s_wav > 0 { 100.0 * s_meer as f64 / s_wav as f64 } else { 0.0 });
    if !details.is_empty() {
        let _ = writeln!(rapport, "\nVoorbeelden van bestanden met meer dan één loop:\n{}", details);
    }
    match uit {
        Some(p) => std::fs::write(p, rapport).expect("rapport schrijven"),
        None => println!("{}", rapport),
    }
}
