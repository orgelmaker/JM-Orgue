//! Realtime-prioriteit voor de mengwerkers (D2).
//!
//! De mengwerkers (`mengpool.rs`) doen een deel van het stemwerk van elke
//! audio-callback. Wordt één van hen door een gewone thread verdrongen, dan
//! wacht de hele callback op hem. Daarom krijgen ze per besturingssysteem de
//! daarvoor bedoelde klasse:
//!
//! - **Windows:** `THREAD_PRIORITY_TIME_CRITICAL`, zoals altijd al (dat is ook
//!   wat cpal zijn WASAPI-thread geeft). De MMCSS-taak "Pro Audio" is er
//!   *niet* standaard bij: gemeten (D2, Rotterdam, ~1090 stemmen, alle kernen
//!   bezet door ander werk) gaf MMCSS + TIME_CRITICAL ruim vijf keer zoveel
//!   overschrijdingen als TIME_CRITICAL alleen. De werkers spinnen tot 2 ms per
//!   ronde; waarschijnlijk houdt MMCSS zo'n bijna continu rekenende thread
//!   in toom, maar dat is niet uitgezocht. MMCSS blijft beschikbaar voor
//!   proeven met `JM_ORGUE_RT=mmcss`.
//! - **Linux:** `SCHED_FIFO` via `pthread_setschedparam`. Zonder rtprio-limiet
//!   (groep `audio` in `limits.conf`) of `CAP_SYS_NICE` weigert de kernel dat;
//!   we melden het en draaien dan gewoon op normale prioriteit. rtkit (D-Bus)
//!   is bewust niet ingebouwd: dat vraagt een D-Bus-client die hier niet te
//!   testen is.
//! - **macOS:** het Mach time-constraint-beleid.
//!
//! Dit verandert alleen *wanneer* de werkers aan de beurt komen, nooit wat ze
//! rekenen: de klank blijft bit-identiek. Aanroepen gebeurt eenmalig bij de
//! start van elke werker, nooit op de audiothread.
//!
//! Instelbaar: Instellingen → "Realtime-prioriteit voor het mengen" (standaard
//! aan), en voor proeven de omgevingsvariabele `JM_ORGUE_RT` (`uit`, of
//! `mmcss` = op Windows ook de MMCSS-taak).

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize, Ordering};

/// Aan/uit-instelling (voorkeur "realtime_prioriteit"); geldt voor werkers
/// die daarna gestart worden, dus voor de volgende (her)bouw van de pool.
static AAN: AtomicBool = AtomicBool::new(true);

pub fn zet_aan(aan: bool) {
    AAN.store(aan, Ordering::Relaxed);
}

pub fn is_aan() -> bool {
    AAN.load(Ordering::Relaxed)
}

/// Welke klasse een werker gekregen heeft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Klasse {
    /// Geen realtime-klasse (normale prioriteit).
    Geen = 0,
    /// Windows: MMCSS-taak "Pro Audio" (plus TIME_CRITICAL), alleen op
    /// verzoek (`JM_ORGUE_RT=mmcss`).
    MmcssProAudio = 1,
    /// Windows: THREAD_PRIORITY_TIME_CRITICAL (de standaard; ook de terugval
    /// als MMCSS op verzoek weigerde).
    TijdKritiek = 2,
    /// Linux: SCHED_FIFO.
    SchedFifo = 3,
    /// macOS: Mach time-constraint.
    TimeConstraint = 4,
}

impl Klasse {
    pub fn naam(self) -> &'static str {
        match self {
            Klasse::Geen => "geen",
            Klasse::MmcssProAudio => "mmcss_pro_audio",
            Klasse::TijdKritiek => "tijdkritiek",
            Klasse::SchedFifo => "sched_fifo",
            Klasse::TimeConstraint => "time_constraint",
        }
    }

    fn uit_code(c: u8) -> Klasse {
        match c {
            1 => Klasse::MmcssProAudio,
            2 => Klasse::TijdKritiek,
            3 => Klasse::SchedFifo,
            4 => Klasse::TimeConstraint,
            _ => Klasse::Geen,
        }
    }
}

/// Waarom een werker geen realtime-klasse kreeg.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Reden {
    /// Er was geen reden nodig: het is gelukt.
    Geen = 0,
    /// Het besturingssysteem weigerde (rechten, limiet).
    Geweigerd = 1,
    /// Dit platform heeft er (hier) geen aanroep voor.
    NietOndersteund = 2,
    /// Door de gebruiker uitgezet (instelling of `JM_ORGUE_RT=uit`).
    Uitgezet = 3,
    /// Overgeslagen: er zijn niet minder werkers dan kernen, dus realtime
    /// spinnende werkers zouden de audiothread en de rest van het systeem
    /// kunnen uithongeren.
    TeVeelWerkers = 4,
}

