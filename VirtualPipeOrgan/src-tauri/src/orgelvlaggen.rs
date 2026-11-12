//! Gedrag van de set, per orgel (E1, D3, F7, D7, E5, E3).
//!
//! Vier dingen kunnen de klank of werking van een orgel veranderen: de niveaus
//! van de maker (E1), de crossfade van de maker bij het loslaten (D3), of
//! koppels doorketenen zoals de set wil (F7) en het afkappen van de stille
//! staart van release-samples (D7). Daarom geldt de regel die de gebruiker koos:
//!
//! - **Een orgel dat al opgeslagen instellingen heeft, houdt het oude gedrag**
//!   (alles uit), zodat niemand na een update iets anders hoort.
//! - **Een nieuw orgel (nog geen opgeslagen instellingen) krijgt alles
//!   standaard aan.**
//! - **Uitzondering: de zwelkast van de set (E5) staat ook voor een nieuw orgel
//!   standaard uit**, omdat de setwaarden het zwelpedaal minder diep maken dan de
//!   vertrouwde instelling en dat een keuze met de oren is.
//! - Per orgel is elke vlag met een vinkje om te zetten; wat de gebruiker koos
//!   wordt bewaard en wint daarna altijd.
//!
//! "Nieuw" is dezelfde grens als waar de app de instelwizard aanbiedt: geen
//! opgeslagen instellingen voor dit orgel. De vlaggen worden bij het eerste
//! opslaan meteen mee bewaard, zodat een nieuw orgel de volgende keer niet
//! alsnog als bestaand telt.

use crate::library::OrganSettings;
use serde::Serialize;

/// De zes vlaggen voor één orgel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OrgelVlaggen {
    /// Niveau van het orgel en per pijplaag uit de definitie (E1).
    pub niveaus_van_set: bool,
    /// Crossfade-duur van de maker bij het loslaten (D3).
    pub overgangen_van_set: bool,
    /// Koppels ketenen zoals de set wil (F7).
    pub koppels_volgen_set: bool,
    /// Stille staart van release-samples afkappen (D7). De drempel is een
    /// algemene instelling (standaard −90 dBFS).
    pub staart_afkap: bool,
    /// De zwelkastwaarden van de set gebruiken als beginwaarde (E5).
    pub zwelkast_van_set: bool,
    /// Klavier-naar-klavierkoppels van een Hauptwerk-set inlezen (E3) in plaats van ze over te slaan (en dan
    /// algemene koppels op naam te maken).
    pub koppels_klavier: bool,
}

impl OrgelVlaggen {
    /// Het gedrag van vóór deze instellingen: alles uit.
    pub const OUD: OrgelVlaggen = OrgelVlaggen {
        niveaus_van_set: false,
        overgangen_van_set: false,
        koppels_volgen_set: false,
        staart_afkap: false,
        zwelkast_van_set: false,
        koppels_klavier: false,
    };

    /// De standaard voor een nieuw orgel: alles aan.
    pub const NIEUW: OrgelVlaggen = OrgelVlaggen {
        niveaus_van_set: true,
        overgangen_van_set: true,
        koppels_volgen_set: true,
        staart_afkap: true,
        // Uitzondering: ook voor een nieuw orgel uit (E5).
        zwelkast_van_set: false,
        koppels_klavier: true,
    };

    /// De vlaggen voor een orgel dat nu geladen wordt.
    ///
    /// - `forceer`: een expliciete keuze van buiten (de test-API voor proeven);
    ///   wint van alles.
    /// - Geen opgeslagen instellingen: nieuw orgel, alles aan.
    /// - Wel opgeslagen instellingen: per vlag wat er staat; een vlag die er nog
    ///   niet in staat (opgeslagen vóór deze instelling bestond) is uit.
    pub fn bepaal(opgeslagen: Option<&OrganSettings>, forceer: Option<OrgelVlaggen>) -> OrgelVlaggen {
        if let Some(f) = forceer {
            return f;
        }
        match opgeslagen {
            None => OrgelVlaggen::NIEUW,
            Some(s) => OrgelVlaggen {
                niveaus_van_set: s.niveaus_van_set.unwrap_or(false),
                overgangen_van_set: s.overgangen_van_set.unwrap_or(false),
                koppels_volgen_set: s.koppels_volgen_set.unwrap_or(false),
                staart_afkap: s.staart_afkap.unwrap_or(false),
                zwelkast_van_set: s.zwelkast_van_set.unwrap_or(false),
                koppels_klavier: s.koppels_klavier.unwrap_or(false),
            },
        }
    }

