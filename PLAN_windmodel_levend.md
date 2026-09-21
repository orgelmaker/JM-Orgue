# Plan: levende wind (het Hollandse windmodel)

*Aangemaakt 21 september 2026, na de klacht op 0.7.50: "ik mis nog iets in het
windmodel waardoor je dat Hollandse levendige effect krijgt."*

## 1. Wat het onderzoek opleverde

Vier onafhankelijke onderzoekslenzen (orgelbouw/akoestiek, concurrenten en
sampleset-data, onze eigen code, de luisteraar), drie ontwerpen, een jury van
drie en een volledigheidscriticus kwamen op hetzelfde uit. Levende wind is
**niet** dat het hele orgel meezakt (dat deed 0.7.50, en dat hoort het oor als
pitch-bend). Het is:

1. **Verschil tussen pijpen.** Bij dezelfde drukdaling zakken labialen 0,8-1,8
   cent per procent (wijde mensuur en kleine pijpen het meest: Holpijp 8' c'''
   1,84, Fluit 4' c'' 0,80 — Logos-meting), strijkers weinig, tongwerken
   nagenoeg niet (de tong bepaalt de toonhoogte). Een pleno met Trompet gaat
   bij een dip dus even *uit elkaar* en trekt daarna weer strak. Een
   gemeenschappelijke daling van 12 cent hoor je alleen tegen de galmstaart;
   3 cent verschil tussen twee gelijktijdige pijpen geeft direct zwevingen.
2. **Verschil in gewicht.** Windverbruik per pijp schaalt met f^-1,5 en met
   √voetmaat (uit de Hauptwerk-orgeldefinities: 16'-C = 4× 8'-C, 4'-C = ¼,
   16'-C ≈ 130× een 2'-c'''). Een pedaal-Subbas trekt de wind uit het manuaal;
   een mixtuurpijpje doet niets. 0.7.50 telde elke stem als 1.
3. **Verschil in tijd.** Twee tijdschalen: de trage balg (2-8 Hz) en de snelle
   ventielkast/lade (10-25 Hz, GOArt-metingen) die bij elke inzet en loslaat
   een korte dip resp. opstoot geeft — Fisk's "negative pulse … positive
   pulse". Kort, aan een echte gebeurtenis gebonden, nooit periodiek.
4. **Wat het model níet mag doen:** iets toevoegen wat de opname al bevat.
   Elke pijp is opgenomen met echte wind, dus aanspraak en eigen wiebel zitten
   al in de sample; een gemeenschappelijke wiebel (0.7.50's flutter) klinkt als
   een tremulant. Sonus Paradisi zet Hauptwerks per-pijp-jitter bewust uit.
5. **Een bug.** Release-staarten (one_shot, releasing=false tot het einde van de
   opname) telden als windverbruikers: op een natte set bleef de balg 2-6 s na
   het loslaten ingezakt en veerde nooit terug.

## 2. Goudwaarden (één kalibratietabel)

| grootheid | waarde | bron |
|---|---|---|
| verbruik 8'-C | 1,0 (eenheid) | definitie |
| per octaaf binnen een register | ×2,83 (f^-1,5) | HW-ODF SJdL: 2,06e-3 / 7,3e-4 / 2,58e-4 / 9,1e-5 / 3,2e-5 kg/s voor C/c/c'/c''/c''' |
| per octaaf voetmaat, zelfde toets | ×4 (extra √(voet/8)) | HW-ODF: 16' C 2,06e-3, 8' C 5,16e-4, 4' C 1,29e-4 |
| vloer kleinste pijp | 0,03 | HW-vloer 1,6e-5 kg/s (1/129 van 16' C) |
| familie verbruik | principaal 1,0; fluit 1,2; gedekt 0,7; strijker 0,6; mixtuur = aantal koren (vloerpijpjes op 2'/4'); tongwerk 0,8 | mensuur; koren uit de naam (0.7.52) |
| toonhoogte referentie (8'-prestant c') | 80 cent per eenheid druk | HW 50-85; Logos 0,8-1,8 cent/% |
| familie toonhoogte | principaal 1,0; fluit 1,4; gedekt 1,3; strijker 0,6; mixtuur 1,2; tongwerk 0 | Logos, HW-praktijk, tongfysica |
| grootte | 0,75 (8' C) → 1,0 (c') → 1,25 (c''') | Logos: klein > groot |
| spreiding per pijp | ±20 %, deterministisch | perceptie: elke pijp nét anders |
| sterkte | labiaal ≈ druk² (−0,9 dB bij −5 %); tongwerk 1,8× | Fletcher; HW AmpLvl 45→101 % |
| statische inzakking | max_sag · q²/(1+q²), q = verbruik / (pleno × balggrootte/0,5) | ΔP ∝ Q² (Logos, Fraunhofer); referentie schaalt met het orgel |
| balg | f0 = 3/balggrootte (6 Hz bij 50 %), ζ 0,12-1,0 | ongewijzigd t.o.v. 0.7.50 |
| lade | 9-15 Hz, ζ 0,55-0,25 ("kanaal" kort → lang) | GOArt 10-25 Hz; Chalmers: kanaallengte dominant |
| stoot | één 16'-C → dip ~3 % op de eigen lade, 0,65× op de andere laden van de groep; loslaten 0,4× omhoog | Woolley/Fisk; kalibratie in unittest |
| klemmen | lade 0,92-1,04; wat de stemmen zien: onder = 1 − max(1,6·daling, 0,08) − 0,02 (0,90 bij 5 %); balg 1-1,6·max_sag (kruipend) … 1,03; stoot per callback ±12 | karikatuurgrens ≤ 8 % kort bij de standaard, ≤ 5 % statisch |
| pijp als resonator | τ = 8 perioden (2' c''' 2 ms, 8' c' 30 ms, 16' C 240 ms) | Fletcher & Rossing; criticus |
| doffer | tilt boven 1,5 kHz, k = 6,0·doffer·daling (max 0,6) — −2 dB bij 3,2 % bij doffer 1,0 (0.7.52; was 2,4) | HW HarmonicShaping −2 dB per 3,2 % flow |
| groepsflutter | 0,04 × daling (was 0,15) | samples bevatten de eigen wiebel al |

## 3. Fasen

**Fase 1 — 0.7.51 (klaar 21/22 september 2026).** Verbruiksgewicht per stem +
staartfix + Q²-curve met orgelgeschaalde referentie; differentiële gevoeligheid
per stem (familie × grootte × spreiding, tongwerk apart); snelle lade per
divisie met gebeurtenisgestuurde stoten (alleen bij een écht gespawnde stem,
niet bij koppel-bumps; loslaatopstoot; groepsbreed gedempt); pijp als
resonator; galmstaarten buigen niet mee (fade over 200 ms na de crossfade);
doffer-tilt per divisie; Karakter (Hollands/Neutraal/Eigen) met eigen
regelaars; meters per lade met vastgehouden dip; pas 1 alleen voor echte
divisies en actieve balgen; test-API `GET /wind` per lade, `POST
/settings/wind?karakter=&stoot=&kanaal=&doffer=&verschil=&tongwerk=`, `POST
/settings/wind_group`.

**Nazorg 0.7.52 (uit de review van 0.7.51).** Windstaat schoon bij
orgelwissel; staarten bevriezen de afwijking van het moment van loslaten
(dev_a = 0) in plaats van een afbouw; mixturen op hun hoogste koorpijp met
koren uit de naam; klem in pas 1 volgt de ingestelde daling; hint rekent met
de halve daling ("vol werk"); voorkeuze Hollands = kleine balg; pedaal-
referentie in het groot octaaf; doffer ×2,5; live herberekening van kp/kg
bij een instelling; kruipende ondergrens van de balg. Nog open uit de
review: doorlopend klavier geeft buiten het bereik de toets i.p.v. de
klinkende noot aan het windprofiel (octaaf verschil; alleen als die stand
aanstaat; fix = klinkende noot meesturen in NoteOn); de stemmenmeter telt
stemmen i.p.v. pijpen bij meerdere perspectieven; de gecombineerde klem ligt
bij de standaard op 10 %, niet op de 8 % uit de tekst.

**Fase 2 — cancel per toets (sleeplade).** Per (divisie, toets) het verbruik
tellen en een one-pole-kanaaldruk (τ 40 ms) die alleen zakt als er veel
registers op díe toets staan: de toets met 16'+mixtuur zakt, de buurtoets met
alleen 2' niet. Slew-staat ín de stem (mengpool-invariant), actieve-
toetsenlijst i.p.v. 4096-scan. Aanslagdip alleen voor het deel dat door de
ándere registers op de cancel komt (de sample heeft de eigen aanspraak al).

**Fase 3 — C/Cis-laden.** Schnitger/Hinsz/Bätz-laden zijn per divisie in twee
helften gedeeld (C-D-E-Fis-Gis-Ais / Cis-Dis-F-G-A-B), elk met eigen kanaal;
de mengloop pant al op toonpariteit. Lade-index = div·2 + (noot % 2), 64
laden, schakelaar in het Hollandse karakter.

**Fase 4 — Hauptwerk-ODF-import.** WindSupply_MassFlowRate per pijp
(genormeerd op 8'-C), SourceWindCompartment + Linkage-graaf → windgroepen
(laden die op dezelfde balg uitkomen = één groep), PitchLvl/HarmonicShaping/
AmpLvl per laag → gevoeligheid per register; noise-/tractuurranks → 0. Balg-
massa/-demping en FlowRandomisation bewust NIET (sjabloonwaarden: 800 kg op
4 m² zou 200 mm WK geven). Alleen startwaarden; opgeslagen instellingen winnen.

**Later, alleen na de luistertest:** tremulant als windapparaat (drukmodulatie
op de lade, asymmetrisch, alleen voor sets zonder tremulant-opnamen); onrust in
rust (balgwissel van spaanbalgen, trage drift) als optie "getreden wind";
per-pijp windjitter als Eigen-optie, standaard uit; pijpkoppeling (Sonus
"pipe coupling": bijna-unisono pijpen trekken naar elkaar toe, 100-300 ms).

## 4. Meetprotocol voor het echte orgel (gebruiker)

De enige gouden referentie is het instrument van de gebruiker zelf. Drie
opnamen met een microfoon dicht bij het discant:

1. **Pedaal-schrik**: een 2'-akkoord (of mixtuur) vasthouden, 16'-pedaal in
   achtsten. Cent-spoor per 10 ms via STFT rond de sterkste partiaal: hoe
   diep, hoe snel, hoeveel naveerslagen, hoe groot de opstoot bij loslaten.
2. **Eén Holpijp, twee handen**: mag níets doen (< 0,5 cent).
3. **Pleno met Trompet, akkoord 2 s**: cent-spoor van Prestant, Mixtuur en
   Trompet apart — labialen omlaag, Trompet blijft staan?

Daarmee zijn stoot, kanaal en de familiegevoeligheden op het echte orgel te
ijken in plaats van op een Hauptwerk-imitatie.

## 5. Luistertest

Drie fragmenten, blind, in willekeurige volgorde (0.7.50 / Neutraal /
Hollands): een koraal in het pleno met liggende sopraan en bewegende bas, een
Sweelinck-variatie met staccato-pedaal onder een 2'-akkoord, en één Holpijp
met twee handen. Twee vragen per fragment: *ademt het, of wiebelt het?* en
*klinkt er iets vals?* Hollands moet "ademt" winnen en nooit "wiebelt" of
"vals" krijgen; de Holpijp mag in geen enkele versie iets doen.
