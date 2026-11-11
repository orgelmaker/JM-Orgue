//! Stille staart van release-samples afkappen (D7).
//!
//! Een release-sample (de opname van de kerkakoestiek na het loslaten van een
//! toets) eindigt vaak in een stuk dat zo zacht is dat niemand het hoort: een
//! uitgefade of weggedeeld staartje, of ruis onder de kwantiseringsgrens van de
//! opname. Zo'n stem blijft toch een stemplaats bezetten en wordt tot het eind
//! van het bestand meegemengd. Metingen op twee echte sets: bij een drempel van
//! −90 dBFS is gemiddeld 8 à 15 % van alle release-frames zulke stille staart,
//! bij −70 dBFS 28 à 33 %.
//!
//! Wat hier gebeurt, en niet meer:
//!
//! - Alleen de **staart** gaat weg: alles tot en met het laatste 10-ms-venster
//!   waarin de piek de drempel haalt blijft **bit voor bit** gelijk. De afkap valt
//!   dus op een stuk dat zachter is dan de drempel (standaard −90 dBFS).
//! - Alleen voor samples **zonder looppunten**. Een sample met loop is een
//!   aangehouden klank (of een release uit hetzelfde bestand als de aanslag) en
//!   wordt nooit geknipt; de engine loopt zonder looppunten bovendien over de
//!   laatste driekwart van het bestand, dus knippen zou daar de klank veranderen.
//! - De aanroeper beslist óf er geknipt wordt (alleen release-samples, alleen
//!   als het orgel de afkap aan heeft). De drempel is proces-breed, gezet vanuit
//!   de instelling; de afkap werkt bij het laden, dus na het opnieuw laden van
//!   het orgel.
//!
//! Dit draait op de laadthread, nooit in de audio-callback.

use crate::loader::SampleData;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Standaarddrempel in dBFS.
pub const STAART_AFKAP_STANDAARD_DB: f32 = -90.0;
/// Laagste en hoogste toegestane drempel in dBFS. Boven −60 begint een staart
/// hoorbaar te worden; onder −120 valt er niets meer af.
pub const STAART_AFKAP_MIN_DB: f32 = -120.0;
pub const STAART_AFKAP_MAX_DB: f32 = -60.0;

/// Staat de afkap aan? Per orgel gezet (zie `orgelvlaggen.rs` in de app) bij het
/// laden van het orgel.
static STAART_AFKAP_AAN: AtomicBool = AtomicBool::new(false);
/// Bitpatroon van de gekozen drempel; 0 = nog niet gekozen (de standaard).
/// Een drempel van precies 0 dBFS bestaat niet: het bereik eindigt op −60.
static STAART_DREMPEL: AtomicU32 = AtomicU32::new(0);

/// Klemt een gekozen drempel binnen het toegestane bereik. Niet-eindige
/// waarden geven de standaard.
pub fn geldige_staart_drempel(db: f32) -> f32 {
    if db.is_finite() {
        db.clamp(STAART_AFKAP_MIN_DB, STAART_AFKAP_MAX_DB)
    } else {
        STAART_AFKAP_STANDAARD_DB
    }
}

/// Zet de afkap aan of uit (proces-breed; de laadthreads lezen dit bij elke
/// geladen release-sample).
pub fn set_staart_afkap_aan(aan: bool) {
    STAART_AFKAP_AAN.store(aan, Ordering::Relaxed);
}

/// Staat de afkap aan?
pub fn staart_afkap_aan() -> bool {
    STAART_AFKAP_AAN.load(Ordering::Relaxed)
}

/// Zet de drempel in dBFS (geklemd op −120…−60).
pub fn set_staart_drempel(db: f32) {
    STAART_DREMPEL.store(geldige_staart_drempel(db).to_bits(), Ordering::Relaxed);
}

/// De gekozen drempel in dBFS (standaard −90).
pub fn staart_drempel() -> f32 {
    match STAART_DREMPEL.load(Ordering::Relaxed) {
        0 => STAART_AFKAP_STANDAARD_DB,
        bits => f32::from_bits(bits),
    }
}

/// De drempel waarmee nu geknipt moet worden, of `None` als de afkap uit staat.
pub fn staart_afkap() -> Option<f32> {
    if staart_afkap_aan() { Some(staart_drempel()) } else { None }
}