    /// Zet de proces-brede schakelaars van de importers vóór het parsen van dit
    /// orgel. De importers lezen ze bij het begin van elke import.
    pub fn zet_importers(&self) {
        vpo_sampler::set_niveaus_toepassen(self.niveaus_van_set);
        vpo_sampler::set_overgangen_toepassen(self.overgangen_van_set);
        vpo_sampler::set_koppels_volgen_set(self.koppels_volgen_set);
        vpo_sampler::set_staart_afkap_aan(self.staart_afkap);
        vpo_sampler::set_koppels_klavier(self.koppels_klavier);
    }

    /// Zet één vlag op naam; onbekende naam geeft None.
    pub fn met(&self, naam: &str, aan: bool) -> Option<OrgelVlaggen> {
        let mut v = *self;
        match naam {
            "niveaus_van_set" => v.niveaus_van_set = aan,
            "overgangen_van_set" => v.overgangen_van_set = aan,
            "koppels_volgen_set" => v.koppels_volgen_set = aan,
            "staart_afkap" => v.staart_afkap = aan,
            "zwelkast_van_set" => v.zwelkast_van_set = aan,
            "koppels_klavier" => v.koppels_klavier = aan,
            _ => return None,
        }
        Some(v)
    }

    /// Wat er bij het opslaan in de instellingen van het orgel komt, per vlag.
    ///
    /// Normaal de huidige vlaggen. Is de keuze door een proef geforceerd
    /// (`geforceerd`), dan blijven de opgeslagen vlaggen zoals ze waren: een
    /// geforceerde keuze mag nooit in iemands bibliotheek terechtkomen.
    pub fn te_bewaren(
        geforceerd: bool,
        huidig: OrgelVlaggen,
        opgeslagen: Option<&OrganSettings>,
    ) -> (Option<bool>, Option<bool>, Option<bool>, Option<bool>, Option<bool>, Option<bool>) {
        if geforceerd {
            match opgeslagen {
                Some(s) => (s.niveaus_van_set, s.overgangen_van_set, s.koppels_volgen_set, s.staart_afkap, s.zwelkast_van_set, s.koppels_klavier),
                None => (None, None, None, None, None, None),
            }
        } else {
            (
                Some(huidig.niveaus_van_set),
                Some(huidig.overgangen_van_set),
                Some(huidig.koppels_volgen_set),
                Some(huidig.staart_afkap),
                Some(huidig.zwelkast_van_set),
                Some(huidig.koppels_klavier),
            )
        }
    }

