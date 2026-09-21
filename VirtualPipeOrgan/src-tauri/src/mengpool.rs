//! Werkerspool voor de mengloop (fase 3 van het meerkernige plan).
//!
//! De stemmenlus is verreweg het duurste van een audio-callback: bij 716
//! klinkende stemmen 97,9 % van de rendertijd (gemeten, zie
//! `PLAN_meerkernige_mengloop.md`). Die lus is bijna ideaal parallel, want elke
//! stem raakt alleen zijn eigen toestand en schrijft alleen in de opteltabel
//! van zijn eigen stuk. Deze pool verdeelt dat werk over vaste werkerthreads.
//!
//! **Waarom geen rayon** (dat al in het project zit): die pool wordt gedeeld
//! met het laden van samples, doet work-stealing met onvoorspelbare latentie en
//! draait op gewone prioriteit. Een audio-callback die daarop wacht, wacht op
//! de willekeur van het besturingssysteem.
//!
//! **De barrière.** Per blok schrijft de audiothread een taak per werker, hoogt
//! de generatieteller op en doet zijn eigen stuk; daarna wacht hij tot alle
//! werkers hun stuk gemeld hebben. De werkers wachten eerst spinnend (dan is de
//! latentie honderden nanoseconden en kost het wakker maken geen systeemaanroep)
//! en parkeren pas als er een tijd niets te doen is — anders zou een stil orgel
//! zeven kernen laten rondtollen.
//!
//! **Veiligheid.** De taken bevatten rauwe pointers naar stukken van de
//! stemmenlijst en naar de opteltabellen. Dat is verantwoord omdat:
//!   * de stukken van `chunks_mut` per definitie disjunct zijn, dus geen twee
//!     werkers raken dezelfde stem;
//!   * elk stuk zijn eigen opteltabel heeft, dus geen twee werkers schrijven op
//!     dezelfde plek;
//!   * de audiothread ná de barrière pas verdergaat, dus de geleende gegevens
//!     leven gegarandeerd langer dan de taak.
//! Die drie punten samen zijn de invariant; `verdeel_en_meng` is de enige plek
//! die hem kan schenden en is daarom de enige `unsafe`-aanroep.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::audio::{meng_stemmen_blok, MengBlok, PlayingVoice};

/// Hoe lang een werker spinnend wacht voordat hij parkeert. Ruim langer dan de
/// tijd die de audiothread tussen twee blokken nodig heeft (pas 3 duurt op een
/// gewoon orgel minder dan 0,1 ms), zodat werkers tijdens het spelen heet
/// blijven en er geen systeemaanroep nodig is om ze te wekken.
const SPIN_BUDGET: std::time::Duration = std::time::Duration::from_micros(2000);

/// Zo lang parkeert een werker per keer. Kort genoeg om een gemiste `unpark`
/// nooit langer dan dit te laten duren (vangnet, geen normale route).
const PARK_DUUR: std::time::Duration = std::time::Duration::from_millis(20);

/// Eén opdracht voor één werker: welk stuk stemmen, waar het naartoe moet, en
/// de (gedeelde, alleen-lezen) blokcontext.
#[derive(Clone, Copy)]
struct Taak {
    stemmen: *mut PlayingVoice,
    aantal: usize,
    doel_l: *mut [f32; 32],
    doel_r: *mut [f32; 32],
    frames: usize,
    blok: *const MengBlok<'static>,
}

// SAFETY: zie de moduletoelichting. De pointers worden alleen gebruikt tussen
// het ophogen van de generatieteller en het melden van "klaar", en in dat
// venster blijft de audiothread staan wachten.
unsafe impl Send for Taak {}
unsafe impl Sync for Taak {}

impl Taak {
    /// Lege taak: deze werker heeft deze ronde niets te doen.
    fn leeg() -> Self {
        Taak {
            stemmen: std::ptr::null_mut(),
            aantal: 0,
            doel_l: std::ptr::null_mut(),
            doel_r: std::ptr::null_mut(),
            frames: 0,
            blok: std::ptr::null(),
        }
    }

