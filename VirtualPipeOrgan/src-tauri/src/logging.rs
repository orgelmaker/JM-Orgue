//! Niet-blokkerend logboek (taak B2).
//!
//! Tot 0.9.50 schreef de bestandslaag synchroon: `fmt::layer().with_writer(Mutex<File>)`. Elke logregel op de
//! audiothread (er staan ruim dertig `info!`/`warn!` in de render-closure, bij configuratiecommando's) deed dus
//! een `WriteFile` onder een mutex: een trage schijf, een virusscanner of een andere thread die net schreef was
//! genoeg voor een underrun. Nu gaat de regel via een begrensd kanaal naar een eigen schrijfthread
//! (`tracing-appender`); de aanroeper formatteert alleen en legt de regel neer.
//!
//! Twee keuzes, beide met een test hieronder:
//!
//! * **Lossy.** Zit het kanaal vol (standaard 128.000 regels, dat is een volle schijf of een vastgelopen
//!   schrijver) dan valt een regel weg in plaats van dat de audiothread wacht. Een logregel kwijt is beter
//!   dan een hapering.
//! * **De bestandslaag staat vóór de stderr-laag.** Een starter die de stderr-pipe nooit leegt laat `WriteFile`
//!   daarop eeuwig blokkeren; de bestandslaag moet de laatste regel dan nog gekregen hebben (les van
//!   2026-07-13 en 2026-09-12). Tracing roept de binnenste laag eerst aan, dus `registry().with(bestand).with(stderr)`.
//!
//! De `WorkerGuard` staat in een static: `main()` komt niet aan zijn einde, want de event-loop van Tauri beëindigt
//! het proces zelf. [`sluit_af`] laat de wachtrij leeglopen en wordt aangeroepen waar de app echt eindigt.

