//! Basmanagement (F6): de lage tonen van de hoofdkanalen naar een subwoofer.
//!
//! Een kleine luidspreker kan het laag van een 16' of 32' niet weergeven; die
//! energie kost alleen vervorming. Basmanagement haalt het laag uit de hoofdkanalen
//! (hoogdoorlaat) en stuurt het, opgeteld, naar één subwooferkanaal (laagdoorlaat).
//! Beide filters zijn vierde-orde Linkwitz-Riley: twee gelijke Butterworth-secties
//! van de tweede orde. Het bijzondere van dat filter is dat laagdoorlaat en
//! hoogdoorlaat bij de kruisfrequentie elk 6 dB zakken en samen weer precies de
//! oorspronkelijke sterkte geven (een allpass): er ontstaat geen gat of bult.
//!
//! Vaste regel van de uitgangstrap: **staat de optie uit, dan kost ze niets en
//! blijft de uitvoer bit-identiek.** `verwerk_frame` doet dan niets en de aanroeper
//! slaat de aanroep over met [`BasManagement::is_identiteit`].
//!
//! De filters rekenen in f64 (transponeerde directe vorm II): bij een kruisfrequentie
//! van enkele tientallen Hz is f32 te onnauwkeurig. Geen allocaties in
//! `verwerk_frame`; het bouwen en wijzigen van de instelling gebeurt buiten de
//! audio-callback of bij een commando.

use std::f64::consts::PI;

/// Welke rol een uitgangskanaal heeft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KanaalRol {
    /// Hoofdluidspreker: het laag eruit, het laag naar de sub.
    Hoofd,
    /// De subwoofer: krijgt het opgetelde laag.
    Sub,
    /// Niet aangeraakt (bijvoorbeeld surround-kanalen die geen bas nodig hebben).
    Overig,
}

