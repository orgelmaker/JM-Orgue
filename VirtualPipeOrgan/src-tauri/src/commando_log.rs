//! Logregels voor configuratie-commando's van de audio-engine (D1, fase 3). Tot nu toe logde de audio-callback deze
//! regels zelf bij het verwerken; het opmaken van de tekst alloceert, en dat hoort niet in de callback. Nu maakt de
//! zendkant (`AudioPlayer::send_command`) dezelfde regel uit het commando zelf. Een bundel (orgelwissel) logt elk
//! commando erin, in volgorde.
//!
//! Verschil met vroeger: waar de callback eigen toestand meelogde (aantal uitgangskanalen, de bus van een zwelkast,
//! het samengestelde "effectief hertemperen"), staat nu alleen wat het commando zelf zegt.

use crate::audio::AudioCommand;

/// Logt `cmd` (en bij een bundel elk commando erin). Noten en andere snelle commando's loggen niets.
pub fn log(cmd: &AudioCommand) {
    if let AudioCommand::Bundel(v) = cmd {
        for c in v {
            log(c);
        }
        return;
    }
    if let Some(regel) = logregel(cmd) {
        tracing::info!("{}", regel);
    }
}

fn aan_uit(b: bool) -> &'static str {
    if b { "aan" } else { "uit" }
}

