# JM-Orgue naast Hauptwerk en Sweelinq

Peildatum 21 september 2026, versie 0.7.45. Bronnen: de editievergelijking op
hauptwerk.com (Lite tegenover Advanced) en de release notes van Sweelinq
Virtual Pipe Organ t/m 3.1.0 (build 1647).

## 1. Hauptwerk: de editietabel, met onze kolom erbij

| | Lite | Advanced | JM-Orgue |
|---|---|---|---|
| Polyfonie | 1.024 | 32.768 | **256–4.096, instelbaar** met belastingsmeter |
| Audio-uitgang | mono/stereo | tot 1.024 kanalen | **alle uitgangen van de kaart**, per klavier toe te wijzen |
| Bit/samplerate | 32-bit/96 kHz | 32-bit/96 kHz | volgt de geluidskaart (WASAPI/ASIO), 32-bit float intern |
| Werkgeheugen | onbeperkt | onbeperkt | onbeperkt (64-bit) |
| MIDI-uit | ✅ | ✅ | ✅ registerlampen, displays, drie protocollen |
| Pc en Mac | ✅ | ✅ | Windows uitgebracht; macOS en Linux bouwen en draaien, nog niet uitgebracht |
| 64-bit | ✅ | ✅ | ✅ |
| Aanraakmenu | ✅ | ✅ | ✅ plus telefoon/tablet als tweede registreerscherm |
| **VST/AU-plug-in** | ❌ | ✅ | **❌ — het enige echte gat** |
| Audio- en perspectiefmixer | ❌ | ✅ | ✅ niveau per perspectief, per divisie, per uitgang |
| Convolutiegalm | ❌ | ✅ | ✅ inclusief eigen IR-bestand |
| Intonatie per pijp | ❌ | ✅ | ✅ volume en stemming per pijp |
| Meerdere schermen | ❌ | ✅ | ✅ losse registerschermen op elk scherm |
| Windmodel | ❌ | ✅ | ✅ per windgroep instelbaar |
| Pistonbalken | 1 per scherm | 4 per scherm | setzerbalk + koppelbalk, vrij in te leren |

Op de tabel van Hauptwerk na één regel dus: wat in hun **Advanced** zit en bij
ons niet, is de **VST/AU-plug-in**. De polyfonie is het tweede aandachtspunt —
onze bovengrens van 4.096 stemmen ligt boven hun Lite maar ver onder Advanced.
In de praktijk is dat zelden bindend (een tutti met staarten haalt zelden
1.000 stemmen), maar het is wel een getal waarop vergeleken wordt.

## 2. Sweelinq: wat zij hebben en wij niet

Uit hun release notes, alleen de punten die bij ons ontbreken.

| Sweelinq | Wat het is | Onze stand |
|---|---|---|
| 2.3.0 | **Perspectieven wisselen tijdens het spelen** | Bij ons kost een andere microfoonpositie een herlaad van het orgel |
| 2.1.0 | **Korte octaaf** | Niet aanwezig |
| 3.0.0 | **Koppels toevoegen** die de sampleset niet heeft | Wij tonen alleen de koppels uit de set |
| 3.0.0 | **Verlengde klaviatuur** (spelen buiten het opgenomen bereik) | Niet aanwezig |
| 2.2.0 | **SysEx als schakelaar-invoer** | Wij sturen SysEx wél uit (Hauptwerk-LCD), maar lezen het niet als trigger |
| 2.1.0 | **Control change als bitveld** voor schakelaars | Niet aanwezig |
| 2.0.0 | **EQ-presets voor bekende koptelefoons** | Wij hebben EQ per kanaal, geen presets per koptelefoon |
| 2.0.0 | **Testsignaal voor de luidsprekeropstelling** | Niet aanwezig |
| 2.3.0 | **Linux (ook ARM)** en **ondertekening op macOS** | Linux bouwt maar is niet uitgebracht; macOS is niet ondertekend |

En omgekeerd, wat wij hebben en in hun notes niet voorkomt: bladmuziek uit je
eigen spel (live notatie, PDF/MIDI), een telefoon of tablet als
registreerscherm, ruim vijftig temperamenten met hertemperen op de gemeten
pijptoon, per pijp intoneren, zeven talen, en een MIDI-archief dat alles wat je
speelt vanzelf bewaart.

## 3. Wat ik eruit zou halen

**Nu, klein en zichtbaar:**
1. **Testsignaal voor de luidsprekeropstelling.** Een halve dag werk en precies
   het soort ding dat iemand bij de eerste installatie mist.
2. **Korte octaaf.** Voor historische orgels een echte omissie, en het is een
   toetsafbeelding — geen nieuw geluidspad.

**Daarna, groter maar de moeite waard:**
3. **Perspectief wisselen zonder herladen.** Sweelinq kan het en het is precies
   wat een organist wil doen terwijl hij luistert. Wij laden de lagen al per
   slot; wat ontbreekt is ze warm houden en live omschakelen.
4. **Koppels toevoegen** die de set niet kent. Veel gevraagd bij oudere sets.

**Bewust laten liggen:**
5. **VST/AU-plug-in.** Dat is een andere productvorm (JM-Orgue als instrument in
   een DAW), geen ontbrekende knop. Pas overwegen als iemand er echt om vraagt.
6. **Polyfonie naar tienduizenden.** Onze grens is een bewuste CPU-keuze met een
   zichtbare belastingsmeter; hoger zetten zonder meerkernig renderen levert
   alleen haperingen op.
