# A/B-klankprotocol: JM-Orgue tegenover een referentiespeler

**Doel:** vaststellen of JM-Orgue een set (bijvoorbeeld een Hauptwerk Custom Organ) klinkt zoals de
referentiespeler, per klankaspect, met meetwaarden én luisterbevindingen. Een indruk van een forum
is geen regressietest.

**Stand:** dit protocol is opgesteld en de meetkant werkt: `testscripts/ab_vergelijk.py` voor één noot
met de hand, en sinds C3 `testscripts/referentieproef.py` voor een vaste MIDI-reeks, automatisch
opgenomen en per noot vergeleken (sectie 7). Er is nog **geen** vergelijking met een referentiespeler
uitgevoerd: daarvoor is een referentiespeler nodig waarop dezelfde set draait, en een opname daarvan
(sectie 7.3). Zonder die opname blijft de uitkomst "nog niet klinkend gevalideerd". Wel is de proef
al bruikbaar als vangnet voor JM-Orgue zelf: dezelfde set voor en na een wijziging (sectie 7.2).

## 1. Randvoorwaarden (voor elke vergelijking vastleggen)

Leg per meting vast en houd gelijk voor A en B:

| onderdeel | vastleggen |
|---|---|
| set en versie | volledige set met alle pakketten; welke pakket-ID's en versies |
| registratie | exact dezelfde registers, koppels en tremulanten (lijst bewaren) |
| MIDI | dezelfde MIDI-opname (bewaar het bestand); zelfde snelheid, kanalen en tempo |
| stemming | beide op concertpitch en gelijkzwevend, of beide Origineel; in JM-Orgue de stand van "bijstemmen" noteren |
| niveau | uitgangsniveau van beide op elkaar afstemmen op een referentietoon; niet via normaliseren achteraf |
| uitvoerroute | dezelfde interface, samplerate (bijv. 48 kHz) en dezelfde opnameroute (loopback of het uitgangssignaal zelf) |
| galm en effecten | beide zonder extra galm en EQ, tenzij dat onderdeel van het aspect is |
| perspectief | dezelfde microfoonpositie (of dezelfde mix) |
| machine | computer, OS, buffermaat, versie van beide programma's |

## 2. Opnemen

1. Neem op in WAV, 24 bit, 48 kHz, stereo, zonder bewerking.
2. Gebruik per aspect een korte, herhaalbare MIDI-reeks (zie hieronder) en neem A en B op met hetzelfde
   startmoment. Lijn de opnames daarna uit op de aanzet van de eerste noot (alleen verschuiven, niet bijsnijden).
3. Bewaar de ruwe opnames en de gebruikte MIDI-bestanden bij de uitkomst.

## 3. Aspecten en wat je per aspect doet

| aspect | MIDI-reeks | meten (`ab_vergelijk.py`) | luisteren |
|---|---|---|---|
| **Aanslag** | losse noten, laag/midden/hoog, zacht en hard | aanzettijd (10-90 %), niveau, zwaartepunt | klopt de aanzet (chiff, tong)? |
| **Sustain** | één noot 4 s | niveau, grondtoon (cent), boventonen, zwaartepunt | stabiel, geen lus-tik, geen drift |
| **Korte release** | noot 80-120 ms | uitsterftijd (-40 dB), niveau | klinkt het als een korte noot, niet afgekapt? |
| **Lange release** | noot 3 s, loslaten | uitsterftijd (-40 dB), nagalm | volle nagalm, geen tik bij het overgaan |
| **Tremulant** | noot 6 s met tremulant aan en uit | modulatiediepte en -snelheid (omhullende en toonhoogte) | beweging, synchroon over registers |
| **Zwelkast** | noot 6 s, zwelkast dicht naar open | niveau- en zwaartepuntverloop | verandert alleen wat in de kast staat |
| **Winddynamiek** | tutti-akkoord aanzetten en loslaten, snel spel | niveau tijdens akkoordwissel | "levend" of vlak; wind zakt in? |
| **Perspectieven** | dezelfde noot per microfoonpositie | niveau en verhouding direct/nagalm | juiste afstand en balans |
| **Koppels** | zie `codm_integratie.py` (kleur per klavier) | welke klavieren op welke noot klinken | zelfde klank als de referentie bij dezelfde koppelstand |

## 4. Beoordelen

- **Meetwaarden:** zet per aspect A, B en het verschil in een tabel. Een afwijking boven ongeveer
  1,5 dB in niveau, 5 cent in toonhoogte of 20 % in aanzet- of uitsterftijd is een aanleiding om te
  luisteren, geen oordeel op zich.
