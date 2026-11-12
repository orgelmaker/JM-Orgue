//! Toezicht op de audio-callback (D1, fase 0): telt geheugenaanvragen en -vrijgaven die gebeuren terwijl een thread
//! in de callback of in een mengwerker zit. Alleen met de cargo-feature `rt-toezicht`; zonder die feature zijn alle
//! functies leeg en is de geheugenbeheerder gewoon mimalloc.
//!
//! Gebruik: `let _bewaker = rt_toezicht::binnen();` aan het begin van het stuk dat realtime moet zijn. Zolang de
//! bewaker leeft, telt elke `alloc`/`dealloc`/`realloc` op deze thread. `neem_callback()` geeft per callback het
//! aantal en zet het op nul; `stand()` geeft het totaal en het maximum per callback voor `/perf/slechtste`.
//! `plek(code)` zet een plekcode voor het stuk callback dat nu loopt; `per_plek()` geeft de aanvragen per code:
//! 1 laden-integratie, 2 commando's, 3 voorrekenen, 4 mengen, 5 nabewerking, 6 afronden, 7 mengwerker,
//! 10 NoteOn, 11 NoteOff, 12 ReleaseStop, 13 SetTremulant.

#[cfg(feature = "rt-toezicht")]
mod aan {
    use std::alloc::{GlobalAlloc, Layout};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU64, Ordering};

    thread_local! {
        static BINNEN: Cell<bool> = const { Cell::new(false) };
        /// Plekcode van het stuk callback dat nu loopt (zie `plek`); per plek worden de aanvragen geteld.
        static PLEK: Cell<u8> = const { Cell::new(0) };
    }

    pub const PLEKKEN: usize = 32;
    static PER_PLEK: [AtomicU64; PLEKKEN] = [const { AtomicU64::new(0) }; PLEKKEN];

    /// Aanvragen + vrijgaven in de lopende callback (alle realtime-threads samen).
    static IN_CALLBACK: AtomicU64 = AtomicU64::new(0);
    /// Waarvan aanvragen (alloc, alloc_zeroed, realloc): die kunnen het systeem in; vrijgaven niet.
    static AANVRAGEN_IN_CALLBACK: AtomicU64 = AtomicU64::new(0);
    static AANVRAGEN_TOTAAL: AtomicU64 = AtomicU64::new(0);
    static AANVRAGEN_MAX: AtomicU64 = AtomicU64::new(0);
    /// Sinds de vorige `stand(true)`.
    static TOTAAL: AtomicU64 = AtomicU64::new(0);
    static MAX_PER_CALLBACK: AtomicU64 = AtomicU64::new(0);
    static CALLBACKS_MET: AtomicU64 = AtomicU64::new(0);

    #[inline]
    fn tel(aanvraag: bool) {
        // `try_with`: tijdens het opruimen van een thread bestaat de thread-lokale vlag niet meer.
        if BINNEN.try_with(|b| b.get()).unwrap_or(false) {
            IN_CALLBACK.fetch_add(1, Ordering::Relaxed);
            if aanvraag {
                AANVRAGEN_IN_CALLBACK.fetch_add(1, Ordering::Relaxed);
                let p = PLEK.try_with(|p| p.get()).unwrap_or(0) as usize;
                PER_PLEK[p.min(PLEKKEN - 1)].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub struct Toezicht;

    unsafe impl GlobalAlloc for Toezicht {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            tel(true);
            unsafe { mimalloc::MiMalloc.alloc(l) }
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            tel(true);
            unsafe { mimalloc::MiMalloc.alloc_zeroed(l) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            tel(false);
            unsafe { mimalloc::MiMalloc.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            tel(true);
            unsafe { mimalloc::MiMalloc.realloc(p, l, n) }
        }
    }

    pub struct Bewaker(bool);

    impl Drop for Bewaker {
        fn drop(&mut self) {
            let vorige = self.0;
            let _ = BINNEN.try_with(|b| b.set(vorige));
        }
    }

    pub fn plek(p: u8) {
        let _ = PLEK.try_with(|c| c.set(p));
    }

    /// Aanvragen per plekcode sinds de vorige uitlezing (alleen plekken met aanvragen).
    pub fn per_plek(wissen: bool) -> Vec<(u8, u64)> {
        (0..PLEKKEN)
            .filter_map(|i| {
                let n = if wissen { PER_PLEK[i].swap(0, Ordering::Relaxed) } else { PER_PLEK[i].load(Ordering::Relaxed) };
                (n > 0).then_some((i as u8, n))
            })
            .collect()
    }

    pub fn binnen() -> Bewaker {
        Bewaker(BINNEN.try_with(|b| b.replace(true)).unwrap_or(false))
    }

    pub fn neem_callback() -> u64 {
        let a = AANVRAGEN_IN_CALLBACK.swap(0, Ordering::Relaxed);
        if a > 0 {
            AANVRAGEN_TOTAAL.fetch_add(a, Ordering::Relaxed);
            AANVRAGEN_MAX.fetch_max(a, Ordering::Relaxed);
        }
        let n = IN_CALLBACK.swap(0, Ordering::Relaxed);
        if n > 0 {
            TOTAAL.fetch_add(n, Ordering::Relaxed);
            MAX_PER_CALLBACK.fetch_max(n, Ordering::Relaxed);
            CALLBACKS_MET.fetch_add(1, Ordering::Relaxed);
        }
        n
    }

    /// (totaal, max per callback, callbacks met, aanvragen totaal, aanvragen max per callback)
    pub fn stand(wissen: bool) -> Option<(u64, u64, u64, u64, u64)> {
        let lees = |a: &AtomicU64| if wissen { a.swap(0, Ordering::Relaxed) } else { a.load(Ordering::Relaxed) };
        Some((lees(&TOTAAL), lees(&MAX_PER_CALLBACK), lees(&CALLBACKS_MET), lees(&AANVRAGEN_TOTAAL), lees(&AANVRAGEN_MAX)))
    }
}

#[cfg(feature = "rt-toezicht")]
pub use aan::{binnen, neem_callback, per_plek, plek, stand, Toezicht};

#[cfg(not(feature = "rt-toezicht"))]
mod uit {
    pub struct Bewaker;
    #[inline(always)]
    pub fn binnen() -> Bewaker {
        Bewaker
    }
    #[inline(always)]
    pub fn neem_callback() -> u64 {
        0
    }
    #[inline(always)]
    pub fn plek(_p: u8) {}
    pub fn per_plek(_wissen: bool) -> Vec<(u8, u64)> {
        Vec::new()
    }
    /// `None`: de feature staat uit, er is niets geteld.
    pub fn stand(_wissen: bool) -> Option<(u64, u64, u64, u64, u64)> {
        None
    }
}

#[cfg(not(feature = "rt-toezicht"))]
pub use uit::{binnen, neem_callback, per_plek, plek, stand};

#[cfg(all(test, feature = "rt-toezicht"))]
mod tests {
    use super::*;

    #[test]
    fn telt_alleen_binnen_de_bewaker() {
        let _ = neem_callback();
        let _buiten = vec![1u8; 16];
        assert_eq!(neem_callback(), 0);
        {
            let _b = binnen();
            let v = vec![1u8; 16];
            drop(v);
        }
        assert_eq!(neem_callback(), 2); // één aanvraag, één vrijgave
    }
}