    /// Schrijft de vlaggen in opgeslagen instellingen (elke vlag expliciet, zodat
    /// ze daarna niet meer van de "bestaand of nieuw"-regel afhangen).
    pub fn schrijf_in(&self, s: &mut OrganSettings) {
        s.niveaus_van_set = Some(self.niveaus_van_set);
        s.overgangen_van_set = Some(self.overgangen_van_set);
        s.koppels_volgen_set = Some(self.koppels_volgen_set);
        s.staart_afkap = Some(self.staart_afkap);
        s.zwelkast_van_set = Some(self.zwelkast_van_set);
        s.koppels_klavier = Some(self.koppels_klavier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instellingen(n: Option<bool>, o: Option<bool>, k: Option<bool>) -> OrganSettings {
        OrganSettings {
            niveaus_van_set: n,
            overgangen_van_set: o,
            koppels_volgen_set: k,
            ..Default::default()
        }
    }

    fn instellingen4(n: Option<bool>, o: Option<bool>, k: Option<bool>, a: Option<bool>) -> OrganSettings {
        OrganSettings { staart_afkap: a, ..instellingen(n, o, k) }
    }

    #[test]
    fn nieuw_orgel_krijgt_alles_standaard_aan() {
        assert_eq!(OrgelVlaggen::bepaal(None, None), OrgelVlaggen::NIEUW);
        assert!(OrgelVlaggen::NIEUW.niveaus_van_set && OrgelVlaggen::NIEUW.overgangen_van_set && OrgelVlaggen::NIEUW.koppels_volgen_set);
        assert!(OrgelVlaggen::NIEUW.staart_afkap, "ook de staartafkap staat voor een nieuw orgel aan");
        assert!(!OrgelVlaggen::NIEUW.zwelkast_van_set, "de zwelkast van de set is de uitzondering: ook voor een nieuw orgel uit");
        assert!(!OrgelVlaggen::OUD.staart_afkap, "en voor een bestaand orgel uit");
    }

    #[test]
    fn bestaand_orgel_zonder_vlaggen_houdt_het_oude_gedrag() {
        // Opgeslagen vóór deze instellingen bestonden: alle drie ontbreken.
        let s = instellingen(None, None, None);
        assert_eq!(OrgelVlaggen::bepaal(Some(&s), None), OrgelVlaggen::OUD);
        // Ook een volledig lege (Default) opgeslagen instelling telt als bestaand.
        assert_eq!(OrgelVlaggen::bepaal(Some(&OrganSettings::default()), None), OrgelVlaggen::OUD);
    }

    #[test]
    fn eigen_keuze_wint_altijd_per_vlag() {
        let s = instellingen(Some(true), Some(false), None);
        let v = OrgelVlaggen::bepaal(Some(&s), None);
        assert!(v.niveaus_van_set);
        assert!(!v.overgangen_van_set);
        assert!(!v.koppels_volgen_set, "ontbrekende vlag bij een bestaand orgel = uit");
        // Expliciet uit blijft uit, ook al is het orgel nieuw geweest toen het begon.
        let alles_uit = instellingen(Some(false), Some(false), Some(false));
        assert_eq!(OrgelVlaggen::bepaal(Some(&alles_uit), None), OrgelVlaggen::OUD);
    }

    #[test]
    fn een_forcering_van_buiten_wint_van_alles() {
        let s = instellingen(Some(true), Some(true), Some(true));
        let f = OrgelVlaggen { niveaus_van_set: false, overgangen_van_set: true, koppels_volgen_set: false, staart_afkap: true, zwelkast_van_set: true, koppels_klavier: false };
        assert_eq!(OrgelVlaggen::bepaal(Some(&s), Some(f)), f);
        assert_eq!(OrgelVlaggen::bepaal(None, Some(f)), f);
    }

    /// De regel die de gebruiker koos, als doorlopend verhaal: een nieuw orgel
    /// wordt bewaard met de vlaggen erin en blijft daarna ook zo.
    #[test]
    fn nieuw_orgel_wordt_na_opslaan_niet_alsnog_bestaand() {
        let eerste = OrgelVlaggen::bepaal(None, None);
        assert_eq!(eerste, OrgelVlaggen::NIEUW);
        let mut opgeslagen = OrganSettings::default();
        eerste.schrijf_in(&mut opgeslagen);
        // Volgende keer: er zijn instellingen, maar de vlaggen staan er expliciet in.
        assert_eq!(OrgelVlaggen::bepaal(Some(&opgeslagen), None), OrgelVlaggen::NIEUW);
    }

    #[test]
    fn geforceerde_vlaggen_worden_nooit_bewaard() {
        let huidig = OrgelVlaggen::NIEUW;
        // Normaal: de huidige stand, expliciet.
        assert_eq!(OrgelVlaggen::te_bewaren(false, huidig, None), (Some(true), Some(true), Some(true), Some(true), Some(false), Some(true)));
        // Geforceerd, orgel zonder opgeslagen instellingen: niets vastleggen.
        assert_eq!(OrgelVlaggen::te_bewaren(true, huidig, None), (None, None, None, None, None, None));
        // Geforceerd, met opgeslagen instellingen: die blijven zoals ze waren.
        let s = instellingen4(Some(false), None, Some(false), Some(false));
        assert_eq!(OrgelVlaggen::te_bewaren(true, huidig, Some(&s)), (Some(false), None, Some(false), Some(false), None, None));
        // Zonder forcering wint de huidige stand over wat er stond (de gebruiker koos).
        assert_eq!(OrgelVlaggen::te_bewaren(false, huidig, Some(&s)), (Some(true), Some(true), Some(true), Some(true), Some(false), Some(true)));
    }

    #[test]
    fn een_vlag_zetten_op_naam() {
        let v = OrgelVlaggen::OUD;
        assert!(v.met("niveaus_van_set", true).unwrap().niveaus_van_set);
        assert!(v.met("overgangen_van_set", true).unwrap().overgangen_van_set);
        assert!(v.met("koppels_volgen_set", true).unwrap().koppels_volgen_set);
        assert!(v.met("staart_afkap", true).unwrap().staart_afkap);
        assert!(v.met("zwelkast_van_set", true).unwrap().zwelkast_van_set);
        assert!(v.met("koppels_klavier", true).unwrap().koppels_klavier);
        // De andere blijven zoals ze waren.
        let w = v.met("niveaus_van_set", true).unwrap();
        assert!(!w.overgangen_van_set && !w.koppels_volgen_set && !w.staart_afkap && !w.zwelkast_van_set && !w.koppels_klavier);
        assert_eq!(v.met("bestaat_niet", true), None);
    }

    #[test]
    fn json_van_de_interface_heeft_de_verwachte_velden() {
        let v = serde_json::to_value(OrgelVlaggen::NIEUW).unwrap();
        assert_eq!(v["niveaus_van_set"], true);
        assert_eq!(v["overgangen_van_set"], true);
        assert_eq!(v["koppels_volgen_set"], true);
        assert_eq!(v["staart_afkap"], true);
        assert_eq!(v["zwelkast_van_set"], false, "nieuw orgel: de zwelkast van de set staat uit");
        assert_eq!(v["koppels_klavier"], true);
    }

    /// Het orgel dat al opgeslagen is met de vier eerdere vlaggen heeft de zwelkastvlag niet:
    /// uit. Een eigen keuze (aan) wint daarna.
    #[test]
    fn zwelkast_van_set_staat_uit_tenzij_de_gebruiker_het_aanzette() {
        let s = instellingen4(Some(true), Some(true), Some(true), Some(true));
        assert!(!OrgelVlaggen::bepaal(Some(&s), None).zwelkast_van_set);
        let aan = OrganSettings { zwelkast_van_set: Some(true), ..instellingen4(None, None, None, None) };
        assert!(OrgelVlaggen::bepaal(Some(&aan), None).zwelkast_van_set);
        // Een nieuw orgel dat wordt opgeslagen blijft uit.
        let mut opgeslagen = OrganSettings::default();
        OrgelVlaggen::bepaal(None, None).schrijf_in(&mut opgeslagen);
        assert_eq!(opgeslagen.zwelkast_van_set, Some(false));
    }

    /// Een orgel dat vóór deze instelling is opgeslagen (de andere drie staan er
    /// wel, de staartafkap niet) houdt het oude gedrag: staart ongemoeid.
    #[test]
    fn bestaand_orgel_zonder_staartvlag_laat_de_staart_staan() {
        let s = instellingen4(Some(true), Some(true), Some(true), None);
        let v = OrgelVlaggen::bepaal(Some(&s), None);
        assert!(v.niveaus_van_set && v.overgangen_van_set && v.koppels_volgen_set);
        assert!(!v.staart_afkap);
        let aan = instellingen4(None, None, None, Some(true));
        assert!(OrgelVlaggen::bepaal(Some(&aan), None).staart_afkap, "een eigen keuze wint");
    }

    /// E3: een orgel dat al instellingen heeft (zonder deze vlag) houdt de koppels zoals ze waren (uit); een nieuw orgel
    /// leest de klavier-naar-klavierkoppels van de set; een eigen keuze wint.
    #[test]
    fn klavierkoppels_bestaand_uit_nieuw_aan() {
        let bestaand = instellingen4(Some(true), Some(true), Some(true), Some(true));
        assert!(!OrgelVlaggen::bepaal(Some(&bestaand), None).koppels_klavier);
        assert!(OrgelVlaggen::bepaal(None, None).koppels_klavier);
        let aan = OrganSettings { koppels_klavier: Some(true), ..instellingen4(None, None, None, None) };
        assert!(OrgelVlaggen::bepaal(Some(&aan), None).koppels_klavier);
        assert!(!OrgelVlaggen::OUD.koppels_klavier);
    }
}
