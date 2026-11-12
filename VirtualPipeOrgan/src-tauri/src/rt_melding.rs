//! Meldingen uit de audio-callback zonder tekst op te maken (D1, fase 3). De callback zet een vaste gebeurtenis met
//! een paar getallen in een begrensd kanaal (`try_send` op een array-kanaal alloceert niet en blokkeert niet); een
//! gewone thread maakt er de logregel van. Is het kanaal vol, dan valt de melding weg en telt `gemist` op.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use crossbeam_channel::{bounded, Receiver, Sender};

#[derive(Debug, Clone, Copy)]
pub enum RtMelding {
    /// Een NoteOn vond voor geen enkele laag een opname of startbuffer.
    GeenSample { stop: u32, pijp: u32 },
    /// Een callback gebruikte meer dan 80 % van de buffertijd (gedempt: hooguit eens per 5 s).
    ZwaarBelast {
        /// Promille van de buffertijd, deze callback en het gemiddelde.
        pm: u32,
        gem_pm: u32,
        frames: u32,
        sample_rate: u32,
        stemmen: u32,
        kap: u32,
        /// Segmenten van deze callback in % van de buffertijd: laden, stemstarts, voorrekenen, mengen.
        seg: [u32; 4],
        /// Gemiddelde per pas in %: mengen, nabewerking, wind.
        meng: u32,
        keten: u32,
        wind: u32,
        wachtend: u32,
        zware: u64,
    },
}

static KANAAL: OnceLock<Sender<RtMelding>> = OnceLock::new();
static GEMIST: AtomicU64 = AtomicU64::new(0);

/// Start de logthread (één keer per proces; latere aanroepen doen niets).
pub fn start() {
    KANAAL.get_or_init(|| {
        let (tx, rx) = bounded::<RtMelding>(256);
        std::thread::Builder::new()
            .name("rt-meldingen".into())
            .spawn(move || schrijf_lus(rx))
            .expect("rt-meldingen thread");
        tx
    });
}

/// Vanuit de callback: geen allocatie, geen lock, geen wachten.
#[inline]
pub fn meld(m: RtMelding) {
    match KANAAL.get() {
        Some(tx) if tx.try_send(m).is_ok() => {}
        _ => {
            GEMIST.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn gemist() -> u64 {
    GEMIST.load(Ordering::Relaxed)
}

fn schrijf_lus(rx: Receiver<RtMelding>) {
    while let Ok(m) = rx.recv() {
        tracing::warn!("{}", tekst(&m));
    }
}

/// De logregel, gelijk aan wat de callback vroeger zelf schreef.
pub fn tekst(m: &RtMelding) -> String {
    match *m {
        RtMelding::GeenSample { stop, pijp } => format!("NoteOn: No sample or preload for stop={}, pipe={}", stop, pijp),
        RtMelding::ZwaarBelast { pm, gem_pm, frames, sample_rate, stemmen, kap, seg, meng, keten, wind, wachtend, zware } => {
            let advies = if frames < 128 {
                "zet de buffergrootte op ten minste 128 frames"
            } else if keten >= meng {
                "de nabewerking (galm/EQ) overheerst — zet de galm uit of verlicht de uitgangscorrectie, of zet de buffer hoger"
            } else {
                "het mengen van de stemmen overheerst — trek minder registers, kies meer rekenkernen bij Audio-uitvoer, of zet de buffer hoger"
            };
            let ls = crate::audio::loader_stats();
            format!(
                "Audio-render zwaar belast: deze callback {}% van de buffertijd (gemiddeld {}%) bij {} frames ({:.2} ms), {} stemmen (kap {}); deze callback: laden {}%, stemstarts {}%, voorrekenen {}%, mengen {}%; gemiddeld: mengen {}%, nabewerking {}%, wind {}%; lichte interpolatie: {}×; laders: {} aanvragen wachtend-gem. {:.0} ms, {} preload-tekorten, {} uitzettingen (plafond {} MB); {} zware callbacks sinds start — {}",
                pm / 10, gem_pm / 10, frames, frames as f64 * 1000.0 / sample_rate.max(1) as f64,
                stemmen, kap, seg[0], seg[1], seg[2], seg[3], meng, keten, wind,
                crate::audio::licht_interp_callbacks(),
                wachtend, ls.latency_avg_ms, ls.preload_underruns, ls.evictions, ls.sample_cap_mb,
                zware, advies
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tekst_geen_sample_is_als_vroeger() {
        assert_eq!(tekst(&RtMelding::GeenSample { stop: 3, pijp: 7 }), "NoteOn: No sample or preload for stop=3, pipe=7");
    }

    #[test]
    fn advies_volgt_buffer_en_passen() {
        let basis = RtMelding::ZwaarBelast {
            pm: 950, gem_pm: 500, frames: 64, sample_rate: 48_000, stemmen: 100, kap: 4096,
            seg: [0, 0, 0, 90], meng: 60, keten: 10, wind: 1, wachtend: 0, zware: 1,
        };
        assert!(tekst(&basis).ends_with("zet de buffergrootte op ten minste 128 frames"));
        if let RtMelding::ZwaarBelast { pm, gem_pm, sample_rate, stemmen, kap, seg, wind, wachtend, zware, .. } = basis {
            let m = RtMelding::ZwaarBelast { pm, gem_pm, frames: 256, sample_rate, stemmen, kap, seg, meng: 60, keten: 10, wind, wachtend, zware };
            assert!(tekst(&m).contains("het mengen van de stemmen overheerst"));
        }
    }
}
