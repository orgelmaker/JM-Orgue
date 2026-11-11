//! Schijfcache van gedecodeerde opnamen (C2, fase 2).
//!
//! Een volledige opname die duur is om te maken (WavPack uitpakken, of resamplen naar de samplerate van de uitgang)
//! wordt na het maken als één bestand in de cachemap gezet. De volgende keer leest de achtergrondlader dat bestand
//! in plaats van opnieuw te decoderen en te resamplen. Gewone WAV-bestanden in de juiste samplerate worden niet
//! gecachet: die lezen al snel.
//!
//! Inhoud: precies de waarden die het decoderen opleverde (vóór harmonische shaping, staartafkap en compacte opslag in
//! het geheugen), dus het afspelen is bit-identiek. Waarden die exact in 16 of 24 bit passen worden zo opgeslagen; de
//! rest als f32.
//!
//! Kop: magische tekst, formaatversie, en de sleutel (bronpad, grootte en wijzigingstijd van de bron, doelsamplerate,
//! looppunten uit de orgeldefinitie, stereo-laden). Klopt iets niet, dan wordt het bestand genegeerd en overschreven.
//! De bestandsnaam is een hash van de sleutel; de volledige sleutel staat in de kop, dus een botsing geeft geen
//! verkeerde klank.
//!
//! Alles gebeurt op de achtergrondlader, nooit in de audio-callback.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::loader::SampleData;
use vpo_core::SampleRate;
use crate::monsters::{Monsters, SCHAAL_16, SCHAAL_24};

const MAGIE: &[u8; 8] = b"JMOCACHE";
/// Ophogen als de inhoud of de manier van decoderen verandert: oude bestanden worden dan niet meer gebruikt.
const VERSIE: u32 = 1;
/// Hoeveel opnamen er hoogstens op de schrijfthread mogen wachten; daarboven wordt een opname niet bewaard (dan
/// gewoon de volgende keer).
const SCHRIJF_WACHTRIJ: usize = 64;
const EXTENSIE: &str = "jmc";

static AAN: AtomicBool = AtomicBool::new(false);
static MAX_BYTES: AtomicU64 = AtomicU64::new(20 * 1024 * 1024 * 1024);
/// Geschatte totale grootte van de cachemap (bijgehouden bij schrijven en opruimen; bij het instellen geteld).
static TOTAAL: AtomicU64 = AtomicU64::new(0);
static MAP: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Eén opruimronde tegelijk.
static OPRUIMEN: Mutex<()> = Mutex::new(());

/// Tellers voor de test-API en het verslag.
pub static TREFFERS: AtomicU64 = AtomicU64::new(0);
pub static GESCHREVEN: AtomicU64 = AtomicU64::new(0);

/// Cache instellen (bij het opstarten en na een wijziging in Instellingen). Telt op de achtergrond de huidige grootte.
pub fn stel_in(map: PathBuf, aan: bool, max_bytes: u64) {
    AAN.store(aan, Ordering::Relaxed);
    MAX_BYTES.store(max_bytes, Ordering::Relaxed);
    let nieuw = {
        let mut m = MAP.lock().unwrap_or_else(|e| e.into_inner());
        let nieuw = m.as_ref() != Some(&map);
        *m = Some(map);
        nieuw
    };
    if nieuw {
        TOTAAL.store(0, Ordering::Relaxed);
        let _ = std::thread::Builder::new().name("schijfcache-tellen".into()).spawn(|| {
            let (bytes, _) = tel();
            TOTAAL.store(bytes, Ordering::Relaxed);
            if bytes > MAX_BYTES.load(Ordering::Relaxed) {
                ruim_op();
            }
        });
    } else if TOTAAL.load(Ordering::Relaxed) > max_bytes {
        let _ = std::thread::Builder::new().name("schijfcache-opruimen".into()).spawn(ruim_op);
    }
}

pub fn aan() -> bool {
    AAN.load(Ordering::Relaxed)
}