    fn is_leeg(&self) -> bool {
        self.stemmen.is_null() || self.aantal == 0 || self.blok.is_null()
    }
}

/// Wat de audiothread en de werkers delen.
struct Gedeeld {
    /// Hoogt op zodra er een nieuwe ronde taken klaarstaat. Werkers wachten
    /// tot hij hoger is dan de generatie die zij het laatst deden.
    generatie: AtomicU64,
    /// Aantal werkers dat deze generatie afgerond heeft.
    klaar: AtomicUsize,
    /// Stoppen (bij het sluiten van de stream).
    stop: AtomicBool,
    /// Per werker: is hij geparkeerd? Zo niet, dan is `unpark` overbodig en
    /// slaat de audiothread de systeemaanroep over.
    geparkeerd: Vec<AtomicBool>,
    /// Per werker zijn taak voor de huidige generatie. Alleen geschreven door
    /// de audiothread vóór het ophogen van `generatie`, alleen gelezen door de
    /// eigenaar ná het zien van de nieuwe generatie.
    taken: Vec<std::cell::UnsafeCell<Taak>>,
}

// SAFETY: de taken worden beschermd door de generatieteller met
// Release/Acquire-ordening; zie de moduletoelichting.
unsafe impl Send for Gedeeld {}
unsafe impl Sync for Gedeeld {}

/// Werkerspool. Leeft zolang de audiostream leeft; `Drop` laat de werkers
/// netjes uitlopen, zodat een audiowissel geen threads achterlaat.
pub struct MengPool {
    gedeeld: Arc<Gedeeld>,
    werkers: Vec<JoinHandle<()>>,
}

impl MengPool {
    /// Bouwt een pool met `n_werkers` threads (0 = geen pool; alles blijft op
    /// de audiothread). `naam` komt in de threadnaam, handig in een debugger.
    pub fn nieuw(n_werkers: usize) -> Self {
        let gedeeld = Arc::new(Gedeeld {
            generatie: AtomicU64::new(0),
            klaar: AtomicUsize::new(0),
            stop: AtomicBool::new(false),
            geparkeerd: (0..n_werkers).map(|_| AtomicBool::new(false)).collect(),
            taken: (0..n_werkers).map(|_| std::cell::UnsafeCell::new(Taak::leeg())).collect(),
        });
        let mut werkers = Vec::with_capacity(n_werkers);
        for idx in 0..n_werkers {
            let g = gedeeld.clone();
            let handle = std::thread::Builder::new()
                .name(format!("vpo-meng-{}", idx))
                .spawn(move || werker_lus(idx, g))
                .expect("kon mengwerker niet starten");
            werkers.push(handle);
        }
        if n_werkers > 0 {
            tracing::info!("Mengloop: {} werkerthread(s) gestart", n_werkers);
        }
        MengPool { gedeeld, werkers }
    }

    /// Aantal werkers naast de audiothread.
    pub fn werkers(&self) -> usize {
        self.werkers.len()
    }

