# Changelog

Alle belangrijke wijzigingen van JM-Orgue worden hier bijgehouden.

Format gebaseerd op [Keep a Changelog](https://keepachangelog.com/),
versies volgen [Semantic Versioning](https://semver.org/).

## [0.7.54] - 2026-09-23

### De registerknoppen lezen weer als registerknoppen

Bij een Hauptwerk-import stond er te veel op de knop. Twee dingen die er al
naast stonden, stonden er ook nog eens op.

**De divisieaanduiding is weg.** Sets zetten er vaak een kort woord voor:
"Pos: Prestant 8", "Pd Subbas 16", "RW Fluit 2". Boven de kolom staat al
welke divisie het is. JM-Orgue herkende zo'n aanduiding tot nu toe aan een
vaste lijst afkortingen, en die dekte "PED " en "GO " wel maar "Pos:" en "Pd "
niet. Zo'n lijst is nooit af, dus nu wordt de divisienaam zelf gebruikt: een
kort woord vóór de naam is een aanduiding wanneer het met dezelfde letter
begint als de divisie en zijn letters in volgorde in die divisienaam
voorkomen. "RW" zit zo in "Rugwerk" en "Pd" in "Pedaal". Een dubbele punt is
altijd goed genoeg bewijs, ook zonder dat.

Het blijft van de divisie afhangen, en dat is de bedoeling. "V Cornet" op het
Bovenwerk houdt zijn V, want dat zijn koren en geen aanduiding.

**De voetmaat staat er nog maar één keer.** Onder elke knop staat de voetmaat
al. Hauptwerk-sets schrijven hem meestal kaal achter de naam ("Gedekt 8",
"Sifflet 1 1/3") of met een voetwoord ("Subbaß 16 Fuß"), en JM-Orgue zocht
alleen naar de vorm mét voetteken ("Gedekt 8'"). Daardoor bleef hij staan en
kwam hij er eronder nog een keer bij. Nu wordt ook de kale vorm herkend, met
breuken en met voetwoord.

Alleen wanneer het achter de naam dezelfde voetmaat is als het register
werkelijk heeft. "Mixtuur 4" op een 2'-rang houdt dus zijn 4: dat zijn koren.

**En die voetmaat klopte niet altijd.** Bij het opruimen viel op dat een
register dat zijn rang een octaaf hoger aanspreekt de voetmaat van de rang
kreeg in plaats van die van zichzelf. Saint-Jean-de-Luz toonde daardoor
"Flûte 4" met "8'" eronder en "Flûte 2" met "8'". De sprong telt nu mee: een
16'-rang die een octaaf hoger wordt aangesproken klinkt als 8', een kwint
hoger als 5 1/3'.

Dat is uitdrukkelijk alleen het opschrift. Het hertemperen blijft rekenen met
de voetmaat van de rang zelf, want die twee door elkaar halen verstemt het
orgel een octaaf. Ze staan nu als twee aparte gegevens in de import.

Beide opschoningen gebeuren bij het inlezen, dus ze werken overal waar de
naam staat: het orgelscherm, de losse registerschermen, de afstandsbediening
en de crescendotabel.

### Vegen over de registers werkt nu ook met de vinger

Met de muis kon je al over de registerknoppen slepen om er in één beweging een
reeks aan of uit te zetten. Op een aanraakscherm gebeurde er niets: de
afhandeling liet alleen de muis toe. Vinger en pen doen nu mee.

Scrollen blijft werken, want het gebaar wordt per richting verdeeld. Staan de
divisies onder elkaar, dan veeg je zijwaarts over een rij registers en scrol
je op en neer door de lijst. Staan ze naast elkaar, dan veeg je op en neer
door een kolom en scrol je zijwaarts langs de divisies.

Herschikken blijft muis-met-Shift. Een aanraakscherm heeft geen Shift.

### Het notatievenster zegt dat het alfa is

Het notenschrift is nog niet nagelopen: toonhoogte, ritme en maatindeling
kunnen afwijken van wat er gespeeld is. Dat stond nergens. Er staat nu een
merkje op de knop "Noteren" en een balk boven in het notatievenster.

### De knop "Extern" is vervallen

Die opende een mapkeuze en gaf het pad door aan dezelfde route als "JM-Rec
import" — hetzelfde werk, twee knoppen. Hauptwerk- en GrandOrgue-sets komen
binnen via de knop ".organ", die beide bestandssoorten accepteert.

### Nagemeten

De zeven talen zijn nagelopen en niet alleen op de nieuwe teksten:

| controle | uitkomst |
|---|---|
| sleutels per taal | 951, in alle zeven gelijk |
| nieuwe alfa-teksten | in alle zeven, elk echt vertaald |
| vervallen Extern-teksten | overal weg |
| vaste teksten in de schermen | geen |
| lege waarden | geen |

En op het draaiende orgel, met Saint-Jean-de-Luz en Ledziny St. Clement:

| in het bestand | op de knop |
|---|---|
| `PED  Soubasse 16` | Soubasse, 16' |
| `PED  Bourdon 8` | Bourdon, 8' |
| `PED  Flûte 4` | Flûte, 4' |
| `GO  Quinte 2 2/3` | Quinte, 2 2/3' |
| `P  Subbaß 16 Fuß` | Subbaß, 16' |
| `M  Portunal-Flöte 8 Fuß` | Portunal-Flöte, 8' |

Verificatie in code: drie nieuwe unittests — de aanduiding die weg moet en de
aanduiding die moet blijven staan, de kale voetmaat met breuk en voetwoord,
"Mixtuur 4" dat zijn 4 houdt, en de octaafsprong die de voetmaat verschuift.

Het vegen met de vinger is niet met een echt aanraakscherm nagelopen; de
afhandeling en de scrollrichtingen zijn op de code gecontroleerd.

## [0.7.53] - 2026-09-22

### Hauptwerk-sets die "leeg" binnenkwamen laden nu wél

Het orgel van de Rotterdamse Laurenskerk liet zich niet inladen: JM-Orgue
meldde dat het gelukt was en toonde vervolgens een orgel zonder één register.
Hetzelfde gold voor Utrecht Dom, St. Anne's Moseley en de polyfonie-testorgels
— in feite voor élke Hauptwerk-set op die schijf.

**Wat er aan de hand was.** Hauptwerk kan een orgeldefinitie "compacted"
wegschrijven. Elk object wordt dan één regel met letter-afkortingen in plaats
van volledige veldnamen:

```
lang:    <Stop><StopID>1</StopID><Name>Gedekt 8</Name><DivisionID>5</DivisionID>…</Stop>
compact: <o><a>1</a><b>Gedekt 8</b><c>5</c></o>
```

De importer kende alleen de lange vorm — die is destijds gebouwd op
Saint-Jean-de-Luz en Ledziny, en dat zijn toevallig de enige twee sets die
níet gecomprimeerd zijn. Alleen het kopblok blijft in beide vormen leesbaar,
en daarom kwamen de naam van het orgel en de bouwer wél door: precies genoeg
om te denken dat het gelukt was.

**Hoe het nu werkt.** De letters volgen de veldvolgorde van het
Hauptwerk-schema, maar die volgorde verschilt per formaatversie: in versie 4
staat bij een zwelkast de pijp voorop, in versie 5 de kast, en `Sample` heeft
in versie 5 meer velden dan in versie 4. Een vaste vertaaltabel zou dus
stilletjes de verkeerde velden pakken. In plaats daarvan leidt JM-Orgue de
betekenis af uit het bestand zelf: welke letter van een pijp bevat uitsluitend
getallen die ook als rangnummer voorkomen, welk veld bevat een bestandsnaam,
welk veld is binnen één rang uniek (dat is de toets). Wat eruit komt is een
gewone veldnamenlijst, zodat de rest van de import — perspectieven,
zwelkasten, release-samples, voetmaten — ongewijzigd werkt.

Vier dingen bleken daarbij verraderlijk, en alle vier zijn ze getest:

- **Een veld dat ontbreekt heeft zijn standaardwaarde, en die is niet altijd
  nul.** Voor de MIDI-noot is het 60, de c' in het midden van het klavier. In
  177 van de 246 Rotterdamse rangen ontbrak daardoor precies die ene pijp.
  JM-Orgue leidt die standaard nu af uit de gaten in de rangen.
- **Het aantal toetsen van een register staat er vaak niet.** St. Anne's
  noteert het bij 8 van de 30 registers; ontbreekt het, dan beslaat het
  register zijn hele rang. Zonder die regel bleef dat orgel helemaal leeg.
- **Een veld op getalbereik kiezen is niet genoeg.** Een veld dat overal `1`
  was viel precies in het bereik van de octaafsprong. Dat kiezen verschuift
  een heel orgel een halve toon — met samples die gewoon bestaan, dus je merkt
  het alleen aan de toonhoogte. Het toetsbereik wordt nu gekozen op wat de
  meeste toetsen een pijp oplevert die in die rang ook echt bestaat.
- **Hetzelfde gold voor de release-grens.** Deze sets hebben drie
  release-opnamen per pijp: één voor een korte tik, één voor een middellange
  en één voor een lange noot. Welk veld die grens is, bleek niet uit het
  bereik — de crossfade-lengte valt er ook in. Het kenmerk is dat releases van
  dezelfde pijp juist in dít veld verschillen, en in de andere velden niet.
  Zonder die regel klonk bij elke toetsduur dezelfde release.

**Stereo-helften van één perspectief horen bij elkaar.** Hauptwerk splitst een
microfoonpositie soms in twee rangen: "(front-L component)" en "(front-R
component)". Die krijgen nu hetzelfde perspectief-label — anders klinken
front-links, front-rechts én rear alle drie tegelijk in plaats van dat je
kiest.

### Twee imports die stil mislukten geven nu een melding

- **Een orgeldefinitie zonder registers wordt geweigerd.** Dat was de tweede
  helft van het probleem: zonder melding is er niets om op te zoeken.
- **Een set met Hauptwerks eigen `.hbw`-opnamen wordt geweigerd.** Hauptwerk
  bewaart zijn eigen meegeleverde sets (waaronder St. Anne's Moseley) in een
  gesloten formaat dat alleen Hauptwerk zelf kan afspelen. De orgeldefinitie
  is gewoon leesbaar, dus zonder deze controle kwam er een volledig orgel
  binnen waar geen noot uit kwam — 4.501 van de 4.649 opnamen weigerden open
  te gaan, alleen zichtbaar als een regel per opname in het log. De melding
  noemt nu het formaat en de aantallen.

### Nagemeten

Elk sample-pad is op schijf gecontroleerd:

| set | klavieren | registers | samples | ontbrekend |
|---|---|---|---|---|
| Rotterdam Laurenskerk (surround) | 6 | 89 | 16.564 | 0 |
| Utrecht Dom v2 (surround) | 4 | 45 | 9.748 | 0 |
| Utrecht Dom demo | 4 | 12 | 2.316 | 0 |
| Polyfonie-testorgel (4 GB) | 1 | 9 | 976 | 0 |
| St. Anne's Moseley | — | — | — | geweigerd (`.hbw`) |

En op het draaiende orgel, met de Utrechtse demo:

- **Klank**: vier registers over drie klavieren, 32 stemmen tegelijk, piek
  0,15.
- **Zwelkast**: open 0,0857 tegen dicht 0,0072, oftewel de ingestelde −20 dB.
  Dat is meteen de test op de omgedraaide velden van versie 5.
- **Koppel**: met alleen een Bovenwerk-register getrokken blijft het
  Hoofdwerk-klavier stil, en klinkt het pas mét de koppel Hoofdwerk→Bovenwerk.
- **Release**: na een tik van 100 ms is de staart hoorbaar korter en zachter
  dan na een noot van 2,5 s (piek 0,0116 tegen 0,0235).

Verificatie in code: elf nieuwe unittests — herkenning van het formaat, de
weggelaten c', het toetsbereik zonder expliciet aantal, de omgedraaide
zwelkast, XML-entiteiten, de release-grens (en dat er géén grens uitkomt als
een pijp maar één release heeft), de `.hbw`-weigering, en drie op de
stereo-helften. Plus een diagnosetest die per objecttype toont welke letter
welk veld werd — handig wanneer Hauptwerk ooit weer een veld toevoegt.

### Goed om te weten

De Rotterdamse surround-set is groot: alleen al het achterperspectief vulde
ruim 20 GB werkgeheugen, en het inladen duurde bijna een uur vanaf een externe
schijf. Beide perspectieven tegelijk past niet in 32 GB.

Nog niet gedaan: de tremulant-opnamen van deze sets. Die hangen in Hauptwerk
aan een "alternatieve rang" die bij het aanzetten van de tremulant wordt
ingeschakeld, en die route leest JM-Orgue nog niet — de registers klinken dus
zonder tremulant-samples.

## [0.7.52] - 2026-09-22

### Nazorg levende wind: wat de review vond

Na 0.7.51 is de hele wijziging door een reviewronde gehaald (vier invalshoeken, elke bevinding door een tegenspreker bestreden): 35 bevindingen, 21 overeind, 14 weerlegd. De zeven die er echt toe doen zijn gerepareerd, plus een handvol kleine.

- **Orgelwissel liet het windmodel van het vórige orgel aanstaan.** De audiothread reset bij een nieuw orgel de zwelkasten en tremulanten, maar niet de wind: laadde je na een orgel met wind een orgel zonder opgeslagen windinstelling, dan ademde en stootte dat met de instellingen van het vorige — terwijl het scherm "uit" toonde. Bestond in mindere mate al sinds 0.7.36; nu worden balgen, laden, stoten, meters en de groepsindeling bij elke orgelwissel schoongeveegd, en pas daarna de opgeslagen groepen van het nieuwe orgel gezet.
- **Galmstaarten bogen nog mee met de loslaatopstoot.** 0.7.51 bouwde de windgevoeligheid van een staart in 200 ms af, maar liet hem intussen de druk volgen — precies in het venster waarin de opstoot komt: een pleno loslaten gaf alle staarten een korte "whoop" omhoog. Nu bevriest een losgelaten pijp (en zijn staart) de afwijking van het moment van loslaten: wat nog klinkt is uitsterven en opgenomen galm, en dat volgt de wind niet meer. Eenvoudiger én fysisch juist; de afbouw over 200 ms is weg.
- **Mixturen zonder voetmaat wogen als een 8'-register.** GrandOrgue-sets zonder HarmonicNumber — Friesach zelf — geven "8'" terug, en de familiefactor 1,5 maakte een Mixtuur IV daarmee ruim vier keer te zwaar in het windverbruik (en te traag als resonator). Een mixtuur rekent nu op zijn hoogste koorpijp (2' op een manuaal, 4' op het pedaal) en weegt zoveel vloerpijpjes als hij koren heeft, uit de naam ("IV", "4-5f.", anders 4): een Mixtuur IV op c' trekt 0,12, een 8'-prestant op dezelfde toets 0,35.
- **De schuif "Maximale winddaling" was boven 10 % dood.** Een vaste klem in de mengloop hield de afwijking die de pijpen zien op −10 %, terwijl balg en meter wél dieper gingen: schuif op 20 %, meter op 80 %, klank op 8 cent. De klem volgt nu de ingestelde daling (bij 5 % blijft alles zoals gemeten). En de hint eronder rekende met de volle daling, terwijl "vol werk" per definitie de hélft is: hij toonde 2× te veel cent. Nu "bij een vol werk zakt een prestant zo'n 2 cent, een fluit of mixtuur 4 cent, 0,4 dB zachter" bij 5 %.
- **De voorkeuze "Hollands" zette per ongeluk de grote balg.** Balggrootte bepaalt sinds 0.7.51 ook hoeveel vol werk de balg aankan; de knop "Ook op de schuiven" zette Hollands op 100 % en maakte het pleno daarmee statisch vlakker dan Neutraal. Omgedraaid, zoals het hoort: spaanbalgen zijn de kleine, nerveuze balg (50 %, daling 6 %), de magazijnbalg met ventilator de grote, rustige (100 %, daling 3 %).
- **De pedaalreferentie stond op de verkeerde toetsen.** "Vol werk" voor het pedaal werd op c–c' gerekend terwijl het pedaal in C–c wordt bespeeld: één Subbas-C op een eigen pedaalbalg zakte al bijna maximaal. Nu twee noten in het groot octaaf.
- **Doffer was 2,5× te zwak** tegenover de Hauptwerk-referentie uit het plan (−2 dB op de hoge boventonen bij 3 % minder wind): bij het gemeten pleno gaf hij −0,4 dB, onder de hoordrempel. Nu −1 dB bij het pleno, −2,4 dB bij een diepe schrik.
- **Verschil per pijp en "tongwerken apart" werken nu meteen** op klinkende pijpen, niet pas bij de volgende aanslag.
- **Live de daling verlagen** terwijl de balg ingezakt was gaf een sprong (de ondergrens verschoof in één sample); die kruipt nu.
- **Meetscript** zet na afloop de eigen windgroep-indeling en windinstellingen van het orgel terug (`GET /settings/wind_group`, `POST /settings/wind_restore`), ook als het halverwege strandt — 0.7.51's versie liet het orgel op "elke divisie een eigen balg" en zonder wind achter.
- Correctie op de 0.7.51-notities: het waren elf nieuwe unittests, niet drieëntwintig. Nu vier erbij, waaronder één op de mengloop zelf (een staart houdt de toonhoogte van het moment van loslaten) en één op de koren-uit-de-naam.

Bewust nog niet gedaan (staat in het plan): bij een doorlopend klavier krijgt een toets buiten het bereik het windprofiel van de toets in plaats van de klinkende pijp (een octaaf verschil, alleen als die stand aanstaat); de stemmenmeter telt stemmen en niet pijpen bij meerdere perspectieven.

## [0.7.51] - 2026-09-22

### Levende wind: het Hollandse windmodel

Na 0.7.50 luidde de klacht: "ik mis nog iets waardoor je dat Hollandse levendige effect krijgt." Vier onderzoekslenzen (orgelbouw en akoestiek, wat Hauptwerk, Sweelinq en Organteq doen en wat samplesets meegeven, onze eigen code, en de luisteraar), drie onafhankelijke ontwerpen, een jury van drie en een volledigheidscriticus kwamen op hetzelfde uit — en dat is nu gebouwd. Het plan met de goudwaarden staat in `PLAN_windmodel_levend.md`.

**Waar het in zat.** Levende wind is niet dat het hele orgel meezakt (dat deed 0.7.50, en het oor hoort dat als een pitch-bend). Het is dat pijpen op dezelfde wind elkaar iets aandoen:

- **Niet elke pijp zakt evenveel.** Gemeten aan echte pijpen: fluiten en kleine pijpen 1,5-1,8 cent per procent druk, een prestant 1, strijkers 0,6, en tongwerken blijven staan — de tong bepaalt hun toonhoogte, ze worden alleen zachter en dunner. Een pleno met Trompet gaat bij een dip dus even *uit elkaar* en trekt weer strak. Elke stem heeft nu zijn eigen gevoeligheid uit familie (uit de registernaam), grootte (kleine pijpen meer) en een vaste kleine spreiding per pijp. Tot nu toe kreeg alles exact dezelfde 120 cent per eenheid; de referentie (een 8'-prestant rond c') staat nu op 80, omdat het karakter uit de verschillen komt en niet uit de diepte.
- **Niet elke pijp trekt evenveel wind.** Uit de Hauptwerk-orgeldefinities: een 16'-C trekt vier keer een 8'-C en ruim honderd keer een 2'-pijpje. Tot nu toe telde elke stem als 1 — een pedaal-Subbas deed evenveel als een mixtuurpijpje. Nu weegt elke pijp naar voetmaat, toonhoogte en familie, en groeit de inzakking kwadratisch met het verbruik: één register doet niets, het pleno ademt. De referentie voor "vol werk" komt van het orgel zelf (het pleno van de groep), zodat een kistorgel en een domorgel bij hún pleno even ver zakken.
- **Twee tijdschalen.** Naast de trage balg (2-8 Hz) is er nu per divisie een snelle lade (9-15 Hz, instelbaar als "Kanaal": kort en wijd → lang en smal). Die krijgt bij elke inzet een stoot — alleen bij een écht geopend ventiel, niet als een koppel een al klinkende pijp raakt — en bij elk loslaten een opstoot. Kort, aan een gebeurtenis gebonden, nooit periodiek. Zet je pedaal en manuaal op één balg, dan laat een 16'-pijp het liggende discantakkoord schrikken, zoals op een orgel met gedeelde wind.
- **De pijp als resonator.** Een 16'-C kán een dip van 14 Hz niet volgen, een 2'-pijp wel: elke stem volgt de druk nu met een traagheid van acht perioden. Bas is de oorzaak, discant het slachtoffer.
- **Doffer bij inzakking.** Bij lagere druk vallen de hoge boventonen het eerst weg (Fletcher; Hauptwerk doet hetzelfde). Per divisie een lichte kanteling boven 1,5 kHz die met de druk meeloopt.

**Een bug die het ademen bij loslaten in de weg zat.** Release-staarten (de opgenomen galm na het loslaten) telden als windverbruikers, tot het einde van hun opname. Op een natte set bleef de balg daardoor twee tot zes seconden ná het loslaten ingezakt en veerde nooit terug. Nu trekt alleen een klinkende speelstem wind; gemeten: een halve seconde na het loslaten van een pleno-akkoord staat de balg weer op 100 %. En de staarten zelf buigen niet meer mee met latere druk — galm heeft geen toonhoogte die met een pedaalnoot van later meezakt; alleen tijdens de crossfade met de speelnoot volgen ze nog.

**Wat het model bewust níet doet.** Niets toevoegen wat de opname al bevat. Elke pijp is opgenomen met echte wind, dus aanspraak en eigen wiebel zitten al in de sample; de gemeenschappelijke wiebel van de balg is teruggebracht van 0,15 naar 0,04 van de daling, want alle pijpen tegelijk laten wiebelen klinkt als een tremulant. Geen per-pijp-jitter, geen balgwissel-stootjes, geen regulateur-jagen: alles wat periodiek is of zonder oorzaak komt, ervaart het oor als defect.

**Bediening.** In het blok Windmodel staat per groep nu **Karakter**: *Hollands* (spaanbalgen, lange kanalen), *Neutraal* (magazijnbalg met ventilator: alles half zo diep, kort kanaal) of *Eigen* met vier regelaars — Windstoot, Kanaal, Doffer, Verschil per pijp — en de schakelaar "Tongwerken blijven in toonhoogte staan". De drie bestaande schuiven (balggrootte, demping, maximale daling) houden hun betekenis; de knop "Ook op de schuiven" zet ze op de waarden van het karakter. Onder de balgmeter staat nu per lade een balk met de druk van dit moment en een streepje voor de diepste dip van de laatste halve seconde — een schrik van 50 ms valt anders tussen twee schermverversingen — plus wat dat een prestant en een fluit in cent doet. Opgeslagen orgels van 0.7.50 laden met hun eigen balggrootte, demping en daling en krijgen het Hollandse karakter. De standaard maximale daling voor nieuwe groepen is 5 % (was 10 %). Alles in de zeven talen.

**Gemeten** (Friesach, test-API, Hollands karakter, standaardschuiven): één 8'-register met een vierklank: lade 99,99 % — het orgel ademt niet op één register. Het pleno van het Schwellwerk (15 registers, vierklank): balg 97,5 %. Een 32'+16'-C in het pedaal onder een liggend 2'-akkoord, pedaal op de balg van het manuaal: de lade van het manuaal dipt 6,0 % binnen 50 ms en staat na een seconde weer op 100,0 %; loslaten geeft een opstoot tot 101,8 %; de meter houdt de dip vast. Neutraal dipt 2,7 % in dezelfde situatie; model uit is exact vlak. Nergens dieper dan 8 % kortstondig of 5 % statisch — de grens waarboven organisten van "shaky wind" spreken.

**Goedkoper dan voorheen.** Pas 1 van de mengloop stapte tot nu toe elke sample 32 balgen en 32 tremulanten, ook voor divisies die niet bestaan; nu alleen de echte divisies en de actieve balgen. Bij een stil orgel zakte die pas van 0,95 % naar 0,44 % van de buffertijd. Per stem kost de wind nu twee vermenigvuldig-optellingen per frame in plaats van een 64-bits vermenigvuldiging met een tabel van een halve megabyte per blok; de telling van het verbruik zit in de bestaande voorbereidingslus (één HashMap-lookup per stem per callback minder).

**Wat nog volgt** (na de luistertest, zie het plan): de cancel per toets van de sleeplade (een toets met 16'+mixtuur zakt, de buurtoets met alleen 2' niet), C/Cis-laden, en het inlezen van de windvoorziening uit Hauptwerk-orgeldefinities (verbruik per pijp, welke laden één balg delen).

Verificatie: 23 nieuwe unittests op balg, lade en pijpprofielen (kalibratie van de stoot, kwadratische inzakking, familie uit de naam, Hauptwerk-verhoudingen, deterministische spreiding, geen naveren bij een kort kanaal, binnen de perken bij de wildste stapeling) en `testscripts/test_windmodel_0751.py` met vijftien metingen aan de echte audiomotor. 204 vpo-app-tests en 31 vpo-audio-tests groen.

## [0.7.50] - 2026-09-21

### Het windmodel doet nu wat het belooft

Drie klachten over het windmodel, alle drie terecht: het was niet duidelijk wat het deed, je hoorde er nauwelijks iets van, en een gewijzigde windgroep bij een klavier kwam niet terug in het overzicht. Alle drie opgelost.

**Je hoort het nu.** De winddaling stuurde de toonhoogte aan met 30 cent per eenheid druk. Bij de standaardinstelling (10 % daling) was dat 3 cent — precies op de grens van wat een mens kan horen, en dus in de praktijk niets. Dat is nu 120 cent per eenheid, oftewel zo'n 12 cent bij een vol werk, en het volume volgt de druk recht evenredig in plaats van via een wortel. Gemeten met Friesach, tien registers van het Hauptwerk en een akkoord van zeven tonen (70 klinkende pijpen): de druk zakt naar 91,3 %, goed voor 10,4 cent lager en bijna 1 dB zachter.

**En de balg gedraagt zich als een balg.** Onder de motorkap zat een eerste-orde filter, terwijl de regelaar "Demping" beloofde het naschommelen te regelen — een eerste-orde filter kán niet naschommelen, dus die regelaar deed nooit waar hij voor stond. Er staat nu een echte balg: een massa (het gewicht erop) op een veer (de lucht eronder). Zet je een akkoord neer, dan zakt de wind in en veert daarna terug; slap gedempt schommelt hij hoorbaar na, strak gedempt zakt hij alleen rustig in. Daar bovenop een lichte turbulentie die meeschaalt met de daling, zodat een stil orgel ook echt stil staat.

**Je ziet nu dát het werkt.** Onder de aan-uitknop van elke windgroep staat een meter met de winddruk van dit moment, hoeveel pijpen er op die balg staan en hoeveel cent dat scheelt. Speel een akkoord en je ziet de balg zakken. Dat was de enige manier om de vraag "werkt het eigenlijk wel?" fatsoenlijk te beantwoorden.

**En het is uit te leggen.** Het blok heet nu "Windmodel" en begint met drie regels over wat een windmodel is. De regelaars heten "Balggrootte", "Demping" en "Maximale winddaling", elk met een uitleg als je erop blijft staan, en onder de laatste staat wat de gekozen waarde in de praktijk betekent ("bij een vol werk zo'n 12 cent lager en 0,9 dB zachter"). Windgroepen zonder divisies worden niet meer getoond: die konden toch niets laten horen. Alles in de zeven talen.

**De windgroep bij een klavier komt nu meteen terug in het overzicht.** Zette je Hauptwerk op groep 1, dan bleef het overzicht in de instellingen de oude indeling tonen. Oorzaak: het overzicht riep een functie aan om de groep op te zoeken, en Svelte ververst alleen wat letterlijk in het scherm genoemd staat. Nu is de indeling een afgeleide waarde en volgt het overzicht direct. Dezelfde fout zat in de nieuwe drukmeter — die is meteen op dezelfde manier gebouwd. De keuze wordt bovendien 800 ms na de laatste wijziging weggeschreven, zodat hij niet pas bij het afsluiten op schijf belandt.

Verificatie: zeven nieuwe unittests op het model zelf (inzakking hoorbaar maar niet overdreven, verzadiging, naveren bij lage demping, grenzen bij de meest extreme stand, en geen toonhoogtesprong bij het uitzetten) plus een meetscript dat aan de echte audiomotor negen controles doet met een geladen sampleset. 204 vpo-app-tests en 20 vpo-audio-tests groen.

## [0.7.49] - 2026-09-21

### De mengloop gebruikt nu meerdere rekenkernen

Tot deze versie deed JM-Orgue al zijn rekenwerk voor het geluid op één kern, terwijl een moderne pc er acht of meer heeft. Dat was de echte grens achter de polyfonie: het getal in de instelling kon nog zo hoog staan, één kern kwam niet verder.

Gemeten op een i9-10885H (WASAPI, 48 kHz, 480 frames per callback) met Friesach en alle 44 registers getrokken, bij 1.024 gelijktijdig klinkende pijpen:

| Rekenkernen | Belasting van de buffertijd |
|---|---|
| 1 | 188 % — ver over de deadline, hoorbaar haperen |
| 7 | 50 % — ruim binnen de marge |

Dat is **een factor 3,6**. Waar één kern rond de 770 stemmen tegen zijn grens liep, is dat nu ruim het dubbele; de polyfonie-instelling is daarmee eerder bindend dan de rekenkracht.

**Het gaat vanzelf goed.** Bij de eerste start op een pc telt JM-Orgue de fysieke rekenkernen, houdt er één vrij voor het besturingssysteem, de bediening en het laden van samples, en topt af op acht. Op een machine met acht kernen komt daar zeven uit; op een tweekerns laptop één, en dan verandert er niets — wat precies de bedoeling is. Die keuze wordt vastgelegd, zodat hij daarna zichtbaar en te wijzigen is.

**En het is bij te stellen.** In Algemene instellingen → Audio staat nu **"Rekenkernen voor het mengen"**, van 1 (uit, precies het oude gedrag) tot het aantal dat deze pc aankan. Eronder staat hoeveel kernen er gevonden zijn en, tijdens het spelen, welk deel van de rendertijd stemwerk is — dat is het deel dat verdeeld wordt. Met de belastingsmeter ernaast is het effect van een andere keuze meteen te zien.

**Wat er onder de motorkap veranderde.** De stemmen van elk blok worden verdeeld over de audiothread en een vaste pool werkerthreads, elk met een eigen opteltabel; die tabellen worden daarna in vaste volgorde opgeteld. De werkers draaien op dezelfde prioriteit als de audio en zetten zelf denormals uit. Ze wachten eerst spinnend en parkeren daarna, zodat een stil orgel geen kernen laat rondtollen.

> **Eén eerlijke kanttekening.** Met meer dan één kern verandert de volgorde waarin de stemmen bij elkaar worden opgeteld, en drijvende-kommaoptelling is niet associatief. Het geluid verschilt daardoor in de zevende decimaal van dat op één kern — onhoorbaar, maar niet bit-identiek. Bij een gelijk aantal kernen is het resultaat wel reproduceerbaar. Wie bit-gelijkheid met vorige versies wil, zet de instelling op 1.

Verificatie: 204 tests, waaronder vier die met 200 echte stemmen aantonen dat verdeeld mengen hetzelfde geluid geeft (verschil onder een honderdduizendste van het niveau) en vijf voor de pool en de kerntelling. Stresstest: 200 keer de pool herbouwen terwijl 576 stemmen klinken — alle stemmen intact, nul gedropte commando's, en evenveel threads voor als na.

## [0.7.48] - 2026-09-21

### De drie getallen uit de Hauptwerk-tabel

**Polyfonie tot 32.768.** Het plafond stond op 4.096 en gaat naar 32.768, hetzelfde getal dat Hauptwerk Advanced noemt. Maar een plafond is geen belofte, en daarover moet ik eerlijk zijn: wat een pc werkelijk haalt bepaalt de rekentijd, niet dit getal. Gemeten op een i9-10885H (WASAPI, 48 kHz, 480 frames per callback, Friesach met alle 44 registers getrokken):

| Klinkende stemmen | Belasting van de buffertijd |
|---|---|
| 129 | 12 % |
| 260 | 23 % |
| 436 | 39 % |
| 576 | 56 % |
| 716 | 74 % |

Elke stem kost dus ongeveer 0,10 % van de buffertijd; op één rekenkern ligt de grens rond de 770 stemmen (bij 80 % belasting, waarboven onderbrekingen dreigen). De belastingsmeter naast de instelling toont dat live. Het echte plafond is de ene rekenkern waarop de mengloop draait — dáár valt winst te halen, niet in het getal.

> *Nagekomen correctie: de eerste versie van deze tabel noemde 0,17 % per stem en een grens rond de 500 stemmen. Die meting liep terwijl de sampleset nog in de achtergrond inlaadde, wat de belasting opdreef. De tabel hierboven is de herhaalde meting op een volledig ingeladen orgel.*

**Uitgangen tot 1.024.** Het plafond van de galm-weging ging van 64 naar 1.024, en — belangrijker — de kanaalnummers zelf waren een byte, dus kanaal 255 was het hoogste dat een klavier kon aanwijzen. Dat zijn nu 16-bits getallen, van de instelling tot aan de audiothread en het opgeslagen instellingenbestand. Een interface met honderden uitgangen is daarmee volledig te routeren.

**Samplerate kiezen, tot 96 kHz.** JM-Orgue nam altijd klakkeloos over wat het apparaat als standaard opgaf. In de audio-instellingen staat nu een keuzelijst; bij Toepassen wordt die samplerate gevraagd. Kan het apparaat hem niet, dan blijft de huidige staan en komt er een regel in het logboek in plaats van een mislukte stream.

Eén waarschuwing daarbij, uit de meting op deze pc: onder **WASAPI in gedeelde modus** legt Windows de samplerate vast op wat er bij Geluid → Eigenschappen → Geavanceerd staat; een andere waarde vragen kán daar niet en wordt netjes geweigerd. Onder **ASIO** komt de rate uit het paneel van de driver. De keuzelijst helpt dus waar de host het toelaat, en de hint eronder zegt waar je het anders instelt. Intern rekent de engine al sinds het begin in 32-bits drijvende komma.

## [0.7.47] - 2026-09-21

### Zeven punten uit de vergelijking met Hauptwerk en Sweelinq

Na het naast elkaar leggen van de editietabel van Hauptwerk en de release notes van Sweelinq stond er een lijstje met wat zij hebben en wij niet. Op één na is dat lijstje nu leeg.

**Testsignaal per uitgang.** Wie zes of acht kanalen aansluit, weet daarna nog niet welke stekker in welke kast zit. In Algemene instellingen → Audio staat nu een rij kanaalknoppen: klik er een aan en er komt roze ruis (of een sinus van 440 Hz) uit precies dat kanaal. Nog een keer klikken zet hem uit, en hij gaat vanzelf uit zodra je het scherm verlaat.

**Opnamepositie wisselen zonder herladen.** Een positie uitzetten kon altijd al direct — dat weten we nu ook: de samples blijven staan en de positie wordt stilgezet. Zet je "alle posities in het geheugen houden" aan, dan staan ook de uitgeschakelde posities klaar en is áánzetten net zo ogenblikkelijk. Dat kost geheugen, ruwweg maal het aantal posities, dus het is een bewuste keuze per orgel. Tot nu toe vroeg elke wijziging om een volledige herlaad.

**Kort octaaf.** Historische klaviaturen hebben een onvolledige onderste octaaf: de toets die eruitziet als E klinkt als C, Fis als D en Gis als E, en Cis en Dis ontbreken daar. Per klavier aan te zetten, zodat oude muziek met de linkerhand op de juiste toetsen valt. Werkt pas als het toetsbereik is ingeleerd — zonder laagste toets weet de app niet waar de onderste octaaf begint, en dan doet het vinkje niets in plaats van iets verkeerds.

**Doorlopend klavier.** Toetsen buiten het opgenomen bereik van een register zwijgen niet meer, maar spelen de pijp een octaaf hoger of lager. Een klavier van 61 toetsen speelt daarmee door op een sampleset van 56 tonen, zoals een orgel met een herhalende bovenoctaaf. De toonhoogte klopt op het octaaf, niet op de halve toon — maar er valt geen gat waar niets klinkt.

**Koppels erbij.** Een sampleset die zelf koppels meebrengt onderdrukte tot nu toe de hele afgeleide lijst: had het orgel geen sub- of superoctaafkoppel, dan was die ook niet te krijgen. Onder de koppels staan nu de koppels die de app uit de klavierindeling kan afleiden maar die in de set ontbreken. Aanvinken en ze staan meteen op de koppelbalk, zonder herlaad. De actiecodes van de bestaande koppels blijven ongemoeid, dus ingeleerde MIDI-knoppen verspringen niet.

**System Exclusive als schakelaar.** Speeltafels die hun registers en pistons via SysEx melden (Hauptwerk-protocol, Johannus) kunnen daar nu ook mee schakelen. De MIDI-invoer gooit SysEx niet langer weg — dat deed de driver standaard wel — en het inleren pakt zo'n bericht net zo op als een noot of een CC. In het instellingenbestand staat de inhoud leesbaar als hex, en met de hand in te typen kan ook.

**Control change als bitveld.** Sommige speeltafels stoppen acht schakelaars in één CC-waarde, elk in een eigen bit. Zo'n bit is nu als schakelaar te kiezen; hij vuurt op de overgang van 0 naar 1.

### Verder

**Luisterprofielen voor de EQ.** Drie knoppen die de banden in één klik vullen: neutraal, koptelefoon (de 16' dreunt op een koptelefoon eerder dan in een kerk) en kleine luidsprekers (onder de 60 Hz kunnen die toch niets, en die energie kost alleen vervorming). Bewust geen correcties per koptelefoonmodel: die zouden gemeten moeten zijn, en dat verzinnen we niet.

### Wat bewust niet is gebouwd

De **VST/AU-plug-in** uit Hauptwerk Advanced. Dat is een andere productvorm — JM-Orgue als instrument in een opnameprogramma — en geen ontbrekende knop. Pas overwegen als iemand er echt om vraagt.

## [0.7.46] - 2026-09-21

### Nagalm: niet meer verpest door een preset

De klacht was dat je de klank van een natte sampleset met de galmpresets kunt verpesten. Twee dingen daaraan:

- **Een knop "Terug naar de opname"** in het nagalmblok zet alle kunstmatige galm in één klik af. Een preset die je klank troebel maakt is daarmee geen zoekplaatje meer, ook niet als je hem al bij dit orgel had opgeslagen.
- **Een regel erboven die zegt dát deze set zijn eigen akoestiek meebrengt**, met het aantal pijpen waarvoor dat geldt. Extra galm is dan smaak, geen noodzaak — en dat staat er nu bij in plaats van dat je het moet weten.

Daaronder is de maatstaf veranderd. JM-Orgue besloot tot nu toe aan de **bestandsnaam** of een set nat was: een `.organ` of een Hauptwerk-ODF kreeg geen standaardgalm, al het andere wel. Dat is nu een feit uit de set zelf: **heeft hij release-opnamen?** Een release is de uitklank van de pijp ín de ruimte, dus staat die erin, dan is de akoestiek al opgenomen. Voor de gangbare sets verandert er niets (drie GrandOrgue-sets in de testbibliotheek leveren 280, 295 en 2.392 pijpen met release-opname en starten net als voorheen droog); wat wel verandert is dat een échte droge ODF-set nu de normale standaardgalm krijgt in plaats van niets, en dat het oordeel niet langer aan een bestandsextensie hangt.

### Tremulant: te zien waar hij vandaan komt

Per klavier staat er nu bij of de tremulant **uit de opname** komt of **nagebootst** wordt. Dat was tot nu toe onzichtbaar, en daarmee was "waarom hoor ik de nagebootste?" een raadsel. Het antwoord is bijna altijd dat de sampleset domweg geen tremulant-opnamen bevat: de meeste GrandOrgue-sets zetten `TremulantType=Synth` en leveren alleen droge pijpopnamen. GrandOrgue zelf bootst de tremulant daar ook na. Heeft een set ze wél (Hauptwerk-"tremmed"-lagen, GrandOrgue `IsTremulant=1`, eigen `_trem`-mappen), dan worden die gespeeld en zwijgt de nabootsing — dat werkte al sinds 0.7.39 en is nu ook te zien.

## [0.7.45] - 2026-09-16

### Treden die ruisen: demping op zwelkast, crescendo en inleren

Een meting op het testorgel met `jm-midimon` bracht aan het licht wat de treden daar werkelijk sturen. Alle drie de zweltreden gebruiken CC 7, elk op een eigen kanaal (6, 7 en 8), dus ze zijn prima uit elkaar te houden. Maar het signaal zelf is ruw: in rust wiebelt een trede eindeloos twee eenheden op en neer, en tijdens het trappen zitten er losse uitschieters van 30 tot 70 eenheden tussen. Bewegen er twee treden tegelijk, dan is bijna een derde van de berichten onbruikbaar — de speeltafel leest zijn treden om beurten uit met één meetschakeling en neemt de tijd niet om tussen de kanalen te settelen, zodat de ene trede de waarde van de andere meekrijgt.

JM-Orgue nam die waarden tot nu toe ongefilterd over. Daardoor:

- **trilde de zwelkast mee** met de rustwiebel, en ging er bij elke wiebel een commando naar de audiothread;
- **sprong het generaal crescendo dwars door zijn hysterese heen**: registers die aan- en uitsprongen terwijl de organist rustig trapte;
- **kwam het inleren nooit tot rust.** Een trede die blijft wiebelen "beweegt" volgens de inleerkiezer altijd, dus wachtte elke inleerbeurt de volle vijftien seconden uit om daarna op de terugval uit te komen. En een uitschieter op het verkeerde moment zette de ingeleerde laagste of hoogste stand tientallen eenheden mis.

Elke ingeleerde trede gaat nu door een demping in drie stappen: een mediaan over de laatste vijf waarden (haalt losse uitschieters weg), een snelheidsbegrenzing van één eenheid per milliseconde (een voet haalt de volle pedaalweg in ruim honderd milliseconde, dus echt spel wordt nooit geremd — een sprong van 70 eenheden in 5 ms wel), en een rustband van twee eenheden zodat de wiebel geen commando's meer stuurt. De uiterste standen 0 en 127 komen altijd door, en na een stilte geldt een nieuwe waarde meteen: het afspelen van een midibestand en de test-API blijven exact. Pistons op een CC blijven ongefilterd, zodat de flankdetectie geen korte druk mist.

Op de gemeten reeksen daalt de gemiddelde sprong tussen twee doorgegeven standen van 17 naar 2 eenheden in het ergste geval, en van 9 naar 2 bij een gewone beweging.

### Opgemerkt bij de meting, niet op te lossen in software

Trede 1 van het testorgel levert over zijn volle weg maar 25 van de 127 eenheden (twee metingen, 2..27 en 3..29). Dat wijst op de mechaniek of de potmeter van die trede, niet op de software; met zo weinig bereik blijft een zwelkast grof en komt een crescendo niet boven de onderste trappen uit.

## [0.7.44] - 2026-09-15

### Zweltreden en generaal crescendo: inleren, verdringen en terugveren

Na de test op het testorgel ("zwelpedaal en generaal crescendo werken nog niet naar behoren") is het hele pedaalpad doorgelicht. De gevonden oorzaken en reparaties:

- **Inleren koos "de laatst geziene CC".** Een tweede trede die in rust een beetje ruist, of een speeltafel die bij elke beweging álle tredewaarden opnieuw stuurt, won daardoor van de trede die je bewoog: je leerde de verkeerde trede in, of het inleren duurde 20 s. Nu wint de trede met de grootste slag die tot rust is gekomen; ruis en stilstaande treden tellen niet mee. Een 14-bits fijnregel-CC wordt per kanaal genegeerd.
- **Een verdrongen of gewiste zwelkoppeling liet de kast op de laatste pedaalstand staan** (gedempt, met laagdoorlaatfilter) zonder trede om hem nog open te zetten. Leerde je het crescendo in op een trede die als zwelkast bekend was, dan bleef die divisie zacht: "als het één werkt, werkt het ander niet". De kast gaat nu open zodra haar koppeling verdwijnt.
- **Een audiowissel of herlaad maakte de door de trede getrokken registers "handmatig"**: terugveren nam ze daarna niet meer weg. De trede-registers overleven de herlaad nu als claims van de trede en veren gewoon terug.
- **Crescendo-koppeling wissen** liet de trede-registers staan; hij zet nu eerst trap 0.
- **Na een setzer-oproep of General Cancel** bleef de crescendotrap op N staan terwijl de trede-registers handregistratie waren geworden: de trede terugnemen trok dan de registers van de lagere trap bíj ("trede omlaag, geluid harder"). Het crescendo begint nu opnieuw vanaf de bodem: de trede telt pas weer mee nadat hij helemaal terug (trap 0) is geweest. Geldt ook als de matrix koppels bevat.
- **Handmatig een andere trede invullen** erfde bereik en spiegelbeeld van de vorige; een nieuwe (kanaal, CC) begint op 0..127 zonder spiegel.
- **Spiegelbeeld en bereik van de zwel** zijn meteen hoorbaar, niet pas bij de volgende pedaalbeweging.
- **Dode zone van het crescendo** van 4 naar 8: een trede die in rust een paar eenheden boven de ingeleerde laagste stand blijft hangen hield trap 1 vast.
- **Zwelstand die bij een volle audiowachtrij niet weg kon** verdween stil; hij wordt nu bij de volgende ronde alsnog gestuurd.
- **Zwelvinkje uit** wordt nu ook opgeslagen; de gewiste koppeling kwam bij de volgende load terug.
- **MIDI-diagnose in het log:** elke binnenkomende CC wordt (hooguit vier keer per seconde per trede) gelogd met zijn interpretatie: "zwelkast Nevenwerk 50 %", "generaal crescendo" of "geen zwel-/crescendokoppeling". Zo laat het logbestand van het testorgel zien wat elke trede doet.
- **Nog vier bevestigingsvragen** (sampleset knippen, loops aanmaken, minder crescendostappen, nieuw token) wachtten niet op het antwoord; nu wel.

### Nieuw: jm-midimon, MIDI-monitor voor de speeltafel

Los consoleprogramma (`jm-midimon.exe`, als bijlage bij de release) dat alle MIDI-ingangen opent, elk bericht ruw toont en logt, en je stap voor stap door "trede 1 open→dicht, trede 2, trede 3, alle drie" leidt. Per stap: welk kanaal en CC-nummer bewoog, bereik, richting, sprongen, ruis, overspraak van andere treden, 14-bits paren en NRPN. Aan het eind een conclusie of JM-Orgue de treden uit elkaar kan houden. Sluit JM-Orgue eerst af (Windows laat één programma per MIDI-poort luisteren).

## [0.7.43] - 2026-09-15

### Geen zelfherstart meer: "stoort en valt terug naar de bibliotheek" opgelost

Op het testorgel viel 0.7.40 tijdens het spelen steeds terug naar de bibliotheek. De oorzaak zat niet in de bibliotheek zelf: de app herstartte zichzélf. Kon de ASIO-driver in het lopende proces niet opnieuw starten (na een wissel naar WASAPI op hetzelfde apparaat, of nadat de stream was stilgevallen en de bewaking hem niet meer aan de praat kreeg), dan deed de audio-wissel sinds 0.7.31 een volledige herstart van het programma. Elke herstart begint op het startscherm, laadt het orgel opnieuw en schakelt daarna weer naar ASIO. De beveiliging tegen een herhaling werd bij elke start gewist, waardoor het zich kon blijven herhalen.

- De app herstart nooit meer uit zichzelf. Kan ASIO in deze sessie niet meer starten, dan blijft de lopende uitgang gewoon spelen (bij een bewuste wissel) of valt het geluid terug op de standaarduitgang (bij een stilgevallen stream). Het orgel blijft op het scherm staan.
- In beide gevallen verschijnt een balk: "De ASIO-driver kan in deze sessie niet opnieuw starten … Herstart JM-Orgue om ASIO te herstellen", met een knop die de herstart uitvoert nadat instellingen en registratie zijn bewaard. Jij beslist wanneer.
- Met een ASIO-voorkeur werd het laatste orgel bij de start twéé keer geladen: eerst op de standaarduitgang en tien seconden later nog eens op de verse ASIO-thread, midden in het spel. De ASIO-wissel start nu zodra het scherm er is en het orgel wordt pas daarna geladen, één keer. Het startscherm meldt intussen "Audio-uitgang (ASIO) wordt gestart…".
- Het bufferadvies ("Zet de buffer op 128") kijkt nu naar de werkelijke framegrootte van de audio-callback, ook wanneer de driver zijn eigen paneelinstelling aanhoudt; wegklikken geldt tien minuten in plaats van de hele sessie.
- Werd een orgel buiten het scherm om geladen (of het scherm herladen terwijl het orgel bleef staan), dan bleef de bibliotheek in beeld met een spelend orgel erachter. De weergave volgt nu het geladen orgel.
- De status (en de test-API) melden voortaan `audio_ready`, `asio_restart_advice`, `backend_reloads` en `render_frames`; het logbestand markeert elke interne herlaad met zijn reden. Zo is een volgend testorgel-log zonder gissen te lezen.
- Geen enkele bevestigingsvraag werkte: de dialoogbibliotheek van Tauri vervangt `window.confirm` door een asynchrone variant, waardoor "Doorgaan?" bij bijwerken, bij **Afsluiten** (computer uitzetten, ook vanaf een extra scherm) en bij het verwijderen van een MIDI-archiefbestand als "ja" gold zonder dat er een vraag verscheen. De vijf vragen wachten nu op het antwoord; Annuleren annuleert.

## [0.7.42] - 2026-09-15

### Oude instellingen netjes overnemen

- Wie in 0.7.38 dezelfde trede als zwelkast én als generaal crescendo had gekoppeld, kreeg die beide koppelingen bij het laden terug, waarna de zwelkast stil dood was. Bij het laden geldt nu dezelfde regel als bij het inleren: de crescendokoppeling wint, de zwelkoppeling op dezelfde trede wordt weggelaten en het logbestand meldt dat.
- Stond de nagebootste tremulant aan op een klavier dat inmiddels echte tremulant-opnamen heeft, dan klonken opname en nabootsing over elkaar. De opgeslagen voorkeur wordt op zo'n klavier niet meer toegepast; het logbestand meldt dat één keer.
- De controletaak in de bouwstraat draait nu ook de tests van de audiobibliotheek en de kern.

## [0.7.41] - 2026-09-15

### Nazorg na twee controlerondes

Dit is de eerste versie die je via "Nu bijwerken" binnenhaalt. Ze bundelt wat twee onafhankelijke controlerondes over 0.7.39 en 0.7.40 aan het licht brachten.

**Klank**
- Boven de geheugenkap gaf een eindigende stem haar samples terug aan het systeem ín het audioblok, bij het loslaten van een akkoord tot enkele milliseconden in één keer. Dat gebeurt nu op de opruimthread.
- De convolutiegalm rekende op elke partitiegrens alles in één blok uit, wat bij zeer kleine buffers hoorbaar tikte zodra je een eigen impulsrespons gebruikte. Het werk is nu gelijkmatig over de blokken verdeeld.
- Het tremulant-commando telde niet mee in het werkbudget van het audioblok en duwde zijn stemmen erin zonder de staartbegrenzing; dat is rechtgezet, net als de boekhouding bij het loslaten van een register.

**MIDI**
- Knoppen die per druk alleen een puls van 127 sturen (voetcontrollers in CC-modus, eigen bouw) vuurden na 0.7.39 nog maar één keer per sessie. Ze werken weer bij elke druk, terwijl pedaalsweeps niet per ongeluk een knop indrukken. Een op waarde 0 ingeleerde knop vuurt bij het indrukken in plaats van bij het loslaten.

**Bijwerken**
- Wie met het msi-bestand installeerde, krijgt voortaan ook een msi-update en niet een tweede installatie ernaast.
- "Controleer op updates" meldt nu eerlijk wanneer de controle mislukt, bijvoorbeeld zonder internet, in plaats van "je gebruikt de nieuwste versie".
- Een vastgelopen download eindigt met een nette melding; twee snelle klikken starten hem niet dubbel.
- De bouwstraat controleert elke handtekening tegen de publieke sleutel voordat een update wordt aangeboden, stopt bij een pre-release, en publiceert niets meer als een bouw wordt geannuleerd.

**Interface en bibliotheek**
- Zeven taalknoppen passen ook op smalle vensters; een bevestigingsvraag toonde letterlijk een plaatshouder; de pagina voor tablet en telefoon meldt de juiste taal en zegt in het Duits "Keine Orgel"; in het Spaans heet het logbestand niet langer "archivo de registro".
- Een orgel op een losgekoppelde schijf wordt niet meer voorgoed als "geen foto" onthouden, ook niet bij Hauptwerk-sets waarvan alleen de pakketten elders staan.
- De bibliotheek wacht bij het openen niet meer op het opfrissen van namen op de achtergrond.

**Bouwstraat**
- Bij elke push draait nu een controletaak met de frontend-build, de vertaalcontrole en alle tests. Tot 0.7.40 draaide er in de bouwstraat geen enkele test.

## [0.7.40] - 2026-09-14

### Bijwerken met één klik, en drie talen erbij

**Bijwerken**
- **Een nieuwe versie installeert zichzelf.** Meldt de app een update, dan haalt de knop "Nu bijwerken" het installatiebestand op, laat de voortgang zien, installeert het en start JM-Orgue opnieuw. De downloadpagina blijft als alternatief bestaan.
- Het pakket wordt ondertekend en de app controleert die handtekening vóór het installeren. Een gedownload bestand dat niet bij de sleutel van JM-Orgue hoort, wordt geweigerd.
- Zoals afgesproken kijkt de app alleen bij het starten en via de knop "Controleer op updates" of er een nieuwe versie is. Er draait geen controle op de achtergrond.
- Let op: het bijwerken werkt vanaf deze versie. Van 0.7.39 naar 0.7.40 gaat nog met de hand, daarna niet meer.

**Talen**
- **Pools, Italiaans en Spaans** erbij. Daarmee spreekt JM-Orgue zeven talen: Nederlands, Engels, Frans, Duits, Pools, Italiaans en Spaans.
- De hele interface is vertaald, inclusief de namen van de 53 stemmingen en de pagina voor tablet of telefoon. Die pagina volgt de taal van het apparaat waarop je hem opent.
- Ontbreekt er ooit een tekst in een taal, dan verschijnt voortaan het Engels in plaats van het Nederlands.

## [0.7.39] - 2026-09-14

### Negen punten van het testorgel: tremulant-opnamen, galm bij korte noten, zweltreden en crescendo, afstandsbediening, startscherm, JM-Rec-namen en orgelfoto's

**Klank**
- **Tremulant met opgenomen samples (GrandOrgue-golfvormtremulant).** Sets die per pijp een aparte tremulant-opname leveren (GrandOrgue `TremulantType=Wave` met `IsTremulant`-attacks en -releases, Hauptwerk-sets met een "tremmed" laag, eigen sets met `_trem`-mappen) krijgen nu een echte tremulantknop: de tremulant-opnamen spelen, inclusief de bijbehorende release, en bij het in- of uitschakelen klinkt een nette overgang op klinkende noten. Voorheen bleef de knop voor zulke sets weg en klonk hooguit de nagebootste tremulant.
- **Galm bij korte noten en het slotakkoord (GrandOrgue-sets).** Drie oorzaken verholpen: de zojuist gestarte nagalm van een losgelaten toets kon door de stemmenbegrenzer meteen weer worden weggehaald (bij een akkoord bleef dan alleen de laatste noot over), korte noten kregen hun nagalm ineens op vol niveau in plaats van vloeiend, en de verkorte releases van toetsduur-varianten werden niet geschaald zoals GrandOrgue dat doet. Staccato gevolgd door een slotakkoord klinkt daardoor weer als in GrandOrgue.

**Zweltreden en generaal crescendo**
- Eén pedaal kan niet langer tegelijk als zwelkast én als generaal crescendo gekoppeld staan. Wie een pedaal opnieuw koppelt, ziet nu welke koppeling daarmee vervalt, en beide vakken worden meteen bijgewerkt. Voorheen kon je twee koppelingen op dezelfde CC maken waarbij er stilletjes maar één werkte.
- Waarschuwing als het crescendopedaal wel gekoppeld is maar het crescendo uit staat of geen trappen heeft.
- **Reactie op pedalen en toetsen tot dertig keer sneller.** De MIDI-lus wachtte per ronde op een Windows-timer die in stappen van ruim vijftien milliseconde tikt, waardoor elke pedaalstand, noot en piston tot 31 ms bleef liggen en een pedaalbeweging in brokjes binnenkwam. De lus wordt nu gewekt door het bericht zelf: gemeten vertraging ging van gemiddeld 29 ms naar ongeveer 1 ms. Dit was de belangrijkste oorzaak van het haperen van zweltrede en crescendo, en het maakt ook het spelen strakker.
- Haperen bij trapwissels: het werk per audio-blok schaalt nu mee met de buffergrootte, zodat een trapwissel met veel registers niet meer in één blok wordt afgehandeld. De statusbalk telt zware blokken en gemiste opdrachten, en bij herhaalde overbelasting met een zeer kleine buffer stelt de app voor de buffer op 128 te zetten.
- Een als knop ingeleerde bediening op een doorlopende regelaar reageert nu alleen op de aanslag. Wie ooit een setzerknop op de CC van een zweltrede leerde, zag bij elke pedaalbeweging registraties omklappen.

**Bediening**
- **Afstandsbediening volgt het orgelscherm.** Tablet of telefoon toont dezelfde indeling als het registreerscherm: dezelfde divisievolgorde, dezelfde registervolgorde, dezelfde knopvorm en alleen de koppels die je zelf zichtbaar hebt gemaakt. Nieuw zijn keuzevinkjes onder Algemene instellingen → Afstandsbediening voor wat er op het externe scherm verschijnt (divisies, koppels, tremulant, setzer, volume, paniekknop). De pagina volgt wijzigingen zonder herladen.

**Starten en bibliotheek**
- Opstartscherm in JM-Orgue-stijl overbrugt de eerste seconden, zodat de bibliotheek en een eventuele updatemelding samen verschijnen in plaats van na elkaar.
- Taalknoppen (NL EN FR DE) op het startscherm.
- Zonder geladen orgel kun je vanuit de instellingen weer terug naar de bibliotheek.
- **JM-Rec-sets heten nu zoals je ze hebt ingevoerd**: kerknaam - orgelbouwer - plaats, in plaats van de afkorting van de projectmap. Bouwer en plaats staan onder de naam op de bibliotheekkaart.
- **Orgelfoto's**: de app zoekt breder (ook de afbeelding uit de GrandOrgue-definitie en de afbeeldingen van een Hauptwerk-pakket) en je kunt op elke bibliotheekkaart zelf een foto kiezen of terugzetten op automatisch.

## [0.7.38] - 2026-09-12

### Gestapelde ranks en perspectieven, hertemperen, afstandsbediening, MIDI-archief, crescendo en zweltreden

**Klank**
- **Gestapelde ranks en microfoonperspectieven.** Registers met meerdere ranks per toets (mixturen/cornetten als aparte ranks in GrandOrgue-ODF's, multi-mic-sets van Hauptwerk, JM-Rec-sets met meerdere microfoonmappen) klinken nu volledig: per toets één stem per rank. Perspectieven (front/rear/dry, of eigen mapnamen) zijn per orgel aan/uit te zetten met eigen volume onder Orgel-Instellingen → Perspectieven; alleen ingeschakelde perspectieven worden geladen (RAM).
- **Hertemperen op gemeten pijptoonhoogte.** Nieuwe standaardstemming "Origineel (zoals opgenomen)": de set klinkt zoals de maker hem afleverde. Kies je een ander temperament, dan wordt — zoals in GrandOrgue en Hauptwerk — per pijp de gemeten afwijking (PitchCorrection, Hauptwerk-pitchvelden, smpl-metadata) verrekend, zodat ook "detuned" geleverde sets zuiver in bijvoorbeeld gelijkzwevend a=440 of middentoon komen te staan. Bestaande opgeslagen stemmingen blijven klinken zoals voorheen (migratie naar "Origineel").

**Generaal crescendo en zweltreden**
- Hysterese op de trapgrenzen: registers klapperen niet meer bij pedaalruis.
- Klikken op een trap in de editor is nu net zo additief als het pedaal: handmatig getrokken registers en echte ODF-koppels blijven staan; terugtreden haalt alleen weg wat het crescendo zelf bijtrok; crescendo uitschakelen ruimt op; een handmatig teruggeduwd register wordt niet meer "geclaimd".
- Lege hogere trappen erven de dichtstbijzijnde gevulde trap; de matrix, aan/uit en het aantal trappen worden per orgel in de backend bewaard (niet meer alleen in het venster).
- Zwelkast: pedaalstand wordt na herladen of audio-wissel teruggezet (geen sprong bij de eerste beweging), het klankfilter wordt bij een orgelwissel gereset (geen doffe divisie meer) en de zwelgain loopt gesmootheerd (geen trapjes of zipper bij treden die in stappen sturen).
- Inleren van treden: annuleren/opnieuw breekt de vorige leerlus af, een te klein bereik wordt geweigerd, het zwelkast-vinkje uitzetten wist de koppeling niet meer; wijzigingen aan bereik/inversie worden bewaard; crescendo-CC werkt nu ook bij het afspelen van MIDI-opnamen.

**Bediening**
- **Afstandsbediening in het netwerk** (Algemene instellingen → Afstandsbediening): schakel in, scan de QR-code of open de link op een tablet of telefoon in hetzelfde netwerk, en bedien registers, koppels, tremulant, setzer en volume zonder MIDI-hardware. Beveiligd met een token; standaard uit. (Windows vraagt bij de eerste keer om firewall-toestemming.)
- **Automatisch MIDI-archief** (Algemene instellingen → Algemeen): alles wat je speelt wordt op de achtergrond als MIDI-bestand bewaard (start bij de eerste noot, stopt na instelbare stilte) in Documenten/JM-Orgue-opnames/MIDI-archief, met een lijst van recente opnamen om af te spelen of te verwijderen. Standaard uit.

**Stabiliteit**
- De app logde altijd ook naar stderr; werd hij gestart door een programma dat die uitvoer niet leest (bijv. een testtool of MCP-server), dan bevroor de hele bediening na ±80 KB log terwijl het geluid doorspeelde. Logging naar stderr gebeurt nu alleen op een echte terminal; het logbestand is leidend.

**Test-API**: `/temperament`, `/tuning`, `/perspectives`, `/ranks`, `/midi/archive/*`, `/midi/player/*`, `/remote/*`, `/crescendo*`, `/swell*`, `/midi/inject?cc=`, `/midi/mapping`.

## [0.7.3] t/m [0.7.37] - 2026-08-07 t/m 2026-09-12

Deze versies zijn niet in dit bestand bijgehouden; de volledige notities staan bij de
GitHub-releases (https://github.com/orgelmaker/JM-Orgue/releases). Hoofdpunten: hoofdbalk
op extra schermen en live notatie (0.7.0–0.7.2), MIDI-uit-terugkoppeling, consoleknoppen,
bibliotheek met sampleset-downloads (0.7.32), galm volgt kanalen en release-crossfade (0.7.33),
echte stereo, instelbare polyfonie, koppelbalk en JM-Rec-import (0.7.36), alle uitgangen van
de geluidskaart bij de klavieren en volledige vertaling NL/EN/FR/DE (0.7.37).

## [0.7.2] - 2026-08-07

### Verbeterd — één "Noteren"-knop met "Openen…" binnen het venster

- 🎼 De aparte "Live noteren"-knop is verdwenen: **"Noteren"** opent nu direct het notatievenster (live-modus). Wil je een bestaand MIDI-bestand inladen? Dat gaat via de knop **"Openen…"** in het venster zelf — het bestand komt binnen als extra take per notenbalk (zodat je bestaande opnames niet kwijtraakt), en vervolgens kun je live verder opnemen bovenop wat er al staat

## [0.7.1] - 2026-08-07

### Opgelost — koppel-toets "ontkoppelt" bij dubbele aanslag

- 🎹 **Zelfde pijp via koppel + direct aangeslagen valt niet meer stil bij loslaten van één bron.** Voorbeeld: Pedaal→Hoofdwerk-koppel actief, Pedaal-C ingedrukt, óók HW-C aangeslagen, HW-C weer los → de HW-pijp bleef eerst kort stil, ondanks dat de pedaal-C nog vasthield. De audio-engine deelt nu één "luchtkolom" per pijp met een tellertje: elke bron (directe klavieraanslag, koppel-route, laterale koppel) telt mee bij aanslag, de release komt pas als alle bronnen hebben losgelaten. Extra winst: geen dubbele amplitude / lichte comb-filter meer wanneer dezelfde pijp via twee routes klinkt

### Nieuw — live noteren met lagen en bewerken

- 🎼 **Nieuwe knop "Live noteren"** naast "Noteren": opent een notatievenster met een verse partituur en een rode opname-knop. Zodra je op Opname klikt en gaat spelen, verschijnen de noten meteen op de bladmuziek (per orgeldivisie een eigen notenbalk, Pedaal in de bassleutel). De maat start bij de eerste toets — geen count-in nodig
- 📚 **Lagen met overdub (meerdere takes per balk)**: elke notenbalk heeft een armed-bolletje; klik → daar wordt opgenomen. Onder elke balk staan de takes met vinkjes; nieuwe opname = nieuwe take. Meerdere zichtbare takes op één balk = overdub (samensmelting op tijd). Nieuwe balken en takes bij te maken met "+ Balk" / "+"
- 🎚️ **Ritme-schuif "los ↔ strak"** (fouttolerantie): schuif tijdens of na de opname; strak = alles op het raster, los = kleine timing-afwijkingen krijgen een fijner sub-raster en blijven zichtbaar
- ✂️ **Bewerken vanuit de toolbar of het toetsenbord**: pijl ←/→ selecteert de vorige/volgende noot; ↑/↓ transponeert een halve toon; Shift+↑/↓ een heel octaaf; Delete verwijdert de selectie. **Undo/redo** (Ctrl+Z / Ctrl+Y) op alle bewerkacties + tempo/toonsoort/tolerantie-wijzigingen

### Bekende beperkingen (bewust in scope gehouden voor deze sessie)

- Selectie is nu cursor-gebaseerd (pijltjestoetsen); klikken direct op een noot in de bladmuziek en muisselectie volgen in een latere versie
- Één stem per notenbalk (meerdere overlappende takes worden ingekort tot de volgende inzet); polyfone stemmen op één balk komen later
- Copy/paste + duurwijziging (kwart ↔ achtste ↔ punt) staan op het groeitraject
- Klik-op-noot om te selecteren, drag-drop, dynamiek en tekstlabels zijn expliciet fase 2+

## [0.7.0] - 2026-08-06

### Nieuw — volledige hoofdbalk op extra schermen

- 🪟 **Extra registerschermen kunnen nu de volledige hoofdbalk tonen**: orgelnaam, de tabbladen Orgel / Orgel-instellingen / Algemene instellingen (die werken écht in het extra venster), de profielwissel speakers↔hoofdtelefoon en audio start/stop. Aan/uit te zetten per scherm met de knop "Balk" in de werkbalk (en het pijltje in de balk zelf om te verbergen); de keuze wordt per scherm en per orgel onthouden. Standaard staat de balk uit — het vertrouwde compacte scherm blijft de start
- 🔁 De extra schermen zijn onder de motorkap nu volwaardige consoles: dezelfde registerweergave als het hoofdscherm (inclusief knop-verslepen en MIDI-inleren via rechtermuisklik), met eigen divisiekeuze en indeling per scherm. Acties die het hele programma aangaan (orgel laden/sluiten, profielwissel, nieuw scherm, afsluiten) lopen netjes via het hoofdvenster
- 🎛️ Wijzigingen in knopgrootte, knopvorm, zwelkast-/koppelzichtbaarheid en registervolgorde volgen nu binnen een seconde in álle vensters (ook het hoofdvenster volgt de extra schermen)

### Nieuw — muziek noteren (bladmuziek uit je eigen spel)

- 🎼 **Nieuwe knop "Noteren"** naast de opnameknoppen: je MIDI-opname wordt omgezet naar notenschrift en getoond in een eigen notatievenster — volledig in de app, geen extern programma nodig. Elke divisie krijgt een eigen notenbalk (Pedaal in de bassleutel), gelijktijdige noten worden akkoorden, rusten en overbindingen worden ingevuld
- 🖨️ Vanuit het notatievenster: **afdrukken** (kies "Microsoft Print to PDF" voor een PDF-bestand) en **opslaan als MusicXML** (te openen in elk notatieprogramma). Tempo, maatsoort, toonsoort, kwantisatieraster en titel zijn achteraf bij te stellen; de weergave volgt direct
- 🎚️ **Vrij instelbare balk-indeling**: per notenbalk vink je aan welke divisies erop komen (bv. Hoofdwerk + Nevenwerk samen op één balk, Pedaal apart), met eigen naam en bassleutel-keuze per balk; balken zijn toe te voegen en te verwijderen
- ℹ️ Eerlijke verwachting: vrij ingespeeld werk kwantiseert het mooist wanneer je in een vast tempo speelde; kies anders een grover raster. Bewerken op nootniveau volgt in een latere versie (onderzocht: een inbouwbare open-source editor à la MuseScore bestaat niet; weergave draait op OpenSheetMusicDisplay, BSD-licentie)

### Bekende beperkingen (bewust geaccepteerd)

- Instellingen-schuiven die tegelijk in twee vensters zichtbaar zijn synchroniseren niet real-time; het extra scherm ververst bij het openen van het tabblad
- Een profielwissel die de audio opnieuw opbouwt sluit en heropent de extra schermen (bestaand gedrag)
- Notatie toont één stem per notenbalk: overlappend spel wordt ingekort tot de volgende inzet

## [0.6.14] - 2026-07-29

### Opgelost — audio-wissel (speakers ↔ hoofdtelefoon) liep vast

- 🔒 **Permanente deadlock gedicht**: de uitgestelde ASIO-wissel en het audio-noodherstel namen de load/wissel-vergrendeling en riepen daarbinnen het orgel-herladen aan, dat dezélfde vergrendeling opnieuw nam — die staat dan voorgoed op slot, waarna elke audio-wissel bleef hangen en de eerstvolgende orgel-load of autosave de hele app bevroor
- 🧊 **UI bevriest niet meer tijdens laden/wisselen**: orgel laden, samples-map laden en het automatische instellingen-opslaan draaien nu op een achtergrond-thread (zoals de audio-wissel zelf al deed) in plaats van op de venster-thread — de voortgangsbalk loopt door en de vensters blijven reageren
- 🔁 **Status-poll kon de wissel doodhouden**: de 100ms-statuspoll nam dezelfde lees-vergrendeling twee keer binnen één expressie; zodra de wissel stond te wachten om te schrijven was dat een blijvende blokkade van de UI-thread — herschreven naar één vergrendeling

### Opgelost — hangers bij de generaal crescendo

- 🎹 **Koppels in crescendotrappen lieten stemmen wees worden**: ging een koppel uit terwijl toetsen ingedrukt waren, dan kregen de gekoppelde stemmen nooit meer een note-off (note-off rekent met de níeuwe koppelstand) — permanente hangers. Koppelwissels (crescendo, presets, koppelknoppen) synchroniseren nu de stemmen: weggevallen routes zwijgen, bijgekomen routes klinken direct mee
- 👻 **Spooktoetsen opgeruimd**: de lijst "ingedrukte toetsen" werd nooit gewist bij panic/all-notes-off, orgelwissel of het stoppen van MIDI-afspelen; één achtergebleven toets wekte bij elke crescendotrap opnieuw stemmen tot leven die nooit meer stopten. Alle panic-/wissel-routes wissen de lijst nu mee
- 🦶 **Pedaal-sweeps overbelastten de MIDI-verwerking niet meer**: bij een snelle crescendobeweging telt per verwerkingsronde alleen de laatste pedaalstand (tussentrappen worden overgeslagen) — voorheen kon de stroom trapwissel-commando's de MIDI-thread seconden blokkeren waardoor binnenkomende note-offs verloren gingen (ook een bron van hangers)
- 🎚️ **Trap-/preset-wissel kon een register laten doorklinken**: de vergelijking "wat is er gewijzigd" keek alleen naar de UI-spiegel; kruiste de crescendotrede het commando, dan miste een weggevallen register zijn release — er wordt nu tegen beide administraties gediffed

### Opgelost — zweltrede en extra schermen

- 💡 **Zwelkast-lampjes lopen weer soepel mee bij meerdere schermen**: elk extra registerscherm bouwde elke 300 ms zijn volledige beeld opnieuw op (de wijzigings-check van het hoofdvenster ontbrak daar) — nu wordt alleen bij échte wijzigingen ververst
- 🔄 **Omgekeerd gemonteerde zweltrede via het oude inleerpad** zette de inversie-vlag niet (lampjes en volume liepen andersom) — gefixt
- 🪟 **Extra schermen kwamen na een herstart niet allemaal terug**: bij het heropenen kregen twee schermen die binnen dezelfde milliseconde openden hetzelfde venster-label, waardoor het tweede stil faalde en ook nog uit de onthouden lijst werd gewist. Elk scherm bewaart zijn positie nu ook bij het sluiten

## [0.6.13] - 2026-07-25

### Nieuw — extra registerschermen worden per orgel onthouden

- 🪟 **Extra schermen (2e, 3e, …) komen terug zoals je ze achterliet**: per orgel wordt onthouden welke schermen open stonden, met per scherm de gekozen divisies én de vensterpositie/-grootte. Bij het (opnieuw) laden van het orgel — ook na een herstart — gaan ze automatisch weer open. Zelf een scherm sluiten haalt het uit het onthouden lijstje; een orgelwissel of het afsluiten van de app deed dat voorheen óók (per ongeluk) — dat is gefixt

### Nieuw — opname- en bedieningsknoppen op de extra schermen

- 🔴 **MP3- en MIDI-opname kunnen nu vanaf elk registerscherm** gestart en gestopt worden (de status loopt op alle schermen synchroon), inclusief "Opname afspelen" na een MIDI-opname. Ook "Nieuw scherm" en "Afsluiten" staan nu op de extra schermen

### Nieuw — ronde registerknoppen

- ⚪ **Keuze tussen rechthoekige en ronde registerknoppen** (klassieke trekregisters): knop "Rond"/"Recht" in de werkbalk van het orgelscherm. De keuze wordt per orgel bewaard en geldt automatisch ook voor de extra schermen

## [0.6.12] - 2026-07-24

### Nieuw — natuurlijke galm bij staccato (onderzoek fase C)

- 🌫️ **Release-galm wordt geschaald naar de nootduur**, zoals GrandOrgue: bij een noot die tijdens de aanslag al wordt losgelaten start de release zachter (20-100%, afhankelijk van hoe ver de pijp "op spraak" was), en bij korte noten wordt de galmstaart actief afgebouwd — in de opname is de kerkgalm dan immers nog niet volledig opgebouwd ("time to full reverb", 100-350 ms afhankelijk van de release-lengte). Staccato op natte sets klinkt daardoor niet meer alsof elke tik de volle kathedraalstaart dumpt; lange noten behouden hun volledige, natuurlijke uitklank

## [0.6.11] - 2026-07-24

### Nieuw — release-fase-uitlijning voor natte samplesets (onderzoek fase B)

- 🎼 **Release-samples starten nu fase-uitgelijnd op het klinkende signaal**, zoals GrandOrgue dat doet. Bij het laden wordt per release-sample een uitlijningstabel gebouwd (16×16 amplitude×helling → startpositie); elke spelende stem onthoudt zijn laatste twee golfvorm-waarden, en bij het loslaten begint de release op de positie die daarop aansluit — in plaats van altijd op het begin. Dit haalt de fase-tik weg die je op sets als Friesach bij élke losgelaten toets kon horen, zowel bij losse noten als bij het wegtrekken van registers

## [0.6.10] - 2026-07-24

### Verbeterd — treden inleren nu ook met stap-voor-stap-popup

- 🦶 **Zweltrede en generaal crescendo leren nu net als de klavieren**: popup zegt "zet de trede in de LAAGSTE stand en houd hem stil", wordt groen zodra de stand herkend is (met CC, kanaal en waarde), vraagt dan de hoogste stand en bevestigt. Een omgekeerd gemonteerde trede wordt automatisch herkend (inversie), en time-outs geven een duidelijke melding met "Opnieuw"

### Opgelost — presets sprongen alle kanten op tijdens het spelen

- 🎛️ **Preset-knoppen op noot onthouden nu ook het MIDI-kanaal.** Een piston ingeleerd op "noot 20" vuurde voorheen óók wanneer je toets 20 op een klavier speelde — vandaar het continue preset-wisselen tijdens het spelen. Nieuw ingeleerde preset-knoppen reageren alleen nog op hun eigen kanaal (bestaande koppelingen: één keer opnieuw inleren)

### Nieuw — koppels in de generaal crescendo

- 🔗 **De crescendo-matrix heeft nu ook rijen voor de koppels**: vink per stap aan welke koppels erbij komen. De trede claimt — net als bij registers — geen koppels die je zelf al had ingeschakeld

### Onderzoek — externe samplesets (GrandOrgue-vergelijking)

- 🔬 De GrandOrgue-broncode is bestudeerd om onze afspeelopzet voor natte GO-sets te toetsen; volledig verslag in `ONDERZOEK-EXTERNE-SETS.md`. Kern: GrandOrgue lijnt release-samples in FASE uit op het klinkende signaal (wij starten op de cue → tikken bij loslaten) en schaalt release-galm bij staccato ("time to full reverb" — verklaart de "rare galm"). Voorgestelde fasering staat in het verslag; als eerste stap is de release-budget-fade verzacht naar 250 ms (GrandOrgue-niveau)

## [0.6.9] - 2026-07-24

### Verbeterd — klavier inleren met echte stap-voor-stap-popup

- 🎹 **Nieuw inleerproces voor klavieren**: een popup vraagt "druk de LAAGSTE toets in", wordt groen zodra de toets herkend is (met nootnaam en kanaal), vraagt dan de hoogste toets en bevestigt het geleerde bereik. Time-outs en een tweede toets op een ander kanaal geven een duidelijke foutmelding met "Opnieuw" — het oude proces raadde met een timer en gaf geen terugkoppeling

### Opgelost — zweltrede inleren na een eerdere crescendo-koppeling

- 🦶 **Zwel- en crescendo-inleer zijn nu wederzijds exclusief (laatst ingeleerd wint).** Was een trede eerder als generaal crescendo ingeleerd en leerde je hem daarna als zwelkast, dan bleef de oude crescendo-koppeling de CC "claimen" en deed de zwelkast niets. Bij het inleren van een zwelkast wordt een crescendo-koppeling op dezelfde CC nu gewist, en andersom

### Opgelost — pistons op het klavierkanaal klonken nog als noten

- 🎛️ **Het ingeleerde toetsbereik filtert nu ook**: pistons/setzerknoppen die MIDI-noten sturen op hetzélfde kanaal als een klavier (maar buiten het ingeleerde laagste-hoogste-bereik) klinken niet meer als orgelnoten en zijn gewoon inleerbaar als preset-knop. Leer wel eerst het klavierbereik in (met de nieuwe popup), anders is er geen bereik om op te filteren

### Opgelost — Friesach: kraken en vastlopen bij vol werk loslaten

- 🌊 **Release-staarten begrensd tot een budget (160 tegelijk).** Bij het loslaten van vol werk op een natte GO-set startte per pijp × register een extra release-stem (de opgenomen kerkakoestiek): 200+ nieuwe stemmen in één klap, ruim 600 totaal — de mixer verzoop, het kraakte en de afbouw duurde heel lang. Boven het budget maakt de stilste bestaande staart nu versneld plaats; eigen droge sets (Puttershoek) hebben geen release-samples en vallen zoals voorheen direct terug naar 0

## [0.6.8] - 2026-07-23

### Opgelost — niets ingeleerd = geen respons (treden én pistons)

- 🦶 **Een zwel-/crescendotrede doet nu niets totdat je hem inleert.** Consoles sturen hun treden standaard als CC7/CC11 (volume/expressie) en een oude "gemaks"-fallback liet elke onbekende CC7/11 meteen het mastervolume regelen — met drie treden had je dus drie ongevraagde volumeregelaars die elkaar overschreven, zonder dat er iets was ingeleerd. Die fallback is verwijderd: een trede krijgt pas een functie als jij hem inleert als zwelkast (per klavier) of als generaal crescendo
- 🎹 **Piston-/setzerknoppen die MIDI-noten sturen klinken niet meer als orgelnoten.** Een klavier zonder ingeleerde koppeling accepteerde voorheen "alle kanalen", waardoor knopdrukken van de console als noten doorklonken. Een klavier reageert nu pas op fysieke MIDI zodra het is ingeleerd (in de MIDI-koppelingen-editor kun je desgewenst nog expliciet "alle kanalen" kiezen). Het schermklavier en de computertoetsen werken gewoon zonder inleren; let op: ook MIDI-bestanden afspelen vereist ingeleerde klavieren op het orgel

### Opgelost — haperen/tikken bij veel registers op grote samplesets

- 🚿 **De logstorm per toetsaanslag is gedempt**: elke noot schreef 1 + (aantal klinkende registers) regels naar console én logbestand — bij vol werk op Friesach honderden regels per seconde, die de MIDI-verwerking lieten stotteren precies op de drukke momenten. Per-noot-logging staat nu standaard uit (alleen nog op trace-niveau voor diagnose)
- ⚙️ **Achtergrond-sampleladers draaien onder normale prioriteit**: het decoderen van volledige samples (vol werk aanslaan/loslaten op een natte set) verdrong de audio-verwerking van de CPU — een bekende bron van tikken. De audio-thread heeft nu altijd voorrang; samples laden hooguit iets langzamer bij, wat de preload-buffers opvangen

## [0.6.7] - 2026-07-23

Grote onderhoudsronde: 53 van de 63 bevindingen uit de code-audit van 18 juli verwerkt
(zie AUDIT-2026-07-18.md en FIXLOG-0.6.7.md in de repo; 3 deels, 7 bewust gelaten met reden).

### Audio-betrouwbaarheid

- 🎛️ **Geen zware operaties meer in de realtime audio-lus**: eigen galm-IR's worden voortaan buiten de audio-thread gedecodeerd en voorbereid (geen haper meer bij het laden), grote datastructuren worden niet meer gekopieerd of gealloceerd in de callback, en de "geen sample"-melding logt nog maar één keer per pijp
- 🔇 **Een corrupt sample kan de filters niet meer blijvend vergiftigen**: niet-eindige waarden worden nu vóór de zwelkast-/galm-/EQ-keten afgevangen
- 🎧 **Eigen galm-IR's klinken correct**: samplerate wordt geresampled (galm speelde te snel/langzaam af), 32-bit IR's hebben geen omgekeerde polariteit meer, en een leeg IR-bestand kan de app niet meer laten crashen
- 🔉 **Voice-stealing zonder klik**: bij het 512-stemmen-plafond krijgt de stilste stem een korte fade in plaats van een harde knip
- 📻 **Kanaal-routering valt terug op het voorste paar** wanneer de geconfigureerde kanalen niet op het apparaat bestaan (droog signaal verdween geluidloos terwijl de galm doorklonk)
- 🌊 **ODF-looppunten gelden nu ook voor de volledig geladen sample** (loop versprong hoorbaar op het moment dat de achtergrond-load klaar was)

### Stabiliteit en samenspel van threads

- 🧵 Diverse blokkades en races opgelost: recv-timeouts richting de MIDI-thread (app bevroor als je iets deed tijdens een leer-modus), lock-volgordes gelijkgetrokken (deadlock-scenario), instellingen-opslag geserialiseerd met orgel-loads, en de audio-wissel houdt zijn lock niet meer 2 s vast
- 🎹 **Klavier-inleren laat klinkende noten niet meer eeuwig hangen** (NoteOffs worden nu doorgestuurd tijdens het leren)
- 🚨 **Na een audio-noodherstel of de uitgestelde ASIO-start worden alle DSP-instellingen (volume, galm, EQ, routing, pans) weer toegepast** — voorheen speelde het orgel dan op kale defaults
- 💾 **.jm-settings.json naast het orgel wordt nu echt gebruikt**: op een verse pc/AppData worden alle instellingen (inleren, presets, voicing) daaruit geïmporteerd in plaats van stilletjes overschreven

### GrandOrgue-import en eigen orgels

- 🛡️ **Corrupte orgelbestanden kunnen de app niet meer crashen**: caps op ODF-tellers (ReleaseCount e.d. gaven een ~200 GB-allocatie), ADPCM/misvormde WAVs worden netjes geweigerd i.p.v. een paniek, misvormde REF-regels wijzen niet meer stil naar de verkeerde pijp, en leesfouten in samples worden gelogd
- 🔁 **Eigen-orgel-export is round-trip-correct**: pijpen behouden hun echte toets (discant-registers verschoven twee octaven), voetmaten verdubbelen niet meer, en bas/discant-mappen worden weer als één register herkend
- 🎚️ **Octaafkoppel op het eigen klavier klinkt één octaaf** — niet tot acht octaven doorgestapeld

### Bediening

- 🎬 **MIDI-afspelen**: zoeken, pauzeren en snelheid wisselen springen niet meer en veroorzaken geen noten-vloed
- 🎛️ **Setzer**: geheugenniveau-wissel laadt het juiste niveau, presets lekken niet meer tussen niveaus/orgels, setzer-presets komen nu wél in de per-orgel-opslag terecht, en opgespaarde piston-drukken van tijdens een orgel-load worden niet meer in één keer afgespeeld
- 🪟 **Extra registerpanelen** kapen geen MIDI-preset-acties meer van het hoofdvenster en tonen na een orgelwissel geen leeg paneel meer
- 🌬️ Windgroep-keuze wordt niet meer teruggedraaid terwijl je hem instelt; na een uitgang-/profielwissel worden per-orgel DSP-instellingen opnieuw toegepast; het wijzigen van het aantal crescendostappen wist de matrix niet meer
- 🔌 Twee identiek genaamde MIDI-interfaces verbinden nu allebei; een tweede app-instantie wist het log van de draaiende instantie niet meer; de sampleset-trimtool bewaart cue/release-markers en schrijft crashveilig

## [0.6.6] - 2026-07-20

### Opgelost — instellingen lekten door naar het volgende orgel (o.a. zweltreden)

- 🎚️ **Zwel-, MIDI-klavier-, preset- en crescendo-koppelingen worden bij een orgelwissel volledig gewist** en daarna alleen gevuld met wat het níeuwe orgel zelf heeft opgeslagen. Voorheen bleven de koppelingen van het vorige orgel actief — een zweltrede kon dan het verkeerde klavier aansturen ("er gebeurt iets met het geluid") — en werden ze via de autosave zelfs blijvend in het nieuwe orgel weggeschreven. Elk orgel heeft nu strikt zijn eigen koppelingen (eenmalig per orgel inleren; daarna onthouden)
- 🔊 **Mastervolume en galm van het vorige orgel lekken niet meer in een vers orgel**: een orgel zonder eigen instellingen start nu écht op de standaardwaarden (-6 dB; GO/Hauptwerk-imports droog) in plaats van op de stand van het vorige orgel — en die lek-stand wordt dus ook niet meer als "instelling" van het nieuwe orgel opgeslagen
- 🌬️ Ook het aantal windgroepen en de C/Cis-spreiding gaan bij een orgelwissel terug naar de standaard (vielen buiten de bestaande reset)

### Gewijzigd — generaal crescendo respecteert handmatige registratie

- 🦶 **De crescendotrede claimt geen registers meer die je zelf al getrokken had** (GrandOrgue-gedrag): bij het terugveren van de trede verdwijnen alleen de registers die de trede zélf bijtrok — je eigen registratie blijft onaangetast staan

## [0.6.5] - 2026-07-18

### Opgelost — generaal-crescendo maakte het orgel zachter

- 🎚️ **De crescendotrede dempt het volume niet meer.** Stuurt de trede CC 7 of CC 11 (volume/expressie — gebruikelijk bij veel consoles), dan werd dezelfde pedaalbeweging óók door de algemene expressie-fallback verwerkt: registers kwamen erbij, maar het mastervolume ging tegelijk fors omlaag ("registers erbij, geluid zachter"). Een als crescendo ingeleerde pedaal is nu exclusief crescendo — hij doet niets meer met zwelkast- of expressievolume, ook niet tijdens het afspelen van een MIDI-opname
- 🦶 **Crescendostand start altijd op 0 bij het laden van een orgel** (de teller bleef voorheen op de oude trap hangen, waardoor de eerste pedaalbeweging binnen dezelfde trap niets deed)
- 🔁 **Na het (opnieuw) inleren van de crescendotrede wordt het mastervolume expliciet teruggezet**: tijdens het inleren is de koppeling tijdelijk gewist en kan een CC7/11-pedaal het volume nog dempen via de expressie-fallback; omdat die CC daarna exclusief crescendo is, bleef die demping anders blijvend hangen (gevonden bij de code-audit)

### Opgelost — audio-wissel: registerknoppen bleven "uit" staan

- 🎛️ **Na een speakers↔hoofdtelefoon-wissel tonen de registerknoppen weer de echte stand.** De registratie wérd al hersteld (sinds 0.6.3), maar het orgel-overzicht dat de herlaad teruggaf was gemaakt vóór dat herstel — alle knoppen oogden "uit", en de verversing corrigeerde dat niet omdat de herstelde stand identiek was aan die van vóór de wissel. Wie dan een knop aanklikte zette het register juist wéér uit — het "registers vallen uit"-gevoel. De load geeft nu de stand ná het herstel terug

### Nieuw — kanaalkeuze wisselt mee met de uitvoerprofielen

- 🔀 **Uitvoerprofielen (speakers/hoofdtelefoon) slaan nu ook de kanaalkeuze per klavier op.** Zo speel je op dezelfde ASIO-interface met speakers op eigen kanalen én een hoofdtelefoon op een ander kanalenpaar — de routering wisselt met één knop (of MIDI-knop) mee. "Huidige selectie opslaan" legt host, apparaat, buffer én de kanaalkeuze per klavier vast (op klaviernaam; ook bij een ander orgel met dezelfde klaviernamen toegepast)
- 🧷 De profiel-kanalen gelden als tijdelijke override: de per-orgel opgeslagen kanaalroutering blijft onaangetast bewaard en komt terug zodra je geen profiel (of een profiel zonder kanalen) actief hebt. Handmatige kanaal-wijzigingen terwijl een profiel actief is werken direct en blijven binnen dat profiel gelden; sla het profiel opnieuw op om ze blijvend te maken

### Gewijzigd — schone start bij het laden van een orgel

- 🧹 **Koppels starten nu ook uit.** "Laatste registratie herstellen bij openen" (standaard uit) gold al voor de registers, maar de koppels kwamen onvoorwaardelijk terug uit de vorige sessie. Nu vallen registers én koppels onder dezelfde keuze: optie uit = volledig schone start, optie aan = alles komt terug. (Een audio-wissel behoudt de live registratie uiteraard wél — dat is een herlaad van hetzelfde orgel, geen nieuwe start)

## [0.6.4] - 2026-07-14

### Opgelost — klavieren inleren blijft nu bewaard

- 🎹 **Ingeleerde klavieren en MIDI-kanalen zijn na een herstart gewoon terug.** Twee oorzaken gevonden: (1) de backend herstelde de koppelingen wél bij het laden van een orgel, maar de UI ververste de lijst nooit — je zág dus "leeg" en leerde onnodig opnieuw in; (2) geleerde koppelingen werden alleen bij een nette afsluiting opgeslagen (met een tijdslimiet die dat kon afbreken) — nu worden ze direct na het inleren automatisch bewaard, net als de registratie
- 🔌 **MIDI-klavieren activeren zichzelf**: een console die pas ná de app wordt ingeschakeld (of waarvan de eerste verbindingspoging tijdens de USB-handshake mislukt) wordt nu elke 5 seconden opnieuw geprobeerd — handmatig MIDI "aan/uit zetten" is niet meer nodig

### Opgelost — zwelkasten uit de orgeldefinitie werken nu

- 🎚️ **GO-/Hauptwerk-zwelkasten worden herkend**: moderne orgelbestanden zetten de windlade-koppeling op de rank in plaats van op het register, waardoor de zwelkast-detectie (windlade → Enclosure) nooit aansloeg. Friesach's Schwellwerk heeft nu gewoon zijn zweltrede in de app, precies zoals de orgeldefinitie voorschrijft

### Verbeterd — klankkwaliteit GO-sets

- 🌊 **Fase-uitgelijnde loops óók in de eerste seconden van elke noot**: de anti-tik-optimalisatie van looppunten draaide alleen op de volledige samples, niet op de preload-buffers waar elke noot zijn eerste ~2 seconden op speelt. Nu overal; en een loop die door de stilte-trim beschadigd raakte wordt herkend i.p.v. stilletjes verschoven
- 🎼 **Vloeiender interpolatie (4-punts Hermite i.p.v. lineair)**: een 48kHz-set op een 44,1kHz-apparaat (en elke tremulant-/temperament-verstemming) klinkt merkbaar schoner op heldere registers — minder aliasing-ruis
- ⚡ **Drie parallelle sample-laders i.p.v. één**: na het loslaten van een vol akkoord op een natte set worden de release-staarten drie keer zo snel bijgeladen
- 🎛️ **Limiter verfijnd** (plafond -0,26 dB, gedoseerde attack van 1,5 ms): één piek trekt niet langer het hele orgel hoorbaar omlaag. Crescendo-meting op Friesach: gelijkmatig stijgend van 1 register tot vol werk, geen "wegzakken" meer

### Gewijzigd — standaardinstellingen voor externe samplesets

- 🌫️ **GO-/Hauptwerk-imports starten voortaan zónder toegevoegde galm** (de kerkakoestiek zit al in de opnamen; de standaard 30% "Dorpskerk" er bovenop gaf een troebel dubbel-galm-geluid). Eigen droge samplemappen behouden de vertrouwde standaardgalm; een zelf ingestelde galm blijft altijd bewaard. Alle andere effecten (EQ, windmodel, C/Cis-spreiding, tremulant-LFO) stonden al correct uit

## [0.6.3] - 2026-07-13

### Opgelost — vol werk: geen vervorming en wegzakkend geluid meer

- 🎛️ **Echte master-limiter in plaats van tanh-vervorming.** Bij een vol werk (honderden stemmen tegelijk) kwam de mix-som ver boven het maximum en drukte de oude tanh-verzadiging per kanaal het signaal plat: hoorbare vervorming ("overstuur"), platgeslagen dynamiek ("gedempt" geluid — meer registers erbij deed niets meer) en een instortend stereobeeld (elk kanaal vervormde onafhankelijk). De nieuwe limiter verlaagt bij een piek de tótale gain (attack direct, release ~250 ms): geen vervorming, de onderlinge balans en het stereobeeld blijven intact, en na de piek veert het niveau vanzelf terug. De MP3-opname gebruikt dezelfde limiter, zodat opnames klinken zoals er gespeeld werd
- 🌬️ **Windmodel-demping herijkt** (alleen relevant als het windmodel aanstaat): de oude formule zat al bij ~3 stemmen op de maximale inzakking, waardoor het model als botte volumedemper werkte bij vollere registraties. De inzakking loopt nu vloeiend op met het aantal stemmen en nadert het maximum pas asymptotisch bij een echt vol werk

### Opgelost — geheugengroei tijdens lang spelen (gekraak/bevriezing)

- 🧠 **RAM-plafond op volledig geladen samples (~1,5 GB, oudste eerst weg).** Elke gespeelde pijp + release bleef voorheen voorgoed in het geheugen: uren spelen op een grote GO-set liet het geheugen tot vele gigabytes groeien → paging → gekraak. Een verdrongen sample is altijd veilig (klinkende stemmen houden hun eigen kopie vast) en komt bij een volgende noot gewoon terug via preload + achtergrondlaad
- 🧹 **Grote geheugenopruiming nooit meer op de audio-thread**: bij een orgelwissel na lang spelen werden gigabytes aan samples ín de audio-callback vrijgegeven — de audio stond dan seconden stil, de watchdog sloeg aan en de app kon volledig vastlopen (live gereproduceerd). Oude sample-verzamelingen gaan nu naar een aparte opruim-thread
- ⏱️ **Commandoverwerking per callback begrensd** (max 256): een tutti-burst wordt over enkele callbacks uitgesmeerd (~5 ms, onhoorbaar) in plaats van één callback over zijn realtime-deadline te duwen

### Opgelost — hoofdtelefoon/kanalen-wissel: registratie blijft nu staan

- 🎚️ **Een audio-uitvoer-wissel gooit de getrokken registers niet meer weg.** De verplichte orgel-herlaad na elke wissel (speakers↔hoofdtelefoon, WASAPI↔ASIO) wiste de registratie en herstelde die alleen wanneer de losse "herstel laatste registratie"-optie aanstond. Nu wordt de live registratie (registers + koppels) vlak vóór de wissel vastgelegd en na de herlaad van hetzélfde orgel automatisch teruggezet — onafhankelijk van die optie
- 🔀 **Race gefixt waarbij het noodherstel/de uitgestelde ASIO-wissel het verkéérde orgel kon terugladen**: laadde je zelf net een ander orgel terwijl het herstel liep, dan werd jouw keuze overschreven door het oude orgel-id. Beide herlaadpaden controleren nu eerst of hun orgel-id nog actueel is
- 🔉 **WASAPI→WASAPI-wissels breken de oude stream nu echt af** (begrensd wachten) in plaats van hem zwevend achter te laten — geen kort dubbel geluid of apparaat-contentie meer direct na de wissel

### Opgelost — GrandOrgue-sets: kwaliteit en stabiliteit

- 🎼 **Loop-naad-tikken op veel GO-sets verholpen**: de fase-uitlijning van looppunten draaide alleen in het standaard-WAV-pad; bestanden met een afwijkende fmt-header (o.a. álle Piotr Grabowski-sets) sloegen die optimalisatie over en hielden hoorbare tikken op de loop-naad. De juiste parser wordt nu vooraf gekozen (fmt-probe) en de loop-optimalisatie draait in álle paden
- 🗝️ **Meerkorige registers worden nu op toets-positie gelegd** (RankFirstAccessibleKeyNumber gerespecteerd): gesplicede koren (geleende bas, repeterende kwint) komen op de juiste toetsen terecht, óók als de ranks niet netjes aansluitend in de ODF staan; een misvormde REF-regel schuift niet langer alle volgende pijpen een toets op. Écht gestapelde koren (zelfde toetsbereik dubbel) spelen nog één koor — dat staat nu expliciet met een waarschuwing in het log
- 🔇 **Stille toetsen zijn nu traceerbaar**: een onopgeloste REF-verwijzing (de bekende stille-toetsen-oorzaak) logt voortaan precies welk register en welke pijp geen sample kreeg; een REF-keten met een cyclus crasht de app niet meer (dieptebegrenzer)
- 🧟 **"Zombie"-stemmen verholpen**: bij een volle achtergrondlaad-wachtrij (tutti loslaten op een natte set) werden laadverzoeken stil gedropt en bleef een stem eindeloos zijn 2-seconden-attackfragment loopen. Wachtrij vergroot (64→1024), verzoeken worden per pijp ontdubbeld en een gedropt verzoek blokkeert een nieuwe poging niet meer
- ⚡ **Zelfde bestand niet meer dubbel gedecodeerd**: REF-gedeelde ranks en same-file-releases gebruiken nu een pad-cache in de achtergrondlader (zelfde geheugen-Arc gedeeld) — minder CPU-pieken bij het loslaten van akkoorden en minder RAM
- 🔁 **Orgelwissel-race gedicht**: een achtergrond-load die pas ná een orgelwissel afrondde kon zijn sample onder een hergebruikte pijp-key van het níeuwe orgel registreren (verkeerde pijp/samplerate). Laadresultaten dragen nu een generatie-stempel en verlopen resultaten worden genegeerd
- 🎛️ **Callback-integratie begrensd**: klaargekomen achtergrond-loads worden met maximaal 16 per audio-callback verwerkt zodat een burst de realtime-deadline niet kan overschrijden
- 🎧 **Extensible-float-WAVs klinken niet meer als ruis** in het preload-pad (WAVE_FORMAT_EXTENSIBLE-subformaat wordt nu gelezen); 8-bit-normalisatie gelijkgetrokken tussen de parsers
- 🚿 **Logspam bij GO-sets weg**: de twee waarschuwingen per bestand (duizenden per orgel-load) zijn gedempt; dit kon bovendien beheerd gestarte processen laten bevriezen zodra de stderr-buffer volliep
- ⌨️ **Tutti-bursts bevriezen de app niet meer**: het commandokanaal naar de audio-thread is vergroot (512→4096) — een vol akkoord op veel registers met koppels stuurde meer commando's dan de wachtrij aankon, waardoor UI/MIDI-aanroepen seconden blokkeerden
- 🧪 Test-API overleeft nu een panic in een handler (alleen relevant voor geautomatiseerd testen)

## [0.6.2] - 2026-07-08

### Opgelost — kraken/haperen bij veel registers (vooral GrandOrgue-sets)

Wortel-oorzaak: de audio-mengloop deed **per klinkende pijp, per sample**
opnieuw werk dat in werkelijkheid constant is — vier HashMap-opzoekingen plus
`powf`/`sin`/`cos`. Bij een volle registratie (honderden gelijktijdige voices
op 44,1 kHz) liep dat op tot tientallen miljoenen bewerkingen per seconde op
één kern, waardoor de render-thread de buffer-deadline miste → buffer-underruns
→ hoorbaar gekraak. Gemeten op Friesach: 13 registers + een akkoord zaten al op
~50–70% van één kern; ná de fix voegt hetzelfde spel vrijwel niets meer toe
boven de rusttoestand.

- ⚡ **Per-voice constanten worden nu één keer per audio-callback voorgerekend** i.p.v. per sample: divisie-index, gestapelde voicing (gebruiker + ODF-intonatie) en de tremulant-keuze worden gecachet op de voice. Wind- en tremulant-modulatie worden per divisie (max 32) berekend i.p.v. per voice, en de constant-power panning-cos/sin per divisie één keer per callback. Het hete pad bevat zo geen HashMap-opzoekingen of transcendente functies meer per voice per sample. Live bijregelen van voicing/pan/temperament blijft werken (wordt de eerstvolgende callback opgepikt)
- 🧯 **Harde polyfonie-grens (512 voices) met voice-stealing**: het echte audiopad had géén limiet — de voice-lijst kon onbeperkt groeien (blijvende noten, koppels, tutti + release-staarten) en zo de render-thread over de deadline duwen. Zit de lijst vol, dan wordt de zachtste (bij voorkeur al uitklinkende) voice gestolen
- 🔇 **Logging uit de audio-callback gehaald**: de per-noot `debug!`-regels (die strings opmaakten en op een met de MIDI-thread gedeelde bestands-mutex konden blokkeren) zijn weg uit het NoteOn/NoteOff/one-shot-pad
- 🌫️ **Galm wordt overgeslagen bij mix 0**: de FDN/convolutie-galm draaide voorheen elke sample door, óók als de galm helemaal dicht stond — nu wordt dat rekenwerk overgeslagen
- 🧵 **Geen heap-allocaties meer per audio-callback**: de zes config-momentopnamen (divisie-gains/pans/routing/C-Cis/wind-groepen/zwelkast) werden elke callback gekloond. Ze worden nu als read-guard vastgehouden (die config wordt alleen in de commandofase van diezelfde callback geschreven, dus zonder concurrentie). Op ASIO4ALL, dat callbacks zeer frequent aanroept, scheelt dit merkbaar allocatie-jitter op de audio-thread — een directe bron van gekraak
- 🩺 **Audio-watchdog rijdt kortstondige ASIO-glitches uit**: een enkele stream-fout onder piekbelasting (typisch een ASIO4ALL-underrun waarbij de callbacks gewoon doorlopen) brak voorheen meteen de stream af en kon escaleren naar een volledige herstart + orgel-herlaad — een korte hapering werd zo een langere stilte. De watchdog wacht nu ~0,8 s en herbouwt alleen als de callbacks ook echt stoppen; bij een tijdelijke glitch gebeurt er niets

### Diagnose-notitie: de CPU-basislast komt van ASIO4ALL, niet van de app

Gemeten op de console-PC: met een orgel geladen maar zónder te spelen gebruikt de app op **WASAPI ~0–3% van één kern**, maar op **ASIO4ALL ~50% van één kern** — een vaste last die ASIO4ALL zelf veroorzaakt (kernel-streaming-wrapper, zwaarder bij een kleine buffer). Op een 16-kern-machine is dat ~3% van het totaal en de audio-thread zit ruim onder zijn deadline, dus dit is geen crackle-oorzaak op zich, maar het eet wél de marge op. Wie lagere CPU/meer marge wil: een grotere ASIO4ALL-buffer, een echte ASIO-driver van de geluidskaart, of WASAPI (hogere latency).

### Nog niet in deze versie (bekende resterende punten)

- De release-samples van GrandOrgue-sets worden bij de eerste keer spelen per toets gedecodeerd via een trage handmatige WAV-parser (de snelle decoders falen op deze bestanden); dat geeft korte CPU-pieken bij het loslaten tot de cache warm is (op een multi-core-machine door reservekernen opgevangen). Nog te doen: eenmalig decoderen/ontdubbelen of releases vooraf inladen.

## [0.6.1] - 2026-07-07

### Opgelost — de speakers↔hoofdtelefoon-wissel werkt nu echt

Wortel-oorzaak gevonden met probes op de console-PC: **ASIO4ALL kan binnen één
app-sessie maar één keer initialiseren** (elke her-init faalt blijvend met
"No audio output device found"), en het láden van de ASIO-driver claimt de
WASAPI-endpoints exclusief — waardoor de app met zijn eigen draaiende stream
(en de watchdog die hem elke 2 s herbouwde) om het apparaat vocht.

- 🎧 **ASIO-driver wordt één keer geladen en daarna procesbreed hergebruikt**: elke volgende wissel naar ASIO bouwt een nieuwe stream op de al geladen driver — heen en weer wisselen tussen ASIO-speakers en een WASAPI-hoofdtelefoon werkt nu onbeperkt vaak
- 🚪 **Wissels van/naar ASIO breken de oude uitgang éérst volledig af** (met echte wacht op de audio-thread) vóór de nieuwe wordt gebouwd, met een retry-ladder voor het asynchroon vrijgeven van apparaten; kiest de gebruiker expliciet WASAPI op het apparaat dat de idle ASIO-driver bezet houdt, dan wordt de driver vrijgegeven en opnieuw geprobeerd
- 📋 **Apparaatlijsten laden geen drivers meer**: de ASIO-lijst in Instellingen komt nu uit het register (namen-scan) — het openen van de instellingen doodt de lopende WASAPI-stream niet meer (dat was de mysterieuze watchdog-herbouw bij elke start)
- 🗣️ **Eerlijke wisselresultaten**: een wissel meldt nu gestructureerd wat er echt gebeurd is (gelukt / vorige uitgang hersteld / op standaardapparaat teruggevallen, mét Nederlandse uitleg). De stille "fallback-gemeld-als-succes" — geluid op het verkeerde apparaat terwijl de knop hoofdtelefoon toont — is weg
- 🎯 **Strikte apparaatkeuze**: een gevraagd apparaat dat er niet (meer) is geeft nu een duidelijke fout in plaats van stilletjes naar het standaardapparaat te routeren; bij falen wordt de vórige uitgang hersteld (niet zomaar de default)
- 🧊 **De wissel bevriest de app niet meer**: het wisselcommando draait asynchroon buiten de UI-thread
- 🩹 De echte foutoorzaak wordt gemeld i.p.v. een misleidende "Audio stream did not start within 8s" (apparaatresolutie-fouten bereikten de melding nooit)
- 🖱️ De "Activeer"-knop bij uitvoerprofielen is niet langer uitgeschakeld voor het actieve profiel — opnieuw activeren is een legitiem herstelmiddel; het afgeleide actieve profiel overschrijft de opgeslagen keuze niet meer

### Gewijzigd — nagalm-presets

- 🏛️ **Vaste ruimte-presets zonder schuifregelaars**: kies je Kleine Kapel t/m Concertzaal, dan geldt die klank zoals ontworpen — de schuiven zijn verborgen
- 🎚️ **Nieuwe preset "Eigen (per orgel)"**: mét schuiven (RT60, pre-delay, demping, ruimte), en die instelling wordt per orgel vastgelegd. Oude opslag met bijgestelde schuiven wordt automatisch als "Eigen" herkend zodat je klank behouden blijft
- 💾 **Eigen impuls-respons (convolutie-galm) blijft nu bewaard**: het gekozen IR-bestand wordt per orgel opgeslagen en na een herstart, orgelwissel of audio-wissel automatisch herladen (voorheen moest je hem elke keer opnieuw laden)

### Toegevoegd — alle MIDI-koppelingen handmatig te bewerken

- 🛠️ **Nieuw blok "MIDI-koppelingen (handmatig bewerken)"** in Orgel-Instellingen: álle koppelingen (setzer-knoppen, SET/GC/±1/±10/geheugens, tremulanten, EQ- en crescendo-schakelaar, afsluiten, speakers↔hoofdtelefoon, koppels én registers) in één tabel — type (noot/CC/program change), kanaal, nummer en CC-drempel direct aan te passen, per koppeling te verwijderen (✕), en onderaan handmatig toe te voegen met doel-keuzelijst. Inleren blijft daarnaast gewoon werken (max 4 signalen per doel)
- ⌨️ **Toetsenbereik handmatig**: per klavier zijn de laagste/hoogste MIDI-noot nu direct in te typen (naast de bestaande twee-toetsen-inleer)
- 🦶 **Zwelpedaal en crescendopedaal handmatig**: MIDI-kanaal en CC-nummer per divisie/crescendo direct in te vullen (naast "Leer pedaal"); de crescendo-pedaalkoppeling heeft nu ook een wisknop in de UI



### Toegevoegd

- 🎧 **Snelle wissel speakers ↔ hoofdtelefoon**: sla twee uitvoerprofielen op (host + apparaat + buffer, bijv. luidsprekers via ASIO en een hoofdtelefoon via WASAPI) en wissel met één knop in de kopbalk — of via een ingeleerde MIDI-knop

### Opgelost (wissel-robuustheid, n.a.v. "geen geluid op speakers")

- 🔇 **Mislukte wissel kan het geluid niet meer meenemen**: de profielknop wordt alléén omgezet als de wissel écht slaagt, en bij falen keert de app automatisch terug naar het vorige (werkende) profiel met een duidelijke foutmelding; een wissel naar een afwezig apparaat (hoofdtelefoon uit/los) wordt vooraf geweigerd i.p.v. stilletjes naar een ander apparaat te routeren
- 🔒 **ASIO-exclusiviteit bij het wisselen**: gaat de wissel van ASIO naar WASAPI mis doordat ASIO het apparaat nog exclusief vasthoudt, dan wordt de oude stream eerst netjes gesloten en opnieuw geprobeerd (daarna default host als laatste redmiddel)
- 🧭 **De knop toont altijd de échte uitgang**: het actieve profiel wordt afgeleid uit wat er werkelijk speelt (ook na een app-herstart of een handmatige wijziging in Instellingen) — staat het geluid op de hoofdtelefoon, dan zie je dat meteen in de kopbalk
- 🚑 **Audio-noodherstel**: krijgt de interne watchdog de stream vijf keer op rij niet herbouwd (bv. apparaat verdwenen en de vervanger heeft een ander formaat), dan wordt de complete audio-uitvoer automatisch opnieuw opgebouwd en het orgel herladen — geen blijvende stilte meer die alleen een herstart oploste; het noodherstel herhaalt zichzelf zolang nodig en pakt ook een volledig weggevallen audio-uitvoer op
- ✅ **Een wissel telt pas als geslaagd wanneer er écht audio stroomt**: een stream die wel "start" maar nooit audio levert (het beruchte stille ASIO4ALL-gedrag) wordt nu herkend en als fout gemeld, zodat de app op de werkende uitgang blijft in plaats van stil te vallen; ook een herbouwde stream wordt pas als hersteld beschouwd zodra er callbacks binnenkomen
- 🛟 **Voorkeuren overleven storingen**: valt de app tijdelijk terug op de standaard-uitgang (nood-fallback), dan blijft je opgeslagen voorkeur (bv. ASIO) onaangetast; een tijdelijke storing kan je vaste instelling niet meer wissen
- 🧵 Diverse vastloop-scenario's rond het wisselen gedicht (blokkerende afsluitcommando's op een volle wachtrij, zombie-audiothreads na een time-out, en een race tussen het noodherstel en een handmatige wissel)
- 🎹 **MIDI-kanaal handmatig instellen werkt nu écht**: de kanaal-keuzelijst (1-16) stuurde intern het verkeerde kanaalnummer (één te hoog), waardoor het klavier zweeg na een handmatige keuze en een geleerd kanaal als lege keuzelijst verscheen — weergave en techniek zijn nu correct gekoppeld
- 🚪 **"Laatst geopende orgel automatisch laden bij starten" is nu instelbaar** (Algemene Instellingen, standaard aan) — los van de bestaande optie "Laatste registratie herstellen bij openen"
- 🔗 **Koppels koppelen door (transitief)**: is Pedaal→Hoofdwerk actief én Hoofdwerk→Nevenwerk, dan klinkt het pedaal nu ook door op het Nevenwerk; octaafkoppels sommeren netjes over de keten (4' + 16' = unisono) en kringen worden veilig afgekapt. T.A. op de doeldivisie onderdrukt ook meegekoppelde tongwerken
- 🎼 **GrandOrgue-import volgens de officiële semantiek** (nieuw importspec-onderzoek):
  - **Same-file releases**: bij sets waar de kerkakoestiek ná de loop in hetzelfde WAV-bestand zit (cue-marker-model) speelt het loslaten nu écht de opgenomen ruimte af; aparte release-bestanden starten voortaan op hun cue-marker zoals GrandOrgue doet
  - **Inschalingshiërarchie compleet**: AmplitudeLevel/Gain/PitchTuning van álle niveaus (orgel × windkast × stop/rank × pijp) worden nu gesommeerd — voorheen ontbraken het windkast- en rank-niveau
  - **PitchCorrection niet meer onterecht toegepast**: GrandOrgue gebruikt die alleen bij hertemperen — een Voix Céleste zweeft nu zoals opgenomen, niet dubbel
  - **smpl-loops exact**: het loop-einde is inclusief (GrandOrgue-semantiek, was 1 frame te kort), alle loop-records worden gevalideerd en de langste geldige wint; ODF-loops winnen nu van de WAV-loops (was andersom)
  - **Release-fade toonhoogte-afhankelijk** zoals GrandOrgue: lage pijpen ~184 ms, hoge ~6 ms (was vast 42 ms) — bassen sterven natuurlijker uit
  - 8-bit PCM-samples worden nu ook door de preload-loader geaccepteerd

### Eerder toegevoegd (2026-07-04)

- 🎛️ **Vrije EQ per uitgangskanaal** (GrandOrgue-stijl): onbeperkt banden toevoegen, per band type (piek, laag-/hoogdoorlaat, banddoorlaat, lage/hoge shelf), frequentie, gain, bandbreedte (octaven) en doelkanaal ("Alle kanalen" of één specifiek fysiek kanaal). Oude 3-bands instellingen worden automatisch gemigreerd
- ⛪ **Stereo-galm die altijd werkt**: de algoritmische galm is nu echt stereo (gedecorreleerde L/R-staart), ~12 dB krachtiger gekalibreerd, en de engine valt automatisch terug op de algoritmische galm zolang er geen IR-bestand geladen is (voorheen: convolutie zonder IR = helemaal geen galm). Slider-wijzigingen (rt60/demping/mix) wissen de klinkende galmstaart niet meer
- 🩺 **Audio-zelfherstel**: valt het uitvoerapparaat weg of stopt de stream (bv. bluetooth-koptelefoon die verbindt, apparaatwissel), dan herbouwt de audio-watchdog de stream binnen ~2 s automatisch — voorheen bleef de app geluidloos tot een herstart
- ⏱️ **Actuele audio-status + geschatte latentie** zichtbaar in Algemene Instellingen (host, apparaat, samplerate, kanalen, buffer, ≈ms)

### Opgelost

- 📊 **Laad-voortgang bij het openen van een orgel**: de voortgangsbalk bleef altijd op 0% staan, waardoor een grote (koude) GrandOrgue-set eruitzag als een vastgelopen app — de balk loopt nu echt mee met het aantal geladen samples
- 💾 **Instellingen naast Hauptwerk-orgels**: het `.jm-settings.json`-bestand naast het orgel werd bij Hauptwerk-orgels (en `.Organ`-bestanden met hoofdletter) nooit geschreven of teruggelezen (het orgelbestand werd als map behandeld); ook de console-afbeelding werd daardoor niet gevonden
- 🎹 **Registreren tijdens het spelen werkt nu overal direct**: een register bijtrekken laat ingedrukte toetsen meteen meeklinken (ook via actieve koppels en melodie-/baskoppels) en wegtrekken laat pijpen direct zwijgen — nu ook bij setzer-/preset-oproepen en het generaal-crescendo, niet alleen bij losse registerklikken
- 🔇 **Tikken in loops verholpen** (o.a. Puttershoek): drie oorzaken gefixt — (1) één sample stilte per loop-omloop wanneer het looppunt op het buffereinde lag (alle preload-buffers en fallback-loops), (2) een hoorbare positiesprong op het moment dat de volledige sample de preload-buffer overneemt (aanloop-trim werd niet gecompenseerd), (3) de FFT-resampler-vertraging werd niet gecompenseerd waardoor 44,1 kHz-sets in de tijd verschoven t.o.v. hun looppunten
- ✂️ **Kniptool bewaart looppunten**: stilte wegknippen gooide de smpl-loop-chunk weg en liet looppunten verschuiven; loops worden nu meegeschoven en teruggeschreven
- ⚡ **Minder aanslagvertraging**: resampler-delay-compensatie (44,1 kHz-sets spreken enkele ms eerder aan) en een strakkere aanslag-envelope (~2 ms i.p.v. ~4 ms)
- 🚦 Eén `ReleaseStop`-audiocommando per weggetrokken register i.p.v. een NoteOff per pijp — een preset-wissel die veel registers wegtrekt kan de command-queue niet meer verstoppen

## [0.5.2] - 2026-07-03

### Opgelost

- 🖱️ **Sorteerlijst-verslepen werkt nu echt**: de registersorteerlijst in Instellingen gebruikte nog native browser-drag (verbodsteken in WebView2); nu hetzelfde betrouwbare pointer-mechanisme als de registerknoppen — met de muis overal op de rij, met touch via het ☰-handvat

## [0.5.1] - 2026-07-03

### Opgelost

- 📚 **Dubbele samplesets in de bibliotheek**: verschillende pad-notaties (forward/backslash) leverden dubbele entries op; paden worden nu genormaliseerd en bestaande duplicaten worden bij het starten automatisch samengevoegd (instellingen blijven behouden)

## [0.5.0] - 2026-07-03

### Toegevoegd

- 🖱️ **Registerknoppen verslepen**: herorden registers direct op het orgelscherm met de muis (klikken blijft trekken; ▲▼ en de sorteerlijst blijven bestaan)
- ↕️ **Verticale indeling vult nu echt kolommen**: van boven naar beneden, kolom vol → volgende kolom; horizontaal blijft rijen vullen
- 🦶 **Pedaalregisters** krijgen geen onterecht "(Bas)"-label meer (bereik-heuristiek geldt alleen voor manualen)
- 🎼 **Echte release-samples** (GrandOrgue + Hauptwerk): bij het loslaten van een toets klinkt nu de opgenomen ruimteakoestiek van de sampleset in plaats van een synthetische fade van 42 ms
- 🎚️ **ODF-inschaling**: Gain/AmplitudeLevel/PitchTuning/PitchCorrection uit de sampleset worden nu toegepast (registerbalans en stemming zoals de maker bedoelde) — als aparte laag naast je eigen intonatie
- 🪗 **Hauptwerk-sets openbaar via de bestandskiezer** (`.Organ_Hauptwerk_xml`), inclusief zwelkasten, tremulant-samplelaag en release-samples; versleutelde sets geven een duidelijke melding
- 🔔 **Percussive-registers** (klok/glockenspiel) spelen one-shot en klinken uit bij note-off, zoals GrandOrgue

### Opgelost

- 🐛 **REF-pijpverwijzingen** resolven nu manual-relatief — herstelt stille toetsen (o.a. de 12 laagste van Bureå's Salicional 8')
- 🎹 **Zwelkast-detectie** is structureel (windchest→enclosure) i.p.v. naam-match — zwelkasten verschijnen nu ook als de windchest-naam afwijkt
- 🌊 **ODF-tremulantparameters** (snelheid/diepte) worden als default overgenomen
- 🎵 MIDI-velocity beïnvloedt het volume niet meer (orgelpijpen klinken altijd vol)
- 🔁 De eerste seconden van een noot gebruiken nu de echte looppunten i.p.v. een noodloop over de attack

- 🔊 **ASIO**: uitgestelde ASIO-wissel bij een opgeslagen ASIO-voorkeur (start op WASAPI, automatische wissel ±10 s na de app-start) — voorkomt dat ASIO4ALL stil blijft na een koude start
- 🔊 **ASIO→ASIO-wissel** sluit nu eerst de oude stream voordat de nieuwe wordt geopend (ASIO-drivers zijn single-client)
- 📊 **Audio-callback-watchdog** in het log — meldt wanneer audio-callbacks stoppen (init én periodiek elke 30 s)
- 🎹 **MIDI-opname**: stopknop stopt de capture nu echt — noten tijdens de opslagdialoog belanden niet meer in het .mid-bestand
- 🎚️ **MIDI-volumepedaal (CC7/CC11)** stuurt nu een correct dB-bereik (was: lineaire waarde als dB geïnterpreteerd, waardoor het pedaal feitelijk niets deed)
- 🩹 **Frontend**: crescendo-stap aanklikken past nu echt de registers toe; SetzerBar-MIDI-acties (tremulant/EQ/afsluiten) werken weer; opgeslagen voicings zichtbaar na herladen; knopgrootte/layout van het registervenster springt niet meer terug; versienummer in de statusbalk is dynamisch

## [0.4.0] - 2026-06-24

### Toegevoegd

#### Opname & weergave
- 🔴 **MIDI-opname** (Standard MIDI File) — neem je spel op als `.mid` in `Documenten/JM-Orgue-opnames`; klein, herbruikbaar, transponeerbaar
- ▶️ **Opname afspelen** — één klik en je hoort jezelf terug op hetzelfde orgel
- 🎧 **MP3-opname** van de live audio-uitvoer (320 kbps stereo, lock-vrij vanuit de audio-thread)

#### Per-orgel instellingen-persistentie
- 💾 **Elk orgel onthoudt z'n eigen instellingen** in `.jm-settings.json`: mastervolume, temperament + fijnstemming, reverb (type/mix/ruimte-preset), 3-bands EQ, per-divisie DSP (zwelkast, tremulant-LFO, stereo-pan, wind-groepen) en UI-voorkeuren (koppel- en zwel-zichtbaarheid, layout, knopgrootte). Geen gelekte instellingen meer tussen orgels

#### Sampleset-tools
- ✂️ **Silence-trim** — detecteert en verwijdert aanloopstilte uit WAV/MP3-samples in een sampleset (originelen naar backup-map)
- 🔁 **Loop-tool** — vindt automatisch een sustain-loop per WAV, bakt een equal-power crossfade op de naad en schrijft de loop points naar de `smpl` chunk

#### Instellingen & UI
- 🌍 **Meertalige UI** (NL/EN/FR/DE) door de hele app, realtime wisselbaar via Sfeer & Layout
- 🗂️ **Instellingen gesplitst** — Algemene Instellingen (audio, MIDI, sfeer, taal) gescheiden van Orgel-Instellingen (reverb, EQ, voicing per orgel); drie tabs Orgel · Orgel-Instellingen · Algemene Instellingen (F1/F2/F3)
- 🖱️ **Universele MIDI-learn** — overal via rechtermuisklik (desktop) of 3 sec vasthouden (touchscreen)

#### Realisme & routing
- 🌬️ **Wind-groepen** — 1 tot 8 windvoorzieningen vrij toewijsbaar aan divisies (pedaal apart, twee manualen op één balg, etc.)
- 🎹 **C/Cis-spreiding** — gescheiden C- en Cis-laden realistisch nagebootst, per orgel instelbaar
- 🔈 **Audiokanaal-verdeling per werk** — stuur elk klavier naar eigen fysieke uitgangen (multi-kanaals opstelling)

#### Developer / testen
- 🧪 **Test-API** — start de app met `--test-api` (standaard poort 8765, alleen 127.0.0.1) voor een HTTP-API t.b.v. testen en automatisering
- 🎚️ **Optionele ASIO-buildfeature** — `cargo build --release -p vpo-app --features asio` compileert cpal's ASIO-backend voor lage latency op Windows

### Gewijzigd

- ⚡ **Snellere sampleset-loading** — bestaande (Hauptwerk/GrandOrgue-)sets laden in seconden; 16 GB RAM volstaat voor grote orgels

## [0.3.0] - 2026-05-05

### Toegevoegd

#### Sfeer & Layout
- 🎨 **Vrij instelbaar layout-systeem** — alle kleuren, lettertype, achtergrond per token aanpasbaar via Instellingen
- 🎨 **5 ingebouwde presets** — Basis (Witte orgelbouwer), Klassiek hout (Eiken), Modern minimalistisch, Nacht (Donker), Avond (Warm donker)
- 💾 **Eigen sferen opslaan** — naam geven en als preset bewaren, met live preview
- 🖼️ **Achtergrond textuur** — eigen afbeelding als achtergrond mogelijk
- 📤 **Sfeer exporteren** als JSON

#### Audio & MIDI
- 🎼 **Per-pijp voicing UI** — interactieve sliders voor volume (±12 dB) en stemming (±50 ¢) per pijp, met visuele indicatoren voor aangepaste pijpen
- 💾 **Voicing persistentie** — automatisch opgeslagen in `.jm-settings.json` per orgel, hersteld bij laden
- ▶️ **MIDI file playback** — `.mid` / `.midi` / `.smf` bestanden afspelen door het orgel met play/pause/stop, seek-balk en snelheidsregelaar (0.5×–2×)
- 🎵 **Sample rate display** — actuele rate + ondersteunde rates van het audio-device, met helptekst voor 96 kHz instelling
- 🎵 **52 historische temperamenten** — Lambert (1774), Barnes-Bach (1979) en Vallotti-Young toegevoegd

#### UX
- 📱 **Touchscreen-optimalisatie systematisch** doorgevoerd via design tokens (`--touch-target`) over alle knoppen, sliders, inputs, lijsten, context menus

### Gewijzigd

- 🎨 **Donker / licht thema toggle vervangen** door vrij instelbaar systeem met presets — meer flexibiliteit voor de gebruiker
- 🎨 **Volledige design tokens migratie** — alle hardcoded kleuren in `styles.css` en componenten omgezet naar CSS custom properties zodat sfeer-aanpassingen consistent doorwerken
- 📦 **CSS architectuur** — één centrale token-set in `:root`, geen losse hex-waarden meer in selectors

### Opgelost

- 🐛 Sample rate stond niet meer expliciet in de UI — nu zichtbaar in Instellingen
- 🐛 Sommige kleur-aanpassingen via Instellingen pakten niet overal door — alle componenten gebruiken nu tokens

## [0.2.0] - 2026-03-28

### Toegevoegd

#### Audio Engine
- 🌬️ **Windmodel per divisie** — reservoir, demping, max drukverlies
- 🎵 **Tremulant LFO synthese** als fallback wanneer geen tremulant-samples beschikbaar
- ⛪ **FDN algoritmische galm** met 6 ruimtepresets (Kapel → Gotische Kathedraal)
- 🎚️ **3-band Parametric EQ** (low shelf + mid peak + high shelf)
- 🔇 **Aanpasbare zwelkast** — volume + frequentie-afhankelijk filter
- 🎼 **Per-pijp voicing** — volume (±20dB) + pitch (±100ct) per individuele pijp
- 🎵 **96 kHz HD audio** support
- 📊 **Per-divisie stereo panning** (constant-power) met persistente waardes

#### MIDI
- 🔵 **Bluetooth LE MIDI** — directe BLE GATT verbinding via `btleplug`
- 🔄 **Auto-connect** alle USB MIDI bij opstarten
- 📝 **MIDI file recording** (Standard MIDI File writer)
- 🎚️ **Activity counter** voor BLE messages

#### Registers & Koppels
- 🎛️ **Drag & drop register sortering** per divisie
- ⬆️ **Superoctaaf** (4', +12)
- ⬇️ **Suboctaaf / Octave Grave** (16', -12)
- 🔇 **Tongwerken Af** per divisie
- 🎼 **Melodiekoppel** (alleen hoogste noot)
- 🎚️ **Baskoppel** (alleen laagste noot, naar pedaal)
- 🎚️ **Generaal Crescendo** met live LED-indicator
- 💾 **8 memory levels × 1000 presets** = 8.000 combinaties
- ⏯️ **Held-notes re-trigger** bij stop toggle

#### Stemmingen
- 🎵 **50 historische temperamenten** (was 33)
- ✏️ **Custom temperament editor** met 12 cent-input fields
- 🎯 **Per-noot tuning** in audio engine

#### UI/UX
- 🌙 **Donker / licht thema** toggle (zon/maan icoon)
- 📱 **Touchscreen-optimalisatie** (`@media pointer: coarse`)
- ⌨️ **Keyboard shortcuts** — F1/F2 navigatie, Esc voor orgel-selectie, arrow keys tussen tabs
- ♿ **ARIA accessibility labels** op stops, koppels, tabs
- 📚 **Orgel bibliotheek** met persistente settings
- 💾 **Auto-save** instellingen in orgelmap + AppData
- 📤 **Export / import** settings als JSON
- 🪟 **Apart register-paneel** venster

#### Bestandsformaten
- 📄 **`.organ` file export** — genereer ODF vanuit sample directory
- 🎵 **MP3 sample support** (naast WAV)
- 🔁 **Loop point extraction** uit WAV smpl chunk
- 🔄 **Auto-resample** naar audio device sample rate

### Gewijzigd

- ✏️ Verwijzingen naar externe orgelsoftware uit UI (algemener gemaakt)
- 🎨 Nieuwe orgelbouwer-stijl UI (crème/bladgoud thema)
- 📁 File logging in `%APPDATA%\nl.jm-orgue.app\jm-orgue.log`
- 🔧 Bundle config voor macOS (.dmg) en Linux (.AppImage/.deb) klaar
- 🎯 Sample rate is nu device-gestuurd (geen hardcoded 48000)

### Opgelost

- 🐛 BLE MIDI HRESULT 0x80000013 ("object closed") door subscribe-retry mechanisme
- 🐛 BLE MIDI deadlock tijdens learn — nieuwe `learn_wait_settled` helpers
- 🐛 BLE MIDI berichten kwamen niet binnen → notification stream wordt nu na subscribe geopend
- 🐛 Crescendo CC werd niet verwerkt — `process_crescendo_cc` toegevoegd
- 🐛 Status check zag BLE-verbonden apparaten niet als "verbonden"
- 🐛 Sample upgrade van preload→full had positie-conversie fout
- 🐛 Custom temperament editor sloot niet correct
- 🐛 Pan slider behield value 0 ongeacht localStorage

## [0.1.0] - 2025-09-17

### Toegevoegd

- Initial release: basis sample playback, MIDI input, .organ ODF parsing
- Convolutie reverb met IR loading
- Setzer met MIDI learn (basis 100 presets)
- Sample directory scanner
- Per-divisie zwelkast (alleen volume)
- 33 temperamenten
- Coupler systeem (unison)
