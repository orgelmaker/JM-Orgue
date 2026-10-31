//! WavPack-samples (0.9.35): een `.wav`/`.wv` die eigenlijk WavPack is (o.a.
//! de sets van Lars Palo) wordt in het geheugen teruggezet naar de originele
//! WAV-bytes. WavPack bewaart de RIFF-header (fmt, smpl, cue, LIST…) en het
//! staartstuk na de data letterlijk; header + PCM + staart is dus exact het
//! bestand dat ooit is ingepakt. Daardoor leest de gewone WAV-route alles
//! verder (looppunten, cue, releases) zonder eigen WavPack-pad.
//!
//! Decoder: `wavicle` (pure Rust, MIT/Apache, controleert de CRC per blok).
//! Getest op 4454 bestanden: bit-identiek aan ffmpeg (steekproef 200/200).
//!
//! Een korte cache voorkomt dat startbuffer, looppunten en het volledige
//! sample hetzelfde bestand elk opnieuw decoderen.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// WavPack-metadata: originele RIFF-header en -staart (function-id incl.
/// de optional-bit, zoals in de referentie-implementatie).
const ID_RIFF_HEADER: u8 = 0x21;
const ID_RIFF_TRAILER: u8 = 0x22;

/// Bovengrens van de cache in bytes (teruggezette WAV's). Klein gehouden: de
/// cache hoeft alleen de opeenvolgende reads van één laadronde te overbruggen
/// en blijft na het laden in het geheugen staan.
const CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Begint deze kop met de WavPack-handtekening?
pub fn is_wavpack(kop: &[u8]) -> bool {
    kop.len() >= 4 && &kop[0..4] == b"wvpk"
}

/// Zet een complete WavPack-stroom terug naar WAV-bytes. Zonder bewaarde
/// RIFF-header (bestand ingepakt met `-r`) komt er een minimale header bij.
pub fn naar_wav(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let d = wavicle::decode_stream(bytes).map_err(|e| format!("WavPack: {e:?}"))?;
    let (mut kop, mut staart) = (Vec::new(), Vec::new());
    for blok in wavicle::Blocks::new(bytes) {
        let blok = blok.map_err(|e| format!("WavPack: {e:?}"))?;
        for sub in blok.sub_blocks().flatten() {
            match sub.id {
                ID_RIFF_HEADER => kop.extend_from_slice(sub.data),
                ID_RIFF_TRAILER => staart.extend_from_slice(sub.data),
                _ => {}
            }
        }
    }
    let bps = (d.bits_per_sample / 8) as usize;
    if !(2..=4).contains(&bps) || d.channels == 0 {
        return Err(format!("WavPack: {} bit / {} kanalen niet ondersteund", d.bits_per_sample, d.channels));
    }
    let data_len = d.samples.len() * bps;
    if kop.len() < 12 || &kop[0..4] != b"RIFF" {
        kop = minimale_header(&d, data_len);
        staart.clear();
    }
    let mut wav = Vec::with_capacity(kop.len() + data_len + staart.len());
    wav.extend_from_slice(&kop);
    for &v in &d.samples {
        // Float komt als de ruwe IEEE-bits terug; integer als little-endian
        // in de oorspronkelijke breedte.
        wav.extend_from_slice(&v.to_le_bytes()[..bps]);
    }
    wav.extend_from_slice(&staart);
    Ok(wav)
}

fn minimale_header(d: &wavicle::DecodedStream, data_len: usize) -> Vec<u8> {
    let bps = d.bits_per_sample / 8;
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&(if d.is_float { 3u16 } else { 1u16 }).to_le_bytes());
    h.extend_from_slice(&(d.channels as u16).to_le_bytes());
    h.extend_from_slice(&d.sample_rate.to_le_bytes());
    h.extend_from_slice(&(d.sample_rate * d.channels * bps).to_le_bytes());
    h.extend_from_slice(&((d.channels * bps) as u16).to_le_bytes());
    h.extend_from_slice(&(d.bits_per_sample as u16).to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&(data_len as u32).to_le_bytes());
    h
}

