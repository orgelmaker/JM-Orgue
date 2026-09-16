# Instagram Post — JM-Orgue: alles wat het nu kan + "eind september live" (v037)

**Status:** showcase met release-aankondiging (8 slides). Vervangt v036.

De vorige post (v036) liet drie dingen zien bij versie 0.7.28: de speeltafel, de
notatie en de update-melding. We staan nu op **0.7.45**. Deze post laat daarom
het **hele instrument** zien — inclusief punten uit eerdere posts, want wie nu
pas meekijkt moet in één carrousel zien waarom JM-Orgue er mag zijn — en
kondigt aan dat de software **eind september live gaat en voor iedereen
beschikbaar is**.

Toon: op hoofdlijnen, geen techniek. Wat de organist eraan heeft, niet hoe het
onder de motorkap werkt.

## Slides (8)

Bestanden in `screenshots/instagram_v037/`. Upload de acht `*_vierkant.png` in
deze volgorde; de `bron_*.png` zijn de onbewerkte schermafbeeldingen.

| # | Uploaden | Kop op de slide | Wat je ziet |
|---|----------|-----------------|-------------|
| 1 | `slide1_bibliotheek_vierkant.png` | De bibliotheek | Orgels als kaarten met foto, orgelbouwer, plaats en aantal registers; zeven taalknoppen; knoppen om een GrandOrgue-set, een JM-Rec-opname of een externe map te laden |
| 2 | `slide2_speeltafel_vierkant.png` | De speeltafel | Friesach: 44 registers over vier werken, registratie getrokken, koppelbalk, setzerbalk en de zwelkastwijzer |
| 3 | `slide3_notatie_vierkant.png` | Bladmuziek uit je eigen spel | Een ingespeeld trio op drie balken (Hoofdwerk, Nevenwerk, Pedaal) met de werkbalk: terugspelen, opslaan als MIDI, afdrukken/PDF |
| 4 | `slide4_telefoon_vierkant.png` | Je telefoon als registreerscherm | Dezelfde registratie als op slide 2, live op een telefoon |
| 5 | `slide5_stemming_vierkant.png` | Stemming en klank | Stemming "Origineel (zoals opgenomen)", fijnstemming en de nagalm met ruimtepreset |
| 6 | `slide6_klavieren_vierkant.png` | Elk klavier zijn eigen weg | Per klavier: MIDI-kanaal, toetsbereik, zwelkast, windvoorziening, panning, uitgangen en tremulant |
| 7 | `slide7_console_vierkant.png` | Je eigen speeltafel praat mee | Terugkoppeling naar registerlampen en displays, consoleknoppen inleren, feedback en updates vanuit de app |
| 8 | `slide8_talen_vierkant.png` | Voor iedereen | Zeven talen, kleurthema's, en de audio-instellingen |

Slide 3 komt uit de vorige post (dezelfde functie, ongewijzigd); de andere zeven
zijn geschoten uit versie 0.7.44; 0.7.45 veranderde niets aan het beeld.

## Controlelijst: is alles meegenomen?

Per uitgebrachte versie sinds de vorige post, en waar het in deze post terugkomt.
"Caption" = benoemd in de tekst, "slide" = ook te zien.

| Versie | Wat erin kwam | In deze post |
|--------|---------------|--------------|
| 0.7.29 | BLE-MIDI naar de notatie; eerste macOS-build | caption (draadloze klavieren) · macOS bewust weggelaten, zie hieronder |
| 0.7.30 | Registerknoppen schalen mee met het venster | zichtbaar op slide 2 |
| 0.7.31 | Venstergrootte onthouden, audiowissel betrouwbaar, geen tikken, feedback met log | slide 7 (feedback) · rest is stabiliteit, niet benoemd |
| 0.7.32 | Samplesets downloaden vanuit de bibliotheek | caption + slide 1 |
| 0.7.33 | Nagalm volgt je luidsprekers; geen tik bij loslaten | caption (klank) |
| 0.7.34/35 | Update-melding tijdens spelen — en weer teruggedraaid | bewust niet benoemd |
| 0.7.36 | Echte stereo, polyfonie instelbaar, koppelbalk, slepen over registers, JM-Rec-import | caption + slides 2 en 8 |
| 0.7.37 | Alle uitgangen van de geluidskaart per klavier, snelle profielwissel, volledige vertaling | caption + slides 6 en 8 |
| 0.7.38 | Gestapelde ranks, microfoonperspectieven, hertemperen, afstandsbediening, MIDI-archief | caption + slides 4 en 5 |
| 0.7.39 | Tremulant-opnamen, galm bij korte noten, snellere treden, afstandsbediening volgt het scherm, startscherm, JM-Rec-namen, orgelfoto's | caption + slides 1 en 4 |
| 0.7.40 | Bijwerken met één klik; Pools, Italiaans en Spaans erbij | caption + slides 1 en 8 |
| 0.7.41–0.7.45 | Nazorg: galm, tremulant, pedalen, zweltreden en crescendo, stabiliteit | caption (klank/treden) · reparaties niet apart benoemd |

Uit eerdere posts herhaald, omdat het de kern van het instrument is: notatie
(slide 3), registerlampen en pistons op de eigen console (slide 7), opgenomen
kerkakoestiek, historische temperamenten, extra registerschermen, MIDI- en
MP3-opname, en de bediening via rond of recht getekende registerknoppen.

