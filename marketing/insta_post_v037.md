# Instagram Post — JM-Orgue: vogelvlucht + "binnen een paar weken live" (v037)

**Status:** voortgangspost met release-aankondiging (5 slides). Vervangt v036.

**Wat is er anders dan v036?** v036 liet de speeltafel, de notatie en de
update-melding zien (versie 0.7.28). Sindsdien staan we op **0.7.44**: zestien
versies verder. Deze post neemt daarom bewust een **vogelvlucht** over alles wat
er sinds die post bij is gekomen, en kondigt aan dat de software **binnen een
paar weken live gaat en voor iedereen beschikbaar is**.

De notatiefunctie komt niet opnieuw in de slides: die stond al in v036. Alles
wat je hier ziet is nieuw sinds die post.

## Slides (5)

Bestanden in `screenshots/instagram_v037/` (printscreens uit de echte app,
versie 0.7.44):

| # | Bestand (bijgesneden: `_vierkant.png`) | Kop op de slide | Inhoud |
|---|---------|-----|--------|
| 1 | `slide1_bibliotheek.png` | De bibliotheek | **De bibliotheek**: elk orgel met foto, orgelbouwer, plaats en aantal registers; taalkeuze in zeven talen; importknoppen voor GrandOrgue (.organ), JM-Rec en externe mappen |
| 2 | `slide2_speeltafel.png` | De speeltafel | **De speeltafel**: Friesach (44 registers, vier werken) met getrokken registratie, koppelbalk onderin, setzerbalk en de zwelkastwijzer bij het Schwellwerk |
| 3 | `slide3_afstandsbediening.png` | Je telefoon als registreerscherm | **Op je telefoon**: dezelfde registratie live op een telefoon in hetzelfde wifi-netwerk — de registers die op de speeltafel getrokken staan, lichten hier mee op |
| 4 | `slide4_stemming_klank.png` | Stemming en klank | **Onder de motorkap**: stemming "Origineel (zoals opgenomen)" met de melding *2.392 van 2.392 pijpen gemeten*, fijnstemming en de nagalm met ruimtepreset (de volledige schermafbeelding toont rechts ook MIDI-kanaal, zwelkast en uitgangen per klavier) |
| 5 | `slide5_talen_audio.png` | Voor iedereen | **Voor iedereen**: zeven talen, kleurthema's voor de speeltafel, ASIO/WASAPI met instelbare polyfonie, buffergrootte en zichtbare latentie |

**Klaar om te plaatsen:** naast elke schermafbeelding staat een
`*_vierkant.png` — dat zijn de bij te snijden versies, alle vijf **1080×1080
(1:1)**, met een korte kop erboven. Upload die vijf. Instagram snijdt een
carrousel bij op de verhouding van de eerste slide, dus alle slides moeten
dezelfde verhouding hebben.

Waarom vierkant en niet staand (4:5): de schermafbeeldingen zijn breed. In een
staand vlak wordt het beeld geen millimeter groter — er komt alleen lege ruimte
onder en boven bij.

Wat er per slide is weggesneden:
- **1**: de Windows-titelbalk, en vier kaarten in plaats van zes — zo zijn de
  orgelnamen op een telefoon leesbaar.
- **2**: alleen de titelbalk; de hele speeltafel blijft in beeld.
- **3**: de titelbalk van het browservenster en de schuifbalk aan de rechterkant,
  zodat het als een telefoonscherm oogt.
- **4**: alleen de kolom met stemming en nagalm, op vol formaat — nu is de regel
  *"gemeten pijptoonhoogtes (2.392 van 2.392 pijpen)"* leesbaar.
- **5**: alleen de titelbalk; taalknoppen en kleurthema's blijven leesbaar.

Het script dat de uitsneden maakt staat in `marketing/snij_insta_v037.py`:
pas daar de uitsnede of de koptekst aan en draai het opnieuw. Wil je de slides
zonder koptekst, haal dan de twee `gecentreerd(...)`-regels weg.

## Caption (NL)

🎹 **JM-Orgue — een jaar bouwen in één post, en dan: hij komt eraan.**