/// Teruggezette WAV's, nieuwste achteraan.
static CACHE: Mutex<VecDeque<(PathBuf, Arc<Vec<u8>>)>> = Mutex::new(VecDeque::new());

fn uit_cache(path: &Path) -> Option<Arc<Vec<u8>>> {
    CACHE.lock().iter().find(|(p, _)| p == path).map(|(_, b)| b.clone())
}

fn in_cache(path: &Path, wav: Arc<Vec<u8>>) {
    let mut c = CACHE.lock();
    if c.iter().any(|(p, _)| p == path) {
        return;
    }
    c.push_back((path.to_path_buf(), wav));
    let mut totaal: usize = c.iter().map(|(_, b)| b.len()).sum();
    while totaal > CACHE_MAX_BYTES && c.len() > 1 {
        if let Some((_, b)) = c.pop_front() {
            totaal -= b.len();
        }
    }
}

/// Lees een audiobestand als bytes: een gewone WAV zoals hij is, een
/// WavPack-bestand teruggezet naar WAV.
pub fn lees_bytes(path: &Path) -> std::io::Result<Arc<Vec<u8>>> {
    if let Some(b) = uit_cache(path) {
        return Ok(b);
    }
    let bytes = std::fs::read(path)?;
    if !is_wavpack(&bytes) {
        return Ok(Arc::new(bytes));
    }
    let wav = Arc::new(naar_wav(&bytes).map_err(|e| std::io::Error::new(
        std::io::ErrorKind::InvalidData, format!("{}: {}", path.display(), e)))?);
    in_cache(path, wav.clone());
    Ok(wav)
}

/// Bytes achter een `Arc`, als `[u8]` leesbaar voor `Cursor`.
#[derive(Clone)]
pub struct GedeeldeBytes(Arc<Vec<u8>>);

impl AsRef<[u8]> for GedeeldeBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Eén WavPack-blok: eerste frame, aantal frames en waar het in de stroom staat.
struct Blok {
    start: u64,
    frames: u64,
    off: usize,
    len: usize,
}

/// Lui gedecodeerde WAV (0.9.36): doet zich voor als de teruggezette WAV
/// (header + PCM + staart), maar decodeert pas een blok als er data uit dat
/// blok gelezen wordt. Looppunten lezen (header en staart) kost zo geen
/// decodeerwerk, een startbuffer alleen de eerste blokken en een release-
/// segment alleen de blokken rond de cue. Elk WavPack-blok is zelfstandig te
/// decoderen (`decode_stream` draagt geen toestand over tussen blokken).
pub struct WavPackLezer {
    bron: Vec<u8>,
    blokken: Vec<Blok>,
    kop: Vec<u8>,
    staart: Vec<u8>,
    frame_bytes: u64,
    bps: usize,
    data_len: u64,
    pos: u64,
    /// Laatst gedecodeerde blok (index, PCM-bytes).
    huidig: Option<(usize, Vec<u8>)>,
}