impl Reden {
    pub fn naam(self) -> &'static str {
        match self {
            Reden::Geen => "",
            Reden::Geweigerd => "geweigerd",
            Reden::NietOndersteund => "niet_ondersteund",
            Reden::Uitgezet => "uitgezet",
            Reden::TeVeelWerkers => "te_veel_werkers",
        }
    }

    fn uit_code(c: u8) -> Reden {
        match c {
            1 => Reden::Geweigerd,
            2 => Reden::NietOndersteund,
            3 => Reden::Uitgezet,
            4 => Reden::TeVeelWerkers,
            _ => Reden::Geen,
        }
    }
}

/// Uitkomst voor één werker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uitkomst {
    pub klasse: Klasse,
    pub reden: Reden,
    /// Foutcode van het systeem als er iets weigerde (0 = geen). Bij een
    /// geslaagde terugval (Windows: MMCSS weigerde, TIME_CRITICAL lukte) staat
    /// hier de MMCSS-fout, ter informatie.
    pub os_fout: i32,
}

impl Uitkomst {
    fn gelukt(klasse: Klasse, os_fout: i32) -> Self {
        Uitkomst { klasse, reden: Reden::Geen, os_fout }
    }
    fn mislukt(reden: Reden, os_fout: i32) -> Self {
        Uitkomst { klasse: Klasse::Geen, reden, os_fout }
    }
    pub fn is_gelukt(&self) -> bool {
        self.klasse != Klasse::Geen
    }
}

/// Wat we proberen. Alleen de omgevingsvariabele en de instelling bepalen dit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modus {
    /// De standaardklasse van het platform (Windows: TIME_CRITICAL, Linux:
    /// SCHED_FIFO, macOS: time-constraint).
    Auto,
    /// Windows: ook de MMCSS-taak "Pro Audio" (slechter gemeten, zie boven).
    /// Op andere platformen gelijk aan `Auto`.
    Mmcss,
    /// Niets doen.
    Uit,
}

/// Vertaalt de waarde van `JM_ORGUE_RT`. Onbekende waarden betekenen `Auto`.
pub fn modus_uit_tekst(waarde: Option<&str>) -> Modus {
    match waarde.map(|w| w.trim().to_lowercase()).as_deref() {
        Some("uit") | Some("0") | Some("nee") | Some("off") => Modus::Uit,
        Some("mmcss") => Modus::Mmcss,
        _ => Modus::Auto,
    }
}

/// Moet er, bij dit aantal werkers en kernen, op Linux een SCHED_FIFO-klasse
/// komen? Realtime-werkers spinnen tot 2 ms; als ze alle kernen bezetten kan
/// de audiothread (normale prioriteit) niet meer draaien. Er moet dus minstens
/// één kern overblijven.
#[cfg(any(target_os = "linux", test))]
pub fn fifo_veilig(n_werkers: usize, kernen: usize) -> bool {
    n_werkers < kernen
}

// ---------------------------------------------------------------------------
// Status voor de melding en de test-API
// ---------------------------------------------------------------------------

/// Telling van de uitkomsten van één pool. Een eigen type (en dus ook lokaal
/// te maken) zodat tests niet met de proces-brede telling botsen.
struct Telling {
    gelukt: AtomicUsize,
    zonder: AtomicUsize,
    klasse: AtomicU8,
    reden: AtomicU8,
    os_fout: AtomicI32,
}

/// Wat `Telling::tel` over de uitkomst zegt.
#[derive(Debug, PartialEq, Eq)]
struct Telresultaat {
    /// Dit is de eerste uitkomst van deze soort (gelukt / niet gelukt) sinds
    /// het wissen: alleen die wordt gelogd.
    eerste: bool,
}

impl Telling {
    const fn nieuw() -> Self {
        Telling {
            gelukt: AtomicUsize::new(0),
            zonder: AtomicUsize::new(0),
            klasse: AtomicU8::new(0),
            reden: AtomicU8::new(0),
            os_fout: AtomicI32::new(0),
        }
    }

    fn wis(&self) {
        self.gelukt.store(0, Ordering::Relaxed);
        self.zonder.store(0, Ordering::Relaxed);
        self.klasse.store(0, Ordering::Relaxed);
        self.reden.store(0, Ordering::Relaxed);
        self.os_fout.store(0, Ordering::Relaxed);
    }