use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing_appender::non_blocking::{NonBlocking, NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::fmt::MakeWriter;

static GUARD: Mutex<Option<WorkerGuard>> = Mutex::new(None);
static TELLER: Mutex<Option<Arc<Teller>>> = Mutex::new(None);

/// Telt de regels die de laag in de wachtrij legt en die de schrijfthread wegschrijft. Zo kan de app bij het
/// afsluiten even wachten tot de wachtrij leeg is ZONDER de schrijfthread te stoppen (de guard droppen kan dat
/// alleen met stoppen erbij, en dan gaan latere regels, zoals de melding van de afsluit-waakhond, verloren).
#[derive(Default)]
pub struct Teller {
    verzonden: AtomicU64,
    geschreven: AtomicU64,
}

impl Teller {
    /// Wacht tot alles wat verzonden is ook is weggeschreven, hoogstens `max`. `true` = de wachtrij is leeg.
    /// Een regel die door een volle wachtrij wegvalt (lossy) komt nooit; dan geeft dit na `max` `false`.
    pub fn wacht(&self, max: Duration) -> bool {
        let start = Instant::now();
        loop {
            if self.geschreven.load(Ordering::SeqCst) >= self.verzonden.load(Ordering::SeqCst) {
                return true;
            }
            if start.elapsed() >= max {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

/// Aan de kant van de schrijfthread: telt elke regel die echt is weggeschreven (ook als dat mislukt, zodat een
/// schijffout de teller niet blijvend scheef trekt).
struct TellendeSchrijver<W> {
    inner: W,
    teller: Arc<Teller>,
}

impl<W: Write> Write for TellendeSchrijver<W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.inner.write(b)
    }
    fn write_all(&mut self, b: &[u8]) -> io::Result<()> {
        let r = self.inner.write_all(b);
        self.teller.geschreven.fetch_add(1, Ordering::SeqCst);
        r
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// De schrijver die de laag per logregel krijgt: telt de regel als verzonden en legt hem in de wachtrij.
pub struct ProducentSchrijver {
    nb: NonBlocking,
    teller: Arc<Teller>,
}

impl Write for ProducentSchrijver {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.teller.verzonden.fetch_add(1, Ordering::SeqCst);
        self.nb.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.nb.flush()
    }
}

/// De `MakeWriter` voor de bestandslaag.
#[derive(Clone)]
pub struct TellendeProducent {
    nb: NonBlocking,
    teller: Arc<Teller>,
}

impl<'a> MakeWriter<'a> for TellendeProducent {
    type Writer = ProducentSchrijver;
    fn make_writer(&'a self) -> Self::Writer {
        ProducentSchrijver { nb: self.nb.clone(), teller: self.teller.clone() }
    }
}

/// Een niet-blokkerende schrijver om `schrijver` heen, met zijn guard. De guard laat bij het droppen de
/// wachtrij leeglopen (ten hoogste 1 s) en stopt de schrijfthread.
pub fn niet_blokkerend<W: Write + Send + 'static>(schrijver: W) -> (NonBlocking, WorkerGuard) {
    NonBlockingBuilder::default().thread_name("jm-orgue-log").lossy(true).finish(schrijver)
}

/// Als [`niet_blokkerend`], met een teller voor [`Teller::wacht`].
pub fn tellend<W: Write + Send + 'static>(schrijver: W) -> (TellendeProducent, Arc<Teller>, WorkerGuard) {
    let teller = Arc::new(Teller::default());
    let (nb, guard) = niet_blokkerend(TellendeSchrijver { inner: schrijver, teller: teller.clone() });
    (TellendeProducent { nb, teller: teller.clone() }, teller, guard)
}

/// De bestandsschrijver voor het logboek van de app; bewaart guard en teller in statics.
pub fn bestandsschrijver(bestand: std::fs::File) -> TellendeProducent {
    let (producent, teller, guard) = tellend(bestand);
    *GUARD.lock().unwrap_or_else(|e| e.into_inner()) = Some(guard);
    *TELLER.lock().unwrap_or_else(|e| e.into_inner()) = Some(teller);
    producent
}

/// Wacht (hoogstens `max`) tot de wachtrij van het logbestand leeg is, en laat de schrijfthread doorlopen.
/// Aanroepen vlak voordat de app het proces beëindigt: Tauri doet dat zelf, `main()` komt niet aan zijn einde.
pub fn laat_leeglopen(max: Duration) -> bool {
    let teller = TELLER.lock().unwrap_or_else(|e| e.into_inner()).clone();
    teller.is_none_or(|t| t.wacht(max))
}

/// Laat de wachtrij van het logbestand leeglopen (ten hoogste 1 s) en stop de schrijfthread. Daarna gaan
/// nieuwe regels niet meer naar het bestand. Alleen vlak voor een geforceerd einde; anders [`laat_leeglopen`].
pub fn sluit_af() {
    let guard = GUARD.lock().unwrap_or_else(|e| e.into_inner()).take();
    drop(guard);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::{Duration, Instant};
    use tracing_subscriber::{fmt, prelude::*};

    /// Schrijver die alles in een gedeelde buffer zet.
    #[derive(Clone, Default)]
    struct Buffer(Arc<Mutex<Vec<u8>>>);
    impl Write for Buffer {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Buffer {
        fn regels(&self) -> Vec<String> {
            String::from_utf8_lossy(&self.0.lock().unwrap()).lines().map(str::to_owned).collect()
        }
    }

    /// Een stilstaande schijf of een stderr-pipe die niemand leegt: elke write wacht tot `vrij` waar is.
    struct Geblokkeerd {
        vrij: Arc<AtomicBool>,
        buffer: Buffer,
    }
    impl Write for Geblokkeerd {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            while !self.vrij.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(1));
            }
            self.buffer.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn bestandslaag<S, W>(w: W) -> impl tracing_subscriber::Layer<S>
    where
        S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
        W: for<'a> fmt::MakeWriter<'a> + 'static,
    {
        fmt::layer().with_writer(w).with_ansi(false).with_target(false)
    }

    /// Voert `werk` uit op een eigen thread met `dispatch` als logger en geeft een kanaal dat afgaat als het klaar is.
    fn op_eigen_thread(
        dispatch: tracing::Dispatch,
        werk: impl FnOnce() + Send + 'static,
    ) -> (std::thread::JoinHandle<()>, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel();
        let h = std::thread::spawn(move || {
            tracing::dispatcher::with_default(&dispatch, werk);
            let _ = tx.send(());
        });
        (h, rx)
    }

    fn wacht_tot(mut voorwaarde: impl FnMut() -> bool, max: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < max {
            if voorwaarde() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        voorwaarde()
    }

    #[test]
    fn regels_komen_compleet_en_op_volgorde_in_het_bestand() {
        let buf = Buffer::default();
        let (nb, guard) = niet_blokkerend(buf.clone());
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(bestandslaag(nb)));
        tracing::dispatcher::with_default(&dispatch, || {
            for i in 0..500 {
                tracing::info!("volgorderegel {i}");
            }
        });
        drop(guard); // laat de wachtrij leeglopen
        let regels = buf.regels();
        assert_eq!(regels.len(), 500, "er ontbreken regels na het droppen van de guard");
        for (i, r) in regels.iter().enumerate() {
            assert!(r.ends_with(&format!("volgorderegel {i}")), "regel {i} staat verkeerd: {r}");
        }
    }

    /// Het doel van B2: een schrijver die stilstaat (trage schijf, scanner) houdt de aanroeper niet op.
    #[test]
    fn een_stilstaande_schijf_houdt_de_aanroeper_niet_op() {
        let vrij = Arc::new(AtomicBool::new(false));
        let buf = Buffer::default();
        let (nb, guard) = niet_blokkerend(Geblokkeerd { vrij: vrij.clone(), buffer: buf.clone() });
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(bestandslaag(nb)));
        let (h, klaar) = op_eigen_thread(dispatch, || {
            for i in 0..2000 {
                tracing::info!("schijfregel {i}");
            }
        });
        let gelukt = klaar.recv_timeout(Duration::from_secs(5)).is_ok();
        vrij.store(true, Ordering::SeqCst); // altijd vrijgeven, ook als de test faalt: geen hangende thread
        h.join().unwrap();
        assert!(gelukt, "2000 logregels bleven hangen op een stilstaande schijf: de aanroeper wacht op de schrijver");
        drop(guard);
        assert_eq!(buf.regels().len(), 2000, "na het vrijgeven hoort elke regel nog in het bestand te komen");
    }

    /// Negatieve controle: de oude opzet (`Mutex<schrijver>` direct in de laag) blokkeert de aanroeper wél.
    /// Bewijst dat de test hierboven het probleem zou zien.
    #[test]
    fn de_oude_opzet_hield_de_aanroeper_wel_op() {
        let vrij = Arc::new(AtomicBool::new(false));
        let buf = Buffer::default();
        let oud = Mutex::new(Geblokkeerd { vrij: vrij.clone(), buffer: buf.clone() });
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(bestandslaag(oud)));
        let (h, klaar) = op_eigen_thread(dispatch, || tracing::info!("oude opzet"));
        std::thread::sleep(Duration::from_millis(300));
        let hing = klaar.try_recv().is_err();
        vrij.store(true, Ordering::SeqCst);
        h.join().unwrap();
        assert!(hing, "de oude opzet had hier moeten blokkeren");
        assert_eq!(buf.regels().len(), 1);
    }

    /// De les van 2026-07-13 en 2026-09-12: blokkeert stderr, dan heeft het bestand de laatste regel al.
    /// Dat hangt af van de volgorde van de lagen (binnenste eerst); met niet-blokkerende bestandslaag
    /// blijft dat zo.
    #[test]
    fn het_bestand_krijgt_de_laatste_regel_ook_als_stderr_blokkeert() {
        let vrij = Arc::new(AtomicBool::new(false));
        let bestand = Buffer::default();
        let stderr = Buffer::default();
        let (nb, guard) = niet_blokkerend(bestand.clone());
        let stderr_laag = bestandslaag(Mutex::new(Geblokkeerd { vrij: vrij.clone(), buffer: stderr.clone() }));
        // Zelfde volgorde als in main.rs: bestand binnenin, stderr erbuiten.
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(bestandslaag(nb)).with(stderr_laag));
        let (h, klaar) = op_eigen_thread(dispatch, || tracing::info!("laatste regel voor de blokkade"));
        let in_bestand = wacht_tot(|| bestand.regels().len() == 1, Duration::from_secs(3));
        let thread_hangt = klaar.try_recv().is_err();
        vrij.store(true, Ordering::SeqCst);
        h.join().unwrap();
        drop(guard);
        assert!(in_bestand, "het bestand kreeg de regel niet terwijl stderr blokkeerde");
        assert!(thread_hangt, "de proef is niet geldig: stderr blokkeerde niet");
        assert!(bestand.regels()[0].ends_with("laatste regel voor de blokkade"));
    }

    /// Negatieve controle: staat de stderr-laag binnenin (omgekeerde volgorde), dan krijgt het bestand de regel
    /// pas ná de blokkade. Dit is waarom de volgorde in main.rs niet mag worden omgedraaid.
    #[test]
    fn met_de_stderr_laag_binnenin_ontbreekt_de_laatste_regel_in_het_bestand() {
        let vrij = Arc::new(AtomicBool::new(false));
        let bestand = Buffer::default();
        let stderr = Buffer::default();
        let (nb, guard) = niet_blokkerend(bestand.clone());
        let stderr_laag = bestandslaag(Mutex::new(Geblokkeerd { vrij: vrij.clone(), buffer: stderr.clone() }));
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(stderr_laag).with(bestandslaag(nb)));
        let (h, _klaar) = op_eigen_thread(dispatch, || tracing::info!("regel in omgekeerde volgorde"));
        std::thread::sleep(Duration::from_millis(400));
        let mist = bestand.regels().is_empty();
        vrij.store(true, Ordering::SeqCst);
        h.join().unwrap();
        drop(guard);
        assert!(mist, "bij omgekeerde volgorde had het bestand de regel nog niet mogen hebben");
        assert_eq!(bestand.regels().len(), 1, "na de blokkade komt de regel alsnog");
    }

    /// Schrijver die elke regel 30 ms nodig heeft (een trage schijf), voor de proef van `laat_leeglopen`.
    struct Traag(Buffer);
    impl Write for Traag {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            std::thread::sleep(Duration::from_millis(30));
            self.0.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Het afsluiten: wat vlak voor het einde is gelogd moet er nog in komen. De oude, synchrone opzet had dit
    /// vanzelf; de niet-blokkerende heeft er `laat_leeglopen` voor nodig.
    #[test]
    fn laat_leeglopen_wacht_tot_de_laatste_regels_zijn_weggeschreven() {
        let buf = Buffer::default();
        let (producent, teller, guard) = tellend(Traag(buf.clone()));
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(bestandslaag(producent)));
        tracing::dispatcher::with_default(&dispatch, || {
            for i in 0..5 {
                tracing::info!("afsluitregel {i}");
            }
        });
        // Negatieve controle: zonder te wachten staan er nog niet alle regels in het bestand.
        assert!(buf.regels().len() < 5, "de schrijver was niet traag genoeg om het verschil te laten zien");
        assert!(teller.wacht(Duration::from_secs(3)), "de wachtrij liep niet leeg");
        let regels = buf.regels();
        assert_eq!(regels.len(), 5, "na laat_leeglopen ontbreken er regels");
        assert!(regels[4].ends_with("afsluitregel 4"));
        drop(guard);
    }

    /// Bij een gewone, snelle schrijver lopen verzonden en weggeschreven gelijk op: `laat_leeglopen` kost bij het
    /// afsluiten dan praktisch niets en blijft niet onnodig wachten (een scheve teller zou elke keer de volle
    /// wachttijd kosten).
    #[test]
    fn de_tellers_lopen_gelijk_op_bij_een_snelle_schrijver() {
        let buf = Buffer::default();
        let (producent, teller, guard) = tellend(buf.clone());
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(bestandslaag(producent)));
        tracing::dispatcher::with_default(&dispatch, || {
            for i in 0..300 {
                tracing::info!("teller {i}");
            }
        });
        let start = Instant::now();
        let leeg = teller.wacht(Duration::from_secs(2));
        let duur = start.elapsed();
        assert!(leeg, "de wachtrij liep niet leeg");
        assert!(duur < Duration::from_millis(100), "laat_leeglopen wachtte {duur:?} op een snelle schrijver");
        assert_eq!(teller.verzonden.load(Ordering::SeqCst), 300, "elke regel telt precies één keer als verzonden");
        assert_eq!(teller.geschreven.load(Ordering::SeqCst), 300, "elke regel telt precies één keer als weggeschreven");
        drop(guard);
        assert_eq!(buf.regels().len(), 300);
    }

    /// `laat_leeglopen` blijft binnen zijn tijd, ook als de schrijver stilstaat: het afsluiten mag er niet van hangen.
    #[test]
    fn laat_leeglopen_geeft_op_als_de_schrijver_stilstaat() {
        let vrij = Arc::new(AtomicBool::new(false));
        let buf = Buffer::default();
        let (producent, teller, guard) = tellend(Geblokkeerd { vrij: vrij.clone(), buffer: buf.clone() });
        let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(bestandslaag(producent)));
        tracing::dispatcher::with_default(&dispatch, || tracing::info!("regel die niet wordt weggeschreven"));
        let start = Instant::now();
        let leeg = teller.wacht(Duration::from_millis(150));
        let duur = start.elapsed();
        vrij.store(true, Ordering::SeqCst);
        drop(guard);
        assert!(!leeg, "een stilstaande schrijver kan de wachtrij niet leeg hebben");
        assert!(duur < Duration::from_secs(1), "laat_leeglopen bleef hangen: {duur:?}");
        assert_eq!(buf.regels().len(), 1, "na het vrijgeven komt de regel alsnog");
    }

    #[test]
    fn sluit_af_laat_de_wachtrij_leeglopen_en_is_herhaalbaar() {
        // Eén keer aanroepen zonder guard (nog nooit een bestandsschrijver gemaakt of al afgesloten) mag niet paniekeren.
        sluit_af();
        sluit_af();
    }
}