Sinds de vorige update is er zestien versies lang doorgebouwd. Een vogelvlucht
over wat er bij kwam:

🎵 **Klank**
▸ Mixturen en cornetten die uit meerdere pijprijen bestaan klinken nu volledig —
elke rij zijn eigen stem
▸ **Microfoonperspectieven** (voor, achter, droog) per orgel aan of uit, met
eigen volume
▸ **Echte tremulant-opnamen** spelen mee waar de sampleset ze levert
▸ **Hertemperen op gemeten pijptoonhoogte**: standaard klinkt een set precies
zoals hij is opgenomen, maar kies je middentoon of gelijkzwevend a=440, dan
wordt elke pijp zuiver gezet
▸ Nagalm bij staccato en het slotakkoord klinkt zoals het hoort — elke losgelaten
toets houdt zijn eigen uitklank

🎛️ **Jouw speeltafel**
▸ **Alle uitgangen van je geluidskaart** zijn per klavier te verdelen; twee
uitvoerprofielen wisselen met één knop tussen luidsprekers en hoofdtelefoon
▸ Zweltreden en generaal crescendo reageren **tot dertig keer sneller** — de
vertraging ging van zo'n 29 naar ongeveer 1 milliseconde
▸ **Een tablet of telefoon als tweede registreerscherm**: QR-code scannen en je
bedient registers, koppels en setzer vanaf de orgelbank
▸ Alles wat je speelt wordt op de achtergrond als MIDI-bestand bewaard

📚 **In gebruik**
▸ Een **bibliotheek** met foto, orgelbouwer en plaats bij elk orgel — en
samplesets die je rechtstreeks vanuit de app downloadt
▸ **Zeven talen**: Nederlands, Engels, Frans, Duits, Pools, Italiaans en Spaans
▸ Kleurthema's voor de speeltafel, van licht eiken tot nachtmodus
▸ **Bijwerken met één klik**: de app haalt de nieuwe versie op, installeert en
start zichzelf opnieuw op

🗓️ **En dan het nieuws: JM-Orgue gaat binnen een paar weken live — en is dan
voor iedereen beschikbaar.**

Volg deze pagina om de release niet te missen. 🎉

#orgel #pipeorgan #virtualorgan #organist #churchmusic #kerkorgel #muziek
#software #orgelbouw #JMOrgue #hauptwerk #grandorgue #sampleset #kerkmuziek

## Alt-teksten

1. Schermafbeelding van de JM-Orgue-bibliotheek: vier orgels als kaarten met een
   foto van het front, de naam van de orgelbouwer, de plaats en het aantal
   registers; bovenaan taalknoppen voor zeven talen.
2. Schermafbeelding van de speeltafel met het orgel van Friesach: vier werken
   naast elkaar (Pedal, Hauptwerk, Schwellwerk, Solowerk) met enkele registers
   getrokken, een balk met koppels en een setzerbalk onderin.
3. Schermafbeelding van een telefoon met de afstandsbediening van JM-Orgue: de
   registers van Pedal en Hauptwerk, waarvan dezelfde registers oplichten als op
   de speeltafel.
4. Schermafbeelding van de orgelinstellingen met het hoofdvolume, de stemming
   "Origineel (zoals opgenomen)" met de melding dat 2.392 van 2.392 pijpen zijn
   gemeten, de fijnstemming en de nagalm met de ruimtepreset "Dorpskerk".
5. Schermafbeelding van de algemene instellingen met zeven taalknoppen,
   kleurthema's voor de speeltafel en de audio-instellingen met polyfonie,
   buffergrootte en latentie.

## Nog te doen vóór plaatsing

- [x] Printscreens gemaakt uit versie 0.7.44 (`screenshots/instagram_v037/`)
- [x] Uitsneden klaar: de vijf `*_vierkant.png` (1080×1080)
- [ ] Datum van de release scherp krijgen voordat "een paar weken" wordt geplaatst
- [ ] Caption kopiëren, de vijf vierkante slides uploaden, plaatsen
