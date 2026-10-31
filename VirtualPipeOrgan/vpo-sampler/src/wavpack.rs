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

/// Lezer over een audiobestand: het bestand zelf (gebufferd), of de
/// teruggezette WAV van een WavPack-bestand in het geheugen.
pub enum AudioLezer {
    Bestand(BufReader<File>),
    Geheugen(Cursor<GedeeldeBytes>),
}

impl Read for AudioLezer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            AudioLezer::Bestand(r) => r.read(buf),
            AudioLezer::Geheugen(r) => r.read(buf),
        }
    }
}

impl Seek for AudioLezer {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            AudioLezer::Bestand(r) => r.seek(pos),
            AudioLezer::Geheugen(r) => r.seek(pos),
        }
    }
}

/// Open een audiobestand voor lezen. Alleen de eerste vier bytes beslissen:
/// een gewone WAV wordt niet in het geheugen gelezen.
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

    #[test]
    fn gewone_wav_wordt_niet_aangeraakt() {
        assert!(!is_wavpack(b"RIFF\0\0\0\0WAVE"));
        assert!(naar_wav(b"RIFF\0\0\0\0WAVE").is_err());
    }
}