/// Knipt de stille staart van `sample` af en geeft het aantal verwijderde
/// frames terug (0 = niets veranderd).
///
/// Behouden blijft alles tot en met het laatste venster van 10 ms waarin de
/// piek (over beide kanalen) de drempel haalt, en altijd minstens één venster.
/// Samples met looppunten, een lege sample en een ongeldige drempel blijven
/// ongemoeid.
pub fn knip_stille_staart(sample: &mut SampleData, drempel_db: f32) -> usize {
    if sample.loop_start.is_some() || sample.loop_end.is_some() || !drempel_db.is_finite() {
        return 0;
    }
    let n = sample.data.len();
    if n == 0 {
        return 0;
    }
    if let Some(r) = &sample.right {
        if r.len() != n {
            return 0; // ongelijke kanalen: niet aanraken
        }
    }
    let venster = (sample.sample_rate as usize / 100).max(1);
    let drempel = 10f32.powf(drempel_db / 20.0);
    let rechts = sample.right.as_deref();
    let laatste_luide = (0..n).rev().find(|&i| {
        sample.data[i].abs() >= drempel || rechts.is_some_and(|r| r[i].abs() >= drempel)
    });
    let behoud = match laatste_luide {
        Some(i) => ((i / venster) + 1).saturating_mul(venster).min(n),
        // Alles stil: één venster laten staan, zodat de sample niet leeg wordt.
        None => venster.min(n),
    };
    if behoud >= n {
        return 0;
    }
    sample.data.truncate(behoud);
    sample.data.shrink_to_fit();
    if let Some(r) = sample.right.as_mut() {
        r.truncate(behoud);
        r.shrink_to_fit();
    }
    n - behoud
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    fn sample(data: Vec<f32>, right: Option<Vec<f32>>) -> SampleData {
        let channels = if right.is_some() { 2 } else { 1 };
        SampleData { data, right, sample_rate: SR, channels, loop_start: None, loop_end: None }
    }

    /// 1 s luid (0,5) gevolgd door een staart van `staart_s` seconden op `niveau`
    /// (lineair), met een bekend kenmerk per frame zodat bit-gelijkheid te zien is.
    fn met_staart(luid_s: f32, staart_s: f32, niveau: f32) -> Vec<f32> {
        let nl = (luid_s * SR as f32) as usize;
        let ns = (staart_s * SR as f32) as usize;
        let mut v: Vec<f32> = (0..nl).map(|i| 0.5 * ((i as f32) * 0.01).sin() + 0.1).collect();
        v.extend((0..ns).map(|i| niveau * (((i as f32) * 0.37).sin())));
        v
    }

    fn db(d: f32) -> f32 { 10f32.powf(d / 20.0) }

    #[test]
    fn de_stille_staart_valt_af_en_het_luide_deel_blijft_bit_voor_bit() {
        let origineel = met_staart(1.0, 2.0, db(-100.0)); // staart ver onder −90
        let mut s = sample(origineel.clone(), None);
        let weg = knip_stille_staart(&mut s, -90.0);
        assert!(weg > 0, "er hoort iets af te vallen");
        assert_eq!(s.data.len() + weg, origineel.len());
        // Alles wat blijft is exact gelijk aan het begin van het origineel.
        assert!(s.data.iter().zip(&origineel).all(|(a, b)| a.to_bits() == b.to_bits()));
        // De afkap valt kort na het luide deel (binnen één venster van 10 ms).
        let luid = (1.0 * SR as f32) as usize;
        assert!(s.data.len() >= luid && s.data.len() <= luid + SR as usize / 100 + 1, "lengte {}", s.data.len());
    }

    #[test]
    fn een_staart_boven_de_drempel_blijft_staan() {
        let origineel = met_staart(1.0, 2.0, db(-60.0)); // staart ruim boven −90
        let mut s = sample(origineel.clone(), None);
        assert_eq!(knip_stille_staart(&mut s, -90.0), 0);
        assert_eq!(s.data.len(), origineel.len());
    }

    #[test]
    fn de_drempel_is_instelbaar_en_werkt_in_de_goede_richting() {
        let staart = db(-75.0);
        let origineel = met_staart(1.0, 2.0, staart);
        let mut bij_90 = sample(origineel.clone(), None);
        let mut bij_70 = sample(origineel.clone(), None);
        assert_eq!(knip_stille_staart(&mut bij_90, -90.0), 0, "−75 dB is luider dan −90 dB");
        assert!(knip_stille_staart(&mut bij_70, -70.0) > 0, "−75 dB is zachter dan −70 dB");
        assert!(bij_70.data.len() < bij_90.data.len());
    }

    #[test]
    fn precies_op_de_drempel_telt_als_hoorbaar() {
        // Een frame dat de drempel precies haalt (>=) houdt zijn venster.
        let mut data = vec![0.0f32; SR as usize];
        let i = 30_000;
        data[i] = db(-90.0);
        let mut s = sample(data, None);
        let weg = knip_stille_staart(&mut s, -90.0);
        let venster = SR as usize / 100;
        assert_eq!(s.data.len(), (i / venster + 1) * venster);
        assert!(weg > 0);
    }

    #[test]
    fn een_sample_met_looppunten_wordt_nooit_geknipt() {
        let mut s = sample(met_staart(1.0, 2.0, db(-110.0)), None);
        let lengte = s.data.len();
        s.loop_start = Some(1000);
        s.loop_end = Some(40_000);
        assert_eq!(knip_stille_staart(&mut s, -90.0), 0);
        assert_eq!(s.data.len(), lengte);
        // Ook met alleen één looppunt.
        s.loop_end = None;
        assert_eq!(knip_stille_staart(&mut s, -90.0), 0);
    }

    #[test]
    fn stereo_telt_het_luidste_kanaal() {
        let n = 3 * SR as usize;
        let links = met_staart(1.0, 2.0, db(-110.0));
        // Rechts heeft een luide klik halverwege de staart.
        let mut rechts = vec![0.0f32; n];
        rechts[2 * SR as usize] = 0.3;
        let mut s = SampleData { data: links.clone(), right: Some(rechts), sample_rate: SR, channels: 2, loop_start: None, loop_end: None };
        let weg = knip_stille_staart(&mut s, -90.0);
        let venster = SR as usize / 100;
        assert_eq!(s.data.len(), (2 * SR as usize / venster + 1) * venster, "rechts bepaalt het einde");
        assert_eq!(s.right.as_ref().unwrap().len(), s.data.len(), "beide kanalen even lang");
        assert_eq!(weg, n - s.data.len());
    }

    #[test]
    fn een_volledig_stille_sample_houdt_een_venster() {
        let mut s = sample(vec![0.0; SR as usize], None);
        let weg = knip_stille_staart(&mut s, -90.0);
        assert_eq!(s.data.len(), SR as usize / 100);
        assert_eq!(weg, SR as usize - SR as usize / 100);
        // Een sample korter dan een venster blijft heel.
        let mut kort = sample(vec![0.0; 100], None);
        assert_eq!(knip_stille_staart(&mut kort, -90.0), 0);
        assert_eq!(kort.data.len(), 100);
    }

    #[test]
    fn lege_sample_ongeldige_drempel_en_ongelijke_kanalen_blijven_ongemoeid() {
        let mut leeg = sample(vec![], None);
        assert_eq!(knip_stille_staart(&mut leeg, -90.0), 0);
        let mut s = sample(met_staart(0.5, 1.0, db(-110.0)), None);
        let lengte = s.data.len();
        assert_eq!(knip_stille_staart(&mut s, f32::NAN), 0);
        assert_eq!(knip_stille_staart(&mut s, f32::NEG_INFINITY), 0);
        assert_eq!(s.data.len(), lengte);
        let mut scheef = SampleData { data: vec![0.0; 1000], right: Some(vec![0.0; 900]), sample_rate: SR, channels: 2, loop_start: None, loop_end: None };
        assert_eq!(knip_stille_staart(&mut scheef, -90.0), 0);
        assert_eq!(scheef.data.len(), 1000);
    }

    #[test]
    fn drempel_wordt_geklemd_en_uit_is_uit() {
        assert_eq!(geldige_staart_drempel(-200.0), STAART_AFKAP_MIN_DB);
        assert_eq!(geldige_staart_drempel(-10.0), STAART_AFKAP_MAX_DB);
        assert_eq!(geldige_staart_drempel(-80.0), -80.0);
        assert_eq!(geldige_staart_drempel(f32::NAN), STAART_AFKAP_STANDAARD_DB);
        // De proces-brede schakelaars: tests lopen parallel, dus alles in één
        // test, en de afkap staat aan het eind weer uit.
        assert_eq!(staart_drempel(), STAART_AFKAP_STANDAARD_DB, "zonder keuze geldt de standaard");
        assert_eq!(staart_afkap(), None, "standaard uit");
        set_staart_afkap_aan(true);
        assert_eq!(staart_afkap(), Some(STAART_AFKAP_STANDAARD_DB));
        set_staart_drempel(-85.0);
        assert_eq!(staart_afkap(), Some(-85.0));
        set_staart_drempel(-5.0);
        assert_eq!(staart_drempel(), STAART_AFKAP_MAX_DB, "geklemd");
        set_staart_afkap_aan(false);
        assert_eq!(staart_afkap(), None);
        assert_eq!(staart_drempel(), STAART_AFKAP_MAX_DB, "de drempel blijft bewaard als de afkap uit gaat");
    }
}
