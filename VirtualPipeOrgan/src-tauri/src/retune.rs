//! Hertemperen op gemeten pijptoonhoogte — pure rekenkern (geen I/O, geen
//! state), zodat de formule los van de laadpijplijn te testen is.
//!
//! GrandOrgue-/Hauptwerk-semantiek (GOSoundingPipe::GetAutoTuningPitchOffset,
//! Hauptwerk BaseTuningScheme):
//!   target(key, h)   = 100·key + 1200·log2(h/8)              (cents, 6900 = a' 440 Hz)
//!   gespeeld         = target + keep%/100 · (origineel − target)
//!   retune(pijp)     = gespeeld − gemeten + PitchCorrection-som
//! waarbij "gemeten" de absolute samplepitch is uit (prioriteit): ODF
//! MIDIKeyNumber/MIDIPitchFraction of Hauptwerk Pitch_ExactSamplePitch →
//! smpl-chunk van de WAV (unity note + fractie). "Origineel" is de toon die de
//! pijp op het echte orgel had (Hauptwerk Pitch_OriginalOrgan_PitchHz).
//! Zonder originele toon: origineel = gemeten (identiek aan de GO-formule
//! (target − gemeten)·(1 − keep%)). Zonder meting: `None` (de engine valt dan
//! terug op de ODF-PitchTuning, zoals in "Origineel"-stand).
//!
//! Zonder dit onderscheid klonk bij keep% > 0 een vervangen pijp (Hauptwerks
//! truc: `045-a.wav` toekennen aan toets 43 omdat `043-g.wav` kapot is) op de
//! toon van het buurbestand in plaats van op de echte orgeltoon.
//!
//! PitchCorrection is een BEWUSTE, OPGETELDE afwijking (zwever van een
//! celeste) — plusteken. In "Origineel" telt hij niet mee.

/// Invoer per pijp voor `retune_cents`.
#[derive(Debug, Clone, Copy)]
pub struct PipePitchInput {
    /// MIDI-toets waarop de pijp klinkt (van de EIGENAAR-stop bij REF).
    pub key_midi: u32,
    /// HarmonicNumber (8 = 8', 16 = 4', …); 0 wordt als 1 behandeld.
    pub harmonic: u32,
    /// Absolute gemeten samplepitch in cents (6900 = a'), of None.
    pub measured_cents: Option<f32>,
    /// Originele toon van de pijp op het echte orgel in cents (Hauptwerk
    /// Pitch_OriginalOrgan_PitchHz). Bij keep% > 0 schuift de pijp (deels) naar
    /// deze toon toe; bij een vervangen opname (bestand van een buurtoon) staat
    /// hij een hele of halve toon van `measured_cents` af. Zonder waarde valt
    /// hij samen met `measured_cents` en gedraagt de formule zich zoals de
    /// oude GO-formule `(target − gemeten)·(1 − keep)`.
    pub original_cents: Option<f32>,
    /// Gesommeerde PitchCorrection (Organ + Windchest + Stop/Rank + Pipe).
    pub pitch_correction_cents: f32,
    /// Hauptwerk BaseTuningDeviation: % van de originele afwijking dat
    /// behouden blijft (0 = volledig hertemperen, 100 = niet hertemperen).
    pub keep_pct: f32,
    /// De ODF-PitchTuning-som van deze pijp (Origineel-waarde) — alleen voor
    /// de plausibiliteitsgrens.
    pub original_pitch_tuning_cents: f32,
}

/// GrandOrgue's waarschuwingsgrens: |auto − manual| > 600 ct wijst op junk
/// (verkeerde unity note, verkeerde HarmonicNumber); dan liever geen retune.
pub const MAX_PLAUSIBLE_DELTA_CENTS: f32 = 600.0;

/// Ruimere grens voor de Hauptwerk-correctie `origineel − bestandstoon`
/// (0.8.3): daar zijn béide waarden expliciet door de maker opgegeven, geen
/// smpl-chunk die een verkeerde unity note kan dragen. Een vervangen pijp
/// kan best een halve octaaf verderop liggen: Nowa Ruda's Tuba 8' leent
/// voor zijn hoogste toets het dis-bestand zes halve tonen lager (+604 ct),
/// net buiten de GO-grens van 600. Meer dan een octaaf is ook hier
/// onwaarschijnlijk en wordt als fout genegeerd.
pub const HW_MAX_ORIGINEEL_DELTA_CENTS: f32 = 1300.0;

/// Gemeten samplepitch uit de smpl-chunk: `100·unity + fractie`.
/// None bij ontbrekende/ongeldige unity (buiten 1..=127) én bij de
/// veelvoorkomende editor-default (60, 0.0) — dat is geen echte meting.
/// Een écht zuivere pijp op toets 60/8' valt hierdoor op PitchTuning terug;
/// zie `measured_from_smpl_for_target` voor die uitzondering.
pub fn measured_from_smpl(unity: Option<u8>, frac_cents: Option<f32>) -> Option<f32> {
    let u = unity?;
    if !(1..=127).contains(&u) {
        return None;
    }
    let f = frac_cents.unwrap_or(0.0);
    if u == 60 && f.abs() < 1e-6 {
        return None;
    }
    Some(100.0 * u as f32 + f)
}

