//! Orgelwissel als één generatie (F3). Tijdens het laden van een orgel gaan de commando's voor de audio-engine niet
//! los de wachtrij in, maar worden ze op de laadthread verzameld. Aan het eind gaan ze als één
//! `AudioCommand::Bundel` naar de engine, die ze in één callback achter elkaar verwerkt. Een noot ziet dus het hele
//! oude of het hele nieuwe orgel, nooit een half opgebouwd orgel.
//!
//! Wordt het laden afgebroken (annuleren, fout), dan valt de bundel weg zonder dat de engine iets heeft gezien: het
//! oude orgel blijft heel.
//!
//! Alleen de thread die `OrgelBundel::begin` aanroept verzamelt; commando's van andere threads (MIDI, interface)
//! gaan gewoon direct.

use std::cell::RefCell;

use crate::audio::AudioCommand;

thread_local! {
    static BUNDEL: RefCell<Option<Vec<AudioCommand>>> = const { RefCell::new(None) };
}

/// Houdt de verzameling open zolang hij leeft; `verstuur` sluit hem af. Valt hij weg zonder `verstuur`, dan
/// worden de verzamelde commando's weggegooid.
pub struct OrgelBundel {
    open: bool,
}

impl OrgelBundel {
    pub fn begin() -> Self {
        BUNDEL.with(|b| *b.borrow_mut() = Some(Vec::with_capacity(64)));
        OrgelBundel { open: true }
    }

    /// Verzamelde commando's als één bundel; `None` als er niets verzameld is.
    pub fn sluit(mut self) -> Option<AudioCommand> {
        self.open = false;
        let v = BUNDEL.with(|b| b.borrow_mut().take()).unwrap_or_default();
        if v.is_empty() { None } else { Some(AudioCommand::Bundel(v)) }
    }
}

impl Drop for OrgelBundel {
    fn drop(&mut self) {
        if self.open {
            let weg = BUNDEL.with(|b| b.borrow_mut().take()).map(|v| v.len()).unwrap_or(0);
            if weg > 0 {
                tracing::info!("Orgelwissel niet afgemaakt: {} commando's niet naar de audio-engine (het vorige orgel blijft)", weg);
            }
        }
    }
}

/// Verzamelt `cmd` als deze thread een bundel open heeft (dan `None`); anders krijgt de aanroeper hem terug om
/// direct te versturen.
pub fn verzamel_of_geef_terug(cmd: AudioCommand) -> Option<AudioCommand> {
    BUNDEL.with(|b| match b.borrow_mut().as_mut() {
        Some(v) => {
            v.push(cmd);
            None
        }
        None => Some(cmd),
    })
}

/// `AudioPlayer::send_command`, maar via de bundel als deze thread er een open heeft.
pub fn stuur(player: &crate::audio::AudioPlayer, cmd: AudioCommand) -> Result<(), String> {
    match verzamel_of_geef_terug(cmd) {
        Some(c) => player.send_command(c),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verzamelt_alleen_op_de_eigen_thread_en_in_volgorde() {
        let bundel = OrgelBundel::begin();
        assert!(verzamel_of_geef_terug(AudioCommand::AllNotesOff).is_none());
        assert!(verzamel_of_geef_terug(AudioCommand::SetForceTempered(true)).is_none());
        // Een andere thread verzamelt niet mee.
        let elders = std::thread::spawn(|| verzamel_of_geef_terug(AudioCommand::AllNotesOff).is_some()).join().unwrap();
        assert!(elders);
        match bundel.sluit() {
            Some(AudioCommand::Bundel(v)) => {
                assert_eq!(v.len(), 2);
                assert!(matches!(v[0], AudioCommand::AllNotesOff));
                assert!(matches!(v[1], AudioCommand::SetForceTempered(true)));
            }
            _ => panic!("bundel verwacht"),
        }
        // Na het sluiten gaat alles weer direct.
        assert!(verzamel_of_geef_terug(AudioCommand::AllNotesOff).is_some());
    }

    #[test]
    fn afgebroken_bundel_valt_weg() {
        {
            let _bundel = OrgelBundel::begin();
            assert!(verzamel_of_geef_terug(AudioCommand::AllNotesOff).is_none());
        }
        assert!(verzamel_of_geef_terug(AudioCommand::AllNotesOff).is_some());
    }
}
