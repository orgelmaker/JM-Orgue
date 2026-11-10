//! Importdiagnose (taak E8): laad Hauptwerk-definities met de diagnose aan en
//! toon per set de ongelezen velden, de onbekende tabellen en de controlelijsten.
//! Daarna een samenvatting over alle sets: welk `Tabel.Veld` blijft in de meeste
//! sets ongelezen en hoeveel rijen dat zijn. Die samenvatting rangschikt het
//! vervolgwerk (E1 t/m E6).
//!
//! Gebruik:
//!
//! ```text
//! cargo run --release -p vpo-sampler --example import_diagnose -- <bestand of map>... [--uit rapport.md] [--top 40]
//! ```
//!
//! Een map wordt doorzocht op `*.Organ_Hauptwerk_xml` en `*.CustomOrgan_Hauptwerk_xml`.
//! Zonder `--uit` gaat het rapport naar stdout.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use vpo_sampler::hauptwerk::load_hauptwerk_met_verslag;
use vpo_sampler::import_diagnose;

fn is_odf(p: &Path) -> bool {
    let naam = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    naam.ends_with(".organ_hauptwerk_xml") || naam.ends_with(".customorgan_hauptwerk_xml")
}

fn zoek(pad: &Path, diepte: usize, uit: &mut Vec<PathBuf>) {
    if pad.is_file() {
        uit.push(pad.to_path_buf());
        return;
    }
    if diepte > 6 {
        return;
    }
    let Ok(lees) = std::fs::read_dir(pad) else { return };
    let mut items: Vec<PathBuf> = lees.flatten().map(|e| e.path()).collect();
    items.sort();
    for p in items {
        if p.is_dir() {
            zoek(&p, diepte + 1, uit);
        } else if is_odf(&p) {
            uit.push(p);
        }
    }
}

#[derive(Default)]
struct Optelling {
    sets: usize,
    rijen: usize,
    voorbeeld: String,
}