/// Als `measured_from_smpl`, maar accepteert de editor-default (60, 0.0)
/// wanneer die exact overeenkomt met de doeltoon van de pijp (toets 60 op
/// 8'): dan is target − measured = 0, dus onschadelijk, én blijft de
/// PitchCorrection van die pijp (bv. −20 ct zwever) in hertemper-modus
/// consistent met haar buren i.p.v. stilletjes weg te vallen.
pub fn measured_from_smpl_for_target(unity: Option<u8>, frac_cents: Option<f32>, target_cents: f32) -> Option<f32> {
    measured_from_smpl(unity, frac_cents).or_else(|| {
        let f = frac_cents.unwrap_or(0.0);
        if unity == Some(60) && f.abs() < 1e-6 && (target_cents - 6000.0).abs() < 0.5 {
            Some(6000.0)
        } else {
            None
        }
    })
}

/// Doeltoon van een pijp in de gelijkzwevende referentie (cents).
pub fn target_cents(key: u32, harmonic: u32) -> f32 {
    100.0 * key as f32 + 1200.0 * (harmonic.max(1) as f32 / 8.0).log2()
}

/// Hertemper-correctie in cents voor één pijp, of None als er geen
/// (plausibele) meting is. Vervangt in hertemper-modus de ODF-PitchTuning.
pub fn retune_cents(inp: &PipePitchInput) -> Option<f32> {
    let measured = inp.measured_cents?;
    let target = target_cents(inp.key_midi, inp.harmonic);
    let keep = inp.keep_pct.clamp(0.0, 100.0) / 100.0;
    // "gespeeld" is de toon waarop de pijp hoort te klinken na hertemperen:
    // op de doel-toon, maar bij keep% > 0 trekt hij (deels) naar de originele
    // orgeltoon. Zonder originele toon valt die samen met `measured`, waardoor
    // `(gespeeld − measured) = (target − measured)·(1 − keep)` — identiek aan de
    // oude GO-formule, dus bestaande tests blijven kloppen.
    let origineel = inp.original_cents.unwrap_or(measured);
    let gespeeld = target + keep * (origineel - target);
    let r = gespeeld - measured + inp.pitch_correction_cents;
    if !r.is_finite() || (r - inp.original_pitch_tuning_cents).abs() > MAX_PLAUSIBLE_DELTA_CENTS {
        return None;
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inp(key: u32, h: u32, measured: Option<f32>, pc: f32, keep: f32, orig: f32) -> PipePitchInput {
        PipePitchInput {
            key_midi: key,
            harmonic: h,
            measured_cents: measured,
            original_cents: None,
            pitch_correction_cents: pc,
            keep_pct: keep,
            original_pitch_tuning_cents: orig,
        }
    }

    /// Zelfde als `inp()`, maar met een opgegeven originele orgeltoon
    /// (Pitch_OriginalOrgan_PitchHz als cents).
    fn inp_orig(key: u32, h: u32, measured: Option<f32>, original: f32, pc: f32, keep: f32) -> PipePitchInput {
        PipePitchInput {
            key_midi: key,
            harmonic: h,
            measured_cents: measured,
            original_cents: Some(original),
            pitch_correction_cents: pc,
            keep_pct: keep,
            original_pitch_tuning_cents: 0.0,
        }
    }

    /// Friesach SW Vox celeste, Pipe013 = toets 48: smpl +24.974 ct,
    /// PitchCorrection 15.9313 → (4800 − 4824.974) + 15.9313 = −9.043.
    #[test]
    fn friesach_celeste() {
        let r = retune_cents(&inp(48, 8, Some(4824.974), 15.9313, 0.0, 0.0)).unwrap();
        assert!((r - -9.043).abs() < 0.01, "{r}");
    }

    /// Friesach HW Principal 8' toets 60: smpl +6.4035 → −6.4035.
    #[test]
    fn friesach_principal() {
        let r = retune_cents(&inp(60, 8, Some(6006.4035), 0.0, 0.0, 0.0)).unwrap();
        assert!((r - -6.4035).abs() < 0.001, "{r}");
    }

    /// REF-octaaf: toets 36 op 4' (h=16) → target 3600+1200 = 4800;
    /// gemeten 4806.09 → −6.09.
    #[test]
    fn ref_octave_harmonic_16() {
        let r = retune_cents(&inp(36, 16, Some(4806.09), 0.0, 0.0, 0.0)).unwrap();
        assert!((r - -6.09).abs() < 0.001, "{r}");
    }

    /// keep 100 % → alleen de PitchCorrection blijft over.
    #[test]
    fn keep_100_gives_pitch_correction_only() {
        let r = retune_cents(&inp(60, 8, Some(6030.0), 0.0, 100.0, 0.0)).unwrap();
        assert!(r.abs() < 1e-6, "{r}");
        let r2 = retune_cents(&inp(60, 8, Some(6030.0), 11.0, 100.0, 0.0)).unwrap();
        assert!((r2 - 11.0).abs() < 1e-6, "{r2}");
    }

    #[test]
    fn no_measurement_is_none() {
        assert_eq!(retune_cents(&inp(60, 8, None, 0.0, 0.0, 0.0)), None);
    }

    /// Junk-unity-note: toets 36 op 8' met unity 60 → 2400 ct → None.
    #[test]
    fn junk_2400_cents_rejected() {
        assert_eq!(retune_cents(&inp(36, 8, Some(6005.0), 0.0, 0.0, 0.0)), None);
    }

    /// Junk net binnen een octaaf: toets 48 op 8' met unity 60/frac 5 →
    /// −1205 ct → 600-ct-grens → None.
    #[test]
    fn junk_1205_cents_rejected() {
        let m = measured_from_smpl(Some(60), Some(5.0));
        assert_eq!(m, Some(6005.0));
        assert_eq!(retune_cents(&inp(48, 8, m, 0.0, 0.0, 0.0)), None);
    }

    #[test]
    fn measured_from_smpl_rules() {
        assert_eq!(measured_from_smpl(Some(60), Some(0.0)), None);
        assert_eq!(measured_from_smpl(Some(60), None), None);
        assert!((measured_from_smpl(Some(60), Some(6.4)).unwrap() - 6006.4).abs() < 1e-4);
        assert_eq!(measured_from_smpl(None, Some(6.4)), None);
        assert_eq!(measured_from_smpl(Some(0), Some(6.4)), None);
        assert!((measured_from_smpl(Some(36), Some(0.0)).unwrap() - 3600.0).abs() < 1e-6);
    }

    /// Nitra RP Copula maior 8', pijp 79 (g''): bestandstoon 708,7 Hz
    /// (`Pitch_ExactSamplePitch`), originele pijptoon 796,29 Hz
    /// (`Pitch_OriginalOrgan_PitchHz`). In Hauptwerks "Original organ tuning"
    /// (keep = 100 %) klinkt deze vervangen pijp op de originele orgeltoon:
    /// retune = origineel − gemeten ≈ +200,7 ct.
    #[test]
    fn nitra_vervangen_pijp_keep_100() {
        // target(79, 8) = 7900, measured = 7726.2, origineel = 7926.9
        let r = retune_cents(&inp_orig(79, 8, Some(7726.2), 7926.9, 0.0, 100.0)).unwrap();
        assert!((r - 200.7).abs() < 0.05, "{r}");
    }

    /// Zelfde pijp bij keep = 0 (volledig hertemperen, bv. gelijkzwevend):
    /// hij speelt op de doel-toon, dus retune = target − gemeten ≈ +173,8 ct.
    /// De originele orgeltoon doet dan niet mee.
    #[test]
    fn nitra_vervangen_pijp_keep_0() {
        let r = retune_cents(&inp_orig(79, 8, Some(7726.2), 7926.9, 0.0, 0.0)).unwrap();
        assert!((r - 173.8).abs() < 0.05, "{r}");
    }

    /// Zonder originele orgeltoon valt de formule samen met de GO-formule:
    /// (target − measured)·(1 − keep). Een vervangen pijp zonder PitchHz-veld
    /// klinkt dan nog altijd fout bij keep = 100, maar dat is dezelfde fout
    /// als vóór de fix (en komt in de praktijk niet voor: Hauptwerk vraagt het
    /// veld voor élke pijp).
    #[test]
    fn zonder_origineel_valt_terug_op_oude_formule() {
        // target(79, 8) = 7900, measured = 7726.2, geen origineel, keep = 50.
        // Oude formule: (7900 - 7726.2) · 0.5 = 86.9
        let r = retune_cents(&inp(79, 8, Some(7726.2), 0.0, 50.0, 0.0)).unwrap();
        assert!((r - 86.9).abs() < 0.05, "{r}");
    }

    /// (60, 0.0) wordt alléén geaccepteerd als de doeltoon exact 6000 is.
    #[test]
    fn measured_from_smpl_for_target_accepts_exact_match() {
        assert_eq!(measured_from_smpl_for_target(Some(60), Some(0.0), target_cents(60, 8)), Some(6000.0));
        assert_eq!(measured_from_smpl_for_target(Some(60), Some(0.0), target_cents(48, 8)), None);
        assert_eq!(measured_from_smpl_for_target(Some(60), Some(0.0), target_cents(48, 16)), Some(6000.0));
        // Echte fractie: gewoon het normale pad.
        assert!((measured_from_smpl_for_target(Some(60), Some(6.4), 6000.0).unwrap() - 6006.4).abs() < 1e-4);
    }

    #[test]
    fn target_cents_table() {
        assert!((target_cents(69, 8) - 6900.0).abs() < 1e-6);
        assert!((target_cents(36, 16) - 4800.0).abs() < 1e-4);
        assert!((target_cents(36, 4) - 2400.0).abs() < 1e-4);
        assert!((target_cents(60, 0) - (6000.0 - 3600.0)).abs() < 1e-3); // h 0 → 1
    }
}
