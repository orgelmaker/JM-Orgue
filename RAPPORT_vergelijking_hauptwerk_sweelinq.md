# JM-Orgue naast Hauptwerk en Sweelinq

Peildatum 21 september 2026. Bijgewerkt na 0.7.48; alles uit dit rapport is
gebouwd op de VST/AU-plug-in na. Bronnen: de editievergelijking op
hauptwerk.com (Lite tegenover Advanced) en de release notes van Sweelinq
Virtual Pipe Organ t/m 3.1.0 (build 1647).

## 1. Hauptwerk: de editietabel, met onze kolom erbij

| | Lite | Advanced | JM-Orgue |
|---|---|---|---|
| Polyfonie | 1.024 | 32.768 | **256–32.768, instelbaar** met belastingsmeter (0.7.48) |
| Audio-uitgang | mono/stereo | tot 1.024 kanalen | **tot 1.024 uitgangen**, per klavier toe te wijzen (0.7.48) |
| Bit/samplerate | 32-bit/96 kHz | 32-bit/96 kHz | **te kiezen tot 96 kHz** (0.7.48), 32-bit float intern |
| Werkgeheugen | onbeperkt | onbeperkt | onbeperkt (64-bit) |
| MIDI-uit | ✅ | ✅ | ✅ registerlampen, displays, drie protocollen |
| Pc en Mac | ✅ | ✅ | Windows uitgebracht; macOS en Linux bouwen en draaien, nog niet uitgebracht |
| 64-bit | ✅ | ✅ | ✅ |
| Aanraakmenu | ✅ | ✅ | ✅ plus telefoon/tablet als tweede registreerscherm |
| **VST/AU-plug-in** | ❌ | ✅ | **❌ — het enige echte gat, bewust open gelaten** |
| Audio- en perspectiefmixer | ❌ | ✅ | ✅ niveau per perspectief, per divisie, per uitgang |
| Convolutiegalm | ❌ | ✅ | ✅ inclusief eigen IR-bestand |
| Intonatie per pijp | ❌ | ✅ | ✅ volume en stemming per pijp |
| Meerdere schermen | ❌ | ✅ | ✅ losse registerschermen op elk scherm |
| Windmodel | ❌ | ✅ | ✅ per windgroep instelbaar |
| Pistonbalken | 1 per scherm | 4 per scherm | setzerbalk + koppelbalk, vrij in te leren |

Op de tabel van Hauptwerk na één regel dus: wat in hun **Advanced** zit en bij
ons niet, is de **VST/AU-plug-in**.

Over de polyfonie hoort een eerlijke kanttekening. Het getal is sinds 0.7.48
gelijk (32.768), maar dat is aan beide kanten een plafond en geen belofte.
Gemeten op een i9-10885H kost bij ons elke klinkende stem ongeveer 0,10 % van
de buffertijd: 436 stemmen geven 39 % belasting, 716 stemmen 74 %. Op één
rekenkern ligt de grens dus rond de 770 stemmen, en dat komt doordat de
mengloop op één rekenkern draait. **Daar** ligt de echte winst,
niet in het getal; meerkernig renderen is het volgende dat de moeite waard is.

Over de samplerate hoort er ook een: de keuze werkt waar de host hem toelaat.
Onder WASAPI in gedeelde modus legt Windows de rate vast (Geluid → Eigenschappen
→ Geavanceerd) en wordt een andere waarde geweigerd; onder ASIO komt hij uit het
paneel van de driver. Dat geldt voor elke toepassing op dat apparaat, niet
alleen voor ons.

## 2. Sweelinq: wat zij hebben en wij niet

Uit hun release notes, alleen de punten die bij ons ontbreken.

| Sweelinq | Wat het is | Onze stand |
|---|---|---|
| 2.3.0 | **Perspectieven wisselen tijdens het spelen** | ✅ 0.7.47 — uitzetten altijd live; met "alle posities in het geheugen houden" ook aanzetten |
| 2.1.0 | **Korte octaaf** | ✅ 0.7.47 — per klavier, C/E |
| 3.0.0 | **Koppels toevoegen** die de sampleset niet heeft | ✅ 0.7.47 — de ontbrekende afgeleide koppels zijn bij te schakelen |
| 3.0.0 | **Verlengde klaviatuur** (spelen buiten het opgenomen bereik) | ✅ 0.7.47 — "doorlopend klavier", octaaf-herhaling (geen resampling) |
| 2.2.0 | **SysEx als schakelaar-invoer** | ✅ 0.7.47 — in te leren en met de hand in te typen |
| 2.1.0 | **Control change als bitveld** voor schakelaars | ✅ 0.7.47 |
| 2.0.0 | **EQ-presets voor bekende koptelefoons** | ◐ 0.7.47 — drie luisterprofielen (neutraal, koptelefoon, kleine luidsprekers); geen correcties per model, die zouden gemeten moeten zijn |
| 2.0.0 | **Testsignaal voor de luidsprekeropstelling** | ✅ 0.7.47 — roze ruis of sinus per kanaal |
| 2.3.0 | **Linux (ook ARM)** en **ondertekening op macOS** | ✖ Linux bouwt maar is niet uitgebracht; macOS is niet ondertekend — beide een beslissing, geen code |

En omgekeerd, wat wij hebben en in hun notes niet voorkomt: bladmuziek uit je
eigen spel (live notatie, PDF/MIDI), een telefoon of tablet als
registreerscherm, ruim vijftig temperamenten met hertemperen op de gemeten
pijptoon, per pijp intoneren, zeven talen, en een MIDI-archief dat alles wat je
speelt vanzelf bewaart.

## 3. Wat er sinds dit rapport gebeurd is

Alles uit paragraaf 2 is in **0.7.47** gebouwd, behalve de twee regels die geen
code zijn maar een beslissing: een Linux-uitgave en het ondertekenen van de
macOS-versie (zie `RAPPORT_macos_linux.md`).

Bewust niet gebouwd:

- **VST/AU-plug-in.** Dat is een andere productvorm — JM-Orgue als instrument in
  een opnameprogramma — en geen ontbrekende knop. Pas overwegen als iemand er
  echt om vraagt.
- **Meerkernig renderen.** De mengloop draait op één kern; dat is wat de
  polyfonie in de praktijk begrenst (zie de meting hierboven). Een zinvolle maar
  ingrijpende verbouwing van de audiothread — een eigen project, geen bijzaak.
- **Correcties per koptelefoonmodel.** Die zouden gemeten moeten zijn. In plaats
  daarvan staan er drie eerlijk omschreven luisterprofielen.