    fn tel(&self, u: Uitkomst) -> Telresultaat {
        if u.is_gelukt() {
            self.klasse.store(u.klasse as u8, Ordering::Relaxed);
            if u.os_fout != 0 {
                self.os_fout.store(u.os_fout, Ordering::Relaxed);
            }
            Telresultaat { eerste: self.gelukt.fetch_add(1, Ordering::Relaxed) == 0 }
        } else {
            self.reden.store(u.reden as u8, Ordering::Relaxed);
            self.os_fout.store(u.os_fout, Ordering::Relaxed);
            Telresultaat { eerste: self.zonder.fetch_add(1, Ordering::Relaxed) == 0 }
        }
    }

    fn status(&self, ingesteld: bool) -> RtStatus {
        let gelukt = self.gelukt.load(Ordering::Relaxed);
        let zonder = self.zonder.load(Ordering::Relaxed);
        RtStatus {
            gelukt,
            zonder,
            klasse: if gelukt > 0 { Klasse::uit_code(self.klasse.load(Ordering::Relaxed)).naam() } else { "geen" },
            reden: if zonder > 0 { Reden::uit_code(self.reden.load(Ordering::Relaxed)).naam() } else { "" },
            os_fout: self.os_fout.load(Ordering::Relaxed),
            ingesteld,
        }
    }
}

static TELLING: Telling = Telling::nieuw();

/// Momentopname voor de interface en `/perf/slechtste`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RtStatus {
    /// Aantal werkers van de huidige pool met een realtime-klasse.
    pub gelukt: usize,
    /// Aantal werkers dat er geen kreeg (geweigerd, overgeslagen of uitgezet).
    pub zonder: usize,
    /// Klasse van de werkers die het wél kregen ("geen" als niemand).
    pub klasse: &'static str,
    /// Reden bij werkers zonder klasse ("" als alles gelukt is).
    pub reden: &'static str,
    /// Foutcode van het systeem (0 = geen).
    pub os_fout: i32,
    /// De instelling zelf.
    pub ingesteld: bool,
}

pub fn status() -> RtStatus {
    TELLING.status(is_aan())
}

/// Wist de telling; aanroepen vóór het starten van een nieuwe pool.
pub fn herstel_status() {
    TELLING.wis();
}

/// Telt een uitkomst mee en logt de eerste van elke soort per pool (zodat
/// zeven werkers niet zeven dezelfde regels geven). De logregel komt van de
/// werkerthread zelf, nooit van de audiothread.
fn registreer(u: Uitkomst) {
    if !TELLING.tel(u).eerste {
        return;
    }
    if u.is_gelukt() {
        if u.os_fout != 0 {
            tracing::info!(
                "Mengwerkers: realtime-klasse {} (de voorkeursklasse weigerde, foutcode {})",
                u.klasse.naam(), u.os_fout);
        } else {
            tracing::info!("Mengwerkers: realtime-klasse {}", u.klasse.naam());
        }
    } else {
        match u.reden {
            Reden::Uitgezet => tracing::info!("Mengwerkers: realtime-prioriteit staat uit"),
            Reden::TeVeelWerkers => tracing::warn!(
                "Mengwerkers: geen realtime-prioriteit, er zijn niet minder werkers dan kernen"),
            Reden::NietOndersteund => tracing::warn!(
                "Mengwerkers: realtime-prioriteit is op dit systeem niet beschikbaar"),
            _ => tracing::warn!(
                "Mengwerkers: het systeem weigerde realtime-prioriteit (foutcode {}); \
                 de werkers draaien op normale prioriteit",
                u.os_fout),
        }
    }
}

/// Aanroepen vanaf elke mengwerker, eenmalig, vóór de eerste ronde.
/// `n_werkers` is het aantal werkers van de pool.
pub fn voor_mengwerker(n_werkers: usize) {
    let modus = if !is_aan() {
        Modus::Uit
    } else {
        modus_uit_tekst(std::env::var("JM_ORGUE_RT").ok().as_deref())
    };
    registreer(zet_voor_deze_thread(modus, n_werkers));
}

