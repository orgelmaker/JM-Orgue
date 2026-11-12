//! Telt per leesbare Hauptwerk-definitie wat het schakelnetwerk van de set doet tegenover wat JM-Orgue ervan maakt
//! (taak E3). Speelt niets af en verandert niets.
//!
//! JM-Orgue maakt één registerknop per `Stop`, één koppelknop per `KeyAction` met een voorwaarde-schakelaar en één
//! tremulantknop per `Tremulant`. In de set zit daartussen het schakelnetwerk (`SwitchLinkage`): een knop van de
//! speeltafel kan via tussenschakelaars meerdere registers, koppels of tremulanten bedienen, en één koppel van de
//! speeltafel kan uit meerdere `KeyAction`s bestaan. Dit programma telt waar die twee uit elkaar lopen.
//!
//! Gecomprimeerde definities (`Control_FileIsCompacted…` = Y) worden overgeslagen: de veldnamen van het schakelnetwerk
//! zijn daar letters die de importer niet kent.
//!
//! Gebruik:
//!
//! ```text
//! cargo run --release -p vpo-sampler --example schakelnet_tellen -- <definitie>... [--uit rapport.md]
//! ```

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::Path;

use vpo_sampler::hauptwerk::load_hauptwerk_met_verslag;

type Obj = HashMap<String, String>;

/// Objecten van één lijst uit een leesbare definitie: `<ObjectList ObjectType="X"> <X><Veld>waarde</Veld>…</X> …`.
fn objecten(xml: &str, soort: &str) -> Vec<Obj> {
    let kop = format!("<ObjectList ObjectType=\"{}\">", soort);
    let Some(b) = xml.find(&kop) else { return Vec::new() };
    let rest = &xml[b + kop.len()..];
    let e = rest.find("</ObjectList>").unwrap_or(rest.len());
    let lijst = &rest[..e];
    let (open, sluit) = (format!("<{}>", soort), format!("</{}>", soort));
    let mut uit = Vec::new();
    let mut i = 0;
    while let Some(s) = lijst[i..].find(&open) {
        let s = i + s + open.len();
        let Some(t) = lijst[s..].find(&sluit) else { break };
        let inhoud = &lijst[s..s + t];
        let mut o = Obj::new();
        let mut j = 0;
        while let Some(a) = inhoud[j..].find('<') {
            let a = j + a;
            let Some(z) = inhoud[a..].find('>') else { break };
            let naam = &inhoud[a + 1..a + z];
            if naam.starts_with('/') || naam.ends_with('/') {
                j = a + z + 1;
                continue;
            }
            let eind = format!("</{}>", naam);
            let Some(w) = inhoud[a + z + 1..].find(&eind) else { break };
            o.insert(naam.to_string(), inhoud[a + z + 1..a + z + 1 + w].trim().to_string());
            j = a + z + 1 + w + eind.len();
        }
        uit.push(o);
        i = s + t + sluit.len();
    }
    uit
}

fn id(o: &Obj, veld: &str) -> Option<u32> {
    o.get(veld).and_then(|v| v.trim().parse().ok())
}

fn verdeling(objs: &[Obj], max_waarden: usize) -> BTreeMap<String, BTreeMap<String, usize>> {
    let mut v: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for o in objs {
        for (k, w) in o {
            *v.entry(k.clone()).or_default().entry(w.clone()).or_default() += 1;
        }
    }
    v.retain(|_, w| w.len() <= max_waarden);
    v
}