    /// Verdeelt `stemmen` over de audiothread en de werkers, mengt het blok en
    /// keert pas terug als iedereen klaar is.
    ///
    /// `deel_l`/`deel_r` zijn de opteltabellen van de stukken 1..; stuk 0
    /// schrijft rechtstreeks in `hoofd_l`/`hoofd_r`. Er worden nooit meer
    /// stukken gemaakt dan er deeltabellen zijn.
    pub fn verdeel_en_meng(
        &self,
        stemmen: &mut [PlayingVoice],
        hoofd_l: &mut [[f32; 32]],
        hoofd_r: &mut [[f32; 32]],
        deel_l: &mut [Vec<[f32; 32]>],
        deel_r: &mut [Vec<[f32; 32]>],
        stukken: usize,
        blok: &MengBlok<'_>,
    ) {
        let stukken = stukken.max(1).min(deel_l.len() + 1).min(self.werkers.len() + 1);
        if stukken <= 1 || stemmen.is_empty() {
            meng_stemmen_blok(stemmen, hoofd_l, hoofd_r, blok);
            return;
        }
        let per_stuk = stemmen.len().div_ceil(stukken).max(1);

        // SAFETY: het venster waarin de pointers geldig moeten zijn loopt van
        // hier tot de barrière onderaan deze functie. In dat venster keert deze
        // functie niet terug, dus `stemmen`, de tabellen en `blok` blijven
        // leven. De stukken uit `chunks_mut` zijn disjunct en elk stuk heeft
        // zijn eigen doeltabel, dus er is geen enkele overlap.
        let blok_ptr = blok as *const MengBlok<'_> as *const MengBlok<'static>;
        let mut eigen: Option<(*mut PlayingVoice, usize)> = None;
        let mut uitgedeeld = 0usize;
        {
            let mut stukken_iter = stemmen.chunks_mut(per_stuk);
            let mut idx = 0usize;
            while let Some(deel) = stukken_iter.next() {
                if idx == 0 {
                    eigen = Some((deel.as_mut_ptr(), deel.len()));
                } else {
                    let taak = Taak {
                        stemmen: deel.as_mut_ptr(),
                        aantal: deel.len(),
                        doel_l: deel_l[idx - 1].as_mut_ptr(),
                        doel_r: deel_r[idx - 1].as_mut_ptr(),
                        frames: blok.bn,
                        blok: blok_ptr,
                    };
                    unsafe { *self.gedeeld.taken[idx - 1].get() = taak; }
                    uitgedeeld += 1;
                }
                idx += 1;
            }
            // Werkers zonder stuk krijgen een lege taak, anders zouden ze de
            // taak van de vorige ronde nog eens uitvoeren.
            for w in uitgedeeld..self.werkers.len() {
                unsafe { *self.gedeeld.taken[w].get() = Taak::leeg(); }
            }
        }

        // Startsein. Release: alles wat hierboven geschreven is, is zichtbaar
        // voor iedere werker die deze generatie leest.
        self.gedeeld.klaar.store(0, Ordering::Relaxed);
        self.gedeeld.generatie.fetch_add(1, Ordering::Release);
        for w in 0..self.werkers.len() {
            if self.gedeeld.geparkeerd[w].load(Ordering::Acquire) {
                self.werkers[w].thread().unpark();
            }
        }

        // Ons eigen stuk, terwijl de werkers bezig zijn.
        if let Some((ptr, len)) = eigen {
            let deel = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
            meng_stemmen_blok(deel, hoofd_l, hoofd_r, blok);
        }

        // Barrière: wachten tot alle werkers hun stuk gemeld hebben. Alleen
        // spinnen — dit duurt hooguit zo lang als één stuk mengen, en de
        // audiothread mag hier niet gaan slapen.
        let doel = self.werkers.len();
        while self.gedeeld.klaar.load(Ordering::Acquire) < doel {
            std::hint::spin_loop();
        }
    }
}

impl Drop for MengPool {
    fn drop(&mut self) {
        self.gedeeld.stop.store(true, Ordering::Release);
        // Wakker maken zodat een geparkeerde werker de stopvlag ziet.
        self.gedeeld.generatie.fetch_add(1, Ordering::Release);
        for h in &self.werkers {
            h.thread().unpark();
        }
        for h in self.werkers.drain(..) {
            let _ = h.join();
        }
    }
}