fn map() -> Option<PathBuf> {
    MAP.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Is de bron een WavPack-bestand (ook als het `.wav` heet)? Dan is decoderen duur genoeg om te cachen.
pub fn is_wavpack(pad: &Path) -> bool {
    let mut kop = [0u8; 4];
    std::fs::File::open(pad).and_then(|mut f| f.read_exact(&mut kop)).is_ok() && crate::wavpack::is_wavpack(&kop)
}

/// De sleutel als bytes: alles wat de gedecodeerde waarden bepaalt.
fn sleutel(pad: &Path, doel: SampleRate, odf_loop: Option<(u32, u32)>) -> Option<Vec<u8>> {
    let md = std::fs::metadata(pad).ok()?;
    let tijd = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    let pad_tekst = pad.to_string_lossy();
    let mut s = Vec::with_capacity(64 + pad_tekst.len());
    s.extend_from_slice(&(pad_tekst.len() as u32).to_le_bytes());
    s.extend_from_slice(pad_tekst.as_bytes());
    s.extend_from_slice(&md.len().to_le_bytes());
    s.extend_from_slice(&tijd.as_secs().to_le_bytes());
    s.extend_from_slice(&tijd.subsec_nanos().to_le_bytes());
    s.extend_from_slice(&doel.to_le_bytes());
    match odf_loop {
        Some((a, b)) => {
            s.push(1);
            s.extend_from_slice(&a.to_le_bytes());
            s.extend_from_slice(&b.to_le_bytes());
        }
        None => s.push(0),
    }
    s.push(crate::loader::stereo_loading() as u8);
    Some(s)
}

/// FNV-1a, 64 bit: stabiel over versies van Rust heen (de bestandsnaam moet morgen hetzelfde zijn).
fn hash(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn bestand(map: &Path, sleutel: &[u8]) -> PathBuf {
    map.join(format!("{:016x}.{EXTENSIE}", hash(sleutel)))
}

/// Kleinste soort die voor beide kanalen exact is: 2 = 16 bit, 3 = 24 bit, 4 = f32 (bytes per monster).
fn soort_voor(l: &[f32], r: Option<&[f32]>) -> u8 {
    if Monsters::past_16(l) && r.is_none_or(Monsters::past_16) {
        2
    } else if Monsters::past_24(l) && r.is_none_or(Monsters::past_24) {
        3
    } else {
        4
    }
}

fn schrijf_kanaal(w: &mut impl Write, v: &[f32], soort: u8) -> std::io::Result<()> {
    let mut blok = Vec::with_capacity(4096 * 4);
    for stuk in v.chunks(4096) {
        blok.clear();
        for &x in stuk {
            match soort {
                2 => blok.extend_from_slice(&((x * 32768.0) as i32 as i16).to_le_bytes()),
                3 => blok.extend_from_slice(&((x * 8388608.0) as i32).to_le_bytes()[..3]),
                _ => blok.extend_from_slice(&x.to_le_bytes()),
            }
        }
        w.write_all(&blok)?;
    }
    Ok(())
}

fn lees_kanaal(r: &mut impl Read, n: usize, soort: u8) -> std::io::Result<Vec<f32>> {
    if soort == 4 {
        let mut v = vec![0f32; n];
        r.read_exact(bytemuck::cast_slice_mut(&mut v[..]))?;
        if cfg!(target_endian = "big") {
            for x in v.iter_mut() {
                *x = f32::from_bits(u32::from_le(x.to_bits()));
            }
        }
        return Ok(v);
    }
    let per = soort as usize;
    let mut v = Vec::with_capacity(n);
    let mut blok = vec![0u8; 4096 * per];
    let mut over = n;
    while over > 0 {
        let k = over.min(4096);
        r.read_exact(&mut blok[..k * per])?;
        for c in blok[..k * per].chunks_exact(per) {
            v.push(if per == 2 {
                i16::from_le_bytes([c[0], c[1]]) as f32 * SCHAAL_16
            } else {
                let q = (i32::from_le_bytes([c[0], c[1], c[2], 0]) << 8) >> 8;
                q as f32 * SCHAAL_24
            });
        }
        over -= k;
    }
    Ok(v)
}

/// Controlesom over de waarden (de bits van de f32's), achteraan in het bestand. Een half geschreven of beschadigd
/// bestand (bijvoorbeeld na stroomuitval: de lengte klopt maar er staan nullen) wordt zo nooit gebruikt.
fn controlesom(l: &[f32], r: Option<&[f32]>) -> u64 {
    let mut h: u64 = 0x6a09_e667_f3bc_c908;
    for &x in l.iter().chain(r.unwrap_or(&[])) {
        h = (h ^ x.to_bits() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(23);
    }
    h
}

fn opt_u64(w: &mut Vec<u8>, x: Option<u64>) {
    w.push(x.is_some() as u8);
    w.extend_from_slice(&x.unwrap_or(0).to_le_bytes());
}

type Opdracht = (PathBuf, SampleRate, Option<(u32, u32)>, std::sync::Arc<SampleData>);

/// Een opname op de achtergrond laten bewaren: de lader wacht niet op de schijf. Is de wachtrij vol, dan wordt deze
/// opname overgeslagen. De waarden mogen compact in het geheugen staan (die zijn exact gelijk aan de f32's).
pub fn bewaar_later(pad: PathBuf, doel: SampleRate, odf_loop: Option<(u32, u32)>, sd: std::sync::Arc<SampleData>) {
    static ZENDER: std::sync::OnceLock<Option<std::sync::mpsc::SyncSender<Opdracht>>> = std::sync::OnceLock::new();
    if !aan() {
        return;
    }
    let zender = ZENDER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Opdracht>(SCHRIJF_WACHTRIJ);
        std::thread::Builder::new()
            .name("schijfcache-schrijven".into())
            .spawn(move || {
                for (pad, doel, odf_loop, sd) in rx {
                    schrijf(&pad, doel, odf_loop, &sd);
                }
            })
            .ok()
            .map(|_| tx)
    });
    if let Some(tx) = zender {
        let _ = tx.try_send((pad, doel, odf_loop, sd));
    }
}

/// Een gedecodeerde opname opslaan (meteen, op de aanroepende thread). Fouten worden genegeerd (de cache is een
/// versnelling, geen voorwaarde).
pub fn schrijf(pad: &Path, doel: SampleRate, odf_loop: Option<(u32, u32)>, sd: &SampleData) {
    if !aan() {
        return;
    }
    let (Some(map), Some(sl)) = (map(), sleutel(pad, doel, odf_loop)) else { return };
    let (l, r) = (sd.data.naar_f32(), sd.right.as_ref().map(|r| r.naar_f32()));
    if r.as_ref().is_some_and(|r| r.len() != l.len()) {
        return;
    }
    let soort = soort_voor(&l, r.as_deref());
    let mut kop = Vec::with_capacity(sl.len() + 64);
    kop.extend_from_slice(MAGIE);
    kop.extend_from_slice(&VERSIE.to_le_bytes());
    kop.extend_from_slice(&(sl.len() as u32).to_le_bytes());
    kop.extend_from_slice(&sl);
    kop.extend_from_slice(&sd.sample_rate.to_le_bytes());
    kop.extend_from_slice(&sd.channels.to_le_bytes());
    opt_u64(&mut kop, sd.loop_start);
    opt_u64(&mut kop, sd.loop_end);
    kop.extend_from_slice(&(l.len() as u64).to_le_bytes());
    kop.push(soort);
    kop.push(r.is_some() as u8);
    let doelbestand = bestand(&map, &sl);
    let tijdelijk = doelbestand.with_extension(format!("tmp{:?}", std::thread::current().id()).replace(['(', ')'], ""));
    let gelukt = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&map)?;
        let mut w = std::io::BufWriter::with_capacity(1 << 16, std::fs::File::create(&tijdelijk)?);
        w.write_all(&kop)?;
        schrijf_kanaal(&mut w, &l, soort)?;
        if let Some(r) = &r {
            schrijf_kanaal(&mut w, r, soort)?;
        }
        w.write_all(&controlesom(&l, r.as_deref()).to_le_bytes())?;
        drop(w.into_inner().map_err(|e| e.into_error())?);
        std::fs::rename(&tijdelijk, &doelbestand)
    })();
    match gelukt {
        Ok(()) => {
            GESCHREVEN.fetch_add(1, Ordering::Relaxed);
            let grootte = std::fs::metadata(&doelbestand).map(|m| m.len()).unwrap_or(0);
            if TOTAAL.fetch_add(grootte, Ordering::Relaxed) + grootte > MAX_BYTES.load(Ordering::Relaxed) {
                ruim_op();
            }
        }
        Err(_) => {
            let _ = std::fs::remove_file(&tijdelijk);
        }
    }
}

