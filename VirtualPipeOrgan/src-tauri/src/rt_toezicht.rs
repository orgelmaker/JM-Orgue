//! Toezicht op de audio-callback (D1, fase 0): telt geheugenaanvragen en -vrijgaven die gebeuren terwijl een thread
//! in de callback of in een mengwerker zit. Alleen met de cargo-feature `rt-toezicht`; zonder die feature zijn alle
//! functies leeg en is de geheugenbeheerder gewoon mimalloc.
//!
//! Gebruik: `let _bewaker = rt_toezicht::binnen();` aan het begin van het stuk dat realtime moet zijn. Zolang de
//! bewaker leeft, telt elke `alloc`/`dealloc`/`realloc` op deze thread. `neem_callback()` geeft per callback het
//! aantal en zet het op nul; `stand()` geeft het totaal en het maximum per callback voor `/perf/slechtste`.

#[cfg(feature = "rt-toezicht")]
mod aan {
    use std::alloc::{GlobalAlloc, Layout};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU64, Ordering};

    thread_local! {
        static BINNEN: Cell<bool> = const { Cell::new(false) };
    }

    /// Aanvragen + vrijgaven in de lopende callback (alle realtime-threads samen).
    static IN_CALLBACK: AtomicU64 = AtomicU64::new(0);
    /// Sinds de vorige `stand(true)`.
    static TOTAAL: AtomicU64 = AtomicU64::new(0);
    static MAX_PER_CALLBACK: AtomicU64 = AtomicU64::new(0);
    static CALLBACKS_MET: AtomicU64 = AtomicU64::new(0);

    #[inline]
    fn tel() {
        // `try_with`: tijdens het opruimen van een thread bestaat de thread-lokale vlag niet meer.
        if BINNEN.try_with(|b| b.get()).unwrap_or(false) {
            IN_CALLBACK.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub struct Toezicht;

    unsafe impl GlobalAlloc for Toezicht {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            tel();
            unsafe { mimalloc::MiMalloc.alloc(l) }
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            tel();
            unsafe { mimalloc::MiMalloc.alloc_zeroed(l) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            tel();
            unsafe { mimalloc::MiMalloc.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            tel();
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

    pub fn binnen() -> Bewaker {
        Bewaker(BINNEN.try_with(|b| b.replace(true)).unwrap_or(false))
    }

    pub fn neem_callback() -> u64 {
        let n = IN_CALLBACK.swap(0, Ordering::Relaxed);
        if n > 0 {
            TOTAAL.fetch_add(n, Ordering::Relaxed);
            MAX_PER_CALLBACK.fetch_max(n, Ordering::Relaxed);
            CALLBACKS_MET.fetch_add(1, Ordering::Relaxed);
        }
        n
    }

    pub fn stand(wissen: bool) -> Option<(u64, u64, u64)> {
        let s = if wissen {
            (TOTAAL.swap(0, Ordering::Relaxed), MAX_PER_CALLBACK.swap(0, Ordering::Relaxed), CALLBACKS_MET.swap(0, Ordering::Relaxed))
        } else {
            (TOTAAL.load(Ordering::Relaxed), MAX_PER_CALLBACK.load(Ordering::Relaxed), CALLBACKS_MET.load(Ordering::Relaxed))
        };
        Some(s)
    }
}

#[cfg(feature = "rt-toezicht")]
pub use aan::{binnen, neem_callback, stand, Toezicht};

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
    /// `None`: de feature staat uit, er is niets geteld.
    pub fn stand(_wissen: bool) -> Option<(u64, u64, u64)> {
        None
    }
}

#[cfg(not(feature = "rt-toezicht"))]
pub use uit::{binnen, neem_callback, stand};

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