impl WavPackLezer {
    /// Indeling uit de blokkoppen, zonder te decoderen. `None` als de stroom
    /// niet in dit luie pad past (geen RIFF-header, >2 kanalen, onverwachte
    /// opbouw of een header die niet bij de blokken klopt): dan decodeert de
    /// aanroeper het hele bestand zoals voorheen.
    pub fn nieuw(bron: Vec<u8>) -> Option<Self> {
        let (mut kop, mut staart) = (Vec::new(), Vec::new());
        let mut blokken = Vec::new();
        let mut off = 0usize;
        let mut formaat: Option<(u32, u32)> = None; // (bytes/sample, kanalen)
        for blok in wavicle::Blocks::new(&bron) {
            let blok = blok.ok()?;
            let h = blok.header;
            for sub in blok.sub_blocks() {
                let sub = sub.ok()?;
                match sub.id {
                    ID_RIFF_HEADER => kop.extend_from_slice(sub.data),
                    ID_RIFF_TRAILER => staart.extend_from_slice(sub.data),
                    _ => {}
                }
            }
            if h.block_samples > 0 {
                // Meer dan twee kanalen = meerdere blokken per framebereik.
                if !h.flags.initial_block() || !h.flags.final_block() {
                    return None;
                }
                let f = (h.flags.bytes_per_sample(), h.flags.output_channels());
                if formaat.is_some_and(|g| g != f) {
                    return None;
                }
                formaat = Some(f);
                let verwacht = blokken.last().map_or(0, |b: &Blok| b.start + b.frames);
                if h.block_index != verwacht {
                    return None;
                }
                blokken.push(Blok { start: h.block_index, frames: h.block_samples as u64, off, len: h.block_len() });
            }
            off += h.block_len();
        }
        let (bps, kanalen) = formaat?;
        if !(2..=4).contains(&bps) || kop.len() < 20 || &kop[0..4] != b"RIFF" || &kop[kop.len() - 8..kop.len() - 4] != b"data" {
            return None;
        }
        let frame_bytes = (bps * kanalen) as u64;
        let frames = blokken.last().map_or(0, |b| b.start + b.frames);
        let data_len = frames * frame_bytes;
        let opgegeven = u32::from_le_bytes(kop[kop.len() - 4..].try_into().ok()?) as u64;
        if opgegeven != data_len {
            return None;
        }
        Some(Self { bron, blokken, kop, staart, frame_bytes, bps: bps as usize, data_len, pos: 0, huidig: None })
    }

    fn totaal(&self) -> u64 {
        self.kop.len() as u64 + self.data_len + self.staart.len() as u64
    }

    /// PCM-bytes van blok `i`, gedecodeerd bij de eerste vraag.
    fn blok_pcm(&mut self, i: usize) -> std::io::Result<&[u8]> {
        if self.huidig.as_ref().map(|(j, _)| *j) != Some(i) {
            let b = &self.blokken[i];
            let d = wavicle::decode_stream(&self.bron[b.off..b.off + b.len])
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("WavPack: {e:?}")))?;
            let mut pcm = Vec::with_capacity(d.samples.len() * self.bps);
            for &v in &d.samples {
                pcm.extend_from_slice(&v.to_le_bytes()[..self.bps]);
            }
            if pcm.len() as u64 != b.frames * self.frame_bytes {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "WavPack: blok met onverwachte lengte"));
            }
            self.huidig = Some((i, pcm));
        }
        Ok(&self.huidig.as_ref().expect("net gezet").1)
    }
}

impl Read for WavPackLezer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let kop_len = self.kop.len() as u64;
        let data_eind = kop_len + self.data_len;
        if buf.is_empty() || self.pos >= self.totaal() {
            return Ok(0);
        }
        let n = if self.pos < kop_len {
            let rest = &self.kop[self.pos as usize..];
            let n = rest.len().min(buf.len());
            buf[..n].copy_from_slice(&rest[..n]);
            n
        } else if self.pos < data_eind {
            let rel = self.pos - kop_len;
            let frame = rel / self.frame_bytes;
            let i = self.blokken.partition_point(|b| b.start + b.frames <= frame);
            let blok_begin = self.blokken[i].start * self.frame_bytes;
            let pcm = self.blok_pcm(i)?;
            let rest = &pcm[(rel - blok_begin) as usize..];
            let n = rest.len().min(buf.len());
            buf[..n].copy_from_slice(&rest[..n]);
            n
        } else {
            let rest = &self.staart[(self.pos - data_eind) as usize..];
            let n = rest.len().min(buf.len());
            buf[..n].copy_from_slice(&rest[..n]);
            n
        };
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for WavPackLezer {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let nieuw = match pos {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
            SeekFrom::End(d) => self.totaal() as i128 + d as i128,
        };
        if nieuw < 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek vóór het begin"));
        }
        self.pos = nieuw as u64;
        Ok(self.pos)
    }
}

