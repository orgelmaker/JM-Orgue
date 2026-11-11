//! Snelheid van het compact maken van samplewaarden (C2): hoeveel monsters per seconde per kern.
//!
//! ```text
//! cargo run --release -p vpo-sampler --example compact_snelheid
//! ```

use std::time::Instant;

use vpo_sampler::Monsters;

fn echte_buffers(map: &str, aantal: usize) {
    let mut paden = Vec::new();
    let mut stapel = vec![std::path::PathBuf::from(map)];
    while let Some(d) = stapel.pop() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() { stapel.push(p); } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")) { paden.push(p); }
            }
        }
        if paden.len() >= aantal { break; }
    }
    paden.truncate(aantal);
    let bufs: Vec<_> = paden.iter().filter_map(|p| vpo_sampler::loader::load_audio_preload_ms(p, 500).ok()).collect();
    let monsters: usize = bufs.iter().map(|b| b.attack_data.len() + b.attack_right.as_ref().map_or(0, |r| r.len())).sum();
    let t = Instant::now();
    let mut tel = [0usize; 3];
    for b in &bufs {
        let mut b = b.clone();
        let r = b.attack_right.as_mut();
        tel[Monsters::compacteer_paar(&mut b.attack_data, r) as usize] += 1;
    }
    let s = t.elapsed().as_secs_f64();
    println!("echt ({} buffers, {} M monsters): {:.1} ns per monster incl. kopie; soorten f32/i16/i24 = {:?}", bufs.len(), monsters / 1_000_000, s / monsters as f64 * 1e9, tel);
}

fn op_schaal() {
    // Zoveel en zo grote buffers als Friesach: 11186 buffers van 47 700 monsters, 24-bit.
    let schaal = 8388608.0f32;
    let mut bufs: Vec<Monsters> = (0..11186usize)
        .map(|b| Monsters::F32((0..47_700usize).map(|i| (((i * 7 + b * 13) % 200000) as i32 - 100000) as f32 / schaal).collect()))
        .collect();
    let t = Instant::now();
    for m in bufs.iter_mut() {
        Monsters::compacteer_paar(m, None);
    }
    let s = t.elapsed().as_secs_f64();
    println!("op schaal: {:.2} s, {:.1} ns per monster", s, s / (11186.0 * 47_700.0) * 1e9);
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("schaal") {
        op_schaal();
        return;
    }
    if let Some(map) = std::env::args().nth(1) {
        echte_buffers(&map, 400);
        return;
    }
    let n = 48_000usize; // één startbuffer van 1 s mono
    let buffers = 2_000usize;
    for (naam, bits) in [("16-bit", 16u32), ("24-bit", 24)] {
        let schaal = (1i64 << (bits - 1)) as f32;
        let bron: Vec<Vec<f32>> = (0..buffers)
            .map(|b| (0..n).map(|i| (((i * 7 + b * 13) % 20000) as i32 - 10000) as f32 / schaal).collect())
            .collect();
        let t = Instant::now();
        let mut soorten = 0usize;
        for v in &bron {
            let mut m = Monsters::F32(v.clone());
            let r = Monsters::compacteer_paar(&mut m, None);
            soorten += r as usize;
        }
        let s = t.elapsed().as_secs_f64();
        // Alleen de kopie meten, om die eraf te halen.
        let t2 = Instant::now();
        let mut som = 0usize;
        for v in &bron {
            som += v.clone().len();
        }
        let s2 = t2.elapsed().as_secs_f64();
        let monsters = (n * buffers) as f64;
        println!(
            "{naam}: {:.1} ns per monster (kopie alleen {:.1} ns), {} M monsters/s | {} {}",
            s / monsters * 1e9,
            s2 / monsters * 1e9,
            (monsters / (s - s2).max(1e-9) / 1e6) as u64,
            soorten,
            som
        );
    }
}