/// Zet de klasse op de aanroepende thread.
pub fn zet_voor_deze_thread(modus: Modus, n_werkers: usize) -> Uitkomst {
    if modus == Modus::Uit {
        return Uitkomst::mislukt(Reden::Uitgezet, 0);
    }
    platform::zet(modus, n_werkers)
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod platform {
    use super::{Klasse, Modus, Reden, Uitkomst};
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::Threading::{
        AvSetMmThreadCharacteristicsW, AvSetMmThreadPriority, GetCurrentThread,
        SetThreadPriority, AVRT_PRIORITY_CRITICAL, THREAD_PRIORITY_TIME_CRITICAL,
    };

    pub fn zet(modus: Modus, _n_werkers: usize) -> Uitkomst {
        // SAFETY: gewone Win32-aanroepen op de eigen thread; de naam is een
        // nul-afgesloten UTF-16-tekst die de aanroep overleeft.
        unsafe {
            let mut mmcss_fout = 0i32;
            let mut mmcss = false;
            if modus == Modus::Mmcss {
                let naam: Vec<u16> = "Pro Audio\0".encode_utf16().collect();
                let mut taakindex = 0u32;
                let handvat = AvSetMmThreadCharacteristicsW(naam.as_ptr(), &mut taakindex);
                if handvat.is_null() {
                    mmcss_fout = GetLastError() as i32;
                } else {
                    mmcss = true;
                    // Relatieve prioriteit binnen de taak; mislukken is niet erg.
                    AvSetMmThreadPriority(handvat, AVRT_PRIORITY_CRITICAL);
                }
            }
            // TIME_CRITICAL, zoals voorheen en zoals de audiothread zelf
            // (cpal): een werker die verdrongen wordt houdt de callback op.
            let tc = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);
            if tc == 0 && !mmcss {
                return Uitkomst::mislukt(Reden::Geweigerd, GetLastError() as i32);
            }
            if mmcss {
                Uitkomst::gelukt(Klasse::MmcssProAudio, 0)
            } else {
                Uitkomst::gelukt(Klasse::TijdKritiek, mmcss_fout)
            }
        }
    }

    /// Voor de test: de huidige thread-prioriteit volgens Windows.
    #[cfg(test)]
    pub fn huidige_prioriteit() -> i32 {
        use windows_sys::Win32::System::Threading::GetThreadPriority;
        unsafe { GetThreadPriority(GetCurrentThread()) }
    }
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

/// SCHED_FIFO-prioriteit voor de werkers. Bescheiden: rtkit staat meestal tot
/// 20 toe, en de audiothread zelf blijft de baas over zijn werkers.
#[cfg(target_os = "linux")]
const LINUX_PRIORITEIT: i32 = 20;

#[cfg(target_os = "linux")]
mod platform {
    use super::{fifo_veilig, Klasse, Modus, Reden, Uitkomst, LINUX_PRIORITEIT};

    pub fn zet(_modus: Modus, n_werkers: usize) -> Uitkomst {
        let kernen = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        if !fifo_veilig(n_werkers, kernen) {
            return Uitkomst::mislukt(Reden::TeVeelWerkers, 0);
        }
        // SAFETY: pthread_setschedparam op de eigen thread met een geldige
        // sched_param; geen geheugen wordt gedeeld.
        unsafe {
            let max = libc::sched_get_priority_max(libc::SCHED_FIFO);
            let min = libc::sched_get_priority_min(libc::SCHED_FIFO);
            if max < 0 || min < 0 {
                return Uitkomst::mislukt(
                    Reden::Geweigerd,
                    std::io::Error::last_os_error().raw_os_error().unwrap_or(-1));
            }
            let param = libc::sched_param { sched_priority: LINUX_PRIORITEIT.clamp(min, max) };
            // pthread_* geeft de foutcode zelf terug (geen errno).
            let r = libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_FIFO, &param);
            if r == 0 {
                Uitkomst::gelukt(Klasse::SchedFifo, 0)
            } else {
                Uitkomst::mislukt(Reden::Geweigerd, r)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// macOS
// ---------------------------------------------------------------------------

/// Periode van het time-constraint-beleid: een buffer van 128 frames bij
/// 48 kHz (2,67 ms), de standaard van deze app.
#[cfg(target_os = "macos")]
const MACOS_PERIODE_NS: u64 = 2_666_667;
/// Rekentijd die een werker per periode mag gebruiken: de helft.
#[cfg(target_os = "macos")]
const MACOS_REKENTIJD_NS: u64 = MACOS_PERIODE_NS / 2;

#[cfg(target_os = "macos")]
mod platform {
    use super::{Klasse, Modus, Uitkomst, MACOS_PERIODE_NS, MACOS_REKENTIJD_NS};
    use super::Reden;
    use mach2::kern_return::KERN_SUCCESS;
    use mach2::mach_init::mach_thread_self;
    use mach2::mach_port::mach_port_deallocate;
    use mach2::mach_time::{mach_timebase_info, mach_timebase_info_data_t};
    use mach2::thread_policy::{
        thread_policy_set, thread_time_constraint_policy_data_t, THREAD_TIME_CONSTRAINT_POLICY,
        THREAD_TIME_CONSTRAINT_POLICY_COUNT,
    };
    use mach2::traps::mach_task_self;

    pub fn zet(_modus: Modus, _n_werkers: usize) -> Uitkomst {
        // SAFETY: Mach-aanroepen op de eigen thread. De poort van
        // mach_thread_self() is een extra verwijzing en wordt hieronder
        // weer losgelaten; het beleid wordt op de stack gevuld.
        unsafe {
            let mut tijdbasis = mach_timebase_info_data_t { numer: 0, denom: 0 };
            let kr = mach_timebase_info(&mut tijdbasis);
            if kr != KERN_SUCCESS || tijdbasis.numer == 0 {
                return Uitkomst::mislukt(Reden::Geweigerd, kr);
            }
            // Nanoseconden naar Mach-tijdseenheden.
            let naar_eenheden = |ns: u64| (ns * tijdbasis.denom as u64 / tijdbasis.numer as u64) as u32;
            let periode = naar_eenheden(MACOS_PERIODE_NS);
            let mut beleid = thread_time_constraint_policy_data_t {
                period: periode,
                computation: naar_eenheden(MACOS_REKENTIJD_NS),
                constraint: periode,
                preemptible: 1,
            };
            let thread = mach_thread_self();
            let kr = thread_policy_set(
                thread,
                THREAD_TIME_CONSTRAINT_POLICY,
                &mut beleid as *mut _ as libc::thread_policy_t,
                THREAD_TIME_CONSTRAINT_POLICY_COUNT,
            );
            mach_port_deallocate(mach_task_self(), thread);
            if kr == KERN_SUCCESS {
                Uitkomst::gelukt(Klasse::TimeConstraint, 0)
            } else {
                Uitkomst::mislukt(Reden::Geweigerd, kr)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Overige platformen
// ---------------------------------------------------------------------------

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::{Modus, Reden, Uitkomst};

    pub fn zet(_modus: Modus, _n_werkers: usize) -> Uitkomst {
        Uitkomst::mislukt(Reden::NietOndersteund, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omgevingsvariabele_wordt_goed_gelezen() {
        assert_eq!(modus_uit_tekst(None), Modus::Auto);
        assert_eq!(modus_uit_tekst(Some("")), Modus::Auto);
        assert_eq!(modus_uit_tekst(Some("auto")), Modus::Auto);
        assert_eq!(modus_uit_tekst(Some("onzin")), Modus::Auto);
        assert_eq!(modus_uit_tekst(Some("uit")), Modus::Uit);
        assert_eq!(modus_uit_tekst(Some(" UIT ")), Modus::Uit);
        assert_eq!(modus_uit_tekst(Some("0")), Modus::Uit);
        assert_eq!(modus_uit_tekst(Some("mmcss")), Modus::Mmcss);
        assert_eq!(modus_uit_tekst(Some(" MMCSS ")), Modus::Mmcss);
        // Het oude woord uit de eerste proeven betekent nu gewoon de standaard.
        assert_eq!(modus_uit_tekst(Some("tijdkritiek")), Modus::Auto);
    }

    #[test]
    fn fifo_laat_minstens_een_kern_vrij() {
        assert!(fifo_veilig(0, 1));
        assert!(fifo_veilig(3, 4));
        assert!(fifo_veilig(1, 2));
        assert!(!fifo_veilig(4, 4));
        assert!(!fifo_veilig(7, 4));
        assert!(!fifo_veilig(1, 1));
    }

    #[test]
    fn uit_doet_niets_en_meldt_dat() {
        let u = zet_voor_deze_thread(Modus::Uit, 3);
        assert!(!u.is_gelukt());
        assert_eq!(u.reden, Reden::Uitgezet);
        assert_eq!(u.os_fout, 0);
    }

    #[test]
    fn codes_lopen_heen_en_terug() {
        for k in [Klasse::Geen, Klasse::MmcssProAudio, Klasse::TijdKritiek, Klasse::SchedFifo, Klasse::TimeConstraint] {
            assert_eq!(Klasse::uit_code(k as u8), k);
        }
        for r in [Reden::Geen, Reden::Geweigerd, Reden::NietOndersteund, Reden::Uitgezet, Reden::TeVeelWerkers] {
            assert_eq!(Reden::uit_code(r as u8), r);
        }
    }

    /// Windows: de standaard is alleen TIME_CRITICAL (geen MMCSS) en dat staat
    /// echt op de thread; `Mmcss` voegt de MMCSS-taak toe. Elk op een eigen
    /// thread, zodat de testthread zelf niet verhoogd blijft.
    #[cfg(windows)]
    #[test]
    fn windows_standaard_is_tijdkritiek_zonder_mmcss() {
        use windows_sys::Win32::System::Threading::THREAD_PRIORITY_TIME_CRITICAL;
        let (auto, prio_auto) = std::thread::spawn(|| {
            let u = zet_voor_deze_thread(Modus::Auto, 3);
            (u, platform::huidige_prioriteit())
        }).join().unwrap();
        assert_eq!(auto.klasse, Klasse::TijdKritiek);
        assert_eq!(auto.reden, Reden::Geen);
        assert_eq!(auto.os_fout, 0);
        assert_eq!(prio_auto, THREAD_PRIORITY_TIME_CRITICAL);

        let (mmcss, prio_mmcss) = std::thread::spawn(|| {
            let u = zet_voor_deze_thread(Modus::Mmcss, 3);
            (u, platform::huidige_prioriteit())
        }).join().unwrap();
        assert!(mmcss.is_gelukt(), "geen klasse gekregen: {:?}", mmcss);
        // MMCSS lukt normaal; weigert het systeem, dan blijft TIME_CRITICAL.
        assert!(matches!(mmcss.klasse, Klasse::MmcssProAudio | Klasse::TijdKritiek));
        assert_eq!(prio_mmcss, THREAD_PRIORITY_TIME_CRITICAL);
    }

    /// Op elk platform: zonder instelling "uit" levert een werker of een
    /// klasse, of een duidelijke reden — nooit een lege uitkomst.
    #[test]
    fn uitkomst_is_altijd_verklaarbaar() {
        let u = std::thread::spawn(|| zet_voor_deze_thread(Modus::Auto, 1)).join().unwrap();
        if u.is_gelukt() {
            assert_eq!(u.reden, Reden::Geen);
        } else {
            assert_ne!(u.reden, Reden::Geen);
        }
    }

    #[test]
    fn status_telt_en_wordt_gewist() {
        // Lokale telling: de tests van de mengpool starten ondertussen echte
        // werkers en tellen op de proces-brede.
        let t = Telling::nieuw();
        assert!(t.tel(Uitkomst::gelukt(Klasse::SchedFifo, 0)).eerste);
        assert!(!t.tel(Uitkomst::gelukt(Klasse::SchedFifo, 0)).eerste);
        assert!(t.tel(Uitkomst::mislukt(Reden::Geweigerd, 1)).eerste);
        let s = t.status(true);
        assert_eq!((s.gelukt, s.zonder), (2, 1));
        assert_eq!(s.klasse, "sched_fifo");
        assert_eq!(s.reden, "geweigerd");
        assert_eq!(s.os_fout, 1);
        assert!(s.ingesteld);
        t.wis();
        let s = t.status(false);
        assert_eq!((s.gelukt, s.zonder), (0, 0));
        assert_eq!(s.klasse, "geen");
        assert_eq!(s.reden, "");
        assert!(!s.ingesteld);
    }

    /// De interface (Console.svelte) leest precies deze veldnamen en waarden.
    #[test]
    fn status_json_heeft_de_velden_die_de_interface_leest() {
        let t = Telling::nieuw();
        t.tel(Uitkomst::mislukt(Reden::TeVeelWerkers, 0));
        let v = serde_json::to_value(t.status(true)).unwrap();
        assert_eq!(v["ingesteld"], true);
        assert_eq!(v["gelukt"], 0);
        assert_eq!(v["zonder"], 1);
        assert_eq!(v["klasse"], "geen");
        assert_eq!(v["reden"], "te_veel_werkers");
        assert_eq!(v["os_fout"], 0);
    }

    #[test]
    fn geslaagde_terugval_bewaart_de_fout_ter_informatie() {
        let t = Telling::nieuw();
        t.tel(Uitkomst::gelukt(Klasse::TijdKritiek, 5));
        let s = t.status(true);
        assert_eq!(s.klasse, "tijdkritiek");
        assert_eq!(s.reden, "");
        assert_eq!(s.os_fout, 5);
    }
}
