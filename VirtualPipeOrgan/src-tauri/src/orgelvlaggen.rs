//! Gedrag van de set, per orgel (E1, D3, F7, D7, E5, E3, D4, C5).
//!
//! Elke vlag hier kan de klank of werking van een orgel veranderen, bijvoorbeeld
//! de niveaus van de maker (E1), de crossfade van de maker bij het loslaten (D3),
//! koppels doorketenen zoals de set wil (F7) of het afkappen van de stille
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
//!
//! # Een vlag toevoegen
//!
//! 1. Een regel in de `orgelvlaggen!`-lijst hieronder (naam, standaard voor een
//!    nieuw orgel, korte naam voor het logboek).
//! 2. Een veld `pub <naam>: Option<bool>` met `#[serde(default)]` in
//!    `library::OrganSettings` (de naam in het instellingenbestand).
//! 3. `<naam>: None` in de `OrganSettings { .. }` van `do_save_organ_settings`;
//!    de compiler wijst die plek aan. De waarde komt van `bewaar_in`.
//!
//! Bepalen, zetten op naam, bewaren, loggen en de test-API volgen dan vanzelf.

use crate::library::OrganSettings;
use serde::Serialize;

/// Maakt `OrgelVlaggen` en alles wat per vlag hetzelfde is uit één lijst, zodat
/// een vlag op één plek staat in plaats van in elke functie apart.
macro_rules! orgelvlaggen {
    ($( $(#[$doc:meta])* $naam:ident: nieuw = $nieuw:expr, log = $log:expr; )+) => {
        /// De vlaggen voor één orgel.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
        pub struct OrgelVlaggen {
            $( $(#[$doc])* pub $naam: bool, )+
        }

        impl OrgelVlaggen {
            /// De namen van alle vlaggen, zoals in het instellingenbestand en de interface.
            pub const NAMEN: &'static [&'static str] = &[ $( stringify!($naam), )+ ];

            /// Het gedrag van vóór deze instellingen: alles uit.
            pub const OUD: OrgelVlaggen = OrgelVlaggen { $( $naam: false, )+ };

            /// De standaard voor een nieuw orgel.
            pub const NIEUW: OrgelVlaggen = OrgelVlaggen { $( $naam: $nieuw, )+ };

            /// Vlaggen uit een functie die per naam aan of uit geeft.
            pub fn uit(waarde: impl Fn(&str) -> bool) -> OrgelVlaggen {
                OrgelVlaggen { $( $naam: waarde(stringify!($naam)), )+ }
            }

            /// Wat er in opgeslagen instellingen staat; een vlag die er nog niet in
            /// staat (opgeslagen vóór die vlag bestond) is uit.
            fn uit_opgeslagen(s: &OrganSettings) -> OrgelVlaggen {
                OrgelVlaggen { $( $naam: s.$naam.unwrap_or(false), )+ }
            }

            /// Zet één vlag op naam; onbekende naam geeft None.
            pub fn met(&self, naam: &str, aan: bool) -> Option<OrgelVlaggen> {
                let mut v = *self;
                match naam {
                    $( stringify!($naam) => v.$naam = aan, )+
                    _ => return None,
                }
                Some(v)
            }

            /// Schrijft de vlaggen in opgeslagen instellingen (elke vlag expliciet, zodat
            /// ze daarna niet meer van de "bestaand of nieuw"-regel afhangen).
            pub fn schrijf_in(&self, s: &mut OrganSettings) {
                $( s.$naam = Some(self.$naam); )+
            }

            /// Neemt de vlaggen over zoals ze in `bron` staan, ook een ontbrekende (None).
            fn kopieer_opgeslagen(bron: Option<&OrganSettings>, doel: &mut OrganSettings) {
                $( doel.$naam = bron.and_then(|s| s.$naam); )+
            }

            /// Eén regel voor het logboek: "niveaus aan, overgangen uit, ...".
            pub fn logregel(&self) -> String {
                [ $( format!("{} {}", $log, if self.$naam { "aan" } else { "uit" }), )+ ].join(", ")
            }
        }
    };
}

orgelvlaggen! {
    /// Niveau van het orgel en per pijplaag uit de definitie (E1).
    niveaus_van_set: nieuw = true, log = "niveaus";
    /// Crossfade-duur van de maker bij het loslaten (D3).
    overgangen_van_set: nieuw = true, log = "overgangen";
    /// Koppels ketenen zoals de set wil (F7).
    koppels_volgen_set: nieuw = true, log = "koppels volgen de set";
    /// Stille staart van release-samples afkappen (D7). De drempel is een
    /// algemene instelling (standaard −90 dBFS).
    staart_afkap: nieuw = true, log = "stille staart afkappen";
    /// De zwelkastwaarden van de set gebruiken als beginwaarde (E5). De uitzondering:
    /// ook voor een nieuw orgel uit.
    zwelkast_van_set: nieuw = false, log = "zwelkast van de set";
    /// Klavier-naar-klavierkoppels van een Hauptwerk-set inlezen (E3) in plaats van ze over te slaan (en dan
    /// algemene koppels op naam te maken).
    koppels_klavier: nieuw = true, log = "klavierkoppels van de set";
    /// Het windmodel sturen met het windwerk van de set (D4): windgroepen per balg, verbruik per pijp, en de
    /// middenstand van balggrootte en maximale daling uit de balg.
    windwerk_van_set: nieuw = true, log = "windwerk van de set";
    /// De tremulant volgt de opgenomen golfvorm van de set (C5, stap 2): vorm en snelheid per divisie, diepte per pijp.
    tremulant_golf_van_set: nieuw = true, log = "tremulant-golfvorm van de set";
}

impl OrgelVlaggen {
    /// De vlaggen voor een orgel dat nu geladen wordt.
    ///
    /// - `forceer`: een expliciete keuze van buiten (de test-API voor proeven);
    ///   wint van alles.
    /// - Geen opgeslagen instellingen: nieuw orgel, de standaard voor nieuw.
    /// - Wel opgeslagen instellingen: per vlag wat er staat; een vlag die er nog
    ///   niet in staat (opgeslagen vóór deze instelling bestond) is uit.
    pub fn bepaal(opgeslagen: Option<&OrganSettings>, forceer: Option<OrgelVlaggen>) -> OrgelVlaggen {
        if let Some(f) = forceer {
            return f;
        }
        match opgeslagen {
            None => OrgelVlaggen::NIEUW,
            Some(s) => OrgelVlaggen::uit_opgeslagen(s),
        }
    }

    /// Zet de proces-brede schakelaars van de importers vóór het parsen van dit
    /// orgel. De importers lezen ze bij het begin van elke import. Alleen de
    /// vlaggen die het inlezen zelf sturen; de andere werken na het laden.
    pub fn zet_importers(&self) {
        vpo_sampler::set_niveaus_toepassen(self.niveaus_van_set);
        vpo_sampler::set_overgangen_toepassen(self.overgangen_van_set);
        vpo_sampler::set_koppels_volgen_set(self.koppels_volgen_set);
        vpo_sampler::set_staart_afkap_aan(self.staart_afkap);
        vpo_sampler::set_koppels_klavier(self.koppels_klavier);
    }

    /// Zet in `doel` wat er bij het opslaan in de instellingen van het orgel komt.
    ///
    /// Normaal de huidige vlaggen, elk expliciet. Is de keuze door een proef
    /// geforceerd (`geforceerd`), dan blijven de opgeslagen vlaggen zoals ze waren
    /// (ook een ontbrekende): een geforceerde keuze mag nooit in iemands
    /// bibliotheek terechtkomen.
    pub fn bewaar_in(
        doel: &mut OrganSettings,
        geforceerd: bool,
        huidig: OrgelVlaggen,
        opgeslagen: Option<&OrganSettings>,
    ) {
        if geforceerd {
            OrgelVlaggen::kopieer_opgeslagen(opgeslagen, doel);
        } else {
            huidig.schrijf_in(doel);
        }
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
        let f = OrgelVlaggen { niveaus_van_set: false, overgangen_van_set: true, koppels_volgen_set: false, staart_afkap: true, zwelkast_van_set: true, koppels_klavier: false, windwerk_van_set: true, tremulant_golf_van_set: false };
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
        // De acht vlaggen zoals ze in de instellingen terechtkomen, in de volgorde van de lijst.
        fn te_bewaren(geforceerd: bool, huidig: OrgelVlaggen, opgeslagen: Option<&OrganSettings>) -> [Option<bool>; 8] {
            // Begin met onzin in het doel: bewaar_in moet elke vlag zelf zetten.
            let mut doel = OrganSettings::default();
            OrgelVlaggen::OUD.schrijf_in(&mut doel);
            OrgelVlaggen::bewaar_in(&mut doel, geforceerd, huidig, opgeslagen);
            [doel.niveaus_van_set, doel.overgangen_van_set, doel.koppels_volgen_set, doel.staart_afkap, doel.zwelkast_van_set, doel.koppels_klavier, doel.windwerk_van_set, doel.tremulant_golf_van_set]
        }
        // Normaal: de huidige stand, expliciet.
        assert_eq!(te_bewaren(false, huidig, None), [Some(true), Some(true), Some(true), Some(true), Some(false), Some(true), Some(true), Some(true)]);
        // Geforceerd, orgel zonder opgeslagen instellingen: niets vastleggen.
        assert_eq!(te_bewaren(true, huidig, None), [None; 8]);
        // Geforceerd, met opgeslagen instellingen: die blijven zoals ze waren.
        let s = instellingen4(Some(false), None, Some(false), Some(false));
        assert_eq!(te_bewaren(true, huidig, Some(&s)), [Some(false), None, Some(false), Some(false), None, None, None, None]);
        // Zonder forcering wint de huidige stand over wat er stond (de gebruiker koos).
        assert_eq!(te_bewaren(false, huidig, Some(&s)), [Some(true), Some(true), Some(true), Some(true), Some(false), Some(true), Some(true), Some(true)]);
    }

    /// De lijst is de enige bron: namen, standaarden en het zetten op naam horen bij elkaar.
    #[test]
    fn de_lijst_is_de_enige_bron() {
        assert_eq!(OrgelVlaggen::NAMEN.len(), 8);
        // De namen zijn de veldnamen in het instellingenbestand; die mogen niet verschuiven.
        assert_eq!(OrgelVlaggen::NAMEN, ["niveaus_van_set", "overgangen_van_set", "koppels_volgen_set", "staart_afkap",
            "zwelkast_van_set", "koppels_klavier", "windwerk_van_set", "tremulant_golf_van_set"]);
        let json_oud = serde_json::to_value(OrgelVlaggen::OUD).unwrap();
        for naam in OrgelVlaggen::NAMEN {
            // Elke naam is op naam te zetten en staat onder die naam in de JSON voor de interface.
            let aan = OrgelVlaggen::OUD.met(naam, true).expect("bekende naam");
            assert_eq!(serde_json::to_value(aan).unwrap()[naam], true, "{naam}");
            assert_eq!(json_oud[naam], false, "{naam}");
            // En in het instellingenbestand staat hij onder dezelfde naam.
            let mut s = OrganSettings::default();
            aan.schrijf_in(&mut s);
            assert_eq!(serde_json::to_value(&s).unwrap()[naam], true, "{naam}");
        }
        // `uit` bouwt dezelfde vlaggen op uit een functie per naam (de test-API).
        assert_eq!(OrgelVlaggen::uit(|_| true).met("zwelkast_van_set", false).unwrap(), OrgelVlaggen::NIEUW);
        assert_eq!(OrgelVlaggen::uit(|_| false), OrgelVlaggen::OUD);
        assert_eq!(OrgelVlaggen::NIEUW.logregel(),
            "niveaus aan, overgangen aan, koppels volgen de set aan, stille staart afkappen aan, zwelkast van de set uit, klavierkoppels van de set aan, windwerk van de set aan, tremulant-golfvorm van de set aan");
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
        assert!(v.met("windwerk_van_set", true).unwrap().windwerk_van_set);
        // De andere blijven zoals ze waren.
        let w = v.met("niveaus_van_set", true).unwrap();
        assert!(!w.overgangen_van_set && !w.koppels_volgen_set && !w.staart_afkap && !w.zwelkast_van_set && !w.koppels_klavier && !w.windwerk_van_set);
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
        assert_eq!(v["windwerk_van_set"], true);
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

    /// D4: een bestaand orgel (zonder deze vlag) houdt het gewone windmodel; een nieuw orgel krijgt het windwerk van
    /// de set; een eigen keuze wint.
    #[test]
    fn tremulant_golf_van_set_bestaand_uit_nieuw_aan() {
        let bestaand = instellingen4(None, None, None, None);
        assert!(!OrgelVlaggen::bepaal(Some(&bestaand), None).tremulant_golf_van_set);
        assert!(OrgelVlaggen::bepaal(None, None).tremulant_golf_van_set);
        let aan = OrganSettings { tremulant_golf_van_set: Some(true), ..instellingen4(None, None, None, None) };
        assert!(OrgelVlaggen::bepaal(Some(&aan), None).tremulant_golf_van_set);
        assert!(OrgelVlaggen::OUD.met("tremulant_golf_van_set", true).unwrap().tremulant_golf_van_set);
    }

    #[test]
    fn windwerk_van_set_bestaand_uit_nieuw_aan() {
        let bestaand = instellingen4(Some(true), Some(true), Some(true), Some(true));
        assert!(!OrgelVlaggen::bepaal(Some(&bestaand), None).windwerk_van_set);
        assert!(OrgelVlaggen::bepaal(None, None).windwerk_van_set);
        let aan = OrganSettings { windwerk_van_set: Some(true), ..instellingen4(None, None, None, None) };
        assert!(OrgelVlaggen::bepaal(Some(&aan), None).windwerk_van_set);
        assert!(!OrgelVlaggen::OUD.windwerk_van_set);
    }
}