/// Een eerder opgeslagen opname lezen, of `None` als er geen (geldige) is.
pub fn lees(pad: &Path, doel: SampleRate, odf_loop: Option<(u32, u32)>) -> Option<SampleData> {
    if !aan() {
        return None;
    }
    let map = map()?;
    let sl = sleutel(pad, doel, odf_loop)?;
    let bestandspad = bestand(&map, &sl);
    let f = std::fs::File::open(&bestandspad).ok()?;
    let mut r = std::io::BufReader::with_capacity(1 << 16, f);
    let mut b8 = [0u8; 8];
    let mut b4 = [0u8; 4];
    let mut b2 = [0u8; 2];
    let mut b1 = [0u8; 1];
    let gelezen = (|| -> std::io::Result<Option<SampleData>> {
        r.read_exact(&mut b8)?;
        if &b8 != MAGIE {
            return Ok(None);
        }
        r.read_exact(&mut b4)?;
        if u32::from_le_bytes(b4) != VERSIE {
            return Ok(None);
        }
        r.read_exact(&mut b4)?;
        let n = u32::from_le_bytes(b4) as usize;
        if n != sl.len() {
            return Ok(None);
        }
        let mut s = vec![0u8; n];
        r.read_exact(&mut s)?;
        if s != sl {
            return Ok(None);
        }
        r.read_exact(&mut b4)?;
        let sample_rate = u32::from_le_bytes(b4);
        r.read_exact(&mut b2)?;
        let channels = u16::from_le_bytes(b2);
        let mut opt = |r: &mut std::io::BufReader<std::fs::File>| -> std::io::Result<Option<u64>> {
            r.read_exact(&mut b1)?;
            let mut v = [0u8; 8];
            r.read_exact(&mut v)?;
            Ok((b1[0] == 1).then_some(u64::from_le_bytes(v)))
        };
        let loop_start = opt(&mut r)?;
        let loop_end = opt(&mut r)?;
        r.read_exact(&mut b8)?;
        let frames = u64::from_le_bytes(b8) as usize;
        r.read_exact(&mut b2)?;
        let (soort, stereo) = (b2[0], b2[1] == 1);
        if !matches!(soort, 2..=4) || frames > (1 << 31) {
            return Ok(None);
        }
        let links = lees_kanaal(&mut r, frames, soort)?;
        let rechts = if stereo { Some(lees_kanaal(&mut r, frames, soort)?) } else { None };
        r.read_exact(&mut b8)?;
        if u64::from_le_bytes(b8) != controlesom(&links, rechts.as_deref()) {
            return Ok(None);
        }
        Ok(Some(SampleData {
            data: links.into(),
            right: rechts.map(Into::into),
            sample_rate,
            channels,
            loop_start,
            loop_end,
        }))
    })();
    match gelezen {
        Ok(Some(sd)) => {
            TREFFERS.fetch_add(1, Ordering::Relaxed);
            // Recent gebruikt: het opruimen gooit de oudste bestanden eerst weg.
            if let Ok(f) = std::fs::File::options().write(true).open(&bestandspad) {
                let _ = f.set_modified(std::time::SystemTime::now());
            }
            Some(sd)
        }
        _ => None,
    }
}

