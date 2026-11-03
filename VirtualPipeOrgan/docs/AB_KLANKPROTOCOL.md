# A/B-klankprotocol: JM-Orgue tegenover een referentiespeler

**Doel:** vaststellen of JM-Orgue een set (bijvoorbeeld een Hauptwerk Custom Organ) klinkt zoals de
referentiespeler, per klankaspect, met meetwaarden én luisterbevindingen. Een indruk van een forum
is geen regressietest.

**Stand:** dit protocol is opgesteld en de meetkant (`testscripts/ab_vergelijk.py`) werkt. Er is nog
**geen** vergelijking uitgevoerd: daarvoor is een complete originele set met al haar samplepakketten
nodig én een referentiespeler waarop die set draait. Zonder die twee blijft de uitkomst "nog niet klinkend
gevalideerd".

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
