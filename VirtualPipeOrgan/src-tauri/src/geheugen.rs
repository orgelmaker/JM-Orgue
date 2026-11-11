//! Geheugenstand en geheugenbudget (F1).
//!
//! Tot nu toe kende de app alleen op Windows het fysieke geheugen, en alleen
//! het totaal: het plafond voor volledig geladen samples was een vast deel van
//! het RAM, ook als een browser of een tweede orgel er al de helft van had.
//! Deze module vraagt per platform zowel het totaal als wat er nu nog
//! beschikbaar is, en leidt daar bij elke orgellading een plafond uit af:
//!
//! - Windows: `GlobalMemoryStatusEx` (beschikbaar = vrij + standby).
//! - Linux: `/proc/meminfo` (`MemTotal`, `MemAvailable`).
//! - macOS: `sysctl hw.memsize` voor het totaal, `host_statistics64` voor
//!   vrije plus inactieve pagina's.
//!
//! Er blijft altijd een reserve over voor het systeem en andere programma's:
//! het grootste van 256 MB en een twintigste van het geheugen. De instelling
//! (`audio::sample_cap_bytes`) blijft de bovengrens; het afgeleide plafond kan
//! alleen lager uitvallen. Faalt de aanvraag, dan verandert er niets.
//!
//! Voor proeven kan `JM_ORGUE_VRIJ_MB` het beschikbare geheugen overschrijven,
//! zodat de krapte-route te testen is zonder het geheugen echt vol te zetten.

use std::sync::atomic::{AtomicBool, Ordering};

/// Wat het systeem nu aan geheugen heeft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct GeheugenStand {
    /// Fysiek geheugen in bytes.
    pub totaal: u64,
    /// Wat een programma nu nog kan nemen zonder dat het systeem iets hoeft
    /// weg te schrijven, in bytes.
    pub beschikbaar: u64,
}

/// Ondergrens van de reserve.
const RESERVE_MIN: u64 = 256_000_000;
/// Het afgeleide plafond zakt nooit onder dit bedrag: daaronder werkt de
/// sample-cache niet meer zinvol (elk akkoord zou alles weer wegduwen).
pub const VRIJ_PLAFOND_MIN: usize = 256_000_000;

/// Reserve voor het systeem en andere programma's: het grootste van 256 MB en
/// een twintigste van het geheugen.
pub fn reserve_bytes(totaal: u64) -> u64 {
    RESERVE_MIN.max(totaal / 20)
}

/// Aan/uit-instelling (voorkeur "geheugenplafond_volgt_vrij"); standaard aan.
static VOLGT_VRIJ: AtomicBool = AtomicBool::new(true);

pub fn zet_volgt_vrij(aan: bool) {
    VOLGT_VRIJ.store(aan, Ordering::Relaxed);
}

pub fn volgt_vrij() -> bool {
    VOLGT_VRIJ.load(Ordering::Relaxed)
}

/// Het plafond voor volledig geladen samples, afgeleid van wat er nu vrij is.
///
/// - `instelling`: het plafond zoals ingesteld (de bovengrens).
/// - `resident`: wat de sample-cache nu al in het geheugen heeft; dat is al van
///   ons en telt dus niet af van wat er nog vrij is.
/// - `preload_geschat`: wat het orgel dat nu geladen wordt aan preloads vraagt
///   (komt nog bovenop wat nu vrij is).
///
/// Ruimte = beschikbaar − reserve − preload; budget = resident + ruimte. Het
/// resultaat is nooit hoger dan de instelling en nooit lager dan
/// `VRIJ_PLAFOND_MIN`.
pub fn plafond_voor_vrij(
    instelling: usize,
    resident: usize,
    stand: GeheugenStand,
    preload_geschat: u64,
) -> usize {
    let ruimte = stand.beschikbaar
        .saturating_sub(reserve_bytes(stand.totaal))
        .saturating_sub(preload_geschat);
    let budget = (resident as u64).saturating_add(ruimte);
    let budget = usize::try_from(budget).unwrap_or(usize::MAX);
    instelling.min(budget.max(VRIJ_PLAFOND_MIN))
}

/// Past een preload-schatting in wat er nu vrij is (na de reserve)?
pub fn past_in_vrij(stand: GeheugenStand, preload_geschat: u64) -> bool {
    preload_geschat <= stand.beschikbaar.saturating_sub(reserve_bytes(stand.totaal))
}