- **Luisterbevindingen:** twee personen luisteren blind (A/B in willekeurige volgorde) en noteren per
  aspect: gelijk / verschil hoorbaar / verschil storend, met een korte omschrijving.
- Vermeld welke combinaties **niet** getest zijn (set, registratie, stemming, perspectief).

## 5. Uitvoeren met de meegeleverde hulpmiddelen

```bash
python testscripts/ab_vergelijk.py jm.wav ref.wav --aan 0.5 --uit 2.5
```

De uitvoer is een tabel met niveau, grondtoon, zwaartepunt, boventonen, aanzettijd en uitsterftijd
van beide opnames en het verschil, plus het toonhoogteverschil in cent.

De zelfgemaakte proeforgels (`testscripts/maak_codm_*.py`) en `testscripts/codm_integratie.py` laten zien
dat de **engine** doet wat de definitie zegt (richting en transpositie van koppels, release-keuze,
toonhoogte van een gedeelde opname, standaard-aan, pistons). Dat is geen klankvergelijking met een
referentie: het bewijst het gedrag, niet de klank.

## 6. Wat dit niet bewijst

- Gelijkheid met Hauptwerk bij sets waarvan de samples ontbreken.
- Tremulant met golfvormen en reverb tail truncation: die worden niet toegepast, dus daar is een
  verschil te verwachten en hoort het verslag dat te melden.
- Prestaties op een andere computer dan die van de meting.

## 7. De vaste proef (`testscripts/referentieproef.py`, taak C3)

Eén herhaalbare proef in plaats van losse noten. De MIDI-reeks is vast
(`testscripts/testdata/referentieproef.mid`, 49,9 s, te regenereren met `referentieproef.py midi`):
1,5 s stilte, daarna zes aangehouden noten (MIDI 36, 48, 60, 72, 84, 96; 2,5 s ingedrukt, 4 s stilte
erna) en drie staccatonoten op 60 (40, 100 en 300 ms). Alle opnamen die je wilt vergelijken gebruiken
**deze** reeks; met een andere `--gap` of ander bestand zijn ze niet vergelijkbaar.

### 7.1 Per noot gemeten

| grootheid | definitie |
|---|---|
| niveau (dB) | RMS over het sustainvenster (1,2 s tot 0,1 s voor het loslaten), ruisvermogen eraf; bij een staccatonoot de piek in vensters van 10 ms |
| toonhoogte (ct) | de **laagste sterke deeltoon** (binnen 15 dB van de sterkste), parabolisch geïnterpoleerd, ten opzichte van gelijkzwevend A=440, gevouwen op een octaaf. Een quint- of terspijp telt niet mee (`partieel`) |
| aanzet (ms) | 10 → 90 % van het sustain-amplitude. Alleen gemeld als de 10 %-drempel minstens 6 dB boven de ruis ligt; onder 30 ms wordt geen verhouding berekend (vensters van 10 ms) |
| uitsterven (ms) | tijd van het eerste zakken tot **−3 dB** onder het sustain tot −10, −20, −30 en −40 dB. Een diepte telt alleen als haar doel 6 dB boven de ruis ligt; de vergelijking gebruikt per noot de diepste diepte die beide opnamen konden meten |

Afwijkingen van sectie 3/`ab_vergelijk.py` en waarom (alle drie kwamen uit de herhaalbaarheidsproef):

1. **Grondtoon = laagste sterke deeltoon**, niet de sterkste piek. In de bas wisselt de sterkste piek tussen de 1e
   en 2e deeltoon en die liggen door onharmonische deeltonen ~4 ct uit elkaar: 6 ct verschil tussen twee identieke
   opnamen.
2. **Uitsterven vanaf −3 dB**, niet vanaf het nominale NoteOff. Dat nulpunt hangt van de uitlijning af en die van
   het niveau: 26 ms verschil tussen opnamen van dezelfde klank op 20 dB van elkaar gaf ×1,24 en ×13 schijnverschil.
3. **Meerdere diepten.** Een zachte opname haalt −40 dB niet boven de ruisvloer (de Kolonics-set met de niveaus van de
   maker: sustain rond −45 dBFS, ruis −69 dBFS); dan meet je de ruis. Het ruisvermogen wordt afgetrokken.

### 7.2 JM-Orgue tegen JM-Orgue (voor en na een wijziging, vangnet voor C1, C2, C4)