/// Eén tweede-orde sectie (Butterworth, Q = 1/√2) in transponeerde directe vorm II.
#[derive(Debug, Clone, Copy)]
struct Sectie {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Sectie {
    fn butterworth(laag: bool, fc_hz: f64, sample_rate: f64) -> Self {
        let w = 2.0 * PI * fc_hz / sample_rate;
        let (sin_w, cos_w) = (w.sin(), w.cos());
        let alpha = sin_w / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let (b0, b1, b2) = if laag {
            ((1.0 - cos_w) / 2.0, 1.0 - cos_w, (1.0 - cos_w) / 2.0)
        } else {
            ((1.0 + cos_w) / 2.0, -(1.0 + cos_w), (1.0 + cos_w) / 2.0)
        };
        let a0 = 1.0 + alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos_w / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    fn verwerk(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Vierde-orde Linkwitz-Riley: twee gelijke secties achter elkaar.
#[derive(Debug, Clone, Copy)]
struct Lr4 {
    a: Sectie,
    b: Sectie,
}

impl Lr4 {
    fn nieuw(laag: bool, fc_hz: f64, sample_rate: f64) -> Self {
        let s = Sectie::butterworth(laag, fc_hz, sample_rate);
        Self { a: s, b: s }
    }

    #[inline]
    fn verwerk(&mut self, x: f64) -> f64 {
        let y = self.a.verwerk(x);
        self.b.verwerk(y)
    }
}

/// Grenzen van de kruisfrequentie in Hz.
pub const KRUIS_MIN_HZ: f32 = 30.0;
pub const KRUIS_MAX_HZ: f32 = 250.0;
/// Standaard kruisfrequentie in Hz.
pub const KRUIS_STANDAARD_HZ: f32 = 80.0;

/// De instelling van het basmanagement, zoals de gebruiker haar kiest.
#[derive(Debug, Clone, PartialEq)]
pub struct BasInstelling {
    pub aan: bool,
    pub kruisfrequentie_hz: f32,
    /// Het uitgangskanaal van de subwoofer (0-gebaseerd).
    pub sub_kanaal: usize,
    /// Niveau van de sub ten opzichte van de hoofdkanalen in dB.
    pub sub_niveau_db: f32,
    /// Welke kanalen hoofdluidsprekers zijn; kanalen daarbuiten (behalve de sub) blijven ongemoeid.
    /// Een lege lijst betekent: alle kanalen behalve de sub.
    pub hoofdkanalen: Vec<usize>,
}

impl Default for BasInstelling {
    fn default() -> Self {
        Self { aan: false, kruisfrequentie_hz: KRUIS_STANDAARD_HZ, sub_kanaal: 2, sub_niveau_db: 0.0, hoofdkanalen: vec![0, 1] }
    }
}

/// Het basmanagement van de uitgang.
pub struct BasManagement {
    sample_rate: f64,
    aan: bool,
    sub: usize,
    sub_gain: f64,
    rollen: Vec<KanaalRol>,
    hoog: Vec<Lr4>,
    laag: Lr4,
}

impl BasManagement {
    /// Uit: doet niets.
    pub fn nieuw(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate as f64,
            aan: false,
            sub: 0,
            sub_gain: 1.0,
            rollen: Vec::new(),
            hoog: Vec::new(),
            laag: Lr4::nieuw(true, KRUIS_STANDAARD_HZ as f64, sample_rate as f64),
        }
    }

    /// Zet een instelling voor een uitgang met `kanalen` kanalen. Een ongeldige instelling
    /// (sub buiten het apparaat, geen hoofdkanaal, sub ook als hoofdkanaal genoemd) zet de
    /// optie uit. De kruisfrequentie wordt begrensd op [`KRUIS_MIN_HZ`]…[`KRUIS_MAX_HZ`].
    pub fn stel_in(&mut self, i: &BasInstelling, kanalen: usize) {
        let hoofd: Vec<usize> = if i.hoofdkanalen.is_empty() {
            (0..kanalen).filter(|c| *c != i.sub_kanaal).collect()
        } else {
            i.hoofdkanalen.iter().copied().filter(|c| *c < kanalen && *c != i.sub_kanaal).collect()
        };
        self.aan = i.aan && i.sub_kanaal < kanalen && !hoofd.is_empty();
        if !self.aan {
            self.rollen.clear();
            self.hoog.clear();
            return;
        }
        let fc = (if i.kruisfrequentie_hz.is_finite() { i.kruisfrequentie_hz } else { KRUIS_STANDAARD_HZ })
            .clamp(KRUIS_MIN_HZ, KRUIS_MAX_HZ) as f64;
        self.sub = i.sub_kanaal;
        self.sub_gain = 10f64.powf(i.sub_niveau_db.clamp(-24.0, 12.0) as f64 / 20.0);
        self.rollen = vec![KanaalRol::Overig; kanalen];
        for c in &hoofd {
            self.rollen[*c] = KanaalRol::Hoofd;
        }
        self.rollen[i.sub_kanaal] = KanaalRol::Sub;
        self.hoog = vec![Lr4::nieuw(false, fc, self.sample_rate); kanalen];
        self.laag = Lr4::nieuw(true, fc, self.sample_rate);
    }

    /// Doet deze trap niets? (De aanroeper slaat `verwerk_frame` dan over.)
    #[inline]
    pub fn is_identiteit(&self) -> bool {
        !self.aan
    }

    /// Verwerkt één frame (één sample per uitgangskanaal) ter plekke. Geen allocaties.
    #[inline]
    pub fn verwerk_frame(&mut self, frame: &mut [f32]) {
        if !self.aan {
            return;
        }
        let n = frame.len().min(self.rollen.len());
        let mut som = 0.0f64;
        for c in 0..n {
            if self.rollen[c] == KanaalRol::Hoofd {
                let x = frame[c] as f64;
                som += x;
                frame[c] = self.hoog[c].verwerk(x) as f32;
            }
        }
        let laag = self.laag.verwerk(som) * self.sub_gain;
        if self.sub < frame.len() {
            frame[self.sub] += laag as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    /// Amplitude (piek) van de uitvoer van een sinus door `verwerk`, gemeten na inzwaaien.
    fn amplitude(f: f64, mut verwerk: impl FnMut(f64) -> f64) -> f64 {
        let n = (SR as f64 * 1.5) as usize;
        let mut piek = 0.0f64;
        for i in 0..n {
            let x = (2.0 * PI * f * i as f64 / SR as f64).sin();
            let y = verwerk(x);
            if i > n / 2 {
                piek = piek.max(y.abs());
            }
        }
        piek
    }

    fn db(x: f64) -> f64 {
        20.0 * x.log10()
    }

    #[test]
    fn laag_en_hoog_zakken_zes_db_bij_de_kruisfrequentie_en_geven_samen_een_vlakke_som() {
        let fc = 80.0;
        for f in [20.0, 40.0, 60.0, 80.0, 120.0, 160.0, 320.0, 1000.0, 4000.0] {
            let mut lp = Lr4::nieuw(true, fc, SR as f64);
            let mut hp = Lr4::nieuw(false, fc, SR as f64);
            let som = amplitude(f, |x| lp.verwerk(x) + hp.verwerk(x));
            // LP + HP is een allpass: de amplitude blijft 1 (binnen 0,05 dB), ook rond de kruisfrequentie.
            assert!(db(som).abs() < 0.05, "som bij {f} Hz: {:.3} dB", db(som));
        }
        let mut lp = Lr4::nieuw(true, fc, SR as f64);
        let mut hp = Lr4::nieuw(false, fc, SR as f64);
        let (l, h) = (amplitude(fc, |x| lp.verwerk(x)), amplitude(fc, |x| hp.verwerk(x)));
        assert!((db(l) + 6.02).abs() < 0.1, "laagdoorlaat bij fc: {:.2} dB", db(l));
        assert!((db(h) + 6.02).abs() < 0.1, "hoogdoorlaat bij fc: {:.2} dB", db(h));
    }

    #[test]
    fn de_helling_is_vierentwintig_db_per_octaaf() {
        let fc = 80.0;
        let mut hp = Lr4::nieuw(false, fc, SR as f64);
        let mut lp = Lr4::nieuw(true, fc, SR as f64);
        let hoog_onder = db(amplitude(fc / 2.0, |x| hp.verwerk(x)));
        let laag_boven = db(amplitude(fc * 2.0, |x| lp.verwerk(x)));
        // Een octaaf naast de kruisfrequentie ligt de afwijking rond −24 dB (met het aandeel van de bocht erbij).
        assert!(hoog_onder < -22.0 && hoog_onder > -28.0, "hoogdoorlaat een octaaf lager: {:.1} dB", hoog_onder);
        assert!(laag_boven < -22.0 && laag_boven > -28.0, "laagdoorlaat een octaaf hoger: {:.1} dB", laag_boven);
    }

    fn standaard(kanalen: usize) -> BasManagement {
        let mut b = BasManagement::nieuw(SR);
        b.stel_in(&BasInstelling { aan: true, ..BasInstelling::default() }, kanalen);
        b
    }

    /// De vaste regel: staat de optie uit, dan blijft de uitvoer bit voor bit gelijk.
    #[test]
    fn uitgeschakeld_laat_de_uitvoer_bit_identiek() {
        let mut uit = BasManagement::nieuw(SR);
        assert!(uit.is_identiteit());
        // Ook na een instelling met aan = false, en na een ongeldige instelling met aan = true.
        let mut uit2 = BasManagement::nieuw(SR);
        uit2.stel_in(&BasInstelling { aan: false, ..BasInstelling::default() }, 4);
        let mut ongeldig = BasManagement::nieuw(SR);
        ongeldig.stel_in(&BasInstelling { aan: true, sub_kanaal: 9, ..BasInstelling::default() }, 4); // sub buiten het apparaat
        assert!(uit2.is_identiteit() && ongeldig.is_identiteit());
        let monsters = [0.0f32, -0.0, 1.0e-40, 0.1, -0.7, 0.999_999, -1.0];
        for b in [&mut uit, &mut uit2, &mut ongeldig] {
            for &x in &monsters {
                let mut frame = [x, -x, 0.25, 0.5];
                let voor = frame;
                b.verwerk_frame(&mut frame);
                assert!(frame.iter().zip(&voor).all(|(a, b)| a.to_bits() == b.to_bits()), "uitvoer veranderd bij {x:e}");
            }
        }
    }

    #[test]
    fn de_sub_krijgt_de_som_van_het_laag_en_de_hoofdkanalen_verliezen_het() {
        let mut b = standaard(4);
        let f = 30.0; // ruim onder de kruisfrequentie van 80 Hz
        let n = (SR as f64 * 1.5) as usize;
        let (mut sub_piek, mut links_piek, mut achter_piek) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..n {
            let x = (2.0 * PI * f * i as f64 / SR as f64).sin() as f32;
            let mut frame = [x, x, 0.0, 0.5]; // L, R, sub, een kanaal dat niet meedoet
            b.verwerk_frame(&mut frame);
            if i > n / 2 {
                sub_piek = sub_piek.max(frame[2].abs());
                links_piek = links_piek.max(frame[0].abs());
                achter_piek = achter_piek.max((frame[3] - 0.5).abs());
            }
        }
        // Som van twee gelijke kanalen: ongeveer 2 × de amplitude, bij 30 Hz iets onder de kruisfrequentie van 80 Hz.
        assert!(sub_piek > 1.8 && sub_piek < 2.1, "sub {sub_piek}");
        assert!(links_piek < 0.15, "links hoort het laag nauwelijks nog: {links_piek}");
        assert_eq!(achter_piek, 0.0, "een kanaal dat geen hoofdkanaal of sub is, blijft ongemoeid");
    }

    #[test]
    fn hoge_tonen_blijven_in_de_hoofdkanalen_en_de_sub_hoort_ze_niet() {
        let mut b = standaard(3);
        let f = 1000.0;
        let n = (SR as f64 * 1.0) as usize;
        let (mut sub_piek, mut links_piek) = (0.0f32, 0.0f32);
        for i in 0..n {
            let x = (2.0 * PI * f * i as f64 / SR as f64).sin() as f32;
            let mut frame = [x, 0.0, 0.0];
            b.verwerk_frame(&mut frame);
            if i > n / 2 {
                sub_piek = sub_piek.max(frame[2].abs());
                links_piek = links_piek.max(frame[0].abs());
            }
        }
        assert!(links_piek > 0.99 && links_piek < 1.01, "links {links_piek}");
        assert!(sub_piek < 0.001, "sub {sub_piek}");
    }

    #[test]
    fn de_sub_telt_op_bij_wat_het_kanaal_al_draagt_en_het_niveau_is_instelbaar() {
        let mut b = BasManagement::nieuw(SR);
        b.stel_in(&BasInstelling { aan: true, sub_niveau_db: -6.0, ..BasInstelling::default() }, 3);
        let n = (SR as f64 * 1.5) as usize;
        let mut sub_piek = 0.0f32;
        for i in 0..n {
            let x = (2.0 * PI * 20.0 * i as f64 / SR as f64).sin() as f32;
            let mut frame = [x, x, 0.1];
            b.verwerk_frame(&mut frame);
            if i > n / 2 {
                sub_piek = sub_piek.max(frame[2]);
            }
        }
        // 2 × (iets minder dan 1) × 0,5 (−6 dB) = ruim onder de 1, plus de 0,1 die er al stond.
        assert!(sub_piek > 0.85 && sub_piek < 1.15, "sub {sub_piek}");
    }

    #[test]
    fn ongeldige_instellingen_zetten_de_optie_uit_en_geven_nooit_een_paniek() {
        let mut b = BasManagement::nieuw(SR);
        // Geen hoofdkanaal over: de sub is zelf het enige genoemde kanaal.
        b.stel_in(&BasInstelling { aan: true, sub_kanaal: 0, hoofdkanalen: vec![0], ..BasInstelling::default() }, 2);
        assert!(b.is_identiteit());
        // Hoofdkanalen buiten het apparaat vallen weg; blijft er niets over, dan uit.
        b.stel_in(&BasInstelling { aan: true, sub_kanaal: 1, hoofdkanalen: vec![5, 6], ..BasInstelling::default() }, 2);
        assert!(b.is_identiteit());
        // Een frame korter dan het aantal ingestelde kanalen: geen paniek, de sub valt dan buiten het frame.
        b.stel_in(&BasInstelling { aan: true, ..BasInstelling::default() }, 4);
        let mut kort = [0.5f32, 0.5];
        b.verwerk_frame(&mut kort);
        assert!(kort.iter().all(|x| x.is_finite()));
        // Onzinnige kruisfrequentie (NaN, nul, oneindig) wordt begrensd en geeft eindige uitvoer.
        for fc in [f32::NAN, 0.0, f32::INFINITY, 1.0e9] {
            b.stel_in(&BasInstelling { aan: true, kruisfrequentie_hz: fc, ..BasInstelling::default() }, 4);
            let mut frame = [0.3f32, -0.2, 0.0, 0.0];
            for _ in 0..1000 {
                b.verwerk_frame(&mut frame);
                frame = [0.3, -0.2, 0.0, 0.0];
            }
            assert!(frame.iter().all(|x| x.is_finite()), "niet eindig bij fc {fc}");
        }
    }

    #[test]
    fn een_lege_lijst_hoofdkanalen_betekent_alle_kanalen_behalve_de_sub() {
        let mut b = BasManagement::nieuw(SR);
        b.stel_in(&BasInstelling { aan: true, sub_kanaal: 2, hoofdkanalen: Vec::new(), ..BasInstelling::default() }, 4);
        assert!(!b.is_identiteit());
        // Kanaal 3 is dan ook een hoofdkanaal: een laag signaal erop wordt weggehaald.
        let n = (SR as f64 * 1.0) as usize;
        let mut piek = 0.0f32;
        for i in 0..n {
            let x = (2.0 * PI * 30.0 * i as f64 / SR as f64).sin() as f32;
            let mut frame = [0.0, 0.0, 0.0, x];
            b.verwerk_frame(&mut frame);
            if i > n / 2 {
                piek = piek.max(frame[3].abs());
            }
        }
        assert!(piek < 0.15, "kanaal 3 hoort het laag niet meer te houden: {piek}");
        // Met alleen een sub (en verder niets) blijft er geen hoofdkanaal over: uit.
        let mut een = BasManagement::nieuw(SR);
        een.stel_in(&BasInstelling { aan: true, sub_kanaal: 0, hoofdkanalen: Vec::new(), ..BasInstelling::default() }, 1);
        assert!(een.is_identiteit());
    }

    #[test]
    fn de_kruisfrequentie_wordt_begrensd() {
        // Onder het minimum telt het minimum: een sinus van 25 Hz staat dan nog in de hoofdkanalen op −6 dB-lijn van 30 Hz.
        let mut laag = BasManagement::nieuw(SR);
        laag.stel_in(&BasInstelling { aan: true, kruisfrequentie_hz: 5.0, ..BasInstelling::default() }, 3);
        let mut gelijk = BasManagement::nieuw(SR);
        gelijk.stel_in(&BasInstelling { aan: true, kruisfrequentie_hz: KRUIS_MIN_HZ, ..BasInstelling::default() }, 3);
        for i in 0..2000 {
            let x = (i as f32 * 0.01).sin();
            let (mut a, mut b) = ([x, x, 0.0], [x, x, 0.0]);
            laag.verwerk_frame(&mut a);
            gelijk.verwerk_frame(&mut b);
            assert!(a.iter().zip(&b).all(|(p, q)| p.to_bits() == q.to_bits()), "5 Hz hoort als {KRUIS_MIN_HZ} Hz te werken");
        }
    }
}
