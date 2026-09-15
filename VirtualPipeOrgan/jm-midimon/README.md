# jm-midimon — MIDI-monitor voor de speeltafel

Klein los consoleprogramma (geen installatie) dat laat zien wat je zweltreden,
crescendotrede en pistons precies over MIDI sturen. Bedoeld om op het orgel
zelf te draaien wanneer JM-Orgue een trede niet goed volgt.

## Gebruik

1. Sluit JM-Orgue af (en GrandOrgue/Hauptwerk als die open staan): Windows laat
   maar één programma per MIDI-poort luisteren.
2. Start `jm-midimon.exe` (bijlage bij de release op GitHub, of
   `cargo build --release -p jm-midimon` → `target/release/jm-midimon.exe`).
3. Volg de stappen op het scherm: beweeg ALLEEN trede 1 rustig van open naar
   dicht en druk op Enter; daarna trede 2, trede 3, en tot slot alle drie kort
   achter elkaar.
4. Het verslag staat in `midi-monitor-<datum>.txt` naast het programma (of in
   `Documenten\JM-Orgue-opnames`). Stuur dat bestand mee met je feedback.

`jm-midimon.exe --vrij` logt alles zonder stappen tot je op Enter drukt.

## Wat het verslag laat zien

- per bericht: tijd, poort, kanaal, soort (CC/noot/program change/…), waarde en
  de ruwe bytes;
- per stap: welke stroom (poort/kanaal/CC) bewoog, bereik (slag), richting,
  grootste sprong, tijd tussen berichten, richtingsomkeringen (ruis),
  andere stromen die óók bewogen (overspraak), 14-bits paren (CC n + n+32)
  en NRPN;
- conclusie: of de drie treden voor JM-Orgue van elkaar te onderscheiden zijn
  (twee treden op exact hetzelfde kanaal+CC zijn dat niet: dan moet de
  speeltafel anders ingesteld worden).