/// De lus van één werker.
fn werker_lus(idx: usize, gedeeld: Arc<Gedeeld>) {
    // Denormals naar nul, net als op de audiothread: zonder dit lopen
    // uitklinkende staarten honderd keer trager.
    #[cfg(target_arch = "x86_64")]
    #[allow(deprecated)]
    unsafe {
        use std::arch::x86_64::{_mm_getcsr, _mm_setcsr};
        _mm_setcsr(_mm_getcsr() | 0x8040); // FTZ (bit 15) | DAZ (bit 6)
    }
    // Dezelfde prioriteit als de audio-callback; een werker die verdrongen
    // wordt, houdt de hele callback op.
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
        };
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
    }

    let mut gezien = 0u64;
    loop {
        // Wachten op een nieuwe generatie: eerst spinnen (heet blijven),
        // daarna parkeren (geen kern verbranden als er niets speelt).
        let begin = std::time::Instant::now();
        loop {
            if gedeeld.stop.load(Ordering::Acquire) {
                return;
            }
            let g = gedeeld.generatie.load(Ordering::Acquire);
            if g != gezien {
                gezien = g;
                break;
            }
            if begin.elapsed() < SPIN_BUDGET {
                std::hint::spin_loop();
            } else {
                gedeeld.geparkeerd[idx].store(true, Ordering::Release);
                // Nog één keer kijken vóór het parkeren: anders kan een
                // startsein tussen de controle en het parkeren vallen en blijft
                // deze werker tot de time-out liggen.
                if gedeeld.generatie.load(Ordering::Acquire) == gezien
                    && !gedeeld.stop.load(Ordering::Acquire)
                {
                    std::thread::park_timeout(PARK_DUUR);
                }
                gedeeld.geparkeerd[idx].store(false, Ordering::Release);
            }
        }
        if gedeeld.stop.load(Ordering::Acquire) {
            return;
        }

        // SAFETY: we hebben zojuist de nieuwe generatie met Acquire gelezen,
        // dus de taak die de audiothread vóór zijn Release schreef is
        // zichtbaar. Deze werker is de enige die zijn eigen taakvakje leest.
        let taak = unsafe { *gedeeld.taken[idx].get() };
        if !taak.is_leeg() {
            // SAFETY: zie de moduletoelichting — disjuncte stukken, eigen
            // doeltabel, en de audiothread wacht tot we klaar melden.
            unsafe {
                let deel = std::slice::from_raw_parts_mut(taak.stemmen, taak.aantal);
                let dl = std::slice::from_raw_parts_mut(taak.doel_l, taak.frames);
                let dr = std::slice::from_raw_parts_mut(taak.doel_r, taak.frames);
                meng_stemmen_blok(deel, dl, dr, &*taak.blok);
            }
        }
        gedeeld.klaar.fetch_add(1, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn een_pool_van_nul_werkers_werkt_en_sluit() {
        let pool = MengPool::nieuw(0);
        assert_eq!(pool.werkers(), 0);
    }

    #[test]
    fn werkers_starten_en_stoppen_netjes() {
        // De belangrijkste eigenschap voor een audiowissel: geen achterblijvende
        // threads. Honderd keer een pool maken en weggooien mag niet vastlopen.
        for _ in 0..100 {
            let pool = MengPool::nieuw(4);
            assert_eq!(pool.werkers(), 4);
            drop(pool);
        }
    }

    #[test]
    fn een_geparkeerde_werker_wordt_weer_wakker() {
        // Na langer dan het spin-budget stilstaan moeten de werkers alsnog
        // reageren; anders zou de eerste noot na een pauze het geluid ophouden.
        let pool = MengPool::nieuw(2);
        std::thread::sleep(SPIN_BUDGET * 3);
        let g0 = pool.gedeeld.generatie.load(Ordering::Acquire);
        pool.gedeeld.klaar.store(0, Ordering::Relaxed);
        for w in 0..pool.werkers() {
            unsafe { *pool.gedeeld.taken[w].get() = Taak::leeg(); }
        }
        pool.gedeeld.generatie.fetch_add(1, Ordering::Release);
        for h in &pool.werkers {
            h.thread().unpark();
        }
        let begin = std::time::Instant::now();
        while pool.gedeeld.klaar.load(Ordering::Acquire) < pool.werkers() {
            assert!(begin.elapsed() < std::time::Duration::from_secs(5),
                "werkers reageren niet na parkeren (generatie {} → {})",
                g0, pool.gedeeld.generatie.load(Ordering::Acquire));
            std::hint::spin_loop();
        }
    }
}