/// Lezer over een audiobestand: het bestand zelf (gebufferd), de lui
/// gedecodeerde WAV van een WavPack-bestand, of een al volledig teruggezette
/// WAV uit de cache.
pub enum AudioLezer {
    Bestand(BufReader<File>),
    WavPack(Box<WavPackLezer>),
    Geheugen(Cursor<GedeeldeBytes>),
}

impl Read for AudioLezer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            AudioLezer::Bestand(r) => r.read(buf),
            AudioLezer::WavPack(r) => r.read(buf),
            AudioLezer::Geheugen(r) => r.read(buf),
        }
    }
}

impl Seek for AudioLezer {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            AudioLezer::Bestand(r) => r.seek(pos),
            AudioLezer::WavPack(r) => r.seek(pos),
            AudioLezer::Geheugen(r) => r.seek(pos),
        }
    }
}

/// Open een audiobestand voor lezen. Alleen de eerste vier bytes beslissen:
/// een gewone WAV wordt niet in het geheugen gelezen. Een WavPack-bestand
/// wordt lui gedecodeerd; past het niet in dat pad, dan in één keer.
pub fn open(path: &Path) -> std::io::Result<AudioLezer> {
    if let Some(b) = uit_cache(path) {
        return Ok(AudioLezer::Geheugen(Cursor::new(GedeeldeBytes(b))));
    }
    let mut r = BufReader::new(File::open(path)?);
    let mut kop = [0u8; 4];
    let n = r.read(&mut kop)?;
    if !is_wavpack(&kop[..n]) {
        r.seek(SeekFrom::Start(0))?;
        return Ok(AudioLezer::Bestand(r));
    }
    let bron = std::fs::read(path)?;
    if let Some(l) = WavPackLezer::nieuw(bron) {
        return Ok(AudioLezer::WavPack(Box::new(l)));
    }
    let b = lees_bytes(path)?;
    Ok(AudioLezer::Geheugen(Cursor::new(GedeeldeBytes(b))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pakken(samples: &[i32], kanalen: u32, bits: u32) -> Vec<u8> {
        let params = wavicle::EncodeParams { channels: kanalen, sample_rate: 44100, bits_per_sample: bits };
        wavicle::encode_int(params, samples).expect("inpakken")
    }

    #[test]
    fn zonder_riff_header_komt_er_een_minimale_wav_uit() {
        let samples: Vec<i32> = (0..2000).map(|i| ((i * 37) % 4000) - 2000).collect();
        let wv = pakken(&samples, 2, 16);
        assert!(is_wavpack(&wv));
        let wav = naar_wav(&wv).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2, "kanalen");
        assert_eq!(&wav[36..40], b"data");
        let data = &wav[44..];
        assert_eq!(data.len(), samples.len() * 2);
        let terug: Vec<i32> = data.chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as i32).collect();
        assert_eq!(terug, samples, "lossless");
    }

    #[test]
    fn vierentwintig_bit_blijft_drie_bytes_per_sample() {
        let samples: Vec<i32> = (0..1000).map(|i| (i * 7919) % 8_000_000 - 4_000_000).collect();
        let wav = naar_wav(&pakken(&samples, 1, 24)).unwrap();
        let data = &wav[44..];
        assert_eq!(data.len(), samples.len() * 3);
        let terug: Vec<i32> = data.chunks(3).map(|c| (i32::from_le_bytes([0, c[0], c[1], c[2]])) >> 8).collect();
        assert_eq!(terug, samples);
    }

    /// Voeg een metadata-sub-blok (klein formaat, even lengte) vooraan in het
    /// blok op `blok_off` in en werk de blokgrootte bij.
    fn voeg_subblok_in(stroom: &mut Vec<u8>, blok_off: usize, id: u8, data: &[u8]) {
        assert!(data.len() % 2 == 0 && data.len() / 2 < 256);
        let mut sub = vec![id, (data.len() / 2) as u8];
        sub.extend_from_slice(data);
        let ck = u32::from_le_bytes(stroom[blok_off + 4..blok_off + 8].try_into().unwrap()) + sub.len() as u32;
        stroom[blok_off + 4..blok_off + 8].copy_from_slice(&ck.to_le_bytes());
        let at = blok_off + 32;
        stroom.splice(at..at, sub);
    }

    /// Een stroom zoals echte samplesets hem hebben: meerdere blokken, de
    /// originele RIFF-header in het eerste en een staart in het laatste blok.
    fn stroom_met_wrapper() -> Vec<u8> {
        let frames = 300_000usize;
        // In i64 rekenen: CI test in debug, daar is een i32-overloop een paniek.
        let samples: Vec<i32> = (0..frames * 2).map(|i| ((i as i64 * 7919) % 60000) as i32 - 30000).collect();
        let mut wv = pakken(&samples, 2, 16);
        let offs: Vec<usize> = {
            let mut v = Vec::new();
            let mut off = 0;
            for b in wavicle::Blocks::new(&wv) { v.push(off); off += b.unwrap().header.block_len(); }
            v
        };
        assert!(offs.len() >= 3, "test vraagt meerdere blokken, kreeg {}", offs.len());
        let d = wavicle::decode_stream(&wv).unwrap();
        let mut kop = minimale_header(&d, frames * 4);
        kop[4..8].copy_from_slice(&((36 + frames * 4 + 12) as u32).to_le_bytes());
        let staart = b"LIST\x04\0\0\0abcd".to_vec();
        let laatste = *offs.last().unwrap();
        voeg_subblok_in(&mut wv, laatste, ID_RIFF_TRAILER, &staart);
        voeg_subblok_in(&mut wv, 0, ID_RIFF_HEADER, &kop);
        wv
    }

    #[test]
    fn luie_lezer_geeft_dezelfde_bytes_als_alles_decoderen() {
        let wv = stroom_met_wrapper();
        let vol = naar_wav(&wv).unwrap();
        assert!(vol.ends_with(b"LIST\x04\0\0\0abcd"));
        let mut l = WavPackLezer::nieuw(wv.clone()).expect("lui pad");
        let mut alles = Vec::new();
        l.read_to_end(&mut alles).unwrap();
        assert_eq!(alles, vol, "hele bestand");
        // Losse sprongen: header, blokgrenzen, staart, één byte voor het eind.
        let mut l = WavPackLezer::nieuw(wv).unwrap();
        for &(p, n) in &[(0usize, 44usize), (44 + 131_072 * 4 - 3, 10), (500_000, 70_000), (vol.len() - 12, 12), (vol.len() - 1, 5), (100, 1)] {
            l.seek(SeekFrom::Start(p as u64)).unwrap();
            let mut buf = vec![0u8; n];
            let mut gelezen = 0;
            while gelezen < n {
                let k = l.read(&mut buf[gelezen..]).unwrap();
                if k == 0 { break; }
                gelezen += k;
            }
            let eind = (p + n).min(vol.len());
            assert_eq!(&buf[..gelezen], &vol[p..eind], "bereik {p}+{n}");
        }
    }

    #[test]
    fn luie_lezer_alleen_met_kloppende_riff_header() {
        // Zonder RIFF-header: geen lui pad (de aanroeper decodeert alles).
        let samples: Vec<i32> = (0..1000).map(|i| i % 100).collect();
        assert!(WavPackLezer::nieuw(pakken(&samples, 1, 16)).is_none());
    }

    #[test]
    fn gewone_wav_wordt_niet_aangeraakt() {
        assert!(!is_wavpack(b"RIFF\0\0\0\0WAVE"));
        assert!(naar_wav(b"RIFF\0\0\0\0WAVE").is_err());
    }
}