/// Bestanden in de cachemap met hun grootte en wijzigingstijd.
fn bestanden() -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let Some(map) = map() else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(&map) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == EXTENSIE))
        .filter_map(|e| {
            let md = e.metadata().ok()?;
            Some((e.path(), md.len(), md.modified().unwrap_or(std::time::UNIX_EPOCH)))
        })
        .collect()
}

/// Grootte in bytes en aantal bestanden (telt de map).
pub fn tel() -> (u64, usize) {
    let b = bestanden();
    (b.iter().map(|x| x.1).sum(), b.len())
}

/// Boven het maximum: de langst niet gebruikte bestanden weg tot 90% van het maximum.
fn ruim_op() {
    let Ok(_slot) = OPRUIMEN.try_lock() else { return };
    let mut b = bestanden();
    let mut totaal: u64 = b.iter().map(|x| x.1).sum();
    let grens = MAX_BYTES.load(Ordering::Relaxed) / 10 * 9;
    b.sort_by_key(|x| x.2);
    for (p, grootte, _) in b {
        if totaal <= grens {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            totaal -= grootte;
        }
    }
    TOTAAL.store(totaal, Ordering::Relaxed);
}

/// Alle cachebestanden weggooien (knop Cache legen). Geeft het aantal verwijderde bestanden.
pub fn legen() -> usize {
    let _slot = OPRUIMEN.lock().unwrap_or_else(|e| e.into_inner());
    let mut weg = 0;
    for (p, _, _) in bestanden() {
        if std::fs::remove_file(&p).is_ok() {
            weg += 1;
        }
    }
    TOTAAL.store(tel().0, Ordering::Relaxed);
    weg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// De tests delen de globale instelling; na elkaar draaien.
    static SLOT: Mutex<()> = Mutex::new(());

    fn proefmap(naam: &str) -> PathBuf {
        let m = std::env::temp_dir().join(format!("jm-schijfcache-{naam}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&m);
        std::fs::create_dir_all(&m).unwrap();
        m
    }

    fn opname(waarden: Vec<f32>, rechts: Option<Vec<f32>>) -> SampleData {
        SampleData {
            data: waarden.into(),
            channels: if rechts.is_some() { 2 } else { 1 },
            right: rechts.map(Into::into),
            sample_rate: 48000,
            loop_start: Some(10),
            loop_end: Some(900),
        }
    }

    fn bits(m: &Monsters) -> Vec<u32> {
        m.naar_f32().iter().map(|x| x.to_bits()).collect()
    }

    #[test]
    fn heen_en_terug_bit_identiek_per_soort() {
        let _s = SLOT.lock().unwrap_or_else(|e| e.into_inner());
        let map = proefmap("heen");
        let bron = map.join("bron.wav");
        std::fs::write(&bron, b"willekeurige inhoud").unwrap();
        stel_in(map.join("cache"), true, 1 << 30);
        let reeksen: Vec<(&str, Vec<f32>)> = vec![
            ("16", (0..1000).map(|i| ((i * 37 % 65536) as i32 - 32768) as f32 / 32768.0).collect()),
            ("24", (0..1000).map(|i| ((i * 7919 % 16_777_216) as i32 - 8_388_608) as f32 / 8_388_608.0).collect()),
            ("f32", (0..1000).map(|i| (i as f32 * 0.01).sin() * 0.7).collect()),
        ];
        for (naam, v) in reeksen {
            let r: Vec<f32> = v.iter().rev().copied().collect();
            let sd = opname(v, Some(r));
            schrijf(&bron, 48000, Some((10, 900)), &sd);
            let terug = lees(&bron, 48000, Some((10, 900))).unwrap_or_else(|| panic!("{naam}: niet gelezen"));
            assert_eq!(bits(&terug.data), bits(&sd.data), "{naam} links");
            assert_eq!(bits(terug.right.as_ref().unwrap()), bits(sd.right.as_ref().unwrap()), "{naam} rechts");
            assert_eq!((terug.sample_rate, terug.channels, terug.loop_start, terug.loop_end), (48000, 2, Some(10), Some(900)));
        }
        let _ = std::fs::remove_dir_all(&map);
    }

    #[test]
    fn andere_sleutel_of_gewijzigde_bron_geeft_geen_treffer() {
        let _s = SLOT.lock().unwrap_or_else(|e| e.into_inner());
        let map = proefmap("sleutel");
        let bron = map.join("bron.wav");
        std::fs::write(&bron, b"eerste inhoud").unwrap();
        stel_in(map.join("cache"), true, 1 << 30);
        let sd = opname(vec![0.25; 100], None);
        schrijf(&bron, 48000, None, &sd);
        assert!(lees(&bron, 48000, None).is_some());
        assert!(lees(&bron, 44100, None).is_none(), "andere doelsamplerate");
        assert!(lees(&bron, 48000, Some((1, 50))).is_none(), "andere looppunten");
        // Bron gewijzigd (andere grootte): niet meer geldig.
        std::fs::write(&bron, b"tweede, langere inhoud").unwrap();
        assert!(lees(&bron, 48000, None).is_none(), "gewijzigde bron");
        // Beschadigd (waarden overschreven met nullen): de controlesom wijst het af.
        std::fs::write(&bron, b"eerste inhoud").unwrap();
        schrijf(&bron, 48000, None, &sd);
        let f = bestand(&map.join("cache"), &sleutel(&bron, 48000, None).unwrap());
        let mut inhoud = std::fs::read(&f).unwrap();
        let n = inhoud.len();
        inhoud[n - 40..n - 8].fill(0);
        std::fs::write(&f, &inhoud).unwrap();
        assert!(lees(&bron, 48000, None).is_none(), "beschadigd bestand");
        // Uit: niets lezen of schrijven.
        stel_in(map.join("cache"), false, 1 << 30);
        schrijf(&bron, 48000, None, &sd);
        assert!(lees(&bron, 48000, None).is_none());
        stel_in(map.join("cache"), true, 1 << 30);
        assert!(legen() >= 1);
        assert_eq!(tel(), (0, 0));
        let _ = std::fs::remove_dir_all(&map);
    }

    #[test]
    fn opruimen_houdt_de_cache_onder_het_maximum() {
        let _s = SLOT.lock().unwrap_or_else(|e| e.into_inner());
        let map = proefmap("max");
        // Elk bestand ~4 kB (1000 f32-monsters); maximum 10 kB.
        stel_in(map.join("cache"), true, 10_000);
        std::thread::sleep(std::time::Duration::from_millis(200)); // het tellen op de achtergrond afwachten
        for i in 0..6 {
            let bron = map.join(format!("bron{i}.wav"));
            std::fs::write(&bron, format!("bron {i}")).unwrap();
            schrijf(&bron, 48000, None, &opname((0..1000).map(|k| (k as f32 * 0.37 + i as f32).sin() * 0.5).collect(), None));
        }
        let (bytes, aantal) = tel();
        assert!(bytes <= 10_000, "{bytes} bytes in {aantal} bestanden");
        assert!(aantal >= 1);
        let _ = std::fs::remove_dir_all(&map);
    }
}