/// Preload-schatting van de laatste orgellading in bytes (voor de weergave).
static LAATSTE_PRELOAD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Aantal preload-buffers van de laatste orgellading, los van de lengte van de startbuffer: daaruit volgt
/// het geheugen bij elke keuze van de startbuffer (F2). 0 = onbekend (nog geen orgel, of een samplemap
/// waarvoor de schatting niet gemaakt wordt).
static LAATSTE_BUFFERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn zet_laatste_buffers(n: u64) {
    LAATSTE_BUFFERS.store(n, Ordering::Relaxed);
}

pub fn laatste_buffers() -> u64 {
    LAATSTE_BUFFERS.load(Ordering::Relaxed)
}

/// Stelt het plafond voor volledig geladen samples in op wat er nu vrij is.
/// Aanroepen bij elke orgellading, buiten de audiothread, zodra de preload-
/// schatting van het orgel bekend is. Staat het volgen uit of is de stand niet
/// op te vragen, dan wordt de beperking opgeheven en geldt alleen de instelling.
pub fn pas_plafond_toe(preload_geschat: u64) {
    LAATSTE_PRELOAD.store(preload_geschat, Ordering::Relaxed);
    let instelling = crate::audio::sample_cap_instelling_bytes();
    let stand = if volgt_vrij() { stand() } else { None };
    let Some(stand) = stand else {
        crate::audio::set_sample_cap_vrij_bytes(0);
        return;
    };
    let resident = crate::audio::samples_resident_bytes();
    let plafond = plafond_voor_vrij(instelling, resident, stand, preload_geschat);
    // Alleen een beperking als het afgeleide plafond lager uitvalt.
    crate::audio::set_sample_cap_vrij_bytes(if plafond < instelling { plafond } else { 0 });
    tracing::info!(
        "Geheugen: {} MB totaal, {} MB beschikbaar (reserve {} MB); orgel vraagt ~{} MB preload; \
         plafond volledig geladen samples {} MB (ingesteld {} MB)",
        stand.totaal / 1_000_000, stand.beschikbaar / 1_000_000,
        reserve_bytes(stand.totaal) / 1_000_000, preload_geschat / 1_000_000,
        plafond / 1_000_000, instelling / 1_000_000);
}

/// Rekent het plafond opnieuw uit met de preload-schatting van de laatste
/// lading (nadat het volgen aan- of uitgezet is).
pub fn herbereken() {
    pas_plafond_toe(LAATSTE_PRELOAD.load(Ordering::Relaxed));
}

/// Voor Instellingen en de test-API: de stand en de plafonds in MB.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeheugenDto {
    pub totaal_mb: Option<u64>,
    pub beschikbaar_mb: Option<u64>,
    pub reserve_mb: Option<u64>,
    /// Het plafond zoals ingesteld.
    pub plafond_instelling_mb: u64,
    /// Het plafond dat nu geldt (ingesteld, of lager door het vrije geheugen).
    pub plafond_mb: u64,
    /// Wat de sample-cache nu in het geheugen heeft.
    pub resident_mb: u64,
    /// Preload-schatting van de laatste lading.
    pub laatste_preload_mb: u64,
    pub volgt_vrij: bool,
}

pub fn dto() -> GeheugenDto {
    let s = stand();
    GeheugenDto {
        totaal_mb: s.map(|s| s.totaal / 1_000_000),
        beschikbaar_mb: s.map(|s| s.beschikbaar / 1_000_000),
        reserve_mb: s.map(|s| reserve_bytes(s.totaal) / 1_000_000),
        plafond_instelling_mb: (crate::audio::sample_cap_instelling_bytes() / 1_000_000) as u64,
        plafond_mb: (crate::audio::sample_cap_bytes() / 1_000_000) as u64,
        resident_mb: (crate::audio::samples_resident_bytes() / 1_000_000) as u64,
        laatste_preload_mb: LAATSTE_PRELOAD.load(Ordering::Relaxed) / 1_000_000,
        volgt_vrij: volgt_vrij(),
    }
}

/// Overschrijving van het beschikbare geheugen voor proeven (in MB).
fn vrij_overschrijving_mb(waarde: Option<&str>) -> Option<u64> {
    waarde.and_then(|w| w.trim().parse::<u64>().ok())
}

/// De stand van het geheugen nu, of None als het systeem het niet geeft.
pub fn stand() -> Option<GeheugenStand> {
    let mut s = platform::stand()?;
    if let Some(mb) = vrij_overschrijving_mb(std::env::var("JM_ORGUE_VRIJ_MB").ok().as_deref()) {
        s.beschikbaar = (mb * 1_000_000).min(s.totaal);
    }
    Some(s)
}