**Bewust niet in de post:**
- **macOS.** Er is sinds 0.7.29 een macOS-build, maar die is niet ondertekend en
  nauwelijks getest. Niet noemen tot dat rond is — anders beloven we iets wat we
  bij de release niet waar kunnen maken.
- **Techniek onder de motorkap** (buffergroottes, latentiecijfers, hoe de galm
  rekent). Interessant voor een handleiding, niet voor een carrousel.
- **Opgeloste fouten.** Een release-aankondiging vertelt wat het kan, niet wat er
  stuk was.
- **De 0.7.34-vergissing** (updatemelding tijdens het spelen, meteen
  teruggedraaid).

## Caption (NL)

> Instagram staat maximaal 2200 tekens toe. Deze caption telt er 1841
> inclusief hashtags, dus er is nog ruimte om iets toe te voegen.

🎹 **JM-Orgue is af — en gaat eind september live.**

Sinds de vorige post is er flink doorgebouwd. Een rondje langs wat het nu is:

⛪ **Je eigen orgel in huis**
▸ Speelt samplesets van GrandOrgue en Hauptwerk, én je eigen opnamen uit JM-Rec
▸ Een bibliotheek met foto, orgelbouwer en plaats bij elk orgel — en sets die je
rechtstreeks vanuit de app binnenhaalt
▸ Laadt in seconden op een gewone Windows-pc

🎵 **Klank waar je naar wilt luisteren**
▸ De opgenomen kerkakoestiek, in stereo, met de nagalm van de ruimte zelf
▸ Mixturen, cornetten, microfoonstandpunten en echte tremulant-opnamen: alles
wat de sampleset levert, klinkt ook
▸ Origineel zoals opgenomen — of stem het hele orgel in een van de ruim vijftig
historische temperamenten
▸ En wil je één pijp net iets zachter of anders gestemd? Dat kan ook

🎛️ **Jouw speeltafel, jouw manier**
▸ Klavieren via usb of draadloos via bluetooth
▸ Elk klavier zijn eigen MIDI-kanaal, zwelkast en luidsprekers
▸ Zweltreden, generaal crescendo, koppels, setzer en pistons: allemaal zelf in
te leren
▸ Registerlampen en displays van je eigen console lichten mee
▸ Extra registerschermen, en een tablet of telefoon als tweede registreerscherm

🎼 **En het onthoudt wat je speelt**
▸ Je spel verschijnt live als bladmuziek — elk werk op zijn eigen balk — en gaat
als PDF of MIDI de deur uit
▸ Alles wat je speelt wordt vanzelf bewaard, en je luistert het terug op
hetzelfde orgel
▸ Opnemen als mp3 om te delen, of een midibestand door het orgel laten spelen

🌍 **Voor iedereen**
▸ Zeven talen, een speeltafel in je eigen kleuren, en bijwerken met één klik

🗓️ **Eind september is JM-Orgue er — voor iedereen.** Volg deze pagina, dan mis
je de release niet. 🎉

#orgel #pipeorgan #virtualorgan #organist #kerkorgel #churchmusic #kerkmuziek
#muziek #orgelbouw #JMOrgue #hauptwerk #grandorgue #sampleset

## Alt-teksten

1. Schermafbeelding van de JM-Orgue-bibliotheek: vier orgels als kaarten met een
   foto van het front, de orgelbouwer, de plaats en het aantal registers;
   bovenaan taalknoppen voor zeven talen.
2. Schermafbeelding van de speeltafel met het orgel van Friesach: vier werken
   naast elkaar met enkele registers getrokken, een balk met koppels en een
   setzerbalk onderin.
3. Schermafbeelding van het notatievenster: een ingespeeld orgeltrio op drie
   notenbalken met de titel "Psalm 134", met knoppen om af te spelen, op te
   slaan als MIDI en af te drukken als PDF.
4. Schermafbeelding van een telefoon met de afstandsbediening van JM-Orgue,
   waarop dezelfde registers oplichten als op de speeltafel.
5. Schermafbeelding van de orgelinstellingen met de stemming "Origineel (zoals
   opgenomen)", de fijnstemming en de nagalm met een ruimtepreset.
6. Schermafbeelding van de instellingen per klavier: MIDI-kanaal, toetsbereik,
   zwelkast, windvoorziening, panning, uitgangen en tremulant.
7. Schermafbeelding van de instellingen voor terugkoppeling naar de console:
   registerlampen en displays, consoleknoppen inleren, en knoppen voor feedback
   en updates.
8. Schermafbeelding van de algemene instellingen met zeven taalknoppen,
   kleurthema's voor de speeltafel en de audio-instellingen.

## Nog te doen vóór plaatsing

- [x] Acht slides geschoten en bijgesneden (1080×1080, `*_vierkant.png`)
- [x] Gecontroleerd tegen alle releases sinds de vorige post (zie controlelijst)
- [x] Datum scherp: de release staat gepland voor eind september 2026
- [ ] Caption kopiëren, de acht slides in volgorde uploaden, plaatsen

Het script dat de uitsneden maakt staat in `marketing/snij_insta_v037.py`
(draaien vanuit de repo-root). Pas daar de uitsnede of de koptekst aan en draai
het opnieuw; zonder koptekst haal je de twee `gecentreerd(...)`-regels weg.