fn main() {
    let mut invoer: Vec<String> = Vec::new();
    let mut uit_pad: Option<String> = None;
    let mut top = 40usize;
    let mut zonder_diagnose = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--uit" => uit_pad = args.next(),
            "--top" => top = args.next().and_then(|t| t.parse().ok()).unwrap_or(40),
            // Alleen laadtijd meten met de diagnose uit (gewone laadpad).
            "--zonder-diagnose" => zonder_diagnose = true,
            _ => invoer.push(a),
        }
    }
    if invoer.is_empty() {
        eprintln!("Gebruik: import_diagnose <bestand of map>... [--uit rapport.md] [--top N]");
        std::process::exit(2);
    }
    let mut bestanden = Vec::new();
    for i in &invoer {
        zoek(Path::new(i), 0, &mut bestanden);
    }
    if zonder_diagnose {
        for odf in &bestanden {
            let t0 = Instant::now();
            let (res, v) = load_hauptwerk_met_verslag(odf, None);
            println!(
                "{:.2} s  {}  ({}, diagnose: {})",
                t0.elapsed().as_secs_f32(),
                odf.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                if res.is_ok() { "gelukt" } else { "MISLUKT" },
                if v.diagnose.is_some() { "aan" } else { "uit" }
            );
        }
        return;
    }
    import_diagnose::zet_aan(true);

    let mut rapport = String::new();
    let mut ongelezen: BTreeMap<String, Optelling> = BTreeMap::new();
    let mut onbekend: BTreeMap<String, Optelling> = BTreeMap::new();
    let mut lijsten: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
    let mut gelukt = 0usize;

    for odf in &bestanden {
        let t0 = Instant::now();
        let (res, v) = load_hauptwerk_met_verslag(odf, None);
        let naam = odf.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let _ = writeln!(rapport, "\n## {naam}\n");
        let _ = writeln!(rapport, "- pad: `{}`", odf.display());
        let _ = writeln!(
            rapport,
            "- formaat: {} {}; laden: {}; {:.1} s",
            v.formaat,
            v.versie,
            if res.is_ok() { "gelukt" } else { "MISLUKT" },
            t0.elapsed().as_secs_f32()
        );
        if let Err(e) = &res {
            let _ = writeln!(rapport, "- fout: {e}");
        }
        let _ = writeln!(
            rapport,
            "- geladen: {} stops, {} koppels",
            v.geladen.stops, v.geladen.koppels
        );
        let Some(d) = &v.diagnose else {
            let _ = writeln!(rapport, "- geen diagnose (definitie niet leesbaar)");
            continue;
        };
        if res.is_ok() {
            gelukt += 1;
        }

        let _ = writeln!(rapport, "\n### Ongelezen velden ({} Tabel.Veld)\n", d.ongelezen_velden.len());
        let _ = writeln!(rapport, "| Tabel.Veld | rijen | ongelezen | verschillend | voorbeelden |");
        let _ = writeln!(rapport, "|---|---:|---:|---:|---|");
        for o in d.ongelezen_velden.iter().take(top) {
            let _ = writeln!(
                rapport,
                "| {}.{} | {} | {} | {} | {} |",
                o.tabel,
                o.veld,
                o.rijen,
                o.ongelezen,
                o.verschillend,
                o.voorbeelden.join(" · ").replace('|', "/")
            );
        }
        if d.ongelezen_velden.len() > top {
            let _ = writeln!(rapport, "\n(nog {} velden niet getoond)", d.ongelezen_velden.len() - top);
        }
        for o in &d.ongelezen_velden {
            let e = ongelezen.entry(format!("{}.{}", o.tabel, o.veld)).or_default();
            e.sets += 1;
            e.rijen += o.ongelezen;
            if e.voorbeeld.is_empty() {
                e.voorbeeld = o.voorbeelden.join(" · ");
            }
        }

        let _ = writeln!(rapport, "\n### Onbekende tabellen ({})\n", d.onbekende_tabellen.len());
        for t in &d.onbekende_tabellen {
            let _ = writeln!(rapport, "- {} ({} rijen)", t.tabel, t.rijen);
            let e = onbekend.entry(t.tabel.clone()).or_default();
            e.sets += 1;
            e.rijen += t.rijen;
        }

        let _ = writeln!(rapport, "\n### Controlelijsten\n");
        for l in &d.lijsten {
            let _ = writeln!(rapport, "- **{}**: {} (gecontroleerd: {})", l.soort, l.totaal, l.gecontroleerd);
            let e = lijsten.entry(l.soort.clone()).or_default();
            if l.totaal > 0 {
                e.0 += 1;
            }
            e.1 += l.totaal;
            if l.gecontroleerd > 0 {
                e.2 += 1;
            }
            for i in l.items.iter().take(8) {
                let _ = writeln!(rapport, "  - {i}");
            }
            if l.totaal > 8 {
                let _ = writeln!(rapport, "  - … ({} meer)", l.totaal - 8);
            }
        }
    }

    // Samenvatting over alle sets.
    let _ = writeln!(rapport, "\n# Samenvatting over {} sets ({} geladen)\n", bestanden.len(), gelukt);
    let mut rang: Vec<(&String, &Optelling)> = ongelezen.iter().collect();
    rang.sort_by(|a, b| b.1.sets.cmp(&a.1.sets).then(b.1.rijen.cmp(&a.1.rijen)));
    let _ = writeln!(rapport, "## Ongelezen velden, meeste sets eerst\n");
    let _ = writeln!(rapport, "| Tabel.Veld | sets | ongelezen rijen (totaal) | voorbeeld |");
    let _ = writeln!(rapport, "|---|---:|---:|---|");
    for (k, o) in rang.iter().take(top * 2) {
        let _ = writeln!(rapport, "| {k} | {} | {} | {} |", o.sets, o.rijen, o.voorbeeld.replace('|', "/"));
    }
    let _ = writeln!(rapport, "\n## Onbekende tabellen\n");
    let mut ob: Vec<(&String, &Optelling)> = onbekend.iter().collect();
    ob.sort_by(|a, b| b.1.sets.cmp(&a.1.sets).then(b.1.rijen.cmp(&a.1.rijen)));
    for (k, o) in ob {
        let _ = writeln!(rapport, "- {k}: {} sets, {} rijen", o.sets, o.rijen);
    }
    let _ = writeln!(rapport, "\n## Controlelijsten\n");
    for (soort, (sets, totaal, bekeken)) in &lijsten {
        let _ = writeln!(rapport, "- {soort}: {sets} sets met bevindingen, {totaal} bevindingen in totaal; bekeken bij {bekeken} sets");
    }

    match uit_pad {
        Some(p) => {
            std::fs::write(&p, &rapport).expect("rapport schrijven");
            eprintln!("Rapport geschreven naar {p}");
        }
        None => print!("{rapport}"),
    }
}