/// De logregel bij een commando, of `None` als het commando niets logt.
pub fn logregel(cmd: &AudioCommand) -> Option<String> {
    use AudioCommand as C;
    Some(match cmd {
        C::RegisterReleaseMeta(m) => format!("Toetsduur-releases geregistreerd voor {} pijpen (MaxKeyPressTime)", m.len()),
        C::RegisterRankLayout(m) => {
            let multi = m.values().filter(|v| v.len() > 1).count();
            if multi == 0 {
                return None;
            }
            format!("Rank-layout: {} stops met meerdere lagen (gestapelde ranks/perspectieven)", multi)
        }
        C::SetBackgroundVoices(v) => format!("Achtergrondgeluiden: {} loop-voices actief", v.len()),
        C::SetBlowerActive(a) => format!("Motor: {}", aan_uit(*a)),
        C::SetToyActive { name, active } => format!("Speelgoedregister '{}': {}", name, aan_uit(*active)),
        C::SetForceTempered(true) => {
            "Set zonder originele orgeltoonhoogte: effectief hertemperen afgedwongen (speelt op getempereerde doeltoon)".to_string()
        }
        C::SetAutoRetune(on) => format!("Automatisch bijstemmen: {}", aan_uit(*on)),
        C::SetPitchOffsetCents(cents) => {
            let c = if cents.is_finite() { cents.clamp(-200.0, 200.0) } else { 0.0 };
            format!("Globale toonhoogte-correctie: {:.2} cent", c)
        }
        C::SetBackgroundChannels { kind, channels } => {
            let soort = match kind {
                0 => "Blower",
                1 => "Ambient",
                2 => "Toy",
                3 => "TremulantNoise",
                _ => return None,
            };
            format!("Bijgeluid-routing {}: kanalen {:?}", soort, channels)
        }
        C::RegisterStopClicks(m) if !m.is_empty() => format!("Registermechaniek: {} stops met klikgeluiden geladen", m.len()),
        C::RegisterKeyboardClicks(m) if !m.is_empty() => format!("Toetsmechaniek: {} klikbuffers geladen", m.len()),
        C::RegisterPercussiveStops(s) if !s.is_empty() => format!("Percussieve (one-shot) stops geregistreerd: {}", s.len()),
        C::RegisterOdfVoicings(m) if !m.is_empty() => format!("ODF-intonatie geregistreerd voor {} pijpen", m.len()),
        C::RegisterOdfRetune(m) if !m.is_empty() => format!("Hertemper-tabel geregistreerd voor {} pijpen", m.len()),
        C::RegisterOdfLoops(m) if !m.is_empty() => {
            format!("ODF-looppunten geregistreerd voor {} bestanden (ook voor full-sample-loads)", m.len())
        }
        C::SetReverbMix(mix) => format!("Reverb mix set to {:.2}", mix),
        C::RegisterPreloadBuffers(b) => format!("Registered {} preload buffers for instant playback", b.len()),
        C::RegisterTremStops(s) if !s.is_empty() => format!("Registers met echte tremulant-opnamen: {}", s.len()),
        C::RegisterReleaseCrossfade(m) if !m.is_empty() => format!("ReleaseCrossfadeLength uit de sampleset: {} release-keys", m.len()),
        C::SetSwellConfig { division_index, min_db, filter_cutoff_closed } => {
            format!("Swell config div {}: min={}dB, filter={}Hz", division_index, min_db, filter_cutoff_closed)
        }
        C::SetTremulantLFO { division_index, active, rate, amp_depth, pitch_depth } => format!(
            "Tremulant LFO div {}: active={}, rate={}Hz, amp={}dB, pitch={}ct",
            division_index, active, rate, amp_depth, pitch_depth
        ),
        C::SetWindModel { division_index, instelling: i } => format!(
            "Windgroep {}: aan={}, balg={:.2}, demping={:.2}, daling={:.3}, stoot={:.2}, kanaal={:.2}, doffer={:.2}, verschil={:.2}, tongwerk apart={}",
            (*division_index as usize).min(31), i.enabled, i.reservoir_size, i.damping, i.max_sag, i.stoot, i.kanaal, i.doffer, i.verschil, i.tongwerk_apart
        ),
        C::SetDivisionWindGroup { division_index, group } => format!("Division {} → wind group {}", division_index, group),
        C::RegisterStopDivisionMap(m) => {
            format!("Registered stop→division map ({} entries); gains, tremulant, routing + C/Cis reset", m.len())
        }
        C::RegisterPipeEnclosures { kaart, ouder } => {
            format!("Zwelkasten per rank: {} kast(en), {} pijpsleutels", ouder.len(), kaart.len())
        }
        C::SetTemperament { fine_tune, retune, .. } => format!("Temperament set: fine_tune={} cents, retune={}", fine_tune, retune),
        C::SetAlgorithmicReverb { preset: Some(p), mix, .. } => format!("FDN reverb preset {} applied, mix={}", p, mix),
        C::SetAlgorithmicReverb { preset: None, rt60, pre_delay_ms, damping, room_size, mix } => format!(
            "FDN reverb configured: rt60={}, pre_delay={}ms, damping={}, room={}, mix={}",
            rt60, pre_delay_ms, damping, room_size, mix
        ),
        C::SetReverbType { algorithmic } => {
            format!("Reverb type: {}", if *algorithmic { "algorithmic FDN" } else { "convolution IR" })
        }
        C::SetDivisionPan { division_index, pan } => format!("Division {} pan: {:.2}", division_index, pan),
        C::SetDivisionOutputChannels { division_index, channels } => {
            format!("Division {} output channels: {:?}", division_index, channels)
        }
        C::SetCcisSpread { strength, falloff, swap } => {
            format!("C/Cis spread: strength={:.2}, falloff={:.2}, swap={}", strength, falloff, swap)
        }
        C::SetDivisionCcis { division_index, enabled } => format!("Division {} C/Cis spread: {}", division_index, enabled),
        C::SetEqBands { enabled, preamp_db, bands } => {
            format!("EQ: enabled={}, {} banden, voorversterking {:.1} dB", enabled, bands.len(), preamp_db)
        }
        C::SetOutputEq { enabled, preamp_db, bands } => {
            format!("Uitgangscorrectie: enabled={}, {} banden, voorversterking {:.1} dB", enabled, bands.len(), preamp_db)
        }
        C::SetBasManagement(i) => format!(
            "Basmanagement: {} (sub op uitgang {}, kruisfrequentie {:.0} Hz, niveau {:.1} dB)",
            aan_uit(i.aan), i.sub_kanaal + 1, i.kruisfrequentie_hz, i.sub_niveau_db
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noten_loggen_niets_en_configuratie_wel() {
        assert!(logregel(&AudioCommand::NoteOn { stop_id: 1, pipe_num: 2, midi_note: 60, velocity: 1.0, verschuiving: 0 }).is_none());
        assert!(logregel(&AudioCommand::AllNotesOff).is_none());
        assert_eq!(logregel(&AudioCommand::SetBlowerActive(true)).as_deref(), Some("Motor: aan"));
        assert_eq!(
            logregel(&AudioCommand::SetDivisionPan { division_index: 3, pan: 0.25 }).as_deref(),
            Some("Division 3 pan: 0.25")
        );
        // Lege kaarten loggen niets, net als vroeger in de callback.
        assert!(logregel(&AudioCommand::RegisterTremStops(Default::default())).is_none());
    }
}