```bash
# app starten:  vpo-app.exe --test-api
python testscripts/referentieproef.py opnemen --orgel "<pad naar de set>" --uit voor --registers Principal Oktav
# wijziging bouwen, app herstarten
python testscripts/referentieproef.py opnemen --orgel "<pad naar de set>" --uit na  --registers Principal Oktav
python testscripts/referentieproef.py vergelijk voor na --uit vergelijking --streng
```

`opnemen` laadt het orgel, zet de galm van JM-Orgue op droog, trekt per register alleen dat register, wijst MIDI-kanaal 1
aan de divisie van dat register toe, speelt het bestand af en bewaart `NN-<register>.mp3` en `manifest.json` (versie,
stemstand, schakelaars, tijdlijn, machine). Met `--voor "POST /pad {json}"` zet je vooraf een instelling. De opname van de
test-API is MP3 (320 kbps): voor toonhoogte en niveau ruim voldoende; tijden hebben een raster van 10 ms. Natte sets:
`--gap 6` of meer. Zonder `--registers` worden de eerste drie registers genomen (`--max`).

`vergelijk` zet per register het niveau, de balans tussen de noten, de grootste toonhoogteafwijking en de verhouding van
aanzet en uitsterven in een tabel, plus per noot de details, en schrijft `vergelijking.md` en `.json`. Een vlag is een
aanleiding om te luisteren (1,5 dB, 5 ct, 20 %), geen oordeel. `--streng` geeft afsluitcode 1 bij een vlag.

**Gemeten ruisvloer van de methode** (Kolonics Brasov near, registers Principal en Oktav, dezelfde instellingen twee keer
opgenomen; de engine is hierbij deterministisch): niveau +0,0 dB, balans 0,0 dB, toonhoogte 0,0 ct, aanzet ×1,00,
uitsterven ×1,00 tot ×1,05, nul vlaggen. De richtwaarden liggen daar ruim boven.

**Controle dat de proef een echte wijziging ziet:** dezelfde set met de niveaus van de maker aan tegen uit
(E1, `POST /niveaus_toepassen`). Uitkomst: niveau +20,0 dB op elke noot (de set stelt −20 dB in), balans 0,0 dB,
toonhoogte 0,0 ct, aanzet ×1,00–1,02, uitsterven ×1,00–1,10; de enige vlag is het niveau.

### 7.3 Tegen een referentiespeler (GrandOrgue, Hauptwerk)

1. Dezelfde set in de referentiespeler laden, op concertpitch en gelijkzwevend, galm en EQ uit.
2. Per register: **alleen dat register** trekken, geen koppels, geen tremulant; MIDI-kanaal 1 naar het klavier met dat
   register. De registerlijst bewaren.
3. `testscripts/testdata/referentieproef.mid` afspelen naar de speler en het uitgangssignaal opnemen als WAV, 24 bit,
   48 kHz, zonder bewerking. Het startmoment hoeft niet gelijk te zijn (de proef zoekt de verschuiving zelf, ±3 s).
4. De bestanden `NN-<register>.wav` noemen in dezelfde volgorde als de JM-Orgue-opnamen (de naam na `NN-` wordt gebruikt
   om te koppelen, anders de volgorde), in één map; versie en instellingen van de speler erbij noteren.
5. `python testscripts/referentieproef.py vergelijk <map jm> <map referentie> --niveau-uitlijnen`. Het
   uitgangsniveau van twee programma's verschilt altijd; `--niveau-uitlijnen` laat het gemiddelde verschil buiten
   beschouwing en toont de balans tussen de noten, die wél telt.

Nog **niet** gedaan: een referentieopname. GrandOrgue staat niet op de ontwikkelcomputer, en de proef vereist een
handmatige GUI-sessie per register. Dit is de volgende stap zodra een referentie beschikbaar is.

### 7.4 Beperkingen

- Een register dat een noot niet kan spelen (bereik) geeft "stil in beide"; klinkt een noot in slechts één opname en is
  hij daar minstens 12 dB boven de ruis, dan is dat een vlag; zachter is een grensgeval en telt niet.
- Aanzet en diepe uitsterftijden blijven achterwege bij een zachte opname (het script meldt `-`). Een hogere
  uitgangsstand maakt meer noten meetbaar.
- Het blazergeluid van de set zit in de opname en wordt als ruis behandeld (`ruis_db` in `analyse-*.json`).
- De toonhoogte van een quint- of terspijp, en van registers met meerdere koren (mixturen), wordt niet vergeleken.
- Staccatonoten tellen niet mee in het niveau van het register (hun niveau hangt van de aanzet af); ze staan wel in de tabel.
- Dit bewijst gedrag van de engine tegenover zichzelf of een opname, niet dat een set "goed klinkt".