/// Leest `MemTotal` en `MemAvailable` (kB) uit de tekst van `/proc/meminfo`.
/// Zuiver, zodat het op elk platform te testen is. Oude kernels zonder
/// `MemAvailable` geven None: dan is er geen eerlijke schatting.
#[cfg(any(target_os = "linux", test))]
pub fn parse_meminfo(tekst: &str) -> Option<GeheugenStand> {
    let kb = |sleutel: &str| -> Option<u64> {
        tekst.lines()
            .find_map(|l| l.strip_prefix(sleutel))
            .and_then(|rest| rest.trim_start_matches(':').split_whitespace().next())
            .and_then(|n| n.parse::<u64>().ok())
    };
    let totaal = kb("MemTotal")? * 1024;
    let beschikbaar = kb("MemAvailable")? * 1024;
    if totaal == 0 {
        return None;
    }
    Some(GeheugenStand { totaal, beschikbaar: beschikbaar.min(totaal) })
}

#[cfg(windows)]
mod platform {
    use super::GeheugenStand;
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    pub fn stand() -> Option<GeheugenStand> {
        // SAFETY: MEMORYSTATUSEX is plain data; dwLength is gezet zoals de API eist.
        unsafe {
            let mut st: MEMORYSTATUSEX = std::mem::zeroed();
            st.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut st) == 0 {
                return None;
            }
            Some(GeheugenStand { totaal: st.ullTotalPhys, beschikbaar: st.ullAvailPhys })
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{parse_meminfo, GeheugenStand};

    pub fn stand() -> Option<GeheugenStand> {
        parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::GeheugenStand;

    // libc markeert de Mach-aanroepen als verouderd ten gunste van `mach2`, maar
    // `mach2` heeft `host_statistics64` niet; de libc-versies blijven dus.
    #[allow(deprecated)]
    pub fn stand() -> Option<GeheugenStand> {
        // SAFETY: sysctlbyname krijgt een nul-afgesloten naam en een u64-buffer
        // van de opgegeven grootte; host_statistics64 vult een vm_statistics64
        // met het aantal woorden dat HOST_VM_INFO64_COUNT aangeeft.
        unsafe {
            let mut totaal: u64 = 0;
            let mut grootte = std::mem::size_of::<u64>();
            let naam = b"hw.memsize\0";
            if libc::sysctlbyname(
                naam.as_ptr() as *const libc::c_char,
                &mut totaal as *mut u64 as *mut libc::c_void,
                &mut grootte,
                std::ptr::null_mut(),
                0,
            ) != 0 || totaal == 0 {
                return None;
            }
            let mut vm: libc::vm_statistics64 = std::mem::zeroed();
            let mut aantal = libc::HOST_VM_INFO64_COUNT;
            let kr = libc::host_statistics64(
                libc::mach_host_self(),
                libc::HOST_VM_INFO64,
                &mut vm as *mut libc::vm_statistics64 as libc::host_info64_t,
                &mut aantal,
            );
            if kr != 0 {
                return None;
            }
            let pagina = libc::sysconf(libc::_SC_PAGESIZE);
            if pagina <= 0 {
                return None;
            }
            // Vrij plus inactief: wat het systeem zonder wegschrijven kan vrijgeven.
            let paginas = vm.free_count as u64 + vm.inactive_count as u64;
            Some(GeheugenStand { totaal, beschikbaar: (paginas * pagina as u64).min(totaal) })
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::GeheugenStand;

    pub fn stand() -> Option<GeheugenStand> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    fn stand(totaal: u64, beschikbaar: u64) -> GeheugenStand {
        GeheugenStand { totaal, beschikbaar }
    }

    #[test]
    fn reserve_is_het_grootste_van_256_mb_en_een_twintigste() {
        assert_eq!(reserve_bytes(2 * GB), 256_000_000); // 100 MB < 256 MB
        assert_eq!(reserve_bytes(8 * GB), 400_000_000); // 8 GB / 20
        assert_eq!(reserve_bytes(64 * GB), 3_200_000_000);
        assert_eq!(reserve_bytes(0), 256_000_000);
    }

    #[test]
    fn plafond_blijft_de_instelling_als_er_ruim_vrij_is() {
        // 32 GB, 24 GB vrij: ruim genoeg voor een plafond van 9,6 GB.
        let p = plafond_voor_vrij(9_600_000_000, 0, stand(32 * GB, 24 * GB), 2 * GB);
        assert_eq!(p, 9_600_000_000);
    }

    #[test]
    fn plafond_zakt_als_er_weinig_vrij_is() {
        // 16 GB totaal, 4 GB vrij, preload 1 GB, reserve 800 MB:
        // ruimte = 4,0 − 0,8 − 1,0 = 2,2 GB < instelling 4,8 GB.
        let p = plafond_voor_vrij(4_800_000_000, 0, stand(16 * GB, 4 * GB), GB);
        assert_eq!(p, 2_200_000_000);
    }

    #[test]
    fn eigen_resident_geheugen_telt_mee_als_budget() {
        // Zelfde situatie, maar de cache heeft al 1,5 GB: dat is al van ons en
        // hoeft niet uit het vrije geheugen.
        let p = plafond_voor_vrij(4_800_000_000, 1_500_000_000, stand(16 * GB, 4 * GB), GB);
        assert_eq!(p, 3_700_000_000);
    }

    #[test]
    fn plafond_zakt_nooit_onder_het_minimum() {
        // Preload groter dan alles wat er vrij is.
        let p = plafond_voor_vrij(4_800_000_000, 0, stand(8 * GB, GB), 3 * GB);
        assert_eq!(p, VRIJ_PLAFOND_MIN);
    }

    #[test]
    fn plafond_stijgt_nooit_boven_de_instelling() {
        let p = plafond_voor_vrij(1_500_000_000, 10 * GB as usize, stand(64 * GB, 60 * GB), 0);
        assert_eq!(p, 1_500_000_000);
    }

    #[test]
    fn instelling_onder_het_minimum_blijft_zoals_ingesteld() {
        // De instelling is de bovengrens, ook als die lager is dan het minimum.
        let p = plafond_voor_vrij(200_000_000, 0, stand(8 * GB, 0), 0);
        assert_eq!(p, 200_000_000);
    }

    #[test]
    fn preload_past_of_past_niet() {
        let s = stand(16 * GB, 4 * GB); // reserve 800 MB → 3,2 GB over
        assert!(past_in_vrij(s, 3_200_000_000));
        assert!(!past_in_vrij(s, 3_200_000_001));
        assert!(!past_in_vrij(stand(8 * GB, 100_000_000), 1));
    }

    #[test]
    fn meminfo_wordt_gelezen() {
        let tekst = "MemTotal:       16384000 kB\nMemFree:         1000000 kB\nMemAvailable:    8192000 kB\nBuffers:          100000 kB\n";
        let s = parse_meminfo(tekst).unwrap();
        assert_eq!(s.totaal, 16_384_000 * 1024);
        assert_eq!(s.beschikbaar, 8_192_000 * 1024);
    }

    #[test]
    fn meminfo_zonder_memavailable_of_leeg_geeft_none() {
        assert_eq!(parse_meminfo("MemTotal: 1000 kB\nMemFree: 500 kB\n"), None);
        assert_eq!(parse_meminfo(""), None);
        assert_eq!(parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB\n"), None);
        assert_eq!(parse_meminfo("MemTotal: abc kB\nMemAvailable: 1 kB\n"), None);
    }

    #[test]
    fn meminfo_beschikbaar_wordt_op_het_totaal_afgekapt() {
        let s = parse_meminfo("MemTotal: 1000 kB\nMemAvailable: 5000 kB\n").unwrap();
        assert_eq!(s.beschikbaar, s.totaal);
    }

    #[test]
    fn overschrijving_van_het_beschikbare_geheugen() {
        assert_eq!(vrij_overschrijving_mb(Some("1500")), Some(1500));
        assert_eq!(vrij_overschrijving_mb(Some(" 800 ")), Some(800));
        assert_eq!(vrij_overschrijving_mb(Some("veel")), None);
        assert_eq!(vrij_overschrijving_mb(None), None);
    }

    /// Het systeem geeft op elk ondersteund platform een bruikbare stand.
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    #[test]
    fn het_systeem_geeft_een_bruikbare_stand() {
        let s = platform::stand().expect("geen geheugenstand van het systeem");
        assert!(s.totaal >= 512_000_000, "totaal onwaarschijnlijk klein: {}", s.totaal);
        assert!(s.beschikbaar > 0, "niets beschikbaar?");
        assert!(s.beschikbaar <= s.totaal);
    }
}