fn main() {
    let mut paden = Vec::new();
    let mut uitbestand = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--uit" {
            uitbestand = args.next();
        } else {
            paden.push(a);
        }
    }
    let mut r = String::new();
    let mut samenvatting = String::new();
    let _ = writeln!(samenvatting, "| set | registers (JM) | via netwerk | knop → >1 register | koppel-KeyActions (JM) | koppels van de set | knop → >1 koppel-KeyAction | tremulanten (JM) | wisselrang-schakelaars | knoppen van de set zonder JM-knop |");
    let _ = writeln!(samenvatting, "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
    for p in &paden {
        let naam = Path::new(p).file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let Ok(xml) = std::fs::read_to_string(p) else {
            let _ = writeln!(r, "\n## {}\n- niet te lezen", naam);
            continue;
        };
        if xml[..xml.len().min(30_000)].contains("FileIsCompacted_AlwaysSetThisToNIfEditingManually>Y") {
            let _ = writeln!(r, "\n## {}\n- gecomprimeerd: overgeslagen", naam);
            continue;
        }
        let switches = objecten(&xml, "Switch");
        let links = objecten(&xml, "SwitchLinkage");
        let stops = objecten(&xml, "Stop");
        let kas = objecten(&xml, "KeyAction");
        let trems = objecten(&xml, "Tremulant");
        let stopranks = objecten(&xml, "StopRank");
        let (def, _) = load_hauptwerk_met_verslag(Path::new(p), None);
        let (jm_stops, jm_kop, jm_trem) = def.as_ref().map(|d| (d.stops.len(), d.couplers.len(), d.tremulants.len())).unwrap_or((0, 0, 0));

        let sw_naam: HashMap<u32, String> = switches.iter().filter_map(|s| Some((id(s, "SwitchID")?, s.get("Name").cloned().unwrap_or_default()))).collect();
        // Netwerk: bron → bestemmingen, en omgekeerd.
        let mut naar: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut van: HashMap<u32, Vec<u32>> = HashMap::new();
        for l in &links {
            if let (Some(a), Some(b)) = (id(l, "SourceSwitchID"), id(l, "DestSwitchID")) {
                naar.entry(a).or_default().push(b);
                van.entry(b).or_default().push(a);
            }
        }
        // Voorouders van een schakelaar (inclusief zichzelf), via de verbindingen terug.
        let voorouders = |s: u32| -> HashSet<u32> {
            let mut gezien = HashSet::new();
            let mut q = VecDeque::from([s]);
            while let Some(x) = q.pop_front() {
                if gezien.insert(x) {
                    if let Some(b) = van.get(&x) {
                        q.extend(b.iter().copied());
                    }
                }
            }
            gezien
        };
        // Wortels van een schakelaar: de knoppen waar hij van afhangt. Een groep schakelaars die elkaar onderling
        // aansturen (een lus, bijvoorbeeld een spiegel op een andere pagina) zonder bron van buiten telt als één knop
        // (de kleinste ID van de groep).
        let wortels = |s: u32| -> HashSet<u32> {
            let alle = voorouders(s);
            let mut uit = HashSet::new();
            for &x in &alle {
                let vx = voorouders(x);
                // x is een top als al zijn voorouders ook van x afstammen (dezelfde lus) of x zelf zijn.
                if vx.iter().all(|&y| y == x || voorouders(y).contains(&x)) {
                    uit.insert(*vx.iter().filter(|&&y| y == x || voorouders(y).contains(&x)).min().unwrap_or(&x));
                }
            }
            uit
        };

        // Registers
        let stop_sw: Vec<(String, u32)> = stops.iter().filter_map(|s| Some((s.get("Name").cloned().unwrap_or_default(), id(s, "ControllingSwitchID")?))).collect();
        let via_netwerk = stop_sw.iter().filter(|(_, sw)| van.contains_key(sw)).count();
        let mut wortel_regs: HashMap<u32, Vec<String>> = HashMap::new();
        for (n, sw) in &stop_sw {
            for w in wortels(*sw) {
                wortel_regs.entry(w).or_default().push(n.clone());
            }
        }
        let meer_regs: Vec<(u32, &Vec<String>)> = wortel_regs.iter().filter(|(_, v)| v.len() > 1).map(|(w, v)| (*w, v)).collect();

        // Koppels: KeyActions met een voorwaarde-schakelaar (wat JM als koppelknop neemt).
        let kop_kas: Vec<&Obj> = kas.iter().filter(|k| k.get("ConditionSwitchID").is_some_and(|c| !c.trim().is_empty())).collect();
        let kop_vw: Vec<u32> = kop_kas.iter().filter_map(|k| id(k, "ConditionSwitchID")).collect();
        let mut per_wortel_kas: HashMap<u32, usize> = HashMap::new();
        for c in &kop_vw {
            for w in wortels(*c) {
                *per_wortel_kas.entry(w).or_default() += 1;
            }
        }
        let koppels_set = per_wortel_kas.len();
        let meer_kas: Vec<(u32, usize)> = per_wortel_kas.iter().filter(|(_, n)| **n > 1).map(|(w, n)| (*w, *n)).collect();

        // Tremulanten en wisselrangen
        let trem_sw: Vec<u32> = trems.iter().filter_map(|t| id(t, "ControllingSwitchID")).collect();
        let wissel: HashSet<u32> = stopranks.iter().filter_map(|s| id(s, "SwitchIDToSwitchToAlternateRank")).collect();

        // Knoppen van de set (wortels die iets bedienen wat JM als knop heeft) die zelf geen JM-knop zijn.
        let jm_knoppen: HashSet<u32> = stop_sw.iter().map(|(_, s)| *s).chain(kop_vw.iter().copied()).chain(trem_sw.iter().copied()).collect();
        let mut wortels_met_doel: HashSet<u32> = HashSet::new();
        for s in &jm_knoppen {
            wortels_met_doel.extend(wortels(*s));
        }
        let zonder_knop: Vec<u32> = wortels_met_doel.iter().copied().filter(|w| !jm_knoppen.contains(w)).collect();

        let _ = writeln!(samenvatting, "| {} | {} ({}) | {} | {} | {} ({}) | {} | {} | {} ({}) | {} | {} |",
            naam, stops.len(), jm_stops, via_netwerk, meer_regs.len(), kop_kas.len(), jm_kop, koppels_set, meer_kas.len(),
            trems.len(), jm_trem, wissel.len(), zonder_knop.len());

        let _ = writeln!(r, "\n## {}\n", naam);
        let _ = writeln!(r, "- {} schakelaars, {} verbindingen ({} met voorwaarde)", switches.len(), links.len(),
            links.iter().filter(|l| l.get("ConditionSwitchID").is_some_and(|c| !c.trim().is_empty())).count());
        let _ = writeln!(r, "- registers: {} in de set, {} in JM-Orgue; {} via het netwerk; {} knoppen bedienen meer dan één register",
            stops.len(), jm_stops, via_netwerk, meer_regs.len());
        let mut mr = meer_regs.clone();
        mr.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        for (w, v) in mr.iter().take(6) {
            let _ = writeln!(r, "  · knop {} \"{}\" → {} registers: {}", w, sw_naam.get(w).cloned().unwrap_or_default(), v.len(),
                v.iter().take(6).cloned().collect::<Vec<_>>().join(", "));
        }
        let _ = writeln!(r, "- koppels: {} KeyActions met voorwaarde (JM-Orgue: {} koppelknoppen), {} knoppen van de set; {} knoppen bedienen meer dan één KeyAction",
            kop_kas.len(), jm_kop, koppels_set, meer_kas.len());
        let mut mk = meer_kas.clone();
        mk.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (w, n) in mk.iter().take(6) {
            let _ = writeln!(r, "  · knop {} \"{}\" → {} KeyActions", w, sw_naam.get(w).cloned().unwrap_or_default(), n);
        }
        let _ = writeln!(r, "- tremulanten: {} in de set, {} in JM-Orgue; wisselrang-schakelaars: {}", trems.len(), jm_trem, wissel.len());
        // Tremulant → divisie volgens de set: de pijpen in TremulantWaveformPipe, via rang → register → divisie. (De
        // importer gebruikt Tremulant-objecten nu alleen om tremulantruis aan een divisie te koppelen; zie C5.)
        {
            let div_naam: HashMap<u32, String> = objecten(&xml, "Division").iter().filter_map(|d| Some((id(d, "DivisionID")?, d.get("Name").cloned().unwrap_or_default()))).collect();
            let stop_div: HashMap<u32, u32> = stops.iter().filter_map(|s| Some((id(s, "StopID")?, id(s, "DivisionID")?))).collect();
            let mut rang_div: HashMap<u32, HashSet<u32>> = HashMap::new();
            for sr in &stopranks {
                if let (Some(st), Some(rk)) = (id(sr, "StopID"), id(sr, "RankID")) {
                    if let Some(d) = stop_div.get(&st) { rang_div.entry(rk).or_default().insert(*d); }
                }
            }
            for s in &stops {
                if let (Some(rk), Some(d)) = (id(s, "Hint_PrimaryAssociatedRankID"), id(s, "DivisionID")) {
                    rang_div.entry(rk).or_default().insert(d);
                }
            }
            let pijp_rang: HashMap<u32, u32> = objecten(&xml, "Pipe_SoundEngine01").iter().filter_map(|p| Some((id(p, "PipeID")?, id(p, "RankID")?))).collect();
            let golf_trem: HashMap<u32, u32> = objecten(&xml, "TremulantWaveform").iter().filter_map(|w| Some((id(w, "TremulantWaveformID")?, id(w, "TremulantID")?))).collect();
            let mut trem_divs: HashMap<u32, BTreeMap<String, usize>> = HashMap::new();
            let mut trem_onbekend: HashMap<u32, usize> = HashMap::new();
            for wp in objecten(&xml, "TremulantWaveformPipe") {
                let (Some(pijp), Some(golf)) = (id(&wp, "PipeID"), id(&wp, "TremulantWaveformID")) else { continue };
                let Some(&t) = golf_trem.get(&golf) else { continue };
                match pijp_rang.get(&pijp).and_then(|rk| rang_div.get(rk)) {
                    Some(ds) => for d in ds { *trem_divs.entry(t).or_default().entry(div_naam.get(d).cloned().unwrap_or_default()).or_default() += 1; },
                    None => *trem_onbekend.entry(t).or_default() += 1,
                }
            }
            for t in &trems {
                let Some(tid) = id(t, "TremulantID") else { continue };
                let naam = t.get("Name").cloned().unwrap_or_default();
                let echt = trem_divs.get(&tid).cloned().unwrap_or_default();
                let _ = writeln!(r, "  · tremulant \"{}\": volgens de pijpen {:?} ({} pijpen zonder bekende divisie)",
                    naam, echt, trem_onbekend.get(&tid).copied().unwrap_or(0));
            }
        }
        let mut zk: Vec<String> = zonder_knop.iter().map(|w| format!("{} \"{}\"", w, sw_naam.get(w).cloned().unwrap_or_default())).collect();
        zk.sort();
        let _ = writeln!(r, "- knoppen van de set die iets bedienen maar zelf geen JM-knop zijn: {}{}", zonder_knop.len(),
            if zk.is_empty() { String::new() } else { format!(" (bijv. {})", zk.iter().take(8).cloned().collect::<Vec<_>>().join("; ")) });
        for (titel, objs) in [("SwitchLinkage", &links), ("KeyAction", &kas)] {
            let v = verdeling(objs, 6);
            let velden: Vec<String> = v.iter()
                .filter(|(k, _)| !k.ends_with("ID") && k.as_str() != "Name")
                .map(|(k, w)| format!("{} {:?}", k, w))
                .collect();
            let _ = writeln!(r, "- velden {} met weinig waarden: {}", titel, velden.join("; "));
        }
    }
    let tekst = format!("# E3: schakelnetwerk tegenover JM-Orgue\n\n{}\n{}", samenvatting, r);
    match uitbestand {
        Some(u) => {
            std::fs::write(&u, &tekst).expect("rapport schrijven");
            println!("{}", samenvatting);
            println!("rapport: {}", u);
        }
        None => println!("{}", tekst),
    }
}
